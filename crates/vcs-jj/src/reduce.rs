//! `resolve --pair`: two adjacent sides of a conflict of more than two sides merged over the
//! base between them, so the conflict keeps one side fewer (`DESIGN.md` §I3).

use camino::Utf8Path;
use jj_lib::backend::{CopyId, MergedTreeValueExt as _, TreeValue};
use jj_lib::conflicts::resolve_file_executable;
use jj_lib::merge::Merge;
use jj_lib::merged_tree_builder::MergedTreeBuilder;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::RepoPathBuf;
use pollster::FutureExt as _;
use vcs::{IdKind, VcsError};

use crate::native::{Mutated, Rewrite, kept_terms, visible};
use crate::{JjError, JjRepository, backend};

/// Terms per side in jj's order: a side, then the base after it.
const TERMS_PER_SIDE: usize = 2;
/// The fewest sides a pair can be taken from: with two, the pair is the whole conflict.
pub const MIN_PAIR_SIDES: usize = 3;

/// Refuse pair `pair`, counted from 1, of a conflict of `sides` sides at `path`: a conflict
/// of two sides, and a pair past the last side.
///
/// # Errors
///
/// [`JjError::PairOfTwoSides`] and [`JjError::PairOutOfRange`].
pub fn check_pair(path: &Utf8Path, pair: usize, sides: usize) -> Result<(), JjError> {
    if sides < MIN_PAIR_SIDES {
        return Err(JjError::PairOfTwoSides(path.to_owned()));
    }
    if pair == 0 || pair >= sides {
        return Err(JjError::PairOutOfRange { pair, sides });
    }
    Ok(())
}

impl JjRepository {
    /// Replace sides `pair` and `pair + 1`, counted from 1 in jj's order after jj simplifies the
    /// conflict, and base `pair`
    /// between them with `content`, in the conflict at `path` in the change of commit `id`:
    /// the conflict keeps one side fewer, and resolves when one is left. The descendants
    /// rebase; a moved working copy is checked out. On a dry run nothing is written.
    ///
    /// # Errors
    ///
    /// [`VcsError::NotConflicted`] for a resolved path; [`VcsError::Backend`] holding
    /// [`JjError::PairOfTwoSides`] for a conflict of two sides,
    /// [`JjError::PairOutOfRange`] for pair 0 or one past the last side, and
    /// [`JjError::NotAFileConflict`] for a conflict that holds anything but files,
    /// [`JjError::ConflictChanged`] when the conflict changed since commit `id`; and the
    /// errors of [`JjRepository::write_conflict`].
    pub fn reduce_pair(
        &mut self,
        id: &vcs::CommitId,
        path: &Utf8Path,
        pair: usize,
        content: &[u8],
        dry_run: bool,
    ) -> Result<Rewrite, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let named = self.commit_by_id(id)?;
        let revsets = self.revsets()?;
        self.check_mutable(std::slice::from_ref(&named), &revsets)?;
        let change_id = named.change_id().clone();
        self.rewrite(&revsets, dry_run, |this, repo| {
            let commit = visible(repo, &change_id)?;
            let value = this.held_conflict(&named, &commit, &repo_path, path)?;
            // Sides count as jj shows them: the pair indexes the simplified conflict, whose
            // terms sit at `kept`'s positions in the tree's value.
            let kept = kept_terms(&value);
            let refused = |error| backend("reducing a conflict")(error);
            check_pair(path, pair, kept.num_sides()).map_err(refused)?;
            let executable = value
                .to_executable_merge()
                .ok_or_else(|| refused(JjError::NotAFileConflict(path.to_owned())))?;
            let first = TERMS_PER_SIDE * (pair - 1);
            let [side, base, next] =
                [first, first + 1, first + TERMS_PER_SIDE].map(|index| kept.as_slice()[index]);
            let bits = Merge::from_vec(
                [side, base, next].map(|index| executable.as_slice()[index]).to_vec(),
            );
            let id = repo
                .store()
                .write_file(&repo_path, &mut &content[..])
                .block_on()
                .map_err(backend("writing the merged pair"))?;
            let merged = Some(TreeValue::File {
                id,
                executable: resolve_file_executable(&bits).unwrap_or_default(),
                copy_id: CopyId::placeholder(),
            });
            // The merged file takes side N's place and side N + 1 takes base N's value, which
            // cancels it: each term stays where the tree's labels name it.
            let mut terms: Vec<Option<TreeValue>> = value.iter().cloned().collect();
            let cancelling = terms[base].clone();
            terms[next] = cancelling;
            terms[side] = merged;
            let reduced = Merge::from_vec(terms);
            let reduced = match reduced.simplify().into_resolved() {
                Ok(resolved) => Merge::resolved(resolved),
                Err(_) => reduced,
            };
            let mut builder = MergedTreeBuilder::new(commit.tree());
            builder
                .set_or_remove_merge(repo_path.clone(), reduced)
                .map_err(backend("writing the reduced conflict"))?;
            let tree = builder.write_tree().block_on().map_err(backend("writing a tree"))?;
            repo.rewrite_commit(&commit)
                .set_tree(tree)
                .write()
                .block_on()
                .map_err(backend("rewriting the commit"))?;
            let change = this.change_info(&commit)?;
            Ok(Mutated {
                description: format!("reduce {path} in {} in triad", change.id(IdKind::Change)),
                roots: vec![commit.clone()],
                target: commit,
                path: Some(repo_path.clone()),
            })
        })
    }
}
