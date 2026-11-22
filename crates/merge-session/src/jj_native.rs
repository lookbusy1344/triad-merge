//! The session side of the jj-native commands (spec §7.5): each
//! opens the jj repository holding the working directory and refuses any other.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use vcs::{RepoKind, Repository as _, VcsError};
use vcs_jj::JjRepository;
/// The reports the front ends show.
pub use vcs_jj::{
    AbsorbHunk, Absorbed, ChangeVersion, CommitEffect, Destination, Evolog, FileVersion, Goes,
    HunkChoice, Interdiff, JjError, Origin, Rewrite, SHORT_OPERATION, StackCommit, StackPath, Stay,
    VersionOperation, VersionState,
};

use crate::repository::Discovery;
use crate::resolve::ResolveError;

/// What a jj-native form listed before its window opened: the window shows it first and lists
/// again only on a refresh.
#[derive(Debug, Clone)]
pub enum Listing {
    Stack(Stacked),
    Absorb(Absorbed),
    History(FileHistory),
    Evolog(Evolog),
    Interdiff(crate::native::InterdiffListing),
    InterdiffFile(crate::native::InterdiffFile),
}

impl Listing {
    /// The stack window's listing; none for another form's.
    #[must_use]
    pub fn stack(self) -> Option<Stacked> {
        match self {
            Self::Stack(listed) => Some(listed),
            _ => None,
        }
    }

    /// The absorb window's listing; none for another form's.
    #[must_use]
    pub fn absorb(self) -> Option<Absorbed> {
        match self {
            Self::Absorb(listed) => Some(listed),
            _ => None,
        }
    }

    /// The history window's listing; none for another form's.
    #[must_use]
    pub fn history(self) -> Option<FileHistory> {
        match self {
            Self::History(listed) => Some(listed),
            _ => None,
        }
    }

    /// The evolog window's listing; none for another form's.
    #[must_use]
    pub fn evolog(self) -> Option<Evolog> {
        match self {
            Self::Evolog(listed) => Some(listed),
            _ => None,
        }
    }

    /// The diff window's file of `diff PATH --since`; none for another form's.
    #[must_use]
    pub fn interdiff_file(self) -> Option<crate::native::InterdiffFile> {
        match self {
            Self::InterdiffFile(file) => Some(file),
            _ => None,
        }
    }

    /// The directory window's listing of `diff --since`; none for another form's.
    #[must_use]
    pub fn interdiff(self) -> Option<crate::native::InterdiffListing> {
        match self {
            Self::Interdiff(listed) => Some(listed),
            _ => None,
        }
    }
}

/// The jj repository holding `cwd`, reading the user's jj config as `discovery` says.
/// `command` names the command in the refusal outside jj.
///
/// # Errors
///
/// [`ResolveError::NeedsJj`] in a git repository, and [`ResolveError::Vcs`] outside any
/// repository or when jj-lib cannot load it.
pub fn open(
    cwd: &Path,
    discovery: &Discovery,
    command: &'static str,
) -> Result<JjRepository, ResolveError> {
    jj_at(cwd, discovery)?.ok_or(ResolveError::NeedsJj(command))
}

/// The jj repository holding `cwd`, reading the user's jj config as `discovery` says; none
/// for a git repository.
///
/// # Errors
///
/// [`VcsError::NotARepository`] outside any repository, and the errors of jj-lib's load.
pub(crate) fn jj_at(cwd: &Path, discovery: &Discovery) -> Result<Option<JjRepository>, VcsError> {
    let found = crate::native::find_from(cwd).ok_or_else(|| {
        VcsError::NotARepository(Utf8PathBuf::from(cwd.to_string_lossy().into_owned()))
    })?;
    if found.kind != RepoKind::Jj {
        return Ok(None);
    }
    let root = Utf8PathBuf::from_path_buf(found.root).map_err(VcsError::NotUtf8)?;
    Ok(Some(JjRepository::discover(&root)?.with_config(discovery.jj_config())))
}

/// The conflicted commits `revset` names in the jj repository holding `cwd`, children first
/// (`stack`); without a revset, the mutable ones.
///
/// # Errors
///
/// As [`open`], and [`ResolveError::Vcs`] when the revset does not resolve.
pub fn stack(
    cwd: &Path,
    revset: Option<&str>,
    discovery: &Discovery,
) -> Result<Vec<StackCommit>, ResolveError> {
    Ok(stacked(cwd, revset, discovery)?.commits)
}

