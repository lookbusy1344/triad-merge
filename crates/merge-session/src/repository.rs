//! The repository a run works in: found once, opened when a step first reads it, and kept
//! for the steps after (spec §6.1). Semantics, marker style, `resolve` and the line origins
//! of one run read the same open repository.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use camino::Utf8PathBuf;
use vcs::{RepoKind, Repository, VcsError};
use vcs_jj::config::ConfigEnv;

use crate::native::{self, Repo};
use crate::resolve;

/// A run's repository: the one holding the first of the run's directories that lies in one
/// its backend can open.
///
/// Two runs that found the same repository can still differ in whether it is open and which
/// jj config their snapshots read, so a run has no equality:
///
/// ```compile_fail,E0369
/// # use merge_session::repository::RunRepository;
/// let _ = RunRepository::none() == RunRepository::none();
/// ```
pub struct RunRepository {
    /// The directories probed, in order: the first is named when there is no repository, and
    /// a [`Handoff`] finds the repository again from them.
    dirs: Vec<PathBuf>,
    found: Option<Repo>,
    open: Option<Box<dyn Repository>>,
    /// Where the user's jj config lives, for jj's snapshots; the process's when unset.
    jj_config: Option<ConfigEnv>,
}

impl RunRepository {
    /// The repository `dirs` lie in, as [`native::discover`] finds each: from the directory
    /// made canonical, so a directory that does not exist, or the empty path, lies in none.
    /// A `.git` git cannot open names no repository, so a git repository opens here, where
    /// opening is cheap; jj's opens on the first read. What the run reads before opening, the
    /// kind and the root, is then what opens.
    #[must_use]
    pub fn find<'a>(dirs: impl IntoIterator<Item = &'a Path>) -> Self {
        let dirs: Vec<PathBuf> = dirs.into_iter().map(Path::to_path_buf).collect();
        let mut open = None;
        let found = dirs
            .iter()
            .filter_map(|dir| native::find_from(dir))
            .find(|repo| match repo.kind {
                RepoKind::Jj => true,
                RepoKind::Git => match native::open(repo, None) {
                    Ok(repo) => {
                        open = Some(repo);
                        true
                    }
                    Err(VcsError::NotARepository(_)) => false,
                    // Still the run's repository: the step that opens it reports the error.
                    Err(error) => {
                        tracing::debug!(%error, root = %repo.root.display(), "git cannot open the repository yet");
                        true
                    }
                },
            });
        Self { dirs, found, open, jj_config: None }
    }

    /// The run's repository, its jj snapshots reading the user's jj config from `config`.
    #[must_use]
    pub fn with_jj_config(self, config: ConfigEnv) -> Self {
        Self { jj_config: Some(config), ..self }
    }

    /// Where the run's jj snapshots read the user's jj config; the process's when `None`.
    #[must_use]
    pub fn jj_config(&self) -> Option<&ConfigEnv> {
        self.jj_config.as_ref()
    }

    /// The repository of a run in `cwd` writing `output`: the working directory's, else the
    /// output's (spec §6.1, §7.3). The merge semantics, the marker style and the line origins
    /// all take it. A relative `output` lies under `cwd`; without one, only `cwd` counts.
    #[must_use]
    pub fn of_merge(cwd: Option<&Path>, output: Option<&Path>) -> Self {
        let dir = output_dir(cwd, output);
        Self::find([cwd, dir.as_deref()].into_iter().flatten())
    }

    /// The repository of `diff LEFT RIGHT`: the working directory's, else the right side's.
    #[must_use]
    pub fn of_diff_tool(cwd: Option<&Path>, right: &Path) -> Self {
        Self::of_merge(cwd, Some(right))
    }

    /// The repository holding the existing file `path`: that of `diff PATH`.
    #[must_use]
    pub fn of_file(path: &Path) -> Self {
        let file = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        Self::find(file.parent())
    }

    /// The repository of `resolve [PATH]`: the working directory's, else, given a path
    /// relative to `cwd`, the one holding that file, which may not exist.
    #[must_use]
    pub fn of_resolve(cwd: &Path, path: Option<&Path>) -> Self {
        // A path that cannot be made absolute holds no repository; loading it reports why.
        let file = path.and_then(|path| resolve::absolute(&cwd.join(path)).ok());
        let dir = file.as_deref().map(|file| resolve::nearest_dir(file).as_std_path());
        Self::find([Some(cwd), dir].into_iter().flatten())
    }

    /// No repository: a run outside every one.
    #[must_use]
    pub fn none() -> Self {
        Self::find([])
    }

    /// The repository found, which [`RunRepository::open`] opens.
    #[must_use]
    pub fn found(&self) -> Option<&Repo> {
        self.found.as_ref()
    }

    /// The kind of the repository found, without opening it.
    #[must_use]
    pub fn kind(&self) -> Option<RepoKind> {
        self.found().map(|repo| repo.kind)
    }

    /// The repository, opened on the first call and kept. `None` outside every one.
    ///
    /// # Errors
    ///
    /// Any error opening the repository.
    pub fn open(&mut self) -> Result<Option<&dyn Repository>, VcsError> {
        if self.open.is_none()
            && let Some(repo) = &self.found
        {
            self.open = Some(native::open(repo, self.jj_config.as_ref())?);
        }
        Ok(self.open.as_deref())
    }

    /// [`RunRepository::open`], for a step that writes to the repository.
    ///
    /// # Errors
    ///
    /// As [`RunRepository::open`].
    pub fn open_mut(&mut self) -> Result<Option<&mut (dyn Repository + 'static)>, VcsError> {
        self.open()?;
        Ok(self.open.as_deref_mut())
    }

    /// [`RunRepository::open`], for a step that needs a repository.
    ///
    /// # Errors
    ///
    /// [`VcsError::NotARepository`] naming the first directory probed when none holds one,
    /// and the errors of [`RunRepository::open`].
    pub fn require(&mut self) -> Result<&dyn Repository, VcsError> {
        let missing = self.missing();
        self.open()?.ok_or_else(missing)
    }

    /// [`RunRepository::require`], for a step that writes to the repository.
    ///
    /// # Errors
    ///
    /// As [`RunRepository::require`].
    pub fn require_mut(&mut self) -> Result<&mut (dyn Repository + 'static), VcsError> {
        let missing = self.missing();
        self.open_mut()?.ok_or_else(missing)
    }

    /// The error for a run that found no repository: [`VcsError::NotARepository`] naming the
    /// first directory probed.
    fn missing(&self) -> impl FnOnce() -> VcsError + use<> {
        let first = self.dirs.first().cloned().unwrap_or_default();
        move || match Utf8PathBuf::from_path_buf(first) {
            Ok(first) => VcsError::NotARepository(first),
            Err(first) => VcsError::NotUtf8(first),
        }
    }

    /// The open repository, opened now if not yet, for the step that keeps it.
    ///
    /// # Errors
    ///
    /// As [`RunRepository::open`].
    pub fn into_open(mut self) -> Result<Option<Box<dyn Repository>>, VcsError> {
        self.open()?;
        Ok(self.open)
    }
}

