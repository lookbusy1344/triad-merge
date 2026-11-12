//! Which revision each input's line origins come from (`docs/DESIGN.md` §C3, §C5.2),
//! one test per row of the plan's table, on repositories built through gix and jj-lib.

use std::path::Path;
use vcs::CommitId;

use camino::Utf8PathBuf;
use merge_core::merge3::Inputs;
use merge_core::text::Text;
use merge_session::origin::{self, Fit, Found, Source};
use merge_session::repository::RunRepository;
use vcs::{Annotation, LineOrigin, VcsError};

const PATH: &str = "a.txt";
const BASE: &[u8] = b"a\nb\nc\n";
const LEFT: &[u8] = b"A\nb\nc\n";
const RIGHT: &[u8] = b"a\nb\nC\n";
/// The file git leaves in the working tree for `git mergetool`; its content plays no part.
const MARKERS: &[u8] = b"<<<<<<< ours\nA\n=======\na\n>>>>>>> theirs\nb\nC\n";
/// "A\n" in UTF-16LE with its byte order mark.
const UTF16: &[u8] = &[0xFF, 0xFE, b'A', 0, b'\n', 0];

/// [`Found`] for `N` inputs, its lists as arrays.
struct Looked<const N: usize> {
    found: Found,
    sources: [Option<Source>; N],
    revisions: [Option<CommitId>; N],
}

impl<const N: usize> Looked<N> {
    fn new(found: Found) -> Self {
        let sources = array(found.sources.clone());
        let revisions = array(found.revisions.clone());
        Self { found, sources, revisions }
    }

    fn annotate(&self, inputs: [&Text; N]) -> [Result<Option<Annotation>, VcsError>; N] {
        array(self.found.annotate(&inputs))
    }

    fn describe(&self) -> [Result<Option<LineOrigin>, VcsError>; N] {
        array(self.found.describe())
    }
}

/// `list` as an array; the error names its length.
fn array<T, const N: usize>(list: Vec<T>) -> [T; N] {
    <[T; N]>::try_from(list).map_err(|list| list.len()).expect("one entry per input")
}

/// [`origin::find_merge`] of a three-way merge in the repository a run in `cwd` writing
/// `output` finds.
fn find_merge(
    output: &Path,
    title: Option<&str>,
    cwd: &Path,
    [base, left, right]: [&Text; 3],
) -> Result<Looked<3>, VcsError> {
    let repo = RunRepository::of_merge(Some(cwd), Some(output));
    // Texts share their content, so the copies are O(1).
    let inputs = Inputs::three_way(base.clone(), left.clone(), right.clone());
    origin::find_merge(repo, &cwd.join(output), title, &inputs).map(Looked::new)
}

fn merge_sources(
    output: &Path,
    title: Option<&str>,
    cwd: &Path,
    inputs: [&Text; 3],
) -> Result<[Option<Source>; 3], VcsError> {
    find_merge(output, title, cwd, inputs).map(|found| found.sources)
}

/// [`origin::find_diff`] in the repository holding `path`.
fn find_diff(path: &Path, rev: Option<&str>, inputs: [&Text; 2]) -> Result<Looked<2>, VcsError> {
    origin::find_diff(RunRepository::of_file(path), path, rev, inputs).map(Looked::new)
}

fn diff_sources(
    path: &Path,
    rev: Option<&str>,
    inputs: [&Text; 2],
) -> Result<[Option<Source>; 2], VcsError> {
    find_diff(path, rev, inputs).map(|found| found.sources)
}

/// [`origin::find_diff_tool`] in the repository of `diff LEFT RIGHT` run in `cwd`.
fn find_diff_tool(
    paths: [&Path; 2],
    title: Option<&str>,
    cwd: &Path,
    inputs: [&Text; 2],
) -> Result<Looked<2>, VcsError> {
    let repo = RunRepository::of_diff_tool(Some(cwd), paths[1]);
    origin::find_diff_tool(repo, &paths.map(|path| cwd.join(path)), title, cwd, inputs)
        .map(Looked::new)
}

fn diff_tool_sources(
    paths: [&Path; 2],
    title: Option<&str>,
    cwd: &Path,
    inputs: [&Text; 2],
) -> Result<[Option<Source>; 2], VcsError> {
    find_diff_tool(paths, title, cwd, inputs).map(|found| found.sources)
}

fn text(bytes: &[u8]) -> Text {
    Text::decode(bytes.to_vec()).expect("the input decodes")
}

fn exact(commit: &impl ToString) -> Source {
    Source {
        path: Utf8PathBuf::from(PATH),
        commit: CommitId::new(commit.to_string()),
        fit: Fit::Exact,
    }
}

