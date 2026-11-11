//! Repositories built in temporary directories through jj-lib, for this crate's tests and other
//! crates' (`testing` feature). They panic on failure, as test code may.
#![expect(
    clippy::missing_panics_doc,
    reason = "test support: each panic is a failed test setup, named by its message"
)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::hash::BuildHasher;
use std::process::Command;
use std::sync::Arc;

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use jj_lib::backend::{CommitId, CopyId, TreeValue};
use jj_lib::commit::Commit;
use jj_lib::config::{ConfigLayer, ConfigSource, StackedConfig};
use jj_lib::conflicts::{
    self, ConflictMarkerStyle, ConflictMaterializeOptions, MIN_CONFLICT_MARKER_LEN,
};
use jj_lib::default_backend_factories::{
    default_backend_factories, default_working_copy_factories,
};
use jj_lib::git;
use jj_lib::local_working_copy::LocalWorkingCopy;
use jj_lib::merged_tree_builder::MergedTreeBuilder;
use jj_lib::object_id::ObjectId as _;
use jj_lib::op_store::RefTarget;
use jj_lib::ref_name::{RefName, WorkspaceName};
use jj_lib::repo::{ReadonlyRepo, Repo};
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::settings::UserSettings;
use jj_lib::workspace::{DefaultWorkspaceLoaderFactory, Workspace, WorkspaceLoaderFactory as _};
use pollster::FutureExt as _;
use tempfile::TempDir;
use vcs::testing::{Entry, SUBMODULE_COMMIT};

use crate::JjRepository;

/// Fixed settings, so change IDs and timestamps repeat from run to run.
const SETTINGS: &str = r#"
user.name = "triad"
user.email = "triad@example.com"
debug.randomness-seed = 42
debug.commit-timestamp = "2001-02-03T04:05:06+00:00"
debug.operation-timestamp = "2001-02-03T04:05:06+00:00"
operation.hostname = "triad"
operation.username = "triad"
"#;

/// A commit the fixture made, with full-length IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The commit ID in hex.
    pub commit: String,
    /// The change ID in jj's reverse hex (`k`–`z`).
    pub change_id: String,
    id: CommitId,
}

impl Committed {
    fn of(commit: &Commit) -> Self {
        Self {
            commit: commit.id().hex(),
            change_id: commit.change_id().reverse_hex(),
            id: commit.id().clone(),
        }
    }
}

/// What the working-copy commit holds at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    Absent,
    File(Vec<u8>),
    Conflict,
    /// A symlink, submodule or directory.
    Other,
}

/// One operation: its description and whether jj marks it as a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub description: String,
    pub is_snapshot: bool,
}

/// How the fixture's git store sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    /// Inside `.jj/repo/store/git`; git sees nothing.
    Internal,
    /// A `.git` beside `.jj`, sharing the working copy.
    Colocated,
}

/// The store `jj git init` makes without flags, from jj's `git.colocate` setting. `Fixture::new`
/// and `Fixture::new_in` follow it. When jj's default changes, change this line; a test that
/// needs one layout names it with `Fixture::colocated` or `Fixture::internal`.
pub const DEFAULT_STORE: Store = Store::Colocated;

/// A jj workspace in a temporary directory, with commits made through jj-lib.
pub struct Fixture {
    dir: TempDir,
    workspace: Workspace,
    store: Store,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

fn settings() -> UserSettings {
    let mut config = StackedConfig::with_defaults();
    config.add_layer(ConfigLayer::parse(ConfigSource::User, SETTINGS).expect("the settings parse"));
    UserSettings::from_config_and_home_dir(config, None).expect("the settings load")
}

impl Fixture {
    /// A workspace laid out as `jj git init` lays it out by default (`DEFAULT_STORE`).
    #[must_use]
    pub fn new() -> Self {
        Self::in_temp_dir(DEFAULT_STORE)
    }

    /// A workspace laid out by default, in a new directory inside `parent`, such as another
    /// repository.
    #[must_use]
    pub fn new_in(parent: &Utf8Path) -> Self {
        let dir = tempfile::tempdir_in(parent).expect("a directory inside the parent");
        Self::init(dir, DEFAULT_STORE)
    }

