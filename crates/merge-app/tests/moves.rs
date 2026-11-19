//! Moved blocks in the merge window's top panes (`PLAN.md` §G16).

use crate::common;

use common::{act, bounds, doc, label, open, open_with, set_caret, three};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Action as _, TestAppContext, px};
use merge_app::window::{cell_id, inherited_pane_pill_id, move_pill_id};
use merge_app::{GoToMoveEnd, TakeBase, WindowSettings};
use merge_core::index::RegionIx;
use merge_core::merge3::Source;
use merge_view::rows::{PaneSet, RowIx};

/// `m1 m2` before `a b c`, which outweighs it in every diff.
const BLOCK_FIRST: &str = "m1\nm2\na\nb\nc\n";
const BLOCK_LAST: &str = "a\nb\nc\nm1\nm2\n";
/// Both sides move `m1 m2` from the top: left to the end, right before `d`.
const TWICE: [&str; 3] =
    ["m1\nm2\na\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\nm1\nm2\n", "a\nb\nc\nm1\nm2\nd\ne\n"];

/// The space between two pills on one row (`STYLE.md`, Pill).
const MOVE_PILL_GAP: gpui_kit::Pixels = px(4.);

const LEFT: Source = Source::Side(0);
const RIGHT: Source = Source::Side(1);
const BASE: Source = Source::Base(0);

/// The pill in `pane` on `row` naming `other`, if the last frame drew one.
fn pill_to(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    pane: usize,
    row: usize,
    other: Source,
) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
    let mut found = None;
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        found =
            window.try_find(move_pill_id(pane, RowIx::new(row), other)).map(|pill| pill.bounds());
    });
    found
}

/// The pill in `pane` on `row`, naming any input, if the last frame drew one.
fn pill(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    pane: usize,
    row: usize,
) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
    [BASE, LEFT, RIGHT].into_iter().find_map(|other| pill_to(cx, window, pane, row, other))
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
        label(cx, window, move_pill_id(0, RowIx::new(0), LEFT)),
        "Moved block: go to its other end, left line 4"
    );
    assert_eq!(
        label(cx, window, move_pill_id(0, RowIx::new(5), BASE)),
        "Moved block: go to its other end, base line 1"
    );
    assert_eq!(pill(cx, window, 1, 0), None, "right moved nothing");
    assert_eq!(pill(cx, window, 0, 1), None, "one pill per end, on its first row");
}

/// Left moves `m1 m2` from the top over `x`, which right changes to `y`: the move's far end
/// starts the conflict at `x`.
const MOVE_INTO_CONFLICT: [&str; 3] =
    ["m1\nm2\na\nb\nc\nx\n", "a\nb\nc\nm1\nm2\n", "m1\nm2\na\nb\nc\ny\n"];

#[gpui_kit::test]
fn an_inherited_pill_follows_a_move_pill_on_the_same_row(cx: &mut TestAppContext) {
    let [base, left, right] = MOVE_INTO_CONFLICT;
    // Regions: left deletes `m1 m2` (rows 0–1), `a b c` (rows 2–4), the conflict of left's
    // `m1 m2` and right's `y` over `x` (rows 5–6), region 2. A parent holds the same conflict.
    let settings =
        WindowSettings { parents: vec![three(base, left, right)], ..WindowSettings::default() };
    let (window, ..) = open_with(cx, doc(base, left, right), &settings);
    let conflict = RegionIx::new(2);
    let moved = pill_to(cx, window, 0, 5, BASE).expect("the move pill shows");
    let inherited = bounds(cx, window, inherited_pane_pill_id(0, conflict));
    let cell = bounds(cx, window, cell_id(0, RowIx::new(5)));
    assert_eq!((moved.top(), inherited.top()), (cell.top(), cell.top()), "both on the row");
    // The run reads move pill, gap, inherited pill, against the pane's right edge.
    assert_eq!(inherited.left() - moved.right(), MOVE_PILL_GAP);
    assert!(moved.size.width > px(0.) && inherited.size.width > px(0.));
    // Right's pane holds no move end on the row: the inherited pill stands alone there.
    assert_eq!(pill(cx, window, 1, 5), None);
    assert_eq!(
        bounds(cx, window, inherited_pane_pill_id(1, conflict)).top(),
        bounds(cx, window, cell_id(1, RowIx::new(5))).top()
    );
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
    act(cx, window, |window, cx| window.click(move_pill_id(0, RowIx::new(0), LEFT), cx));
    assert_eq!(current(cx, &view), RegionIx::new(2));
    assert!(cx.read(|cx| view.read(cx).flashing()), "the jump blinks");
    act(cx, window, |window, cx| window.click(move_pill_id(0, RowIx::new(5), BASE), cx));
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
        label(cx, window, move_pill_id(1, RowIx::new(3), BASE)),
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
        label(cx, window, move_pill_id(0, RowIx::new(0), LEFT)),
        "Moved block: go to its other end, left line 6"
    );
    assert_eq!(
        label(cx, window, move_pill_id(2, RowIx::new(0), RIGHT)),
        "Moved block: go to its other end, right line 4"
    );
    assert!(pill(cx, window, 0, 9).is_some());
    assert!(pill(cx, window, 2, 5).is_some());
}

