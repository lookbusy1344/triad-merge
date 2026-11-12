//! The open dialog (spec §6.1): a Merge tab with four file slots and a Diff tab with two,
//! then a merge or diff window.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use gpui_kit::assets::IconName;
use gpui_kit::base::{Tab, Tabs};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClickEvent, Context, ElementId, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, PathPromptOptions, Pixels, Render,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    TestSupportExt as _, Window, WindowBounds, WindowOptions, div, prelude::FluentBuilder as _, px,
    size,
};
use merge_core::document::Labels;
use merge_session::invocation::MergeFiles;
use merge_session::repository::{Handoff, RunRepository};
use merge_syntax::language::Language;
use merge_view::status::FileNames;

use crate::diff_window::{self, DiffFile, DiffLookup, Origin};
use crate::window::{OpenError, WindowSettings};
use crate::{ExitCode, finish, ignore_closed_view, logo, run, strings};

const WINDOW_WIDTH: Pixels = px(440.);
const WINDOW_HEIGHT: Pixels = px(232.);
const LABEL_WIDTH: Pixels = px(52.);
/// The room around a tab inside the tab strip, and between two tabs.
const TAB_INSET: Pixels = px(2.);
const TAB_HEIGHT: Pixels = px(22.);

/// The button that starts the merge or diff, also used by tests.
pub const START_ID: &str = "start-merge";
pub const ERROR_ID: &str = "open-error";
pub const MARK_ID: &str = "mark";
const TABS_ID: &str = "open-mode";

/// A file slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Base,
    Left,
    Right,
    Output,
}

const SLOTS: [Slot; 4] = [Slot::Base, Slot::Left, Slot::Right, Slot::Output];
const DIFF_SLOTS: [Slot; 2] = [Slot::Left, Slot::Right];

impl Slot {
    /// The slot's path field.
    #[must_use]
    pub fn id(self) -> ElementId {
        ElementId::Name(format!("slot-{self:?}").into())
    }

    /// The slot's button that opens the platform's file picker.
    #[must_use]
    pub fn choose_id(self) -> ElementId {
        ElementId::Name(format!("choose-{self:?}").into())
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// What the dialog opens: a three-way merge or a two-way diff. Each has a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Merge,
    Diff,
}

const MODES: [Mode; 2] = [Mode::Merge, Mode::Diff];

impl Mode {
    /// The mode's tab.
    #[must_use]
    pub fn id(self) -> ElementId {
        ElementId::Name(format!("mode-{self:?}").into())
    }

    /// The slots the mode shows and fills, in order.
    fn slots(self) -> &'static [Slot] {
        match self {
            Self::Merge => &SLOTS,
            Self::Diff => &DIFF_SLOTS,
        }
    }
}

