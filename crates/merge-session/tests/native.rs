//! `diff PATH` inputs from git and jj repositories built through gix and jj-lib (spec §6.1,
//! PLAN.md §A7.1).

use merge_session::native::{self, NativeDiff, NativeError};
use merge_session::repository::RunRepository;
use vcs::VcsError;
use vcs_git::testing::Fixture;

fn load(path: &camino::Utf8Path, rev: Option<&str>) -> NativeDiff {
    try_load(path.as_std_path(), rev).expect("the inputs load")
}

/// [`native::load`] in the repository holding `path`, as `diff PATH` finds it.
fn try_load(path: &std::path::Path, rev: Option<&str>) -> Result<NativeDiff, NativeError> {
    native::load(&mut RunRepository::of_file(path), path, rev)
}

#[test]
fn an_unstaged_file_is_compared_with_head_titled_by_branch() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("src/a.txt", b"committed\n")]);
    fixture.stage(&[("src/a.txt", b"committed\n")]);
    let path = fixture.write("src/a.txt", b"edited\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "committed\n");
    assert_eq!(diff.left_title, "main");
    assert_eq!(diff.right, path.as_std_path());
}

#[test]
fn a_staged_file_is_compared_with_the_index() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"committed\n")]);
    fixture.stage(&[("a.txt", b"staged\n")]);
    let path = fixture.write("a.txt", b"edited\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "staged\n");
    assert_eq!(diff.left_title, "index");
}

#[test]
fn rev_picks_the_revision_and_titles_the_pane() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"one\n")]);
    fixture.commit("two", &[("a.txt", b"two\n")]);
    fixture.stage(&[("a.txt", b"two\n")]);
    let path = fixture.write("a.txt", b"three\n");
    let diff = load(&path, Some("HEAD~1"));
    assert_eq!(diff.left.content(), "one\n");
    assert_eq!(diff.left_title, "HEAD~1");
}

#[test]
fn a_file_absent_from_the_parent_is_compared_with_nothing() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.stage(&[("a.txt", b"a\n")]);
    let path = fixture.write("new.txt", b"new\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "");
    assert_eq!(diff.left_title, "main (no file)");
}

#[test]
fn a_file_absent_from_a_given_revision_says_so_in_the_title() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.commit("two", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    fixture.stage(&[("b.txt", b"b\n")]);
    let path = fixture.write("b.txt", b"edited\n");
    let diff = load(&path, Some("HEAD~1"));
    assert_eq!(diff.left.content(), "");
    assert_eq!(diff.left_title, "HEAD~1 (no file)");
}

#[test]
fn an_empty_file_at_the_revision_keeps_the_plain_title() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"")]);
    fixture.stage(&[("a.txt", b"")]);
    let path = fixture.write("a.txt", b"edited\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "");
    assert_eq!(diff.left_title, "main");
}

#[test]
fn a_path_given_through_a_parent_directory_resolves() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"committed\n")]);
    fixture.stage(&[("a.txt", b"committed\n")]);
    fixture.write("a.txt", b"edited\n");
    let indirect = fixture.write("src/b.txt", b"b\n").with_file_name("../a.txt");
    assert_eq!(load(&indirect, None).left.content(), "committed\n");
}

#[test]
fn a_file_outside_any_repository_is_named() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "a\n").expect("the file is written");
    match try_load(&path, None) {
        Err(NativeError::Vcs(VcsError::NotARepository(_))) => {}
        other => panic!("expected NotARepository, got {other:?}"),
    }
}

#[test]
fn a_missing_working_copy_file_is_named() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let path = fixture.root().join("gone.txt");
    match try_load(path.as_std_path(), None) {
        Err(NativeError::Find { path: named, .. }) => assert_eq!(named, path.as_std_path()),
        other => panic!("expected Find, got {other:?}"),
    }
}

#[test]
fn an_unknown_revision_is_named() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let path = fixture.write("a.txt", b"b\n");
    match try_load(path.as_std_path(), Some("nope")) {
        Err(NativeError::Vcs(VcsError::UnknownRevision(rev))) => assert_eq!(rev, "nope"),
        other => panic!("expected UnknownRevision, got {other:?}"),
    }
}

