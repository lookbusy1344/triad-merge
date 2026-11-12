//! Headless tests of the recovery journal (spec §6.2; DESIGN.md §A6.4).

use crate::common;
use merge_core::index::RegionIx;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{Finished, act, open_with, set_caret};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use merge_app::window::JOURNAL_DEBOUNCE;
use merge_app::{ExitCode, MergeWindow, WindowSettings};
use merge_core::diff::{DiffOptions, Whitespace};
use merge_core::document::{Command, Labels, MergeDocument, MergeOptions, Resolution};
use merge_core::journal::{JOURNAL_VERSION, Journal};
use merge_core::merge3::{Inputs, Source};
use merge_core::testing::three;
use tempfile::TempDir;

const RESTORE: &str = "Restore";
const START_OVER: &str = "Start Over";
const DISCARD: &str = "Discard";
const OK: &str = "OK";
const SAVE_WITH_CONFLICTS: &str = "Save with Conflicts";
const JUST_BEFORE: Duration = Duration::from_millis(499);
const ONE_MS: Duration = Duration::from_millis(1);

fn inputs() -> Inputs {
    three("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n")
}

/// One conflict, region 1.
fn conflict(options: MergeOptions) -> MergeDocument {
    MergeDocument::new(inputs(), options, Labels::none()).expect("no labels")
}

struct Session {
    dir: TempDir,
}

impl Session {
    fn new() -> Self {
        Self { dir: tempfile::tempdir().expect("tempdir") }
    }

    fn output(&self) -> PathBuf {
        self.dir.path().join("out")
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            output: Some(self.output()),
            journal_dir: Some(self.dir.path().join("journal")),
            ..WindowSettings::default()
        }
    }

    fn journal_path(&self) -> PathBuf {
        self.journal_path_for(&self.output())
    }

    fn journal_path_for(&self, output: &Path) -> PathBuf {
        self.dir
            .path()
            .join("journal")
            .join(Journal::new(&conflict(MergeOptions::default())).file_name(Some(output)))
    }

    fn journal(&self) -> Option<Journal> {
        self.journal_for(&self.output())
    }

    fn journal_for(&self, output: &Path) -> Option<Journal> {
        let bytes = std::fs::read(self.journal_path_for(output)).ok()?;
        Some(Journal::parse(&bytes).expect("the journal parses"))
    }

    /// Open a window and let it hash its inputs, which offers any restore.
    fn open(
        &self,
        cx: &mut TestAppContext,
        doc: MergeDocument,
    ) -> (AnyWindowHandle, Entity<MergeWindow>, Finished) {
        let opened = open_with(cx, doc, &self.settings());
        cx.run_until_parked();
        opened
    }
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
}

fn wait(cx: &mut TestAppContext, by: Duration) {
    cx.executor().advance_clock(by);
    cx.run_until_parked();
}

/// Pick right in the conflict, then type at the start: two commands.
fn work(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<MergeWindow>) {
    press(cx, window, "secondary-alt-down");
    press(cx, window, "secondary-3");
    set_caret(cx, view, 0);
    act(cx, window, |window, cx| window.input("x", cx));
}

fn output(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> (String, String) {
    cx.read(|cx| {
        let view = view.read(cx);
        (view.document().text(), view.result_editor().read(cx).text().to_string())
    })
}

#[gpui_kit::test]
fn the_journal_waits_for_a_pause(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, _view, _) = session.open(cx, conflict(MergeOptions::default()));
    press(cx, window, "secondary-alt-down");
    press(cx, window, "secondary-3");
    wait(cx, JUST_BEFORE);
    assert_eq!(session.journal(), None, "not before 500 ms");
    // A second command restarts the wait.
    press(cx, window, "secondary-1");
    wait(cx, JUST_BEFORE);
    assert_eq!(session.journal(), None, "500 ms from the last command");
    wait(cx, ONE_MS);
    let journal = session.journal().expect("written after the pause");
    assert_eq!(journal.commands().len(), 2);
}

