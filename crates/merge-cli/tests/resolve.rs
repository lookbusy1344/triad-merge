//! `resolve`'s targets: the conflicted files it lists and opens (`DESIGN.md` §D5.3).

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use merge_session::repository::RunRepository;
use merge_session::resolve::ResolveError;
use vcs::VcsError;
use vcs::testing::Entry;

/// git shortens commit IDs to this many characters in a small repository.
const GIT_SHORT: usize = 7;

const BASE: &[u8] = b"a\n";
const LEFT: &[u8] = b"left\n";
const RIGHT: &[u8] = b"right\n";

fn canonical(path: &Utf8Path) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dunce::canonicalize(path).expect("the path exists")).expect("UTF-8")
}

/// `one`, then `left` on it as `HEAD` on `main`, `right` on `one` beside it, and the index
/// stopped on `conflicts` with `clean` at stage 0. Returns the short IDs of `one` and `right`.
fn git_stopped(
    clean: &[(&str, &[u8])],
    conflicts: &[(&str, [Option<Entry>; 3])],
) -> (vcs_git::testing::Fixture, [String; 2]) {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[("a.txt", BASE)]);
    fixture.commit("left", &[("a.txt", LEFT)]);
    let right = fixture.commit_on(&[one], "right", &[("a.txt", RIGHT)]);
    fixture.stage_conflicts(clean, conflicts);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    let short = |id: gix::ObjectId| id.to_string()[..GIT_SHORT].to_owned();
    (fixture, [short(one), short(right)])
}

fn files(terms: [Option<&'static [u8]>; 3]) -> [Option<Entry<'static>>; 3] {
    terms.map(|t| t.map(Entry::File))
}

const BOTH_CHANGED: [Option<&[u8]>; 3] = [Some(BASE), Some(LEFT), Some(RIGHT)];

fn target(cwd: &Utf8Path, path: Option<&str>) -> (Vec<std::path::PathBuf>, String) {
    let mut out = Vec::new();
    let (cwd, path) = (cwd.as_std_path(), path.map(Path::new));
    let mut repo = RunRepository::of_resolve(cwd, path);
    let found =
        merge_cli::resolve_targets(&mut repo, cwd, path, &mut out).expect("the targets are found");
    (found, String::from_utf8(out).expect("UTF-8"))
}

#[test]
fn without_a_path_every_conflicted_file_is_listed_and_targeted() {
    let (fixture, _) = git_stopped(
        &[("clean.txt", b"c\n")],
        &[("z.txt", files(BOTH_CHANGED)), ("src/b.txt", files(BOTH_CHANGED))],
    );
    let (found, out) = target(fixture.root(), None);
    assert_eq!(out, "src/b.txt\nz.txt\n");
    let root = canonical(fixture.root());
    assert_eq!(
        found,
        [root.join("src/b.txt").into_std_path_buf(), root.join("z.txt").into_std_path_buf()]
    );
}

#[test]
fn a_given_path_is_the_target_and_nothing_is_listed() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let (found, out) = target(fixture.root(), Some("a.txt"));
    assert_eq!(out, "");
    assert_eq!(found, [fixture.root().join("a.txt").into_std_path_buf()]);
}

#[test]
fn listing_outside_a_repository_is_an_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let error = merge_cli::resolve_targets(
        &mut RunRepository::of_resolve(dir.path(), None),
        dir.path(),
        None,
        &mut Vec::new(),
    )
    .expect_err("no repository");
    assert!(
        matches!(
            error,
            merge_cli::CliError::Resolve(ResolveError::Vcs(VcsError::NotARepository(_)))
        ),
        "{error:?}"
    );
}

mod jj {
    use super::*;

    #[test]
    fn without_a_path_or_a_conflict_nothing_opens() {
        let fixture = vcs_jj::testing::Fixture::new();
        fixture.commit("one", &[("a.txt", BASE)]);
        fixture.check_out_to_disk();
        let (found, out) = target(fixture.root(), None);
        assert_eq!(out, "no conflicted files\n");
        assert_eq!(found, Vec::<std::path::PathBuf>::new());
    }
}
