//! Conflicts as `gix-merge`'s tree merge leaves them in the index: renames and a criss-cross
//! (`DESIGN.md` §D5.2). The expected stages follow git's `ort` strategy.

use camino::{Utf8Path, Utf8PathBuf};
use gix::merge::blob::builtin_driver::text::Labels;
use gix::merge::tree::TreatAsUnresolved;
use gix::merge::tree::apply_index_entries::RemovalMode;
use vcs::{Conflict, Repository, Term, TermContent};
use vcs_git::testing::Fixture;

/// Ten lines, enough that one changed line keeps a renamed file similar to its base.
const BASE: &[u8] = b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
const OURS: &[u8] = b"ours\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
const THEIRS: &[u8] = b"theirs\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";

/// Merge `theirs` into `HEAD` with `gix-merge`, then leave the index and `MERGE_HEAD` as
/// `git merge` leaves them when it stops on the conflicts.
fn stop_merge(fixture: &Fixture, theirs: gix::ObjectId) {
    let repo = gix::open(fixture.root()).expect("the repository opens");
    let ours = repo.head_id().expect("HEAD is a commit").detach();
    let options = repo.tree_merge_options().expect("the merge options");
    let outcome = repo
        .merge_commits(ours, theirs, Labels::default(), options.into())
        .expect("the merge runs");
    let mut tree_merge = outcome.tree_merge;
    assert!(tree_merge.has_unresolved_conflicts(TreatAsUnresolved::git()));
    let tree = tree_merge.tree.write().expect("the tree is written");
    let mut index = repo.index_from_tree(&tree).expect("an index");
    tree_merge.index_changed_after_applying_conflicts(
        &mut index,
        TreatAsUnresolved::git(),
        RemovalMode::Prune,
    );
    index.write(gix::index::write::Options::default()).expect("the index is written");
    fixture.write_pseudo_ref("MERGE_HEAD", theirs);
}

fn file(bytes: &[u8]) -> TermContent {
    TermContent::File { bytes: bytes.to_vec(), executable: false }
}

/// The contents of `path`'s sides, then its bases.
fn contents(conflict: &Conflict) -> (Vec<TermContent>, Vec<TermContent>) {
    let of = |terms: &[vcs::ConflictTerm]| terms.iter().map(|t| t.content.clone()).collect();
    (of(&conflict.sides), of(&conflict.bases))
}

fn read(fixture: &Fixture, path: &str) -> Option<Conflict> {
    fixture.open().read_conflict(Utf8Path::new(path)).expect("the conflict reads")
}

#[test]
fn a_renamed_and_modified_file_conflicts_at_its_new_name() {
    let fixture = Fixture::new();
    let one = fixture.commit("one", &[("a.txt", BASE)]);
    fixture.commit("ours", &[("b.txt", OURS)]);
    let theirs = fixture.commit_on(&[one], "theirs", &[("a.txt", THEIRS)]);
    stop_merge(&fixture, theirs);
    let repo = fixture.open();
    assert_eq!(repo.conflicts().expect("the conflicts list"), [Utf8PathBuf::from("b.txt")]);
    assert_eq!(read(&fixture, "a.txt"), None);
    let conflict = read(&fixture, "b.txt").expect("b.txt is conflicted");
    assert_eq!(contents(&conflict), (vec![file(OURS), file(THEIRS)], vec![file(BASE)]));
}

#[test]
fn a_file_renamed_two_ways_leaves_one_stage_at_each_name() {
    // git's ort strategy: the base stays at the old name, and each side sits at the name it
    // chose, the others absent.
    let fixture = Fixture::new();
    let one = fixture.commit("one", &[("a.txt", BASE)]);
    fixture.commit("ours", &[("b.txt", BASE)]);
    let theirs = fixture.commit_on(&[one], "theirs", &[("c.txt", BASE)]);
    stop_merge(&fixture, theirs);
    assert_eq!(
        fixture.open().conflicts().expect("the conflicts list"),
        ["a.txt", "b.txt", "c.txt"].map(Utf8PathBuf::from)
    );
    let absent = TermContent::Absent;
    for (path, expected) in [
        ("a.txt", (vec![absent.clone(), absent.clone()], vec![file(BASE)])),
        ("b.txt", (vec![file(BASE), absent.clone()], vec![absent.clone()])),
        ("c.txt", (vec![absent.clone(), file(BASE)], vec![absent.clone()])),
    ] {
        let conflict = read(&fixture, path).expect("conflicted");
        assert_eq!(contents(&conflict), expected, "{path}");
    }
}

#[test]
fn a_criss_cross_conflict_takes_the_virtual_merge_base() {
    // A and B change the first and last lines. Each merges the other, and the two merges
    // change the middle line apart. The two merge bases, A and B, merge cleanly into the
    // virtual base, which is neither of them.
    let fixture = Fixture::new();
    let one = fixture.commit("one", &[("a.txt", b"a\nb\nc\n")]);
    let a = fixture.commit("A", &[("a.txt", b"A\nb\nc\n")]);
    let b = fixture.commit_on(&[one], "B", &[("a.txt", b"a\nb\nC\n")]);
    fixture.merge(&[a, b], "ours", &[("a.txt", b"A\nours\nC\n")]);
    let theirs = fixture.commit_on(&[b, a], "theirs", &[("a.txt", b"A\ntheirs\nC\n")]);
    stop_merge(&fixture, theirs);
    let conflict = read(&fixture, "a.txt").expect("a.txt is conflicted");
    assert_eq!(
        contents(&conflict),
        (vec![file(b"A\nours\nC\n"), file(b"A\ntheirs\nC\n")], vec![file(b"A\nb\nC\n")])
    );
    // No one commit is the base, so its term names none.
    assert_eq!(conflict.bases[0].term, Term::default());
}
