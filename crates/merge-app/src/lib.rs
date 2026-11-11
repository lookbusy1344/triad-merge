//! GPUI shell: windows, panes, toolbar, menus and shortcuts. A thin renderer over `merge-view`.

pub mod about;
mod assets;
mod blink;
mod choice;
pub mod diff_window;
mod editor_sync;
mod editor_syntax;
mod go_to_line;
mod input_find;
mod input_select;
mod keys;
mod logo;
pub mod measure;
mod menu_bar;
pub mod open_dialog;
pub mod origin_column;
mod pane_scroll;
mod patches;
mod prompt_view;
mod recorder;
mod recovery;
pub mod resolve_queue;
mod row_wrap;
mod ruler;
mod run;
mod scrollbars;
pub mod settings_banner;
mod settings_file;
mod sideways;
mod split_menu;
mod strings;
mod syntax;
mod syntax_colours;
mod tones;
pub mod toolbar;
pub mod window;
mod wrap;

use gpui_kit::base::input::TabSize;
use std::rc::Rc;

use gpui_kit::{
    App, Bounds, Global, Menu, MenuItem, Pixels, SharedString, Size, WindowBounds, actions, px,
};

use merge_config::keybind::{KeyAction, KeyBindings};
pub use merge_session::ExitCode;
pub use run::run;
pub use window::{MergeWindow, OpenError, WindowSettings, open};

/// Hold the windows to `theme`'s appearance; [`Theme::System`] keeps the system's, which
/// `gpui_kit::init` takes.
fn apply_theme(theme: merge_config::settings::Theme, cx: &mut App) {
    use gpui_kit::component::{Theme as Appearance, ThemeMode};
    use merge_config::settings::Theme;
    let mode = match theme {
        Theme::System => return,
        Theme::Light => ThemeMode::Light,
        Theme::Dark => ThemeMode::Dark,
    };
    Appearance::change(mode, None, cx);
}

/// Take the result of an update through a weak handle. It fails only when the view or window
/// has closed, and then nothing is left to update.
fn ignore_closed_view<T>(update: anyhow::Result<T>) {
    if let Err(error) = update {
        tracing::debug!(%error, "the view closed before its update ran");
    }
}

/// The default text size of the merge and diff windows in points (spec §6.4). GPUI pixels are
/// points on macOS.
const FONT_POINTS: f32 = merge_config::settings::DEFAULT_FONT_SIZE;
/// The default line height as a multiple of the text size, for the editors and the rows
/// beside them.
const LINE_SPACING: f32 = merge_config::settings::DEFAULT_LINE_HEIGHT;
pub(crate) const FONT_SIZE: Pixels = px(FONT_POINTS);
/// The toolbar's default text size: its labels, its conflict badge and its menus.
pub(crate) const TOOLBAR_FONT_SIZE: Pixels = px(merge_config::settings::DEFAULT_TOOLBAR_FONT_SIZE);
/// The height of one line at the default size, in the editors and the merge window's rows
/// alike. GPUI rounds an editor's line height to whole pixels.
pub(crate) const LINE_HEIGHT: Pixels = px((FONT_POINTS * LINE_SPACING).round());

/// A window size from the settings, in pixels.
#[must_use]
pub fn window_size(size: merge_config::settings::WindowSize) -> Size<Pixels> {
    gpui_kit::size(px(size.width.into()), px(size.height.into()))
}

/// Bounds of `size` centred on the primary display. `Bounds::centered` fits them in the
/// display's visible area.
fn window_bounds(size: Size<Pixels>, cx: &App) -> WindowBounds {
    WindowBounds::Windowed(Bounds::centered(None, size, cx))
}

/// The text of a window's panes and toolbar (spec §6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    /// The font family; the theme's monospace font without one.
    pub font: Option<SharedString>,
    pub size: Pixels,
    /// The toolbar's text: its labels, its conflict badge and its menus.
    pub toolbar_size: Pixels,
    /// Tab stops every this many spaces, and the spaces the Tab key types.
    pub tab_width: usize,
    /// Line height as a multiple of [`Self::size`].
    pub spacing: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font: None,
            size: FONT_SIZE,
            toolbar_size: TOOLBAR_FONT_SIZE,
            tab_width: merge_config::settings::DEFAULT_TAB_WIDTH,
            spacing: LINE_SPACING,
        }
    }
}

