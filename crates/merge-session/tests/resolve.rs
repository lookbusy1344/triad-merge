//! `resolve`'s inputs from conflicts in git, colocated jj and internal jj repositories built
//! through gix and jj-lib (`DESIGN.md` §D5.3).

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use merge_core::document::Labels;
use merge_core::text::DecodeError;
use merge_session::repository::RunRepository;
use merge_session::resolve::{self, Conflicted, Loaded, ResolveError, Snapshot};
use merge_session::{EncodedOutput, ExitCode};
use vcs::testing::Entry;
use vcs::{EntryKind, RepoKind, VcsError};
use vcs_jj::config::ConfigEnv;

/// git shortens commit IDs to this many characters in a small repository.
const GIT_SHORT: usize = 7;
/// jj's conflict labels show change and commit IDs to this many characters.
const JJ_SHORT: usize = 8;

const BASE: &[u8] = b"a\n";
const LEFT: &[u8] = b"left\n";
const RIGHT: &[u8] = b"right\n";

fn canonical(path: &Utf8Path) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dunce::canonicalize(path).expect("the path exists")).expect("UTF-8")
}

fn content(texts: &[merge_core::text::Text]) -> Vec<&str> {
    texts.iter().map(merge_core::text::Text::content).collect()
}

fn texts(loaded: &Loaded) -> (Vec<&str>, Vec<&str>) {
    (content(loaded.inputs.sides()), content(loaded.inputs.bases()))
}

fn labels(sides: &[&str], bases: &[&str]) -> Labels {
    let owned = |l: &[&str]| l.iter().map(|&s| s.to_owned()).collect();
    Labels::new(owned(sides), owned(bases))
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

/// [`resolve::load`] in the repository a `resolve` in `cwd` on `path` finds, with jj's
/// default config for its snapshots.
fn load(cwd: &Path, path: &Path) -> Result<Loaded, ResolveError> {
    let mut run = RunRepository::of_resolve(cwd, Some(path)).with_jj_config(ConfigEnv::default());
    resolve::load(&mut run, cwd, path, Snapshot::Take)
}

/// [`resolve::conflicted`] in the repository holding `cwd`, with jj's default config.
fn conflicted(cwd: &Path) -> Result<Conflicted, ResolveError> {
    resolve::conflicted(&mut RunRepository::find([cwd]).with_jj_config(ConfigEnv::default()))
}

#[test]
fn a_git_conflict_loads_its_stages_titled_by_the_stopped_merge() {
    let (fixture, [one, right]) =
        git_stopped(&[("clean.txt", b"c\n")], &[("a.txt", files(BOTH_CHANGED))]);
    let loaded =
        load(fixture.root().as_std_path(), Path::new("a.txt")).expect("the conflict loads");
    assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
    assert_eq!(
        loaded.labels,
        labels(&["main", &format!("{right} right")], &[&format!("{one} one")])
    );
    assert_eq!(loaded.repo, Some(RepoKind::Git));
    assert_eq!(loaded.path, Utf8PathBuf::from("a.txt"));
    assert_eq!(loaded.file, canonical(fixture.root()).join("a.txt"));
}

#[test]
fn an_absent_term_is_an_empty_text() {
    let (fixture, _) = git_stopped(
        &[],
        &[
            ("new.txt", files([None, Some(LEFT), Some(RIGHT)])),
            ("gone.txt", files([Some(BASE), Some(LEFT), None])),
        ],
    );
    let root = fixture.root().as_std_path();
    let added = load(root, Path::new("new.txt")).expect("add/add loads");
    assert_eq!(texts(&added), (vec!["left\n", "right\n"], vec![""]));
    let deleted = load(root, Path::new("gone.txt")).expect("modify/delete loads");
    assert_eq!(texts(&deleted), (vec!["left\n", ""], vec!["a\n"]));
}

#[test]
fn a_path_resolves_against_the_working_directory() {
    let (fixture, _) = git_stopped(&[], &[("src/b.txt", files(BOTH_CHANGED))]);
    let src = fixture.root().join("src");
    std::fs::create_dir_all(&src).expect("the directory is made");
    let loaded = load(src.as_std_path(), Path::new("b.txt")).expect("the conflict loads");
    assert_eq!(loaded.path, Utf8PathBuf::from("src/b.txt"));
    assert_eq!(loaded.file, canonical(fixture.root()).join("src/b.txt"));
    assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
}

fn jj_label(committed: &vcs_jj::testing::Committed, summary: &str) -> String {
    format!("{} {} \"{summary}\"", &committed.change_id[..JJ_SHORT], &committed.commit[..JJ_SHORT])
}

fn jj_conflict(fixture: &vcs_jj::testing::Fixture) -> [String; 3] {
    let [base, left, right] = fixture.three_way("a.txt", [BASE, LEFT, RIGHT]);
    fixture.check_out_to_disk();
    [jj_label(&base, "base"), jj_label(&left, "left"), jj_label(&right, "right")]
}

#[test]
fn the_conflicted_paths_are_listed_in_order() {
    let (fixture, _) = git_stopped(
        &[("clean.txt", b"c\n")],
        &[("z.txt", files(BOTH_CHANGED)), ("src/b.txt", files(BOTH_CHANGED))],
    );
    let listed = conflicted(fixture.root().as_std_path()).expect("the list is read");
    assert_eq!(listed.root, canonical(fixture.root()));
    assert_eq!(listed.paths, [Utf8PathBuf::from("src/b.txt"), Utf8PathBuf::from("z.txt")]);
}

#[test]
fn outside_a_repository_a_file_without_markers_is_not_conflicted() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("a.txt"), "a\n").expect("written");
    let error = load(dir.path(), Path::new("a.txt")).expect_err("no conflict");
    assert!(matches!(&error, ResolveError::NotConflicted(path) if path == "a.txt"), "{error:?}");
    let error = conflicted(dir.path()).expect_err("no repository");
    assert!(matches!(error, ResolveError::Vcs(VcsError::NotARepository(_))), "{error:?}");
}