fn changed(commit: &impl ToString) -> Source {
    Source {
        path: Utf8PathBuf::from(PATH),
        commit: CommitId::new(commit.to_string()),
        fit: Fit::Changed,
    }
}

/// The sources `git mergetool` gives for `base`, `left` and `right` in `fixture`.
fn git_merge(fixture: &vcs_git::testing::Fixture, inputs: [&[u8]; 3]) -> [Option<Source>; 3] {
    let output = fixture.write(PATH, MARKERS);
    let [base, left, right] = inputs.map(text);
    merge_sources(output.as_std_path(), None, fixture.root().as_std_path(), [&base, &left, &right])
        .expect("the sources are found")
}

/// `one` holding [`BASE`], then `left` holding [`LEFT`] on `HEAD`'s branch.
fn git_fork() -> (vcs_git::testing::Fixture, gix::ObjectId, gix::ObjectId) {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[(PATH, BASE)]);
    let left = fixture.commit("left", &[(PATH, LEFT)]);
    (fixture, one, left)
}

#[test]
fn a_git_merge_takes_head_merge_head_and_their_merge_base() {
    let (fixture, one, left) = git_fork();
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(
        git_merge(&fixture, [BASE, LEFT, RIGHT]),
        [Some(exact(&one)), Some(exact(&left)), Some(exact(&right))]
    );
}

#[test]
fn a_git_merge_whose_output_is_missing_still_finds_its_sources() {
    // `resolve` writes the working-tree file only on save; a side that deletes it leaves none.
    let (fixture, one, left) = git_fork();
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    // The fixture commits without writing the working tree.
    assert!(!fixture.root().join(PATH).exists());
    let [base, l, r] = [BASE, LEFT, RIGHT].map(text);
    let found = merge_sources(
        fixture.root().join(PATH).as_std_path(),
        None,
        fixture.root().as_std_path(),
        [&base, &l, &r],
    )
    .expect("the sources are found");
    assert_eq!(found, [Some(exact(&one)), Some(exact(&left)), Some(exact(&right))]);
}

#[test]
fn a_git_cherry_pick_takes_the_picked_commit_over_its_parent() {
    let (fixture, one, left) = git_fork();
    let parent = fixture.commit_on(&[one], "parent", &[(PATH, b"a\nB\nc\n")]);
    let picked = fixture.commit_on(&[parent], "picked", &[(PATH, b"a\nB\nC\n")]);
    fixture.write_pseudo_ref("CHERRY_PICK_HEAD", picked);
    assert_eq!(
        git_merge(&fixture, [b"a\nB\nc\n", LEFT, b"a\nB\nC\n"]),
        [Some(exact(&parent)), Some(exact(&left)), Some(exact(&picked))]
    );
}

#[test]
fn a_git_revert_takes_the_reverted_commits_parent_over_it() {
    let (fixture, one, left) = git_fork();
    let parent = fixture.commit_on(&[one], "parent", &[(PATH, RIGHT)]);
    let reverted = fixture.commit_on(&[parent], "reverted", &[(PATH, b"a\nb\nC!\n")]);
    fixture.write_pseudo_ref("REVERT_HEAD", reverted);
    assert_eq!(
        git_merge(&fixture, [b"a\nb\nC!\n", LEFT, RIGHT]),
        [Some(exact(&reverted)), Some(exact(&left)), Some(exact(&parent))]
    );
}

