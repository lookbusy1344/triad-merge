//! `triad-merge-gui`: the Windows GUI-subsystem entry point, started by `triad-merge` (spec §6.1).
//! It takes the same command line and runs window forms itself.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io;

use merge_cli::Outcome;

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
        Outcome::Window(form) => merge_app::run(*form),
    };
    code.into()
}
