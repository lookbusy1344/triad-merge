//! `record_save` and `snapshot` on git: a resolved save takes the place of the conflict's
//! stages, as `git add` does (spec §7.4, `DESIGN.md` §D5.5).

use camino::Utf8Path;
use gix::index::entry::{Mode, Stage};
use vcs::testing::Entry;
use vcs::{Repository, VcsError};
use vcs_git::testing::Fixture;

const BASE: &[u8] = b"a\n";
const OURS: &[u8] = b"left\n";
const THEIRS: &[u8] = b"right\n";
const MERGED: &[u8] = b"merged\n";
const STAGE_0: u32 = 0;
const STAGE_1: u32 = 1;
const STAGE_2: u32 = 2;
const STAGE_3: u32 = 3;

fn both_changed() -> [Option<Entry<'static>>; 3] {
    [BASE, OURS, THEIRS].map(|bytes| Some(Entry::File(bytes)))
}

/// `clean.txt` at stage 0, and `a.txt` and `b.txt` conflicted at stages 1 to 3.
fn two_conflicts() -> Fixture {
    let fixture = Fixture::new();
    fixture.commit("one", &[("clean.txt", b"c\n")]);
    fixture.stage_conflicts(
        &[("clean.txt", b"c\n")],
        &[("a.txt", both_changed()), ("b.txt", both_changed())],
    );
    fixture
}

fn index(fixture: &Fixture) -> gix::index::File {
    gix::open(fixture.root())
        .expect("the repository opens")
        .open_index()
        .expect("the index is read")
}

/// Each index entry as its path, stage and blob.
fn entries(fixture: &Fixture) -> Vec<(String, u32, Vec<u8>)> {
    let repo = gix::open(fixture.root()).expect("the repository opens");
    let index = index(fixture);
    index
        .entries()
        .iter()
        .map(|entry| {
            let blob = repo.find_object(entry.id).expect("the blob").detach().data;
            (entry.path(&index).to_string(), stage_number(entry.stage()), blob)
        })
        .collect()
}

fn stage_number(stage: Stage) -> u32 {
    match stage {
        Stage::Unconflicted => STAGE_0,
        Stage::Base => STAGE_1,
        Stage::Ours => STAGE_2,
        Stage::Theirs => STAGE_3,
    }
}

fn entry(path: &str, stage: u32, bytes: &[u8]) -> (String, u32, Vec<u8>) {
    (path.to_owned(), stage, bytes.to_vec())
}

fn index_bytes(fixture: &Fixture) -> Vec<u8> {
    std::fs::read(fixture.root().join(".git/index")).expect("the index file is read")
}

fn record(fixture: &Fixture, path: &str, resolved: bool) -> Result<(), VcsError> {
    fixture.open().record_save(Utf8Path::new(path), resolved)
}

#[test]
fn a_resolved_save_replaces_the_stages_with_one_entry() {
    let fixture = two_conflicts();
    fixture.write("a.txt", MERGED);
    record(&fixture, "a.txt", true).expect("the resolution is staged");
    assert_eq!(
        entries(&fixture),
        [
            entry("a.txt", STAGE_0, MERGED),
            entry("b.txt", STAGE_1, BASE),
            entry("b.txt", STAGE_2, OURS),
            entry("b.txt", STAGE_3, THEIRS),
            entry("clean.txt", STAGE_0, b"c\n"),
        ]
    );
    assert_eq!(fixture.open().conflicts().expect("listed"), ["b.txt"]);
}

#[test]
fn a_save_with_markers_leaves_the_stages() {
    let fixture = two_conflicts();
    let before = index_bytes(&fixture);
    fixture.write("a.txt", b"<<<<<<< ours\nleft\n=======\nright\n>>>>>>> theirs\n");
    record(&fixture, "a.txt", false).expect("nothing to record");
    assert_eq!(index_bytes(&fixture), before);
}

#[test]
fn a_snapshot_writes_nothing() {
    let fixture = two_conflicts();
    let before = index_bytes(&fixture);
    fixture.write("a.txt", MERGED);
    fixture.write("new.txt", b"new\n");
    fixture.open().snapshot().expect("nothing to snapshot");
    assert_eq!(index_bytes(&fixture), before);
}

#[test]
fn a_path_without_stages_is_left_alone() {
    let fixture = two_conflicts();
    let before = index_bytes(&fixture);
    fixture.write("clean.txt", b"edited\n");
    fixture.write("untracked.txt", b"new\n");
    record(&fixture, "clean.txt", true).expect("nothing to record");
    record(&fixture, "untracked.txt", true).expect("nothing to record");
    assert_eq!(index_bytes(&fixture), before);
}

