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
    /// The write running on a background thread. One runs at a time.
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
    use merge_core::document::{Command, MergeOptions};
    use merge_core::index::RegionIx;
    use merge_core::journal::input_hashes;
    use merge_core::merge3::Inputs;
    use merge_core::text::Text;

    use super::*;

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes().to_vec()).expect("decodes")
    }

    fn ready(dir: &std::path::Path) -> Recovery {
        let mut journal = Recovery::new(dir.to_owned(), None);
        journal.record(Command::Clear { region: RegionIx::new(0) });
        let inputs = Inputs::three_way(text("b\n"), text("l\n"), text("r\n"));
        journal.hashed(input_hashes(&inputs), MergeOptions::default());
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
    fn cancel_drops_both_tasks() {
        let mut tasks =
            JournalTasks { waiting: Some(Task::ready(())), running: Some(Task::ready(())) };
        tasks.cancel();
        assert!(tasks.waiting.is_none() && tasks.running.is_none());
    }
}
