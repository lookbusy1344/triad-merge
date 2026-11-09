//! The `Repository` trait and the types shared by the git and jj backends. Types follow jj's model: n sides, n − 1 bases, an optional label per term.

mod annotation;
#[cfg(feature = "testing")]
pub mod testing;

pub use annotation::{Annotation, LineOrigin, OriginRun, Timestamp};

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use camino::{Utf8Path, Utf8PathBuf};

/// The version control system behind a repository. A colocated repository is `Jj`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoKind {
    Git,
    Jj,
}

/// A revision to read a file at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevSpec {
    /// git's index: the staged version. jj has none.
    Index,
    /// A revision in the backend's own syntax: `HEAD~1` for git, `@-` for jj.
    Expr(String),
    /// One commit, by its full ID. The backend looks it up without parsing an expression.
    Commit(CommitId),
}

impl fmt::Display for RevSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Index => f.write_str("index"),
            Self::Expr(expr) => f.write_str(expr),
            Self::Commit(id) => f.write_str(id.as_str()),
        }
    }
}

/// The full ID of a commit, in hex. It is also a revision expression that names the commit;
/// [`RevSpec::Commit`] and the methods that take one look it up without parsing.
///
/// ```
/// # use vcs::{CommitId, Repository, VcsError};
/// fn parents(repo: &dyn Repository, id: &CommitId) -> Result<Vec<CommitId>, VcsError> {
///     repo.commits(id.as_str())
/// }
/// ```
///
/// A revision expression is not a commit ID, and a commit ID says so when it becomes one:
///
/// ```compile_fail,E0308
/// # use vcs::{CommitId, Repository, VcsError};
/// fn parents(repo: &dyn Repository, id: &CommitId) -> Result<Vec<CommitId>, VcsError> {
///     repo.commits(id)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitId(String);

impl CommitId {
    #[must_use]
    pub fn new(hex: String) -> Self {
        Self(hex)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A commit, as the VCS records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeInfo {
    /// The commit ID, shortened by the backend.
    pub commit: String,
    /// jj's change ID, shortened by the backend. git has none.
    pub change_id: Option<String>,
    /// The first line of the description.
    pub summary: Option<String>,
}

/// Which ID names a revision where jj gives two: the change ID, stable while the change is
/// rewritten, or the commit ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdKind {
    #[default]
    Change,
    Commit,
}

impl ChangeInfo {
    /// The ID of `kind`: the change ID, or the commit where there is none (git), or the
    /// commit.
    #[must_use]
    pub fn id(&self, kind: IdKind) -> &str {
        match kind {
            IdKind::Change => self.change_id.as_deref().unwrap_or(&self.commit),
            IdKind::Commit => &self.commit,
        }
    }
}

/// What the VCS knows about one side or base.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Term {
    /// A name the VCS gives the term: `index`, a branch, a jj conflict label.
    pub label: Option<String>,
    pub change: Option<ChangeInfo>,
}

impl Term {
    /// The pane title: the label; else the change ID, or the commit without one, then the
    /// summary; none when the VCS knows nothing.
    #[must_use]
    pub fn title(&self) -> Option<Cow<'_, str>> {
        if let Some(label) = self.label.as_deref().filter(|l| !l.is_empty()) {
            return Some(Cow::Borrowed(label));
        }
        let change = self.change.as_ref()?;
        let id = change.change_id.as_deref().unwrap_or(&change.commit);
        Some(match change.summary.as_deref().filter(|s| !s.is_empty()) {
            Some(summary) => Cow::Owned(format!("{id} {summary}")),
            None => Cow::Borrowed(id),
        })
    }
}

/// The revision a working-copy file is compared with by default, and what the VCS knows
/// about it. git: the index when the file has staged changes, otherwise `HEAD`. jj: `@-`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parent {
    pub rev: RevSpec,
    pub term: Term,
}

/// What one term of a conflict holds at the conflicted path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermContent {
    File {
        bytes: Vec<u8>,
        executable: bool,
    },
    /// No entry: the file was added on another term, or deleted on this one.
    Absent,
    /// An entry with no text to merge.
    Other(EntryKind),
}

/// The kinds of entry a conflict term may hold besides a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Symlink,
    Submodule,
    Directory,
}

