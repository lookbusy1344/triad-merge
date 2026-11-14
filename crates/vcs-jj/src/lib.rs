//! `Repository` for jj, through jj-lib. It reads at the head operation, and writes only the
//! working-copy snapshots of `resolve` (spec §7.4): concurrent operations are refused, not
//! reconciled.

mod absorb;
pub mod config;
mod history;
mod native;
mod reduce;
#[cfg(feature = "testing")]
pub mod testing;
pub mod tool_config;

pub use absorb::{AbsorbHunk, Absorbed, DEFAULT_INTO, Destination, Goes, HunkChoice, Stay};
pub use history::{FileVersion, SHORT_OPERATION, VersionState};
pub use native::{CommitEffect, DEFAULT_ALIASES, Rewrite, StackCommit, StackPath};
pub use reduce::{MIN_PAIR_SIDES, check_pair};

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use futures::{AsyncReadExt as _, StreamExt as _, TryStreamExt as _};
use jj_lib::annotate::FileAnnotator;
use jj_lib::backend::{FileId, TreeValue};
use jj_lib::commit::Commit;
use jj_lib::config::StackedConfig;
use jj_lib::default_backend_factories::{
    default_backend_factories, default_working_copy_factories,
};
use jj_lib::fileset::FilesetAliasesMap;
use jj_lib::git::REMOTE_NAME_FOR_LOCAL_GIT_REPO;
use jj_lib::local_working_copy::LocalWorkingCopy;
use jj_lib::merged_tree::MergedTree;
use jj_lib::object_id::ObjectId as _;
use jj_lib::ref_name::WorkspaceNameBuf;
use jj_lib::repo::{ReadonlyRepo, Repo as _};
use jj_lib::repo_path::{RepoPath, RepoPathBuf};
use jj_lib::revset::{
    self, ResolvedRevsetExpression, RevsetAliasesMap, RevsetDiagnostics, RevsetExtensions,
    RevsetParseContext, RevsetResolutionError, RevsetStreamExt as _, RevsetWorkspaceContext,
    SymbolResolver, SymbolResolverExtension,
};
use jj_lib::settings::UserSettings;
use jj_lib::ui_path::RepoPathUiConverter;
use jj_lib::working_copy::{WorkingCopy as _, WorkingCopyFreshness};
use jj_lib::workspace::{DefaultWorkspaceLoaderFactory, Workspace, WorkspaceLoaderFactory as _};
use pollster::FutureExt as _;
use vcs::{
    Annotation, ChangeInfo, Conflict, ConflictStyle, ConflictTerm, EntryKind, LineOrigin, Parent,
    Recorded, RepoKind, Repository, RevSpec, Term, TermContent, Timestamp, VcsError,
};

/// The directory that marks a jj workspace's root.
pub(crate) const JJ_DIR: &str = ".jj";
/// The description jj gives the operation that snapshots the working copy.
const SNAPSHOT_DESCRIPTION: &str = "snapshot working copy";
/// The operation that imports a moved git HEAD, as jj names it.
const IMPORT_HEAD_DESCRIPTION: &str = "import git head";
/// The working copy's parents, in revset syntax.
const PARENT: &str = "@-";
/// jj shows commit and change IDs with at least this many characters, more when shorter
/// prefixes are ambiguous.
const MIN_SHORT_ID: usize = 8;
const JJ: &str = "jj";
const MILLIS_PER_SECOND: i64 = 1000;
const SECONDS_PER_MINUTE: i32 = 60;

