//! The session layer under both front ends: the merge semantics, repository discovery,
//! conflict loading, line origins, the recovery journal, tool installation, and reading and
//! writing a merge's files. The CLI and the GUI build on it. No GUI and no argument parser.
//! The settings and key bindings it applies live in `merge-config`.

pub mod diff_tool;
pub mod dir_diff;
pub mod ending;
pub mod env;
pub mod install;
pub mod invocation;
pub mod io;
pub mod jj_config;
pub mod jj_native;
pub mod native;
pub mod origin;
pub mod recovery;
pub mod replay;
pub mod repository;
pub mod resolve;
pub mod semantics;
pub mod session;
pub mod stamp;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::io::{WriteError, write_atomic, write_atomic_with};
use camino::Utf8PathBuf;
use merge_core::boundaries::Boundaries;
use merge_core::document::{
    LabelCountError, Labels, MarkerStyle, MergeDocument, MergeOptions, Output, OutputEnding,
};
use merge_core::merge3::{Inputs, Semantics};
use merge_core::text::{DecodeError, EncodeError, Encoding, Text, output_encoding};
use merge_syntax::boundaries::SyntaxBoundaries;
use merge_syntax::language::Language;
use vcs::{RepoKind, VcsError};

use crate::env::Env;
use crate::invocation::{MergeFiles, SemanticsFlags, Window};
use crate::repository::RunRepository;
use crate::resolve::Record;
use crate::semantics::{Caller, Mode, SemanticsError, choose};
use merge_config::settings::Settings;

/// The application's name in the platform's directories.
pub const APPLICATION: &str = "triad";

/// The process exit codes of spec §6.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Output written, fully resolved. Also a printed help text.
    Resolved = 0,
    /// Output written with conflict markers. Also: `inspect` found conflicts, or
    /// `diff --inspect` found differences.
    Conflicts = 1,
    /// Cancelled; nothing written.
    Cancelled = 2,
    Error = 3,
}

impl ExitCode {
    /// The code a process exited with, if it is one of these.
    #[must_use]
    pub fn from_code(code: i32) -> Option<Self> {
        [Self::Resolved, Self::Conflicts, Self::Cancelled, Self::Error]
            .into_iter()
            .find(|&c| i32::from(c as u8) == code)
    }
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        Self::from(code as u8)
    }
}

/// A command line that needs a window, and the settings it runs with.
#[derive(Debug)]
pub struct WindowForm {
    pub window: Window,
    pub settings: Settings,
    /// The config file's lines that could not be used, for the window's banner.
    pub settings_errors: Vec<String>,
    /// The run's working directory, which relative paths and repository discovery start
    /// from; none when the process has none.
    pub cwd: Option<PathBuf>,
    /// The run's repository, open when a step before the window read it. The window's line
    /// origins take it, so a run opens it once.
    pub repo: RunRepository,
}