/// The directory of `output`, under `cwd` when `output` is relative.
pub(crate) fn output_dir(cwd: Option<&Path>, output: Option<&Path>) -> Option<PathBuf> {
    let parent = output?.parent()?;
    Some(match cwd {
        Some(cwd) if parent.is_relative() => cwd.join(parent),
        _ => parent.to_owned(),
    })
}

impl fmt::Debug for RunRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunRepository")
            .field("dirs", &self.dirs)
            .field("found", &self.found)
            .field("open", &self.open.is_some())
            .field("jj_config", &self.jj_config.is_some())
            .finish()
    }
}

/// Two runs share a repository when they probed the same directories and found the same
/// repository, open or not.
/// A run's repository on its way from the command line to the window that reads line origins
/// from it, across threads. Its clones share one slot: the first take gets the repository the
/// command line found, open or not; a take after that finds it again from the same
/// directories, by the same rule, reading the same jj config.
#[derive(Clone)]
pub struct Handoff {
    dirs: Arc<[PathBuf]>,
    /// The run's jj config, which a repository found again reads as the run's did.
    jj_config: Option<Arc<ConfigEnv>>,
    slot: Arc<Mutex<Option<RunRepository>>>,
}

impl Handoff {
    /// A slot holding `repo`.
    #[must_use]
    pub fn new(repo: RunRepository) -> Self {
        Self {
            dirs: Arc::from(repo.dirs.as_slice()),
            jj_config: repo.jj_config.clone().map(Arc::new),
            slot: Arc::new(Mutex::new(Some(repo))),
        }
    }

