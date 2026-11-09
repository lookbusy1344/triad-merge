//! Headless tests of soft wrap and sideways scrolling in the merge window (spec §5, §6.4).

use crate::common;

use common::{act, doc};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, ElementId, Entity, Pixels, ScrollDelta, TestAppContext, point, px,
};
use merge_app::toolbar::Tool;
use merge_app::window::{SOFT_WRAP_MAX_LINES, cell_id, cell_text_id, cell_text_part_id};
use merge_app::{MergeWindow, NextConflict, ToggleSoftWrap, Typography, WindowSettings};
use merge_core::document::MergeDocument;
use merge_view::rows::RowIx;

/// Frames drawn after a scroll, so a change made in one frame shows in the next.
const SETTLE_FRAMES: usize = 3;
/// Characters in the long line: wider than any pane.
const LONG: u16 = 400;
/// A sideways scroll well inside the long line.
const SIDEWAYS: Pixels = px(40.);
/// A sideways scroll past the end of any line.
const FAR: Pixels = px(100_000.);
/// The panes a window shows by default: left and right.
const SHOWN_PANES: usize = 2;
/// The row of the long line: row 0 is the conflict.
const LONG_ROW: usize = 1;

/// Lines 0 and 2 conflict; line 1 is [`LONG`] characters in every input.
fn wide() -> MergeDocument {
    let long = "x".repeat(usize::from(LONG));
    doc(&format!("b\n{long}\nb\n"), &format!("l\n{long}\nl\n"), &format!("r\n{long}\nr\n"))
}

/// [`wide`] with more lines than soft wrap takes.
fn too_long_to_wrap() -> MergeDocument {
    let long = "x".repeat(usize::from(LONG));
    let tail = "t\n".repeat(SOFT_WRAP_MAX_LINES);
    doc(&format!("b\n{long}\n{tail}"), &format!("l\n{long}\n{tail}"), &format!("r\n{long}\n{tail}"))
}

/// Open `doc` with `settings`, once the window has counted its inputs' widest lines.
fn open_with(
    cx: &mut TestAppContext,
    doc: MergeDocument,
    settings: &WindowSettings,
) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let (window, view, _) = common::open_with(cx, doc, settings);
    settle(cx, window);
    (window, view)
}

fn open(cx: &mut TestAppContext, doc: MergeDocument) -> (AnyWindowHandle, Entity<MergeWindow>) {
    open_with(cx, doc, &WindowSettings::default())
}

fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.run_until_parked();
    for _ in 0..SETTLE_FRAMES {
        act(cx, window, TestWindowExt::render_frame);
    }
}

/// How far the result is scrolled right.
fn result_left(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    cx.read(|cx| -view.read(cx).result_editor().read(cx).scroll_offset().x)
}

fn rows_left(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Pixels {
    cx.read(|cx| view.read(cx).rows_scroll_left())
}

/// Scroll the result `by` to the right, as its scrollbar does.
fn scroll_result(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<MergeWindow>,
    by: Pixels,
) {
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| {
            let offset = state.scroll_offset();
            state.set_scroll_offset(point(offset.x - by, offset.y), cx);
        });
    });
    settle(cx, window);
}

/// Scroll the pane under `id` `by` to the right with the wheel.
fn wheel_right(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
    by: Pixels,
) {
    act(cx, window, |window, cx| {
        window.scroll(id, ScrollDelta::Pixels(point(-by, px(0.))), cx);
    });
    settle(cx, window);
}

fn toggle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(ToggleSoftWrap), cx);
    });
    settle(cx, window);
}

fn wrapped(settings: WindowSettings) -> WindowSettings {
    WindowSettings { soft_wrap: true, ..settings }
}

/// The display rows of the result's long line.
fn long_line_rows(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> usize {
    cx.read(|cx| {
        let state = view.read(cx).result_editor().read(cx);
        let line = state
            .text()
            .to_string()
            .lines()
            .position(|line| line.len() == usize::from(LONG))
            .expect("the result holds the long line");
        state.display_map().buffer_line_to_display_row_range(line).expect("the line shows").len()
    })
}

#[gpui_kit::test]
fn the_toggle_wraps_the_long_line_as_the_setting_does(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, wide(), &wrapped(WindowSettings::default()));
    let wrapped_rows = long_line_rows(cx, &view);
    let (window_off, view_off) = open(cx, wide());
    assert_eq!(long_line_rows(cx, &view_off), 1, "unwrapped");
    toggle(cx, window_off);
    assert_eq!(long_line_rows(cx, &view_off), wrapped_rows, "toggled on");
    toggle(cx, window);
    assert_eq!(long_line_rows(cx, &view), 1, "toggled off");
}

#[gpui_kit::test]
fn the_result_scrolls_sideways_by_default(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(result_left(cx, &view), SIDEWAYS);
}

