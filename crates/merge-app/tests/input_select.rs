//! Headless tests of selecting and copying text in the top panes (spec §5, Selection in the
//! top panes): drags, presses and keys through the window, and the clipboard they fill.

use crate::common;

use std::fmt::Write as _;

use common::{act, doc, open};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::{ClickOptions, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, ClipboardItem, Entity, HighlightStyle, InputEvent as _, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollDelta,
    TestAppContext, point, px,
};
use merge_app::MergeWindow;
use merge_app::window::{INPUT_LINES_ID, RESULT_ID, cell_text_id};
use merge_config::settings::DEFAULT_FONT_SIZE;
use merge_core::document::MergeDocument;
use merge_core::merge3::Source;
use merge_view::rows::RowIx;

const LEFT_PANE: usize = 0;
const RIGHT_PANE: usize = 1;

/// The headless text system advances every character 0.6 em.
const CHAR_EMS: f32 = 0.6;

/// What the clipboard holds before a test copies, so a copy of nothing shows.
const BEFORE: &str = "before";

/// Rows: 0 `a`, 1 the conflict `hello world|B`, 2 `c`.
fn hello() -> MergeDocument {
    doc("a\nb\nc\n", "a\nhello world\nc\n", "a\nB\nc\n")
}

/// Rows: 0 `a|a`, 1 `X|-`, 2 `Y|-`, 3 `b|b`: the right pane has filler rows 1 and 2.
fn inserted() -> MergeDocument {
    doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n")
}

fn opened(cx: &mut TestAppContext, doc: MergeDocument) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let opened = open(cx, doc);
    // Focus events reach only an active window.
    act(cx, opened.0, |window, _| window.activate_window());
    cx.run_until_parked();
    cx.write_to_clipboard(ClipboardItem::new_string(BEFORE.to_owned()));
    opened
}

/// The window point at the boundary before character `column` of `pane`'s text on `row`,
/// half a row down.
fn at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    pane: usize,
    row: usize,
    column: usize,
) -> Point<Pixels> {
    #[expect(clippy::cast_precision_loss, reason = "test columns are small")]
    at_column(cx, window, pane, row, column as f32)
}

/// [`at`] for a point `column` characters in, which may fall inside a character.
fn at_column(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    pane: usize,
    row: usize,
    column: f32,
) -> Point<Pixels> {
    let mut found = None;
    act(cx, window, |window, _| {
        let bounds = window.find(cell_text_id(pane, RowIx::new(row))).bounds();
        found = Some(point(bounds.origin.x, bounds.center().y));
    });
    let start = found.expect("the cell shows");
    let x = px(DEFAULT_FONT_SIZE * CHAR_EMS * column);
    point(start.x + x, start.y)
}

fn drag(cx: &mut TestAppContext, window: AnyWindowHandle, from: Point<Pixels>, to: Point<Pixels>) {
    act(cx, window, |window, cx| window.drag(from, to, cx));
    cx.run_until_parked();
}

/// `count` presses at `column` of `pane`'s text on `row`, with `modifiers`.
fn press_at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    pane: usize,
    row: usize,
    column: f32,
    count: usize,
    modifiers: Modifiers,
) {
    let target = at_column(cx, window, pane, row, column);
    let mut origin = None;
    act(cx, window, |window, _| {
        origin = Some(window.find(cell_text_id(pane, RowIx::new(row))).bounds().origin);
    });
    let origin = origin.expect("the cell shows");
    let options = ClickOptions::new()
        .with_offset(point(target.x - origin.x, target.y - origin.y))
        .with_count(count)
        .with_modifiers(modifiers);
    act(cx, window, |window, cx| {
        window.click_with_options(cell_text_id(pane, RowIx::new(row)), options, cx);
    });
    cx.run_until_parked();
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
    cx.run_until_parked();
}

