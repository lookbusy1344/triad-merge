//! triad as git's and jj's merge and diff tool, end to end (spec §7.1, PLAN.md §A7.3).
//!
//! The entries come from `install`. git runs a tool's `cmd` with `sh -c` and its variables;
//! the tests do the same with a stub program that records its arguments, then parse them with
//! triad's own parser. jj's merge path runs through `vcs_jj::testing::Fixture::resolve`, which
//! follows `jj resolve` through jj-lib, with the real binary in `--headless` mode in place of
//! the window. What a window adds is checked by hand (`docs/release-checklist.md`).

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use merge_cli::args::{Invocation, parse};
use merge_config::settings::Choice;
use merge_session::install;
use merge_session::invocation::Window;
use vcs_jj::testing::{Fixture, MergeTool, ToolOutcome};

const BINARY: &str = env!("CARGO_BIN_EXE_triad-merge");
const ERROR_EXIT: i32 = 3;

/// The binary, logging into a directory that outlives the process but not the test run, and
/// reading a settings file that does not exist, so tests leave the user's directories alone.
fn command() -> Command {
    static LOGS: std::sync::LazyLock<tempfile::TempDir> =
        std::sync::LazyLock::new(|| tempfile::tempdir().expect("log dir"));
    let mut command = Command::new(BINARY);
    command
        .env("TRIAD_LOG_DIR", LOGS.path())
        .env("TRIAD_CONFIG", LOGS.path().join("no-config.triad"))
        .env_remove("TRIAD_LOG");
    command
}

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

/// triad's reading of a command line, program name first.
fn parsed(args: impl IntoIterator<Item = OsString>) -> Invocation {
    parse(std::iter::once(OsString::from("triad-merge")).chain(args))
        .expect("the tool's command line parses")
}

mod jj {
    use super::*;

    /// The `[merge-tools.triad]` table `install --jj` writes.
    fn tool_table() -> toml::Table {
        let dir = dir();
        let path = dir.path().join("config.toml");
        install::jj(&path, Path::new(BINARY)).expect("the install succeeds");
        let mut config: toml::Table =
            std::fs::read_to_string(&path).expect("the config reads").parse().expect("TOML");
        let tools = config.remove("merge-tools").expect("merge-tools");
        tools.get("triad").and_then(toml::Value::as_table).expect("the triad tool").clone()
    }

    fn strings(table: &toml::Table, key: &str) -> Vec<String> {
        table[key]
            .as_array()
            .expect("an array")
            .iter()
            .map(|v| v.as_str().expect("a string").to_owned())
            .collect()
    }

    /// The installed tool, run headless: the one change from what jj runs.
    fn merge_tool() -> MergeTool {
        let table = tool_table();
        assert_eq!(table["program"].as_str(), Some(BINARY));
        let mut command = command();
        command.arg("--headless");
        MergeTool {
            command,
            merge_args: strings(&table, "merge-args"),
            edits_conflict_markers: table["merge-tool-edits-conflict-markers"]
                .as_bool()
                .expect("a boolean"),
            conflict_exit_codes: table["merge-conflict-exit-codes"]
                .as_array()
                .expect("an array")
                .iter()
                .map(|v| i32::try_from(v.as_integer().expect("an integer")).expect("small"))
                .collect(),
        }
    }

    fn unchanged(base: &[u8], left: &[u8], right: &[u8]) -> ToolOutcome {
        ToolOutcome::Conflicted {
            sides: vec![left.to_vec(), right.to_vec()],
            bases: vec![base.to_vec()],
        }
    }

    #[test]
    fn an_unresolved_conflict_stays_conflicted_in_jj() {
        let (base, left, right) = (b"a\nb\nc\n", b"a\nL\nc\n", b"a\nR\nc\n");
        let outcome = Fixture::new().resolve("src/a.txt", [base, left, right], merge_tool());
        assert_eq!(outcome, unchanged(base, left, right));
    }

