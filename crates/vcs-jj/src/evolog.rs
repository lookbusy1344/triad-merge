//! The versions of a change across the operations that rewrote it (`evolog`, spec §7.5.4,
//! `DESIGN.md` §I6).

use std::collections::HashMap;

use camino::Utf8Path;
use futures::StreamExt as _;
use jj_lib::backend::{CommitId, MergedTreeValue, TreeValue};
use jj_lib::evolution::{CommitEvolutionEntry, walk_predecessors};
use jj_lib::object_id::ObjectId as _;
use jj_lib::repo_path::RepoPathBuf;
use pollster::FutureExt as _;
use vcs::{Repository as _, TermContent, VcsError};

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

/// An earlier resolution of the conflict at a path: the oldest of a run of versions that
/// held a conflict there, and the newer version, a successor of the run, that held a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPair {
    /// The conflict the run's oldest version held, simplified as every read is.
    pub conflict: vcs::Conflict,
    /// The file the resolving version held.
    pub resolved: Vec<u8>,
    /// The resolving version.
    pub version: ChangeVersion,
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
                Ok(version_of(&entry))
            })
            .collect::<Result<_, VcsError>>()?;
        Ok(Evolog { change_id: start.change_id().reverse_hex(), versions })
    }

    /// The earlier resolutions of the conflict at `path` in the evolog of the commit `id`,
    /// newest resolving version first (spec §7.5.5). A version that holds a file at `path`
    /// and has a predecessor that holds a conflict there resolves it; the run of conflicted
    /// versions goes back through first conflicted predecessors to its oldest. Reads only:
    /// the caller has snapshotted.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] for a path with `..`, a leading `.` or a root;
    /// [`VcsError::UnknownRevision`] when the repository holds no commit `id`; and
    /// [`VcsError::Backend`] when a commit, a tree or the operation log cannot be read.
    pub fn earlier_resolutions(
        &self,
        id: &vcs::CommitId,
        path: &Utf8Path,
    ) -> Result<Vec<ResolvedPair>, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let start = self.commit_by_id(id)?;
        let entries: Vec<(CommitEvolutionEntry, MergedTreeValue)> =
            std::pin::pin!(walk_predecessors(&self.repo, std::slice::from_ref(start.id())))
                .collect::<Vec<_>>()
                .block_on()
                .into_iter()
                .map(|entry| {
                    let entry = entry.map_err(backend("walking a change's predecessors"))?;
                    let value = entry
                        .commit
                        .tree()
                        .path_value(&repo_path)
                        .block_on()
                        .map_err(backend("finding a path in a tree"))?;
                    Ok((entry, value))
                })
                .collect::<Result<_, VcsError>>()?;
        let at: HashMap<&CommitId, usize> =
            entries.iter().enumerate().map(|(ix, (entry, _))| (entry.commit.id(), ix)).collect();
        let conflicted_predecessor = |ix: usize| {
            entries[ix]
                .0
                .predecessor_ids()
                .iter()
                .filter_map(|id| at.get(id).copied())
                .find(|&p| !entries[p].1.is_resolved())
        };
        let mut pairs = Vec::new();
        for (ix, (entry, value)) in entries.iter().enumerate() {
            let Some(Some(file @ TreeValue::File { .. })) = value.as_resolved() else {
                continue;
            };
            let Some(mut oldest) = conflicted_predecessor(ix) else {
                continue;
            };
            while let Some(earlier) = conflicted_predecessor(oldest) {
                oldest = earlier;
            }
            let Some(conflict) = self.conflict_in(&entries[oldest].0.commit, path)? else {
                continue;
            };
            let TermContent::File { bytes, .. } = self.term_content(&repo_path, Some(file))? else {
                continue;
            };
            pairs.push(ResolvedPair { conflict, resolved: bytes, version: version_of(entry) });
        }
        Ok(pairs)
    }
}

/// `entry` as a version of its change.
fn version_of(entry: &CommitEvolutionEntry) -> ChangeVersion {
    ChangeVersion {
        commit: crate::commit_id(entry.commit.id()),
        operation: entry.operation.as_ref().map(|op| VersionOperation {
            id: op.id().hex(),
            time: history::time(op),
            description: op.metadata().description.clone(),
        }),
        conflicted: entry.commit.has_conflict(),
    }
}
