//! A file's versions in the working-copy commit across the operation log (`history`,
//! `diff --op`, `DESIGN.md` §I5).

use camino::Utf8Path;
use futures::TryStreamExt as _;
use jj_lib::backend::{MergedTreeValue, TreeValue};
use jj_lib::object_id::ObjectId as _;
use jj_lib::op_walk::{resolve_op_with_repo, walk_ancestors};
use jj_lib::operation::Operation;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::RepoPathBuf;
use pollster::FutureExt as _;
use vcs::{Repository as _, VcsError};

use crate::{JjError, JjRepository, backend};

/// The length of an operation ID as jj shows it.
pub const SHORT_OPERATION: usize = 12;

/// What a version of the file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionState {
    File,
    Conflict,
    /// No file at the path.
    Absent,
    /// A symlink, submodule or directory.
    Other,
}

/// One version of a file in the working-copy commit: the operation where it appeared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersion {
    /// The operation's full ID.
    pub operation: String,
    /// When the operation ended, RFC 3339 in the operation's offset.
    pub time: String,
    pub description: String,
    /// The working-copy commit's full ID at the operation.
    pub commit: vcs::CommitId,
    pub state: VersionState,
}

impl JjRepository {
    /// The versions of `path` in the working-copy commit, newest first, one per run of
    /// operations that hold the same version, named by the oldest of the run: where it
    /// appeared. The working copy is snapshotted first.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] for a path outside the root; the errors of a
    /// snapshot; [`VcsError::Backend`] when the operation log cannot be read.
    pub fn history(&mut self, path: &Utf8Path) -> Result<Vec<FileVersion>, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        self.snapshot()?;
        let head = self.repo.operation().clone();
        let operations: Vec<Operation> = walk_ancestors(std::slice::from_ref(&head))
            .try_collect()
            .block_on()
            .map_err(backend("walking the operation log"))?;
        let mut versions: Vec<(MergedTreeValue, FileVersion)> = Vec::new();
        let mut last_commit = None;
        for op in operations {
            let view = op.view().block_on().map_err(backend("reading an operation's view"))?;
            let Some(id) = view.get_wc_commit_id(&self.workspace).cloned() else {
                continue;
            };
            let unchanged = last_commit.as_ref() == Some(&id);
            let value = match versions.last() {
                Some((value, _)) if unchanged => value.clone(),
                _ => {
                    let commit =
                        self.repo.store().get_commit(&id).map_err(backend("reading a commit"))?;
                    commit
                        .tree()
                        .path_value(&repo_path)
                        .block_on()
                        .map_err(backend("finding a path in a tree"))?
                }
            };
            let version = FileVersion {
                operation: op.id().hex(),
                time: time(&op),
                description: op.metadata().description.clone(),
                commit: vcs::CommitId::new(id.hex()),
                state: state(&value),
            };
            last_commit = Some(id);
            match versions.last_mut() {
                Some((last, named)) if *last == value => *named = version,
                _ => versions.push((value, version)),
            }
        }
        Ok(versions.into_iter().map(|(_, version)| version).collect())
    }

    /// The working-copy commit at the operation `op` names, as jj resolves an operation:
    /// an ID prefix, `@`, `@-`. Its short ID and the commit.
    ///
    /// # Errors
    ///
    /// [`VcsError::Backend`] holding [`JjError::UnknownOperation`] for an operation jj
    /// cannot resolve, and [`JjError::NoWorkingCopyAt`] for one where this workspace had no
    /// working copy.
    pub fn working_copy_at(&self, op: &str) -> Result<(String, vcs::CommitId), VcsError> {
        let unknown = |error: jj_lib::op_walk::OpsetEvaluationError| {
            backend("resolving an operation")(JjError::UnknownOperation {
                op: op.to_owned(),
                reason: error.to_string(),
            })
        };
        let operation = resolve_op_with_repo(&self.repo, op).block_on().map_err(unknown)?;
        let view = operation.view().block_on().map_err(backend("reading an operation's view"))?;
        let short: String = operation.id().hex().chars().take(SHORT_OPERATION).collect();
        let id = view.get_wc_commit_id(&self.workspace).ok_or_else(|| {
            backend("resolving an operation")(JjError::NoWorkingCopyAt(short.clone()))
        })?;
        Ok((short, vcs::CommitId::new(id.hex())))
    }
}

/// When `op` ended, RFC 3339 in its own offset; the instant in UTC when the offset is out of
/// range.
fn time(op: &Operation) -> String {
    let end = &op.metadata().time.end;
    let Ok(instant) = jiff::Timestamp::from_millisecond(end.timestamp.0) else {
        return end.timestamp.0.to_string();
    };
    let seconds = end.tz_offset * SECONDS_PER_MINUTE;
    match jiff::tz::Offset::from_seconds(seconds) {
        Ok(offset) => instant
            .to_zoned(jiff::tz::TimeZone::fixed(offset))
            .strftime("%Y-%m-%dT%H:%M:%S%:z")
            .to_string(),
        Err(_) => instant.to_string(),
    }
}

const SECONDS_PER_MINUTE: i32 = 60;

fn state(value: &MergedTreeValue) -> VersionState {
    match value.as_resolved() {
        None => VersionState::Conflict,
        Some(None) => VersionState::Absent,
        Some(Some(TreeValue::File { .. })) => VersionState::File,
        Some(Some(_)) => VersionState::Other,
    }
}
