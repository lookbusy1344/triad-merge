//! `triad-merge --headless` on the hand-written fixtures in `fixtures/core`, and its errors.
//!
//! Each fixture directory holds `base`, `left`, `right`, the `expected` output bytes, the
//! `exit` code, and optionally `args`, one extra argument per line, and `config`, the config
//! file the merge reads.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BINARY: &str = env!("CARGO_BIN_EXE_triad-merge");

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
/// Fixture directories in `fixtures/core`. A new one must be added here.
const CASES: [&str; 22] = [
    "add-add-conflict",
    "add-add-takes-left-encoding",
    "all-empty",
    "clean-merge",
    "conflict-bare-markers",
    "conflict-crlf",
    "conflict-labels",
    "conflict-left-label-only",
    "conflict-no-final-newline",
    "deep-refine-splits-at-a-shared-brace",
    "line-ending-base",
    "line-ending-lf",
    "marker-length-9",
    "same-change-both-sides",
    "syntax-boundaries-keep-a-shared-brace-whole",
    "syntax-boundaries-keep-a-string-word-merge-whole",
    "syntax-boundaries-need-a-known-language",
    "utf16le-bom-kept",
    "utf8-bom-kept",
    "whitespace-none-adjacent-conflict",
    "whitespace-trailing-avoids-conflict",
    "word-merge-inside-a-string",
];
const ERROR_EXIT: i32 = 3;
/// What git or jj may leave in the output file before calling the tool. triad ignores it.
const STALE_OUTPUT: &str = "<<<<<<< stale\n";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/core")
}

fn triad(args: &[&Path]) -> Output {
    command().args(args).output().expect("binary runs")
}

fn stderr(result: &Output) -> String {
    String::from_utf8_lossy(&result.stderr).into_owned()
}

#[test]
fn a_huge_marker_length_is_a_usage_error_and_writes_nothing() {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo");
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.txt");
    let length = usize::MAX.to_string();
    let result = triad(&[
        &demo.join("base.txt"),
        &demo.join("left.txt"),
        &demo.join("right.txt"),
        Path::new("-o"),
        &output,
        Path::new("--marker-length"),
        Path::new(&length),
    ]);
    assert_eq!(result.status.code(), Some(ERROR_EXIT), "{}", stderr(&result));
    assert!(stderr(&result).contains("must be at most 65536"), "{}", stderr(&result));
    assert!(!output.exists());
}

#[test]
fn every_fixture_directory_is_listed() {
    let mut found: Vec<String> = fs::read_dir(fixtures())
        .expect("fixtures list")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert_eq!(found, CASES);
}

#[test]
fn headless_merges_match_the_fixtures() {
    for case in CASES {
        let dir = fixtures().join(case);
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = tmp.path().join("out");
        fs::write(&output, STALE_OUTPUT).expect("stale output");
        let extra = fs::read_to_string(dir.join("args")).unwrap_or_default();
        let mut command = command();
        let config = dir.join("config");
        if config.exists() {
            command.env("TRIAD_CONFIG", config);
        }
        let result = command
            .arg("--headless")
            .args(["base", "left", "right"].map(|f| dir.join(f)))
            .arg("-o")
            .arg(&output)
            .args(extra.lines())
            .output()
            .expect("binary runs");
        let exit: i32 = fs::read_to_string(dir.join("exit"))
            .expect("exit file")
            .trim()
            .parse()
            .expect("exit code");
        assert_eq!(stderr(&result), "", "{case}: stderr");
        assert_eq!(result.status.code(), Some(exit), "{case}: exit code");
        assert_eq!(
            fs::read(&output).expect("output"),
            fs::read(dir.join("expected")).expect("expected"),
            "{case}: output bytes"
        );
    }
}

#[test]
fn the_positional_output_works_headless() {
    let dir = fixtures().join("clean-merge");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("out");
    let files = ["base", "left", "right"].map(|f| dir.join(f));
    let result = triad(&[Path::new("--headless"), &files[0], &files[1], &files[2], &output]);
    assert_eq!(stderr(&result), "");
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(fs::read(&output).expect("output"), b"A\nb\nC\n");
}

