//! The versions of a change across the operations that rewrote it (`evolog`, spec §7.5.4,
//! `DESIGN.md` §I6).

use futures::StreamExt as _;
use jj_lib::evolution::walk_predecessors;
use jj_lib::object_id::ObjectId as _;
use pollster::FutureExt as _;
use vcs::{Repository as _, VcsError};

use crate::{JjRepository, backend, history};

/// The operation that wrote a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionOperation {
    /// The operation's full ID.
    pub id: String,
    /// When the operation ended, RFC 3339 in the operation's offset.
    pub time: String,
    pub description: String,
}

/// One version of a change: a commit the change held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeVersion {
    pub commit: vcs::CommitId,
    /// None for a version from before jj recorded predecessors.
    pub operation: Option<VersionOperation>,
    /// The commit holds a conflict.
    pub conflicted: bool,
}

/// The versions of a change, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evolog {
    /// The change ID of the commit the walk starts from, in jj's reverse hex.
    pub change_id: String,
    pub versions: Vec<ChangeVersion>,
}

impl JjRepository {
    /// The versions of the change in the one commit `revset` names, newest first, as
    /// `jj evolog` walks them: the commit, then its predecessors. With `limit`, the newest
    /// `limit`. The working copy is snapshotted first.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] when `revset` names no commit and
    /// [`VcsError::AmbiguousRevision`] when it names several; the errors of a snapshot; and
    /// [`VcsError::Backend`] when the config, a commit or the operation log cannot be read.
    pub fn evolog(&mut self, revset: &str, limit: Option<usize>) -> Result<Evolog, VcsError> {
        let revsets = self.revsets()?;
        self.snapshot()?;
        let start = self.one_commit(self.repo.as_ref(), revset, &revsets)?;
        let entries = walk_predecessors(&self.repo, std::slice::from_ref(start.id()))
            .take(limit.unwrap_or(usize::MAX));
        let versions = std::pin::pin!(entries)
            .collect::<Vec<_>>()
            .block_on()
            .into_iter()
            .map(|entry| {
                let entry = entry.map_err(backend("walking a change's predecessors"))?;
                Ok(ChangeVersion {
                    commit: crate::commit_id(entry.commit.id()),
                    operation: entry.operation.as_ref().map(|op| VersionOperation {
                        id: op.id().hex(),
                        time: history::time(op),
                        description: op.metadata().description.clone(),
                    }),
                    conflicted: entry.commit.has_conflict(),
                })
            })
            .collect::<Result<_, VcsError>>()?;
        Ok(Evolog { change_id: start.change_id().reverse_hex(), versions })
    }
}
