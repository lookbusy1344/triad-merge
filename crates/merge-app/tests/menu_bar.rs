//! The in-window menu bar of Linux and Windows (spec §2.2).

use crate::common;

use common::{doc, open};
use gpui_kit::TestAppContext;
use gpui_kit::component::GlobalState;

#[gpui_kit::test]
fn the_bar_holds_the_menus_of_the_one_definition(cx: &mut TestAppContext) {
    let _opened = open(cx, doc("a\n", "b\n", "c\n"));
    let titles: Vec<String> = cx.update(|cx| {
        GlobalState::global(cx).app_menus().iter().map(|menu| menu.name.to_string()).collect()
    });
    assert_eq!(titles, ["triad", "Edit", "Merge", "Go", "View"]);
}

#[gpui_kit::test]
fn the_merge_window_shows_the_bar_except_on_macos(cx: &mut TestAppContext) {
    let (_, view) = open(cx, doc("a\n", "b\n", "c\n"));
    assert_eq!(cx.update(|cx| view.read(cx).has_menu_bar()), !cfg!(target_os = "macos"));
}

#[gpui_kit::test]
fn the_mark_is_a_button_named_for_the_about_window(cx: &mut TestAppContext) {
    use gpui_kit::Role;
    use gpui_kit::test::TestWindowExt as _;
    use merge_app::toolbar::MARK_ID;

    let (window, _) = open(cx, doc("a\n", "b\n", "c\n"));
    let mut found = None;
    common::act(cx, window, |window, _| {
        found = Some(
            window.try_find(MARK_ID).map(|mark| (mark.role(), mark.label().map(str::to_owned))),
        );
    });
    assert_eq!(
        found.expect("the window ran"),
        Some((Some(Role::Button), Some("About triad".to_owned())))
    );
}

/// Whether a left press at `at` propagates past the window's handlers. On Windows the title
/// bar is the caption: a press nothing claims goes on to the system, which starts a window
/// move and swallows the release.
fn press_propagates(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    at: gpui_kit::Point<gpui_kit::Pixels>,
) -> bool {
    use gpui_kit::{InputEvent as _, Modifiers, MouseButton, MouseDownEvent};

    let mut propagates = None;
    common::act(cx, window, |window, cx| {
        let down = MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        };
        propagates = Some(window.dispatch_event(down.to_platform_input(), cx).propagate);
    });
    propagates.expect("the window ran")
}

#[gpui_kit::test]
fn the_mark_claims_a_press_and_the_rest_of_the_title_bar_leaves_it_to_the_system(
    cx: &mut TestAppContext,
) {
    use gpui_kit::point;
    use merge_app::toolbar::MARK_ID;

    /// gpui-component's title bar.
    const TITLE_BAR_ID: &str = "title-bar";

    let (window, _) = open(cx, doc("a\n", "b\n", "c\n"));
    let mark = common::bounds(cx, window, MARK_ID);
    let bar = common::bounds(cx, window, TITLE_BAR_ID);
    let empty = point(bar.center().x, bar.center().y);
    assert!(empty.x > mark.right(), "the bar's middle {empty:?} lies past the mark {mark:?}");
    assert!(!press_propagates(cx, window, mark.center()), "a press on the mark");
    assert!(press_propagates(cx, window, empty), "a press on the empty title bar");
}
