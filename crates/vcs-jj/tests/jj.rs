//! `JjRepository` on repositories built in temporary directories through jj-lib (PLAN.md §A7.1).

#![expect(clippy::panic, reason = "test helpers outside `#[test]` panic on a failed check")]

#[path = "jj/snapshot.rs"]
mod snapshot;

use camino::{Utf8Path, Utf8PathBuf};
use vcs::testing::Entry;
use vcs::{Parent, RepoKind, Repository, RevSpec, TermContent, VcsError};
use vcs_jj::testing::{DEFAULT_STORE, Fixture, Store};
use vcs_jj::{JjError, JjRepository};

/// jj's root commit: all-zero commit ID, all-`z` change ID.
const ROOT_COMMIT: &str = "00000000";
const ROOT_CHANGE: &str = "zzzzzzzz";
/// jj shows IDs with at least this many characters.
const MIN_SHORT_ID: usize = 8;

fn rev(expr: &str) -> RevSpec {
    RevSpec::Expr(expr.to_owned())
}

fn read(repo: &JjRepository, path: &str, expr: &str) -> Option<Vec<u8>> {
    repo.file_at(Utf8Path::new(path), &rev(expr)).expect("the read succeeds")
}

fn read_err(repo: &JjRepository, path: &str, spec: &RevSpec) -> VcsError {
    match repo.file_at(Utf8Path::new(path), spec) {
        Err(error) => error,
        Ok(bytes) => panic!("expected an error, read {bytes:?}"),
    }
}

fn parent(fixture: &Fixture) -> Parent {
    fixture.open().default_parent(Utf8Path::new("a.txt")).expect("the parent is found")
}

fn assert_short(short: &str, full: &str) {
    assert!(
        full.starts_with(short) && short.len() >= MIN_SHORT_ID,
        "{short} is a prefix of {full} of at least {MIN_SHORT_ID} characters"
    );
}

#[test]
fn discovery_climbs_from_a_subdirectory() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let sub = fixture.root().join("src/deep");
    std::fs::create_dir_all(&sub).expect("the directories are made");
    let repo = JjRepository::discover(&sub).expect("the repository is found");
    assert_eq!(repo.kind(), RepoKind::Jj);
    assert_eq!(read(&repo, "a.txt", "@-").as_deref(), Some(&b"a\n"[..]));
}

#[test]
fn the_root_is_the_workspace_root() {
    let fixture = Fixture::new();
    let sub = fixture.root().join("src");
    std::fs::create_dir_all(&sub).expect("the directory is made");
    let repo = JjRepository::discover(&sub).expect("the repository is found");
    // jj-lib gives the plain form, without Windows' verbatim `\\?\` prefix.
    assert_eq!(repo.root(), dunce::canonicalize(fixture.root()).expect("the fixture exists"));
}

#[test]
fn a_directory_outside_any_repository_is_named() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("UTF-8");
    match JjRepository::discover(&path) {
        Err(VcsError::NotARepository(named)) => assert_eq!(named, path),
        Err(other) => panic!("expected NotARepository, got {other}"),
        Ok(_) => panic!("expected NotARepository, found a repository"),
    }
}

#[test]
fn a_plain_git_repository_is_not_a_jj_repository() {
    let git = vcs_git::testing::Fixture::new();
    git.commit("one", &[("a.txt", b"a\n")]);
    match JjRepository::discover(git.root()) {
        Err(VcsError::NotARepository(named)) => assert_eq!(named, git.root()),
        Err(other) => panic!("expected NotARepository, got {other}"),
        Ok(_) => panic!("expected NotARepository, found a jj repository"),
    }
}

/// Where `.jj` and `.git` sit in each layout, as `jj git init` and `jj git init --no-colocate`
/// make them: `(.git beside .jj, git store inside .jj)`.
fn layout(fixture: &Fixture) -> (bool, bool) {
    let root = fixture.root();
    (root.join(".git").is_dir(), root.join(".jj/repo/store/git").is_dir())
}

