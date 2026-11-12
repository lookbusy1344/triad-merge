//! One rule finds a repository (`docs/DESIGN.md` §A13.7): the nearest directory holding
//! `.jj` or `.git`, jj when it holds both. The probe that picks the merge semantics and the
//! discovery that opens the repository give the same answer.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use merge_session::native;
use merge_session::semantics::Caller;
use vcs::{RepoKind, VcsError};

fn canonical(path: &Utf8Path) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dunce::canonicalize(path).expect("the path exists")).expect("UTF-8")
}

/// A subdirectory of `root`, made.
fn sub(root: &Utf8Path) -> Utf8PathBuf {
    let sub = root.join("src/deep");
    fs::create_dir_all(&sub).expect("the directories are made");
    sub
}

/// The probe's and discovery's kind and canonical root from `dir`, after checking they agree.
fn found(dir: &Utf8Path) -> (RepoKind, Utf8PathBuf) {
    let probed =
        Caller::find(Some(dir.as_std_path()), None).repo.expect("the probe finds a repository");
    let opened = native::discover(dir).expect("discovery opens a repository");
    let probed_root = Utf8PathBuf::from_path_buf(probed.root).expect("UTF-8");
    assert_eq!(probed.kind, opened.kind(), "the kinds agree");
    assert_eq!(canonical(&probed_root), opened.root(), "the roots agree");
    (opened.kind(), opened.root().to_owned())
}

#[test]
fn a_git_repository_is_git_at_its_root() {
    let git = vcs_git::testing::Fixture::new();
    assert_eq!(found(&sub(git.root())), (RepoKind::Git, canonical(git.root())));
}

#[test]
fn no_repository_is_none_to_the_probe_and_an_error_naming_the_path_to_discovery() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("UTF-8");
    assert_eq!(Caller::find(Some(dir.path()), None).repo, None);
    match native::discover(&path) {
        Err(VcsError::NotARepository(named)) => assert_eq!(named, path),
        Err(other) => panic!("expected NotARepository, got {other}"),
        Ok(_) => panic!("expected NotARepository, found a repository"),
    }
}

#[test]
fn a_git_directory_git_cannot_open_is_an_error_not_the_outer_repository() {
    // `sub`'s `.git` names no repository git opens, so the run has none; neither the probe
    // nor discovery climbs to `outer`.
    let outer = vcs_git::testing::Fixture::new();
    let sub = outer.root().join("sub");
    fs::create_dir_all(sub.join(".git")).expect("the directories are made");
    assert_eq!(Caller::find(Some(sub.as_std_path()), None).repo, None);
    match native::discover(&sub) {
        Err(VcsError::NotARepository(named)) => assert_eq!(named, canonical(&sub)),
        Err(other) => panic!("expected NotARepository, got {other}"),
        Ok(repo) => panic!("expected NotARepository, opened {}", repo.root()),
    }
}

/// Tests that build a jj repository. `script/pre-commit` leaves them out (`.config/nextest.toml`).
mod jj {
    use super::*;

    #[test]
    fn a_colocated_jj_repository_is_jj_at_its_root() {
        let jj = vcs_jj::testing::Fixture::colocated();
        assert!(jj.root().join(".git").is_dir(), "the fixture is colocated");
        assert_eq!(found(&sub(jj.root())), (RepoKind::Jj, canonical(jj.root())));
    }

    #[test]
    fn an_internal_jj_repository_is_jj() {
        let jj = vcs_jj::testing::Fixture::internal();
        assert!(!jj.root().join(".git").exists(), "the git store sits inside .jj");
        assert_eq!(found(&sub(jj.root())), (RepoKind::Jj, canonical(jj.root())));
    }

    #[test]
    fn git_inside_jj_is_the_inner_git() {
        let outer = vcs_jj::testing::Fixture::new();
        let inner = vcs_git::testing::Fixture::new_in(outer.root());
        assert_eq!(found(&sub(inner.root())), (RepoKind::Git, canonical(inner.root())));
    }

    #[test]
    fn jj_inside_git_is_the_inner_jj() {
        let outer = vcs_git::testing::Fixture::new();
        let inner = vcs_jj::testing::Fixture::new_in(outer.root());
        assert_eq!(found(&sub(inner.root())), (RepoKind::Jj, canonical(inner.root())));
    }
}
