//! Headless tests of find, go to line, the toolbar and the menus (spec §6.3; DESIGN.md §A6.2).

use crate::common;
use merge_core::index::Line;
use merge_core::index::RegionIx;
use merge_view::rows::RowIx;

use std::fmt::Write as _;

use common::{act, caret, doc, open, set_caret, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, OwnedMenuItem, TestAppContext};
use merge_app::toolbar::{
    Badge, COMPACT_MIN_WIDTH, LABELS_MIN_WIDTH, MORE_ID, MORE_LABEL_ID, MORE_MENU_BASE,
    MORE_MENU_LEFT_THEN_RIGHT, MORE_MENU_NONE, MORE_MENU_RIGHT_THEN_LEFT, POPUP_MENU_ID, Tool,
    labels_min_width,
};
use merge_app::window::{CONFLICT_BADGE_ID, GO_TO_LINE_ID, RESULT_ID, TOOLBAR_ID, cell_id};
use merge_app::{Find, MergeWindow, Typography, WindowSettings};
use merge_core::document::{Labels, MergeDocument, MergeOptions, Resolution};
use merge_core::merge3::{Inputs, Source};
use merge_view::rows::PaneSet;

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
}

fn type_text(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    act(cx, window, |window, cx| window.input(text, cx));
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<gpui_kit::ElementId>) {
    act(cx, window, |window, cx| window.click(id, cx));
}

fn bar_open(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> bool {
    cx.read(|cx| view.read(cx).go_to_line_open())
}

fn search_open(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> bool {
    cx.read(|cx| view.read(cx).result_editor().read(cx).search_session().open)
}

/// Lines a, B, C, d: region 1 is the left side's two lines, merged automatically.
fn two_line_region() -> MergeDocument {
    doc("a\nb\nc\nd\n", "a\nB\nC\nd\n", "a\nb\nc\nd\n")
}

/// Regions, one row each: 0 a, 1 b (conflict), 2 c, 3 D (left only), 4 e.
fn mixed() -> MergeDocument {
    doc("a\nb\nc\nd\ne\n", "a\nL\nc\nD\ne\n", "a\nR\nc\nd\ne\n")
}

const NUMBERED_LINES: usize = 200;

/// 200 unchanged lines, `l1` to `l200`: more rows than the window shows.
fn numbered() -> MergeDocument {
    let text = (1..=NUMBERED_LINES).fold(String::new(), |mut text, n| {
        writeln!(text, "l{n}").expect("a String takes any write");
        text
    });
    doc(&text, &text, &text)
}

fn in_result(
    cx: &mut TestAppContext,
    doc: MergeDocument,
) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let (window, view) = open(cx, doc);
    click(cx, window, RESULT_ID);
    set_caret(cx, &view, 0);
    (window, view)
}

#[gpui_kit::test]
fn go_to_line_moves_the_caret_inside_a_merged_region(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, two_line_region());
    press(cx, window, "secondary-l");
    assert!(bar_open(cx, &view));
    type_text(cx, window, "3");
    press(cx, window, "enter");
    assert!(!bar_open(cx, &view));
    // Line 3 is C, after "a\nB\n".
    assert_eq!(caret(cx, &view), 4);
    assert_eq!(cx.read(|cx| view.read(cx).current_region(cx)), RegionIx::new(1));
    // The result has the focus again.
    type_text(cx, window, "x");
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "a\nB\nxC\nd\n");
    assert_eq!(editor, core);
}

#[gpui_kit::test]
fn go_to_line_scrolls_the_rows_to_the_line(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, numbered());
    press(cx, window, "secondary-l");
    type_text(cx, window, "180");
    press(cx, window, "enter");
    assert_eq!(
        caret(cx, &view),
        cx.read(|cx| view.read(cx).document().byte_of_line(Line::new(179)).get())
    );
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(cell_id(0, RowIx::new(179))).is_some(), "row 179 shows");
        assert!(window.try_find(cell_id(0, RowIx::new(0))).is_none(), "row 0 is scrolled away");
    });
}

#[gpui_kit::test]
fn go_to_line_past_the_end_goes_to_the_last_line(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, two_line_region());
    press(cx, window, "secondary-l");
    type_text(cx, window, "999");
    press(cx, window, "enter");
    // Line 4 is d, after "a\nB\nC\n".
    assert_eq!(caret(cx, &view), 6);
}

#[gpui_kit::test]
fn go_to_line_refuses_what_is_not_a_line_number(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, two_line_region());
    set_caret(cx, &view, 2);
    press(cx, window, "secondary-l");
    type_text(cx, window, "0");
    press(cx, window, "enter");
    assert!(bar_open(cx, &view), "the field stays open");
    assert_eq!(caret(cx, &view), 2);
    let (_, core) = texts(cx, &view);
    assert_eq!(core, "a\nB\nC\nd\n", "nothing typed reaches the result");
}

#[gpui_kit::test]
fn escape_closes_go_to_line_and_keeps_the_caret(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, two_line_region());
    set_caret(cx, &view, 2);
    press(cx, window, "secondary-l");
    type_text(cx, window, "4");
    press(cx, window, "escape");
    assert!(!bar_open(cx, &view));
    assert_eq!(caret(cx, &view), 2);
    type_text(cx, window, "x");
    let (_, core) = texts(cx, &view);
    assert_eq!(core, "a\nxB\nC\nd\n", "the result has the focus again");
}

