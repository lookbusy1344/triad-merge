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
