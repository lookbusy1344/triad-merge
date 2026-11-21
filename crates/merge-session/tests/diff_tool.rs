//! The working-copy file the right side of `jj diff --tool` stands for (spec §7.1).

use std::path::{Path, PathBuf};

use merge_core::diff2::Rope;
use merge_core::text::Encoding;
use merge_session::SessionError;
use merge_session::diff_tool::{SideSave, ToolPair, ToolSide, jj_working_copy, read_pair, save};
use merge_session::repository::RunRepository;
use vcs::Repository;

const PATH: &str = "a.txt";
const NESTED: &str = "src/a.txt";
const PARENT: &[u8] = b"a\nb\n";
const WORKING: &[u8] = b"a\nB\n";
const EDITED: &[u8] = b"a\nB\nc\n";

/// jj's `jj-diff-<random>` directory with `left/<path>` and `right/<path>`.
fn jj_diff_copies(path: &str, [left, right]: [&[u8]; 2]) -> (tempfile::TempDir, [PathBuf; 2]) {
    let dir = tempfile::Builder::new().prefix("jj-diff-").tempdir().expect("a temporary directory");
    let copies = ["left", "right"].map(|side| dir.path().join(side).join(path));
    for (copy, bytes) in copies.iter().zip([left, right]) {
        std::fs::create_dir_all(copy.parent().expect("a directory")).expect("directories");
        std::fs::write(copy, bytes).expect("the copy is written");
    }
    (dir, copies)
}

fn canonical(path: &Path) -> PathBuf {
    dunce::canonicalize(path).expect("the path exists")
}

const NOT_READ_ONLY: [bool; 2] = [false, false];

/// The text a [`ToolSide::File`] holds, its path and whether it is read-only; none for an
/// absent side.
fn file(side: &ToolSide) -> Option<(&str, &Path, bool)> {
    match side {
        ToolSide::File { path, text, read_only } => Some((text.content(), path, *read_only)),
        ToolSide::Absent { .. } => None,
    }
}

fn pair(paths: [PathBuf; 2], readonly: [bool; 2]) -> ToolPair {
    ToolPair { paths, absent: [None, None], readonly }
}

fn side_save(path: PathBuf, text: &str) -> SideSave {
    SideSave { path, encoding: Encoding::Utf8, text: Rope::from_str(text) }
}

#[test]
fn a_git_pair_opens_both_copies_as_given() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[(PATH, WORKING)]);
    fixture.write(PATH, WORKING);
    let (_dir, [left, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
    let root = fixture.root().as_std_path();
    let mut repo = RunRepository::find([root]);
    let [l, r] = read_pair(&pair([left.clone(), right.clone()], NOT_READ_ONLY), &mut repo, root)
        .expect("both sides read");
    assert_eq!(file(&l), Some(("a\nb\n", left.as_path(), false)));
    assert_eq!(file(&r), Some(("a\nB\n", right.as_path(), false)));
}

#[test]
fn an_absent_side_opens_empty_and_titled() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let right = dir.path().join(PATH);
    std::fs::write(&right, WORKING).expect("written");
    let missing = dir.path().join("missing.txt");
    let pair = ToolPair {
        paths: [missing, right.clone()],
        absent: [Some("no file".to_owned()), None],
        readonly: NOT_READ_ONLY,
    };
    let [l, r] =
        read_pair(&pair, &mut RunRepository::none(), dir.path()).expect("the right side reads");
    match l {
        ToolSide::Absent { title, text } => {
            assert_eq!((title.as_str(), text.content()), ("no file", ""));
        }
        ToolSide::File { path, .. } => panic!("the absent side, got {}", path.display()),
    }
    assert_eq!(file(&r), Some(("a\nB\n", right.as_path(), false)));
}

#[test]
fn an_unreadable_side_names_its_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let missing = dir.path().join("missing.txt");
    let pair = pair([missing.clone(), missing.clone()], NOT_READ_ONLY);
    let error = read_pair(&pair, &mut RunRepository::none(), dir.path()).unwrap_err();
    assert!(matches!(&error, SessionError::Read { path, .. } if *path == missing), "{error:?}");
}

#[test]
fn a_save_writes_each_side_and_snapshots_nothing_outside_jj() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let [a, b] = ["a.txt", "b.txt"].map(|name| dir.path().join(name));
    let saved = save(&[side_save(a.clone(), "one\n"), side_save(b.clone(), "two\n")], None);
    assert!(saved.written.iter().all(Result::is_ok), "{:?}", saved.written);
    assert!(saved.snapshot.is_none());
    assert_eq!(std::fs::read(&a).expect("a"), b"one\n");
    assert_eq!(std::fs::read(&b).expect("b"), b"two\n");
}

