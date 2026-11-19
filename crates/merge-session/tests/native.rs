//! `diff PATH` inputs from git and jj repositories built through gix and jj-lib (spec §6.1,
//! DESIGN.md §A7.1).

use std::path::Path;

use merge_session::dir_diff::{Change, Content};
use merge_session::native::{self, LeftAt, NativeDiff, NativeError, Revision};
use merge_session::repository::RunRepository;
use vcs::VcsError;
use vcs_git::testing::Fixture;

fn load(path: &camino::Utf8Path, rev: Option<&str>) -> NativeDiff {
    try_load(path.as_std_path(), rev).expect("the inputs load")
}

/// [`native::load`] in the repository holding `path`, as `diff PATH` finds it.
fn try_load(path: &std::path::Path, rev: Option<&str>) -> Result<NativeDiff, NativeError> {
    native::load(&mut RunRepository::of_file(path), path, rev)
}

/// [`native::changed_files`] from the working directory `dir`, as path, change and content.
fn listed(dir: &camino::Utf8Path, rev: Option<&str>) -> (String, Vec<(String, Change, Content)>) {
    let listing = try_list(dir, rev).expect("the files are listed");
    let files = listing
        .files
        .iter()
        .map(|file| (file.display_path(), file.change.clone(), file.content))
        .collect();
    (listing.against.rev, files)
}

fn try_list(dir: &camino::Utf8Path, rev: Option<&str>) -> Result<native::Listing, NativeError> {
    let mut repo = RunRepository::find([dir.as_std_path()]);
    native::changed_files(&mut repo, rev)
}

fn file(path: &str, change: Change, content: Content) -> (String, Change, Content) {
    (path.to_owned(), change, content)
}

#[test]
fn the_working_tree_lists_its_staged_and_unstaged_changes_against_head() {
    let fixture = Fixture::new();
    let committed: &[(&str, &[u8])] = &[
        ("a.txt", b"1\n"),
        ("gone.txt", b"g\n"),
        ("same.txt", b"s\n"),
        ("undone.txt", b"u\n"),
        ("img.bin", b"\x00a"),
    ];
    fixture.commit("one", committed);
    // A staged change undone on disk, and a staged new file.
    fixture.stage(&[
        ("a.txt", b"1\n"),
        ("gone.txt", b"g\n"),
        ("same.txt", b"s\n"),
        ("undone.txt", b"staged\n"),
        ("img.bin", b"\x00a"),
        ("new.txt", b"n\n"),
    ]);
    for (path, bytes) in [
        ("a.txt", &b"2\n"[..]),
        ("same.txt", b"s\n"),
        ("undone.txt", b"u\n"),
        ("img.bin", b"\x00b"),
        ("new.txt", b"n\n"),
        ("untracked.txt", b"x\n"),
    ] {
        fixture.write(path, bytes);
    }
    assert_eq!(
        listed(fixture.root(), None),
        (
            "HEAD".to_owned(),
            vec![
                file("a.txt", Change::Modified, Content::Text),
                file("gone.txt", Change::Deleted, Content::Text),
                file("img.bin", Change::Modified, Content::Binary),
                file("new.txt", Change::Added, Content::Text),
            ]
        )
    );
}

#[test]
fn rev_lists_the_changes_against_that_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"1\n")]);
    fixture.commit("two", &[("a.txt", b"2\n"), ("b.txt", b"b\n")]);
    fixture.stage(&[("a.txt", b"2\n"), ("b.txt", b"b\n")]);
    fixture.write("a.txt", b"2\n");
    fixture.write("b.txt", b"b\n");
    assert_eq!(listed(fixture.root(), Some("HEAD")), ("HEAD".to_owned(), vec![]));
    assert_eq!(
        listed(fixture.root(), Some("HEAD~1")),
        (
            "HEAD~1".to_owned(),
            vec![
                file("a.txt", Change::Modified, Content::Text),
                file("b.txt", Change::Added, Content::Text),
            ]
        )
    );
}

#[test]
fn a_listing_names_an_unknown_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"1\n")]);
    let error = try_list(fixture.root(), Some("nope")).expect_err("no such revision");
    assert!(
        matches!(&error, NativeError::Vcs(VcsError::UnknownRevision(rev)) if rev == "nope"),
        "{error:?}"
    );
}