#[test]
fn the_entry_carries_the_files_size_and_modification_time() {
    let fixture = two_conflicts();
    let file = fixture.write("a.txt", MERGED);
    record(&fixture, "a.txt", true).expect("the resolution is staged");
    let metadata = std::fs::symlink_metadata(&file).expect("the file exists");
    let modified = metadata
        .modified()
        .expect("a modification time")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch");
    let index = index(&fixture);
    let staged = index
        .entry_by_path_and_stage("a.txt".into(), Stage::Unconflicted)
        .expect("a stage-0 entry");
    assert_eq!(u64::from(staged.stat.size), metadata.len());
    assert_eq!(u64::from(staged.stat.mtime.secs), modified.as_secs());
    assert_eq!(staged.mode, Mode::FILE);
}

#[test]
fn line_endings_are_normalised_as_git_add_does() {
    let fixture = two_conflicts();
    fixture.write(".gitattributes", b"a.txt text\n");
    fixture.write("a.txt", b"x\r\ny\r\n");
    record(&fixture, "a.txt", true).expect("the resolution is staged");
    assert_eq!(entries(&fixture)[0], entry("a.txt", STAGE_0, b"x\ny\n"));
}

#[cfg(unix)]
fn make_executable(path: &Utf8Path) {
    use std::os::unix::fs::PermissionsExt as _;
    const EXECUTABLE: u32 = 0o755;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(EXECUTABLE))
        .expect("the mode is set");
}

#[cfg(unix)]
#[test]
fn an_executable_file_is_staged_executable() {
    let fixture = two_conflicts();
    let file = fixture.write("a.txt", MERGED);
    make_executable(&file);
    record(&fixture, "a.txt", true).expect("the resolution is staged");
    let staged = index(&fixture)
        .entry_by_path_and_stage("a.txt".into(), Stage::Unconflicted)
        .map(|entry| entry.mode);
    assert_eq!(staged, Some(Mode::FILE_EXECUTABLE));
}

#[cfg(unix)]
#[test]
fn without_core_file_mode_the_bit_comes_from_ours() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("x", b"x\n")]);
    let theirs_executable =
        [Some(Entry::File(BASE)), Some(Entry::File(OURS)), Some(Entry::Executable(THEIRS))];
    fixture.stage_conflicts(&[], &[("a.txt", theirs_executable)]);
    fixture.append_config("[core]\n\tfileMode = false\n");
    let file = fixture.write("a.txt", MERGED);
    make_executable(&file);
    record(&fixture, "a.txt", true).expect("the resolution is staged");
    let staged = index(&fixture)
        .entry_by_path_and_stage("a.txt".into(), Stage::Unconflicted)
        .map(|entry| entry.mode);
    assert_eq!(staged, Some(Mode::FILE));
}

#[test]
fn a_locked_index_is_an_error_and_stays_as_it_was() {
    let fixture = two_conflicts();
    let before = index_bytes(&fixture);
    let lock = fixture.write(".git/index.lock", b"");
    fixture.write("a.txt", MERGED);
    let error = record(&fixture, "a.txt", true).expect_err("the index is locked");
    assert!(
        matches!(&error, VcsError::Backend { context, .. } if *context == "writing the index"),
        "{error:?}"
    );
    std::fs::remove_file(lock).expect("the lock is removed");
    assert_eq!(index_bytes(&fixture), before);
}

#[test]
fn the_index_is_read_under_its_lock() {
    // A writer between the read and the write would be lost, so the lock comes first: with
    // the lock taken, the index is not read at all, and its damage goes unseen.
    let fixture = two_conflicts();
    let lock = fixture.write(".git/index.lock", b"");
    fixture.write(".git/index", b"not an index");
    fixture.write("a.txt", MERGED);
    let error = record(&fixture, "a.txt", true).expect_err("the index is locked");
    assert!(
        matches!(&error, VcsError::Backend { context, .. } if *context == "writing the index"),
        "{error:?}"
    );
    std::fs::remove_file(lock).expect("the lock is removed");
    assert_eq!(index_bytes(&fixture), b"not an index");
}

#[test]
fn a_path_outside_the_repository_is_refused() {
    let fixture = two_conflicts();
    let error = record(&fixture, "../a.txt", true).expect_err("outside");
    assert!(matches!(&error, VcsError::OutsideRepository(path) if path == "../a.txt"), "{error:?}");
}
