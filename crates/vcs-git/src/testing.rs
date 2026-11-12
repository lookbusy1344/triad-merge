//! Repositories built in temporary directories through gix, for this crate's tests and other
//! crates' (`testing` feature). They panic on failure, as test code may.
#![expect(
    clippy::missing_panics_doc,
    clippy::expect_used,
    reason = "test support: each panic is a failed test setup, named by its message"
)]

use camino::{Utf8Path, Utf8PathBuf};
use gix::index::entry::Mode;
use gix::object::tree::EntryKind;
use gix::refs::Target;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit};
use tempfile::TempDir;
use vcs::testing::{Entry, SUBMODULE_COMMIT};

use crate::GitRepository;

/// The branch a new fixture is on, whatever the machine's `init.defaultBranch`.
const BRANCH: &str = "main";

/// The author and committer of every fixture commit, at the epoch.
fn signature() -> gix::actor::Signature {
    gix::actor::Signature {
        name: "triad".into(),
        email: "triad@example.com".into(),
        time: gix::date::Time::new(0, 0),
    }
}

/// A repository in a temporary directory, with commits made through gix.
pub struct Fixture {
    dir: TempDir,
    repo: gix::Repository,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    #[must_use]
    pub fn new() -> Self {
        Self::init(tempfile::tempdir().expect("a temporary directory"))
    }

    /// A repository in a new directory inside `parent`, such as another repository.
    #[must_use]
    pub fn new_in(parent: &Utf8Path) -> Self {
        Self::init(tempfile::tempdir_in(parent).expect("a directory inside the parent"))
    }

    fn init(dir: TempDir) -> Self {
        let repo = gix::init(dir.path()).expect("git init");
        repo.edit_reference(RefEdit {
            change: Change::Update {
                log: LogChange::default(),
                expected: PreviousValue::Any,
                new: Target::Symbolic(
                    format!("refs/heads/{BRANCH}").try_into().expect("a branch name"),
                ),
            },
            name: "HEAD".try_into().expect("HEAD"),
            deref: false,
        })
        .expect("HEAD points at the branch");
        Self { dir, repo }
    }

    pub fn root(&self) -> &Utf8Path {
        Utf8Path::from_path(self.dir.path()).expect("a UTF-8 temporary path")
    }

