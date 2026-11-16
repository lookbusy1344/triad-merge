//! The inputs of `diff PATH`: the working-copy file and its parent revision (spec §6.1).

use std::io;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use merge_core::text::{DecodeError, Text};
use vcs::{RepoKind, Repository, RevSpec, VcsError};
use vcs_git::GitRepository;
use vcs_jj::JjRepository;
use vcs_jj::config::ConfigEnv;

use crate::dir_diff::{self, Change, ChangedFile, Content};
use crate::repository::RunRepository;

/// What `diff PATH` shows: a revision of the file on the left, the working copy on the right.
#[derive(Debug)]
pub struct NativeDiff {
    /// The file at the revision; empty when the file does not exist there.
    pub left: Text,
    /// The left pane's title: the VCS's label for the revision, such as git's branch or
    /// `index`, or else the revision itself, then [`NO_FILE`] when the file does not exist
    /// there.
    pub left_title: String,
    pub right: PathBuf,
}

/// Ends the left title when the file does not exist at the revision.
pub const NO_FILE: &str = "(no file)";

#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    #[error("cannot find {}: {source}", path.display())]
    Find { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("{path} at {rev}: {source}")]
    Decode { path: Utf8PathBuf, rev: String, source: DecodeError },
    /// A jj-native option outside a jj repository (spec §1.2).
    #[error("{0} needs a jj repository")]
    NeedsJj(&'static str),
}

/// The left title's revision for `diff --op`: the working copy at operation `short`.
#[must_use]
pub fn operation_title(short: &str) -> String {
    format!("@ at operation {short}")
}

/// The repository holding `start`, as [`nearest_repo`] finds it from `start` made canonical,
/// opened by its backend at its root.
///
/// # Errors
///
/// [`VcsError::NotARepository`] when `start` does not exist or no ancestor of it is a
/// repository, and for a `.git` that git cannot open; any error from opening the repository.
pub fn discover(start: &Utf8Path) -> Result<Box<dyn Repository>, VcsError> {
    let repo =
        find_from(start.as_std_path()).ok_or_else(|| VcsError::NotARepository(start.to_owned()))?;
    open(&repo, None)
}

/// The repository holding `dir` made canonical, as [`nearest_repo`] finds it; none for a
/// directory that does not exist or is not UTF-8. The root then comes out canonical, so roots
/// compare.
pub(crate) fn find_from(dir: &Path) -> Option<Repo> {
    let canonical = canonical(Utf8Path::from_path(dir)?).ok()?;
    nearest_repo(canonical.as_std_path())
}

/// The repository `repo` names, opened by its backend at its root without searching up. A
/// jj repository takes the user's jj config for its snapshots from `jj_config`, else from
/// the process.
///
/// # Errors
///
/// [`VcsError::NotUtf8`] for a root that is not UTF-8, [`VcsError::NotARepository`] for a
/// `.git` git cannot open, and any error from opening the repository.
pub(crate) fn open(
    repo: &Repo,
    jj_config: Option<&ConfigEnv>,
) -> Result<Box<dyn Repository>, VcsError> {
    let root =
        Utf8Path::from_path(&repo.root).ok_or_else(|| VcsError::NotUtf8(repo.root.clone()))?;
    Ok(match repo.kind {
        RepoKind::Jj => {
            let jj = JjRepository::discover(root)?;
            Box::new(match jj_config {
                Some(config) => jj.with_config(config.clone()),
                None => jj,
            })
        }
        RepoKind::Git => Box::new(GitRepository::open(root)?),
    })
}

pub(crate) const JJ_DIR: &str = ".jj";
pub(crate) const GIT_DIR: &str = ".git";

/// A repository found from a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub kind: RepoKind,
    pub root: PathBuf,
}

/// The nearest of `dir` and its ancestors that holds `.jj` or `.git`, jj when it holds both.
/// A `.git` file, as in a git worktree, counts. The one rule that finds a repository:
/// [`RunRepository`](crate::repository::RunRepository) takes its candidates from it, and
/// [`discover`] opens the repository it names.
#[must_use]
pub fn nearest_repo(dir: &Path) -> Option<Repo> {
    dir.ancestors().find_map(|root| {
        let kind = if root.join(JJ_DIR).is_dir() {
            RepoKind::Jj
        } else if root.join(GIT_DIR).exists() {
            RepoKind::Git
        } else {
            return None;
        };
        Some(Repo { kind, root: root.to_owned() })
    })
}