#[test]
fn a_colocated_fixture_has_git_beside_jj() {
    let fixture = Fixture::colocated();
    assert_eq!(fixture.store(), Store::Colocated);
    assert_eq!(layout(&fixture), (true, false));
}

#[test]
fn an_internal_fixture_keeps_git_inside_jj() {
    let fixture = Fixture::internal();
    assert_eq!(fixture.store(), Store::Internal);
    assert_eq!(layout(&fixture), (false, true));
}

#[test]
fn the_default_fixture_takes_jj_s_default_store() {
    let expected = match DEFAULT_STORE {
        Store::Colocated => (true, false),
        Store::Internal => (false, true),
    };
    let fixture = Fixture::new();
    assert_eq!(fixture.store(), DEFAULT_STORE);
    assert_eq!(layout(&fixture), expected);
    let parent = tempfile::tempdir().unwrap();
    let nested = Fixture::new_in(Utf8Path::from_path(parent.path()).unwrap());
    assert_eq!(nested.store(), DEFAULT_STORE);
    assert_eq!(layout(&nested), expected);
}

#[test]
fn an_internal_workspace_opens_as_jj() {
    let fixture = Fixture::internal();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let repo = fixture.open();
    assert_eq!(repo.kind(), RepoKind::Jj);
    assert_eq!(read(&repo, "a.txt", "@-").as_deref(), Some(&b"a\n"[..]));
}

#[test]
fn a_revision_gives_the_file_as_committed_then() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"one\n")]);
    fixture.commit("two", &[("a.txt", b"two\n")]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "a.txt", "@-").as_deref(), Some(&b"two\n"[..]));
    assert_eq!(read(&repo, "a.txt", "@--").as_deref(), Some(&b"one\n"[..]));
}

#[test]
fn change_and_commit_id_prefixes_name_revisions() {
    let fixture = Fixture::new();
    let one = fixture.commit("one", &[("a.txt", b"one\n")]);
    fixture.commit("two", &[("a.txt", b"two\n")]);
    let repo = fixture.open();
    let change = &one.change_id[..MIN_SHORT_ID];
    let commit = &one.commit[..MIN_SHORT_ID];
    assert_eq!(read(&repo, "a.txt", change).as_deref(), Some(&b"one\n"[..]));
    assert_eq!(read(&repo, "a.txt", commit).as_deref(), Some(&b"one\n"[..]));
}

#[test]
fn a_nested_path_is_found() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("src/lib/a.rs", b"fn a() {}\n")]);
    assert_eq!(read(&fixture.open(), "src/lib/a.rs", "@-").as_deref(), Some(&b"fn a() {}\n"[..]));
}

#[test]
fn a_missing_path_or_a_directory_is_none() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("src/a.rs", b"a\n")]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "b.rs", "@-"), None);
    assert_eq!(read(&repo, "src/b.rs", "@-"), None);
    assert_eq!(read(&repo, "src", "@-"), None);
    assert_eq!(read(&repo, "src/a.rs/deeper", "@-"), None);
}

#[test]
fn a_file_added_later_is_none_at_an_earlier_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.commit("two", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    assert_eq!(read(&fixture.open(), "b.txt", "@--"), None);
}

#[test]
fn a_renamed_file_is_found_only_under_its_name_at_each_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("old.txt", b"same\n")]);
    fixture.commit("rename", &[("new.txt", b"same\n")]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "new.txt", "@-").as_deref(), Some(&b"same\n"[..]));
    assert_eq!(read(&repo, "old.txt", "@-"), None);
    assert_eq!(read(&repo, "new.txt", "@--"), None);
    assert_eq!(read(&repo, "old.txt", "@--").as_deref(), Some(&b"same\n"[..]));
}

#[test]
fn binary_bytes_come_back_unchanged() {
    let bytes: &[u8] = &[0, 0xff, b'\r', b'\n', 0x80, 0];
    let fixture = Fixture::new();
    fixture.commit("one", &[("blob.bin", bytes)]);
    assert_eq!(read(&fixture.open(), "blob.bin", "@-").as_deref(), Some(bytes));
}

