//! The directory diff window (spec §6.1): the files that differ between two directories,
//! by path. A text file opens in the diff window as `diff LEFT RIGHT` opens it, one at a
//! time; closing that window lists the files again. Binary, undecodable and special files
//! are listed and do not open. Closing the window ends the run with 0.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    WindowBounds, WindowOptions, div,
};
use merge_core::diff::DiffOptions;
use merge_session::dir_diff::{self, Change, ChangedFile, DirDiffError};
use merge_session::native::NO_FILE;
use merge_session::repository::RunRepository;

use crate::diff_window::{self, DiffSettings};
use crate::pick_list::{self, Activate, SelectNext, SelectPrevious, State};
use crate::run::{ToolDiff, ToolPair};
use crate::window::WindowSettings;
use crate::{ExitCode, Finish, ignore_closed_view, strings};

/// The element ids tests look up.
pub const REFRESH_ID: &str = "dirs-refresh";
pub const NOTE_ID: &str = "dirs-note";
pub const EMPTY_ID: &str = "dirs-empty";
const LIST_ID: &str = "dirs-list";

/// The id of the row of file `file`, counted from 0 in the listed order.
#[must_use]
pub fn file_id(file: usize) -> ElementId {
    ElementId::Name(format!("dirs-file-{file}").into())
}

/// The window's size: a list beside the diff window, not over it.
const WINDOW_WIDTH: f32 = 520.;
const WINDOW_HEIGHT: f32 = 560.;

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// What the directory window opens with: the two directories and the flags of
/// `diff LEFT_DIR RIGHT_DIR`, and what its diff windows take.
pub struct DirStart {
    pub cwd: PathBuf,
    pub left: PathBuf,
    pub right: PathBuf,
    /// The window's title; a generic one without it.
    pub title: Option<String>,
    pub readonly_left: bool,
    pub readonly_right: bool,
    /// The comparison options each diff window takes.
    pub diff: DiffOptions,
    pub settings: WindowSettings,
    /// The diff windows watch their files on disk; tests turn it off.
    pub watch: bool,
}

pub struct DirWindow {
    start: DirStart,
    exit: Rc<ExitFn>,
    /// The files listed; none before a listing lands.
    files: Option<Vec<ChangedFile>>,
    /// The list is loading.
    listing: bool,
    /// The diff window open on a file.
    open: Option<AnyWindowHandle>,
    /// A file is loading for its diff window.
    loading: bool,
    /// A close came while a job ran: the run ends when the job lands.
    closing: bool,
    /// The last error.
    note: Option<SharedString>,
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
            gpui_kit::size(gpui_kit::px(WINDOW_WIDTH), gpui_kit::px(WINDOW_HEIGHT)),
            cx,
        ))),
        ..crate::menu_bar::plain_options()
    };
    let exit: Rc<ExitFn> = Rc::new(exit);
    let (window, view) = gpui_kit::open_window(options, cx, |window, cx| {
        let title = start.title.as_deref().filter(|title| !title.is_empty());
        window.set_window_title(title.unwrap_or(strings::DIRS_TITLE));
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let view = cx.new(|_| DirWindow {
            start,
            exit,
            files: None,
            listing: false,
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
    view.update(cx, DirWindow::refresh);
    Ok((window, view))
}

/// The diff window's sides for `file`, below `start`'s directories. A side without the file
/// opens empty and read-only.
fn load_file(
    file: &ChangedFile,
    [left, right]: [PathBuf; 2],
    readonly: [bool; 2],
    cwd: PathBuf,
) -> Result<ToolDiff, String> {
    let shown = file.display_path();
    let absent = || Some(format!("{shown} {NO_FILE}"));
    let absent = match file.change {
        Change::Added => [absent(), None],
        Change::Deleted => [None, absent()],
        Change::Modified => [None, None],
    };
    let paths = [left.join(&file.path), right.join(&file.path)];
    let repo = RunRepository::of_diff_tool(Some(&cwd), &paths[1]);
    let pair = ToolPair { paths, absent, readonly, title: Some(shown) };
    crate::run::tool_diff(pair, repo, cwd).map_err(|error| error.to_string())
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
        self.note.as_deref()
    }

    /// List the files again, off the UI thread.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.listing {
            return;
        }
        self.listing = true;
        let (left, right) = (self.start.left.clone(), self.start.right.clone());
        let listed = cx.background_spawn(async move { dir_diff::changed_files(&left, &right) });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            ignore_closed_view(this.update(cx, |this, cx| this.arrived_list(listed, cx)));
        })
        .detach();
        cx.notify();
    }

    fn arrived_list(
        &mut self,
        listed: Result<Vec<ChangedFile>, DirDiffError>,
        cx: &mut Context<Self>,
    ) {
        self.listing = false;
        if self.end_if_closing(cx) {
            if let Err(error) = &listed {
                tracing::warn!(%error, "listing failed as the window closed; the run ends anyway");
            }
            return;
        }
        match listed {
            Ok(files) => {
                let len = files.len();
                self.files = Some(files);
                self.note = None;
                self.selected = self.selected.filter(|&ix| ix < len);
            }
            Err(error) => self.note = Some(error.to_string().into()),
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
        self.loading = true;
        let DirStart { cwd, left, right, readonly_left, readonly_right, .. } = &self.start;
        let dirs = [left.clone(), right.clone()];
        let (readonly, cwd) = ([*readonly_left, *readonly_right], cwd.clone());
        let loaded = cx.background_spawn(async move { load_file(&listed, dirs, readonly, cwd) });
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
                self.note = Some(error.into());
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
            Err(error) => self.note = Some(error.to_string().into()),
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
        if self.listing || self.loading {
            self.closing = true;
            return false;
        }
        let exit = Rc::clone(&self.exit);
        cx.defer(move |cx| exit(ExitCode::Resolved, cx));
        true
    }

    /// End the run when a close waits and no job runs: a loaded file opens no window.
    fn end_if_closing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.closing || self.listing || self.loading {
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (background, foreground, muted) =
            (theme.background, theme.foreground, theme.muted_foreground);
        let note = self.note.clone().map(|note| {
            div()
                .id(NOTE_ID)
                .test_support()
                .role(Role::Status)
                .aria_label(note.clone())
                .text_color(muted)
                .whitespace_normal()
                .child(note)
        });
        let empty = (self.files.as_ref().is_some_and(Vec::is_empty) && !self.listing)
            .then(|| pick_list::note(EMPTY_ID, strings::DIRS_EMPTY, muted));
        v_flex()
            .size_full()
            .p_3()
            .gap_2()
            .bg(background)
            .text_color(foreground)
            .text_xs()
            .child(
                h_flex().gap_2().child(
                    Button::new(REFRESH_ID)
                        .small()
                        .label(strings::REFRESH)
                        .disabled(self.listing)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx))),
                ),
            )
            .children(note)
            .child(
                pick_list::list(LIST_ID, &self.focus, &self.scroll)
                    .flex_1()
                    .on_action(cx.listener(Self::select_next))
                    .on_action(cx.listener(Self::select_previous))
                    .on_action(cx.listener(Self::activate))
                    .children(self.render_files(cx))
                    .children(empty),
            )
    }
}