/// Why a session step failed.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("{}: {source}", path.display())]
    Decode { path: PathBuf, source: DecodeError },
    #[error("cannot save {}: {source}", path.display())]
    Encode { path: PathBuf, source: EncodeError },
    #[error(transparent)]
    Labels(#[from] LabelCountError),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Semantics(#[from] SemanticsError),
    #[error(transparent)]
    JjConfig(#[from] vcs_jj::config::ConfigError),
    #[error("{}: {source}", path.display())]
    Commands { path: PathBuf, source: replay::ReplayError },
}

/// The semantics, same-change rule and hunk level of a merge in the run's repository `repo`,
/// writing `output` from `cwd`; `inspect` has no output. A flag wins over the settings file. The semantics then come from the caller
/// ([`semantics::choose`]); under jj's, the other two fall back to jj's config. jj's config is
/// read only when a flag or setting leaves one of them open.
///
/// # Errors
///
/// [`SessionError::Semantics`] for git semantics in a jj repository, and
/// [`SessionError::JjConfig`] when jj's config cannot be read.
pub fn choose_semantics(
    flags: SemanticsFlags,
    settings: &Settings,
    repo: &RunRepository,
    output: Option<&Path>,
    cwd: Option<&Path>,
    env: &Env,
) -> Result<Mode, SessionError> {
    let caller = Caller::new(repo, cwd, output);
    let semantics = choose(flags.choice.unwrap_or(settings.semantics), &caller)?;
    let given =
        (flags.same_change.or(settings.same_change), flags.hunk_level.or(settings.hunk_level));
    let jj = match (given, semantics) {
        ((Some(_), Some(_)), _) | (_, Semantics::Git) => jj_config::JjMerge::default(),
        (_, Semantics::Jj) => jj_config::merge_config(env, caller.jj_root())?,
    };
    let mode = Mode {
        semantics,
        same_change: given.0.or(jj.same_change).unwrap_or_default(),
        hunk_level: given.1.or(jj.hunk_level).unwrap_or_default(),
    };
    tracing::debug!(?caller, ?mode, "chose the merge semantics");
    Ok(mode)
}

/// The markers of a merge under `semantics` ([`semantics::marker_style`]), with git's
/// `merge.conflictStyle` from the run's repository when it is git's. The repository stays
/// open for the steps after.
#[must_use]
pub fn choose_marker_style(
    semantics: Semantics,
    settings: &Settings,
    repo: &mut RunRepository,
) -> MarkerStyle {
    semantics::marker_style(semantics, settings.conflict_style, || {
        if repo.kind() != Some(RepoKind::Git) {
            return None;
        }
        repo.require()
            .and_then(vcs::Repository::conflict_style)
            .inspect_err(|error| {
                tracing::warn!(%error, "git's conflict style is unread; the markers are diff3");
            })
            .ok()?
    })
}

/// The platform cache directory's journal folder (spec §6.2).
#[must_use]
pub fn journal_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", APPLICATION).map(|dirs| dirs.cache_dir().join("journal"))
}

/// A merge's language (spec §4.3): the one its names give, else the one the first line of
/// a side, then of a base, names.
#[must_use]
pub fn language(names: Option<Language>, inputs: &Inputs) -> Option<Language> {
    names.or_else(|| {
        Language::of_scripts(inputs.sides().iter().chain(inputs.bases()).map(Text::content))
    })
}

/// `inputs` aligned under `options`, with the syntax boundaries of the merge's [`language`].
pub(crate) fn align(
    inputs: Inputs,
    options: MergeOptions,
    labels: Labels,
    names: Option<Language>,
) -> Result<MergeDocument, LabelCountError> {
    let boundaries = language(names, &inputs).map(SyntaxBoundaries::new);
    MergeDocument::new_bounded(
        inputs,
        options,
        labels,
        boundaries.as_ref().map(|b| b as &dyn Boundaries),
    )
}

/// Read and decode the inputs, and align them, with the syntax boundaries of the language
/// the names give, else the inputs' first lines, when the options turn them on (spec §4.3).
///
/// # Errors
///
/// [`SessionError`] when an input cannot be read or decoded, or the labels do not fit.
pub fn load(
    files: &MergeFiles,
    options: MergeOptions,
    labels: Labels,
    names: Option<Language>,
) -> Result<MergeDocument, SessionError> {
    let base = read_text(&files.base)?;
    let left = read_text(&files.left)?;
    let right = read_text(&files.right)?;
    Ok(align(Inputs::three_way(base, left, right), options, labels, names)?)
}

/// Why a save of a conflict the VCS holds failed (spec §6.2, §7.4).
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// The output encoding cannot hold a character of the result: nothing was written, and
    /// nothing recorded.
    #[error(transparent)]
    Encode(#[from] EncodeError),
    /// Nothing was written, and nothing recorded.
    #[error(transparent)]
    Write(#[from] WriteError),
    /// The output is written, ending the merge with `code`, but the repository did not
    /// record the save. [`Record::resave`] records it again without writing.
    #[error("{path} is written, but the repository did not record the save: {source}")]
    Record {
        /// The conflicted path, relative to the repository root.
        path: Utf8PathBuf,
        code: ExitCode,
        #[source]
        source: VcsError,
    },
}

/// The result in the output encoding, with conflict markers for unresolved regions and line
/// endings made as asked, and the exit code that goes with it. It holds no borrow of its
/// document, so a thread of its own can write it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedOutput {
    bytes: Vec<u8>,
    code: ExitCode,
}