#[test]
fn jj_has_no_index() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    match read_err(&fixture.open(), "a.txt", &RevSpec::Index) {
        VcsError::NoIndex(vcs) => assert_eq!(vcs, "jj"),
        other => panic!("expected NoIndex, got {other}"),
    }
}

#[test]
fn an_unknown_or_empty_revision_is_named() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let repo = fixture.open();
    for expr in ["nope", "none()"] {
        match read_err(&repo, "a.txt", &rev(expr)) {
            VcsError::UnknownRevision(named) => assert_eq!(named, expr),
            other => panic!("{expr}: expected UnknownRevision, got {other}"),
        }
    }
}

#[test]
fn an_ambiguous_commit_prefix_is_named() {
    // 17 commits, plus the root and the working copy, share 16 hex digits: two commits
    // start with the same digit.
    const COMMITS: usize = 17;
    let fixture = Fixture::new();
    let ids: Vec<String> = (0..COMMITS)
        .map(|n| fixture.commit(&format!("c{n}"), &[("a.txt", n.to_string().as_bytes())]))
        .map(|committed| committed.commit)
        .collect();
    let shared = ids
        .iter()
        .enumerate()
        .find_map(|(i, id)| {
            ids[i + 1..].iter().any(|other| other[..1] == id[..1]).then(|| &id[..1])
        })
        .expect("two commits share a first digit");
    match read_err(&fixture.open(), "a.txt", &rev(shared)) {
        VcsError::AmbiguousRevision(named) => assert_eq!(named, shared),
        other => panic!("expected AmbiguousRevision, got {other}"),
    }
}

#[test]
fn a_malformed_revision_is_a_parse_error() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    match read_err(&fixture.open(), "a.txt", &rev("@-(")) {
        VcsError::Backend { context, .. } => assert_eq!(context, "parsing a revision"),
        other => panic!("expected a parse error, got {other}"),
    }
}

#[test]
fn a_path_leaving_the_repository_is_refused() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    match read_err(&fixture.open(), "../a.txt", &rev("@-")) {
        VcsError::OutsideRepository(path) => assert_eq!(path, "../a.txt"),
        other => panic!("expected OutsideRepository, got {other}"),
    }
}

#[test]
fn the_parent_is_at_minus_titled_by_change_and_summary() {
    let fixture = Fixture::new();
    fixture.commit("first", &[("a.txt", b"a\n")]);
    let second = fixture.commit("second change\n\nmore words\n", &[("a.txt", b"b\n")]);
    let parent = parent(&fixture);
    assert_eq!(parent.rev, rev("@-"));
    assert_eq!(parent.term.label, None);
    let change = parent.term.change.expect("@-'s commit");
    assert_short(&change.commit, &second.commit);
    assert_short(change.change_id.as_deref().expect("a change ID"), &second.change_id);
    assert_eq!(change.summary.as_deref(), Some("second change"));
}

#[test]
fn a_parent_without_a_first_line_has_no_summary() {
    for description in ["", "\nbody only\n"] {
        let fixture = Fixture::new();
        fixture.commit(description, &[("a.txt", b"a\n")]);
        let change = parent(&fixture).term.change.expect("@-'s commit");
        assert_eq!(change.summary, None, "{description:?}");
    }
}

#[test]
fn in_a_new_repository_the_parent_is_the_empty_root() {
    let fixture = Fixture::new();
    let parent = parent(&fixture);
    assert_eq!(parent.rev, rev("@-"));
    let change = parent.term.change.expect("the root commit");
    assert_eq!(change.commit, ROOT_COMMIT);
    assert_eq!(change.change_id.as_deref(), Some(ROOT_CHANGE));
    assert_eq!(change.summary, None);
    assert_eq!(read(&fixture.open(), "a.txt", "@-"), None);
}

