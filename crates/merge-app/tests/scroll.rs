//! Headless tests of scroll sync between the rows and the result (spec §5; DESIGN.md §A6.3).

use crate::common;
use merge_core::index::Line;
use merge_view::rows::RowIx;

use common::{act, doc, open, set_caret};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, ElementId, Entity, InputEvent as _, Pixels, ScrollDelta, ScrollEvent,
    TestAppContext, TouchPhase, point, px,
};
use merge_app::MergeWindow;
use merge_app::toolbar::Tool;
use merge_app::window::{INPUT_LINES_ID, RESULT_ID, ROW_HEIGHT, cell_id};

const LINES: usize = 200;
const CONFLICT_LINE: usize = 2;
/// Frames drawn after a scroll, so a change made in one frame shows in the next.
const SETTLE_FRAMES: usize = 3;

fn numbered(edit: Option<&str>) -> String {
    (0..LINES)
        .map(|n| match edit {
            Some(text) if n == CONFLICT_LINE => format!("{text}\n"),
            _ => format!("l{n}\n"),
        })
        .collect()
}

/// Line 2 conflicts. Rows: 0-1 unchanged, 2 the conflict, 3-199 unchanged. Result lines:
/// 0-1, then 2-3 for the conflict (left, then right), then 4-200.
fn conflict(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    open(cx, doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R"))))
}

fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..SETTLE_FRAMES {
        act(cx, window, TestWindowExt::render_frame);
    }
}

/// Scroll the pane under `id` down by `by`.
fn wheel(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>, by: Pixels) {
    act(cx, window, |window, cx| {
        window.scroll(id, ScrollDelta::Pixels(point(px(0.), -by)), cx);
    });
    settle(cx, window);
}

/// The rows' and the result's scroll tops, and the result's line height.
fn tops(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> (Pixels, Pixels, Pixels) {
    cx.read(|cx| {
        let view = view.read(cx);
        let result = view.result_editor().read(cx);
        (
            view.rows_scroll_top(),
            -result.scroll_offset().y,
            result.line_height().expect("the result is laid out"),
        )
    })
}

/// The rows' and the result's view heights.
fn views(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> (Pixels, Pixels) {
    cx.read(|cx| {
        let view = view.read(cx);
        (view.rows_view_height(), view.result_editor().read(cx).input_bounds().size.height)
    })
}

// Near its top a pane's anchor is twice its top: it starts at the top edge and reaches the
// middle of the view half a view down (spec §5). Further down it sits at the middle.

#[gpui_kit::test]
fn scrolling_the_rows_moves_the_result_past_the_conflict(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW_HEIGHT * 5.5);
    // The rows' anchor is at row 11, past the conflict's one row: result line 12, the
    // result's anchor at twice its top.
    let (rows, result, line) = tops(cx, &view);
    assert_eq!(rows, ROW_HEIGHT * 5.5);
    assert_eq!(result, line * 6.);
}

#[gpui_kit::test]
fn scrolling_the_result_through_the_conflict_moves_the_rows_through_its_row(
    cx: &mut TestAppContext,
) {
    let (window, view) = conflict(cx);
    let line = tops(cx, &view).2;
    wheel(cx, window, RESULT_ID, line * 1.5);
    // The result's anchor at line 3 is half way through the conflict's lines 2-3: half way
    // through its row 2, so the rows' anchor is at row 2.5.
    let (rows, result, _) = tops(cx, &view);
    assert_eq!(result, line * 1.5);
    assert_eq!(rows, ROW_HEIGHT * 1.25);
    // Anchor at line 3.5, three quarters through: row 2.75.
    wheel(cx, window, RESULT_ID, line * 0.25);
    assert_eq!(tops(cx, &view).0, ROW_HEIGHT * 1.375);
}

#[gpui_kit::test]
fn scrolling_the_result_past_the_conflict_moves_the_rows(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    let line = tops(cx, &view).2;
    wheel(cx, window, RESULT_ID, line * 20.25);
    // Both anchors sit at the middles of their views. Past the conflict a result line is
    // one row less, and the views differ in height.
    let (rows_view, result_view) = views(cx, &view);
    let (rows, _, _) = tops(cx, &view);
    assert_eq!(rows, ROW_HEIGHT * 19.25 + (result_view - rows_view) / 2.);
}

#[gpui_kit::test]
fn the_pane_scrolled_last_leads(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW_HEIGHT * 5.5);
    let line = tops(cx, &view).2;
    // From result line 6 down to 20.25: the rows as above.
    wheel(cx, window, RESULT_ID, line * 14.25);
    let (rows_view, result_view) = views(cx, &view);
    let (rows, result, _) = tops(cx, &view);
    assert_eq!(result, line * 20.25);
    assert_eq!(rows, ROW_HEIGHT * 19.25 + (result_view - rows_view) / 2.);
}