#[gpui_kit::test]
fn a_session_restores_after_a_crash(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let expected = output(cx, &view);
    assert_eq!(expected.0, "xa\nR\nc\n");
    // The first window never finishes: a crash. The same inputs open again.
    let (again, after, _) = session.open(cx, conflict(MergeOptions::default()));
    assert!(cx.has_pending_prompt(), "a restore is offered");
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    assert_eq!(output(cx, &after), expected);
    let resolution = cx.read(|cx| after.read(cx).document().resolutions()[1].clone());
    assert_eq!(resolution, Resolution::Pick(vec![Source::Side(1)]));
    // Later commands add to the restored journal, so a second crash loses nothing.
    press(cx, again, "secondary-alt-down");
    press(cx, again, "secondary-1");
    wait(cx, JOURNAL_DEBOUNCE);
    let journal = session.journal().expect("written");
    assert_eq!(journal.commands().len(), 3);
}

#[gpui_kit::test]
fn the_restore_is_offered_once_the_inputs_are_hashed(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    // The inputs hash off the UI thread, so the window shows before the offer.
    let _ = open_with(cx, conflict(MergeOptions::default()), &session.settings());
    assert!(!cx.has_pending_prompt(), "not before the inputs are hashed");
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "a restore is offered");
}

#[gpui_kit::test]
fn commands_before_the_inputs_are_hashed_are_journalled(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = open_with(cx, conflict(MergeOptions::default()), &session.settings());
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let journal = session.journal().expect("written after the pause");
    assert_eq!(journal.commands().len(), 2);
}

#[gpui_kit::test]
fn ending_before_the_inputs_are_hashed_still_keeps_the_latest_journal(cx: &mut TestAppContext) {
    let session = Session::new();
    let output = session.dir.path().join("no-such-dir").join("out");
    let settings = WindowSettings { output: Some(output.clone()), ..session.settings() };
    let (window, view, _) = open_with(cx, conflict(MergeOptions::default()), &settings);
    work(cx, window, &view);
    // The save fails. Its job writes the journal first, hashing the inputs itself.
    press(cx, window, "secondary-s");
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "the save failed");
    let journal = session.journal_for(&output).expect("written before the alert");
    assert_eq!(journal.commands().len(), 2);
}

#[gpui_kit::test]
fn start_over_discards_the_journal(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let (_, after, _) = session.open(cx, conflict(MergeOptions::default()));
    cx.simulate_prompt_answer(START_OVER);
    cx.run_until_parked();
    assert_eq!(session.journal(), None);
    let (core, editor) = output(cx, &after);
    assert_eq!(core, "a\nL\nR\nc\n");
    assert_eq!(editor, core);
}

#[gpui_kit::test]
fn commands_while_the_offer_is_open_keep_the_crashed_journal(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let (again, after, _) = session.open(cx, conflict(MergeOptions::default()));
    assert!(cx.has_pending_prompt(), "a restore is offered");
    set_caret(cx, &after, 0);
    act(cx, again, |window, cx| window.input("y", cx));
    wait(cx, JOURNAL_DEBOUNCE);
    let kept = session.journal().expect("the crashed journal");
    assert_eq!(kept.commands().len(), 2, "a crash now still finds the first session");
}

#[gpui_kit::test]
fn start_over_journals_the_commands_made_while_the_offer_was_open(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    assert_eq!(session.journal().expect("written").commands().len(), 2);
    let (again, after, _) = session.open(cx, conflict(MergeOptions::default()));
    assert!(cx.has_pending_prompt(), "a restore is offered");
    set_caret(cx, &after, 0);
    act(cx, again, |window, cx| window.input("x", cx));
    // The answer comes before the pause ends, so the removal starts the write.
    cx.simulate_prompt_answer(START_OVER);
    cx.run_until_parked();
    let journal = session.journal().expect("written after the removal");
    assert_eq!(journal.commands().len(), 1, "only this session's command");
}