impl EncodedOutput {
    /// `document`'s result as a save writes it, its line endings made as `ending` says.
    ///
    /// # Errors
    ///
    /// [`EncodeError`] when the output encoding cannot hold a character of the result.
    pub fn new(document: &MergeDocument, ending: OutputEnding) -> Result<Self, EncodeError> {
        let inputs = document.inputs();
        let encoding = output_encoding(&inputs.bases()[0], &inputs.sides()[0]);
        let (text, code) = match document.output_with(ending) {
            Output::Resolved(text) => (text, ExitCode::Resolved),
            Output::Conflicted(text) => (text, ExitCode::Conflicts),
        };
        // UTF-8 without a mark encodes as the text's own bytes: take them, not a copy.
        let encoded = match encoding.encode(&text)? {
            Cow::Owned(bytes) => Some(bytes),
            Cow::Borrowed(_) => None,
        };
        let bytes = encoded.unwrap_or_else(|| text.into_bytes());
        Ok(Self { bytes, code })
    }

    /// The bytes a save writes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The exit code a save of this output ends the merge with.
    #[must_use]
    pub fn code(&self) -> ExitCode {
        self.code
    }

    /// Write the bytes to `output` atomically, and nothing else.
    ///
    /// # Errors
    ///
    /// [`WriteError`] when `output` cannot be written; it is then unchanged.
    pub fn write(&self, output: &Path) -> Result<ExitCode, WriteError> {
        write_atomic(output, &self.bytes)?;
        let resolved = self.code == ExitCode::Resolved;
        tracing::info!(output = %output.display(), resolved, "wrote the merge");
        Ok(self.code)
    }

    /// Write the bytes to `output`, then record the save in the repository when `record`
    /// names the conflict there (spec §7.4): through `run`, the run's repository, which then
    /// reads at the save's operation, or without one in the conflict's repository opened
    /// afresh.
    ///
    /// # Errors
    ///
    /// [`SaveError::Write`] when `output` cannot be written; it is then unchanged and nothing
    /// is recorded. [`SaveError::Record`] when the output is written but the repository
    /// refuses the record, or `run` holds another repository than the conflict's.
    pub fn save(
        &self,
        output: &Path,
        record: Option<&Record>,
        run: Option<&mut RunRepository>,
    ) -> Result<ExitCode, SaveError> {
        let code = self.write(output)?;
        let Some(record) = record else {
            return Ok(code);
        };
        match run {
            Some(run) => record.saved_in(run, code),
            None => record.saved(code),
        }
        .map_err(|source| record.unrecorded(code, source))?;
        Ok(code)
    }
}

/// Write `chunks`, pieces of one text split at `char` boundaries such as a rope's, to `path`
/// in `encoding` with its byte order mark, as [`write_atomic`] does. No piece is copied
/// into one string first.
///
/// # Errors
///
/// [`SessionError::Encode`] when `encoding` cannot hold a character of the text, and
/// [`SessionError::Write`] when `path` cannot be written; it is then unchanged.
pub fn save_text<'t, C>(path: &Path, encoding: Encoding, chunks: C) -> Result<(), SessionError>
where
    C: IntoIterator<Item = &'t str> + Clone,
{
    encoding
        .holds(chunks.clone())
        .map_err(|source| SessionError::Encode { path: path.to_owned(), source })?;
    write_atomic_with(path, |out| {
        out.write_all(encoding.bom())?;
        chunks.into_iter().try_for_each(|chunk| {
            // `holds` checked every character, so this encodes; an error stays typed.
            let bytes = encoding.encode_chunk(chunk).map_err(std::io::Error::other)?;
            out.write_all(&bytes)
        })
    })?;
    tracing::info!(path = %path.display(), "saved");
    Ok(())
}

