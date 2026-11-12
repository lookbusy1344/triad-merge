//! Headless tests of picks, navigation and the current region (spec §5, §6.3; DESIGN.md §A6.2).

use crate::common;
use merge_core::index::RegionIx;
use merge_view::rows::RowIx;

use common::{act, caret, doc, open, set_caret, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use merge_app::window::{RESULT_ID, cell_id, pick_id};
use merge_app::{
    CarryMovedEdit, MergeWindow, Redo, TakeBase, TakeLeft, TakeRight, TakeRightAllUnresolved,
    TakeSortedUnion, TakeWholeLeft, TakeWholeRight, Undo,
};
use merge_core::document::{MergeDocument, Resolution};
use merge_core::merge3::Source;
use merge_view::rows::PaneSet;

const LEFT: Source = Source::Side(0);
const RIGHT: Source = Source::Side(1);
const BASE: Source = Source::Base(0);

/// Regions, one row each: 0 a, 1 b (conflict), 2 c, 3 D (left only), 4 e, 5 f (conflict), 6 g.
fn mixed() -> MergeDocument {
    doc("a\nb\nc\nd\ne\nf\ng\n", "a\nL\nc\nD\ne\nF\ng\n", "a\nR\nc\nd\ne\nX\ng\n")
}

const ROWS: usize = 7;

/// The second conflict's sides, which stay in the result while the first one changes.
const SECOND: &str = "F\nX\n";

fn start(cx: &mut TestAppContext, view: &Entity<MergeWindow>, region: RegionIx) -> usize {
    cx.read(|cx| view.read(cx).document().region_bytes(region).start)
}

fn resolution(cx: &mut TestAppContext, view: &Entity<MergeWindow>, region: RegionIx) -> Resolution {
    cx.read(|cx| view.read(cx).document().resolution(region).clone())
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
}

/// The window's text from region 1 on, up to the second conflict.
fn first_conflict_text(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> String {
    let (editor, core) = texts(cx, view);
    assert_eq!(editor, core);
    let rest = core.strip_prefix("a\n").expect("region 0 stays");
    let end = rest.find("c\nD\ne\n").expect("regions 2 to 4 stay");
    assert!(rest.ends_with(&format!("{SECOND}g\n")));
    rest[..end].to_owned()
}

fn opened(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let (window, view) = open(cx, mixed());
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    (window, view)
}

#[gpui_kit::test]
fn next_and_previous_difference_stop_at_the_ends(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    let starts: Vec<usize> = [1, 3, 5].map(|r| start(cx, &view, RegionIx::new(r))).to_vec();
    for &expected in &starts {
        press(cx, window, "secondary-down");
        assert_eq!(caret(cx, &view), expected);
    }
    press(cx, window, "secondary-down");
    assert_eq!(caret(cx, &view), starts[2], "no wrap past the last difference");
    press(cx, window, "secondary-up");
    assert_eq!(caret(cx, &view), starts[1]);
    press(cx, window, "secondary-up");
    press(cx, window, "secondary-up");
    assert_eq!(caret(cx, &view), starts[0], "no wrap before the first difference");
}

#[gpui_kit::test]
fn conflict_navigation_skips_resolved_conflicts(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-alt-down");
    assert_eq!(caret(cx, &view), start(cx, &view, RegionIx::new(1)));
    press(cx, window, "secondary-3");
    press(cx, window, "secondary-alt-down");
    let second = start(cx, &view, RegionIx::new(5));
    assert_eq!(caret(cx, &view), second);
    press(cx, window, "secondary-alt-up");
    assert_eq!(caret(cx, &view), second, "the first conflict is resolved");
}

#[gpui_kit::test]
fn choices_toggle_in_order_and_clear(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-down");
    press(cx, window, "secondary-1");
    assert_eq!(first_conflict_text(cx, &view), "L\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![LEFT]));
    assert_eq!(caret(cx, &view), start(cx, &view, RegionIx::new(1)), "the region stays current");
    press(cx, window, "secondary-3");
    assert_eq!(first_conflict_text(cx, &view), "L\nR\n");
    press(cx, window, "secondary-1");
    assert_eq!(first_conflict_text(cx, &view), "R\n");
    press(cx, window, "secondary-2");
    assert_eq!(first_conflict_text(cx, &view), "R\nb\n");
    assert_eq!(
        resolution(cx, &view, RegionIx::new(1)),
        Resolution::Pick(vec![RIGHT, Source::Base(0)])
    );
    press(cx, window, "secondary-0");
    assert_eq!(first_conflict_text(cx, &view), "");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(Vec::new()));
    // The empty region holds no byte, but stays current.
    press(cx, window, "secondary-1");
    assert_eq!(first_conflict_text(cx, &view), "L\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(2)), Resolution::Auto);
    press(cx, window, "secondary-z");
    press(cx, window, "secondary-z");
    assert_eq!(first_conflict_text(cx, &view), "R\nb\n");
    let status = cx.read(|cx| view.read(cx).status_text().clone());
    assert!(status.contains("2 conflicts · 1 unresolved"), "status: {status}");
}

#[gpui_kit::test]
fn a_pick_target_toggles_its_panes_source(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    // Region 1 is row 1; pane 1 is right.
    act(cx, window, |window, cx| {
        window.click(pick_id(1, RowIx::new(1)), cx);
    });
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![RIGHT]));
    assert_eq!(first_conflict_text(cx, &view), "R\n");
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![RIGHT, LEFT]));
    act(cx, window, |window, cx| {
        window.click(pick_id(1, RowIx::new(1)), cx);
    });
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![LEFT]));
}