#[gpui_kit::test]
fn restore_uses_the_journals_options_over_the_current_ones(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let all = MergeOptions {
        diff: DiffOptions { whitespace: Whitespace::All, ..DiffOptions::default() },
        ..MergeOptions::default()
    };
    let (_, after, _) = session.open(cx, conflict(all));
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    let options = cx.read(|cx| after.read(cx).document().options());
    assert_eq!(options, MergeOptions::default());
    assert_eq!(output(cx, &after).0, "xa\nR\nc\n");
}

#[gpui_kit::test]
fn a_new_session_offers_nothing(cx: &mut TestAppContext) {
    let session = Session::new();
    let _ = session.open(cx, conflict(MergeOptions::default()));
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn save_and_exit_deletes_the_journal(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, finished) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    assert!(session.journal().is_some());
    press(cx, window, "secondary-s");
    cx.run_until_parked();
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
    assert_eq!(session.journal(), None);
}

/// `git mergetool` puts back its own file after a save with conflicts (spec §6.2), so the
/// journal is the only copy of the session's work.
#[gpui_kit::test]
fn a_save_with_conflicts_keeps_the_journal_for_a_restore(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, finished) = session.open(cx, conflict(MergeOptions::default()));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.input("x", cx));
    // No wait: the session ends before the journal's pause.
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(SAVE_WITH_CONFLICTS);
    cx.run_until_parked();
    assert_eq!(finished.codes(), [ExitCode::Conflicts]);
    let journal = session.journal().expect("written at once and kept");
    assert_eq!(journal.commands().len(), 1);
    let expected = output(cx, &view);
    assert_eq!(expected.0, "xa\nL\nR\nc\n");
    let (_, after, _) = session.open(cx, conflict(MergeOptions::default()));
    assert!(cx.has_pending_prompt(), "a restore is offered");
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    assert_eq!(output(cx, &after), expected);
}

#[gpui_kit::test]
fn a_restored_session_puts_the_caret_on_its_first_conflict(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.input("x", cx));
    wait(cx, JOURNAL_DEBOUNCE);
    let (again, after, _) = session.open(cx, conflict(MergeOptions::default()));
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    // "xa\n" comes before the conflict.
    assert_eq!(common::caret(cx, &after), 3);
    press(cx, again, "secondary-1");
    assert_eq!(output(cx, &after).0, "xa\nL\nc\n");
}

#[gpui_kit::test]
fn cancel_deletes_the_journal_and_a_waiting_write(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, finished) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    press(cx, window, "secondary-w");
    cx.simulate_prompt_answer(DISCARD);
    cx.run_until_parked();
    assert_eq!(finished.codes(), [ExitCode::Cancelled]);
    wait(cx, JOURNAL_DEBOUNCE);
    assert_eq!(session.journal(), None, "the waiting write never lands");
}

#[gpui_kit::test]
fn a_write_failure_keeps_the_latest_journal(cx: &mut TestAppContext) {
    let session = Session::new();
    let output = session.dir.path().join("no-such-dir").join("out");
    let settings = WindowSettings { output: Some(output.clone()), ..session.settings() };
    let (window, view, finished) = open_with(cx, conflict(MergeOptions::default()), &settings);
    work(cx, window, &view);
    press(cx, window, "secondary-s");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Quit");
    cx.run_until_parked();
    assert_eq!(finished.codes(), [ExitCode::Error]);
    let journal = session.journal_for(&output).expect("written at once, without the wait");
    assert_eq!(journal.commands().len(), 2);
}

#[gpui_kit::test]
fn without_a_journal_directory_nothing_is_written(cx: &mut TestAppContext) {
    let session = Session::new();
    let settings = WindowSettings { journal_dir: None, ..session.settings() };
    let (window, view, _) = open_with(cx, conflict(MergeOptions::default()), &settings);
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    assert!(!session.dir.path().join("journal").exists());
}

