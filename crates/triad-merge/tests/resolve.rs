//! `triad-merge resolve --headless` on git and jj repositories (`PLAN.md`
//! §D5.4): what reaches the working-tree file, what stays in the repository, and what jj
//! reads back.

use std::process::{Command, Output};

use camino::Utf8Path;
use vcs::testing::Entry;
use vcs::{Repository, TermContent};

const BINARY: &str = env!("CARGO_BIN_EXE_triad-merge");
const RESOLVED: i32 = 0;
const CONFLICTS: i32 = 1;
const ERROR: i32 = 3;
/// git shortens commit IDs to this many characters in a small repository.
const GIT_SHORT: usize = 7;
/// jj's conflict labels show change and commit IDs to this many characters.
const JJ_SHORT: usize = 8;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// The binary in `dir`, logging into a directory that outlives the process but not the test
/// run, and reading no settings file, no user or system git config and no user jj config.
fn triad(dir: &Utf8Path, args: &[&str]) -> Output {
    triad_with_jj_config(dir, &LOGS.path().join("no-jjconfig.toml"), args)
}

static LOGS: std::sync::LazyLock<tempfile::TempDir> =
    std::sync::LazyLock::new(|| tempfile::tempdir().expect("log dir"));

/// As [`triad`], with `jj_config` as the user's jj config.
fn triad_with_jj_config(dir: &Utf8Path, jj_config: &std::path::Path, args: &[&str]) -> Output {
    Command::new(BINARY)
        .env("TRIAD_LOG_DIR", LOGS.path())
        .env("TRIAD_CONFIG", LOGS.path().join("no-config.triad"))
        .env("GIT_CONFIG_GLOBAL", LOGS.path().join("no-gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("JJ_CONFIG", jj_config)
        .env_remove("TRIAD_LOG")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("binary runs")
}

fn outcome(output: &Output) -> (Option<i32>, String, String) {
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    (output.status.code(), text(&output.stdout), text(&output.stderr))
}

fn read(root: &Utf8Path, path: &str) -> Vec<u8> {
    std::fs::read(root.join(path)).expect("the file reads")
}

/// A git merge of `left` and `right` over `base` stopped on `paths`, with `MERGE_HEAD` set.
/// Returns the short IDs of the base and theirs.
fn git_stopped(
    paths: &[&str],
    [base, left, right]: [&'static [u8]; 3],
) -> (vcs_git::testing::Fixture, [String; 2]) {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[("a.txt", base)]);
    fixture.commit("left", &[("a.txt", left)]);
    let theirs = fixture.commit_on(&[one], "right", &[("a.txt", right)]);
    let conflicts: Vec<(&str, [Option<Entry>; 3])> = paths
        .iter()
        .map(|&path| (path, [base, left, right].map(|t| Some(Entry::File(t)))))
        .collect();
    fixture.stage_conflicts(&[], &conflicts);
    fixture.write_pseudo_ref("MERGE_HEAD", theirs);
    let short = |id: gix::ObjectId| id.to_string()[..GIT_SHORT].to_owned();
    (fixture, [short(one), short(theirs)])
}

fn terms(repo: &dyn Repository, path: &str) -> Vec<TermContent> {
    let conflict = repo
        .read_conflict(Utf8Path::new(path))
        .expect("the conflict reads")
        .expect("still conflicted");
    conflict.sides.into_iter().chain(conflict.bases).map(|term| term.content).collect()
}

fn file(bytes: &[u8]) -> TermContent {
    TermContent::File { bytes: bytes.to_vec(), executable: false }
}

#[test]
fn a_git_conflict_is_written_with_markers_and_its_stages_stay() {
    let (fixture, [one, theirs]) = git_stopped(&["a.txt"], [b"a\n", b"left\n", b"right\n"]);
    let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
    assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
    assert_eq!(
        String::from_utf8(read(fixture.root(), "a.txt")).expect("UTF-8"),
        format!(
            "<<<<<<< main\nleft\n||||||| {one} one\na\n=======\nright\n>>>>>>> {theirs} right\n"
        )
    );
    assert_eq!(terms(&fixture.open(), "a.txt"), [file(b"left\n"), file(b"right\n"), file(b"a\n")]);
}

#[test]
fn a_git_conflict_is_written_in_gits_conflict_style() {
    let (fixture, [_, theirs]) = git_stopped(&["a.txt"], [b"a\n", b"left\n", b"right\n"]);
    fixture.append_config("[merge]\n\tconflictStyle = merge\n");
    let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
    assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
    assert_eq!(
        String::from_utf8(read(fixture.root(), "a.txt")).expect("UTF-8"),
        format!("<<<<<<< main\nleft\n=======\nright\n>>>>>>> {theirs} right\n")
    );
}

#[test]
fn commands_resolve_a_git_conflict_in_the_inputs_encoding() {
    let (fixture, _) = git_stopped(
        &["a.txt"],
        [b"\xEF\xBB\xBFa\n", b"\xEF\xBB\xBFleft\n", b"\xEF\xBB\xBFright\n"],
    );
    let commands =
        fixture.write("commands.jsonl", br#"{"Pick":{"region":0,"sources":[{"Side":1}]}}"#);
    let result =
        triad(fixture.root(), &["resolve", "--headless", "a.txt", "--commands", commands.as_str()]);
    assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
    assert_eq!(read(fixture.root(), "a.txt"), [UTF8_BOM, b"right\n"].concat());
    // The resolution replaces the stages, as `git add` does.
    assert_eq!(fixture.open().conflicts().expect("listed"), Vec::<camino::Utf8PathBuf>::new());
    assert_eq!(staged(&fixture, "a.txt"), [UTF8_BOM, b"right\n"].concat());
}

#[test]
fn a_save_the_index_refuses_is_written_and_named_as_unrecorded() {
    let (fixture, _) = git_stopped(&["a.txt"], [b"a\n", b"left\n", b"right\n"]);
    let commands =
        fixture.write("commands.jsonl", br#"{"Pick":{"region":0,"sources":[{"Side":1}]}}"#);
    // Another git command holds the index.
    fixture.write(".git/index.lock", b"");
    let result =
        triad(fixture.root(), &["resolve", "--headless", "a.txt", "--commands", commands.as_str()]);
    let (code, stdout, stderr) = outcome(&result);
    assert_eq!((code, stdout.as_str()), (Some(ERROR), ""));
    assert!(
        stderr.starts_with(
            "triad-merge: a.txt is written, but the repository did not record the save: "
        ),
        "{stderr}"
    );
    assert_eq!(read(fixture.root(), "a.txt"), b"right\n");
    assert_eq!(terms(&fixture.open(), "a.txt"), [file(b"left\n"), file(b"right\n"), file(b"a\n")]);
}

/// The blob at stage 0 for `path` in the fixture's index.
fn staged(fixture: &vcs_git::testing::Fixture, path: &str) -> Vec<u8> {
    let repo = gix::open(fixture.root()).expect("the repository opens");
    let index = repo.open_index().expect("the index is read");
    let entry = index
        .entry_by_path_and_stage(path.into(), gix::index::entry::Stage::Unconflicted)
        .expect("a stage-0 entry");
    repo.find_object(entry.id).expect("the blob").detach().data
}

#[test]
fn without_a_path_every_conflicted_file_is_written() {
    let (fixture, _) = git_stopped(&["a.txt", "src/b.txt"], [b"a\n", b"left\n", b"right\n"]);
    std::fs::create_dir_all(fixture.root().join("src")).expect("the directory is made");
    let result = triad(fixture.root(), &["resolve", "--headless"]);
    assert_eq!(outcome(&result), (Some(CONFLICTS), "a.txt\nsrc/b.txt\n".to_owned(), String::new()));
    for path in ["a.txt", "src/b.txt"] {
        assert!(read(fixture.root(), path).starts_with(b"<<<<<<< main\nleft\n"), "{path}");
    }
}

#[test]
fn without_a_path_a_refused_file_is_skipped_with_its_reason() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[("x.txt", b"x\n")]);
    // b.txt merges cleanly: only the right side changes it.
    let terms = |t: [Option<&'static [u8]>; 3]| t.map(|t| t.map(Entry::File));
    fixture.stage_conflicts(
        &[],
        &[
            ("a.txt", terms([Some(b"a\n"), None, None])),
            ("b.txt", terms([Some(b"a\n"), Some(b"a\n"), Some(b"b\n")])),
        ],
    );
    let result = triad(fixture.root(), &["resolve", "--headless"]);
    assert_eq!(
        outcome(&result),
        (
            Some(CONFLICTS),
            "a.txt\nb.txt\n".to_owned(),
            "triad-merge: a.txt is deleted on every side; nothing to merge\n".to_owned()
        )
    );
    assert!(!fixture.root().join("a.txt").exists(), "a.txt stays deleted");
    assert_eq!(read(fixture.root(), "b.txt"), b"b\n");
}

#[test]
fn without_a_path_a_file_resolved_by_hand_is_skipped_and_kept() {
    let (fixture, _) = git_stopped(&["a.txt", "b.txt"], [b"a\n", b"left\n", b"right\n"]);
    fixture.write("a.txt", b"resolved by hand\n");
    let result = triad(fixture.root(), &["resolve", "--headless"]);
    assert_eq!(
        outcome(&result),
        (
            Some(CONFLICTS),
            "a.txt\nb.txt\n".to_owned(),
            "triad-merge: a.txt has no conflict markers left in the working tree, so it looks \
             resolved; mark it resolved, or put its markers back to merge it again\n"
                .to_owned()
        )
    );
    assert_eq!(read(fixture.root(), "a.txt"), b"resolved by hand\n");
    assert!(read(fixture.root(), "b.txt").starts_with(b"<<<<<<< main\nleft\n"));
}

#[test]
fn a_conflict_resolved_by_hand_beside_another_keeps_the_hand_resolution() {
    let (fixture, _) =
        git_stopped(&["a.txt"], [b"a\nb\nc\nd\ne\n", b"a\nL1\nc\nL2\ne\n", b"a\nR1\nc\nR2\ne\n"]);
    fixture.write(
        "a.txt",
        b"a\nL1\nc\n<<<<<<< ours\nL2\n||||||| base\nd\n=======\nR2\n>>>>>>> theirs\ne\n",
    );
    let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
    assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
    // jj's default `base` counts as no label (`PLAN.md` §D5.7), so its marker has none.
    assert_eq!(
        String::from_utf8(read(fixture.root(), "a.txt")).expect("UTF-8"),
        "a\nL1\nc\n<<<<<<< ours\nL2\n|||||||\nd\n=======\nR2\n>>>>>>> theirs\ne\n"
    );
}

#[test]
fn with_a_path_a_file_resolved_by_hand_is_an_error_and_kept() {
    let (fixture, _) = git_stopped(&["a.txt"], [b"a\n", b"left\n", b"right\n"]);
    fixture.write("a.txt", b"resolved by hand\n");
    let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
    assert_eq!(outcome(&result).0, Some(ERROR));
    assert_eq!(read(fixture.root(), "a.txt"), b"resolved by hand\n");
}

#[test]
fn with_a_path_a_refused_file_is_an_error() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[("x.txt", b"x\n")]);
    fixture.stage_conflicts(&[], &[("a.txt", [Some(Entry::File(b"a\n")), None, None])]);
    let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
    assert_eq!(
        outcome(&result),
        (
            Some(ERROR),
            String::new(),
            "triad-merge: a.txt is deleted on every side; nothing to merge\n".to_owned()
        )
    );
    assert!(!fixture.root().join("a.txt").exists());
}

#[test]
fn commands_need_a_path() {
    let (fixture, _) = git_stopped(&["a.txt"], [b"a\n", b"left\n", b"right\n"]);
    let result = triad(fixture.root(), &["resolve", "--headless", "--commands", "c.jsonl"]);
    assert_eq!(
        outcome(&result),
        (
            Some(ERROR),
            String::new(),
            "triad-merge: --commands applies to one file: give resolve a PATH\n".to_owned()
        )
    );
}

#[test]
fn a_file_outside_any_repository_resolves_from_its_markers() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8Path::from_path(dir.path()).expect("UTF-8");
    std::fs::write(
        root.join("m.txt"),
        "<<<<<<< ours\nL\n||||||| anc\nB\n=======\nR\n>>>>>>> theirs\n",
    )
    .expect("written");
    let commands = root.join("c.jsonl");
    std::fs::write(&commands, r#"{"Pick":{"region":0,"sources":[{"Side":0}]}}"#).expect("written");
    let result = triad(root, &["resolve", "--headless", "m.txt", "--commands", commands.as_str()]);
    assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
    assert_eq!(read(root, "m.txt"), b"L\n");
}

/// Three conflicts, one unchanged line apart, so each is a region of its own: regions 1, 3
/// and 5.
const THREE_CONFLICTS: [&[u8]; 3] =
    [b"a\nb\nc\nd\ne\nf\ng\n", b"a\nL1\nc\nL2\ne\nL3\ng\n", b"a\nR1\nc\nR2\ne\nR3\ng\n"];
/// `THREE_CONFLICTS` with the first conflict resolved to the left, in every term.
const FIRST_TAKEN: [&[u8]; 3] =
    [b"a\nL1\nc\nd\ne\nf\ng\n", b"a\nL1\nc\nL2\ne\nL3\ng\n", b"a\nL1\nc\nR2\ne\nR3\ng\n"];
/// The first conflict to the left, the second to the right, the third to left then right.
const ALL_TAKEN: &[u8] = b"a\nL1\nc\nR2\ne\nL3\nR3\ng\n";
/// Commands that take the first of `THREE_CONFLICTS` to the left.
const TAKE_FIRST: &[u8] = br#"{"Pick":{"region":1,"sources":[{"Side":0}]}}"#;

#[test]
fn a_git_save_of_one_of_three_conflicts_reopens_with_the_two_left() {
    let (fixture, _) = git_stopped(&["a.txt"], THREE_CONFLICTS);
    let commands = fixture.write("commands.jsonl", TAKE_FIRST);
    let result =
        triad(fixture.root(), &["resolve", "--headless", "a.txt", "--commands", commands.as_str()]);
    assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
    // The stages keep all three conflicts; the file holds the resolution of the first.
    let [base, left, right] = THREE_CONFLICTS;
    assert_eq!(terms(&fixture.open(), "a.txt"), [file(left), file(right), file(base)]);
    // The next run opens from the file's markers, so the regions are those of `FIRST_TAKEN`.
    let commands = fixture.write(
        "commands.jsonl",
        br#"{"Pick":{"region":1,"sources":[{"Side":1}]}}
{"Pick":{"region":3,"sources":[{"Side":0},{"Side":1}]}}"#,
    );
    let result =
        triad(fixture.root(), &["resolve", "--headless", "a.txt", "--commands", commands.as_str()]);
    assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
    assert_eq!(read(fixture.root(), "a.txt"), ALL_TAKEN);
    assert_eq!(fixture.open().conflicts().expect("listed"), Vec::<camino::Utf8PathBuf>::new());
    assert_eq!(staged(&fixture, "a.txt"), ALL_TAKEN);
}

mod jj {
    use super::*;
    use vcs_jj::testing::{Held, Op, ToolOutcome};

    /// A jj conflict in `a.txt` between `left` and `right` over `a\n`, checked out to disk.
    /// Returns jj's labels for the base, left and right.
    fn jj_on_disk(left: &[u8], right: &[u8]) -> (vcs_jj::testing::Fixture, [String; 3]) {
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
        let l = fixture.commit_on(&[&base], "left", &[("a.txt", left)]);
        let r = fixture.commit_on(&[&base], "right", &[("a.txt", right)]);
        fixture.check_out(&[&l, &r]);
        fixture.check_out_to_disk();
        let label = |c: &vcs_jj::testing::Committed, summary: &str| {
            format!("{} {} \"{summary}\"", &c.change_id[..JJ_SHORT], &c.commit[..JJ_SHORT])
        };
        let labels = [label(&base, "base"), label(&l, "left"), label(&r, "right")];
        (fixture, labels)
    }

    #[test]
    fn without_a_path_each_jj_file_loads_after_the_save_before_it() {
        let fixture = vcs_jj::testing::Fixture::new();
        let files = |text: &'static [u8]| [("a.txt", text), ("b.txt", text)];
        let base = fixture.commit_on(&[], "base", &files(b"a\n"));
        let l = fixture.commit_on(&[&base], "left", &files(b"left\n"));
        let r = fixture.commit_on(&[&base], "right", &files(b"right\n"));
        fixture.check_out(&[&l, &r]);
        fixture.check_out_to_disk();
        let operations = fixture.operations();
        let result = triad(fixture.root(), &["resolve", "--headless"]);
        assert_eq!(outcome(&result), (Some(CONFLICTS), "a.txt\nb.txt\n".to_owned(), String::new()));
        let conflicted = ToolOutcome::Conflicted {
            sides: vec![b"left\n".to_vec(), b"right\n".to_vec()],
            bases: vec![b"a\n".to_vec()],
        };
        for path in ["a.txt", "b.txt"] {
            assert_eq!(fixture.snapshot_reads(path), conflicted, "{path}");
            assert_eq!(fixture.held_at(0, path), Held::Conflict, "{path}");
        }
        // Markers jj reads back as its conflict leave the tree as it was: no operation.
        assert_eq!(fixture.operations(), operations);
    }

    #[test]
    fn jj_reads_saved_markers_back_as_the_same_conflict() {
        // A marker-like run of 7 makes jj's markers 11 long. triad's own rule would give 8,
        // which jj would not read as markers.
        let (fixture, [base, left, right]) = jj_on_disk(b"<<<<<<< left\n", b"right\n");
        let operations = fixture.operations();
        let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
        assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
        let long = |c: char| c.to_string().repeat(11);
        assert_eq!(
            String::from_utf8(read(fixture.root(), "a.txt")).expect("UTF-8"),
            format!(
                "{} {left}\n<<<<<<< left\n{} {base}\na\n{}\nright\n{} {right}\n",
                long('<'),
                long('|'),
                long('='),
                long('>')
            )
        );
        assert_eq!(
            fixture.snapshot_reads("a.txt"),
            ToolOutcome::Conflicted {
                sides: vec![b"<<<<<<< left\n".to_vec(), b"right\n".to_vec()],
                bases: vec![b"a\n".to_vec()],
            }
        );
        // The markers read back as the conflict jj holds: its tree is unchanged, so no
        // operation is written.
        assert_eq!(fixture.operations(), operations);
        assert_eq!(fixture.held_at(0, "a.txt"), Held::Conflict);
    }

    /// The operation that records a save of `path`.
    fn save_op(path: &str) -> Op {
        Op { description: format!("save {path} in triad"), is_snapshot: false }
    }

    #[test]
    fn jj_reads_a_resolved_save_as_resolved() {
        let (fixture, _) = jj_on_disk(b"left\n", b"right\n");
        let commands = fixture
            .write("commands.jsonl", br#"{"Pick":{"region":0,"sources":[{"Side":0},{"Side":1}]}}"#);
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
        assert_eq!(
            fixture.snapshot_reads("a.txt"),
            ToolOutcome::Resolved(b"left\nright\n".to_vec())
        );
        // The save is its own operation; `jj undo` brings the conflict back.
        assert_eq!(fixture.operations()[0], save_op("a.txt"));
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(b"left\nright\n".to_vec()));
        assert_eq!(fixture.held_at(1, "a.txt"), Held::Conflict);
    }

    #[test]
    fn a_jj_file_resolved_by_hand_is_taken_in_and_left_alone() {
        let (fixture, _) = jj_on_disk(b"left\n", b"right\n");
        let edited: &[u8] = b"resolved by hand\n";
        fixture.write("a.txt", edited);
        let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
        assert_eq!(
            outcome(&result),
            (Some(ERROR), String::new(), "triad-merge: a.txt is not conflicted\n".to_owned())
        );
        assert_eq!(read(fixture.root(), "a.txt"), edited);
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(edited.to_vec()));
    }

    #[test]
    fn jjs_snapshot_follows_the_users_jj_config() {
        let (fixture, _) = jj_on_disk(b"left\n", b"right\n");
        fixture.write("new.txt", b"n\n");
        let dir = tempfile::tempdir().expect("a temporary directory");
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "snapshot.auto-track = \"none()\"\n")
            .expect("the config is written");
        let result =
            triad_with_jj_config(fixture.root(), &config, &["resolve", "--headless", "a.txt"]);
        assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
        assert_eq!(fixture.held_at(0, "new.txt"), Held::Absent);
        // jj's default tracks the new file.
        let result = triad(fixture.root(), &["resolve", "--headless", "a.txt"]);
        assert_eq!(result.status.code(), Some(CONFLICTS));
        assert_eq!(fixture.held_at(0, "new.txt"), Held::File(b"n\n".to_vec()));
    }

    /// `THREE_CONFLICTS` in a jj working-copy commit, checked out to disk. Returns the left
    /// commit.
    fn jj_three_conflicts() -> (vcs_jj::testing::Fixture, vcs_jj::testing::Committed) {
        let [base, left, right] = THREE_CONFLICTS;
        let fixture = vcs_jj::testing::Fixture::new();
        let b = fixture.commit_on(&[], "base", &[("a.txt", base)]);
        let l = fixture.commit_on(&[&b], "left", &[("a.txt", left)]);
        let r = fixture.commit_on(&[&b], "right", &[("a.txt", right)]);
        fixture.check_out(&[&l, &r]);
        fixture.check_out_to_disk();
        (fixture, l)
    }

    #[test]
    fn jj_records_a_save_that_resolves_one_of_three_conflicts() {
        let (fixture, _) = jj_three_conflicts();
        let commands = fixture.write("commands.jsonl", TAKE_FIRST);
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
        assert_eq!(fixture.operations()[0], save_op("a.txt"));
        assert_eq!(fixture.held_at(0, "a.txt"), Held::Conflict);
        // jj holds the two conflicts left; the resolved one is in every term.
        let [base, left, right] = FIRST_TAKEN;
        assert_eq!(terms(&fixture.open(), "a.txt"), [file(left), file(right), file(base)]);
        assert_eq!(fixture.open().conflicts().expect("listed"), ["a.txt"]);
    }

    #[test]
    fn a_partly_resolved_jj_commit_survives_a_move_and_resolves_in_a_child() {
        let (fixture, left) = jj_three_conflicts();
        let commands = fixture.write("commands.jsonl", TAKE_FIRST);
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(result.status.code(), Some(CONFLICTS));
        let partial = fixture.working_copy_commit();

        // `jj edit` to another commit writes its file; the partial resolution stays in history.
        fixture.edit(&left);
        fixture.check_out_to_disk();
        assert_eq!(read(fixture.root(), "a.txt"), THREE_CONFLICTS[1]);
        assert_eq!(fixture.held_in(&partial, "a.txt"), Held::Conflict);

        // `jj new` on the partial commit writes the two conflicts left as markers.
        fixture.check_out(&[&partial]);
        fixture.check_out_to_disk();
        let [base, left_text, right_text] = FIRST_TAKEN;
        assert_eq!(
            terms(&fixture.open(), "a.txt"),
            [file(left_text), file(right_text), file(base)]
        );
        // Regions 1 and 3: the first conflict is now part of the unchanged region at the start.
        let commands = fixture.write(
            "commands.jsonl",
            br#"{"Pick":{"region":1,"sources":[{"Side":1}]}}
{"Pick":{"region":3,"sources":[{"Side":0},{"Side":1}]}}"#,
        );
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
        assert_eq!(read(fixture.root(), "a.txt"), ALL_TAKEN);
        assert_eq!(fixture.operations()[0], save_op("a.txt"));
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(ALL_TAKEN.to_vec()));
        assert_eq!(fixture.parent_ids(0), std::slice::from_ref(&partial.commit));
        assert_eq!(fixture.open().conflicts().expect("listed"), Vec::<camino::Utf8PathBuf>::new());
        // The parent keeps its conflict: the child records the resolution.
        assert_eq!(fixture.held_in(&partial, "a.txt"), Held::Conflict);
    }

    #[test]
    fn jj_records_a_save_that_resolves_all_three_conflicts() {
        let (fixture, _) = jj_three_conflicts();
        let commands = fixture.write(
            "commands.jsonl",
            br#"{"Pick":{"region":1,"sources":[{"Side":0}]}}
{"Pick":{"region":3,"sources":[{"Side":1}]}}
{"Pick":{"region":5,"sources":[{"Side":0},{"Side":1}]}}"#,
        );
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(ALL_TAKEN.to_vec()));
        assert_eq!(fixture.held_at(1, "a.txt"), Held::Conflict);
        assert_eq!(fixture.open().conflicts().expect("listed"), Vec::<camino::Utf8PathBuf>::new());
    }

    /// `text` with a line appended, as a descendant's edit outside the conflicts changes it in
    /// every term. The last line, `g`, keeps the edit apart from the third conflict; an edit
    /// next to a conflict would conflict with its resolution.
    fn line_appended(text: &[u8]) -> Vec<u8> {
        [text, b"h\n"].concat()
    }

    #[test]
    fn a_jj_ancestor_resolved_in_two_saves_carries_each_to_its_descendants() {
        let (fixture, _) = jj_three_conflicts();
        let merge = fixture.working_copy_commit();
        // `jj new`, then a line appended below the conflicts and a new file, taken in by a
        // snapshot.
        fixture.check_out(&[&merge]);
        fixture.check_out_to_disk();
        let on_disk = read(fixture.root(), "a.txt");
        fixture.write("a.txt", &line_appended(&on_disk));
        fixture.write("b.txt", b"b\n");
        let snapshot = || {
            let mut repo = fixture.open().with_config(vcs_jj::config::ConfigEnv::default());
            repo.snapshot().expect("the snapshot is taken");
            fixture.working_copy_commit()
        };
        let child = snapshot();
        assert_eq!(fixture.held_in(&child, "a.txt"), Held::Conflict);
        // `jj new` again, with a file of its own: `jj edit` away from an empty commit abandons it.
        fixture.check_out(&[&child]);
        fixture.check_out_to_disk();
        fixture.write("c.txt", b"c\n");
        let grandchild = snapshot();

        // `jj edit` back to the merge, and resolve one conflict there.
        fixture.edit(&merge);
        fixture.check_out_to_disk();
        assert!(!fixture.root().join("b.txt").exists());
        let commands = fixture.write("commands.jsonl", TAKE_FIRST);
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
        assert_eq!(fixture.operations()[0], save_op("a.txt"));
        let [base, left, right] = FIRST_TAKEN;
        assert_eq!(terms(&fixture.open(), "a.txt"), [file(left), file(right), file(base)]);

        // The descendants rebased onto the save. jj rebases a conflict as tree algebra, not hunk
        // by hunk: the grandchild holds M' + (C − M), with the one equal pair of terms cancelled,
        // M's left side against M''s. Each hunk still reduces to the two conflicts left.
        let rebased = fixture.latest(&grandchild);
        assert_ne!(rebased.commit, grandchild.commit);
        fixture.edit(&rebased);
        fixture.check_out_to_disk();
        let conflict = fixture
            .open()
            .read_conflict(Utf8Path::new("a.txt"))
            .expect("the conflict reads")
            .expect("still conflicted");
        let sorted = |terms: Vec<vcs::ConflictTerm>| {
            let mut contents: Vec<TermContent> = terms.into_iter().map(|t| t.content).collect();
            contents.sort_by_key(|content| format!("{content:?}"));
            contents
        };
        let expected = |texts: &[&[u8]]| {
            let mut contents: Vec<TermContent> = texts.iter().map(|text| file(text)).collect();
            contents.sort_by_key(|content| format!("{content:?}"));
            contents
        };
        let [m_base, m_left, m_right] = THREE_CONFLICTS;
        let (c_left, c_right, c_base) =
            (line_appended(m_left), line_appended(m_right), line_appended(m_base));
        // Sides: M''s right, C's two sides, M's base. Bases: M''s base, C's base, M's right.
        assert_eq!(sorted(conflict.sides), expected(&[right, &c_left, &c_right, m_base]));
        assert_eq!(sorted(conflict.bases), expected(&[base, &c_base, m_right]));
        assert_eq!(read(fixture.root(), "b.txt"), b"b\n");

        // Back to the merge to resolve the rest.
        fixture.edit(&fixture.latest(&merge));
        fixture.check_out_to_disk();
        let commands = fixture.write(
            "commands.jsonl",
            br#"{"Pick":{"region":1,"sources":[{"Side":1}]}}
{"Pick":{"region":3,"sources":[{"Side":0},{"Side":1}]}}"#,
        );
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(ALL_TAKEN.to_vec()));
        let edited = Held::File(line_appended(ALL_TAKEN));
        assert_eq!(fixture.held_in(&fixture.latest(&child), "a.txt"), edited);
        assert_eq!(fixture.held_in(&fixture.latest(&child), "b.txt"), Held::File(b"b\n".to_vec()));
        assert_eq!(fixture.held_in(&fixture.latest(&grandchild), "a.txt"), edited);

        // `jj edit` to the grandchild writes the resolved file.
        fixture.edit(&fixture.latest(&grandchild));
        fixture.check_out_to_disk();
        assert_eq!(read(fixture.root(), "a.txt"), line_appended(ALL_TAKEN));
        assert_eq!(fixture.open().conflicts().expect("listed"), Vec::<camino::Utf8PathBuf>::new());
    }

    #[test]
    fn a_three_sided_jj_conflict_resolves_one_hunk_then_the_rest_in_a_child() {
        let fixture = vcs_jj::testing::Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\nb\nc\nd\ne\n")]);
        let sides = [b"a\nL1\nc\nL2\ne\n", b"a\nM1\nc\nM2\ne\n", b"a\nR1\nc\nR2\ne\n"]
            .map(|text| fixture.commit_on(&[&base], "side", &[("a.txt", text)]));
        fixture.check_out(&sides.iter().collect::<Vec<_>>());
        fixture.check_out_to_disk();
        // Regions 1 and 3; the first takes the middle side.
        let commands =
            fixture.write("commands.jsonl", br#"{"Pick":{"region":1,"sources":[{"Side":1}]}}"#);
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(CONFLICTS), String::new(), String::new()));
        let partial = fixture.working_copy_commit();
        let sides_left: [&[u8]; 3] =
            [b"a\nM1\nc\nL2\ne\n", b"a\nM1\nc\nM2\ne\n", b"a\nM1\nc\nR2\ne\n"];
        let bases_left: [&[u8]; 2] = [b"a\nM1\nc\nd\ne\n"; 2];
        let expected: Vec<TermContent> =
            sides_left.into_iter().chain(bases_left).map(file).collect();
        assert_eq!(terms(&fixture.open(), "a.txt"), expected);

        // `jj new`: the one conflict left is region 1 now.
        fixture.check_out(&[&partial]);
        fixture.check_out_to_disk();
        let commands =
            fixture.write("commands.jsonl", br#"{"Pick":{"region":1,"sources":[{"Side":2}]}}"#);
        let result = triad(
            fixture.root(),
            &["resolve", "--headless", "a.txt", "--commands", commands.as_str()],
        );
        assert_eq!(outcome(&result), (Some(RESOLVED), String::new(), String::new()));
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(b"a\nM1\nc\nR2\ne\n".to_vec()));
        assert_eq!(fixture.held_in(&partial, "a.txt"), Held::Conflict);
    }
}
