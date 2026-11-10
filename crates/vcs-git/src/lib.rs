//! `Repository` for git, through gix. Pinned to the gix version jj-lib uses, so the build holds one copy.

#[cfg(feature = "testing")]
pub mod testing;
pub mod tool_config;

use std::error::Error;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use gix::bstr::BString;
use vcs::{
    Annotation, ChangeInfo, CommitId, Conflict, ConflictStyle, ConflictTerm, EntryKind, LineOrigin,
    MergeCandidates, Parent, Recorded, RepoKind, Repository, RevSpec, Term, TermContent, Timestamp,
    VcsError,
};

const HEAD: &str = "HEAD";
const CONFLICT_STYLE_KEY: &str = "merge.conflictStyle";
/// Whether git trusts the file system's executable bit; true when unset.
const FILE_MODE_KEY: &str = "core.fileMode";
const INDEX_LABEL: &str = "index";

/// The pseudo-refs git leaves when an operation stops on a conflict, in the order triad
/// tries them, with how each gives the base and theirs (spec §7.3).
const PENDING: [(&str, Pending); 4] = [
    ("MERGE_HEAD", Pending::Merge),
    ("CHERRY_PICK_HEAD", Pending::Pick),
    ("REVERT_HEAD", Pending::Revert),
    ("REBASE_HEAD", Pending::Pick),
];

/// How a stopped operation's commit `X` gives the base and theirs, `HEAD` being ours.
#[derive(Debug, Clone, Copy)]
enum Pending {
    /// Theirs `X`; the base the merge base of `HEAD` and `X`.
    Merge,
    /// Theirs `X`; the base `X`'s first parent. A cherry-pick, or a commit a rebase replays.
    Pick,
    /// Theirs `X`'s first parent; the base `X`.
    Revert,
}

/// A git repository, opened through gix.
pub struct GitRepository {
    repo: gix::Repository,
    root: Utf8PathBuf,
}

impl GitRepository {
    /// The repository holding `start`, searching up from it.
    ///
    /// # Errors
    ///
    /// [`VcsError::NotARepository`] when no ancestor is a git repository,
    /// [`VcsError::NoWorkingCopy`] for a bare repository, and [`VcsError::NotUtf8`] when the
    /// working copy's path is not UTF-8.
    pub fn discover(start: &Utf8Path) -> Result<Self, VcsError> {
        let repo = gix::discover(start).map_err(|error| {
            if error.classify().is_not_found() {
                VcsError::NotARepository(start.to_owned())
            } else {
                backend("opening the repository")(error)
            }
        })?;
        Self::with_working_tree(repo)
    }

    /// The repository whose working tree is `root`, without searching up from it.
    ///
    /// # Errors
    ///
    /// [`VcsError::NotARepository`] when `root` holds no git repository, though a parent may,
    /// [`VcsError::NoWorkingCopy`] for a bare repository, and [`VcsError::NotUtf8`] when the
    /// working copy's path is not UTF-8.
    pub fn open(root: &Utf8Path) -> Result<Self, VcsError> {
        let repo = gix::open(root).map_err(|error| {
            if error.classify().is_not_found() {
                VcsError::NotARepository(root.to_owned())
            } else {
                backend("opening the repository")(error)
            }
        })?;
        Self::with_working_tree(repo)
    }

    /// `repo`, which must have a working tree.
    fn with_working_tree(repo: gix::Repository) -> Result<Self, VcsError> {
        let Some(workdir) = repo.workdir() else {
            let git_dir = Utf8Path::from_path(repo.path())
                .ok_or_else(|| VcsError::NotUtf8(repo.path().to_owned()))?;
            return Err(VcsError::NoWorkingCopy(git_dir.to_owned()));
        };
        let root = Utf8PathBuf::from_path_buf(workdir.to_owned()).map_err(VcsError::NotUtf8)?;
        Ok(Self { repo, root })
    }

    fn blob(&self, id: impl Into<gix::ObjectId>) -> Result<Vec<u8>, VcsError> {
        let object = self.repo.find_object(id).map_err(backend("reading a blob"))?;
        Ok(object.detach().data)
    }

