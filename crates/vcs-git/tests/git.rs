//! `GitRepository` on repositories built in temporary directories through gix (DESIGN.md §A7.1).

use camino::{Utf8Path, Utf8PathBuf};
use vcs::testing::Entry;
use vcs::{ChangeInfo, CommitId, ConflictStyle, RepoKind, Repository, RevSpec, Term, VcsError};
use vcs_git::GitRepository;
use vcs_git::testing::Fixture;

fn read(repo: &GitRepository, path: &str, rev: &RevSpec) -> Option<Vec<u8>> {
    repo.file_at(Utf8Path::new(path), rev).expect("the read succeeds")
}

fn head() -> RevSpec {
    RevSpec::Expr("HEAD".to_owned())
}

#[test]
fn discovery_climbs_from_a_subdirectory() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let sub = fixture.root().join("src/deep");
    std::fs::create_dir_all(&sub).expect("the directories are made");
    let repo = GitRepository::discover(&sub).expect("the repository is found");
    assert_eq!(repo.kind(), RepoKind::Git);
    assert_eq!(read(&repo, "a.txt", &head()).as_deref(), Some(&b"a\n"[..]));
}

#[test]
fn the_root_is_the_working_tree() {
    let fixture = Fixture::new();
    let sub = fixture.root().join("src");
    std::fs::create_dir_all(&sub).expect("the directory is made");
    let repo = GitRepository::discover(&sub).expect("the repository is found");
    assert_eq!(
        repo.root().canonicalize_utf8().expect("the root exists"),
        fixture.root().canonicalize_utf8().expect("the fixture exists")
    );
}

#[test]
fn a_bare_repository_has_no_working_copy_and_names_its_git_directory() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    gix::init_bare(dir.path()).expect("a bare repository");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("UTF-8");
    let canonical = |path: &Utf8Path| path.canonicalize_utf8().expect("the path exists");
    match GitRepository::discover(&path) {
        Err(VcsError::NoWorkingCopy(named)) => assert_eq!(canonical(&named), canonical(&path)),
        Err(other) => panic!("expected NoWorkingCopy, got {other}"),
        Ok(_) => panic!("expected NoWorkingCopy, the repository opened"),
    }
}

#[test]
fn a_directory_outside_any_repository_is_named() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("UTF-8");
    match GitRepository::discover(&path) {
        Err(VcsError::NotARepository(named)) => assert_eq!(named, path),
        Err(other) => panic!("expected NotARepository, got {other}"),
        Ok(_) => panic!("expected NotARepository, found a repository"),
    }
}

#[test]
fn open_takes_the_working_tree_at_the_root() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let repo = GitRepository::open(fixture.root()).expect("the repository opens");
    assert_eq!(repo.kind(), RepoKind::Git);
    assert_eq!(
        repo.root().canonicalize_utf8().expect("the root exists"),
        fixture.root().canonicalize_utf8().expect("the fixture exists")
    );
    assert_eq!(read(&repo, "a.txt", &head()).as_deref(), Some(&b"a\n"[..]));
}

#[test]
fn open_does_not_climb_from_a_subdirectory() {
    let fixture = Fixture::new();
    let sub = fixture.root().join("src");
    std::fs::create_dir_all(&sub).expect("the directory is made");
    match GitRepository::open(&sub) {
        Err(VcsError::NotARepository(named)) => assert_eq!(named, sub),
        Err(other) => panic!("expected NotARepository, got {other}"),
        Ok(_) => panic!("expected NotARepository, the parent opened"),
    }
}

#[test]
fn open_refuses_a_git_directory_that_holds_no_repository() {
    // An empty `.git` inside a repository: discovery climbs past it, open does not.
    let fixture = Fixture::new();
    let sub = fixture.root().join("sub");
    std::fs::create_dir_all(sub.join(".git")).expect("the directories are made");
    match GitRepository::open(&sub) {
        Err(VcsError::NotARepository(named)) => assert_eq!(named, sub),
        Err(other) => panic!("expected NotARepository, got {other}"),
        Ok(_) => panic!("expected NotARepository, a repository opened"),
    }
}

