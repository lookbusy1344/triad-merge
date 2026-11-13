//! The session side of the jj-native commands (spec §1.2, `docs/PLAN-jj-native.md`): each
//! opens the jj repository holding the working directory and refuses any other.

use std::path::Path;

use camino::Utf8PathBuf;
use vcs::{RepoKind, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::{JjRepository, StackCommit};

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
    Ok(open(cwd, jj_config, "stack")?.stack(revset)?)
}