/// Merge `base`, `left` and `right`, written into a temporary directory, headless.
fn merge_bytes(base: &[u8], left: &[u8], right: &[u8]) -> (Output, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    for (name, bytes) in [("base", base), ("left", left), ("right", right)] {
        fs::write(tmp.path().join(name), bytes).expect("input");
    }
    fs::write(tmp.path().join("out"), STALE_OUTPUT).expect("stale output");
    let p = |f: &str| tmp.path().join(f);
    let result = triad(&[
        Path::new("--headless"),
        &p("base"),
        &p("left"),
        &p("right"),
        Path::new("-o"),
        &p("out"),
    ]);
    (result, tmp)
}

fn assert_failed(result: &Output, stderr_text: &str) {
    assert_eq!(stderr(result), stderr_text);
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
    assert_eq!(result.stdout, b"");
}

#[test]
fn a_bad_line_ending_in_the_config_file_is_reported_and_the_merge_keeps_the_endings() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join("config.triad");
    fs::write(&config, "output-line-ending = bogus\n").expect("config");
    let dir = fixtures().join("clean-merge");
    let output = tmp.path().join("out");
    let files = ["base", "left", "right"].map(|f| dir.join(f));
    let result = command()
        .env("TRIAD_CONFIG", &config)
        .arg("--headless")
        .args(&files)
        .arg(&output)
        .output()
        .expect("binary runs");
    assert_eq!(
        stderr(&result),
        format!(
            "triad-merge: {}:1: output-line-ending = bogus: expected keep, base, lf or crlf\n",
            config.display()
        )
    );
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(fs::read(&output).expect("output"), b"A\nb\nC\n");
}

#[test]
fn a_config_key_is_no_flag_and_writes_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = fixtures().join("clean-merge");
    let output = tmp.path().join("out");
    fs::write(&output, STALE_OUTPUT).expect("stale output");
    let files = ["base", "left", "right"].map(|f| dir.join(f));
    let result = triad(&[
        Path::new("--headless"),
        Path::new("--output-line-ending=lf"),
        &files[0],
        &files[1],
        &files[2],
        &output,
    ]);
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
    assert!(
        stderr(&result).contains("unexpected argument '--output-line-ending'"),
        "{}",
        stderr(&result)
    );
    assert_eq!(fs::read(&output).expect("output"), STALE_OUTPUT.as_bytes());
}

#[test]
fn binary_input_fails_with_3_and_writes_nothing() {
    let (result, tmp) = merge_bytes(b"a\n", b"a\0\n", b"a\n");
    assert_failed(
        &result,
        &format!(
            "triad-merge: {}: binary file: a NUL byte within the first 8000 bytes\n",
            tmp.path().join("left").display()
        ),
    );
    assert_eq!(fs::read(tmp.path().join("out")).expect("output"), STALE_OUTPUT.as_bytes());
}

#[test]
fn invalid_utf8_fails_with_3_and_writes_nothing() {
    let (result, tmp) = merge_bytes(b"a\n", b"a\n", b"a\xff\n");
    assert_failed(
        &result,
        &format!("triad-merge: {}: invalid UTF-8 at byte 1\n", tmp.path().join("right").display()),
    );
    assert_eq!(fs::read(tmp.path().join("out")).expect("output"), STALE_OUTPUT.as_bytes());
}

#[test]
fn a_missing_input_fails_with_3() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("nope");
    let output = tmp.path().join("out");
    let result = triad(&[Path::new("--headless"), &missing, &missing, &missing, &output]);
    let text = stderr(&result);
    // The OS words the reason; the prefix and the path are triad's.
    assert!(
        text.starts_with(&format!("triad-merge: cannot read {}: ", missing.display())),
        "{text}"
    );
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
    assert!(!output.exists());
}

#[test]
fn an_output_in_a_missing_directory_fails_with_3() {
    let dir = fixtures().join("clean-merge");
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("missing").join("out");
    let files = ["base", "left", "right"].map(|f| dir.join(f));
    let result = triad(&[Path::new("--headless"), &files[0], &files[1], &files[2], &output]);
    let text = stderr(&result);
    assert!(
        text.starts_with(&format!("triad-merge: cannot write {}: ", output.display())),
        "{text}"
    );
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
}

#[test]
fn bad_arguments_fail_with_3() {
    let result = triad(&[Path::new("--marker-length"), Path::new("0")]);
    let text = stderr(&result);
    assert!(text.starts_with("error: invalid value '0' for '--marker-length <N>'"), "{text}");
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
}