/// Copy with ⌘C and give the clipboard's text.
fn copy(cx: &mut TestAppContext, window: AnyWindowHandle) -> String {
    press(cx, window, "secondary-c");
    cx.read_from_clipboard().and_then(|item| item.text()).expect("the clipboard holds text")
}

/// One range; a one-range array literal reads as a mistake to clippy.
fn one(range: std::ops::Range<usize>) -> [std::ops::Range<usize>; 1] {
    [range]
}

fn selection(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
) -> Option<(Source, std::ops::Range<usize>)> {
    cx.read(|cx| view.read(cx).input_selection().map(|s| (s.source(), s.range())))
}

#[gpui_kit::test]
fn a_drag_over_characters_copies_them(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    let (from, to) = (at(cx, window, LEFT_PANE, 1, 2), at(cx, window, LEFT_PANE, 1, 8));
    drag(cx, window, from, to);
    assert_eq!(copy(cx, window), "llo wo");
}

#[gpui_kit::test]
fn a_drag_to_the_left_copies_the_same_characters(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    let (from, to) = (at(cx, window, LEFT_PANE, 1, 8), at(cx, window, LEFT_PANE, 1, 2));
    drag(cx, window, from, to);
    assert_eq!(copy(cx, window), "llo wo");
}

#[gpui_kit::test]
fn a_drag_in_the_right_pane_copies_the_right_input(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, hello());
    let (from, to) = (at(cx, window, RIGHT_PANE, 0, 0), at(cx, window, RIGHT_PANE, 1, 1));
    drag(cx, window, from, to);
    assert_eq!(selection(cx, &view), Some((Source::Side(1), 0..3)));
    assert_eq!(copy(cx, window), "a\nB");
}

#[gpui_kit::test]
fn a_drag_over_rows_copies_the_lines_between_with_their_terminators(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    let (from, to) = (at(cx, window, LEFT_PANE, 0, 0), at(cx, window, LEFT_PANE, 2, 1));
    drag(cx, window, from, to);
    assert_eq!(copy(cx, window), "a\nhello world\nc");
}

#[gpui_kit::test]
fn a_drag_past_the_text_ends_at_the_lines_end(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    let (from, to) = (at(cx, window, LEFT_PANE, 1, 6), at(cx, window, LEFT_PANE, 1, 40));
    drag(cx, window, from, to);
    assert_eq!(copy(cx, window), "world");
}

#[gpui_kit::test]
fn filler_rows_add_nothing_to_a_copy(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, inserted());
    let (from, to) = (at(cx, window, RIGHT_PANE, 0, 0), at(cx, window, RIGHT_PANE, 3, 1));
    drag(cx, window, from, to);
    assert_eq!(copy(cx, window), "a\nb");
}

#[gpui_kit::test]
fn a_drag_from_a_filler_row_starts_at_the_next_line(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, inserted());
    let (from, to) = (at(cx, window, RIGHT_PANE, 1, 3), at(cx, window, RIGHT_PANE, 3, 1));
    drag(cx, window, from, to);
    assert_eq!(selection(cx, &view), Some((Source::Side(1), 2..3)));
    assert_eq!(copy(cx, window), "b");
}

#[gpui_kit::test]
fn a_copy_keeps_tabs_and_crlf_terminators(cx: &mut TestAppContext) {
    // Left row 1 is `\tx\r\n`, drawn as four spaces then x. Column 3 lies nearer the tab's end.
    let (window, _) = opened(cx, doc("a\r\nb\r\n", "a\r\n\tx\r\n", "a\r\nB\r\n"));
    let (from, to) = (at(cx, window, LEFT_PANE, 0, 1), at(cx, window, LEFT_PANE, 1, 3));
    drag(cx, window, from, to);
    assert_eq!(copy(cx, window), "\r\n\t");
}

