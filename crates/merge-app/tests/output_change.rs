//! Headless tests of a change another program makes to OUTPUT while the merge window is open
//! (spec §6.3).

use crate::common;

use common::{act, doc, open_saving, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, Role, TestAppContext};
use merge_app::window::{OUTPUT_BANNER_ID, RESULT_ID};
use merge_app::{ExitCode, MergeWindow};
use merge_core::document::MergeDocument;
use tempfile::TempDir;

const KEEP_EDITING: &str = "Keep Editing";
const QUIT: &str = "Quit";
const BANNER: &str = "out changed on disk since the merge opened. Save & Exit replaces it.";
/// What the merge of [`clean`] writes.
const MERGED: &str = "A\nb\nC\n";

/// Resolves without conflicts to [`MERGED`].
fn clean() -> MergeDocument {
    doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n")
}

struct Opened {
    dir: TempDir,
    window: AnyWindowHandle,
    view: Entity<MergeWindow>,
    finished: common::Finished,
}

impl Opened {
    fn output(&self) -> std::path::PathBuf {
        self.dir.path().join("out")
    }

    fn written(&self) -> Option<String> {
        std::fs::read_to_string(self.output()).ok()
    }

    fn write(&self, text: &str) {
        std::fs::write(self.output(), text).expect("output");
    }
}

/// Open a window on [`clean`] whose OUTPUT holds `held`, or does not exist, and let it read
/// what OUTPUT holds.
fn open(cx: &mut TestAppContext, held: Option<&str>) -> Opened {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("out");
    if let Some(held) = held {
        std::fs::write(&output, held).expect("output");
    }
    let (window, view, finished) = open_saving(cx, clean(), &output);
    cx.run_until_parked();
    Opened { dir, window, view, finished }
}

/// Tell the window that OUTPUT changed on disk, as its watcher does.
fn output_changed(cx: &mut TestAppContext, o: &Opened) {
    let view = o.view.clone();
    act(cx, o.window, |window, cx| {
        view.update(cx, |view, cx| view.output_disk_changed(window, cx));
    });
    cx.run_until_parked();
}

fn banner(cx: &mut TestAppContext, o: &Opened) -> Option<(Option<Role>, Option<String>)> {
    let mut found = None;
    act(cx, o.window, |window, cx| {
        window.render_frame(cx);
        found = window.try_find(OUTPUT_BANNER_ID).map(|e| (e.role(), e.label().map(str::to_owned)));
    });
    found
}

