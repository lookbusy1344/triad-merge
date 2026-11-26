//! The text of an error a window shows, in the run's language (spec §6.5, `DESIGN.md` §E3.6).
//! Each error triad defines for a window has a message per variant, and its sources are named
//! the same way. Text from jj-lib, gix, GPUI, serde and the system keeps its own English.
//! Errors that mark a bug in triad, and those of the command line alone, keep their English
//! `Display`, so a bug report reads the same everywhere.

use std::error::Error;
use std::io;
use std::path::Path;

use merge_core::journal::{JOURNAL_VERSION, ParseError, RestoreError};
use merge_core::markers::MarkerError;
use merge_core::merge3::ShapeError;
use merge_core::text::{BINARY_SNIFF_BYTES, DecodeError, EncodeError};
use merge_session::diff_tool::DiffToolError;
use merge_session::dir_diff::DirDiffError;
use merge_session::io::WriteError;
use merge_session::jj_config::ConfigError;
use merge_session::jj_native::JjError;
use merge_session::native::NativeError;
use merge_session::resolve::ResolveError;
use merge_session::semantics::SemanticsError;
use merge_session::session::{CannotSave, PreviewError, Refused};
use merge_session::{SaveError, SessionError};
use vcs::{EntryKind, VcsError};

use crate::i18n::{self, Arg, Locale};
use crate::resolve_queue::QueueError;
use crate::window::OpenError;