/// One side or base of a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictTerm {
    pub term: Term,
    pub content: TermContent,
}

/// A conflicted path: n sides and n − 1 bases, in jj's order. git gives two sides, stage 2
/// (ours) then stage 3 (theirs), and one base, stage 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: Utf8PathBuf,
    pub sides: Vec<ConflictTerm>,
    pub bases: Vec<ConflictTerm>,
}

/// The commits each term of a merge in the working copy may hold, each list in the order to
/// try (spec §7.3). A term tries its own commits, then the shared ones. Terms follow jj's
/// order, so a merge of any width finds its terms' lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeCandidates {
    /// Each base's own commits, by its index; a base past the end has none.
    pub bases: Vec<Vec<CommitId>>,
    /// Each side's own commits, by its index; a side past the end has none.
    pub sides: Vec<Vec<CommitId>>,
    /// The commits every term may hold, after its own.
    pub shared: Vec<CommitId>,
}

impl MergeCandidates {
    /// The commits base `n` may hold, in the order to try.
    pub fn base(&self, n: usize) -> impl Iterator<Item = &CommitId> {
        Self::term(&self.bases, n).chain(&self.shared)
    }

    /// The commits side `n` may hold, in the order to try.
    pub fn side(&self, n: usize) -> impl Iterator<Item = &CommitId> {
        Self::term(&self.sides, n).chain(&self.shared)
    }

    fn term(own: &[Vec<CommitId>], n: usize) -> impl Iterator<Item = &CommitId> {
        own.get(n).into_iter().flatten()
    }
}

/// What the VCS recorded about a working-tree file when it last read the working tree: jj's
/// snapshot. git records nothing a merge needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    /// The file's modification time or size differs from the record: it holds edits the
    /// VCS has not taken in. A file deleted since counts as unchanged.
    pub changed: bool,
    /// The conflict marker length the VCS wrote the file with, when it wrote markers.
    pub marker_length: Option<usize>,
}

/// The conflict markers a VCS's config asks for: git's `merge.conflictStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStyle {
    /// Both sides, without the base.
    Merge,
    /// Both sides and the base.
    Diff3,
    /// As `Diff3`, with lines common to both sides moved out of the conflict.
    Zdiff3,
}

