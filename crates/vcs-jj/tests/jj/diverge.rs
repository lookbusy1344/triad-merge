//! `diverge` on jj: the copies of a divergent change merged over their shared predecessor
//! (spec §7.5.2).

use vcs::{IdKind, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture, Held};
use vcs_jj::{JjError, JjRepository};

const ORIGINAL: &[u8] = b"1\n2\n3\n4\n5\n";
const FIRST_LINE: &[u8] = b"ONE\n2\n3\n4\n5\n";
const LAST_LINE: &[u8] = b"1\n2\n3\n4\nFIVE\n";
const BOTH_LINES: &[u8] = b"ONE\n2\n3\n4\nFIVE\n";
const OTHER_FIRST_LINE: &[u8] = b"uno\n2\n3\n4\n5\n";

fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

/// `change`, holding `ORIGINAL` in `a.txt` on `base`, rewritten two ways: `one` and `two`,
/// each a description and an `a.txt`.
fn divergent(
    fixture: &Fixture,
    one: (&str, &[u8]),
    two: (&str, &[u8]),
) -> (Committed, Committed, Committed) {
    let base = fixture.commit_on(&[], "base", &[("b.txt", b"b\n")]);
    let change = fixture.commit_on(&[&base], "change", &[("a.txt", ORIGINAL), ("b.txt", b"b\n")]);
    let copies = fixture.diverge(
        &change,
        &[
            (one.0, &[("a.txt", one.1), ("b.txt", b"b\n")]),
            (two.0, &[("a.txt", two.1), ("b.txt", b"b\n")]),
        ],
    );
    let [first, second] = <[Committed; 2]>::try_from(copies).expect("two copies");
    (change, first, second)
}

#[test]
fn changes_to_different_lines_merge_clean_into_one_commit() {
    for fixture in [Fixture::colocated(), Fixture::internal()] {
        let (change, one, two) = divergent(&fixture, ("change", FIRST_LINE), ("change", LAST_LINE));
        assert_eq!(fixture.visible(&change).len(), 2, "divergent before");
        let merged = open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
        assert!(merged.rewrite.written && merged.rewrite.resolved, "{merged:?}");
        assert_eq!(merged.conflicted, Vec::<camino::Utf8PathBuf>::new());
        assert!(!merged.description_kept);
        let visible = fixture.visible(&change);
        assert_eq!(visible.len(), 1, "one commit after");
        assert_eq!(fixture.held_in(&visible[0], "a.txt"), Held::File(BOTH_LINES.to_vec()));
        assert_eq!(fixture.description(&visible[0]), "change");
        let [base] = merged.bases.as_slice() else { panic!("one base: {:?}", merged.bases) };
        assert!(change.commit.starts_with(base.id(IdKind::Commit)), "the base is the original");
        let kept = merged.rewrite.target.id(IdKind::Commit);
        let [abandoned] = merged.abandoned.as_slice() else { panic!("{:?}", merged.abandoned) };
        let mut copies = [kept, abandoned.id(IdKind::Commit)];
        copies.sort_unstable();
        let mut expected = [&one.commit[..copies[0].len()], &two.commit[..copies[1].len()]];
        expected.sort_unstable();
        assert_eq!(copies, expected);
        assert_eq!(
            fixture.operations()[0].description,
            format!("merge divergent change {} in triad", merged.rewrite.target.id(IdKind::Change))
        );
    }
}