/// Read and decode one input.
///
/// # Errors
///
/// [`SessionError::Read`] when the file cannot be read, [`SessionError::Decode`] when it is binary
/// or not valid text.
pub fn read_text(path: &Path) -> Result<Text, SessionError> {
    let bytes = std::fs::read(path)
        .map_err(|source| SessionError::Read { path: path.to_owned(), source })?;
    Text::decode(bytes).map_err(|source| SessionError::Decode { path: path.to_owned(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes().to_vec()).expect("decodes")
    }

    #[test]
    fn the_names_language_wins_over_the_inputs_shebang() {
        let inputs = Inputs::three_way(text("#!/bin/sh\n"), text("#!/bin/sh\n"), text("a\n"));
        assert_eq!(language(Some(Language::Rust), &inputs), Some(Language::Rust));
        assert_eq!(language(None, &inputs), Some(Language::Bash));
    }

    #[test]
    fn the_sides_shebangs_count_before_the_bases() {
        let python = "#!/usr/bin/env python3\n";
        let inputs = Inputs::three_way(text("#!/bin/sh\n"), text("a\n"), text(python));
        assert_eq!(language(None, &inputs), Some(Language::Python));
        let inputs = Inputs::three_way(text("#!/bin/sh\n"), text("a\n"), text("b\n"));
        assert_eq!(language(None, &inputs), Some(Language::Bash));
        let inputs = Inputs::three_way(text("a\n"), text("b\n"), text(""));
        assert_eq!(language(None, &inputs), None);
    }

    #[test]
    fn a_load_without_a_named_language_bounds_the_merge_by_the_inputs_shebang() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\nf() {{\n{body}}}\n")).unwrap();
            path
        };
        let files = MergeFiles {
            base: write("BASE", "    a\n"),
            left: write("LEFT", "    p && {\n        b\n    }\n    c\n"),
            right: write("RIGHT", "    q || {\n        d\n    }\n    e\n"),
        };
        let options = MergeOptions {
            refine: merge_core::merge3::Refine::Deep,
            syntax_boundaries: true,
            ..MergeOptions::default()
        };
        let doc = load(&files, options, Labels::none(), None).unwrap();
        // Unchanged, conflict, unchanged: the shared `}` stays inside the conflict. As plain
        // text it splits off between two conflicts, in five regions.
        assert_eq!(doc.regions().len(), 3);
    }

    /// Lines of `é\n`, more than one rope chunk holds.
    const LINES: usize = 3000;

    #[test]
    fn a_text_is_saved_chunk_by_chunk_after_its_byte_order_mark() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        save_text(&path, Encoding::Utf16LeBom, std::iter::repeat_n("é\n", LINES)).unwrap();
        // `é\n` is E9 00 0A 00 in UTF-16LE.
        let expected = [&[0xFF, 0xFE][..], &[0xE9, 0x00, 0x0A, 0x00].repeat(LINES)].concat();
        assert_eq!(std::fs::read(&path).unwrap(), expected);
        save_text(&path, Encoding::Utf8Bom, ["a", "é\n"]).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), [0xEF, 0xBB, 0xBF, b'a', 0xC3, 0xA9, b'\n']);
    }

    #[test]
    fn a_windows_1252_text_saves_one_byte_per_character() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        save_text(&path, Encoding::Windows1252, ["caf", "é \u{201C}q\u{201D}\n"]).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"caf\xE9 \x93q\x94\n");
    }

    #[test]
    fn a_character_the_encoding_cannot_hold_refuses_the_save_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        std::fs::write(&path, b"before\n").unwrap();
        // `ł` sits on line 3, in the third chunk.
        let error = save_text(&path, Encoding::Windows1252, ["a\n", "b\n", "Zdzisław\n"])
            .expect_err("Windows-1252 has no byte for `ł`");
        assert!(
            matches!(&error, SessionError::Encode { path: p, source }
            if *p == path
                && *source
                    == EncodeError {
                        encoding: Encoding::Windows1252,
                        character: 'ł',
                        line: 3,
                    }),
            "{error:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"before\n");
    }

    #[test]
    fn a_text_saved_into_a_missing_directory_names_the_path_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("out.txt");
        match save_text(&path, Encoding::Utf8, ["a\n"]) {
            Err(SessionError::Write(WriteError { path: named, .. })) => assert_eq!(named, path),
            other => panic!("expected a write error, got {other:?}"),
        }
        assert!(!dir.path().join("missing").exists());
    }

    fn fixture(case: &str, file: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/core").join(case).join(file)
    }

    fn files(case: &str) -> MergeFiles {
        MergeFiles {
            base: fixture(case, "base"),
            left: fixture(case, "left"),
            right: fixture(case, "right"),
        }
    }

    #[test]
    fn load_gives_the_automatic_merge() {
        let doc = fixture_doc("clean-merge");
        assert_eq!(doc.output(), Output::Resolved("A\nb\nC\n".to_owned()));
    }

    #[test]
    fn load_names_the_file_it_cannot_read() {
        use merge_core::document::{Labels, MergeOptions};

        let mut files = files("clean-merge");
        files.right = fixture("clean-merge", "missing");
        let error = load(&files, MergeOptions::default(), Labels::none(), None)
            .expect_err("right is missing");
        assert!(
            matches!(&error, SessionError::Read { path, source }
                if *path == files.right && source.kind() == std::io::ErrorKind::NotFound),
            "{error:?}"
        );
    }

    /// The fixture `name` loaded with the default options and no labels.
    fn fixture_doc(name: &str) -> MergeDocument {
        use merge_core::document::{Labels, MergeOptions};
        load(&files(name), MergeOptions::default(), Labels::none(), None)
            .expect("the fixture loads")
    }

    #[test]
    fn save_writes_the_markers_and_gives_exit_1_for_a_conflict() {
        let doc = fixture_doc("conflict-bare-markers");
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = tmp.path().join("out");
        assert_eq!(
            EncodedOutput::new(&doc, OutputEnding::Keep).unwrap().write(&output).expect("written"),
            ExitCode::Conflicts
        );
        assert_eq!(
            std::fs::read(&output).expect("output"),
            b"a\n<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n"
        );
    }

    #[test]
    fn save_converts_line_endings_in_the_output_encoding() {
        let doc = fixture_doc("conflict-bare-markers");
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = tmp.path().join("out");
        assert_eq!(
            EncodedOutput::new(&doc, OutputEnding::CrLf).unwrap().write(&output).expect("written"),
            ExitCode::Conflicts
        );
        assert_eq!(
            std::fs::read(&output).expect("output"),
            b"a\r\n<<<<<<<\r\nL\r\n|||||||\r\nb\r\n=======\r\nR\r\n>>>>>>>\r\nc\r\n"
        );
    }

    #[test]
    fn save_gives_exit_0_when_resolved_and_names_a_path_it_cannot_write() {
        let doc = fixture_doc("clean-merge");
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = tmp.path().join("out");
        assert_eq!(
            EncodedOutput::new(&doc, OutputEnding::Keep).unwrap().write(&output).expect("written"),
            ExitCode::Resolved
        );
        assert_eq!(std::fs::read(&output).expect("output"), b"A\nb\nC\n");
        let missing = tmp.path().join("no-such-dir").join("out");
        let error = EncodedOutput::new(&doc, OutputEnding::Keep)
            .unwrap()
            .write(&missing)
            .expect_err("no directory");
        assert!(matches!(&error, WriteError { path, .. } if *path == missing), "{error:?}");
    }

    #[test]
    fn an_encoded_output_outlives_its_document_and_writes_as_save_does() {
        let doc = fixture_doc("conflict-bare-markers");
        let encoded = EncodedOutput::new(&doc, OutputEnding::CrLf).unwrap();
        drop(doc);
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = tmp.path().join("out");
        assert_eq!(encoded.write(&output).expect("written"), ExitCode::Conflicts);
        assert_eq!(
            std::fs::read(&output).expect("output"),
            b"a\r\n<<<<<<<\r\nL\r\n|||||||\r\nb\r\n=======\r\nR\r\n>>>>>>>\r\nc\r\n"
        );
        let missing = tmp.path().join("no-such-dir").join("out");
        let error = encoded.write(&missing).expect_err("no directory");
        assert!(matches!(&error, WriteError { path, .. } if *path == missing), "{error:?}");
    }

    #[test]
    fn an_encoded_resolved_output_gives_exit_0() {
        let doc = fixture_doc("clean-merge");
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = tmp.path().join("out");
        let encoded = EncodedOutput::new(&doc, OutputEnding::Keep).unwrap();
        assert_eq!(encoded.write(&output).expect("written"), ExitCode::Resolved);
        assert_eq!(std::fs::read(&output).expect("output"), b"A\nb\nC\n");
    }

    #[test]
    fn process_codes_map_back_to_exit_codes() {
        let codes: Vec<Option<ExitCode>> = (-1..=4).map(ExitCode::from_code).collect();
        assert_eq!(
            codes,
            [
                None,
                Some(ExitCode::Resolved),
                Some(ExitCode::Conflicts),
                Some(ExitCode::Cancelled),
                Some(ExitCode::Error),
                None
            ]
        );
    }

    #[test]
    fn exit_codes_follow_the_spec() {
        let codes = [ExitCode::Resolved, ExitCode::Conflicts, ExitCode::Cancelled, ExitCode::Error]
            .map(|c| c as u8);
        assert_eq!(codes, [0, 1, 2, 3]);
    }
}
