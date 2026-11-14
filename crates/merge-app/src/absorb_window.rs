//! The absorb window (spec §7.5.4): the working copy's hunks against its parents, each with
//! the commit it goes into, beside the commits a hunk may go into. Click a hunk, then a
//! commit or Working Copy, to send it there or keep it; the next hunk is then selected.
//! From the keys: ↑ and ↓ select a hunk, Enter moves to the commits, on the one the hunk
//! goes into; there ↑ and ↓ move, Enter sends the hunk and comes back, Escape comes back.
//! Apply writes the choices as `absorb --apply --listed-at` does and lists the hunks
//! again. Closing the window ends the run with 0.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, Role, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    WindowBounds, WindowOptions, div, px,
};
use merge_session::jj_config::ConfigEnv;
use merge_session::jj_native::{AbsorbHunk, Absorbed, Destination, HunkChoice};
use merge_session::resolve::ResolveError;
use merge_view::diff::tone;

use crate::pick_list::{self, Activate, Leave, SelectNext, SelectPrevious, State};
use crate::tones::Palette;
use crate::window::WindowSettings;
use crate::{ExitCode, Typography, ignore_closed_view, strings};

/// The element ids tests look up.
pub const APPLY_ID: &str = "absorb-apply";
pub const REFRESH_ID: &str = "absorb-refresh";
pub const KEEP_ID: &str = "absorb-keep";
pub const NOTE_ID: &str = "absorb-note";
pub const EMPTY_ID: &str = "absorb-empty";
const SKIPPED_ID: &str = "absorb-skipped";
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

/// The id of hunk `hunk`'s lines, counted from 0.
#[must_use]
pub fn lines_id(hunk: usize) -> ElementId {
    ElementId::Name(format!("absorb-lines-{hunk}").into())
}

/// The window's size: hunks and their lines beside the commits.
const WINDOW_WIDTH: f32 = 900.;
const WINDOW_HEIGHT: f32 = 600.;
/// The commits' column, beside the hunks.
const COMMITS_WIDTH: Pixels = px(300.);

/// How the run ends: the process exits, or a test records the code.
type ExitFn = dyn Fn(ExitCode, &mut App);

/// What the absorb window opens with: the paths and `--into` it lists with, the settings
/// whose font and colours show the hunks' lines, and the user's jj config.
pub struct AbsorbStart {
    pub cwd: PathBuf,
    pub paths: Vec<PathBuf>,
    pub into: Option<String>,
    pub settings: WindowSettings,
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
    /// The hunks' lines: their font, and their tones by side.
    typography: Typography,
    palette: Palette,
    /// The focus of the hunks' list and the commits', which each take the list keys.
    hunks_focus: FocusHandle,
    commits_focus: FocusHandle,
    hunks_scroll: ScrollHandle,
    commits_scroll: ScrollHandle,
    /// The commit row the keys are on while the commits hold the focus: an index into the
    /// destinations, or one past them for Working copy.
    highlighted: Option<usize>,
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
        let hunks_focus = cx.focus_handle();
        hunks_focus.focus(window, cx);
        let typography = start.settings.typography.clone();
        let palette = Palette::new(&start.settings.colours);
        let view = cx.new(|cx| AbsorbWindow {
            start,
            exit,
            listed: None,
            chosen: Vec::new(),
            selected: None,
            busy: false,
            note: None,
            typography,
            palette,
            hunks_focus,
            commits_focus: cx.focus_handle(),
            hunks_scroll: ScrollHandle::new(),
            commits_scroll: ScrollHandle::new(),
            highlighted: None,
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
        let AbsorbStart { cwd, paths, into, jj_config, .. } = &self.start;
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
                self.highlighted = None;
                self.listed = Some(listed);
            }
            Err(error) => self.note = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn select(&mut self, hunk: usize, cx: &mut Context<Self>) {
        self.selected = Some(hunk);
        self.highlighted = None;
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

    /// The commit rows: each destination, then Working copy.
    fn commit_rows(&self) -> usize {
        self.destinations().len() + 1
    }

    fn select_next_hunk(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_hunk(pick_list::next, cx);
    }

    fn select_previous_hunk(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step_hunk(pick_list::previous, cx);
    }

    fn step_hunk(
        &mut self,
        step: fn(Option<usize>, usize) -> Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.selected = step(self.selected, self.hunks().len());
        if let Some(hunk) = self.selected {
            self.hunks_scroll.scroll_to_item(hunk);
        }
        cx.notify();
    }

    /// Move to the commits, on the row of the selected hunk's choice.
    fn enter_commits(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chosen) = self.selected.and_then(|hunk| self.chosen.get(hunk)) else {
            return;
        };
        let row = chosen.unwrap_or(self.destinations().len());
        self.highlighted = Some(row);
        self.commits_scroll.scroll_to_item(row);
        self.commits_focus.focus(window, cx);
        cx.notify();
    }

    fn select_next_commit(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_commit(pick_list::next, cx);
    }

    fn select_previous_commit(
        &mut self,
        _: &SelectPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_commit(pick_list::previous, cx);
    }

    fn step_commit(
        &mut self,
        step: fn(Option<usize>, usize) -> Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.highlighted = step(self.highlighted, self.commit_rows());
        if let Some(row) = self.highlighted {
            self.commits_scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    /// Send the selected hunk to the highlighted row and go back to the hunks.
    fn choose_highlighted(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.highlighted else {
            return;
        };
        let destination = (row < self.destinations().len()).then_some(row);
        self.choose(destination, cx);
        self.leave_commits(&Leave, window, cx);
    }

    fn leave_commits(&mut self, _: &Leave, window: &mut Window, cx: &mut Context<Self>) {
        self.highlighted = None;
        self.hunks_focus.focus(window, cx);
        cx.notify();
    }

    fn render_hunks(&self, cx: &mut Context<Self>) -> impl Iterator<Item = impl IntoElement> {
        let theme = cx.theme();
        let font = self.typography.font.clone().unwrap_or_else(|| theme.mono_font_family.clone());
        let tones = *self.palette.tones(theme.is_dark());
        let (size, line_height) = (self.typography.size, self.typography.line_height());
        let destinations = self.destinations();
        self.hunks().iter().zip(&self.chosen).enumerate().map(move |(h, (hunk, chosen))| {
            let target = chosen.and_then(|d| destinations.get(d)).map(|d| &d.change);
            let state = State { selected: self.selected == Some(h), ..State::default() };
            let lines = strings::absorb_lines(hunk)
                .into_iter()
                .map(|(side, line)| div().px_2().bg(tones.band(tone(side))).truncate().child(line));
            v_flex()
                .pb_2()
                .child(
                    pick_list::row(hunk_id(h), &strings::absorb_hunk(hunk, target), state, cx)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.select(h, cx);
                        })),
                )
                .child(
                    div()
                        .id(lines_id(h))
                        .test_support()
                        .font_family(font.clone())
                        .text_size(size)
                        .line_height(line_height)
                        .children(lines),
                )
        })
    }