/// Why a repository operation failed.
#[derive(Debug, thiserror::Error)]
pub enum VcsError {
    #[error("{0} is not inside a git or jj repository")]
    NotARepository(Utf8PathBuf),
    #[error("{} is not a UTF-8 path", .0.display())]
    NotUtf8(PathBuf),
    #[error("{0} has no working copy")]
    NoWorkingCopy(Utf8PathBuf),
    #[error("{0} is outside the repository")]
    OutsideRepository(Utf8PathBuf),
    /// The file on disk was materialised from another version than the working-copy
    /// commit's, as when `@` moved in another workspace.
    #[error(
        "{0} on disk is not the working-copy commit's; run jj workspace update-stale and try again"
    )]
    StaleWorkingCopy(Utf8PathBuf),
    /// The working copy's record is of an operation behind or beside the repository's, so
    /// a snapshot would lose or mix changes.
    #[error("the working copy at {0} is stale; run jj workspace update-stale and try again")]
    StaleWorkspace(Utf8PathBuf),
    #[error("unknown revision {0}")]
    UnknownRevision(String),
    /// A short ID that matches more than one commit, or a jj change or name with several targets.
    #[error("revision {0} is ambiguous")]
    AmbiguousRevision(String),
    /// The file holds an unresolved conflict at the revision (jj records conflicts in commits).
    #[error("{path} is conflicted at {rev}")]
    Conflicted { path: Utf8PathBuf, rev: String },
    #[error("{0} has no index")]
    NoIndex(&'static str),
    /// A config value the VCS itself would refuse.
    #[error("{key} has the unknown value {value:?}")]
    BadConfig { key: &'static str, value: String },
    /// An error from gix or jj-lib, whose types this crate does not name.
    #[error("{context}: {source}")]
    Backend {
        context: &'static str,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

/// A git or jj repository, read through gix or jj-lib. `Send`, so a run hands its open
/// repository to the thread that looks up line origins.
pub trait Repository: Send {
    fn kind(&self) -> RepoKind;

    /// The working copy's root directory, which paths are relative to.
    fn root(&self) -> &Utf8Path;

    /// The bytes of `path`, relative to the repository root, at `rev`. `None` when the file
    /// does not exist at that revision.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] for a path with `..`, a leading `.` or a root,
    /// [`VcsError::UnknownRevision`] when `rev` does not resolve,
    /// [`VcsError::AmbiguousRevision`] when a prefix or name in it is ambiguous,
    /// [`VcsError::NoIndex`] for [`RevSpec::Index`] on jj, [`VcsError::Conflicted`] when the
    /// file is conflicted at `rev`, and [`VcsError::Backend`] when reading fails.
    fn file_at(&self, path: &Utf8Path, rev: &RevSpec) -> Result<Option<Vec<u8>>, VcsError>;

    /// The revision `path`'s working-copy version is compared with by default.
    ///
    /// # Errors
    ///
    /// [`VcsError::Backend`] when the repository cannot be read.
    fn default_parent(&self, path: &Utf8Path) -> Result<Parent, VcsError>;

    /// The commit that last changed each line of `path` at the commit `commit`, following
    /// every parent. `None` when the file does not exist at that commit.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] as [`Repository::file_at`],
    /// [`VcsError::UnknownRevision`] when the repository holds no commit `commit`, and
    /// [`VcsError::Backend`] when the history cannot be read.
    fn annotate(&self, path: &Utf8Path, commit: &CommitId) -> Result<Option<Annotation>, VcsError>;

    /// The change, author and author time of the commit `id`, shortened as an annotation's
    /// origins are.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] when the repository holds no commit `id`, and
    /// [`VcsError::Backend`] when the commit cannot be read.
    fn describe(&self, id: &CommitId) -> Result<LineOrigin, VcsError>;

    /// The full IDs, in hex, of the commits the revision expression `rev` names; at least one.
    /// A git expression names one commit; a jj revset may name several.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] when `rev` names no commit,
    /// [`VcsError::AmbiguousRevision`] when a prefix or name in it is ambiguous, and
    /// [`VcsError::Backend`] when the history cannot be read.
    fn commits(&self, rev: &str) -> Result<Vec<CommitId>, VcsError>;

    /// The full ID of the nearest common ancestor of the commits `one` and `two`. `None` when
    /// they have none, or several that are not ancestors of one another (a criss-cross merge),
    /// since no one commit is then the base.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] when the repository holds no commit `one` or `two`, and
    /// [`VcsError::Backend`] when the history cannot be read.
    fn merge_base(&self, one: &CommitId, two: &CommitId) -> Result<Option<CommitId>, VcsError>;

    /// The conflicted paths in the working copy, in path order, component by component. git:
    /// the paths the index holds at a stage above 0. jj: the working-copy commit's conflicted
    /// paths.
    ///
    /// # Errors
    ///
    /// [`VcsError::NotUtf8`] for a conflicted path that is not UTF-8, and
    /// [`VcsError::Backend`] when the index or the tree cannot be read.
    fn conflicts(&self) -> Result<Vec<Utf8PathBuf>, VcsError>;

    /// The conflict at `path` in the working copy; `None` when `path` is not conflicted. git
    /// reads the index's stages; jj, the working-copy commit's tree.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] for a path that is not inside the repository, and
    /// [`VcsError::Backend`] when the index, the tree or a term's content cannot be read.
    fn read_conflict(&self, path: &Utf8Path) -> Result<Option<Conflict>, VcsError>;

    /// What the VCS recorded about the working-tree file at `path` when it last read the
    /// working tree; `None` when it recorded nothing for the file. git: always `None`.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] for a path that is not inside the repository, and
    /// [`VcsError::Backend`] when the record or the file's metadata cannot be read.
    fn recorded(&self, path: &Utf8Path) -> Result<Option<Recorded>, VcsError>;

    /// The commits each term of a merge in the working copy may hold (spec §7.3). git: side 0
    /// (ours) is `HEAD`; base 0 and side 1 (theirs) follow the operation stopped on the
    /// conflict, and are empty without one; git merges no wider. jj: every term shares `@`,
    /// each parent of `@` in order, and with several parents their one common merge base.
    ///
    /// # Errors
    ///
    /// [`VcsError::Backend`] when the history cannot be read.
    fn merge_candidates(&self) -> Result<MergeCandidates, VcsError>;

    /// The commits a diff tool's copy of a working-copy file may hold, in the order to try.
    /// git: `HEAD`. jj: `@-` when it is one commit, then `@`.
    ///
    /// # Errors
    ///
    /// [`VcsError::Backend`] when the history cannot be read.
    fn copy_candidates(&self) -> Result<Vec<CommitId>, VcsError>;

    /// The working copy's commit: jj's `@`. git's working tree is no commit: `None`.
    ///
    /// # Errors
    ///
    /// [`VcsError::Backend`] when the working copy cannot be read.
    fn working_copy_id(&self) -> Result<Option<CommitId>, VcsError>;

    /// The commit the index stages changes on, whose history the index's lines take: git's
    /// `HEAD`. `None` before the first commit, and for jj, which has no index.
    ///
    /// # Errors
    ///
    /// [`VcsError::Backend`] when `HEAD` cannot be read.
    fn index_base(&self) -> Result<Option<CommitId>, VcsError>;

    /// The conflict marker style the VCS's config asks for: git's `merge.conflictStyle`.
    /// `None` when no config sets it, and for jj, whose merges always write diff3 markers.
    ///
    /// # Errors
    ///
    /// [`VcsError::BadConfig`] for a value the VCS does not know.
    fn conflict_style(&self) -> Result<Option<ConflictStyle>, VcsError>;

    /// Take in the working tree as the VCS's next command would, before a conflict is read.
    /// jj snapshots the working copy with the user's jj config, in an operation of its own
    /// when anything changed, and reads at that operation afterwards. In a colocated
    /// workspace it first imports a moved git `HEAD`, as jj does, in an operation of its own.
    /// git records nothing.
    ///
    /// # Errors
    ///
    /// [`VcsError::StaleWorkspace`] when the working copy is behind or beside the
    /// repository's operation, before any write, and [`VcsError::Backend`] when the config,
    /// the working copy or the operation cannot be read or written.
    fn snapshot(&mut self) -> Result<(), VcsError>;

    /// Record a save of the conflicted `path`'s working-tree file; `resolved` when the save
    /// left no conflict. git: a resolved file replaces the conflict's stages with one stage-0
    /// entry, as `git add` does, and a file with markers leaves them. jj: the working copy is
    /// snapshotted again, as [`Repository::snapshot`] does, in an operation of its own, so
    /// jj reads markers back as the conflict and `jj undo` reverts the save alone.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] for a path that is not inside the repository, the
    /// errors of [`Repository::snapshot`], and [`VcsError::Backend`] when the file, the index
    /// or an object cannot be read or written.
    fn record_save(&mut self, path: &Utf8Path, resolved: bool) -> Result<(), VcsError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(commit: &str, change_id: Option<&str>, summary: Option<&str>) -> ChangeInfo {
        ChangeInfo {
            commit: commit.to_owned(),
            change_id: change_id.map(str::to_owned),
            summary: summary.map(str::to_owned),
        }
    }

    fn title(term: &Term) -> Option<String> {
        term.title().map(Cow::into_owned)
    }

    fn ids(hex: &[&str]) -> Vec<CommitId> {
        hex.iter().map(|h| CommitId::new((*h).to_owned())).collect()
    }

    fn listed<'a>(commits: impl Iterator<Item = &'a CommitId>) -> Vec<&'a str> {
        commits.map(CommitId::as_str).collect()
    }

    #[test]
    fn a_term_tries_its_own_commits_then_the_shared_ones() {
        let candidates = MergeCandidates {
            bases: vec![ids(&["b0"])],
            sides: vec![ids(&["s0"]), ids(&["s1a", "s1b"])],
            shared: ids(&["at", "parent"]),
        };
        assert_eq!(listed(candidates.base(0)), ["b0", "at", "parent"]);
        assert_eq!(listed(candidates.side(0)), ["s0", "at", "parent"]);
        assert_eq!(listed(candidates.side(1)), ["s1a", "s1b", "at", "parent"]);
    }

    #[test]
    fn a_term_past_the_lists_tries_the_shared_commits_alone() {
        let shared = MergeCandidates { shared: ids(&["at"]), ..MergeCandidates::default() };
        assert_eq!(listed(shared.base(1)), ["at"]);
        assert_eq!(listed(shared.side(2)), ["at"]);
        let git = MergeCandidates {
            bases: vec![ids(&["b0"])],
            sides: vec![ids(&["s0"]), ids(&["s1"])],
            shared: Vec::new(),
        };
        assert_eq!(listed(git.side(2)), Vec::<&str>::new());
        assert_eq!(listed(git.base(1)), Vec::<&str>::new());
    }

    #[test]
    fn a_commit_id_reads_and_prints_as_its_hex() {
        let id = CommitId::new("abc123".to_owned());
        assert_eq!(id.as_str(), "abc123");
        assert_eq!(id.to_string(), "abc123");
        assert_eq!(id.into_string(), "abc123");
    }

    #[test]
    fn jj_names_a_revision_by_either_id() {
        let jj = change("1a2b3c", Some("kxqv"), None);
        assert_eq!(jj.id(IdKind::Change), "kxqv");
        assert_eq!(jj.id(IdKind::Commit), "1a2b3c");
    }

    #[test]
    fn git_names_a_revision_by_its_commit_under_either_kind() {
        let git = change("1a2b3c", None, None);
        assert_eq!(git.id(IdKind::Change), "1a2b3c");
        assert_eq!(git.id(IdKind::Commit), "1a2b3c");
    }

    #[test]
    fn the_change_id_is_the_default() {
        assert_eq!(IdKind::default(), IdKind::Change);
    }

    #[test]
    fn a_label_titles_the_term() {
        let term = Term {
            label: Some("main".to_owned()),
            change: Some(change("1a2b3c", None, Some("fix parsing"))),
        };
        assert_eq!(title(&term).as_deref(), Some("main"));
    }

    #[test]
    fn jj_titles_with_the_change_id_and_summary() {
        let term =
            Term { label: None, change: Some(change("1a2b3c", Some("kxqv"), Some("fix parsing"))) };
        assert_eq!(title(&term).as_deref(), Some("kxqv fix parsing"));
    }

    #[test]
    fn git_titles_with_the_commit_and_summary() {
        let term = Term { label: None, change: Some(change("1a2b3c", None, Some("fix parsing"))) };
        assert_eq!(title(&term).as_deref(), Some("1a2b3c fix parsing"));
    }

    #[test]
    fn empty_label_and_summary_count_as_absent() {
        let term = Term {
            label: Some(String::new()),
            change: Some(change("1a2b3c", Some("kxqv"), Some(""))),
        };
        assert_eq!(title(&term).as_deref(), Some("kxqv"));
    }

    #[test]
    fn an_empty_term_has_no_title() {
        assert_eq!(title(&Term::default()), None);
        let label_only_empty = Term { label: Some(String::new()), change: None };
        assert_eq!(title(&label_only_empty), None);
    }

    #[test]
    fn a_revision_shows_as_the_user_would_type_it() {
        assert_eq!(RevSpec::Index.to_string(), "index");
        assert_eq!(RevSpec::Expr("HEAD~1".to_owned()).to_string(), "HEAD~1");
    }

    #[test]
    fn errors_name_what_failed() {
        assert_eq!(
            VcsError::NotARepository("/tmp/x".into()).to_string(),
            "/tmp/x is not inside a git or jj repository"
        );
        assert_eq!(VcsError::NoIndex("jj").to_string(), "jj has no index");
        assert_eq!(
            VcsError::BadConfig { key: "merge.conflictStyle", value: "Merge".to_owned() }
                .to_string(),
            r#"merge.conflictStyle has the unknown value "Merge""#
        );
        assert_eq!(
            VcsError::AmbiguousRevision("k".to_owned()).to_string(),
            "revision k is ambiguous"
        );
        assert_eq!(
            VcsError::Conflicted { path: "src/a.rs".into(), rev: "@-".to_owned() }.to_string(),
            "src/a.rs is conflicted at @-"
        );
        let backend =
            VcsError::Backend { context: "reading HEAD", source: "object missing".into() };
        assert_eq!(backend.to_string(), "reading HEAD: object missing");
        assert_eq!(backend.source().map(ToString::to_string).as_deref(), Some("object missing"));
    }
}