    #[test]
    fn clean_regions_beside_a_conflict_are_resolved_as_jj_resolves_them() {
        let base = b"1\n2\n3\n4\n5\n6\n7\n";
        let left = b"1\nL\n3\n4\n5\n6\n7\n";
        let right = b"1\nR\n3\n4\n5\nR6\n7\n";
        let outcome = Fixture::new().resolve("a.txt", [base, left, right], merge_tool());
        // jj compares the parsed hunks with its own; equal hunks keep the original conflict.
        assert_eq!(outcome, unchanged(base, left, right));
    }

    #[test]
    fn marker_like_lines_lengthen_the_markers_on_both_sides() {
        let base = b"x\n<<<<<<< not a marker\nb\n";
        let left = b"x\n<<<<<<< not a marker\nL\n";
        let right = b"x\n<<<<<<< not a marker\nR\n";
        let outcome = Fixture::new().resolve("a.txt", [base, left, right], merge_tool());
        // jj materialises at 7 + 4 and parses only markers that long, which triad learns
        // from $marker_length; shorter markers would leave exit 1 without valid markers,
        // InvalidConflictMarkers.
        assert_eq!(outcome, unchanged(base, left, right));
    }

    #[test]
    fn jj_s_marker_length_reaches_triad_when_only_jj_counts_the_line() {
        // jj lengthens markers past any marker-like line, `+` runs included; triad's
        // diff3 rule counts only `<`, `|`, `=` and `>`, so its own rule would not grow them.
        let base = b"x\n+++++++ added\nb\n";
        let left = b"x\n+++++++ added\nL\n";
        let right = b"x\n+++++++ added\nR\n";
        let outcome = Fixture::new().resolve("a.txt", [base, left, right], merge_tool());
        assert_eq!(outcome, unchanged(base, left, right));
    }

    #[test]
    fn an_error_exit_is_an_abort_to_jj_not_a_conflict() {
        let mut tool = merge_tool();
        tool.command.args(["--diff-algorithm", "bogus"]);
        let outcome = Fixture::new().resolve("a.txt", [b"a\n", b"L\n", b"R\n"], tool);
        assert_eq!(outcome, ToolOutcome::Aborted(Some(ERROR_EXIT)));
    }

