//! `triad-merge inspect` and `diff --inspect` through the binary: stdout, exit codes, errors.

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
const ERROR_EXIT: i32 = 3;

fn fixture(case: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/core").join(case).join(file)
}

fn triad(args: &[&std::ffi::OsStr]) -> Output {
    command().args(args).output().expect("binary runs")
}

fn merge_args<'a>(sub: &'a str, case: &str, extra: &'a [&'a str]) -> Vec<std::ffi::OsString> {
    std::iter::once(sub.into())
        .chain(["base", "left", "right"].map(|f| fixture(case, f).into_os_string()))
        .chain(extra.iter().map(Into::into))
        .collect()
}

fn run(args: &[std::ffi::OsString]) -> Output {
    triad(&args.iter().map(std::ffi::OsString::as_os_str).collect::<Vec<_>>())
}

#[test]
fn inspect_json_prints_the_schema_and_exits_1_on_conflicts() {
    let result = run(&merge_args("inspect", "conflict-bare-markers", &["--json"]));
    assert_eq!(String::from_utf8_lossy(&result.stderr), "");
    assert_eq!(result.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).expect("stdout is JSON");
    assert_eq!(value["schema"], 1);
    assert_eq!(value["conflicts"], 1);
    assert_eq!(value["regions"][1]["kind"], "conflict");
}

#[test]
fn inspect_exits_0_without_conflicts_and_writes_no_file() {
    let result = run(&merge_args("inspect", "clean-merge", &[]));
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        "left-only base 0..1 left 0..1 right 0..1\n\
         unchanged base 1..2 left 1..2 right 1..2\n\
         right-only base 2..3 left 2..3 right 2..3\n\
         conflicts: 0\n"
    );
}

#[test]
fn diff_inspect_follows_diff_exit_codes() {
    let base = fixture("clean-merge", "base");
    let left = fixture("clean-merge", "left");
    let differ =
        triad(&["diff".as_ref(), "--inspect".as_ref(), base.as_os_str(), left.as_os_str()]);
    assert_eq!(differ.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&differ.stdout),
        "changed left 0..1 right 0..1\nunchanged left 1..3 right 1..3\nchanges: 1\n"
    );
    let same = triad(&["diff".as_ref(), "--inspect".as_ref(), base.as_os_str(), base.as_os_str()]);
    assert_eq!(same.status.code(), Some(0));
}

#[test]
fn inspect_errors_exit_3() {
    let missing = fixture("clean-merge", "missing");
    let result =
        triad(&["inspect".as_ref(), missing.as_os_str(), missing.as_os_str(), missing.as_os_str()]);
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
    assert_eq!(result.stdout, b"");
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    assert!(
        stderr.starts_with(&format!("triad-merge: cannot read {}: ", missing.display())),
        "{stderr}"
    );
}

/// `inspect` on the case that conflicts unless trailing whitespace is ignored, with a
/// settings file holding `settings`.
fn inspect_with_settings(settings: &str, extra: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.triad");
    std::fs::write(&path, settings).expect("written");
    command()
        .env("TRIAD_CONFIG", &path)
        .args(merge_args("inspect", "whitespace-trailing-avoids-conflict", extra))
        .output()
        .expect("binary runs")
}

const CONFLICTS_EXIT: i32 = 1;

#[test]
fn the_settings_file_sets_the_whitespace_mode() {
    let none = inspect_with_settings("", &[]);
    assert_eq!(none.status.code(), Some(CONFLICTS_EXIT), "no setting: conflicts");
    let trailing = inspect_with_settings("ignore-whitespace = trailing\n", &[]);
    assert_eq!(trailing.status.code(), Some(0), "the setting avoids the conflict");
    assert!(trailing.stderr.is_empty(), "{}", String::from_utf8_lossy(&trailing.stderr));
}

#[test]
fn a_flag_overrides_the_settings_file() {
    let output =
        inspect_with_settings("ignore-whitespace = trailing\n", &["--ignore-whitespace", "none"]);
    assert_eq!(output.status.code(), Some(CONFLICTS_EXIT));
}

#[test]
fn a_bad_line_warns_and_the_other_lines_apply() {
    let output = inspect_with_settings("ignore-whitespace = trailing\ntab-width = 0\n", &[]);
    assert_eq!(output.status.code(), Some(0), "the whitespace line applies");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("triad-merge: ")
            && stderr.ends_with("config.triad:2: tab-width 0 is outside 1–16\n")
            && stderr.lines().count() == 1,
        "{stderr}"
    );
}