#[test]
fn a_binary_parent_is_refused_with_its_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.bin", b"\0\x01\x02")]);
    fixture.stage(&[("a.bin", b"\0\x01\x02")]);
    let path = fixture.write("a.bin", b"text now\n");
    match try_load(path.as_std_path(), None) {
        Err(NativeError::Decode { path, rev, .. }) => {
            assert_eq!(path, "a.bin");
            assert_eq!(rev, "HEAD");
        }
        other => panic!("expected Decode, got {other:?}"),
    }
}

mod jj {
    use super::load;
    use merge_session::native;
    use vcs::RepoKind;
    use vcs_jj::testing::Fixture;

    #[test]
    fn a_file_is_compared_with_at_minus_titled_by_its_revision() {
        let fixture = Fixture::new();
        fixture.commit("fix parsing\n\nbody\n", &[("src/a.txt", b"committed\n")]);
        let path = fixture.write("src/a.txt", b"edited\n");
        let diff = load(&path, None);
        assert_eq!(diff.left.content(), "committed\n");
        // The heading's revision chip names the change (spec §5).
        assert_eq!(diff.left_title, "@-");
        assert_eq!(diff.right, path.as_std_path());
    }

    #[test]
    fn a_file_absent_from_at_minus_says_so_in_the_title() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"a\n")]);
        let path = fixture.write("new.txt", b"new\n");
        let diff = load(&path, None);
        assert_eq!(diff.left.content(), "");
        assert_eq!(diff.left_title, "@- (no file)");
    }

    #[test]
    fn rev_takes_a_revset() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"one\n")]);
        fixture.commit("two", &[("a.txt", b"two\n")]);
        let path = fixture.write("a.txt", b"three\n");
        let diff = load(&path, Some("@--"));
        assert_eq!(diff.left.content(), "one\n");
        assert_eq!(diff.left_title, "@--");
    }

    #[test]
    fn a_merge_working_copy_is_titled_by_its_revision() {
        let fixture = Fixture::new();
        let left = fixture.commit_on(&[], "left", &[("a.txt", b"left\n")]);
        let right = fixture.commit_on(&[], "right", &[("b.txt", b"right\n")]);
        fixture.check_out(&[&left, &right]);
        let path = fixture.write("a.txt", b"edited\n");
        let diff = load(&path, None);
        assert_eq!(diff.left.content(), "left\n");
        assert_eq!(diff.left_title, "@-");
    }

    #[test]
    fn a_colocated_repository_is_read_as_jj_though_git_sees_it_too() {
        let fixture = Fixture::colocated();
        fixture.commit("one", &[("a.txt", b"committed\n")]);
        let path = fixture.write("a.txt", b"edited\n");
        let repo = native::discover(fixture.root()).expect("the repository is found");
        assert_eq!(repo.kind(), RepoKind::Jj);
        let diff = load(&path, None);
        assert_eq!(diff.left_title, "@-");
        // git's detached HEAD is jj's @-.
        let git = vcs_git::GitRepository::discover(fixture.root()).expect("git sees it");
        let head = vcs::Repository::file_at(
            &git,
            camino::Utf8Path::new("a.txt"),
            &vcs::RevSpec::Expr("HEAD".to_owned()),
        )
        .expect("git reads HEAD");
        assert_eq!(head.as_deref(), Some(&b"committed\n"[..]));
    }

    #[test]
    fn the_nearest_repository_wins_when_one_nests_in_another() {
        let outer_jj = Fixture::new();
        let inner_git = vcs_git::testing::Fixture::new_in(outer_jj.root());
        let found = native::discover(inner_git.root()).expect("a repository is found");
        assert_eq!(found.kind(), RepoKind::Git, "git inside jj");
        assert_eq!(native::discover(outer_jj.root()).expect("the outer one").kind(), RepoKind::Jj);

        let outer_git = vcs_git::testing::Fixture::new();
        let inner_jj = Fixture::new_in(outer_git.root());
        let found = native::discover(inner_jj.root()).expect("a repository is found");
        assert_eq!(found.kind(), RepoKind::Jj, "jj inside git");
        assert_eq!(
            native::discover(outer_git.root()).expect("the outer one").kind(),
            RepoKind::Git
        );
    }
}