/// Why jj-lib's view of a repository is unusable.
#[derive(Debug, thiserror::Error)]
pub enum JjError {
    #[error("the repository has {0} concurrent operations; any jj command reconciles them")]
    ConcurrentOperations(usize),
    /// `resolve --pair` on a conflict of two sides, where the pair is the whole conflict.
    #[error("{0} has two sides; resolve it without --pair")]
    PairOfTwoSides(camino::Utf8PathBuf),
    /// `resolve --pair N` past the conflict's last pair.
    #[error("pair {pair} needs sides {pair} and {next}; the conflict has {sides}", next = pair + 1)]
    PairOutOfRange { pair: usize, sides: usize },
    /// A conflict with a symlink, submodule or directory among its terms.
    #[error("{0} is not a conflict of files")]
    NotAFileConflict(camino::Utf8PathBuf),
    /// `diff --op` with an operation jj cannot resolve.
    #[error("no operation {op}: {reason}")]
    UnknownOperation { op: String, reason: String },
    /// `diff --op` at an operation before the workspace had a working copy.
    #[error("the workspace has no working copy at operation {0}")]
    NoWorkingCopyAt(String),
    /// `absorb --move` or `--keep` of a hunk that does not exist.
    #[error("no hunk {hunk}; there are {count}")]
    NoHunk { hunk: usize, count: usize },
    /// `absorb --move` to a commit outside the ones a hunk may go into.
    #[error("{0} is not a mutable ancestor of the working copy in --into")]
    NotADestination(String),
    /// `absorb --apply` at an operation the repository has moved past: the hunk numbers may
    /// name other hunks now.
    #[error("the repository changed since operation {0}; list the hunks again")]
    RepositoryMoved(String),
    /// A save into a commit whose conflict at the path differs from the one triad read: the
    /// commit was rewritten while the window was open.
    #[error("{path} in {change} changed since triad read it; nothing is written")]
    ConflictChanged { path: camino::Utf8PathBuf, change: String },
    /// jj parsed a saved conflict back with another number of sides than it holds.
    #[error("{path}: jj read back a conflict of {after} sides for one of {before}")]
    SidesChanged { path: camino::Utf8PathBuf, before: usize, after: usize },
}

/// What a revset resolves with: the aliases, and the email `mine()` matches.
#[derive(Default)]
pub(crate) struct Revsets {
    pub aliases: RevsetAliasesMap,
    pub user_email: String,
}

/// A jj workspace, read through jj-lib at its head operation.
pub struct JjRepository {
    repo: Arc<ReadonlyRepo>,
    workspace: WorkspaceNameBuf,
    root: Utf8PathBuf,
    /// Where the user's jj config lives, for the snapshots; the process's when unset.
    config: Option<config::ConfigEnv>,
}

fn backend<E: Error + Send + Sync + 'static>(context: &'static str) -> impl FnOnce(E) -> VcsError {
    move |error| VcsError::Backend { context, source: Box::new(error) }
}

/// `time` in milliseconds since the epoch, as jj records a file's modification time.
fn millis(time: std::time::SystemTime) -> Option<i64> {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).ok(),
        Err(before) => i64::try_from(before.duration().as_millis()).ok().map(|m| -m),
    }
}

/// The repository at the workspace's single head operation.
fn head_repo(workspace: &Workspace) -> Result<Arc<ReadonlyRepo>, VcsError> {
    let loader = workspace.repo_loader();
    let heads = loader
        .op_heads_store()
        .get_op_heads()
        .block_on()
        .map_err(backend("reading the operation heads"))?;
    let [head] = heads.as_slice() else {
        return Err(backend("loading the head operation")(JjError::ConcurrentOperations(
            heads.len(),
        )));
    };
    let operation =
        loader.load_operation(head).block_on().map_err(backend("loading the head operation"))?;
    loader.load_at(&operation).block_on().map_err(backend("loading the repository"))
}

/// The workspace at `root`, with jj's default settings and factories.
fn load_workspace(root: &std::path::Path) -> Result<Workspace, VcsError> {
    let settings = UserSettings::from_config_and_home_dir(StackedConfig::with_defaults(), None)
        .map_err(backend("reading jj's default settings"))?;
    DefaultWorkspaceLoaderFactory
        .create(root)
        .map_err(backend("finding the jj workspace"))?
        .load(&settings, &default_backend_factories(), &default_working_copy_factories())
        .map_err(backend("loading the jj workspace"))
}

impl JjRepository {
    /// The workspace at the root, loaded afresh for its working copy's state.
    fn workspace(&self) -> Result<Workspace, VcsError> {
        load_workspace(self.root.as_std_path())
    }

    /// The workspace holding `start`, searching up from it. The repository is read at its
    /// single head operation.
    ///
    /// # Errors
    ///
    /// [`VcsError::NotARepository`] when no ancestor is a jj workspace,
    /// [`VcsError::NotUtf8`] when the workspace root is not UTF-8, [`VcsError::NoWorkingCopy`]
    /// when the workspace has no working-copy commit, and [`VcsError::Backend`] when jj-lib
    /// cannot load it, holding [`JjError::ConcurrentOperations`] when there is more than one
    /// head operation.
    pub fn discover(start: &Utf8Path) -> Result<Self, VcsError> {
        let found = start
            .ancestors()
            .find(|dir| dir.join(JJ_DIR).is_dir())
            .ok_or_else(|| VcsError::NotARepository(start.to_owned()))?;
        let workspace = load_workspace(found.as_std_path())?;
        let root = Utf8PathBuf::from_path_buf(workspace.workspace_root().to_owned())
            .map_err(VcsError::NotUtf8)?;
        let repo = head_repo(&workspace)?;
        let workspace = workspace.workspace_name().to_owned();
        if repo.view().get_wc_commit_id(&workspace).is_none() {
            return Err(VcsError::NoWorkingCopy(root));
        }
        Ok(Self { repo, workspace, root, config: None })
    }

