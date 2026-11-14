//! The stack window (spec §7.5.1): the conflicted jj commits a revset names, parents first,
//! each with its conflicted paths. A path opens in the merge window as `resolve -r` opens it;
//! one merge window at a time. A save refreshes the list. Preview shows what saving the open
//! merge window's result would do to each descendant's conflicts, and writes nothing.
//! Closing the stack window ends the run: 0 when no conflicted commit is left, else 1.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity,
    FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    WeakEntity, Window, WindowBounds, WindowOptions, div,
};
use merge_core::document::MergeOptions;
use merge_session::jj_config::ConfigEnv;
use merge_session::jj_native::{Rewrite, StackCommit, Stacked};
use merge_session::repository::RunRepository;
use merge_session::resolve::{Opened, ResolveError};
use vcs::IdKind;

use crate::pick_list::{self, Activate, SelectNext, SelectPrevious, State};
use crate::window::{MergeWindow, WindowSettings};
use crate::{ExitCode, Finish, ignore_closed_view, strings};

/// The element ids tests look up.
pub const PREVIEW_ID: &str = "stack-preview";
pub const REFRESH_ID: &str = "stack-refresh";
pub const NOTE_ID: &str = "stack-note";
pub const EFFECTS_ID: &str = "stack-effects";
pub const EMPTY_ID: &str = "stack-empty";
const LIST_ID: &str = "stack-list";

/// The id of commit `commit`'s heading, counted from 0.
#[must_use]
pub fn commit_id(commit: usize) -> ElementId {
    ElementId::Name(format!("stack-commit-{commit}").into())
}

/// The id of the button that opens path `path` of commit `commit`, both counted from 0.
#[must_use]
pub fn path_id(commit: usize, path: usize) -> ElementId {
    ElementId::Name(format!("stack-path-{commit}-{path}").into())
}

/// The window's size: a list beside the merge window, not over it.
const WINDOW_WIDTH: f32 = 420.;
const WINDOW_HEIGHT: f32 = 560.;

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// What the stack window opens with: the revset, the merge options its merge windows take,
/// the settings they open with, and the user's jj config.
pub struct StackStart {
    pub cwd: PathBuf,
    pub revset: Option<String>,
    pub options: MergeOptions,
    pub settings: WindowSettings,
    pub jj_config: Option<ConfigEnv>,
}

/// The merge window open on one of the list's paths.
struct OpenMerge {
    window: AnyWindowHandle,
    view: WeakEntity<MergeWindow>,
    /// The commit's change ID and the path, for the note after it closes.
    change: String,
    path: String,
}

/// The preview of the open merge window's save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    Running,
    /// The save's report.
    Done(Rewrite),
    Failed(String),
}

pub struct StackWindow {
    start: StackStart,
    exit: Rc<ExitFn>,
    /// The repository's root, which the listed paths are relative to; none before the first
    /// list lands.
    root: Option<PathBuf>,
    commits: Vec<StackCommit>,
    /// The list is loading.
    listing: bool,
    open: Option<OpenMerge>,
    /// A path is loading for its merge window.
    loading: bool,
    /// A close came while a job ran: the run ends when the job lands (spec §7.5).
    closing: bool,
    preview: Option<Preview>,
    /// What happened last: a save, or an error.
    note: Option<SharedString>,
    /// The list's focus, which takes the list keys.
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// The selected path, counted from 0 across the commits.
    selected: Option<usize>,
}

/// Open the stack window on `start`, take over the app's [`Finish`] so each merge window it
/// opens comes back to it, and list the commits. `exit` ends the run.
///
/// # Errors
///
/// When the platform refuses the window.
pub fn start(
    start: StackStart,
    exit: impl Fn(ExitCode, &mut App) + 'static,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<StackWindow>)> {
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
        window.set_window_title(strings::STACK_TITLE);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let view = cx.new(|_| StackWindow {
            start,
            exit,
            root: None,
            commits: Vec::new(),
            listing: false,
            open: None,
            loading: false,
            closing: false,
            preview: None,
            note: None,
            focus,
            scroll: ScrollHandle::new(),
            selected: None,
        });
        let this = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            this.update(cx, StackWindow::close_requested).unwrap_or(true)
        });
        view
    })?;
    let returned = Rc::new(RefCell::new(view.downgrade()));
    cx.set_global(Finish::new(move |code, cx| {
        let view = returned.borrow().clone();
        // The finishing merge window is still borrowed; close it once that ends.
        cx.defer(move |cx| ignore_closed_view(view.update(cx, |this, cx| this.merged(code, cx))));
    }));
    view.update(cx, StackWindow::refresh);
    Ok((window, view))
}

impl StackWindow {
    /// The commits listed, parents first.
    #[must_use]
    pub fn commits(&self) -> &[StackCommit] {
        &self.commits
    }

    /// The merge window open on a listed path, if any.
    #[must_use]
    pub fn merge_window(&self) -> Option<AnyWindowHandle> {
        self.open.as_ref().map(|open| open.window)
    }

