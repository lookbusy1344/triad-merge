//! jj-native operations (spec §1.2, `DESIGN.md` part I): what git cannot record, reached
//! through `JjRepository`'s own API. Every write snapshots the working copy first, rewrites
//! in one transaction, rebases the descendants, and checks out a moved working-copy commit.

use camino::{Utf8Path, Utf8PathBuf};
use futures::{StreamExt as _, TryStreamExt as _};
use jj_lib::backend::{ChangeId, CopyId, MergedTreeValue, MergedTreeValueExt as _, TreeValue};
use jj_lib::commit::Commit;
use jj_lib::conflicts::{resolve_file_executable, update_from_content};
use jj_lib::merge::Merge;
use jj_lib::merged_tree_builder::MergedTreeBuilder;
use jj_lib::object_id::ObjectId as _;
use jj_lib::repo::{MutableRepo, Repo as _};
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::revset::RevsetAliasesMap;
use pollster::FutureExt as _;
use vcs::{ChangeInfo, VcsError};

use crate::{JjRepository, backend, colocated_git, config, export_refs};

/// jj's default revset aliases for what may be rewritten, as jj's own config sets them. The
/// user's `revset-aliases` replace any of them.
const DEFAULT_ALIASES: [(&str, &str); 5] = [
    (
        "trunk()",
        r#"latest(
            remote_bookmarks(exact:"main", exact:"origin") |
            remote_bookmarks(exact:"master", exact:"origin") |
            remote_bookmarks(exact:"trunk", exact:"origin") |
            remote_bookmarks(exact:"main", exact:"upstream") |
            remote_bookmarks(exact:"master", exact:"upstream") |
            remote_bookmarks(exact:"trunk", exact:"upstream") |
            root()
        )"#,
    ),
    ("builtin_immutable_heads()", "present(trunk()) | tags() | untracked_remote_bookmarks()"),
    ("immutable_heads()", "builtin_immutable_heads()"),
    ("immutable()", "::(immutable_heads() | root())"),
    ("mutable()", "~immutable()"),
];
/// The config table that holds the user's revset aliases.
const ALIASES_TABLE: &str = "revset-aliases";
/// `stack` without a revset: every commit jj would let the user resolve in.
pub const DEFAULT_STACK: &str = "conflicts() & mutable()";

/// A conflicted commit, as `stack` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackCommit {
    /// The commit's full ID.
    pub id: vcs::CommitId,
    pub change: ChangeInfo,
    /// The conflicted paths, in path order.
    pub paths: Vec<StackPath>,
}

/// A conflicted path of a [`StackCommit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackPath {
    pub path: Utf8PathBuf,
    /// A parent holds a conflict at the same path: the conflict came with the parent.
    pub inherited: bool,
}

/// What a rewrite did, or would do on a dry run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// The commit rewritten, as it was.
    pub target: ChangeInfo,
    /// The rewrite left the commit's path resolved.
    pub resolved: bool,
    /// Each descendant the rewrite rebased, parents first.
    pub descendants: Vec<DescendantEffect>,
    /// The rewrite is written: false on a dry run.
    pub written: bool,
}

/// One descendant's conflicted paths before and after a rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescendantEffect {
    /// The descendant, as it was.
    pub change: ChangeInfo,
    pub before: Vec<Utf8PathBuf>,
    pub after: Vec<Utf8PathBuf>,
}

/// What a rewrite's closure changed: the commit the rewrite is about, as it was; the
/// commits it rewrote, as they were, none when nothing changed; and the operation's
/// description.
pub(crate) struct Mutated {
    pub target: Commit,
    pub roots: Vec<Commit>,
    pub description: String,
}

impl JjRepository {
    /// The user's revset aliases over jj's defaults for `trunk()`, `immutable_heads()`,
    /// `immutable()` and `mutable()`.
    fn aliases(&self) -> Result<RevsetAliasesMap, VcsError> {
        let env = self.config.clone().unwrap_or_else(config::ConfigEnv::from_process);
        let loaded =
            config::load(&env, self.root.as_std_path()).map_err(backend("reading jj's config"))?;
        let config = loaded.settings.config();
        let mut aliases = RevsetAliasesMap::new();
        for (decl, defn) in DEFAULT_ALIASES {
            aliases.insert(decl, defn, None).map_err(backend("declaring a revset alias"))?;
        }
        for decl in config.table_keys(ALIASES_TABLE) {
            let defn: String =
                config.get([ALIASES_TABLE, decl]).map_err(backend("reading a revset alias"))?;
            aliases.insert(decl, defn, None).map_err(backend("declaring a revset alias"))?;
        }
        Ok(aliases)
    }