#[test]
fn a_git_rebase_takes_the_replayed_commit_over_its_parent() {
    let (fixture, one, left) = git_fork();
    let replayed = fixture.commit_on(&[one], "replayed", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("REBASE_HEAD", replayed);
    assert_eq!(
        git_merge(&fixture, [BASE, LEFT, RIGHT]),
        [Some(exact(&one)), Some(exact(&left)), Some(exact(&replayed))]
    );
}

#[test]
fn a_git_merge_head_comes_before_a_rebase_head() {
    let (fixture, one, left) = git_fork();
    let merged = fixture.commit_on(&[one], "merged", &[(PATH, RIGHT)]);
    let replayed = fixture.commit_on(&[merged], "replayed", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", merged);
    fixture.write_pseudo_ref("REBASE_HEAD", replayed);
    assert_eq!(
        git_merge(&fixture, [BASE, LEFT, RIGHT]),
        [Some(exact(&one)), Some(exact(&left)), Some(exact(&merged))]
    );
}

#[test]
fn without_a_pending_operation_only_head_is_known() {
    let (fixture, _, left) = git_fork();
    assert_eq!(git_merge(&fixture, [BASE, LEFT, RIGHT]), [None, Some(exact(&left)), None]);
}

#[test]
fn an_input_that_matches_no_revision_has_no_source() {
    let (fixture, one, _) = git_fork();
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(
        git_merge(&fixture, [BASE, b"A\nb\nc\nedited\n", RIGHT]),
        [Some(exact(&one)), None, Some(exact(&right))]
    );
}

#[test]
fn an_input_that_is_not_utf8_has_no_source_even_when_its_bytes_match() {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[(PATH, BASE)]);
    // `HEAD` holds the left input's bytes; only its encoding rules it out.
    fixture.commit("left", &[(PATH, UTF16)]);
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    assert_eq!(
        git_merge(&fixture, [BASE, UTF16, RIGHT]),
        [Some(exact(&one)), None, Some(exact(&right))]
    );
}

#[test]
fn a_utf8_input_with_a_byte_order_mark_matches_its_bytes() {
    let fixture = vcs_git::testing::Fixture::new();
    let bom_left: &[u8] = b"\xEF\xBB\xBFA\nb\nc\n";
    fixture.commit("one", &[(PATH, BASE)]);
    let left = fixture.commit("left", &[(PATH, bom_left)]);
    assert_eq!(git_merge(&fixture, [BASE, bom_left, RIGHT]), [None, Some(exact(&left)), None]);
}

#[test]
fn an_output_outside_the_working_directorys_repository_has_no_sources() {
    let (fixture, one, _) = git_fork();
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    let output = elsewhere.path().join(PATH);
    std::fs::write(&output, MARKERS).expect("the output is written");
    let [base, left, right] = [BASE, LEFT, RIGHT].map(text);
    assert_eq!(
        merge_sources(&output, None, elsewhere.path(), [&base, &left, &right])
            .expect("no repository is no error"),
        [None, None, None]
    );
}

/// A jj working copy merging `left` and `right`, both on `base`, whose `a.txt` conflicts.
fn jj_conflict(fixture: &vcs_jj::testing::Fixture) -> [vcs_jj::testing::Committed; 3] {
    let [base, left, right] = fixture.three_way(PATH, [BASE, LEFT, RIGHT]);
    [base, left, right]
}

/// The sources `jj resolve` gives, run in `cwd` with `$path` as the title. The output lies in
/// jj's temporary directory, outside the repository.
fn jj_merge(cwd: &Path, title: Option<&str>, inputs: [&[u8]; 3]) -> [Option<Source>; 3] {
    let resolve =
        tempfile::Builder::new().prefix("jj-resolve-").tempdir().expect("a temporary directory");
    let output = resolve.path().join(format!("output_{PATH}"));
    std::fs::write(&output, MARKERS).expect("the output is written");
    let [base, left, right] = inputs.map(text);
    merge_sources(&output, title, cwd, [&base, &left, &right]).expect("the sources are found")
}

fn a_jj_conflict_takes_the_working_copys_parents_and_their_fork_point(
    fixture: &vcs_jj::testing::Fixture,
) {
    let [base, left, right] = jj_conflict(fixture);
    assert_eq!(
        jj_merge(fixture.root().as_std_path(), Some(PATH), [BASE, LEFT, RIGHT]),
        [Some(exact(&base.commit)), Some(exact(&left.commit)), Some(exact(&right.commit))]
    );
}

fn diff(path: &Path, rev: Option<&str>, inputs: [&[u8]; 2]) -> [Option<Source>; 2] {
    let [left, right] = inputs.map(text);
    diff_sources(path, rev, [&left, &right]).expect("the sources are found")
}

#[test]
fn a_git_diff_takes_head_and_annotates_the_working_copy_against_it() {
    let (fixture, _, left) = git_fork();
    fixture.stage(&[(PATH, LEFT)]);
    let path = fixture.write(PATH, b"A\nb\nc\nnew\n");
    assert_eq!(
        diff(path.as_std_path(), None, [LEFT, b"A\nb\nc\nnew\n"]),
        [Some(exact(&left)), Some(changed(&left))]
    );
}

#[test]
fn a_git_diff_against_the_index_annotates_both_sides_against_head() {
    let (fixture, _, left) = git_fork();
    fixture.stage(&[(PATH, b"A\nstaged\n")]);
    let path = fixture.write(PATH, b"A\nstaged\nnew\n");
    assert_eq!(
        diff(path.as_std_path(), None, [b"A\nstaged\n", b"A\nstaged\nnew\n"]),
        [Some(changed(&left)), Some(changed(&left))]
    );
}

#[test]
fn a_git_diff_against_a_revision_takes_it_on_the_left_only() {
    let (fixture, one, left) = git_fork();
    fixture.stage(&[(PATH, LEFT)]);
    let path = fixture.write(PATH, LEFT);
    assert_eq!(
        diff(path.as_std_path(), Some("HEAD~1"), [BASE, LEFT]),
        [Some(exact(&one)), Some(changed(&left))]
    );
}

#[test]
fn a_diff_outside_any_repository_has_no_sources() {
    let outside = tempfile::tempdir().expect("a temporary directory");
    let path = outside.path().join(PATH);
    std::fs::write(&path, LEFT).expect("the file is written");
    assert_eq!(diff(&path, None, [LEFT, LEFT]), [None, None]);
}

/// A file `name` holding `bytes` in a new temporary directory, as git's and jj's tools copy
/// a side.
fn temp_copy(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("the copy is written");
    (dir, path)
}

fn diff_tool(
    paths: [&Path; 2],
    title: Option<&str>,
    cwd: &Path,
    inputs: [&[u8]; 2],
) -> [Option<Source>; 2] {
    let [left, right] = inputs.map(text);
    diff_tool_sources(paths, title, cwd, [&left, &right]).expect("the sources are found")
}

#[test]
fn git_difftool_of_the_working_tree_takes_head_and_annotates_the_file_against_it() {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[(PATH, BASE)]);
    fixture.stage(&[(PATH, BASE)]);
    let worktree = fixture.write(PATH, RIGHT);
    let (_dir, local) = temp_copy("a_LOCAL_1234.txt", BASE);
    let root = fixture.root().as_std_path();
    // The working-tree path names the file, whatever the title says.
    for title in [None, Some("b.txt")] {
        assert_eq!(
            diff_tool([&local, worktree.as_std_path()], title, root, [BASE, RIGHT]),
            [Some(exact(&one)), Some(changed(&one))],
            "title {title:?}"
        );
    }
}

