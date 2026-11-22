//! `interdiff` on jj: a change against an earlier version of itself, rebased onto the
//! change's parents (spec §7.5.4).

use camino::Utf8PathBuf;
use vcs::VcsError;
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture, without_conflict_labels};
use vcs_jj::{Interdiff, InterdiffSide, JjRepository};

fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

fn interdiff(fixture: &Fixture, since: &str, rev: &str) -> (JjRepository, Interdiff) {
    let mut repo = open(fixture);
    let interdiff = repo.interdiff(since, rev).expect("the interdiff");
    (repo, interdiff)
}

fn paths(repo: &JjRepository, interdiff: &Interdiff) -> Vec<Utf8PathBuf> {
    repo.interdiff_paths(interdiff).expect("the changed paths")
}

fn sides(repo: &JjRepository, interdiff: &Interdiff, path: &str) -> [InterdiffSide; 2] {
    repo.interdiff_sides(interdiff, path.into()).expect("the two sides")
}

fn file(bytes: &[u8]) -> InterdiffSide {
    InterdiffSide::File(bytes.to_vec())
}

/// `parent` holding `a.txt`, `child` on it adding `b.txt`; then `parent` rewritten to hold
/// `a_now`, which rebases `child`. The child before and after the rebase.
fn rebased(fixture: &Fixture, a_now: &[u8]) -> (Committed, Committed) {
    let parent = fixture.commit_on(&[], "parent", &[("a.txt", b"1\n")]);
    let child = fixture.commit_on(&[&parent], "child", &[("a.txt", b"1\n"), ("b.txt", b"x\n")]);
    fixture.amend(&parent, &[("a.txt", a_now)]);
    (child.clone(), fixture.latest(&child))
}

#[test]
fn a_rebase_alone_shows_no_difference() {
    for fixture in [Fixture::colocated(), Fixture::internal()] {
        let (old, new) = rebased(&fixture, b"0\n");
        let (repo, interdiff) = interdiff(&fixture, &old.commit, &new.change_id);
        assert!(interdiff.rebased);
        assert_eq!(paths(&repo, &interdiff), Vec::<Utf8PathBuf>::new());
        assert_eq!((interdiff.old.as_str(), interdiff.new.as_str()), (&*old.commit, &*new.commit));
    }
}

#[test]
fn an_edit_after_a_rebase_shows_alone() {
    let fixture = Fixture::new();
    let (old, rebased) = rebased(&fixture, b"0\n");
    let new = fixture.amend(&rebased, &[("a.txt", b"0\n"), ("b.txt", b"y\n")]);
    let (repo, interdiff) = interdiff(&fixture, &old.commit, &new.commit);
    assert_eq!(paths(&repo, &interdiff), [Utf8PathBuf::from("b.txt")]);
    assert_eq!(sides(&repo, &interdiff, "b.txt"), [file(b"x\n"), file(b"y\n")]);
    // The parent's edit sits on both sides.
    assert_eq!(sides(&repo, &interdiff, "a.txt"), [file(b"0\n"), file(b"0\n")]);
}

#[test]
fn with_the_same_parents_the_old_tree_is_compared_as_it_is() {
    let fixture = Fixture::new();
    let parent = fixture.commit_on(&[], "parent", &[("a.txt", b"1\n")]);
    let old = fixture.commit_on(&[&parent], "child", &[("a.txt", b"2\n")]);
    let new = fixture.amend(&old, &[("a.txt", b"3\n")]);
    let (repo, interdiff) = interdiff(&fixture, &old.commit, &new.commit);
    assert!(!interdiff.rebased);
    assert_eq!(interdiff.new.as_str(), new.commit);
    assert_eq!(sides(&repo, &interdiff, "a.txt"), [file(b"2\n"), file(b"3\n")]);
}

#[test]
fn a_file_one_version_lacks_is_absent_on_that_side() {
    let fixture = Fixture::new();
    let (old, rebased) = rebased(&fixture, b"0\n");
    let new = fixture.amend(&rebased, &[("a.txt", b"0\n"), ("c.txt", b"c\n")]);
    let (repo, interdiff) = interdiff(&fixture, &old.commit, &new.commit);
    assert_eq!(paths(&repo, &interdiff), [Utf8PathBuf::from("b.txt"), Utf8PathBuf::from("c.txt")]);
    assert_eq!(sides(&repo, &interdiff, "b.txt"), [file(b"x\n"), InterdiffSide::Absent]);
    assert_eq!(sides(&repo, &interdiff, "c.txt"), [InterdiffSide::Absent, file(b"c\n")]);
}

#[test]
fn a_conflict_the_rebase_leaves_shows_as_jjs_markers() {
    let fixture = Fixture::new();
    let parent = fixture.commit_on(&[], "parent", &[("a.txt", b"1\n")]);
    let old = fixture.commit_on(&[&parent], "child", &[("a.txt", b"2\n")]);
    fixture.amend(&parent, &[("a.txt", b"3\n")]);
    let conflicted = fixture.latest(&old);
    let new = fixture.amend(&conflicted, &[("a.txt", b"4\n")]);
    let (repo, interdiff) = interdiff(&fixture, &old.commit, &new.commit);
    let [left, right] = sides(&repo, &interdiff, "a.txt");
    let InterdiffSide::File(left) = left else { panic!("a file on the left, got {left:?}") };
    // jj's default `diff` style: side 1 is the new parent's file, side 2 the old change's.
    assert_eq!(
        without_conflict_labels(&String::from_utf8(left).expect("UTF-8")),
        "<<<<<<<\n%%%%%%%\n\\\\\\\\\\\\\\\n-1\n+3\n+++++++\n2\n>>>>>>>\n"
    );
    assert_eq!(right, file(b"4\n"));
}

#[test]
fn an_earlier_version_naming_no_commit_is_refused() {
    let fixture = Fixture::new();
    fixture.commit_on(&[], "one", &[("a.txt", b"1\n")]);
    match open(&fixture).interdiff("none()", "@") {
        Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, "none()"),
        other => panic!("expected an unknown revision, got {other:?}"),
    }
}
