//! Headless tests of collapsed unchanged regions in the merge window (spec §5; DESIGN.md
//! §A6.2).

use std::fmt::Write as _;
use std::ops::Range;

use crate::common;
use common::{act, doc, open_with, set_caret};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, Pixels, ScrollDelta, TestAppContext, point, px};
use merge_app::diff_window::Collapsing;
use merge_app::window::{RESULT_ID, ROW_HEIGHT, fold_row_id, input_match_next_id};
use merge_app::{MergeWindow, WindowSettings};
use merge_core::document::MergeDocument;
use merge_view::rows::RowIx;

const LINES: usize = 400;
const CONFLICTS: [usize; 2] = [100, 300];
const LEFT_PANE: usize = 0;
const NO_LINES: [Range<usize>; 0] = [];
const NO_NOTES: [(usize, String); 0] = [];
/// Frames drawn after a scroll, so a change made in one frame shows in the next.
const SETTLE_FRAMES: usize = 3;
/// The rounding of f32 scroll tops through the display rows' maps.
const PIXEL_TOLERANCE: Pixels = px(0.01);

/// 400 lines `l0` to `l399`; lines 100 and 300 conflict. Regions: 0 unchanged (rows and
/// result lines 0-99), 1 a conflict (row 100, result lines 100-101: left then right),
/// 2 unchanged (rows 101-299, result lines 102-300), 3 a conflict (row 300, result lines
/// 301-302), 4 unchanged (rows 301-399, result lines 303-401).
fn two_conflicts() -> MergeDocument {
    let side = |name: &str| {
        (0..LINES).fold(String::new(), |mut text, n| {
            if CONFLICTS.contains(&n) {
                writeln!(text, "{name}{n}").expect("a String takes any write");
            } else {
                writeln!(text, "l{n}").expect("a String takes any write");
            }
            text
        })
    };
    doc(&side("l"), &side("left"), &side("right"))
}

fn settings(on: bool, context_lines: usize) -> WindowSettings {
    WindowSettings { collapsing: Collapsing { on, context_lines }, ..WindowSettings::default() }
}

fn open_set(
    cx: &mut TestAppContext,
    on: bool,
    context: usize,
) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let (window, view, _) = open_with(cx, two_conflicts(), &settings(on, context));
    (window, view)
}

fn collapsed(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    open_set(cx, true, 3)
}

fn settle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..SETTLE_FRAMES {
        act(cx, window, TestWindowExt::render_frame);
    }
}

/// The lines the result hides.
fn hidden(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Vec<Range<usize>> {
    cx.read(|cx| view.read(cx).result_editor().read(cx).display_map().hidden_lines().to_vec())
}

/// The result's fold rows: the line each sits above, and its text.
fn fold_notes(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Vec<(usize, String)> {
    cx.read(|cx| {
        view.read(cx)
            .result_editor()
            .read(cx)
            .spacer_notes()
            .iter()
            .filter(|note| note.text().ends_with("unchanged lines"))
            .map(|note| (note.line(), note.text().to_string()))
            .collect()
    })
}

/// The top panes' display rows.
fn row_count(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> usize {
    cx.read(|cx| view.read(cx).row_heights().rows())
}

fn notes(expected: &[(usize, &str)]) -> Vec<(usize, String)> {
    expected.iter().map(|&(line, text)| (line, text.to_owned())).collect()
}

/// At three context lines region 0 hides rows and result lines 0-96; region 2 hides rows
/// 104-296 and result lines 105-297; region 4 hides rows 304-399 and result lines 306-401.
const FOLDED: [Range<usize>; 3] = [0..97, 105..298, 306..402];
/// The top panes' display rows then: 400 rows, less all but one row of each fold.
const FOLDED_ROWS: usize = LINES - 96 - 192 - 95;

fn folded_notes() -> Vec<(usize, String)> {
    notes(&[(97, "97 unchanged lines"), (298, "193 unchanged lines"), (402, "96 unchanged lines")])
}

/// Region 2 open: its 193 rows show again.
fn middle_open(cx: &mut TestAppContext, view: &Entity<MergeWindow>) {
    assert_eq!(hidden(cx, view), [FOLDED[0].clone(), FOLDED[2].clone()]);
    assert_eq!(
        fold_notes(cx, view),
        notes(&[(97, "97 unchanged lines"), (402, "96 unchanged lines")])
    );
    assert_eq!(row_count(cx, view), FOLDED_ROWS + 192);
}

fn toggle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(merge_app::ToggleCollapseUnchanged), cx);
    });
    act(cx, window, TestWindowExt::render_frame);
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
}