#[test]
fn git_difftool_of_staged_changes_annotates_the_index_against_head() {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[(PATH, BASE)]);
    fixture.stage(&[(PATH, LEFT)]);
    let (_l, local) = temp_copy("a_LOCAL.txt", BASE);
    let (_r, remote) = temp_copy("a_REMOTE.txt", LEFT);
    assert_eq!(
        diff_tool([&local, &remote], Some(PATH), fixture.root().as_std_path(), [BASE, LEFT]),
        [Some(exact(&one)), Some(changed(&one))]
    );
}

#[test]
fn git_difftool_of_two_commits_knows_only_head() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[(PATH, BASE)]);
    let two = fixture.commit("two", &[(PATH, LEFT)]);
    fixture.stage(&[(PATH, LEFT)]);
    let (_l, local) = temp_copy("a_LOCAL.txt", BASE);
    let (_r, remote) = temp_copy("a_REMOTE.txt", LEFT);
    assert_eq!(
        diff_tool([&local, &remote], Some(PATH), fixture.root().as_std_path(), [BASE, LEFT]),
        [None, Some(exact(&two))]
    );
}

#[test]
fn a_diff_of_temporary_copies_without_a_title_inside_the_repository_has_no_sources() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[(PATH, BASE)]);
    let (_l, local) = temp_copy("a_LOCAL.txt", BASE);
    let (_r, remote) = temp_copy("a_REMOTE.txt", BASE);
    let root = fixture.root().as_std_path();
    for title in [None, Some("../a.txt"), Some("/a.txt"), Some("")] {
        assert_eq!(
            diff_tool([&local, &remote], title, root, [BASE, BASE]),
            [None, None],
            "title {title:?}"
        );
    }
}

/// jj's `jj-diff-<random>` directory with `left/<path>` and `right/<path>`.
fn jj_diff_copies(
    path: &str,
    [left, right]: [&[u8]; 2],
) -> (tempfile::TempDir, [std::path::PathBuf; 2]) {
    let dir = tempfile::Builder::new().prefix("jj-diff-").tempdir().expect("a temporary directory");
    let copies = ["left", "right"].map(|side| dir.path().join(side).join(path));
    for (copy, bytes) in copies.iter().zip([left, right]) {
        std::fs::create_dir_all(copy.parent().expect("a directory")).expect("directories");
        std::fs::write(copy, bytes).expect("the copy is written");
    }
    (dir, copies)
}

#[test]
fn a_jj_diff_copy_names_the_repository_path_after_its_side() {
    let cwd = Path::new("/work");
    let copy = |path: &str| origin::jj_diff_copy(Path::new(path), cwd);
    assert_eq!(copy("/tmp/jj-diff-a1b2/left/src/a.rs").as_deref(), Some(Path::new("src/a.rs")));
    assert_eq!(copy("/tmp/jj-diff-a1b2/right/../left/a.rs").as_deref(), Some(Path::new("a.rs")));
    assert_eq!(
        copy("jj-diff-a1/output/a.rs").as_deref(),
        Some(Path::new("a.rs")),
        "relative to the working directory"
    );
    assert_eq!(copy("/tmp/jj-diff-a1b2/left").as_deref(), Some(Path::new("")));
    assert_eq!(copy("/tmp/jj-diff-a1b2/left_state/a.rs"), None);
    assert_eq!(copy("/tmp/jj-diff-/left/a.rs"), None);
    assert_eq!(copy("/home/u/project/left/a.rs"), None);
}

