//! The inputs of `resolve`: a conflict read straight from the repository, or else from the
//! file's conflict markers (spec §7.4).

use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use merge_core::boundaries::Boundaries;
use merge_core::document::{LabelCountError, Labels, MergeDocument, MergeOptions, Output};
use merge_core::markers::{self, MarkerError, Parsed};
use merge_core::merge3::{Inputs, Semantics, ShapeError};
use merge_core::text::{DecodeError, Text};
use merge_syntax::boundaries::SyntaxBoundaries;
use merge_syntax::language::Language;
use vcs::{ConflictTerm, EntryKind, RepoKind, Repository, TermContent, VcsError};
use vcs_jj::config::ConfigEnv;

use crate::repository::RunRepository;
use crate::{ExitCode, SaveError};

/// A conflict, loaded as a merge's inputs.
#[derive(Debug)]
pub struct Loaded {
    /// The conflicted path, relative to the repository root, or to the working directory
    /// outside a repository.
    pub path: Utf8PathBuf,
    /// The working-tree file, absolute: where the result goes.
    pub file: Utf8PathBuf,
    pub inputs: Inputs,
    /// Each term's title, or empty without one.
    pub labels: Labels,
    /// The repository holding the file; none outside one.
    pub repo: Option<RepoKind>,
    /// The shortest markers the VCS reads back as this conflict: jj's length for the file.
    /// None when any length does.
    pub marker_length: Option<usize>,
    /// Where each save is recorded: the VCS's conflict at the path. None for a file the VCS
    /// holds no conflict for.
    pub record: Option<Record>,
}

/// The VCS's conflict at one path, which records each save of its working-tree file
/// (spec §7.4): git stages a resolved file, jj snapshots the working copy.
#[derive(Debug, Clone)]
pub struct Record {
    /// The repository's root.
    root: Utf8PathBuf,
    /// The conflicted path, relative to the root.
    path: Utf8PathBuf,
    jj_config: Option<ConfigEnv>,
}

impl Record {
    /// Record a save that ended with `code` through `run`, the run's repository, as
    /// [`Record::saved`] does. `run` then reads at the save's operation, so a load right
    /// after may pass [`Snapshot::Taken`].
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] when `run` holds another repository than the
    /// conflict's, before any write; [`VcsError`] when `run` holds none or cannot be written.
    pub fn saved_in(&self, run: &mut RunRepository, code: ExitCode) -> Result<(), VcsError> {
        let Some(resolved) = resolved_by(code) else {
            return Ok(());
        };
        let repo = run.require_mut()?;
        if repo.root() != self.root {
            return Err(VcsError::OutsideRepository(self.root.join(&self.path)));
        }
        self.record(repo, resolved)
    }

    /// The conflicted path, relative to the repository root.
    #[must_use]
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Record a save that ended with `code`: 0 resolved the conflict, 1 left markers. Other
    /// codes wrote nothing to record. The repository opens afresh, at its newest state.
    ///
    /// # Errors
    ///
    /// [`VcsError`] when the repository cannot be opened or written.
    pub fn saved(&self, code: ExitCode) -> Result<(), VcsError> {
        let Some(resolved) = resolved_by(code) else {
            return Ok(());
        };
        let run = RunRepository::find([self.root.as_std_path()]);
        let mut run = match &self.jj_config {
            Some(config) => run.with_jj_config(config.clone()),
            None => run,
        };
        self.record(run.require_mut()?, resolved)
    }

    /// Record again a save whose output is written with `code`, after a [`SaveError::Record`],
    /// without writing it again.
    ///
    /// # Errors
    ///
    /// [`SaveError::Record`] when the repository refuses the record again.
    pub fn resave(&self, code: ExitCode) -> Result<ExitCode, SaveError> {
        self.saved(code).map_err(|source| self.unrecorded(code, source))?;
        Ok(code)
    }

    /// The error of a save written with `code` that the repository refused to record.
    pub(crate) fn unrecorded(&self, code: ExitCode, source: VcsError) -> SaveError {
        SaveError::Record { path: self.path.clone(), code, source }
    }

    fn record(&self, repo: &mut dyn Repository, resolved: bool) -> Result<(), VcsError> {
        repo.record_save(&self.path, resolved)?;
        tracing::info!(path = %self.path, resolved, "recorded the save in the repository");
        Ok(())
    }
}

