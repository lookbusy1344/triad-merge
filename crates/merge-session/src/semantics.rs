//! Choosing git's or jj's merge semantics for a merge (spec §6.1,
//! `docs/DESIGN.md` §B3).
//!
//! A repository with `.jj`, colocated or not, merges with jj's semantics, and a git merge there
//! is an error. `auto` takes the nearest repository of the working directory, then of the
//! output's directory. `inspect`, which has no output, probes the working directory only.

use std::path::{Path, PathBuf};

use merge_config::settings::{Choice, ConflictStyle};
use merge_core::document::MarkerStyle;
use merge_core::merge3::{HunkLevel, SameChange, Semantics};
use vcs::{ConflictStyle as GitStyle, RepoKind};

use crate::native::Repo;
#[cfg(test)]
use crate::native::{GIT_DIR, JJ_DIR, nearest_repo};
use crate::repository::{RunRepository, output_dir};

/// The merge a run makes: whose semantics, and the rules jj's config also sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub semantics: Semantics,
    pub same_change: SameChange,
    pub hunk_level: HunkLevel,
}

/// jj writes a merge tool's files to a temporary directory whose name starts with this
/// (jj-cli 0.47 `merge_tools/external.rs`).
pub const JJ_RESOLVE_PREFIX: &str = "jj-resolve-";
/// What the paths of a merge say about who started it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The working directory's repository, or else the output's.
    pub repo: Option<Repo>,
    /// The output's directory, when it is jj's `jj-resolve-` directory.
    pub jj_resolve: Option<PathBuf>,
}

impl Caller {
    /// The caller of a run in the repository `repo`, writing `output` from `cwd`. A relative
    /// `output` lies under `cwd`.
    #[must_use]
    pub fn new(repo: &RunRepository, cwd: Option<&Path>, output: Option<&Path>) -> Self {
        let jj_resolve = output_dir(cwd, output).filter(|dir| {
            dir.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(JJ_RESOLVE_PREFIX))
        });
        Self { repo: repo.found().cloned(), jj_resolve }
    }

    /// The caller of a run in `cwd` writing `output`, in the repository
    /// [`RunRepository::of_merge`] finds.
    #[must_use]
    pub fn find(cwd: Option<&Path>, output: Option<&Path>) -> Self {
        Self::new(&RunRepository::of_merge(cwd, output), cwd, output)
    }

    /// The path that names a merge writing `output` from one run to the next, for the recovery
    /// journal: `output`. jj's output lies in a new temporary directory each run, so there the
    /// name is the file `title` names, from the repository's root.
    #[must_use]
    pub fn journal_name(&self, output: &Path, title: Option<&str>) -> PathBuf {
        match (&self.jj_resolve, title) {
            (Some(_), Some(title)) => {
                self.jj_root().map_or_else(|| PathBuf::from(title), |root| root.join(title))
            }
            _ => output.to_owned(),
        }
    }

    /// The jj repository's root, when the caller's repository is jj's.
    #[must_use]
    pub fn jj_root(&self) -> Option<&Path> {
        self.repo.as_ref().filter(|repo| repo.kind == RepoKind::Jj).map(|repo| repo.root.as_path())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SemanticsError {
    #[error(
        "{} is a jj repository, where triad merges with jj's semantics only; resolve with `jj resolve`",
        .0.display()
    )]
    GitInJjRepository(PathBuf),
    #[error(
        "{} is jj's resolve directory, where triad merges with jj's semantics only; resolve with `jj resolve`",
        .0.display()
    )]
    GitInJjResolve(PathBuf),
}

/// The semantics `choice` gives for `caller`.
///
/// # Errors
///
/// [`SemanticsError`] for `git` when the caller is jj: its repository is a jj repository, or
/// the file lies in jj's resolve directory.
pub fn choose(choice: Choice, caller: &Caller) -> Result<Semantics, SemanticsError> {
    let jj = caller.jj_root().is_some() || caller.jj_resolve.is_some();
    match choice {
        Choice::Jj => Ok(Semantics::Jj),
        Choice::Auto if jj => Ok(Semantics::Jj),
        Choice::Auto => Ok(Semantics::Git),
        Choice::Git => match (caller.jj_root(), &caller.jj_resolve) {
            (Some(root), _) => Err(SemanticsError::GitInJjRepository(root.to_owned())),
            (None, Some(dir)) => Err(SemanticsError::GitInJjResolve(dir.clone())),
            (None, None) => Ok(Semantics::Git),
        },
    }
}

