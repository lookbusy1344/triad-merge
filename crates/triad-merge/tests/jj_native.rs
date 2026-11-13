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

    #[test]
    fn a_path_resolved_in_the_commit_is_refused() {
        let (fixture, merge, _) = stacked();
        let args = ["resolve", "b.txt", "-r", &merge.change_id, "--headless"];
        let (code, _, err) = outcome(&triad(fixture.root(), &args));
        assert_eq!((code, err.as_str()), (Some(ERROR), "triad-merge: b.txt is not conflicted\n"));
    }
}