/// Each run's lines and its commit's summary.
fn summaries(annotation: &vcs::Annotation) -> Vec<(std::ops::Range<usize>, &str)> {
    annotation
        .runs
        .iter()
        .map(|run| {
            let summary = annotation.origins[run.origin].change.summary.as_deref();
            (run.lines.clone(), summary.expect("fixture commits have summaries"))
        })
        .collect()
}

#[test]
fn a_git_merge_annotates_each_input_at_its_commit() {
    let (fixture, one, _) = git_fork();
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    let output = fixture.write(PATH, MARKERS);
    let [base, left, right] = [BASE, LEFT, RIGHT].map(text);
    let found = find_merge(
        output.as_std_path(),
        None,
        fixture.root().as_std_path(),
        [&base, &left, &right],
    )
    .expect("the sources are found");
    let [base, left, right] = found
        .annotate([&base, &left, &right])
        .map(|result| result.expect("the annotation succeeds").expect("the file exists"));
    assert_eq!(summaries(&base), [(0..3, "one")]);
    assert_eq!(summaries(&left), [(0..1, "left"), (1..3, "one")]);
    assert_eq!(summaries(&right), [(0..2, "one"), (2..3, "right")]);
}

#[test]
fn a_working_copy_takes_its_parents_origins_on_the_lines_it_kept() {
    let (fixture, _, _) = git_fork();
    fixture.stage(&[(PATH, LEFT)]);
    let path = fixture.write(PATH, b"A\nnew\nb\nc\n");
    let [left, right] = [LEFT, b"A\nnew\nb\nc\n"].map(text);
    let found =
        find_diff(path.as_std_path(), None, [&left, &right]).expect("the sources are found");
    let [left, right] = found.annotate([&left, &right]);
    let left = left.expect("annotated").expect("the file exists");
    let right = right.expect("annotated").expect("the file exists");
    assert_eq!(summaries(&left), [(0..1, "left"), (1..3, "one")]);
    assert_eq!(summaries(&right), [(0..1, "left"), (2..4, "one")]);
}

#[test]
fn a_new_file_and_an_input_without_a_source_have_no_annotation() {
    let (fixture, _, _) = git_fork();
    fixture.stage(&[(PATH, LEFT)]);
    let path = fixture.write("new.txt", b"fresh\n");
    let [left, right] = [b"".as_slice(), b"fresh\n"].map(text);
    let found =
        find_diff(path.as_std_path(), None, [&left, &right]).expect("the sources are found");
    let annotations = found.annotate([&left, &right]).map(|result| result.expect("annotated"));
    assert_eq!(annotations, [None, None]);
}

// The commit each input shows, which its pane heading names (spec §5, §7.3).

fn id(commit: &impl ToString) -> CommitId {
    CommitId::new(commit.to_string())
}

fn diff_revisions(path: &Path, rev: Option<&str>, inputs: [&[u8]; 2]) -> [Option<CommitId>; 2] {
    let [left, right] = inputs.map(text);
    find_diff(path, rev, [&left, &right]).expect("the sources are found").revisions
}

/// The full ID of the commit `rev` names in the repository holding `dir`.
fn commit_at(dir: &camino::Utf8Path, rev: &str) -> CommitId {
    let repo = merge_session::native::discover(dir).expect("the repository is found");
    let [commit] = <[CommitId; 1]>::try_from(repo.commits(rev).expect("the revision resolves"))
        .expect("one commit");
    commit
}

#[test]
fn a_git_merges_inputs_show_the_commits_that_hold_them() {
    let (fixture, one, left) = git_fork();
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    let output = fixture.write(PATH, MARKERS);
    let [base, l, r] = [BASE, LEFT, RIGHT].map(text);
    let found =
        find_merge(output.as_std_path(), None, fixture.root().as_std_path(), [&base, &l, &r])
            .expect("the sources are found");
    assert_eq!(found.revisions, [Some(id(&one)), Some(id(&left)), Some(id(&right))]);
    let summaries = found.describe().map(|described| {
        described.expect("the commit is described").and_then(|origin| origin.change.summary)
    });
    assert_eq!(summaries, [Some("one"), Some("left"), Some("right")].map(|s| s.map(str::to_owned)));
}

