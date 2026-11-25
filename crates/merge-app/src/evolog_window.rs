//! The evolog window (spec §7.5.4): a jj change's versions across its evolog, newest first.
//! A version opens `diff --since COMMIT -r REV` in the directory window: the files the change
//! edited since that version, with a rebase taken out. A version that differs in nothing
//! opens no window and says so. One directory window at a time. Closing the evolog window,
//! or Escape or Cancel, ends the run with 0.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    Styled as _, Window, WindowBounds, WindowOptions, px,
};
use merge_core::diff::DiffOptions;
use merge_session::jj_native::Evolog;
use merge_session::native::InterdiffListing;
use merge_session::repository::Discovery;

use crate::decorations::Fit;
use crate::dir_window::{self, DirStart, Source};
use crate::pick_list::{self, Activate, SelectNext, SelectPrevious, State};
use crate::tool_window::{self, Note};
use crate::window::WindowSettings;
use crate::{ExitCode, ignore_closed_view, strings};
use gpui_kit::SharedString;

/// The element ids tests look up.
pub const REFRESH_ID: &str = "evolog-refresh";
pub const NOTE_ID: &str = "evolog-note";
pub const EMPTY_ID: &str = "evolog-empty";
pub const SUMMARY_ID: &str = "evolog-summary";
const LIST_ID: &str = "evolog-list";

/// The id of the row of version `version`, counted from 0, newest first.
#[must_use]
pub fn version_id(version: usize) -> ElementId {
    ElementId::Name(format!("evolog-version-{version}").into())
}

/// The window's size: a list beside the directory window, not over it.
const WINDOW_WIDTH: Pixels = px(520.);
const WINDOW_HEIGHT: Pixels = px(560.);

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// What the evolog window opens with: the revision and limit as the command line gave them,
/// the versions listed before it opened, and what its directory windows take.
pub struct EvologStart {
    pub cwd: PathBuf,
    /// The commit whose change to list; none for `@`.
    pub rev: Option<String>,
    pub limit: Option<usize>,
    /// The versions listed before the window opened, shown first; none to list them.
    pub listed: Option<Evolog>,
    /// The comparison options each diff window takes.
    pub diff: DiffOptions,
    pub settings: WindowSettings,
    pub discovery: Discovery,
    /// The diff windows watch their files on disk; tests turn it off.
    pub watch: bool,
}

pub struct EvologWindow {
    start: EvologStart,
    exit: Rc<ExitFn>,
    fit: Fit,
    /// The versions listed; none before the first list lands.
    evolog: Option<Evolog>,
    listing: bool,
    /// The directory window open on a version.
    open: Option<AnyWindowHandle>,
    /// A version's files are being listed.
    loading: bool,
    /// A close came while a job ran: the run ends when the job lands (spec §7.5).
    closing: bool,
    note: Option<Note>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    selected: Option<usize>,
}

/// Open the evolog window on `start` and list the versions. `exit` ends the run.
///
/// # Errors
///
/// When the platform refuses the window.
pub fn start(
    start: EvologStart,
    exit: impl Fn(ExitCode, &mut App) + 'static,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<EvologWindow>)> {
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
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let view = cx.new(|_| EvologWindow {
            start,
            exit,
            fit: Fit::default(),
            evolog: None,
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
            this.update(cx, EvologWindow::close_requested).unwrap_or(true)
        });
        view
    })?;
    view.update(cx, |this, cx| match this.start.listed.take() {
        Some(listed) => this.listed(Ok(listed), cx),
        None => this.refresh(cx),
    });
    Ok((window, view))
}

impl EvologWindow {
    /// The versions listed, newest first.
    #[must_use]
    pub fn versions(&self) -> &[merge_session::jj_native::ChangeVersion] {
        self.evolog.as_ref().map_or(&[], |evolog| evolog.versions.as_slice())
    }

    /// The directory window open on a version, if any.
    #[must_use]
    pub fn dir_window(&self) -> Option<AnyWindowHandle> {
        self.open
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_ref().map(Note::text)
    }