#[gpui_kit::test]
fn synced_panes_stay_put(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW_HEIGHT * 40.);
    let synced = tops(cx, &view);
    settle(cx, window);
    assert_eq!(tops(cx, &view), synced, "no echo between the panes");
}

#[gpui_kit::test]
fn a_caret_moved_off_screen_moves_the_rows(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    // Result line 150 is row 3 + (150 - 4) = 149.
    let at = cx.read(|cx| view.read(cx).document().byte_of_line(Line::new(150)).get());
    set_caret(cx, &view, at);
    settle(cx, window);
    act(cx, window, |window, _| {
        assert!(window.try_find(cell_id(0, RowIx::new(149))).is_some(), "row 149 shows");
        assert!(window.try_find(cell_id(0, RowIx::new(0))).is_none(), "row 0 is scrolled away");
    });
}

/// 10 pt text at 1.5 lines (spec §6.4).
const DEFAULT_ROW: Pixels = px(15.);

#[gpui_kit::test]
fn rows_and_result_lines_are_one_height(cx: &mut TestAppContext) {
    let (_, view) = conflict(cx);
    let (_, _, line) = tops(cx, &view);
    assert_eq!(ROW_HEIGHT, DEFAULT_ROW);
    assert_eq!(line, DEFAULT_ROW);
}

#[gpui_kit::test]
fn scroll_sync_off_leaves_the_result_still(cx: &mut TestAppContext) {
    let settings =
        merge_app::WindowSettings { scroll_sync: false, ..merge_app::WindowSettings::default() };
    let (window, view, _) = common::open_with(
        cx,
        doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R"))),
        &settings,
    );
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW_HEIGHT * 5.5);
    let (rows, result, _) = tops(cx, &view);
    assert_eq!(rows, ROW_HEIGHT * 5.5);
    assert_eq!(result, px(0.));
    let line = tops(cx, &view).2;
    wheel(cx, window, RESULT_ID, line * 3.);
    assert_eq!(tops(cx, &view).0, ROW_HEIGHT * 5.5, "the rows stay too");
}

fn toggle_scroll_sync(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(merge_app::ToggleScrollSync), cx);
    });
    settle(cx, window);
}

#[gpui_kit::test]
fn scroll_sync_turned_off_leaves_each_pane_alone(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    toggle_scroll_sync(cx, window);
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW_HEIGHT * 5.5);
    let line = tops(cx, &view).2;
    assert_eq!(tops(cx, &view).1, px(0.), "the result stays");
    wheel(cx, window, RESULT_ID, line * 20.25);
    assert_eq!(tops(cx, &view).0, ROW_HEIGHT * 5.5, "the rows stay");
}

#[gpui_kit::test]
fn scroll_sync_turned_on_brings_the_rows_to_the_result(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    toggle_scroll_sync(cx, window);
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW_HEIGHT * 5.5);
    let line = tops(cx, &view).2;
    wheel(cx, window, RESULT_ID, line * 20.25);
    toggle_scroll_sync(cx, window);
    // As in `scrolling_the_result_past_the_conflict_moves_the_rows`: both anchors at the
    // middles of their views, a result line past the conflict one row less.
    let (rows_view, result_view) = views(cx, &view);
    let (rows, result, _) = tops(cx, &view);
    assert_eq!(result, line * 20.25, "the result leads and stays");
    assert_eq!(rows, ROW_HEIGHT * 19.25 + (result_view - rows_view) / 2.);
    // Synced again: the rows lead the result as before.
    wheel(cx, window, cell_id(0, RowIx::new(30)), ROW_HEIGHT);
    assert_ne!(tops(cx, &view).1, line * 20.25, "the result follows the rows");
}

#[gpui_kit::test]
fn scroll_sync_off_in_the_settings_turns_on_from_the_window(cx: &mut TestAppContext) {
    let settings =
        merge_app::WindowSettings { scroll_sync: false, ..merge_app::WindowSettings::default() };
    let (window, view, _) = common::open_with(
        cx,
        doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R"))),
        &settings,
    );
    let line = tops(cx, &view).2;
    wheel(cx, window, RESULT_ID, line * 20.25);
    assert_eq!(tops(cx, &view).0, px(0.));
    toggle_scroll_sync(cx, window);
    let (rows_view, result_view) = views(cx, &view);
    assert_eq!(tops(cx, &view).0, ROW_HEIGHT * 19.25 + (result_view - rows_view) / 2.);
}