#[test]
fn a_git_merge_input_that_matches_no_commit_shows_none() {
    let (fixture, one, left) = git_fork();
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT)]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    let output = fixture.write(PATH, MARKERS);
    let [base, l, r] = [BASE, LEFT, b"edited\n".as_slice()].map(text);
    let found =
        find_merge(output.as_std_path(), None, fixture.root().as_std_path(), [&base, &l, &r])
            .expect("the sources are found");
    assert_eq!(found.revisions, [Some(id(&one)), Some(id(&left)), None]);
    let [_, _, right] = found.describe();
    assert!(matches!(right, Ok(None)), "{right:?}");
}

#[test]
fn a_git_diff_shows_head_on_the_left_and_no_commit_for_the_working_tree() {
    let (fixture, _, left) = git_fork();
    fixture.stage(&[(PATH, LEFT)]);
    let path = fixture.write(PATH, b"A\nb\nc\nnew\n");
    assert_eq!(
        diff_revisions(path.as_std_path(), None, [LEFT, b"A\nb\nc\nnew\n"]),
        [Some(id(&left)), None]
    );
}

#[test]
fn a_git_diff_against_the_index_shows_no_commit() {
    let (fixture, _, _) = git_fork();
    fixture.stage(&[(PATH, b"A\nstaged\n")]);
    let path = fixture.write(PATH, b"A\nstaged\nnew\n");
    assert_eq!(
        diff_revisions(path.as_std_path(), None, [b"A\nstaged\n", b"A\nstaged\nnew\n"]),
        [None, None]
    );
}

#[test]
fn a_git_diff_against_a_revision_shows_it_even_where_the_file_is_missing_or_not_utf8() {
    let (fixture, one, _) = git_fork();
    fixture.stage(&[(PATH, LEFT)]);
    let path = fixture.write(PATH, LEFT);
    assert_eq!(
        diff_revisions(path.as_std_path(), Some("HEAD~1"), [BASE, LEFT]),
        [Some(id(&one)), None]
    );
    let new = fixture.write("new.txt", b"fresh\n");
    assert_eq!(
        diff_revisions(new.as_std_path(), Some("HEAD~1"), [b"", b"fresh\n"]),
        [Some(id(&one)), None],
        "the file does not exist at the revision"
    );
    assert_eq!(
        diff_revisions(path.as_std_path(), Some("HEAD~1"), [UTF16, LEFT]),
        [Some(id(&one)), None],
        "an input that is not UTF-8 has no origins, but its revision is known"
    );
}

#[test]
fn a_diff_outside_any_repository_shows_no_commit() {
    let outside = tempfile::tempdir().expect("a temporary directory");
    let path = outside.path().join(PATH);
    std::fs::write(&path, LEFT).expect("the file is written");
    assert_eq!(diff_revisions(&path, None, [LEFT, LEFT]), [None, None]);
}

fn tool_revisions(
    paths: [&Path; 2],
    title: Option<&str>,
    cwd: &Path,
    inputs: [&[u8]; 2],
) -> [Option<CommitId>; 2] {
    let [left, right] = inputs.map(text);
    find_diff_tool(paths, title, cwd, [&left, &right]).expect("the sources are found").revisions
}

#[test]
fn git_difftool_shows_the_commit_a_copy_holds_and_none_for_the_working_tree_or_index() {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[(PATH, BASE)]);
    fixture.stage(&[(PATH, LEFT)]);
    let worktree = fixture.write(PATH, RIGHT);
    let (_l, local) = temp_copy("a_LOCAL.txt", BASE);
    let (_r, remote) = temp_copy("a_REMOTE.txt", LEFT);
    let root = fixture.root().as_std_path();
    assert_eq!(
        tool_revisions([&local, worktree.as_std_path()], None, root, [BASE, RIGHT]),
        [Some(id(&one)), None],
        "the working tree"
    );
    assert_eq!(
        tool_revisions([&local, &remote], Some(PATH), root, [BASE, LEFT]),
        [Some(id(&one)), None],
        "the index"
    );
}

/// Tests that build a jj repository. `script/pre-commit` leaves them out (`.config/nextest.toml`).
mod jj {
    use super::*;

    #[test]
    fn a_jj_conflict_takes_the_working_copys_parents_and_their_fork_point_colocated() {
        a_jj_conflict_takes_the_working_copys_parents_and_their_fork_point(
            &vcs_jj::testing::Fixture::colocated(),
        );
    }

    #[test]
    fn a_jj_conflict_takes_the_working_copys_parents_and_their_fork_point_internal() {
        a_jj_conflict_takes_the_working_copys_parents_and_their_fork_point(
            &vcs_jj::testing::Fixture::internal(),
        );
    }