fn save_and_exit(cx: &mut TestAppContext, o: &Opened) {
    act(cx, o.window, |window, cx| window.press("secondary-q", cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_change_to_the_output_shows_a_banner(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    assert_eq!(banner(cx, &o), None, "no banner at first");
    o.write("other\n");
    output_changed(cx, &o);
    assert_eq!(banner(cx, &o), Some((Some(Role::Alert), Some(BANNER.to_owned()))));
}

#[gpui_kit::test]
fn an_output_another_program_creates_shows_a_banner(cx: &mut TestAppContext) {
    let o = open(cx, None);
    o.write("new\n");
    output_changed(cx, &o);
    assert_eq!(banner(cx, &o).and_then(|b| b.1).as_deref(), Some(BANNER));
}

#[gpui_kit::test]
fn an_output_another_program_deletes_shows_a_banner(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    std::fs::remove_file(o.output()).expect("remove");
    output_changed(cx, &o);
    assert_eq!(banner(cx, &o).and_then(|b| b.1).as_deref(), Some(BANNER));
}

#[gpui_kit::test]
fn the_output_written_back_as_it_was_shows_no_banner(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    o.write("theirs\n");
    output_changed(cx, &o);
    assert_eq!(banner(cx, &o), None);
}

#[gpui_kit::test]
fn the_banner_goes_once_the_output_holds_what_it_held_again(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    o.write("other\n");
    output_changed(cx, &o);
    o.write("theirs\n");
    output_changed(cx, &o);
    assert_eq!(banner(cx, &o), None);
}

#[gpui_kit::test]
fn an_output_that_cannot_be_read_keeps_the_banner_as_it_was(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    std::fs::remove_file(o.output()).expect("remove");
    std::fs::create_dir(o.output()).expect("a directory in its place");
    output_changed(cx, &o);
    assert_eq!(banner(cx, &o), None, "an unreadable file is no news");
}

#[gpui_kit::test]
fn save_and_exit_without_a_change_asks_nothing(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    save_and_exit(cx, &o);
    assert!(!cx.has_pending_prompt());
    assert_eq!(o.written().as_deref(), Some(MERGED));
    assert_eq!(o.finished.codes(), [ExitCode::Resolved]);
}

/// Click the result at its start, then type `text` there.
fn type_in_result(cx: &mut TestAppContext, o: &Opened, text: &str) {
    common::set_caret(cx, &o.view, 0);
    act(cx, o.window, |window, cx| window.click(RESULT_ID, cx));
    common::set_caret(cx, &o.view, 0);
    act(cx, o.window, |window, cx| window.input(text, cx));
    cx.run_until_parked();
}

/// Write `text` to OUTPUT and tell the window, as the watcher does.
fn change(cx: &mut TestAppContext, o: &Opened, text: &str) {
    o.write(text);
    output_changed(cx, o);
}

#[gpui_kit::test]
fn a_change_asks_at_once_and_locks_the_merge_until_answered(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    change(cx, &o, "other\n");
    assert!(cx.has_pending_prompt());
    type_in_result(cx, &o, "x");
    assert_eq!(texts(cx, &o.view), (MERGED.to_owned(), MERGED.to_owned()), "no typing now");
    // ⌘1 toggles the left side's pick in the first region; it changes nothing now.
    act(cx, o.window, |window, cx| window.press("secondary-1", cx));
    assert_eq!(texts(cx, &o.view).0, MERGED, "no picks while the question is open");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    act(cx, o.window, |window, cx| window.press("secondary-1", cx));
    // The region holds no pick now, so its line goes.
    assert_eq!(texts(cx, &o.view).0, "b\nC\n", "picks work again");
}

#[gpui_kit::test]
fn after_keep_editing_further_changes_and_save_ask_nothing(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    change(cx, &o, "other\n");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    change(cx, &o, "another\n");
    assert!(!cx.has_pending_prompt(), "one question per session");
    type_in_result(cx, &o, "x");
    assert_eq!(texts(cx, &o.view).1, "xA\nb\nC\n", "typing works");
    assert_eq!(banner(cx, &o).and_then(|b| b.1).as_deref(), Some(BANNER), "the banner stays");
    save_and_exit(cx, &o);
    assert!(!cx.has_pending_prompt());
    assert_eq!(o.written().as_deref(), Some("xA\nb\nC\n"));
    assert_eq!(o.finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn a_change_undone_before_the_answer_still_counts_as_asked(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    change(cx, &o, "other\n");
    change(cx, &o, "theirs\n");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert_eq!(banner(cx, &o), None);
    change(cx, &o, "other\n");
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn quit_without_edits_exits_2_and_leaves_the_output(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    change(cx, &o, "other\n");
    cx.simulate_prompt_answer(QUIT);
    cx.run_until_parked();
    assert_eq!(o.written().as_deref(), Some("other\n"));
    assert_eq!(o.finished.codes(), [ExitCode::Cancelled]);
}

#[gpui_kit::test]
fn quit_after_edits_asks_to_save_as_close_does(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    act(cx, o.window, |window, cx| window.press("secondary-1", cx));
    change(cx, &o, "other\n");
    cx.simulate_prompt_answer(QUIT);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "Save your changes?");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert_eq!(o.finished.codes(), []);
    type_in_result(cx, &o, "x");
    assert_eq!(texts(cx, &o.view).1, "xb\nC\n", "the merge goes on");
}