#[test]
fn a_revision_gives_the_file_as_committed_then() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"one\n")]);
    fixture.commit("two", &[("a.txt", b"two\n")]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "a.txt", &head()).as_deref(), Some(&b"two\n"[..]));
    let previous = RevSpec::Expr("HEAD~1".to_owned());
    assert_eq!(read(&repo, "a.txt", &previous).as_deref(), Some(&b"one\n"[..]));
}

#[test]
fn a_nested_path_is_found() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("src/lib/a.rs", b"fn a() {}\n")]);
    assert_eq!(
        read(&fixture.open(), "src/lib/a.rs", &head()).as_deref(),
        Some(&b"fn a() {}\n"[..])
    );
}

#[test]
fn a_missing_path_or_a_directory_is_none() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("src/a.rs", b"a\n")]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "b.rs", &head()), None);
    assert_eq!(read(&repo, "src/b.rs", &head()), None);
    assert_eq!(read(&repo, "src", &head()), None);
}

#[test]
fn a_file_added_later_is_none_at_an_earlier_revision() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.commit("two", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "b.txt", &RevSpec::Expr("HEAD~1".to_owned())), None);
}

#[test]
fn binary_bytes_come_back_unchanged() {
    let bytes: &[u8] = &[0, 0xff, b'\r', b'\n', 0x80, 0];
    let fixture = Fixture::new();
    fixture.commit("one", &[("blob.bin", bytes)]);
    assert_eq!(read(&fixture.open(), "blob.bin", &head()).as_deref(), Some(bytes));
}

#[test]
fn the_index_gives_the_staged_version() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"committed\n")]);
    fixture.stage(&[("a.txt", b"staged\n"), ("new.txt", b"new\n")]);
    let repo = fixture.open();
    assert_eq!(read(&repo, "a.txt", &RevSpec::Index).as_deref(), Some(&b"staged\n"[..]));
    assert_eq!(read(&repo, "new.txt", &RevSpec::Index).as_deref(), Some(&b"new\n"[..]));
    assert_eq!(read(&repo, "gone.txt", &RevSpec::Index), None);
    assert_eq!(read(&repo, "a.txt", &head()).as_deref(), Some(&b"committed\n"[..]));
}

#[test]
fn an_unknown_revision_is_named() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    match fixture.open().file_at(Utf8Path::new("a.txt"), &RevSpec::Expr("nope".to_owned())) {
        Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, "nope"),
        other => panic!("expected UnknownRevision, got {other:?}"),
    }
}

fn parent(fixture: &Fixture, path: &str) -> vcs::Parent {
    fixture.open().default_parent(Utf8Path::new(path)).expect("the parent is found")
}

#[test]
fn an_unstaged_file_is_compared_with_head_on_its_branch() {
    let fixture = Fixture::new();
    fixture.commit("first\n\nbody", &[("a.txt", b"a\n")]);
    let commit = fixture.commit("second change\n\nmore words", &[("a.txt", b"b\n")]);
    // The index matches HEAD, as after a checkout.
    fixture.stage(&[("a.txt", b"b\n")]);
    let parent = parent(&fixture, "a.txt");
    assert_eq!(parent.rev, head());
    // gix initialises HEAD to refs/heads/main.
    assert_eq!(parent.term.label.as_deref(), Some("main"));
    let change = parent.term.change.expect("HEAD's commit");
    assert!(
        commit.to_string().starts_with(&change.commit) && change.commit.len() >= 7,
        "{} is a prefix of {commit}",
        change.commit
    );
    assert_eq!(change.change_id, None);
    assert_eq!(change.summary.as_deref(), Some("second change"));
}

#[test]
fn a_staged_file_is_compared_with_the_index() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n"), ("b.txt", b"b\n")]);
    fixture.stage(&[("a.txt", b"staged\n"), ("b.txt", b"b\n")]);
    let staged = parent(&fixture, "a.txt");
    assert_eq!(staged.rev, RevSpec::Index);
    assert_eq!(staged.term.label.as_deref(), Some("index"));
    assert_eq!(staged.term.change, None);
    assert_eq!(parent(&fixture, "b.txt").rev, head(), "b.txt has no staged change");
}

