//! How a merge session saves and ends (spec §6.2): Save, Save & Exit, Close, a save's outcome
//! and the answers to the alerts on the way. Each function takes what the window knows and gives the
//! [`Next`] step; the window asks, writes and finishes as that step says. No GUI.

use std::sync::Arc;

use merge_core::text::EncodeError;

use crate::{ExitCode, SaveError};

/// What Save & Exit does while conflicts are unresolved (spec §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnresolvedSave {
    /// Ask first.
    #[default]
    Confirm,
    /// Write the markers at once.
    Write,
}

/// The `confirm-unresolved-save` setting: `true` is [`UnresolvedSave::Confirm`].
impl From<bool> for UnresolvedSave {
    fn from(confirm: bool) -> Self {
        if confirm { Self::Confirm } else { Self::Write }
    }
}

/// What follows a save that wrote and recorded the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    /// End the session with the save's code: Save & Exit.
    Exit,
    /// Go on merging: Save.
    Stay,
}

/// The answers when the window closes after a change, in button order. The first keeps the
/// work, so the default button never loses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    SaveAndExit,
    Discard,
    KeepEditing,
}

impl Closing {
    pub const ALL: [Self; 3] = [Self::SaveAndExit, Self::Discard, Self::KeepEditing];
}

/// What a failed Save & Exit left undone (spec §6.2, §7.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Undone {
    /// Nothing is written.
    Write,
    /// The output is written, ending the merge with the code; the repository has not
    /// recorded the save. Try Again records it without writing, and a copy has no use.
    Record(ExitCode),
}

impl Undone {
    /// What `error` left undone.
    #[must_use]
    pub fn of(error: &SaveError) -> Self {
        match error {
            SaveError::Encode(_) | SaveError::Write(_) | SaveError::Commit { .. } => Self::Write,
            SaveError::Record { code, .. } => Self::Record(*code),
        }
    }

    /// The answers the alert after the failure offers, in button order.
    #[must_use]
    pub fn answers(self) -> &'static [SaveFailed] {
        match self {
            Self::Write => &SaveFailed::ALL,
            Self::Record(_) => &SaveFailed::AFTER_WRITE,
        }
    }
}

/// The answers of the alert after a failed save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFailed {
    TryAgain,
    SaveCopyAs,
    KeepEditing,
    Quit,
}

impl SaveFailed {
    const ALL: [Self; 4] = [Self::TryAgain, Self::SaveCopyAs, Self::KeepEditing, Self::Quit];
    const AFTER_WRITE: [Self; 3] = [Self::TryAgain, Self::KeepEditing, Self::Quit];
}

/// What the journal does as the session ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Journal {
    /// Delete it: the work is saved, or the user discarded it.
    Remove,
    /// Write it at once and keep it for the next run.
    Flush,
    /// Leave it as it is: it holds the work already.
    Keep,
}

/// The window's next step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// Nothing: a save is running, or the user keeps editing.
    Stay,
    /// Ask before writing markers for `unresolved` conflicts.
    ConfirmUnresolved { unresolved: usize },
    /// Ask whether to save, discard or keep editing ([`Closing`]).
    ConfirmClose,
    /// Run Save & Exit, with its own checks.
    SaveAndExit,
    /// Write the journal, then write the output and record the save; then as [`After`] says.
    Write(After),
    /// Record again a save whose output is written with the code; then as [`After`] says.
    Record(ExitCode, After),
    /// Ask for a path and write a copy there.
    SaveCopyAs,
    /// Write the journal, then say the save failed with `error`, and offer
    /// [`Undone::answers`]. Try Again saves as `after` says.
    Failed { undone: Undone, error: SaveFault, after: After },
    /// Say why the output cannot be written: its encoding cannot hold a character of the
    /// result, as `error` says. Nothing is written, and the merge and its journal go on.
    Refused { error: EncodeError },
    /// Save wrote and recorded the output with the code, and the merge goes on.
    Saved(ExitCode),
    /// Save wrote the output with `code`, but the repository did not record it, as `error`
    /// says. The merge goes on; the next save or jj command records it.
    SavedUnrecorded { code: ExitCode, error: SaveFault },
    /// End the session with `code`, the journal as `journal` says.
    Finish { code: ExitCode, journal: Journal },
}

/// Save & Exit, with `unresolved` conflicts left. Nothing while a save runs.
#[must_use]
pub fn save_requested(saving: bool, unresolved: usize, policy: UnresolvedSave) -> Next {
    match (saving, unresolved, policy) {
        (true, ..) => Next::Stay,
        (false, 0, _) | (false, _, UnresolvedSave::Write) => Next::Write(After::Exit),
        (false, unresolved, UnresolvedSave::Confirm) => Next::ConfirmUnresolved { unresolved },
    }
}