#[test]
fn diff_inspect_ignores_line_endings_unless_the_setting_is_off() {
    let dir = tempfile::tempdir().expect("tempdir");
    let crlf = dir.path().join("crlf.txt");
    let lf = dir.path().join("lf.txt");
    std::fs::write(&crlf, "a\r\nb\r\nc\r\n").expect("written");
    std::fs::write(&lf, "a\nB\nc\n").expect("written");
    let compare = dir.path().join("config.triad");
    std::fs::write(&compare, "diff-ignore-line-endings = false\n").expect("written");
    let diff = |config: Option<&std::path::Path>| {
        let mut command = command();
        if let Some(config) = config {
            command.env("TRIAD_CONFIG", config);
        }
        command
            .arg("diff")
            .args(["--inspect".as_ref(), crlf.as_os_str(), lf.as_os_str()])
            .output()
            .expect("binary runs")
    };
    let ignored = diff(None);
    assert_eq!(
        String::from_utf8_lossy(&ignored.stdout),
        "unchanged left 0..1 right 0..1\n\
         changed left 1..2 right 1..2\n\
         unchanged left 2..3 right 2..3\n\
         changes: 1\n"
    );
    let compared = diff(Some(&compare));
    assert_eq!(
        String::from_utf8_lossy(&compared.stdout),
        "changed left 0..3 right 0..3\nchanges: 1\n"
    );
    std::fs::write(&lf, "a\nb\nc\n").expect("written");
    assert_eq!(diff(None).status.code(), Some(0), "equal but for line endings");
    assert_eq!(diff(Some(&compare)).status.code(), Some(1));
}

/// `inspect` on `base`, `left` and `right`, written to files named with `extension`, with a
/// settings file holding `settings`.
fn inspect_texts(settings: &str, extension: &str, [base, left, right]: [&str; 3]) -> Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config.triad");
    std::fs::write(&config, settings).expect("written");
    let files = [("base", base), ("left", left), ("right", right)].map(|(name, text)| {
        let path = dir.path().join(format!("{name}{extension}"));
        std::fs::write(&path, text).expect("written");
        path
    });
    command().env("TRIAD_CONFIG", &config).arg("inspect").args(files).output().expect("binary runs")
}

#[test]
fn inspect_refines_as_the_settings_file_says() {
    // Two shared leading lines: refinement splits them off; `refine-conflicts = false` keeps them in
    // the conflict, as `git merge-file --diff3` does.
    let texts = ["a\nb\nd\n", "a\nS1\nS2\nL\nd\n", "a\nS1\nS2\nR\nd\n"];
    let unrefined = inspect_texts("refine-conflicts = false\n", "", texts);
    assert_eq!(unrefined.status.code(), Some(CONFLICTS_EXIT));
    assert_eq!(
        String::from_utf8_lossy(&unrefined.stdout),
        "unchanged base 0..1 left 0..1 right 0..1\n\
         conflict base 1..2 left 1..4 right 1..4\n\
         unchanged base 2..3 left 4..5 right 4..5\n\
         conflicts: 1\n"
    );
    let refined = inspect_texts("", "", texts);
    assert_eq!(
        String::from_utf8_lossy(&refined.stdout),
        "unchanged base 0..1 left 0..1 right 0..1\n\
         both-same base 1..1 left 1..3 right 1..3\n\
         conflict base 1..2 left 3..4 right 3..4\n\
         unchanged base 2..3 left 4..5 right 4..5\n\
         conflicts: 1\n"
    );
}

#[test]
fn inspect_keeps_syntax_boundaries_as_the_settings_file_says() {
    // fixtures/core/syntax-boundaries-keep-a-shared-brace-whole: the boundaries keep deep
    // refinement from splitting at the shared `}`, so one conflict spans the body.
    let case = |file| {
        std::fs::read_to_string(fixture("syntax-boundaries-keep-a-shared-brace-whole", file))
            .expect("fixture")
    };
    let (base, left, right) = (case("base"), case("left"), case("right"));
    let result = inspect_texts(
        "refine-conflicts = \"deep\"\nsyntax-boundaries = true\n",
        ".rs",
        [&base, &left, &right],
    );
    assert_eq!(String::from_utf8_lossy(&result.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&result.stdout),
        "unchanged base 0..1 left 0..1 right 0..1\n\
         conflict base 1..2 left 1..5 right 1..5\n\
         unchanged base 2..3 left 5..6 right 5..6\n\
         conflicts: 1\n"
    );
}