#[gpui_kit::test]
fn go_to_line_opens_empty_each_time(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, two_line_region());
    press(cx, window, "secondary-l");
    type_text(cx, window, "2");
    press(cx, window, "escape");
    press(cx, window, "secondary-l");
    type_text(cx, window, "3");
    press(cx, window, "enter");
    assert_eq!(caret(cx, &view), 4, "line 3, not line 23");
}

#[gpui_kit::test]
fn a_new_window_takes_keys_in_the_result(cx: &mut TestAppContext) {
    let (window, view) = open(cx, mixed());
    // The window opens on the first conflict; start at the top.
    set_caret(cx, &view, 0);
    press(cx, window, "secondary-down");
    let first = cx.read(|cx| view.read(cx).document().region_bytes(RegionIx::new(1)).start);
    assert_eq!(caret(cx, &view), first);
    press(cx, window, "secondary-f");
    assert!(search_open(cx, &view));
}

#[gpui_kit::test]
fn the_find_action_opens_the_search_from_the_go_to_line_field(cx: &mut TestAppContext) {
    // Menu items dispatch the action to whatever has the focus.
    let (window, view) = in_result(cx, mixed());
    press(cx, window, "secondary-l");
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(Find), cx);
    });
    assert!(search_open(cx, &view));
}

#[gpui_kit::test]
fn find_in_the_result_opens_its_search(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    press(cx, window, "secondary-f");
    assert!(search_open(cx, &view));
}

#[gpui_kit::test]
fn replace_all_goes_through_the_core(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, two_line_region());
    act(cx, window, |window, cx| {
        view.read(cx).result_editor().clone().update(cx, |state, cx| {
            state.set_search_query("C", false, cx);
            assert_eq!(state.replace_all_search_matches("Z", window, cx), 1);
        });
    });
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "a\nB\nZ\nd\n");
    assert_eq!(editor, core);
    press(cx, window, "secondary-z");
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "a\nB\nC\nd\n", "the core undoes the replacement");
    assert_eq!(editor, core);
}

#[gpui_kit::test]
fn toolbar_navigation_moves_the_caret_and_keeps_the_focus(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    let starts = cx.read(|cx| {
        let doc = view.read(cx).document();
        [1, 3].map(|r| doc.region_bytes(RegionIx::new(r)).start)
    });
    click(cx, window, Tool::NextDifference.id());
    assert_eq!(caret(cx, &view), starts[0]);
    click(cx, window, Tool::NextDifference.id());
    assert_eq!(caret(cx, &view), starts[1]);
    click(cx, window, Tool::PreviousDifference.id());
    assert_eq!(caret(cx, &view), starts[0]);
    click(cx, window, Tool::NextConflict.id());
    assert_eq!(caret(cx, &view), starts[0], "region 1 is the only conflict");
    // Keys still reach the window and the result.
    press(cx, window, "secondary-down");
    assert_eq!(caret(cx, &view), starts[1]);
    type_text(cx, window, "x");
    let (_, core) = texts(cx, &view);
    assert!(core.ends_with("c\nxD\ne\n"), "{core:?}");
}

#[gpui_kit::test]
fn toolbar_choices_act_on_the_current_region(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    let resolution =
        |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).document().resolutions()[1].clone());
    click(cx, window, Tool::NextConflict.id());
    click(cx, window, Tool::Right.id());
    assert_eq!(resolution(cx), Resolution::Pick(vec![Source::Side(1)]));
    click(cx, window, Tool::Left.id());
    assert_eq!(resolution(cx), Resolution::Pick(vec![Source::Side(1), Source::Side(0)]));
    from_more(cx, window, MORE_MENU_BASE);
    assert_eq!(
        resolution(cx),
        Resolution::Pick(vec![Source::Side(1), Source::Side(0), Source::Base(0)])
    );
    click(cx, window, Tool::Right.id());
    assert_eq!(resolution(cx), Resolution::Pick(vec![Source::Side(0), Source::Base(0)]));
    click(cx, window, Tool::Undo.id());
    assert_eq!(
        resolution(cx),
        Resolution::Pick(vec![Source::Side(1), Source::Side(0), Source::Base(0)])
    );
    click(cx, window, Tool::Redo.id());
    assert_eq!(resolution(cx), Resolution::Pick(vec![Source::Side(0), Source::Base(0)]));
}

/// Whether each of `tools` shows pressed; `None` for a button that is no toggle.
fn pressed_of<const N: usize>(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    tools: [Tool; N],
) -> [Option<bool>; N] {
    let mut state = None;
    act(cx, window, |window, _| {
        state = Some(tools.map(|tool| window.find(tool.id()).checked()));
    });
    state.expect("the window ran")
}

fn open_more_menu(cx: &mut TestAppContext, window: AnyWindowHandle) {
    click(cx, window, MORE_ID);
}

fn more_menu_open(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    let mut open = false;
    act(cx, window, |window, _| {
        open = window.try_find(POPUP_MENU_ID).is_some();
    });
    open
}

/// Run the More menu's row `row`.
fn from_more(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) {
    open_more_menu(cx, window);
    act(cx, window, |window, cx| {
        window.within(POPUP_MENU_ID).click(row, cx);
    });
}