    fn render_commits(&self, cx: &mut Context<Self>) -> impl Iterator<Item = impl IntoElement> {
        let disabled = self.selected.is_none();
        let highlighted = self.highlighted;
        let state =
            move |row| State { selected: highlighted == Some(row), disabled, ..State::default() };
        let counts = |d: Option<usize>| self.chosen.iter().filter(|c| **c == d).count();
        let keep_row = self.destinations().len();
        let keep =
            pick_list::row(KEEP_ID, &strings::absorb_keep(counts(None)), state(keep_row), cx)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.choose(None, cx)));
        let rows = self.destinations().iter().enumerate().map(move |(d, destination)| {
            let content = strings::absorb_destination(&destination.change, counts(Some(d)));
            pick_list::row(destination_id(d), &content, state(d), cx).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.choose(Some(d), cx);
                },
            ))
        });
        rows.chain([keep])
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
            .then(|| pick_list::note(EMPTY_ID, strings::ABSORB_EMPTY, muted));
        let skipped = self.listed.iter().flat_map(|listed| &listed.skipped).enumerate().map(
            |(ix, (path, reason))| {
                let id = ElementId::Name(format!("{SKIPPED_ID}-{ix}").into());
                pick_list::note(id, strings::absorb_skipped(path.as_str(), reason), muted)
            },
        );
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
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx))),
                    )
                    .child(
                        Button::new(APPLY_ID)
                            .small()
                            .label(strings::ABSORB_APPLY)
                            .disabled(self.busy || self.hunks().is_empty())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.apply(cx))),
                    ),
            )
            .children(note)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .gap_4()
                    .child(
                        pick_list::list(HUNKS_ID, &self.hunks_focus, &self.hunks_scroll)
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .on_action(cx.listener(Self::select_next_hunk))
                            .on_action(cx.listener(Self::select_previous_hunk))
                            .on_action(cx.listener(Self::enter_commits))
                            .children(self.render_hunks(cx))
                            .children(empty)
                            .children(skipped),
                    )
                    .child(
                        pick_list::list(COMMITS_ID, &self.commits_focus, &self.commits_scroll)
                            .w(COMMITS_WIDTH)
                            .h_full()
                            .on_action(cx.listener(Self::select_next_commit))
                            .on_action(cx.listener(Self::select_previous_commit))
                            .on_action(cx.listener(Self::choose_highlighted))
                            .on_action(cx.listener(Self::leave_commits))
                            .children(self.render_commits(cx)),
                    ),
            )
    }
}