const DIFF3: &[u8] = b"a\n<<<<<<< ours\nL\n||||||| anc\nB\n=======\nR\n>>>>>>> theirs\nz\n";

#[test]
fn a_clean_file_with_markers_opens_from_them() {
    let (fixture, _) = git_stopped(&[("m.txt", b"m\n")], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("src/m.txt", DIFF3);
    let loaded =
        load(fixture.root().as_std_path(), Path::new("src/m.txt")).expect("the markers load");
    assert_eq!(texts(&loaded), (vec!["a\nL\nz\n", "a\nR\nz\n"], vec!["a\nB\nz\n"]));
    assert_eq!(loaded.labels, labels(&["ours", "theirs"], &["anc"]));
    assert_eq!(loaded.repo, Some(RepoKind::Git));
    assert_eq!(loaded.path, Utf8PathBuf::from("src/m.txt"));
    assert_eq!(loaded.file, canonical(fixture.root()).join("src/m.txt"));
}

#[test]
fn outside_a_repository_a_file_with_markers_opens_from_them() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("m.txt"), DIFF3).expect("written");
    let loaded = load(dir.path(), Path::new("m.txt")).expect("the markers load");
    assert_eq!(texts(&loaded), (vec!["a\nL\nz\n", "a\nR\nz\n"], vec!["a\nB\nz\n"]));
    assert_eq!(loaded.repo, None);
    assert_eq!(loaded.path, Utf8PathBuf::from("m.txt"));
    let dir = Utf8Path::from_path(dir.path()).expect("UTF-8");
    assert_eq!(loaded.file, canonical(dir).join("m.txt"));
}

/// git's two-way markers for `a.txt`'s conflict: they hold no base.
const MERGE_STYLE: &[u8] = b"<<<<<<< ours\nleft\n=======\nright\n>>>>>>> theirs\n";

#[test]
fn from_outside_any_repository_a_path_into_one_loads_from_it() {
    let outer = tempfile::tempdir().expect("a temporary directory");
    let outer_root = Utf8Path::from_path(outer.path()).expect("UTF-8");
    let fixture = vcs_git::testing::Fixture::new_in(outer_root);
    let one = fixture.commit("one", &[("a.txt", BASE)]);
    fixture.commit("left", &[("a.txt", LEFT)]);
    let right = fixture.commit_on(&[one], "right", &[("a.txt", RIGHT)]);
    fixture.stage_conflicts(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    fixture.write("a.txt", MERGE_STYLE);
    let into =
        fixture.root().strip_prefix(outer_root).expect("the repository is inside").join("a.txt");
    let loaded = load(outer.path(), into.as_std_path()).expect("the conflict loads");
    assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
    assert_eq!(loaded.repo, Some(RepoKind::Git));
    assert_eq!(loaded.path, Utf8PathBuf::from("a.txt"));
    assert_eq!(loaded.file, canonical(fixture.root()).join("a.txt"));
}

#[test]
fn malformed_markers_name_the_file_and_line() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("m.txt"), "<<<<<<< x\nL\n=======\n").expect("written");
    let error = load(dir.path(), Path::new("m.txt")).expect_err("unclosed");
    assert!(
        matches!(&error, ResolveError::Markers { path, source: merge_core::markers::MarkerError::Unclosed { line: 1 } } if path == "m.txt"),
        "{error:?}"
    );
    assert_eq!(error.to_string(), "m.txt: line 1: the conflict is not closed");
}

#[test]
fn a_missing_clean_file_cannot_be_read() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[("a.txt", BASE)]);
    let error = load(fixture.root().as_std_path(), Path::new("gone.txt")).expect_err("missing");
    assert!(
        matches!(&error, ResolveError::Read { path, source } if path == "gone.txt" && source.kind() == std::io::ErrorKind::NotFound),
        "{error:?}"
    );
}

#[test]
fn a_path_outside_the_repository_is_refused() {
    let outer = tempfile::tempdir().expect("a temporary directory");
    let outer_root = Utf8Path::from_path(outer.path()).expect("UTF-8");
    let fixture = vcs_git::testing::Fixture::new_in(outer_root);
    fixture.commit("one", &[("a.txt", BASE)]);
    std::fs::write(outer.path().join("x.txt"), "x\n").expect("written");
    let error = load(fixture.root().as_std_path(), Path::new("../x.txt"))
        .expect_err("outside the repository");
    let expected = canonical(outer_root).join("x.txt");
    assert!(
        matches!(&error, ResolveError::Vcs(VcsError::OutsideRepository(path)) if *path == expected),
        "{error:?}"
    );
}