/// Whether the More menu ticks its Base row, read with the menu open, then closed.
fn base_ticked(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<bool> {
    open_more_menu(cx, window);
    let mut ticked = None;
    act(cx, window, |window, _| {
        ticked = window.within(POPUP_MENU_ID).find(MORE_MENU_BASE).checked();
    });
    press(cx, window, "escape");
    ticked
}

/// Whether Left, Base and Right are on: the Left and Right buttons pressed, the More menu's
/// Base row ticked.
fn pressed(cx: &mut TestAppContext, window: AnyWindowHandle) -> [Option<bool>; 3] {
    let [left, right] = pressed_of(cx, window, [Tool::Left, Tool::Right]);
    [left, base_ticked(cx, window), right]
}

/// The view toggles: base pane, syntax colours and soft wrap.
const VIEW_TOOLS: [Tool; 3] = [Tool::BasePane, Tool::Syntax, Tool::SoftWrap];

#[gpui_kit::test]
fn the_view_toggles_show_pressed_while_their_view_is_on(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, mixed());
    // The defaults: base pane and soft wrap off, syntax colours on (spec §6.4).
    assert_eq!(pressed_of(cx, window, VIEW_TOOLS), [Some(false), Some(true), Some(false)]);
    click(cx, window, Tool::BasePane.id());
    assert_eq!(pressed_of(cx, window, VIEW_TOOLS), [Some(true), Some(true), Some(false)]);
    click(cx, window, Tool::Syntax.id());
    assert_eq!(pressed_of(cx, window, VIEW_TOOLS), [Some(true), Some(false), Some(false)]);
    click(cx, window, Tool::SoftWrap.id());
    assert_eq!(pressed_of(cx, window, VIEW_TOOLS), [Some(true), Some(false), Some(true)]);
    click(cx, window, Tool::BasePane.id());
    assert_eq!(pressed_of(cx, window, VIEW_TOOLS), [Some(false), Some(false), Some(true)]);
    // The key and the menu turn the same state.
    dispatch(cx, window, &merge_app::ToggleSoftWrap);
    assert_eq!(pressed_of(cx, window, VIEW_TOOLS), [Some(false), Some(false), Some(false)]);
}

#[gpui_kit::test]
fn only_the_toggles_report_a_pressed_state(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, mixed());
    let actions = [
        Tool::PreviousDifference,
        Tool::NextDifference,
        Tool::PreviousConflict,
        Tool::NextConflict,
        Tool::Undo,
        Tool::Redo,
        Tool::Find,
        Tool::GoToLine,
    ];
    assert_eq!(pressed_of(cx, window, actions), [None; 8]);
    let mut more = Some(false);
    act(cx, window, |window, _| more = window.find(MORE_ID).checked());
    assert_eq!(more, None, "More opens a menu");
}

#[gpui_kit::test]
fn the_pick_buttons_show_pressed_while_the_current_region_takes_their_source(
    cx: &mut TestAppContext,
) {
    const NONE: [Option<bool>; 3] = [Some(false); 3];
    let (window, _) = in_result(cx, mixed());
    // Region 0 is unchanged.
    assert_eq!(pressed(cx, window), NONE);
    click(cx, window, Tool::NextConflict.id());
    assert_eq!(pressed(cx, window), NONE, "an unresolved conflict picks nothing");
    click(cx, window, Tool::Right.id());
    assert_eq!(pressed(cx, window), [Some(false), Some(false), Some(true)]);
    click(cx, window, Tool::Left.id());
    assert_eq!(pressed(cx, window), [Some(true), Some(false), Some(true)]);
    from_more(cx, window, MORE_MENU_BASE);
    assert_eq!(pressed(cx, window), [Some(true); 3]);
    click(cx, window, Tool::Right.id());
    assert_eq!(pressed(cx, window), [Some(true), Some(true), Some(false)]);
    // Region 3 takes the left side automatically.
    click(cx, window, Tool::NextDifference.id());
    assert_eq!(pressed(cx, window), [Some(true), Some(false), Some(false)]);
}

#[gpui_kit::test]
fn the_more_menu_takes_both_orders_and_nothing(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    let resolution =
        |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).document().resolutions()[1].clone());
    click(cx, window, Tool::NextConflict.id());
    from_more(cx, window, MORE_MENU_LEFT_THEN_RIGHT);
    assert_eq!(resolution(cx), Resolution::Pick(vec![Source::Side(0), Source::Side(1)]));
    from_more(cx, window, MORE_MENU_RIGHT_THEN_LEFT);
    assert_eq!(resolution(cx), Resolution::Pick(vec![Source::Side(1), Source::Side(0)]));
    // None picks nothing on purpose: the conflict resolves to no text.
    from_more(cx, window, MORE_MENU_NONE);
    assert_eq!(resolution(cx), Resolution::Pick(Vec::new()));
    assert!(!more_menu_open(cx, window));
    // The menu gives the focus back to the result.
    type_text(cx, window, "z");
    assert_eq!(texts(cx, &view).1.matches('z').count(), 1);
}

