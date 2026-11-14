//! How a merge session ends (spec §6.2): Save & Exit, Close, a save's outcome and the answers
//! to the alerts on the way. Each function takes what the window knows and gives the
//! [`Next`] step; the window asks, writes and finishes as that step says. No GUI.

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
            SaveError::Write(_) => Self::Write,
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
    /// Write the journal, then write the output and record the save.
    Write,
    /// Record again a save whose output is written with the code.
    Record(ExitCode),
    /// Ask for a path and write a copy there.
    SaveCopyAs,
    /// Write the journal, then say the save failed, why in `detail`, and offer
    /// [`Undone::answers`].
    Failed { undone: Undone, detail: String },
    /// End the session with `code`, the journal as `journal` says.
    Finish { code: ExitCode, journal: Journal },
}

/// Save & Exit, with `unresolved` conflicts left. Nothing while a save runs.
#[must_use]
pub fn save_requested(saving: bool, unresolved: usize, policy: UnresolvedSave) -> Next {
    match (saving, unresolved, policy) {
        (true, ..) => Next::Stay,
        (false, 0, _) | (false, _, UnresolvedSave::Write) => Next::Write,
        (false, unresolved, UnresolvedSave::Confirm) => Next::ConfirmUnresolved { unresolved },
    }
}

/// The answer to the unresolved-conflicts question: go ahead or not.
#[must_use]
pub fn unresolved_answered(go_ahead: bool) -> Next {
    if go_ahead { Next::Write } else { Next::Stay }
}

/// Close, of a document `changed` since it opened. Nothing while a save runs, since it ends
/// the session or fails and leaves the window open.
#[must_use]
pub fn close_requested(saving: bool, changed: bool) -> Next {
    match (saving, changed) {
        (true, _) => Next::Stay,
        (false, false) => discard(),
        (false, true) => Next::ConfirmClose,
    }
}

/// The answer to the close question; none when the alert closed with its window.
#[must_use]
pub fn close_answered(answer: Option<Closing>) -> Next {
    match answer {
        Some(Closing::SaveAndExit) => Next::SaveAndExit,
        Some(Closing::Discard) => discard(),
        Some(Closing::KeepEditing) | None => Next::Stay,
    }
}

fn discard() -> Next {
    Next::Finish { code: ExitCode::Cancelled, journal: Journal::Remove }
}

/// The outcome of a save. A resolved save removes the journal. A save with conflicts keeps
/// it, since `git mergetool` puts back its own file after one (spec §6.2). A failure keeps
/// the latest work in it and leaves the window open.
#[must_use]
pub fn saved(outcome: &Result<ExitCode, SaveError>) -> Next {
    match outcome {
        Ok(ExitCode::Resolved) => {
            Next::Finish { code: ExitCode::Resolved, journal: Journal::Remove }
        }
        Ok(code) => Next::Finish { code: *code, journal: Journal::Flush },
        Err(error) => Next::Failed { undone: Undone::of(error), detail: error.to_string() },
    }
}