#[test]
fn copies_on_different_parents_merge_as_if_rebased_onto_the_kept_copy_s_parents() {
    const P0: &[u8] = b"p0\n";
    const P1: &[u8] = b"p1\n";
    // One copy amended on the old parent, the other only rebased onto a new parent; each
    // arrangement puts the rebased copy on the other side of the index order.
    for rebased_first in [true, false] {
        let fixture = Fixture::new();
        let old = fixture.commit_on(&[], "old", &[("a.txt", ORIGINAL), ("p.txt", P0)]);
        let new = fixture.commit_on(&[&old], "new", &[("a.txt", ORIGINAL), ("p.txt", P1)]);
        let change = fixture.commit_on(
            &[&old],
            "change",
            &[("a.txt", ORIGINAL), ("p.txt", P0), ("c.txt", b"c\n")],
        );
        let amended: (&str, &[&Committed], _) =
            ("change", &[&old], &[("a.txt", FIRST_LINE), ("p.txt", P0), ("c.txt", b"c\n")][..]);
        let rebased: (&str, &[&Committed], _) =
            ("change", &[&new], &[("a.txt", ORIGINAL), ("p.txt", P1), ("c.txt", b"c\n")][..]);
        let copies = if rebased_first { [rebased, amended] } else { [amended, rebased] };
        fixture.diverge_on(&change, &copies);
        let merged = open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
        assert!(merged.rewrite.resolved, "{merged:?}");
        let [survivor] = <[Committed; 1]>::try_from(fixture.visible(&change)).expect("one commit");
        let [parent] = <[String; 1]>::try_from(fixture.parents_of(&survivor)).expect("one parent");
        let (parent_p, at_new) = if parent == new.commit { (P1, "new") } else { (P0, "old") };
        // The patch is the change's alone: p.txt is the parent's, whichever parent is kept.
        assert_eq!(fixture.held_in(&survivor, "p.txt"), Held::File(parent_p.to_vec()), "{at_new}");
        assert_eq!(
            fixture.held_in(&survivor, "a.txt"),
            Held::File(FIRST_LINE.to_vec()),
            "{at_new}"
        );
        assert_eq!(fixture.held_in(&survivor, "c.txt"), Held::File(b"c\n".to_vec()), "{at_new}");
    }
}

#[test]
fn a_copy_rewritten_from_the_other_merges_over_that_copy() {
    // `amended` holds `FIRST_LINE` over the original; `rewritten` comes from `amended` and
    // changes that line again. Over the original the two would conflict; over `amended`,
    // their shared predecessor, `rewritten` wins.
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("b.txt", b"b\n")]);
    let change = fixture.commit_on(&[&base], "change", &[("a.txt", ORIGINAL)]);
    let amended = fixture.amend(&change, &[("a.txt", FIRST_LINE)]);
    let rewritten = fixture.rewrite_keeping(&amended, &[("a.txt", OTHER_FIRST_LINE)]);
    assert_eq!(fixture.visible(&change).len(), 2, "divergent before");
    let merged = open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
    assert!(merged.rewrite.resolved, "{merged:?}");
    let [base] = merged.bases.as_slice() else { panic!("one base: {:?}", merged.bases) };
    assert!(amended.commit.starts_with(base.id(IdKind::Commit)), "{base:?} is not {amended:?}");
    let [survivor] = <[Committed; 1]>::try_from(fixture.visible(&change)).expect("one commit");
    assert_eq!(fixture.held_in(&survivor, "a.txt"), Held::File(OTHER_FIRST_LINE.to_vec()));
    let mut copies =
        [merged.rewrite.target.id(IdKind::Commit), merged.abandoned[0].id(IdKind::Commit)];
    copies.sort_unstable();
    let mut expected = [&amended.commit[..copies[0].len()], &rewritten.commit[..copies[1].len()]];
    expected.sort_unstable();
    assert_eq!(copies, expected, "the two copies merged");
}

#[test]
fn the_same_line_changed_two_ways_stays_conflicted_in_the_commit() {
    let fixture = Fixture::new();
    let (change, ..) = divergent(&fixture, ("change", FIRST_LINE), ("change", OTHER_FIRST_LINE));
    let merged = open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
    assert!(merged.rewrite.written && !merged.rewrite.resolved, "{merged:?}");
    assert_eq!(merged.conflicted, ["a.txt"]);
    let visible = fixture.visible(&change);
    assert_eq!(visible.len(), 1);
    assert_eq!(fixture.held_in(&visible[0], "a.txt"), Held::Conflict);
}

#[test]
fn a_description_changed_on_one_copy_wins_and_two_changes_keep_the_first() {
    // Either copy may come first: the changed description wins from both places.
    for (one, two) in [("change", "better"), ("better", "change")] {
        let fixture = Fixture::new();
        let (change, ..) = divergent(&fixture, (one, FIRST_LINE), (two, LAST_LINE));
        let merged = open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
        assert!(!merged.description_kept);
        assert_eq!(fixture.description(&fixture.visible(&change)[0]), "better", "{one}, {two}");
    }

    let fixture = Fixture::new();
    let (change, one, two) = divergent(&fixture, ("left", FIRST_LINE), ("right", LAST_LINE));
    let merged = open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
    assert!(merged.description_kept);
    let kept = merged.rewrite.target.id(IdKind::Commit);
    let first = if one.commit.starts_with(kept) { "left" } else { "right" };
    assert!(one.commit.starts_with(kept) || two.commit.starts_with(kept));
    assert_eq!(fixture.description(&fixture.visible(&change)[0]), first);
}