/// Whether a save that ended with `code` resolved the conflict; `None` when it wrote
/// nothing to record.
fn resolved_by(code: ExitCode) -> Option<bool> {
    match code {
        ExitCode::Resolved => Some(true),
        ExitCode::Conflicts => Some(false),
        ExitCode::Cancelled | ExitCode::Error => None,
    }
}

/// A loaded conflict, aligned.
#[derive(Debug)]
pub struct Opened {
    pub doc: MergeDocument,
    /// As [`Loaded::path`].
    pub path: Utf8PathBuf,
    /// As [`Loaded::file`].
    pub file: Utf8PathBuf,
    pub repo: Option<RepoKind>,
    /// As [`Loaded::record`].
    pub record: Option<Record>,
}

impl Loaded {
    /// Align the inputs with `options`, its markers at least as long as the VCS reads back.
    ///
    /// # Errors
    ///
    /// [`LabelCountError`] when the labels do not fit the inputs.
    pub fn open(self, options: MergeOptions) -> Result<Opened, LabelCountError> {
        let options = MergeOptions {
            marker_length: options.marker_length.max(self.marker_length.unwrap_or_default()),
            ..options
        };
        let boundaries = Language::detect([self.path.as_std_path()]).map(SyntaxBoundaries::new);
        Ok(Opened {
            doc: MergeDocument::new_bounded(
                self.inputs,
                options,
                self.labels,
                boundaries.as_ref().map(|b| b as &dyn Boundaries),
            )?,
            path: self.path,
            file: self.file,
            repo: self.repo,
            record: self.record,
        })
    }
}

/// The conflicted paths of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflicted {
    pub root: Utf8PathBuf,
    /// Relative to `root`, in path order.
    pub paths: Vec<Utf8PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("{0} is not conflicted")]
    NotConflicted(Utf8PathBuf),
    #[error("{path}: {term} is not text: {source}")]
    NotText { path: Utf8PathBuf, term: String, source: DecodeError },
    #[error("{path}: {term} is a {}; only text conflicts can be resolved", kind_name(*.kind))]
    NotAFile { path: Utf8PathBuf, term: String, kind: EntryKind },
    #[error("{0} is deleted on every side; nothing to merge")]
    DeletedEverywhere(Utf8PathBuf),
    #[error(
        "{0} has no conflict markers left in the working tree, so it looks resolved; mark it resolved, or put its markers back to merge it again"
    )]
    ResolvedInWorkingTree(Utf8PathBuf),
    #[error("cannot read {path}: {source}")]
    Read { path: Utf8PathBuf, source: std::io::Error },
    #[error("{path}: {source}")]
    Decode { path: Utf8PathBuf, source: DecodeError },
    #[error("{path}: {source}")]
    Markers { path: Utf8PathBuf, source: MarkerError },
    #[error("{path}: {source}")]
    Shape { path: Utf8PathBuf, source: ShapeError },
}

impl ResolveError {
    /// Whether the error is about the conflicted file itself, so a run over every conflicted
    /// file skips it and goes on.
    #[must_use]
    pub fn is_refusal(&self) -> bool {
        matches!(
            self,
            Self::NotText { .. }
                | Self::NotAFile { .. }
                | Self::DeletedEverywhere(_)
                | Self::ResolvedInWorkingTree(_)
                | Self::Markers { .. }
        )
    }
}

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Symlink => "symlink",
        EntryKind::Submodule => "submodule",
        EntryKind::Directory => "directory",
    }
}

/// The conflicted paths of the run's repository, after jj takes in the working tree
/// ([`vcs::Repository::snapshot`]). The repository stays open for the steps after.
///
/// # Errors
///
/// [`ResolveError::Vcs`] when there is no repository, the snapshot fails, or the conflicts
/// cannot be read.
pub fn conflicted(repo: &mut RunRepository) -> Result<Conflicted, ResolveError> {
    let repo = repo.require_mut()?;
    repo.snapshot()?;
    let paths = repo.conflicts()?;
    Ok(Conflicted { root: repo.root().to_owned(), paths })
}