#[test]
fn a_clean_path_is_not_conflicted() {
    let (fixture, _) = git_stopped(&[("clean.txt", b"c\n")], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("clean.txt", b"c\n");
    let error = load(fixture.root().as_std_path(), Path::new("clean.txt")).expect_err("clean");
    assert!(
        matches!(&error, ResolveError::NotConflicted(path) if path == "clean.txt"),
        "{error:?}"
    );
    assert_eq!(error.to_string(), "clean.txt is not conflicted");
}

#[test]
fn a_binary_term_is_refused_by_name() {
    let (fixture, [_, right]) =
        git_stopped(&[], &[("a.bin", files([Some(BASE), Some(LEFT), Some(b"\0\x01binary")]))]);
    let error = load(fixture.root().as_std_path(), Path::new("a.bin")).expect_err("binary");
    match &error {
        ResolveError::NotText { path, term, .. } => {
            assert_eq!(path, "a.bin");
            assert_eq!(term, &format!("side 2 ({right} right)"));
        }
        other => panic!("expected NotText, got {other:?}"),
    }
    assert!(
        error.to_string().starts_with(&format!("a.bin: side 2 ({right} right) is not text: ")),
        "{error}"
    );
}

#[test]
fn a_symlink_term_is_refused_by_name_and_kind() {
    let (fixture, _) = git_stopped(
        &[],
        &[(
            "link",
            [Some(Entry::File(BASE)), Some(Entry::Symlink("target")), Some(Entry::File(RIGHT))],
        )],
    );
    let error = load(fixture.root().as_std_path(), Path::new("link")).expect_err("a symlink");
    assert!(
        matches!(&error, ResolveError::NotAFile { path, term, kind: EntryKind::Symlink }
            if path == "link" && term == "side 1 (main)"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "link: side 1 (main) is a symlink; only text conflicts can be resolved"
    );
}

#[test]
fn a_conflict_deleted_on_every_side_is_refused() {
    // git's ort strategy leaves the base alone at a name both sides renamed away from.
    let (fixture, _) = git_stopped(&[], &[("a.txt", files([Some(BASE), None, None]))]);
    let error = load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("no sides");
    assert!(
        matches!(&error, ResolveError::DeletedEverywhere(path) if path == "a.txt"),
        "{error:?}"
    );
    assert_eq!(error.to_string(), "a.txt is deleted on every side; nothing to merge");
    assert!(error.is_refusal());
}

#[test]
fn a_conflict_with_one_side_present_loads() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files([None, Some(LEFT), None]))]);
    let loaded =
        load(fixture.root().as_std_path(), Path::new("a.txt")).expect("the conflict loads");
    assert_eq!(texts(&loaded), (vec!["left\n", ""], vec![""]));
}

#[test]
fn only_errors_about_the_file_itself_are_refusals() {
    let path = || Utf8PathBuf::from("a.txt");
    assert!(ResolveError::ResolvedInWorkingTree(path()).is_refusal());
    assert!(
        ResolveError::NotAFile {
            path: path(),
            term: "base 1".to_owned(),
            kind: EntryKind::Symlink
        }
        .is_refusal()
    );
    assert!(!ResolveError::NotConflicted(path()).is_refusal());
    assert!(!ResolveError::Vcs(VcsError::NotARepository(path())).is_refusal());
}

#[test]
fn a_term_without_a_title_is_named_by_position() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[("a.txt", BASE)]);
    fixture.stage_conflicts(
        &[],
        &[("d", [Some(Entry::Submodule), Some(Entry::File(LEFT)), Some(Entry::File(RIGHT))])],
    );
    let error = load(fixture.root().as_std_path(), Path::new("d")).expect_err("a submodule");
    assert_eq!(error.to_string(), "d: base 1 is a submodule; only text conflicts can be resolved");
}

/// A jj conflict in `a.txt` between `left` and `right` over `a\n`, checked out to disk.
fn jj_on_disk(left: &[u8], right: &[u8]) -> vcs_jj::testing::Fixture {
    let fixture = vcs_jj::testing::Fixture::new();
    fixture.three_way("a.txt", [BASE, left, right]);
    fixture.check_out_to_disk();
    fixture
}

/// The file git writes for `BOTH_CHANGED`, in its diff3 style.
const GIT_MARKERS: &[u8] = b"<<<<<<< ours\nleft\n||||||| base\na\n=======\nright\n>>>>>>> theirs\n";

#[test]
fn a_git_file_as_git_wrote_it_loads_from_the_stages() {
    // The labels and the marker style differ from triad's; the conflict is the same.
    let (fixture, [one, right]) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", GIT_MARKERS);
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(loaded.marker_length, None);
    assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
    assert_eq!(
        loaded.labels,
        labels(&["main", &format!("{right} right")], &[&format!("{one} one")])
    );
}

#[test]
fn git_leaves_the_marker_length_to_the_tool_though_a_side_holds_a_marker_like_line() {
    let marker_like: [Option<&[u8]>; 3] = [Some(BASE), Some(b"<<<<<<< left\n"), Some(RIGHT)];
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(marker_like))]);
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(loaded.repo, Some(RepoKind::Git));
    assert_eq!(loaded.marker_length, None);
}

#[test]
fn a_git_file_edited_beside_its_markers_opens_from_them() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", &[b"edited\n", GIT_MARKERS].concat());
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(texts(&loaded), (vec!["edited\nleft\n", "edited\nright\n"], vec!["edited\na\n"]));
    // jj's default `base` counts as no label (`DESIGN.md` §D5.7).
    assert_eq!(loaded.labels, labels(&["ours", "theirs"], &[""]));
    assert_eq!(loaded.repo, Some(RepoKind::Git));
    assert_eq!(loaded.path, Utf8PathBuf::from("a.txt"));
    assert_eq!(loaded.file, canonical(fixture.root()).join("a.txt"));
    assert_eq!(loaded.marker_length, None);
}

/// Two conflicts, at lines 2 and 4.
const TWO_CONFLICTS: [Option<&[u8]>; 3] =
    [Some(b"a\nb\nc\nd\ne\n"), Some(b"a\nL1\nc\nL2\ne\n"), Some(b"a\nR1\nc\nR2\ne\n")];

#[test]
fn a_git_file_with_one_conflict_resolved_by_hand_opens_from_its_markers() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(TWO_CONFLICTS))]);
    fixture.write(
        "a.txt",
        b"a\nL1\nc\n<<<<<<< ours\nL2\n||||||| base\nd\n=======\nR2\n>>>>>>> theirs\ne\n",
    );
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(
        texts(&loaded),
        (vec!["a\nL1\nc\nL2\ne\n", "a\nL1\nc\nR2\ne\n"], vec!["a\nL1\nc\nd\ne\n"])
    );
}

