//! Headless tests of visible whitespace (spec §5): View → Show Whitespace draws spaces, tabs,
//! carriage returns and a missing final newline as symbols in every pane.

use crate::common;

use common::{act, doc, open};
use gpui_kit::base::input;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, HighlightStyle, SharedString, TestAppContext};
use merge_app::origin_column::Visibility;
use merge_app::{MergeWindow, ToggleWhitespace, WindowSettings};
use merge_view::rows::RowIx;
use merge_view::tabs;

const LEFT_PANE: usize = 0;
const FIRST_ROW: usize = 0;

/// What the left pane draws on the first row.
fn left_cell(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
) -> (SharedString, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    view.update(cx, |view, cx| view.cell_highlights(LEFT_PANE, RowIx::new(FIRST_ROW), cx))
        .expect("the cell has a line")
}

fn toggle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| window.dispatch_action(Box::new(ToggleWhitespace), cx));
    act(cx, window, TestWindowExt::render_frame);
}

fn result_shows_whitespace(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> bool {
    cx.read(|cx| view.read(cx).result_editor().read(cx).shows_whitespaces())
}

#[gpui_kit::test]
fn the_top_panes_draw_whitespace_symbols_in_the_muted_text_colour(cx: &mut TestAppContext) {
    let (window, view) = open(cx, doc("a b\r\n", "a b\r\n", "a b\r\n"));
    assert_eq!(left_cell(cx, &view).0.as_ref(), "a b");
    toggle(cx, window);
    let (text, highlights) = left_cell(cx, &view);
    let muted = cx.update(|cx| cx.theme().muted_foreground);
    let symbol = HighlightStyle { color: Some(muted), ..HighlightStyle::default() };
    // a, the dot's two bytes, b, then the carriage return's three.
    assert_eq!(text.as_ref(), "a·b␍");
    assert_eq!(highlights, [(1..3, symbol), (4..7, symbol)]);
    toggle(cx, window);
    assert_eq!(left_cell(cx, &view).0.as_ref(), "a b");
}

#[gpui_kit::test]
fn the_toggle_shows_and_hides_the_results_whitespace(cx: &mut TestAppContext) {
    let (window, view) = open(cx, doc("a\n", "a\n", "a\n"));
    assert!(!result_shows_whitespace(cx, &view));
    toggle(cx, window);
    assert!(result_shows_whitespace(cx, &view));
    toggle(cx, window);
    assert!(!result_shows_whitespace(cx, &view));
}

#[gpui_kit::test]
fn the_setting_opens_every_pane_with_whitespace_shown(cx: &mut TestAppContext) {
    let settings = WindowSettings { whitespace: Visibility::Shown, ..WindowSettings::default() };
    let (window, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::open(doc("a\tb", "a\tb", "a\tb"), &settings, cx).expect("window opens")
    });
    act(cx, window, TestWindowExt::render_frame);
    assert!(result_shows_whitespace(cx, &view));
    // The tab runs from column 1 to the stop at 4; the last line has no newline.
    assert_eq!(left_cell(cx, &view).0.as_ref(), "a→  b∅");
}

#[test]
fn the_top_panes_and_the_editors_draw_the_same_symbols() {
    assert_eq!(tabs::SPACE_SYMBOL.to_string(), input::SPACE_SYMBOL);
    assert_eq!(tabs::TAB_SYMBOL.to_string(), input::TAB_SYMBOL);
    assert_eq!(tabs::CARRIAGE_RETURN_SYMBOL.to_string(), input::CARRIAGE_RETURN_SYMBOL);
    assert_eq!(tabs::NO_NEWLINE_SYMBOL.to_string(), input::NO_NEWLINE_SYMBOL);
}
