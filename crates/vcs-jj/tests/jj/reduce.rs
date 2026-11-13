//! `reduce_pair` on jj: two adjacent sides of a conflict of three merged over the base
//! between them (spec §7.5.3).

use camino::Utf8Path;
use vcs::{ConflictTerm, TermContent, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture, Held};
use vcs_jj::{JjError, JjRepository};

const BASE: &[u8] = b"a\n";
const ONE: &[u8] = b"one\n";
const TWO: &[u8] = b"two\n";
const THREE: &[u8] = b"three\n";
const MERGED: &[u8] = b"merged\n";

fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

fn id(commit: &Committed) -> vcs::CommitId {
    vcs::CommitId::new(commit.commit.clone())
}

fn a_txt() -> &'static Utf8Path {
    Utf8Path::new("a.txt")
}

/// A merge of three parents that each changed `a.txt` from `BASE`: a conflict of sides
/// `ONE`, `TWO`, `THREE` over two copies of `BASE`.
fn three_sides(fixture: &Fixture) -> Committed {
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let parents: Vec<Committed> = [("one", ONE), ("two", TWO), ("three", THREE)]
        .into_iter()
        .map(|(name, bytes)| fixture.commit_on(&[&base], name, &[("a.txt", bytes)]))
        .collect();
    let parents: Vec<&Committed> = parents.iter().collect();
    fixture.commit_merged(&parents, "merge", &[])
}

fn bytes(terms: &[ConflictTerm]) -> Vec<Vec<u8>> {
    terms
        .iter()
        .map(|term| match &term.content {
            TermContent::File { bytes, .. } => bytes.clone(),
            other => panic!("a file term, got {other:?}"),
        })
        .collect()
}

/// The sides and bases of `a.txt` in `commit`'s change now.
fn terms(fixture: &Fixture, commit: &Committed) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let latest = fixture.latest(commit);
    let conflict =
        open(fixture).conflict_at(&id(&latest), a_txt()).expect("reads").expect("a conflict");
    (bytes(&conflict.sides), bytes(&conflict.bases))
}

#[test]
fn each_pair_becomes_one_side_over_the_other_base() {
    let cases: [(usize, &[&[u8]]); 2] = [(1, &[MERGED, THREE]), (2, &[ONE, MERGED])];
    for (pair, sides) in cases {
        for fixture in [Fixture::colocated(), Fixture::internal()] {
            let merge = three_sides(&fixture);
            assert_eq!(terms(&fixture, &merge).0.len(), 3, "three sides before");
            let rewrite = open(&fixture)
                .reduce_pair(&id(&merge), a_txt(), pair, MERGED, false)
                .expect("the pair reduces");
            assert!(rewrite.written && !rewrite.resolved, "pair {pair}: {rewrite:?}");
            assert_eq!(rewrite.sides, Some(2), "pair {pair}");
            let expected: Vec<Vec<u8>> = sides.iter().map(|s| s.to_vec()).collect();
            assert_eq!(terms(&fixture, &merge), (expected, vec![BASE.to_vec()]), "pair {pair}");
            assert_eq!(
                fixture.operations()[0].description,
                format!(
                    "reduce a.txt in {} in triad",
                    &merge.change_id[..rewrite.target.id(vcs::IdKind::Change).len()]
                )
            );
        }
    }
}

#[test]
fn a_pair_merged_to_the_base_leaves_the_third_side_resolved() {
    let fixture = Fixture::new();
    let merge = three_sides(&fixture);
    let rewrite =
        open(&fixture).reduce_pair(&id(&merge), a_txt(), 1, BASE, false).expect("the pair reduces");
    assert!(rewrite.written && rewrite.resolved, "{rewrite:?}");
    assert_eq!(rewrite.sides, Some(1));
    assert_eq!(fixture.held_in(&fixture.latest(&merge), "a.txt"), Held::File(THREE.to_vec()));
}

#[test]
fn a_dry_run_writes_nothing() {
    let fixture = Fixture::new();
    let merge = three_sides(&fixture);
    let operations = fixture.operation_count();
    let rewrite =
        open(&fixture).reduce_pair(&id(&merge), a_txt(), 2, MERGED, true).expect("a dry run");
    assert!(!rewrite.written);
    assert_eq!(rewrite.sides, Some(2));
    assert_eq!(fixture.operation_count(), operations);
    assert_eq!(terms(&fixture, &merge).0.len(), 3);
}

fn refusal(result: Result<vcs_jj::Rewrite, VcsError>) -> String {
    match result {
        Err(VcsError::Backend { source, .. }) => source
            .downcast_ref::<JjError>()
            .map_or_else(|| format!("not a JjError: {source}"), ToString::to_string),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_pair_out_of_range_is_refused_before_any_write() {
    for (pair, message) in [
        (0, "pair 0 needs sides 0 and 1; the conflict has 3"),
        (3, "pair 3 needs sides 3 and 4; the conflict has 3"),
    ] {
        let fixture = Fixture::new();
        let merge = three_sides(&fixture);
        let operations = fixture.operation_count();
        let refused =
            refusal(open(&fixture).reduce_pair(&id(&merge), a_txt(), pair, MERGED, false));
        assert_eq!(refused, message);
        assert_eq!(fixture.operation_count(), operations, "pair {pair}");
    }
}

#[test]
fn a_conflict_of_two_sides_is_refused() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let one = fixture.commit_on(&[&base], "one", &[("a.txt", ONE)]);
    let two = fixture.commit_on(&[&base], "two", &[("a.txt", TWO)]);
    let merge = fixture.commit_merged(&[&one, &two], "merge", &[]);
    let refused = refusal(open(&fixture).reduce_pair(&id(&merge), a_txt(), 1, MERGED, false));
    assert_eq!(refused, "a.txt has two sides; resolve it without --pair");
}

#[test]
fn a_resolved_path_is_refused() {
    let fixture = Fixture::new();
    let commit = fixture.commit_on(&[], "c", &[("a.txt", BASE)]);
    let refused = open(&fixture).reduce_pair(&id(&commit), a_txt(), 1, MERGED, false);
    assert!(
        matches!(&refused, Err(VcsError::NotConflicted { path, .. }) if path == "a.txt"),
        "{refused:?}"
    );
}
