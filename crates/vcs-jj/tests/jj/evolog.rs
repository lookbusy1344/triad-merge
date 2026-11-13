//! `evolog_diff` on jj: a change's file against an earlier version of the change rebased
//! onto its parents now (`docs/PLAN-jj-native.md` §I6).

use camino::Utf8Path;
use vcs::{IdKind, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture};
use vcs_jj::{JjError, JjRepository};

// The base's edit and the change's sit lines apart: jj conflicts on adjacent lines.
const BASE: &[u8] = b"1\n2\n3\n4\n5\n";
const BASE_AMENDED: &[u8] = b"ONE\n2\n3\n4\n5\n";
const CHANGE: &[u8] = b"1\n2\n3\nFOUR\n5\n";
const CHANGE_REBASED: &[u8] = b"ONE\n2\n3\nFOUR\n5\n";
const CHANGE_AMENDED: &[u8] = b"ONE\n2\n3\nFOUR\nFIVE\n";

fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

fn a_txt() -> &'static Utf8Path {
    Utf8Path::new("a.txt")
}

/// `change` edits line 4 on `base`; then `base` is amended at line 1, which rebases
/// `change`. Returns the original change and the base.
fn rebased(fixture: &Fixture) -> (Committed, Committed) {
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let change = fixture.commit_on(&[&base], "change", &[("a.txt", CHANGE)]);
    fixture.amend(&base, &[("a.txt", BASE_AMENDED)]);
    (change, base)
}

#[test]
fn a_rebase_alone_shows_no_difference() {
    for fixture in [Fixture::colocated(), Fixture::internal()] {
        let (change, _) = rebased(&fixture);
        let now = fixture.latest(&change);
        assert_ne!(now.commit, change.commit, "the rebase rewrote the change");
        let diff = open(&fixture).evolog_diff(&now.commit, None, a_txt()).expect("the diff");
        assert_eq!(diff.right.as_deref(), Some(CHANGE_REBASED));
        assert_eq!(diff.left.as_deref(), Some(CHANGE_REBASED));
        assert!(change.commit.starts_with(diff.predecessor.id(IdKind::Commit)));
    }
}

#[test]
fn an_amend_after_the_rebase_is_the_one_difference() {
    let fixture = Fixture::new();
    let (change, _) = rebased(&fixture);
    let rebased_commit = fixture.latest(&change);
    fixture.amend(&rebased_commit, &[("a.txt", CHANGE_AMENDED)]);
    let now = fixture.latest(&change);
    let diff = open(&fixture).evolog_diff(&now.commit, None, a_txt()).expect("the diff");
    assert_eq!(diff.left.as_deref(), Some(CHANGE_REBASED));
    assert_eq!(diff.right.as_deref(), Some(CHANGE_AMENDED));
    assert!(rebased_commit.commit.starts_with(diff.predecessor.id(IdKind::Commit)));
}

#[test]
fn the_nearest_version_that_differs_is_skipped_to_past_rebases() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let change = fixture.commit_on(&[&base], "change", &[("a.txt", CHANGE)]);
    fixture.amend(&change, &[("a.txt", b"1\n2\n3\nFOUR\nFIVE\n")]);
    // A rebase after the amend: the nearest version differs only by its parent.
    fixture.amend(&base, &[("a.txt", BASE_AMENDED)]);
    let now = fixture.latest(&change);
    let diff = open(&fixture).evolog_diff(&now.commit, None, a_txt()).expect("the diff");
    assert_eq!(diff.left.as_deref(), Some(CHANGE_REBASED));
    assert_eq!(diff.right.as_deref(), Some(CHANGE_AMENDED));
    assert!(change.commit.starts_with(diff.predecessor.id(IdKind::Commit)));
}

#[test]
fn from_names_the_earlier_version() {
    let fixture = Fixture::new();
    let (change, _) = rebased(&fixture);
    fixture.amend(&fixture.latest(&change), &[("a.txt", CHANGE_AMENDED)]);
    let now = fixture.latest(&change);
    let diff =
        open(&fixture).evolog_diff(&now.commit, Some(&change.commit), a_txt()).expect("the diff");
    assert_eq!(diff.left.as_deref(), Some(CHANGE_REBASED));
    assert!(change.commit.starts_with(diff.predecessor.id(IdKind::Commit)));
}

#[test]
fn an_earlier_version_whose_rebase_conflicts_shows_jj_s_markers() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let change = fixture.commit_on(&[&base], "change", &[("a.txt", b"c1\n2\n3\n4\n5\n")]);
    let now = fixture.amend(&change, &[("a.txt", b"c1\n2\n3\n4\nFIVE\n")]);
    fixture.amend(&base, &[("a.txt", BASE_AMENDED)]);
    let now = fixture.latest(&now);
    let diff =
        open(&fixture).evolog_diff(&now.commit, Some(&change.commit), a_txt()).expect("the diff");
    let left = String::from_utf8(diff.left.expect("a left side")).expect("UTF-8");
    assert!(left.starts_with("<<<<<<< conflict 1 of 1\n"), "{left}");
    assert!(left.contains("c1\n") && left.contains("ONE\n"), "{left}");
    assert!(left.ends_with(">>>>>>> conflict 1 of 1 ends\n2\n3\n4\n5\n"), "{left}");
}

#[test]
fn a_file_added_by_the_amend_is_absent_on_the_left() {
    let fixture = Fixture::new();
    let change = fixture.commit_on(&[], "change", &[("b.txt", b"b\n")]);
    fixture.amend(&change, &[("a.txt", CHANGE), ("b.txt", b"b\n")]);
    let now = fixture.latest(&change);
    let diff = open(&fixture).evolog_diff(&now.commit, None, a_txt()).expect("the diff");
    assert_eq!((diff.left, diff.right.as_deref()), (None, Some(CHANGE)));
}

#[test]
fn a_change_with_no_earlier_version_is_refused() {
    let fixture = Fixture::new();
    let change = fixture.commit_on(&[], "change", &[("a.txt", CHANGE)]);
    match open(&fixture).evolog_diff(&change.commit, None, a_txt()) {
        Err(VcsError::Backend { source, .. }) => assert_eq!(
            source.downcast_ref::<JjError>().map(ToString::to_string),
            Some(format!("change {} has no earlier version", change.change_id))
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
}