    /// List the versions again, off the UI thread.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.listing {
            return;
        }
        self.listing = true;
        let EvologStart { cwd, rev, limit, discovery, .. } = &self.start;
        let (cwd, rev, limit, discovery) = (cwd.clone(), rev.clone(), *limit, discovery.clone());
        let listed = cx.background_spawn(async move {
            merge_session::jj_native::evolog(&cwd, rev.as_deref(), limit, &discovery)
                .map_err(|error| crate::error_text::describe(&error))
        });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            ignore_closed_view(this.update(cx, |this, cx| this.listed(listed, cx)));
        })
        .detach();
        cx.notify();
    }

    fn listed(&mut self, listed: Result<Evolog, String>, cx: &mut Context<Self>) {
        self.listing = false;
        if self.end_if_closing(cx) {
            if let Err(error) = &listed {
                tracing::warn!(%error, "listing failed as the window closed; the run ends anyway");
            }
            return;
        }
        match listed {
            Ok(evolog) => {
                let len = evolog.versions.len();
                self.selected = self.selected.filter(|&ix| ix < len);
                self.evolog = Some(evolog);
            }
            Err(error) => self.note = Some(Note::Error(error.into())),
        }
        cx.notify();
    }

    /// List the files version `version` differs in, off the UI thread, then open them in the
    /// directory window. While a directory window is open, bring it forward instead.
    fn open_version(&mut self, version: usize, cx: &mut Context<Self>) {
        if let Some(window) = self.open {
            cx.defer(move |cx| {
                ignore_closed_view(window.update(cx, |_, window, _| window.activate_window()));
            });
            return;
        }
        let Some(since) = self.versions().get(version).map(|v| v.commit.as_str().to_owned()) else {
            return;
        };
        if self.loading {
            return;
        }
        self.loading = true;
        let EvologStart { cwd, rev, discovery, .. } = &self.start;
        let (cwd, rev, discovery) = (cwd.clone(), rev.clone(), discovery.clone());
        let loaded = cx.background_spawn(async move {
            merge_session::native::interdiff(&cwd, &since, rev.as_deref(), &discovery)
                .map(|listing| (since, listing))
                .map_err(|error| crate::error_text::describe(&error))
        });
        cx.spawn(async move |this, cx| {
            let loaded = loaded.await;
            ignore_closed_view(this.update(cx, |this, cx| this.arrived(loaded, cx)));
        })
        .detach();
        cx.notify();
    }

    fn arrived(
        &mut self,
        loaded: Result<(String, InterdiffListing), String>,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        if self.end_if_closing(cx) {
            if let Err(error) = &loaded {
                tracing::warn!(%error, "a version failed to list as the window closed");
            }
            return;
        }
        let (since, listing) = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                self.note = Some(Note::Error(error.into()));
                return cx.notify();
            }
        };
        if listing.files.is_empty() {
            self.note = Some(Note::Outcome(strings::evolog_no_difference()));
            return cx.notify();
        }
        let title = dir_window::interdiff_title(&listing.interdiff);
        let start = DirStart {
            cwd: self.start.cwd.clone(),
            source: Source::Interdiff { since, rev: self.start.rev.clone() },
            discovery: self.start.discovery.clone(),
            title: Some(title),
            diff: self.start.diff.clone(),
            listed: Some(listing),
            settings: self.start.settings.clone(),
            watch: self.start.watch,
        };
        let this = cx.entity().downgrade();
        let closed = move |_: ExitCode, cx: &mut App| {
            ignore_closed_view(this.update(cx, EvologWindow::dir_closed));
        };
        match dir_window::start(start, closed, cx) {
            Ok((window, _)) => {
                self.open = Some(window);
                self.note = None;
            }
            Err(error) => self.note = Some(Note::Error(format!("{error:#}").into())),
        }
        cx.notify();
    }

    /// The directory window closed.
    fn dir_closed(&mut self, cx: &mut Context<Self>) {
        self.open = None;
        cx.notify();
    }

    /// Whether the window may close: not while a directory window is open, which comes
    /// forward instead. Closing ends the run with 0.
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

    /// End the run when a close waits and no job runs: a listed version opens no window.
    fn end_if_closing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.closing || self.listing || self.loading {
            return false;
        }
        let exit = Rc::clone(&self.exit);
        cx.defer(move |cx| exit(ExitCode::Resolved, cx));
        true
    }

    fn step(&mut self, step: fn(Option<usize>, usize) -> Option<usize>, cx: &mut Context<Self>) {
        self.selected = step(self.selected, self.versions().len());
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
        self.versions().iter().enumerate().map(|(v, version)| {
            let state = State { selected: self.selected == Some(v), ..State::default() };
            pick_list::row(version_id(v), &strings::evolog_version(version), state, cx).on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.selected = Some(v);
                    this.open_version(v, cx);
                }),
            )
        })
    }
}

impl Render for EvologWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The title names the change, which the first listing gives.
        let title: SharedString =
            self.evolog.as_ref().map_or_else(SharedString::default, |evolog| {
                strings::evolog_title(&evolog.short_change_id).into()
            });
        if !title.is_empty() {
            window.set_window_title(&title);
        }
        let muted = cx.theme().muted_foreground;
        let listed = self.evolog.is_some() && !self.listing;
        let summary = listed.then(|| strings::history_summary(self.versions().len()).into());
        let empty = (self.versions().is_empty() && listed)
            .then(|| pick_list::note(EMPTY_ID, strings::history_empty(), muted));
        let refresh = tool_window::refresh_button(REFRESH_ID, self.listing, cx)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx)));
        let this = cx.entity().downgrade();
        tool_window::frame(&self.fit, title, window, cx, move |_, cx| {
            this.update(cx, EvologWindow::close_requested).unwrap_or(true)
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
                    .children(self.render_versions(cx))
                    .children(empty),
            ),
        )
    }
}
