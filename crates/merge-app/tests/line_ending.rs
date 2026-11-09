//! Headless tests of the output line ending in the merge window (`docs/PLAN.md` §E5).

use crate::common;
use merge_view::rows::RowIx;

use common::{act, doc, open_with};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use merge_app::window::{MIXED_ENDINGS_ID, pick_id};
use merge_app::{ExitCode, MergeWindow, WindowSettings};
use merge_core::document::{MergeDocument, OutputEnding};
use tempfile::TempDir;

/// Base in CRLF; left changes `b` to `B` with LF; right keeps base.
fn crlf_base_lf_edit() -> MergeDocument {
    doc("a\r\nb\r\nc\r\n", "a\r\nB\nc\r\n", "a\r\nb\r\nc\r\n")
}

fn setup(
    cx: &mut TestAppContext,
    ending: OutputEnding,
) -> (TempDir, AnyWindowHandle, Entity<MergeWindow>, common::Finished) {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = WindowSettings {
        output: Some(dir.path().join("out")),
        line_ending: ending,
        ..WindowSettings::default()
    };
    let (window, view, finished) = open_with(cx, crlf_base_lf_edit(), &settings);
    (dir, window, view, finished)
}

fn shows_mixed(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    let mut shown = false;
    act(cx, window, |window, _| {
        shown = window.try_find(MIXED_ENDINGS_ID).is_some();
    });
    shown
}

#[gpui_kit::test]
fn save_writes_the_setting_s_endings_and_the_pane_keeps_the_inputs(cx: &mut TestAppContext) {
    let (dir, window, view, finished) = setup(cx, OutputEnding::Lf);
    let shown = cx.read(|cx| view.read(cx).result_editor().read(cx).text().to_string());
    assert_eq!(shown, "a\r\nB\nc\r\n");
    act(cx, window, |window, cx| window.press("secondary-s", cx));
    cx.run_until_parked();
    assert_eq!(std::fs::read(dir.path().join("out")).expect("written"), b"a\nB\nc\n");
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn mixed_endings_show_under_keep_until_a_pick_ends_them(cx: &mut TestAppContext) {
    let (_dir, window, view, _) = setup(cx, OutputEnding::Keep);
    assert!(shows_mixed(cx, window));
    // Region 1's targets: add right, which keeps base's CRLF `b`, then drop left's LF `B`.
    act(cx, window, |window, cx| {
        window.click(pick_id(1, RowIx::new(1)), cx);
    });
    assert!(shows_mixed(cx, window));
    act(cx, window, |window, cx| {
        window.click(pick_id(0, RowIx::new(1)), cx);
    });
    let text = cx.read(|cx| view.read(cx).document().text());
    assert_eq!(text, "a\r\nb\r\nc\r\n");
    assert!(!shows_mixed(cx, window));
}

#[gpui_kit::test]
fn mixed_endings_do_not_show_when_the_save_converts(cx: &mut TestAppContext) {
    let (_dir, window, _, _) = setup(cx, OutputEnding::CrLf);
    assert!(!shows_mixed(cx, window));
    // Left's LF `B` and right's CRLF `b`, which `keep` would save mixed.
    act(cx, window, |window, cx| {
        window.click(pick_id(1, RowIx::new(1)), cx);
    });
    assert!(!shows_mixed(cx, window));
}