    fn committed(
        &self,
        path: &Utf8Path,
        commit: gix::ObjectId,
    ) -> Result<Option<Vec<u8>>, VcsError> {
        self.entry(path, commit)?.map(|blob| self.blob(blob)).transpose()
    }

    /// The commit `id` names: the full ID of a commit the repository holds. No expression is
    /// parsed, so a ref whose name looks like an ID does not stand in for it.
    fn commit_by_id(&self, id: &CommitId) -> Result<gix::ObjectId, VcsError> {
        let unknown = || VcsError::UnknownRevision(id.to_string());
        let object = gix::ObjectId::from_hex(id.as_str().as_bytes()).map_err(|_| unknown())?;
        match self.repo.try_find_header(object).map_err(backend("reading a commit"))? {
            Some(header) if header.kind() == gix::object::Kind::Commit => Ok(object),
            _ => Err(unknown()),
        }
    }

    /// The object `expr` names.
    fn revision(&self, expr: &str) -> Result<gix::ObjectId, VcsError> {
        self.repo.rev_parse_single(expr).map(gix::Id::detach).map_err(|error| {
            if error.classify().is_not_found() {
                VcsError::UnknownRevision(expr.to_owned())
            } else {
                backend("resolving a revision")(error)
            }
        })
    }

    /// The commit `expr` names, peeling a tag. An expression naming a tree or a blob names no
    /// commit.
    fn commit(&self, expr: &str) -> Result<gix::ObjectId, VcsError> {
        let id = self.revision(expr)?;
        let object = self.repo.find_object(id).map_err(backend("reading a revision"))?;
        match object.peel_to_kind(gix::object::Kind::Commit) {
            Ok(commit) => Ok(commit.id),
            Err(_) => Err(VcsError::UnknownRevision(expr.to_owned())),
        }
    }

    /// The blob `path` has in `revision`'s tree, if it is a file there.
    fn entry(
        &self,
        path: &Utf8Path,
        revision: gix::ObjectId,
    ) -> Result<Option<gix::ObjectId>, VcsError> {
        let tree = self
            .repo
            .find_object(revision)
            .and_then(gix::Object::peel_to_tree)
            .map_err(backend("reading a revision's tree"))?;
        let entry = tree
            .lookup_entry_by_path(path.as_std_path())
            .map_err(backend("finding a path in a tree"))?;
        Ok(entry.filter(|entry| entry.mode().is_blob()).map(|entry| entry.object_id()))
    }

    /// Commit `id`'s short ID, summary, author and author time.
    fn origin(&self, id: gix::ObjectId) -> Result<LineOrigin, VcsError> {
        let commit = self.repo.find_commit(id).map_err(backend("reading a commit"))?;
        let author = commit.author().map_err(backend("reading a commit's author"))?;
        let time = author.time().map_err(backend("reading a commit's time"))?;
        Ok(LineOrigin {
            change: change_info(&commit)?,
            author: author.name.to_string(),
            time: Timestamp { seconds: time.seconds, offset_seconds: time.offset },
        })
    }

    /// The base's, ours and theirs terms of the operation that stopped on a conflict, as
    /// [`GitRepository::stopped`] gives their commits. Ours is titled by `HEAD`'s branch.
    fn stopped_terms(&self) -> Result<[Term; 3], VcsError> {
        let [base, _, theirs] = self.stopped()?;
        let ours = match self.head_commit()? {
            Some(head) => self.head_term(&head)?,
            None => Term::default(),
        };
        Ok([self.commit_term(base)?, ours, self.commit_term(theirs)?])
    }

    /// The base's, ours and theirs commits of the operation that stopped on a conflict. Ours
    /// is `HEAD`; theirs and the base follow the first of [`PENDING`] that exists, and are
    /// none without one, for a merge without a single merge base, or for a pick of a root
    /// commit.
    fn stopped(&self) -> Result<[Option<gix::ObjectId>; 3], VcsError> {
        let ours = self.head_id()?;
        for (name, pending) in PENDING {
            let commit = match self.commit(name) {
                Ok(commit) => commit,
                Err(VcsError::UnknownRevision(_)) => continue,
                Err(error) => return Err(error),
            };
            let parent = || self.first_parent(commit);
            let (base, theirs) = match pending {
                Pending::Merge => (self.single_merge_base(commit)?, Some(commit)),
                Pending::Pick => (parent()?, Some(commit)),
                Pending::Revert => (Some(commit), parent()?),
            };
            return Ok([base, ours, theirs]);
        }
        Ok([None, ours, None])
    }

