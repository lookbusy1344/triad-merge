//! Headless tests of the status bars (`docs/STYLE.md` § Status bar). The test text system
//! shapes every family alike, so the unit tests of `status_bar` and `toolbar` check the
//! font family; these check the geometry and the text size.

use crate::common;

use common::{act, bounds, doc, open, viewport_width};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Pixels, SharedString, TestAppContext, Window, px};
use merge_app::diff_window::{DiffFile, DiffSettings, Origin};
use merge_core::text::Text;

/// Chrome text (`docs/STYLE.md` § Type).
const CHROME_TEXT: Pixels = px(12.);
/// A status bar's height, a list row's, under its border (`docs/STYLE.md` § Status bar).
const STATUS_HEIGHT: Pixels = px(24.);
/// The border above a status bar, outside its height.
const STATUS_BORDER: Pixels = px(1.);
/// The space between a status bar's text and the window's left edge, a toolbar's end's.
const STATUS_SIDE: Pixels = px(8.);
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
    // Spec §5: the repository, the differences and the whitespace mode.
    assert_status_bar(
        cx,
        window,
        STATUS_ID,
        STATUS_TEXT_ID,
        "no repo · 1 difference · whitespace significant",
    );
}