#[test]
fn a_merge_reads_from_its_parents_merged_tree() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n"), ("b.txt", b"b\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"a\n"), ("b.txt", b"right\n")]);
    fixture.check_out(&[&left, &right]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "a.txt", "@-").as_deref(), Some(&b"left\n"[..]));
    assert_eq!(read(&repo, "b.txt", "@-").as_deref(), Some(&b"right\n"[..]));
    let parent = parent(&fixture);
    assert_eq!(parent.rev, rev("@-"));
    assert_eq!(parent.term.title(), None, "no single commit to name");
}

#[test]
fn a_conflict_between_merged_parents_is_named() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
    fixture.check_out(&[&left, &right]);
    match read_err(&fixture.open(), "a.txt", &rev("@-")) {
        VcsError::Conflicted { path, rev } => {
            assert_eq!(path, "a.txt");
            assert_eq!(rev, "@-");
        }
        other => panic!("expected Conflicted, got {other}"),
    }
}

#[test]
fn a_forgotten_workspace_has_no_working_copy() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.forget_workspace();
    let canonical = |path: &Utf8Path| path.canonicalize_utf8().expect("the path exists");
    match JjRepository::discover(fixture.root()) {
        Err(VcsError::NoWorkingCopy(root)) => {
            assert_eq!(canonical(&root), canonical(fixture.root()));
        }
        Err(other) => panic!("expected NoWorkingCopy, got {other}"),
        Ok(_) => panic!("expected NoWorkingCopy, the repository opened"),
    }
}

#[test]
fn concurrent_operations_are_refused_and_left_alone() {
    const HEADS: usize = 2;
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.diverge_operations();
    match JjRepository::discover(fixture.root()) {
        Err(VcsError::Backend { context, source }) => {
            assert_eq!(context, "loading the head operation");
            match source.downcast_ref::<JjError>() {
                Some(JjError::ConcurrentOperations(heads)) => assert_eq!(*heads, HEADS),
                other => panic!("expected ConcurrentOperations, got {other:?}"),
            }
        }
        Err(other) => panic!("expected ConcurrentOperations, got {other}"),
        Ok(_) => panic!("expected ConcurrentOperations, the repository opened"),
    }
    assert_eq!(fixture.op_heads(), HEADS, "discovery reconciled nothing");
}

/// A working copy that merges two parents whose `a.txt` conflict.
fn conflicted_merge() -> Fixture {
    conflicted_merge_of_sides().0
}

/// [`conflicted_merge`], with the full IDs of its parents.
fn conflicted_merge_of_sides() -> (Fixture, [String; 2]) {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
    fixture.check_out(&[&left, &right]);
    (fixture, [left.commit, right.commit])
}

#[test]
fn annotating_a_conflicted_file_names_the_conflict() {
    let fixture = conflicted_merge();
    let repo = fixture.open();
    let working_copy = repo
        .working_copy_id()
        .expect("the working copy is read")
        .expect("jj has a working-copy commit");
    match repo.annotate(Utf8Path::new("a.txt"), &working_copy) {
        Err(VcsError::Conflicted { path, rev }) => {
            assert_eq!(path, "a.txt");
            assert_eq!(rev, working_copy.as_str());
        }
        other => panic!("expected Conflicted, got {other:?}"),
    }
}

#[test]
fn a_commit_id_reads_its_own_tree_not_a_merge_of_parents() {
    let (fixture, [left, _]) = conflicted_merge_of_sides();
    let rev = vcs::RevSpec::Commit(vcs::CommitId::new(left));
    assert_eq!(
        fixture
            .open()
            .file_at(Utf8Path::new("a.txt"), &rev)
            .expect("the commit is read")
            .as_deref(),
        Some(&b"left\n"[..])
    );
}

#[test]
fn the_parents_of_a_merge_are_two_commits() {
    let (fixture, mut sides) = conflicted_merge_of_sides();
    let mut parents: Vec<_> = fixture
        .open()
        .commits("@-")
        .expect("the parents resolve")
        .into_iter()
        .map(vcs::CommitId::into_string)
        .collect();
    parents.sort();
    sides.sort();
    assert_eq!(parents, sides);
}