/// `path`, which must exist, made canonical as repository roots are.
fn canonical_file(path: &Path) -> Result<Utf8PathBuf, NativeError> {
    let found = |source| NativeError::Find { path: path.to_owned(), source };
    Ok(Utf8PathBuf::from_path_buf(dunce::canonicalize(path).map_err(found)?)
        .map_err(VcsError::NotUtf8)?)
}

/// `path` made absolute with links resolved, in the form repository roots take: on Windows,
/// without the verbatim `\\?\` prefix where the path allows.
fn canonical(path: &Utf8Path) -> std::io::Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(dunce::canonicalize(path)?)
        .map_err(|_| std::io::Error::other("a canonical path that is not UTF-8"))
}

/// Load `path` against `rev`, or against its parent revision without one, from the run's
/// repository, the one holding `path` ([`RunRepository::of_file`]), which stays open for
/// the steps after.
///
/// # Errors
///
/// [`NativeError::Find`] when `path` does not exist, [`NativeError::Vcs`] when no repository
/// holds it or the revision cannot be read, and [`NativeError::Decode`] when the revision's
/// file is not text.
pub fn load(
    repo: &mut RunRepository,
    path: &Path,
    rev: Option<&str>,
) -> Result<NativeDiff, NativeError> {
    let absolute = canonical_file(path)?;
    let repo = repo.require()?;
    // Discovery canonicalises, so the root shares `absolute`'s form.
    let relative = absolute
        .strip_prefix(repo.root())
        .map_err(|_| VcsError::OutsideRepository(absolute.clone()))?;
    let (spec, title) = if let Some(expr) = rev {
        (RevSpec::Expr(expr.to_owned()), expr.to_owned())
    } else {
        // The heading's revision chip names the commit (spec §5), so the title holds no ID.
        let parent = repo.default_parent(relative)?;
        let title = parent
            .term
            .label
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| parent.rev.to_string());
        (parent.rev, title)
    };
    read_left(repo, relative, &spec, &title, path)
}

/// Load `path` against the working-copy commit at the jj operation `op` names (`diff
/// --op`), from the run's repository. Also returns the commit, the revision the line
/// origins read.
///
/// # Errors
///
/// [`NativeError::NeedsJj`] outside a jj repository; [`NativeError::Vcs`] for an operation
/// jj cannot resolve; and the errors of [`load`].
pub fn load_at_operation(
    repo: &mut RunRepository,
    path: &Path,
    op: &str,
) -> Result<(NativeDiff, vcs::CommitId), NativeError> {
    let absolute = canonical_file(path)?;
    let repo = repo.require()?;
    if repo.kind() != RepoKind::Jj {
        return Err(NativeError::NeedsJj("diff --op"));
    }
    let relative = absolute
        .strip_prefix(repo.root())
        .map_err(|_| VcsError::OutsideRepository(absolute.clone()))?;
    let (short, commit) = JjRepository::discover(repo.root())?.working_copy_at(op)?;
    let spec = RevSpec::Expr(commit.as_str().to_owned());
    let diff = read_left(repo, relative, &spec, &operation_title(&short), path)?;
    Ok((diff, commit))
}

/// `relative` at `spec` on the left, titled `title`, against the working-copy file `path`.
fn read_left(
    repo: &dyn Repository,
    relative: &Utf8Path,
    spec: &RevSpec,
    title: &str,
    path: &Path,
) -> Result<NativeDiff, NativeError> {
    let (bytes, left_title) = match repo.file_at(relative, spec)? {
        Some(bytes) => (bytes, title.to_owned()),
        None => (Vec::new(), format!("{title} {NO_FILE}")),
    };
    let left = Text::decode(bytes).map_err(|source| NativeError::Decode {
        path: relative.to_owned(),
        rev: spec.to_string(),
        source,
    })?;
    Ok(NativeDiff { left, left_title, right: path.to_owned() })
}

/// The working copy's changed files, as `diff` with no path lists them (spec §6.1).
#[derive(Debug)]
pub struct Listing {
    /// The repository's root, which the paths are relative to.
    pub root: PathBuf,
    /// The revision the files are compared with, as the VCS reads it.
    pub rev: String,
    pub files: Vec<ChangedFile>,
}

