//! `triad-merge`: the merge and diff tool entry point.

use std::io;

use merge_cli::{ExitCode, Outcome};

fn main() -> std::process::ExitCode {
    merge_cli::logging::init();
    let settings = merge_config::settings::path();
    let outcome = merge_cli::run(
        std::env::args_os(),
        settings.as_deref(),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    );
    let code = match outcome {
        Outcome::Exit(code) => code,
        Outcome::Window(form) => window(*form),
    };
    code.into()
}

/// Windows: the GUI-subsystem binary beside this one runs the window, with the same
/// arguments, so this console process keeps git's or jj's console (spec §6.1).
#[cfg(windows)]
fn window(_: merge_session::WindowForm) -> ExitCode {
    let mut stderr = io::stderr().lock();
    let gui = match std::env::current_exe() {
        Ok(exe) => exe.with_file_name(merge_cli::forward::GUI_EXE),
        Err(e) => {
            use std::io::Write;
            let _ = writeln!(stderr, "triad-merge: cannot find this program's path: {e}");
            return ExitCode::Error;
        }
    };
    let mut command = std::process::Command::new(gui);
    command.args(std::env::args_os().skip(1));
    merge_cli::forward::forward(command, &mut stderr)
}

#[cfg(not(windows))]
fn window(form: merge_session::WindowForm) -> ExitCode {
    merge_app::run(form)
}