#[gpui_kit::test]
fn unchanged_regions_have_no_pick_target(cx: &mut TestAppContext) {
    let (window, _view) = opened(cx);
    act(cx, window, |window, _| {
        assert!(window.try_find(pick_id(0, RowIx::new(0))).is_none());
        assert!(window.try_find(pick_id(0, RowIx::new(3))).is_some());
    });
}

#[gpui_kit::test]
fn clicking_a_row_makes_its_region_current(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    act(cx, window, |window, cx| {
        window.click(cell_id(1, RowIx::new(5)), cx);
    });
    assert_eq!(caret(cx, &view), start(cx, &view, RegionIx::new(5)));
    // The next choice acts on that region.
    press(cx, window, "secondary-1");
    assert_eq!(resolution(cx, &view, RegionIx::new(5)), Resolution::Pick(vec![LEFT]));
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Unresolved);
}

#[gpui_kit::test]
fn the_base_pane_toggles(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    let panes = |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).panes());
    press(cx, window, "secondary-b");
    assert_eq!(panes(cx), PaneSet::LeftBaseRight);
    // Pane 1 is now base: its pick target toggles base.
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        // Every row has three cells, including the rows drawn before the change.
        for row in 0..ROWS {
            assert!(window.try_find(cell_id(2, RowIx::new(row))).is_some(), "row {row}");
        }
        window.click(pick_id(1, RowIx::new(1)), cx);
    });
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![Source::Base(0)]));
    press(cx, window, "secondary-b");
    assert_eq!(panes(cx), PaneSet::LeftRight);
}

#[gpui_kit::test]
fn shift_alt_down_adds_a_cursor(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "shift-alt-down");
    act(cx, window, |window, cx| window.input("x", cx));
    let (editor, core) = texts(cx, &view);
    assert_eq!(editor, core);
    assert!(core.starts_with("xa\nxL\n"), "{core:?}");
}

#[gpui_kit::test]
fn the_base_pane_setting_opens_with_base_showing(cx: &mut TestAppContext) {
    let settings =
        merge_app::WindowSettings { base_pane: true, ..merge_app::WindowSettings::default() };
    let (_, view, _) = common::open_with(cx, mixed(), &settings);
    assert_eq!(cx.read(|cx| view.read(cx).panes()), PaneSet::LeftBaseRight);
}

#[gpui_kit::test]
fn ordered_choices_take_both_sides_in_that_order(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-down");
    press(cx, window, "secondary-5");
    assert_eq!(first_conflict_text(cx, &view), "R\nL\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![RIGHT, LEFT]));
    assert_eq!(caret(cx, &view), start(cx, &view, RegionIx::new(1)), "the region stays current");
    // A take sets the pick; pressing it again changes nothing.
    press(cx, window, "secondary-5");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![RIGHT, LEFT]));
    press(cx, window, "secondary-4");
    assert_eq!(first_conflict_text(cx, &view), "L\nR\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![LEFT, RIGHT]));
    press(cx, window, "secondary-z");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![RIGHT, LEFT]));
}

