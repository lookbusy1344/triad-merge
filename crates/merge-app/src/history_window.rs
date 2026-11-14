//! The history window (spec §7.5.2): a file's versions in the working-copy commit across
//! jj's operation log, newest first. A version opens in the diff window as
//! `diff PATH --op OP` opens it: the version on the left, the working-copy file on the
//! right, where lines copied back save. One diff window at a time; closing it lists the
//! versions again. Closing the history window ends the run with 0.

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
use merge_session::jj_config::ConfigEnv;
use merge_session::jj_native::{FileHistory, FileVersion};
use merge_session::repository::{Handoff, RunRepository};
use merge_session::resolve::ResolveError;
use vcs::RepoKind;

use crate::diff_window::{self, DiffFile, DiffLookup, DiffSettings};
use crate::pick_list::{self, Activate, SelectNext, SelectPrevious, State};
use crate::window::WindowSettings;
use crate::{ExitCode, Finish, ignore_closed_view, strings};

/// The element ids tests look up.
pub const REFRESH_ID: &str = "history-refresh";
pub const NOTE_ID: &str = "history-note";
pub const EMPTY_ID: &str = "history-empty";
const LIST_ID: &str = "history-list";

/// The id of the button that opens version `version`, counted from 0, newest first.
#[must_use]
pub fn version_id(version: usize) -> ElementId {
    ElementId::Name(format!("history-version-{version}").into())
}

/// The window's size: a list beside the diff window, not over it.
const WINDOW_WIDTH: f32 = 520.;
const WINDOW_HEIGHT: f32 = 560.;

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// What the history window opens with: the file, as the command line gave it, the limit,
/// the settings its diff windows take, and the user's jj config.
pub struct HistoryStart {
    pub cwd: PathBuf,
    pub path: PathBuf,
    pub limit: Option<usize>,
    pub settings: WindowSettings,
    pub jj_config: Option<ConfigEnv>,
    /// The diff windows watch their files on disk; tests turn it off.
    pub watch: bool,
}

/// A diff window's sides and the repository its line origins read, loaded off the UI
/// thread.
struct Loaded {
    left: DiffFile,
    right: DiffFile,
    /// The working-copy file's absolute path.
    file: PathBuf,
    commit: String,
    repo: RunRepository,
}

pub struct HistoryWindow {
    start: HistoryStart,
    exit: Rc<ExitFn>,
    /// The file's absolute path; none before the first list lands.
    file: Option<PathBuf>,
    versions: Vec<FileVersion>,
    /// The list is loading.
    listing: bool,
    /// The diff window open on a version.
    open: Option<AnyWindowHandle>,
    /// A version is loading for its diff window.
    loading: bool,
    /// A close came while a job ran: the run ends when the job lands (spec §7.5).
    closing: bool,
    /// The last error.
    note: Option<SharedString>,
    /// The list's focus, which takes the list keys.
    focus: FocusHandle,
    scroll: ScrollHandle,
    selected: Option<usize>,
}

/// Open the history window on `start`, take over the app's [`Finish`] so each diff window it
/// opens comes back to it, and list the versions. `exit` ends the run.
///
/// # Errors
///
/// When the platform refuses the window.
pub fn start(
    start: HistoryStart,
    exit: impl Fn(ExitCode, &mut App) + 'static,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<HistoryWindow>)> {
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
        window.set_window_title(&strings::history_title(&start.path.to_string_lossy()));
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let view = cx.new(|_| HistoryWindow {
            start,
            exit,
            file: None,
            versions: Vec::new(),
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
            this.update(cx, HistoryWindow::close_requested).unwrap_or(true)
        });
        view
    })?;
    let returned = Rc::new(RefCell::new(view.downgrade()));
    cx.set_global(Finish::new(move |_, cx| {
        let view = returned.borrow().clone();
        // The finishing diff window is still borrowed; close it once that ends.
        cx.defer(move |cx| ignore_closed_view(view.update(cx, HistoryWindow::compared)));
    }));
    view.update(cx, HistoryWindow::refresh);
    Ok((window, view))
}

impl HistoryWindow {
    /// The versions listed, newest first.
    #[must_use]
    pub fn versions(&self) -> &[FileVersion] {
        &self.versions
    }

