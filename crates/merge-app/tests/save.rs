//! Headless tests of Save & Exit, Cancel and the exit codes (spec §6.2; PLAN.md §A6.4).

use crate::common;

use common::{act, doc, open_saving, set_caret};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, TestAppContext, VisualTestContext};
use merge_app::ExitCode;
use merge_app::window::RESULT_ID;
use merge_core::document::{MergeDocument, Resolution};
use merge_core::merge3::{LEFT, Source};
use tempfile::TempDir;

const SAVE_WITH_CONFLICTS: &str = "Save with Conflicts";
const KEEP_EDITING: &str = "Keep Editing";
const DISCARD: &str = "Discard";
const QUIT: &str = "Quit";
const TRY_AGAIN: &str = "Try Again";
const SAVE_COPY_AS: &str = "Save Copy As…";
const CANNOT_WRITE: &str = "Cannot write the output";
const SAVE_CHANGES: &str = "Save your changes?";
const SAVE_AND_EXIT: &str = "Save & Exit";

/// Resolves without conflicts to `A b C`.
fn clean() -> MergeDocument {
    doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n")
}

/// One conflict at line 2.
fn conflict() -> MergeDocument {
    doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n")
}

const MARKERS: &str = "a\n<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n";

/// Conflicts at lines 2 and 4.
fn two_conflicts() -> MergeDocument {
    doc("a\nb\nc\nd\ne\n", "a\nL\nc\nM\ne\n", "a\nR\nc\nN\ne\n")
}

/// `two_conflicts` with the first taken from the left.
const SECOND_MARKERS: &str = "a\nL\nc\n<<<<<<<\nM\n|||||||\nd\n=======\nN\n>>>>>>>\ne\n";

/// Press `key`, then run what it started, a save's background write included.
fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
    cx.run_until_parked();
}

fn written(dir: &TempDir) -> Option<String> {
    std::fs::read_to_string(dir.path().join("out")).ok()
}

fn setup(
    cx: &mut TestAppContext,
    doc: MergeDocument,
) -> (TempDir, AnyWindowHandle, common::Finished) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (window, _view, finished) = open_saving(cx, doc, &dir.path().join("out"));
    (dir, window, finished)
}

#[gpui_kit::test]
fn save_when_resolved_writes_and_exits_0(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, clean());
    press(cx, window, "secondary-s");
    assert!(!cx.has_pending_prompt());
    assert_eq!(written(&dir).as_deref(), Some("A\nb\nC\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn save_with_unresolved_conflicts_asks_then_writes_markers_and_exits_1(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, conflict());
    press(cx, window, "secondary-s");
    assert!(cx.has_pending_prompt());
    assert_eq!(written(&dir), None, "nothing is written before the answer");
    cx.simulate_prompt_answer(SAVE_WITH_CONFLICTS);
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some(MARKERS));
    assert_eq!(finished.codes(), [ExitCode::Conflicts]);
}

#[gpui_kit::test]
fn without_the_confirmation_save_writes_markers_and_exits_1_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = merge_app::WindowSettings {
        output: Some(dir.path().join("out")),
        unresolved_save: merge_session::ending::UnresolvedSave::Write,
        ..merge_app::WindowSettings::default()
    };
    let (window, _view, finished) = common::open_with(cx, conflict(), &settings);
    press(cx, window, "secondary-s");
    assert!(!cx.has_pending_prompt());
    assert_eq!(written(&dir).as_deref(), Some(MARKERS));
    assert_eq!(finished.codes(), [ExitCode::Conflicts]);
}

/// Press `key` and let nothing run after it: a write in the background has not landed.
fn press_only(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
}