    /// A workspace colocated with a git repository, as `jj git init --colocate` makes. git
    /// sees the same commits.
    #[must_use]
    pub fn colocated() -> Self {
        Self::in_temp_dir(Store::Colocated)
    }

    /// A workspace whose git store is internal to `.jj`, as `jj git init --no-colocate`
    /// makes. git sees nothing.
    #[must_use]
    pub fn internal() -> Self {
        Self::in_temp_dir(Store::Internal)
    }

    /// How this workspace's git store sits.
    #[must_use]
    pub fn store(&self) -> Store {
        self.store
    }

    fn in_temp_dir(store: Store) -> Self {
        Self::init(tempfile::tempdir().expect("a temporary directory"), store)
    }

    fn init(dir: TempDir, store: Store) -> Self {
        let settings = settings();
        let hash = gix::hash::Kind::Sha1;
        let (workspace, _) = match store {
            Store::Internal => {
                Workspace::init_internal_git(&settings, dir.path(), WorkspaceName::DEFAULT, hash)
                    .block_on()
            }
            Store::Colocated => {
                Workspace::init_colocated_git(&settings, dir.path(), WorkspaceName::DEFAULT, hash)
                    .block_on()
            }
        }
        .expect("jj init");
        Self { dir, workspace, store }
    }

    #[must_use]
    pub fn root(&self) -> &Utf8Path {
        Utf8Path::from_path(self.dir.path()).expect("a UTF-8 temporary path")
    }