/// [`stack`]'s commits, and the root of the repository their paths are relative to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stacked {
    pub root: Utf8PathBuf,
    pub commits: Vec<StackCommit>,
}

/// As [`stack`], with the repository's root (the stack window).
///
/// # Errors
///
/// As [`stack`].
pub fn stacked(
    cwd: &Path,
    revset: Option<&str>,
    discovery: &Discovery,
) -> Result<Stacked, ResolveError> {
    let mut repo = open(cwd, discovery, "stack")?;
    let commits = repo.stack(revset)?;
    Ok(Stacked { root: repo.root().to_owned(), commits })
}

/// The versions of `path`, relative to `cwd`, in the working-copy commit across the
/// operation log of the jj repository holding `cwd`, newest first, at most `limit` of them
/// (`history`).
///
/// # Errors
///
/// As [`open`]; [`ResolveError::Vcs`] for a path outside the repository, and as
/// [`JjRepository::history`] fails.
pub fn history(
    cwd: &Path,
    path: &Path,
    limit: Option<usize>,
    discovery: &Discovery,
) -> Result<(Utf8PathBuf, Vec<FileVersion>), ResolveError> {
    let FileHistory { path, versions, .. } = file_history(cwd, path, limit, discovery)?;
    Ok((path, versions))
}

/// [`history`]'s versions, with the repository's root and the path relative to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistory {
    pub root: Utf8PathBuf,
    pub path: Utf8PathBuf,
    pub versions: Vec<FileVersion>,
}

/// As [`history`], with the repository's root (the history window).
///
/// # Errors
///
/// As [`history`].
pub fn file_history(
    cwd: &Path,
    path: &Path,
    limit: Option<usize>,
    discovery: &Discovery,
) -> Result<FileHistory, ResolveError> {
    let mut repo = open(cwd, discovery, "history")?;
    let file = crate::resolve::absolute(&cwd.join(path))?;
    let relative = file
        .strip_prefix(repo.root())
        .map_err(|_| VcsError::OutsideRepository(file.clone()))?
        .to_owned();
    let versions = repo.history(&relative, limit)?;
    Ok(FileHistory { root: repo.root().to_owned(), path: relative, versions })
}

/// The hunks of the working-copy commit of the jj repository holding `cwd`, under `paths`
/// relative to `cwd`, each with the commit it goes into among `into`; with `apply`, moved
/// there, at the operation `listed_at` names when given (`absorb`).
///
/// # Errors
///
/// As [`open`]; [`ResolveError::Vcs`] for a path outside the repository and as
/// [`JjRepository::absorb`] fails.
pub fn absorb(
    cwd: &Path,
    paths: &[std::path::PathBuf],
    into: Option<&str>,
    choices: &[HunkChoice],
    apply: bool,
    listed_at: Option<&str>,
    discovery: &Discovery,
) -> Result<Absorbed, ResolveError> {
    let mut repo = open(cwd, discovery, "absorb")?;
    let relative = paths
        .iter()
        .map(|path| {
            let file = crate::resolve::absolute(&cwd.join(path))?;
            file.strip_prefix(repo.root())
                .map(Utf8Path::to_owned)
                .map_err(|_| VcsError::OutsideRepository(file.clone()))
        })
        .collect::<Result<Vec<_>, VcsError>>()?;
    Ok(repo.absorb(&relative, into, choices, apply, listed_at)?)
}

/// What `absorb`'s listing says after a hunk that stays in the working copy, and why
/// (spec §7.5.3).
#[must_use]
pub fn stays(stay: &Stay) -> String {
    match stay {
        Stay::NoCommit => "stays: no one commit in --into wrote its lines".to_owned(),
        Stay::WouldConflict(change) => {
            format!("stays: would conflict in {}", change.id(vcs::IdKind::Change))
        }
        Stay::Kept => "stays: kept".to_owned(),
    }
}