impl Typography {
    /// One line of an editor, and one row beside it: whole pixels, as GPUI rounds an editor's.
    #[must_use]
    pub fn line_height(&self) -> Pixels {
        px((f32::from(self.size) * self.spacing).round())
    }

    /// The editors' tab size: stops every [`Self::tab_width`] spaces, and the Tab key types
    /// that many spaces.
    pub(crate) fn tab_size(&self) -> TabSize {
        TabSize { tab_size: self.tab_width, hard_tabs: false }
    }
}

/// The commands, one GPUI action per [`KeyAction`] of the same name, and a binding of each.
macro_rules! commands {
    ($($name:ident),+ $(,)?) => {
        actions!(triad, [$($name),+]);

        /// The action of the same name as `action`.
        fn action(action: KeyAction) -> Box<dyn gpui_kit::Action> {
            match action {
                $(KeyAction::$name => Box::new($name),)+
            }
        }
    };
}

commands!(
    Undo,
    Redo,
    NextDifference,
    PreviousDifference,
    NextConflict,
    PreviousConflict,
    ToggleLeft,
    ToggleBase,
    ToggleRight,
    ClearChoice,
    TakeBase,
    TakeLeft,
    TakeRight,
    TakeLeftThenRight,
    TakeRightThenLeft,
    TakeLeftAllUnresolved,
    TakeRightAllUnresolved,
    TakeWholeLeft,
    TakeWholeRight,
    TakeSortedUnion,
    CarryMovedEdit,
    ToggleBasePane,
    ToggleOriginColumn,
    ToggleSyntax,
    ToggleSoftWrap,
    ToggleScrollSync,
    Find,
    GoToLine,
    SaveAndExit,
    SaveCopyAs,
    Save,
    Cancel,
    OpenSettings,
    About,
);

// The top panes' copy and select all (spec §5). Their keys are fixed, as the editors' are.
actions!(triad, [CopyInput, SelectAllInput]);

/// What happens when a window finishes with an exit code (spec §6.2). [`run`] exits the
/// process; tests record the code.
#[derive(Clone)]
pub struct Finish(Rc<FinishFn>);

type FinishFn = dyn Fn(ExitCode, &mut App);

impl Finish {
    pub fn new(finish: impl Fn(ExitCode, &mut App) + 'static) -> Self {
        Self(Rc::new(finish))
    }
}

impl Global for Finish {}

/// End the session with `code`, through the [`Finish`] set for the app.
pub(crate) fn finish(code: ExitCode, cx: &mut App) {
    if let Some(Finish(finish)) = cx.try_global::<Finish>().cloned() {
        finish(code, cx);
    } else {
        tracing::error!(?code, "no finish handler is set; quitting");
        cx.quit();
    }
}

/// Set up the app with the default keys.
pub fn init(cx: &mut App) {
    init_with(&KeyBindings::default(), cx);
}

/// Set up the app with `bindings` (spec §6.3). The menus show them, so they bind first.
pub fn init_with(bindings: &KeyBindings, cx: &mut App) {
    syntax_colours::install(cx);
    syntax::register_queries();
    keys::bind(bindings, cx);
    split_menu::init(cx);
    cx.on_action(|_: &OpenSettings, cx| settings_file::open(cx));
    cx.on_action(|_: &About, cx| about::show(cx));
    cx.set_menus(menus(ViewTicks::default()));
    menu_bar::publish(menus(ViewTicks::default()), cx);
}

/// Which View menu items show a tick: the views the active window shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "each bool is one View menu item's tick")]
pub(crate) struct ViewTicks {
    pub base_pane: bool,
    pub origin_column: bool,
    /// Whether the window has origins, so the View menu holds Blame (spec §7.3).
    pub origins_loaded: bool,
    pub syntax: bool,
    pub soft_wrap: bool,
    pub scroll_sync: bool,
}

