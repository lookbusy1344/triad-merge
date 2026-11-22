//! `evolog` on jj: the versions of a change across the operations that rewrote it
//! (spec §7.5.4).

use vcs::{Repository as _, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture};
use vcs_jj::{ChangeVersion, Evolog, JjRepository};

fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

fn evolog(fixture: &Fixture, revset: &str, limit: Option<usize>) -> Evolog {
    open(fixture).evolog(revset, limit).expect("the evolog")
}

/// Each version's commit ID and the description of the operation that wrote it.
fn summary(versions: &[ChangeVersion]) -> Vec<(String, Option<String>)> {
    versions
        .iter()
        .map(|v| {
            (v.commit.as_str().to_owned(), v.operation.as_ref().map(|op| op.description.clone()))
        })
        .collect()
}

/// Write `a.txt` on disk and snapshot it; the working-copy commit and the head operation.
fn snapshot(fixture: &Fixture, bytes: &[u8]) -> (String, String) {
    std::fs::write(fixture.root().join("a.txt"), bytes).expect("a.txt is written");
    open(fixture).snapshot().expect("the snapshot");
    (fixture.working_copy_id(), fixture.operation_ids()[0].clone())
}

#[test]
fn each_snapshot_of_the_working_copy_is_a_version_newest_first() {
    for fixture in [Fixture::colocated(), Fixture::internal()] {
        fixture.check_out_to_disk();
        let first = fixture.working_copy_id();
        let one = snapshot(&fixture, b"1\n");
        let two = snapshot(&fixture, b"2\n");
        let log = evolog(&fixture, "@", None);
        let ops: Vec<_> = log
            .versions
            .iter()
            .map(|v| (v.commit.as_str().to_owned(), v.operation.as_ref().map(|op| op.id.clone())))
            .collect();
        assert_eq!(ops[..2], [(two.0, Some(two.1)), (one.0, Some(one.1))]);
        assert_eq!(log.versions.len(), 3, "{log:?}");
        assert_eq!(log.versions[2].commit.as_str(), first);
        assert_eq!(log.change_id, fixture.working_copy_commit().change_id);
    }
}

#[test]
fn an_edit_on_disk_is_snapshotted_before_the_walk() {
    let fixture = Fixture::new();
    fixture.check_out_to_disk();
    let before = fixture.working_copy_id();
    std::fs::write(fixture.root().join("a.txt"), b"1\n").expect("a.txt is written");
    let log = evolog(&fixture, "@", None);
    assert_eq!(log.versions.len(), 2, "{log:?}");
    assert_eq!(log.versions[0].commit.as_str(), fixture.working_copy_id());
    assert_ne!(log.versions[0].commit.as_str(), before);
    assert_eq!(
        log.versions[0].operation.as_ref().map(|op| op.description.as_str()),
        Some("snapshot working copy")
    );
}

/// A parent rewritten under a change rebases it: the rebase is a version of the change.
fn rebased_child(fixture: &Fixture) -> (Committed, Committed) {
    let parent = fixture.commit_on(&[], "parent", &[("a.txt", b"1\n")]);
    let child = fixture.commit_on(&[&parent], "child", &[("a.txt", b"1\n"), ("b.txt", b"b\n")]);
    fixture.amend(&parent, &[("a.txt", b"0\n")]);
    (child.clone(), fixture.latest(&child))
}

#[test]
fn a_rebase_by_a_rewritten_parent_is_a_version() {
    let fixture = Fixture::new();
    let (old, new) = rebased_child(&fixture);
    let log = evolog(&fixture, &new.change_id, None);
    assert_eq!(
        summary(&log.versions),
        [(new.commit, Some("amend".to_owned())), (old.commit, Some("commit".to_owned()))]
    );
    assert_eq!(log.change_id, old.change_id);
    let amend = log.versions[0].operation.as_ref().expect("an operation");
    assert_eq!(amend.id, fixture.operation_ids()[0]);
}

#[test]
fn a_hidden_commit_names_the_change_from_that_version_back() {
    let fixture = Fixture::new();
    let (old, _) = rebased_child(&fixture);
    let log = evolog(&fixture, &old.commit, None);
    assert_eq!(summary(&log.versions), [(old.commit, Some("commit".to_owned()))]);
}

#[test]
fn a_limit_keeps_the_newest_versions() {
    let fixture = Fixture::new();
    let (_, new) = rebased_child(&fixture);
    let one = evolog(&fixture, &new.change_id, Some(1));
    assert_eq!(summary(&one.versions), [(new.commit, Some("amend".to_owned()))]);
    let none = evolog(&fixture, &new.change_id, Some(0));
    assert_eq!(none.versions, []);
    assert_eq!(none.change_id, new.change_id);
}

#[test]
fn a_version_says_whether_it_holds_a_conflict() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"0\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"1\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"2\n")]);
    let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
    let resolved = fixture.amend(&merge, &[("a.txt", b"3\n")]);
    let log = evolog(&fixture, &resolved.commit, None);
    let conflicted: Vec<_> =
        log.versions.iter().map(|v| (v.commit.as_str().to_owned(), v.conflicted)).collect();
    assert_eq!(conflicted, [(resolved.commit, false), (merge.commit, true)]);
}

#[test]
fn a_revision_naming_no_commit_or_several_is_refused() {
    let fixture = Fixture::new();
    fixture.commit_on(&[], "one", &[("a.txt", b"1\n")]);
    fixture.commit_on(&[], "two", &[("a.txt", b"2\n")]);
    match open(&fixture).evolog("none()", None) {
        Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, "none()"),
        other => panic!("expected an unknown revision, got {other:?}"),
    }
    match open(&fixture).evolog("description(substring:\"o\")", None) {
        Err(VcsError::AmbiguousRevision(rev)) => assert_eq!(rev, "description(substring:\"o\")"),
        other => panic!("expected an ambiguous revision, got {other:?}"),
    }
}
