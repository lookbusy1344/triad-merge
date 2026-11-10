//! The working-copy file the right side of `jj diff --tool` stands for (spec §7.1).

use std::path::{Path, PathBuf};

use merge_session::diff_tool::jj_working_copy;
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