/// `error` and its sources as the run's language says them.
pub(crate) fn describe(error: &(dyn Error + 'static)) -> String {
    describe_in(i18n::current(), error)
}

/// `error` and its sources in `locale`. A path, a label or a library's message may hold any
/// character: each hidden one shows as its escape (spec §5), whichever message names it.
pub(crate) fn describe_in(locale: Locale, error: &(dyn Error + 'static)) -> String {
    merge_core::unusual::escaped_owned(Namer { locale }.name(error))
}

struct Namer {
    locale: Locale,
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

impl Namer {
    fn say(&self, id: &str, args: &[(&str, Arg<'_>)]) -> String {
        i18n::format_in(self.locale, id, args).into_owned()
    }

    /// The message `id` about `path`.
    fn about(&self, id: &str, path: &str) -> String {
        self.say(id, &[("path", path.into())])
    }

    /// The message `id` about `path`, with `source` as its detail.
    fn about_with(&self, id: &str, path: &str, source: &(dyn Error + 'static)) -> String {
        let detail = self.name(source);
        self.say(id, &[("path", path.into()), ("detail", Arg::Text(&detail))])
    }

    fn name(&self, error: &(dyn Error + 'static)) -> String {
        macro_rules! named_as {
            ($($ty:ty => $by:ident),* $(,)?) => {
                $(if let Some(error) = error.downcast_ref::<$ty>() {
                    return self.$by(error);
                })*
            };
        }
        if let Some(&error) = error.downcast_ref::<CannotSave>() {
            return self.cannot_save(error);
        }
        named_as!(
            SessionError => session,
            SaveError => save,
            WriteError => write,
            Refused => refused,
            PreviewError => preview,
            VcsError => vcs,
            JjError => jj,
            ConfigError => jj_config,
            NativeError => native,
            ResolveError => resolve,
            DirDiffError => dir_diff,
            DiffToolError => diff_tool,
            SemanticsError => semantics,
            DecodeError => decode,
            EncodeError => encode,
            MarkerError => markers,
            ShapeError => shape,
            ParseError => journal_parse,
            RestoreError => journal_restore,
            OpenError => open,
            QueueError => queue,
            io::Error => io,
        );
        error.to_string()
    }

    fn session(&self, error: &SessionError) -> String {
        match error {
            SessionError::Read { path, source } => {
                self.about_with("err-cannot-read", &shown(path), source)
            }
            SessionError::Decode { path, source } => {
                self.about_with("err-in", &shown(path), source)
            }
            SessionError::Encode { path, source } => {
                self.about_with("err-cannot-save", &shown(path), source)
            }
            SessionError::Write(error) => self.write(error),
            SessionError::Semantics(error) => self.semantics(error),
            SessionError::JjConfig(error) => self.jj_config(error),
            // Label counts and command files come from the command line.
            SessionError::Labels(_) | SessionError::Commands { .. } => error.to_string(),
        }
    }

    fn save(&self, error: &SaveError) -> String {
        match error {
            SaveError::Encode(error) => self.encode(error),
            SaveError::Write(error) => self.write(error),
            SaveError::Record { path, source, .. } => {
                self.about_with("err-not-recorded", path.as_str(), source)
            }
            SaveError::Commit { path, source } => {
                self.about_with("err-cannot-write", path.as_str(), source)
            }
        }
    }

    fn write(&self, error: &WriteError) -> String {
        self.about_with("err-cannot-write", &shown(&error.path), &error.source)
    }

    fn refused(&self, error: &Refused) -> String {
        match error {
            Refused::Saving => self.say("err-save-writing", &[]),
            Refused::Core(_) => error.to_string(),
        }
    }

    fn cannot_save(&self, error: CannotSave) -> String {
        match error {
            CannotSave::NoOutput => self.say("err-no-output", &[]),
            CannotSave::Saving => self.say("err-save-running", &[]),
        }
    }

    fn preview(&self, error: &PreviewError) -> String {
        match error {
            PreviewError::Encode(error) => self.encode(error),
            PreviewError::Vcs(error) => self.vcs(error),
        }
    }

    fn vcs(&self, error: &VcsError) -> String {
        let rev = |id, rev: &str| self.say(id, &[("rev", rev.into())]);
        let at =
            |id, path: &str, at: &str| self.say(id, &[("path", path.into()), ("rev", at.into())]);
        match error {
            VcsError::NotARepository(path) => self.about("err-not-a-repository", path.as_str()),
            VcsError::NotUtf8(path) => self.about("err-not-utf8-path", &shown(path)),
            VcsError::NoWorkingCopy(path) => self.about("err-no-working-copy", path.as_str()),
            VcsError::OutsideRepository(path) => {
                self.about("err-outside-repository", path.as_str())
            }
            VcsError::StaleWorkingCopy(path) => self.about("err-stale-file", path.as_str()),
            VcsError::StaleWorkspace(path) => self.about("err-stale-workspace", path.as_str()),
            VcsError::UnknownRevision(name) => rev("err-unknown-revision", name),
            VcsError::AmbiguousRevision(name) => rev("err-ambiguous-revision", name),
            VcsError::Immutable(name) => rev("err-immutable", name),
            VcsError::Conflicted { path, rev } => at("err-conflicted-at", path.as_str(), rev),
            VcsError::NotConflicted { path, rev } => {
                at("err-not-conflicted-at", path.as_str(), rev)
            }
            VcsError::NoIndex(name) => self.say("err-no-index", &[("name", (*name).into())]),
            VcsError::BadConfig { key, value } => {
                let value = format!("{value:?}");
                self.say("err-bad-config", &[("key", (*key).into()), ("value", Arg::Text(&value))])
            }
            VcsError::Backend { context, source } => {
                let detail = self.name(source.as_ref());
                self.say(
                    "err-backend",
                    &[("context", (*context).into()), ("detail", Arg::Text(&detail))],
                )
            }
        }
    }

    fn jj(&self, error: &JjError) -> String {
        match error {
            JjError::ConcurrentOperations(count) => {
                self.say("err-concurrent-operations", &[("count", (*count).into())])
            }
            JjError::NotAFileConflict(path) => self.about("err-not-a-file-conflict", path.as_str()),
            JjError::UnknownOperation { op, reason } => {
                self.say("err-unknown-operation", &[("op", op.into()), ("reason", reason.into())])
            }
            JjError::NoWorkingCopyAt(op) => {
                self.say("err-no-working-copy-at", &[("op", op.into())])
            }
            JjError::NoHunk { hunk, count } => {
                self.say("err-no-hunk", &[("hunk", (*hunk).into()), ("count", (*count).into())])
            }
            JjError::NotADestination(rev) => {
                self.say("err-not-a-destination", &[("rev", rev.into())])
            }
            JjError::RepositoryMoved(op) => self.say("err-repository-moved", &[("op", op.into())]),
            JjError::ConflictChanged { path, change } => self.say(
                "err-conflict-changed",
                &[("path", path.as_str().into()), ("change", change.into())],
            ),
            JjError::SidesChanged { path, before, after } => self.say(
                "err-sides-changed",
                &[
                    ("path", path.as_str().into()),
                    ("before", (*before).into()),
                    ("after", (*after).into()),
                ],
            ),
        }
    }

    fn jj_config(&self, error: &ConfigError) -> String {
        match error {
            ConfigError::Load { path, source } => {
                self.about_with("err-jj-config-load", &shown(path), source)
            }
            ConfigError::Read { path, source } => {
                self.about_with("err-cannot-read", &shown(path), source)
            }
            ConfigError::BadId { path } => self.about("err-jj-config-id", &shown(path)),
            ConfigError::Value { key, path, source } => {
                let detail = self.name(source);
                let (key, detail) = (Arg::Text(key), Arg::Text(&detail));
                match path {
                    Some(path) => {
                        let path = shown(path);
                        self.say(
                            "err-jj-config-value-in",
                            &[("path", Arg::Text(&path)), ("key", key), ("detail", detail)],
                        )
                    }
                    None => self.say("err-jj-config-value", &[("key", key), ("detail", detail)]),
                }
            }
            ConfigError::Get(_) => self.say("err-jj-config-get", &[]),
            ConfigError::Update(_) => self.say("err-jj-config-set", &[]),
            ConfigError::Secure(_) => self.say("err-jj-config-find", &[]),
            ConfigError::AutoTrack { text, .. } => {
                let text = format!("{text:?}");
                self.say("err-auto-track", &[("text", Arg::Text(&text))])
            }
            ConfigError::Ignores(_) => self.say("err-git-ignores", &[]),
            ConfigError::Workspace(_) => self.say("err-jj-workspace", &[]),
        }
    }

    fn native(&self, error: &NativeError) -> String {
        match error {
            NativeError::Find { path, source } => {
                self.about_with("err-cannot-find", &shown(path), source)
            }
            NativeError::Vcs(error) => self.vcs(error),
            NativeError::Decode { path, rev, source } => {
                let detail = self.name(source);
                self.say(
                    "err-at-rev",
                    &[
                        ("path", path.as_str().into()),
                        ("rev", rev.into()),
                        ("detail", Arg::Text(&detail)),
                    ],
                )
            }
            NativeError::NeedsJj(command) => self.needs_jj(command),
        }
    }

    fn needs_jj(&self, command: &str) -> String {
        self.say("err-needs-jj", &[("command", command.into())])
    }

    fn resolve(&self, error: &ResolveError) -> String {
        match error {
            ResolveError::Vcs(error) => self.vcs(error),
            ResolveError::NotConflicted(path) => self.about("err-not-conflicted", path.as_str()),
            ResolveError::NotText { path, term, source } => {
                let detail = self.name(source);
                self.say(
                    "err-term-not-text",
                    &[
                        ("path", path.as_str().into()),
                        ("term", term.into()),
                        ("detail", Arg::Text(&detail)),
                    ],
                )
            }
            ResolveError::NotAFile { path, term, kind } => {
                let kind = self.say(
                    match kind {
                        EntryKind::Symlink => "err-kind-symlink",
                        EntryKind::Submodule => "err-kind-submodule",
                        EntryKind::Directory => "err-kind-directory",
                    },
                    &[],
                );
                self.say(
                    "err-term-not-a-file",
                    &[
                        ("path", path.as_str().into()),
                        ("term", term.into()),
                        ("kind", Arg::Text(&kind)),
                    ],
                )
            }
            ResolveError::DeletedEverywhere(path) => {
                self.about("err-deleted-everywhere", path.as_str())
            }
            ResolveError::ResolvedInWorkingTree(path) => {
                self.about("err-resolved-in-tree", path.as_str())
            }
            ResolveError::WorkingTreeNotText { path, source } => {
                self.about_with("err-tree-not-text", path.as_str(), source)
            }
            ResolveError::Read { path, source } => {
                self.about_with("err-cannot-read", path.as_str(), source)
            }
            ResolveError::Decode { path, source } => {
                self.about_with("err-in", path.as_str(), source)
            }
            ResolveError::Markers { path, source } => {
                self.about_with("err-in", path.as_str(), source)
            }
            ResolveError::Shape { path, source } => {
                self.about_with("err-in", path.as_str(), source)
            }
            ResolveError::NeedsJj(command) => self.needs_jj(command),
        }
    }

    fn dir_diff(&self, error: &DirDiffError) -> String {
        match error {
            DirDiffError::NotADirectory(path) => self.about("err-not-a-directory", &shown(path)),
            DirDiffError::Mixed { dir, other } => {
                let (dir, other) = (shown(dir), shown(other));
                self.say("err-mixed", &[("dir", Arg::Text(&dir)), ("other", Arg::Text(&other))])
            }
            DirDiffError::Read { path, .. } => self.about("err-cannot-read-plain", &shown(path)),
            DirDiffError::Vcs(error) => self.vcs(error),
        }
    }

    fn diff_tool(&self, error: &DiffToolError) -> String {
        match error {
            DiffToolError::Vcs(error) => self.vcs(error),
            DiffToolError::Read { path, .. } => self.about("err-cannot-read-plain", &shown(path)),
        }
    }

    fn semantics(&self, error: &SemanticsError) -> String {
        match error {
            SemanticsError::GitInJjRepository(path) => self.about("err-git-in-jj", &shown(path)),
            SemanticsError::GitInJjResolve(path) => {
                self.about("err-git-in-jj-resolve", &shown(path))
            }
        }
    }

    fn decode(&self, error: &DecodeError) -> String {
        let at = |id, offset: usize| self.say(id, &[("offset", offset.into())]);
        match error {
            DecodeError::Binary => self.say("err-binary", &[("bytes", BINARY_SNIFF_BYTES.into())]),
            DecodeError::InvalidUtf8 { offset } => at("err-invalid-utf8", *offset),
            DecodeError::InvalidUtf16 { offset } => at("err-invalid-utf16", *offset),
            DecodeError::OddUtf16Length => self.say("err-odd-utf16", &[]),
        }
    }

    fn encode(&self, error: &EncodeError) -> String {
        let encoding = crate::strings::encoding(error.encoding);
        let character = format!("{:?}", error.character);
        let code = format!("{:04X}", u32::from(error.character));
        self.say(
            "err-encode",
            &[
                ("encoding", Arg::Text(&encoding)),
                ("character", Arg::Text(&character)),
                ("code", Arg::Text(&code)),
                ("line", error.line.into()),
            ],
        )
    }

    fn markers(&self, error: &MarkerError) -> String {
        let at = |id, line: usize| self.say(id, &[("line", line.into())]);
        match *error {
            MarkerError::NoMarkers => self.say("err-no-markers", &[]),
            MarkerError::Unclosed { line } => at("err-unclosed", line),
            MarkerError::Nested { line } => at("err-nested", line),
            MarkerError::OutOfOrder { line } => at("err-out-of-order", line),
            MarkerError::NoSeparator { line } => at("err-no-separator", line),
            MarkerError::BadDiffLine { line } => at("err-bad-diff-line", line),
            MarkerError::Shape { line, sides, bases } => self.say(
                "err-marker-shape",
                &[("line", line.into()), ("sides", sides.into()), ("bases", bases.into())],
            ),
            MarkerError::MixedArity { line, expected, found } => self.say(
                "err-mixed-arity",
                &[("line", line.into()), ("expected", expected.into()), ("found", found.into())],
            ),
            MarkerError::TooManySides { line, sides, max } => self.say(
                "err-too-many-sides",
                &[("line", line.into()), ("sides", sides.into()), ("max", max.into())],
            ),
        }
    }

    fn shape(&self, error: &ShapeError) -> String {
        self.say("err-shape", &[("sides", error.sides.into()), ("bases", error.bases.into())])
    }

    fn journal_parse(&self, error: &ParseError) -> String {
        match error {
            ParseError::NoHeader => self.say("err-journal-header", &[]),
            ParseError::Line { index, source } => {
                let detail = self.name(source);
                self.say(
                    "err-journal-line",
                    &[("index", (*index).into()), ("detail", Arg::Text(&detail))],
                )
            }
        }
    }

    fn journal_restore(&self, error: &RestoreError) -> String {
        match error {
            RestoreError::Version { found } => {
                let (found, expected) = (found.to_string(), JOURNAL_VERSION.to_string());
                self.say(
                    "err-journal-version",
                    &[("found", Arg::Text(&found)), ("expected", Arg::Text(&expected))],
                )
            }
            RestoreError::Inputs => self.say("err-journal-inputs", &[]),
            RestoreError::Alignment => self.say("err-journal-alignment", &[]),
            RestoreError::Command { index, source } => {
                let detail = self.name(source);
                self.say(
                    "err-journal-command",
                    &[("index", (*index).into()), ("detail", Arg::Text(&detail))],
                )
            }
            RestoreError::Labels(_) => error.to_string(),
        }
    }

    fn open(&self, error: &OpenError) -> String {
        match error {
            OpenError::Window(source) => {
                let detail = source.to_string();
                self.say("err-cannot-open-window", &[("detail", Arg::Text(&detail))])
            }
            OpenError::Panes(_) => error.to_string(),
        }
    }

    fn queue(&self, error: &QueueError) -> String {
        match error {
            QueueError::Resolve(error) => self.resolve(error),
            QueueError::Open(error) => self.open(error),
            QueueError::Labels(_) => error.to_string(),
        }
    }

    /// A system error by its kind; an error the code wraps in one, by its own text.
    fn io(&self, error: &io::Error) -> String {
        if let Some(inner) = error.get_ref() {
            return self.name(inner);
        }
        let id = match error.kind() {
            io::ErrorKind::NotFound => "err-io-not-found",
            io::ErrorKind::PermissionDenied => "err-io-permission",
            io::ErrorKind::AlreadyExists => "err-io-exists",
            io::ErrorKind::IsADirectory => "err-io-is-directory",
            io::ErrorKind::NotADirectory => "err-io-not-directory",
            io::ErrorKind::StorageFull => "err-io-full",
            io::ErrorKind::ReadOnlyFilesystem => "err-io-read-only",
            _ => return error.to_string(),
        };
        self.say(id, &[])
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use merge_core::document::LabelCountError;
    use merge_core::text::Encoding;

    use super::*;

    fn write_error(kind: io::ErrorKind) -> WriteError {
        WriteError { path: PathBuf::from("out.c"), source: io::Error::from(kind) }
    }

    /// Errors with no repository behind them, one per kind of message: plain, with a path,
    /// with counts, with a nested source and with a system error inside.
    fn samples() -> Vec<Box<dyn Error + 'static>> {
        vec![
            Box::new(MarkerError::Unclosed { line: 7 }),
            Box::new(MarkerError::MixedArity { line: 9, expected: 2, found: 3 }),
            Box::new(MarkerError::TooManySides { line: 3, sides: 17, max: 16 }),
            Box::new(MarkerError::Shape { line: 4, sides: 1, bases: 1 }),
            Box::new(DecodeError::Binary),
            Box::new(DecodeError::InvalidUtf8 { offset: 12 }),
            Box::new(ShapeError { sides: 3, bases: 1 }),
            Box::new(EncodeError { encoding: Encoding::Windows1252, character: 'é', line: 2 }),
            Box::new(EncodeError { encoding: Encoding::Utf16Le, character: '\u{1F600}', line: 1 }),
            Box::new(VcsError::UnknownRevision("abc".to_owned())),
            Box::new(VcsError::NotARepository("/tmp/x".into())),
            Box::new(VcsError::BadConfig { key: "merge.conflictStyle", value: "Merge".to_owned() }),
            Box::new(JjError::NoHunk { hunk: 4, count: 3 }),
            Box::new(JjError::ConcurrentOperations(2)),
            Box::new(SemanticsError::GitInJjRepository(PathBuf::from("repo"))),
            Box::new(CannotSave::NoOutput),
            Box::new(Refused::Saving),
            Box::new(RestoreError::Version { found: JOURNAL_VERSION + 1 }),
            Box::new(RestoreError::Inputs),
            Box::new(SessionError::Read {
                path: PathBuf::from("in.c"),
                source: io::Error::other(DecodeError::OddUtf16Length),
            }),
            Box::new(SaveError::Encode(EncodeError {
                encoding: Encoding::Windows1252,
                character: '€',
                line: 3,
            })),
        ]
    }

    /// An error no message names, such as a library's, that quotes outside text.
    #[derive(Debug, thiserror::Error)]
    #[error("bad revset `a\u{202E}b`")]
    struct Quoting;

    #[test]
    fn hidden_characters_in_an_unnamed_error_show_as_escapes() {
        assert_eq!(describe_in(Locale::En, &Quoting), r"bad revset `a\u202eb`");
    }

    #[test]
    fn english_reads_as_each_errors_display() {
        for error in samples() {
            assert_eq!(describe_in(Locale::En, error.as_ref()), error.to_string());
        }
    }

    #[test]
    fn a_system_error_reads_by_its_kind() {
        let error = write_error(io::ErrorKind::NotFound);
        assert_eq!(
            describe_in(Locale::En, &error),
            "cannot write out.c: no such file or directory"
        );
        assert_eq!(
            describe_in(Locale::Es, &error),
            "no se puede escribir out.c: no existe el archivo o directorio"
        );
        let error = write_error(io::ErrorKind::PermissionDenied);
        assert_eq!(
            describe_in(Locale::Ja, &error),
            "out.c に書き込めません: アクセスが拒否されました"
        );
    }

    #[test]
    fn a_system_error_of_another_kind_keeps_its_own_text() {
        let error = write_error(io::ErrorKind::Interrupted);
        assert_eq!(
            describe_in(Locale::Es, &error),
            "no se puede escribir out.c: operation interrupted"
        );
    }

    #[test]
    fn an_error_wrapped_in_a_system_error_reads_as_itself() {
        let error = SessionError::Read {
            path: PathBuf::from("in.c"),
            source: io::Error::other(DecodeError::InvalidUtf8 { offset: 5 }),
        };
        assert_eq!(
            describe_in(Locale::Es, &error),
            "no se puede leer in.c: UTF-8 no válido en el byte 5"
        );
    }

    #[test]
    fn japanese_orders_and_counts_in_japanese() {
        assert_eq!(
            describe_in(Locale::Ja, &MarkerError::MixedArity { line: 9, expected: 2, found: 3 }),
            "9 行目: 2 面の競合の後に 3 面の競合があります"
        );
        assert_eq!(
            describe_in(Locale::Ja, &JjError::NoHunk { hunk: 4, count: 3 }),
            "ハンク 4 はありません。ハンクは 3 件です"
        );
    }

    #[test]
    fn spanish_names_the_kind_of_entry_and_keeps_paths_and_names() {
        let error = SaveError::Encode(EncodeError {
            encoding: Encoding::Windows1252,
            character: '€',
            line: 3,
        });
        assert_eq!(
            describe_in(Locale::Es, &error),
            "Windows-1252 no puede contener '€' (U+20AC) en la línea 3"
        );
        assert_eq!(
            describe_in(Locale::Es, &VcsError::UnknownRevision("abc".to_owned())),
            "revisión desconocida abc"
        );
    }

    #[test]
    fn a_bug_keeps_its_english_display() {
        let error =
            QueueError::Labels(LabelCountError { label_sides: 3, label_bases: 2, sides: 2 });
        assert_eq!(describe_in(Locale::Ja, &error), error.to_string());
    }
}