#[test]
fn a_newly_staged_or_staged_deleted_file_counts_as_staged() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    fixture.stage(&[("new.txt", b"new\n")]);
    assert_eq!(parent(&fixture, "new.txt").rev, RevSpec::Index);
    assert_eq!(parent(&fixture, "a.txt").rev, RevSpec::Index, "deletion staged");
}

#[test]
fn before_the_first_commit_the_parent_is_the_index() {
    let fixture = Fixture::new();
    fixture.stage(&[("a.txt", b"a\n")]);
    assert_eq!(parent(&fixture, "a.txt").rev, RevSpec::Index);
    assert_eq!(parent(&fixture, "b.txt").rev, RevSpec::Index);
}

#[test]
fn an_expression_naming_a_tree_names_no_commit() {
    let fixture = Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    match fixture.open().commits("HEAD^{tree}") {
        Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, "HEAD^{tree}"),
        other => panic!("expected UnknownRevision, got {other:?}"),
    }
}

#[test]
fn unrelated_histories_have_no_merge_base() {
    let fixture = Fixture::new();
    let one = fixture.commit("one", &[("a.txt", b"a\n")]);
    let other = fixture.commit_on(&[], "other", &[("a.txt", b"b\n")]);
    assert_eq!(
        fixture
            .open()
            .merge_base(&CommitId::new(one.to_string()), &CommitId::new(other.to_string()))
            .expect("the bases are found"),
        None
    );
}

// The terms of a conflict follow the operation that stopped (spec §7.3, §7.4).

const CONFLICTED: &str = "a.txt";
/// git shortens commit IDs to this many characters in a small repository (`core.abbrev`).
const SHORT: usize = 7;

fn commit_term(id: gix::ObjectId, summary: &str) -> Term {
    Term {
        label: None,
        change: Some(ChangeInfo {
            commit: id.to_string()[..SHORT].to_owned(),
            change_id: None,
            summary: Some(summary.to_owned()),
        }),
    }
}

/// `one`, then `left` on it on `main` as `HEAD`, and `right` on `one` beside it, with the
/// index stopped on a conflict in [`CONFLICTED`].
fn forked() -> (Fixture, [gix::ObjectId; 3]) {
    let fixture = Fixture::new();
    let one = fixture.commit("one", &[(CONFLICTED, b"a\n")]);
    let left = fixture.commit("left", &[(CONFLICTED, b"left\n")]);
    let right = fixture.commit_on(&[one], "right", &[(CONFLICTED, b"right\n")]);
    let terms: [&[u8]; 3] = [b"a\n", b"left\n", b"right\n"];
    fixture.stage_conflicts(&[], &[(CONFLICTED, terms.map(|t| Some(Entry::File(t))))]);
    (fixture, [one, left, right])
}

/// The base's, ours and theirs terms of the conflict.
fn terms(fixture: &Fixture) -> [Term; 3] {
    let conflict = fixture
        .open()
        .read_conflict(Utf8Path::new(CONFLICTED))
        .expect("the conflict is read")
        .expect("the path is conflicted");
    let [base] = <[_; 1]>::try_from(conflict.bases).expect("one base");
    let [ours, theirs] = <[_; 2]>::try_from(conflict.sides).expect("two sides");
    [base.term, ours.term, theirs.term]
}

fn ours(left: gix::ObjectId) -> Term {
    Term { label: Some("main".to_owned()), ..commit_term(left, "left") }
}

#[test]
fn a_stopped_merge_names_head_merge_head_and_their_merge_base() {
    let (fixture, [one, left, right]) = forked();
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(terms(&fixture), [commit_term(one, "one"), ours(left), commit_term(right, "right")]);
}

#[test]
fn a_stopped_cherry_pick_or_rebase_takes_the_commit_over_its_parent() {
    for name in ["CHERRY_PICK_HEAD", "REBASE_HEAD"] {
        let (fixture, [one, left, right]) = forked();
        fixture.write_pseudo_ref(name, right);
        assert_eq!(
            terms(&fixture),
            [commit_term(one, "one"), ours(left), commit_term(right, "right")],
            "{name}"
        );
    }
}

