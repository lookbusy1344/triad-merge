//! The stack window (spec §7.5.1): the conflicted jj commits a revset names, parents first,
//! each with its conflicted paths. A path opens in the merge window as `resolve -r` opens it;
//! one merge window at a time. A save refreshes the list. Preview shows what saving the open
//! merge window's result would do to each descendant's conflicts, and writes nothing.
//! Closing the stack window ends the run: 0 when no conflicted commit is left, else 1.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, WeakEntity, Window,
    WindowBounds, WindowOptions, div,
};
use merge_core::document::MergeOptions;
use merge_session::jj_config::ConfigEnv;
use merge_session::jj_native::{Rewrite, StackCommit, Stacked};
use merge_session::repository::RunRepository;
use merge_session::resolve::{Opened, ResolveError};
use vcs::IdKind;

use crate::window::{MergeWindow, WindowSettings};
use crate::{ExitCode, Finish, ignore_closed_view, strings};

/// The element ids tests look up.
pub const PREVIEW_ID: &str = "stack-preview";
pub const REFRESH_ID: &str = "stack-refresh";
pub const NOTE_ID: &str = "stack-note";
pub const EFFECTS_ID: &str = "stack-effects";
const LIST_ID: &str = "stack-list";

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
    /// The save's report; none when it would write nothing.
    Done(Option<Rewrite>),
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
    preview: Option<Preview>,
    /// What happened last: a save, or an error.
    note: Option<SharedString>,
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
        let view = cx.new(|_| StackWindow {
            start,
            exit,
            root: None,
            commits: Vec::new(),
            listing: false,
            open: None,
            loading: false,
            preview: None,
            note: None,
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
            }
            Err(error) => self.note = Some(error.to_string().into()),
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
            let loaded = merge_session::resolve::load_at_commit(
                &root,
                &rev,
                &load_path,
                None,
                config.clone(),
            )
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
        let (exit, code) = (Rc::clone(&self.exit), self.code());
        cx.defer(move |cx| exit(code, cx));
        true
    }

    fn render_commits(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        v_flex().gap_2().children(self.commits.iter().enumerate().map(|(c, commit)| {
            let heading = strings::stack_commit(commit);
            v_flex()
                .gap_1()
                .child(
                    div()
                        .id(ElementId::Name(format!("stack-commit-{c}").into()))
                        .test_support()
                        .role(Role::Heading)
                        .aria_label(heading.clone())
                        .child(heading),
                )
                .children(commit.paths.iter().enumerate().map(|(p, path)| {
                    let label = strings::stack_path(path);
                    h_flex().pl_4().child(
                        Button::new(path_id(c, p))
                            .ghost()
                            .small()
                            .label(label.clone())
                            .accessibility_label(label)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.open_path(c, p, cx);
                            })),
                    )
                }))
        }))
    }

    fn render_preview(&self) -> Option<impl IntoElement + use<>> {
        let text: SharedString = match self.preview.as_ref()? {
            Preview::Running => strings::PREVIEW_RUNNING.into(),
            Preview::Done(rewrite) => strings::stack_preview(rewrite.as_ref()).into(),
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
            .then(|| div().text_color(muted).child(strings::STACK_EMPTY));
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
            .p_4()
            .gap_4()
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
            .child(div().id(LIST_ID).flex_1().overflow_y_scroll().child(self.render_commits(cx)))
            .children(empty)
            .children(self.render_preview())
    }
}