#[gpui_kit::test]
fn toggling_off_both_sides_leaves_the_conflict_unresolved(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    let resolution =
        |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).document().resolutions()[1].clone());
    click(cx, window, Tool::NextConflict.id());
    click(cx, window, Tool::Left.id());
    click(cx, window, Tool::Left.id());
    assert_eq!(resolution(cx), Resolution::Unresolved, "left on, then off");
    click(cx, window, Tool::Right.id());
    click(cx, window, Tool::Left.id());
    click(cx, window, Tool::Right.id());
    click(cx, window, Tool::Left.id());
    assert_eq!(resolution(cx), Resolution::Unresolved, "both on, then both off");
    from_more(cx, window, MORE_MENU_LEFT_THEN_RIGHT);
    click(cx, window, Tool::Left.id());
    click(cx, window, Tool::Right.id());
    assert_eq!(resolution(cx), Resolution::Unresolved, "both from the menu, then both off");
}

#[gpui_kit::test]
fn the_more_menu_base_row_toggles_base(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    let resolution =
        |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).document().resolutions()[1].clone());
    click(cx, window, Tool::NextConflict.id());
    from_more(cx, window, MORE_MENU_BASE);
    assert_eq!(resolution(cx), Resolution::Pick(vec![Source::Base(0)]));
    // Toggling off a conflict's last source leaves it unresolved (spec §4.4).
    from_more(cx, window, MORE_MENU_BASE);
    assert_eq!(resolution(cx), Resolution::Unresolved);
}

/// A compact menu row, as `docs/STYLE.md` sets it.
const MENU_ROW_HEIGHT: gpui_kit::Pixels = gpui_kit::px(20.);

#[gpui_kit::test]
fn the_more_menu_has_compact_rows(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, mixed());
    open_more_menu(cx, window);
    let mut heights = Vec::new();
    act(cx, window, |window, _| {
        heights = MORE_ROWS
            .map(|item| window.within(POPUP_MENU_ID).find(item).bounds().size.height)
            .to_vec();
    });
    assert_eq!(heights, [MENU_ROW_HEIGHT; 4]);
}

/// The More menu's rows, in order.
const MORE_ROWS: [usize; 4] =
    [MORE_MENU_BASE, MORE_MENU_LEFT_THEN_RIGHT, MORE_MENU_RIGHT_THEN_LEFT, MORE_MENU_NONE];

/// The toolbar's label size, 0.525 rem of the theme's 16 px rem.
const TOOLBAR_TEXT_HEIGHT: gpui_kit::Pixels = gpui_kit::px(8.4);

/// The debug selector of the More menu's key hint for Left then right: ⌘4 on macOS, Ctrl-4
/// elsewhere.
const MORE_KEY_HINT: &str = if cfg!(target_os = "macos") { "kbd:cmd-4" } else { "kbd:ctrl-4" };

#[gpui_kit::test]
fn the_more_menu_text_matches_the_toolbar_labels(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, mixed());
    open_more_menu(cx, window);
    // A key hint has no padding and a line height of one text size.
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    let hint = visual.debug_bounds(MORE_KEY_HINT).expect("the menu shows the key");
    // Layout lands on whole device pixels.
    let scale = visual.update(|window, _| window.scale_factor());
    let expected = gpui_kit::px((f32::from(TOOLBAR_TEXT_HEIGHT) * scale).round() / scale);
    assert_eq!(hint.size.height, expected);
}

