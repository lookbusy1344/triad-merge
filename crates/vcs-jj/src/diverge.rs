//! `diverge`: one commit from the visible commits of a divergent change, merged over the
//! predecessor they share in the evolog (`DESIGN.md` §I3).

use std::collections::HashSet;

use camino::Utf8PathBuf;
use futures::TryStreamExt as _;
use jj_lib::backend::{ChangeId, CommitId};
use jj_lib::commit::Commit;
use jj_lib::evolution::walk_predecessors;
use jj_lib::merge::Merge;
use jj_lib::merged_tree::MergedTree;
use jj_lib::object_id::{HexPrefix, PrefixResolution};
use jj_lib::repo::{ReadonlyRepo, Repo};
use pollster::FutureExt as _;
use vcs::{ChangeInfo, IdKind, VcsError};

use crate::native::{Mutated, Rewrite, conflicted_paths};
use crate::{JjError, JjRepository, backend};

/// What `diverge` did, or would do on a dry run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The rewrite of the first copy, which keeps the change.
    pub rewrite: Rewrite,
    /// The predecessor each further copy merged over, in the order they merged.
    pub bases: Vec<ChangeInfo>,
    /// The further copies, abandoned into the merged commit.
    pub abandoned: Vec<ChangeInfo>,
    /// The merged commit's conflicted paths.
    pub conflicted: Vec<Utf8PathBuf>,
    /// The copies changed the description two ways; the first copy's stays.
    pub description_kept: bool,
}

impl JjRepository {
    /// Merge the visible commits of the divergent change `change`, a change ID prefix or a
    /// revision naming one of its commits, into the first of them: its tree becomes jj's
    /// merge of each further copy over the nearest predecessor the two share, and its
    /// description merges the same way. The further copies are abandoned into it, so their
    /// descendants move onto it. On a dry run nothing is written.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] and [`VcsError::AmbiguousRevision`] when `change` names
    /// no change or several; [`VcsError::Backend`] holding [`JjError::NotDivergent`] for a
    /// change with one visible commit and [`JjError::NoCommonPredecessor`] for copies that
    /// share none; [`VcsError::Immutable`] before anything is written; and the errors of a
    /// write ([`JjRepository::write_conflict`]).
    pub fn diverge(&mut self, change: &str, dry_run: bool) -> Result<Divergence, VcsError> {
        let change_id = self.change_named(change)?;
        let copies = copies(self.repo.as_ref(), &change_id)?;
        self.check_mutable(&copies)?;
        let mut merged = None;
        let rewrite = self.rewrite(dry_run, |this, repo| {
            let copies = copies_after_snapshot(repo, &change_id)?;
            let [first, rest @ ..] = copies.as_slice() else {
                return Err(not_divergent(&change_id));
            };
            let mut tree = first.tree();
            let mut description = first.description().to_owned();
            let mut description_kept = false;
            let mut bases = Vec::with_capacity(rest.len());
            for other in rest {
                let base = common_predecessor(&this.repo, first, other)?;
                tree = MergedTree::merge(Merge::from_vec(vec![
                    (tree, first.conflict_label()),
                    (base.tree(), base.conflict_label()),
                    (other.tree(), other.conflict_label()),
                ]))
                .block_on()
                .map_err(backend("merging the copies' trees"))?;
                let (merged_description, kept) =
                    merge_description(base.description(), &description, other.description());
                description = merged_description;
                description_kept |= kept;
                bases.push(this.change_info(&base)?);
            }
            let commit = repo
                .rewrite_commit(first)
                .set_tree(tree)
                .set_description(description)
                .set_predecessors(copies.iter().map(|c| c.id().clone()).collect())
                .write()
                .block_on()
                .map_err(backend("writing the merged commit"))?;
            for other in rest {
                repo.set_rewritten_commit(other.id().clone(), commit.id().clone());
            }
            let abandoned = rest.iter().map(|c| this.change_info(c)).collect::<Result<_, _>>()?;
            merged = Some((bases, abandoned, conflicted_paths(&commit)?, description_kept));
            let change = this.change_info(first)?;
            Ok(Mutated {
                description: format!(
                    "merge divergent change {} in triad",
                    change.id(IdKind::Change)
                ),
                target: first.clone(),
                path: None,
                roots: copies.clone(),
            })
        })?;
        let (bases, abandoned, conflicted, description_kept) =
            merged.ok_or_else(|| not_divergent(&change_id))?;
        Ok(Divergence { rewrite, bases, abandoned, conflicted, description_kept })
    }