/// Whether [`load`] takes in the working tree first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snapshot {
    /// Snapshot the working copy, as a jj command does.
    Take,
    /// The caller snapshotted the same [`RunRepository`] just before, through [`conflicted`]
    /// or [`Record::saved_in`], with nothing to take in since: read at that snapshot without
    /// scanning the working tree again. A caller that waits on the user in between takes it.
    Taken,
}

/// The conflict at `path`, relative to `cwd`: from the run's repository
/// ([`RunRepository::of_resolve`]), which stays open for the steps after, else from the
/// file's conflict markers, also outside any repository. jj first takes in the working tree
/// ([`vcs::Repository::snapshot`]), so the conflict holds the user's unrecorded edits.
///
/// # Errors
///
/// [`ResolveError::Vcs`] when `path` lies outside the run's repository, the snapshot fails,
/// or the conflict cannot be read; [`ResolveError::NotText`] and [`ResolveError::NotAFile`] for a term with
/// no text to merge; [`ResolveError::NotConflicted`] for a file without markers;
/// [`ResolveError::Read`], [`ResolveError::Decode`] and [`ResolveError::Markers`] when the
/// file cannot be read, is not text, or its markers are malformed.
pub fn load(
    run: &mut RunRepository,
    cwd: &Path,
    path: &Path,
    snapshot: Snapshot,
) -> Result<Loaded, ResolveError> {
    let file = absolute(&cwd.join(path))?;
    let jj_config = run.jj_config().cloned();
    let Some(repo) = run.open_mut()? else {
        let cwd = absolute(cwd)?;
        let shown = file.strip_prefix(&cwd).map_or_else(|_| file.clone(), Utf8Path::to_owned);
        return from_markers(shown, file, None);
    };
    // Discovery canonicalises, so the root shares `file`'s form.
    let relative = file
        .strip_prefix(repo.root())
        .map_err(|_| VcsError::OutsideRepository(file.clone()))?
        .to_owned();
    if snapshot == Snapshot::Take {
        repo.snapshot()?;
    }
    let Some(conflict) = repo.read_conflict(&relative)? else {
        return from_markers(relative, file, Some(repo.kind()));
    };
    let recorded = repo.recorded(&relative)?;
    let record = Some(Record { root: repo.root().to_owned(), path: relative.clone(), jj_config });
    if conflict.sides.iter().all(|side| side.content == TermContent::Absent) {
        return Err(ResolveError::DeletedEverywhere(relative));
    }
    let working = working_text(&file, &conflict.sides);
    if working.as_ref().is_some_and(|text| !markers::has_conflict_start(text.content())) {
        return Err(ResolveError::ResolvedInWorkingTree(relative));
    }
    let decode = |role: &str, terms: Vec<ConflictTerm>| -> Result<_, ResolveError> {
        terms
            .into_iter()
            .enumerate()
            .map(|(i, term)| text(&relative, &format!("{role} {}", i + 1), term))
            .collect::<Result<(Vec<Text>, Vec<String>), _>>()
    };
    let (sides, side_labels) = decode("side", conflict.sides)?;
    let (bases, base_labels) = decode("base", conflict.bases)?;
    let inputs = Inputs::new(sides, bases)
        .map_err(|source| ResolveError::Shape { path: relative.clone(), source })?;
    // A VCS without a record of the working-tree file, such as git, cannot say whether it
    // holds edits. Markers that differ from the stages' conflict hold some, so they win.
    if recorded.is_none()
        && let Some(text) = working
    {
        let parsed = parse_markers(&relative, &text)?;
        if !same_conflict(&parsed, &inputs) {
            tracing::info!(path = %relative, "the working-tree file differs from the stages; opening it from its markers");
            return Ok(Loaded { record, ..from_parsed(relative, file, Some(repo.kind()), parsed) });
        }
    }
    // jj parses back only markers at least as long as its materialiser writes them.
    let marker_length = recorded
        .and_then(|r| r.marker_length)
        .or_else(|| (repo.kind() == RepoKind::Jj).then(|| merge_core::jj::marker_length(&inputs)));
    Ok(Loaded {
        path: relative,
        file,
        inputs,
        labels: Labels::new(side_labels, base_labels),
        repo: Some(repo.kind()),
        marker_length,
        record,
    })
}