#[gpui_kit::test]
fn the_soft_wrap_setting_wraps_the_result(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, wide(), &wrapped(WindowSettings::default()));
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(result_left(cx, &view), px(0.));
}

#[gpui_kit::test]
fn the_action_turns_soft_wrap_on_and_off(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    scroll_result(cx, window, &view, SIDEWAYS);
    toggle(cx, window);
    assert_eq!(result_left(cx, &view), px(0.), "wrapped");
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(result_left(cx, &view), px(0.), "wrapped text does not scroll");
    toggle(cx, window);
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(result_left(cx, &view), SIDEWAYS, "unwrapped");
}

#[gpui_kit::test]
fn the_toolbar_button_toggles_soft_wrap(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    act(cx, window, |window, cx| {
        window.click(Tool::SoftWrap.id(), cx);
    });
    settle(cx, window);
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(result_left(cx, &view), px(0.));
}

#[gpui_kit::test]
fn inputs_past_the_limit_never_wrap(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, too_long_to_wrap(), &wrapped(WindowSettings::default()));
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(result_left(cx, &view), SIDEWAYS, "the setting");
    toggle(cx, window);
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(result_left(cx, &view), SIDEWAYS * 2., "the toggle");
}

#[gpui_kit::test]
fn scrolling_the_result_sideways_moves_the_rows(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    scroll_result(cx, window, &view, SIDEWAYS);
    assert_eq!(rows_left(cx, &view), SIDEWAYS);
}

#[gpui_kit::test]
fn scrolling_the_rows_sideways_moves_the_result(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    wheel_right(cx, window, cell_id(0, RowIx::new(LONG_ROW)), SIDEWAYS);
    assert_eq!(rows_left(cx, &view), SIDEWAYS);
    assert_eq!(result_left(cx, &view), SIDEWAYS);
    wheel_right(cx, window, cell_id(1, RowIx::new(LONG_ROW)), -SIDEWAYS / 2.);
    assert_eq!(rows_left(cx, &view), SIDEWAYS / 2.);
    assert_eq!(result_left(cx, &view), SIDEWAYS / 2.);
}

#[gpui_kit::test]
fn the_rows_stop_where_their_widest_line_ends(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    wheel_right(cx, window, cell_id(0, RowIx::new(LONG_ROW)), FAR);
    let mut expected = px(0.);
    act(cx, window, |window, cx| {
        let view_width = window.find(cell_text_id(0, RowIx::new(LONG_ROW))).bounds().size.width;
        let text = window.text_system();
        let font = text.resolve_font(&gpui_kit::font(cx.theme().mono_font_family.clone()));
        let advance =
            text.advance(font, Typography::default().size, 'x').expect("the font has x").width;
        expected = advance * f32::from(LONG) - view_width;
    });
    assert_eq!(rows_left(cx, &view), expected);
    // The rows hold their end: the result's own end does not pull them back.
    settle(cx, window);
    assert_eq!(rows_left(cx, &view), expected);
}

#[gpui_kit::test]
fn rows_whose_lines_fit_do_not_scroll_sideways(cx: &mut TestAppContext) {
    let (window, view) = open(cx, doc("b\nend\n", "l\nend\n", "r\nend\n"));
    wheel_right(cx, window, cell_id(0, RowIx::new(LONG_ROW)), SIDEWAYS);
    assert_eq!(rows_left(cx, &view), px(0.));
}

#[gpui_kit::test]
fn a_jump_takes_both_back_to_the_left_edge(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    wheel_right(cx, window, cell_id(0, RowIx::new(LONG_ROW)), SIDEWAYS);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(NextConflict), cx);
    });
    settle(cx, window);
    assert_eq!((rows_left(cx, &view), result_left(cx, &view)), (px(0.), px(0.)));
}

#[gpui_kit::test]
fn without_scroll_sync_each_pane_scrolls_sideways_alone(cx: &mut TestAppContext) {
    let settings = WindowSettings { scroll_sync: false, ..WindowSettings::default() };
    let (window, view) = open_with(cx, wide(), &settings);
    wheel_right(cx, window, cell_id(0, RowIx::new(LONG_ROW)), SIDEWAYS);
    assert_eq!((rows_left(cx, &view), result_left(cx, &view)), (SIDEWAYS, px(0.)));
    scroll_result(cx, window, &view, SIDEWAYS * 2.);
    assert_eq!((rows_left(cx, &view), result_left(cx, &view)), (SIDEWAYS, SIDEWAYS * 2.));
}

#[gpui_kit::test]
fn wrapped_rows_do_not_scroll_sideways(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, wide(), &wrapped(WindowSettings::default()));
    wheel_right(cx, window, cell_id(0, RowIx::new(LONG_ROW)), SIDEWAYS);
    assert_eq!((rows_left(cx, &view), result_left(cx, &view)), (px(0.), px(0.)));
}