/// The second region's resolution: the conflict in `mixed`.
fn conflict_resolution(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Resolution {
    cx.read(|cx| view.read(cx).document().resolutions()[1].clone())
}

/// The conflict in `mixed`, current, with the focus in the result.
fn at_the_conflict(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let (window, view) = in_result(cx, mixed());
    click(cx, window, Tool::NextConflict.id());
    (window, view)
}

#[gpui_kit::test]
fn down_selects_each_row_in_turn_and_enter_takes_it(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    open_more_menu(cx, window);
    // Base, Left then right, Right then left.
    for _ in 0..3 {
        press(cx, window, "down");
    }
    press(cx, window, "enter");
    assert_eq!(
        conflict_resolution(cx, &view),
        Resolution::Pick(vec![Source::Side(1), Source::Side(0)])
    );
    assert!(!more_menu_open(cx, window));
    // Enter gives the focus back to the result.
    type_text(cx, window, "z");
    assert_eq!(texts(cx, &view).1.matches('z').count(), 1);
}

#[gpui_kit::test]
fn down_past_the_last_row_wraps_to_the_first(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    open_more_menu(cx, window);
    // Four rows, then Base again.
    for _ in 0..5 {
        press(cx, window, "down");
    }
    press(cx, window, "enter");
    assert_eq!(conflict_resolution(cx, &view), Resolution::Pick(vec![Source::Base(0)]));
}

#[gpui_kit::test]
fn up_from_no_selection_selects_the_last_row(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    open_more_menu(cx, window);
    press(cx, window, "up");
    press(cx, window, "enter");
    // The last row is None.
    assert_eq!(conflict_resolution(cx, &view), Resolution::Pick(Vec::new()));
}

#[gpui_kit::test]
fn enter_with_no_selection_takes_nothing_and_keeps_the_menu(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_more_menu(cx, window);
    press(cx, window, "enter");
    assert_eq!(conflict_resolution(cx, &view), before);
    assert!(more_menu_open(cx, window));
}

#[gpui_kit::test]
fn a_reopened_menu_starts_with_no_selection(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_more_menu(cx, window);
    press(cx, window, "down");
    press(cx, window, "escape");
    open_more_menu(cx, window);
    press(cx, window, "enter");
    assert_eq!(conflict_resolution(cx, &view), before);
}

#[gpui_kit::test]
fn escape_closes_the_menu_and_gives_the_focus_back(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_more_menu(cx, window);
    assert!(more_menu_open(cx, window));
    press(cx, window, "escape");
    assert!(!more_menu_open(cx, window));
    assert_eq!(conflict_resolution(cx, &view), before);
    type_text(cx, window, "z");
    assert_eq!(texts(cx, &view).1.matches('z').count(), 1);
}

#[gpui_kit::test]
fn a_click_outside_closes_the_menu(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_more_menu(cx, window);
    click(cx, window, CONFLICT_BADGE_ID);
    assert!(!more_menu_open(cx, window));
    assert_eq!(conflict_resolution(cx, &view), before);
}

#[gpui_kit::test]
fn the_trigger_closes_an_open_menu(cx: &mut TestAppContext) {
    let (window, _) = at_the_conflict(cx);
    open_more_menu(cx, window);
    open_more_menu(cx, window);
    assert!(!more_menu_open(cx, window));
}

/// Whether `tool` shows its text label.
fn label_shown(cx: &mut TestAppContext, window: AnyWindowHandle, tool: Tool) -> bool {
    shown(cx, window, tool.label_id())
}

/// Whether the element `id` is drawn.
fn shown(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> bool {
    let id = id.into();
    let mut shown = false;
    act(cx, window, |window, _| {
        shown = window.try_find(id.clone()).is_some();
    });
    shown
}

#[gpui_kit::test]
fn each_navigation_pair_shows_one_label(cx: &mut TestAppContext) {
    let (window, _) = open(cx, mixed());
    assert!(label_shown(cx, window, Tool::PreviousDifference));
    assert!(!label_shown(cx, window, Tool::NextDifference));
    assert!(label_shown(cx, window, Tool::PreviousConflict));
    assert!(!label_shown(cx, window, Tool::NextConflict));
}

/// The choice tools keep their labels; the rest show icons alone.
const LABELLED: [(Tool, bool); 9] = [
    (Tool::Left, true),
    (Tool::Right, true),
    (Tool::Undo, false),
    (Tool::Redo, false),
    (Tool::BasePane, false),
    (Tool::Find, false),
    (Tool::GoToLine, false),
    (Tool::Syntax, false),
    (Tool::SoftWrap, false),
];

#[gpui_kit::test]
fn only_the_choice_tools_show_labels(cx: &mut TestAppContext) {
    let (window, _) = open(cx, mixed());
    assert!(shown(cx, window, MORE_LABEL_ID));
    for (tool, labelled) in LABELLED {
        assert_eq!(label_shown(cx, window, tool), labelled, "{tool:?}");
    }
}

/// Twelve conflicts, so the badge reads its longest: `Conflict 10 of 12 · 12 unresolved`.
fn twelve_conflicts() -> MergeDocument {
    const CONFLICTS: usize = 12;
    let (mut base, mut left, mut right) = (String::new(), String::new(), String::new());
    for n in 0..CONFLICTS {
        writeln!(base, "b{n}\nkeep{n}").expect("a String takes any write");
        writeln!(left, "l{n}\nkeep{n}").expect("a String takes any write");
        writeln!(right, "r{n}\nkeep{n}").expect("a String takes any write");
    }
    doc(&base, &left, &right)
}

/// The toolbar's inner padding at each end: `px_2`, 0.5 rem.
const BAR_PADDING: gpui_kit::Pixels = gpui_kit::px(8.);

/// Whether every tool and the badge lie inside the toolbar's padding, in order, without
/// overlap.
fn tools_fit(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    let mut fits = false;
    act(cx, window, |window, _| {
        let bar = window.find(TOOLBAR_ID).bounds();
        let last = window.find(Tool::SoftWrap.id()).bounds();
        let badge = window.find(CONFLICT_BADGE_ID).bounds();
        fits = last.right() <= badge.left() && badge.right() <= bar.right() - BAR_PADDING;
    });
    fits
}

fn resize(cx: &mut TestAppContext, window: AnyWindowHandle, width: gpui_kit::Pixels) {
    cx.simulate_window_resize(window, gpui_kit::size(width, WINDOW_TEST_HEIGHT));
    act(cx, window, TestWindowExt::render_frame);
}

const WINDOW_TEST_HEIGHT: gpui_kit::Pixels = gpui_kit::px(600.);
/// Next Conflict presses from the first conflict, which holds the caret, to the tenth.
const TO_CONFLICT_TEN: usize = 9;

#[gpui_kit::test]
fn the_labelled_toolbar_fits_from_its_minimum_width(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, twelve_conflicts());
    for _ in 0..TO_CONFLICT_TEN {
        click(cx, window, Tool::NextConflict.id());
    }
    let mut badge = None;
    act(cx, window, |window, _| {
        badge = window.find(CONFLICT_BADGE_ID).label().map(str::to_owned);
    });
    assert_eq!(badge.as_deref(), Some("Conflict 10 of 12 · 12 unresolved"));
    resize(cx, window, LABELS_MIN_WIDTH);
    assert!(label_shown(cx, window, Tool::Left));
    assert!(tools_fit(cx, window));
}

/// Twelve conflicts of three sides, in jj's order s0 b0 s1 b1 s2. Side 1 holds base 1's line,
/// so jj drops that pair and each conflict keeps two of its three sides.
fn twelve_three_side_conflicts() -> MergeDocument {
    const CONFLICTS: usize = 12;
    let mut terms: [String; 5] = Default::default();
    for n in 0..CONFLICTS {
        for (term, line) in terms.iter_mut().zip(["x", "b", "c", "c", "y"]) {
            writeln!(term, "{line}{n}\nkeep{n}").expect("a String takes any write");
        }
    }
    let [s0, b0, s1, b1, s2] = terms.map(|term| common::text(&term));
    let inputs = Inputs::new(vec![s0, s1, s2], vec![b0, b1]).expect("three sides, two bases");
    MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels")
}

fn badge_label(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<String> {
    let mut badge = None;
    act(cx, window, |window, _| {
        badge = window.find(CONFLICT_BADGE_ID).label().map(str::to_owned);
    });
    badge
}

#[gpui_kit::test]
fn the_badge_shows_the_sides_jj_keeps_in_a_conflict_of_more_than_two(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, twelve_three_side_conflicts());
    assert_eq!(
        badge_label(cx, window).as_deref(),
        Some("Conflict 1 of 12 · 2 of 3 sides · 12 unresolved")
    );
}

#[gpui_kit::test]
fn the_labelled_toolbar_fits_the_badge_with_its_sides(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, twelve_three_side_conflicts());
    for _ in 0..TO_CONFLICT_TEN {
        click(cx, window, Tool::NextConflict.id());
    }
    assert_eq!(
        badge_label(cx, window).as_deref(),
        Some("Conflict 10 of 12 · 2 of 3 sides · 12 unresolved")
    );
    let mut threshold = gpui_kit::Pixels::ZERO;
    act(cx, window, |window, _| {
        threshold = labels_min_width(DEFAULT_TOOLBAR_TEXT, Badge::Sides, window);
    });
    assert!(threshold > LABELS_MIN_WIDTH, "{threshold:?}");
    resize(cx, window, threshold - gpui_kit::px(1.));
    assert!(!label_shown(cx, window, Tool::Left));
    resize(cx, window, threshold);
    assert!(label_shown(cx, window, Tool::Left));
    assert!(tools_fit(cx, window));
}

