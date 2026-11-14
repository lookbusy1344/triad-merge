//! The session side of the jj-native commands (spec §7.5): each
//! opens the jj repository holding the working directory and refuses any other.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use vcs::{RepoKind, Repository as _, VcsError};
use vcs_jj::JjRepository;
use vcs_jj::config::ConfigEnv;
/// The reports the front ends show.
pub use vcs_jj::{
    AbsorbHunk, Absorbed, CommitEffect, Destination, FileVersion, Goes, HunkChoice, Rewrite,
    SHORT_OPERATION, StackCommit, StackPath, Stay, VersionState,
};

use crate::resolve::ResolveError;

/// The jj repository holding `cwd`, with the user's jj config from `jj_config`, else the
/// process's. `command` names the command in the refusal outside jj.
///
/// # Errors
///
/// [`ResolveError::NeedsJj`] in a git repository, and [`ResolveError::Vcs`] outside any
/// repository or when jj-lib cannot load it.
pub fn open(
    cwd: &Path,
    jj_config: Option<ConfigEnv>,
    command: &'static str,
) -> Result<JjRepository, ResolveError> {
    let found = crate::native::find_from(cwd).ok_or_else(|| {
        VcsError::NotARepository(Utf8PathBuf::from(cwd.to_string_lossy().into_owned()))
    })?;
    if found.kind != RepoKind::Jj {
        return Err(ResolveError::NeedsJj(command));
    }
    let root = Utf8PathBuf::from_path_buf(found.root).map_err(VcsError::NotUtf8)?;
    let config = jj_config.unwrap_or_else(ConfigEnv::from_process);
    Ok(JjRepository::discover(&root)?.with_config(config))
}

/// The conflicted commits `revset` names in the jj repository holding `cwd`, parents first
/// (`stack`); without a revset, the mutable ones.
///
/// # Errors
///
/// As [`open`], and [`ResolveError::Vcs`] when the revset does not resolve.
pub fn stack(
    cwd: &Path,
    revset: Option<&str>,
    jj_config: Option<ConfigEnv>,
) -> Result<Vec<StackCommit>, ResolveError> {
    Ok(stacked(cwd, revset, jj_config)?.commits)
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
    jj_config: Option<ConfigEnv>,
) -> Result<Stacked, ResolveError> {
    let mut repo = open(cwd, jj_config, "stack")?;
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
    jj_config: Option<ConfigEnv>,
) -> Result<(Utf8PathBuf, Vec<FileVersion>), ResolveError> {
    let FileHistory { path, versions, .. } = file_history(cwd, path, limit, jj_config)?;
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
    jj_config: Option<ConfigEnv>,
) -> Result<FileHistory, ResolveError> {
    let mut repo = open(cwd, jj_config, "history")?;
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
    jj_config: Option<ConfigEnv>,
) -> Result<Absorbed, ResolveError> {
    let mut repo = open(cwd, jj_config, "absorb")?;
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