/// Save, which writes unresolved conflicts as markers without asking, since the merge goes
/// on. Without an output it asks for a path, as Save Copy As does. Nothing while a save runs.
#[must_use]
pub fn save_in_place_requested(saving: bool, has_output: bool) -> Next {
    match (saving, has_output) {
        (true, _) => Next::Stay,
        (false, true) => Next::Write(After::Stay),
        (false, false) => Next::SaveCopyAs,
    }
}

/// The answer to the unresolved-conflicts question: go ahead or not.
#[must_use]
pub fn unresolved_answered(go_ahead: bool) -> Next {
    if go_ahead { Next::Write(After::Exit) } else { Next::Stay }
}

/// Close, of a document `changed` since the last Save, or since it opened before any.
/// `last_saved` is the code of the last Save; none before any. Nothing while a save runs,
/// since it ends the session or fails and leaves the window open.
#[must_use]
pub fn close_requested(saving: bool, changed: bool, last_saved: Option<ExitCode>) -> Next {
    match (saving, changed, last_saved) {
        (true, ..) => Next::Stay,
        (false, true, _) => Next::ConfirmClose,
        (false, false, None) => discard(None),
        (false, false, Some(code)) => finished(code),
    }
}

/// The answer to the close question; none when the alert closed with its window.
/// `last_saved` is the code of the last Save, which Discard ends with.
#[must_use]
pub fn close_answered(answer: Option<Closing>, last_saved: Option<ExitCode>) -> Next {
    match answer {
        Some(Closing::SaveAndExit) => Next::SaveAndExit,
        Some(Closing::Discard) => discard(last_saved),
        Some(Closing::KeepEditing) | None => Next::Stay,
    }
}

/// End without writing: with 2 before any Save, else with the last Save's code, since the
/// output holds what it wrote.
fn discard(last_saved: Option<ExitCode>) -> Next {
    Next::Finish { code: last_saved.unwrap_or(ExitCode::Cancelled), journal: Journal::Remove }
}

/// End after a save written with `code`. A resolved save removes the journal. A save with
/// conflicts keeps it, since `git mergetool` puts back its own file after one (spec §6.2).
fn finished(code: ExitCode) -> Next {
    let journal = if code == ExitCode::Resolved { Journal::Remove } else { Journal::Flush };
    Next::Finish { code, journal }
}

/// A failed save's error, shared with the window that names it. Two are equal when they
/// read the same.
#[derive(Debug, Clone)]
pub struct SaveFault(Arc<SaveError>);

impl SaveFault {
    #[must_use]
    pub fn error(&self) -> &SaveError {
        &self.0
    }
}

impl PartialEq for SaveFault {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_string() == other.0.to_string()
    }
}

impl Eq for SaveFault {}

/// The outcome of a save, which `after` follows. Save & Exit ends the session as
/// [`finished`] says; Save goes on with the journal, and a record the repository refused only
/// says so. A failure keeps the latest work in the journal and leaves the window open.
#[must_use]
pub fn saved(outcome: Result<ExitCode, SaveError>, after: After) -> Next {
    match (outcome, after) {
        (Ok(code), After::Exit) => finished(code),
        (Ok(code), After::Stay) => Next::Saved(code),
        (Err(error @ SaveError::Record { code, .. }), After::Stay) => {
            Next::SavedUnrecorded { code, error: SaveFault(Arc::new(error)) }
        }
        (Err(SaveError::Encode(error)), _) => Next::Refused { error },
        (Err(error), _) => {
            Next::Failed { undone: Undone::of(&error), error: SaveFault(Arc::new(error)), after }
        }
    }
}

