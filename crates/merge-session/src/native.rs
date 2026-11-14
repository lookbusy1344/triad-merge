//! The inputs of `diff PATH`: the working-copy file and its parent revision (spec §6.1).

use std::io;
use std::path::{Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use merge_core::text::{DecodeError, Text};
use vcs::{RepoKind, Repository, RevSpec, VcsError};
use vcs_git::GitRepository;
use vcs_jj::JjRepository;
use vcs_jj::config::ConfigEnv;

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