/// The versions of the change in the one commit `rev` names, default `@`, in the jj
/// repository holding `cwd`, newest first, at most `limit` of them (`evolog`).
///
/// # Errors
///
/// As [`open`], and as [`JjRepository::evolog`] fails.
pub fn evolog(
    cwd: &Path,
    rev: Option<&str>,
    limit: Option<usize>,
    discovery: &Discovery,
) -> Result<Evolog, ResolveError> {
    let mut repo = open(cwd, discovery, "evolog")?;
    Ok(repo.evolog(rev.unwrap_or(WORKING_COPY), limit)?)
}

/// An earlier resolution of a conflict (spec §7.5.5): an earlier version of it, the file
/// the version that resolved it held, and that version.
#[derive(Debug, Clone)]
pub struct EarlierResolution {
    pub conflict: merge_core::merge3::Inputs,
    pub resolved: merge_core::text::Text,
    pub version: ChangeVersion,
}

/// A jj conflict whose earlier resolutions a merge looks for (spec §7.5.5): the commit it
/// was read from, or none for the working-copy commit, and its path.
#[derive(Debug, Clone)]
pub struct EarlierSource {
    pub(crate) root: Utf8PathBuf,
    pub(crate) path: Utf8PathBuf,
    pub(crate) commit: Option<vcs::CommitId>,
    pub(crate) discovery: Discovery,
}

impl EarlierSource {
    /// The conflict's earlier resolutions, newest first, as [`earlier_resolutions`] finds
    /// them. Reads only: no snapshot.
    ///
    /// # Errors
    ///
    /// As [`earlier_resolutions`], and [`vcs::VcsError::NoWorkingCopy`] when the workspace
    /// has no working copy.
    pub fn find(&self) -> Result<Vec<EarlierResolution>, ResolveError> {
        let repo = open(self.root.as_std_path(), &self.discovery, "resolve")?;
        let id = match &self.commit {
            Some(id) => id.clone(),
            None => repo
                .working_copy_id()?
                .ok_or_else(|| VcsError::NoWorkingCopy(repo.root().to_owned()))?,
        };
        Ok(decoded(&self.path, repo.earlier_resolutions(&id, &self.path)?))
    }
}

/// The earlier resolutions of the conflict at `path`, relative to the root, in the evolog of
/// the commit `id` in the jj repository at `root`, newest first. A version whose conflict or
/// file does not decode as text is logged and left out: it only loses an offer.
///
/// # Errors
///
/// As [`open`], and as [`JjRepository::earlier_resolutions`] fails.
pub fn earlier_resolutions(
    root: &Utf8Path,
    id: &vcs::CommitId,
    path: &Utf8Path,
    discovery: &Discovery,
) -> Result<Vec<EarlierResolution>, ResolveError> {
    let repo = open(root.as_std_path(), discovery, "resolve")?;
    Ok(decoded(path, repo.earlier_resolutions(id, path)?))
}

/// `pairs` decoded as text; a pair that does not decode is logged and left out.
fn decoded(path: &Utf8Path, pairs: Vec<vcs_jj::ResolvedPair>) -> Vec<EarlierResolution> {
    pairs
        .into_iter()
        .filter_map(|pair| {
            let decoded = crate::resolve::parent_inputs(path, pair.conflict).and_then(|conflict| {
                merge_core::text::Text::decode(pair.resolved)
                    .map(|resolved| (conflict, resolved))
                    .map_err(|source| ResolveError::NotText {
                        path: path.to_owned(),
                        term: format!("the file at {}", pair.version.commit),
                        source,
                    })
            });
            match decoded {
                Ok((conflict, resolved)) => {
                    Some(EarlierResolution { conflict, resolved, version: pair.version })
                }
                Err(error) => {
                    tracing::warn!(%error, %path, "an earlier resolution does not load");
                    None
                }
            }
        })
        .collect()
}

/// The revision `evolog` and `diff --since` read without `-r`.
pub const WORKING_COPY: &str = "@";

#[cfg(test)]
mod tests {
    use vcs::ChangeInfo;

    use super::*;

    #[test]
    fn a_hunk_that_stays_says_why() {
        let change = ChangeInfo {
            commit: "c-kkkk".to_owned(),
            change_id: Some("kkkk".to_owned()),
            summary: None,
        };
        assert_eq!(stays(&Stay::NoCommit), "stays: no one commit in --into wrote its lines");
        assert_eq!(stays(&Stay::WouldConflict(change)), "stays: would conflict in kkkk");
        assert_eq!(stays(&Stay::Kept), "stays: kept");
    }
}
