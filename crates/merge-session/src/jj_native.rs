//! The session side of the jj-native commands (spec §7.5): each
//! opens the jj repository holding the working directory and refuses any other.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use merge_core::text::Text;
use vcs::{IdKind, RepoKind, Repository as _, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::{Absorbed, Divergence, FileVersion, HunkChoice, JjRepository, StackCommit};

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

/// Merge the copies of the divergent change `change` in the jj repository holding `cwd`
/// (`diverge`); on a dry run, only report.
///
/// # Errors
///
/// As [`open`], and [`ResolveError::Vcs`] as [`JjRepository::diverge`] fails.
pub fn diverge(
    cwd: &Path,
    change: &str,
    dry_run: bool,
    jj_config: Option<ConfigEnv>,
) -> Result<Divergence, ResolveError> {
    Ok(open(cwd, jj_config, "diverge")?.diverge(change, dry_run)?)
}

/// The versions of `path`, relative to `cwd`, in the working-copy commit across the
/// operation log of the jj repository holding `cwd`, newest first (`history`).
///
/// # Errors
///
/// As [`open`]; [`ResolveError::Vcs`] for a path outside the repository, and as
/// [`JjRepository::history`] fails.
pub fn history(
    cwd: &Path,
    path: &Path,
    jj_config: Option<ConfigEnv>,
) -> Result<(Utf8PathBuf, Vec<FileVersion>), ResolveError> {
    let mut repo = open(cwd, jj_config, "history")?;
    let file = crate::resolve::absolute(&cwd.join(path))?;
    let relative = file
        .strip_prefix(repo.root())
        .map_err(|_| VcsError::OutsideRepository(file.clone()))?
        .to_owned();
    let versions = repo.history(&relative)?;
    Ok((relative, versions))
}

/// The two sides of `diff PATH --evolog`, as text, with their pane titles.
#[derive(Debug)]
pub struct EvologTexts {
    pub left: Text,
    pub right: Text,
    pub left_title: String,
    pub right_title: String,
}

/// `path`, relative to `cwd`, in the change `rev` names, `@` without one, against an
/// earlier version of the change, `from` or the evolog's, rebased onto its parents now
/// (`diff --evolog`). An absent side is empty, its title ending in
/// [`NO_FILE`](crate::native::NO_FILE).
///
/// # Errors
///
/// As [`open`]; [`ResolveError::Vcs`] for a path outside the repository and as
/// [`JjRepository::evolog_diff`] fails; [`ResolveError::Decode`] when a side is not text.
pub fn evolog(
    cwd: &Path,
    path: &Path,
    rev: Option<&str>,
    from: Option<&str>,
    jj_config: Option<ConfigEnv>,
) -> Result<EvologTexts, ResolveError> {
    let repo = open(cwd, jj_config, "diff --evolog")?;
    let file = crate::resolve::absolute(&cwd.join(path))?;
    let relative = file
        .strip_prefix(repo.root())
        .map_err(|_| VcsError::OutsideRepository(file.clone()))?
        .to_owned();
    let diff =
        repo.evolog_diff(rev.unwrap_or(crate::invocation::WORKING_COPY_REV), from, &relative)?;
    let side = |bytes: Option<Vec<u8>>, title: String| -> Result<(Text, String), ResolveError> {
        let (bytes, title) = match bytes {
            Some(bytes) => (bytes, title),
            None => (Vec::new(), format!("{title} {}", crate::native::NO_FILE)),
        };
        let text = Text::decode(bytes)
            .map_err(|source| ResolveError::Decode { path: relative.clone(), source })?;
        Ok((text, title))
    };
    let name = |change: &vcs::ChangeInfo| {
        format!("{} {}", change.id(IdKind::Change), change.id(IdKind::Commit))
    };
    let (left, left_title) = side(diff.left, format!("{} rebased", name(&diff.predecessor)))?;
    let (right, right_title) = side(diff.right, name(&diff.change))?;
    Ok(EvologTexts { left, right, left_title, right_title })
}

/// The hunks of the working-copy commit of the jj repository holding `cwd`, under `paths`
/// relative to `cwd`, each with the commit it goes into among `into`; with `apply`, moved
/// there (`absorb`).
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
    Ok(repo.absorb(&relative, into, choices, apply)?)
}
