//! jj-native operations (spec §1.2, `DESIGN.md` part I): what git cannot record, reached
//! through `JjRepository`'s own API. Every write snapshots the working copy first, rewrites
//! in one transaction, rebases the descendants, and checks out a moved working-copy commit.

use std::collections::HashSet;

use camino::{Utf8Path, Utf8PathBuf};
use futures::{StreamExt as _, TryStreamExt as _};
use jj_lib::backend::{
    ChangeId, CommitId, CopyId, MergedTreeValue, MergedTreeValueExt as _, TreeValue,
};
use jj_lib::commit::Commit;
use jj_lib::conflict_labels::ConflictLabels;
use jj_lib::conflicts::{resolve_file_executable, update_from_content};
use jj_lib::merge::Merge;
use jj_lib::merged_tree::MergedTree;
use jj_lib::merged_tree_builder::MergedTreeBuilder;
use jj_lib::object_id::ObjectId as _;
use jj_lib::repo::{MutableRepo, ReadonlyRepo, Repo};
use jj_lib::repo_path::{RepoPath, RepoPathBuf};
use jj_lib::revset::RevsetAliasesMap;
use pollster::FutureExt as _;
use vcs::{ChangeInfo, VcsError};

use crate::{JjError, JjRepository, Revsets, backend, colocated_git, config, export_refs};

/// jj's default revset aliases, as jj's own config sets them, less `builtin_log()`, which
/// only `jj log` reads. The user's `revset-aliases` replace any of them.
/// `fixtures/oracle/jj-cli-aliases.json` holds jj's set.
pub const DEFAULT_ALIASES: &[(&str, &str)] = &[
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
    (
        "builtin_immutable_heads()",
        "trunk() | tags() | untracked_remote_bookmarks() | untracked_remote_tags()",
    ),
    ("immutable_heads()", "builtin_immutable_heads()"),
    ("immutable()", "::(immutable_heads() | root())"),
    ("mutable()", "~immutable()"),
    ("visible()", "::visible_heads()"),
    ("hidden()", "~visible()"),
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
    /// The rewrite left its path resolved in the commit; without a path, the whole commit.
    pub resolved: bool,
    /// The number of sides the path's conflict has after the rewrite, simplified: 1 when it
    /// is resolved. None for a rewrite of no one path.
    pub sides: Option<usize>,
    /// Each other commit the rewrite rewrote or rebased, parents first: the commits
    /// `absorb` moves hunks into, and the descendants.
    pub rewritten: Vec<CommitEffect>,
    /// The rewrite is written: false on a dry run.
    pub written: bool,
    /// The target's commit once the rewrite is done: the commit a later write into the
    /// change holds its conflict against ([`JjRepository::write_conflict_since`]). The
    /// target itself when the rewrite changed nothing. None on a dry run, and when the
    /// rewrite abandoned the target.
    pub latest: Option<vcs::CommitId>,
    /// The operation is written but the working copy could not be checked out to it:
    /// `jj workspace update-stale` updates it.
    pub stale: bool,
}

/// One rewritten commit's conflicted paths before and after a rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitEffect {
    /// The commit, as it was.
    pub change: ChangeInfo,
    pub before: Vec<Utf8PathBuf>,
    pub after: Vec<Utf8PathBuf>,
}

/// What a rewrite's closure changed: the commit the rewrite is about, as it was; the
/// commits it rewrote, as they were, none when nothing changed; and the operation's
/// description.
pub(crate) struct Mutated {
    pub target: Commit,
    /// The one path the rewrite is about, whose state the report gives; none for a rewrite
    /// of the whole commit.
    pub path: Option<RepoPathBuf>,
    pub roots: Vec<Commit>,
    pub description: String,
}

impl JjRepository {
    /// The user's revset aliases over jj's defaults for `trunk()`, `immutable_heads()`,
    /// `immutable()` and `mutable()`, and the user's email for `mine()`.
    pub(crate) fn revsets(&self) -> Result<Revsets, VcsError> {
        let env = self.config.clone().unwrap_or_else(config::ConfigEnv::from_process);
        let loaded =
            config::load(&env, self.root.as_std_path()).map_err(backend("reading jj's config"))?;
        let config = loaded.settings.config();
        let mut aliases = RevsetAliasesMap::new();
        for &(decl, defn) in DEFAULT_ALIASES {
            aliases.insert(decl, defn, None).map_err(backend("declaring a revset alias"))?;
        }
        for decl in config.table_keys(ALIASES_TABLE) {
            let defn: String =
                config.get([ALIASES_TABLE, decl]).map_err(backend("reading a revset alias"))?;
            aliases.insert(decl, defn, None).map_err(backend("declaring a revset alias"))?;
        }
        Ok(Revsets { aliases, user_email: loaded.settings.user_email().to_owned() })
    }