#[test]
fn a_git_file_with_both_conflicts_as_git_wrote_them_loads_from_the_stages() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(TWO_CONFLICTS))]);
    fixture.write(
        "a.txt",
        b"a\n<<<<<<< HEAD\nL1\n=======\nR1\n>>>>>>> right\nc\n\
          <<<<<<< HEAD\nL2\n=======\nR2\n>>>>>>> right\ne\n",
    );
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(
        texts(&loaded),
        (vec!["a\nL1\nc\nL2\ne\n", "a\nR1\nc\nR2\ne\n"], vec!["a\nb\nc\nd\ne\n"])
    );
}

#[test]
fn a_git_file_in_zdiff3_style_loads_from_the_stages() {
    // zdiff3 moves the lines both sides share out of the conflict.
    let shared: [Option<&[u8]>; 3] = [Some(b"a\n"), Some(b"s\nL\ne\n"), Some(b"s\nR\ne\n")];
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(shared))]);
    fixture.write("a.txt", b"s\n<<<<<<< HEAD\nL\n||||||| base\na\n=======\nR\n>>>>>>> right\ne\n");
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(texts(&loaded), (vec!["s\nL\ne\n", "s\nR\ne\n"], vec!["a\n"]));
}

#[test]
fn a_git_file_with_broken_markers_is_refused_and_names_the_line() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", b"<<<<<<< ours\nleft\n=======\nright\n");
    let error = load(fixture.root().as_std_path(), Path::new("a.txt"))
        .expect_err("the conflict is not closed");
    assert_eq!(error.to_string(), "a.txt: line 1: the conflict is not closed");
    assert!(error.is_refusal());
}

#[test]
fn a_git_conflict_resolved_by_hand_in_the_working_tree_is_refused() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", b"resolved by hand\n");
    let error = load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("no markers");
    assert!(
        matches!(&error, ResolveError::ResolvedInWorkingTree(path) if path == "a.txt"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "a.txt has no conflict markers left in the working tree, so it looks resolved; \
         mark it resolved, or put its markers back to merge it again"
    );
    assert!(error.is_refusal());
}

#[test]
fn a_git_conflict_with_markers_longer_than_seven_loads() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", b"<<<<<<<<<< ours\nleft\n==========\nright\n>>>>>>>>>> theirs\n");
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
}

#[test]
fn a_git_conflict_emptied_in_the_working_tree_is_refused() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", b"");
    let error = load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("no markers");
    assert!(
        matches!(&error, ResolveError::ResolvedInWorkingTree(path) if path == "a.txt"),
        "{error:?}"
    );
}

#[test]
fn a_git_conflict_missing_from_the_working_tree_loads_from_its_stages() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    assert!(!fixture.root().join("a.txt").exists());
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
}

#[test]
fn a_modify_delete_conflict_loads_although_git_writes_no_markers() {
    // git leaves the modified side in the working tree, without markers.
    let (fixture, _) = git_stopped(&[], &[("a.txt", files([Some(BASE), Some(LEFT), None]))]);
    fixture.write("a.txt", LEFT);
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    assert_eq!(texts(&loaded), (vec!["left\n", ""], vec!["a\n"]));
}

/// The bytes of `path` in `fixture`'s working tree.
fn on_disk(fixture: &vcs_git::testing::Fixture, path: &str) -> Vec<u8> {
    std::fs::read(fixture.root().join(path)).expect("the file is readable")
}

#[test]
fn a_binary_working_tree_file_is_refused_since_a_save_would_overwrite_it() {
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", b"\0binary\n");
    let error = load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("binary");
    assert!(
        matches!(&error, ResolveError::WorkingTreeNotText { path, source: DecodeError::Binary }
            if path == "a.txt"),
        "{error:?}"
    );
    assert!(error.is_refusal(), "a run over every file skips it");
    assert_eq!(on_disk(&fixture, "a.txt"), b"\0binary\n");
}

#[test]
fn a_hand_edit_that_is_not_utf8_is_refused_at_its_first_bad_byte() {
    // An editor saved the hand resolution in Latin-1: `caf\xe9` is `café`.
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", b"caf\xe9\n");
    let error = load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("Latin-1");
    assert!(
        matches!(&error, ResolveError::WorkingTreeNotText {
            path,
            source: DecodeError::InvalidUtf8 { offset: 3 },
        } if path == "a.txt"),
        "{error:?}"
    );
    assert_eq!(on_disk(&fixture, "a.txt"), b"caf\xe9\n");
}

#[cfg(unix)]
#[test]
fn an_unreadable_working_tree_file_fails_with_its_read_error() {
    use std::os::unix::fs::PermissionsExt as _;
    const NO_ACCESS: u32 = 0o000;
    const READ_WRITE: u32 = 0o644;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    fixture.write("a.txt", b"<<<<<<< ours\nleft\n=======\nright\n>>>>>>> theirs\n");
    let file = fixture.root().join("a.txt");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(NO_ACCESS)).expect("chmod");
    let result = load(fixture.root().as_std_path(), Path::new("a.txt"));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(READ_WRITE)).expect("chmod");
    let error = result.expect_err("unreadable");
    assert!(
        matches!(&error, ResolveError::Read { path, source }
            if path == "a.txt" && source.kind() == std::io::ErrorKind::PermissionDenied),
        "{error:?}"
    );
    assert!(!error.is_refusal(), "a run over every file stops");
}

/// Tests that build a jj repository. `script/pre-commit` leaves them out (`.config/nextest.toml`).
/// The stage numbers of `path`'s index entries in the fixture's repository.
fn git_stages(fixture: &vcs_git::testing::Fixture, path: &str) -> Vec<gix::index::entry::Stage> {
    let index = gix::open(fixture.root())
        .expect("the repository opens")
        .open_index()
        .expect("the index is read");
    index
        .entries()
        .iter()
        .filter(|entry| entry.path(&index) == path)
        .map(gix::index::Entry::stage)
        .collect()
}