#[test]
fn a_listing_outside_any_repository_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dir = camino::Utf8Path::from_path(dir.path()).expect("UTF-8");
    let error = try_list(dir, None).expect_err("no repository");
    assert!(matches!(&error, NativeError::Vcs(VcsError::NotARepository(_))), "{error:?}");
}

/// [`native::load_listed`] of `left` against `right`, from `listing`.
fn load_from(listing: &native::Listing, left: &str, right: &str) -> NativeDiff {
    let mut repo = RunRepository::find([listing.against.root.as_path()]);
    native::load_listed(&mut repo, &listing.against, Path::new(left), Path::new(right))
        .expect("loaded")
}

#[test]
fn a_listed_file_loads_from_the_commit_listed_though_head_moved() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"one\n"), ("old.txt", b"1\n2\n3\n4\n")]);
    fixture.stage(&[("a.txt", b"one\n"), ("new.txt", b"1\n2\n3\n4\n")]);
    fixture.write("a.txt", b"edited\n");
    fixture.write("new.txt", b"1\n2\n3\n4\n");
    let listing = try_list(fixture.root(), None).expect("listed");
    // A commit moves `HEAD` after the listing.
    fixture.commit("two", &[("a.txt", b"two\n"), ("new.txt", b"1\n2\n3\n4\n")]);
    let diff = load_from(&listing, "a.txt", "a.txt");
    assert_eq!((diff.left.content(), diff.left_at.clone()), ("one\n", named("HEAD")));
    let diff = load_from(&listing, "old.txt", "new.txt");
    assert_eq!((diff.left.content(), diff.left_at.clone()), ("1\n2\n3\n4\n", named("HEAD")));
}

#[cfg(unix)]
#[test]
fn a_symlink_deleted_retargeted_or_made_a_file_is_listed_as_special() {
    use std::os::unix::fs::symlink;
    use vcs::testing::Entry;
    let fixture = Fixture::new();
    let entries = [
        ("gone", Entry::Symlink("t")),
        ("moved", Entry::Symlink("t")),
        ("now-file", Entry::Symlink("t")),
        ("same", Entry::Symlink("t")),
        ("t", Entry::File(b"t\n")),
    ];
    fixture.commit_entries("one", &entries);
    fixture.stage_entries(&entries);
    let root = fixture.root().as_std_path();
    fixture.write("t", b"t\n");
    symlink("t", root.join("same")).expect("same");
    symlink("u", root.join("moved")).expect("moved");
    fixture.write("now-file", b"f\n");
    assert_eq!(
        listed(fixture.root(), None).1,
        [
            file("gone", Change::Deleted, Content::Special),
            file("moved", Change::Modified, Content::Special),
            file("now-file", Change::Modified, Content::Special),
        ]
    );
}

#[test]
fn a_submodule_moved_added_or_removed_is_listed_as_special() {
    use vcs::testing::Entry;
    let fixture = Fixture::new();
    let plain = Entry::File(b"f\n");
    fixture.commit_entries(
        "one",
        &[("f.txt", plain), ("gone", Entry::Submodule), ("moved", Entry::Submodule)],
    );
    // `git submodule update` after a move, `git submodule add`, `git rm`.
    fixture.stage_entries(&[
        ("added", Entry::Submodule),
        ("f.txt", plain),
        ("moved", Entry::SubmoduleMoved),
    ]);
    fixture.write("f.txt", b"f\n");
    fixture.write("added/inner.txt", b"i\n");
    fixture.write("moved/inner.txt", b"i\n");
    assert_eq!(
        listed(fixture.root(), None).1,
        [
            file("added", Change::Added, Content::Special),
            file("gone", Change::Deleted, Content::Special),
            file("moved", Change::Modified, Content::Special),
        ]
    );
}

