//! The directory diff window (spec §6.1): the files that differ between two directories,
//! between a revision and the working copy, or between two versions of a jj change
//! (spec §7.5.4), by path. A text file opens in the diff window,
//! one at a time: as `diff LEFT RIGHT` opens it, or against the revision as `diff PATH
//! --rev REV` does. Closing that window lists the files again. Binary, undecodable and
//! special files are listed and do not open. Closing the window, or Escape or Cancel,
//! ends the run with 0.

use merge_core::unusual::escaped;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    Styled as _, Window, WindowBounds, WindowOptions, px,
};
use merge_core::diff::DiffOptions;
use merge_core::text::Text;
use merge_session::dir_diff::{self, Change, ChangedFile};
use merge_session::jj_native::Interdiff;
use merge_session::native::{self, Against, InterdiffListing, Listing};
use merge_session::repository::{Discovery, Handoff};

use crate::decorations::Fit;
use crate::diff_window::{self, DiffFile, DiffLookup, DiffSettings};
use crate::pick_list::{self, Activate, SelectNext, SelectPrevious, State};
use crate::run::ToolDiff;
use crate::tool_window::{self, Note};
use crate::window::WindowSettings;
use crate::{ExitCode, Finish, ignore_closed_view, strings};
use gpui_kit::SharedString;
use merge_session::diff_tool::ToolPair;

/// The element ids tests look up.
pub const REFRESH_ID: &str = "dirs-refresh";
pub const NOTE_ID: &str = "dirs-note";
pub const EMPTY_ID: &str = "dirs-empty";
pub const SUMMARY_ID: &str = "dirs-summary";
const LIST_ID: &str = "dirs-list";

/// The id of the row of file `file`, counted from 0 in the listed order.
#[must_use]
pub fn file_id(file: usize) -> ElementId {
    ElementId::Name(format!("dirs-file-{file}").into())
}

/// The window's size: a list beside the diff window, not over it.
const WINDOW_WIDTH: Pixels = px(520.);
const WINDOW_HEIGHT: Pixels = px(560.);

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// What the directory window lists.
pub enum Source {
    /// `diff LEFT_DIR RIGHT_DIR`: two directories, and whether each side is read-only.
    Dirs { left: PathBuf, right: PathBuf, readonly: [bool; 2] },
    /// `diff [--rev REV]`: the working copy's changes against `rev`, or its parent revision,
    /// in the repository holding the working directory.
    Changes { rev: Option<String> },
    /// `diff --since OLD [-r REV]`: a jj change against its earlier version `since`,
    /// rebased onto the change's parents. Both sides are read-only.
    Interdiff { since: String, rev: Option<String> },
}

/// What a listing compared, which a listed file loads from.
#[derive(Clone)]
enum Compared {
    Changes(Against),
    /// The repository's root and the two versions.
    Interdiff {
        root: PathBuf,
        interdiff: Interdiff,
    },
}

/// What the directory window opens with: what it lists, and what its diff windows take.
pub struct DirStart {
    pub cwd: PathBuf,
    pub source: Source,
    /// How the listing and each diff window find their repository.
    pub discovery: Discovery,
    /// The window's title; a generic one without it.
    pub title: Option<String>,
    /// The comparison options each diff window takes.
    pub diff: DiffOptions,
    /// For [`Source::Interdiff`]: the files listed before the window opened, shown first.
    pub listed: Option<InterdiffListing>,
    pub settings: WindowSettings,
    /// The diff windows watch their files on disk; tests turn it off.
    pub watch: bool,
}

pub struct DirWindow {
    start: DirStart,
    exit: Rc<ExitFn>,
    fit: Fit,
    title: SharedString,
    /// The files listed; none before a listing lands.
    files: Option<Vec<ChangedFile>>,
    /// For a repository's listing: what the last listing compared.
    compared: Option<Compared>,
    listing: ListJob,
    /// The diff window open on a file.
    open: Option<AnyWindowHandle>,
    /// A file is loading for its diff window.
    loading: bool,
    /// A close came while a job ran: the run ends when the job lands.
    closing: bool,
    /// The last error.
    note: Option<Note>,
    /// The list's focus, which takes the list keys.
    focus: FocusHandle,
    scroll: ScrollHandle,
    selected: Option<usize>,
}