#[gpui_kit::test]
fn a_point_nearer_a_tabs_start_takes_the_tab_in(cx: &mut TestAppContext) {
    // Column 1 lies nearer the start of the tab's four spaces; column 5 is the end of `x`.
    let (window, _) = opened(cx, doc("a\nb\n", "a\n\tx\n", "a\nB\n"));
    let (from, to) = (at(cx, window, LEFT_PANE, 1, 5), at(cx, window, LEFT_PANE, 1, 1));
    drag(cx, window, from, to);
    assert_eq!(copy(cx, window), "\tx");
}

#[gpui_kit::test]
fn shift_press_moves_the_end(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    press_at(cx, window, LEFT_PANE, 1, 2.0, 1, Modifiers::none());
    press_at(cx, window, LEFT_PANE, 2, 1.0, 1, Modifiers::shift());
    assert_eq!(copy(cx, window), "llo world\nc");
    press_at(cx, window, LEFT_PANE, 0, 0.0, 1, Modifiers::shift());
    assert_eq!(copy(cx, window), "a\nhe");
}

#[gpui_kit::test]
fn shift_press_in_another_pane_starts_a_new_selection(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, hello());
    press_at(cx, window, LEFT_PANE, 1, 2.0, 1, Modifiers::none());
    press_at(cx, window, RIGHT_PANE, 1, 1.0, 1, Modifiers::shift());
    assert_eq!(selection(cx, &view), Some((Source::Side(1), 3..3)));
}

#[gpui_kit::test]
fn a_double_press_selects_the_word(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    // At the `o` of `world`.
    press_at(cx, window, LEFT_PANE, 1, 7.0, 2, Modifiers::none());
    assert_eq!(copy(cx, window), "world");
}

#[gpui_kit::test]
fn a_double_press_takes_the_character_under_the_pointer(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    // Over the right part of the `o` that ends `hello`: the nearest boundary is the space's.
    press_at(cx, window, LEFT_PANE, 1, 4.75, 2, Modifiers::none());
    assert_eq!(copy(cx, window), "hello");
}

#[gpui_kit::test]
fn a_triple_press_selects_the_line_and_its_terminator(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    press_at(cx, window, LEFT_PANE, 1, 3.0, 3, Modifiers::none());
    assert_eq!(copy(cx, window), "hello world\n");
}

#[gpui_kit::test]
fn select_all_takes_the_whole_input_of_the_selections_pane(cx: &mut TestAppContext) {
    let (window, _) = opened(cx, hello());
    press_at(cx, window, RIGHT_PANE, 1, 1.0, 1, Modifiers::none());
    press(cx, window, "secondary-a");
    assert_eq!(copy(cx, window), "a\nB\nc\n");
}

#[gpui_kit::test]
fn a_press_without_a_drag_copies_nothing(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, hello());
    press_at(cx, window, LEFT_PANE, 1, 3.0, 1, Modifiers::none());
    assert_eq!(selection(cx, &view), Some((Source::Side(0), 5..5)));
    assert_eq!(copy(cx, window), BEFORE);
}

#[gpui_kit::test]
fn a_press_still_makes_its_region_current(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, doc("a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nb\nc\nD\n"));
    press_at(cx, window, LEFT_PANE, 1, 0.0, 1, Modifiers::none());
    let caret = |cx: &mut TestAppContext| common::caret(cx, &view);
    // Region 1 starts at byte 2 of the result.
    assert_eq!(caret(cx), 2);
    press_at(cx, window, RIGHT_PANE, 3, 0.0, 1, Modifiers::none());
    assert_eq!(caret(cx), 6);
}

#[gpui_kit::test]
fn the_selection_ends_when_the_result_takes_the_focus(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, hello());
    let (from, to) = (at(cx, window, LEFT_PANE, 1, 0), at(cx, window, LEFT_PANE, 1, 5));
    drag(cx, window, from, to);
    assert_eq!(selection(cx, &view), Some((Source::Side(0), 2..7)));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    cx.run_until_parked();
    assert_eq!(selection(cx, &view), None);
}