#[test]
fn a_repository_without_commits_lists_its_files_as_added() {
    let fixture = Fixture::new();
    fixture.stage(&[("a.txt", b"1\n"), ("b.txt", b"b\n")]);
    fixture.write("a.txt", b"2\n");
    fixture.write("b.txt", b"b\n");
    fixture.write("untracked.txt", b"u\n");
    assert_eq!(
        listed(fixture.root(), None),
        (
            "HEAD".to_owned(),
            vec![
                file("a.txt", Change::Added, Content::Text),
                file("b.txt", Change::Added, Content::Text),
            ]
        )
    );
    let listing = try_list(fixture.root(), None).expect("listed");
    let diff = load_from(&listing, "a.txt", "a.txt");
    assert_eq!((diff.left.content(), diff.left_at.clone()), ("", absent("HEAD")));
    // A revision given by name must exist.
    let error = try_list(fixture.root(), Some("HEAD")).expect_err("no commit");
    assert!(
        matches!(&error, NativeError::Vcs(VcsError::UnknownRevision(rev)) if rev == "HEAD"),
        "{error:?}"
    );
}

#[test]
fn a_listed_file_loads_against_the_listing_s_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("sub/a.txt", b"one\n"), ("gone.txt", b"g\n")]);
    fixture.commit("two", &[("sub/a.txt", b"two\n"), ("gone.txt", b"g\n")]);
    fixture.write("sub/a.txt", b"three\n");
    let listing = try_list(fixture.root(), Some("HEAD~1")).expect("listed");
    let diff = load_from(&listing, "sub/a.txt", "sub/a.txt");
    assert_eq!(diff.left.content(), "one\n");
    assert_eq!(diff.left_at, named("HEAD~1"));
    // The repository's root is canonical.
    let root = dunce::canonicalize(fixture.root()).expect("canonical");
    assert_eq!(listing.against.root, root);
    assert_eq!(diff.right, root.join("sub").join("a.txt"));
    // A file deleted on disk still loads its revision. The fixture commits without
    // writing files, so `gone.txt` is absent on disk.
    assert!(!root.join("gone.txt").exists());
    let diff = load_from(&listing, "gone.txt", "gone.txt");
    assert_eq!(diff.left.content(), "g\n");
    assert_eq!(diff.left_at, named("HEAD~1"));
    assert_eq!(load_from(&listing, "new.txt", "new.txt").left_at, absent("HEAD~1"));
    // A rename reads its old path at the revision and its new path on disk.
    let diff = load_from(&listing, "gone.txt", "sub/a.txt");
    assert_eq!(diff.left.content(), "g\n");
    assert_eq!(diff.right, root.join("sub").join("a.txt"));
}

#[test]
fn a_staged_rename_is_listed_at_its_new_path() {
    let fixture = Fixture::new();
    let body = b"1\n2\n3\n4\n";
    fixture.commit("one", &[("old.txt", body), ("keep.txt", b"k\n")]);
    // `git mv old.txt sub/new.txt`, then an edit that keeps 6 of 8 bytes.
    fixture.stage(&[("sub/new.txt", body), ("keep.txt", b"k\n")]);
    fixture.write("sub/new.txt", b"1\n2\n3\n5\n");
    fixture.write("keep.txt", b"k\n");
    let (_, files) = listed(fixture.root(), None);
    assert_eq!(
        files,
        [file("sub/new.txt", Change::Renamed { from: "old.txt".into() }, Content::Text)]
    );
}

#[test]
fn a_staged_copy_of_a_modified_file_is_listed_as_a_copy() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"1\n2\n3\n4\n")]);
    fixture.stage(&[("a.txt", b"1\n2\n3\n5\n"), ("sub/c.txt", b"1\n2\n3\n5\n")]);
    fixture.write("a.txt", b"1\n2\n3\n5\n");
    fixture.write("sub/c.txt", b"1\n2\n3\n5\n");
    let (_, files) = listed(fixture.root(), None);
    assert_eq!(
        files,
        [
            file("a.txt", Change::Modified, Content::Text),
            file("sub/c.txt", Change::Copied { from: "a.txt".into() }, Content::Text),
        ]
    );
    // A copy reads its source at the revision and its own path on disk.
    let listing = try_list(fixture.root(), None).expect("listed");
    let diff = load_from(&listing, "a.txt", "sub/c.txt");
    assert_eq!(diff.left.content(), "1\n2\n3\n4\n");
    assert_eq!(diff.right, listing.against.root.join("sub").join("c.txt"));
}

