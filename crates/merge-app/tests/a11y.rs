//! Headless tests of the merge window's accessible roles and labels (spec §6.5; DESIGN.md §A6.6).
//! The test platform has no screen reader, so they read the roles and labels gpui-kit
//! records for each element.

use crate::common;
use merge_view::rows::RowIx;

use common::{act, doc, open, set_caret};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, ElementId, Entity, Role, TestAppContext};
use merge_app::MergeWindow;
use merge_app::toolbar::{
    BOTH_ID, BOTH_MENU_LEFT_THEN_RIGHT, BOTH_MENU_RIGHT_THEN_LEFT, MENU_TRIGGER_ID, POPUP_MENU_ID,
    Tool,
};
use merge_app::window::{
    CONFLICT_BADGE_ID, GO_TO_LINE_FIELD_ID, GO_TO_LINE_ID, INPUT_LINES_ID, RESULT_ID, STATUS_ID,
    TOOLBAR_ID, cell_id, input_match_count_id, input_match_next_id, input_match_previous_id,
    pick_id,
};

/// Rows: 0 a, 1 the conflict (left L, right deleted b), 2 c. Three regions.
fn deleted_on_the_right(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    open(cx, doc("a\nb\nc\n", "a\nL\nc\n", "a\nc\n"))
}