#[test]
fn a_failed_write_leaves_the_other_side_written() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let into_a_directory = dir.path().to_owned();
    let b = dir.path().join("b.txt");
    let saved = save(&[side_save(into_a_directory, "one\n"), side_save(b.clone(), "two\n")], None);
    assert!(saved.written[0].is_err(), "a directory is not written over");
    assert!(saved.written[1].is_ok(), "{:?}", saved.written[1]);
    assert_eq!(std::fs::read(&b).expect("b"), b"two\n");
}

#[test]
fn git_has_no_jj_working_copy() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[(PATH, WORKING)]);
    fixture.write(PATH, WORKING);
    let (_dir, [_, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
    let repo = fixture.open();
    let found = jj_working_copy(&repo, &right, fixture.root().as_std_path()).expect("read");
    assert_eq!(found, None);
}

/// Tests that build a jj repository. `script/pre-commit` leaves them out (`.config/nextest.toml`).
mod jj {
    use super::*;
    use vcs_jj::testing::Fixture;

    /// A repository whose `@` holds `WORKING` at `path` on a parent holding `PARENT`, with
    /// the working-copy file on disk as `@` holds it.
    fn working(path: &str) -> Fixture {
        let fixture = Fixture::new();
        let one = fixture.commit("one", &[(path, PARENT)]);
        let two = fixture.commit_on(&[&one], "two", &[(path, WORKING)]);
        fixture.edit(&two);
        fixture.write(path, WORKING);
        fixture
    }

    fn found(fixture: &Fixture, right: &Path, cwd: &Path) -> Option<PathBuf> {
        let repo = fixture.open();
        assert_eq!(repo.kind(), vcs::RepoKind::Jj);
        jj_working_copy(&repo, right, cwd).expect("the repository reads")
    }

    #[test]
    fn a_right_copy_of_the_working_copy_names_its_file() {
        let fixture = working(PATH);
        let (_dir, [_, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
        let root = canonical(fixture.root().as_std_path());
        assert_eq!(found(&fixture, &right, &root), Some(root.join(PATH)));
    }

    #[test]
    fn a_nested_path_from_a_subdirectory_names_the_file_under_the_root() {
        let fixture = working(NESTED);
        let (_dir, [_, right]) = jj_diff_copies(NESTED, [PARENT, WORKING]);
        let root = canonical(fixture.root().as_std_path());
        let cwd = root.join("src");
        assert_eq!(found(&fixture, &right, &cwd), Some(root.join("src").join("a.txt")));
    }

    #[test]
    fn a_relative_right_copy_resolves_against_the_working_directory() {
        let fixture = working(PATH);
        let (dir, _) = jj_diff_copies(PATH, [PARENT, WORKING]);
        let root = canonical(fixture.root().as_std_path());
        let relative = Path::new("right").join(PATH);
        let repo = fixture.open();
        // The tool runs in jj's directory: the repository came from elsewhere.
        assert_eq!(
            jj_working_copy(&repo, &relative, dir.path()).expect("read"),
            Some(root.join(PATH))
        );
    }

    #[test]
    fn a_left_copy_is_never_the_working_copy() {
        let fixture = working(PATH);
        // `jj diff --from @ --to @-` puts the working copy's bytes on the left.
        let (_dir, [left, _]) = jj_diff_copies(PATH, [WORKING, PARENT]);
        let root = canonical(fixture.root().as_std_path());
        assert_eq!(found(&fixture, &left, &root), None);
    }

    #[test]
    fn a_right_copy_of_another_revision_is_not_the_working_copy() {
        let fixture = working(PATH);
        // `jj diff -r @-`: the right side holds the parent's bytes.
        let (_dir, [_, right]) = jj_diff_copies(PATH, [b"", PARENT]);
        let root = canonical(fixture.root().as_std_path());
        assert_eq!(found(&fixture, &right, &root), None);
    }

    #[test]
    fn a_copy_matching_the_disk_but_not_the_working_copy_commit_is_not_it() {
        let fixture = working(PATH);
        // Edited back to the parent's bytes after the snapshot, then `jj diff -r @-`.
        fixture.write(PATH, PARENT);
        let (_dir, [_, right]) = jj_diff_copies(PATH, [b"", PARENT]);
        let root = canonical(fixture.root().as_std_path());
        assert_eq!(found(&fixture, &right, &root), None);
    }

    #[test]
    fn a_file_changed_on_disk_since_the_snapshot_is_not_overwritten() {
        let fixture = working(PATH);
        fixture.write(PATH, EDITED);
        let (_dir, [_, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
        let root = canonical(fixture.root().as_std_path());
        assert_eq!(found(&fixture, &right, &root), None);
    }

    #[test]
    fn a_file_missing_from_disk_is_not_the_working_copy() {
        let fixture = working(PATH);
        std::fs::remove_file(fixture.root().join(PATH)).expect("removed");
        let (_dir, [_, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
        let root = canonical(fixture.root().as_std_path());
        assert_eq!(found(&fixture, &right, &root), None);
    }

    #[test]
    fn a_path_outside_jjs_directory_is_not_a_copy() {
        let fixture = working(PATH);
        let elsewhere = tempfile::tempdir().expect("a temporary directory");
        let right = elsewhere.path().join("right").join(PATH);
        std::fs::create_dir_all(right.parent().expect("a directory")).expect("directories");
        std::fs::write(&right, WORKING).expect("written");
        let root = canonical(fixture.root().as_std_path());
        assert_eq!(found(&fixture, &right, &root), None);
    }

    #[test]
    fn a_right_copy_of_the_working_copy_opens_the_file_and_locks_the_left() {
        let fixture = working(PATH);
        let (_dir, [left, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
        let root = canonical(fixture.root().as_std_path());
        let mut repo = RunRepository::find([root.as_path()]);
        let [l, r] = read_pair(&pair([left.clone(), right], NOT_READ_ONLY), &mut repo, &root)
            .expect("both sides read");
        assert_eq!(file(&l), Some(("a\nb\n", left.as_path(), true)), "jj drops edits to its copy");
        assert_eq!(file(&r), Some(("a\nB\n", root.join(PATH).as_path(), false)));
    }

    #[test]
    fn a_read_only_right_copy_stays_jj_s_copy() {
        let fixture = working(PATH);
        let (_dir, [left, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
        let root = canonical(fixture.root().as_std_path());
        let mut repo = RunRepository::find([root.as_path()]);
        let [l, r] =
            read_pair(&pair([left.clone(), right.clone()], [false, true]), &mut repo, &root)
                .expect("both sides read");
        assert_eq!(file(&l), Some(("a\nb\n", left.as_path(), false)));
        assert_eq!(file(&r), Some(("a\nB\n", right.as_path(), true)));
    }

    #[test]
    fn a_save_in_jj_snapshots_the_working_copy() {
        let fixture = working(PATH);
        fixture.check_out_to_disk();
        let root = canonical(fixture.root().as_std_path());
        let repo = RunRepository::find([root.as_path()]);
        let working_copy = repo.working_copy().expect("a jj working copy");
        let saved = save(&[side_save(root.join(PATH), "a\nB\nc\n")], Some(&working_copy));
        assert!(saved.written[0].is_ok(), "{:?}", saved.written[0]);
        assert!(matches!(saved.snapshot, Some(Ok(()))), "{:?}", saved.snapshot);
        let open = fixture.open();
        let at = open.working_copy_id().expect("read").expect("a working copy");
        let held = open
            .file_at(camino::Utf8Path::new(PATH), &vcs::RevSpec::Commit(at))
            .expect("read")
            .expect("the file");
        assert_eq!(held, EDITED, "@ holds the save");
    }

    #[test]
    fn a_save_with_no_write_takes_no_snapshot() {
        let fixture = working(PATH);
        let root = canonical(fixture.root().as_std_path());
        let repo = RunRepository::find([root.as_path()]);
        let working_copy = repo.working_copy().expect("a jj working copy");
        let ops = fixture.operation_ids().len();
        let saved = save(&[side_save(root.clone(), "x\n")], Some(&working_copy));
        assert!(saved.written[0].is_err(), "a directory is not written over");
        assert!(saved.snapshot.is_none());
        assert_eq!(fixture.operation_ids().len(), ops, "no operation");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_on_disk_is_not_written_through() {
        let fixture = working(PATH);
        let root = canonical(fixture.root().as_std_path());
        let target = root.join("target.txt");
        std::fs::write(&target, WORKING).expect("written");
        std::fs::remove_file(root.join(PATH)).expect("removed");
        std::os::unix::fs::symlink(&target, root.join(PATH)).expect("linked");
        let (_dir, [_, right]) = jj_diff_copies(PATH, [PARENT, WORKING]);
        assert_eq!(found(&fixture, &right, &root), None);
    }
}