#[test]
fn a_resolved_git_save_is_staged_as_git_add_stages_it() {
    use gix::index::entry::Stage;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    let record = loaded.record.expect("git holds the conflict");
    fixture.write("a.txt", b"merged\n");
    record.saved(ExitCode::Resolved).expect("the save is recorded");
    assert_eq!(git_stages(&fixture, "a.txt"), [Stage::Unconflicted]);
    assert_eq!(
        conflicted(fixture.root().as_std_path()).expect("listed").paths,
        Vec::<Utf8PathBuf>::new()
    );
}

#[test]
fn a_git_save_with_markers_or_without_a_write_leaves_the_stages() {
    use gix::index::entry::Stage;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    let record = loaded.record.expect("git holds the conflict");
    fixture.write("a.txt", GIT_MARKERS);
    for code in [ExitCode::Conflicts, ExitCode::Cancelled, ExitCode::Error] {
        record.saved(code).expect("nothing to record");
        assert_eq!(
            git_stages(&fixture, "a.txt"),
            [Stage::Base, Stage::Ours, Stage::Theirs],
            "{code:?}"
        );
    }
}

/// The conflict at `a.txt` in `fixture`, opened and resolved to left.
fn resolved_to_left(fixture: &vcs_git::testing::Fixture) -> merge_session::resolve::Opened {
    use merge_core::document::{Command, MergeOptions, PickScope};
    use merge_core::merge3::Source;
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    let mut opened = loaded.open(MergeOptions::default()).expect("it aligns");
    opened
        .doc
        .apply(Command::PickAll { scope: PickScope::Unresolved, sources: vec![Source::Side(0)] })
        .expect("left is an input");
    opened
}

#[test]
fn a_resolved_save_writes_the_file_then_stages_it() {
    use gix::index::entry::Stage;
    use merge_core::document::OutputEnding;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let opened = resolved_to_left(&fixture);
    let code = EncodedOutput::new(&opened.doc, OutputEnding::Keep)
        .save(opened.file.as_std_path(), opened.record.as_ref(), None)
        .expect("the save is written and recorded");
    assert_eq!(code, ExitCode::Resolved);
    assert_eq!(std::fs::read(&opened.file).expect("written"), LEFT);
    assert_eq!(git_stages(&fixture, "a.txt"), [Stage::Unconflicted]);
}

#[test]
fn a_save_the_index_refuses_is_written_and_says_it_is_not_recorded() {
    use gix::index::entry::Stage;
    use merge_core::document::OutputEnding;
    use merge_session::SaveError;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let opened = resolved_to_left(&fixture);
    // Another git command holds the index.
    fixture.write(".git/index.lock", b"");
    let error = EncodedOutput::new(&opened.doc, OutputEnding::Keep)
        .save(opened.file.as_std_path(), opened.record.as_ref(), None)
        .expect_err("the index is locked");
    match &error {
        SaveError::Record { path, code: ExitCode::Resolved, source: VcsError::Backend { .. } } => {
            assert_eq!(path, "a.txt");
        }
        other => panic!("expected a record error, got {other:?}"),
    }
    assert!(
        error
            .to_string()
            .starts_with("a.txt is written, but the repository did not record the save: ")
    );
    assert_eq!(std::fs::read(&opened.file).expect("written"), LEFT);
    assert_eq!(git_stages(&fixture, "a.txt"), [Stage::Base, Stage::Ours, Stage::Theirs]);
    // Once the lock goes, recording again stages the file written before.
    std::fs::remove_file(fixture.root().join(".git/index.lock")).expect("the lock is removed");
    let code = opened.record.expect("git holds the conflict").resave(ExitCode::Resolved);
    assert_eq!(code.expect("recorded"), ExitCode::Resolved);
    assert_eq!(git_stages(&fixture, "a.txt"), [Stage::Unconflicted]);
}

#[test]
fn a_save_that_cannot_be_written_records_nothing() {
    use gix::index::entry::Stage;
    use merge_core::document::OutputEnding;
    use merge_session::SaveError;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let opened = resolved_to_left(&fixture);
    let output = fixture.root().join("missing").join("a.txt");
    let error = EncodedOutput::new(&opened.doc, OutputEnding::Keep)
        .save(output.as_std_path(), opened.record.as_ref(), None)
        .expect_err("the directory is missing");
    assert!(
        matches!(&error, SaveError::Write(write) if write.path == output.as_std_path()),
        "{error:?}"
    );
    assert_eq!(git_stages(&fixture, "a.txt"), [Stage::Base, Stage::Ours, Stage::Theirs]);
}

