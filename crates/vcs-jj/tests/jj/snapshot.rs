//! `snapshot` and `record_save` on jj: the working copy taken in before `resolve` reads a
//! conflict, and again after each save, each in an operation of its own (`PLAN.md` §D5.5).

use std::collections::HashMap;

use camino::Utf8Path;
use vcs::{Repository, TermContent, VcsError};
use vcs_jj::JjRepository;
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Fixture, Held, Op};

const BASE: &[u8] = b"a\n";
const LEFT: &[u8] = b"left\n";
const RIGHT: &[u8] = b"right\n";
const MERGED: &[u8] = b"merged\n";
const EDITED: &[u8] = b"edited\n";
const SNAPSHOT: &str = "snapshot working copy";
/// A size limit for new files, in bytes, and files on either side of it.
const MAX_NEW_FILE_SIZE: usize = 10;

/// A conflict in `a.txt` between `left` and `right`, with `other.txt` clean, checked out to
/// disk as jj leaves it.
fn conflict_on_disk(fixture: Fixture) -> Fixture {
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE), ("other.txt", b"o\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT), ("other.txt", b"o\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", RIGHT), ("other.txt", b"o\n")]);
    fixture.check_out(&[&left, &right]);
    fixture.check_out_to_disk();
    fixture
}

fn both_stores() -> [(Fixture, &'static str); 2] {
    [
        (conflict_on_disk(Fixture::colocated()), "colocated"),
        (conflict_on_disk(Fixture::internal()), "internal"),
    ]
}

/// The repository, with no user config: jj's defaults.
fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

/// The repository, with `toml` as the user's jj config.
fn open_with_config(fixture: &Fixture, dir: &tempfile::TempDir, toml: &str) -> JjRepository {
    let path = dir.path().join("config.toml");
    std::fs::write(&path, toml).expect("the config is written");
    let environment =
        HashMap::from([("JJ_CONFIG".to_owned(), path.to_str().expect("UTF-8").to_owned())]);
    fixture.open().with_config(ConfigEnv { environment, ..ConfigEnv::default() })
}

/// `a.txt` as jj wrote it, with the left side's line changed to `LEFT` inside the markers.
fn edit_inside_markers(fixture: &Fixture) {
    let on_disk = std::fs::read_to_string(fixture.root().join("a.txt")).expect("a.txt reads");
    assert!(on_disk.contains("+left\n"), "jj's diff style: {on_disk}");
    fixture.write("a.txt", on_disk.replace("+left\n", "+LEFT\n").as_bytes());
}

fn first_side(repo: &JjRepository) -> TermContent {
    repo.read_conflict(Utf8Path::new("a.txt"))
        .expect("the conflict reads")
        .expect("a.txt is conflicted")
        .sides
        .swap_remove(0)
        .content
}

fn file(bytes: &[u8]) -> TermContent {
    TermContent::File { bytes: bytes.to_vec(), executable: false }
}

fn held(bytes: &[u8]) -> Held {
    Held::File(bytes.to_vec())
}

#[test]
fn a_snapshot_takes_in_unrecorded_edits_in_one_operation() {
    for (fixture, store) in both_stores() {
        let before = fixture.operations().len();
        edit_inside_markers(&fixture);
        fixture.write("other.txt", EDITED);
        let mut repo = open(&fixture);
        repo.snapshot().expect("the snapshot is taken");
        let ops = fixture.operations();
        assert_eq!(ops.len(), before + 1, "{store}");
        assert_eq!(ops[0], Op { description: SNAPSHOT.to_owned(), is_snapshot: true }, "{store}");
        assert_eq!(fixture.held_at(0, "other.txt"), held(EDITED), "{store}");
        assert_eq!(fixture.held_at(0, "a.txt"), Held::Conflict, "{store}");
        // The repository reads at the new operation: the edit is in the conflict.
        assert_eq!(first_side(&repo), file(b"LEFT\n"), "{store}");
    }
}

#[test]
fn a_snapshot_of_an_unchanged_working_copy_writes_no_operation() {
    for (fixture, store) in both_stores() {
        let before = fixture.operations();
        open(&fixture).snapshot().expect("nothing to snapshot");
        assert_eq!(fixture.operations(), before, "{store}");
    }
}

#[test]
fn a_save_is_a_second_operation_that_changes_only_its_path() {
    for (fixture, store) in both_stores() {
        let before = fixture.operations().len();
        fixture.write("other.txt", EDITED);
        let mut repo = open(&fixture);
        repo.snapshot().expect("the snapshot is taken");
        fixture.write("a.txt", MERGED);
        repo.record_save(Utf8Path::new("a.txt"), true).expect("the save is recorded");
        let ops = fixture.operations();
        assert_eq!(ops.len(), before + 2, "{store}");
        assert_eq!(
            ops[0],
            Op { description: "save a.txt in triad".to_owned(), is_snapshot: false },
            "{store}"
        );
        assert_eq!(fixture.held_at(0, "a.txt"), held(MERGED), "{store}");
        assert_eq!(fixture.held_at(0, "other.txt"), held(EDITED), "{store}");
        // `jj undo` restores the operation before the save: the conflict, with the edit.
        assert_eq!(fixture.held_at(1, "a.txt"), Held::Conflict, "{store}");
        assert_eq!(fixture.held_at(1, "other.txt"), held(EDITED), "{store}");
        assert_eq!(repo.conflicts().expect("listed"), Vec::<camino::Utf8PathBuf>::new(), "{store}");
    }
}

#[test]
fn a_save_with_markers_stays_a_conflict_with_the_edit() {
    for (fixture, store) in both_stores() {
        let mut repo = open(&fixture);
        repo.snapshot().expect("nothing to snapshot");
        edit_inside_markers(&fixture);
        repo.record_save(Utf8Path::new("a.txt"), false).expect("the save is recorded");
        assert_eq!(fixture.held_at(0, "a.txt"), Held::Conflict, "{store}");
        assert_eq!(first_side(&repo), file(b"LEFT\n"), "{store}");
    }
}

#[test]
fn a_file_resolved_by_hand_is_clean_after_the_snapshot() {
    let fixture = conflict_on_disk(Fixture::new());
    fixture.write("a.txt", MERGED);
    let mut repo = open(&fixture);
    repo.snapshot().expect("the snapshot is taken");
    assert_eq!(repo.read_conflict(Utf8Path::new("a.txt")).expect("the path reads"), None);
    assert_eq!(fixture.held_at(0, "a.txt"), held(MERGED));
}

#[test]
fn an_ignored_new_file_stays_untracked() {
    let fixture = conflict_on_disk(Fixture::new());
    fixture.write(".gitignore", b"ignored.txt\n");
    fixture.write("ignored.txt", b"i\n");
    fixture.write("new.txt", b"n\n");
    open(&fixture).snapshot().expect("the snapshot is taken");
    assert_eq!(fixture.held_at(0, "ignored.txt"), Held::Absent);
    assert_eq!(fixture.held_at(0, "new.txt"), held(b"n\n"));
}

#[test]
fn a_new_file_over_the_size_limit_stays_untracked() {
    let fixture = conflict_on_disk(Fixture::new());
    let big = "b".repeat(MAX_NEW_FILE_SIZE + 1);
    let small = "s".repeat(MAX_NEW_FILE_SIZE);
    fixture.write("big.txt", big.as_bytes());
    fixture.write("small.txt", small.as_bytes());
    let dir = tempfile::tempdir().expect("a temporary directory");
    let toml = format!("snapshot.max-new-file-size = {MAX_NEW_FILE_SIZE}\n");
    open_with_config(&fixture, &dir, &toml).snapshot().expect("the snapshot is taken");
    assert_eq!(fixture.held_at(0, "big.txt"), Held::Absent);
    assert_eq!(fixture.held_at(0, "small.txt"), held(small.as_bytes()));
}

#[test]
fn with_auto_track_none_a_new_file_stays_untracked_and_an_edit_is_taken() {
    let fixture = conflict_on_disk(Fixture::new());
    fixture.write("new.txt", b"n\n");
    fixture.write("other.txt", EDITED);
    let dir = tempfile::tempdir().expect("a temporary directory");
    open_with_config(&fixture, &dir, "snapshot.auto-track = \"none()\"\n")
        .snapshot()
        .expect("the snapshot is taken");
    assert_eq!(fixture.held_at(0, "new.txt"), Held::Absent);
    assert_eq!(fixture.held_at(0, "other.txt"), held(EDITED));
}

#[test]
fn a_stale_working_copy_is_refused_before_any_operation() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT)]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", RIGHT)]);
    fixture.check_out(&[&left, &right]);
    fixture.check_out_to_disk();
    // `@` moves on without the files, as from another workspace.
    fixture.check_out(&[&left]);
    fixture.write("other.txt", EDITED);
    let before = fixture.operations();
    let mut repo = open(&fixture);
    let root = repo.root().to_owned();
    let error = repo.snapshot().expect_err("a stale working copy");
    assert!(matches!(&error, VcsError::StaleWorkspace(at) if *at == root), "{error:?}");
    assert_eq!(fixture.operations(), before);
}