#[gpui_kit::test]
fn a_narrower_window_drops_every_label_and_the_tools_fit(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, twelve_conflicts());
    for _ in 0..TO_CONFLICT_TEN {
        click(cx, window, Tool::NextConflict.id());
    }
    resize(cx, window, LABELS_MIN_WIDTH - gpui_kit::px(1.));
    for (tool, _) in LABELLED {
        assert!(!label_shown(cx, window, tool), "{tool:?}");
    }
    assert!(!shown(cx, window, MORE_LABEL_ID));
    assert!(!label_shown(cx, window, Tool::PreviousDifference));
    assert!(!label_shown(cx, window, Tool::PreviousConflict));
    resize(cx, window, COMPACT_MIN_WIDTH);
    assert!(tools_fit(cx, window));
    // Wider again, the labels return.
    resize(cx, window, LABELS_MIN_WIDTH);
    assert!(label_shown(cx, window, Tool::Left));
}

/// The largest toolbar text the `toolbar-font-size` setting takes (spec §6.4).
const LARGE_TOOLBAR_TEXT: gpui_kit::Pixels = gpui_kit::px(12.);
/// The default toolbar text (spec §6.4).
const DEFAULT_TOOLBAR_TEXT: gpui_kit::Pixels = gpui_kit::px(8.4);
/// The conflict badge's padding, both sides (`docs/STYLE.md`).
const BADGE_PADDING: gpui_kit::Pixels = gpui_kit::px(16.);
/// Rounding to the pixel grid moves a measured width by at most this.
const ROUNDING: gpui_kit::Pixels = gpui_kit::px(1.);

/// A merge window of `doc` whose toolbar text is `size`, with the focus in the result.
fn with_toolbar_text(
    cx: &mut TestAppContext,
    doc: MergeDocument,
    size: gpui_kit::Pixels,
) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let settings = WindowSettings {
        typography: Typography { toolbar_size: size, ..Typography::default() },
        ..WindowSettings::default()
    };
    let (window, view, _) = common::open_with(cx, doc, &settings);
    click(cx, window, RESULT_ID);
    set_caret(cx, &view, 0);
    (window, view)
}

fn width_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> gpui_kit::Pixels {
    let id = id.into();
    let mut width = gpui_kit::Pixels::ZERO;
    act(cx, window, |window, _| {
        width = window.find(id).bounds().size.width;
    });
    width
}

#[gpui_kit::test]
fn the_toolbar_text_setting_scales_the_labels_and_the_badge(cx: &mut TestAppContext) {
    let (small, _) = with_toolbar_text(cx, mixed(), DEFAULT_TOOLBAR_TEXT);
    let (large, _) = with_toolbar_text(cx, mixed(), LARGE_TOOLBAR_TEXT);
    let scale = LARGE_TOOLBAR_TEXT / DEFAULT_TOOLBAR_TEXT;
    // The label is its text alone; the badge has `px_2` padding, 8 px a side, which stays.
    for (id, padding) in
        [(Tool::Left.label_id(), gpui_kit::Pixels::ZERO), (CONFLICT_BADGE_ID.into(), BADGE_PADDING)]
    {
        let (from, to) = (width_of(cx, small, id.clone()), width_of(cx, large, id.clone()));
        let expected = (from - padding) * scale + padding;
        assert!((to - expected).abs() <= ROUNDING, "{id:?}: {from:?} to {to:?}");
    }
}