#[test]
fn an_untracked_copy_of_a_deleted_file_is_no_rename_in_git() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("old.txt", b"body\n")]);
    fixture.stage(&[]);
    fixture.write("new.txt", b"body\n");
    let (_, files) = listed(fixture.root(), None);
    assert_eq!(files, [file("old.txt", Change::Deleted, Content::Text)]);
}

#[test]
fn an_unstaged_file_is_compared_with_head_titled_by_branch() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("src/a.txt", b"committed\n")]);
    fixture.stage(&[("src/a.txt", b"committed\n")]);
    let path = fixture.write("src/a.txt", b"edited\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "committed\n");
    assert_eq!(diff.left_at, named("main"));
    assert_eq!(diff.right, path.as_std_path());
}

#[test]
fn an_operation_needs_jj() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"committed\n")]);
    let path = fixture.write("a.txt", b"edited\n");
    let refused = native::load_at_operation(
        &mut RunRepository::of_file(path.as_std_path()),
        path.as_std_path(),
        "@",
    );
    assert!(matches!(refused, Err(NativeError::NeedsJj("diff --op"))), "{refused:?}");
}

#[test]
fn a_staged_file_is_compared_with_the_index() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"committed\n")]);
    fixture.stage(&[("a.txt", b"staged\n")]);
    let path = fixture.write("a.txt", b"edited\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "staged\n");
    assert_eq!(diff.left_at, named("index"));
}

#[test]
fn rev_picks_the_revision_and_titles_the_pane() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"one\n")]);
    fixture.commit("two", &[("a.txt", b"two\n")]);
    fixture.stage(&[("a.txt", b"two\n")]);
    let path = fixture.write("a.txt", b"three\n");
    let diff = load(&path, Some("HEAD~1"));
    assert_eq!(diff.left.content(), "one\n");
    assert_eq!(diff.left_at, named("HEAD~1"));
}

#[test]
fn a_file_absent_from_the_parent_is_compared_with_nothing() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.stage(&[("a.txt", b"a\n")]);
    let path = fixture.write("new.txt", b"new\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "");
    assert_eq!(diff.left_at, absent("main"));
}

#[test]
fn a_file_absent_from_a_given_revision_says_so_in_the_title() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.commit("two", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    fixture.stage(&[("b.txt", b"b\n")]);
    let path = fixture.write("b.txt", b"edited\n");
    let diff = load(&path, Some("HEAD~1"));
    assert_eq!(diff.left.content(), "");
    assert_eq!(diff.left_at, absent("HEAD~1"));
}

#[test]
fn an_empty_file_at_the_revision_keeps_the_plain_title() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"")]);
    fixture.stage(&[("a.txt", b"")]);
    let path = fixture.write("a.txt", b"edited\n");
    let diff = load(&path, None);
    assert_eq!(diff.left.content(), "");
    assert_eq!(diff.left_at, named("main"));
}

#[test]
fn a_path_given_through_a_parent_directory_resolves() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"committed\n")]);
    fixture.stage(&[("a.txt", b"committed\n")]);
    fixture.write("a.txt", b"edited\n");
    let indirect = fixture.write("src/b.txt", b"b\n").with_file_name("../a.txt");
    assert_eq!(load(&indirect, None).left.content(), "committed\n");
}

#[test]
fn a_file_outside_any_repository_is_named() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "a\n").expect("the file is written");
    match try_load(&path, None) {
        Err(NativeError::Vcs(VcsError::NotARepository(_))) => {}
        other => panic!("expected NotARepository, got {other:?}"),
    }
}

#[test]
fn a_missing_working_copy_file_is_named() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let path = fixture.root().join("gone.txt");
    match try_load(path.as_std_path(), None) {
        Err(NativeError::Find { path: named, .. }) => assert_eq!(named, path.as_std_path()),
        other => panic!("expected Find, got {other:?}"),
    }
}

