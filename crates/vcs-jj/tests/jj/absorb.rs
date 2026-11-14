//! `absorb` on jj: the working copy's hunks, each with the ancestor that wrote its lines,
//! moved there on request (spec §7.5.4).

use camino::Utf8PathBuf;
use vcs::{IdKind, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture, Held};
use vcs_jj::{AbsorbHunk, Absorbed, HunkChoice, JjError, JjRepository};

fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

/// `one` writes `a.txt`, `two` on it writes `b.txt`; the working copy sits on `two`,
/// checked out to disk, with `a` and `b` written over the files.
fn stack(a: &[u8], b: &[u8]) -> (Fixture, Committed, Committed) {
    let fixture = Fixture::new();
    let one = fixture.commit_on(&[], "one", &[("a.txt", b"1\n2\n3\n4\n5\n")]);
    let two = fixture.commit_merged(&[&one], "two", &[("b.txt", b"x\ny\nz\n")]);
    fixture.check_out(&[&two]);
    fixture.check_out_to_disk();
    std::fs::write(fixture.root().join("a.txt"), a).expect("a.txt is written");
    std::fs::write(fixture.root().join("b.txt"), b).expect("b.txt is written");
    (fixture, one, two)
}

fn listed(fixture: &Fixture, choices: &[HunkChoice], apply: bool) -> Absorbed {
    open(fixture).absorb(&[], None, choices, apply, None).expect("the hunks are listed")
}

/// Each hunk's number, path, line ranges and target's full commit ID prefix.
fn summary(absorbed: &Absorbed) -> Vec<(usize, &str, [usize; 4], Option<&str>)> {
    absorbed
        .hunks
        .iter()
        .map(|AbsorbHunk { number, path, left, right, target, .. }| {
            (
                *number,
                path.as_str(),
                [left.start, left.end, right.start, right.end],
                target.as_ref().map(|t| t.id(IdKind::Commit)),
            )
        })
        .collect()
}

fn short(commit: &Committed) -> &str {
    &commit.commit[..8]
}

#[test]
fn each_hunk_goes_to_the_commit_that_wrote_its_lines() {
    let (fixture, one, two) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let absorbed = listed(&fixture, &[], false);
    assert_eq!(
        summary(&absorbed),
        [
            (1, "a.txt", [2, 3, 2, 3], Some(short(&one))),
            (2, "b.txt", [1, 2, 1, 2], Some(short(&two))),
        ]
    );
    assert!(!absorbed.rewrite.written);
    assert_eq!(
        fixture.held_in(&fixture.latest(&one), "a.txt"),
        Held::File(b"1\n2\n3\n4\n5\n".to_vec())
    );
}

#[test]
fn each_hunk_carries_its_lines_before_and_after() {
    let (fixture, ..) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nY2\nz\n");
    let texts: Vec<_> =
        listed(&fixture, &[], false).hunks.into_iter().map(|h| (h.before, h.after)).collect();
    assert_eq!(
        texts,
        [(b"3\n".to_vec(), b"THREE\n".to_vec()), (b"y\n".to_vec(), b"Y\nY2\n".to_vec())]
    );
}

#[test]
fn the_listing_names_the_commits_a_hunk_may_go_into_children_first() {
    let (fixture, one, two) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\ny\nz\n");
    let commits = |absorbed: &Absorbed| -> Vec<String> {
        absorbed.destinations.iter().map(|d| d.id.as_str().to_owned()).collect()
    };
    assert_eq!(commits(&listed(&fixture, &[], false)), [two.commit.clone(), one.commit.clone()]);
    let only_one = open(&fixture)
        .absorb(&[], Some(&one.commit), &[], false, None)
        .expect("the hunks are listed");
    assert_eq!(commits(&only_one), [one.commit]);
}

#[test]
fn an_insertion_inside_one_commit_s_lines_goes_there() {
    let (fixture, one, _) = stack(b"1\n2\nNEW\n3\n4\n5\n", b"x\ny\nz\n");
    assert_eq!(
        summary(&listed(&fixture, &[], false)),
        [(1, "a.txt", [2, 2, 2, 3], Some(short(&one)))]
    );
}