    #[test]
    fn a_jj_working_copy_holding_an_input_comes_first() {
        let fixture = vcs_jj::testing::Fixture::new();
        fixture.commit("one", &[(PATH, BASE)]);
        let working_copy = fixture.open();
        let at = vcs::Repository::commits(&working_copy, "@").expect("@ resolves");
        let [at] = at.as_slice() else {
            panic!("one working-copy commit, got {at:?}");
        };
        assert_eq!(
            jj_merge(fixture.root().as_std_path(), Some(PATH), [BASE, LEFT, RIGHT]),
            [Some(exact(&at)), None, None]
        );
    }

    #[test]
    fn a_jj_merge_found_from_a_subdirectory_takes_the_title_from_the_root() {
        let fixture = vcs_jj::testing::Fixture::new();
        let [base, left, right] = jj_conflict(&fixture);
        let sub = fixture.root().join("sub");
        std::fs::create_dir(&sub).expect("the subdirectory is made");
        assert_eq!(
            jj_merge(sub.as_std_path(), Some(PATH), [BASE, LEFT, RIGHT]),
            [Some(exact(&base.commit)), Some(exact(&left.commit)), Some(exact(&right.commit))]
        );
    }

    #[test]
    fn a_jj_merge_without_a_title_or_with_one_outside_the_repository_has_no_sources() {
        let fixture = vcs_jj::testing::Fixture::new();
        jj_conflict(&fixture);
        let root = fixture.root().as_std_path();
        assert_eq!(jj_merge(root, None, [BASE, LEFT, RIGHT]), [None, None, None]);
        assert_eq!(jj_merge(root, Some("../a.txt"), [BASE, LEFT, RIGHT]), [None, None, None]);
        assert_eq!(jj_merge(root, Some("missing.txt"), [BASE, LEFT, RIGHT]), [None, None, None]);
    }

    #[test]
    fn a_jj_merge_run_outside_any_repository_has_no_sources() {
        let fixture = vcs_jj::testing::Fixture::new();
        jj_conflict(&fixture);
        let outside = tempfile::tempdir().expect("a temporary directory");
        assert_eq!(jj_merge(outside.path(), Some(PATH), [BASE, LEFT, RIGHT]), [None, None, None]);
    }

    #[test]
    fn a_jj_diff_takes_the_parent_and_annotates_the_working_copy_against_it() {
        let fixture = vcs_jj::testing::Fixture::new();
        let one = fixture.commit("one", &[(PATH, BASE)]);
        let path = fixture.write(PATH, LEFT);
        assert_eq!(
            diff(path.as_std_path(), None, [BASE, LEFT]),
            [Some(exact(&one.commit)), Some(changed(&one.commit))]
        );
    }