#[test]
fn an_unknown_revision_is_named() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let path = fixture.write("a.txt", b"b\n");
    match try_load(path.as_std_path(), Some("nope")) {
        Err(NativeError::Vcs(VcsError::UnknownRevision(rev))) => assert_eq!(rev, "nope"),
        other => panic!("expected UnknownRevision, got {other:?}"),
    }
}

#[test]
fn a_binary_parent_is_refused_with_its_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.bin", b"\0\x01\x02")]);
    fixture.stage(&[("a.bin", b"\0\x01\x02")]);
    let path = fixture.write("a.bin", b"text now\n");
    match try_load(path.as_std_path(), None) {
        Err(NativeError::Decode { path, rev, .. }) => {
            assert_eq!(path, "a.bin");
            assert_eq!(rev, "HEAD");
        }
        other => panic!("expected Decode, got {other:?}"),
    }
}

/// git takes the working tree in at `git add`, so a save in a git repository snapshots
/// nothing (spec §6.1).
#[test]
fn a_git_repository_has_no_working_copy_to_snapshot() {
    let fixture = Fixture::new();
    let path = fixture.write("a.txt", b"edited\n");
    assert!(RunRepository::of_file(path.as_std_path()).working_copy().is_none());
    assert!(RunRepository::none().working_copy().is_none());
}

mod jj {
    use super::{Change, Content, LeftAt, Revision, absent, file, listed, load, named};
    use merge_session::dir_diff::ChangedFile;
    use merge_session::native;
    use merge_session::repository::RunRepository;
    use vcs::RepoKind;
    use vcs_jj::testing::Fixture;

    #[test]
    fn a_file_is_compared_with_at_minus_titled_by_its_revision() {
        let fixture = Fixture::new();
        fixture.commit("fix parsing\n\nbody\n", &[("src/a.txt", b"committed\n")]);
        let path = fixture.write("src/a.txt", b"edited\n");
        let diff = load(&path, None);
        assert_eq!(diff.left.content(), "committed\n");
        // The heading's revision chip names the change (spec §5).
        assert_eq!(diff.left_at, named("@-"));
        assert_eq!(diff.right, path.as_std_path());
    }