#[gpui_kit::test]
fn save_and_exit_writes_in_the_background_then_exits(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, clean());
    press_only(cx, window, "secondary-s");
    assert_eq!(written(&dir), None, "the write waits for the background executor");
    assert_eq!(finished.codes(), []);
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some("A\nb\nC\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn save_and_exit_while_a_save_is_writing_does_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (window, view, finished) = open_saving(cx, clean(), &dir.path().join("out"));
    press_only(cx, window, "secondary-s");
    // A second save would write the typed text.
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.input("x", cx));
    press_only(cx, window, "secondary-s");
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some("A\nb\nC\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

/// A conflict window that writes markers without asking.
fn setup_writing_markers(
    cx: &mut TestAppContext,
) -> (TempDir, AnyWindowHandle, gpui_kit::Entity<merge_app::MergeWindow>, common::Finished) {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = merge_app::WindowSettings {
        output: Some(dir.path().join("out")),
        unresolved_save: merge_session::ending::UnresolvedSave::Write,
        ..merge_app::WindowSettings::default()
    };
    let (window, view, finished) = common::open_with(cx, conflict(), &settings);
    (dir, window, view, finished)
}

#[gpui_kit::test]
fn a_pick_while_save_and_exit_writes_changes_nothing(cx: &mut TestAppContext) {
    let (dir, window, view, finished) = setup_writing_markers(cx);
    press_only(cx, window, "secondary-s");
    // Pick the left side of the conflict while the write runs.
    press_only(cx, window, "secondary-alt-down");
    press_only(cx, window, "secondary-1");
    let resolutions = cx.read(|cx| view.read(cx).document().resolutions().to_vec());
    assert_eq!(resolutions[1], Resolution::Unresolved, "the document stays as the save writes it");
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some(MARKERS));
    assert_eq!(finished.codes(), [ExitCode::Conflicts]);
}