    /// The commits `revset` names that hold conflicts, parents first, with the user's
    /// aliases, after a snapshot of the working copy, as a jj command takes. Default:
    /// [`DEFAULT_STACK`].
    ///
    /// # Errors
    ///
    /// [`VcsError::UnknownRevision`] and [`VcsError::AmbiguousRevision`] as for
    /// [`vcs::Repository::commits`]; the errors of a snapshot; and [`VcsError::Backend`] when
    /// the config or the history cannot be read.
    pub fn stack(&mut self, revset: Option<&str>) -> Result<Vec<StackCommit>, VcsError> {
        let revsets = self.revsets()?;
        vcs::Repository::snapshot(self)?;
        let mut commits =
            self.evaluate(self.repo.as_ref(), revset.unwrap_or(DEFAULT_STACK), &revsets)?;
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
    /// [`VcsError::NotConflicted`] for a path resolved there, and [`VcsError::Backend`]
    /// holding [`JjError::ConflictChanged`] when the change's visible commit holds another
    /// conflict at the path than commit `id`, all before any write;
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
        self.write_conflict_since(id, id, path, content, marker_length, dry_run)
    }

    /// Write `content` as `path` into the change of commit `read`, as
    /// [`JjRepository::write_conflict`] does, after an earlier write left `held`: the
    /// markers parse against the conflict in `read`, and the change's visible commit must
    /// hold at `path` what `held` holds there. `held` is the [`Rewrite::latest`] of the
    /// earlier write, or `read` before any.
    ///
    /// # Errors
    ///
    /// As for [`JjRepository::write_conflict`], with [`JjError::ConflictChanged`] when the
    /// change's visible commit holds another value at `path` than `held`.
    pub fn write_conflict_since(
        &mut self,
        read: &vcs::CommitId,
        held: &vcs::CommitId,
        path: &Utf8Path,
        content: &[u8],
        marker_length: usize,
        dry_run: bool,
    ) -> Result<Rewrite, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let named = self.commit_by_id(read)?;
        let held = self.commit_by_id(held)?;
        let revsets = self.revsets()?;
        self.check_mutable(std::slice::from_ref(&held), &revsets)?;
        self.rewrite(&revsets, dry_run, |this, repo| {
            let commit = current(repo, &held)?;
            let value = this.held_conflict(&named, &held, &commit, &repo_path, path)?;
            let new_value = parse_back(repo, &repo_path, &value, content, marker_length)?;
            let mut builder =
                MergedTreeBuilder::new(widened(commit.tree(), &named.tree(), &new_value));
            builder
                .set_or_remove_merge(repo_path.clone(), new_value)
                .map_err(backend("writing the resolved path"))?;
            let tree = builder.write_tree().block_on().map_err(backend("writing a tree"))?;
            let change = this.change_info(&commit)?;
            let description =
                format!("resolve {path} in {} in triad", change.id(vcs::IdKind::Change));
            // Markers that read back as the conflict jj holds leave the commit as it is.
            if tree.tree_ids_and_labels() == commit.tree().tree_ids_and_labels() {
                return Ok(Mutated {
                    target: commit,
                    path: Some(repo_path),
                    roots: Vec::new(),
                    description,
                });
            }
            repo.rewrite_commit(&commit)
                .set_tree(tree)
                .write()
                .block_on()
                .map_err(backend("rewriting the commit"))?;
            Ok(Mutated {
                roots: vec![commit.clone()],
                target: commit,
                path: Some(repo_path),
                description,
            })
        })
    }

    /// The conflict at `path` in `read`, when `current`, the visible commit of the change,
    /// holds there what `held` holds: the same value as jj simplifies both, with the
    /// executable bit a resolved file would take. `held` is `read` before any write by
    /// triad, else the commit triad's last write left.
    ///
    /// # Errors
    ///
    /// [`VcsError::NotConflicted`] when `read` holds no conflict at `path`, and
    /// [`VcsError::Backend`] holding [`JjError::ConflictChanged`] when `current` holds
    /// another value there than `held`.
    pub(crate) fn held_conflict(
        &self,
        read: &Commit,
        held: &Commit,
        current: &Commit,
        repo_path: &RepoPath,
        path: &Utf8Path,
    ) -> Result<MergedTreeValue, VcsError> {
        let value_in = |commit: &Commit| {
            commit
                .tree()
                .path_value(repo_path)
                .block_on()
                .map_err(backend("finding a path in a tree"))
        };
        let was = value_in(read)?;
        if was.is_resolved() {
            return Err(VcsError::NotConflicted { path: path.to_owned(), rev: read.id().hex() });
        }
        let expected = value_in(held)?;
        let now = value_in(current)?;
        // The simplified terms, and the bit a resolved file takes from every term: a term
        // that cancels by content still counts toward it.
        let key = |value: &MergedTreeValue, commit: &Commit| {
            let bit = value.to_executable_merge().map(|bits| resolve_file_executable(&bits));
            (simplified(value, commit.tree().labels()).0, bit)
        };
        if key(&expected, held) != key(&now, current) {
            let change = self.change_info(current)?;
            return Err(backend("writing into the commit")(JjError::ConflictChanged {
                path: path.to_owned(),
                change: change.id(vcs::IdKind::Change).to_owned(),
            }));
        }
        // Before triad writes, the change's own terms, as `jj resolve -r` reads them. After,
        // the terms triad read, which its markers follow.
        Ok(if read.id() == held.id() { now } else { was })
    }

    /// Refuse a write to any of `commits` that jj's `immutable()` holds, with the user's
    /// `revsets`.
    pub(crate) fn check_mutable(
        &self,
        commits: &[Commit],
        revsets: &Revsets,
    ) -> Result<(), VcsError> {
        if commits.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = commits.iter().map(|commit| commit.id().hex()).collect();
        let expr = format!("({}) & immutable()", ids.join(" | "));
        let immutable = self.evaluate(self.repo.as_ref(), &expr, revsets)?;
        match immutable.first() {
            Some(commit) => Err(VcsError::Immutable(
                self.change_info(commit)?.id(vcs::IdKind::Commit).to_owned(),
            )),
            None => Ok(()),
        }
    }

    /// `roots` and their descendants in `repo`, except `target`, parents first, each with
    /// its conflicted paths.
    fn affected(
        &self,
        repo: &dyn jj_lib::repo::Repo,
        roots: &[Commit],
        target: &Commit,
    ) -> Result<Vec<(Commit, Vec<Utf8PathBuf>)>, VcsError> {
        if roots.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = roots.iter().map(|commit| commit.id().hex()).collect();
        let set = ids.join(" | ");
        let expr = format!("descendants({set}) ~ {}", target.id().hex());
        let mut found = self.evaluate(repo, &expr, &Revsets::default())?;
        found.reverse();
        found
            .into_iter()
            .map(|commit| conflicted_paths(&commit).map(|paths| (commit, paths)))
            .collect()
    }

    /// Snapshot the working copy, rewrite through `mutate` in one transaction, rebase the
    /// descendants, and unless `dry_run` commit the operation and check out a moved working
    /// copy. `mutate` sees the repository after the snapshot and names the commits it
    /// rewrote, which must be mutable under `revsets`.
    pub(crate) fn rewrite(
        &mut self,
        revsets: &Revsets,
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
        let Mutated { target, path, roots, description } = mutate(self, tx.repo_mut())?;
        self.check_mutable(&roots, revsets)?;
        let before = self.affected(repo.as_ref(), &roots, &target)?;
        tx.repo_mut()
            .rebase_descendants()
            .block_on()
            .map_err(backend("rebasing the descendants"))?;
        let written = !dry_run && tx.repo().has_changes();
        let new_wc = if written {
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
            new_wc
        } else {
            None
        };
        // The report follows each commit through the predecessors the operation records, so
        // each copy of a divergent change maps to its own rewrite. A dry run writes the
        // operation without publishing it, as it writes the commits.
        let after = if tx.repo().has_changes() {
            let unpublished =
                tx.write(description).block_on().map_err(backend("writing the operation"))?;
            if written {
                unpublished.publish().block_on().map_err(backend("writing the operation"))?
            } else {
                unpublished.leave_unpublished()
            }
        } else {
            std::sync::Arc::clone(&repo)
        };
        let now = successor(&after, &target)?;
        let latest =
            now.as_ref().filter(|_| !dry_run).map(|now| vcs::CommitId::new(now.id().hex()));
        let (resolved, sides) = match now {
            Some(now) => {
                let sides = path
                    .map(|path| {
                        now.tree()
                            .path_value(&path)
                            .block_on()
                            .map(|value| value.simplify().num_sides())
                            .map_err(backend("finding a path in a tree"))
                    })
                    .transpose()?;
                (sides.map_or(!now.has_conflict(), |sides| sides == 1), sides)
            }
            // `absorb` abandons a working-copy commit it empties; an abandoned commit holds
            // no conflict.
            None => (true, None),
        };
        let target = self.change_info(&target)?;
        let rewritten = before
            .into_iter()
            .map(|(commit, before)| {
                let after = successor(&after, &commit)?
                    .map(|now| conflicted_paths(&now))
                    .transpose()?
                    .unwrap_or_default();
                Ok(CommitEffect { change: self.change_info(&commit)?, before, after })
            })
            .collect::<Result<Vec<_>, VcsError>>()?;
        let report = |stale| Rewrite { target, resolved, sides, rewritten, written, latest, stale };
        if !written {
            return Ok(report(false));
        }
        // The operation is published: a failed checkout leaves the rewrite in place, and
        // the working copy stale until `jj workspace update-stale`.
        let stale = new_wc.as_ref().is_some_and(|new_wc| {
            workspace
                .check_out(after.op_id().clone(), Some(&old_wc.tree()), new_wc)
                .block_on()
                .inspect_err(|error| {
                    tracing::warn!(%error, "the rewrite is written; the working copy is stale");
                })
                .is_err()
        });
        self.repo = after;
        Ok(report(stale))
    }
}

