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
    Command::new(BINARY)
        .env("TRIAD_LOG_DIR", SCRATCH.path())
        .env("TRIAD_CONFIG", SCRATCH.path().join("no-config.triad"))
        .env("GIT_CONFIG_GLOBAL", SCRATCH.path().join("no-gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("JJ_CONFIG", SCRATCH.path().join("no-jjconfig.toml"))
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
    let evolog = triad(fixture.root(), &["diff", "a.txt", "--evolog", "--inspect"]);
    assert_eq!(
        outcome(&evolog),
        (
            Some(ERROR),
            String::new(),
            "triad-merge: diff --evolog needs a jj repository\n".to_owned()
        )
    );
    let absorb = triad(fixture.root(), &["absorb"]);
    assert_eq!(
        outcome(&absorb),
        (Some(ERROR), String::new(), "triad-merge: absorb needs a jj repository\n".to_owned())
    );
    let diverge = triad(fixture.root(), &["diverge", "HEAD"]);
    assert_eq!(
        outcome(&diverge),
        (Some(ERROR), String::new(), "triad-merge: diverge needs a jj repository\n".to_owned())
    );
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
    fn stack_lists_the_conflicted_commits_parents_first() {
        let (fixture, merge, child) = stacked();
        let wc = fixture.working_copy_commit();
        let (code, out, err) = outcome(&triad(fixture.root(), &["stack", "--list"]));
        let expected = format!(
            "{} {} merge\n  a.txt\n{} {} child\n  a.txt (inherited)\n{} {}\n  a.txt (inherited)\n",
            short(&merge.change_id),
            short(&merge.commit),
            short(&child.change_id),
            short(&child.commit),
            short(&wc.change_id),
            short(&wc.commit),
        );
        assert_eq!((code, out, err), (Some(CONFLICTS), expected, String::new()));
        let (code, out, _) = outcome(&triad(fixture.root(), &["stack", "--json"]));
        let listed: serde_json::Value = serde_json::from_str(&out).expect("JSON");
        assert_eq!(code, Some(CONFLICTS));
        assert_eq!(listed[0]["commit"], merge.commit.as_str());
        assert_eq!(listed[2]["paths"][0]["inherited"], true);
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

    /// A change holding five lines in `a.txt`, rewritten two ways: `one` and `two`.
    fn divergent(one: &[u8], two: &[u8]) -> (Fixture, Committed, Vec<Committed>) {
        let fixture = Fixture::new();
        let change = fixture.commit_on(&[], "change", &[("a.txt", b"1\n2\n3\n4\n5\n")]);
        let copies = fixture
            .diverge(&change, &[("change", &[("a.txt", one)]), ("change", &[("a.txt", two)])]);
        (fixture, change, copies)
    }

    /// The report's first line when the copy at `kept` stays and the other goes.
    fn merged_line(change: &Committed, copies: &[Committed], kept: usize) -> String {
        format!(
            "merged change {}: kept {}, abandoned {} over {}",
            short(&change.change_id),
            short(&copies[kept].commit),
            short(&copies[1 - kept].commit),
            short(&change.commit),
        )
    }

    #[test]
    fn diverge_merges_the_copies_and_names_them() {
        let (fixture, change, copies) = divergent(b"ONE\n2\n3\n4\n5\n", b"1\n2\n3\n4\nFIVE\n");
        let (code, out, err) = outcome(&triad(fixture.root(), &["diverge", &change.change_id]));
        assert_eq!((code, err.as_str()), (Some(RESOLVED), ""), "{out}");
        let first = out.lines().next().unwrap_or_default();
        assert!(
            [merged_line(&change, &copies, 0), merged_line(&change, &copies, 1)]
                .contains(&first.to_owned()),
            "{out}"
        );
        assert_eq!(out.lines().count(), 1, "{out}");
        let visible = fixture.visible(&change);
        assert_eq!(visible.len(), 1);
        assert_eq!(
            fixture.held_in(&visible[0], "a.txt"),
            Held::File(b"ONE\n2\n3\n4\nFIVE\n".to_vec())
        );
    }

    #[test]
    fn diverge_with_a_conflict_names_it_and_exits_one() {
        let (fixture, change, _) = divergent(b"ONE\n2\n3\n4\n5\n", b"uno\n2\n3\n4\n5\n");
        let (code, out, _) = outcome(&triad(fixture.root(), &["diverge", &change.change_id]));
        assert_eq!(code, Some(CONFLICTS));
        assert_eq!(out.lines().nth(1), Some("  conflicted: a.txt"), "{out}");
        assert_eq!(fixture.held_in(&fixture.visible(&change)[0], "a.txt"), Held::Conflict);
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

    /// A change that edits line 4, rebased by an amend of its base at line 1, then
    /// amended itself at line 5 when `amend` holds. Returns the change now.
    fn evolved(amend: bool) -> (Fixture, Committed) {
        let fixture = Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", b"1\n2\n3\n4\n5\n")]);
        let change = fixture.commit_on(&[&base], "change", &[("a.txt", b"1\n2\n3\nFOUR\n5\n")]);
        fixture.amend(&base, &[("a.txt", b"ONE\n2\n3\n4\n5\n")]);
        if amend {
            fixture.amend(&fixture.latest(&change), &[("a.txt", b"ONE\n2\n3\nFOUR\nFIVE\n")]);
        }
        let now = fixture.latest(&change);
        (fixture, now)
    }

    #[test]
    fn evolog_inspect_shows_the_amend_and_not_the_rebase() {
        let (fixture, now) = evolved(true);
        let args = ["diff", "a.txt", "--evolog", "--rev", &now.commit, "--inspect"];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(CONFLICTS), ""), "{out}");
        assert_eq!(
            out,
            "unchanged left 0..4 right 0..4\nchanged left 4..5 right 4..5\nchanges: 1\n"
        );
        let (fixture, now) = evolved(false);
        let args = ["diff", "a.txt", "--evolog", "--rev", &now.commit, "--inspect"];
        let (code, out, _) = outcome(&triad(fixture.root(), &args));
        assert_eq!(
            (code, out.as_str()),
            (Some(RESOLVED), "unchanged left 0..5 right 0..5\nchanges: 0\n")
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
        let (code, out, err) = outcome(&triad(fixture.root(), &["absorb"]));
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
    fn absorb_refuses_a_missing_hunk() {
        let fixture = Fixture::new();
        fixture.check_out_to_disk();
        let (code, out, err) = outcome(&triad(fixture.root(), &["absorb", "--keep", "1"]));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (Some(ERROR), "", "triad-merge: choosing a hunk: no hunk 1; there are 0\n")
        );
    }

    /// The working copy on a merge of three parents that each changed `a.txt` from `a`:
    /// sides `one`, `two`, `three`, checked out to disk.
    fn three_sides() -> (Fixture, Committed) {
        let fixture = Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
        let parents: Vec<Committed> = ["one", "two", "three"]
            .into_iter()
            .map(|name| {
                fixture.commit_on(&[&base], name, &[("a.txt", format!("{name}\n").as_bytes())])
            })
            .collect();
        let parents: Vec<&Committed> = parents.iter().collect();
        fixture.check_out(&parents);
        fixture.check_out_to_disk();
        let wc = fixture.working_copy_commit();
        (fixture, wc)
    }

    #[test]
    fn a_resolved_pair_in_the_working_copy_leaves_two_sides_on_disk() {
        let (fixture, wc) = three_sides();
        let commands = SCRATCH.path().join("pair-pick-second.jsonl");
        std::fs::write(&commands, br#"{"Pick":{"region":0,"sources":[{"Side":1}]}}"#)
            .expect("the commands are written");
        let args = [
            "resolve",
            "a.txt",
            "--pair",
            "2",
            "--headless",
            "--commands",
            commands.to_str().expect("UTF-8"),
        ];
        let (code, out, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(RESOLVED), ""), "{out}");
        assert_eq!(out, format!("saved with conflicts a.txt in {}\n", short(&wc.change_id)));
        let on_disk =
            String::from_utf8(std::fs::read(fixture.root().join("a.txt")).expect("reads"))
                .expect("UTF-8");
        assert!(on_disk.contains("one\n") && on_disk.contains("three\n"), "{on_disk}");
        assert!(!on_disk.contains("two\n"), "{on_disk}");
        assert_eq!(fixture.held_in(&fixture.working_copy_commit(), "a.txt"), Held::Conflict);
    }

    #[test]
    fn a_pair_whose_merge_conflicts_writes_nothing() {
        let (fixture, _) = three_sides();
        let operations = fixture.operation_count();
        let (code, out, _) =
            outcome(&triad(fixture.root(), &["resolve", "a.txt", "--pair", "1", "--headless"]));
        assert_eq!(code, Some(CONFLICTS));
        assert_eq!(out, "a.txt is unchanged: pair 1 has conflicts\n");
        assert_eq!(fixture.operation_count(), operations);
    }

    #[test]
    fn a_pair_out_of_range_is_refused() {
        let (fixture, _) = three_sides();
        let (code, out, err) =
            outcome(&triad(fixture.root(), &["resolve", "a.txt", "--pair", "3", "--headless"]));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (Some(ERROR), "", "triad-merge: pair 3 needs sides 3 and 4; the conflict has 3\n")
        );
    }

    #[test]
    fn a_path_resolved_in_the_commit_is_refused() {
        let (fixture, merge, _) = stacked();
        let args = ["resolve", "b.txt", "-r", &merge.change_id, "--headless"];
        let (code, _, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(ERROR), "triad-merge: b.txt is not conflicted\n"));
    }
}
