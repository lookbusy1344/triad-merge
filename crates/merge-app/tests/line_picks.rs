//! Headless tests of picking lines one by one in the top panes (spec §4.4, §5; `PLAN.md`
//! §G8): presses on line numbers, shift-presses and Pick Selected Lines.

use crate::common;

use common::{act, doc, label, open, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, Modifiers, Pixels, Point, TestAppContext, point, px};
use merge_app::MergeWindow;
use merge_app::window::{cell_text_id, line_pick_id};
use merge_config::settings::DEFAULT_FONT_SIZE;
use merge_core::document::{MergeDocument, Resolution};
use merge_core::index::RegionIx;
use merge_core::line_pick::PickedLines;
use merge_core::merge3::Source;
use merge_view::rows::RowIx;

const LEFT_PANE: usize = 0;
const RIGHT_PANE: usize = 1;
const LEFT: Source = Source::Side(0);
const RIGHT: Source = Source::Side(1);
const CONFLICT: RegionIx = RegionIx::new(1);

/// The headless text system advances every character 0.6 em.
const CHAR_EMS: f32 = 0.6;

/// Rows: 0 `a`, 1–3 the conflict (left `L1 L2`, right `R1 R2 R3`), 4 `c`.
fn conflict() -> MergeDocument {
    doc("a\nb1\nb2\nc\n", "a\nL1\nL2\nc\n", "a\nR1\nR2\nR3\nc\n")
}

fn opened(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let opened = open(cx, conflict());
    act(cx, opened.0, |window, _| window.activate_window());
    cx.run_until_parked();
    opened
}

fn press_line(cx: &mut TestAppContext, window: AnyWindowHandle, pane: usize, row: usize) {
    act(cx, window, |window, cx| window.click(line_pick_id(pane, RowIx::new(row)), cx));
}

fn shift_press_line(cx: &mut TestAppContext, window: AnyWindowHandle, pane: usize, row: usize) {
    act(cx, window, |window, cx| {
        window.click_with_modifiers(line_pick_id(pane, RowIx::new(row)), Modifiers::shift(), cx);
    });
}

fn resolution(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Resolution {
    cx.read(|cx| view.read(cx).document().resolution(CONFLICT).clone())
}

fn result(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> String {
    let (editor, core) = texts(cx, view);
    assert_eq!(editor, core);
    core
}

fn picked(source: Source, lines: std::ops::Range<usize>) -> PickedLines {
    PickedLines::new(source, lines)
}

fn toggled(cx: &mut TestAppContext, window: AnyWindowHandle, pane: usize, row: usize) -> bool {
    let mut checked = None;
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        checked = window.find(line_pick_id(pane, RowIx::new(row))).checked();
    });
    checked.expect("a line target is a toggle")
}

/// The window point a character into `pane`'s text on `row`, half a row down.
fn in_text(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    pane: usize,
    row: usize,
) -> Point<Pixels> {
    let mut found = None;
    act(cx, window, |window, _| {
        let bounds = window.find(cell_text_id(pane, RowIx::new(row))).bounds();
        found = Some(point(bounds.origin.x, bounds.center().y));
    });
    let start = found.expect("the cell shows");
    point(start.x + px(DEFAULT_FONT_SIZE * CHAR_EMS), start.y)
}

#[gpui_kit::test]
fn a_press_on_a_line_number_picks_that_line(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press_line(cx, window, RIGHT_PANE, 2);
    assert_eq!(resolution(cx, &view), Resolution::Lines(vec![picked(RIGHT, 1..2)]));
    assert_eq!(result(cx, &view), "a\nR2\nc\n");
    press_line(cx, window, LEFT_PANE, 1);
    assert_eq!(result(cx, &view), "a\nR2\nL1\nc\n", "right was picked first");
}

#[gpui_kit::test]
fn a_second_press_turns_the_line_off(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press_line(cx, window, RIGHT_PANE, 2);
    press_line(cx, window, RIGHT_PANE, 2);
    assert_eq!(resolution(cx, &view), Resolution::Unresolved);
}

#[gpui_kit::test]
fn a_shift_press_picks_the_run_from_the_last_line_pressed(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press_line(cx, window, RIGHT_PANE, 3);
    shift_press_line(cx, window, RIGHT_PANE, 1);
    assert_eq!(resolution(cx, &view), Resolution::Pick(vec![RIGHT]), "every line of right");
    assert_eq!(result(cx, &view), "a\nR1\nR2\nR3\nc\n");
    shift_press_line(cx, window, LEFT_PANE, 2);
    shift_press_line(cx, window, LEFT_PANE, 1);
    assert_eq!(
        resolution(cx, &view),
        Resolution::Pick(vec![RIGHT, LEFT]),
        "the run starts at the last line pressed, shift or not, and covers all of left"
    );
}

#[gpui_kit::test]
fn a_shift_press_in_another_pane_picks_its_line_alone(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press_line(cx, window, RIGHT_PANE, 1);
    shift_press_line(cx, window, LEFT_PANE, 2);
    assert_eq!(
        resolution(cx, &view),
        Resolution::Lines(vec![picked(RIGHT, 0..1), picked(LEFT, 1..2)])
    );
}

#[gpui_kit::test]
fn a_shift_press_without_an_earlier_press_picks_its_line_alone(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    shift_press_line(cx, window, RIGHT_PANE, 2);
    assert_eq!(resolution(cx, &view), Resolution::Lines(vec![picked(RIGHT, 1..2)]));
}

#[gpui_kit::test]
fn line_targets_show_and_name_their_state(cx: &mut TestAppContext) {
    let (window, _) = opened(cx);
    press_line(cx, window, RIGHT_PANE, 2);
    assert!([1, 3].iter().all(|&row| !toggled(cx, window, RIGHT_PANE, row)));
    assert!(toggled(cx, window, RIGHT_PANE, 2));
    assert!(!toggled(cx, window, LEFT_PANE, 1));
    assert_eq!(label(cx, window, line_pick_id(RIGHT_PANE, RowIx::new(2))), "Line 3 of Right");
}

#[gpui_kit::test]
fn unchanged_rows_and_filler_rows_have_no_line_target(cx: &mut TestAppContext) {
    let (window, _) = opened(cx);
    act(cx, window, |window, _| {
        assert!(window.try_find(line_pick_id(LEFT_PANE, RowIx::new(0))).is_none());
        assert!(window.try_find(line_pick_id(LEFT_PANE, RowIx::new(4))).is_none());
        assert!(window.try_find(line_pick_id(LEFT_PANE, RowIx::new(3))).is_none(), "filler");
        assert!(window.try_find(line_pick_id(RIGHT_PANE, RowIx::new(3))).is_some());
    });
}

#[gpui_kit::test]
fn pick_selected_lines_toggles_the_lines_a_selection_touches(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    let (from, to) = (in_text(cx, window, RIGHT_PANE, 1), in_text(cx, window, RIGHT_PANE, 2));
    act(cx, window, |window, cx| window.drag(from, to, cx));
    cx.run_until_parked();
    act(cx, window, |window, cx| window.press("secondary-shift-l", cx));
    assert_eq!(resolution(cx, &view), Resolution::Lines(vec![picked(RIGHT, 0..2)]));
    act(cx, window, |window, cx| window.press("secondary-shift-l", cx));
    assert_eq!(resolution(cx, &view), Resolution::Unresolved, "the same lines turn off");
}

#[gpui_kit::test]
fn pick_selected_lines_without_a_selection_does_nothing(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    act(cx, window, |window, cx| window.press("secondary-shift-l", cx));
    assert_eq!(resolution(cx, &view), Resolution::Unresolved);
}