#[gpui_kit::test]
fn unwrapping_lets_the_rows_scroll_sideways_again(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, wide(), &wrapped(WindowSettings::default()));
    toggle(cx, window);
    wheel_right(cx, window, cell_id(0, RowIx::new(LONG_ROW)), SIDEWAYS);
    assert_eq!(rows_left(cx, &view), SIDEWAYS);
}

/// The display rows the long line wraps to in a top pane: as many columns as fit the
/// pane's text each, the last row holding the rest.
fn long_row_rows(cx: &mut TestAppContext, window: AnyWindowHandle) -> usize {
    let mut rows = 0;
    act(cx, window, |window, cx| {
        let width = window.find(cell_text_id(0, RowIx::new(LONG_ROW))).bounds().size.width;
        let text = window.text_system();
        let font = text.resolve_font(&gpui_kit::font(cx.theme().mono_font_family.clone()));
        let advance =
            text.advance(font, Typography::default().size, 'x').expect("the font has x").width;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a pane holds a few hundred columns"
        )]
        let columns = (width / advance).floor() as usize;
        rows = usize::from(LONG).div_ceil(columns);
    });
    rows
}

fn row_rows(cx: &mut TestAppContext, view: &Entity<MergeWindow>, row: usize) -> usize {
    cx.read(|cx| view.read(cx).row_heights().of(row).len())
}

#[gpui_kit::test]
fn soft_wrap_wraps_the_long_row_in_every_top_pane(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, wide(), &wrapped(WindowSettings::default()));
    let rows = long_row_rows(cx, window);
    assert!(rows > 1, "the line is wider than a pane");
    assert_eq!(row_rows(cx, &view, LONG_ROW), rows);
    assert_eq!((row_rows(cx, &view, 0), row_rows(cx, &view, 2)), (1, 1), "short rows");
    // Each pane shows the line's last part on the row's last display row, level with the
    // others.
    act(cx, window, |window, _| {
        let tops: Vec<_> = (0..SHOWN_PANES)
            .map(|pane| {
                window
                    .find(cell_text_part_id(pane, RowIx::new(LONG_ROW), rows - 1))
                    .bounds()
                    .origin
                    .y
            })
            .collect();
        assert_eq!(tops, vec![tops[0]; SHOWN_PANES]);
    });
}

#[gpui_kit::test]
fn the_toggle_wraps_and_unwraps_the_top_panes(cx: &mut TestAppContext) {
    let (window, view) = open(cx, wide());
    assert_eq!(row_rows(cx, &view, LONG_ROW), 1, "unwrapped");
    toggle(cx, window);
    let rows = long_row_rows(cx, window);
    assert_eq!(row_rows(cx, &view, LONG_ROW), rows, "toggled on");
    toggle(cx, window);
    assert_eq!(row_rows(cx, &view, LONG_ROW), 1, "toggled off");
}

#[gpui_kit::test]
fn inputs_past_the_limit_never_wrap_the_top_panes(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, too_long_to_wrap(), &wrapped(WindowSettings::default()));
    assert_eq!(row_rows(cx, &view, LONG_ROW), 1, "the setting");
    toggle(cx, window);
    assert_eq!(row_rows(cx, &view, LONG_ROW), 1, "the toggle");
}

/// Lines in every input, the same in all.
const SAME_LINES: usize = 400;
/// The first lines are [`LONG`] characters, so they wrap in every pane.
const LONG_LINES: usize = 40;
/// An aligned row far below the long lines, away from the anchors' ramps at the ends.
const MIDDLE_ROW: usize = 200;

/// [`SAME_LINES`] lines, alike in every input: row `n` is result line `n`.
fn same_long() -> MergeDocument {
    let text: String = (0..SAME_LINES)
        .map(|n| {
            if n < LONG_LINES {
                format!("{}\n", "x".repeat(usize::from(LONG)))
            } else {
                format!("l{n}\n")
            }
        })
        .collect();
    doc(&text, &text, &text)
}

/// The aligned row at the middle of the rows' view, and the result line at the middle of
/// the result's view.
fn middles(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> (usize, usize) {
    cx.read(|cx| {
        let view = view.read(cx);
        let row_h = Typography::default().line_height();
        let middle = (view.rows_scroll_top() + view.rows_view_height() / 2.) / row_h;
        let state = view.result_editor().read(cx);
        let line_h = state.line_height().expect("the result is laid out");
        let result_middle =
            (-state.scroll_offset().y + state.input_bounds().size.height / 2.) / line_h;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "non-negative row positions"
        )]
        let (row, line) = (middle.floor() as usize, result_middle.floor() as usize);
        (
            view.row_heights().at(row).expect("a row shows").0,
            state.display_map().display_row_to_buffer_line(line),
        )
    })
}