fn type_text(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    act(cx, window, |window, cx| window.input(text, cx));
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<gpui_kit::ElementId>) {
    act(cx, window, |window, cx| window.click(id, cx));
    act(cx, window, TestWindowExt::render_frame);
}

/// A press on the result's spacer row above `line`, as the editor reports it.
fn click_result_spacer(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<MergeWindow>,
    line: usize,
) {
    act(cx, window, |_, cx| {
        view.read(cx).result_editor().clone().update(cx, |_, cx| {
            cx.emit(gpui_kit::component::input::InputEvent::SpacerClick { line });
        });
    });
    act(cx, window, TestWindowExt::render_frame);
}

fn set_query(cx: &mut TestAppContext, view: &Entity<MergeWindow>, query: &str) {
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| state.set_search_query(query, false, cx));
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_panes_open_whole_by_default(cx: &mut TestAppContext) {
    let (_, view) = open_set(cx, false, 3);
    assert_eq!(hidden(cx, &view), NO_LINES);
    assert_eq!(fold_notes(cx, &view), NO_NOTES);
    assert_eq!(row_count(cx, &view), LINES);
}

#[gpui_kit::test]
fn the_setting_collapses_unchanged_regions_to_their_context(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    assert_eq!(hidden(cx, &view), FOLDED);
    assert_eq!(fold_notes(cx, &view), folded_notes());
    assert_eq!(row_count(cx, &view), FOLDED_ROWS);
    act(cx, window, |window, _| {
        assert!(window.try_find(fold_row_id(RowIx::new(0))).is_some(), "region 0's fold row");
        assert!(window.try_find(fold_row_id(RowIx::new(104))).is_some(), "region 2's fold row");
    });
}

#[gpui_kit::test]
fn the_action_collapses_and_expands_every_region(cx: &mut TestAppContext) {
    let (window, view) = open_set(cx, false, 3);
    toggle(cx, window);
    assert_eq!(hidden(cx, &view), FOLDED);
    assert_eq!(row_count(cx, &view), FOLDED_ROWS);
    toggle(cx, window);
    assert_eq!(hidden(cx, &view), NO_LINES);
    assert_eq!(fold_notes(cx, &view), NO_NOTES);
    assert_eq!(row_count(cx, &view), LINES);
}

#[gpui_kit::test]
fn a_click_on_the_results_fold_row_opens_the_region_everywhere(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    click_result_spacer(cx, window, &view, 298);
    middle_open(cx, &view);
    // A spacer row that is no fold row opens nothing.
    click_result_spacer(cx, window, &view, 120);
    middle_open(cx, &view);
}

#[gpui_kit::test]
fn a_click_on_a_top_fold_row_opens_the_region_everywhere(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    click(cx, window, fold_row_id(RowIx::new(104)));
    middle_open(cx, &view);
}

#[gpui_kit::test]
fn the_action_after_an_open_collapses_every_region_again(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    click_result_spacer(cx, window, &view, 298);
    toggle(cx, window);
    toggle(cx, window);
    assert_eq!(hidden(cx, &view), FOLDED);
}

#[gpui_kit::test]
fn go_to_a_hidden_line_opens_its_region(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    press(cx, window, "secondary-l");
    // Line 200 is result line 199, in region 2's hidden lines.
    type_text(cx, window, "200");
    press(cx, window, "enter");
    act(cx, window, TestWindowExt::render_frame);
    middle_open(cx, &view);
}