    /// The repository, taking the user's jj config for its snapshots from `config`.
    #[must_use]
    pub fn with_config(self, config: config::ConfigEnv) -> Self {
        Self { config: Some(config), ..self }
    }

    /// Snapshot the working copy as jj does before a command, with the user's jj config.
    /// When the snapshot changed the working-copy commit's tree, it is written in an
    /// operation described by `description`, marked as a snapshot when `as_snapshot`. The
    /// repository reads at the resulting operation afterwards.
    fn take_snapshot(&mut self, description: &str, as_snapshot: bool) -> Result<(), VcsError> {
        let env = self.config.clone().unwrap_or_else(config::ConfigEnv::from_process);
        let config::JjConfig { mut workspace, snapshot, .. } =
            config::load(&env, self.root.as_std_path()).map_err(backend("reading jj's config"))?;
        let name = workspace.workspace_name().to_owned();
        let mut repo = head_repo(&workspace)?;
        let git = colocated_git(&repo, workspace.workspace_root());
        if git.is_some() {
            repo = import_git_head(&mut workspace, repo, &name)?;
        }
        let wc_id = |repo: &ReadonlyRepo| {
            repo.view()
                .get_wc_commit_id(&name)
                .cloned()
                .ok_or_else(|| VcsError::NoWorkingCopy(self.root.clone()))
        };
        let mut wc_commit = repo
            .store()
            .get_commit(&wc_id(&repo)?)
            .map_err(backend("reading the working-copy commit"))?;
        let mut locked = workspace
            .start_working_copy_mutation()
            .block_on()
            .map_err(backend("locking the working copy"))?;
        match WorkingCopyFreshness::check_stale(locked.locked_wc(), &wc_commit, &repo)
            .block_on()
            .map_err(backend("reading the working copy's operation"))?
        {
            WorkingCopyFreshness::Fresh => {}
            WorkingCopyFreshness::Updated(operation) => {
                repo = repo
                    .loader()
                    .load_at(&operation)
                    .block_on()
                    .map_err(backend("loading the working copy's operation"))?;
                wc_commit = repo
                    .store()
                    .get_commit(&wc_id(&repo)?)
                    .map_err(backend("reading the working-copy commit"))?;
            }
            WorkingCopyFreshness::WorkingCopyStale | WorkingCopyFreshness::SiblingOperation => {
                return Err(VcsError::StaleWorkspace(self.root.clone()));
            }
        }
        let old_tree = wc_commit.tree();
        let (new_tree, _) = locked
            .locked_wc()
            .snapshot(&snapshot.options())
            .block_on()
            .map_err(backend("snapshotting the working copy"))?;
        if new_tree.tree_ids_and_labels() != old_tree.tree_ids_and_labels() {
            let mut tx = repo.start_transaction();
            tx.set_is_snapshot(as_snapshot);
            tx.set_workspace_name(&name);
            let commit = tx
                .repo_mut()
                .rewrite_commit(&wc_commit)
                .set_tree(new_tree.clone())
                .write()
                .block_on()
                .map_err(backend("writing the working-copy commit"))?;
            tx.repo_mut()
                .set_wc_commit(name.clone(), commit.id().clone())
                .map_err(backend("moving the working copy"))?;
            tx.repo_mut()
                .rebase_descendants()
                .block_on()
                .map_err(backend("rebasing the working copy's descendants"))?;
            if git.is_some() {
                // As jj does in a colocated workspace: new files show in git's index as
                // intended to be added.
                jj_lib::git::update_intent_to_add(
                    tx.repo_mut(),
                    self.root.as_std_path(),
                    &old_tree,
                    &new_tree,
                )
                .block_on()
                .map_err(backend("updating git's index"))?;
                export_refs(tx.repo_mut())?;
            }
            repo = tx
                .commit(description)
                .block_on()
                .map_err(backend("writing the snapshot's operation"))?;
        }
        locked
            .finish(repo.op_id().clone())
            .block_on()
            .map_err(backend("saving the working copy's state"))?;
        self.repo = repo;
        Ok(())
    }