#[gpui_kit::test]
fn an_undo_while_save_and_exit_writes_changes_nothing(cx: &mut TestAppContext) {
    let (dir, window, view, finished) = setup_writing_markers(cx);
    press(cx, window, "secondary-alt-down");
    press(cx, window, "secondary-1");
    press_only(cx, window, "secondary-s");
    press_only(cx, window, "secondary-z");
    let resolutions = cx.read(|cx| view.read(cx).document().resolutions().to_vec());
    assert_eq!(resolutions[1], Resolution::Pick(vec![Source::Side(LEFT)]));
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some("a\nL\nc\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn typing_while_save_and_exit_writes_leaves_the_result_as_saved(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (window, view, finished) = open_saving(cx, clean(), &dir.path().join("out"));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    press_only(cx, window, "secondary-s");
    act(cx, window, |window, cx| window.input("x", cx));
    assert_eq!(
        common::texts(cx, &view),
        ("A\nb\nC\n".to_owned(), "A\nb\nC\n".to_owned()),
        "the editor refuses the typing, and the core holds what the save writes"
    );
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some("A\nb\nC\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn after_a_failed_save_the_result_takes_edits_again(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("later").join("out");
    let (window, view, finished) = open_saving(cx, clean(), &output);
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.input("x", cx));
    cx.run_until_parked();
    assert_eq!(common::texts(cx, &view), ("xA\nb\nC\n".to_owned(), "xA\nb\nC\n".to_owned()));
    make_output_dir(&output);
    press(cx, window, "secondary-s");
    assert_eq!(std::fs::read_to_string(&output).expect("written"), "xA\nb\nC\n");
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn closing_while_a_save_is_writing_leaves_the_end_to_the_save(cx: &mut TestAppContext) {
    // Nothing changed, so a close would exit with 2 at once.
    let (dir, window, finished) = setup(cx, clean());
    press_only(cx, window, "secondary-s");
    press_only(cx, window, "secondary-w");
    assert!(!cx.has_pending_prompt());
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some("A\nb\nC\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn a_failed_background_save_lets_the_next_save_run(cx: &mut TestAppContext) {
    let (_dir, output, window, finished) = setup_failing(cx, clean());
    press(cx, window, "secondary-s");
    cx.run_until_parked();
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    make_output_dir(&output);
    press(cx, window, "secondary-s");
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&output).expect("written"), "A\nb\nC\n");
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[test]
fn the_confirmation_is_on_by_default() {
    assert_eq!(
        merge_app::WindowSettings::default().unresolved_save,
        merge_session::ending::UnresolvedSave::Confirm
    );
}

#[gpui_kit::test]
fn keep_editing_writes_nothing_and_stays_open(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, conflict());
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert_eq!(written(&dir), None);
    assert_eq!(finished.codes(), []);
    // A pick resolves the conflict, and the next save needs no answer.
    press(cx, window, "secondary-alt-down");
    press(cx, window, "secondary-1");
    press(cx, window, "secondary-s");
    assert_eq!(written(&dir).as_deref(), Some("a\nL\nc\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn save_writes_text_typed_just_before(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (window, view, finished) = open_saving(cx, clean(), &dir.path().join("out"));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    // In one update, so the edit still waits when the save runs.
    act(cx, window, |window, cx| {
        window.input("x", cx);
        window.press("secondary-s", cx);
    });
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some("xA\nb\nC\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn cancel_without_changes_exits_2_at_once(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, conflict());
    press(cx, window, "secondary-w");
    assert!(!cx.has_pending_prompt());
    assert_eq!(written(&dir), None);
    assert_eq!(finished.codes(), [ExitCode::Cancelled]);
}

#[gpui_kit::test]
fn cancel_after_a_change_asks_first(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, conflict());
    press(cx, window, "secondary-alt-down");
    press(cx, window, "secondary-1");
    press(cx, window, "secondary-w");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert_eq!(finished.codes(), []);
    press(cx, window, "secondary-q");
    cx.simulate_prompt_answer(DISCARD);
    cx.run_until_parked();
    assert_eq!(written(&dir), None);
    assert_eq!(finished.codes(), [ExitCode::Cancelled]);
}

#[gpui_kit::test]
fn closing_the_window_cancels(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, clean());
    let mut visual = VisualTestContext::from_window(window, cx);
    assert!(!visual.simulate_close(), "the window decides when it closes");
    assert_eq!(written(&dir), None);
    assert_eq!(finished.codes(), [ExitCode::Cancelled]);
}

/// Click the window's close button; true when the platform may close it at once.
fn close_button(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    VisualTestContext::from_window(window, cx).simulate_close()
}

#[gpui_kit::test]
fn closing_after_resolving_offers_save_and_exit_which_writes_and_exits_0(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, conflict());
    press(cx, window, "secondary-1");
    assert!(!close_button(cx, window), "the window decides when it closes");
    let (message, _) = cx.pending_prompt().expect("a prompt");
    assert_eq!(message, SAVE_CHANGES);
    assert_eq!(written(&dir), None, "nothing is written before the answer");
    cx.simulate_prompt_answer(SAVE_AND_EXIT);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "a resolved merge needs no second answer");
    assert_eq!(written(&dir).as_deref(), Some("a\nL\nc\n"));
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn save_and_exit_on_close_with_conflicts_left_confirms_then_writes_markers_and_exits_1(
    cx: &mut TestAppContext,
) {
    let (dir, window, finished) = setup(cx, two_conflicts());
    press(cx, window, "secondary-1");
    press(cx, window, "secondary-w");
    cx.simulate_prompt_answer(SAVE_AND_EXIT);
    cx.run_until_parked();
    let (message, _) = cx.pending_prompt().expect("the unresolved confirmation");
    assert_eq!(message, "Save with unresolved conflicts?");
    assert_eq!(written(&dir), None);
    cx.simulate_prompt_answer(SAVE_WITH_CONFLICTS);
    cx.run_until_parked();
    assert_eq!(written(&dir).as_deref(), Some(SECOND_MARKERS));
    assert_eq!(finished.codes(), [ExitCode::Conflicts]);
}

#[gpui_kit::test]
fn keep_editing_in_the_unresolved_confirmation_after_close_stays_open(cx: &mut TestAppContext) {
    let (dir, window, finished) = setup(cx, two_conflicts());
    press(cx, window, "secondary-1");
    press(cx, window, "secondary-q");
    cx.simulate_prompt_answer(SAVE_AND_EXIT);
    cx.run_until_parked();
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(written(&dir), None);
    assert_eq!(finished.codes(), []);
    // The window still holds the pick: closing asks again, and Discard ends it with 2.
    press(cx, window, "secondary-w");
    assert_eq!(cx.pending_prompt().expect("a prompt").0, SAVE_CHANGES);
    cx.simulate_prompt_answer(DISCARD);
    cx.run_until_parked();
    assert_eq!(written(&dir), None);
    assert_eq!(finished.codes(), [ExitCode::Cancelled]);
}

#[gpui_kit::test]
fn a_write_failure_alerts_and_exits_3(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("no-such-dir").join("out");
    let (window, _view, finished) = open_saving(cx, clean(), &output);
    press(cx, window, "secondary-s");
    let (message, detail) = cx.pending_prompt().expect("an alert");
    assert_eq!(message, "Cannot write the output");
    assert!(detail.starts_with(&format!("cannot write {}: ", output.display())), "{detail}");
    assert_eq!(finished.codes(), [], "the alert comes first");
    cx.simulate_prompt_answer(QUIT);
    cx.run_until_parked();
    assert!(!output.exists());
    assert_eq!(finished.codes(), [ExitCode::Error]);
}

/// A window whose output lies in a directory that does not exist yet, so a save fails until
/// [`make_output_dir`] makes it.
fn setup_failing(
    cx: &mut TestAppContext,
    doc: MergeDocument,
) -> (TempDir, std::path::PathBuf, AnyWindowHandle, common::Finished) {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("later").join("out");
    let (window, _view, finished) = open_saving(cx, doc, &output);
    (dir, output, window, finished)
}

fn make_output_dir(output: &std::path::Path) {
    std::fs::create_dir(output.parent().expect("a parent")).expect("the directory is made");
}

fn alert(cx: &mut TestAppContext) -> String {
    cx.pending_prompt().expect("an alert").0
}

#[gpui_kit::test]
fn try_again_after_the_cause_is_gone_writes_and_exits_0(cx: &mut TestAppContext) {
    let (_dir, output, window, finished) = setup_failing(cx, clean());
    press(cx, window, "secondary-s");
    assert_eq!(alert(cx), CANNOT_WRITE);
    make_output_dir(&output);
    cx.simulate_prompt_answer(TRY_AGAIN);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(std::fs::read_to_string(&output).expect("written"), "A\nb\nC\n");
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn try_again_that_fails_again_alerts_again_and_stays_open(cx: &mut TestAppContext) {
    let (_dir, output, window, finished) = setup_failing(cx, clean());
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(TRY_AGAIN);
    cx.run_until_parked();
    assert_eq!(alert(cx), CANNOT_WRITE);
    assert!(!output.exists());
    assert_eq!(finished.codes(), []);
}

#[gpui_kit::test]
fn try_again_with_conflicts_writes_markers_without_asking_twice(cx: &mut TestAppContext) {
    let (_dir, output, window, finished) = setup_failing(cx, conflict());
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(SAVE_WITH_CONFLICTS);
    cx.run_until_parked();
    assert_eq!(alert(cx), CANNOT_WRITE);
    make_output_dir(&output);
    cx.simulate_prompt_answer(TRY_AGAIN);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(std::fs::read_to_string(&output).expect("written"), MARKERS);
    assert_eq!(finished.codes(), [ExitCode::Conflicts]);
}

#[gpui_kit::test]
fn keep_editing_after_a_failure_stays_open_and_a_later_save_writes(cx: &mut TestAppContext) {
    let (_dir, output, window, finished) = setup_failing(cx, clean());
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(finished.codes(), []);
    make_output_dir(&output);
    press(cx, window, "secondary-s");
    assert_eq!(std::fs::read_to_string(&output).expect("written"), "A\nb\nC\n");
    assert_eq!(finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn save_copy_as_after_a_failure_writes_the_copy_and_stays_open(cx: &mut TestAppContext) {
    let (dir, output, window, finished) = setup_failing(cx, clean());
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(SAVE_COPY_AS);
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    let copy = dir.path().join("copy");
    cx.simulate_new_path_selection(|_| Some(copy.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&copy).expect("written"), "A\nb\nC\n");
    assert!(!output.exists());
    assert!(!cx.has_pending_prompt());
    assert_eq!(finished.codes(), []);
}
