//! Headless tests of Save Copy As (`docs/DESIGN.md` §E6). The save dialog is GPUI's;
//! the tests call the write path with a chosen path.

use crate::common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{act, doc, open_with};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use merge_app::window::COPY_NOTE_ID;
use merge_app::{ExitCode, MergeWindow, WindowSettings};
use merge_core::document::{MergeDocument, OutputEnding};
use tempfile::TempDir;

/// Past every journal delay, so each write has run.
const SETTLE: Duration = Duration::from_secs(60);

/// Resolves without conflicts to `A b C`.
fn clean() -> MergeDocument {
    doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n")
}

/// One conflict at line 2.
fn conflict() -> MergeDocument {
    doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n")
}

const MARKERS_LF: &str = "a\n<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n";
const MARKERS_CRLF: &str = "a\r\n<<<<<<<\r\nL\r\n|||||||\r\nb\r\n=======\r\nR\r\n>>>>>>>\r\nc\r\n";

struct Session {
    dir: TempDir,
    window: AnyWindowHandle,
    view: Entity<MergeWindow>,
    finished: common::Finished,
}

impl Session {
    fn open(cx: &mut TestAppContext, doc: MergeDocument, ending: OutputEnding) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = WindowSettings {
            output: Some(dir.path().join("out")),
            journal_dir: Some(dir.path().join("journal")),
            line_ending: ending,
            ..WindowSettings::default()
        };
        let (window, view, finished) = open_with(cx, doc, &settings);
        cx.run_until_parked();
        Self { dir, window, view, finished }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn copy(&self, cx: &mut TestAppContext, to: &Path) {
        let view = self.view.clone();
        act(cx, self.window, |window, cx| {
            view.update(cx, |view, cx| view.save_copy(to, window, cx));
        });
        cx.run_until_parked();
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        act(cx, self.window, |window, cx| window.press(key, cx));
        cx.run_until_parked();
    }

    fn note(&self, cx: &mut TestAppContext) -> Option<String> {
        let mut note = None;
        act(cx, self.window, |window, _| {
            note = window.try_find(COPY_NOTE_ID).and_then(|found| found.label().map(str::to_owned));
        });
        note
    }

    /// Every journal file's name and bytes, after the journal's writes have run.
    fn journal(&self, cx: &mut TestAppContext) -> Vec<(PathBuf, Vec<u8>)> {
        cx.executor().advance_clock(SETTLE);
        cx.run_until_parked();
        let Ok(entries) = std::fs::read_dir(self.path("journal")) else {
            return Vec::new();
        };
        let mut files: Vec<_> = entries
            .map(|e| {
                let path = e.expect("entry").path();
                let bytes = std::fs::read(&path).expect("journal file");
                (path, bytes)
            })
            .collect();
        files.sort();
        files
    }
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).expect("written")
}

#[gpui_kit::test]
fn a_copy_of_a_conflict_holds_what_save_and_exit_writes_under_each_ending(cx: &mut TestAppContext) {
    let expected = [
        (OutputEnding::Keep, MARKERS_LF),
        (OutputEnding::Base, MARKERS_LF),
        (OutputEnding::Lf, MARKERS_LF),
        (OutputEnding::CrLf, MARKERS_CRLF),
    ];
    for (ending, bytes) in expected {
        let s = Session::open(cx, conflict(), ending);
        s.copy(cx, &s.path("copy"));
        assert_eq!(read(&s.path("copy")), bytes.as_bytes(), "{ending:?}");
        assert!(!cx.has_pending_prompt(), "{ending:?}: no confirmation");
        assert_eq!(s.finished.codes(), [], "{ending:?}: the window stays");
        assert!(!s.path("out").exists(), "{ending:?}: OUTPUT untouched");
        assert_eq!(s.note(cx).as_deref(), Some("Copy saved with 1 conflict"), "{ending:?}");
        s.press(cx, "secondary-s");
        cx.simulate_prompt_answer("Save with Conflicts");
        cx.run_until_parked();
        assert_eq!(read(&s.path("out")), bytes.as_bytes(), "{ending:?}");
        assert_eq!(s.finished.codes(), [ExitCode::Conflicts], "{ending:?}");
    }
}