#[gpui_kit::test]
fn taking_one_side_replaces_the_pick(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-down");
    press(cx, window, "secondary-4");
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeLeft), cx);
    });
    assert_eq!(first_conflict_text(cx, &view), "L\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![LEFT]));
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeLeft), cx);
    });
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![LEFT]), "no toggle");
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeRight), cx);
    });
    assert_eq!(first_conflict_text(cx, &view), "R\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![RIGHT]));
}

#[gpui_kit::test]
fn taking_base_replaces_the_pick_with_base_and_undoes(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-down");
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeLeft), cx);
    });
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeBase), cx);
    });
    assert_eq!(first_conflict_text(cx, &view), "b\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![BASE]));
    assert_eq!(caret(cx, &view), start(cx, &view, RegionIx::new(1)));
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(Undo), cx);
    });
    assert_eq!(first_conflict_text(cx, &view), "L\n");
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![LEFT]));
}

fn dispatch(cx: &mut TestAppContext, window: AnyWindowHandle, action: Box<dyn gpui_kit::Action>) {
    act(cx, window, |window, cx| window.dispatch_action(action, cx));
}

#[gpui_kit::test]
fn taking_right_for_all_unresolved_keeps_a_conflict_already_picked(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-down");
    dispatch(cx, window, Box::new(TakeLeft));
    dispatch(cx, window, Box::new(TakeRightAllUnresolved));
    let taken = "a\nL\nc\nD\ne\nX\ng\n".to_owned();
    assert_eq!(texts(cx, &view), (taken.clone(), taken.clone()));
    assert_eq!(resolution(cx, &view, RegionIx::new(5)), Resolution::Pick(vec![RIGHT]));
    dispatch(cx, window, Box::new(Undo));
    let before = "a\nL\nc\nD\ne\nF\nX\ng\n".to_owned();
    assert_eq!(texts(cx, &view), (before.clone(), before));
    assert_eq!(
        resolution(cx, &view, RegionIx::new(1)),
        Resolution::Pick(vec![LEFT]),
        "one undo reverts the bulk take only"
    );
    dispatch(cx, window, Box::new(Redo));
    assert_eq!(texts(cx, &view), (taken.clone(), taken));
}

#[gpui_kit::test]
fn taking_a_whole_file_gives_that_inputs_text_in_editor_and_core(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    dispatch(cx, window, Box::new(TakeWholeLeft));
    let left = "a\nL\nc\nD\ne\nF\ng\n".to_owned();
    assert_eq!(texts(cx, &view), (left.clone(), left.clone()));
    dispatch(cx, window, Box::new(TakeWholeRight));
    let right = "a\nR\nc\nd\ne\nX\ng\n".to_owned();
    assert_eq!(texts(cx, &view), (right.clone(), right));
    dispatch(cx, window, Box::new(Undo));
    assert_eq!(texts(cx, &view), (left.clone(), left));

    // Left deletes `b`; right changes it and adds `d` at the end, an empty range in left.
    let (window, view) = open(cx, doc("a\nb\nc\n", "a\nc\n", "a\nB\nc\nd\n"));
    dispatch(cx, window, Box::new(TakeWholeLeft));
    assert_eq!(texts(cx, &view), ("a\nc\n".to_owned(), "a\nc\n".to_owned()));
    dispatch(cx, window, Box::new(TakeWholeRight));
    let right = "a\nB\nc\nd\n".to_owned();
    assert_eq!(texts(cx, &view), (right.clone(), right));
}

/// Imports each side adds out of order: region 1 is the conflict, `use c;` against `use b;`.
fn imports() -> MergeDocument {
    doc("use a;\nuse d;\n", "use a;\nuse c;\nuse d;\n", "use a;\nuse b;\nuse d;\n")
}