#[test]
fn version_prints_to_stdout_and_exits_0() {
    let result = triad(&[Path::new("--version")]);
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        format!("triad-merge {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(result.status.code(), Some(0));
}

#[cfg(unix)]
#[test]
fn a_linked_output_stays_a_link() {
    let dir = fixtures().join("clean-merge");
    let tmp = tempfile::tempdir().expect("tempdir");
    let (file, link) = (tmp.path().join("file"), tmp.path().join("link"));
    fs::write(&file, STALE_OUTPUT).expect("seed");
    std::os::unix::fs::symlink(&file, &link).expect("link");
    let result = command()
        .args([&dir.join("base"), &dir.join("left"), &dir.join("right"), &link])
        .arg("--headless")
        .output()
        .expect("binary runs");
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    assert_eq!(fs::read_link(&link).expect("still a link"), file);
    assert_eq!(fs::read(&file).expect("read"), fs::read(dir.join("expected")).expect("expected"));
}

/// Windows forwards window forms to `triad-merge-gui` instead (tests/forward.rs).
#[cfg(not(windows))]
#[test]
fn a_merge_window_with_a_missing_input_fails_before_any_window() {
    let dir = fixtures().join("clean-merge");
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = dir.join("missing");
    let output = tmp.path().join("out");
    let result = triad(&[&dir.join("base"), &missing, &dir.join("right"), &output]);
    let text = stderr(&result);
    assert!(
        text.starts_with(&format!("triad-merge: cannot read {}: ", missing.display())),
        "{text}"
    );
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
    assert!(!output.exists());
}

#[cfg(not(windows))]
#[test]
fn a_diff_window_with_a_missing_input_fails_before_any_window() {
    let dir = fixtures().join("clean-merge");
    let missing = dir.join("missing");
    let result = triad(&[Path::new("diff"), &dir.join("left"), &missing]);
    let text = stderr(&result);
    assert!(
        text.starts_with(&format!("triad-merge: cannot read {}: ", missing.display())),
        "{text}"
    );
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
}

#[cfg(not(windows))]
#[test]
fn native_diff_outside_a_repository_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("a.txt");
    std::fs::write(&file, "a\n").expect("the file is written");
    let dir = tmp.path().canonicalize().expect("the directory exists");
    let result = triad(&[Path::new("diff"), &file]);
    assert_failed(
        &result,
        &format!("triad-merge: {} is not inside a git or jj repository\n", dir.display()),
    );
}

#[cfg(not(windows))]
#[test]
fn native_diff_of_a_missing_file_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("gone.txt");
    let result = triad(&[Path::new("diff"), &file]);
    assert_failed(
        &result,
        &format!(
            "triad-merge: cannot find {}: No such file or directory (os error 2)\n",
            file.display()
        ),
    );
}

#[test]
fn resolve_outside_a_repository_exits_3() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = dunce::canonicalize(tmp.path()).expect("the directory exists");
    let result = command().arg("resolve").current_dir(&dir).output().expect("binary runs");
    assert_failed(
        &result,
        &format!("triad-merge: {} is not inside a git or jj repository\n", dir.display()),
    );
}

#[test]
fn triad_log_shows_events_on_stderr_and_in_the_log_file() {
    let dir = fixtures().join("clean-merge");
    let tmp = tempfile::tempdir().expect("tempdir");
    let logs = tmp.path().join("logs");
    let output = tmp.path().join("out");
    let result = Command::new(BINARY)
        .env("TRIAD_LOG", "info")
        .env("TRIAD_LOG_DIR", &logs)
        .arg("--headless")
        .args(["base", "left", "right"].map(|f| dir.join(f)))
        .arg(&output)
        .output()
        .expect("binary runs");
    assert_eq!(result.status.code(), Some(0));
    let line = format!(" INFO wrote the merge output={} resolved=true\n", output.display());
    assert_eq!(stderr(&result), line);
    let files: Vec<PathBuf> =
        fs::read_dir(&logs).expect("log dir").map(|e| e.expect("entry").path()).collect();
    let [file] = files.as_slice() else {
        panic!("one log file, got {files:?}");
    };
    let logged = fs::read_to_string(file).expect("log file");
    // The file adds a timestamp and the target.
    let file_line =
        format!(" INFO merge_session: wrote the merge output={} resolved=true\n", output.display());
    assert_eq!(logged.lines().count(), 1, "{logged}");
    assert!(logged.ends_with(&file_line), "{logged}");
}