/// The visible commits of `change_id` in `repo`.
fn visible_ids(repo: &dyn Repo, change_id: &ChangeId) -> Result<Vec<CommitId>, VcsError> {
    let targets =
        repo.resolve_change_id(change_id).block_on().map_err(backend("looking up a change"))?;
    Ok(targets.map_or_default(|targets| {
        targets.visible_with_offsets().map(|(_, id)| id.clone()).collect()
    }))
}

/// The commit `named` is in `repo`: itself while visible, else the one visible commit of its
/// change, as after a rewrite elsewhere.
///
/// # Errors
///
/// [`VcsError::UnknownRevision`] for an abandoned change, and
/// [`VcsError::AmbiguousRevision`] when `named` is hidden and its change is divergent.
pub(crate) fn current(repo: &dyn Repo, named: &Commit) -> Result<Commit, VcsError> {
    let ids = visible_ids(repo, named.change_id())?;
    let read = |id| repo.store().get_commit(id).map_err(backend("reading a commit"));
    // Read from `repo`'s store: a rewrite builds only on commits of its own store.
    if ids.contains(named.id()) {
        return read(named.id());
    }
    let change = || named.change_id().reverse_hex();
    match ids.as_slice() {
        [id] => read(id),
        [] => Err(VcsError::UnknownRevision(change())),
        _ => Err(VcsError::AmbiguousRevision(change())),
    }
}