    /// Write `bytes` to `path` in the working copy, making its directories.
    #[expect(
        clippy::must_use_candidate,
        reason = "called for its effect on the repository; tests often ignore the result"
    )]
    pub fn write(&self, path: &str, bytes: &[u8]) -> Utf8PathBuf {
        let path = self.root().join(path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("the directories are made");
        }
        std::fs::write(&path, bytes).expect("the file is written");
        path
    }

    fn repo(&self) -> Arc<ReadonlyRepo> {
        self.workspace.load_at_head().block_on().expect("the repository loads")
    }

    fn working_copy(&self, repo: &dyn Repo) -> Commit {
        let id = repo
            .view()
            .get_wc_commit_id(self.workspace.workspace_name())
            .expect("the workspace has a working-copy commit");
        repo.store().get_commit(id).expect("the working-copy commit")
    }

    /// Commit `files` as the whole tree on the working copy's parents, and start a new empty
    /// working-copy commit on it, as `jj commit` does.
    #[expect(
        clippy::must_use_candidate,
        reason = "called for its effect on the repository; tests often ignore the result"
    )]
    pub fn commit(&self, message: &str, files: &[(&str, &[u8])]) -> Committed {
        let repo = self.repo();
        let parents = self.working_copy(repo.as_ref()).parent_ids().to_vec();
        let committed = self.write_commit(&repo, parents, message, &Entry::files(files));
        self.check_out(&[&committed]);
        committed
    }

    /// Commit `files` as the whole tree on `parents`, or on the root commit without any. The
    /// working copy stays where it is.
    #[expect(
        clippy::must_use_candidate,
        reason = "called for its effect on the repository; tests often ignore the result"
    )]
    pub fn commit_on(
        &self,
        parents: &[&Committed],
        message: &str,
        files: &[(&str, &[u8])],
    ) -> Committed {
        self.commit_entries_on(parents, message, &Entry::files(files))
    }

    /// Commit `entries` as the whole tree on `parents`, as [`Fixture::commit_on`] does.
    #[expect(
        clippy::must_use_candidate,
        reason = "called for its effect on the repository; tests often ignore the result"
    )]
    pub fn commit_entries_on(
        &self,
        parents: &[&Committed],
        message: &str,
        entries: &[(&str, Entry)],
    ) -> Committed {
        let repo = self.repo();
        let parents = if parents.is_empty() {
            vec![repo.store().root_commit_id().clone()]
        } else {
            parents.iter().map(|parent| parent.id.clone()).collect()
        };
        self.write_commit(&repo, parents, message, entries)
    }

    fn write_commit(
        &self,
        repo: &Arc<ReadonlyRepo>,
        parents: Vec<CommitId>,
        message: &str,
        entries: &[(&str, Entry)],
    ) -> Committed {
        let store = repo.store();
        let mut builder = MergedTreeBuilder::new(store.empty_merged_tree());
        for &(path, entry) in entries {
            let path = RepoPathBuf::from_internal_string(path).expect("a repository path");
            let file = |mut bytes: &[u8], executable| TreeValue::File {
                id: store.write_file(&path, &mut bytes).block_on().expect("a file"),
                executable,
                copy_id: CopyId::placeholder(),
            };
            let value = match entry {
                Entry::File(bytes) => file(bytes, false),
                Entry::Executable(bytes) => file(bytes, true),
                Entry::Symlink(target) => TreeValue::Symlink(
                    store.write_symlink(&path, target).block_on().expect("a symlink"),
                ),
                Entry::Submodule => TreeValue::GitSubmodule(
                    CommitId::try_from_hex(SUBMODULE_COMMIT).expect("a commit ID in hex"),
                ),
            };
            builder.set_or_remove(path, Some(value));
        }
        let tree = builder.write_tree().block_on().expect("a tree");
        let mut tx = repo.start_transaction();
        let commit = tx
            .repo_mut()
            .new_commit(parents, tree)
            .set_description(message)
            .write()
            .block_on()
            .expect("a commit");
        self.finish(tx, "commit");
        Committed::of(&commit)
    }

    /// Start a new empty working-copy commit on `parents`; more than one makes a merge.
    pub fn check_out(&self, parents: &[&Committed]) {
        let repo = self.repo();
        let parent_ids = parents.iter().map(|parent| parent.id.clone()).collect();
        let parent_commits: Vec<Commit> = parents
            .iter()
            .map(|parent| repo.store().get_commit(&parent.id).expect("the parent"))
            .collect();
        let tree = jj_lib::rewrite::merge_commit_trees(repo.as_ref(), &parent_commits)
            .block_on()
            .expect("the parents' trees merge");
        let mut tx = repo.start_transaction();
        let wc = tx
            .repo_mut()
            .new_commit(parent_ids, tree)
            .write()
            .block_on()
            .expect("a working-copy commit");
        let name = self.workspace.workspace_name().to_owned();
        tx.repo_mut().edit(name, &wc).block_on().expect("the working copy moves");
        self.finish(tx, "check out");
    }

    /// Write the working-copy commit's tree to disk, as jj does after a command that moves
    /// `@`: files, conflict markers, and the working copy's record of each file.
    pub fn check_out_to_disk(&self) {
        let mut workspace = self.fresh_workspace();
        let repo = workspace.load_at_head().block_on().expect("the repository loads");
        let wc = self.working_copy(repo.as_ref());
        workspace
            .check_out(repo.op_id().clone(), None, &wc)
            .block_on()
            .expect("the working copy is checked out");
    }

    /// The workspace loaded afresh: its working copy's state as it is on disk now, which a
    /// checkout or a snapshot may have changed since the fixture's own workspace loaded.
    fn fresh_workspace(&self) -> Workspace {
        DefaultWorkspaceLoaderFactory
            .create(self.workspace.workspace_root())
            .expect("the workspace is found")
            .load(&settings(), &default_backend_factories(), &default_working_copy_factories())
            .expect("the workspace loads")
    }

    /// Point bookmark `name` at the working-copy commit, as `jj bookmark set NAME -r @` does.
    /// A colocated workspace exports it to git.
    pub fn set_bookmark_on_working_copy(&self, name: &str) {
        let repo = self.repo();
        let wc = self.working_copy(repo.as_ref()).id().clone();
        let mut tx = repo.start_transaction();
        tx.repo_mut().set_local_bookmark_target(RefName::new(name), RefTarget::normal(wc));
        self.finish(tx, "set bookmark");
    }

    /// The full ID of the working-copy commit at the head operation.
    #[must_use]
    pub fn working_copy_id(&self) -> String {
        self.working_copy(self.repo().as_ref()).id().hex()
    }

    /// The working-copy commit at the head operation, for `check_out` or `edit` to name.
    #[must_use]
    pub fn working_copy_commit(&self) -> Committed {
        let commit = self.working_copy(self.repo().as_ref());
        Committed::of(&commit)
    }

    /// The full ID git's branch `name` points at, in a colocated workspace.
    ///
    /// # Panics
    ///
    /// If git has no such branch.
    #[must_use]
    pub fn git_branch(&self, name: &str) -> String {
        let repo = gix::open(self.root()).expect("git's repository opens");
        let reference =
            repo.find_reference(format!("refs/heads/{name}").as_str()).expect("the branch exists");
        reference.id().to_string()
    }

    /// Make `commit` the working-copy commit, as `jj edit` does.
    pub fn edit(&self, commit: &Committed) {
        let repo = self.repo();
        let commit = repo.store().get_commit(&commit.id).expect("the commit");
        let mut tx = repo.start_transaction();
        let name = self.workspace.workspace_name().to_owned();
        tx.repo_mut().edit(name, &commit).block_on().expect("the working copy moves");
        self.finish(tx, "edit");
    }

    fn finish(&self, mut tx: jj_lib::transaction::Transaction, description: &str) {
        tx.repo_mut().rebase_descendants().block_on().expect("descendants rebase");
        if self.store == Store::Colocated {
            git::export_refs(tx.repo_mut()).expect("refs export to git");
            let wc = self.working_copy(tx.repo_mut());
            git::reset_head(
                tx.repo_mut(),
                self.workspace.workspace_name(),
                self.workspace.workspace_root(),
                (None, &wc),
            )
            .block_on()
            .expect("git's HEAD follows @-");
        }
        tx.commit(description).block_on().expect("the operation");
    }

    /// Commit two operations on the same parent operation, as two jj processes running at
    /// once do, leaving two operation heads.
    pub fn diverge_operations(&self) {
        let repo = self.repo();
        for message in ["concurrent one", "concurrent two"] {
            let mut tx = repo.start_transaction();
            let root = repo.store().root_commit_id().clone();
            tx.repo_mut()
                .new_commit(vec![root], repo.store().empty_merged_tree())
                .set_description(message)
                .write()
                .block_on()
                .expect("a commit");
            tx.commit(message).block_on().expect("the operation");
        }
    }

    /// Remove this workspace from the repository's view, as `jj workspace forget` run from
    /// another workspace does. The workspace's directory stays.
    pub fn forget_workspace(&self) {
        let repo = self.repo();
        let mut tx = repo.start_transaction();
        tx.repo_mut()
            .remove_workspace(self.workspace.workspace_name())
            .block_on()
            .expect("the workspace is removed");
        tx.repo_mut()
            .rebase_descendants()
            .block_on()
            .expect("the abandoned commit's descendants are rebased");
        tx.commit("forget the workspace").block_on().expect("the operation");
    }

    /// The operations from the head back to the root, following first parents.
    #[must_use]
    pub fn operations(&self) -> Vec<Op> {
        let mut operations = Vec::new();
        let mut operation = Some(self.repo().operation().clone());
        while let Some(op) = operation {
            let metadata = op.metadata();
            operations.push(Op {
                description: metadata.description.clone(),
                is_snapshot: metadata.is_snapshot,
            });
            operation = op.parents().block_on().expect("the parents load").into_iter().next();
        }
        operations
    }

    /// The full IDs of the working-copy commit's parents, `back` operations before the head
    /// (0: at the head), in order.
    #[must_use]
    pub fn parent_ids(&self, back: usize) -> Vec<String> {
        let repo = self.repo_at(back);
        self.working_copy(repo.as_ref()).parent_ids().iter().map(CommitId::hex).collect()
    }

    /// The repository `back` operations before the head, following first parents.
    fn repo_at(&self, back: usize) -> Arc<ReadonlyRepo> {
        let head = self.repo();
        let operation = (0..back).fold(head.operation().clone(), |op, _| {
            op.parents()
                .block_on()
                .expect("the parents load")
                .into_iter()
                .next()
                .expect("an operation that far back")
        });
        head.loader().load_at(&operation).block_on().expect("the repository loads at the operation")
    }

    /// What the working-copy commit holds at `path` as of the operation `back` first parents
    /// before the head.
    #[must_use]
    pub fn held_at(&self, back: usize, path: &str) -> Held {
        let repo = self.repo_at(back);
        let commit = self.working_copy(repo.as_ref());
        held(&repo, &commit, path)
    }

    /// The visible commit of `commit`'s change at the head operation: `commit` itself, or the
    /// commit that rewrote it, as after a rebase.
    ///
    /// # Panics
    ///
    /// If the change has no visible commit, or is divergent.
    #[must_use]
    pub fn latest(&self, commit: &Committed) -> Committed {
        let repo = self.repo();
        let change_id = jj_lib::backend::ChangeId::try_from_reverse_hex(&commit.change_id)
            .expect("a change ID in reverse hex");
        let targets = repo
            .resolve_change_id(&change_id)
            .block_on()
            .expect("the index reads")
            .expect("the change has commits");
        let mut visible = targets.visible_with_offsets().map(|(_, id)| id);
        let id = visible.next().expect("the change has a visible commit");
        assert!(visible.next().is_none(), "change {} is divergent", commit.change_id);
        let commit = repo.store().get_commit(id).expect("the commit");
        Committed::of(&commit)
    }

    /// What `commit` holds at `path`, wherever the working copy is.
    #[must_use]
    pub fn held_in(&self, commit: &Committed, path: &str) -> Held {
        let repo = self.repo();
        let commit = repo.store().get_commit(&commit.id).expect("the commit");
        held(&repo, &commit, path)
    }

    /// The number of operation heads.
    #[must_use]
    pub fn op_heads(&self) -> usize {
        self.workspace
            .repo_loader()
            .op_heads_store()
            .get_op_heads()
            .block_on()
            .expect("the operation heads")
            .len()
    }

    #[must_use]
    pub fn open(&self) -> JjRepository {
        JjRepository::discover(self.root()).expect("the repository is found")
    }
}

