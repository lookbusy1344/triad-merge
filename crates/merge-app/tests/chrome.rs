//! Headless tests of the title and status bars (`docs/STYLE.md` § Title bar, § Status bar).
//! The test text system shapes every family alike, so the unit tests of `status_bar` and
//! `toolbar` check the font family; these check the geometry and the text size.

use crate::common;

use common::{act, bounds, doc, open, viewport_width};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Pixels, SharedString, TestAppContext, Window, px};
use merge_app::diff_window::{DiffFile, DiffSettings, Origin};
use merge_app::toolbar::MARK_ID;
use merge_app::window::TOOLBAR_ID;
use merge_core::text::Text;

/// Chrome text (`docs/STYLE.md` § Type).
const CHROME_TEXT: Pixels = px(12.);
/// A status bar's height, a list row's, under its border (`docs/STYLE.md` § Status bar).
const STATUS_HEIGHT: Pixels = px(24.);
/// The border above a status bar, outside its height.
const STATUS_BORDER: Pixels = px(1.);
/// The space between a status bar's text and the window's left edge, a toolbar's end's.
const STATUS_SIDE: Pixels = px(8.);
/// gpui-component's title bar: its element id and its height, Zed's.
const TITLE_BAR_ID: &str = "title-bar";
const TITLE_BAR_HEIGHT: Pixels = px(34.);
/// The room the title bar keeps for macOS's traffic lights, gpui-component's left padding.
const TRAFFIC_LIGHTS: Pixels = px(80.);
/// The title bar's inset in full screen, where macOS hides the traffic lights.
const FULL_SCREEN_INSET: Pixels = px(12.);
/// The space between a toolbar's mark and its start.
const TOOLBAR_SIDE: Pixels = px(8.);
/// Rounding to the pixel grid moves a measured width by at most this.
const ROUNDING: Pixels = px(1.);

/// The width of `text` shaped in `family` at `size`.
fn shaped(window: &Window, family: SharedString, size: Pixels, text: &str) -> Pixels {
    let mut style = window.text_style();
    style.font_family = family;
    let run = style.to_run(text.len());
    window.text_system().shape_line(text.to_owned().into(), size, &[run], None).width
}

/// `text`'s width at `size` in the theme's UI font.
fn ui_width(cx: &mut TestAppContext, window: AnyWindowHandle, size: Pixels, text: &str) -> Pixels {
    let mut width = Pixels::ZERO;
    act(cx, window, |window, cx| {
        width = shaped(window, cx.theme().font_family.clone(), size, text);
    });
    width
}

/// The window's content height.
fn viewport_height(cx: &mut TestAppContext, window: AnyWindowHandle) -> Pixels {
    let mut height = Pixels::ZERO;
    act(cx, window, |window, _| height = window.viewport_size().height);
    height
}

/// A diff window of two two-line files with one difference.
fn open_diff(cx: &mut TestAppContext) -> AnyWindowHandle {
    let file = |name: &str, body: &str| DiffFile {
        origin: Origin::Revision { title: name.to_owned() },
        text: Text::decode(body.as_bytes().to_vec()).expect("decodes"),
        read_only: false,
    };
    let (window, _) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::diff_window::open(
            file("left", "a\nb\n"),
            file("right", "a\nB\n"),
            &DiffSettings::default(),
            cx,
        )
        .expect("window opens")
    });
    act(cx, window, TestWindowExt::render_frame);
    window
}

