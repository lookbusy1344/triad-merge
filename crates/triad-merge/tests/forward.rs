//! Forwarding to the GUI binary (PLAN.md §A4.13), with the `forward_stand_in`
//! example as the child.

use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

use merge_cli::forward::forward;
use merge_session::ExitCode;

/// Test binaries sit in `target/<profile>/deps`, examples in `target/<profile>/examples`.
static STAND_IN: LazyLock<PathBuf> = LazyLock::new(|| {
    let test = std::env::current_exe().expect("the test's path");
    let profile = test
        .parent()
        .and_then(std::path::Path::parent)
        .expect("a test binary sits two levels down");
    profile.join("examples").join(format!("forward_stand_in{}", std::env::consts::EXE_SUFFIX))
});
const EXIT_VAR: &str = "TRIAD_STAND_IN_EXIT";

fn stand_in(exit: &str) -> Command {
    let mut command = Command::new(&*STAND_IN);
    command.arg("x").env(EXIT_VAR, exit);
    command
}

fn forwarded(command: Command) -> (ExitCode, String) {
    let mut stderr = Vec::new();
    let code = forward(command, &mut stderr);
    (code, String::from_utf8(stderr).expect("utf-8"))
}

#[test]
fn exit_codes_0_to_3_pass_through() {
    for (exit, code) in [
        ("0", ExitCode::Resolved),
        ("1", ExitCode::Conflicts),
        ("2", ExitCode::Cancelled),
        ("3", ExitCode::Error),
    ] {
        assert_eq!(forwarded(stand_in(exit)), (code, String::new()), "{exit}");
    }
}

#[test]
fn another_exit_code_is_an_error() {
    let (code, stderr) = forwarded(stand_in("7"));
    assert_eq!(code, ExitCode::Error);
    // The OS words the status.
    assert!(
        stderr.starts_with(&format!("triad-merge: {} ended with ", STAND_IN.display()))
            && stderr.ends_with("7\n"),
        "{stderr}"
    );
}

#[test]
fn a_missing_gui_binary_is_an_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("triad-merge-gui.exe");
    let (code, stderr) = forwarded(Command::new(&missing));
    assert_eq!(code, ExitCode::Error);
    assert!(
        stderr.starts_with(&format!("triad-merge: cannot start {}: ", missing.display())),
        "{stderr}"
    );
}

#[test]
fn the_stand_in_names_its_arguments_and_defaults_to_3() {
    let result = Command::new(&*STAND_IN)
        .args(["b", "l r"])
        .env_remove(EXIT_VAR)
        .output()
        .expect("stand-in runs");
    assert_eq!(result.status.code(), Some(3));
    assert_eq!(
        String::from_utf8_lossy(&result.stderr),
        "triad-merge-gui: stand-in window for [\"b\", \"l r\"]\n"
    );
}

/// The console binary forwards a window form with its arguments to the real GUI binary, and
/// the child's exit code and stderr reach the caller. The missing input ends it before a window.
#[cfg(windows)]
#[test]
fn the_console_binary_forwards_window_forms() {
    let result = Command::new(env!("CARGO_BIN_EXE_triad-merge"))
        .args(["b", "l", "r", "o"])
        .env("TRIAD_LOG_DIR", std::env::temp_dir().join("triad-forward-test-logs"))
        .output()
        .expect("binary runs");
    assert_eq!(result.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.starts_with("triad-merge: cannot read b: "), "{stderr}");
}