    /// `HEAD`'s commit ID, or none before the first commit.
    fn head_id(&self) -> Result<Option<gix::ObjectId>, VcsError> {
        Ok(self.head_commit()?.map(|commit| commit.id))
    }

    /// The term of `commit`: its short ID and summary; empty without a commit.
    fn commit_term(&self, commit: Option<gix::ObjectId>) -> Result<Term, VcsError> {
        let Some(commit) = commit else {
            return Ok(Term::default());
        };
        let commit = self.repo.find_commit(commit).map_err(backend("reading a commit"))?;
        Ok(Term { label: None, change: Some(change_info(&commit)?) })
    }

    fn first_parent(&self, commit: gix::ObjectId) -> Result<Option<gix::ObjectId>, VcsError> {
        let commit = self.repo.find_commit(commit).map_err(backend("reading a commit"))?;
        Ok(commit.parent_ids().next().map(gix::Id::detach))
    }

    /// The one merge base of `HEAD` and `commit`; none when there is none, or several.
    fn single_merge_base(&self, commit: gix::ObjectId) -> Result<Option<gix::ObjectId>, VcsError> {
        let Some(head) = self.head_commit()? else {
            return Ok(None);
        };
        let bases = self
            .repo
            .merge_bases_many(head.id, &[commit])
            .map_err(backend("finding a merge base"))?;
        Ok(match bases.as_slice() {
            [base] => Some(base.detach()),
            _ => None,
        })
    }

    /// What a conflict stage holds; `Absent` without an entry.
    fn term_content(&self, entry: Option<&gix::index::Entry>) -> Result<TermContent, VcsError> {
        use gix::index::entry::Mode;
        let Some(entry) = entry else {
            return Ok(TermContent::Absent);
        };
        let file =
            |executable| self.blob(entry.id).map(|bytes| TermContent::File { bytes, executable });
        match entry.mode {
            Mode::FILE => file(false),
            Mode::FILE_EXECUTABLE => file(true),
            Mode::SYMLINK => Ok(TermContent::Other(EntryKind::Symlink)),
            mode if mode.is_submodule() => Ok(TermContent::Other(EntryKind::Submodule)),
            _ => Ok(TermContent::Other(EntryKind::Directory)),
        }
    }

    fn staged(&self, path: &Utf8Path) -> Result<Option<Vec<u8>>, VcsError> {
        self.staged_id(path)?.map(|id| self.blob(id)).transpose()
    }

    /// The blob `path` has in the index, if any.
    fn staged_id(&self, path: &Utf8Path) -> Result<Option<gix::ObjectId>, VcsError> {
        let index = self.repo.index_or_empty().map_err(backend("reading the index"))?;
        Ok(index.entry_by_path(git_path(path).as_ref()).map(|entry| entry.id))
    }