/// What `commit` holds at `path` in `repo`.
fn held(repo: &ReadonlyRepo, commit: &Commit, path: &str) -> Held {
    let repo_path = RepoPathBuf::from_internal_string(path).expect("a repository path");
    let value = commit.tree().path_value(&repo_path).block_on().expect("the path's value");
    match value.into_resolved() {
        Err(_) => Held::Conflict,
        Ok(None) => Held::Absent,
        Ok(Some(TreeValue::File { id, .. })) => {
            let mut bytes = Vec::new();
            futures::AsyncReadExt::read_to_end(
                &mut repo.store().read_file(&repo_path, &id).block_on().expect("the file opens"),
                &mut bytes,
            )
            .block_on()
            .expect("the file reads");
            Held::File(bytes)
        }
        Ok(Some(_)) => Held::Other,
    }
}

/// A merge tool as jj's `[merge-tools.NAME]` table configures it (spec §7.1).
pub struct MergeTool {
    /// `program`, with any environment the caller needs; [`Fixture::resolve`] adds the
    /// arguments.
    pub command: Command,
    pub merge_args: Vec<String>,
    pub edits_conflict_markers: bool,
    pub conflict_exit_codes: Vec<i32>,
}

/// What `jj resolve` makes of a merge tool's run, by the rules of jj-cli 0.46
/// (`cli/src/merge_tools/external.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutcome {
    /// The file is resolved to these bytes.
    Resolved(Vec<u8>),
    /// The file stays conflicted with these sides and bases, in jj's order.
    Conflicted { sides: Vec<Vec<u8>>, bases: Vec<Vec<u8>> },
    /// The tool exited with a code that is neither 0 nor a conflict code.
    Aborted(Option<i32>),
    /// The output is empty or as jj wrote it, which jj refuses.
    EmptyOrUnchanged,
    /// The exit code promised conflict markers, but jj found none it could parse.
    InvalidConflictMarkers,
}