/// Open the directory window on `start`, take over the app's [`Finish`] so each diff window
/// it opens comes back to it, and list the files. `exit` ends the run.
///
/// # Errors
///
/// When the platform refuses the window.
pub fn start(
    start: DirStart,
    exit: impl Fn(ExitCode, &mut App) + 'static,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<DirWindow>)> {
    crate::apply_theme(start.settings.theme, cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui_kit::Bounds::centered(
            None,
            gpui_kit::size(WINDOW_WIDTH, WINDOW_HEIGHT),
            cx,
        ))),
        ..crate::menu_bar::plain_options()
    };
    let exit: Rc<ExitFn> = Rc::new(exit);
    let (window, view) = gpui_kit::open_window(options, cx, |window, cx| {
        let title = start.title.as_deref().filter(|title| !title.is_empty());
        let title =
            title.map_or_else(strings::dirs_title, |title| escaped(title).into_owned().into());
        window.set_window_title(&title);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let view = cx.new(|_| DirWindow {
            start,
            exit,
            fit: Fit::default(),
            title,
            files: None,
            compared: None,
            listing: ListJob::Idle,
            open: None,
            loading: false,
            closing: false,
            note: None,
            focus,
            scroll: ScrollHandle::new(),
            selected: None,
        });
        let this = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            this.update(cx, DirWindow::close_requested).unwrap_or(true)
        });
        view
    })?;
    let returned = Rc::new(RefCell::new(view.downgrade()));
    cx.set_global(Finish::new(move |_, cx| {
        let view = returned.borrow().clone();
        // The finishing diff window is still borrowed; close it once that ends.
        cx.defer(move |cx| ignore_closed_view(view.update(cx, DirWindow::compared)));
    }));
    view.update(cx, |this, cx| match this.start.listed.take() {
        Some(InterdiffListing { root, interdiff, files }) => {
            this.arrived_list(Ok((files, Some(Compared::Interdiff { root, interdiff }))), cx);
        }
        None => this.refresh(cx),
    });
    Ok((window, view))
}

/// Whether a listing runs off the UI thread.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListJob {
    Idle,
    Running,
    /// A refresh came while it ran: its result may predate that, so it lists again.
    Stale,
}

/// A listing: its files, and for a repository's listing what it compared.
type Listed = Result<(Vec<ChangedFile>, Option<Compared>), String>;

/// List `source`'s files from the working directory `cwd`.
fn list(source: &Source, cwd: &Path, discovery: &Discovery) -> Listed {
    match source {
        Source::Dirs { left, right, .. } => {
            dir_diff::changed_files(left, right).map(|files| (files, None)).map_err(to_string)
        }
        Source::Changes { rev } => {
            let mut repo = discovery.find([cwd]);
            let Listing { against, files } =
                native::changed_files(&mut repo, rev.as_deref()).map_err(to_string)?;
            Ok((files, Some(Compared::Changes(against))))
        }
        Source::Interdiff { since, rev } => {
            let InterdiffListing { root, interdiff, files } =
                native::interdiff(cwd, since, rev.as_deref(), discovery).map_err(to_string)?;
            Ok((files, Some(Compared::Interdiff { root, interdiff })))
        }
    }
}

/// The diff window's sides for `file` of a [`Source::Interdiff`] listing.
fn load_interdiff(
    file: &ChangedFile,
    root: &Path,
    interdiff: &Interdiff,
    discovery: &Discovery,
) -> Result<ToolDiff, String> {
    let from = match &file.change {
        Change::Renamed { from } | Change::Copied { from } => from,
        Change::Added | Change::Deleted | Change::Modified => &file.path,
    };
    let texts =
        native::load_interdiff(root, interdiff, from, &file.path, discovery).map_err(to_string)?;
    Ok(interdiff_diff(interdiff, texts, file.display_path()))
}

/// The change both versions belong to, as titles name it.
fn change_of(interdiff: &Interdiff) -> &str {
    interdiff.new_info.change_id.as_deref().unwrap_or(&interdiff.new_info.commit)
}