#[test]
fn a_stopped_revert_takes_the_commits_parent_over_the_commit() {
    let (fixture, [one, left, right]) = forked();
    fixture.write_pseudo_ref("REVERT_HEAD", right);
    assert_eq!(terms(&fixture), [commit_term(right, "right"), ours(left), commit_term(one, "one")]);
}

#[test]
fn a_merge_comes_before_a_cherry_pick_left_over_from_earlier() {
    let (fixture, [one, left, right]) = forked();
    fixture.write_pseudo_ref("CHERRY_PICK_HEAD", left);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(terms(&fixture), [commit_term(one, "one"), ours(left), commit_term(right, "right")]);
}

#[test]
fn without_a_stopped_operation_only_head_is_known() {
    let (fixture, [_, left, _]) = forked();
    assert_eq!(terms(&fixture), [Term::default(), ours(left), Term::default()]);
}

#[test]
fn a_stopped_merges_base_is_the_merge_base_not_the_parent_of_merge_head() {
    let (fixture, [one, left, _]) = forked();
    let middle = fixture.commit_on(&[one], "middle", &[(CONFLICTED, b"middle\n")]);
    let right = fixture.commit_on(&[middle], "right", &[(CONFLICTED, b"right\n")]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(terms(&fixture), [commit_term(one, "one"), ours(left), commit_term(right, "right")]);
}

#[test]
fn git_records_no_working_tree_state() {
    let (fixture, _) = forked();
    fixture.write(CONFLICTED, b"edited\n");
    assert_eq!(fixture.open().recorded(Utf8Path::new(CONFLICTED)).expect("nothing to read"), None);
}

#[test]
fn the_conflict_style_comes_from_the_repositorys_config() {
    for (value, style) in [
        ("merge", ConflictStyle::Merge),
        ("diff3", ConflictStyle::Diff3),
        ("zdiff3", ConflictStyle::Zdiff3),
    ] {
        let fixture = Fixture::new();
        fixture.append_config(&format!("[merge]\n\tconflictStyle = {value}\n"));
        assert_eq!(fixture.open().conflict_style().expect("a known style"), Some(style), "{value}");
    }
}

#[test]
fn a_conflict_style_git_does_not_know_is_an_error_naming_it() {
    // git compares the value case-sensitively and refuses one it does not know.
    for value in ["Merge", "ours", ""] {
        let fixture = Fixture::new();
        fixture.append_config(&format!("[merge]\n\tconflictStyle = \"{value}\"\n"));
        match fixture.open().conflict_style() {
            Err(VcsError::BadConfig { key, value: found }) => {
                assert_eq!(key, "merge.conflictStyle");
                assert_eq!(found, value);
            }
            other => panic!("{value:?}: {other:?}"),
        }
    }
}

#[test]
fn without_the_key_there_is_no_conflict_style() {
    assert_eq!(Fixture::new().open().conflict_style().expect("no key"), None);
}

#[test]
fn the_config_key_is_case_insensitive_as_in_git() {
    let fixture = Fixture::new();
    fixture.append_config("[MERGE]\n\tconflictstyle = merge\n");
    assert_eq!(fixture.open().conflict_style().expect("a known style"), Some(ConflictStyle::Merge));
}

// The commits each input of a merge may hold follow the stopped operation, as its terms do
// (spec §7.3).

fn ids(commits: &[gix::ObjectId]) -> Vec<CommitId> {
    commits.iter().map(|id| CommitId::new(id.to_string())).collect()
}

/// The candidates of git's one base and two sides, in the order base, ours, theirs. git
/// shares no commit between the terms.
fn merge_candidates(fixture: &Fixture) -> [Vec<CommitId>; 3] {
    triple(fixture.open().merge_candidates().expect("the candidates are found"))
}

fn triple(candidates: vcs::MergeCandidates) -> [Vec<CommitId>; 3] {
    let vcs::MergeCandidates { bases, sides, shared } = candidates;
    assert_eq!(shared, Vec::new(), "git shares no commit");
    let [base] = <[_; 1]>::try_from(bases).map_err(|b| b.len()).expect("git names one base");
    let [ours, theirs] =
        <[_; 2]>::try_from(sides).map_err(|s| s.len()).expect("git names two sides");
    [base, ours, theirs]
}

#[test]
fn a_stopped_merge_offers_the_merge_base_head_and_merge_head() {
    let (fixture, [one, left, right]) = forked();
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(merge_candidates(&fixture), [ids(&[one]), ids(&[left]), ids(&[right])]);
}

#[test]
fn a_stopped_cherry_pick_or_rebase_offers_the_commits_parent_head_and_the_commit() {
    for name in ["CHERRY_PICK_HEAD", "REBASE_HEAD"] {
        let (fixture, [one, left, right]) = forked();
        fixture.write_pseudo_ref(name, right);
        assert_eq!(
            merge_candidates(&fixture),
            [ids(&[one]), ids(&[left]), ids(&[right])],
            "{name}"
        );
    }
}

#[test]
fn a_stopped_revert_offers_the_commit_head_and_the_commits_parent() {
    let (fixture, [one, left, right]) = forked();
    fixture.write_pseudo_ref("REVERT_HEAD", right);
    assert_eq!(merge_candidates(&fixture), [ids(&[right]), ids(&[left]), ids(&[one])]);
}

#[test]
fn a_stopped_merge_is_offered_before_a_cherry_pick_left_over_from_earlier() {
    let (fixture, [one, left, right]) = forked();
    fixture.write_pseudo_ref("CHERRY_PICK_HEAD", left);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(merge_candidates(&fixture), [ids(&[one]), ids(&[left]), ids(&[right])]);
}

#[test]
fn a_stopped_merges_base_candidate_is_the_merge_base_not_the_parent() {
    let (fixture, [one, left, _]) = forked();
    let middle = fixture.commit_on(&[one], "middle", &[(CONFLICTED, b"middle\n")]);
    let right = fixture.commit_on(&[middle], "right", &[(CONFLICTED, b"right\n")]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(merge_candidates(&fixture), [ids(&[one]), ids(&[left]), ids(&[right])]);
}

#[test]
fn a_cherry_pick_of_a_root_commit_offers_no_base() {
    let (fixture, [_, left, _]) = forked();
    let root = fixture.commit_on(&[], "root", &[(CONFLICTED, b"root\n")]);
    fixture.write_pseudo_ref("CHERRY_PICK_HEAD", root);
    assert_eq!(merge_candidates(&fixture), [Vec::new(), ids(&[left]), ids(&[root])]);
}

#[test]
fn without_a_stopped_operation_only_head_is_offered() {
    let (fixture, [_, left, _]) = forked();
    assert_eq!(merge_candidates(&fixture), [Vec::new(), ids(&[left]), Vec::new()]);
}

#[test]
fn before_the_first_commit_nothing_is_offered() {
    let fixture = Fixture::new();
    let repo = fixture.open();
    assert_eq!(
        triple(repo.merge_candidates().expect("the candidates are found")),
        [Vec::new(), Vec::new(), Vec::new()]
    );
    assert_eq!(repo.copy_candidates().expect("HEAD reads"), Vec::new());
    assert_eq!(repo.index_base().expect("HEAD reads"), None);
}

#[test]
fn a_diff_tools_copy_may_hold_head() {
    let (fixture, [_, left, _]) = forked();
    assert_eq!(fixture.open().copy_candidates().expect("HEAD reads"), ids(&[left]));
}

#[test]
fn the_index_stages_changes_on_head() {
    let (fixture, [_, left, _]) = forked();
    assert_eq!(
        fixture.open().index_base().expect("HEAD reads"),
        Some(CommitId::new(left.to_string()))
    );
}

#[test]
fn the_working_tree_is_no_commit() {
    let (fixture, _) = forked();
    assert_eq!(fixture.open().working_copy_id().expect("nothing to read"), None);
}