/// The answer to the alert after a save that left `undone` undone; none when the alert
/// closed with its window. Quit, or no answer, ends the session with 3 and the journal kept.
#[must_use]
pub fn failure_answered(answer: Option<SaveFailed>, undone: Undone) -> Next {
    match (answer, undone) {
        (Some(SaveFailed::TryAgain), Undone::Write) => Next::Write,
        (Some(SaveFailed::TryAgain), Undone::Record(code)) => Next::Record(code),
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
        assert_eq!(save_requested(false, 0, UnresolvedSave::Confirm), Next::Write);
        assert_eq!(save_requested(false, 0, UnresolvedSave::Write), Next::Write);
        assert_eq!(save_requested(false, 3, UnresolvedSave::Write), Next::Write);
    }

    #[test]
    fn save_and_exit_with_conflicts_asks_first_and_names_their_count() {
        assert_eq!(
            save_requested(false, 1, UnresolvedSave::Confirm),
            Next::ConfirmUnresolved { unresolved: 1 }
        );
        assert_eq!(unresolved_answered(true), Next::Write);
        assert_eq!(unresolved_answered(false), Next::Stay);
    }

    #[test]
    fn nothing_starts_while_a_save_runs() {
        assert_eq!(save_requested(true, 0, UnresolvedSave::Write), Next::Stay);
        assert_eq!(save_requested(true, 2, UnresolvedSave::Confirm), Next::Stay);
        assert_eq!(close_requested(true, false), Next::Stay);
        assert_eq!(close_requested(true, true), Next::Stay);
    }

    #[test]
    fn closing_an_unchanged_merge_cancels_and_removes_the_journal() {
        let cancel = Next::Finish { code: ExitCode::Cancelled, journal: Journal::Remove };
        assert_eq!(close_requested(false, false), cancel);
        assert_eq!(close_requested(false, true), Next::ConfirmClose);
        assert_eq!(close_answered(Some(Closing::Discard)), cancel);
    }

    #[test]
    fn the_close_question_saves_or_keeps_editing() {
        assert_eq!(Closing::ALL[0], Closing::SaveAndExit, "the default keeps the work");
        assert_eq!(close_answered(Some(Closing::SaveAndExit)), Next::SaveAndExit);
        assert_eq!(close_answered(Some(Closing::KeepEditing)), Next::Stay);
        assert_eq!(close_answered(None), Next::Stay, "an alert gone with its window");
    }

    #[test]
    fn a_resolved_save_removes_the_journal_and_a_conflicted_one_keeps_it() {
        assert_eq!(
            saved(&Ok(ExitCode::Resolved)),
            Next::Finish { code: ExitCode::Resolved, journal: Journal::Remove }
        );
        assert_eq!(
            saved(&Ok(ExitCode::Conflicts)),
            Next::Finish { code: ExitCode::Conflicts, journal: Journal::Flush }
        );
    }

    #[test]
    fn a_failed_save_names_what_it_left_undone() {
        assert_eq!(
            saved(&Err(write_error())),
            Next::Failed {
                undone: Undone::Write,
                detail: "cannot write out.txt: permission denied".to_owned()
            }
        );
        assert_eq!(
            saved(&Err(record_error(ExitCode::Conflicts))),
            Next::Failed {
                undone: Undone::Record(ExitCode::Conflicts),
                detail:
                    "a.txt is written, but the repository did not record the save: jj has no index"
                        .to_owned()
            }
        );
    }

    #[test]
    fn a_failed_write_offers_a_copy_and_a_failed_record_does_not() {
        use SaveFailed::{KeepEditing, Quit, SaveCopyAs, TryAgain};
        assert_eq!(Undone::Write.answers(), [TryAgain, SaveCopyAs, KeepEditing, Quit]);
        assert_eq!(Undone::Record(ExitCode::Resolved).answers(), [TryAgain, KeepEditing, Quit]);
    }

    #[test]
    fn try_again_writes_again_or_records_again() {
        assert_eq!(failure_answered(Some(SaveFailed::TryAgain), Undone::Write), Next::Write);
        assert_eq!(
            failure_answered(Some(SaveFailed::TryAgain), Undone::Record(ExitCode::Resolved)),
            Next::Record(ExitCode::Resolved)
        );
    }

    #[test]
    fn the_other_answers_copy_keep_editing_or_quit_with_3() {
        assert_eq!(failure_answered(Some(SaveFailed::SaveCopyAs), Undone::Write), Next::SaveCopyAs);
        assert_eq!(failure_answered(Some(SaveFailed::KeepEditing), Undone::Write), Next::Stay);
        let record = Undone::Record(ExitCode::Conflicts);
        assert_eq!(failure_answered(Some(SaveFailed::Quit), record), FINISH_ERROR);
        assert_eq!(failure_answered(None, Undone::Write), FINISH_ERROR);
    }
}