    /// `HEAD`'s commit, or none before the first commit.
    fn head_commit(&self) -> Result<Option<gix::Commit<'_>>, VcsError> {
        let mut head = self.repo.head().map_err(backend("reading HEAD"))?;
        if head.is_unborn() {
            return Ok(None);
        }
        head.peel_to_commit().map(Some).map_err(backend("reading HEAD's commit"))
    }

    /// Replace `path`'s conflict stages with one stage-0 entry for its working-tree file, as
    /// `git add` does: the bytes through git's clean filters, the executable bit from the file
    /// under `core.fileMode`, else from ours, then theirs, and the file's stat. A path
    /// without stages is left alone. The tree cache goes, since it may name the conflict.
    /// The index is read and written under its lock, so no other writer's change is lost.
    fn stage_resolution(&self, path: &Utf8Path) -> Result<(), VcsError> {
        use gix::index::entry::{Flags, Mode, Stage, Stat};
        use gix::lock::acquire::Fail;
        /// The mode bits `core.sharedRepository` would add; gix's own index write adds none.
        const NO_SHARED_PERMISSIONS: i32 = 0;
        let git_path = git_path(path);
        let lock = gix::lock::File::acquire_to_update_resource(
            self.repo.index_path(),
            Fail::Immediately,
            None,
            NO_SHARED_PERMISSIONS,
        )
        .map_err(backend("writing the index"))?;
        let mut index = self.repo.open_index().map_err(backend("reading the index"))?;
        let staged_mode =
            |stage| index.entry_by_path_and_stage(git_path.as_ref(), stage).map(|entry| entry.mode);
        let Some(conflict_mode) =
            [Stage::Ours, Stage::Theirs, Stage::Base].into_iter().find_map(staged_mode)
        else {
            return Ok(());
        };
        let file = self.root.join(path);
        let metadata = gix::index::fs::Metadata::from_path_no_follow(file.as_std_path())
            .map_err(backend("reading a file's metadata"))?;
        let mode = if self.trusts_file_mode() {
            if metadata.is_executable() { Mode::FILE_EXECUTABLE } else { Mode::FILE }
        } else if conflict_mode == Mode::FILE_EXECUTABLE {
            Mode::FILE_EXECUTABLE
        } else {
            Mode::FILE
        };
        let bytes = std::fs::read(&file).map_err(backend("reading the resolved file"))?;
        let blob = self.clean(path, &bytes, &index)?;
        let id = self.repo.write_blob(blob).map_err(backend("writing the resolved file"))?.detach();
        let stat = Stat::from_fs(&metadata).map_err(backend("reading a file's times"))?;
        index.remove_entries(|_, entry_path, _| entry_path == git_path);
        index.dangerously_push_entry(stat, id, Flags::empty(), mode, git_path.as_ref());
        index.sort_entries();
        index.remove_tree();
        let mut out = std::io::BufWriter::new(lock);
        index
            .write_to(&mut out, gix::index::write::Options::default())
            .map_err(backend("writing the index"))?;
        let lock = out.into_inner().map_err(|e| backend("writing the index")(e.into_error()))?;
        lock.commit().map_err(|e| backend("writing the index")(e.error))?;
        Ok(())
    }

    /// `bytes`, the working-tree file at `path`, as `git add` stores them: through the
    /// clean filters and line-ending conversion the attributes and config ask for.
    fn clean(
        &self,
        path: &Utf8Path,
        bytes: &[u8],
        index: &gix::index::State,
    ) -> Result<Vec<u8>, VcsError> {
        use gix::filter::plumbing::pipeline::convert::ToGitOutcome;
        use std::io::Read as _;
        let (mut pipeline, _) =
            self.repo.filter_pipeline(None).map_err(backend("loading git's filters"))?;
        let outcome = pipeline
            .convert_to_git(bytes, path.as_std_path(), index)
            .map_err(backend("running git's clean filters"))?;
        Ok(match outcome {
            ToGitOutcome::Unchanged(_) => bytes.to_vec(),
            ToGitOutcome::Buffer(converted) => converted.to_vec(),
            ToGitOutcome::Process(mut reader) => {
                let mut converted = Vec::new();
                reader
                    .read_to_end(&mut converted)
                    .map_err(backend("running git's clean filters"))?;
                converted
            }
        })
    }

    fn trusts_file_mode(&self) -> bool {
        self.repo.config_snapshot().boolean(FILE_MODE_KEY).unwrap_or(true)
    }

    fn head_term(&self, commit: &gix::Commit<'_>) -> Result<Term, VcsError> {
        let label = self
            .repo
            .head_name()
            .map_err(backend("reading HEAD's branch"))?
            .map(|name| name.shorten().to_string());
        Ok(Term { label, change: Some(change_info(commit)?) })
    }
}

/// `id` as the `vcs` crate names a commit: its full hex.
fn commit_id(id: &gix::ObjectId) -> CommitId {
    CommitId::new(id.to_string())
}