pub struct OpenDialog {
    fields: [Entity<InputState>; 4],
    mode: Mode,
    error: Option<SharedString>,
    settings: WindowSettings,
    watch: bool,
    /// The inputs loading off the UI thread; Start waits for them.
    loading: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// What Start loads off the UI thread.
enum Load {
    Merge { files: MergeFiles, output: PathBuf, settings: WindowSettings },
    Diff { left: PathBuf, right: PathBuf, settings: WindowSettings, watch: bool },
}

/// Opens the window of the inputs a [`Load`] loaded.
type Opener = Box<dyn FnOnce(&mut App) -> Result<(), String> + Send>;

impl Load {
    /// Read the inputs, find their repository and merge them.
    fn run(self) -> Result<Opener, String> {
        match self {
            Self::Merge { files, output, settings } => {
                let language = Language::of_run(files.paths(), Some(&output), None);
                let doc = merge_session::load(&files, settings.options, Labels::none(), language)
                    .map_err(|e| e.to_string())?;
                let settings = WindowSettings {
                    repo: run::repo_kind(settings.cwd.as_deref(), &output),
                    output: Some(output),
                    file_names: FileNames::three_way(&files.base, &files.left, &files.right),
                    ..settings
                };
                Ok(Box::new(move |cx| {
                    crate::window::open(doc, &settings, cx).map(drop).map_err(|e| e.to_string())
                }))
            }
            Self::Diff { left, right, settings, watch } => {
                let cwd = settings.cwd.as_deref();
                let repo = RunRepository::of_diff_tool(cwd, &right);
                let kind = repo.kind();
                let origins = DiffLookup::Tool {
                    paths: [left.clone(), right.clone()],
                    title: None,
                    cwd: run::working_dir(cwd),
                    repo: Handoff::new(repo),
                };
                let read = |path: PathBuf| {
                    merge_session::read_text(&path)
                        .map(|text| DiffFile { origin: Origin::File(path), text, read_only: false })
                        .map_err(|e| e.to_string())
                };
                let left = read(left)?;
                let right = read(right)?;
                let diff_options = settings.diff_options;
                let settings = diff_window::DiffSettings {
                    watch,
                    ..run::diff_settings(None, diff_options, settings, origins, kind)
                };
                Ok(Box::new(move |cx| {
                    diff_window::open(left, right, &settings, cx)
                        .map(drop)
                        .map_err(|e| e.to_string())
                }))
            }
        }
    }
}

/// Open the dialog. The merge window it starts gets `settings`, with the output slot's path;
/// the diff window gets their text, colour and scroll settings, and watches its files when
/// `watch` is set (spec §6.1).
///
/// # Errors
///
/// [`OpenError::Window`] when the platform refuses a window.
pub fn open(
    settings: WindowSettings,
    watch: bool,
    cx: &mut App,
) -> Result<(AnyWindowHandle, Entity<OpenDialog>), OpenError> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(WINDOW_WIDTH, WINDOW_HEIGHT),
            cx,
        ))),
        ..crate::menu_bar::plain_options()
    };
    Ok(gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title(strings::open_title(Mode::default()));
        // Closing the dialog cancels: nothing is written.
        window.on_window_should_close(cx, |_, cx| {
            finish(ExitCode::Cancelled, cx);
            true
        });
        cx.new(|cx| OpenDialog::new(settings, watch, window, cx))
    })?)
}

