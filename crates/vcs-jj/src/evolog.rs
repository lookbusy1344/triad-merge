//! `diff --evolog`: a change's file now against an earlier version of the change rebased
//! onto its current parents, so only what the rewrite changed in the patch shows
//! (`DESIGN.md` §I6).

use camino::Utf8Path;
use futures::TryStreamExt as _;
use jj_lib::backend::{MergedTreeValue, TreeValue};
use jj_lib::commit::Commit;
use jj_lib::conflicts::{self, ConflictMarkerStyle, ConflictMaterializeOptions};
use jj_lib::evolution::walk_predecessors;
use jj_lib::merge::Merge;
use jj_lib::merged_tree::MergedTree;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::{RepoPath, RepoPathBuf};
use pollster::FutureExt as _;
use vcs::{ChangeInfo, VcsError};

use crate::{JjError, JjRepository, backend};

/// A change's file now and at an earlier version of the change, rebased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvologDiff {
    /// The change now.
    pub change: ChangeInfo,
    /// The earlier version the left side comes from.
    pub predecessor: ChangeInfo,
    /// The file in the earlier version rebased onto the change's parents now, with jj's
    /// conflict markers where the rebase conflicts; none when the path holds no file there.
    pub left: Option<Vec<u8>>,
    /// The file in the change now, with jj's conflict markers where it is conflicted; none
    /// when the path holds no file.
    pub right: Option<Vec<u8>>,
}

impl JjRepository {
    /// `path` in the commit `rev` names against an earlier version of its change: `from`'s
    /// commit, else the nearest predecessor whose rebased file differs from the file now,
    /// else the nearest predecessor. The earlier version is rebased onto the commit's
    /// parents in memory; nothing is written.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] and [`VcsError::AmbiguousRevision`] when `rev` or
    /// `from` names no commit or several; [`VcsError::OutsideRepository`] for a path outside
    /// the root; [`VcsError::Backend`] holding [`JjError::NoPredecessor`] for a commit with
    /// no earlier version and [`JjError::NotAFile`] when the path holds a symlink,
    /// submodule or directory; and [`VcsError::Backend`] when the repository cannot be read.
    pub fn evolog_diff(
        &self,
        rev: &str,
        from: Option<&str>,
        path: &Utf8Path,
    ) -> Result<EvologDiff, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let current = self.one_commit(rev)?;
        let right = self.materialized(&current.tree(), &repo_path, path)?;
        let parents = self.parent_tree(&current)?;
        let rebased = |earlier: &Commit| -> Result<Option<Vec<u8>>, VcsError> {
            let tree = MergedTree::merge(Merge::from_vec(vec![
                (earlier.tree(), earlier.conflict_label()),
                (self.parent_tree(earlier)?, format!("parents of {}", earlier.conflict_label())),
                (parents.clone(), format!("parents of {}", current.conflict_label())),
            ]))
            .block_on()
            .map_err(backend("rebasing an earlier version"))?;
            self.materialized(&tree, &repo_path, path)
        };
        let (earlier, left) = if let Some(from) = from {
            let earlier = self.one_commit(from)?;
            let left = rebased(&earlier)?;
            (earlier, left)
        } else {
            let predecessors = self.predecessors(&current)?;
            let mut nearest = None;
            let mut differing = None;
            for earlier in predecessors {
                let left = rebased(&earlier)?;
                if left != right {
                    differing = Some((earlier, left));
                    break;
                }
                nearest.get_or_insert((earlier, left));
            }
            differing.or(nearest).ok_or_else(|| {
                backend("finding an earlier version")(JjError::NoPredecessor(
                    current.change_id().reverse_hex(),
                ))
            })?
        };
        Ok(EvologDiff {
            change: self.change_info(&current)?,
            predecessor: self.change_info(&earlier)?,
            left,
            right,
        })
    }

    /// The one commit `rev` names.
    fn one_commit(&self, rev: &str) -> Result<Commit, VcsError> {
        match <[Commit; 1]>::try_from(self.resolve(rev)?) {
            Ok([commit]) => Ok(commit),
            Err(_) => Err(VcsError::AmbiguousRevision(rev.to_owned())),
        }
    }

    /// `commit`'s parents' trees merged.
    fn parent_tree(&self, commit: &Commit) -> Result<MergedTree, VcsError> {
        commit
            .parent_tree(self.repo.index())
            .block_on()
            .map_err(backend("merging a commit's parents"))
    }

    /// `commit`'s earlier versions in the evolog, nearest first, `commit` left out.
    fn predecessors(&self, commit: &Commit) -> Result<Vec<Commit>, VcsError> {
        let found: Vec<Commit> = walk_predecessors(&self.repo, std::slice::from_ref(commit.id()))
            .map_ok(|entry| entry.commit)
            .try_collect()
            .block_on()
            .map_err(backend("walking the evolog"))?;
        Ok(found.into_iter().filter(|c| c.id() != commit.id()).collect())
    }

    /// `path` in `tree` as bytes: a file's content, a conflict of files with jj's markers,
    /// none when absent.
    fn materialized(
        &self,
        tree: &MergedTree,
        repo_path: &RepoPath,
        path: &Utf8Path,
    ) -> Result<Option<Vec<u8>>, VcsError> {
        let value: MergedTreeValue =
            tree.path_value(repo_path).block_on().map_err(backend("finding a path in a tree"))?;
        let not_a_file = || backend("reading a file")(JjError::NotAFile(path.to_owned()));
        match value.as_resolved() {
            Some(None) => return Ok(None),
            Some(Some(TreeValue::File { id, .. })) => {
                return self.read_file(repo_path, id).map(Some);
            }
            Some(Some(_)) => return Err(not_a_file()),
            None => {}
        }
        let store = self.repo.store();
        let conflict =
            conflicts::try_materialize_file_conflict_value(store, repo_path, &value, tree.labels())
                .block_on()
                .map_err(backend("reading a conflict"))?
                .ok_or_else(not_a_file)?;
        let options = ConflictMaterializeOptions {
            marker_style: ConflictMarkerStyle::Diff,
            marker_len: Some(conflicts::choose_materialized_conflict_marker_len(
                &conflict.contents,
            )),
            merge: store.merge_options().clone(),
        };
        let bytes = conflicts::materialize_merge_result_to_bytes(
            &conflict.contents,
            &conflict.labels,
            &options,
        );
        Ok(Some(bytes.into()))
    }
}