/// The answer to the alert after a save that left `undone` undone and was to go on as
/// `after` says; none when the alert closed with its window. Quit, or no answer, ends the session with 3 and the journal kept.
#[must_use]
pub fn failure_answered(answer: Option<SaveFailed>, undone: Undone, after: After) -> Next {
    match (answer, undone) {
        (Some(SaveFailed::TryAgain), Undone::Write) => Next::Write(after),
        (Some(SaveFailed::TryAgain), Undone::Record(code)) => Next::Record(code, after),
        (Some(SaveFailed::SaveCopyAs), _) => Next::SaveCopyAs,
        (Some(SaveFailed::KeepEditing), _) => Next::Stay,
        (Some(SaveFailed::Quit) | None, _) => {
            Next::Finish { code: ExitCode::Error, journal: Journal::Keep }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::WriteError;
    use merge_core::text::{EncodeError, Encoding};
    use vcs::VcsError;

    const FINISH_ERROR: Next = Next::Finish { code: ExitCode::Error, journal: Journal::Keep };

    fn write_error() -> SaveError {
        SaveError::Write(WriteError {
            path: "out.txt".into(),
            source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        })
    }

    fn record_error(code: ExitCode) -> SaveError {
        SaveError::Record { path: "a.txt".into(), code, source: VcsError::NoIndex("jj") }
    }

    #[test]
    fn save_and_exit_writes_at_once_without_conflicts_or_when_set_to() {
        let write = Next::Write(After::Exit);
        assert_eq!(save_requested(false, 0, UnresolvedSave::Confirm), write);
        assert_eq!(save_requested(false, 0, UnresolvedSave::Write), write);
        assert_eq!(save_requested(false, 3, UnresolvedSave::Write), write);
    }

    #[test]
    fn save_and_exit_with_conflicts_asks_first_and_names_their_count() {
        assert_eq!(
            save_requested(false, 1, UnresolvedSave::Confirm),
            Next::ConfirmUnresolved { unresolved: 1 }
        );
        assert_eq!(unresolved_answered(true), Next::Write(After::Exit));
        assert_eq!(unresolved_answered(false), Next::Stay);
    }

    #[test]
    fn nothing_starts_while_a_save_runs() {
        assert_eq!(save_requested(true, 0, UnresolvedSave::Write), Next::Stay);
        assert_eq!(save_requested(true, 2, UnresolvedSave::Confirm), Next::Stay);
        assert_eq!(close_requested(true, false, None), Next::Stay);
        assert_eq!(close_requested(true, true, None), Next::Stay);
        assert_eq!(close_requested(true, false, Some(ExitCode::Resolved)), Next::Stay);
        assert_eq!(save_in_place_requested(true, true), Next::Stay);
    }

    #[test]
    fn save_writes_and_stays_without_asking_about_conflicts() {
        assert_eq!(save_in_place_requested(false, true), Next::Write(After::Stay));
    }

    #[test]
    fn save_without_an_output_asks_for_a_path_as_save_copy_as_does() {
        assert_eq!(save_in_place_requested(false, false), Next::SaveCopyAs);
    }

    #[test]
    fn a_save_that_stays_reports_its_code_and_keeps_the_journal_going() {
        assert_eq!(saved(Ok(ExitCode::Resolved), After::Stay), Next::Saved(ExitCode::Resolved));
        assert_eq!(saved(Ok(ExitCode::Conflicts), After::Stay), Next::Saved(ExitCode::Conflicts));
    }

    #[test]
    fn a_save_that_stays_and_the_repository_did_not_record_says_so_without_an_alert() {
        let next = saved(Err(record_error(ExitCode::Conflicts)), After::Stay);
        let Next::SavedUnrecorded { code, error } = next else {
            panic!("not an unrecorded save: {next:?}");
        };
        assert_eq!(code, ExitCode::Conflicts);
        assert_eq!(
            error.error().to_string(),
            "a.txt is written, but the repository did not record the save: jj has no index"
        );
    }

    #[test]
    fn a_failed_save_that_stays_tries_again_without_exiting() {
        let Next::Failed { undone, after, .. } = saved(Err(write_error()), After::Stay) else {
            panic!("not a failure");
        };
        assert_eq!((undone, after), (Undone::Write, After::Stay));
        assert_eq!(
            failure_answered(Some(SaveFailed::TryAgain), Undone::Write, After::Stay),
            Next::Write(After::Stay)
        );
        assert_eq!(
            failure_answered(
                Some(SaveFailed::TryAgain),
                Undone::Record(ExitCode::Conflicts),
                After::Stay
            ),
            Next::Record(ExitCode::Conflicts, After::Stay)
        );
    }

    #[test]
    fn closing_unchanged_since_a_save_ends_with_that_saves_code() {
        assert_eq!(
            close_requested(false, false, Some(ExitCode::Resolved)),
            Next::Finish { code: ExitCode::Resolved, journal: Journal::Remove }
        );
        assert_eq!(
            close_requested(false, false, Some(ExitCode::Conflicts)),
            Next::Finish { code: ExitCode::Conflicts, journal: Journal::Flush }
        );
        assert_eq!(close_requested(false, true, Some(ExitCode::Resolved)), Next::ConfirmClose);
    }

    #[test]
    fn discard_after_a_save_keeps_what_the_save_wrote() {
        assert_eq!(
            close_answered(Some(Closing::Discard), Some(ExitCode::Conflicts)),
            Next::Finish { code: ExitCode::Conflicts, journal: Journal::Remove }
        );
        assert_eq!(
            close_answered(Some(Closing::Discard), Some(ExitCode::Resolved)),
            Next::Finish { code: ExitCode::Resolved, journal: Journal::Remove }
        );
    }

    #[test]
    fn closing_an_unchanged_merge_cancels_and_removes_the_journal() {
        let cancel = Next::Finish { code: ExitCode::Cancelled, journal: Journal::Remove };
        assert_eq!(close_requested(false, false, None), cancel);
        assert_eq!(close_requested(false, true, None), Next::ConfirmClose);
        assert_eq!(close_answered(Some(Closing::Discard), None), cancel);
    }

    #[test]
    fn the_close_question_saves_or_keeps_editing() {
        assert_eq!(Closing::ALL[0], Closing::SaveAndExit, "the default keeps the work");
        assert_eq!(close_answered(Some(Closing::SaveAndExit), None), Next::SaveAndExit);
        assert_eq!(close_answered(Some(Closing::KeepEditing), None), Next::Stay);
        assert_eq!(close_answered(None, None), Next::Stay, "an alert gone with its window");
    }

    #[test]
    fn a_resolved_save_removes_the_journal_and_a_conflicted_one_keeps_it() {
        assert_eq!(
            saved(Ok(ExitCode::Resolved), After::Exit),
            Next::Finish { code: ExitCode::Resolved, journal: Journal::Remove }
        );
        assert_eq!(
            saved(Ok(ExitCode::Conflicts), After::Exit),
            Next::Finish { code: ExitCode::Conflicts, journal: Journal::Flush }
        );
    }

    #[test]
    fn a_failed_save_names_what_it_left_undone() {
        let failed = |next| match next {
            Next::Failed { undone, error, after: After::Exit } => {
                (undone, error.error().to_string())
            }
            other => panic!("not a failure: {other:?}"),
        };
        assert_eq!(
            failed(saved(Err(write_error()), After::Exit)),
            (Undone::Write, "cannot write out.txt: permission denied".to_owned())
        );
        assert_eq!(
            failed(saved(Err(record_error(ExitCode::Conflicts)), After::Exit)),
            (
                Undone::Record(ExitCode::Conflicts),
                "a.txt is written, but the repository did not record the save: jj has no index"
                    .to_owned()
            )
        );
    }

    #[test]
    fn an_output_the_encoding_cannot_hold_refuses_the_save_with_the_reason() {
        let error = EncodeError { encoding: Encoding::Windows1252, character: 'ł', line: 3 };
        assert_eq!(saved(Err(SaveError::Encode(error)), After::Exit), Next::Refused { error });
        assert_eq!(saved(Err(SaveError::Encode(error)), After::Stay), Next::Refused { error });
    }

    #[test]
    fn a_failed_write_offers_a_copy_and_a_failed_record_does_not() {
        use SaveFailed::{KeepEditing, Quit, SaveCopyAs, TryAgain};
        assert_eq!(Undone::Write.answers(), [TryAgain, SaveCopyAs, KeepEditing, Quit]);
        assert_eq!(Undone::Record(ExitCode::Resolved).answers(), [TryAgain, KeepEditing, Quit]);
    }

    #[test]
    fn try_again_writes_again_or_records_again() {
        assert_eq!(
            failure_answered(Some(SaveFailed::TryAgain), Undone::Write, After::Exit),
            Next::Write(After::Exit)
        );
        assert_eq!(
            failure_answered(
                Some(SaveFailed::TryAgain),
                Undone::Record(ExitCode::Resolved),
                After::Exit
            ),
            Next::Record(ExitCode::Resolved, After::Exit)
        );
    }

    #[test]
    fn the_other_answers_copy_keep_editing_or_quit_with_3() {
        let exit = After::Exit;
        assert_eq!(
            failure_answered(Some(SaveFailed::SaveCopyAs), Undone::Write, exit),
            Next::SaveCopyAs
        );
        assert_eq!(
            failure_answered(Some(SaveFailed::KeepEditing), Undone::Write, exit),
            Next::Stay
        );
        let record = Undone::Record(ExitCode::Conflicts);
        assert_eq!(failure_answered(Some(SaveFailed::Quit), record, exit), FINISH_ERROR);
        assert_eq!(failure_answered(None, Undone::Write, After::Stay), FINISH_ERROR);
    }
}
