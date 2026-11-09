//! Headless tests of the About window (spec §6.3).

use crate::common;

use common::{act, doc, open};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Role, TestAppContext, px};
use merge_app::About;
use merge_app::about::{BUILD_ID, DESCRIPTION_ID, LOGO_ID, VCS_ID, VERSION_ID};

/// 56 units high at the logo's ratio of 734.68 to 186, rounded.
const LOGO_WIDTH: f32 = 221.;
const LOGO_HEIGHT: f32 = 56.;
/// The window's content height: 16 padding, the logo, 8 below it, 8 gap, the version's line of
/// 16 px text at the theme's line height of 1.618 (25.9, laid out as 26), 8 gap, two 16 lines of
/// 12 px description text, 16 padding.
const WINDOW_HEIGHT: f32 = 170.;
/// A debug build without `TRIAD_COMMIT` shows placeholder ids in one 16 line of 12 px text,
/// 8 below the version.
#[cfg(debug_assertions)]
const BUILD_LINE: f32 = 24.;
/// The flex gap between the window's lines.
#[cfg(debug_assertions)]
const BUILD_GAP: f32 = 8.;
#[cfg(not(debug_assertions))]
const BUILD_LINE: f32 = 0.;
/// The padding above the logo and below the description.
const PADDING: f32 = 16.;
const VCS_TEXT: &str = "Built for Jujutsu, with full Git support";
/// STYLE.md's dialog text.
const SMALL_TEXT: f32 = 12.;
/// The line height of the About window's 12 px lines: 12 px text with 4 px of leading.
const SMALL_LINE_HEIGHT: f32 = 16.;

fn window_count(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| cx.windows().len())
}

/// Opens a merge window, then the About window. Returns both handles.
fn shown(cx: &mut TestAppContext) -> (AnyWindowHandle, AnyWindowHandle) {
    let (merge, _) = open(cx, doc("a\n", "b\n", "c\n"));
    show(cx);
    let about = cx
        .update(|cx| {
            cx.windows().into_iter().find(|window| window.window_id() != merge.window_id())
        })
        .expect("the About window is open");
    act(cx, about, TestWindowExt::render_frame);
    (merge, about)
}

fn show(cx: &mut TestAppContext) {
    cx.update(|cx| cx.dispatch_action(&About));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_window_shows_the_logo_the_version_and_the_description(cx: &mut TestAppContext) {
    let (_, about) = shown(cx);
    act(cx, about, |window, _| {
        let logo = window.find(LOGO_ID);
        assert_eq!(logo.role(), Some(Role::Image));
        assert_eq!(logo.label(), Some("triad"));
        assert_eq!(logo.bounds().size.width, px(LOGO_WIDTH));
        assert_eq!(logo.bounds().size.height, px(LOGO_HEIGHT));
        assert_eq!(
            window.find(VERSION_ID).label(),
            Some(format!("Version {}", env!("CARGO_PKG_VERSION")).as_str())
        );
        assert_eq!(window.find(DESCRIPTION_ID).label(), Some("Three-way merge and two-way diff"));
        assert_eq!(window.find(VCS_ID).label(), Some(VCS_TEXT));
    });
}

#[gpui_kit::test]
fn the_description_takes_small_text_under_the_version(cx: &mut TestAppContext) {
    let (_, about) = shown(cx);
    act(cx, about, |window, _| {
        let description = window.find(DESCRIPTION_ID).bounds();
        let vcs = window.find(VCS_ID).bounds();
        assert_eq!(description.size.height, px(SMALL_LINE_HEIGHT));
        assert_eq!(vcs.size.height, px(SMALL_LINE_HEIGHT));
        assert_eq!(vcs.top(), description.bottom());
        let run = window.text_style().to_run(VCS_TEXT.len());
        let width =
            window.text_system().shape_line(VCS_TEXT.into(), px(SMALL_TEXT), &[run], None).width;
        // Layout rounds to whole pixels.
        assert_eq!(vcs.size.width, width.round());
    });
}

/// Test builds get no `TRIAD_COMMIT`: only release scripts set it.
#[cfg(not(debug_assertions))]
#[gpui_kit::test]
fn a_build_without_a_commit_shows_no_build_line(cx: &mut TestAppContext) {
    let (_, about) = shown(cx);
    act(cx, about, |window, _| {
        assert!(window.try_find(BUILD_ID).is_none());
    });
}

/// Test builds get no `TRIAD_COMMIT`, so a debug build shows the placeholder.
#[cfg(debug_assertions)]
#[gpui_kit::test]
fn a_debug_build_without_a_commit_shows_the_null_ids_under_the_version(cx: &mut TestAppContext) {
    let (_, about) = shown(cx);
    act(cx, about, |window, _| {
        let build = window.find(BUILD_ID);
        assert_eq!(build.label(), Some("Commit 000000000000, change zzzzzzzzzzzz"));
        assert_eq!(build.bounds().size.height, px(SMALL_LINE_HEIGHT));
        assert_eq!(build.bounds().top(), window.find(VERSION_ID).bounds().bottom() + px(BUILD_GAP));
    });
}

#[gpui_kit::test]
fn the_content_sits_centred_in_a_window_with_no_close_button(cx: &mut TestAppContext) {
    let (_, about) = shown(cx);
    act(cx, about, |window, _| {
        let top = window.find(LOGO_ID).bounds().top();
        let bottom = window.find(VCS_ID).bounds().bottom();
        assert_eq!(top, px(PADDING));
        assert_eq!(px(WINDOW_HEIGHT + BUILD_LINE) - bottom, px(PADDING));
        assert!(window.try_find("about-close").is_none());
    });
}

#[gpui_kit::test]
fn a_click_on_the_mark_opens_the_window(cx: &mut TestAppContext) {
    use merge_app::toolbar::MARK_ID;

    let (merge, _) = open(cx, doc("a\n", "b\n", "c\n"));
    assert_eq!(window_count(cx), 1);
    act(cx, merge, |window, cx| window.click(MARK_ID, cx));
    cx.run_until_parked();
    assert_eq!(window_count(cx), 2);
}

#[gpui_kit::test]
fn a_second_request_opens_no_second_window(cx: &mut TestAppContext) {
    let _ = shown(cx);
    show(cx);
    assert_eq!(window_count(cx), 2);
}

#[gpui_kit::test]
fn closing_leaves_the_merge_window_open(cx: &mut TestAppContext) {
    let (merge, about) = shown(cx);
    act(cx, about, |window, cx| window.press("escape", cx));
    cx.run_until_parked();
    let windows = cx.update(|cx| cx.windows());
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].window_id(), merge.window_id());
}

#[gpui_kit::test]
fn escape_closes_the_window(cx: &mut TestAppContext) {
    let (_, about) = shown(cx);
    act(cx, about, |window, cx| window.press("escape", cx));
    cx.run_until_parked();
    assert_eq!(window_count(cx), 1);
}

#[gpui_kit::test]
fn the_window_opens_again_after_it_closes(cx: &mut TestAppContext) {
    let (_, about) = shown(cx);
    act(cx, about, |window, cx| window.press("escape", cx));
    cx.run_until_parked();
    show(cx);
    assert_eq!(window_count(cx), 2);
}