    /// The commits `revset` names that hold conflicts, parents first, with the user's
    /// aliases. Default: [`DEFAULT_STACK`].
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] and [`VcsError::AmbiguousRevision`] as for
    /// [`vcs::Repository::commits`], and [`VcsError::Backend`] when the config or the history
    /// cannot be read.
    pub fn stack(&self, revset: Option<&str>) -> Result<Vec<StackCommit>, VcsError> {
        let aliases = self.aliases()?;
        let mut commits =
            self.evaluate(self.repo.as_ref(), revset.unwrap_or(DEFAULT_STACK), &aliases)?;
        commits.reverse();
        commits
            .iter()
            .filter(|commit| commit.has_conflict())
            .map(|commit| {
                let parents: Vec<Commit> = commit
                    .parent_ids()
                    .iter()
                    .map(|id| self.repo.store().get_commit(id))
                    .collect::<Result<_, _>>()
                    .map_err(backend("reading a parent commit"))?;
                let paths = conflicted_paths(commit)?
                    .into_iter()
                    .map(|path| {
                        let repo_path = RepoPathBuf::from_internal_string(path.as_str())
                            .map_err(backend("naming a conflicted path"))?;
                        let inherited = parents.iter().try_fold(false, |found, parent| {
                            Ok::<_, VcsError>(
                                found
                                    || !parent
                                        .tree()
                                        .path_value(&repo_path)
                                        .block_on()
                                        .map_err(backend("finding a path in a tree"))?
                                        .is_resolved(),
                            )
                        })?;
                        Ok(StackPath { path, inherited })
                    })
                    .collect::<Result<_, VcsError>>()?;
                Ok(StackCommit {
                    id: crate::commit_id(commit.id()),
                    change: self.change_info(commit)?,
                    paths,
                })
            })
            .collect()
    }

    /// The conflict at `path` in the commit `id`, as [`vcs::Repository::read_conflict`]
    /// reads the working copy's. None when the path is resolved there.
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] when the repository holds no commit `id`, and the errors
    /// of [`vcs::Repository::read_conflict`].
    pub fn conflict_at(
        &self,
        id: &vcs::CommitId,
        path: &Utf8Path,
    ) -> Result<Option<vcs::Conflict>, VcsError> {
        self.conflict_in(&self.commit_by_id(id)?, path)
    }

    /// Write `content` as `path` in the commit `id`'s change, as `jj resolve -r` does: markers
    /// at least `marker_length` long that jj parses back stay a conflict, of fewer sides when
    /// the content resolves some. The descendants rebase; a moved working copy is checked
    /// out. On a dry run nothing is written.
    ///
    /// # Errors
    ///
    /// [`VcsError::Immutable`] for a commit jj would not rewrite and
    /// [`VcsError::NotConflicted`] for a path resolved there, both before any write;
    /// [`VcsError::StaleWorkspace`] as for [`vcs::Repository::snapshot`]; and
    /// [`VcsError::Backend`] when the repository cannot be read or written.
    pub fn write_conflict(
        &mut self,
        id: &vcs::CommitId,
        path: &Utf8Path,
        content: &[u8],
        marker_length: usize,
        dry_run: bool,
    ) -> Result<Rewrite, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let named = self.commit_by_id(id)?;
        self.check_mutable(std::slice::from_ref(&named))?;
        let change_id = named.change_id().clone();
        self.rewrite(dry_run, |this, repo| {
            let commit = visible(repo, &change_id)?;
            let value = commit
                .tree()
                .path_value(&repo_path)
                .block_on()
                .map_err(backend("finding a path in a tree"))?;
            if value.is_resolved() {
                return Err(VcsError::NotConflicted { path: path.to_owned(), rev: id.to_string() });
            }
            let new_value = parse_back(repo, &repo_path, &value, content, marker_length)?;
            let sides = commit.tree().tree_ids().num_sides();
            let mut builder = MergedTreeBuilder::new(commit.tree());
            builder
                .set_or_remove_merge(repo_path.clone(), pad(new_value, sides))
                .map_err(backend("writing the resolved path"))?;
            let tree = builder.write_tree().block_on().map_err(backend("writing a tree"))?;
            let change = this.change_info(&commit)?;
            let description =
                format!("resolve {path} in {} in triad", change.id(vcs::IdKind::Change));
            // Markers that read back as the conflict jj holds leave the commit as it is.
            if tree.tree_ids_and_labels() == commit.tree().tree_ids_and_labels() {
                return Ok(Mutated { target: commit, roots: Vec::new(), description });
            }
            repo.rewrite_commit(&commit)
                .set_tree(tree)
                .write()
                .block_on()
                .map_err(backend("rewriting the commit"))?;
            Ok(Mutated { roots: vec![commit.clone()], target: commit, description })
        })
    }

    /// Refuse a write to any of `commits` that jj's `immutable()` holds, with the user's
    /// aliases.
    pub(crate) fn check_mutable(&self, commits: &[Commit]) -> Result<(), VcsError> {
        if commits.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = commits.iter().map(|commit| commit.id().hex()).collect();
        let expr = format!("({}) & immutable()", ids.join(" | "));
        let immutable = self.evaluate(self.repo.as_ref(), &expr, &self.aliases()?)?;
        match immutable.first() {
            Some(commit) => Err(VcsError::Immutable(
                self.change_info(commit)?.id(vcs::IdKind::Commit).to_owned(),
            )),
            None => Ok(()),
        }
    }

    /// Each strict descendant of `roots` in `repo`, parents first, with its conflicted paths.
    fn descendants(
        &self,
        repo: &dyn jj_lib::repo::Repo,
        roots: &[Commit],
    ) -> Result<Vec<(Commit, Vec<Utf8PathBuf>)>, VcsError> {
        if roots.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = roots.iter().map(|commit| commit.id().hex()).collect();
        let set = ids.join(" | ");
        let mut found = self.evaluate(
            repo,
            &format!("descendants({set}) ~ ({set})"),
            &RevsetAliasesMap::new(),
        )?;
        found.reverse();
        found
            .into_iter()
            .map(|commit| conflicted_paths(&commit).map(|paths| (commit, paths)))
            .collect()
    }

    /// Snapshot the working copy, rewrite through `mutate` in one transaction, rebase the
    /// descendants, and unless `dry_run` commit the operation and check out a moved working
    /// copy. `mutate` sees the repository after the snapshot and names the commits it
    /// rewrote, which must be mutable.
    pub(crate) fn rewrite(
        &mut self,
        dry_run: bool,
        mutate: impl FnOnce(&Self, &mut MutableRepo) -> Result<Mutated, VcsError>,
    ) -> Result<Rewrite, VcsError> {
        vcs::Repository::snapshot(self)?;
        let env = self.config.clone().unwrap_or_else(config::ConfigEnv::from_process);
        let config::JjConfig { mut workspace, .. } =
            config::load(&env, self.root.as_std_path()).map_err(backend("reading jj's config"))?;
        // One store for the whole rewrite: the checkout compares trees by their store.
        self.repo = crate::head_repo(&workspace)?;
        let repo = std::sync::Arc::clone(&self.repo);
        let old_wc = self.working_copy_commit()?;
        let mut tx = repo.start_transaction();
        tx.set_workspace_name(&self.workspace);
        let Mutated { target, roots, description } = mutate(self, tx.repo_mut())?;
        self.check_mutable(&roots)?;
        let before = self.descendants(repo.as_ref(), &roots)?;
        tx.repo_mut()
            .rebase_descendants()
            .block_on()
            .map_err(backend("rebasing the descendants"))?;
        let resolved = !visible(tx.repo(), target.change_id())?.has_conflict();
        let target = self.change_info(&target)?;
        let descendants = before
            .into_iter()
            .map(|(commit, before)| {
                let after = match visible(tx.repo(), commit.change_id()) {
                    Ok(now) => conflicted_paths(&now)?,
                    Err(VcsError::UnknownRevision(_) | VcsError::AmbiguousRevision(_)) => {
                        Vec::new()
                    }
                    Err(error) => return Err(error),
                };
                Ok(DescendantEffect { change: self.change_info(&commit)?, before, after })
            })
            .collect::<Result<Vec<_>, VcsError>>()?;
        if dry_run || !tx.repo().has_changes() {
            return Ok(Rewrite { target, resolved, descendants, written: false });
        }
        let new_wc_id = tx
            .repo()
            .view()
            .get_wc_commit_id(&self.workspace)
            .cloned()
            .ok_or_else(|| VcsError::NoWorkingCopy(self.root.clone()))?;
        let new_wc = (new_wc_id != *old_wc.id())
            .then(|| tx.repo().store().get_commit(&new_wc_id))
            .transpose()
            .map_err(backend("reading the working-copy commit"))?;
        if colocated_git(&repo, workspace.workspace_root()).is_some() {
            if let Some(new_wc) = &new_wc {
                jj_lib::git::reset_head(
                    tx.repo_mut(),
                    &self.workspace,
                    workspace.workspace_root(),
                    (Some(&old_wc), new_wc),
                )
                .block_on()
                .map_err(backend("moving git's HEAD"))?;
            }
            export_refs(tx.repo_mut())?;
        }
        let repo = tx.commit(description).block_on().map_err(backend("writing the operation"))?;
        if let Some(new_wc) = &new_wc {
            workspace
                .check_out(repo.op_id().clone(), Some(&old_wc.tree()), new_wc)
                .block_on()
                .map_err(backend("checking out the working copy"))?;
        }
        self.repo = repo;
        Ok(Rewrite { target, resolved, descendants, written: true })
    }
}