#[gpui_kit::test]
fn a_larger_text_size_makes_taller_rows_and_lines(cx: &mut TestAppContext) {
    // 13 pt at 1.5 lines is 19.5, which GPUI rounds to 20.
    const LARGER: f32 = 13.;
    const ROW: Pixels = px(20.);
    let settings = merge_app::WindowSettings {
        typography: merge_app::Typography { size: px(LARGER), ..merge_app::Typography::default() },
        ..merge_app::WindowSettings::default()
    };
    let (window, view, _) = common::open_with(
        cx,
        doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R"))),
        &settings,
    );
    assert_eq!(tops(cx, &view).2, ROW);
    act(cx, window, |window, _| {
        assert_eq!(window.find(cell_id(0, RowIx::new(1))).bounds().size.height, ROW);
    });
    // As at the default size: rows top 5.5 rows puts the result's top at line 6.
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW * 5.5);
    assert_eq!(tops(cx, &view), (ROW * 5.5, ROW * 6., ROW));
}

#[gpui_kit::test]
fn a_line_height_of_2_makes_rows_and_lines_twice_the_text_size(cx: &mut TestAppContext) {
    // 10 pt at 2 lines is 20.
    const ROW: Pixels = px(20.);
    let settings = merge_app::WindowSettings {
        typography: merge_app::Typography { spacing: 2., ..merge_app::Typography::default() },
        ..merge_app::WindowSettings::default()
    };
    let (window, view, _) = common::open_with(
        cx,
        doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R"))),
        &settings,
    );
    assert_eq!(tops(cx, &view).2, ROW);
    act(cx, window, |window, _| {
        assert_eq!(window.find(cell_id(0, RowIx::new(1))).bounds().size.height, ROW);
    });
    wheel(cx, window, cell_id(0, RowIx::new(1)), ROW * 5.5);
    assert_eq!(tops(cx, &view), (ROW * 5.5, ROW * 6., ROW));
}

/// Lines one wheel packet scrolls in the sweep below.
const SWEEP_STEP_LINES: f32 = 3.;
/// More packets than it takes to sweep the result from top to end.
const SWEEP_PACKETS: usize = 80;
/// Packets a sweep sends after it reaches its end, each checking that the pane stays there.
const PACKETS_PAST_END: usize = 3;
/// The editor reserves half its viewport below the last row, and at least this many rows
/// (gpui-base `scroll_beyond_last_line`).
const BOTTOM_MARGIN_ROWS: f32 = 3.;

/// The furthest the result scrolls on its own: its rows, an empty one after the final
/// newline, and the bottom margin, less the viewport.
fn result_end(cx: &mut TestAppContext, view: &Entity<MergeWindow>, line: Pixels) -> Pixels {
    let (lines, viewport) = cx.read(|cx| {
        let view = view.read(cx);
        let result = view.result_editor().read(cx);
        // Placeholder rows add to the result's height.
        let placeholders: usize =
            result.display_map().spacers().iter().map(|&(_, rows)| rows).sum();
        (view.document().line_count() + placeholders, result.input_bounds().size.height)
    });
    let rows = f32::from(u16::try_from(lines + 1).expect("a small document"));
    let margin = (viewport / 2.).max(line * BOTTOM_MARGIN_ROWS);
    line * rows + margin - viewport
}

/// Wheel the pane under `id` down by `step` per packet, from the top, and check after each
/// packet that `top` gives the whole step until `end` stops it. The sweep ends
/// [`PACKETS_PAST_END`] packets after it reaches `end`.
fn sweep(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &'static str,
    (step, end): (Pixels, Pixels),
    top: impl Fn(&mut TestAppContext) -> Pixels,
) {
    let mut expected = px(0.);
    let mut past_end = 0;
    for packet in 0..SWEEP_PACKETS {
        // The scrolled pane moves in the packet's frame; the panes it syncs settle at the end.
        act(cx, window, |window, cx| {
            window.scroll(id, ScrollDelta::Pixels(point(px(0.), -step)), cx);
        });
        act(cx, window, TestWindowExt::render_frame);
        expected = (expected + step).min(end);
        assert_eq!(top(cx), expected, "{id} top after packet {packet}");
        if expected == end {
            past_end += 1;
            if past_end > PACKETS_PAST_END {
                break;
            }
        }
    }
    assert!(past_end > PACKETS_PAST_END, "{SWEEP_PACKETS} packets did not sweep {id} to its end");
    settle(cx, window);
}