/// The arguments with each `$name` replaced by its variable, as jj does (`$` then
/// `[a-z0-9_]+`, up to a word boundary); unknown names stay as written.
#[must_use]
pub fn interpolate<S: BuildHasher>(
    args: &[String],
    variables: &HashMap<&str, OsString, S>,
) -> Vec<OsString> {
    let is_name = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
    args.iter()
        .map(|arg| {
            let mut out = OsString::new();
            let mut rest = arg.as_str();
            while let Some(dollar) = rest.find('$') {
                out.push(&rest[..dollar]);
                let after = &rest[dollar + 1..];
                let len = after.find(|c: char| !is_name(c)).unwrap_or(after.len());
                let boundary =
                    after[len..].chars().next().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                match variables.get(&after[..len]) {
                    Some(value) if len > 0 && boundary => out.push(value),
                    _ => out.push(&rest[dollar..=dollar + len]),
                }
                rest = &after[len..];
            }
            out.push(rest);
            out
        })
        .collect()
}

/// A conflicted file as `jj resolve` hands it to a tool.
struct Conflict {
    repo: Arc<ReadonlyRepo>,
    path: RepoPathBuf,
    file: conflicts::MaterializedFileConflictValue,
}

impl Fixture {
    /// Make `path` conflicted, as merging `left` and `right` over `base` does, then run
    /// `tool` on it the way `jj resolve` does: jj's input files and pre-filled output, its
    /// variables, its marker length, and its reading of the exit code and output.
    #[must_use]
    pub fn resolve(&self, path: &str, sides: [&[u8]; 3], mut tool: MergeTool) -> ToolOutcome {
        let conflict = self.conflict(path, sides);
        let store = conflict.repo.store();
        let uses_marker_length = tool.merge_args.iter().any(|arg| arg.contains("$marker_length"));
        let marker_len = if tool.edits_conflict_markers || uses_marker_length {
            conflicts::choose_materialized_conflict_marker_len(&conflict.file.contents)
        } else {
            MIN_CONFLICT_MARKER_LEN
        };
        let initial_output: Vec<u8> = if tool.edits_conflict_markers {
            let options = ConflictMaterializeOptions {
                marker_style: ConflictMarkerStyle::Git,
                marker_len: Some(marker_len),
                merge: store.merge_options().clone(),
            };
            conflicts::materialize_merge_result_to_bytes(
                &conflict.file.contents,
                &conflict.file.labels,
                &options,
            )
            .into()
        } else {
            Vec::new()
        };

        let dir = tempfile::Builder::new()
            .prefix("jj-resolve-")
            .tempdir()
            .expect("a temporary directory");
        let mut variables = tool_files(dir.path(), path, &conflict.file.contents, &initial_output);
        variables.insert("marker_length", marker_len.to_string().into());
        variables.insert("path", path.into());

        let status = tool
            .command
            .args(interpolate(&tool.merge_args, &variables))
            .status()
            .expect("the tool runs");
        let implies_conflict =
            status.code().is_some_and(|code| tool.conflict_exit_codes.contains(&code));
        if !status.success() && !implies_conflict {
            return ToolOutcome::Aborted(status.code());
        }
        let output = std::fs::read(&variables["output"]).expect("the output reads");
        if output.is_empty() || output == initial_output {
            return ToolOutcome::EmptyOrUnchanged;
        }
        let new_ids = if tool.edits_conflict_markers || implies_conflict {
            conflicts::update_from_content(
                &conflict.file.unsimplified_ids,
                store,
                &conflict.path,
                &output,
                marker_len,
            )
            .block_on()
            .expect("jj reparses the output")
        } else {
            let id = store
                .write_file(&conflict.path, &mut output.as_slice())
                .block_on()
                .expect("the output is stored");
            jj_lib::merge::Merge::normal(id)
        };
        if implies_conflict && new_ids.is_resolved() {
            return ToolOutcome::InvalidConflictMarkers;
        }
        outcome(&new_ids, store, &conflict.path)
    }