/// `--headless` on `conflict-bare-markers` with `commands` written to a command file, into a
/// stale output.
fn merge_with_commands(commands: &str) -> (Output, tempfile::TempDir) {
    let dir = fixtures().join("conflict-bare-markers");
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("commands"), commands).expect("command file");
    fs::write(tmp.path().join("out"), STALE_OUTPUT).expect("stale output");
    let result = command()
        .arg("--headless")
        .args(["base", "left", "right"].map(|f| dir.join(f)))
        .arg("-o")
        .arg(tmp.path().join("out"))
        .arg("--commands")
        .arg(tmp.path().join("commands"))
        .output()
        .expect("binary runs");
    (result, tmp)
}

#[test]
fn a_command_file_resolves_the_conflict_and_exits_0() {
    let (result, tmp) =
        merge_with_commands("{\"Pick\":{\"region\":1,\"sources\":[{\"Side\":1},{\"Side\":0}]}}\n");
    assert_eq!(stderr(&result), "");
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(fs::read(tmp.path().join("out")).expect("output"), b"a\nR\nL\nc\n");
}

#[test]
fn a_bulk_pick_in_a_command_file_resolves_every_conflict() {
    let (result, tmp) = merge_with_commands(
        "{\"PickAll\":{\"scope\":\"Unresolved\",\"sources\":[{\"Side\":1}]}}\n",
    );
    assert_eq!(stderr(&result), "");
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(fs::read(tmp.path().join("out")).expect("output"), b"a\nR\nc\n");
}

#[test]
fn a_command_file_that_leaves_a_conflict_writes_markers_and_exits_1() {
    let (result, tmp) = merge_with_commands("{\"Clear\":{\"region\":0}}\n");
    assert_eq!(stderr(&result), "");
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        fs::read(tmp.path().join("out")).expect("output"),
        b"<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n"
    );
}

#[test]
fn a_bad_command_fails_with_3_and_writes_nothing() {
    let (result, tmp) = merge_with_commands("\"Undo\"\n{\"Clear\":{\"region\":7}}\n");
    assert_failed(
        &result,
        &format!(
            "triad-merge: {}: line 2 does not apply: no region 7: the merge has 3 regions\n",
            tmp.path().join("commands").display()
        ),
    );
    assert_eq!(fs::read(tmp.path().join("out")).expect("output"), STALE_OUTPUT.as_bytes());
}

#[test]
fn a_missing_command_file_fails_with_3_and_writes_nothing() {
    let dir = fixtures().join("clean-merge");
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("no-commands");
    let output = tmp.path().join("out");
    let result = command()
        .arg("--headless")
        .args(["base", "left", "right"].map(|f| dir.join(f)))
        .arg(&output)
        .arg("--commands")
        .arg(&missing)
        .output()
        .expect("binary runs");
    let text = stderr(&result);
    // The OS words the reason; the prefix and the path are triad's.
    assert!(
        text.starts_with(&format!("triad-merge: cannot read {}: ", missing.display())),
        "{text}"
    );
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
    assert!(!output.exists());
}

mod jj {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn resolve_of_a_clean_file_is_refused_before_any_window() {
        let fixture = vcs_jj::testing::Fixture::new();
        fixture.commit("one", &[("a.txt", b"a\n")]);
        fixture.check_out_to_disk();
        let result = command()
            .args(["resolve", "a.txt"])
            .current_dir(fixture.root())
            .output()
            .expect("binary runs");
        assert_failed(&result, "triad-merge: a.txt is not conflicted\n");
    }

    #[test]
    fn resolve_without_conflicts_says_so_and_exits_0() {
        let fixture = vcs_jj::testing::Fixture::new();
        fixture.commit("one", &[("a.txt", b"a\n")]);
        fixture.check_out_to_disk();
        let result =
            command().arg("resolve").current_dir(fixture.root()).output().expect("binary runs");
        assert_eq!(
            (
                result.status.code(),
                String::from_utf8_lossy(&result.stdout).as_ref(),
                stderr(&result).as_str()
            ),
            (Some(0), "no conflicted files\n", "")
        );
    }
}
