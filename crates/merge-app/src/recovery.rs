//! The tasks that write the session's recovery journal, [`merge_session::recovery::Recovery`],
//! which [`merge_session::session::Session`] holds.

use std::io;

use gpui_kit::Task;
use merge_session::recovery::{Recovery, WriteMode, Written};

/// The tasks that write a journal.
#[derive(Default)]
pub(crate) struct JournalTasks {
    /// The write waiting for a pause. Replacing it cancels it.
    pub(crate) waiting: Option<Task<()>>,
    /// The write or removal running on a background thread. One runs at a time.
    pub(crate) running: Option<Task<()>>,
}

impl JournalTasks {
    /// Take the result of the running write into `journal`. Returns whether to start the
    /// next write now; a write waiting for a pause starts it itself.
    pub(crate) fn written(
        &mut self,
        journal: &mut Recovery,
        mode: WriteMode,
        upto: usize,
        result: io::Result<Written>,
    ) -> bool {
        self.running = None;
        journal.written(mode, upto, result) && self.waiting.is_none()
    }

    /// Take the result of the running removal into `journal`. Returns whether to start the
    /// next write now, as [`JournalTasks::written`] does.
    pub(crate) fn removed(&mut self, journal: &mut Recovery, result: io::Result<Written>) -> bool {
        self.running = None;
        journal.removed(result) && self.waiting.is_none()
    }

    /// Drop the tasks, which cancels them. A write already on its background thread runs
    /// to its end; the journal's gate waits for it. Call before the journal is flushed,
    /// discarded or restored.
    pub(crate) fn cancel(&mut self) {
        self.waiting = None;
        self.running = None;
    }
}

#[cfg(test)]
mod tests {
    use merge_core::document::{Command, Labels, MergeDocument, MergeOptions};
    use merge_core::index::RegionIx;
    use merge_core::journal::input_hashes;
    use merge_core::testing::three;

    use super::*;

    fn ready(dir: &std::path::Path) -> Recovery {
        let mut journal = Recovery::new(dir.to_owned(), None);
        journal.record(Command::Clear { region: RegionIx::new(0) });
        let inputs = three("b\n", "l\n", "r\n");
        let hashes = input_hashes(&inputs);
        let start = MergeDocument::new(inputs, MergeOptions::default(), Labels::none())
            .expect("no labels")
            .start();
        journal.hashed(hashes, start);
        journal
    }

    #[test]
    fn a_write_waiting_for_a_pause_starts_the_next_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut journal = ready(dir.path());
        let mut tasks = JournalTasks { waiting: Some(Task::ready(())), running: None };
        assert!(!tasks.written(&mut journal, WriteMode::Whole, 1, Ok(Written::Done)));
        assert!(journal.next_write().is_none());
    }

    #[test]
    fn without_a_waiting_write_a_lagging_file_starts_the_next_now() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut journal = ready(dir.path());
        journal.record(Command::Clear { region: RegionIx::new(1) });
        let mut tasks = JournalTasks { waiting: None, running: Some(Task::ready(())) };
        assert!(tasks.written(&mut journal, WriteMode::Whole, 1, Ok(Written::Done)));
        assert!(tasks.running.is_none());
    }

    #[test]
    fn a_removal_starts_the_next_write_unless_one_waits_for_a_pause() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut journal = ready(dir.path());
        let removal = journal.discard().expect("a named journal");
        let mut tasks = JournalTasks { waiting: None, running: Some(Task::ready(())) };
        assert!(tasks.removed(&mut journal, removal.run()));
        assert!(tasks.running.is_none());
        let removal = journal.discard().expect("a named journal");
        let mut tasks = JournalTasks { waiting: Some(Task::ready(())), running: None };
        assert!(!tasks.removed(&mut journal, removal.run()));
        assert!(journal.next_write().is_some(), "the waiting write carries it");
    }

    #[test]
    fn cancel_drops_both_tasks() {
        let mut tasks =
            JournalTasks { waiting: Some(Task::ready(())), running: Some(Task::ready(())) };
        tasks.cancel();
        assert!(tasks.waiting.is_none() && tasks.running.is_none());
    }
}