    /// The commits `expr` names, in jj's revset syntax; at least one. Only jj-lib's built-in
    /// functions: no aliases.
    fn resolve(&self, expr: &str) -> Result<Vec<Commit>, VcsError> {
        let commits = self.evaluate(self.repo.as_ref(), expr, &Revsets::default())?;
        if commits.is_empty() {
            return Err(VcsError::UnknownRevision(expr.to_owned()));
        }
        Ok(commits)
    }

    /// The commits `expr` names in `repo`, children before parents, with `revsets`; possibly
    /// none.
    fn evaluate(
        &self,
        repo: &dyn jj_lib::repo::Repo,
        expr: &str,
        revsets: &Revsets,
    ) -> Result<Vec<Commit>, VcsError> {
        self.resolve_expression(repo, expr, revsets)?
            .evaluate()
            .map_err(backend("evaluating a revision"))?
            .stream()
            .commits(repo.store())
            .try_collect()
            .block_on()
            .map_err(backend("reading a revision's commits"))
    }

    /// `expr` parsed with `revsets` and resolved in `repo`, unevaluated.
    fn resolve_expression<'a>(
        &self,
        repo: &'a dyn jj_lib::repo::Repo,
        expr: &str,
        revsets: &Revsets,
    ) -> Result<jj_lib::revset::ResolvedRevset<'a>, VcsError> {
        let path_converter = RepoPathUiConverter::Fs {
            cwd: self.root.clone().into(),
            base: self.root.clone().into(),
        };
        let fileset_aliases = FilesetAliasesMap::new();
        let extensions = RevsetExtensions::default();
        let context = RevsetParseContext {
            aliases_map: &revsets.aliases,
            local_variables: HashMap::new(),
            user_email: &revsets.user_email,
            current_time: jiff::Zoned::now(),
            default_ignored_remote: Some(REMOTE_NAME_FOR_LOCAL_GIT_REPO),
            fileset_aliases_map: &fileset_aliases,
            extensions: &extensions,
            workspace: Some(RevsetWorkspaceContext {
                path_converter: &path_converter,
                workspace_name: &self.workspace,
            }),
        };
        let expression = revset::parse(&mut RevsetDiagnostics::new(), expr, &context)
            .map_err(backend("parsing a revision"))?;
        let no_extensions: [Box<dyn SymbolResolverExtension>; 0] = [];
        let resolver = SymbolResolver::new(repo, &no_extensions);
        expression
            .resolve_user_expression(repo, &resolver)
            .map_err(|error| resolution_error(expr, error))
    }

    fn read_file(&self, path: &RepoPath, id: &FileId) -> Result<Vec<u8>, VcsError> {
        let mut reader =
            self.repo.store().read_file(path, id).block_on().map_err(backend("opening a file"))?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).block_on().map_err(backend("reading a file"))?;
        Ok(bytes)
    }

    fn working_copy_commit(&self) -> Result<Commit, VcsError> {
        let id = self
            .repo
            .view()
            .get_wc_commit_id(&self.workspace)
            .ok_or_else(|| VcsError::NoWorkingCopy(self.root.clone()))?;
        self.repo.store().get_commit(id).map_err(backend("reading the working-copy commit"))
    }

    /// The commit `id` names: the full ID of a commit jj's index holds. No revset is parsed.
    fn commit_by_id(&self, id: &vcs::CommitId) -> Result<Commit, VcsError> {
        let unknown = || VcsError::UnknownRevision(id.to_string());
        let jj_id = jj_lib::backend::CommitId::try_from_hex(id.as_str())
            .filter(|jj_id| jj_id.as_bytes().len() == self.repo.store().commit_id_length())
            .ok_or_else(unknown)?;
        let indexed =
            self.repo.index().has_id(&jj_id).block_on().map_err(backend("looking up a commit"))?;
        if !indexed {
            return Err(unknown());
        }
        self.repo.store().get_commit(&jj_id).map_err(backend("reading a commit"))
    }

    /// The one nearest common ancestor of `one` and `two`; none when there is none, or
    /// several.
    fn single_merge_base(
        &self,
        one: &jj_lib::backend::CommitId,
        two: &jj_lib::backend::CommitId,
    ) -> Result<Option<jj_lib::backend::CommitId>, VcsError> {
        let bases = self
            .repo
            .index()
            .common_ancestors(std::slice::from_ref(one), std::slice::from_ref(two))
            .block_on()
            .map_err(backend("finding a merge base"))?;
        Ok(<[_; 1]>::try_from(bases).ok().map(|[base]| base))
    }

    /// What one term of a conflict holds; `Absent` without a value.
    fn term_content(
        &self,
        path: &RepoPath,
        value: Option<&TreeValue>,
    ) -> Result<TermContent, VcsError> {
        Ok(match value {
            None => TermContent::Absent,
            Some(TreeValue::File { id, executable, .. }) => {
                TermContent::File { bytes: self.read_file(path, id)?, executable: *executable }
            }
            Some(TreeValue::Symlink(_)) => TermContent::Other(EntryKind::Symlink),
            Some(TreeValue::GitSubmodule(_)) => TermContent::Other(EntryKind::Submodule),
            Some(TreeValue::Tree(_)) => TermContent::Other(EntryKind::Directory),
        })
    }

    /// The conflict at `path` in `commit`'s tree: its adds are the sides and its removes the
    /// bases, in jj's order, each labelled from the tree. None when the path is resolved.
    fn conflict_in(&self, commit: &Commit, path: &Utf8Path) -> Result<Option<Conflict>, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let tree = commit.tree();
        let value =
            tree.path_value(&repo_path).block_on().map_err(backend("finding a path in a tree"))?;
        if value.is_resolved() {
            return Ok(None);
        }
        let (value, labels) = native::simplified(&value, tree.labels());
        let terms = |values: &mut dyn Iterator<Item = &Option<TreeValue>>, side: bool| {
            values
                .enumerate()
                .map(|(index, value)| {
                    let label = if side { labels.get_add(index) } else { labels.get_remove(index) };
                    self.term_content(&repo_path, value.as_ref()).map(|content| ConflictTerm {
                        term: Term { label: label.map(str::to_owned), change: None },
                        content,
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(Some(Conflict {
            path: path.to_owned(),
            sides: terms(&mut value.adds(), true)?,
            bases: terms(&mut value.removes(), false)?,
        }))
    }

    fn change_info(&self, commit: &Commit) -> Result<ChangeInfo, VcsError> {
        let change_len = self
            .repo
            .shortest_unique_change_id_prefix_len(commit.change_id())
            .block_on()
            .map_err(backend("shortening a change ID"))?;
        let commit_len = self
            .repo
            .index()
            .shortest_unique_commit_id_prefix_len(commit.id())
            .block_on()
            .map_err(backend("shortening a commit ID"))?;
        let short = |mut hex: String, len: usize| {
            hex.truncate(len.max(MIN_SHORT_ID));
            hex
        };
        Ok(ChangeInfo {
            commit: short(commit.id().hex(), commit_len),
            change_id: Some(short(commit.change_id().reverse_hex(), change_len)),
            summary: commit
                .description()
                .lines()
                .next()
                .filter(|line| !line.is_empty())
                .map(str::to_owned),
        })
    }
}

impl JjRepository {
    /// Commit `id`'s change, author and author time.
    fn origin(&self, commit: &Commit) -> Result<LineOrigin, VcsError> {
        let author = commit.author();
        Ok(LineOrigin {
            change: self.change_info(commit)?,
            author: author.name.clone(),
            time: timestamp(&author.timestamp),
        })
    }
}

/// jj's timestamp, in milliseconds and an offset in minutes, as seconds. Milliseconds round
/// down, so a time before the epoch keeps its second.
/// `id` as the `vcs` crate names a commit: its full hex.
fn commit_id(id: &jj_lib::backend::CommitId) -> vcs::CommitId {
    vcs::CommitId::new(id.hex())
}

fn timestamp(time: &jj_lib::backend::Timestamp) -> Timestamp {
    Timestamp {
        seconds: time.timestamp.0.div_euclid(MILLIS_PER_SECOND),
        offset_seconds: time.tz_offset * SECONDS_PER_MINUTE,
    }
}

fn resolution_error(expr: &str, error: RevsetResolutionError) -> VcsError {
    match error {
        RevsetResolutionError::NoSuchRevision { .. }
        | RevsetResolutionError::EmptyString
        | RevsetResolutionError::WorkspaceMissingWorkingCopy { .. } => {
            VcsError::UnknownRevision(expr.to_owned())
        }
        RevsetResolutionError::AmbiguousCommitIdPrefix(_)
        | RevsetResolutionError::AmbiguousChangeIdPrefix(_)
        | RevsetResolutionError::DivergentChangeId { .. }
        | RevsetResolutionError::ConflictedRef { .. } => {
            VcsError::AmbiguousRevision(expr.to_owned())
        }
        other => backend("resolving a revision")(other),
    }
}

impl Repository for JjRepository {
    fn kind(&self) -> RepoKind {
        RepoKind::Jj
    }

    fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// A revision naming several commits reads from the merge of their trees, as jj does for
    /// a merge commit's parents.
    fn file_at(&self, path: &Utf8Path, rev: &RevSpec) -> Result<Option<Vec<u8>>, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let (commits, named) = match rev {
            RevSpec::Index => return Err(VcsError::NoIndex(JJ)),
            RevSpec::Expr(expr) => (self.resolve(expr)?, expr.as_str()),
            RevSpec::Commit(id) => (vec![self.commit_by_id(id)?], id.as_str()),
        };
        let tree = jj_lib::rewrite::merge_commit_trees(self.repo.as_ref(), &commits)
            .block_on()
            .map_err(backend("merging a revision's trees"))?;
        let Some(id) = file_id(&tree, &repo_path, path, named)? else {
            return Ok(None);
        };
        self.read_file(&repo_path, &id).map(Some)
    }

    /// `@-`, titled by its change ID and summary; untitled when the working copy is a merge.
    fn default_parent(&self, _path: &Utf8Path) -> Result<Parent, VcsError> {
        let term = match self.resolve(PARENT)?.as_slice() {
            [commit] => Term { label: None, change: Some(self.change_info(commit)?) },
            _ => Term::default(),
        };
        Ok(Parent { rev: RevSpec::Expr(PARENT.to_owned()), term })
    }
    /// jj-lib's annotation over every commit.
    fn annotate(
        &self,
        path: &Utf8Path,
        id: &vcs::CommitId,
    ) -> Result<Option<Annotation>, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let commit = self.commit_by_id(id)?;
        if file_id(&commit.tree(), &repo_path, path, id.as_str())?.is_none() {
            return Ok(None);
        }
        let mut annotator = FileAnnotator::from_commit(&commit, &repo_path)
            .block_on()
            .map_err(backend("reading a file to annotate"))?;
        annotator
            .compute(self.repo.as_ref(), &ResolvedRevsetExpression::all())
            .block_on()
            .map_err(backend("annotating a file"))?;
        let annotation = annotator.to_annotation();
        // Over every commit no line's origin lies outside the domain, so `Err` does not occur;
        // it would name the commit where the search stopped, which is the best origin known.
        let runs = annotation
            .lines()
            .enumerate()
            .map(|(line, (id, _))| (line..line + 1, id.unwrap_or_else(|id| id)));
        Annotation::from_runs(runs, |commit: &&Commit| self.origin(commit)).map(Some)
    }

    fn commits(&self, rev: &str) -> Result<Vec<vcs::CommitId>, VcsError> {
        Ok(self.resolve(rev)?.iter().map(|commit| vcs::CommitId::new(commit.id().hex())).collect())
    }

    fn describe(&self, id: &vcs::CommitId) -> Result<LineOrigin, VcsError> {
        self.origin(&self.commit_by_id(id)?)
    }

    fn merge_base(
        &self,
        one: &vcs::CommitId,
        two: &vcs::CommitId,
    ) -> Result<Option<vcs::CommitId>, VcsError> {
        let (one, two) = (self.commit_by_id(one)?, self.commit_by_id(two)?);
        Ok(self.single_merge_base(one.id(), two.id())?.map(|base| commit_id(&base)))
    }

    fn conflicts(&self) -> Result<Vec<Utf8PathBuf>, VcsError> {
        // The tree walk gives paths in jj's order, component by component.
        self.working_copy_commit()?
            .tree()
            .conflicts()
            .map(|(path, value)| {
                value.map_err(backend("reading a conflict"))?;
                Ok(Utf8PathBuf::from(path.as_internal_file_string()))
            })
            .try_collect()
            .block_on()
    }

    /// The working copy's record of `path`. A record of another value than the working-copy
    /// commit's is stale: the file on disk is not the conflict `read_conflict` reads.
    fn recorded(&self, path: &Utf8Path) -> Result<Option<Recorded>, VcsError> {
        let repo_path = RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        let workspace = self.workspace()?;
        let local = workspace
            .working_copy()
            .downcast_ref::<LocalWorkingCopy>()
            .ok_or(VcsError::NoWorkingCopy(self.root.clone()))?;
        let states = local.file_states().map_err(backend("reading the working copy's state"))?;
        let Some(state) = states.get(&repo_path) else {
            return Ok(None);
        };
        let value = |tree: &MergedTree| {
            tree.path_value(&repo_path).block_on().map_err(backend("finding a path in a tree"))
        };
        let on_disk = value(local.tree().map_err(backend("reading the working copy's tree"))?)?;
        if on_disk != value(&self.working_copy_commit()?.tree())? {
            return Err(VcsError::StaleWorkingCopy(path.to_owned()));
        }
        let changed = match std::fs::symlink_metadata(self.root.join(path)) {
            Ok(metadata) => {
                let modified =
                    metadata.modified().map_err(backend("reading a file's modification time"))?;
                millis(modified) != Some(state.mtime.0) || metadata.len() != state.size
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(backend("reading a file's metadata")(error)),
        };
        let marker_length = state
            .materialized_conflict_data
            .map(|data| usize::try_from(data.conflict_marker_len))
            .transpose()
            .map_err(backend("reading a marker length"))?;
        Ok(Some(Recorded { changed, marker_length }))
    }

    /// The terms of the working-copy commit's merged tree value: its adds are the sides and
    /// its removes the bases, in jj's order.
    fn read_conflict(&self, path: &Utf8Path) -> Result<Option<Conflict>, VcsError> {
        self.conflict_in(&self.working_copy_commit()?, path)
    }

    fn merge_candidates(&self) -> Result<vcs::MergeCandidates, VcsError> {
        let wc = self.working_copy_commit()?;
        let parents = wc.parent_ids();
        // The base every parent shares: each further parent's merge base with the base so far.
        let base = match parents {
            [first, rest @ ..] if !rest.is_empty() => {
                rest.iter().try_fold(Some(first.clone()), |base, parent| match base {
                    Some(base) => self.single_merge_base(&base, parent),
                    None => Ok(None),
                })?
            }
            _ => None,
        };
        let shared =
            std::iter::once(wc.id()).chain(parents).chain(base.as_ref()).map(commit_id).collect();
        Ok(vcs::MergeCandidates { shared, ..vcs::MergeCandidates::default() })
    }

    fn copy_candidates(&self) -> Result<Vec<vcs::CommitId>, VcsError> {
        let wc = self.working_copy_commit()?;
        let parent = match wc.parent_ids() {
            [parent] => Some(parent),
            _ => None,
        };
        Ok(parent.into_iter().chain([wc.id()]).map(commit_id).collect())
    }

    fn working_copy_id(&self) -> Result<Option<vcs::CommitId>, VcsError> {
        Ok(Some(commit_id(self.working_copy_commit()?.id())))
    }

    fn index_base(&self) -> Result<Option<vcs::CommitId>, VcsError> {
        Ok(None)
    }

    fn conflict_style(&self) -> Result<Option<ConflictStyle>, VcsError> {
        Ok(None)
    }

    fn snapshot(&mut self) -> Result<(), VcsError> {
        self.take_snapshot(SNAPSHOT_DESCRIPTION, true)
    }

    /// Not marked as a snapshot, so `jj undo` reverts it as any other operation.
    fn record_save(&mut self, path: &Utf8Path, _resolved: bool) -> Result<(), VcsError> {
        RepoPathBuf::from_relative_path(path)
            .map_err(|_| VcsError::OutsideRepository(path.to_owned()))?;
        self.take_snapshot(&format!("save {path} in triad"), false)
    }
}

/// As jj does before its snapshot in a colocated workspace: when git's HEAD moved since jj
/// last read it, import it, check out a new working-copy commit on it, and reset the working
/// copy's record to that commit without touching the files, which git already updated. The
/// repository at the new operation; `repo` when HEAD did not move.
fn import_git_head(
    workspace: &mut Workspace,
    repo: Arc<ReadonlyRepo>,
    name: &WorkspaceNameBuf,
) -> Result<Arc<ReadonlyRepo>, VcsError> {
    let mut tx = repo.start_transaction();
    jj_lib::git::import_head(tx.repo_mut(), name, workspace.workspace_root())
        .block_on()
        .map_err(backend("importing git's HEAD"))?;
    if !tx.repo().has_changes() {
        return Ok(repo);
    }
    let Some(head) = tx.repo().view().git_head(name).as_normal().cloned() else {
        // git's HEAD is unborn: jj records that and keeps its working-copy commit.
        return tx
            .commit(IMPORT_HEAD_DESCRIPTION)
            .block_on()
            .map_err(backend("writing the import's operation"));
    };
    let head = tx.repo().store().get_commit(&head).map_err(backend("reading git's HEAD commit"))?;
    let wc_commit = tx
        .repo_mut()
        .check_out(name.clone(), &head)
        .block_on()
        .map_err(backend("checking out git's HEAD"))?;
    let mut locked = workspace
        .start_working_copy_mutation()
        .block_on()
        .map_err(backend("locking the working copy"))?;
    locked
        .locked_wc()
        .reset(&wc_commit)
        .block_on()
        .map_err(backend("resetting the working copy to git's HEAD"))?;
    tx.repo_mut()
        .rebase_descendants()
        .block_on()
        .map_err(backend("rebasing the working copy's descendants"))?;
    let repo = tx
        .commit(IMPORT_HEAD_DESCRIPTION)
        .block_on()
        .map_err(backend("writing the import's operation"))?;
    locked
        .finish(repo.op_id().clone())
        .block_on()
        .map_err(backend("saving the working copy's state"))?;
    Ok(repo)
}

/// Export jj's bookmarks and tags to the colocated git repository, as jj does after each
/// command, so a bookmark the snapshot moved with `@` moves in git too. A ref git holds
/// otherwise than jj last saw stays as it is, as in jj; the next jj command reports it.
fn export_refs(repo: &mut jj_lib::repo::MutableRepo) -> Result<(), VcsError> {
    let stats = jj_lib::git::export_refs(repo).map_err(backend("exporting refs to git"))?;
    for (symbol, reason) in stats.failed_bookmarks.iter().chain(&stats.failed_tags) {
        tracing::warn!(%symbol, %reason, "cannot export a ref to git");
    }
    Ok(())
}

/// git's repository at the workspace root when the workspace is colocated with it, as jj
/// finds it; none for an internal git store.
fn colocated_git(repo: &ReadonlyRepo, root: &std::path::Path) -> Option<gix::Repository> {
    jj_lib::git::get_git_backend(repo.store()).ok()?.open_git_repo_at_workdir(root).ok()
}

/// The file `path` holds in `tree`: `None` when it is no file there.
///
/// # Errors
///
/// [`VcsError::Conflicted`] when the path is conflicted in `tree`, which `expr` named.
fn file_id(
    tree: &MergedTree,
    repo_path: &RepoPath,
    path: &Utf8Path,
    expr: &str,
) -> Result<Option<FileId>, VcsError> {
    let value =
        tree.path_value(repo_path).block_on().map_err(backend("finding a path in a tree"))?;
    match value.into_resolved() {
        Ok(Some(TreeValue::File { id, .. })) => Ok(Some(id)),
        Ok(_) => Ok(None),
        Err(_) => Err(VcsError::Conflicted { path: path.to_owned(), rev: expr.to_owned() }),
    }
}

#[cfg(test)]
mod tests {
    use jj_lib::backend::MillisSinceEpoch;

    use super::*;

    fn jj_time(millis: i64, tz_offset: i32) -> jj_lib::backend::Timestamp {
        jj_lib::backend::Timestamp { timestamp: MillisSinceEpoch(millis), tz_offset }
    }

    #[test]
    fn a_time_before_the_epoch_has_negative_milliseconds() {
        const OFFSET: std::time::Duration = std::time::Duration::from_millis(1500);
        let epoch = std::time::UNIX_EPOCH;
        assert_eq!(millis(epoch - OFFSET), Some(-1500));
        assert_eq!(millis(epoch), Some(0));
        assert_eq!(millis(epoch + OFFSET), Some(1500));
    }

    #[test]
    fn a_timestamp_converts_to_seconds_and_an_offset_in_seconds() {
        // 2001-02-03T04:05:06.789+05:30
        assert_eq!(
            timestamp(&jj_time(981_173_106_789, 330)),
            Timestamp { seconds: 981_173_106, offset_seconds: 19_800 }
        );
    }

    #[test]
    fn a_timestamp_before_the_epoch_rounds_down_and_keeps_a_negative_offset() {
        // 1969-12-31T23:59:58.500-08:00
        assert_eq!(
            timestamp(&jj_time(-1_500, -480)),
            Timestamp { seconds: -2, offset_seconds: -28_800 }
        );
    }
}