/// The one visible commit of `change_id` in `repo`.
pub(crate) fn visible(
    repo: &dyn jj_lib::repo::Repo,
    change_id: &ChangeId,
) -> Result<Commit, VcsError> {
    let named = || change_id.reverse_hex();
    let targets = repo
        .resolve_change_id(change_id)
        .block_on()
        .map_err(backend("looking up a change"))?
        .ok_or_else(|| VcsError::UnknownRevision(named()))?;
    let mut visible = targets.visible_with_offsets().map(|(_, id)| id);
    let (Some(id), None) = (visible.next(), visible.next()) else {
        return Err(VcsError::AmbiguousRevision(named()));
    };
    repo.store().get_commit(id).map_err(backend("reading a commit"))
}

/// `content` parsed back into `value`'s conflict, as jj does after a merge tool: markers jj
/// reads keep their hunks conflicted, and content without them resolves the path. Each term
/// keeps its executable bit; a resolved file takes the bit the terms agree on.
fn parse_back(
    repo: &MutableRepo,
    path: &jj_lib::repo_path::RepoPath,
    value: &MergedTreeValue,
    content: &[u8],
    marker_length: usize,
) -> Result<MergedTreeValue, VcsError> {
    let not_a_file = || VcsError::NotConflicted {
        path: path.as_internal_file_string().into(),
        rev: String::new(),
    };
    let file_ids = value.to_file_merge().ok_or_else(not_a_file)?;
    let executable = value.to_executable_merge().ok_or_else(not_a_file)?;
    let new_ids = update_from_content(&file_ids, repo.store(), path, content, marker_length)
        .block_on()
        .map_err(backend("parsing the conflict back"))?;
    Ok(match new_ids.into_resolved() {
        Ok(None) => Merge::absent(),
        Ok(Some(id)) => Merge::normal(TreeValue::File {
            id,
            executable: resolve_file_executable(&executable).unwrap_or_default(),
            copy_id: CopyId::placeholder(),
        }),
        Err(ids) if ids.num_sides() == value.num_sides() => value.with_new_file_ids(&ids),
        Err(ids) => {
            let simplified = value.simplify();
            if simplified.num_sides() == ids.num_sides() {
                simplified.with_new_file_ids(&ids)
            } else {
                ids.map(|id| {
                    id.clone().map(|id| TreeValue::File {
                        id,
                        executable: false,
                        copy_id: CopyId::placeholder(),
                    })
                })
            }
        }
    })
}