#[test]
fn a_hunk_across_two_commits_lines_stays() {
    let fixture = Fixture::new();
    let one = fixture.commit_on(&[], "one", &[("a.txt", b"1\n2\n")]);
    let two = fixture.commit_merged(&[&one], "two", &[("a.txt", b"1\n2\n3\n4\n")]);
    fixture.check_out(&[&two]);
    fixture.check_out_to_disk();
    std::fs::write(fixture.root().join("a.txt"), b"1\nTWO\nTHREE\n4\n").expect("written");
    std::fs::write(fixture.root().join("c.txt"), b"new\n").expect("written");
    let absorbed = listed(&fixture, &[], false);
    assert_eq!(summary(&absorbed), [(1, "a.txt", [1, 3, 1, 3], None)]);
    // An insertion where `one`'s lines end and `two`'s start stays too.
    std::fs::write(fixture.root().join("a.txt"), b"1\n2\nMID\n3\n4\n").expect("written");
    assert_eq!(summary(&listed(&fixture, &[], false)), [(1, "a.txt", [2, 2, 2, 3], None)]);
}

#[test]
fn move_and_keep_override_a_hunk_s_commit() {
    let (fixture, one, _) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let choices = [HunkChoice::Keep(1), HunkChoice::Move { hunk: 2, rev: one.commit.clone() }];
    assert_eq!(
        summary(&listed(&fixture, &choices, false)),
        [(1, "a.txt", [2, 3, 2, 3], None), (2, "b.txt", [1, 2, 1, 2], Some(short(&one)))]
    );
}

#[test]
fn apply_moves_each_hunk_and_leaves_the_rest_in_the_working_copy() {
    for fixture_kind in ["colocated", "internal"] {
        let fixture =
            if fixture_kind == "colocated" { Fixture::colocated() } else { Fixture::internal() };
        let one = fixture.commit_on(&[], "one", &[("a.txt", b"1\n2\n3\n4\n5\n")]);
        let two = fixture.commit_merged(&[&one], "two", &[("b.txt", b"x\ny\nz\n")]);
        fixture.check_out(&[&two]);
        fixture.check_out_to_disk();
        std::fs::write(fixture.root().join("a.txt"), b"1\n2\nTHREE\n4\n5\n").expect("written");
        std::fs::write(fixture.root().join("b.txt"), b"x\nY\nz\n").expect("written");
        std::fs::write(fixture.root().join("c.txt"), b"new\n").expect("written");
        let absorbed = listed(&fixture, &[], true);
        assert!(absorbed.rewrite.written, "{fixture_kind}");
        assert_eq!(
            fixture.held_in(&fixture.latest(&one), "a.txt"),
            Held::File(b"1\n2\nTHREE\n4\n5\n".to_vec()),
            "{fixture_kind}"
        );
        let two_now = fixture.latest(&two);
        assert_eq!(fixture.held_in(&two_now, "b.txt"), Held::File(b"x\nY\nz\n".to_vec()));
        assert_eq!(fixture.held_in(&two_now, "a.txt"), Held::File(b"1\n2\nTHREE\n4\n5\n".to_vec()));
        let wc = fixture.working_copy_commit();
        assert_eq!(fixture.parents_of(&wc), std::slice::from_ref(&two_now.commit));
        assert_eq!(fixture.held_in(&wc, "c.txt"), Held::File(b"new\n".to_vec()));
        assert_eq!(fixture.operations()[0].description, "absorb in triad");
        let on_disk = std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads");
        assert_eq!(on_disk, b"1\n2\nTHREE\n4\n5\n");
    }
}

#[test]
fn an_emptied_working_copy_is_replaced_by_a_new_one_on_its_parent() {
    let (fixture, one, two) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\ny\nz\n");
    let before = fixture.working_copy_commit();
    let absorbed = listed(&fixture, &[], true);
    assert!(absorbed.rewrite.written && absorbed.rewrite.resolved);
    let wc = fixture.working_copy_commit();
    assert_ne!(wc.change_id, before.change_id);
    assert_eq!(fixture.parents_of(&wc), [fixture.latest(&two).commit]);
    assert_eq!(
        fixture.held_in(&fixture.latest(&one), "a.txt"),
        Held::File(b"1\n2\nTHREE\n4\n5\n".to_vec())
    );
}