#[gpui_kit::test]
fn a_resolved_copy_leaves_save_and_exit_to_exit_0(cx: &mut TestAppContext) {
    let s = Session::open(cx, clean(), OutputEnding::CrLf);
    s.copy(cx, &s.path("copy"));
    assert_eq!(read(&s.path("copy")), b"A\r\nb\r\nC\r\n");
    assert_eq!(s.note(cx).as_deref(), Some("Copy saved, resolved"));
    assert_eq!(s.finished.codes(), []);
    s.press(cx, "secondary-s");
    assert_eq!(read(&s.path("out")), b"A\r\nb\r\nC\r\n");
    assert_eq!(s.finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn a_copy_over_an_existing_file_replaces_it(cx: &mut TestAppContext) {
    let s = Session::open(cx, clean(), OutputEnding::Keep);
    std::fs::write(s.path("copy"), "old contents, longer than the merge\n").expect("old");
    s.copy(cx, &s.path("copy"));
    assert_eq!(read(&s.path("copy")), b"A\nb\nC\n");
}

#[gpui_kit::test]
fn a_failed_copy_alerts_and_leaves_the_window_and_journal_alone(cx: &mut TestAppContext) {
    let s = Session::open(cx, conflict(), OutputEnding::Keep);
    // Pick right in the conflict: a change the journal records.
    s.press(cx, "secondary-alt-down");
    s.press(cx, "secondary-3");
    let journal = s.journal(cx);
    assert_eq!(journal.len(), 1, "the pick is journalled");
    let text = cx.read(|cx| s.view.read(cx).document().text());
    s.copy(cx, &s.path("no-such-dir").join("copy"));
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("OK");
    cx.run_until_parked();
    assert_eq!(s.finished.codes(), []);
    assert_eq!(s.note(cx), None);
    assert_eq!(cx.read(|cx| s.view.read(cx).document().text()), text);
    assert_eq!(s.journal(cx), journal);
    // The window still saves to OUTPUT.
    s.press(cx, "secondary-s");
    assert_eq!(read(&s.path("out")), b"a\nR\nc\n");
    assert_eq!(s.finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn the_copy_note_goes_with_the_next_change(cx: &mut TestAppContext) {
    let s = Session::open(cx, conflict(), OutputEnding::Keep);
    s.copy(cx, &s.path("copy"));
    assert_eq!(s.note(cx).as_deref(), Some("Copy saved with 1 conflict"));
    s.press(cx, "secondary-alt-down");
    s.press(cx, "secondary-3");
    assert_eq!(s.note(cx), None);
}

#[gpui_kit::test]
fn a_copy_leaves_the_journal_as_it_was(cx: &mut TestAppContext) {
    let s = Session::open(cx, conflict(), OutputEnding::Keep);
    s.press(cx, "secondary-alt-down");
    s.press(cx, "secondary-3");
    let journal = s.journal(cx);
    assert_eq!(journal.len(), 1, "the pick is journalled");
    s.copy(cx, &s.path("copy"));
    assert_eq!(read(&s.path("copy")), b"a\nR\nc\n");
    assert_eq!(s.journal(cx), journal);
}

#[gpui_kit::test]
fn the_key_asks_for_a_path_in_outputs_directory_and_writes_there(cx: &mut TestAppContext) {
    let s = Session::open(cx, clean(), OutputEnding::Keep);
    s.press(cx, "secondary-shift-s");
    assert!(cx.did_prompt_for_new_path());
    let mut started = None;
    let copy = s.path("copy");
    cx.simulate_new_path_selection(|dir| {
        started = Some(dir.to_owned());
        Some(copy.clone())
    });
    cx.run_until_parked();
    assert_eq!(started.as_deref(), Some(s.dir.path()));
    assert_eq!(read(&copy), b"A\nb\nC\n");
    assert_eq!(s.finished.codes(), []);
}

#[gpui_kit::test]
fn a_cancelled_dialog_writes_nothing(cx: &mut TestAppContext) {
    let s = Session::open(cx, clean(), OutputEnding::Keep);
    s.press(cx, "secondary-shift-s");
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert_eq!(s.note(cx), None);
    assert_eq!(s.finished.codes(), []);
    // Without a change the journal writes nothing, so nothing is in the directory.
    let names: Vec<_> = std::fs::read_dir(s.dir.path())
        .expect("dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(names, Vec::<std::ffi::OsString>::new());
}

#[gpui_kit::test]
fn a_copy_is_written_off_the_ui_thread(cx: &mut TestAppContext) {
    let s = Session::open(cx, conflict(), OutputEnding::Keep);
    let view = s.view.clone();
    let copy = s.path("copy");
    act(cx, s.window, |window, cx| {
        view.update(cx, |view, cx| view.save_copy(&copy, window, cx));
    });
    assert!(!copy.exists(), "not written yet");
    assert_eq!(s.note(cx), None, "no note before the write lands");
    cx.run_until_parked();
    assert_eq!(read(&copy), MARKERS_LF.as_bytes());
    assert_eq!(s.note(cx).as_deref(), Some("Copy saved with 1 conflict"));
}

#[gpui_kit::test]
fn save_and_exit_waits_for_a_copy_being_written(cx: &mut TestAppContext) {
    let s = Session::open(cx, clean(), OutputEnding::Keep);
    let view = s.view.clone();
    let copy = s.path("copy");
    act(cx, s.window, |window, cx| {
        view.update(cx, |view, cx| view.save_copy(&copy, window, cx));
        window.press("secondary-s", cx);
    });
    assert_eq!(s.finished.codes(), [], "the copy is still being written");
    cx.run_until_parked();
    assert_eq!(read(&copy), b"A\nb\nC\n");
    assert_eq!(read(&s.path("out")), b"A\nb\nC\n");
    assert_eq!(s.finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn cancel_waits_for_a_copy_being_written(cx: &mut TestAppContext) {
    let s = Session::open(cx, clean(), OutputEnding::Keep);
    let view = s.view.clone();
    let copy = s.path("copy");
    act(cx, s.window, |window, cx| {
        view.update(cx, |view, cx| view.save_copy(&copy, window, cx));
        window.press("secondary-w", cx);
    });
    assert_eq!(s.finished.codes(), [], "the copy is still being written");
    cx.run_until_parked();
    assert_eq!(read(&copy), b"A\nb\nC\n");
    assert!(!s.path("out").exists(), "cancel writes no output");
    assert_eq!(s.finished.codes(), [ExitCode::Cancelled]);
}

#[gpui_kit::test]
fn copies_to_one_path_land_in_the_order_asked(cx: &mut TestAppContext) {
    let s = Session::open(cx, conflict(), OutputEnding::Keep);
    let view = s.view.clone();
    let copy = s.path("copy");
    act(cx, s.window, |window, cx| {
        view.update(cx, |view, cx| view.save_copy(&copy, window, cx));
        // Pick right: the second copy holds no markers.
        window.press("secondary-alt-down", cx);
        window.press("secondary-3", cx);
        view.update(cx, |view, cx| view.save_copy(&copy, window, cx));
    });
    cx.run_until_parked();
    assert_eq!(read(&copy), b"a\nR\nc\n");
    assert_eq!(s.note(cx).as_deref(), Some("Copy saved, resolved"));
}