/// `value` with as many sides as a tree of `sides`: each missing side comes as a base and a
/// side holding the last side's value, which cancel. A resolved value stays as it is.
pub(crate) fn pad(value: MergedTreeValue, sides: usize) -> MergedTreeValue {
    if value.is_resolved() || value.num_sides() >= sides {
        return value;
    }
    let missing = sides - value.num_sides();
    let filler = value.adds().last().cloned().flatten();
    let mut terms: Vec<Option<TreeValue>> = value.into_iter().collect();
    terms.extend(std::iter::repeat_n(filler, 2 * missing));
    Merge::from_vec(terms)
}

/// The conflicted paths of `commit`'s tree, in path order.
pub(crate) fn conflicted_paths(commit: &Commit) -> Result<Vec<Utf8PathBuf>, VcsError> {
    commit
        .tree()
        .conflicts()
        .map(|(path, value)| {
            value.map_err(backend("reading a conflict"))?;
            Ok(Utf8PathBuf::from(path.as_internal_file_string()))
        })
        .try_collect()
        .block_on()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_aliases_declare_and_name_mutable() {
        let mut aliases = RevsetAliasesMap::new();
        for (decl, defn) in DEFAULT_ALIASES {
            assert!(aliases.insert(decl, defn, None).is_ok(), "{decl} declares");
        }
        assert!(aliases.function_names().any(|name| name == "mutable"));
        assert!(DEFAULT_STACK.contains("mutable()"));
    }

    #[test]
    fn padding_adds_cancelling_pairs_up_to_the_tree_s_sides() {
        let value = |n: u8| Some(TreeValue::GitSubmodule(jj_lib::backend::CommitId::new(vec![n])));
        let two = Merge::from_vec(vec![value(1), value(0), value(2)]);
        let padded = pad(two.clone(), 3);
        assert_eq!(padded.num_sides(), 3);
        assert_eq!(padded.simplify(), two);
        assert_eq!(pad(two.clone(), 2), two);
        let resolved =
            Merge::normal(TreeValue::GitSubmodule(jj_lib::backend::CommitId::new(vec![9])));
        assert_eq!(pad(resolved.clone(), 3), resolved);
    }
}
