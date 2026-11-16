//! Moved blocks in the merge window's top panes (`PLAN.md` §G16).

use crate::common;

use common::{act, bounds, doc, label, open, open_with, set_caret};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Action as _, TestAppContext, px};
use merge_app::window::{cell_id, move_pill_id};
use merge_app::{GoToMoveEnd, WindowSettings};
use merge_core::index::RegionIx;
use merge_view::rows::RowIx;

/// `m1 m2` before `a b c`, which outweighs it in every diff.
const BLOCK_FIRST: &str = "m1\nm2\na\nb\nc\n";
const BLOCK_LAST: &str = "a\nb\nc\nm1\nm2\n";
/// Both sides move `m1 m2` from the top: left to the end, right before `d`.
const TWICE: [&str; 3] =
    ["m1\nm2\na\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\nm1\nm2\n", "a\nb\nc\nm1\nm2\nd\ne\n"];

/// The pill in `pane` on `row`, if the last frame drew one.
fn pill(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    pane: usize,
    row: usize,
) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
    let mut found = None;
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        found = window.try_find(move_pill_id(pane, RowIx::new(row))).map(|pill| pill.bounds());
    });
    found
}

fn current(cx: &mut TestAppContext, view: &gpui_kit::Entity<merge_app::MergeWindow>) -> RegionIx {
    cx.read(|cx| view.read(cx).current_region(cx))
}

#[gpui_kit::test]
fn each_end_of_a_left_move_shows_a_pill_naming_the_other_ends_line(cx: &mut TestAppContext) {
    // Left pane: left deletes `m1 m2` (rows 0–1), `a b c` (rows 2–4), left inserts it
    // (rows 5–6) as its lines 3–4.
    let (window, _) = open(cx, doc(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST));
    assert_eq!(
        label(cx, window, move_pill_id(0, RowIx::new(0))),
        "Moved block: go to its other end, left line 4"
    );
    assert_eq!(
        label(cx, window, move_pill_id(0, RowIx::new(5))),
        "Moved block: go to its other end, base line 1"
    );
    assert_eq!(pill(cx, window, 1, 0), None, "right moved nothing");
    assert_eq!(pill(cx, window, 0, 1), None, "one pill per end, on its first row");
}

#[gpui_kit::test]
fn a_pill_sits_on_its_row_against_the_panes_right_edge(cx: &mut TestAppContext) {
    let (window, _) = open(cx, doc(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST));
    for row in [0, 5] {
        let pill = pill(cx, window, 0, row).expect("the pill shows");
        let cell = bounds(cx, window, cell_id(0, RowIx::new(row)));
        assert_eq!(pill.top(), cell.top(), "row {row}");
        assert_eq!(pill.size.height, cell.size.height, "row {row}");
        assert_eq!(pill.right(), cell.right() - px(16.), "row {row}");
    }
}

#[gpui_kit::test]
fn a_click_on_a_pill_goes_to_the_other_end(cx: &mut TestAppContext) {
    let (window, view) = open(cx, doc(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.click(move_pill_id(0, RowIx::new(0)), cx));
    assert_eq!(current(cx, &view), RegionIx::new(2));
    assert!(cx.read(|cx| view.read(cx).flashing()), "the jump blinks");
    act(cx, window, |window, cx| window.click(move_pill_id(0, RowIx::new(5)), cx));
    assert_eq!(current(cx, &view), RegionIx::new(0));
}

#[gpui_kit::test]
fn go_to_other_end_of_move_jumps_from_either_end(cx: &mut TestAppContext) {
    // The result is left's text: region 0 holds nothing, `a b c` bytes 0–6, `m1 m2` 6–12.
    let (window, view) = open(cx, doc(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST));
    let go = |cx: &mut TestAppContext| {
        act(cx, window, |window, cx| window.dispatch_action(GoToMoveEnd.boxed_clone(), cx));
        cx.run_until_parked();
    };
    set_caret(cx, &view, 8);
    go(cx);
    assert_eq!(current(cx, &view), RegionIx::new(0));
    go(cx);
    assert_eq!(current(cx, &view), RegionIx::new(2));
    set_caret(cx, &view, 2);
    go(cx);
    assert_eq!(current(cx, &view), RegionIx::new(1), "`a b c` holds no end");
}

#[gpui_kit::test]
fn a_block_both_sides_move_has_pills_in_both_panes(cx: &mut TestAppContext) {
    // Without base the deletion by both has no rows, so no origin pill. Right inserts the
    // block on rows 3–4, left on rows 7–8.
    let [base, left, right] = TWICE;
    let (window, _) = open(cx, doc(base, left, right));
    let shown = |cx: &mut TestAppContext, pane, row| pill(cx, window, pane, row).is_some();
    assert!(shown(cx, 0, 7));
    assert!(shown(cx, 1, 3));
    assert!(!shown(cx, 0, 0) && !shown(cx, 1, 0));
    assert_eq!(
        label(cx, window, move_pill_id(1, RowIx::new(3))),
        "Moved block: go to its other end, base line 1"
    );
}

#[gpui_kit::test]
fn with_base_shown_a_block_both_sides_move_has_an_origin_in_each_side(cx: &mut TestAppContext) {
    // The deletion takes rows 0–1; every later row moves down two. Right is pane 2.
    let [base, left, right] = TWICE;
    let settings = WindowSettings { base_pane: true, ..WindowSettings::default() };
    let (window, _, _) = open_with(cx, doc(base, left, right), &settings);
    assert_eq!(
        label(cx, window, move_pill_id(0, RowIx::new(0))),
        "Moved block: go to its other end, left line 6"
    );
    assert_eq!(
        label(cx, window, move_pill_id(2, RowIx::new(0))),
        "Moved block: go to its other end, right line 4"
    );
    assert!(pill(cx, window, 0, 9).is_some());
    assert!(pill(cx, window, 2, 5).is_some());
    assert_eq!(pill(cx, window, 1, 0), None, "base moves nothing");
}

#[gpui_kit::test]
fn a_merge_without_moves_shows_no_pills(cx: &mut TestAppContext) {
    let (window, _) = open(cx, doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n"));
    for (pane, row) in [(0, 0), (0, 1), (1, 1)] {
        assert_eq!(pill(cx, window, pane, row), None);
    }
}
