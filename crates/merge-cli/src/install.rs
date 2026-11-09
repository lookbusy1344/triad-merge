//! `triad-merge install`: the spec §7.1 entries for this program, written by
//! [`merge_session::install`].

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use merge_session::env::Env;
use vcs::RepoKind;

use crate::PROGRAM;
use crate::forward::{CONSOLE_EXE, GUI_EXE};

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("cannot find this program's path: {0}")]
    Program(#[source] io::Error),
    #[error(transparent)]
    Config(#[from] merge_session::install::InstallError),
}

/// The path of the running `AppImage`, which its runtime sets.
const APPIMAGE_VAR: &str = "APPIMAGE";

/// The program the entries name: this one, by its canonical path. On Windows that is the
/// console binary, also when the GUI binary runs the install (spec §6.1).
fn program() -> Result<PathBuf, InstallError> {
    let exe =
        std::env::current_exe().and_then(dunce::canonicalize).map_err(InstallError::Program)?;
    Ok(entry_program(&exe, std::env::var_os(APPIMAGE_VAR).as_deref()))
}

/// What the entries name: the `AppImage` at `appimage`, when one runs this binary, since
/// `exe` then lies in a mount that ends with the run; else [`console_program`] of `exe`.
fn entry_program(exe: &Path, appimage: Option<&OsStr>) -> PathBuf {
    appimage
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .map_or_else(|| console_program(exe), Path::to_path_buf)
}

/// `exe`, or the console binary beside it when `exe` is the Windows GUI binary.
fn console_program(exe: &Path) -> PathBuf {
    if exe.file_name().is_some_and(|name| name == GUI_EXE) {
        exe.with_file_name(CONSOLE_EXE)
    } else {
        exe.to_owned()
    }
}

/// Install for `vcs` in `config`, or in the user's config file, and say where.
///
/// # Errors
///
/// [`InstallError::Program`] when this program's path is unknown, else as
/// [`merge_session::install::install`].
pub fn run(vcs: RepoKind, config: Option<&Path>) -> Result<String, InstallError> {
    let program = program()?;
    let installed = merge_session::install::install(vcs, config, &program, &Env::from_process())?;
    let state = if installed.changed { "installed" } else { "already installed" };
    Ok(format!("{PROGRAM}: {state} in {}\n", installed.path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_gui_binary_installs_the_console_binary_beside_it() {
        assert_eq!(
            console_program(Path::new("/apps/triad/triad-merge-gui.exe")),
            Path::new("/apps/triad/triad-merge.exe")
        );
        assert_eq!(
            console_program(Path::new("/apps/triad/triad-merge")),
            Path::new("/apps/triad/triad-merge")
        );
    }

    #[test]
    fn an_appimage_is_named_in_place_of_its_passing_mount() {
        assert_eq!(
            entry_program(
                Path::new("/tmp/.mount_triadX/usr/bin/triad-merge"),
                Some(OsStr::new("/home/me/triad-merge.AppImage"))
            ),
            Path::new("/home/me/triad-merge.AppImage")
        );
    }

    #[test]
    fn without_an_appimage_the_binary_is_named() {
        let exe = Path::new("/opt/triad/triad-merge");
        assert_eq!(entry_program(exe, None), exe);
    }

    #[test]
    fn an_empty_or_relative_appimage_path_is_ignored() {
        let exe = Path::new("/opt/triad/triad-merge");
        assert_eq!(entry_program(exe, Some(OsStr::new(""))), exe);
        assert_eq!(entry_program(exe, Some(OsStr::new("triad.AppImage"))), exe);
    }

    #[test]
    fn without_an_appimage_the_gui_binary_still_names_the_console_binary() {
        assert_eq!(
            entry_program(Path::new("/opt/triad/triad-merge-gui.exe"), None),
            Path::new("/opt/triad/triad-merge.exe")
        );
    }
}
