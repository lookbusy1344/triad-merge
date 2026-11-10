//! The working-copy file the right side of `jj diff --tool` stands for (spec §7.1).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use camino::Utf8Path;
use vcs::{Repository, RevSpec, VcsError};

use crate::origin::{JJ_DIFF_RIGHT, jj_diff_side};

#[derive(Debug, thiserror::Error)]
pub enum DiffToolError {
    #[error(transparent)]
    Vcs(#[from] VcsError),
    #[error("cannot read {}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The working-copy file that the right side of `jj diff --tool`, `right`, is a copy of, so
/// edits to that side can be saved there. `right` is taken from `cwd` when relative.
///
/// It is the file at `right`'s repository path in `repo`'s working tree when `right` lies in
/// the right side of jj's `jj-diff-<random>` directory and the copy holds the bytes of both
/// the regular file on disk and that file in the working-copy commit `@`. jj passes no
/// revisions, so the bytes stand in for them. git has no working-copy commit, so `None` there
/// and in every other case.
///
/// # Errors
///
/// [`DiffToolError`] when the repository, the copy or the file on disk cannot be read.
pub fn jj_working_copy(
    repo: &dyn Repository,
    right: &Path,
    cwd: &Path,
) -> Result<Option<PathBuf>, DiffToolError> {
    let Some((JJ_DIFF_RIGHT, path)) = jj_diff_side(right, cwd) else {
        return Ok(None);
    };
    let Some(path) = Utf8Path::from_path(&path).filter(|path| !path.as_str().is_empty()) else {
        return Ok(None);
    };
    let file = repo.root().join(path).into_std_path_buf();
    let Some(on_disk) = regular_file(&file)? else {
        return Ok(None);
    };
    let copy = cwd.join(right);
    let copied =
        std::fs::read(&copy).map_err(|source| DiffToolError::Read { path: copy, source })?;
    if copied != on_disk {
        return Ok(None);
    }
    let Some(working_copy) = repo.working_copy_id()? else {
        return Ok(None);
    };
    match repo.file_at(path, &RevSpec::Commit(working_copy)) {
        Ok(at) => Ok(at.is_some_and(|at| at == copied).then_some(file)),
        // A conflicted file holds no one side's bytes.
        Err(VcsError::Conflicted { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// The bytes of `path` when it is a regular file; `None` when it is missing or anything else.
fn regular_file(path: &Path) -> Result<Option<Vec<u8>>, DiffToolError> {
    let read_error = |source| DiffToolError::Read { path: path.to_path_buf(), source };
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => std::fs::read(path).map(Some).map_err(read_error),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(read_error(error)),
    }
}