/// The role and label of the element `id`.
fn named(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> (Option<Role>, Option<String>) {
    let id = id.into();
    let mut found = None;
    act(cx, window, |window, _| {
        let element = window.find(id);
        found = Some((element.role(), element.label().map(str::to_owned)));
    });
    found.expect("the window ran")
}

#[gpui_kit::test]
fn every_tool_is_a_named_button_in_a_toolbar(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    assert_eq!(
        named(cx, window, TOOLBAR_ID),
        (Some(Role::Toolbar), Some("Merge tools".to_owned()))
    );
    let tools = [
        (Tool::PreviousDifference, "Previous difference"),
        (Tool::NextDifference, "Next difference"),
        (Tool::PreviousConflict, "Previous conflict"),
        (Tool::NextConflict, "Next conflict"),
        (Tool::Left, "Toggle left in the current region"),
        (Tool::Base, "Toggle base in the current region"),
        (Tool::Right, "Toggle right in the current region"),
        (Tool::Clear, "Clear the current region's choice"),
        (Tool::LeftThenRight, "Take left then right in the current region"),
        (Tool::Undo, "Undo"),
        (Tool::Redo, "Redo"),
        (Tool::BasePane, "Show or hide the base pane"),
        (Tool::Find, "Find"),
        (Tool::GoToLine, "Go to a line in the result"),
        (Tool::Syntax, "Turn syntax colours on or off"),
        (Tool::SoftWrap, "Turn soft wrap on or off"),
    ];
    for (tool, name) in tools {
        assert_eq!(
            named(cx, window, tool.id()),
            (Some(Role::Button), Some(name.to_owned())),
            "{tool:?}"
        );
    }
    let mut trigger = None;
    act(cx, window, |window, _| {
        let element = window.within(BOTH_ID).find(MENU_TRIGGER_ID);
        trigger = Some((element.role(), element.label().map(str::to_owned)));
    });
    assert_eq!(
        trigger,
        Some((Some(Role::Button), Some("Take both sides in either order".to_owned())))
    );
}

#[gpui_kit::test]
fn the_both_menu_names_each_order(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    act(cx, window, |window, cx| {
        window.within(BOTH_ID).click(MENU_TRIGGER_ID, cx);
    });
    let mut labels = Vec::new();
    act(cx, window, |window, _| {
        labels = [BOTH_MENU_LEFT_THEN_RIGHT, BOTH_MENU_RIGHT_THEN_LEFT]
            .map(|item| window.within(POPUP_MENU_ID).find(item).label().map(str::to_owned))
            .to_vec();
    });
    assert_eq!(labels, [Some("Left then right".to_owned()), Some("Right then left".to_owned())]);
}

#[gpui_kit::test]
fn the_both_menu_is_a_menu_of_menu_items(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    act(cx, window, |window, cx| {
        window.within(BOTH_ID).click(MENU_TRIGGER_ID, cx);
    });
    let mut roles = None;
    act(cx, window, |window, _| {
        let menu = window.find(POPUP_MENU_ID);
        let items = [BOTH_MENU_LEFT_THEN_RIGHT, BOTH_MENU_RIGHT_THEN_LEFT]
            .map(|item| window.within(POPUP_MENU_ID).find(item).role());
        roles = Some((menu.role(), menu.label().map(str::to_owned), items));
    });
    assert_eq!(
        roles,
        Some((
            Some(Role::Menu),
            Some("Take both sides in either order".to_owned()),
            [Some(Role::MenuItem), Some(Role::MenuItem)]
        ))
    );
}

#[gpui_kit::test]
fn the_conflict_badge_counts_unresolved_conflicts(cx: &mut TestAppContext) {
    let (window, view) = deleted_on_the_right(cx);
    // The window opens on the conflict; start outside it.
    set_caret(cx, &view, 0);
    let badge = |cx: &mut TestAppContext| named(cx, window, CONFLICT_BADGE_ID);
    assert_eq!(badge(cx), (Some(Role::Status), Some("1 of 1 unresolved".to_owned())));
    // A pick makes its conflict current.
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!(badge(cx), (Some(Role::Status), Some("Conflict 1 of 1 · all resolved".to_owned())));
    // Undo leaves the caret at the end of the change: the start of region 2.
    act(cx, window, |window, cx| window.press("secondary-z", cx));
    assert_eq!(badge(cx), (Some(Role::Status), Some("1 of 1 unresolved".to_owned())));
}

#[gpui_kit::test]
fn the_conflict_badge_names_the_conflict_holding_the_caret(cx: &mut TestAppContext) {
    // Regions: 0 a, 1 conflict, 2 c, 3 conflict, 4 e. Result lines: a L R c M N e.
    let (window, view) = open(cx, doc("a\nb\nc\nd\ne\n", "a\nL\nc\nM\ne\n", "a\nR\nc\nN\ne\n"));
    // The window opens on the first conflict; start outside it.
    set_caret(cx, &view, 0);
    let badge = |cx: &mut TestAppContext| named(cx, window, CONFLICT_BADGE_ID).1;
    let press = |cx: &mut TestAppContext, key: &str| {
        act(cx, window, |window, cx| window.press(key, cx));
    };
    assert_eq!(badge(cx).as_deref(), Some("2 of 2 unresolved"));
    press(cx, "secondary-alt-down");
    assert_eq!(badge(cx).as_deref(), Some("Conflict 1 of 2 · 2 unresolved"));
    press(cx, "secondary-alt-down");
    assert_eq!(badge(cx).as_deref(), Some("Conflict 2 of 2 · 2 unresolved"));
    // The caret moving by itself, out of the conflict and into the one before.
    press(cx, "up");
    assert_eq!(badge(cx).as_deref(), Some("2 of 2 unresolved"));
    press(cx, "up");
    assert_eq!(badge(cx).as_deref(), Some("Conflict 1 of 2 · 2 unresolved"));
}

#[gpui_kit::test]
fn a_merge_without_conflicts_has_no_badge(cx: &mut TestAppContext) {
    // An unchanged line keeps the two changes apart; touching changes conflict.
    let (window, _) = open(cx, doc("a\nm\nb\n", "A\nm\nb\n", "a\nm\nB\n"));
    act(cx, window, |window, _| {
        assert!(window.try_find(CONFLICT_BADGE_ID).is_none());
    });
}

#[gpui_kit::test]
fn pick_targets_are_toggle_buttons_named_by_pane_and_region(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    assert_eq!(
        named(cx, window, pick_id(0, RowIx::new(1))),
        (Some(Role::Button), Some("Left in region 2 of 3, unresolved".to_owned()))
    );
    assert_eq!(
        named(cx, window, pick_id(1, RowIx::new(1))),
        (Some(Role::Button), Some("Right in region 2 of 3, unresolved".to_owned()))
    );
    let checked = |cx: &mut TestAppContext, pane| {
        let mut checked = None;
        act(cx, window, |window, _| {
            checked = window.find(pick_id(pane, RowIx::new(1))).checked();
        });
        checked
    };
    assert_eq!((checked(cx, 0), checked(cx, 1)), (Some(false), Some(false)));
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!((checked(cx, 0), checked(cx, 1)), (Some(true), Some(false)));
}

/// Whether the pick target of `pane` on `row` is on, and whether it is mixed.
fn pick_state(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    pane: usize,
    row: usize,
) -> (Option<bool>, Option<bool>) {
    let mut state = None;
    act(cx, window, |window, _| {
        let target = window.find(pick_id(pane, RowIx::new(row)));
        state = Some((target.checked(), target.indeterminate()));
    });
    state.expect("the window ran")
}

const ON: (Option<bool>, Option<bool>) = (Some(true), Some(false));
const OFF: (Option<bool>, Option<bool>) = (Some(false), Some(false));
/// The label of the pick target of `pane` on `row`.
fn pick_label(cx: &mut TestAppContext, window: AnyWindowHandle, pane: usize, row: usize) -> String {
    named(cx, window, pick_id(pane, RowIx::new(row))).1.expect("a pick target has a label")
}

// macOS reports a mixed toggle as on (AccessKit maps every state but off to 1), so an
// unresolved conflict's targets are off, and their labels say the conflict is unresolved.
#[gpui_kit::test]
fn an_unresolved_conflicts_side_targets_are_off_and_say_so(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    assert_eq!(pick_state(cx, window, 0, 1), OFF);
    assert_eq!(pick_state(cx, window, 1, 1), OFF);
    assert_eq!(pick_label(cx, window, 0, 1), "Left in region 2 of 3, unresolved");
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!(pick_state(cx, window, 0, 1), ON);
    assert_eq!(pick_state(cx, window, 1, 1), OFF);
    assert_eq!(pick_label(cx, window, 0, 1), "Left in region 2 of 3");
    assert_eq!(pick_label(cx, window, 1, 1), "Right in region 2 of 3");
}

#[gpui_kit::test]
fn an_unresolved_conflicts_base_target_is_off(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    act(cx, window, |window, cx| window.press("secondary-b", cx));
    // Pane 1 is base; pane 2 is right.
    assert_eq!(pick_state(cx, window, 1, 1), OFF);
    assert_eq!(pick_state(cx, window, 2, 1), OFF);
    assert_eq!(pick_label(cx, window, 1, 1), "Base in region 2 of 3, unresolved");
}

#[gpui_kit::test]
fn an_automatic_regions_pick_target_is_on_for_the_side_it_takes(cx: &mut TestAppContext) {
    // Rows: 0 a, 1 left's B (left only), 2 c, 3 right's D (right only).
    let (window, _) = open(cx, doc("a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nb\nc\nD\n"));
    let checked = |cx: &mut TestAppContext, pane, row| {
        let mut checked = None;
        act(cx, window, |window, _| {
            checked = window.find(pick_id(pane, row)).checked();
        });
        checked
    };
    assert_eq!(
        (checked(cx, 0, RowIx::new(1)), checked(cx, 1, RowIx::new(1))),
        (Some(true), Some(false))
    );
    assert_eq!(
        (checked(cx, 0, RowIx::new(3)), checked(cx, 1, RowIx::new(3))),
        (Some(false), Some(true))
    );
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!(
        (checked(cx, 0, RowIx::new(1)), checked(cx, 1, RowIx::new(1))),
        (Some(false), Some(false))
    );
}

#[gpui_kit::test]
fn cells_name_their_pane_line_and_text(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    assert_eq!(
        named(cx, window, INPUT_LINES_ID),
        (Some(Role::Table), Some("Input lines".to_owned()))
    );
    assert_eq!(
        named(cx, window, cell_id(0, RowIx::new(0))),
        (Some(Role::Cell), Some("Left, line 1: a".to_owned()))
    );
    assert_eq!(
        named(cx, window, cell_id(0, RowIx::new(1))),
        (Some(Role::Cell), Some("Left, line 2: L".to_owned()))
    );
    assert_eq!(
        named(cx, window, cell_id(1, RowIx::new(1))),
        (Some(Role::Cell), Some("Right, no line: 1 line deleted".to_owned()))
    );
    assert_eq!(
        named(cx, window, cell_id(1, RowIx::new(2))),
        (Some(Role::Cell), Some("Right, line 2: c".to_owned()))
    );
}

#[gpui_kit::test]
fn a_filler_cell_beside_an_insertion_names_no_deletion(cx: &mut TestAppContext) {
    // Left inserts X; right's filler row stands for no base line.
    let (window, _) = open(cx, doc("a\nc\n", "a\nX\nc\n", "a\nc\n"));
    assert_eq!(
        named(cx, window, cell_id(1, RowIx::new(1))),
        (Some(Role::Cell), Some("Right, no line".to_owned()))
    );
}

#[gpui_kit::test]
fn the_result_and_the_status_bar_are_named(cx: &mut TestAppContext) {
    let (window, view) = deleted_on_the_right(cx);
    let editor = cx.read(|cx| view.read(cx).result_editor().entity_id());
    assert_eq!(
        named(cx, window, ("input", editor)),
        (Some(Role::MultilineTextInput), Some("Result".to_owned()))
    );
    let status = cx.read(|cx| view.read(cx).status_text().to_string());
    assert_eq!(named(cx, window, STATUS_ID), (Some(Role::Status), Some(status)));
}

#[gpui_kit::test]
fn the_status_label_follows_a_pick(cx: &mut TestAppContext) {
    let (window, _) = deleted_on_the_right(cx);
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    assert_eq!(
        named(cx, window, STATUS_ID).1,
        Some(
            "no repo · git merge · 1 difference · 1 conflict · 0 unresolved · UTF-8 · LF · whitespace significant"
                .to_owned()
        )
    );
}

#[gpui_kit::test]
fn the_go_to_line_field_is_a_named_group(cx: &mut TestAppContext) {
    let lines = "a\nb\n";
    let (window, _) = open(cx, doc(lines, lines, lines));
    act(cx, window, |window, cx| window.press("secondary-l", cx));
    assert_eq!(
        named(cx, window, GO_TO_LINE_ID),
        (Some(Role::Group), Some("Go to line (1–2):".to_owned()))
    );
    assert_eq!(
        named(cx, window, GO_TO_LINE_FIELD_ID),
        (Some(Role::TextInput), Some("Line number".to_owned()))
    );
}

#[gpui_kit::test]
fn each_input_panes_search_controls_are_named(cx: &mut TestAppContext) {
    let (window, view) = open(cx, doc("a\nb\n", "a\nx\n", "a\ny\n"));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    act(cx, window, |window, cx| window.press("secondary-f", cx));
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| state.set_search_query("x", true, cx));
    });
    cx.run_until_parked();
    let expected = [
        (input_match_previous_id(0), Role::Button, "Previous match in Left"),
        (input_match_count_id(0), Role::Status, "1 of 1"),
        (input_match_next_id(0), Role::Button, "Next match in Left"),
        (input_match_previous_id(1), Role::Button, "Previous match in Right"),
        (input_match_count_id(1), Role::Status, "No matches"),
        (input_match_next_id(1), Role::Button, "Next match in Right"),
    ];
    for (id, role, label) in expected {
        assert_eq!(named(cx, window, id), (Some(role), Some(label.to_owned())));
    }
}