    /// The working-tree file at `path`, conflicted in the working-copy commit, read back as
    /// jj's next snapshot reads it: its markers parsed at the marker length the working copy
    /// recorded for it, or jj's shortest without a record.
    #[must_use]
    pub fn snapshot_reads(&self, path: &str) -> ToolOutcome {
        let repo = self.repo();
        let store = repo.store();
        let repo_path = RepoPathBuf::from_internal_string(path).expect("a repository path");
        let tree = self.working_copy(repo.as_ref()).tree();
        let value = tree.path_value(&repo_path).block_on().expect("the path's value");
        let file = conflicts::try_materialize_file_conflict_value(
            store,
            &repo_path,
            &value,
            tree.labels(),
        )
        .block_on()
        .expect("the conflict reads")
        .expect("a file conflict");
        let workspace = self.fresh_workspace();
        let local = workspace
            .working_copy()
            .downcast_ref::<LocalWorkingCopy>()
            .expect("a local working copy");
        let marker_len = local
            .file_states()
            .expect("the working copy's state reads")
            .get(&repo_path)
            .and_then(|state| state.materialized_conflict_data)
            .map_or(MIN_CONFLICT_MARKER_LEN, |data| {
                usize::try_from(data.conflict_marker_len).expect("a small length")
            });
        let content = std::fs::read(self.root().join(path)).expect("the file reads");
        let new_ids = conflicts::update_from_content(
            &file.unsimplified_ids,
            store,
            &repo_path,
            &content,
            marker_len,
        )
        .block_on()
        .expect("jj reads the file");
        outcome(&new_ids, store, &repo_path)
    }

