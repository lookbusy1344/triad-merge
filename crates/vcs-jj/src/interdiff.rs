//! A change against an earlier version of itself, rebased onto the change's parents, as
//! `jj interdiff` compares them (`diff --since`, spec §7.5.4, `DESIGN.md` §I6).

use camino::{Utf8Path, Utf8PathBuf};
use futures::StreamExt as _;
use jj_lib::commit::Commit;
use jj_lib::conflicts::{
    ConflictMarkerStyle, ConflictMaterializeOptions, materialize_merge_result_to_bytes,
    try_materialize_file_conflict_value,
};
use jj_lib::matchers::EverythingMatcher;
use jj_lib::merged_tree::MergedTree;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::rewrite::rebase_to_dest_parent;
use pollster::FutureExt as _;
use vcs::{ChangeInfo, Repository as _, TermContent, VcsError};

use crate::{JjRepository, backend, config};

/// Two versions of a change: `old` to be rebased onto `new`'s parents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interdiff {
    pub old: vcs::CommitId,
    pub new: vcs::CommitId,
    /// Short IDs and summaries, as titles name them.
    pub old_info: ChangeInfo,
    pub new_info: ChangeInfo,
    /// `old`'s parents differ from `new`'s, so `old` is rebased before the comparison.
    pub rebased: bool,
}

/// What a path holds on one side of an interdiff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterdiffSide {
    Absent,
    /// A file's bytes; a conflict of files written with jj's markers.
    File(Vec<u8>),
    /// A symlink, a submodule, or a conflict that is not of files alone.
    Other,
}

/// The config key jj reads for the markers it writes.
const MARKER_STYLE_KEY: &str = "ui.conflict-marker-style";

impl JjRepository {
    /// The commits `since` and `rev` each name, after a snapshot of the working copy, as
    /// `jj interdiff --from SINCE --to REV` reads them. A hidden commit resolves by its ID.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] when either names no commit and
    /// [`VcsError::AmbiguousRevision`] when either names several; the errors of a snapshot;
    /// and [`VcsError::Backend`] when the config or the history cannot be read.
    pub fn interdiff(&mut self, since: &str, rev: &str) -> Result<Interdiff, VcsError> {
        let revsets = self.revsets()?;
        self.snapshot()?;
        let old = self.one_commit(self.repo.as_ref(), since, &revsets)?;
        let new = self.one_commit(self.repo.as_ref(), rev, &revsets)?;
        Ok(Interdiff {
            old: crate::commit_id(old.id()),
            new: crate::commit_id(new.id()),
            old_info: self.change_info(&old)?,
            new_info: self.change_info(&new)?,
            rebased: old.parent_ids() != new.parent_ids(),
        })
    }

    /// The paths whose values differ between the rebased old tree and the new one, in path
    /// order.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] when a commit is gone, and [`VcsError::Backend`] when
    /// the trees cannot be read or merged.
    pub fn interdiff_paths(&self, interdiff: &Interdiff) -> Result<Vec<Utf8PathBuf>, VcsError> {
        let [old, new] = self.interdiff_trees(interdiff)?;
        let mut paths = Vec::new();
        let mut stream = old.diff(&new, &EverythingMatcher);
        while let Some(entry) = stream.next().block_on() {
            entry.values.map_err(backend("comparing the trees"))?;
            paths.push(Utf8PathBuf::from(entry.path.as_internal_file_string()));
        }
        Ok(paths)
    }

    /// What `path` holds in the rebased old tree and in the new one. A conflict of files is
    /// written with jj's markers in the user's `ui.conflict-marker-style`, as `jj diff`
    /// shows it.
    ///
    /// # Errors
    ///
    /// [`VcsError::OutsideRepository`] for a path with `..`, a leading `.` or a root; and the
    /// errors of [`JjRepository::interdiff_paths`].
    pub fn interdiff_sides(
        &self,
        interdiff: &Interdiff,
        path: &Utf8Path,
    ) -> Result<[InterdiffSide; 2], VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let options = self.materialize_options()?;
        let [old, new] = self.interdiff_trees(interdiff)?;
        Ok([self.side(&old, &repo_path, &options)?, self.side(&new, &repo_path, &options)?])
    }
}

impl JjRepository {
    /// The old tree rebased onto the new commit's parents, and the new commit's tree.
    fn interdiff_trees(&self, interdiff: &Interdiff) -> Result<[MergedTree; 2], VcsError> {
        let old: Commit = self.commit_by_id(&interdiff.old)?;
        let new: Commit = self.commit_by_id(&interdiff.new)?;
        let rebased = rebase_to_dest_parent(self.repo.index(), std::slice::from_ref(&old), &new)
            .block_on()
            .map_err(backend("rebasing the earlier version"))?;
        Ok([rebased, new.tree()])
    }

    fn materialize_options(&self) -> Result<ConflictMaterializeOptions, VcsError> {
        let env = self.config.clone().unwrap_or_else(config::ConfigEnv::from_process);
        let loaded =
            config::load(&env, self.root.as_std_path()).map_err(backend("reading jj's config"))?;
        let marker_style: ConflictMarkerStyle = loaded
            .settings
            .get(MARKER_STYLE_KEY)
            .map_err(backend("reading jj's conflict marker style"))?;
        Ok(ConflictMaterializeOptions {
            marker_style,
            marker_len: None,
            merge: self.repo.store().merge_options().clone(),
        })
    }

    fn side(
        &self,
        tree: &MergedTree,
        path: &RepoPathBuf,
        options: &ConflictMaterializeOptions,
    ) -> Result<InterdiffSide, VcsError> {
        let value =
            tree.path_value(path).block_on().map_err(backend("finding a path in a tree"))?;
        if let Some(resolved) = value.as_resolved() {
            return Ok(match self.term_content(path, resolved.as_ref())? {
                TermContent::Absent => InterdiffSide::Absent,
                TermContent::File { bytes, .. } => InterdiffSide::File(bytes),
                TermContent::Other(_) => InterdiffSide::Other,
            });
        }
        let file =
            try_materialize_file_conflict_value(self.repo.store(), path, &value, tree.labels())
                .block_on()
                .map_err(backend("reading a conflict"))?;
        Ok(file.map_or(InterdiffSide::Other, |file| {
            InterdiffSide::File(
                materialize_merge_result_to_bytes(&file.contents, &file.labels, options).to_vec(),
            )
        }))
    }
}