    /// Write `bytes` to `path` in the working copy, making its directories.
    pub fn write(&self, path: &str, bytes: &[u8]) -> Utf8PathBuf {
        let path = self.root().join(path);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("the directories are made");
        }
        std::fs::write(&path, bytes).expect("the file is written");
        path
    }

    fn tree(&self, files: &[(&str, &[u8])]) -> gix::ObjectId {
        self.tree_of(&Entry::files(files))
    }

    fn tree_of(&self, entries: &[(&str, Entry)]) -> gix::ObjectId {
        let mut editor = self.repo.edit_tree(self.repo.empty_tree().id).expect("a tree editor");
        for &(path, entry) in entries {
            let (kind, _, id) = self.object(entry);
            editor.upsert(path, kind, id).expect("an entry");
        }
        editor.write().expect("a tree").detach()
    }

    /// The tree entry kind, index mode and object of `entry`, writing its blob if it has one.
    fn object(&self, entry: Entry) -> (EntryKind, Mode, gix::ObjectId) {
        let blob = |bytes| self.repo.write_blob(bytes).expect("a blob").detach();
        match entry {
            Entry::File(bytes) => (EntryKind::Blob, Mode::FILE, blob(bytes)),
            Entry::Executable(bytes) => {
                (EntryKind::BlobExecutable, Mode::FILE_EXECUTABLE, blob(bytes))
            }
            Entry::Symlink(target) => (EntryKind::Link, Mode::SYMLINK, blob(target.as_bytes())),
            Entry::Submodule => (
                EntryKind::Commit,
                Mode::COMMIT,
                gix::ObjectId::from_hex(SUBMODULE_COMMIT.as_bytes()).expect("a commit ID in hex"),
            ),
        }
    }

    /// Commit `files` as the whole tree on `HEAD`.
    pub fn commit(&self, message: &str, files: &[(&str, &[u8])]) -> gix::ObjectId {
        self.commit_entries(message, &Entry::files(files))
    }

    /// Commit `entries` as the whole tree on `HEAD`.
    pub fn commit_entries(&self, message: &str, entries: &[(&str, Entry)]) -> gix::ObjectId {
        let parents: Vec<gix::ObjectId> =
            self.repo.head_id().map(gix::Id::detach).into_iter().collect();
        self.commit_tree(&parents, message, self.tree_of(entries))
    }

    /// Commit `files` as the whole tree on `parents` and move `HEAD`'s branch to it. The
    /// first parent is `HEAD`'s commit; more than one parent makes a merge.
    pub fn merge(
        &self,
        parents: &[gix::ObjectId],
        message: &str,
        files: &[(&str, &[u8])],
    ) -> gix::ObjectId {
        self.commit_tree(parents, message, self.tree(files))
    }

    /// Commit `tree` on `parents` and move `HEAD`'s branch to it.
    fn commit_tree(
        &self,
        parents: &[gix::ObjectId],
        message: &str,
        tree: gix::ObjectId,
    ) -> gix::ObjectId {
        let mut time = gix::date::parse::TimeBuf::default();
        let signature = signature();
        let signature = signature.to_ref(&mut time);
        self.repo
            .commit_as(signature, signature, "HEAD", message, tree, parents.iter().copied())
            .expect("a commit")
            .detach()
    }

    /// Commit `files` as the whole tree on `parents`, moving no branch.
    pub fn commit_on(
        &self,
        parents: &[gix::ObjectId],
        message: &str,
        files: &[(&str, &[u8])],
    ) -> gix::ObjectId {
        let tree = self.tree(files);
        let mut time = gix::date::parse::TimeBuf::default();
        let signature = signature();
        let signature = signature.to_ref(&mut time);
        self.repo
            .new_commit_as(signature, signature, message, tree, parents.iter().copied())
            .expect("a commit")
            .id
    }

    /// Make the index hold `files` as its whole content.
    pub fn stage(&self, files: &[(&str, &[u8])]) {
        let tree = self.tree(files);
        let mut index = self.repo.index_from_tree(&tree).expect("an index");
        index.write(gix::index::write::Options::default()).expect("the index is written");
    }

    /// Make the index hold `clean` at stage 0 and each of `conflicts` as git leaves it when a
    /// merge stops: its base, ours and theirs at stages 1, 2 and 3. An absent term has no
    /// entry.
    pub fn stage_conflicts(
        &self,
        clean: &[(&str, &[u8])],
        conflicts: &[(&str, [Option<Entry>; 3])],
    ) {
        use gix::index::entry::{Flags, Stage, Stat};
        const STAGES: [Stage; 3] = [Stage::Base, Stage::Ours, Stage::Theirs];
        let tree = self.tree(clean);
        let mut index = self.repo.index_from_tree(&tree).expect("an index");
        for &(path, terms) in conflicts {
            for (stage, entry) in STAGES.into_iter().zip(terms) {
                if let Some(entry) = entry {
                    let (_, mode, id) = self.object(entry);
                    index.dangerously_push_entry(
                        Stat::default(),
                        id,
                        Flags::from_stage(stage),
                        mode,
                        path.into(),
                    );
                }
            }
        }
        index.sort_entries();
        index.write(gix::index::write::Options::default()).expect("the index is written");
    }

    /// Point the pseudo-ref `name` (`MERGE_HEAD`, `CHERRY_PICK_HEAD`, …) at `id`, as git does
    /// when a merge, cherry-pick, revert or rebase stops on a conflict.
    pub fn write_pseudo_ref(&self, name: &str, id: gix::ObjectId) {
        std::fs::write(self.repo.path().join(name), format!("{id}\n"))
            .expect("the pseudo-ref is written");
    }

    /// Append `text`, in git's config format, to the repository's own config file.
    pub fn append_config(&self, text: &str) {
        let path = self.repo.path().join("config");
        let mut config = std::fs::read_to_string(&path).expect("the config is read");
        config.push_str(text);
        std::fs::write(&path, config).expect("the config is written");
    }

    pub fn open(&self) -> GitRepository {
        GitRepository::discover(self.root()).expect("the repository is found")
    }
}
