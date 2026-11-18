//! Headless tests of a change another program makes to OUTPUT while the merge window is open
//! (spec §6.3).

use crate::common;

use common::{act, doc, open_saving};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, Role, TestAppContext};
use merge_app::window::OUTPUT_BANNER_ID;
use merge_app::{ExitCode, MergeWindow};
use merge_core::document::MergeDocument;
use tempfile::TempDir;

const OVERWRITE: &str = "Overwrite";
const KEEP_EDITING: &str = "Keep Editing";
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
    act(cx, o.window, |window, cx| window.press("secondary-s", cx));
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

#[gpui_kit::test]
fn save_and_exit_after_a_change_asks_and_overwrite_writes(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    o.write("other\n");
    output_changed(cx, &o);
    save_and_exit(cx, &o);
    assert!(cx.has_pending_prompt());
    assert_eq!(o.written().as_deref(), Some("other\n"), "nothing written before the answer");
    cx.simulate_prompt_answer(OVERWRITE);
    cx.run_until_parked();
    assert_eq!(o.written().as_deref(), Some(MERGED));
    assert_eq!(o.finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn keep_editing_leaves_the_changed_output_and_the_window(cx: &mut TestAppContext) {
    let o = open(cx, Some("theirs\n"));
    o.write("other\n");
    output_changed(cx, &o);
    save_and_exit(cx, &o);
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert_eq!(o.written().as_deref(), Some("other\n"));
    assert_eq!(o.finished.codes(), []);
    assert_eq!(banner(cx, &o).and_then(|b| b.1).as_deref(), Some(BANNER), "the banner stays");
}