#[gpui_kit::test]
fn taking_the_sorted_union_replaces_the_region_and_undoes(cx: &mut TestAppContext) {
    let (window, view) = open(cx, imports());
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    press(cx, window, "secondary-down");
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeSortedUnion), cx);
    });
    let union = "use a;\nuse b;\nuse c;\nuse d;\n".to_owned();
    assert_eq!(texts(cx, &view), (union.clone(), union));
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Edited);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(Undo), cx);
    });
    let sides = "use a;\nuse c;\nuse b;\nuse d;\n".to_owned();
    assert_eq!(texts(cx, &view), (sides.clone(), sides));
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Unresolved);
}

#[gpui_kit::test]
fn taking_the_sorted_union_without_one_changes_nothing(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-down");
    let before = texts(cx, &view);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeSortedUnion), cx);
    });
    assert_eq!(texts(cx, &view), before);
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Unresolved);
}

/// Left moves `m1 m2` below `a b c`; right edits `m2`. Regions: 0 the conflict over `m1 m2`,
/// 1 `a b c`, 2 left's insert of `m1 m2`.
fn moved() -> MergeDocument {
    doc("m1\nm2\na\nb\nc\n", "a\nb\nc\nm1\nm2\n", "m1\nM2\na\nb\nc\n")
}

#[gpui_kit::test]
fn carrying_a_moved_edit_moves_it_in_one_undo_step(cx: &mut TestAppContext) {
    let (window, view) = open(cx, moved());
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(CarryMovedEdit), cx);
    });
    let carried = "a\nb\nc\nm1\nM2\n".to_owned();
    assert_eq!(texts(cx, &view), (carried.clone(), carried));
    assert_eq!(resolution(cx, &view, RegionIx::new(0)), Resolution::Edited);
    assert_eq!(resolution(cx, &view, RegionIx::new(2)), Resolution::Edited);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(Undo), cx);
    });
    // The conflict shows every side in order: left's nothing, then right's `m1 M2`.
    let sides = "m1\nM2\na\nb\nc\nm1\nm2\n".to_owned();
    assert_eq!(texts(cx, &view), (sides.clone(), sides));
    assert_eq!(resolution(cx, &view, RegionIx::new(0)), Resolution::Unresolved);
    assert_eq!(resolution(cx, &view, RegionIx::new(2)), Resolution::Auto);
}

#[gpui_kit::test]
fn carrying_a_moved_edit_without_a_move_changes_nothing(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    press(cx, window, "secondary-down");
    let before = texts(cx, &view);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(CarryMovedEdit), cx);
    });
    assert_eq!(texts(cx, &view), before);
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Unresolved);
}

#[gpui_kit::test]
fn clicking_a_conflicts_only_picked_side_again_leaves_it_unresolved(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Pick(vec![LEFT]));
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!(resolution(cx, &view, RegionIx::new(1)), Resolution::Unresolved);
    assert_eq!(first_conflict_text(cx, &view), "L\nR\n");
}

/// Regions: 0 a, 1 B (left only), 2 c, 3 conflict (D or X), 4 e.
fn one_side_then_conflict() -> MergeDocument {
    doc("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n", "a\nb\nc\nX\ne\n")
}

#[gpui_kit::test]
fn the_window_opens_on_the_first_unresolved_conflict(cx: &mut TestAppContext) {
    let (window, view) = open(cx, one_side_then_conflict());
    // "a\nB\nc\n" comes before the conflict.
    assert_eq!(caret(cx, &view), 6);
    // A choice acts on that conflict without any navigation first.
    press(cx, window, "secondary-1");
    let (editor, core) = texts(cx, &view);
    assert_eq!(editor, core);
    assert_eq!(core, "a\nB\nc\nD\ne\n");
}

#[gpui_kit::test]
fn without_a_conflict_the_window_opens_on_the_first_difference(cx: &mut TestAppContext) {
    let (_, view) = open(cx, doc("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n"));
    assert_eq!(caret(cx, &view), 2);
}

#[gpui_kit::test]
fn without_a_difference_the_window_opens_at_the_start(cx: &mut TestAppContext) {
    let (_, view) = open(cx, doc("a\nb\n", "a\nb\n", "a\nb\n"));
    assert_eq!(caret(cx, &view), 0);
}