#[gpui_kit::test]
fn the_toolbar_text_setting_sizes_the_more_menu(cx: &mut TestAppContext) {
    let (window, _) = with_toolbar_text(cx, mixed(), LARGE_TOOLBAR_TEXT);
    open_more_menu(cx, window);
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    let hint = visual.debug_bounds(MORE_KEY_HINT).expect("the menu shows the key");
    assert_eq!(hint.size.height, LARGE_TOOLBAR_TEXT);
}

/// The narrowest window that shows labels with toolbar text of `size`.
fn labels_min_width_at(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    size: gpui_kit::Pixels,
) -> gpui_kit::Pixels {
    let mut width = gpui_kit::Pixels::ZERO;
    act(cx, window, |window, _| {
        width = labels_min_width(size, Badge::Counts, window);
    });
    width
}

#[gpui_kit::test]
fn the_default_toolbar_text_keeps_the_label_threshold(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, mixed());
    assert_eq!(labels_min_width_at(cx, window, DEFAULT_TOOLBAR_TEXT), LABELS_MIN_WIDTH);
}

#[gpui_kit::test]
fn larger_toolbar_text_needs_a_wider_window_for_its_labels(cx: &mut TestAppContext) {
    let (window, _) = with_toolbar_text(cx, twelve_conflicts(), LARGE_TOOLBAR_TEXT);
    for _ in 0..TO_CONFLICT_TEN {
        click(cx, window, Tool::NextConflict.id());
    }
    let threshold = labels_min_width_at(cx, window, LARGE_TOOLBAR_TEXT);
    assert!(threshold > LABELS_MIN_WIDTH, "{threshold:?}");
    resize(cx, window, LABELS_MIN_WIDTH);
    assert!(!label_shown(cx, window, Tool::Left));
    resize(cx, window, threshold - gpui_kit::px(1.));
    assert!(!label_shown(cx, window, Tool::Left));
    resize(cx, window, threshold);
    assert!(label_shown(cx, window, Tool::Left));
    assert!(tools_fit(cx, window));
}

#[gpui_kit::test]
fn toolbar_opens_the_base_pane_find_and_go_to_line(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    click(cx, window, Tool::BasePane.id());
    assert_eq!(cx.read(|cx| view.read(cx).panes()), PaneSet::LeftBaseRight);
    click(cx, window, Tool::Find.id());
    assert!(search_open(cx, &view));
    click(cx, window, Tool::GoToLine.id());
    assert!(bar_open(cx, &view));
    act(cx, window, |window, _| {
        assert!(window.try_find(GO_TO_LINE_ID).is_some());
    });
}

/// A menu entry: an item's name and action, or anything else.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Item(String, &'static str),
    Other,
}

/// Each menu's name and entries.
fn menus(cx: &mut TestAppContext) -> Vec<(String, Vec<Entry>)> {
    let menus = cx.update(|cx| cx.get_menus()).expect("the test platform keeps menus");
    menus
        .into_iter()
        .map(|menu| {
            let items = menu
                .items
                .into_iter()
                .map(|item| match item {
                    OwnedMenuItem::Action { name, action, .. } => Entry::Item(name, action.name()),
                    _ => Entry::Other,
                })
                .collect();
            (menu.name.to_string(), items)
        })
        .collect()
}

fn item(name: &str, action: &'static str) -> Entry {
    Entry::Item(name.to_owned(), action)
}

#[gpui_kit::test]
fn the_menus_hold_every_window_action(cx: &mut TestAppContext) {
    let _ = open(cx, mixed());
    assert_eq!(
        menus(cx),
        [
            (
                "triad".to_owned(),
                vec![
                    item("About triad", "triad::About"),
                    Entry::Other,
                    item("Settings…", "triad::OpenSettings"),
                    Entry::Other,
                    item("Save & Exit", "triad::SaveAndExit"),
                    item("Save Copy As…", "triad::SaveCopyAs"),
                    item("Save", "triad::Save"),
                    item("Cancel", "triad::Cancel"),
                ]
            ),
            (
                "Edit".to_owned(),
                vec![
                    item("Undo", "triad::Undo"),
                    item("Redo", "triad::Redo"),
                    Entry::Other,
                    item("Find…", "triad::Find"),
                    item("Go to Line…", "triad::GoToLine"),
                ]
            ),
            (
                "Merge".to_owned(),
                vec![
                    item("Toggle Left", "triad::ToggleLeft"),
                    item("Toggle Base", "triad::ToggleBase"),
                    item("Toggle Right", "triad::ToggleRight"),
                    item("Clear Choice", "triad::ClearChoice"),
                    Entry::Other,
                    item("Take Base", "triad::TakeBase"),
                    item("Take Left", "triad::TakeLeft"),
                    item("Take Right", "triad::TakeRight"),
                    item("Take Left then Right", "triad::TakeLeftThenRight"),
                    item("Take Right then Left", "triad::TakeRightThenLeft"),
                    item("Take Sorted Union", "triad::TakeSortedUnion"),
                    item("Carry Edit to Moved Block", "triad::CarryMovedEdit"),
                    Entry::Other,
                    item("Take Left for All Unresolved", "triad::TakeLeftAllUnresolved"),
                    item("Take Right for All Unresolved", "triad::TakeRightAllUnresolved"),
                    item("Take Whole Left File", "triad::TakeWholeLeft"),
                    item("Take Whole Right File", "triad::TakeWholeRight"),
                ]
            ),
            (
                "Go".to_owned(),
                vec![
                    item("Next Difference", "triad::NextDifference"),
                    item("Previous Difference", "triad::PreviousDifference"),
                    item("Next Conflict", "triad::NextConflict"),
                    item("Previous Conflict", "triad::PreviousConflict"),
                ]
            ),
            (
                "View".to_owned(),
                vec![
                    // Blame shows only in a window with origins (`origins.rs`).
                    item("Base Pane", "triad::ToggleBasePane"),
                    item("Syntax Highlighting", "triad::ToggleSyntax"),
                    item("Soft Wrap", "triad::ToggleSoftWrap"),
                    item("Collapse Unchanged", "triad::ToggleCollapseUnchanged"),
                    item("Sync Result Scrolling", "triad::ToggleScrollSync"),
                ]
            ),
        ]
    );
}