/// The markers of a merge under `semantics` (spec §6.4): diff3 under jj's semantics, which
/// write the style `install --jj` configures; under git's, `setting`, else git's
/// `merge.conflictStyle` as `git_style` reads it, else diff3. git's `zdiff3` reads as diff3.
/// A value git does not know is the caller's to log. `git_style` runs only when the style
/// depends on it.
pub fn marker_style(
    semantics: Semantics,
    setting: Option<ConflictStyle>,
    git_style: impl FnOnce() -> Option<GitStyle>,
) -> MarkerStyle {
    match (semantics, setting) {
        (Semantics::Jj, _) => MarkerStyle::Diff3,
        (Semantics::Git, Some(style)) => style.into(),
        (Semantics::Git, None) => match git_style() {
            Some(GitStyle::Merge) => MarkerStyle::Merge,
            Some(GitStyle::Diff3 | GitStyle::Zdiff3) | None => MarkerStyle::Diff3,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn jj_semantics_write_diff3_whatever_the_setting_and_git_say() {
        for setting in [None, Some(ConflictStyle::Merge), Some(ConflictStyle::Diff3)] {
            let style = marker_style(Semantics::Jj, setting, || {
                panic!("git's config is not read under jj semantics")
            });
            assert_eq!(style, MarkerStyle::Diff3, "{setting:?}");
        }
    }

    #[test]
    fn under_git_semantics_the_setting_wins_without_reading_gits_config() {
        let unread = || -> Option<GitStyle> { panic!("git's config is not read") };
        assert_eq!(
            marker_style(Semantics::Git, Some(ConflictStyle::Merge), unread),
            MarkerStyle::Merge
        );
        assert_eq!(
            marker_style(Semantics::Git, Some(ConflictStyle::Diff3), unread),
            MarkerStyle::Diff3
        );
    }

    #[test]
    fn without_a_setting_gits_merge_style_alone_drops_the_base() {
        for (git, style) in [
            (Some(GitStyle::Merge), MarkerStyle::Merge),
            (Some(GitStyle::Diff3), MarkerStyle::Diff3),
            (Some(GitStyle::Zdiff3), MarkerStyle::Diff3),
            (None, MarkerStyle::Diff3),
        ] {
            assert_eq!(marker_style(Semantics::Git, None, || git), style, "{git:?}");
        }
    }

    #[test]
    fn a_plain_output_names_its_own_journal() {
        let dir = tree(&[("repo", ".git")]);
        let output = dir.path().join("repo/a.txt");
        let caller = Caller::find(Some(&dir.path().join("repo")), Some(&output));
        assert_eq!(caller.journal_name(&output, Some("a.txt")), output);
    }

    #[test]
    fn a_jj_output_with_no_resolve_directory_names_its_own_journal() {
        let dir = tree(&[("repo", ".jj")]);
        let output = dir.path().join("repo/a.txt");
        let caller = Caller::find(Some(&dir.path().join("repo")), Some(&output));
        assert_eq!(caller.journal_name(&output, Some("a.txt")), output);
    }

    #[test]
    fn jj_resolves_of_one_file_share_a_journal_name_across_temporary_directories() {
        let dir = tree(&[("repo/sub", ".jj")]);
        let root = dir.path().join("repo/sub");
        let name = |temp: &str| {
            let output = dir.path().join(temp).join("output");
            Caller::find(Some(&root), Some(&output)).journal_name(&output, Some("src/a.txt"))
        };
        assert_eq!(name("jj-resolve-AAAA"), canonical(&root).join("src/a.txt"));
        assert_eq!(name("jj-resolve-BBBB"), canonical(&root).join("src/a.txt"));
    }

    #[test]
    fn jj_resolves_in_two_files_keep_two_journal_names() {
        let dir = tree(&[("repo", ".jj")]);
        let root = dir.path().join("repo");
        let output = dir.path().join("jj-resolve-AAAA/output");
        let caller = Caller::find(Some(&root), Some(&output));
        assert_ne!(
            caller.journal_name(&output, Some("a.txt")),
            caller.journal_name(&output, Some("b.txt"))
        );
    }

    #[test]
    fn a_jj_resolve_from_a_subdirectory_keeps_the_roots_journal_name() {
        let dir = tree(&[("repo", ".jj")]);
        let root = dir.path().join("repo");
        let below = root.join("deep");
        fs::create_dir_all(&below).unwrap();
        let output = dir.path().join("jj-resolve-AAAA/output");
        let caller = Caller::find(Some(&below), Some(&output));
        assert_eq!(caller.journal_name(&output, Some("a.txt")), canonical(&root).join("a.txt"));
    }

    #[test]
    fn a_jj_resolve_outside_any_repository_is_named_by_its_title() {
        let dir = tree(&[]);
        let output = dir.path().join("jj-resolve-AAAA/output");
        let caller = Caller::find(Some(dir.path()), Some(&output));
        assert_eq!(caller.journal_name(&output, Some("a.txt")), Path::new("a.txt"));
    }

    #[test]
    fn a_jj_resolve_without_a_title_keeps_its_output() {
        let dir = tree(&[("repo", ".jj")]);
        let output = dir.path().join("jj-resolve-AAAA/output");
        let caller = Caller::find(Some(&dir.path().join("repo")), Some(&output));
        assert_eq!(caller.journal_name(&output, None), output);
    }

    /// A directory tree with `.jj` and `.git` where named.
    fn tree(markers: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, marker) in markers {
            // A run's repository is a `.git` git opens; jj's is any `.jj` directory.
            if *marker == GIT_DIR {
                gix::init(dir.path().join(path)).unwrap();
            } else {
                fs::create_dir_all(dir.path().join(path).join(marker)).unwrap();
            }
        }
        dir
    }

    fn repo(kind: RepoKind, root: &Path) -> Repo {
        Repo { kind, root: root.to_owned() }
    }

    /// `path` made canonical, the form a run's repository root takes.
    fn canonical(path: &Path) -> PathBuf {
        dunce::canonicalize(path).unwrap()
    }

    /// The repository a caller finds at `root`: its root is canonical.
    fn found(kind: RepoKind, root: &Path) -> Repo {
        repo(kind, &canonical(root))
    }

    #[test]
    fn the_nearest_marker_names_the_repository() {
        let t = tree(&[("jj", JJ_DIR), ("git", GIT_DIR), ("both", JJ_DIR), ("both", GIT_DIR)]);
        let root = t.path();
        fs::create_dir_all(root.join("jj/sub/deep")).unwrap();
        assert_eq!(
            nearest_repo(&root.join("jj/sub/deep")),
            Some(repo(RepoKind::Jj, &root.join("jj")))
        );
        assert_eq!(nearest_repo(&root.join("git")), Some(repo(RepoKind::Git, &root.join("git"))));
        assert_eq!(nearest_repo(&root.join("both")), Some(repo(RepoKind::Jj, &root.join("both"))));
        assert_eq!(nearest_repo(root), None);
    }

    #[test]
    fn a_git_file_marks_a_git_worktree() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join(GIT_DIR), "gitdir: elsewhere\n").unwrap();
        assert_eq!(nearest_repo(t.path()), Some(repo(RepoKind::Git, t.path())));
    }

    #[test]
    fn a_jj_file_is_not_a_jj_repository() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join(JJ_DIR), "").unwrap();
        assert_eq!(nearest_repo(t.path()), None);
    }

    #[test]
    fn the_nearer_repository_wins_when_one_nests_in_another() {
        let t = tree(&[("", JJ_DIR), ("inner", GIT_DIR), ("inner/jj", JJ_DIR)]);
        let root = t.path();
        assert_eq!(
            nearest_repo(&root.join("inner")),
            Some(repo(RepoKind::Git, &root.join("inner")))
        );
        assert_eq!(
            nearest_repo(&root.join("inner/jj")),
            Some(repo(RepoKind::Jj, &root.join("inner/jj")))
        );
    }

    #[test]
    fn the_working_directory_wins_over_the_files_directory() {
        let t = tree(&[("git", GIT_DIR), ("jj", JJ_DIR)]);
        let root = t.path();
        let caller = Caller::find(Some(&root.join("git")), Some(&root.join("jj/out.txt")));
        assert_eq!(caller.repo, Some(found(RepoKind::Git, &root.join("git"))));
        assert_eq!(choose(Choice::Auto, &caller), Ok(Semantics::Git));
    }

    #[test]
    fn a_caller_takes_the_runs_repository_not_its_paths() {
        let t = tree(&[("git", GIT_DIR), ("jj", JJ_DIR)]);
        let root = t.path();
        let (git, output) = (root.join("git"), root.join("git/out.txt"));
        let run = RunRepository::find([root.join("jj").as_path()]);
        let caller = Caller::new(&run, Some(&git), Some(&output));
        assert_eq!(caller.repo, Some(found(RepoKind::Jj, &root.join("jj"))));
        assert_eq!(choose(Choice::Auto, &caller), Ok(Semantics::Jj));
        let outside = Caller::new(&RunRepository::none(), Some(&git), Some(&output));
        assert_eq!(outside.repo, None);
        assert_eq!(choose(Choice::Auto, &outside), Ok(Semantics::Git));
    }

    #[test]
    fn the_files_directory_counts_when_the_working_directory_has_no_repository() {
        let t = tree(&[("jj", JJ_DIR)]);
        let root = t.path();
        let caller = Caller::find(Some(root), Some(&root.join("jj/out.txt")));
        assert_eq!(caller.repo, Some(found(RepoKind::Jj, &root.join("jj"))));
        assert_eq!(choose(Choice::Auto, &caller), Ok(Semantics::Jj));
    }

    #[test]
    fn a_relative_file_lies_under_the_working_directory() {
        let t = tree(&[("jj", JJ_DIR)]);
        let root = t.path();
        fs::create_dir_all(root.join("plain")).unwrap();
        let caller = Caller::find(None, Some(&root.join("jj/out.txt")));
        assert_eq!(caller.repo, Some(found(RepoKind::Jj, &root.join("jj"))));
        let relative = Caller::find(Some(&root.join("plain")), Some(Path::new("../jj/out.txt")));
        assert_eq!(relative.repo.map(|r| r.kind), Some(RepoKind::Jj), "found through plain/../jj");
    }

    #[test]
    fn auto_is_git_outside_any_repository() {
        let t = tempfile::tempdir().unwrap();
        let caller = Caller::find(Some(t.path()), Some(&t.path().join("out.txt")));
        assert_eq!(caller, Caller { repo: None, jj_resolve: None });
        assert_eq!(choose(Choice::Auto, &caller), Ok(Semantics::Git));
        assert_eq!(choose(Choice::Git, &caller), Ok(Semantics::Git));
        assert_eq!(choose(Choice::Jj, &caller), Ok(Semantics::Jj));
    }

    #[test]
    fn jjs_resolve_directory_marks_jj_from_any_working_directory() {
        let t = tree(&[("git", GIT_DIR)]);
        let root = t.path();
        let resolve = root.join("jj-resolve-rUnIyZ");
        fs::create_dir(&resolve).unwrap();
        let output = resolve.join("output_f.txt");
        for cwd in [root.to_owned(), root.join("git")] {
            let caller = Caller::find(Some(&cwd), Some(&output));
            assert_eq!(caller.jj_resolve.as_deref(), Some(resolve.as_path()), "{cwd:?}");
            assert_eq!(choose(Choice::Auto, &caller), Ok(Semantics::Jj), "{cwd:?}");
            assert_eq!(
                choose(Choice::Git, &caller),
                Err(SemanticsError::GitInJjResolve(resolve.clone())),
                "{cwd:?}"
            );
        }
    }

    #[test]
    fn without_an_output_only_the_working_directory_counts() {
        let t = tree(&[("jj", JJ_DIR)]);
        let root = t.path().join("jj");
        assert_eq!(Caller::find(Some(&root), None).repo, Some(found(RepoKind::Jj, &root)));
        assert_eq!(Caller::find(Some(t.path()), None), Caller { repo: None, jj_resolve: None });
    }

    #[test]
    fn a_name_that_only_contains_the_prefix_is_not_jjs_resolve_directory() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("my-jj-resolve-x");
        let caller = Caller::find(Some(t.path()), Some(&dir.join("out.txt")));
        assert_eq!(caller.jj_resolve, None);
    }

    #[test]
    fn git_semantics_in_a_jj_repository_is_an_error_naming_it() {
        for markers in [&[("r", JJ_DIR)][..], &[("r", JJ_DIR), ("r", GIT_DIR)]] {
            let t = tree(markers);
            let root = t.path().join("r");
            let caller = Caller::find(Some(&root), Some(&root.join("out.txt")));
            assert_eq!(
                choose(Choice::Git, &caller),
                Err(SemanticsError::GitInJjRepository(canonical(&root)))
            );
            assert_eq!(choose(Choice::Auto, &caller), Ok(Semantics::Jj));
        }
    }

    #[test]
    fn the_error_message_names_the_repository_and_jj_resolve() {
        let message = SemanticsError::GitInJjRepository("/work/repo".into()).to_string();
        assert_eq!(
            message,
            "/work/repo is a jj repository, where triad merges with jj's semantics only; \
             resolve with `jj resolve`"
        );
    }
}