    #[test]
    fn a_file_absent_from_at_minus_says_so_in_the_title() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"a\n")]);
        let path = fixture.write("new.txt", b"new\n");
        let diff = load(&path, None);
        assert_eq!(diff.left.content(), "");
        assert_eq!(diff.left_at, absent("@-"));
    }

    /// Write `bytes` to `a.txt` and snapshot it; the new head operation's ID.
    fn snapshot(fixture: &Fixture, bytes: &[u8]) -> String {
        let path = fixture.write("a.txt", bytes);
        let mut repo = vcs_jj::JjRepository::discover(fixture.root()).expect("the repository");
        vcs::Repository::snapshot(&mut repo).expect("the snapshot");
        drop(path);
        fixture.operation_ids()[0].clone()
    }

    #[test]
    fn an_operation_compares_with_the_working_copy_then() {
        let fixture = Fixture::new();
        let one = snapshot(&fixture, b"one\n");
        let commit_then = fixture.working_copy_id();
        snapshot(&fixture, b"two\n");
        let path = fixture.write("a.txt", b"three\n");
        let (diff, commit) = native::load_at_operation(
            &mut RunRepository::of_file(path.as_std_path()),
            path.as_std_path(),
            &one[..8],
        )
        .expect("the inputs load");
        assert_eq!(diff.left.content(), "one\n");
        assert_eq!(
            diff.left_at,
            LeftAt {
                revision: Revision::Operation(one[..vcs_jj::SHORT_OPERATION].to_owned()),
                exists: true
            }
        );
        assert_eq!(commit.as_str(), commit_then);
    }

    #[test]
    fn an_operation_before_the_file_says_so_in_the_title() {
        let fixture = Fixture::new();
        let ids = fixture.operation_ids();
        let first = ids[ids.len() - 2].clone();
        let path = fixture.write("a.txt", b"new\n");
        let (diff, _) = native::load_at_operation(
            &mut RunRepository::of_file(path.as_std_path()),
            path.as_std_path(),
            &first,
        )
        .expect("the inputs load");
        assert_eq!(diff.left.content(), "");
        assert_eq!(
            diff.left_at,
            LeftAt {
                revision: Revision::Operation(first[..vcs_jj::SHORT_OPERATION].to_owned()),
                exists: false
            }
        );
    }

    #[test]
    fn rev_takes_a_revset() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"one\n")]);
        fixture.commit("two", &[("a.txt", b"two\n")]);
        let path = fixture.write("a.txt", b"three\n");
        let diff = load(&path, Some("@--"));
        assert_eq!(diff.left.content(), "one\n");
        assert_eq!(diff.left_at, named("@--"));
    }

    #[test]
    fn rev_takes_the_user_s_revset_aliases() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"one\n")]);
        fixture.commit("two", &[("a.txt", b"two\n")]);
        let path = fixture.write("a.txt", b"three\n");
        let dir = tempfile::tempdir().expect("a temporary directory");
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "[revset-aliases]\n\"older()\" = \"@--\"\n")
            .expect("the config is written");
        let environment = std::collections::HashMap::from([(
            "JJ_CONFIG".to_owned(),
            config.to_str().expect("UTF-8").to_owned(),
        )]);
        let config = vcs_jj::config::ConfigEnv { environment, ..Default::default() };
        let mut run = RunRepository::of_file(path.as_std_path()).with_jj_config(config);
        let diff = native::load(&mut run, path.as_std_path(), Some("older()"))
            .expect("the alias names a revision");
        assert_eq!(diff.left.content(), "one\n");
        assert_eq!(diff.left_at, named("older()"));
    }

    #[test]
    fn rev_takes_jj_s_default_trunk_alias() {
        // Without a remote bookmark, jj's `trunk()` is the root commit, which holds no file.
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"one\n")]);
        let path = fixture.write("a.txt", b"two\n");
        let mut run = RunRepository::of_file(path.as_std_path())
            .with_jj_config(vcs_jj::config::ConfigEnv::default());
        let diff = native::load(&mut run, path.as_std_path(), Some("trunk()"))
            .expect("jj's default alias names a revision");
        assert_eq!(diff.left.content(), "");
        assert_eq!(diff.left_at, absent("trunk()"));
    }

    #[test]
    fn a_merge_working_copy_is_titled_by_its_revision() {
        let fixture = Fixture::new();
        let left = fixture.commit_on(&[], "left", &[("a.txt", b"left\n")]);
        let right = fixture.commit_on(&[], "right", &[("b.txt", b"right\n")]);
        fixture.check_out(&[&left, &right]);
        let path = fixture.write("a.txt", b"edited\n");
        let diff = load(&path, None);
        assert_eq!(diff.left.content(), "left\n");
        assert_eq!(diff.left_at, named("@-"));
    }

    #[test]
    fn a_colocated_repository_is_read_as_jj_though_git_sees_it_too() {
        let fixture = Fixture::colocated();
        fixture.commit("one", &[("a.txt", b"committed\n")]);
        let path = fixture.write("a.txt", b"edited\n");
        let repo = native::discover(fixture.root()).expect("the repository is found");
        assert_eq!(repo.kind(), RepoKind::Jj);
        let diff = load(&path, None);
        assert_eq!(diff.left_at, named("@-"));
        // git's detached HEAD is jj's @-.
        let git = vcs_git::GitRepository::discover(fixture.root()).expect("git sees it");
        let head = vcs::Repository::file_at(
            &git,
            camino::Utf8Path::new("a.txt"),
            &vcs::RevSpec::Expr("HEAD".to_owned()),
        )
        .expect("git reads HEAD");
        assert_eq!(head.as_deref(), Some(&b"committed\n"[..]));
    }

    #[test]
    fn the_nearest_repository_wins_when_one_nests_in_another() {
        let outer_jj = Fixture::new();
        let inner_git = vcs_git::testing::Fixture::new_in(outer_jj.root());
        let found = native::discover(inner_git.root()).expect("a repository is found");
        assert_eq!(found.kind(), RepoKind::Git, "git inside jj");
        assert_eq!(native::discover(outer_jj.root()).expect("the outer one").kind(), RepoKind::Jj);

        let outer_git = vcs_git::testing::Fixture::new();
        let inner_jj = Fixture::new_in(outer_git.root());
        let found = native::discover(inner_jj.root()).expect("a repository is found");
        assert_eq!(found.kind(), RepoKind::Jj, "jj inside git");
        assert_eq!(
            native::discover(outer_git.root()).expect("the outer one").kind(),
            RepoKind::Git
        );
    }

    #[test]
    fn a_saves_snapshot_takes_the_file_into_the_working_copy_commit() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"one\n")]);
        fixture.check_out_to_disk();
        let path = fixture.write("a.txt", b"saved\n");
        let working_copy =
            RunRepository::of_file(path.as_std_path()).working_copy().expect("a jj working copy");
        let before = fixture.operation_count();
        working_copy.snapshot().expect("the snapshot");
        assert_eq!(fixture.operation_count(), before + 1, "one snapshot operation");
        let tree = fixture.tree_text(&fixture.working_copy_commit());
        assert_eq!(tree.get("a.txt").map(String::as_str), Some("saved\n"));
    }

    #[test]
    fn a_snapshot_of_a_stale_working_copy_fails_and_names_it() {
        let fixture = Fixture::new();
        let one = fixture.commit_on(&[], "one", &[("a.txt", b"one\n")]);
        let two = fixture.commit_on(&[], "two", &[("a.txt", b"two\n")]);
        fixture.check_out(&[&one]);
        fixture.check_out_to_disk();
        // The view moves on; the files on disk stay at `one`.
        fixture.check_out(&[&two]);
        let path = fixture.write("a.txt", b"saved\n");
        let working_copy =
            RunRepository::of_file(path.as_std_path()).working_copy().expect("a jj working copy");
        let operations = fixture.operations();
        assert!(
            matches!(working_copy.snapshot(), Err(vcs::VcsError::StaleWorkspace(_))),
            "the snapshot refuses a stale working copy"
        );
        assert_eq!(fixture.operations(), operations, "nothing is written");
    }

    #[test]
    fn a_listing_snapshots_the_working_copy_first() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"1\n"), ("gone.txt", b"g\n")]);
        fixture.check_out_to_disk();
        fixture.write("a.txt", b"2\n");
        fixture.write("new.txt", b"n\n");
        std::fs::remove_file(fixture.root().join("gone.txt")).expect("removed");
        let before = fixture.operation_count();
        assert_eq!(
            listed(fixture.root(), None),
            (
                "@-".to_owned(),
                vec![
                    file("a.txt", Change::Modified, Content::Text),
                    file("gone.txt", Change::Deleted, Content::Text),
                    file("new.txt", Change::Added, Content::Text),
                ]
            )
        );
        assert_eq!(fixture.operation_count(), before + 1, "one snapshot operation");
    }

    #[test]
    fn a_file_conflicted_at_the_revision_is_special() {
        let fixture = Fixture::new();
        fixture.three_way("a.txt", [b"base\n", b"left\n", b"right\n"]);
        fixture.check_out_to_disk();
        // Resolved on disk; conflicted in the merge of `@-`'s trees.
        fixture.write("a.txt", b"resolved\n");
        assert_eq!(
            listed(fixture.root(), None).1,
            [file("a.txt", Change::Modified, Content::Special)]
        );
    }

    /// The listing of `fixture`'s working copy against `@-`.
    fn listing(fixture: &Fixture) -> native::Listing {
        let mut repo = RunRepository::find([fixture.root().as_std_path()]);
        native::changed_files(&mut repo, None).expect("listed")
    }

    #[test]
    fn a_listed_file_loads_from_the_commit_listed_though_at_minus_moved() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"one\n")]);
        fixture.check_out_to_disk();
        fixture.write("a.txt", b"edited\n");
        let listing = listing(&fixture);
        // `jj commit` from elsewhere: `@-` is now `two`.
        fixture.commit("two", &[("a.txt", b"two\n")]);
        let diff = super::load_from(&listing, "a.txt", "a.txt");
        assert_eq!((diff.left.content(), diff.left_at.clone()), ("one\n", named("@-")));
    }

    #[test]
    fn a_merge_working_copy_loads_from_the_merge_of_its_parents() {
        let fixture = Fixture::new();
        let left = fixture.commit_on(&[], "left", &[("a.txt", b"a\n")]);
        let right = fixture.commit_on(&[], "right", &[("b.txt", b"b\n")]);
        fixture.check_out(&[&left, &right]);
        fixture.check_out_to_disk();
        fixture.write("a.txt", b"edited\n");
        let listing = listing(&fixture);
        assert_eq!(
            listing.files.iter().map(ChangedFile::display_path).collect::<Vec<_>>(),
            ["a.txt"]
        );
        let diff = super::load_from(&listing, "a.txt", "a.txt");
        assert_eq!((diff.left.content(), diff.left_at.clone()), ("a\n", named("@-")));
        assert_eq!(super::load_from(&listing, "b.txt", "b.txt").left.content(), "b\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_deleted_retargeted_or_made_a_file_is_listed_as_special() {
        use std::os::unix::fs::symlink;
        use vcs::testing::Entry;
        let fixture = Fixture::new();
        fixture.commit_entries(
            "one",
            &[
                ("gone", Entry::Symlink("t")),
                ("moved", Entry::Symlink("t")),
                ("now-file", Entry::Symlink("t")),
                ("same", Entry::Symlink("t")),
                ("t", Entry::File(b"t\n")),
            ],
        );
        fixture.check_out_to_disk();
        let root = fixture.root().as_std_path();
        for path in ["gone", "moved", "now-file"] {
            std::fs::remove_file(root.join(path)).expect("removed");
        }
        symlink("u", root.join("moved")).expect("moved");
        fixture.write("now-file", b"f\n");
        symlink("t", root.join("added")).expect("added");
        assert_eq!(
            listed(fixture.root(), None).1,
            [
                file("added", Change::Added, Content::Special),
                file("gone", Change::Deleted, Content::Special),
                file("moved", Change::Modified, Content::Special),
                file("now-file", Change::Modified, Content::Special),
            ]
        );
    }

    #[test]
    fn a_submodule_moved_in_the_working_copy_is_listed_as_special() {
        use vcs::testing::Entry;
        let fixture = Fixture::new();
        let plain = Entry::File(b"f\n");
        fixture.commit_entries("one", &[("f.txt", plain), ("sub", Entry::Submodule)]);
        // jj leaves a submodule's directory alone; its commit moves in `@`.
        fixture.amend_entries(
            &fixture.working_copy_commit(),
            &[("f.txt", plain), ("sub", Entry::SubmoduleMoved)],
        );
        fixture.check_out_to_disk();
        assert_eq!(
            listed(fixture.root(), None).1,
            [file("sub", Change::Modified, Content::Special)]
        );
    }

    #[test]
    fn a_copy_of_a_file_edited_on_disk_is_a_copy_after_the_snapshot() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("src/a.rs", b"fn a() {}\n")]);
        fixture.check_out_to_disk();
        fixture.write("src/a.rs", b"fn a() {}\nfn b() {}\n");
        fixture.write("lib/c.rs", b"fn a() {}\nfn b() {}\n");
        assert_eq!(
            listed(fixture.root(), None).1,
            [
                file("lib/c.rs", Change::Copied { from: "src/a.rs".into() }, Content::Text),
                file("src/a.rs", Change::Modified, Content::Text),
            ]
        );
    }

    #[test]
    fn a_file_moved_on_disk_is_a_rename_after_the_snapshot() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("src/old.rs", b"fn a() {}\n")]);
        fixture.check_out_to_disk();
        std::fs::remove_file(fixture.root().join("src/old.rs")).expect("removed");
        fixture.write("lib/new.rs", b"fn a() {}\n");
        assert_eq!(
            listed(fixture.root(), None).1,
            [file("lib/new.rs", Change::Renamed { from: "src/old.rs".into() }, Content::Text)]
        );
    }
}

/// The left side at the revision `name`, where the file exists.
fn named(name: &str) -> LeftAt {
    LeftAt { revision: Revision::Named(name.to_owned()), exists: true }
}

/// The left side at the revision `name`, where the file does not exist.
fn absent(name: &str) -> LeftAt {
    LeftAt { revision: Revision::Named(name.to_owned()), exists: false }
}