#[test]
fn a_conflict_of_three_sides_is_listed_once() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let sides: [&[u8]; 3] = [b"one\n", b"two\n", b"three\n"];
    let sides = sides.map(|bytes| fixture.commit_on(&[&base], "side", &[("a.txt", bytes)]));
    fixture.check_out(&sides.each_ref());
    let conflicts = fixture.open().conflicts().expect("the conflicts are read");
    assert_eq!(conflicts, [Utf8PathBuf::from("a.txt")]);
}

#[test]
fn a_conflict_of_three_sides_reads_three_sides_and_two_bases_in_order() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let sides: [&[u8]; 3] = [b"one\n", b"two\n", b"three\n"];
    let commits = sides.map(|bytes| fixture.commit_on(&[&base], "side", &[("a.txt", bytes)]));
    fixture.check_out(&commits.each_ref());
    let conflict = fixture
        .open()
        .read_conflict(Utf8Path::new("a.txt"))
        .expect("the conflict is read")
        .expect("a.txt is conflicted");
    let contents = |terms: &[vcs::ConflictTerm]| {
        terms.iter().map(|term| term.content.clone()).collect::<Vec<_>>()
    };
    let file = |bytes: &[u8]| TermContent::File { bytes: bytes.to_vec(), executable: false };
    assert_eq!(contents(&conflict.sides), sides.map(file));
    assert_eq!(contents(&conflict.bases), [file(b"a\n"), file(b"a\n")]);
}

#[test]
fn sides_that_differ_only_in_the_executable_bit_merge_cleanly() {
    // Two sides set the bit and one leaves it: jj counts the two equal changes as one, so the
    // bit resolves to set and the path holds no conflict.
    const BYTES: &[u8] = b"same\n";
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.sh", BYTES)]);
    let sides = [Entry::Executable(BYTES), Entry::Executable(BYTES), Entry::File(BYTES)]
        .map(|entry| fixture.commit_entries_on(&[&base], "side", &[("a.sh", entry)]));
    fixture.check_out(&sides.each_ref());
    let repo = fixture.open();
    assert_eq!(repo.conflicts().expect("the conflicts are read"), Vec::<Utf8PathBuf>::new());
    assert_eq!(repo.read_conflict(Utf8Path::new("a.sh")).expect("the conflict is read"), None);
}

#[test]
fn a_directory_against_files_is_a_conflict_with_a_directory_term() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("d", b"a\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("d/inner.txt", b"inner\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("d", b"right\n")]);
    fixture.check_out(&[&left, &right]);
    let repo = fixture.open();
    assert_eq!(repo.conflicts().expect("the conflicts are read"), [Utf8PathBuf::from("d")]);
    let conflict = repo
        .read_conflict(Utf8Path::new("d"))
        .expect("the conflict is read")
        .expect("d is conflicted");
    let file = |bytes: &[u8]| TermContent::File { bytes: bytes.to_vec(), executable: false };
    let contents = |terms: Vec<vcs::ConflictTerm>| {
        terms.into_iter().map(|term| term.content).collect::<Vec<_>>()
    };
    assert_eq!(contents(conflict.bases), [file(b"a\n")]);
    assert_eq!(
        contents(conflict.sides),
        [TermContent::Other(vcs::EntryKind::Directory), file(b"right\n")]
    );
}

/// jj's label for a fixture commit in conflict markers: change ID and commit ID to eight
/// characters, then the description's first line in quotes.
fn jj_label(committed: &vcs_jj::testing::Committed, summary: &str) -> vcs::Term {
    vcs::Term {
        label: Some(format!(
            "{} {} \"{summary}\"",
            &committed.change_id[..8],
            &committed.commit[..8]
        )),
        change: None,
    }
}