/// The directory window's title for `diff --since` (spec §7.5.4).
pub(crate) fn interdiff_title(interdiff: &Interdiff) -> String {
    strings::interdiff_title(change_of(interdiff), &interdiff.old_info.commit)
}

/// A `diff --since` diff window's sides (spec §7.5.4): the file in the earlier version,
/// rebased, on the left and in the change on the right, both read-only. A side without the
/// file opens empty.
pub(crate) fn interdiff_diff(
    interdiff: &Interdiff,
    [old, new]: [Option<Text>; 2],
    shown: String,
) -> ToolDiff {
    let change = change_of(interdiff);
    let side = |text: Option<Text>, title: String| match text {
        Some(text) => DiffFile::revision(text, title),
        None => DiffFile::revision(Text::from_utf8(String::new()), strings::no_file(&shown)),
    };
    let left =
        side(old, strings::interdiff_side(change, &interdiff.old_info.commit, interdiff.rebased));
    let right = side(new, strings::interdiff_side(change, &interdiff.new_info.commit, false));
    ToolDiff { left, right, origins: None, kind: Some(vcs::RepoKind::Jj), title: Some(shown) }
}

fn to_string(error: impl std::error::Error + 'static) -> String {
    crate::error_text::describe(&error)
}

/// The diff window's sides for `file` of a [`Source::Changes`] listing: the file at the
/// listing's revision on the left, read-only, and the working-copy file on the right. A file
/// deleted on disk opens an empty, read-only right side.
fn load_changed(
    file: &ChangedFile,
    against: &Against,
    discovery: &Discovery,
) -> Result<ToolDiff, String> {
    let shown = file.display_path();
    let mut repo = discovery.find([against.root.as_path()]);
    let from = match &file.change {
        Change::Renamed { from } | Change::Copied { from } => from,
        Change::Added | Change::Deleted | Change::Modified => &file.path,
    };
    let loaded = native::load_listed(&mut repo, against, from, &file.path).map_err(to_string)?;
    let path = loaded.right.clone();
    let (left, right) = match file.change {
        Change::Deleted => {
            let empty = Text::decode(Vec::new()).map_err(to_string)?;
            let left = DiffFile::revision(loaded.left, strings::revision_title(&loaded.left_at));
            (left, DiffFile::revision(empty, strings::no_file(&shown)))
        }
        Change::Added | Change::Modified | Change::Renamed { .. } | Change::Copied { .. } => {
            crate::run::native_sides(loaded).map_err(to_string)?
        }
    };
    let kind = repo.kind();
    let rev = against.pinned().map(str::to_owned);
    let origins = DiffLookup::Native { path, rev, repo: Handoff::new(repo) };
    Ok(ToolDiff { left, right, origins: Some(origins), kind, title: Some(shown) })
}

/// The diff window's sides for `file`, below `start`'s directories. A side without the file
/// opens empty and read-only.
fn load_file(
    file: &ChangedFile,
    [left, right]: [PathBuf; 2],
    readonly: [bool; 2],
    cwd: PathBuf,
    discovery: &Discovery,
) -> Result<ToolDiff, String> {
    let shown = file.display_path();
    let absent = || Some(strings::no_file(&shown));
    let (absent, from) = match &file.change {
        Change::Added => ([absent(), None], &file.path),
        Change::Deleted => ([None, absent()], &file.path),
        Change::Modified => ([None, None], &file.path),
        Change::Renamed { from } | Change::Copied { from } => ([None, None], from),
    };
    let paths = [left.join(from), right.join(&file.path)];
    let repo = discovery.of_diff_tool(Some(&cwd), &paths[1]);
    let pair = ToolPair { paths, absent, readonly };
    crate::run::tool_diff(pair, Some(shown), repo, cwd)
        .map_err(|error| crate::error_text::describe(&error))
}

impl DirWindow {
    /// The files listed, by path.
    #[must_use]
    pub fn files(&self) -> &[ChangedFile] {
        self.files.as_deref().unwrap_or_default()
    }