    #[must_use]
    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The code the run ends with now: 0 when no conflicted commit is left, else 1.
    fn code(&self) -> ExitCode {
        if self.commits.is_empty() { ExitCode::Resolved } else { ExitCode::Conflicts }
    }

    /// List the commits again, off the UI thread.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.listing {
            return;
        }
        self.listing = true;
        let (cwd, revset, config) =
            (self.start.cwd.clone(), self.start.revset.clone(), self.start.jj_config.clone());
        let listed = cx.background_spawn(async move {
            merge_session::jj_native::stacked(&cwd, revset.as_deref(), config)
        });
        cx.spawn(async move |this, cx| {
            let listed = listed.await;
            ignore_closed_view(this.update(cx, |this, cx| this.listed(listed, cx)));
        })
        .detach();
        cx.notify();
    }

    fn listed(&mut self, listed: Result<Stacked, ResolveError>, cx: &mut Context<Self>) {
        self.listing = false;
        match listed {
            Ok(Stacked { root, commits }) => {
                self.root = Some(root.into_std_path_buf());
                self.commits = commits;
                let len = self.path_count();
                self.selected = self.selected.filter(|&ix| ix < len);
            }
            Err(error) => self.note = Some(error.to_string().into()),
        }
        if self.end_if_closing(cx) {
            if let Some(note) = &self.note {
                tracing::warn!(%note, "the window closed with this note unseen");
            }
            return;
        }
        cx.notify();
    }

    /// Open path `path` of commit `commit` in the merge window, loaded off the UI thread. While
    /// a merge window is open, bring it forward instead.
    fn open_path(&mut self, commit: usize, path: usize, cx: &mut Context<Self>) {
        if let Some(open) = &self.open {
            let window = open.window;
            cx.defer(move |cx| {
                ignore_closed_view(window.update(cx, |_, window, _| window.activate_window()));
            });
            return;
        }
        let (Some(root), Some(listed)) = (self.root.clone(), self.commits.get(commit)) else {
            return;
        };
        let Some(conflict) = listed.paths.get(path) else {
            return;
        };
        if self.loading {
            return;
        }
        self.loading = true;
        let rev = listed.id.as_str().to_owned();
        let change = listed.change.id(IdKind::Change).to_owned();
        let path = conflict.path.to_string();
        let (options, config) = (self.start.options, self.start.jj_config.clone());
        let load_path = PathBuf::from(&path);
        let loaded = cx.background_spawn(async move {
            let loaded =
                merge_session::resolve::load_at_commit(&root, &rev, &load_path, config.clone())
                    .map_err(|e| e.to_string())?;
            let opened = loaded.open(options).map_err(|e| e.to_string())?;
            let repo = RunRepository::find([root.as_path()]);
            let repo = match config {
                Some(config) => repo.with_jj_config(config),
                None => repo,
            };
            Ok::<_, String>((opened, repo, root))
        });
        cx.spawn(async move |this, cx| {
            let loaded = loaded.await;
            ignore_closed_view(this.update(cx, |this, cx| this.arrived(loaded, change, path, cx)));
        })
        .detach();
        self.preview = None;
        cx.notify();
    }

    fn arrived(
        &mut self,
        loaded: Result<(Opened, RunRepository, PathBuf), String>,
        change: String,
        path: String,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        if self.end_if_closing(cx) {
            if let Err(error) = &loaded {
                tracing::warn!(%error, "a path failed to load as the window closed");
            }
            return;
        }
        let (opened, repo, root) = match loaded {
            Ok(loaded) => loaded,
            Err(error) => {
                self.note = Some(error.into());
                return cx.notify();
            }
        };
        let (doc, settings) =
            crate::resolve_queue::window_settings(opened, &root, repo, self.start.settings.clone());
        match crate::window::open(doc, &settings, cx) {
            Ok((window, view)) => {
                self.open = Some(OpenMerge { window, view: view.downgrade(), change, path });
                self.note = None;
            }
            Err(error) => self.note = Some(error.to_string().into()),
        }
        cx.notify();
    }

    /// The open merge window finished with `code`: close it, say what the save did, and list
    /// the commits again.
    fn merged(&mut self, code: ExitCode, cx: &mut Context<Self>) {
        let Some(open) = self.open.take() else {
            return;
        };
        // A window already closing has nothing left to close.
        ignore_closed_view(open.window.update(cx, |_, window, _| window.remove_window()));
        self.preview = None;
        self.note = strings::stack_note(code, &open.path, &open.change).map(Into::into);
        self.refresh(cx);
    }

    /// Run the open merge window's save as a dry run, off the UI thread.
    fn start_preview(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.open.as_ref().and_then(|open| open.view.upgrade()) else {
            return;
        };
        let Some(job) = view.read(cx).preview_job() else {
            return;
        };
        self.preview = Some(Preview::Running);
        let previewed = cx.background_spawn(async move { job.run() });
        cx.spawn(async move |this, cx| {
            let previewed = previewed.await;
            ignore_closed_view(this.update(cx, |this, cx| {
                this.preview = Some(match previewed {
                    Ok(rewrite) => Preview::Done(rewrite),
                    Err(error) => Preview::Failed(error.to_string()),
                });
                cx.notify();
            }));
        })
        .detach();
        cx.notify();
    }

    /// Whether the window may close: not while a merge window is open, which comes forward
    /// instead. Closing ends the run.
    fn close_requested(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(open) = &self.open {
            let window = open.window;
            cx.defer(move |cx| {
                ignore_closed_view(window.update(cx, |_, window, _| window.activate_window()));
            });
            return false;
        }
        if self.listing || self.loading {
            self.closing = true;
            return false;
        }
        let (exit, code) = (Rc::clone(&self.exit), self.code());
        cx.defer(move |cx| exit(code, cx));
        true
    }

    /// End the run when a close waits and no job runs, with the code of the last list: a
    /// loaded path opens no window.
    fn end_if_closing(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.closing || self.listing || self.loading {
            return false;
        }
        let (exit, code) = (Rc::clone(&self.exit), self.code());
        cx.defer(move |cx| exit(code, cx));
        true
    }

    /// Every listed path as its commit and its place there, in list order.
    fn paths(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.commits
            .iter()
            .enumerate()
            .flat_map(|(c, commit)| (0..commit.paths.len()).map(move |p| (c, p)))
    }

    fn path_count(&self) -> usize {
        self.commits.iter().map(|commit| commit.paths.len()).sum()
    }

    /// Select the path `step` gives, and scroll its row into view: the list's children are
    /// each commit's heading, then its paths.
    fn step(&mut self, step: fn(Option<usize>, usize) -> Option<usize>, cx: &mut Context<Self>) {
        self.selected = step(self.selected, self.path_count());
        if let Some(ix) = self.selected
            && let Some((c, _)) = self.paths().nth(ix)
        {
            // Each commit's heading comes before its paths.
            self.scroll.scroll_to_item(ix + c + 1);
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
        if let Some((c, p)) = self.selected.and_then(|ix| self.paths().nth(ix)) {
            self.open_path(c, p, cx);
        }
    }

    fn render_commits(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut ix = 0;
        let mut rows = Vec::with_capacity(self.commits.len() + self.path_count());
        for (c, commit) in self.commits.iter().enumerate() {
            rows.push(
                pick_list::heading(commit_id(c), &strings::stack_commit(commit), cx)
                    .into_any_element(),
            );
            for (p, path) in commit.paths.iter().enumerate() {
                let (row, selected) = (ix, self.selected == Some(ix));
                let state = State { selected, indented: true, ..State::default() };
                rows.push(
                    pick_list::row(path_id(c, p), &strings::stack_path(path), state, cx)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.selected = Some(row);
                            this.open_path(c, p, cx);
                        }))
                        .into_any_element(),
                );
                ix += 1;
            }
        }
        rows
    }

    fn render_preview(&self) -> Option<impl IntoElement + use<>> {
        let text: SharedString = match self.preview.as_ref()? {
            Preview::Running => strings::PREVIEW_RUNNING.into(),
            Preview::Done(rewrite) => strings::stack_preview(rewrite).into(),
            Preview::Failed(error) => error.clone().into(),
        };
        Some(
            div()
                .id(EFFECTS_ID)
                .test_support()
                .role(Role::Status)
                .aria_label(text.clone())
                .whitespace_normal()
                .child(text),
        )
    }
}