/// `tree`, with as many sides as `value` when `tree` is resolved and `value` a conflict:
/// each side and base the same tree, labelled as `read`, the tree the conflict came from.
/// A save that brings back the conflict an earlier save resolved writes into it.
fn widened(tree: MergedTree, read: &MergedTree, value: &MergedTreeValue) -> MergedTree {
    match tree.tree_ids().as_resolved() {
        Some(id) if !value.is_resolved() => MergedTree::new(
            std::sync::Arc::clone(tree.store()),
            Merge::repeated(id.clone(), value.num_sides()),
            read.labels().clone(),
        ),
        _ => tree,
    }
}

/// What `old` became in the operation `repo` is at: itself while visible, else the visible
/// commit of its change that the operation rewrote it into, through the predecessors the
/// operation records; none when the operation abandoned it.
fn successor(repo: &ReadonlyRepo, old: &Commit) -> Result<Option<Commit>, VcsError> {
    let ids = visible_ids(repo, old.change_id())?;
    if ids.contains(old.id()) {
        return Ok(Some(old.clone()));
    }
    let operation = repo.operation();
    let rewritten_from_old = |id: &CommitId| {
        let mut pending = vec![id.clone()];
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if id == *old.id() {
                return true;
            }
            if seen.insert(id.clone())
                && let Some(predecessors) = operation.predecessors_for_commit(&id)
            {
                pending.extend(predecessors.iter().cloned());
            }
        }
        false
    };
    let found: Vec<&CommitId> = ids.iter().filter(|id| rewritten_from_old(id)).collect();
    match found.as_slice() {
        [] => Ok(None),
        [id] => repo.store().get_commit(id).map(Some).map_err(backend("reading a commit")),
        _ => Err(VcsError::AmbiguousRevision(old.change_id().reverse_hex())),
    }
}