#[gpui_kit::test]
fn base_shows_one_pill_per_move_of_its_lines_left_first(cx: &mut TestAppContext) {
    // Base pane 1, rows 0–1: base's `m1 m2`, which left moves to its line 6 and right to its
    // line 4.
    let [base, left, right] = TWICE;
    let settings = WindowSettings { base_pane: true, ..WindowSettings::default() };
    let (window, _, _) = open_with(cx, doc(base, left, right), &settings);
    assert_eq!(
        label(cx, window, move_pill_id(1, RowIx::new(0), LEFT)),
        "Moved block: go to its other end, left line 6"
    );
    assert_eq!(
        label(cx, window, move_pill_id(1, RowIx::new(0), RIGHT)),
        "Moved block: go to its other end, right line 4"
    );
    let to_left = pill_to(cx, window, 1, 0, LEFT).expect("left's pill");
    let to_right = pill_to(cx, window, 1, 0, RIGHT).expect("right's pill");
    let cell = bounds(cx, window, cell_id(1, RowIx::new(0)));
    assert_eq!(to_right.right(), cell.right() - px(16.));
    assert_eq!(to_left.right(), to_right.left() - px(4.), "left's pill first, 4 px apart");
    assert_eq!(to_left.top(), cell.top());
    assert_eq!(pill(cx, window, 1, 1), None, "pills on an end's first row only");
}

#[gpui_kit::test]
fn a_click_on_a_base_pill_goes_to_that_sides_copy(cx: &mut TestAppContext) {
    // Right's copy is region 2, left's region 4.
    let [base, left, right] = TWICE;
    let settings = WindowSettings { base_pane: true, ..WindowSettings::default() };
    let (window, view, _) = open_with(cx, doc(base, left, right), &settings);
    act(cx, window, |window, cx| window.click(move_pill_id(1, RowIx::new(0), RIGHT), cx));
    assert_eq!(current(cx, &view), RegionIx::new(2));
    act(cx, window, |window, cx| window.click(move_pill_id(1, RowIx::new(0), LEFT), cx));
    assert_eq!(current(cx, &view), RegionIx::new(4));
    assert_eq!(panes(cx, &view), PaneSet::LeftBaseRight);
}

#[gpui_kit::test]
fn a_merge_without_moves_shows_no_pills(cx: &mut TestAppContext) {
    let (window, _) = open(cx, doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n"));
    for (pane, row) in [(0, 0), (0, 1), (1, 1)] {
        assert_eq!(pill(cx, window, pane, row), None);
    }
}

#[gpui_kit::test]
fn the_status_bar_counts_blocks_the_result_holds_twice_until_one_goes(cx: &mut TestAppContext) {
    // Right's copy is region 2, result bytes 6–12 (`m1 m2` after `a b c`).
    let [base, left, right] = TWICE;
    let (window, view) = open(cx, doc(base, left, right));
    let status = |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).status_text().to_string());
    assert!(status(cx).contains(" · 1 duplicated block · "), "{}", status(cx));
    set_caret(cx, &view, 8);
    act(cx, window, |window, cx| window.dispatch_action(TakeBase.boxed_clone(), cx));
    cx.run_until_parked();
    assert!(!status(cx).contains("duplicated"), "{}", status(cx));
}

fn panes(cx: &mut TestAppContext, view: &gpui_kit::Entity<merge_app::MergeWindow>) -> PaneSet {
    cx.read(|cx| view.read(cx).panes())
}

#[gpui_kit::test]
fn a_click_on_a_pill_naming_base_shows_the_base_pane(cx: &mut TestAppContext) {
    // Base hides, so the deletion by both has no rows; right's copy is on rows 3–4 of pane 1,
    // its pill naming base line 1.
    let [base, left, right] = TWICE;
    let (window, view) = open(cx, doc(base, left, right));
    assert_eq!(panes(cx, &view), PaneSet::LeftRight);
    act(cx, window, |window, cx| window.click(move_pill_id(1, RowIx::new(3), BASE), cx));
    assert_eq!(panes(cx, &view), PaneSet::LeftBaseRight);
    assert_eq!(current(cx, &view), RegionIx::new(0), "at the deletion, now on rows 0–1");
    assert!(pill(cx, window, 2, 0).is_some(), "right's origin pill, in pane 2 now");
}

#[gpui_kit::test]
fn a_click_on_a_pill_naming_the_side_leaves_the_panes(cx: &mut TestAppContext) {
    let (window, view) = open(cx, doc(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST));
    act(cx, window, |window, cx| window.click(move_pill_id(0, RowIx::new(0), LEFT), cx));
    assert_eq!(current(cx, &view), RegionIx::new(2));
    assert_eq!(panes(cx, &view), PaneSet::LeftRight);
}

#[gpui_kit::test]
fn go_to_other_end_from_a_destination_shows_the_base_pane(cx: &mut TestAppContext) {
    // The result is left's text: `m1 m2`, left's destination, is bytes 6–12.
    let (window, view) = open(cx, doc(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST));
    set_caret(cx, &view, 8);
    act(cx, window, |window, cx| window.dispatch_action(GoToMoveEnd.boxed_clone(), cx));
    cx.run_until_parked();
    assert_eq!(panes(cx, &view), PaneSet::LeftBaseRight);
    assert_eq!(current(cx, &view), RegionIx::new(0));
}