    /// The change `name` names: a change ID prefix, else the change of the one commit a
    /// revision names.
    fn change_named(&self, name: &str) -> Result<ChangeId, VcsError> {
        if let Some(prefix) = HexPrefix::try_from_reverse_hex(name) {
            match self
                .repo
                .resolve_change_id_prefix(&prefix)
                .block_on()
                .map_err(backend("looking up a change"))?
            {
                PrefixResolution::SingleMatch(targets) => {
                    if let Some((_, id)) = targets.visible_with_offsets().next() {
                        let commit = self
                            .repo
                            .store()
                            .get_commit(id)
                            .map_err(backend("reading a commit"))?;
                        return Ok(commit.change_id().clone());
                    }
                }
                PrefixResolution::AmbiguousMatch => {
                    return Err(VcsError::AmbiguousRevision(name.to_owned()));
                }
                PrefixResolution::NoMatch => {}
            }
        }
        match self.resolve(name)?.as_slice() {
            [commit] => Ok(commit.change_id().clone()),
            _ => Err(VcsError::AmbiguousRevision(name.to_owned())),
        }
    }
}

/// The refusal of a change with one visible commit.
fn not_divergent(change_id: &ChangeId) -> VcsError {
    backend("merging a divergent change")(JjError::NotDivergent(change_id.reverse_hex()))
}

/// The visible commits of `change_id`, in the order the index lists them; at least two.
fn copies(repo: &dyn Repo, change_id: &ChangeId) -> Result<Vec<Commit>, VcsError> {
    let found = copies_after_snapshot(repo, change_id)?;
    if found.len() < 2 {
        return Err(not_divergent(change_id));
    }
    Ok(found)
}

/// The visible commits of `change_id`, in the order the index lists them.
fn copies_after_snapshot(repo: &dyn Repo, change_id: &ChangeId) -> Result<Vec<Commit>, VcsError> {
    let Some(targets) =
        repo.resolve_change_id(change_id).block_on().map_err(backend("looking up a change"))?
    else {
        return Err(VcsError::UnknownRevision(change_id.reverse_hex()));
    };
    targets
        .visible_with_offsets()
        .map(|(_, id)| repo.store().get_commit(id).map_err(backend("reading a commit")))
        .collect()
}

/// The nearest predecessor `one` and `two` share in the evolog: the first of `two`'s
/// predecessors, nearest first, that is also one of `one`'s.
fn common_predecessor(repo: &ReadonlyRepo, one: &Commit, two: &Commit) -> Result<Commit, VcsError> {
    let evolog = |commit: &Commit| -> Result<Vec<Commit>, VcsError> {
        walk_predecessors(repo, std::slice::from_ref(commit.id()))
            .map_ok(|entry| entry.commit)
            .try_collect()
            .block_on()
            .map_err(backend("walking the evolog"))
    };
    let own: HashSet<CommitId> = evolog(one)?.into_iter().map(|c| c.id().clone()).collect();
    evolog(two)?
        .into_iter()
        .filter(|commit| commit.id() != one.id() && commit.id() != two.id())
        .find(|commit| own.contains(commit.id()))
        .ok_or_else(|| {
            backend("merging a divergent change")(JjError::NoCommonPredecessor(
                one.change_id().reverse_hex(),
            ))
        })
}

/// `first` and `other` merged over `base`: the side that changed wins; when both changed it
/// differently, `first` stays and the second value is true.
fn merge_description(base: &str, first: &str, other: &str) -> (String, bool) {
    if first == other || other == base {
        (first.to_owned(), false)
    } else if first == base {
        (other.to_owned(), false)
    } else {
        (first.to_owned(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_changed_on_one_copy_takes_that_change() {
        assert_eq!(merge_description("b", "b", "o"), ("o".to_owned(), false));
        assert_eq!(merge_description("b", "f", "b"), ("f".to_owned(), false));
        assert_eq!(merge_description("b", "s", "s"), ("s".to_owned(), false));
        assert_eq!(merge_description("b", "b", "b"), ("b".to_owned(), false));
    }

    #[test]
    fn a_description_changed_two_ways_keeps_the_first_copy_s() {
        assert_eq!(merge_description("b", "f", "o"), ("f".to_owned(), true));
    }
}