/// A colocated workspace on `left` over `base`, checked out to disk, then
/// `git checkout --detach base`: git's HEAD at `base` and `a.txt` as git leaves it.
fn git_checked_out_base() -> (Fixture, String) {
    let fixture = Fixture::colocated();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT)]);
    fixture.check_out(&[&left]);
    fixture.check_out_to_disk();
    fixture.write(".git/HEAD", format!("{}\n", base.commit).as_bytes());
    fixture.write("a.txt", BASE);
    (fixture, base.commit)
}

const IMPORT: &str = "import git head";

#[test]
fn in_a_colocated_workspace_a_moved_git_head_is_imported_as_jj_imports_it() {
    let (fixture, base) = git_checked_out_base();
    let before = fixture.operations().len();
    open(&fixture).snapshot().expect("git's HEAD is imported");
    let ops = fixture.operations();
    assert_eq!(ops.len(), before + 1);
    assert_eq!(ops[0], Op { description: IMPORT.to_owned(), is_snapshot: false });
    // A new working-copy commit on git's HEAD, holding the files git left: nothing to
    // snapshot after the import.
    assert_eq!(fixture.parent_ids(0), [base]);
    assert_eq!(fixture.held_at(0, "a.txt"), held(BASE));
}

#[test]
fn edits_after_a_git_checkout_are_snapshotted_on_the_imported_head() {
    let (fixture, base) = git_checked_out_base();
    fixture.write("a.txt", EDITED);
    let before = fixture.operations().len();
    let mut repo = open(&fixture);
    repo.snapshot().expect("git's HEAD is imported");
    let ops = fixture.operations();
    assert_eq!(ops.len(), before + 2);
    assert_eq!(
        ops[..2],
        [
            Op { description: SNAPSHOT.to_owned(), is_snapshot: true },
            Op { description: IMPORT.to_owned(), is_snapshot: false }
        ]
    );
    assert_eq!(fixture.parent_ids(0), [base]);
    assert_eq!(fixture.held_at(0, "a.txt"), held(EDITED));
    assert_eq!(fixture.held_at(1, "a.txt"), held(BASE));
    // The repository reads at the newest operation.
    assert_eq!(repo.read_conflict(Utf8Path::new("a.txt")).expect("the path reads"), None);
}

