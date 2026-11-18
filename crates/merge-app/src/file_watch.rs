//! Watches files for changes another program makes (spec §6.1, §6.3). The diff window watches
//! both its files; the merge window watches OUTPUT.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_channel::Receiver;
use gpui_kit::{AppContext as _, Context, Task, Window};

/// How long the watcher waits after a change on disk for the events that follow it.
pub(crate) const DISK_SETTLE: Duration = Duration::from_millis(100);

/// The task that starts the watcher on the files' directories and passes its events on.
/// The watcher lives as long as the task.
pub(crate) struct Watch {
    _events: Task<()>,
}

/// Watch the files `paths` names, through their directories: editors save by writing a new
/// file and renaming it over the old one, which a watch on the file itself would lose. Once
/// changes settle for [`DISK_SETTLE`], `changed` hears of each changed file once, by its
/// index in `paths`, in index order. Starting a platform watch can take seconds (`FSEvents`
/// on macOS), so it starts off the UI thread.
pub(crate) fn watch<V: 'static>(
    paths: &[Option<&Path>],
    cwd: &Path,
    changed: fn(&mut V, usize, &mut Window, &mut Context<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) -> Watch {
    let (sender, receiver) = async_channel::unbounded::<usize>();
    let targets: Vec<Option<PathBuf>> =
        paths.iter().map(|path| path.and_then(|path| watch_target(path, cwd))).collect();
    let dirs: Vec<PathBuf> = targets
        .iter()
        .flatten()
        .filter_map(|target| target.parent().map(Path::to_path_buf))
        .collect();
    let handler = move |event: notify::Result<notify::Event>| match event {
        Ok(event) if !event.kind.is_access() => {
            for (at, target) in targets.iter().enumerate() {
                if event.paths.iter().any(|p| Some(p.as_path()) == target.as_deref()) {
                    // The window may have closed; then nobody listens.
                    let _ = sender.try_send(at);
                }
            }
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "the file watcher failed"),
    };
    let starting = cx.background_spawn(async move {
        let mut watcher = notify::recommended_watcher(handler)
            .inspect_err(|error| tracing::warn!(%error, "cannot watch the files"))
            .ok()?;
        for dir in &dirs {
            if let Err(error) =
                notify::Watcher::watch(&mut watcher, dir, notify::RecursiveMode::NonRecursive)
            {
                tracing::warn!(%error, dir = %dir.display(), "cannot watch a directory");
            }
        }
        Some(watcher)
    });
    let count = paths.len();
    let events = cx.spawn_in(window, async move |this, cx| {
        let Some(_watcher) = starting.await else {
            return;
        };
        while let Ok(first) = receiver.recv().await {
            // A save gives several events; read each file once they settle.
            cx.background_executor().timer(DISK_SETTLE).await;
            let gone = this
                .update_in(cx, |this, window, cx| {
                    for at in changed_files(first, &receiver, count) {
                        changed(this, at, window, cx);
                    }
                })
                .is_err();
            if gone {
                break;
            }
        }
    });
    Watch { _events: events }
}

/// Where `path`, as given on the command line, sits for the watcher: its directory made
/// absolute and free of symlinks, as the platform's events name it. The file itself may not
/// exist: a rename may be replacing it. `None` without a file name or an existing directory.
fn watch_target(path: &Path, cwd: &Path) -> Option<PathBuf> {
    let absolute = cwd.join(path);
    let name = absolute.file_name()?;
    let dir = absolute
        .parent()?
        .canonicalize()
        .inspect_err(|error| tracing::warn!(%error, path = %path.display(), "cannot watch a file"))
        .ok()?;
    Some(dir.join(name))
}

/// The file of `first` and of every event waiting in `receiver`, each once, in index order,
/// of `count` files.
fn changed_files(
    first: usize,
    receiver: &Receiver<usize>,
    count: usize,
) -> impl Iterator<Item = usize> + use<> {
    let mut changed = vec![false; count];
    let mut mark = |at: usize| {
        if let Some(slot) = changed.get_mut(at) {
            *slot = true;
        }
    };
    mark(first);
    while let Ok(at) = receiver.try_recv() {
        mark(at);
    }
    changed.into_iter().enumerate().filter_map(|(at, changed)| changed.then_some(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: usize = 2;

    fn changed(events: &[usize]) -> Vec<usize> {
        let (sender, receiver) = async_channel::unbounded();
        let (&first, rest) = events.split_first().expect("an event");
        for &at in rest {
            sender.try_send(at).expect("open channel");
        }
        let files: Vec<usize> = changed_files(first, &receiver, FILES).collect();
        assert!(receiver.is_empty(), "every waiting event is taken");
        files
    }

    #[test]
    fn repeated_events_for_one_file_read_it_once() {
        assert_eq!(changed(&[0, 0, 0]), [0]);
    }

    #[test]
    fn events_for_both_files_read_each_once_in_order() {
        assert_eq!(changed(&[1, 0, 1]), [0, 1]);
    }

    #[test]
    fn one_event_reads_its_file_alone() {
        assert_eq!(changed(&[1]), [1]);
    }

    fn canonical(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().canonicalize().expect("the directory exists")
    }

    #[test]
    fn a_bare_name_sits_in_the_working_directory() {
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(watch_target(Path::new("x"), cwd.path()), Some(canonical(&cwd).join("x")));
    }

    #[test]
    fn a_relative_directory_resolves_against_the_working_directory() {
        let cwd = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(cwd.path().join("sub")).expect("mkdir");
        assert_eq!(
            watch_target(Path::new("./sub/../sub/x"), cwd.path()),
            Some(canonical(&cwd).join("sub").join("x"))
        );
    }

    #[test]
    fn an_absolute_path_ignores_the_working_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            watch_target(&dir.path().join("x"), cwd.path()),
            Some(canonical(&dir).join("x"))
        );
    }

    #[test]
    fn the_file_need_not_exist_for_a_rename_can_replace_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!dir.path().join("gone").exists());
        assert_eq!(
            watch_target(&dir.path().join("gone"), dir.path()),
            Some(canonical(&dir).join("gone"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_resolves_to_its_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("real")).expect("mkdir");
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link"))
            .expect("symlink");
        assert_eq!(
            watch_target(&dir.path().join("link").join("x"), dir.path()),
            Some(canonical(&dir).join("real").join("x"))
        );
    }

    #[test]
    fn a_missing_directory_has_no_target() {
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(watch_target(Path::new("no/such/x"), cwd.path()), None);
    }

    #[test]
    fn a_path_without_a_file_name_has_no_target() {
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(watch_target(Path::new(".."), cwd.path()), None);
    }
}
