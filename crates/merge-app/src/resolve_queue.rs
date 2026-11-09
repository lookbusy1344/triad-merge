//! `resolve` without a path: each conflicted file opens in turn (spec §7.4).
//!
//! Each save moves on to the next conflicted file not yet opened, listed afresh after the
//! save. Cancel or an error ends the run with its code; the last save ends it with 0 when
//! every file resolved, else 1.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::{AnyWindowHandle, App, AppContext as _};
use merge_core::document::{MergeDocument, MergeOptions};
use merge_session::jj_config::ConfigEnv;
use merge_session::repository::{Handoff, RunRepository};
use merge_session::resolve::Opened;
use merge_session::resolve::Snapshot;

use crate::window::{OpenError, OriginLookup, WindowSettings};
use crate::{ExitCode, Finish};

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// The run's state between windows.
pub struct Queue {
    finder: Finder,
    settings: WindowSettings,
    exit: Rc<ExitFn>,
    /// Whether a file was saved with conflict markers.
    unresolved: bool,
    /// The merge window of the file open now; a save closes it, and no other window.
    window: Option<AnyWindowHandle>,
}

/// Why a file in the queue did not open.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error(transparent)]
    Resolve(#[from] merge_session::resolve::ResolveError),
    #[error(transparent)]
    Labels(#[from] merge_core::document::LabelCountError),
    #[error(transparent)]
    Open(#[from] OpenError),
}

impl Queue {
    /// A queue of the conflicted files of the repository at `cwd`, opened with `options` and
    /// `settings`, ending with `exit`.
    pub fn new(
        cwd: PathBuf,
        options: MergeOptions,
        settings: WindowSettings,
        exit: impl Fn(ExitCode, &mut App) + 'static,
    ) -> Self {
        Self {
            finder: Finder { cwd, options, jj_config: None, opened: Vec::new() },
            settings,
            exit: Rc::new(exit),
            unresolved: false,
            window: None,
        }
    }

    /// The queue, with `config` as the user's jj config for jj's snapshots.
    #[must_use]
    pub fn with_jj_config(mut self, config: ConfigEnv) -> Self {
        self.finder.jj_config = Some(config);
        self
    }

    /// The code the run ends with after its last save.
    fn code(&self) -> ExitCode {
        if self.unresolved { ExitCode::Conflicts } else { ExitCode::Resolved }
    }
}

/// What finding and loading the next file needs, off the UI thread.
#[derive(Clone)]
struct Finder {
    cwd: PathBuf,
    options: MergeOptions,
    /// The user's jj config for jj's snapshots; the process's without one.
    jj_config: Option<ConfigEnv>,
    /// The working-tree files opened or refused so far.
    opened: Vec<PathBuf>,
}

/// The next file, loaded, with the repository its window reads origins from.
struct Next {
    file: PathBuf,
    opened: Opened,
    repo: RunRepository,
}

/// What a search for the next file found.
struct Found {
    /// The files `resolve` refused on the way, each with why, in the order met.
    refused: Vec<(PathBuf, QueueError)>,
    /// The file to open; none when every conflicted file has been opened or refused.
    next: Result<Option<Next>, QueueError>,
}

impl Finder {
    /// The repository holding the run's working directory, with its jj config.
    fn repository(&self) -> RunRepository {
        let repo = RunRepository::find([self.cwd.as_path()]);
        match &self.jj_config {
            Some(config) => repo.with_jj_config(config.clone()),
            None => repo,
        }
    }

    /// The next conflicted file not yet opened, in path order, listed in `repo`.
    fn next(&self, repo: &mut RunRepository) -> Result<Option<PathBuf>, QueueError> {
        let listed = merge_session::resolve::conflicted(repo)?;
        Ok(listed
            .paths
            .iter()
            .map(|path| listed.root.join(path).into_std_path_buf())
            .find(|file| !self.opened.contains(file)))
    }

    /// List and load the next conflicted file, skipping each one `resolve` refuses. One
    /// repository per file: it lists the conflicts, loads the file and finds its origins.
    /// Each file is listed afresh, after the save before it, and loads at the list's snapshot
    /// ([`Snapshot::Taken`]). Blocks on the repository.
    fn find(mut self) -> Found {
        let mut refused = Vec::new();
        loop {
            let mut repo = self.repository();
            let file = match self.next(&mut repo) {
                Ok(Some(file)) => file,
                Ok(None) => return Found { refused, next: Ok(None) },
                Err(error) => return Found { refused, next: Err(error) },
            };
            self.opened.push(file.clone());
            let loaded = merge_session::resolve::load(&mut repo, &self.cwd, &file, Snapshot::Taken)
                .map_err(QueueError::from)
                .and_then(|loaded| Ok(loaded.open(self.options)?));
            match loaded {
                Ok(opened) => {
                    return Found { refused, next: Ok(Some(Next { file, opened, repo })) };
                }
                Err(QueueError::Resolve(error)) if error.is_refusal() => {
                    refused.push((file, QueueError::Resolve(error)));
                }
                Err(error) => return Found { refused, next: Err(error) },
            }
        }
    }
}

/// Take over the app's [`Finish`], so each save opens the next file, and open the first.
pub fn start(queue: Queue, cx: &mut App) {
    let queue = Rc::new(RefCell::new(queue));
    let finished = Rc::clone(&queue);
    cx.set_global(Finish::new(move |code, cx| {
        let queue = Rc::clone(&finished);
        // The finishing window is still borrowed; close it once that ends.
        cx.defer(move |cx| saved(&queue, code, cx));
    }));
    advance(&queue, cx);
}

/// The window of `opened`, run from `cwd`: titled by its path, saving to its working-tree
/// file, and with its terms' origins, read from `repo`. The terms follow the commits a merge
/// the VCS stopped on names (spec §7.3), so the merge's lookup finds them.
pub(crate) fn window_settings(
    opened: Opened,
    cwd: &Path,
    repo: RunRepository,
    base: WindowSettings,
) -> (MergeDocument, WindowSettings) {
    let file = opened.file.into_std_path_buf();
    let title = opened.path.into_string();
    let settings = WindowSettings {
        repo: opened.repo,
        origins: Some(OriginLookup {
            output: file.clone(),
            title: Some(title.clone()),
            cwd: cwd.to_owned(),
            repo: Handoff::new(repo),
        }),
        title: Some(title),
        output: Some(file),
        record: opened.record,
        ..base
    };
    (opened.doc, settings)
}

/// A window finished with `code`: close it, then open the next file or end the run.
fn saved(queue: &Rc<RefCell<Queue>>, code: ExitCode, cx: &mut App) {
    let finished = queue.borrow_mut().window.take();
    if let Some(window) = finished {
        // A window already closing has nothing left to close.
        let _ = window.update(cx, |_, window, _| window.remove_window());
    }
    match code {
        ExitCode::Cancelled | ExitCode::Error => return end(queue, code, cx),
        ExitCode::Conflicts => queue.borrow_mut().unresolved = true,
        ExitCode::Resolved => {}
    }
    advance(queue, cx);
}

/// Find and load the next conflicted file off the UI thread, then open its window; end the
/// run when none is left.
fn advance(queue: &Rc<RefCell<Queue>>, cx: &mut App) {
    let finder = queue.borrow().finder.clone();
    let found = cx.background_spawn(async move { finder.find() });
    let queue = Rc::clone(queue);
    cx.spawn(async move |cx| {
        let found = found.await;
        cx.update(|cx| arrived(&queue, found, cx));
    })
    .detach();
}

/// Report the files refused on the way, then open the one found, or end the run.
fn arrived(queue: &Rc<RefCell<Queue>>, found: Found, cx: &mut App) {
    let report = |error: &QueueError| eprintln!("{}: {error}", crate::strings::PROGRAM);
    for (file, error) in &found.refused {
        report(error);
        let mut state = queue.borrow_mut();
        state.finder.opened.push(file.clone());
        state.unresolved = true;
    }
    let next = match found.next {
        Ok(Some(next)) => next,
        Ok(None) => {
            let code = queue.borrow().code();
            return end(queue, code, cx);
        }
        Err(error) => {
            report(&error);
            return end(queue, ExitCode::Error, cx);
        }
    };
    let state = queue.borrow();
    let (doc, settings) =
        window_settings(next.opened, &state.finder.cwd, next.repo, state.settings.clone());
    drop(state);
    queue.borrow_mut().finder.opened.push(next.file);
    match crate::window::open(doc, &settings, cx) {
        Ok((window, _)) => queue.borrow_mut().window = Some(window),
        Err(error) => {
            report(&QueueError::from(error));
            end(queue, ExitCode::Error, cx);
        }
    }
}

/// End the run with `code`. The callback runs with the queue unborrowed, so it may use it.
fn end(queue: &Rc<RefCell<Queue>>, code: ExitCode, cx: &mut App) {
    let exit = Rc::clone(&queue.borrow().exit);
    exit(code, cx);
}

#[cfg(test)]
mod tests {
    use std::rc::Weak;

    use gpui_kit::TestAppContext;

    use super::*;

    #[gpui_kit::test]
    fn the_exit_callback_may_borrow_the_queue(cx: &mut TestAppContext) {
        let this: Rc<RefCell<Weak<RefCell<Queue>>>> = Rc::default();
        let exits: Rc<RefCell<Vec<ExitCode>>> = Rc::default();
        let (slot, record) = (Rc::clone(&this), Rc::clone(&exits));
        let queue = Rc::new(RefCell::new(Queue::new(
            PathBuf::new(),
            MergeOptions::default(),
            WindowSettings::default(),
            move |code, _: &mut App| {
                let queue = slot.borrow().upgrade().expect("the test holds the queue");
                queue.borrow_mut().unresolved = true;
                record.borrow_mut().push(code);
            },
        )));
        *this.borrow_mut() = Rc::downgrade(&queue);

        cx.update(|cx| end(&queue, ExitCode::Cancelled, cx));

        assert_eq!(*exits.borrow(), vec![ExitCode::Cancelled]);
        assert!(queue.borrow().unresolved, "the callback's write lands");
    }
}
