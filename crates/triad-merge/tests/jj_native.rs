//! The jj-native commands end to end (spec §7.5): what they print, what
//! they write into the repository, and their refusal outside jj.

use std::process::{Command, Output};

use camino::Utf8Path;

const BINARY: &str = env!("CARGO_BIN_EXE_triad-merge");
const RESOLVED: i32 = 0;
const CONFLICTS: i32 = 1;
const ERROR: i32 = 3;

static SCRATCH: std::sync::LazyLock<tempfile::TempDir> =
    std::sync::LazyLock::new(|| tempfile::tempdir().expect("a scratch directory"));

/// The binary in `dir`, with no settings file, no git config and no user jj config.
fn triad(dir: &Utf8Path, args: &[&str]) -> Output {
    triad_with_jj_config(dir, &SCRATCH.path().join("no-jjconfig.toml"), args)
}

/// The binary in `dir`, with no settings file, no git config and the user jj config at
/// `jj_config`.
fn triad_with_jj_config(dir: &Utf8Path, jj_config: &std::path::Path, args: &[&str]) -> Output {
    Command::new(BINARY)
        .env("TRIAD_LOG_DIR", SCRATCH.path())
        .env("TRIAD_CONFIG", SCRATCH.path().join("no-config.triad"))
        .env("GIT_CONFIG_GLOBAL", SCRATCH.path().join("no-gitconfig"))
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

#[test]
fn a_jj_native_command_in_a_git_repository_says_it_needs_jj() {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let stack = triad(fixture.root(), &["stack"]);
    assert_eq!(
        outcome(&stack),
        (Some(ERROR), String::new(), "triad-merge: stack needs a jj repository\n".to_owned())
    );
    let resolve = triad(fixture.root(), &["resolve", "a.txt", "-r", "HEAD", "--headless"]);
    assert_eq!(
        outcome(&resolve),
        (Some(ERROR), String::new(), "triad-merge: resolve -r needs a jj repository\n".to_owned())
    );
    let history = triad(fixture.root(), &["history", "a.txt"]);
    assert_eq!(
        outcome(&history),
        (Some(ERROR), String::new(), "triad-merge: history needs a jj repository\n".to_owned())
    );
    let absorb = triad(fixture.root(), &["absorb"]);
    assert_eq!(
        outcome(&absorb),
        (Some(ERROR), String::new(), "triad-merge: absorb needs a jj repository\n".to_owned())
    );
    let evolog = triad(fixture.root(), &["evolog", "--list"]);
    assert_eq!(
        outcome(&evolog),
        (Some(ERROR), String::new(), "triad-merge: evolog needs a jj repository\n".to_owned())
    );
    let since = triad(fixture.root(), &["diff", "--since", "HEAD", "--inspect"]);
    assert_eq!(
        outcome(&since),
        (
            Some(ERROR),
            String::new(),
            "triad-merge: diff --since needs a jj repository\n".to_owned()
        )
    );
    // The window forms list before a window opens, so they refuse the same way.
    for (args, command) in [
        (&["evolog"][..], "evolog"),
        (&["diff", "--since", "HEAD"], "diff --since"),
        (&["diff", "a.txt", "--since", "HEAD"], "diff --since"),
        (&["resolve", "a.txt", "--reuse", "--headless"], "resolve --reuse"),
        (&["resolve", "a.txt", "-r", "HEAD", "--reuse", "--headless"], "resolve --reuse"),
    ] {
        let message = format!("triad-merge: {command} needs a jj repository\n");
        assert_eq!(outcome(&triad(fixture.root(), args)), (Some(ERROR), String::new(), message));
    }
}

mod jj {
    use vcs_jj::testing::{Committed, Fixture, Held};

    use super::*;

    /// jj's shortest change ID prefix in these small repositories.
    const SHORT: usize = 8;

    /// `merge` of `left` and `right` conflicts in `a.txt`; `child` adds `b.txt`; the
    /// working copy sits on `child`, on disk.
    fn stacked() -> (Fixture, Committed, Committed) {
        let fixture = Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
        let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
        let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
        let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
        let child = fixture.commit_merged(&[&merge], "child", &[("b.txt", b"b\n")]);
        fixture.check_out(&[&child]);
        fixture.check_out_to_disk();
        (fixture, merge, child)
    }

    fn short(id: &str) -> &str {
        &id[..SHORT]
    }

    #[test]
    fn stack_lists_the_conflicted_commits_newest_first() {
        let (fixture, merge, child) = stacked();
        let wc = fixture.working_copy_commit();
        let (code, out, err) = outcome(&triad(fixture.root(), &["stack", "--list"]));
        let expected = format!(
            "{} {}\n  a.txt (inherited)\n{} {} child\n  a.txt (inherited)\n{} {} merge\n  a.txt\n",
            short(&wc.change_id),
            short(&wc.commit),
            short(&child.change_id),
            short(&child.commit),
            short(&merge.change_id),
            short(&merge.commit),
        );
        assert_eq!((code, out, err), (Some(CONFLICTS), expected, String::new()));
        let (code, out, _) = outcome(&triad(fixture.root(), &["stack", "--json"]));
        let listed: serde_json::Value = serde_json::from_str(&out).expect("JSON");
        assert_eq!(code, Some(CONFLICTS));
        assert_eq!(listed[0]["commit"], wc.commit.as_str());
        assert_eq!(listed[2]["commit"], merge.commit.as_str());
        assert_eq!(listed[0]["paths"][0]["inherited"], true);
        assert_eq!(listed[2]["paths"][0]["inherited"], false);
    }

    #[test]
    fn stack_without_conflicted_commits_opens_no_window_and_exits_0() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"a\n")]);
        fixture.check_out_to_disk();
        assert_eq!(
            outcome(&triad(fixture.root(), &["stack"])),
            (Some(RESOLVED), String::new(), String::new())
        );
    }

    #[test]
    fn stack_with_a_revset_jj_cannot_resolve_fails_before_any_window() {
        let (fixture, ..) = stacked();
        let (code, out, err) = outcome(&triad(fixture.root(), &["stack", "no_such_function()"]));
        assert_eq!((code, out.as_str()), (Some(ERROR), ""));
        assert!(err.starts_with("triad-merge: "), "{err}");
    }

    #[test]
    fn resolve_in_a_commit_writes_the_result_there_and_reports_the_descendants() {
        let (fixture, merge, child) = stacked();
        let commands = SCRATCH.path().join("pick-right.jsonl");
        std::fs::write(&commands, br#"{"Pick":{"region":0,"sources":[{"Side":1}]}}"#)
            .expect("the commands are written");
        let args = [
            "resolve",
            "a.txt",
            "-r",
            &merge.change_id,
            "--headless",
            "--commands",
            commands.to_str().expect("UTF-8"),
        ];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        let wc = fixture.working_copy_commit();
        assert_eq!((code, err.as_str()), (Some(RESOLVED), ""), "{out}");
        assert_eq!(
            out,
            format!(
                "resolved a.txt in {}\n  {}: a.txt -> none\n  {}: a.txt -> none\n",
                short(&merge.change_id),
                short(&child.change_id),
                short(&wc.change_id),
            )
        );
        assert_eq!(
            fixture.held_in(&fixture.latest(&merge), "a.txt"),
            Held::File(b"right\n".to_vec())
        );
        assert_eq!(std::fs::read(fixture.root().join("a.txt")).expect("reads"), b"right\n");
    }

    #[test]
    fn a_dry_run_reports_and_leaves_the_commit_conflicted() {
        let (fixture, merge, child) = stacked();
        let operations = fixture.operation_count();
        let commands = SCRATCH.path().join("pick-left.jsonl");
        std::fs::write(&commands, br#"{"Pick":{"region":0,"sources":[{"Side":0}]}}"#)
            .expect("the commands are written");
        let args = [
            "resolve",
            "a.txt",
            "-r",
            &merge.commit,
            "--headless",
            "--dry-run",
            "--commands",
            commands.to_str().expect("UTF-8"),
        ];
        let (code, out, _) = outcome(&triad(fixture.root(), &args));
        assert_eq!(code, Some(RESOLVED));
        assert_eq!(
            out.lines().take(2).collect::<Vec<_>>(),
            [
                format!("would resolve a.txt in {}", short(&merge.change_id)),
                format!("  {}: a.txt -> none", short(&child.change_id)),
            ]
        );
        assert_eq!(fixture.held_in(&merge, "a.txt"), Held::Conflict);
        assert_eq!(fixture.operation_count(), operations);
    }

    #[test]
    fn the_jj_native_commands_take_the_users_revset_aliases() {
        let (fixture, merge, _) = stacked();
        let dir = tempfile::tempdir().expect("a temporary directory");
        let config = dir.path().join("config.toml");
        let toml = format!("[revset-aliases]\n\"the_merge()\" = \"{}\"\n", merge.commit);
        std::fs::write(&config, toml).expect("the config is written");
        let run = |args: &[&str]| outcome(&triad_with_jj_config(fixture.root(), &config, args));
        assert_eq!(
            run(&["stack", "the_merge()", "--list"]),
            (
                Some(CONFLICTS),
                format!("{} {} merge\n  a.txt\n", short(&merge.change_id), short(&merge.commit)),
                String::new()
            )
        );
        assert_eq!(
            run(&["resolve", "a.txt", "-r", "the_merge()", "--headless"]),
            (
                Some(CONFLICTS),
                format!("a.txt in {} is unchanged\n", short(&merge.change_id)),
                String::new()
            )
        );
        // Without the user's config the alias names nothing.
        let (code, out, _) = outcome(&triad(fixture.root(), &["stack", "the_merge()"]));
        assert_eq!((code, out.as_str()), (Some(ERROR), ""));
    }

    #[test]
    fn the_automatic_merge_of_a_conflict_saves_its_markers_unchanged() {
        let (fixture, merge, _) = stacked();
        let args = ["resolve", "a.txt", "-r", &merge.change_id, "--headless"];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!(
            (code, out, err),
            (
                Some(CONFLICTS),
                format!("a.txt in {} is unchanged\n", short(&merge.change_id)),
                String::new()
            )
        );
        assert_eq!(fixture.held_in(&merge, "a.txt"), Held::Conflict);
    }

    #[test]
    fn history_lists_each_snapshot_newest_first() {
        let fixture = Fixture::new();
        fixture.check_out_to_disk();
        let mut ops = Vec::new();
        for bytes in [&b"1\n"[..], b"2\n"] {
            std::fs::write(fixture.root().join("a.txt"), bytes).expect("a.txt is written");
            let mut repo = vcs_jj::JjRepository::discover(fixture.root()).expect("the repository");
            vcs::Repository::snapshot(&mut repo).expect("the snapshot");
            ops.push(fixture.operation_ids()[0].clone());
        }
        let (code, out, err) = outcome(&triad(fixture.root(), &["history", "a.txt", "--list"]));
        assert_eq!((code, err.as_str()), (Some(RESOLVED), ""), "{out}");
        let starts: Vec<String> =
            out.lines().map(|l| l.split(' ').next().unwrap_or_default().to_owned()).collect();
        let short = |id: &str| id[..vcs_jj::SHORT_OPERATION].to_owned();
        assert_eq!(starts[..2], [short(&ops[1]), short(&ops[0])], "{out}");
        assert_eq!(out.lines().count(), 3, "{out}");
        assert!(out.lines().nth(2).is_some_and(|l| l.ends_with(" (absent)")), "{out}");
        let (_, json, _) = outcome(&triad(fixture.root(), &["history", "a.txt", "--json"]));
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        assert_eq!(value["versions"][0]["operation"], ops[1].as_str());
        assert_eq!(value["versions"][0]["state"], "file");
        let args = ["history", "a.txt", "--list", "--limit", "1"];
        let (code, out, _) = outcome(&triad(fixture.root(), &args));
        assert_eq!(code, Some(RESOLVED));
        assert_eq!(out.lines().count(), 1, "{out}");
        assert!(out.starts_with(&short(&ops[1])), "{out}");
    }

    #[test]
    fn history_of_a_path_outside_the_repository_fails_before_any_window() {
        let fixture = Fixture::new();
        fixture.check_out_to_disk();
        let outside = tempfile::tempdir().expect("a directory outside");
        let file = dunce::canonicalize(outside.path()).expect("its path").join("b.txt");
        std::fs::write(&file, b"b\n").expect("b.txt is written");
        let path = file.to_str().expect("a UTF-8 path");
        assert_eq!(
            outcome(&triad(fixture.root(), &["history", path])),
            (
                Some(ERROR),
                String::new(),
                format!("triad-merge: {path} is outside the repository\n")
            )
        );
    }

    #[test]
    fn absorb_lists_then_applies_the_hunks() {
        let fixture = Fixture::new();
        let one = fixture.commit_on(&[], "one", &[("a.txt", b"1\n2\n3\n")]);
        let two = fixture.commit_merged(&[&one], "two", &[("b.txt", b"x\ny\n")]);
        fixture.check_out(&[&two]);
        fixture.check_out_to_disk();
        std::fs::write(fixture.root().join("a.txt"), b"1\nTWO\n3\n").expect("written");
        std::fs::write(fixture.root().join("b.txt"), b"x\nY\n").expect("written");
        let (code, out, err) = outcome(&triad(fixture.root(), &["absorb", "--list"]));
        assert_eq!((code, err.as_str()), (Some(RESOLVED), ""), "{out}");
        let operation = &fixture.operation_ids()[0][..vcs_jj::SHORT_OPERATION];
        assert_eq!(
            out,
            format!(
                "at operation {operation}\n\
                 1 a.txt left 1..2 right 1..2 -> {} {}\n2 b.txt left 1..2 right 1..2 -> {} {}\n",
                short(&one.change_id),
                short(&one.commit),
                short(&two.change_id),
                short(&two.commit),
            )
        );
        let args = ["absorb", "--keep", "2", "--apply", "--listed-at", operation];
        let (code, out, _) = outcome(&triad(fixture.root(), &args));
        assert_eq!(code, Some(RESOLVED));
        assert_eq!(out.lines().nth(2), Some("absorbed 1 hunks into 1 commits"), "{out}");
        assert_eq!(
            fixture.held_in(&fixture.latest(&one), "a.txt"),
            Held::File(b"1\nTWO\n3\n".to_vec())
        );
        assert_eq!(fixture.held_in(&fixture.latest(&two), "b.txt"), Held::File(b"x\ny\n".to_vec()));
        assert_eq!(
            fixture.held_in(&fixture.working_copy_commit(), "b.txt"),
            Held::File(b"x\nY\n".to_vec())
        );
        // The apply moved the repository on: the listing's numbers no longer hold.
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (
                Some(ERROR),
                "",
                format!(
                    "triad-merge: checking the listing's operation: the repository changed since \
                     operation {operation}; list the hunks again\n"
                )
                .as_str()
            )
        );
    }

    #[test]
    fn absorb_with_nothing_to_absorb_opens_no_window_and_exits_0() {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"a\n")]);
        fixture.check_out_to_disk();
        assert_eq!(
            outcome(&triad(fixture.root(), &["absorb"])),
            (Some(RESOLVED), String::new(), String::new())
        );
    }

    #[test]
    fn absorb_into_a_revset_jj_cannot_resolve_fails_before_any_window() {
        let fixture = Fixture::new();
        fixture.check_out_to_disk();
        let args = ["absorb", "--into", "no_such_function()"];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, out.as_str()), (Some(ERROR), ""));
        assert!(err.starts_with("triad-merge: "), "{err}");
    }

    #[test]
    fn absorb_refuses_a_missing_hunk() {
        let fixture = Fixture::new();
        fixture.check_out_to_disk();
        let (code, out, err) = outcome(&triad(fixture.root(), &["absorb", "--keep", "1"]));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (Some(ERROR), "", "triad-merge: choosing a hunk: no hunk 1; there are 0\n")
        );
    }

    #[test]
    fn a_path_resolved_in_the_commit_is_refused() {
        let (fixture, merge, _) = stacked();
        let args = ["resolve", "b.txt", "-r", &merge.change_id, "--headless"];
        let (code, _, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(ERROR), "triad-merge: b.txt is not conflicted\n"));
    }

    /// `parent` holding `a.txt`; `child` on it adding `b.txt` and `c.txt`; then `parent`
    /// rewritten, which rebases `child`. The child before and after the rebase.
    fn rebased_child() -> (Fixture, Committed, Committed) {
        let fixture = Fixture::new();
        let parent = fixture.commit_on(&[], "parent", &[("a.txt", b"1\n")]);
        let child = fixture.commit_on(
            &[&parent],
            "child",
            &[("a.txt", b"1\n"), ("b.txt", b"x\n"), ("c.txt", b"c1\nc2\nc3\n")],
        );
        fixture.amend(&parent, &[("a.txt", b"0\n")]);
        let rebased = fixture.latest(&child);
        (fixture, child, rebased)
    }

    #[test]
    fn evolog_lists_a_changes_versions_newest_first() {
        let (fixture, old, new) = rebased_child();
        let ops = fixture.operation_ids();
        let short = |id: &str| id[..vcs_jj::SHORT_OPERATION].to_owned();
        let args = ["evolog", "-r", &new.change_id, "--list"];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(RESOLVED), ""), "{out}");
        let fields: Vec<(String, String, String)> = out
            .lines()
            .map(|l| {
                let mut words = l.split(' ');
                let mut next = || words.next().unwrap_or_default().to_owned();
                let (commit, op, _time) = (next(), next(), next());
                (commit, op, words.collect::<Vec<_>>().join(" "))
            })
            .collect();
        assert_eq!(
            fields,
            [
                (short(&new.commit), short(&ops[0]), "amend".to_owned()),
                (short(&old.commit), short(&ops[1]), "commit".to_owned()),
            ],
            "{out}"
        );
        let args = ["evolog", "-r", &new.change_id, "--json", "--limit", "1"];
        let (_, json, _) = outcome(&triad(fixture.root(), &args));
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        assert_eq!(value["change_id"], new.change_id.as_str());
        assert_eq!(value["versions"].as_array().map(Vec::len), Some(1));
        assert_eq!(value["versions"][0]["commit"], new.commit.as_str());
        assert_eq!(value["versions"][0]["operation"], ops[0].as_str());
        assert_eq!(value["versions"][0]["description"], "amend");
        assert_eq!(value["versions"][0]["conflicted"], false);
    }

    #[test]
    fn diff_since_a_version_a_rebase_made_lists_nothing() {
        let (fixture, old, new) = rebased_child();
        let args = ["diff", "--since", &old.commit, "-r", &new.change_id, "--inspect"];
        assert_eq!(
            outcome(&triad(fixture.root(), &args)),
            (Some(RESOLVED), "files: 0\n".to_owned(), String::new())
        );
    }

    #[test]
    fn diff_since_lists_the_changes_own_edits_with_renames() {
        let (fixture, old, rebased) = rebased_child();
        let new = fixture
            .amend(&rebased, &[("a.txt", b"0\n"), ("b.txt", b"y\n"), ("d.txt", b"c1\nc2\nc3\n")]);
        let args = ["diff", "--since", &old.commit, "-r", &new.commit, "--inspect"];
        assert_eq!(
            outcome(&triad(fixture.root(), &args)),
            (
                Some(CONFLICTS),
                "modified text b.txt\nrenamed text c.txt => d.txt\nfiles: 2\n".to_owned(),
                String::new()
            )
        );
        let json_args = ["diff", "--since", &old.commit, "-r", &new.commit, "--inspect", "--json"];
        let (_, json, _) = outcome(&triad(fixture.root(), &json_args));
        let value: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        assert_eq!(value["command"], "diff-changes");
        assert_eq!(value["changes"], 2);
    }

    /// `child`, the working-copy commit on disk, on `parent`; `parent` rewritten so `child`
    /// conflicts in `a.txt`, sides `3\n` and `2\n`; `child` resolved as `4\n`; then the
    /// conflict brought back, as `jj restore --from` the conflicted version does. The child.
    fn conflict_back(fixture: &Fixture) -> Committed {
        let parent = fixture.commit_on(&[], "parent", &[("a.txt", b"1\n")]);
        let child = fixture.commit_on(&[&parent], "child", &[("a.txt", b"2\n")]);
        fixture.edit(&child);
        fixture.amend(&parent, &[("a.txt", b"3\n")]);
        let conflicted = fixture.latest(&child);
        let resolved = fixture.amend(&conflicted, &[("a.txt", b"4\n")]);
        fixture.restore(&resolved, &conflicted);
        fixture.check_out_to_disk();
        fixture.latest(&child)
    }

    #[test]
    fn resolve_reuse_takes_the_earlier_resolution_and_records_the_save() {
        let fixture = Fixture::new();
        conflict_back(&fixture);
        let args = ["resolve", "a.txt", "--reuse", "--headless"];
        assert_eq!(
            outcome(&triad(fixture.root(), &args)),
            (Some(RESOLVED), "reused 1 earlier resolution\n".to_owned(), String::new())
        );
        let on_disk = std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads");
        assert_eq!(on_disk, b"4\n");
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(b"4\n".to_vec()));
    }

    #[test]
    fn resolve_reuse_dry_run_reports_and_writes_nothing() {
        let fixture = Fixture::new();
        conflict_back(&fixture);
        let before = std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads");
        let operations = fixture.operation_count();
        let args = ["resolve", "a.txt", "--reuse", "--headless", "--dry-run"];
        assert_eq!(
            outcome(&triad(fixture.root(), &args)),
            (Some(RESOLVED), "would reuse 1 earlier resolution\n".to_owned(), String::new())
        );
        assert_eq!(std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads"), before);
        assert_eq!(fixture.operation_count(), operations, "nothing written");
    }

    #[test]
    fn resolve_reuse_runs_before_the_commands() {
        let fixture = Fixture::new();
        conflict_back(&fixture);
        let commands = fixture.root().join("commands.jsonl");
        // The undo takes back the reuse, so the conflict stays.
        std::fs::write(&commands, "\"Undo\"\n").expect("the commands are written");
        let args = ["resolve", "a.txt", "--reuse", "--headless", "--commands", commands.as_str()];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (Some(CONFLICTS), "reused 1 earlier resolution\n", "")
        );
        assert_eq!(fixture.held_at(0, "a.txt"), Held::Conflict);
    }

    #[test]
    fn resolve_reuse_with_no_earlier_resolution_reuses_none_and_leaves_the_conflict() {
        let (fixture, merge, _) = stacked();
        let args = ["resolve", "a.txt", "-r", &merge.change_id, "--reuse", "--headless"];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(CONFLICTS), ""), "{out}");
        assert_eq!(out.lines().next(), Some("reused 0 earlier resolutions"));
    }

    #[test]
    fn resolve_reuse_in_a_commit_writes_the_resolution_into_it() {
        let fixture = Fixture::new();
        let child = conflict_back(&fixture);
        fixture.check_out(&[&child]);
        let args = ["resolve", "a.txt", "-r", "@-", "--reuse", "--headless"];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(RESOLVED), ""), "{out}");
        let change = &child.change_id[..SHORT];
        let heading: Vec<_> = out.lines().take(2).collect();
        assert_eq!(
            heading,
            ["reused 1 earlier resolution", &format!("resolved a.txt in {change}")]
        );
        assert_eq!(fixture.held_in(&fixture.latest(&child), "a.txt"), Held::File(b"4\n".to_vec()));
    }

    #[test]
    fn diff_since_a_revision_naming_no_commit_fails() {
        let (fixture, _, _) = rebased_child();
        let message = "triad-merge: unknown revision none()\n".to_owned();
        // The window forms load before a window opens, so they fail the same way.
        for args in [
            &["diff", "--since", "none()", "--inspect"][..],
            &["diff", "--since", "none()"],
            &["diff", "a.txt", "--since", "none()"],
            &["evolog", "-r", "none()"],
        ] {
            let failed = outcome(&triad(fixture.root(), args));
            assert_eq!(failed, (Some(ERROR), String::new(), message.clone()), "{args:?}");
        }
    }
}