#[gpui_kit::test]
fn scrolling_wrapped_rows_brings_the_same_line_to_the_middle_of_the_result(
    cx: &mut TestAppContext,
) {
    let (window, view) = open_with(cx, same_long(), &wrapped(WindowSettings::default()));
    let start = cx.read(|cx| view.read(cx).row_heights().of(MIDDLE_ROW).start);
    let top = tall(start, Typography::default().line_height());
    wheel_down(cx, window, cell_id(0, RowIx::new(0)), top);
    let (row, line) = middles(cx, &view);
    assert!(row > MIDDLE_ROW, "the middle is below the top");
    assert_eq!(line, row);
}

#[gpui_kit::test]
fn scrolling_the_wrapped_result_brings_the_same_row_to_the_middle_of_the_rows(
    cx: &mut TestAppContext,
) {
    let (window, view) = open_with(cx, same_long(), &wrapped(WindowSettings::default()));
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| {
            let line_h = state.line_height().expect("the result is laid out");
            let rows = state
                .display_map()
                .buffer_line_to_display_row_range(MIDDLE_ROW)
                .expect("the line shows");
            state.set_scroll_offset(point(px(0.), -tall(rows.start, line_h)), cx);
        });
    });
    settle(cx, window);
    let (row, line) = middles(cx, &view);
    assert!(line > MIDDLE_ROW, "the middle is below the top");
    assert_eq!(row, line);
}

/// Scroll the pane under `id` down by `by` with the wheel.
fn wheel_down(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
    by: Pixels,
) {
    act(cx, window, |window, cx| {
        window.scroll(id, ScrollDelta::Pixels(point(px(0.), -by)), cx);
    });
    settle(cx, window);
}

/// The height of `rows` rows of `row_h` each.
fn tall(rows: usize, row_h: Pixels) -> Pixels {
    row_h * f32::from(u16::try_from(rows).expect("a test scrolls a few hundred rows"))
}

/// The aligned row at the top of the rows' view.
fn top_row(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> usize {
    cx.read(|cx| {
        let view = view.read(cx);
        let top = view.rows_scroll_top() / Typography::default().line_height();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a non-negative row position"
        )]
        let row = top.round() as usize;
        view.row_heights().at(row).expect("a row shows").0
    })
}

#[gpui_kit::test]
fn the_rows_keep_their_top_row_as_they_wrap_and_unwrap(cx: &mut TestAppContext) {
    let (window, view) = open(cx, same_long());
    wheel_down(
        cx,
        window,
        cell_id(0, RowIx::new(0)),
        tall(MIDDLE_ROW, Typography::default().line_height()),
    );
    assert_eq!(top_row(cx, &view), MIDDLE_ROW, "unwrapped");
    toggle(cx, window);
    assert_eq!(top_row(cx, &view), MIDDLE_ROW, "wrapped");
    toggle(cx, window);
    assert_eq!(top_row(cx, &view), MIDDLE_ROW, "unwrapped again");
}

/// The left side deletes base line 1, which the right side changes: row 1 is a conflict
/// whose left cell is a filler carrying the deletion note. Row 2 is the long line, and
/// [`SAME_LINES`] short lines follow, so the rows scroll.
fn deleted_then_long() -> MergeDocument {
    let long = "x".repeat(usize::from(LONG));
    let tail = "t\n".repeat(SAME_LINES);
    doc(
        &format!("a\nb\n{long}\n{tail}"),
        &format!("a\n{long}\n{tail}"),
        &format!("a\nB\n{long}\n{tail}"),
    )
}

/// The row of [`deleted_then_long`] whose left cell carries the deletion note.
const NOTE_ROW: usize = 1;

#[gpui_kit::test]
fn the_deletion_note_leaves_the_text_its_full_width(cx: &mut TestAppContext) {
    let (window, _) = open_with(cx, deleted_then_long(), &wrapped(WindowSettings::default()));
    act(cx, window, |window, _| {
        let width = |row| window.find(cell_text_id(0, RowIx::new(row))).bounds().size.width;
        assert_eq!(width(NOTE_ROW), width(0));
    });
}

#[gpui_kit::test]
fn rows_keep_their_wrap_when_the_deletion_note_scrolls_to_the_top(cx: &mut TestAppContext) {
    let (window, view) = open_with(cx, deleted_then_long(), &wrapped(WindowSettings::default()));
    let long_row = NOTE_ROW + 1;
    let before = row_rows(cx, &view, long_row);
    assert!(before > 1, "the long line wraps");
    wheel_down(
        cx,
        window,
        cell_id(0, RowIx::new(0)),
        tall(NOTE_ROW, Typography::default().line_height()),
    );
    assert_eq!(top_row(cx, &view), NOTE_ROW, "the note's row is at the top");
    assert_eq!(row_rows(cx, &view, long_row), before);
}
