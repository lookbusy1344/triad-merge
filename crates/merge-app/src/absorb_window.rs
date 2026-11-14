//! The absorb window (spec §7.5.6): the working copy's hunks against its parents, each with
//! the commit it goes into, beside the commits a hunk may go into. Click a hunk, then a
//! commit or Working Copy, to send it there or keep it; the next hunk is then selected.
//! Apply writes the choices as `absorb --apply --listed-at` does and lists the hunks
//! again. Closing the window ends the run with 0.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, WindowBounds,
    WindowOptions, div,
};
use merge_session::jj_config::ConfigEnv;
use merge_session::jj_native::{AbsorbHunk, Absorbed, Destination, HunkChoice};
use merge_session::resolve::ResolveError;

use crate::{ExitCode, ignore_closed_view, strings};

/// The element ids tests look up.
pub const APPLY_ID: &str = "absorb-apply";
pub const REFRESH_ID: &str = "absorb-refresh";
pub const KEEP_ID: &str = "absorb-keep";
pub const NOTE_ID: &str = "absorb-note";
const HUNKS_ID: &str = "absorb-hunks";
const COMMITS_ID: &str = "absorb-commits";

/// The id of the button that selects hunk `hunk`, counted from 0.
#[must_use]
pub fn hunk_id(hunk: usize) -> ElementId {
    ElementId::Name(format!("absorb-hunk-{hunk}").into())
}

/// The id of the button that sends the selected hunk into destination `destination`,
/// counted from 0, children first.
#[must_use]
pub fn destination_id(destination: usize) -> ElementId {
    ElementId::Name(format!("absorb-destination-{destination}").into())
}

/// The window's size: hunks and their lines beside the commits.
const WINDOW_WIDTH: f32 = 900.;
const WINDOW_HEIGHT: f32 = 600.;
/// The commits' column, beside the hunks.
const COMMITS_WIDTH: f32 = 300.;

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// What the absorb window opens with: the paths and `--into` it lists with, and the user's
/// jj config.
pub struct AbsorbStart {
    pub cwd: PathBuf,
    pub paths: Vec<PathBuf>,
    pub into: Option<String>,
    pub jj_config: Option<ConfigEnv>,
}

pub struct AbsorbWindow {
    start: AbsorbStart,
    exit: Rc<ExitFn>,
    /// The last listing; none before the first lands.
    listed: Option<Absorbed>,
    /// Each hunk's destination, by index into the listing's destinations; none to keep it.
    chosen: Vec<Option<usize>>,
    selected: Option<usize>,
    /// The list is loading, or Apply is writing.
    busy: bool,
    /// What happened last: an apply, or an error.
    note: Option<SharedString>,
}

/// Open the absorb window on `start` and list the hunks. `exit` ends the run.
///
/// # Errors
///
/// When the platform refuses the window.
pub fn start(
    start: AbsorbStart,
    exit: impl Fn(ExitCode, &mut App) + 'static,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<AbsorbWindow>)> {
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
        window.set_window_title(strings::ABSORB_TITLE);
        let view = cx.new(|_| AbsorbWindow {
            start,
            exit,
            listed: None,
            chosen: Vec::new(),
            selected: None,
            busy: false,
            note: None,
        });
        let this = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            this.update(cx, AbsorbWindow::close_requested).unwrap_or(true)
        });
        view
    })?;
    view.update(cx, AbsorbWindow::refresh);
    Ok((window, view))
}

impl AbsorbWindow {
    /// The hunks listed, in path order then line order.
    #[must_use]
    pub fn hunks(&self) -> &[AbsorbHunk] {
        self.listed.as_ref().map_or(&[], |listed| &listed.hunks)
    }

    /// The commits a hunk may go into, children first.
    #[must_use]
    pub fn destinations(&self) -> &[Destination] {
        self.listed.as_ref().map_or(&[], |listed| &listed.destinations)
    }

