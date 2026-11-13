//! The jj-native commands end to end (`docs/PLAN-jj-native.md`): what they print, what
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
        let (code, out, err) = outcome(&triad(fixture.root(), &["stack"]));
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