#[gpui_kit::test]
fn wheel_over_the_result_scrolls_it_to_its_end(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    let line = tops(cx, &view).2;
    let end = result_end(cx, &view, line);
    let step = line * SWEEP_STEP_LINES;
    // Each packet moves the result by the whole step until the result's own end stops it.
    sweep(cx, window, RESULT_ID, (step, end), |cx| tops(cx, &view).1);
}

/// Left deletes the second half. Rows: 0-99 unchanged, 100-199 the deletion. Result
/// lines: 0-99, so the rows run on past the result's end.
const KEPT_LINES: usize = 100;

#[gpui_kit::test]
fn wheel_over_the_rows_scrolls_them_past_the_end_of_the_result(cx: &mut TestAppContext) {
    let base = numbered(None);
    let left: String = base.split_inclusive('\n').take(KEPT_LINES).collect();
    let (window, view) = open(cx, doc(&base, &left, &base));
    let mut viewport = px(0.);
    act(cx, window, |window, _| {
        viewport = window.find(INPUT_LINES_ID).bounds().size.height;
    });
    let rows = f32::from(u16::try_from(LINES).expect("a small document"));
    // The window opens on the deletion; scroll back to the top first.
    wheel(cx, window, INPUT_LINES_ID, -ROW_HEIGHT * rows);
    let end = ROW_HEIGHT * rows - viewport;
    let step = ROW_HEIGHT * SWEEP_STEP_LINES;
    sweep(cx, window, INPUT_LINES_ID, (step, end), |cx| tops(cx, &view).0);
    // The rows' end meets the result's: its last row, the deletion's placeholder, at the
    // bottom of its view.
    let (_, result, line) = tops(cx, &view);
    let result_view = views(cx, &view).1;
    let result_rows = f32::from(u16::try_from(KEPT_LINES + 1).expect("a small document"));
    assert_eq!(result, line * result_rows - result_view);
}

/// A trackpad packet whose f32 sums round: from 6.85 px, `6.85 + 1.37` falls short of the
/// next float by one bit.
const FRACTIONAL_STEP: Pixels = px(1.37);
/// Packets in one gesture, well inside the result: 274 px of its 2834.5.
const GESTURE_PACKETS: usize = 200;

/// Send one trackpad packet scrolling down by `by` to the middle of the result.
fn trackpad(cx: &mut TestAppContext, window: AnyWindowHandle, by: Pixels, phase: TouchPhase) {
    act(cx, window, |window, cx| {
        let position = window.find(RESULT_ID).bounds().center();
        window.dispatch_event(
            ScrollEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), -by)),
                touch_phase: phase,
                ..ScrollEvent::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    });
}

#[gpui_kit::test]
fn one_trackpad_gesture_scrolls_the_result_by_every_packet(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    trackpad(cx, window, px(0.), TouchPhase::Started);
    let mut expected = px(0.);
    for packet in 0..GESTURE_PACKETS {
        trackpad(cx, window, FRACTIONAL_STEP, TouchPhase::Moved);
        expected += FRACTIONAL_STEP;
        assert_eq!(tops(cx, &view).1, expected, "result top after packet {packet}");
    }
    trackpad(cx, window, px(0.), TouchPhase::Ended);
}

/// A trackpad packet at 60 Hz, rounded to whole milliseconds.
const PACKET_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
/// Packets in each gesture of the pair below.
const PAIR_PACKETS: usize = 10;
const PAIR_STEP: Pixels = px(5.);

#[gpui_kit::test]
fn the_gesture_after_a_lift_scrolls_the_result_from_its_first_packet(cx: &mut TestAppContext) {
    let (window, view) = conflict(cx);
    let mut expected = px(0.);
    for gesture in 0..2 {
        trackpad(cx, window, px(0.), TouchPhase::Started);
        for packet in 0..PAIR_PACKETS {
            cx.executor().advance_clock(PACKET_INTERVAL);
            trackpad(cx, window, PAIR_STEP, TouchPhase::Moved);
            expected += PAIR_STEP;
            assert_eq!(tops(cx, &view).1, expected, "gesture {gesture}, packet {packet}");
        }
        // The fingers lift while still moving.
        trackpad(cx, window, px(0.), TouchPhase::Ended);
        cx.executor().advance_clock(PACKET_INTERVAL);
        settle(cx, window);
    }
}