    fn vars(entries: &[(&'static str, &str)]) -> HashMap<&'static str, OsString> {
        entries.iter().map(|&(name, value)| (name, OsString::from(value))).collect()
    }

    #[test]
    fn jj_s_merge_arguments_parse_as_a_titled_merge() {
        let args = vcs_jj::testing::interpolate(
            &strings(&tool_table(), "merge-args"),
            &vars(&[
                ("base", "/t/base_a.rs"),
                ("left", "/t/left_a.rs"),
                ("right", "/t/right_a.rs"),
                ("output", "/t/output_a.rs"),
                ("path", "src/a.rs"),
                ("marker_length", "9"),
            ]),
        );
        let Invocation::Window(Window::Merge(merge)) = parsed(args) else {
            panic!("a merge window");
        };
        assert_eq!(merge.files.base, PathBuf::from("/t/base_a.rs"));
        assert_eq!(merge.files.left, PathBuf::from("/t/left_a.rs"));
        assert_eq!(merge.files.right, PathBuf::from("/t/right_a.rs"));
        assert_eq!(merge.output, PathBuf::from("/t/output_a.rs"));
        assert_eq!(merge.title.as_deref(), Some("src/a.rs"));
        assert_eq!(merge.options.marker_length, 9);
        assert_eq!(merge.semantics.choice, Some(Choice::Jj));
    }

    #[test]
    fn jj_s_diff_arguments_parse_as_a_two_way_diff() {
        let args = vcs_jj::testing::interpolate(
            &strings(&tool_table(), "diff-args"),
            &vars(&[("left", "left/a.rs"), ("right", "right/a.rs")]),
        );
        let Invocation::Window(Window::Diff(diff)) = parsed(args) else {
            panic!("a diff window");
        };
        assert_eq!(diff.left, PathBuf::from("left/a.rs"));
        assert_eq!(diff.right, PathBuf::from("right/a.rs"));
    }
}

/// git runs `cmd` through `sh`; Git for Windows does too, but this suite needs a Unix `sh`.
#[cfg(unix)]
mod git {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    /// A program that writes its arguments, NUL-terminated, to `$TRIAD_ARGV`.
    const STUB: &str =
        "#!/bin/sh\nfor arg in \"$@\"; do printf '%s\\0' \"$arg\"; done > \"$TRIAD_ARGV\"\n";
    const EXECUTABLE: u32 = 0o755;

    /// The installed `key`, for a stub program in a directory whose name `sh` would split or
    /// end a quote at.
    fn installed(dir: &Path, key: &str) -> (String, PathBuf) {
        let bin = dir.join("tool's dir");
        std::fs::create_dir_all(&bin).expect("the stub's directory");
        let stub = bin.join("triad merge");
        std::fs::write(&stub, STUB).expect("the stub is written");
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(EXECUTABLE))
            .expect("the stub is executable");
        let config = dir.join("gitconfig");
        install::git(&config, &stub).expect("the install succeeds");
        let text = std::fs::read_to_string(&config).expect("the config reads");
        let file = gix::config::File::try_from(text.as_str()).expect("git parses it");
        let value = file.string(key).expect("the key is set").to_string();
        (value, stub)
    }

    /// Run `cmd` as git does, with `vars` set, and give the program's arguments.
    fn run(dir: &Path, cmd: &str, vars: &[(&str, &Path)]) -> Vec<OsString> {
        let argv = dir.join("argv");
        let status = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .envs(vars.iter().copied())
            .env("TRIAD_ARGV", &argv)
            .status()
            .expect("sh runs");
        assert!(status.success(), "{cmd}: {status}");
        let bytes = std::fs::read(&argv).expect("the stub ran");
        bytes
            .split(|&b| b == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| OsString::from(String::from_utf8(arg.to_vec()).expect("UTF-8")))
            .collect()
    }

    #[test]
    fn git_mergetool_runs_a_merge_of_its_four_files() {
        let dir = dir();
        let (cmd, _) = installed(dir.path(), "mergetool.triad.cmd");
        let file = |name: &str| dir.path().join(format!("my file_{name}_1234.txt"));
        let (base, local, remote, merged) =
            (file("BASE"), file("LOCAL"), file("REMOTE"), dir.path().join("my file.txt"));
        let args = run(
            dir.path(),
            &cmd,
            &[("BASE", &base), ("LOCAL", &local), ("REMOTE", &remote), ("MERGED", &merged)],
        );
        let Invocation::Window(Window::Merge(merge)) = parsed(args) else {
            panic!("a merge window");
        };
        assert_eq!(merge.files.base, base);
        assert_eq!(merge.files.left, local);
        assert_eq!(merge.files.right, remote);
        assert_eq!(merge.output, merged);
        assert_eq!(merge.semantics.choice, Some(Choice::Git));
    }

    #[test]
    fn git_difftool_runs_a_two_way_diff() {
        let dir = dir();
        let (cmd, _) = installed(dir.path(), "difftool.triad.cmd");
        let local = dir.path().join("a b.txt");
        let remote = dir.path().join("tmp 'x'/a b.txt");
        // git sets `MERGED` to the file's path in the repository.
        let merged = Path::new("src/a b.txt");
        let args =
            run(dir.path(), &cmd, &[("LOCAL", &local), ("REMOTE", &remote), ("MERGED", merged)]);
        let Invocation::Window(Window::Diff(diff)) = parsed(args) else {
            panic!("a diff window");
        };
        assert_eq!(diff.left, local);
        assert_eq!(diff.right, remote);
        assert_eq!(diff.title.as_deref(), Some("src/a b.txt"));
        // git's left side is a temporary copy it deletes after the run (spec §7.1).
        assert_eq!((diff.readonly_left, diff.readonly_right), (true, false));
    }
}