/// `commit`'s short ID and summary.
fn change_info(commit: &gix::Commit<'_>) -> Result<ChangeInfo, VcsError> {
    let summary =
        commit.message().map_err(backend("reading a commit's message"))?.summary().to_string();
    let short = commit.id().shorten().map_err(backend("shortening a commit ID"))?;
    Ok(ChangeInfo { commit: short.to_string(), change_id: None, summary: Some(summary) })
}

/// `path` with `/` between its components, as git stores it.
/// jj's rule for a repository path (`RepoPathBuf::from_relative_path`): `.` is the root, and
/// any other path holds only named components.
fn check_inside(path: &Utf8Path) -> Result<(), VcsError> {
    let named = |component| matches!(component, Utf8Component::Normal(_));
    if path == "." || path.components().all(named) {
        Ok(())
    } else {
        Err(VcsError::OutsideRepository(path.to_owned()))
    }
}

fn git_path(path: &Utf8Path) -> BString {
    path.components().map(|component| component.as_str()).collect::<Vec<_>>().join("/").into()
}

fn backend<E: Error + Send + Sync + 'static>(context: &'static str) -> impl FnOnce(E) -> VcsError {
    move |error| VcsError::Backend { context, source: Box::new(error) }
}

impl Repository for GitRepository {
    fn kind(&self) -> RepoKind {
        RepoKind::Git
    }

    fn root(&self) -> &Utf8Path {
        &self.root
    }

    fn file_at(&self, path: &Utf8Path, rev: &RevSpec) -> Result<Option<Vec<u8>>, VcsError> {
        check_inside(path)?;
        match rev {
            RevSpec::Index => self.staged(path),
            RevSpec::Expr(expr) => self.committed(path, self.revision(expr)?),
            RevSpec::Commit(id) => self.committed(path, self.commit_by_id(id)?),
        }
    }

    /// The index when `path` has staged changes or there is no commit yet; otherwise `HEAD`.
    fn default_parent(&self, path: &Utf8Path) -> Result<Parent, VcsError> {
        let index = Parent {
            rev: RevSpec::Index,
            term: Term { label: Some(INDEX_LABEL.to_owned()), change: None },
        };
        let Some(commit) = self.head_commit()? else {
            return Ok(index);
        };
        let tree = commit.tree().map_err(backend("reading HEAD's tree"))?;
        let committed = tree
            .lookup_entry_by_path(path.as_std_path())
            .map_err(backend("finding a path in a tree"))?
            .map(|entry| entry.object_id());
        if committed != self.staged_id(path)? {
            return Ok(index);
        }
        Ok(Parent { rev: RevSpec::Expr(HEAD.to_owned()), term: self.head_term(&commit)? })
    }
    fn describe(&self, id: &CommitId) -> Result<LineOrigin, VcsError> {
        self.origin(self.commit_by_id(id)?)
    }

    /// gix's blame, without rename tracking.
    fn annotate(&self, path: &Utf8Path, commit: &CommitId) -> Result<Option<Annotation>, VcsError> {
        check_inside(path)?;
        let commit = self.commit_by_id(commit)?;
        if self.entry(path, commit)?.is_none() {
            return Ok(None);
        }
        let outcome = self
            .repo
            .blame_file(
                git_path(path).as_ref(),
                commit,
                gix::repository::blame_file::Options::default(),
            )
            .map_err(backend("blaming a file"))?;
        let runs =
            outcome.entries.iter().map(|entry| (entry.range_in_blamed_file(), entry.commit_id));
        Annotation::from_runs(runs, |&id| self.origin(id)).map(Some)
    }

    fn commits(&self, rev: &str) -> Result<Vec<vcs::CommitId>, VcsError> {
        Ok(vec![vcs::CommitId::new(self.commit(rev)?.to_string())])
    }

    fn merge_base(
        &self,
        one: &CommitId,
        two: &CommitId,
    ) -> Result<Option<vcs::CommitId>, VcsError> {
        let bases = self
            .repo
            .merge_bases_many(self.commit_by_id(one)?, &[self.commit_by_id(two)?])
            .map_err(backend("finding a merge base"))?;
        Ok(match bases.as_slice() {
            [base] => Some(vcs::CommitId::new(base.to_string())),
            _ => None,
        })
    }