    /// The diff window open on a version, if any.
    #[must_use]
    pub fn diff_window(&self) -> Option<AnyWindowHandle> {
        self.open
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// List the versions again, off the UI thread. The listing snapshots the working copy,
    /// so a save shows as a new version.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.listing {
            return;
        }
        self.listing = true;
        let HistoryStart { cwd, path, limit, jj_config, .. } = &self.start;
        let (cwd, path, limit, config) = (cwd.clone(), path.clone(), *limit, jj_config.clone());
        let listed = cx.background_spawn(async move {
            merge_session::jj_native::file_history(&cwd, &path, limit, config)
        });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            ignore_closed_view(this.update(cx, |this, cx| this.listed(listed, cx)));
        })
        .detach();
        cx.notify();
    }

    fn listed(&mut self, listed: Result<FileHistory, ResolveError>, cx: &mut Context<Self>) {
        self.listing = false;
        if self.end_if_closing(cx) {
            if let Err(error) = &listed {
                tracing::warn!(%error, "listing failed as the window closed; the run ends anyway");
            }
            return;
        }
        match listed {
            Ok(FileHistory { root, path, versions }) => {
                self.file = Some(root.join(path).into_std_path_buf());
                self.versions = versions;
                let len = self.versions.len();
                self.selected = self.selected.filter(|&ix| ix < len);
            }
            Err(error) => self.note = Some(error.to_string().into()),
        }
        cx.notify();
    }

    /// Open version `version` in the diff window, loaded off the UI thread. While a diff
    /// window is open, bring it forward instead.
    fn open_version(&mut self, version: usize, cx: &mut Context<Self>) {
        if let Some(window) = self.open {
            cx.defer(move |cx| {
                ignore_closed_view(window.update(cx, |_, window, _| window.activate_window()));
            });
            return;
        }
        let (Some(file), Some(listed)) = (self.file.clone(), self.versions.get(version)) else {
            return;
        };
        if self.loading {
            return;
        }
        self.loading = true;
        let (op, config) = (listed.operation.clone(), self.start.jj_config.clone());
        let dir = self.start.cwd.clone();
        let loaded = cx.background_spawn(async move {
            let repo = RunRepository::find([dir.as_path()]);
            let mut repo = match config {
                Some(config) => repo.with_jj_config(config),
                None => repo,
            };
            let (native, commit) = merge_session::native::load_at_operation(&mut repo, &file, &op)
                .map_err(|e| e.to_string())?;
            let (left, right) = crate::run::native_sides(native).map_err(|e| e.to_string())?;
            let commit = commit.as_str().to_owned();
            Ok::<_, String>(Loaded { left, right, file, commit, repo })
        });
        cx.spawn(async move |this, cx| {
            let loaded = loaded.await;
            ignore_closed_view(this.update(cx, |this, cx| this.arrived(loaded, cx)));
        })
        .detach();
        cx.notify();
    }

    fn arrived(&mut self, loaded: Result<Loaded, String>, cx: &mut Context<Self>) {
        self.loading = false;
        if self.end_if_closing(cx) {
            if let Err(error) = &loaded {
                tracing::warn!(%error, "a version failed to load as the window closed");
            }
            return;
        }
        let Loaded { left, right, file, commit, repo } = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                self.note = Some(error.into());
                return cx.notify();
            }
        };
        let origins =
            DiffLookup::Native { path: file, rev: Some(commit), repo: Handoff::new(repo) };
        let base = self.start.settings.clone();
        let options = base.diff_options;
        let settings = DiffSettings {
            watch: self.start.watch,
            ..crate::run::diff_settings(None, options, base, origins, Some(RepoKind::Jj))
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

    /// The open diff window finished: close it and list the versions again.
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

    /// End the run when a close waits and no job runs: a loaded version opens no window.
    fn end_if_closing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.closing || self.listing || self.loading {
            return false;
        }
        let exit = Rc::clone(&self.exit);
        cx.defer(move |cx| exit(ExitCode::Resolved, cx));
        true
    }

    fn step(&mut self, step: fn(Option<usize>, usize) -> Option<usize>, cx: &mut Context<Self>) {
        self.selected = step(self.selected, self.versions.len());
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
        if let Some(version) = self.selected {
            self.open_version(version, cx);
        }
    }

    fn render_versions(&self, cx: &mut Context<Self>) -> impl Iterator<Item = impl IntoElement> {
        self.versions.iter().enumerate().map(|(v, version)| {
            let state = State { selected: self.selected == Some(v), ..State::default() };
            pick_list::row(version_id(v), &strings::history_version(version), state, cx).on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.selected = Some(v);
                    this.open_version(v, cx);
                }),
            )
        })
    }
}

impl Render for HistoryWindow {
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
        let empty = (self.versions.is_empty() && self.file.is_some() && !self.listing)
            .then(|| pick_list::note(EMPTY_ID, strings::HISTORY_EMPTY, muted));
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
                    .children(self.render_versions(cx))
                    .children(empty),
            )
    }
}