#[test]
fn each_term_of_a_conflict_takes_jjs_label_for_its_commit() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
    fixture.check_out(&[&left, &right]);
    let conflict = fixture
        .open()
        .read_conflict(Utf8Path::new("a.txt"))
        .expect("the conflict is read")
        .expect("a.txt is conflicted");
    let terms = |terms: &[vcs::ConflictTerm]| {
        terms.iter().map(|term| term.term.clone()).collect::<Vec<_>>()
    };
    assert_eq!(terms(&conflict.sides), [jj_label(&left, "left"), jj_label(&right, "right")]);
    assert_eq!(terms(&conflict.bases), [jj_label(&base, "base")]);
}

/// A working copy that merges two parents whose `a.txt` conflict, checked out to disk.
fn checked_out_conflict(left: &[u8], right: &[u8]) -> Fixture {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n"), ("clean.txt", b"c\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", left), ("clean.txt", b"c\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", right), ("clean.txt", b"c\n")]);
    fixture.check_out(&[&left, &right]);
    fixture.check_out_to_disk();
    fixture
}

fn recorded(fixture: &Fixture, path: &str) -> Option<vcs::Recorded> {
    fixture.open().recorded(Utf8Path::new(path)).expect("the working copy's state reads")
}

#[test]
fn a_file_as_jj_wrote_it_is_recorded_unchanged_with_its_marker_length() {
    let fixture = checked_out_conflict(b"left\n", b"right\n");
    assert_eq!(
        recorded(&fixture, "a.txt"),
        Some(vcs::Recorded { changed: false, marker_length: Some(7) })
    );
    assert_eq!(
        recorded(&fixture, "clean.txt"),
        Some(vcs::Recorded { changed: false, marker_length: None })
    );
}

#[test]
fn marker_like_lines_lengthen_jjs_markers_by_four() {
    // The longest marker-like line is a run of 7, so jj writes markers of 7 + 4.
    let fixture = checked_out_conflict(b"<<<<<<< left\n", b"right\n");
    assert_eq!(recorded(&fixture, "a.txt").and_then(|r| r.marker_length), Some(11));
}

#[test]
fn a_file_edited_since_the_checkout_is_changed() {
    let fixture = checked_out_conflict(b"left\n", b"right\n");
    fixture.write("a.txt", b"resolved by hand, a different size\n");
    assert_eq!(recorded(&fixture, "a.txt").map(|r| r.changed), Some(true));
}

#[test]
fn a_deleted_file_is_unchanged_and_an_unknown_one_unrecorded() {
    let fixture = checked_out_conflict(b"left\n", b"right\n");
    std::fs::remove_file(fixture.root().join("a.txt")).expect("removed");
    assert_eq!(recorded(&fixture, "a.txt").map(|r| r.changed), Some(false));
    fixture.write("new.txt", b"new\n");
    assert_eq!(recorded(&fixture, "new.txt"), None);
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_examined_is_a_backend_error_not_a_deleted_file() {
    use std::os::unix::fs::PermissionsExt as _;
    const NO_ACCESS: u32 = 0o000;
    const OPEN: u32 = 0o755;

    let fixture = Fixture::new();
    let commit = fixture.commit_on(&[], "one", &[("sub/a.txt", b"a\n")]);
    fixture.check_out(&[&commit]);
    fixture.check_out_to_disk();
    let sub = fixture.root().join("sub");
    let set_mode = |mode| {
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(mode)).expect("chmod");
    };
    let repo = fixture.open();
    set_mode(NO_ACCESS);
    let result = repo.recorded(Utf8Path::new("sub/a.txt"));
    set_mode(OPEN);
    match result {
        Err(VcsError::Backend { context, .. }) => {
            assert_eq!(context, "reading a file's metadata");
        }
        other => panic!("expected a backend error, got {other:?}"),
    }
}