    /// The diff window open on a file, if any.
    #[must_use]
    pub fn diff_window(&self) -> Option<AnyWindowHandle> {
        self.open
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_ref().map(Note::text)
    }

    /// List the files again, off the UI thread; once the running listing lands if one runs.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.listing != ListJob::Idle {
            self.listing = ListJob::Stale;
            return;
        }
        self.listing = ListJob::Running;
        let (source, cwd) = (self.source_owned(), self.start.cwd.clone());
        let discovery = self.start.discovery.clone();
        let listed = cx.background_spawn(async move { list(&source, &cwd, &discovery) });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            ignore_closed_view(this.update(cx, |this, cx| this.arrived_list(listed, cx)));
        })
        .detach();
        cx.notify();
    }

    /// A copy of the source for a job off the UI thread.
    fn source_owned(&self) -> Source {
        match &self.start.source {
            Source::Dirs { left, right, readonly } => {
                Source::Dirs { left: left.clone(), right: right.clone(), readonly: *readonly }
            }
            Source::Changes { rev } => Source::Changes { rev: rev.clone() },
            Source::Interdiff { since, rev } => {
                Source::Interdiff { since: since.clone(), rev: rev.clone() }
            }
        }
    }

    fn arrived_list(&mut self, listed: Listed, cx: &mut Context<Self>) {
        let stale = self.listing == ListJob::Stale;
        self.listing = ListJob::Idle;
        if self.end_if_closing(cx) {
            if let Err(error) = &listed {
                tracing::warn!(%error, "listing failed as the window closed; the run ends anyway");
            }
            return;
        }
        if stale {
            return self.refresh(cx);
        }
        match listed {
            Ok((files, compared)) => {
                // The selection follows its file by path; it goes when the file drops out.
                let selected = self.selected.and_then(|ix| self.files().get(ix));
                self.selected =
                    selected.and_then(|old| files.iter().position(|file| file.path == old.path));
                self.files = Some(files);
                self.compared = compared;
                self.note = None;
            }
            Err(error) => self.note = Some(Note::Error(error.into())),
        }
        cx.notify();
    }

    /// Open file `file` in the diff window, loaded off the UI thread. While a diff window is
    /// open, bring it forward instead. A file that does not open does nothing.
    fn open_file(&mut self, file: usize, cx: &mut Context<Self>) {
        if let Some(window) = self.open {
            cx.defer(move |cx| {
                ignore_closed_view(window.update(cx, |_, window, _| window.activate_window()));
            });
            return;
        }
        let Some(listed) = self.files().get(file).filter(|file| file.opens()).cloned() else {
            return;
        };
        if self.loading {
            return;
        }
        let discovery = self.start.discovery.clone();
        let loaded = match (self.source_owned(), self.compared.clone()) {
            (Source::Dirs { left, right, readonly }, _) => {
                let cwd = self.start.cwd.clone();
                cx.background_spawn(async move {
                    load_file(&listed, [left, right], readonly, cwd, &discovery)
                })
            }
            (_, Some(Compared::Changes(against))) => {
                cx.background_spawn(async move { load_changed(&listed, &against, &discovery) })
            }
            (_, Some(Compared::Interdiff { root, interdiff })) => cx.background_spawn(async move {
                load_interdiff(&listed, &root, &interdiff, &discovery)
            }),
            (Source::Changes { .. } | Source::Interdiff { .. }, None) => return,
        };
        self.loading = true;
        cx.spawn(async move |this, cx| {
            let loaded = loaded.await;
            ignore_closed_view(this.update(cx, |this, cx| this.arrived_file(loaded, cx)));
        })
        .detach();
        cx.notify();
    }

    fn arrived_file(&mut self, loaded: Result<ToolDiff, String>, cx: &mut Context<Self>) {
        self.loading = false;
        if self.end_if_closing(cx) {
            if let Err(error) = &loaded {
                tracing::warn!(%error, "a file failed to load as the window closed");
            }
            return;
        }
        let ToolDiff { left, right, origins, kind, title } = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                self.note = Some(Note::Error(error.into()));
                return cx.notify();
            }
        };
        let base = self.start.settings.clone();
        let settings = DiffSettings {
            watch: self.start.watch,
            ..crate::run::diff_settings(title, self.start.diff.clone(), base, origins, kind)
        };
        match diff_window::open(left, right, &settings, cx) {
            Ok((window, _)) => {
                self.open = Some(window);
                self.note = None;
            }
            Err(error) => self.note = Some(Note::Error(crate::error_text::describe(&error).into())),
        }
        cx.notify();
    }

    /// The open diff window finished: close it and list the files again.
    fn compared(&mut self, cx: &mut Context<Self>) {
        let Some(window) = self.open.take() else {
            return;
        };
        // A window already closing has nothing left to close.
        ignore_closed_view(window.update(cx, |_, window, _| window.remove_window()));
        self.refresh(cx);
    }

    /// Whether the window may close: not while a diff window is open, which comes forward
    /// instead. Closing ends the run with 0.
    fn close_requested(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(window) = self.open {
            cx.defer(move |cx| {
                ignore_closed_view(window.update(cx, |_, window, _| window.activate_window()));
            });
            return false;
        }
        if self.listing != ListJob::Idle || self.loading {
            self.closing = true;
            return false;
        }
        let exit = Rc::clone(&self.exit);
        cx.defer(move |cx| exit(ExitCode::Resolved, cx));
        true
    }

    /// End the run when a close waits and no job runs: a loaded file opens no window.
    fn end_if_closing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.closing || self.listing != ListJob::Idle || self.loading {
            return false;
        }
        let exit = Rc::clone(&self.exit);
        cx.defer(move |cx| exit(ExitCode::Resolved, cx));
        true
    }

    fn step(&mut self, step: fn(Option<usize>, usize) -> Option<usize>, cx: &mut Context<Self>) {
        self.selected = step(self.selected, self.files().len());
        if let Some(ix) = self.selected {
            self.scroll.scroll_to_item(ix);
        }
        cx.notify();
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(pick_list::next, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(pick_list::previous, cx);
    }

    fn activate(&mut self, _: &Activate, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(file) = self.selected {
            self.open_file(file, cx);
        }
    }

    fn render_files(&self, cx: &mut Context<Self>) -> impl Iterator<Item = impl IntoElement> {
        self.files().iter().enumerate().map(|(ix, file)| {
            let state = State {
                selected: self.selected == Some(ix),
                disabled: !file.opens(),
                ..State::default()
            };
            pick_list::row(file_id(ix), &strings::dir_file(file), state, cx).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.selected = Some(ix);
                    this.open_file(ix, cx);
                },
            ))
        })
    }
}