    #[test]
    fn a_jj_diff_against_a_merge_of_parents_has_no_sources() {
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[(PATH, BASE)]);
        let left = fixture.commit_on(&[&base], "left", &[(PATH, LEFT)]);
        let right = fixture.commit_on(&[&base], "right", &[("b.txt", RIGHT)]);
        fixture.check_out(&[&left, &right]);
        let path = fixture.write(PATH, LEFT);
        assert_eq!(diff(path.as_std_path(), None, [LEFT, LEFT]), [None, None]);
    }

    #[test]
    fn jj_diff_tool_takes_the_path_from_its_directory_and_tries_the_parent_then_the_working_copy() {
        let fixture = vcs_jj::testing::Fixture::new();
        let nested = "src/a.txt";
        fixture.commit("one", &[(nested, BASE)]);
        let two = fixture.commit("two", &[(nested, LEFT)]);
        let (_dir, [left, right]) = jj_diff_copies(nested, [LEFT, RIGHT]);
        let sources = diff_tool([&left, &right], None, fixture.root().as_std_path(), [LEFT, RIGHT]);
        let expected = Source {
            path: Utf8PathBuf::from(nested),
            commit: CommitId::new(two.commit),
            fit: Fit::Exact,
        };
        assert_eq!(sources, [Some(expected), None]);
    }

    #[test]
    fn jj_diff_tool_matches_the_working_copy_when_it_holds_a_change() {
        let fixture = vcs_jj::testing::Fixture::new();
        let one = fixture.commit("one", &[(PATH, BASE)]);
        let working = fixture.commit_on(&[&one], "working", &[(PATH, RIGHT)]);
        fixture.edit(&working);
        let (_dir, [left, right]) = jj_diff_copies(PATH, [BASE, RIGHT]);
        assert_eq!(
            diff_tool([&left, &right], None, fixture.root().as_std_path(), [BASE, RIGHT]),
            [Some(exact(&one.commit)), Some(exact(&working.commit))]
        );
    }

    #[test]
    fn a_three_sided_jj_conflict_finds_every_term() {
        const THIRD: &[u8] = b"a\nB\nc\n";
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[(PATH, BASE)]);
        let sides = [("one", LEFT), ("two", RIGHT), ("three", THIRD)]
            .map(|(name, bytes)| fixture.commit_on(&[&base], name, &[(PATH, bytes)]));
        fixture.check_out(&sides.each_ref());
        let inputs =
            Inputs::new([LEFT, RIGHT, THIRD].map(text).to_vec(), [BASE, BASE].map(text).to_vec())
                .expect("three sides, two bases");
        let output = tempfile::tempdir().expect("a temporary directory");
        let output = output.path().join(format!("output_{PATH}"));
        let cwd = fixture.root().as_std_path();
        let repo = RunRepository::of_merge(Some(cwd), Some(&output));
        let found = origin::find_merge(repo, &output, Some(PATH), &inputs).expect("found");
        let [one, two, three] = &sides;
        let expected =
            [&base, &base, one, two, three].map(|commit| Some(exact(&commit.commit))).to_vec();
        assert_eq!(found.sources, expected);
        let texts: Vec<&Text> = origin::merge_inputs(&inputs).map(|(_, text)| text).collect();
        let annotations: Vec<_> = found
            .annotate(&texts)
            .into_iter()
            .map(|result| result.expect("the annotation succeeds").expect("the file exists"))
            .collect();
        assert_eq!(summaries(&annotations[4]), [(0..1, "base"), (1..2, "three"), (2..3, "base")]);
    }

    #[test]
    fn a_jj_conflict_annotates_its_sides_and_base() {
        let fixture = vcs_jj::testing::Fixture::new();
        jj_conflict(&fixture);
        let resolve = tempfile::Builder::new()
            .prefix("jj-resolve-")
            .tempdir()
            .expect("a temporary directory");
        let output = resolve.path().join(format!("output_{PATH}"));
        let [base, left, right] = [BASE, LEFT, RIGHT].map(text);
        let found =
            find_merge(&output, Some(PATH), fixture.root().as_std_path(), [&base, &left, &right])
                .expect("the sources are found");
        let [base, left, right] = found
            .annotate([&base, &left, &right])
            .map(|result| result.expect("the annotation succeeds").expect("the file exists"));
        assert_eq!(summaries(&base), [(0..3, "base")]);
        assert_eq!(summaries(&left), [(0..1, "left"), (1..3, "base")]);
        assert_eq!(summaries(&right), [(0..2, "base"), (2..3, "right")]);
    }

    #[test]
    fn a_jj_diff_shows_the_parent_and_the_working_copy_commit() {
        let fixture = vcs_jj::testing::Fixture::new();
        let one = fixture.commit("one", &[(PATH, BASE)]);
        let path = fixture.write(PATH, LEFT);
        let working = commit_at(fixture.root(), "@");
        assert_ne!(working.as_str(), one.commit);
        assert_eq!(
            diff_revisions(path.as_std_path(), None, [BASE, LEFT]),
            [Some(id(&one.commit)), Some(working)]
        );
    }

    #[test]
    fn a_jj_diff_against_a_merge_of_parents_shows_only_the_working_copy() {
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[(PATH, BASE)]);
        let left = fixture.commit_on(&[&base], "left", &[(PATH, LEFT)]);
        let right = fixture.commit_on(&[&base], "right", &[("b.txt", RIGHT)]);
        fixture.check_out(&[&left, &right]);
        let path = fixture.write(PATH, LEFT);
        let working = commit_at(fixture.root(), "@");
        assert_eq!(diff_revisions(path.as_std_path(), None, [LEFT, LEFT]), [None, Some(working)]);
    }

    #[test]
    fn jj_diff_tool_shows_the_commit_each_copy_holds() {
        let fixture = vcs_jj::testing::Fixture::new();
        let one = fixture.commit("one", &[(PATH, BASE)]);
        let working = fixture.commit_on(&[&one], "working", &[(PATH, RIGHT)]);
        fixture.edit(&working);
        let (_dir, [left, right]) = jj_diff_copies(PATH, [BASE, RIGHT]);
        let root = fixture.root().as_std_path();
        assert_eq!(
            tool_revisions([&left, &right], None, root, [BASE, RIGHT]),
            [Some(id(&one.commit)), Some(id(&working.commit))]
        );
        assert_eq!(
            tool_revisions([&left, &right], None, root, [BASE, b"edited\n"]),
            [Some(id(&one.commit)), None],
            "an edited copy holds no commit's bytes"
        );
    }
}