#[test]
fn paths_limit_the_hunks() {
    let (fixture, _, two) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let absorbed = open(&fixture)
        .absorb(&[Utf8PathBuf::from("b.txt")], None, &[], false, None)
        .expect("the hunks are listed");
    assert_eq!(summary(&absorbed), [(1, "b.txt", [1, 2, 1, 2], Some(short(&two)))]);
}

#[test]
fn into_limits_the_commits() {
    let (fixture, _, two) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let absorbed = open(&fixture)
        .absorb(&[], Some(&two.commit), &[], false, None)
        .expect("the hunks are listed");
    assert_eq!(
        summary(&absorbed),
        [(1, "a.txt", [2, 3, 2, 3], None), (2, "b.txt", [1, 2, 1, 2], Some(short(&two)))]
    );
}

fn refusal(result: Result<Absorbed, VcsError>) -> String {
    match result {
        Err(VcsError::Backend { source, .. }) => source
            .downcast_ref::<JjError>()
            .map_or_else(|| format!("not a JjError: {source}"), ToString::to_string),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_choice_of_a_missing_hunk_or_a_foreign_commit_is_refused_before_any_write() {
    let (fixture, ..) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let operations = fixture.operation_count();
    for (choice, message) in [
        (HunkChoice::Keep(3), "no hunk 3; there are 2"),
        (HunkChoice::Keep(0), "no hunk 0; there are 2"),
        (
            HunkChoice::Move { hunk: 1, rev: "@".to_owned() },
            "@ is not a mutable ancestor of the working copy in --into",
        ),
    ] {
        assert_eq!(refusal(open(&fixture).absorb(&[], None, &[choice], true, None)), message);
    }
    // The snapshot of the edits is the one operation the refusals leave.
    assert_eq!(fixture.operation_count(), operations + 1);
    assert_eq!(
        fixture.held_in(&fixture.working_copy_commit(), "b.txt"),
        Held::File(b"x\nY\nz\n".to_vec())
    );
}

#[test]
fn a_listing_names_the_operation_it_read_and_apply_at_it_takes_its_choices() {
    let (fixture, one, two) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let listing = listed(&fixture, &[], false);
    assert_eq!(listing.operation, fixture.operation_ids()[0], "the snapshot's operation");
    let applied = open(&fixture)
        .absorb(&[], None, &[HunkChoice::Keep(2)], true, Some(&listing.operation[..12]))
        .expect("the hunks move");
    assert!(applied.rewrite.written);
    assert_eq!(
        fixture.held_in(&fixture.latest(&one), "a.txt"),
        Held::File(b"1\n2\nTHREE\n4\n5\n".to_vec())
    );
    assert_eq!(fixture.held_in(&fixture.latest(&two), "b.txt"), Held::File(b"x\ny\nz\n".to_vec()));
    assert_eq!(
        fixture.held_in(&fixture.working_copy_commit(), "b.txt"),
        Held::File(b"x\nY\nz\n".to_vec())
    );
}

#[test]
fn apply_after_the_repository_moved_past_the_listing_is_refused_before_any_write() {
    let (fixture, one, _) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let listing = listed(&fixture, &[], false);
    // A new first hunk: the listing's hunk 2 is hunk 3 now.
    std::fs::write(fixture.root().join("a.txt"), b"ONE\n2\nTHREE\n4\n5\n").expect("written");
    let operations = fixture.operation_count();
    let refused = refusal(open(&fixture).absorb(
        &[],
        None,
        &[HunkChoice::Keep(2)],
        true,
        Some(&listing.operation),
    ));
    assert_eq!(
        refused,
        format!(
            "the repository changed since operation {}; list the hunks again",
            &listing.operation[..vcs_jj::SHORT_OPERATION]
        )
    );
    // The snapshot of the edit is the one operation the refusal leaves.
    assert_eq!(fixture.operation_count(), operations + 1);
    assert_eq!(
        fixture.held_in(&fixture.latest(&one), "a.txt"),
        Held::File(b"1\n2\n3\n4\n5\n".to_vec())
    );
}

#[test]
fn apply_at_an_unknown_operation_is_refused() {
    let (fixture, ..) = stack(b"1\n2\nTHREE\n4\n5\n", b"x\nY\nz\n");
    let refused = refusal(open(&fixture).absorb(&[], None, &[], true, Some("not-an-op")));
    assert!(refused.starts_with("no operation not-an-op: "), "{refused}");
}