impl OpenDialog {
    fn new(
        settings: WindowSettings,
        watch: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields = SLOTS.map(|_| cx.new(|cx| InputState::new(window, cx)));
        let subscriptions = fields
            .iter()
            .map(|field| {
                cx.subscribe_in(field, window, |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.start(window, cx);
                    }
                })
            })
            .collect();
        Self {
            fields,
            mode: Mode::default(),
            error: None,
            settings,
            watch,
            loading: None,
            _subscriptions: subscriptions,
        }
    }

    /// The slots' text: base, left, right, output.
    #[must_use]
    pub fn paths(&self, cx: &App) -> [String; 4] {
        self.fields.each_ref().map(|field| field.read(cx).value().to_string())
    }

    /// The tab showing.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Show `mode`'s tab. The slots keep their paths; the last error goes.
    fn select(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.error = None;
        window.set_window_title(strings::open_title(mode));
        cx.notify();
    }

    /// Why the last start failed.
    #[must_use]
    pub fn error(&self) -> Option<&SharedString> {
        self.error.as_ref()
    }

    /// Load the inputs off the UI thread and open their window, or say why not and stay
    /// open. A start while the inputs load does nothing.
    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading.is_some() {
            return;
        }
        let load = match self.load(cx) {
            Ok(load) => load,
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
                return;
            }
        };
        self.error = None;
        self.loading = Some(cx.spawn_in(window, async move |this, cx| {
            let loaded = cx.background_spawn(async move { load.run() }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.loaded(loaded, window, cx);
            }));
        }));
        cx.notify();
    }

    /// What Start loads: the slots' paths, once each the mode needs is filled.
    fn load(&self, cx: &App) -> Result<Load, String> {
        let paths = self.paths(cx);
        if let Some(&slot) =
            self.mode.slots().iter().find(|slot| paths[slot.index()].trim().is_empty())
        {
            return Err(strings::missing_slot(slot));
        }
        let [base, left, right, output] = paths.map(PathBuf::from);
        let settings = self.settings.clone();
        Ok(match self.mode {
            Mode::Merge => {
                Load::Merge { files: MergeFiles { base, left, right }, output, settings }
            }
            Mode::Diff => Load::Diff { left, right, settings, watch: self.watch },
        })
    }

    /// Open the window of the inputs `loaded`, or say why not.
    fn loaded(
        &mut self,
        loaded: Result<Opener, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.loading = None;
        match loaded.and_then(|open| open(cx)) {
            Ok(()) => window.remove_window(),
            Err(error) => self.error = Some(error.into()),
        }
        cx.notify();
    }

    /// Fill `slot` from the platform's file picker. A cancelled picker leaves it as it was.
    fn choose(&mut self, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        let chosen: Pin<Box<dyn Future<Output = Option<PathBuf>>>> = if slot == Slot::Output {
            let picked = cx.prompt_for_new_path(&self.start_directory(cx), None);
            Box::pin(async move { picked.await.ok()?.ok()? })
        } else {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: None,
            });
            Box::pin(async move { picked.await.ok()?.ok()??.into_iter().next() })
        };
        let field = self.fields[slot.index()].clone();
        cx.spawn_in(window, async move |_, cx| {
            let Some(path) = chosen.await else {
                return;
            };
            let text = path.display().to_string();
            // The dialog may have closed while the picker was open.
            ignore_closed_view(cx.update(|window, cx| {
                field.update(cx, |state, cx| state.set_value(text, window, cx));
            }));
        })
        .detach();
    }

    /// Where the output picker starts: base's directory, or the working directory.
    fn start_directory(&self, cx: &App) -> PathBuf {
        let base = self.fields[Slot::Base.index()].read(cx).value();
        Path::new(base.as_ref())
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .or_else(|| self.settings.cwd.clone())
            .unwrap_or_default()
    }

    fn render_slot(&self, slot: Slot, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let name = strings::slot_name(slot);
        let choose = strings::choose_label(slot);
        h_flex()
            .w_full()
            .gap_2()
            .child(
                div()
                    .w(LABEL_WIDTH)
                    .flex_none()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(name),
            )
            .child(div().flex_1().child(
                Input::new(&self.fields[slot.index()]).id(slot.id()).aria_label(name).small(),
            ))
            .child(
                Button::new(slot.choose_id())
                    .ghost()
                    .small()
                    .icon(IconName::FolderOpen)
                    .tooltip(choose.clone())
                    .accessibility_label(choose)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose(slot, window, cx);
                    })),
            )
    }

    /// The Merge and Diff tabs, as one strip with the chosen tab raised.
    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (strip, raised, border) = (theme.muted, theme.background, theme.border);
        let (text, muted) = (theme.foreground, theme.muted_foreground);
        let radius = theme.radius;
        Tabs::new(TABS_ID).flex().p(TAB_INSET).gap(TAB_INSET).rounded(radius).bg(strip).children(
            MODES.into_iter().enumerate().map(|(ix, mode)| {
                let selected = mode == self.mode;
                let label = strings::mode_name(mode);
                Tab::new(mode.id())
                    .selected(selected)
                    .accessibility_label(label)
                    .set_position(ix + 1, MODES.len())
                    .h(TAB_HEIGHT)
                    .px_3()
                    .rounded(radius - TAB_INSET)
                    .text_xs()
                    .border_1()
                    .map(|tab| {
                        if selected {
                            tab.bg(raised).border_color(border).text_color(text)
                        } else {
                            tab.border_color(strip)
                                .text_color(muted)
                                .hover(|style| style.text_color(text))
                        }
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select(mode, window, cx);
                    }))
            }),
        )
    }
}

impl Render for OpenDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let danger = cx.theme().danger;
        let dark = cx.theme().is_dark();
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_sm()
            .p_3()
            .gap_2()
            .child(
                h_flex()
                    .flex_none()
                    .gap_2()
                    .mb_1()
                    .child(
                        div()
                            .id(MARK_ID)
                            .test_support()
                            .role(Role::Image)
                            .aria_label(strings::APP_NAME)
                            .child(logo::mark(dark)),
                    )
                    .child(self.render_tabs(cx)),
            )
            .children(self.mode.slots().iter().map(|&slot| self.render_slot(slot, cx)))
            // The error takes the height left over and scrolls, so the start button stays in
            // view.
            .child(
                div()
                    .id(ERROR_ID)
                    .test_support()
                    .when_some(self.error.clone(), |d, error| d.role(Role::Alert).aria_label(error))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .text_xs()
                    .when_some(self.error.clone(), |d, error| d.text_color(danger).child(error)),
            )
            .child(
                h_flex().w_full().flex_none().justify_end().child(
                    Button::new(START_ID)
                        .primary()
                        .small()
                        .label(strings::start(self.mode))
                        .loading(self.loading.is_some())
                        .accessibility_label(strings::start(self.mode))
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.start(window, cx);
                        })),
                ),
            )
    }
}