/// The working-tree `file` of a conflict with `sides` as text, when the VCS wrote markers
/// there because every side is a file. None for a file that is missing, unreadable or not
/// text: it holds no work a save could destroy.
fn working_text(file: &Utf8Path, sides: &[ConflictTerm]) -> Option<Text> {
    let markers_written = sides.iter().all(|side| matches!(side.content, TermContent::File { .. }));
    if !markers_written {
        return None;
    }
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(%error, %file, "cannot read the working-tree file; the conflict loads from the repository");
            return None;
        }
    };
    Text::decode(bytes).ok()
}

/// Whether `parsed`, a working-tree file's markers, shows the conflict of `inputs` with no
/// edits: each side it rebuilds equals the side triad's git-semantics merge of `inputs`
/// rebuilds. Labels, marker length and style, and lines moved out of a conflict, as zdiff3
/// moves them, do not count. Bases do not count either: the `merge` style writes none.
fn same_conflict(parsed: &Parsed, inputs: &Inputs) -> bool {
    let options = MergeOptions { semantics: Semantics::Git, ..MergeOptions::default() };
    // Texts share their content, so the copy is O(1).
    let doc = MergeDocument::new(inputs.clone(), options, Labels::none())
        .expect("no labels fit any inputs");
    let Output::Conflicted(merged) = doc.output() else {
        return false;
    };
    Text::decode(merged.into_bytes())
        .ok()
        .and_then(|merged| markers::parse(&merged).ok())
        .is_some_and(|expected| sides(&expected).eq(sides(parsed)))
}

/// The content of each side `parsed` rebuilds.
fn sides(parsed: &Parsed) -> impl Iterator<Item = &str> {
    parsed.inputs.sides().iter().map(Text::content)
}

/// The conflict in `file`'s markers, `path` naming it in errors.
fn from_markers(
    path: Utf8PathBuf,
    file: Utf8PathBuf,
    repo: Option<RepoKind>,
) -> Result<Loaded, ResolveError> {
    let bytes = match std::fs::read(&file) {
        Ok(bytes) => bytes,
        Err(source) => return Err(ResolveError::Read { path, source }),
    };
    let text = match Text::decode(bytes) {
        Ok(text) => text,
        Err(source) => return Err(ResolveError::Decode { path, source }),
    };
    let parsed = parse_markers(&path, &text)?;
    Ok(from_parsed(path, file, repo, parsed))
}

/// `text`'s conflict markers, `path` naming the file in errors.
fn parse_markers(path: &Utf8Path, text: &Text) -> Result<Parsed, ResolveError> {
    markers::parse(text).map_err(|source| match source {
        MarkerError::NoMarkers => ResolveError::NotConflicted(path.to_owned()),
        source => ResolveError::Markers { path: path.to_owned(), source },
    })
}

fn from_parsed(
    path: Utf8PathBuf,
    file: Utf8PathBuf,
    repo: Option<RepoKind>,
    parsed: Parsed,
) -> Loaded {
    Loaded {
        path,
        file,
        inputs: parsed.inputs,
        labels: parsed.labels,
        repo,
        // The VCS reads the markers back at any length.
        marker_length: None,
        record: None,
    }
}

/// One term's text and label. `position` names the term in an error, with its title.
fn text(
    path: &Utf8Path,
    position: &str,
    ConflictTerm { term, content }: ConflictTerm,
) -> Result<(Text, String), ResolveError> {
    let title = term.title().map(std::borrow::Cow::into_owned);
    let name = || match &title {
        Some(title) => format!("{position} ({title})"),
        None => position.to_owned(),
    };
    let bytes = match content {
        TermContent::File { bytes, .. } => bytes,
        TermContent::Absent => Vec::new(),
        TermContent::Other(kind) => {
            return Err(ResolveError::NotAFile { path: path.to_owned(), term: name(), kind });
        }
    };
    let text = Text::decode(bytes).map_err(|source| ResolveError::NotText {
        path: path.to_owned(),
        term: name(),
        source,
    })?;
    Ok((text, title.unwrap_or_default()))
}

