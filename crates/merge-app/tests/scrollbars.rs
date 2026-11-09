//! Headless tests of the merge window's scrollbars (spec §5, §6.4): when they show, and
//! where the result's vertical one goes.

use crate::common;

use common::{act, doc, open, open_with};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, Pixels, ScrollDelta, TestAppContext, point, px};
use merge_app::window::{INPUT_LINES_ID, RESULT_ID, ROW_HEIGHT};
use merge_app::{MergeWindow, WindowSettings};
use merge_config::settings::Scrollbars;

const LINES: usize = 200;
/// Frames drawn after a click, so a scroll set in one frame applies in the next.
const SETTLE_FRAMES: usize = 3;
/// Inside a vertical bar: its thumb is 6 px wide, 4 px in from the pane's right edge.
const BAR_FROM_RIGHT: Pixels = px(7.);
/// Inside a horizontal bar, up from the pane's bottom edge.
const BAR_FROM_BOTTOM: Pixels = px(7.);
/// A line longer than any pane is wide.
const LONG_LINE: usize = 2000;

fn same(lines: usize, long: bool) -> String {
    (0..lines)
        .map(|n| {
            if long && n == 0 { format!("{}\n", "x".repeat(LONG_LINE)) } else { format!("l{n}\n") }
        })
        .collect()
}

fn unchanged(long: bool) -> merge_core::document::MergeDocument {
    let text = same(LINES, long);
    doc(&text, &text, &text)
}

fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..SETTLE_FRAMES {
        act(cx, window, TestWindowExt::render_frame);
    }
}

/// Click `id` at `from_right` in from its right edge and `from_bottom` up from its bottom.
fn click_near_corner(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &'static str,
    from_right: Pixels,
    from_bottom: Pixels,
) {
    act(cx, window, |window, cx| {
        let size = window.find(id).bounds().size;
        window.click_at(id, point(size.width - from_right, size.height - from_bottom), cx);
    });
    settle(cx, window);
}

fn rows_top(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    cx.read(|cx| view.read(cx).rows_scroll_top())
}

fn rows_end(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    // One display row per line: the rows do not wrap.
    #[expect(clippy::cast_precision_loss, reason = "200 rows")]
    let rows = LINES as f32;
    ROW_HEIGHT * rows - cx.read(|cx| view.read(cx).rows_view_height())
}

fn result_offset(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> gpui_kit::Point<Pixels> {
    cx.read(|cx| view.read(cx).result_editor().read(cx).scroll_offset())
}

#[gpui_kit::test]
fn the_rows_bar_shows_by_default_so_a_click_at_its_foot_scrolls_to_the_end(
    cx: &mut TestAppContext,
) {
    let (window, view) = open(cx, unchanged(false));
    click_near_corner(cx, window, INPUT_LINES_ID, BAR_FROM_RIGHT, px(1.));
    assert_eq!(rows_top(cx, &view), rows_end(cx, &view));
}

#[gpui_kit::test]
fn a_bar_shown_when_scrolling_lets_a_click_on_an_idle_pane_through(cx: &mut TestAppContext) {
    let settings =
        WindowSettings { scrollbars: Scrollbars::WhenScrolling, ..WindowSettings::default() };
    let (window, view, _) = open_with(cx, unchanged(false), &settings);
    click_near_corner(cx, window, INPUT_LINES_ID, BAR_FROM_RIGHT, px(1.));
    assert_eq!(rows_top(cx, &view), Pixels::ZERO);
}

#[gpui_kit::test]
fn the_result_has_no_vertical_bar_of_its_own(cx: &mut TestAppContext) {
    let (window, view) = open(cx, unchanged(false));
    // Where the result's own vertical bar would take a click halfway down; the caret
    // lands in view, so nothing else scrolls.
    let half = cx.read(|cx| view.read(cx).result_editor().read(cx).input_bounds().size.height) / 2.;
    click_near_corner(cx, window, RESULT_ID, BAR_FROM_RIGHT, half);
    assert_eq!(result_offset(cx, &view).y, Pixels::ZERO);
}

#[gpui_kit::test]
fn the_results_sideways_bar_shows_by_default_and_reaches_the_wheels_end(cx: &mut TestAppContext) {
    let (window, view) = open(cx, unchanged(true));
    act(cx, window, |window, cx| {
        window.scroll(RESULT_ID, ScrollDelta::Pixels(point(px(-100_000.), px(0.))), cx);
    });
    settle(cx, window);
    let wheel_end = result_offset(cx, &view).x;
    assert!(wheel_end < px(-1000.), "the long line scrolls: {wheel_end:?}");
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| {
            state.set_scroll_offset(point(Pixels::ZERO, Pixels::ZERO), cx);
        });
    });
    settle(cx, window);
    click_near_corner(cx, window, RESULT_ID, px(1.), BAR_FROM_BOTTOM);
    assert_eq!(result_offset(cx, &view).x, wheel_end);
}