#[gpui_kit::test]
fn go_to_a_shown_line_opens_nothing(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    press(cx, window, "secondary-l");
    // Line 103 is result line 102, region 2's first context line.
    type_text(cx, window, "103");
    press(cx, window, "enter");
    act(cx, window, TestWindowExt::render_frame);
    assert_eq!(hidden(cx, &view), FOLDED);
}

#[gpui_kit::test]
fn the_results_search_match_in_hidden_lines_opens_its_region(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    press(cx, window, "secondary-f");
    // `l250` is result line 251, in region 2's hidden lines.
    set_query(cx, &view, "l250");
    act(cx, window, TestWindowExt::render_frame);
    middle_open(cx, &view);
    // The same match does not open the region again once collapsed.
    toggle(cx, window);
    toggle(cx, window);
    act(cx, window, TestWindowExt::render_frame);
    assert_eq!(hidden(cx, &view), FOLDED);
}

#[gpui_kit::test]
fn a_top_panes_search_step_into_hidden_rows_opens_their_region(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    press(cx, window, "secondary-f");
    set_query(cx, &view, "l250");
    toggle(cx, window);
    toggle(cx, window);
    act(cx, window, TestWindowExt::render_frame);
    assert_eq!(hidden(cx, &view), FOLDED, "the result's match opened its region once");
    click(cx, window, input_match_next_id(LEFT_PANE));
    middle_open(cx, &view);
}

#[gpui_kit::test]
fn an_edit_to_a_context_line_shows_the_region_whole_until_undone(cx: &mut TestAppContext) {
    let (window, view) = collapsed(cx);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    // The start of result line 102, `l101`, region 2's first context line.
    let at = cx
        .read(|cx| view.read(cx).document().byte_of_line(merge_core::index::Line::new(102)).get());
    set_caret(cx, &view, at);
    type_text(cx, window, "x");
    act(cx, window, TestWindowExt::render_frame);
    assert_eq!(hidden(cx, &view), [FOLDED[0].clone(), FOLDED[2].clone()]);
    press(cx, window, "secondary-z");
    act(cx, window, TestWindowExt::render_frame);
    assert_eq!(hidden(cx, &view), FOLDED);
}

/// The rows' and the result's scroll tops, and the result's line height.
fn tops(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> (Pixels, Pixels, Pixels) {
    cx.read(|cx| {
        let view = view.read(cx);
        let result = view.result_editor().read(cx);
        (view.rows_scroll_top(), -result.scroll_offset().y, result.line_height().expect("laid out"))
    })
}

#[gpui_kit::test]
fn scroll_sync_counts_a_fold_row_as_one_row_in_each_pane(cx: &mut TestAppContext) {
    // At twenty context lines the result's display rows run: the fold of lines 0-79,
    // lines 80-101 (the conflict is 100-101), 102-121, the fold of 122-280, lines 281-300.
    // The rows': the fold of rows 0-79, rows 80-100 (the conflict is 100), 101-120, the fold
    // of 121-279, rows 280-299. Past the first conflict a result display row is the next
    // row's display row.
    let (window, view) = open_set(cx, true, 20);
    // The window opens scrolled to the first conflict.
    let (_, start, line) = tops(cx, &view);
    act(cx, window, |window, cx| {
        window.scroll(RESULT_ID, ScrollDelta::Pixels(point(px(0.), start - line * 45.)), cx);
    });
    settle(cx, window);
    let (rows_view, result_view) = cx.read(|cx| {
        let view = view.read(cx);
        (view.rows_view_height(), view.result_editor().read(cx).input_bounds().size.height)
    });
    let (rows, result, _) = tops(cx, &view);
    assert_eq!(result, line * 45.);
    // The result's anchor, mid-view, lies among lines 281-300: one display row more than
    // the rows' anchor.
    let expected = ROW_HEIGHT * 44. + (result_view - rows_view) / 2.;
    assert!((rows - expected).abs() < PIXEL_TOLERANCE, "rows at {rows:?}, not {expected:?}");
}