    fn conflicts(&self) -> Result<Vec<Utf8PathBuf>, VcsError> {
        let index = self.repo.index_or_empty().map_err(backend("reading the index"))?;
        let mut stages: Vec<&gix::bstr::BStr> = index
            .entries()
            .iter()
            .filter(|entry| entry.stage() != gix::index::entry::Stage::Unconflicted)
            .map(|entry| entry.path(&index))
            .collect();
        // The index sorts its entries by path, so a path's stages lie together.
        stages.dedup();
        let mut paths = stages
            .into_iter()
            .map(|path| {
                let path = gix::path::from_bstr(path).map_err(backend("reading an index path"))?;
                Utf8PathBuf::from_path_buf(path.into_owned()).map_err(VcsError::NotUtf8)
            })
            .collect::<Result<Vec<_>, _>>()?;
        // The index's byte order puts `a.txt` before `a/b.txt`; path order is by component.
        paths.sort();
        Ok(paths)
    }

    /// Stage 1 is the base, stages 2 and 3 the sides, ours then theirs.
    fn recorded(&self, path: &Utf8Path) -> Result<Option<Recorded>, VcsError> {
        check_inside(path)?;
        Ok(None)
    }

    fn read_conflict(&self, path: &Utf8Path) -> Result<Option<Conflict>, VcsError> {
        use gix::index::entry::Stage;
        check_inside(path)?;
        let index = self.repo.index_or_empty().map_err(backend("reading the index"))?;
        let git_path = git_path(path);
        let entries = [Stage::Base, Stage::Ours, Stage::Theirs]
            .map(|stage| index.entry_by_path_and_stage(git_path.as_ref(), stage));
        if entries.iter().all(Option::is_none) {
            return Ok(None);
        }
        let [base, ours, theirs] = self.stopped_terms()?;
        let [base, ours, theirs] = [(base, entries[0]), (ours, entries[1]), (theirs, entries[2])]
            .map(|(term, entry)| {
                self.term_content(entry).map(|content| ConflictTerm { term, content })
            });
        Ok(Some(Conflict {
            path: path.to_owned(),
            sides: vec![ours?, theirs?],
            bases: vec![base?],
        }))
    }

    fn merge_candidates(&self) -> Result<MergeCandidates, VcsError> {
        let [base, ours, theirs] =
            self.stopped()?.map(|commit| commit.iter().map(commit_id).collect());
        Ok(MergeCandidates { bases: vec![base], sides: vec![ours, theirs], shared: Vec::new() })
    }

    fn copy_candidates(&self) -> Result<Vec<CommitId>, VcsError> {
        Ok(self.head_id()?.iter().map(commit_id).collect())
    }

    fn working_copy_id(&self) -> Result<Option<CommitId>, VcsError> {
        Ok(None)
    }

    fn index_base(&self) -> Result<Option<CommitId>, VcsError> {
        Ok(self.head_id()?.as_ref().map(commit_id))
    }

    /// git takes the working tree in at `git add`, never by itself.
    fn snapshot(&mut self) -> Result<(), VcsError> {
        Ok(())
    }

    fn record_save(&mut self, path: &Utf8Path, resolved: bool) -> Result<(), VcsError> {
        check_inside(path)?;
        if resolved {
            self.stage_resolution(path)?;
        }
        Ok(())
    }

    fn conflict_style(&self) -> Result<Option<ConflictStyle>, VcsError> {
        let Some(value) = self.repo.config_snapshot().string(CONFLICT_STYLE_KEY) else {
            return Ok(None);
        };
        let bytes: &[u8] = &value;
        // git compares the value case-sensitively.
        match bytes {
            b"merge" => Ok(Some(ConflictStyle::Merge)),
            b"diff3" => Ok(Some(ConflictStyle::Diff3)),
            b"zdiff3" => Ok(Some(ConflictStyle::Zdiff3)),
            _ => Err(VcsError::BadConfig { key: CONFLICT_STYLE_KEY, value: value.to_string() }),
        }
    }
}