/// `content` parsed back into `value`'s conflict, as jj does after a merge tool: markers jj
/// reads with the conflict's sides, simplified, keep their hunks conflicted, and content
/// without them resolves the path. Each term keeps its executable bit; a resolved file takes
/// the bit the terms agree on.
fn parse_back(
    repo: &MutableRepo,
    path: &jj_lib::repo_path::RepoPath,
    value: &MergedTreeValue,
    content: &[u8],
    marker_length: usize,
) -> Result<MergedTreeValue, VcsError> {
    let not_a_file = || {
        backend("parsing the conflict back")(JjError::NotAFileConflict(
            path.as_internal_file_string().into(),
        ))
    };
    let file_ids = value.to_file_merge().ok_or_else(not_a_file)?;
    let executable = value.to_executable_merge().ok_or_else(not_a_file)?;
    let new_ids = update_from_content(&file_ids, repo.store(), path, content, marker_length)
        .block_on()
        .map_err(backend("parsing the conflict back"))?;
    match new_ids.into_resolved() {
        Ok(None) => Ok(Merge::absent()),
        Ok(Some(id)) => Ok(Merge::normal(TreeValue::File {
            id,
            executable: resolve_file_executable(&executable).unwrap_or_default(),
            copy_id: CopyId::placeholder(),
        })),
        // jj expands a parsed conflict back to the terms it was given.
        Err(ids) if ids.num_sides() == value.num_sides() => Ok(value.with_new_file_ids(&ids)),
        Err(ids) => Err(backend("parsing the conflict back")(JjError::SidesChanged {
            path: Utf8PathBuf::from(path.as_internal_file_string()),
            before: value.num_sides(),
            after: ids.num_sides(),
        })),
    }
}

/// `value` as jj shows and parses it, with the labels of the terms that stay: simplified by
/// file contents, as `jj resolve` and `update_from_content` simplify a conflict of files. A
/// tree keeps every path at its own number of sides; a path that conflicts in fewer has
/// pairs of terms that cancel.
pub(crate) fn simplified(
    value: &MergedTreeValue,
    labels: &ConflictLabels,
) -> (MergedTreeValue, ConflictLabels) {
    let kept = kept_terms(value);
    let terms = value.as_slice();
    let names = labels.as_slice();
    let labels = if names.is_empty() {
        ConflictLabels::unlabeled()
    } else {
        ConflictLabels::from_vec(kept.iter().map(|&i| names[i].clone()).collect())
    };
    (kept.map(|&i| terms[i].clone()), labels)
}

/// The positions in `value` of the terms jj keeps when it simplifies the conflict by file
/// contents, in jj's order: side, base, side, … A tree's labels index the same positions.
fn kept_terms(value: &MergedTreeValue) -> Merge<usize> {
    let key = |term: &Option<TreeValue>| {
        term.as_ref().map(|term| match term {
            TreeValue::File { id, .. } => Ok(id.clone()),
            other => Err(other.clone()),
        })
    };
    let keys: Vec<_> = value.iter().map(key).collect();
    Merge::from_vec((0..keys.len()).collect::<Vec<usize>>()).simplify_by(|&i| &keys[i])
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
        for &(decl, defn) in DEFAULT_ALIASES {
            assert!(aliases.insert(decl, defn, None).is_ok(), "{decl} declares");
        }
        assert!(aliases.function_names().any(|name| name == "mutable"));
        assert!(DEFAULT_STACK.contains("mutable()"));
    }
}