/// The View menu's items and whether each shows a tick.
fn view_ticks(cx: &mut TestAppContext) -> Vec<(String, bool)> {
    let menus = cx.update(|cx| cx.get_menus()).expect("the test platform keeps menus");
    menus
        .into_iter()
        .find(|menu| menu.name.as_ref() == "View")
        .expect("a View menu")
        .items
        .into_iter()
        .filter_map(|item| match item {
            OwnedMenuItem::Action { name, checked, .. } => Some((name, checked)),
            _ => None,
        })
        .collect()
}

/// The View menu's items in order, with `ticked`: base pane, syntax, soft wrap, collapse
/// unchanged and scroll sync. A merge without origins has no Blame item.
fn ticks(ticked: [bool; 5]) -> Vec<(String, bool)> {
    ["Base Pane", "Syntax Highlighting", "Soft Wrap", "Collapse Unchanged", "Sync Result Scrolling"]
        .into_iter()
        .map(str::to_owned)
        .zip(ticked)
        .collect()
}

/// Make `window` the active window and let it draw.
fn activate(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, _| window.activate_window());
    cx.run_until_parked();
    act(cx, window, TestWindowExt::render_frame);
}

fn dispatch(cx: &mut TestAppContext, window: AnyWindowHandle, action: &dyn gpui_kit::Action) {
    act(cx, window, |window, cx| {
        window.dispatch_action(action.boxed_clone(), cx);
    });
    act(cx, window, TestWindowExt::render_frame);
}

#[gpui_kit::test]
fn the_view_menu_ticks_what_the_merge_window_shows(cx: &mut TestAppContext) {
    let (window, _) = open(cx, mixed());
    activate(cx, window);
    // The defaults: syntax colours and scroll sync on (spec §6.4).
    assert_eq!(view_ticks(cx), ticks([false, true, false, false, true]));
    dispatch(cx, window, &merge_app::ToggleBasePane);
    assert_eq!(view_ticks(cx), ticks([true, true, false, false, true]));
    // A merge without origins has no blame to show: the key does nothing.
    dispatch(cx, window, &merge_app::ToggleOriginColumn);
    assert_eq!(view_ticks(cx), ticks([true, true, false, false, true]));
    dispatch(cx, window, &merge_app::ToggleSyntax);
    assert_eq!(view_ticks(cx), ticks([true, false, false, false, true]));
    dispatch(cx, window, &merge_app::ToggleSoftWrap);
    assert_eq!(view_ticks(cx), ticks([true, false, true, false, true]));
    dispatch(cx, window, &merge_app::ToggleCollapseUnchanged);
    assert_eq!(view_ticks(cx), ticks([true, false, true, true, true]));
    dispatch(cx, window, &merge_app::ToggleCollapseUnchanged);
    assert_eq!(view_ticks(cx), ticks([true, false, true, false, true]));
    dispatch(cx, window, &merge_app::ToggleScrollSync);
    assert_eq!(view_ticks(cx), ticks([true, false, true, false, false]));
    dispatch(cx, window, &merge_app::ToggleBasePane);
    assert_eq!(view_ticks(cx), ticks([false, false, true, false, false]));
}

#[gpui_kit::test]
fn the_view_menu_follows_the_active_window(cx: &mut TestAppContext) {
    let (first, _) = open(cx, mixed());
    activate(cx, first);
    assert_eq!(view_ticks(cx), ticks([false, true, false, false, true]));
    let settings = WindowSettings { base_pane: true, soft_wrap: true, ..WindowSettings::default() };
    let (second, _) =
        cx.update(|cx| merge_app::open(mixed(), &settings, cx).expect("window opens"));
    activate(cx, second);
    assert_eq!(view_ticks(cx), ticks([true, true, true, false, true]));
    activate(cx, first);
    assert_eq!(view_ticks(cx), ticks([false, true, false, false, true]));
    // A window that is not active leaves the menu to the one that is.
    dispatch(cx, second, &merge_app::ToggleSyntax);
    assert_eq!(view_ticks(cx), ticks([false, true, false, false, true]));
    activate(cx, second);
    assert_eq!(view_ticks(cx), ticks([true, false, true, false, true]));
}