#[test]
fn a_save_recorded_in_another_repository_is_refused_before_any_write() {
    use gix::index::entry::Stage;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
    let record = loaded.record.expect("git holds the conflict");
    fixture.write("a.txt", b"merged\n");
    let (other, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let mut run = RunRepository::find([other.root().as_std_path()]);
    let error = record.saved_in(&mut run, ExitCode::Resolved).expect_err("the run holds another");
    let expected = canonical(fixture.root()).join("a.txt");
    assert!(matches!(&error, VcsError::OutsideRepository(path) if *path == expected), "{error:?}");
    for repo in [&fixture, &other] {
        assert_eq!(git_stages(repo, "a.txt"), [Stage::Base, Stage::Ours, Stage::Theirs]);
    }
}

#[test]
fn a_save_through_the_runs_repository_stages_it_there_and_refuses_another() {
    use gix::index::entry::Stage;
    use merge_core::document::OutputEnding;
    use merge_session::SaveError;
    let (fixture, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let opened = resolved_to_left(&fixture);
    let encoded = EncodedOutput::new(&opened.doc, OutputEnding::Keep);
    let (other, _) = git_stopped(&[], &[("a.txt", files(BOTH_CHANGED))]);
    let mut elsewhere = RunRepository::find([other.root().as_std_path()]);
    let error = encoded
        .save(opened.file.as_std_path(), opened.record.as_ref(), Some(&mut elsewhere))
        .expect_err("the run holds another repository");
    assert!(
        matches!(&error, SaveError::Record { path, code: ExitCode::Resolved, source: VcsError::OutsideRepository(_) } if path == "a.txt"),
        "{error:?}"
    );
    assert_eq!(std::fs::read(&opened.file).expect("written"), LEFT);
    assert_eq!(git_stages(&fixture, "a.txt"), [Stage::Base, Stage::Ours, Stage::Theirs]);
    let mut run = RunRepository::find([fixture.root().as_std_path()]);
    let code = encoded
        .save(opened.file.as_std_path(), opened.record.as_ref(), Some(&mut run))
        .expect("written and recorded");
    assert_eq!(code, ExitCode::Resolved);
    assert_eq!(git_stages(&fixture, "a.txt"), [Stage::Unconflicted]);
    assert_eq!(git_stages(&other, "a.txt"), [Stage::Base, Stage::Ours, Stage::Theirs]);
}

#[test]
fn a_file_opened_from_its_markers_alone_has_no_record() {
    let (fixture, _) = git_stopped(&[("clean.txt", b"c\n")], &[]);
    fixture.write("clean.txt", b"<<<<<<< l\nleft\n=======\nright\n>>>>>>> r\n");
    let loaded =
        load(fixture.root().as_std_path(), Path::new("clean.txt")).expect("the markers load");
    assert!(loaded.record.is_none(), "{:?}", loaded.record);
}

mod jj {
    use super::*;
    use vcs_jj::testing::{Held, Op};

    const SNAPSHOT: &str = "snapshot working copy";

    #[test]
    fn loading_snapshots_unrecorded_edits_in_one_operation_and_a_save_in_another() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        fixture.write("b.txt", b"new\n");
        let before = fixture.operations().len();
        let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        let ops = fixture.operations();
        assert_eq!(ops.len(), before + 1);
        assert_eq!(ops[0], Op { description: SNAPSHOT.to_owned(), is_snapshot: true });
        assert_eq!(fixture.held_at(0, "b.txt"), Held::File(b"new\n".to_vec()));
        fixture.write("a.txt", b"merged\n");
        let record = loaded.record.expect("jj holds the conflict");
        record.saved(ExitCode::Resolved).expect("the save is recorded");
        let ops = fixture.operations();
        assert_eq!(ops.len(), before + 2);
        assert_eq!(
            ops[0],
            Op { description: "save a.txt in triad".to_owned(), is_snapshot: false }
        );
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(b"merged\n".to_vec()));
        assert_eq!(fixture.held_at(1, "a.txt"), Held::Conflict);
    }

    #[test]
    fn loading_an_unchanged_working_copy_writes_no_operation() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        let before = fixture.operations();
        load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        assert_eq!(fixture.operations(), before);
    }

    /// The run's repository at `fixture`'s root, with jj's default config.
    fn run_at(fixture: &vcs_jj::testing::Fixture) -> RunRepository {
        RunRepository::find([fixture.root().as_std_path()]).with_jj_config(ConfigEnv::default())
    }

    #[test]
    fn a_load_after_the_list_reads_at_the_lists_snapshot_without_a_scan() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        let root = fixture.root().as_std_path();
        let mut run = run_at(&fixture);
        resolve::conflicted(&mut run).expect("listed");
        // An edit after the list: a scan would take it in, in an operation of its own.
        fixture.write("b.txt", b"new\n");
        let before = fixture.operations();
        let loaded =
            resolve::load(&mut run, root, Path::new("a.txt"), Snapshot::Taken).expect("it loads");
        assert_eq!(fixture.operations(), before, "no scan");
        assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
        // Taking the snapshot scans and takes the edit in.
        resolve::load(&mut run, root, Path::new("a.txt"), Snapshot::Take).expect("it loads");
        assert_eq!(fixture.operations().len(), before.len() + 1);
        assert_eq!(fixture.held_at(0, "b.txt"), Held::File(b"new\n".to_vec()));
    }

    #[test]
    fn a_save_recorded_in_the_run_leaves_its_repository_current() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        let root = fixture.root().as_std_path();
        let mut run = run_at(&fixture);
        resolve::conflicted(&mut run).expect("listed");
        let loaded =
            resolve::load(&mut run, root, Path::new("a.txt"), Snapshot::Taken).expect("it loads");
        fixture.write("a.txt", b"merged\n");
        let record = loaded.record.expect("jj holds the conflict");
        record.saved_in(&mut run, ExitCode::Resolved).expect("the save is recorded");
        assert_eq!(
            fixture.operations()[0],
            Op { description: "save a.txt in triad".to_owned(), is_snapshot: false }
        );
        // The run reads at the save's operation without another snapshot.
        let open = run.require().expect("open");
        assert_eq!(open.conflicts().expect("listed"), Vec::<Utf8PathBuf>::new());
    }

    #[test]
    fn a_cancelled_jj_run_records_nothing() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        let before = fixture.operations();
        fixture.write("a.txt", b"merged\n");
        let record = loaded.record.expect("jj holds the conflict");
        record.saved(ExitCode::Cancelled).expect("nothing to record");
        assert_eq!(fixture.operations(), before);
    }

    #[test]
    fn a_jj_conflict_loads_its_terms_labelled_by_jj() {
        for (fixture, kind) in [
            (vcs_jj::testing::Fixture::colocated(), "jj colocated"),
            (vcs_jj::testing::Fixture::internal(), "jj internal"),
        ] {
            let [base, left, right] = jj_conflict(&fixture);
            let loaded = load(fixture.root().as_std_path(), Path::new("a.txt"))
                .unwrap_or_else(|e| panic!("{kind}: {e}"));
            assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]), "{kind}");
            assert_eq!(loaded.labels, labels(&[&left, &right], &[&base]), "{kind}");
            assert_eq!(loaded.repo, Some(RepoKind::Jj), "{kind}");
        }
    }

    #[test]
    fn a_three_sided_jj_conflict_loads_three_sides_and_two_bases_in_order() {
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
        let sides: [&[u8]; 3] = [b"one\n", b"two\n", b"three\n"];
        let commits = sides.map(|bytes| fixture.commit_on(&[&base], "side", &[("a.txt", bytes)]));
        fixture.check_out(&commits.each_ref());
        fixture.check_out_to_disk();
        let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        assert_eq!(texts(&loaded), (vec!["one\n", "two\n", "three\n"], vec!["a\n", "a\n"]));
        let side = |c| jj_label(c, "side");
        assert_eq!(
            loaded.labels,
            labels(
                &[&side(&commits[0]), &side(&commits[1]), &side(&commits[2])],
                &[&jj_label(&base, "base"), &jj_label(&base, "base")]
            )
        );
    }

    #[test]
    fn a_jj_file_with_jj_markers_opens_from_them() {
        let fixture = vcs_jj::testing::Fixture::new();
        fixture.commit("one", &[("a.txt", BASE)]);
        fixture.check_out_to_disk();
        fixture.write(
            "a.txt",
            b"<<<<<<< conflict 1 of 1\n%%%%%%% diff from: b\n\\\\\\\\\\\\\\        to: l\n-a\n+left\n\
              +++++++ r\nright\n>>>>>>> conflict 1 of 1 ends\n",
        );
        let loaded =
            load(fixture.root().as_std_path(), Path::new("a.txt")).expect("the markers load");
        assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
        assert_eq!(loaded.labels, labels(&["l", "r"], &["b"]));
        assert_eq!(loaded.repo, Some(RepoKind::Jj));
    }

    #[test]
    fn from_outside_any_repository_jjs_checks_still_hold() {
        let outer = tempfile::tempdir().expect("a temporary directory");
        let outer_root = Utf8Path::from_path(outer.path()).expect("UTF-8");
        let fixture = vcs_jj::testing::Fixture::new_in(outer_root);
        fixture.three_way("a.txt", [BASE, b"<<<<<<< left\n", RIGHT]);
        fixture.check_out_to_disk();
        let into = fixture
            .root()
            .strip_prefix(outer_root)
            .expect("the repository is inside")
            .join("a.txt");
        let loaded = load(outer.path(), into.as_std_path()).expect("the conflict loads");
        assert_eq!(loaded.repo, Some(RepoKind::Jj));
        // A marker-like run of 7 makes jj's markers 7 + 4.
        assert_eq!(loaded.marker_length, Some(11));
        // jj's snapshot takes the resolution in, so the path is clean.
        fixture.write("a.txt", b"resolved by hand, not yet seen by jj\n");
        let error = load(outer.path(), into.as_std_path()).expect_err("resolved by hand");
        assert!(
            matches!(&error, ResolveError::NotConflicted(path) if path == "a.txt"),
            "{error:?}"
        );
    }

    #[test]
    fn a_jj_file_as_jj_wrote_it_loads_with_jjs_marker_length() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        assert_eq!(texts(&loaded), (vec!["left\n", "right\n"], vec!["a\n"]));
        assert_eq!(loaded.marker_length, Some(7));
        let longer = jj_on_disk(b"<<<<<<< left\n", RIGHT);
        let loaded = load(longer.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        // A marker-like run of 7 makes jj's markers 7 + 4.
        assert_eq!(loaded.marker_length, Some(11));
    }

    #[test]
    fn a_jj_file_resolved_by_hand_since_jjs_snapshot_is_clean() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        fixture.write("a.txt", b"resolved by hand, not yet seen by jj\n");
        let error =
            load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("resolved by hand");
        assert!(
            matches!(&error, ResolveError::NotConflicted(path) if path == "a.txt"),
            "{error:?}"
        );
        assert_eq!(error.to_string(), "a.txt is not conflicted");
    }

    #[test]
    fn a_jj_file_checked_out_from_another_conflict_is_refused() {
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
        let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT)]);
        let right = fixture.commit_on(&[&base], "right", &[("a.txt", RIGHT)]);
        let other = fixture.commit_on(&[&base], "other", &[("a.txt", b"other\n")]);
        fixture.check_out(&[&left, &right]);
        fixture.check_out_to_disk();
        fixture.check_out(&[&left, &other]);
        let operations = fixture.operations();
        let error =
            load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("a stale file");
        // The snapshot refuses the stale working copy before writing anything.
        let root = canonical(fixture.root());
        assert!(
            matches!(&error, ResolveError::Vcs(VcsError::StaleWorkspace(at)) if *at == root),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            format!(
                "the working copy at {root} is stale; run jj workspace update-stale and try again"
            )
        );
        assert_eq!(fixture.operations(), operations);
    }

    #[test]
    fn after_git_checks_out_one_side_jj_imports_it_and_the_file_is_clean() {
        let fixture = vcs_jj::testing::Fixture::colocated();
        let [_, _, right] = fixture.three_way("a.txt", [BASE, LEFT, RIGHT]);
        fixture.check_out_to_disk();
        // jj points git's HEAD at the merge's first parent, left; `git checkout --detach
        // right` moves it.
        fixture.write(".git/HEAD", format!("{}\n", right.commit).as_bytes());
        fixture.write("a.txt", RIGHT);
        let error =
            load(fixture.root().as_std_path(), Path::new("a.txt")).expect_err("no conflict");
        assert!(
            matches!(&error, ResolveError::NotConflicted(path) if path == "a.txt"),
            "{error:?}"
        );
        assert_eq!(fixture.parent_ids(0), [right.commit]);
    }

    #[test]
    fn a_marker_like_run_of_eight_in_a_term_gives_jjs_length_of_twelve() {
        let fixture = vcs_jj::testing::Fixture::new();
        fixture.three_way("a.txt", [BASE, b"|||||||| bar\n", RIGHT]);
        fixture.check_out_to_disk();
        let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        // The longest marker-like run is 8, so jj chooses 8 + 4.
        assert_eq!(loaded.marker_length, Some(12));
    }

    /// `merge` of `left` and `right` conflicts in `a.txt`; `child` adds `b.txt`; the working
    /// copy sits on `child`, on disk.
    fn stacked() -> (vcs_jj::testing::Fixture, vcs_jj::testing::Committed) {
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
        let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT)]);
        let right = fixture.commit_on(&[&base], "right", &[("a.txt", RIGHT)]);
        let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
        let child = fixture.commit_merged(&[&merge], "child", &[("b.txt", b"b\n")]);
        fixture.check_out(&[&child]);
        fixture.check_out_to_disk();
        (fixture, merge)
    }

    /// Each descendant's conflicted paths before and after, parents first.
    fn effects(rewrite: &vcs_jj::Rewrite) -> Vec<(Vec<&str>, Vec<&str>)> {
        rewrite.descendants.iter().map(|d| (strs(&d.before), strs(&d.after))).collect()
    }

    fn strs(paths: &[Utf8PathBuf]) -> Vec<&str> {
        paths.iter().map(|p| p.as_str()).collect()
    }

    fn at_commit(fixture: &vcs_jj::testing::Fixture, rev: &str, pair: Option<usize>) -> Loaded {
        resolve::load_at_commit(
            fixture.root().as_std_path(),
            rev,
            Path::new("a.txt"),
            pair,
            Some(ConfigEnv::default()),
        )
        .expect("it loads")
    }

    #[test]
    fn a_preview_reports_a_resolution_and_writes_nothing() {
        let (fixture, merge) = stacked();
        let record = at_commit(&fixture, &merge.commit, None).record.expect("a commit's record");
        let operations = fixture.operation_count();
        let rewrite = record.preview(RIGHT, true).expect("previewed").expect("a commit's rewrite");
        assert!(rewrite.resolved);
        assert!(!rewrite.written);
        // The child and the working copy on it inherit the conflict, and lose it with it.
        assert_eq!(effects(&rewrite), [(vec!["a.txt"], vec![]), (vec!["a.txt"], vec![])]);
        assert_eq!(fixture.held_in(&merge, "a.txt"), vcs_jj::testing::Held::Conflict);
        assert_eq!(fixture.operation_count(), operations);
    }

    /// The automatic merge of `a.txt` in `merge`, with its markers, and the record that
    /// writes it there.
    fn markers_in(
        fixture: &vcs_jj::testing::Fixture,
        merge: &vcs_jj::testing::Committed,
    ) -> (Vec<u8>, resolve::Record) {
        let opened = at_commit(fixture, &merge.commit, None)
            .open(merge_core::document::MergeOptions::default())
            .expect("labels fit");
        let markers = EncodedOutput::new(&opened.doc, merge_core::document::OutputEnding::Keep);
        assert_eq!(markers.code(), ExitCode::Conflicts);
        (markers.bytes().to_vec(), opened.record.expect("a commit's record"))
    }

    #[test]
    fn a_preview_of_changed_markers_keeps_the_descendants_conflicted() {
        let (fixture, merge) = stacked();
        let (markers, record) = markers_in(&fixture, &merge);
        let text = String::from_utf8(markers).expect("UTF-8");
        let edited = text.replacen("\nleft\n", "\nleft edited\n", 1);
        assert_ne!(edited, text, "the left side is in the markers");
        let rewrite =
            record.preview(edited.as_bytes(), false).expect("previewed").expect("a rewrite");
        assert!(!rewrite.resolved);
        assert_eq!(
            effects(&rewrite),
            [(vec!["a.txt"], vec!["a.txt"]), (vec!["a.txt"], vec!["a.txt"])]
        );
    }

    #[test]
    fn a_preview_of_the_held_conflicts_markers_rebases_nothing() {
        let (fixture, merge) = stacked();
        let (markers, record) = markers_in(&fixture, &merge);
        let rewrite = record.preview(&markers, false).expect("previewed").expect("a rewrite");
        // The markers read back as the conflict the commit holds: the write changes nothing
        // (spec §7.5.1).
        assert!(!rewrite.resolved);
        assert_eq!(effects(&rewrite), []);
    }

    #[test]
    fn a_preview_reads_the_content_given_not_the_saved_file() {
        let (fixture, merge) = stacked();
        let opened = at_commit(&fixture, &merge.commit, None)
            .open(merge_core::document::MergeOptions::default())
            .expect("labels fit");
        // The saved file holds the conflict as the commit does: a write of it changes nothing.
        let markers = EncodedOutput::new(&opened.doc, merge_core::document::OutputEnding::Keep);
        std::fs::write(&opened.file, markers.bytes()).expect("written");
        let record = opened.record.expect("a commit's record");
        let rewrite = record.preview(LEFT, true).expect("previewed").expect("a rewrite");
        assert!(rewrite.resolved);
        assert_eq!(effects(&rewrite), [(vec!["a.txt"], vec![]), (vec!["a.txt"], vec![])]);
    }

    #[test]
    fn the_working_copys_conflict_has_no_preview() {
        let fixture = jj_on_disk(LEFT, RIGHT);
        let loaded = load(fixture.root().as_std_path(), Path::new("a.txt")).expect("it loads");
        let record = loaded.record.expect("jj holds the conflict");
        assert_eq!(record.preview(RIGHT, true).expect("nothing to preview"), None);
    }
}
