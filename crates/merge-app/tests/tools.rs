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
    BOTH_ID, BOTH_MENU_LEFT_THEN_RIGHT, BOTH_MENU_RIGHT_THEN_LEFT, COMPACT_MIN_WIDTH,
    LABELS_MIN_WIDTH, MENU_TRIGGER_ID, POPUP_MENU_ID, Tool, labels_min_width,
};
use merge_app::window::{CONFLICT_BADGE_ID, GO_TO_LINE_ID, RESULT_ID, TOOLBAR_ID, cell_id};
use merge_app::{Find, MergeWindow, Typography, WindowSettings};
use merge_core::document::{MergeDocument, Resolution};
use merge_core::merge3::Source;
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
    click(cx, window, Tool::Base.id());
    assert_eq!(
        resolution(cx),
        Resolution::Pick(vec![Source::Side(1), Source::Side(0), Source::Base(0)])
    );
    click(cx, window, Tool::Clear.id());
    assert_eq!(resolution(cx), Resolution::Pick(Vec::new()));
    click(cx, window, Tool::Undo.id());
    assert_eq!(
        resolution(cx),
        Resolution::Pick(vec![Source::Side(1), Source::Side(0), Source::Base(0)])
    );
    click(cx, window, Tool::Redo.id());
    assert_eq!(resolution(cx), Resolution::Pick(Vec::new()));
}

#[gpui_kit::test]
fn toolbar_takes_both_sides_in_either_order(cx: &mut TestAppContext) {
    let (window, view) = in_result(cx, mixed());
    let resolution =
        |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).document().resolutions()[1].clone());
    let left_right = Resolution::Pick(vec![Source::Side(0), Source::Side(1)]);
    let right_left = Resolution::Pick(vec![Source::Side(1), Source::Side(0)]);
    let from_menu = |cx: &mut TestAppContext, item: usize| {
        act(cx, window, |window, cx| {
            window.within(BOTH_ID).click(MENU_TRIGGER_ID, cx);
        });
        act(cx, window, |window, cx| {
            window.within(POPUP_MENU_ID).click(item, cx);
        });
    };
    click(cx, window, Tool::NextConflict.id());
    // The button takes left then right; the menu offers both orders.
    click(cx, window, Tool::LeftThenRight.id());
    assert_eq!(resolution(cx), left_right);
    from_menu(cx, BOTH_MENU_RIGHT_THEN_LEFT);
    assert_eq!(resolution(cx), right_left);
    from_menu(cx, BOTH_MENU_LEFT_THEN_RIGHT);
    assert_eq!(resolution(cx), left_right);
    // The menu gives the focus back to the result.
    type_text(cx, window, "z");
    assert_eq!(texts(cx, &view).1.matches('z').count(), 1);
}

/// A compact menu row, as `docs/STYLE.md` sets it.
const MENU_ROW_HEIGHT: gpui_kit::Pixels = gpui_kit::px(20.);

#[gpui_kit::test]
fn the_both_menu_has_compact_rows(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, mixed());
    act(cx, window, |window, cx| {
        window.within(BOTH_ID).click(MENU_TRIGGER_ID, cx);
    });
    let mut heights = Vec::new();
    act(cx, window, |window, _| {
        heights = [BOTH_MENU_LEFT_THEN_RIGHT, BOTH_MENU_RIGHT_THEN_LEFT]
            .map(|item| window.within(POPUP_MENU_ID).find(item).bounds().size.height)
            .to_vec();
    });
    assert_eq!(heights, [MENU_ROW_HEIGHT, MENU_ROW_HEIGHT]);
}

/// The toolbar's label size, 0.525 rem of the theme's 16 px rem.
const TOOLBAR_TEXT_HEIGHT: gpui_kit::Pixels = gpui_kit::px(8.4);

/// The debug selector of the Both menu's key hint: ⌘4 on macOS, Ctrl-4 elsewhere.
const BOTH_KEY_HINT: &str = if cfg!(target_os = "macos") { "kbd:cmd-4" } else { "kbd:ctrl-4" };

#[gpui_kit::test]
fn the_both_menu_text_matches_the_toolbar_labels(cx: &mut TestAppContext) {
    let (window, _) = in_result(cx, mixed());
    act(cx, window, |window, cx| {
        window.within(BOTH_ID).click(MENU_TRIGGER_ID, cx);
    });
    // A key hint has no padding and a line height of one text size.
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    let hint = visual.debug_bounds(BOTH_KEY_HINT).expect("the menu shows the key");
    // Layout lands on whole device pixels.
    let scale = visual.update(|window, _| window.scale_factor());
    let expected = gpui_kit::px((f32::from(TOOLBAR_TEXT_HEIGHT) * scale).round() / scale);
    assert_eq!(hint.size.height, expected);
}

fn open_both_menu(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| {
        window.within(BOTH_ID).click(MENU_TRIGGER_ID, cx);
    });
}

fn both_menu_open(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    let mut open = false;
    act(cx, window, |window, _| {
        open = window.try_find(POPUP_MENU_ID).is_some();
    });
    open
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
    open_both_menu(cx, window);
    press(cx, window, "down");
    press(cx, window, "down");
    press(cx, window, "enter");
    assert_eq!(
        conflict_resolution(cx, &view),
        Resolution::Pick(vec![Source::Side(1), Source::Side(0)])
    );
    assert!(!both_menu_open(cx, window));
    // Enter gives the focus back to the result.
    type_text(cx, window, "z");
    assert_eq!(texts(cx, &view).1.matches('z').count(), 1);
}