#[gpui_kit::test]
fn hiding_the_base_pane_ends_a_selection_in_it(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, hello());
    press(cx, window, "secondary-b");
    // Panes: left, base, right.
    let (from, to) = (at(cx, window, 1, 1, 0), at(cx, window, 1, 1, 1));
    drag(cx, window, from, to);
    assert_eq!(selection(cx, &view), Some((Source::Base(0), 2..3)));
    press(cx, window, "secondary-b");
    assert_eq!(selection(cx, &view), None);
}

#[gpui_kit::test]
fn the_selected_characters_fill_in_the_selection_colour(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, hello());
    let (from, to) = (at(cx, window, LEFT_PANE, 0, 0), at(cx, window, LEFT_PANE, 1, 3));
    drag(cx, window, from, to);
    let selected = cx.read(|cx| cx.theme().selection);
    // The selected bytes of `row`, adjacent runs joined: a change mark may split them.
    let fills = |cx: &mut TestAppContext, row| -> Vec<std::ops::Range<usize>> {
        view.update(cx, |view, cx| {
            view.cell_highlights(LEFT_PANE, RowIx::new(row), cx)
                .expect("the cell has a line")
                .1
                .into_iter()
                .filter(|(_, style): &(_, HighlightStyle)| style.background_color == Some(selected))
                .fold(Vec::new(), |mut joined: Vec<std::ops::Range<usize>>, (range, _)| {
                    match joined.last_mut() {
                        Some(last) if last.end == range.start => last.end = range.end,
                        _ => joined.push(range),
                    }
                    joined
                })
        })
    };
    assert_eq!(fills(cx, 0), one(0..1));
    assert_eq!(fills(cx, 1), one(0..3));
    assert_eq!(fills(cx, 2), Vec::<std::ops::Range<usize>>::new());
}

const NUMBERED_LINES: usize = 200;

/// The time between autoscroll steps (spec §5).
const TICK: std::time::Duration = std::time::Duration::from_millis(16);

/// Enough steps of the most scroll, two rows, to cross 200 rows.
const MANY_TICKS: usize = 120;

/// `l1` to `l200`, one a line, the same in every input: more rows than the panes show.
fn numbered_text() -> String {
    (1..=NUMBERED_LINES).fold(String::new(), |mut text, n| {
        writeln!(text, "l{n}").expect("a String takes any write");
        text
    })
}

fn numbered() -> MergeDocument {
    let text = numbered_text();
    doc(&text, &text, &text)
}

/// The top panes' bounds in the window.
fn panes(cx: &mut TestAppContext, window: AnyWindowHandle) -> gpui_kit::Bounds<Pixels> {
    let mut bounds = None;
    act(cx, window, |window, _| bounds = Some(window.find(INPUT_LINES_ID).bounds()));
    bounds.expect("the panes show")
}