    /// The repository, leaving the slot empty; found again from the run's directories when
    /// taken already.
    #[must_use]
    pub fn take(&self) -> RunRepository {
        // A panic while the lock was held could only interrupt an `Option` move, which
        // leaves the slot whole.
        let taken = self.slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        taken.unwrap_or_else(|| {
            let again = RunRepository::find(self.dirs.iter().map(PathBuf::as_path));
            match &self.jj_config {
                Some(config) => again.with_jj_config(ConfigEnv::clone(config)),
                None => again,
            }
        })
    }

    /// Whether a take has emptied the slot.
    #[must_use]
    pub fn is_taken(&self) -> bool {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner).is_none()
    }
}

impl fmt::Debug for Handoff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        f.debug_struct("Handoff")
            .field("dirs", &self.dirs)
            .field("jj_config", &self.jj_config.is_some())
            .field("slot", &*slot)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical(path: &camino::Utf8Path) -> PathBuf {
        dunce::canonicalize(path).unwrap()
    }

    fn root(repo: &dyn Repository) -> PathBuf {
        repo.root().as_std_path().to_owned()
    }

    #[test]
    fn opening_twice_reads_the_same_open_repository() {
        let fixture = vcs_git::testing::Fixture::new();
        let mut run = RunRepository::find([fixture.root().as_std_path()]);
        let address = |repo: &dyn Repository| std::ptr::from_ref(repo).cast::<u8>();
        let first = address(run.open().unwrap().expect("a repository"));
        let second = address(run.open().unwrap().expect("a repository"));
        assert_eq!(first, second, "opened once");
        let kept = run.into_open().unwrap().expect("a repository");
        assert_eq!(first, address(kept.as_ref()), "handed on, not reopened");
    }

    #[test]
    fn the_first_directory_in_a_repository_wins() {
        let git = vcs_git::testing::Fixture::new();
        let jj = vcs_jj::testing::Fixture::new();
        let outside = tempfile::tempdir().unwrap();
        let mut run = RunRepository::find([jj.root().as_std_path(), git.root().as_std_path()]);
        assert_eq!(run.kind(), Some(RepoKind::Jj));
        assert_eq!(root(run.require().unwrap()), canonical(jj.root()));
        let mut run = RunRepository::find([outside.path(), git.root().as_std_path()]);
        assert_eq!(run.kind(), Some(RepoKind::Git));
        assert_eq!(root(run.require().unwrap()), canonical(git.root()));
    }

    #[test]
    fn a_merge_takes_the_working_directorys_repository_else_the_outputs() {
        let git = vcs_git::testing::Fixture::new();
        let jj = vcs_jj::testing::Fixture::new();
        let output = jj.root().join("out.txt");
        let mut run =
            RunRepository::of_merge(Some(git.root().as_std_path()), Some(output.as_std_path()));
        assert_eq!(root(run.require().unwrap()), canonical(git.root()));
        let outside = tempfile::tempdir().unwrap();
        let mut run = RunRepository::of_merge(Some(outside.path()), Some(output.as_std_path()));
        assert_eq!(root(run.require().unwrap()), canonical(jj.root()));
        // A relative output lies under the working directory.
        let mut run =
            RunRepository::of_merge(Some(jj.root().as_std_path()), Some(Path::new("sub/out.txt")));
        assert_eq!(root(run.require().unwrap()), canonical(jj.root()));
    }

    #[test]
    fn a_file_takes_its_own_repository() {
        let git = vcs_git::testing::Fixture::new();
        let file = git.write("src/a.txt", b"a\n");
        let mut run = RunRepository::of_file(file.as_std_path());
        assert_eq!(root(run.require().unwrap()), canonical(git.root()));
    }

    #[test]
    fn resolve_takes_the_working_directorys_repository_else_the_files() {
        let git = vcs_git::testing::Fixture::new();
        let jj = vcs_jj::testing::Fixture::new();
        let outside = tempfile::tempdir().unwrap();
        let mut run = RunRepository::of_resolve(jj.root().as_std_path(), Some(Path::new("a.txt")));
        assert_eq!(root(run.require().unwrap()), canonical(jj.root()));
        // A file deleted in the working tree lies in the directory that holds it.
        let deleted = git.root().join("gone/deeper/a.txt");
        let mut run = RunRepository::of_resolve(outside.path(), Some(deleted.as_std_path()));
        assert_eq!(run.kind(), Some(RepoKind::Git));
        assert_eq!(root(run.require().unwrap()), canonical(git.root()));
        // Without a path, only the working directory counts.
        let mut run = RunRepository::of_resolve(outside.path(), None);
        assert!(run.open().unwrap().is_none());
    }

    #[test]
    fn a_handoff_gives_its_open_repository_once_then_finds_it_by_the_same_rule() {
        let git = vcs_git::testing::Fixture::new();
        let outside = tempfile::tempdir().unwrap();
        // `resolve` of a file deleted in the working tree: the repository holding its nearest
        // existing directory, which the file's own directory, missing, does not name.
        let deleted = git.root().join("gone/deeper/a.txt");
        let mut run = RunRepository::of_resolve(outside.path(), Some(deleted.as_std_path()));
        let address = std::ptr::from_ref(run.require().unwrap()).cast::<u8>();
        let handoff = Handoff::new(run);
        let window = handoff.clone();
        let taken = std::thread::scope(|scope| scope.spawn(move || window.take()).join().unwrap());
        let kept = taken.into_open().unwrap().expect("a repository");
        assert_eq!(address, std::ptr::from_ref(kept.as_ref()).cast::<u8>(), "the open one");
        assert!(handoff.is_taken());
        let mut again = handoff.take();
        assert_eq!(again.kind(), Some(RepoKind::Git), "found again once taken");
        assert_eq!(root(again.require().unwrap()), canonical(git.root()));
        assert_ne!(
            address,
            std::ptr::from_ref(again.require().unwrap()).cast::<u8>(),
            "opened afresh"
        );
        assert!(!Handoff::new(RunRepository::none()).is_taken());
    }

    #[test]
    fn a_handoff_found_again_reads_the_runs_jj_config() {
        let jj = vcs_jj::testing::Fixture::new();
        let config =
            ConfigEnv { hostname: Some("handoff-host".to_owned()), ..ConfigEnv::default() };
        let run = RunRepository::find([jj.root().as_std_path()]).with_jj_config(config);
        let handoff = Handoff::new(run);
        let host = |run: &RunRepository| run.jj_config().and_then(|c| c.hostname.clone());
        assert_eq!(host(&handoff.take()).as_deref(), Some("handoff-host"), "the run itself");
        assert_eq!(host(&handoff.take()).as_deref(), Some("handoff-host"), "found again");
    }

    #[test]
    fn outside_every_repository_there_is_none() {
        let outside = tempfile::tempdir().unwrap();
        let missing = outside.path().join("missing");
        // The process runs inside a repository; the empty path still names no directory.
        let mut run = RunRepository::find([outside.path(), &missing, Path::new("")]);
        assert_eq!(run.kind(), None);
        assert!(run.open().unwrap().is_none());
        match run.require() {
            Err(VcsError::NotARepository(path)) => assert_eq!(path.as_std_path(), outside.path()),
            other => panic!("expected NotARepository, got {:?}", other.map(root)),
        }
        assert!(RunRepository::none().into_open().unwrap().is_none());
    }

    #[test]
    fn a_git_entry_git_cannot_open_gives_way_to_the_next_directory() {
        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join(".git"), "not a gitdir line\n").unwrap();
        let git = vcs_git::testing::Fixture::new();
        let mut run = RunRepository::find([broken.path(), git.root().as_std_path()]);
        assert_eq!(root(run.require().unwrap()), canonical(git.root()));
        let mut alone = RunRepository::find([broken.path()]);
        assert!(alone.open().unwrap().is_none());
    }

    #[test]
    fn a_git_entry_git_cannot_open_is_not_found_before_any_step_opens() {
        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join(".git"), "not a gitdir line\n").unwrap();
        let jj = vcs_jj::testing::Fixture::new();
        let mut run = RunRepository::find([broken.path(), jj.root().as_std_path()]);
        // The kind and root the semantics and journal read agree with the one that opens.
        assert_eq!(run.kind(), Some(RepoKind::Jj));
        assert_eq!(run.found().map(|repo| repo.root.clone()), Some(canonical(jj.root())));
        assert_eq!(root(run.require().unwrap()), canonical(jj.root()));
        assert_eq!(run.kind(), Some(RepoKind::Jj), "unchanged by opening");
        assert_eq!(RunRepository::find([broken.path()]).kind(), None);
    }
}