impl Render for DirWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let summary = self.files.as_ref().map(|files| {
            match &self.compared {
                Some(Compared::Changes(against)) => {
                    strings::dirs_summary(files.len(), Some(&against.rev))
                }
                Some(Compared::Interdiff { interdiff, .. }) => {
                    strings::interdiff_summary(files.len(), &interdiff.old_info.commit)
                }
                None => strings::dirs_summary(files.len(), None),
            }
            .into()
        });
        let empty = (self.files.as_ref().is_some_and(Vec::is_empty)
            && self.listing == ListJob::Idle)
            .then(|| pick_list::note(EMPTY_ID, strings::dirs_empty(), muted));
        let refresh = tool_window::refresh_button(REFRESH_ID, self.listing != ListJob::Idle, cx)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx)));
        let this = cx.entity().downgrade();
        tool_window::frame(&self.fit, self.title.clone(), window, cx, move |_, cx| {
            this.update(cx, DirWindow::close_requested).unwrap_or(true)
        })
        .child(tool_window::header(SUMMARY_ID, summary, [refresh], cx))
        .children(tool_window::status(NOTE_ID, self.note.as_ref(), cx))
        .child(
            tool_window::body().child(
                pick_list::list(LIST_ID, &self.focus, &self.scroll)
                    .flex_1()
                    .on_action(cx.listener(Self::select_next))
                    .on_action(cx.listener(Self::select_previous))
                    .on_action(cx.listener(Self::activate))
                    .children(self.render_files(cx))
                    .children(empty),
            ),
        )
    }
}