#[gpui_kit::test]
fn later_writes_append_to_the_journal(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let first = std::fs::read(session.journal_path()).expect("written");
    // The caret is still after the typed "x", in region 0.
    press(cx, window, "secondary-1");
    wait(cx, JOURNAL_DEBOUNCE);
    let second = std::fs::read(session.journal_path()).expect("written");
    assert_eq!(&second[..first.len()], first.as_slice());
    assert_eq!(&second[first.len()..], b"{\"Toggle\":{\"region\":0,\"source\":{\"Side\":0}}}\n");
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_append_keeps_the_journal_file(cx: &mut TestAppContext) {
    use std::os::unix::fs::MetadataExt as _;
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    let inode =
        |session: &Session| std::fs::metadata(session.journal_path()).expect("written").ino();
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let first = inode(&session);
    press(cx, window, "secondary-1");
    wait(cx, JOURNAL_DEBOUNCE);
    assert_eq!(inode(&session), first, "appended in place, not replaced");
}

#[gpui_kit::test]
fn a_journal_removed_between_writes_is_written_whole(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    std::fs::remove_file(session.journal_path()).expect("removed");
    press(cx, window, "secondary-1");
    wait(cx, JOURNAL_DEBOUNCE);
    let journal = session.journal().expect("written again");
    assert_eq!(journal.commands().len(), 3);
}

#[gpui_kit::test]
fn a_restored_journal_with_a_torn_line_is_written_whole(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    // The crash cut the last write short.
    let mut torn = std::fs::read(session.journal_path()).expect("written");
    torn.extend_from_slice(b"{\"Clear\":{\"reg");
    std::fs::write(session.journal_path(), torn).expect("rewritten");
    let (again, _after, _) = session.open(cx, conflict(MergeOptions::default()));
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    press(cx, again, "secondary-alt-down");
    press(cx, again, "secondary-1");
    wait(cx, JOURNAL_DEBOUNCE);
    // Every line is whole: the file reads back and writes out as the same bytes.
    let bytes = std::fs::read(session.journal_path()).expect("written");
    let journal = Journal::parse(&bytes).expect("parses");
    assert_eq!(journal.to_lines().expect("serialises"), bytes);
    assert_eq!(journal.commands().len(), 3);
}

#[gpui_kit::test]
fn another_output_has_its_own_journal(cx: &mut TestAppContext) {
    let session = Session::new();
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let other = session.dir.path().join("other-out");
    let settings = WindowSettings { output: Some(other.clone()), ..session.settings() };
    let _ = open_with(cx, conflict(MergeOptions::default()), &settings);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "nothing to restore for this output");
    assert!(session.journal().is_some(), "the first session's journal stays");
    assert!(!session.journal_path_for(&other).exists());
}