#[test]
fn a_file_checked_out_from_another_conflict_is_stale() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n"), ("b.txt", b"b\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n"), ("b.txt", b"b\n")]);
    let other = fixture.commit_on(&[&base], "other", &[("a.txt", b"other\n"), ("b.txt", b"b\n")]);
    fixture.check_out(&[&left, &right]);
    fixture.check_out_to_disk();
    // `@` moves, as in another workspace, and the files on disk stay as they were.
    fixture.check_out(&[&left, &other]);
    let error = fixture
        .open()
        .recorded(Utf8Path::new("a.txt"))
        .expect_err("a.txt on disk is the old conflict");
    assert!(matches!(&error, VcsError::StaleWorkingCopy(path) if path == "a.txt"), "{error:?}");
    assert_eq!(
        recorded(&fixture, "b.txt"),
        Some(vcs::Recorded { changed: false, marker_length: None }),
        "b.txt is the same in both"
    );
}

#[test]
fn a_new_working_copy_commit_with_the_same_conflict_is_not_stale() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
    fixture.check_out(&[&left, &right]);
    fixture.check_out_to_disk();
    fixture.check_out(&[&left, &right]);
    assert_eq!(
        recorded(&fixture, "a.txt"),
        Some(vcs::Recorded { changed: false, marker_length: Some(7) })
    );
}

#[test]
fn a_third_parent_that_leaves_the_file_alone_is_not_stale() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n"), ("b.txt", b"b\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n"), ("b.txt", b"b\n")]);
    let third = fixture.commit_on(&[&base], "third", &[("a.txt", b"a\n"), ("b.txt", b"third\n")]);
    fixture.check_out(&[&left, &right]);
    fixture.check_out_to_disk();
    // `@`'s tree has three sides, and a.txt's conflict, simplified, is the one on disk.
    fixture.check_out(&[&left, &right, &third]);
    assert_eq!(
        recorded(&fixture, "a.txt"),
        Some(vcs::Recorded { changed: false, marker_length: Some(7) })
    );
}

#[test]
fn a_same_size_edit_is_changed_by_its_modification_time() {
    /// A time well before the checkout: 1000 s after the epoch.
    const EARLIER: std::time::Duration = std::time::Duration::from_secs(1000);
    let fixture = checked_out_conflict(b"left\n", b"right\n");
    let path = fixture.root().join("a.txt");
    let size = std::fs::metadata(&path).expect("the file exists").len();
    let same_size = vec![b'x'; usize::try_from(size).expect("a small file")];
    fixture.write("a.txt", &same_size);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_modified(std::time::UNIX_EPOCH + EARLIER))
        .expect("the modification time is set");
    assert_eq!(recorded(&fixture, "a.txt").map(|r| r.changed), Some(true));
}

// The commits each input of a merge or a diff tool's copy may hold (spec §7.3).

fn id(committed: &vcs_jj::testing::Committed) -> vcs::CommitId {
    vcs::CommitId::new(committed.commit.clone())
}

#[test]
fn a_merges_inputs_may_hold_the_working_copy_its_parents_in_order_and_their_merge_base() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
    let merge = fixture.commit_on(&[&left, &right], "merge", &[("a.txt", b"merged\n")]);
    fixture.edit(&merge);
    let every = vec![id(&merge), id(&left), id(&right), id(&base)];
    assert_eq!(
        fixture.open().merge_candidates().expect("the candidates are found"),
        vcs::MergeCandidates { shared: every, ..vcs::MergeCandidates::default() }
    );
}

#[test]
fn every_term_of_a_wider_merge_may_hold_the_working_copy_a_parent_or_their_merge_base() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let one = fixture.commit_on(&[&base], "one", &[("a.txt", b"one\n")]);
    let two = fixture.commit_on(&[&base], "two", &[("a.txt", b"two\n")]);
    let three = fixture.commit_on(&[&base], "three", &[("a.txt", b"three\n")]);
    let merge = fixture.commit_on(&[&one, &two, &three], "merge", &[("a.txt", b"merged\n")]);
    fixture.edit(&merge);
    let candidates = fixture.open().merge_candidates().expect("the candidates are found");
    let every = vec![id(&merge), id(&one), id(&two), id(&three), id(&base)];
    assert_eq!(candidates.shared, every);
    assert_eq!(candidates.side(2).cloned().collect::<Vec<_>>(), every);
    assert_eq!(candidates.base(1).cloned().collect::<Vec<_>>(), every);
}