impl Render for StackWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (background, foreground, muted) =
            (theme.background, theme.foreground, theme.muted_foreground);
        let empty = (self.commits.is_empty() && !self.listing)
            .then(|| pick_list::note(EMPTY_ID, strings::STACK_EMPTY, muted));
        let note = self.note.clone().map(|note| {
            div()
                .id(NOTE_ID)
                .test_support()
                .role(Role::Status)
                .aria_label(note.clone())
                .text_color(muted)
                .child(note)
        });
        let can_preview = self.open.is_some();
        v_flex()
            .size_full()
            .p_3()
            .gap_2()
            .bg(background)
            .text_color(foreground)
            .text_xs()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new(REFRESH_ID)
                            .small()
                            .label(strings::REFRESH)
                            .disabled(self.listing)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx))),
                    )
                    .child(
                        Button::new(PREVIEW_ID)
                            .small()
                            .label(strings::STACK_PREVIEW)
                            .disabled(!can_preview)
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.start_preview(cx)),
                            ),
                    ),
            )
            .children(note)
            .child(
                pick_list::list(LIST_ID, &self.focus, &self.scroll)
                    .flex_1()
                    .on_action(cx.listener(Self::select_next))
                    .on_action(cx.listener(Self::select_previous))
                    .on_action(cx.listener(Self::activate))
                    .children(self.render_commits(cx))
                    .children(empty),
            )
            .children(self.render_preview())
    }
}