#[gpui_kit::test]
fn down_past_the_last_row_wraps_to_the_first(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    open_both_menu(cx, window);
    for _ in 0..3 {
        press(cx, window, "down");
    }
    press(cx, window, "enter");
    assert_eq!(
        conflict_resolution(cx, &view),
        Resolution::Pick(vec![Source::Side(0), Source::Side(1)])
    );
}

#[gpui_kit::test]
fn up_from_no_selection_selects_the_last_row(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    open_both_menu(cx, window);
    press(cx, window, "up");
    press(cx, window, "enter");
    assert_eq!(
        conflict_resolution(cx, &view),
        Resolution::Pick(vec![Source::Side(1), Source::Side(0)])
    );
}

#[gpui_kit::test]
fn enter_with_no_selection_takes_nothing_and_keeps_the_menu(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_both_menu(cx, window);
    press(cx, window, "enter");
    assert_eq!(conflict_resolution(cx, &view), before);
    assert!(both_menu_open(cx, window));
}

#[gpui_kit::test]
fn a_reopened_menu_starts_with_no_selection(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_both_menu(cx, window);
    press(cx, window, "down");
    press(cx, window, "escape");
    open_both_menu(cx, window);
    press(cx, window, "enter");
    assert_eq!(conflict_resolution(cx, &view), before);
}

#[gpui_kit::test]
fn escape_closes_the_menu_and_gives_the_focus_back(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_both_menu(cx, window);
    assert!(both_menu_open(cx, window));
    press(cx, window, "escape");
    assert!(!both_menu_open(cx, window));
    assert_eq!(conflict_resolution(cx, &view), before);
    type_text(cx, window, "z");
    assert_eq!(texts(cx, &view).1.matches('z').count(), 1);
}

#[gpui_kit::test]
fn a_click_outside_closes_the_menu(cx: &mut TestAppContext) {
    let (window, view) = at_the_conflict(cx);
    let before = conflict_resolution(cx, &view);
    open_both_menu(cx, window);
    click(cx, window, CONFLICT_BADGE_ID);
    assert!(!both_menu_open(cx, window));
    assert_eq!(conflict_resolution(cx, &view), before);
}

#[gpui_kit::test]
fn the_trigger_closes_an_open_menu(cx: &mut TestAppContext) {
    let (window, _) = at_the_conflict(cx);
    open_both_menu(cx, window);
    open_both_menu(cx, window);
    assert!(!both_menu_open(cx, window));
}

/// Whether `tool` shows its text label.
fn label_shown(cx: &mut TestAppContext, window: AnyWindowHandle, tool: Tool) -> bool {
    let mut shown = false;
    act(cx, window, |window, _| {
        shown = window.try_find(tool.label_id()).is_some();
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
const LABELLED: [(Tool, bool); 12] = [
    (Tool::Left, true),
    (Tool::Base, true),
    (Tool::Right, true),
    (Tool::Clear, true),
    (Tool::LeftThenRight, true),
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
fn the_toolbar_text_setting_sizes_the_both_menu(cx: &mut TestAppContext) {
    let (window, _) = with_toolbar_text(cx, mixed(), LARGE_TOOLBAR_TEXT);
    open_both_menu(cx, window);
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    let hint = visual.debug_bounds(BOTH_KEY_HINT).expect("the menu shows the key");
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
        width = labels_min_width(size, window);
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

/// The View menu's items in order, with `ticked`: base pane, syntax, soft wrap and scroll
/// sync. A merge without origins has no Blame item.
fn ticks(ticked: [bool; 4]) -> Vec<(String, bool)> {
    ["Base Pane", "Syntax Highlighting", "Soft Wrap", "Sync Result Scrolling"]
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
    assert_eq!(view_ticks(cx), ticks([false, true, false, true]));
    dispatch(cx, window, &merge_app::ToggleBasePane);
    assert_eq!(view_ticks(cx), ticks([true, true, false, true]));
    // A merge without origins has no blame to show: the key does nothing.
    dispatch(cx, window, &merge_app::ToggleOriginColumn);
    assert_eq!(view_ticks(cx), ticks([true, true, false, true]));
    dispatch(cx, window, &merge_app::ToggleSyntax);
    assert_eq!(view_ticks(cx), ticks([true, false, false, true]));
    dispatch(cx, window, &merge_app::ToggleSoftWrap);
    assert_eq!(view_ticks(cx), ticks([true, false, true, true]));
    dispatch(cx, window, &merge_app::ToggleScrollSync);
    assert_eq!(view_ticks(cx), ticks([true, false, true, false]));
    dispatch(cx, window, &merge_app::ToggleBasePane);
    assert_eq!(view_ticks(cx), ticks([false, false, true, false]));
}

#[gpui_kit::test]
fn the_view_menu_follows_the_active_window(cx: &mut TestAppContext) {
    let (first, _) = open(cx, mixed());
    activate(cx, first);
    assert_eq!(view_ticks(cx), ticks([false, true, false, true]));
    let settings = WindowSettings { base_pane: true, soft_wrap: true, ..WindowSettings::default() };
    let (second, _) =
        cx.update(|cx| merge_app::open(mixed(), &settings, cx).expect("window opens"));
    activate(cx, second);
    assert_eq!(view_ticks(cx), ticks([true, true, true, true]));
    activate(cx, first);
    assert_eq!(view_ticks(cx), ticks([false, true, false, true]));
    // A window that is not active leaves the menu to the one that is.
    dispatch(cx, second, &merge_app::ToggleSyntax);
    assert_eq!(view_ticks(cx), ticks([false, true, false, true]));
    activate(cx, second);
    assert_eq!(view_ticks(cx), ticks([true, false, true, true]));
}