/// Write `journal` where a session on the conflict looks for it, with `version`.
fn plant(session: &Session, journal: &Journal, version: u32) {
    let lines = String::from_utf8(journal.to_lines().expect("serialises")).expect("JSON is UTF-8");
    let planted = lines.replacen(
        &format!(r#"{{"version":{JOURNAL_VERSION},"#),
        &format!(r#"{{"version":{version},"#),
        1,
    );
    let path = session.journal_path();
    std::fs::create_dir_all(path.parent().expect("in the journal directory")).expect("created");
    std::fs::write(&path, planted).expect("written");
}

#[gpui_kit::test]
fn a_journal_of_another_format_offers_no_restore(cx: &mut TestAppContext) {
    let session = Session::new();
    let mut journal = Journal::new(&conflict(MergeOptions::default()));
    journal.record(Command::Clear { region: RegionIx::new(1) });
    plant(&session, &journal, JOURNAL_VERSION - 1);
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    assert!(!cx.has_pending_prompt(), "it could not be restored");
    // This session's first write replaces the file with the current format.
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let replaced = session.journal().expect("written");
    assert!(replaced.is_current());
    assert_eq!(replaced.commands().len(), 2);
}

#[gpui_kit::test]
fn a_restore_that_fails_says_so_and_the_session_goes_on(cx: &mut TestAppContext) {
    let session = Session::new();
    let mut journal = Journal::new(&conflict(MergeOptions::default()));
    journal.record(Command::Clear { region: RegionIx::new(99) });
    plant(&session, &journal, JOURNAL_VERSION);
    let (window, view, _) = session.open(cx, conflict(MergeOptions::default()));
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "an alert says the restore failed");
    cx.simulate_prompt_answer(OK);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    wait(cx, JOURNAL_DEBOUNCE);
    assert!(!session.journal_path().exists(), "the next open offers nothing");
    work(cx, window, &view);
    assert_eq!(output(cx, &view).0, "xa\nR\nc\n");
    wait(cx, JOURNAL_DEBOUNCE);
    let written = session.journal().expect("written");
    assert!(written.is_current());
    assert_eq!(written.commands().len(), 2, "only this session's commands");
}

#[gpui_kit::test]
fn opening_a_window_deletes_journals_past_their_age(cx: &mut TestAppContext) {
    const DAY: Duration = Duration::from_hours(24);
    let session = Session::new();
    let dir = session.dir.path().join("journal");
    std::fs::create_dir_all(&dir).expect("created");
    let now = std::time::SystemTime::now();
    let written_at = |name: &str, age: Duration| {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).expect("created");
        file.set_modified(now - age).expect("set");
        path
    };
    let old = written_at("old.jsonl", 31 * DAY);
    let recent = written_at("recent.jsonl", 29 * DAY);
    session.open(cx, conflict(MergeOptions::default()));
    assert!(!old.exists());
    assert!(recent.exists());
}

#[gpui_kit::test]
fn a_journal_name_survives_a_new_temporary_output(cx: &mut TestAppContext) {
    let session = Session::new();
    let named = |temp: &str| WindowSettings {
        output: Some(session.dir.path().join(temp).join("output")),
        journal_name: Some(PathBuf::from("/repo/a.txt")),
        ..session.settings()
    };
    let (window, view, _) =
        open_with(cx, conflict(MergeOptions::default()), &named("jj-resolve-A"));
    cx.run_until_parked();
    work(cx, window, &view);
    wait(cx, JOURNAL_DEBOUNCE);
    let expected = output(cx, &view);
    let (_, after, _) = open_with(cx, conflict(MergeOptions::default()), &named("jj-resolve-B"));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "a restore is offered");
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    assert_eq!(output(cx, &after), expected);
    let saves_to = cx.read(|cx| after.read(cx).output().map(Path::to_path_buf));
    assert_eq!(saves_to, named("jj-resolve-B").output, "this run's output");
}

#[gpui_kit::test]
fn a_journal_that_does_not_replay_leaves_the_session_as_opened(cx: &mut TestAppContext) {
    let session = Session::new();
    let mut journal = Journal::new(&conflict(MergeOptions::default()));
    journal.record(Command::Clear { region: RegionIx::new(99) });
    let path = session.journal_path();
    std::fs::create_dir_all(path.parent().expect("in the journal directory")).expect("created");
    std::fs::write(&path, journal.to_lines().expect("serialises")).expect("written");
    let (_, view, _) = session.open(cx, conflict(MergeOptions::default()));
    assert!(cx.has_pending_prompt(), "a restore is offered");
    cx.simulate_prompt_answer(RESTORE);
    cx.run_until_parked();
    let (core, editor) = output(cx, &view);
    assert_eq!(core, "a\nL\nR\nc\n");
    assert_eq!(editor, core);
    assert_eq!(
        cx.read(|cx| view.read(cx).document().resolutions()[1].clone()),
        Resolution::Unresolved
    );
}