fn mouse_down(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    act(cx, window, |window, cx| {
        let down = MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        };
        window.dispatch_event(down.to_platform_input(), cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
}

/// Move the pointer to `at` with the left button held.
fn mouse_drag(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    act(cx, window, |window, cx| {
        let moved = MouseMoveEvent {
            position: at,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        };
        window.dispatch_event(moved.to_platform_input(), cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
}

fn mouse_up(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<Pixels>) {
    act(cx, window, |window, cx| {
        let up = MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
        };
        window.dispatch_event(up.to_platform_input(), cx);
        window.render_frame(cx);
    });
    cx.run_until_parked();
}

/// Let `ticks` autoscroll steps pass, then draw a frame.
fn tick(cx: &mut TestAppContext, window: AnyWindowHandle, ticks: usize) {
    for _ in 0..ticks {
        cx.executor().advance_clock(TICK);
        cx.run_until_parked();
    }
    act(cx, window, TestWindowExt::render_frame);
}

fn scroll_top(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    cx.read(|cx| view.read(cx).rows_scroll_top())
}

/// How far the rows scroll at most: 200 rows, each as high as a drawn cell, less the view.
fn scroll_end(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<MergeWindow>,
) -> Pixels {
    let mut row = None;
    act(cx, window, |window, _| {
        row = Some(window.find(cell_text_id(LEFT_PANE, RowIx::new(0))).bounds().size.height);
    });
    #[expect(clippy::cast_precision_loss, reason = "a small row count")]
    let rows = row.expect("row 0 shows") * NUMBERED_LINES as f32;
    rows - cx.read(|cx| view.read(cx).rows_view_height())
}

#[gpui_kit::test]
fn a_drag_held_below_the_panes_scrolls_them_and_selects_to_the_end(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, numbered());
    let end = scroll_end(cx, window, &view);
    let below = panes(cx, window).bottom() + px(20.);
    let start = at(cx, window, LEFT_PANE, 0, 0);
    mouse_down(cx, window, start);
    mouse_drag(cx, window, point(start.x, below));
    tick(cx, window, 1);
    // Half of 20 px past the edge.
    assert_eq!(scroll_top(cx, &view), px(10.));
    mouse_drag(cx, window, point(start.x, below + px(200.)));
    tick(cx, window, MANY_TICKS);
    assert_eq!(scroll_top(cx, &view), end);
    // The bottom row is `l200`; its line's end leaves out the last terminator.
    let text = numbered_text();
    assert_eq!(copy(cx, window), text[..text.len() - 1]);
}

#[gpui_kit::test]
fn a_drag_held_above_the_panes_scrolls_them_up_and_selects_to_the_start(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, numbered());
    let bounds = panes(cx, window);
    let end = scroll_end(cx, window, &view);
    act(cx, window, |window, cx| {
        window.scroll(INPUT_LINES_ID, ScrollDelta::Pixels(point(px(0.), -px(10_000.))), cx);
    });
    cx.run_until_parked();
    assert_eq!(scroll_top(cx, &view), end);
    // After `l2` on the last row, `l200`.
    let start = at(cx, window, LEFT_PANE, NUMBERED_LINES - 1, 2);
    mouse_down(cx, window, start);
    mouse_drag(cx, window, point(start.x, bounds.top() - px(200.)));
    tick(cx, window, MANY_TICKS);
    assert_eq!(scroll_top(cx, &view), px(0.));
    let text = numbered_text();
    assert_eq!(copy(cx, window), text[..text.len() - "00\n".len()]);
}

#[gpui_kit::test]
fn moving_back_over_the_panes_stops_the_scrolling(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, numbered());
    let below = panes(cx, window).bottom() + px(40.);
    let start = at(cx, window, LEFT_PANE, 0, 0);
    mouse_down(cx, window, start);
    mouse_drag(cx, window, point(start.x, below));
    tick(cx, window, 3);
    assert_eq!(scroll_top(cx, &view), px(60.));
    let inside = panes(cx, window).center().y;
    mouse_drag(cx, window, point(start.x, inside));
    tick(cx, window, 10);
    assert_eq!(scroll_top(cx, &view), px(60.));
}

#[gpui_kit::test]
fn releasing_past_the_edge_stops_the_scrolling_and_the_drag(cx: &mut TestAppContext) {
    let (window, view) = opened(cx, numbered());
    let below = panes(cx, window).bottom() + px(40.);
    let start = at(cx, window, LEFT_PANE, 0, 0);
    mouse_down(cx, window, start);
    mouse_drag(cx, window, point(start.x, below));
    tick(cx, window, 2);
    mouse_up(cx, window, point(start.x, below));
    let held = selection(cx, &view);
    tick(cx, window, 10);
    assert_eq!(scroll_top(cx, &view), px(40.));
    // A later move with the button down, as from another drag, leaves the selection.
    mouse_drag(cx, window, point(start.x, below));
    tick(cx, window, 3);
    assert_eq!(scroll_top(cx, &view), px(40.));
    assert_eq!(selection(cx, &view), held);
}
