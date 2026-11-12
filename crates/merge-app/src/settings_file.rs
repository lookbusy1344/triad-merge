//! Settings…: open the config file in the default text editor (`DESIGN.md` §E3.5). A
//! missing file is first written from [`settings::template`]. Changes apply from the next run.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use gpui_kit::{App, AppContext as _};
use merge_config::settings;

use crate::strings;

/// The command that opens a file in the default text editor.
#[derive(Debug)]
pub(crate) struct Editor {
    pub command: Command,
    /// Whether the command exits once the editor has the file, so its status tells whether
    /// the file opened. Notepad is the editor itself and exits when it closes.
    pub waits: bool,
}

/// Why Settings… could not open the config file.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenError {
    #[error("no config file location: set TRIAD_CONFIG")]
    NoLocation,
    #[error("cannot write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot start a text editor for {}: {source}", path.display())]
    Launch {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("no text editor opened {}: the opener exited with {status}", path.display())]
    Refused { path: PathBuf, status: ExitStatus },
}

impl OpenError {
    /// The file to reveal in the file manager instead: one that exists but did not open.
    fn unopened(&self) -> Option<&Path> {
        match self {
            Self::Launch { path, .. } | Self::Refused { path, .. } => Some(path),
            Self::NoLocation | Self::Write { .. } => None,
        }
    }
}

/// The default text editor on `path`: `open -t` on macOS, `xdg-open` on Linux, Notepad on
/// Windows.
pub(crate) fn editor(path: &Path) -> Editor {
    let (program, flags, waits): (&str, &[&str], bool) = if cfg!(target_os = "macos") {
        ("open", &["-t"], true)
    } else if cfg!(windows) {
        ("notepad.exe", &[], false)
    } else {
        ("xdg-open", &[], true)
    };
    let mut command = Command::new(program);
    command.args(flags).arg(path);
    Editor { command, waits }
}

/// Write the template to `path` when the file is missing, then open it through `run`, which
/// gives the opener's exit status, or none for an editor that is not waited for.
pub(crate) fn open_with(
    path: &Path,
    run: impl FnOnce(Editor) -> io::Result<Option<ExitStatus>>,
) -> Result<(), OpenError> {
    settings::write_template(path)
        .map_err(|source| OpenError::Write { path: path.to_owned(), source })?;
    match run(editor(path)) {
        Ok(Some(status)) if !status.success() => {
            Err(OpenError::Refused { path: path.to_owned(), status })
        }
        Ok(_) => Ok(()),
        Err(source) => Err(OpenError::Launch { path: path.to_owned(), source }),
    }
}

/// Start `editor`, and wait for it when it [`Editor::waits`].
fn run(mut editor: Editor) -> io::Result<Option<ExitStatus>> {
    let mut child = editor.command.spawn()?;
    editor.waits.then(|| child.wait()).transpose()
}

/// Settings…: open this process's config file off the UI thread. A failure goes to stderr,
/// and a file that exists but did not open is revealed in the file manager.
pub(crate) fn open(cx: &mut App) {
    let opened = cx.background_spawn(async {
        let path = settings::path().ok_or(OpenError::NoLocation)?;
        open_with(&path, run)
    });
    cx.spawn(async move |cx| {
        if let Err(error) = opened.await {
            eprintln!("{}: {error}", strings::PROGRAM);
            if let Some(path) = error.unopened() {
                cx.update(|cx| cx.reveal_path(path));
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::ffi::OsStr;

    use super::*;

    /// An exit status with `code`, as the platform reports it.
    fn status(code: i32) -> ExitStatus {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt as _;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt as _;
        #[cfg(unix)]
        const SHIFT: i32 = 8;
        #[cfg(unix)]
        return ExitStatus::from_raw(code << SHIFT);
        #[cfg(windows)]
        return ExitStatus::from_raw(code.cast_unsigned());
    }

    #[test]
    fn the_editor_is_the_platform_s_text_editor_on_the_file() {
        let path = Path::new("/c/triad/config.triad");
        let editor = editor(path);
        let program = editor.command.get_program();
        let args: Vec<&OsStr> = editor.command.get_args().collect();
        if cfg!(target_os = "macos") {
            assert_eq!(program, "open");
            assert_eq!(args, [OsStr::new("-t"), path.as_os_str()]);
            assert!(editor.waits);
        } else if cfg!(windows) {
            assert_eq!(program, "notepad.exe");
            assert_eq!(args, [path.as_os_str()]);
            assert!(!editor.waits);
        } else {
            assert_eq!(program, "xdg-open");
            assert_eq!(args, [path.as_os_str()]);
            assert!(editor.waits);
        }
    }

    #[test]
    fn a_missing_file_is_written_from_the_template_then_opened() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("triad").join("config.triad");
        let opened = RefCell::new(None);
        let result = open_with(&path, |editor| {
            let args: Vec<_> = editor.command.get_args().map(OsStr::to_owned).collect();
            opened.replace(Some(args));
            Ok(Some(status(0)))
        });
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(std::fs::read_to_string(&path).expect("written"), settings::template());
        let args = opened.take().expect("the editor ran");
        assert_eq!(args.last().map(PathBuf::from), Some(path));
    }

    #[test]
    fn an_existing_file_opens_unchanged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.triad");
        std::fs::write(&path, "font-size = 14\n").expect("written");
        let result = open_with(&path, |_| Ok(None));
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "font-size = 14\n");
    }

    #[test]
    fn an_opener_that_fails_is_refused_and_the_file_is_revealed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.triad");
        let error = open_with(&path, |_| Ok(Some(status(1)))).expect_err("refused");
        assert!(
            matches!(&error, OpenError::Refused { path: p, status: s } if *p == path && s.code() == Some(1)),
            "{error:?}"
        );
        assert_eq!(error.unopened(), Some(path.as_path()));
    }

    #[test]
    fn an_editor_that_cannot_start_is_a_launch_error_and_the_file_is_revealed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.triad");
        let error =
            open_with(&path, |_| Err(io::ErrorKind::NotFound.into())).expect_err("no editor");
        assert!(
            matches!(&error, OpenError::Launch { path: p, source } if *p == path && source.kind() == io::ErrorKind::NotFound),
            "{error:?}"
        );
        assert_eq!(error.unopened(), Some(path.as_path()));
        assert_eq!(
            error.to_string(),
            format!("cannot start a text editor for {}: entity not found", path.display())
        );
    }

    #[test]
    fn a_file_that_cannot_be_written_is_not_opened_or_revealed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").expect("written");
        let path = blocker.join("config.triad");
        let ran = RefCell::new(false);
        let error = open_with(&path, |_| {
            ran.replace(true);
            Ok(None)
        })
        .expect_err("cannot write");
        assert!(
            matches!(&error, OpenError::Write { path: p, source } if *p == path && source.kind() == io::ErrorKind::AlreadyExists),
            "{error:?}"
        );
        assert!(!ran.take());
        assert_eq!(error.unopened(), None);
        assert_eq!(OpenError::NoLocation.unopened(), None);
    }
}