    /// Each hunk's chosen commit, by full ID; none where it stays in the working copy.
    #[must_use]
    pub fn targets(&self) -> Vec<Option<&str>> {
        let destinations = self.destinations();
        self.chosen
            .iter()
            .map(|d| d.and_then(|d| destinations.get(d)).map(|d| d.id.as_str()))
            .collect()
    }

    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// List the hunks again, off the UI thread; the choices start over from the listing.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.run(Vec::new(), false, None, cx);
    }

    /// Write each hunk's choice, checked against the listing's operation, off the UI thread.
    fn apply(&mut self, cx: &mut Context<Self>) {
        let Some(listed) = &self.listed else {
            return;
        };
        if self.busy {
            return;
        }
        let operation = listed.operation.clone();
        let choices = self
            .chosen
            .iter()
            .enumerate()
            .map(|(index, chosen)| {
                let hunk = index + 1;
                match chosen.and_then(|d| listed.destinations.get(d)) {
                    Some(destination) => {
                        HunkChoice::Move { hunk, rev: destination.id.as_str().to_owned() }
                    }
                    None => HunkChoice::Keep(hunk),
                }
            })
            .collect();
        self.run(choices, true, Some(operation), cx);
    }

    /// `absorb` with `choices`, writing when `apply` holds, then [`Self::ran`].
    fn run(
        &mut self,
        choices: Vec<HunkChoice>,
        apply: bool,
        listed_at: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.busy = true;
        let AbsorbStart { cwd, paths, into, jj_config } = &self.start;
        let (cwd, paths, into, config) =
            (cwd.clone(), paths.clone(), into.clone(), jj_config.clone());
        let ran = cx.background_spawn(async move {
            merge_session::jj_native::absorb(
                &cwd,
                &paths,
                into.as_deref(),
                &choices,
                apply,
                listed_at.as_deref(),
                config,
            )
        });
        cx.spawn(async move |this, cx| {
            let ran = ran.await;
            ignore_closed_view(this.update(cx, |this, cx| this.ran(ran, apply, cx)));
        })
        .detach();
        cx.notify();
    }

    fn ran(&mut self, ran: Result<Absorbed, ResolveError>, applied: bool, cx: &mut Context<Self>) {
        self.busy = false;
        match ran {
            Ok(absorbed) if applied => {
                self.note = Some(strings::absorb_note(&absorbed).into());
                // The working copy lost the moved hunks: list what is left.
                self.refresh(cx);
            }
            Ok(listed) => {
                self.chosen = listed
                    .hunks
                    .iter()
                    .map(|hunk| {
                        let target = hunk.target.as_ref()?;
                        listed.destinations.iter().position(|d| &d.change == target)
                    })
                    .collect();
                self.selected = None;
                self.listed = Some(listed);
            }
            Err(error) => self.note = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn select(&mut self, hunk: usize, cx: &mut Context<Self>) {
        self.selected = Some(hunk);
        cx.notify();
    }

    /// Send the selected hunk into `destination`, or keep it with none; then select the
    /// next hunk.
    fn choose(&mut self, destination: Option<usize>, cx: &mut Context<Self>) {
        let Some(hunk) = self.selected else {
            return;
        };
        let Some(chosen) = self.chosen.get_mut(hunk) else {
            return;
        };
        *chosen = destination;
        if hunk + 1 < self.chosen.len() {
            self.selected = Some(hunk + 1);
        }
        cx.notify();
    }

    /// Closing ends the run with 0.
    fn close_requested(&mut self, cx: &mut Context<Self>) -> bool {
        let exit = Rc::clone(&self.exit);
        cx.defer(move |cx| exit(ExitCode::Resolved, cx));
        true
    }

    fn render_hunks(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (mono, muted) = (theme.mono_font_family.clone(), theme.muted_foreground);
        let destinations = self.destinations();
        let rows = self.hunks().iter().zip(&self.chosen).enumerate().map(|(h, (hunk, chosen))| {
            let target = chosen.and_then(|d| destinations.get(d)).map(|d| &d.change);
            let label = strings::absorb_hunk(hunk, target);
            let lines = strings::absorb_lines(hunk);
            v_flex()
                .gap_1()
                .child(
                    h_flex().child(
                        Button::new(hunk_id(h))
                            .ghost()
                            .small()
                            .selected(self.selected == Some(h))
                            .label(label.clone())
                            .accessibility_label(label)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.select(h, cx);
                            })),
                    ),
                )
                .child(
                    div()
                        .pl_4()
                        .font_family(mono.clone())
                        .text_color(muted)
                        .whitespace_normal()
                        .child(lines),
                )
        });
        v_flex().gap_2().children(rows)
    }

    fn render_commits(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let none = self.selected.is_none();
        let counts = |d: Option<usize>| self.chosen.iter().filter(|c| **c == d).count();
        let rows = self.destinations().iter().enumerate().map(|(d, destination)| {
            let label = strings::absorb_destination(&destination.change, counts(Some(d)));
            Button::new(destination_id(d))
                .ghost()
                .small()
                .disabled(none)
                .label(label.clone())
                .accessibility_label(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.choose(Some(d), cx);
                }))
        });
        let keep = strings::absorb_keep(counts(None));
        v_flex().gap_1().children(rows).child(
            Button::new(KEEP_ID)
                .ghost()
                .small()
                .disabled(none)
                .label(keep.clone())
                .accessibility_label(keep)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.choose(None, cx))),
        )
    }
}

impl Render for AbsorbWindow {
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
        let empty = (self.hunks().is_empty() && self.listed.is_some())
            .then(|| div().text_color(muted).child(strings::ABSORB_EMPTY));
        let skipped =
            self.listed.iter().flat_map(|listed| &listed.skipped).map(|(path, reason)| {
                div().text_color(muted).child(strings::absorb_skipped(path.as_str(), reason))
            });
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
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx))),
                    )
                    .child(
                        Button::new(APPLY_ID)
                            .small()
                            .primary()
                            .label(strings::ABSORB_APPLY)
                            .disabled(self.busy || self.hunks().is_empty())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.apply(cx))),
                    ),
            )
            .children(note)
            .child(
                h_flex()
                    .flex_1()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .id(HUNKS_ID)
                            .flex_1()
                            .h_full()
                            .overflow_y_scroll()
                            .child(self.render_hunks(cx))
                            .children(empty)
                            .children(skipped),
                    )
                    .child(
                        div()
                            .id(COMMITS_ID)
                            .w(gpui_kit::px(COMMITS_WIDTH))
                            .h_full()
                            .overflow_y_scroll()
                            .child(self.render_commits(cx)),
                    ),
            )
    }
}
