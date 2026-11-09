//! `triad-merge-gui` runs the same command line as `triad-merge` and opens the real window
//! (spec §6.1). Forms that end before a window show what it runs without opening one.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GUI: &str = env!("CARGO_BIN_EXE_triad-merge-gui");
const ERROR_EXIT: i32 = 3;

fn command(logs: &Path) -> Command {
    let mut command = Command::new(GUI);
    command
        .env("TRIAD_LOG_DIR", logs)
        .env("TRIAD_CONFIG", logs.join("no-config.triad"))
        .env_remove("TRIAD_LOG");
    command
}

fn clean_merge() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/core/clean-merge")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn run(command: &mut Command) -> Output {
    command.output().expect("binary runs")
}

#[test]
fn a_window_form_with_a_missing_input_fails_with_the_real_message() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = clean_merge();
    let missing = dir.join("missing");
    let result = run(command(tmp.path()).args([
        &dir.join("base"),
        &missing,
        &dir.join("right"),
        &tmp.path().join("out"),
    ]));
    let stderr = text(&result.stderr);
    assert!(
        stderr.starts_with(&format!("triad-merge: cannot read {}: ", missing.display())),
        "{stderr}"
    );
    assert_eq!(result.status.code(), Some(ERROR_EXIT));
    assert!(!tmp.path().join("out").exists());
}

#[test]
fn the_version_form_prints_the_version() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let result = run(command(tmp.path()).arg("--version"));
    assert_eq!(text(&result.stdout), format!("triad-merge {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(result.status.code(), Some(0));
}

#[test]
fn a_headless_merge_writes_its_output() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = clean_merge();
    let output = tmp.path().join("out");
    let result = run(command(tmp.path())
        .args([&dir.join("base"), &dir.join("left"), &dir.join("right"), &output])
        .arg("--headless"));
    assert_eq!(result.status.code(), Some(0), "{}", text(&result.stderr));
    assert_eq!(
        fs::read(&output).expect("written"),
        fs::read(dir.join("expected")).expect("expected")
    );
}