#[test]
fn parents_without_one_common_merge_base_offer_none() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
    // Two merges of the same parents: criss-cross, so their two forks have two merge bases.
    let one = fixture.commit_on(&[&left, &right], "one", &[("a.txt", b"one\n")]);
    let two = fixture.commit_on(&[&left, &right], "two", &[("a.txt", b"two\n")]);
    let third = fixture.commit_on(&[&base], "third", &[("a.txt", b"third\n")]);
    let merge = fixture.commit_on(&[&one, &two, &third], "merge", &[("a.txt", b"merged\n")]);
    fixture.edit(&merge);
    let candidates = fixture.open().merge_candidates().expect("the candidates are found");
    assert_eq!(candidates.shared, vec![id(&merge), id(&one), id(&two), id(&third)]);
}

#[test]
fn with_one_parent_a_merges_inputs_may_hold_the_working_copy_or_its_parent() {
    let fixture = Fixture::new();
    let one = fixture.commit_on(&[], "one", &[("a.txt", b"a\n")]);
    let two = fixture.commit_on(&[&one], "two", &[("a.txt", b"b\n")]);
    fixture.edit(&two);
    let every = vec![id(&two), id(&one)];
    assert_eq!(
        fixture.open().merge_candidates().expect("the candidates are found"),
        vcs::MergeCandidates { shared: every, ..vcs::MergeCandidates::default() }
    );
}

#[test]
fn parents_on_the_root_commit_offer_it_as_their_merge_base() {
    let fixture = Fixture::new();
    let left = fixture.commit_on(&[], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[], "right", &[("a.txt", b"right\n")]);
    let merge = fixture.commit_on(&[&left, &right], "merge", &[("a.txt", b"merged\n")]);
    fixture.edit(&merge);
    // Both parents sit on the root commit, so their one merge base is the root.
    let root = vcs::CommitId::new(format!("{ROOT_COMMIT}{}", "0".repeat(32)));
    let every = vec![id(&merge), id(&left), id(&right), root];
    assert_eq!(
        fixture.open().merge_candidates().expect("the candidates are found"),
        vcs::MergeCandidates { shared: every, ..vcs::MergeCandidates::default() }
    );
}

#[test]
fn a_diff_tools_copy_may_hold_the_parent_then_the_working_copy() {
    let fixture = Fixture::new();
    let one = fixture.commit_on(&[], "one", &[("a.txt", b"a\n")]);
    let two = fixture.commit_on(&[&one], "two", &[("a.txt", b"b\n")]);
    fixture.edit(&two);
    assert_eq!(fixture.open().copy_candidates().expect("@ reads"), vec![id(&one), id(&two)]);
}

#[test]
fn a_diff_tools_copy_of_a_merge_may_hold_only_the_working_copy() {
    let fixture = Fixture::new();
    let left = fixture.commit_on(&[], "left", &[("a.txt", b"left\n")]);
    let right = fixture.commit_on(&[], "right", &[("a.txt", b"right\n")]);
    let merge = fixture.commit_on(&[&left, &right], "merge", &[("a.txt", b"merged\n")]);
    fixture.edit(&merge);
    assert_eq!(fixture.open().copy_candidates().expect("@ reads"), vec![id(&merge)]);
}

#[test]
fn the_working_copy_is_a_commit() {
    let fixture = Fixture::new();
    let one = fixture.commit_on(&[], "one", &[("a.txt", b"a\n")]);
    fixture.edit(&one);
    assert_eq!(fixture.open().working_copy_id().expect("@ reads"), Some(id(&one)));
}

#[test]
fn jj_has_no_index_to_stage_on_and_no_conflict_style() {
    let repo = Fixture::new().open();
    assert_eq!(repo.index_base().expect("nothing to read"), None);
    assert_eq!(repo.conflict_style().expect("jj reads no style"), None);
}