#[test]
fn the_descendants_of_an_abandoned_copy_move_onto_the_merged_commit() {
    let fixture = Fixture::new();
    let (change, one, two) = divergent(&fixture, ("change", FIRST_LINE), ("change", LAST_LINE));
    let on_one = fixture.commit_merged(&[&one], "on one", &[("c.txt", b"c\n")]);
    let on_two = fixture.commit_merged(&[&two], "on two", &[("d.txt", b"d\n")]);
    let merged = open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
    let survivor = &fixture.visible(&change)[0];
    for child in [&on_one, &on_two] {
        let latest = fixture.latest(child);
        assert_eq!(fixture.parents_of(&latest), std::slice::from_ref(&survivor.commit));
        assert_eq!(fixture.held_in(&latest, "a.txt"), Held::File(BOTH_LINES.to_vec()));
    }
    assert_eq!(merged.rewrite.descendants.len(), 2, "{merged:?}");
}

#[test]
fn a_working_copy_on_an_abandoned_copy_moves_to_the_merged_commit_on_disk() {
    let fixture = Fixture::new();
    let (change, one, two) = divergent(&fixture, ("change", FIRST_LINE), ("change", LAST_LINE));
    for copy in [&one, &two] {
        fixture.edit(copy);
        fixture.check_out_to_disk();
        let merged = open(&fixture).diverge(&change.change_id, true).expect("a dry run");
        assert!(!merged.rewrite.written);
    }
    // `@` is `two` now; the merge keeps whichever copy comes first.
    open(&fixture).diverge(&change.change_id, false).expect("the copies merge");
    let survivor = &fixture.visible(&change)[0];
    assert_eq!(fixture.working_copy_id(), survivor.commit);
    let on_disk = std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads");
    assert_eq!(on_disk, BOTH_LINES);
}

#[test]
fn a_dry_run_writes_nothing() {
    let fixture = Fixture::new();
    let (change, ..) = divergent(&fixture, ("change", FIRST_LINE), ("change", LAST_LINE));
    let operations = fixture.operation_count();
    let merged = open(&fixture).diverge(&change.change_id, true).expect("a dry run");
    assert!(!merged.rewrite.written && merged.rewrite.resolved);
    assert_eq!(fixture.visible(&change).len(), 2);
    assert_eq!(fixture.operation_count(), operations);
}

fn backend_error(result: Result<vcs_jj::Divergence, VcsError>) -> String {
    match result {
        Err(VcsError::Backend { source, .. }) => {
            let jj = source.downcast_ref::<JjError>().map(ToString::to_string);
            jj.unwrap_or_else(|| format!("not a JjError: {source}"))
        }
        other => panic!("expected a backend error, got {other:?}"),
    }
}

#[test]
fn a_change_with_one_visible_commit_is_refused() {
    let fixture = Fixture::new();
    let change = fixture.commit_on(&[], "change", &[("a.txt", ORIGINAL)]);
    let refused = backend_error(open(&fixture).diverge(&change.change_id, false));
    assert_eq!(refused, format!("change {} is not divergent", change.change_id));
}

#[test]
fn copies_that_share_no_predecessor_are_refused() {
    let fixture = Fixture::new();
    let change = fixture.commit_on(&[], "change", &[("a.txt", ORIGINAL)]);
    fixture.commit_as_change(&change, &[], "twin", &[("a.txt", FIRST_LINE)]);
    let operations = fixture.operation_count();
    let refused = backend_error(open(&fixture).diverge(&change.change_id, false));
    assert_eq!(refused, format!("the copies of change {} share no predecessor", change.change_id));
    assert_eq!(fixture.visible(&change).len(), 2);
    assert_eq!(fixture.operation_count(), operations);
}

#[test]
fn an_unknown_change_is_refused_by_name() {
    let fixture = Fixture::new();
    fixture.commit_on(&[], "change", &[("a.txt", ORIGINAL)]);
    let refused = open(&fixture).diverge("zzzzzzzzzzzk", false);
    assert!(matches!(refused, Err(VcsError::UnknownRevision(_))), "{refused:?}");
}