/// Check that the title bar spans the window's top, `TITLE_BAR_HEIGHT` tall. On macOS it
/// holds the toolbar, whose mark starts `inset` and `TOOLBAR_SIDE` in; elsewhere it holds
/// the menus over the toolbar.
fn assert_title_bar(cx: &mut TestAppContext, window: AnyWindowHandle, inset: Pixels) {
    let width = viewport_width(cx, window);
    let bar = bounds(cx, window, TITLE_BAR_ID);
    assert_eq!((bar.top(), bar.size.height), (Pixels::ZERO, TITLE_BAR_HEIGHT), "bar at the top");
    assert_eq!((bar.left(), bar.size.width), (Pixels::ZERO, width), "bar across the window");
    let tools = bounds(cx, window, TOOLBAR_ID);
    if cfg!(target_os = "macos") {
        assert!(tools.top() >= bar.top() && tools.bottom() <= bar.bottom(), "{tools:?} in {bar:?}");
        assert_eq!(bounds(cx, window, MARK_ID).left(), inset + TOOLBAR_SIDE, "mark's inset");
    } else {
        assert_eq!(tools.top(), bar.bottom(), "toolbar under the title bar");
    }
}

#[gpui_kit::test]
fn the_merge_toolbar_sits_in_the_title_bar_on_macos(cx: &mut TestAppContext) {
    let (window, _) = open(cx, doc("a\nb\n", "a\nL\n", "a\nR\n"));
    assert_title_bar(cx, window, TRAFFIC_LIGHTS);
}

#[gpui_kit::test]
fn the_diff_toolbar_sits_in_the_title_bar_on_macos(cx: &mut TestAppContext) {
    let window = open_diff(cx);
    assert_title_bar(cx, window, TRAFFIC_LIGHTS);
}

#[gpui_kit::test]
fn in_full_screen_the_toolbar_gives_up_the_traffic_lights_room(cx: &mut TestAppContext) {
    let (window, _) = open(cx, doc("a\nb\n", "a\nL\n", "a\nR\n"));
    act(cx, window, |window, _| window.toggle_fullscreen());
    assert_title_bar(cx, window, FULL_SCREEN_INSET);
}

/// Check that status bar `bar` spans the window's foot, `STATUS_HEIGHT` tall, and that
/// `text`, its element `text_id`, starts `STATUS_SIDE` in, in chrome text and the UI font.
fn assert_status_bar(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    bar: &'static str,
    text_id: &'static str,
    text: &str,
) {
    let (width, height) = (viewport_width(cx, window), viewport_height(cx, window));
    let found = bounds(cx, window, bar);
    assert_eq!(found.size.height, STATUS_BORDER + STATUS_HEIGHT, "bar height");
    assert_eq!((found.left(), found.size.width), (Pixels::ZERO, width), "bar across the window");
    assert_eq!(found.bottom(), height, "bar at the window's foot");
    let shown = bounds(cx, window, text_id);
    assert_eq!(shown.left(), STATUS_SIDE, "text's inset");
    let middle = found.top() + STATUS_BORDER + STATUS_HEIGHT / 2.;
    assert_eq!(shown.center().y, middle, "text centred in the bar");
    let expected = ui_width(cx, window, CHROME_TEXT, text);
    assert!((shown.size.width - expected).abs() <= ROUNDING, "{:?} for {expected:?}", shown.size);
}

#[gpui_kit::test]
fn the_merge_status_bar_spans_the_foot_in_chrome_text(cx: &mut TestAppContext) {
    use merge_app::window::{STATUS_ID, STATUS_TEXT_ID};

    let (window, view) = open(cx, doc("a\nb\n", "a\nL\n", "a\nR\n"));
    let status = cx.read(|cx| view.read(cx).status_text().to_string());
    assert_status_bar(cx, window, STATUS_ID, STATUS_TEXT_ID, &status);
}

#[gpui_kit::test]
fn the_diff_status_bar_spans_the_foot_in_chrome_text(cx: &mut TestAppContext) {
    use merge_app::diff_window::{STATUS_ID, STATUS_TEXT_ID};

    let window = open_diff(cx);
    // Spec §5: the repository, the differences and the whitespace mode.
    assert_status_bar(
        cx,
        window,
        STATUS_ID,
        STATUS_TEXT_ID,
        "no repo · 1 difference · whitespace significant",
    );
}