#[test]
fn with_auto_track_none_a_file_git_checks_out_is_tracked_on_the_imported_head() {
    // The working copy's record moves to git's HEAD, so a file git wrote that `@` did not
    // hold is tracked although jj tracks no new files.
    let fixture = Fixture::colocated();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE), ("b.txt", b"b\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT)]);
    fixture.check_out(&[&left]);
    fixture.check_out_to_disk();
    fixture.write(".git/HEAD", format!("{}\n", base.commit).as_bytes());
    fixture.write("a.txt", BASE);
    fixture.write("b.txt", b"b\n");
    let before = fixture.operations().len();
    let dir = tempfile::tempdir().expect("a temporary directory");
    open_with_config(&fixture, &dir, "snapshot.auto-track = \"none()\"\n")
        .snapshot()
        .expect("git's HEAD is imported");
    assert_eq!(fixture.operations().len(), before + 1);
    assert_eq!(fixture.held_at(0, "b.txt"), held(b"b\n"));
}

#[test]
fn an_unmoved_git_head_imports_nothing() {
    let fixture = Fixture::colocated();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    fixture.check_out(&[&base]);
    fixture.check_out_to_disk();
    let before = fixture.operations();
    open(&fixture).snapshot().expect("nothing to import");
    assert_eq!(fixture.operations(), before);
    assert_eq!(fixture.parent_ids(0), [base.commit]);
}

#[test]
fn in_a_colocated_workspace_a_bookmark_the_snapshot_moves_is_exported_to_git() {
    // As jj exports after each command: git's branch follows the rewritten `@`.
    let fixture = conflict_on_disk(Fixture::colocated());
    fixture.set_bookmark_on_working_copy("topic");
    let before = fixture.working_copy_id();
    assert_eq!(fixture.git_branch("topic"), before);
    fixture.write("other.txt", EDITED);
    let mut repo = open(&fixture);
    repo.snapshot().expect("the snapshot is taken");
    let snapshotted = fixture.working_copy_id();
    assert_ne!(snapshotted, before, "the snapshot rewrites @");
    assert_eq!(fixture.git_branch("topic"), snapshotted);
    fixture.write("a.txt", MERGED);
    repo.record_save(Utf8Path::new("a.txt"), true).expect("the save is recorded");
    let saved = fixture.working_copy_id();
    assert_ne!(saved, snapshotted, "the save rewrites @");
    assert_eq!(fixture.git_branch("topic"), saved);
}

#[test]
fn a_save_outside_the_repository_is_refused() {
    let fixture = conflict_on_disk(Fixture::new());
    let error = open(&fixture).record_save(Utf8Path::new("../a.txt"), true).expect_err("outside");
    assert!(matches!(&error, VcsError::OutsideRepository(path) if path == "../a.txt"), "{error:?}");
}