const REVEAL_LINES: usize = 700;
/// A conflict shorter than the view and taller than half of it, at rows 100-119.
const SHORT_CONFLICT: std::ops::Range<usize> = 100..120;
/// A conflict taller than the view, at rows 200-599.
const TALL_CONFLICT: std::ops::Range<usize> = 200..600;

fn two_conflicts(side: &str) -> String {
    (0..REVEAL_LINES)
        .map(|n| {
            if SHORT_CONFLICT.contains(&n) || TALL_CONFLICT.contains(&n) {
                format!("{side}{n}\n")
            } else {
                format!("l{n}\n")
            }
        })
        .collect()
}

fn rows_tall(range: &std::ops::Range<usize>) -> Pixels {
    #[expect(clippy::cast_precision_loss, reason = "a few hundred rows")]
    let rows = range.len() as f32;
    ROW_HEIGHT * rows
}

fn row_top(row: usize) -> Pixels {
    #[expect(clippy::cast_precision_loss, reason = "a few hundred rows")]
    let row = row as f32;
    ROW_HEIGHT * row
}

fn rows_view(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    cx.read(|cx| view.read(cx).rows_view_height())
}

fn result_view(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    cx.read(|cx| view.read(cx).result_editor().read(cx).input_bounds().size.height)
}

fn next_conflict(cx: &mut TestAppContext, window: AnyWindowHandle, step: Tool) {
    act(cx, window, |window, cx| window.click(step.id(), cx));
    settle(cx, window);
}

#[gpui_kit::test]
fn navigation_centres_a_conflict_that_fits_and_tops_one_that_does_not(cx: &mut TestAppContext) {
    let (window, view) =
        open(cx, doc(&two_conflicts("l"), &two_conflicts("L"), &two_conflicts("R")));
    settle(cx, window);
    let height = rows_view(cx, &view);
    let short = rows_tall(&SHORT_CONFLICT);
    assert!(short <= height && short * 2. > height, "the short conflict fits, just");
    assert!(rows_tall(&TALL_CONFLICT) > height, "the tall conflict does not fit");
    // The short conflict's 20 rows take 40 result lines (both sides), too many for the
    // result pane: its first line, line 100, goes to the top.
    let line = tops(cx, &view).2;
    assert!(line * 40. > result_view(cx, &view), "the result lines do not fit");
    // Opening goes to the first conflict: centred, with the room left split above and below.
    let centred = row_top(SHORT_CONFLICT.start) - (height - short) / 2.;
    assert_eq!(tops(cx, &view), (centred, line * 100., line));

    next_conflict(cx, window, Tool::NextConflict);
    // Row 200 is result line 100 + 40 + 80 = 220.
    assert_eq!(tops(cx, &view), (row_top(TALL_CONFLICT.start), line * 220., line));

    next_conflict(cx, window, Tool::PreviousConflict);
    assert_eq!(tops(cx, &view), (centred, line * 100., line));
}

/// A conflict whose rows and result lines both fit, at rows 100-104.
const SMALL_CONFLICT: std::ops::Range<usize> = 100..105;

fn small_conflict(side: &str) -> String {
    use std::fmt::Write as _;
    (0..REVEAL_LINES).fold(String::new(), |mut text, n| {
        let prefix = if SMALL_CONFLICT.contains(&n) { side } else { "l" };
        writeln!(text, "{prefix}{n}").expect("a String takes any write");
        text
    })
}

#[gpui_kit::test]
fn a_jump_centres_the_rows_and_the_result_lines_each_by_their_own_height(cx: &mut TestAppContext) {
    let (window, view) =
        open(cx, doc(&small_conflict("l"), &small_conflict("L"), &small_conflict("R")));
    settle(cx, window);
    let (rows_h, result_h) = (rows_view(cx, &view), result_view(cx, &view));
    let line = tops(cx, &view).2;
    // 5 rows; 10 result lines, both sides.
    let (rows, lines) = (rows_tall(&SMALL_CONFLICT), line * 10.);
    assert!(rows <= rows_h && lines <= result_h, "both fit");
    let rows_top = row_top(SMALL_CONFLICT.start) - (rows_h - rows) / 2.;
    let result_top = line * 100. - (result_h - lines) / 2.;
    assert_eq!(tops(cx, &view), (rows_top, result_top, line));
}
