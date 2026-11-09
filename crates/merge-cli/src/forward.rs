//! The Windows console binary's half of the GUI launch (spec §6.1): start the GUI-subsystem
//! binary, wait for it and pass its exit code on.

use std::io::Write;
use std::process::Command;

use crate::{ExitCode, PROGRAM};

/// The GUI-subsystem binary, beside the console one.
pub const GUI_EXE: &str = "triad-merge-gui.exe";
/// The console-subsystem binary, which git and jj run.
pub const CONSOLE_EXE: &str = "triad-merge.exe";

/// Run `command` with this process's stdin, stdout and stderr, wait, and give its exit code.
/// A code outside 0–3, an exit by signal, or a failure to start gives [`ExitCode::Error`],
/// with a line on `stderr`.
pub fn forward(mut command: Command, stderr: &mut impl Write) -> ExitCode {
    let program = command.get_program().to_owned();
    let message = match command.status() {
        Ok(status) => match status.code().and_then(ExitCode::from_code) {
            Some(code) => return code,
            None => format!("{} ended with {status}", program.display()),
        },
        Err(e) => format!("cannot start {}: {e}", program.display()),
    };
    // Nothing is left to tell the user if stderr is closed.
    let _ = writeln!(stderr, "{PROGRAM}: {message}");
    ExitCode::Error
}