/// The nearest existing directory holding `file`, absolute.
pub(crate) fn nearest_dir(file: &Utf8Path) -> &Utf8Path {
    file.ancestors().skip(1).find(|dir| dir.is_dir()).unwrap_or(file)
}

/// `path` made absolute with links resolved. A path that does not exist, such as a file
/// deleted in the working tree, resolves through its nearest existing ancestor.
pub(crate) fn absolute(path: &Path) -> Result<Utf8PathBuf, VcsError> {
    let mut existing = path;
    let mut missing = Vec::new();
    let resolved = loop {
        match dunce::canonicalize(existing) {
            Ok(dir) => break dir,
            Err(_) => match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    missing.push(name);
                    existing = parent;
                }
                _ => return Err(VcsError::NotARepository(lossy(path))),
            },
        }
    };
    let resolved: PathBuf = missing.into_iter().rev().fold(resolved, |dir, name| dir.join(name));
    Utf8PathBuf::from_path_buf(resolved).map_err(VcsError::NotUtf8)
}

fn lossy(path: &Path) -> Utf8PathBuf {
    Utf8PathBuf::from(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A conflict whose sides share the `}` that closes a different block on each.
    fn shared_brace(path: &str) -> Loaded {
        let text = |s: &str| Text::decode(s.as_bytes().to_vec()).expect("decodes");
        Loaded {
            path: Utf8PathBuf::from(path),
            file: Utf8PathBuf::from("/repo").join(path),
            inputs: Inputs::three_way(
                text("fn f() {\n    a();\n}\n"),
                text("fn f() {\n    if p {\n        b();\n    }\n    c();\n}\n"),
                text("fn f() {\n    while q {\n        d();\n    }\n    e();\n}\n"),
            ),
            labels: Labels::none(),
            repo: None,
            marker_length: None,
            record: None,
        }
    }

    #[test]
    fn the_conflicted_paths_language_gives_the_syntax_boundaries() {
        let options = MergeOptions {
            refine: merge_core::merge3::Refine::Deep,
            syntax_boundaries: true,
            ..MergeOptions::default()
        };
        let regions =
            |path| shared_brace(path).open(options).expect("no labels").doc.regions().len();
        // Unchanged, conflict, unchanged: the shared `}` stays inside the conflict.
        assert_eq!(regions("src/f.rs"), 3);
        // No grammar: the `}` splits off between two conflicts.
        assert_eq!(regions("src/f.txt"), 5);
    }

    #[test]
    fn a_missing_file_resolves_through_its_nearest_existing_directory() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = dunce::canonicalize(dir.path()).expect("canonical");
        let resolved = absolute(&dir.path().join("gone/deeper/a.txt")).expect("resolves");
        assert_eq!(resolved.as_std_path(), root.join("gone/deeper/a.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_is_refused_by_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt as _;

        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = dunce::canonicalize(dir.path()).expect("canonical");
        let name = OsStr::from_bytes(b"\xff.txt");
        let path = dir.path().join(name);
        let in_memory = Path::new(name);
        let listed = conflicted(&mut RunRepository::find([in_memory]));
        assert!(
            matches!(&listed, Err(ResolveError::Vcs(VcsError::NotUtf8(p))) if p == in_memory),
            "{listed:?}"
        );
        assert!(
            matches!(absolute(&path), Err(VcsError::NotUtf8(p)) if p == root.join(name)),
            "{:?}",
            absolute(&path)
        );
    }

    #[test]
    fn a_relative_path_with_no_existing_ancestor_names_itself() {
        let path = Path::new("no-such-directory-for-triad/a.txt");
        assert!(
            matches!(
                absolute(path),
                Err(VcsError::NotARepository(p)) if p == "no-such-directory-for-triad/a.txt"
            ),
            "{:?}",
            absolute(path)
        );
    }
}
