//! The two files `diff LEFT RIGHT` compares, as `git difftool` and `jj diff --tool` pass
//! them, and the save of a diff window's sides (spec §6.1, §7.1).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use camino::Utf8Path;
use merge_core::diff2::Rope;
use merge_core::text::{Encoding, Text};
use vcs::{Repository, RevSpec, VcsError};

use crate::origin::{JJ_DIFF_RIGHT, jj_diff_copy, jj_diff_side};
use crate::repository::{RunRepository, WorkingCopy};
use crate::{SessionError, read_text, save_text};

/// The files `diff LEFT RIGHT` compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPair {
    pub paths: [PathBuf; 2],
    /// A side whose file is absent opens empty and read-only, titled so.
    pub absent: [Option<String>; 2],
    pub readonly: [bool; 2],
}

/// One side of a tool diff, read.
#[derive(Debug, Clone)]
pub enum ToolSide {
    /// The file at `path`, which a save writes unless it is read-only.
    File { path: PathBuf, text: Text, read_only: bool },
    /// No file: an empty, read-only side titled `title`.
    Absent { title: String, text: Text },
}

/// Read `pair`'s sides from the working directory `cwd`, in the run's repository `repo`. A
/// right side that copies jj's working copy reads the working-copy file, and the left
/// turns read-only, since jj drops edits to its copies (spec §7.1).
///
/// # Errors
///
/// [`SessionError`] when a side cannot be read or decoded.
pub fn read_pair(
    pair: &ToolPair,
    repo: &mut RunRepository,
    cwd: &Path,
) -> Result<[ToolSide; 2], SessionError> {
    let ToolPair { paths: [left, right], absent: [left_absent, right_absent], readonly } = pair;
    let [readonly_left, readonly_right] = *readonly;
    let working_copy = (!readonly_right && right_absent.is_none())
        .then(|| working_copy_file(repo, right, cwd))
        .flatten();
    let (right_file, readonly_left) = match working_copy {
        Some(file) => (file, true),
        None => (right.clone(), readonly_left),
    };
    Ok([
        side(left.clone(), left_absent.clone(), readonly_left)?,
        side(right_file, right_absent.clone(), readonly_right)?,
    ])
}

/// The working-copy file that `right` copies, when it is jj's right-side copy. Only a path
/// in jj's temporary directory opens `repo`.
fn working_copy_file(repo: &mut RunRepository, right: &Path, cwd: &Path) -> Option<PathBuf> {
    jj_diff_copy(right, cwd)?;
    let found = match repo.open() {
        Ok(Some(open)) => jj_working_copy(open, right, cwd),
        Ok(None) => Ok(None),
        Err(error) => Err(error.into()),
    };
    found.unwrap_or_else(|error| {
        // The window still opens on jj's copy, and its banner says edits there are lost.
        tracing::warn!(%error, "cannot tell whether the right side is the working copy");
        None
    })
}

/// The side at `path`, or an empty read-only side titled `absent`.
fn side(path: PathBuf, absent: Option<String>, read_only: bool) -> Result<ToolSide, SessionError> {
    match absent {
        None => Ok(ToolSide::File { text: read_text(&path)?, path, read_only }),
        Some(title) => Text::decode(Vec::new())
            .map(|text| ToolSide::Absent { title, text })
            .map_err(|source| SessionError::Decode { path, source }),
    }
}

/// A side of a diff window to write: its text as it stands, in its file's encoding.
#[derive(Debug, Clone)]
pub struct SideSave {
    pub path: PathBuf,
    pub encoding: Encoding,
    pub text: Rope,
}

/// What a diff window's save did: each side's write, in order, and the snapshot of jj's
/// working copy, none when it took none.
#[derive(Debug)]
pub struct Saved {
    pub written: Vec<Result<(), SessionError>>,
    pub snapshot: Option<Result<(), VcsError>>,
}

/// Write each of `sides` to its path, then snapshot `working_copy` when a write succeeded,
/// as a jj command would (spec §6.1). A failed write leaves the other sides written.
#[must_use]
pub fn save(sides: &[SideSave], working_copy: Option<&WorkingCopy>) -> Saved {
    let written: Vec<_> =
        sides.iter().map(|side| save_text(&side.path, side.encoding, side.text.chunks())).collect();
    let snapshot =
        working_copy.filter(|_| written.iter().any(Result::is_ok)).map(WorkingCopy::snapshot);
    Saved { written, snapshot }
}

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
