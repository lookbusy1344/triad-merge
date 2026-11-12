//! The child process of the forwarding tests (DESIGN.md §A4.13). It names its arguments on stderr and
//! exits with `TRIAD_STAND_IN_EXIT`, or with 3. Test builds compile it; plain builds leave it out.

use merge_session::ExitCode;

/// The exit code the stand-in gives, as a number.
const EXIT_VAR: &str = "TRIAD_STAND_IN_EXIT";

fn main() -> std::process::ExitCode {
    let args: Vec<String> =
        std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    eprintln!("triad-merge-gui: stand-in window for {args:?}");
    std::env::var(EXIT_VAR)
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(ExitCode::Error.into(), std::process::ExitCode::from)
}