    /// `path` conflicted by commits of `left` and `right` on one of `base`.
    fn conflict(&self, path: &str, [base, left, right]: [&[u8]; 3]) -> Conflict {
        let base = self.commit_on(&[], "base", &[(path, base)]);
        let left = self.commit_on(&[&base], "left", &[(path, left)]);
        let right = self.commit_on(&[&base], "right", &[(path, right)]);
        let repo = self.repo();
        let store = repo.store();
        let parents: Vec<Commit> = [&left, &right]
            .iter()
            .map(|c| store.get_commit(&c.id).expect("the side commit"))
            .collect();
        let tree = jj_lib::rewrite::merge_commit_trees(repo.as_ref(), &parents)
            .block_on()
            .expect("the trees merge");
        let repo_path = RepoPathBuf::from_internal_string(path).expect("a repository path");
        let value = tree.path_value(&repo_path).block_on().expect("the path's value");
        assert!(!value.is_resolved(), "{path} must conflict for jj to call the tool");
        let file = conflicts::try_materialize_file_conflict_value(
            store,
            &repo_path,
            &value,
            tree.labels(),
        )
        .block_on()
        .expect("the conflict reads")
        .expect("a file conflict");
        Conflict { repo, path: repo_path, file }
    }
}

/// The file jj holds for `ids`: resolved bytes, or a conflict's sides and bases.
fn outcome(
    ids: &jj_lib::merge::Merge<Option<jj_lib::backend::FileId>>,
    store: &Arc<jj_lib::store::Store>,
    path: &RepoPathBuf,
) -> ToolOutcome {
    let hunk = conflicts::extract_as_single_hunk(&ids.simplify(), store, path)
        .block_on()
        .expect("the result reads");
    match hunk.into_resolved() {
        Ok(bytes) => ToolOutcome::Resolved(bytes.into()),
        Err(merge) => ToolOutcome::Conflicted {
            sides: merge.adds().map(|b| b.to_vec()).collect(),
            bases: merge.removes().map(|b| b.to_vec()).collect(),
        },
    }
}

/// jj's tool files in `dir`: `base_NAME`, `left_NAME`, `right_NAME` and `output_NAME`, as
/// variables naming them.
fn tool_files(
    dir: &Path,
    path: &str,
    contents: &jj_lib::merge::Merge<impl AsRef<[u8]>>,
    output: &[u8],
) -> HashMap<&'static str, OsString> {
    let name = path.rsplit('/').next().expect("a file name");
    let files: [(&str, &[u8]); 4] = [
        ("base", contents.get_remove(0).expect("a base").as_ref()),
        ("left", contents.get_add(0).expect("a left side").as_ref()),
        ("right", contents.get_add(1).expect("a right side").as_ref()),
        ("output", output),
    ];
    files
        .into_iter()
        .map(|(role, bytes)| {
            let file = dir.join(format!("{role}_{name}"));
            std::fs::write(&file, bytes).expect("the tool's input is written");
            (role, file.into_os_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> HashMap<&'static str, OsString> {
        HashMap::from([("left", "/t/left_a".into()), ("marker_length", "9".into())])
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|&a| a.to_owned()).collect()
    }

    #[test]
    fn whole_and_embedded_variables_are_replaced() {
        assert_eq!(
            interpolate(&strings(&["$left", "--n=$marker_length", "x$left.y"]), &vars()),
            ["/t/left_a", "--n=9", "x/t/left_a.y"].map(OsString::from)
        );
    }

    #[test]
    fn unknown_names_upper_case_and_bare_dollars_stay() {
        assert_eq!(
            interpolate(&strings(&["$right", "$LEFT", "$", "a$", "$lefty"]), &vars()),
            ["$right", "$LEFT", "$", "a$", "$lefty"].map(OsString::from)
        );
    }
}