/// The files that differ between `rev`, or the parent revision without one, and the working
/// copy, from the run's repository. jj snapshots the working copy first, as its commands do;
/// git compares the files on disk, staged or not, and leaves untracked files out. The parent
/// is jj's `@-` and git's `HEAD`: a staged change counts, so the index is no parent here. Each
/// candidate the VCS lists is compared by bytes, so a change to the executable bit alone, or a
/// staged change undone on disk, is left out. A path that is no regular file at the revision
/// reads as absent there, and one conflicted there is [`Content::Special`].
///
/// # Errors
///
/// [`NativeError::Vcs`] when no repository is found, the snapshot fails, or the revision or
/// a file cannot be read; [`NativeError::Find`] when a file on disk cannot be read.
pub fn changed_files(repo: &mut RunRepository, rev: Option<&str>) -> Result<Listing, NativeError> {
    let repo = repo.require_mut()?;
    repo.snapshot()?;
    let rev = rev.map_or_else(
        || {
            match repo.kind() {
                RepoKind::Jj => JJ_PARENT,
                RepoKind::Git => GIT_PARENT,
            }
            .to_owned()
        },
        str::to_owned,
    );
    let spec = RevSpec::Expr(rev.clone());
    let root = repo.root().as_std_path().to_owned();
    let mut files = Vec::new();
    for path in repo.changed_paths(&spec)? {
        let before = match repo.file_at(&path, &spec) {
            Ok(Some(bytes)) => Side::File(bytes),
            Ok(None) => Side::Absent,
            Err(VcsError::Conflicted { .. }) => Side::Special,
            Err(error) => return Err(error.into()),
        };
        let after = on_disk(&root.join(&path))?;
        if let Some((change, content)) = compared(before, after) {
            files.push(ChangedFile { path: path.into_std_path_buf(), change, content });
        }
    }
    Ok(Listing { root, rev, files })
}

/// Load the file at `relative`, a path [`changed_files`] listed, against the listing's
/// revision `rev`, from the run's repository, whose root is `root`. The right side is the
/// path on disk, which may no longer exist.
///
/// # Errors
///
/// [`NativeError::Vcs`] when the path is not UTF-8 or the revision cannot be read, and
/// [`NativeError::Decode`] when the revision's file is not text.
pub fn load_listed(
    repo: &mut RunRepository,
    root: &Path,
    rev: &str,
    relative: &Path,
) -> Result<NativeDiff, NativeError> {
    let path =
        Utf8Path::from_path(relative).ok_or_else(|| VcsError::NotUtf8(relative.to_owned()))?;
    let spec = RevSpec::Expr(rev.to_owned());
    read_left(repo.require()?, path, &spec, rev, &root.join(relative))
}

/// The parent revisions [`changed_files`] compares with by default.
const JJ_PARENT: &str = "@-";
const GIT_PARENT: &str = "HEAD";

/// What a path holds on one side of a listing.
enum Side {
    Absent,
    File(Vec<u8>),
    /// No regular file: a symlink, a conflict, a device.
    Special,
}

/// What `path` holds on disk. A directory holds no file at its path.
fn on_disk(path: &Path) -> Result<Side, NativeError> {
    let find = |source| NativeError::Find { path: path.to_owned(), source };
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => std::fs::read(path).map(Side::File).map_err(find),
        Ok(meta) if meta.is_dir() => Ok(Side::Absent),
        Ok(_) => Ok(Side::Special),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Side::Absent),
        Err(error) => Err(find(error)),
    }
}

/// How a path changed from `before` to `after`; `None` when it did not.
fn compared(before: Side, after: Side) -> Option<(Change, Content)> {
    let content = |side: Side| match side {
        Side::File(bytes) => dir_diff::decoded(bytes),
        Side::Absent | Side::Special => Content::Special,
    };
    match (before, after) {
        (Side::Absent, Side::Absent) => None,
        (Side::File(a), Side::File(b)) if a == b => None,
        (Side::Absent, side) => Some((Change::Added, content(side))),
        (side, Side::Absent) => Some((Change::Deleted, content(side))),
        (a, b) => Some((Change::Modified, content(a).max(content(b)))),
    }
}
