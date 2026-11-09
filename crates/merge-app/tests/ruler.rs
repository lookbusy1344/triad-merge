//! Headless tests of the overview ruler beside the result (`docs/PLAN.md` §E4).

use crate::common;

use common::{act, caret, doc, open, set_caret};
use gpui_kit::test::{TestEventExt as _, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, Entity, MouseButton, Pixels, ScrollDelta, TestAppContext, point, px,
};
use merge_app::MergeWindow;
use merge_app::window::{RESULT_ID, RULER_ID};

const LINES: usize = 1000;
/// Result lines: 0-39, the conflict's left 40-49 and right 50-59, then 60-1009.
const RESULT_LINES: f32 = 1010.;
/// Frames drawn after a click, so a scroll set in one frame applies in the next.
const SETTLE_FRAMES: usize = 3;
/// Across the strip, inside its 12 px.
const ACROSS: Pixels = px(6.);
/// 10 pt text at 1.5 lines (spec §6.4).
const LINE_HEIGHT: f32 = 15.;

fn numbered(prefix: Option<&str>) -> String {
    (0..LINES)
        .map(|n| match prefix {
            Some(prefix) if (40..50).contains(&n) => format!("{prefix}{n}\n"),
            _ => format!("l{n}\n"),
        })
        .collect()
}

fn conflict(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    open(cx, doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R"))))
}

fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..SETTLE_FRAMES {
        act(cx, window, TestWindowExt::render_frame);
    }
}

/// Click the strip `line` lines of the result down.
fn click_at_line(cx: &mut TestAppContext, window: AnyWindowHandle, line: f32) {
    act(cx, window, |window, cx| {
        let height = f32::from(window.find(RULER_ID).bounds().size.height);
        let y = px(line / RESULT_LINES * height);
        window.click_at(RULER_ID, point(ACROSS, y), cx);
    });
    settle(cx, window);
}

fn result_top(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    cx.read(|cx| -view.read(cx).result_editor().read(cx).scroll_offset().y)
}

#[gpui_kit::test]
fn a_click_on_a_mark_goes_to_its_region(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    click_at_line(cx, window, 50.);
    // Lines l0-l9 take 3 bytes, l10-l39 4: the conflict starts at byte 150.
    assert_eq!(caret(cx, &view), 150);
}

#[gpui_kit::test]
fn a_click_between_marks_scrolls_there_and_leaves_the_caret(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    settle(cx, window);
    click_at_line(cx, window, 500.5);
    let view_height =
        cx.read(|cx| f32::from(view.read(cx).result_editor().read(cx).input_bounds().size.height));
    // Line 500 in the middle of the view.
    assert_eq!(result_top(cx, &view), px(500. * LINE_HEIGHT - view_height / 2.));
    assert_eq!(caret(cx, &view), 0);
}

fn ruler_height(cx: &mut TestAppContext, window: AnyWindowHandle) -> Pixels {
    let mut height = Pixels::ZERO;
    act(cx, window, |window, _| height = window.find(RULER_ID).bounds().size.height);
    height
}

/// Scroll the result so `top` px of it lie above the view.
fn scroll_result(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<MergeWindow>,
    top: f32,
) {
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| {
            state.set_scroll_offset(point(Pixels::ZERO, px(-top)), cx);
        });
    });
    settle(cx, window);
}

/// Press the strip at `from`, move to `to`, both down from its top, and let go there.
fn drag_strip(cx: &mut TestAppContext, window: AnyWindowHandle, from: Pixels, to: Pixels) {
    act(cx, window, |window, cx| {
        let origin = window.find(RULER_ID).bounds().origin;
        let at = |y: Pixels| point(origin.x + ACROSS, origin.y + y);
        window.pointer_down(at(from), MouseButton::Left, cx);
        window.pointer_move(at(to), Some(MouseButton::Left), cx);
        window.pointer_up(at(to), MouseButton::Left, cx);
    });
    settle(cx, window);
}

/// The end of the result's scroll, where the wheel stops; the result is back at its top after.
fn result_end(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<MergeWindow>,
) -> Pixels {
    act(cx, window, |window, cx| {
        window.scroll(RESULT_ID, ScrollDelta::Pixels(point(px(0.), px(-1_000_000.))), cx);
    });
    settle(cx, window);
    let end = result_top(cx, view);
    scroll_result(cx, window, view, 0.);
    end
}

#[gpui_kit::test]
fn the_thumb_drags_the_result_to_its_end_and_back(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    settle(cx, window);
    let end = result_end(cx, window, &view);
    let height = ruler_height(cx, window);
    // The thumb starts at the top; a drag well past the foot holds it there.
    drag_strip(cx, window, px(1.), height + px(100.));
    assert_eq!(result_top(cx, &view), end);
    drag_strip(cx, window, height - px(1.), px(-100.));
    assert_eq!(result_top(cx, &view), Pixels::ZERO);
}

#[gpui_kit::test]
fn a_press_on_the_track_centres_there_and_then_drags_the_thumb(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    settle(cx, window);
    let end = result_end(cx, window, &view);
    let height = ruler_height(cx, window);
    // Line 500 of 1010, between the marks, then on past the foot.
    let line_500 = height * (500.5 / RESULT_LINES);
    drag_strip(cx, window, line_500, height + px(100.));
    assert_eq!(result_top(cx, &view), end);
}

#[gpui_kit::test]
fn a_click_on_a_mark_under_the_thumb_goes_to_its_region(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    // Lines 30 on in view: the conflict's lines 40-59 lie under the thumb.
    scroll_result(cx, window, &view, 30. * LINE_HEIGHT);
    click_at_line(cx, window, 50.);
    assert_eq!(caret(cx, &view), 150);
}

#[gpui_kit::test]
fn a_click_on_the_thumb_off_the_marks_leaves_the_scroll(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    scroll_result(cx, window, &view, 100. * LINE_HEIGHT);
    // Line 110 is in view, under the thumb, and unmarked.
    click_at_line(cx, window, 110.5);
    assert_eq!(result_top(cx, &view), px(100. * LINE_HEIGHT));
    assert_eq!(caret(cx, &view), 0);
}