/// The menu bar. Each item runs the same action as its key.
pub(crate) fn menus(ticks: ViewTicks) -> Vec<Menu> {
    use strings::menu;
    vec![
        Menu::new(strings::APP_NAME).items([
            MenuItem::action(strings::ABOUT_TITLE, About),
            MenuItem::separator(),
            MenuItem::action(menu::SETTINGS, OpenSettings),
            MenuItem::separator(),
            MenuItem::action(menu::SAVE_AND_EXIT, SaveAndExit),
            MenuItem::action(menu::SAVE_COPY_AS, SaveCopyAs),
            MenuItem::action(menu::SAVE, Save),
            MenuItem::action(menu::CANCEL, Cancel),
        ]),
        Menu::new(menu::EDIT).items([
            MenuItem::action(menu::UNDO, Undo),
            MenuItem::action(menu::REDO, Redo),
            MenuItem::separator(),
            MenuItem::action(menu::FIND, Find),
            MenuItem::action(menu::GO_TO_LINE, GoToLine),
        ]),
        Menu::new(menu::MERGE).items([
            MenuItem::action(menu::TOGGLE_LEFT, ToggleLeft),
            MenuItem::action(menu::TOGGLE_BASE, ToggleBase),
            MenuItem::action(menu::TOGGLE_RIGHT, ToggleRight),
            MenuItem::action(menu::CLEAR_CHOICE, ClearChoice),
            MenuItem::separator(),
            MenuItem::action(menu::TAKE_BASE, TakeBase),
            MenuItem::action(menu::TAKE_LEFT, TakeLeft),
            MenuItem::action(menu::TAKE_RIGHT, TakeRight),
            MenuItem::action(menu::TAKE_LEFT_THEN_RIGHT, TakeLeftThenRight),
            MenuItem::action(menu::TAKE_RIGHT_THEN_LEFT, TakeRightThenLeft),
            MenuItem::action(menu::TAKE_SORTED_UNION, TakeSortedUnion),
            MenuItem::action(menu::CARRY_MOVED_EDIT, CarryMovedEdit),
            MenuItem::separator(),
            MenuItem::action(menu::TAKE_LEFT_ALL_UNRESOLVED, TakeLeftAllUnresolved),
            MenuItem::action(menu::TAKE_RIGHT_ALL_UNRESOLVED, TakeRightAllUnresolved),
            MenuItem::action(menu::TAKE_WHOLE_LEFT, TakeWholeLeft),
            MenuItem::action(menu::TAKE_WHOLE_RIGHT, TakeWholeRight),
        ]),
        Menu::new(menu::GO).items([
            MenuItem::action(menu::NEXT_DIFFERENCE, NextDifference),
            MenuItem::action(menu::PREVIOUS_DIFFERENCE, PreviousDifference),
            MenuItem::action(menu::NEXT_CONFLICT, NextConflict),
            MenuItem::action(menu::PREVIOUS_CONFLICT, PreviousConflict),
        ]),
        // Blame shows only in a window with origins: without them there is nothing to blame.
        Menu::new(menu::VIEW).items(
            [
                Some(MenuItem::action(menu::BASE_PANE, ToggleBasePane).checked(ticks.base_pane)),
                ticks.origins_loaded.then(|| {
                    MenuItem::action(menu::BLAME, ToggleOriginColumn).checked(ticks.origin_column)
                }),
                Some(MenuItem::action(menu::SYNTAX, ToggleSyntax).checked(ticks.syntax)),
                Some(MenuItem::action(menu::SOFT_WRAP, ToggleSoftWrap).checked(ticks.soft_wrap)),
                Some(
                    MenuItem::action(menu::SCROLL_SYNC, ToggleScrollSync)
                        .checked(ticks.scroll_sync),
                ),
            ]
            .into_iter()
            .flatten(),
        ),
    ]
}
