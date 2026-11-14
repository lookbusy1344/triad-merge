//! A merge window's session (spec §6.2): the document, its recovery journal, where a save
//! writes and records, and whether a save is running. The front end holds the tasks that run
//! a save and the journal's writes, and asks the session what each step does.

use std::path::{Path, PathBuf};

use merge_core::document::{Applied, Command, CommandError, MergeDocument, OutputEnding, Start};
use merge_core::journal::{Journal as JournalFile, input_hashes};
use merge_core::merge3::Inputs;

use crate::ending::{self, Journal, Next, UnresolvedSave};
use crate::recovery::Recovery;
use crate::resolve::Record;
use crate::{EncodedOutput, ExitCode, SaveError};
use vcs::VcsError;
use vcs_jj::Rewrite;

/// Why the session did not apply a command.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    /// Save & Exit is writing the merge as it stood when it started; a later change would
    /// be lost when the save ends the session.
    #[error("a save is writing the merge")]
    Saving,
    #[error(transparent)]
    Core(#[from] CommandError),
}

/// Why a save cannot start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CannotSave {
    #[error("the merge has no output to write")]
    NoOutput,
    #[error("a save is running")]
    Saving,
}

/// What a session saves to and how.
#[derive(Debug, Clone, Default)]
pub struct SaveTarget {
    /// Where Save & Exit writes; none for a merge that only saves copies.
    pub output: Option<PathBuf>,
    /// The VCS conflict each save to `output` is recorded in (spec §7.4).
    pub record: Option<Record>,
    pub line_ending: OutputEnding,
    pub unresolved_save: UnresolvedSave,
}

/// One merge, from open to its end.
pub struct Session {
    doc: MergeDocument,
    recovery: Option<Recovery>,
    target: SaveTarget,
    saving: bool,
}

/// A save's write and record, to run on a thread of its own. It holds no borrow of the
/// session.
pub struct SaveJob {
    work: SaveWork,
    /// The session's journal, written before the output. The session takes it back in
    /// [`Session::saved`]; it takes no commands while the save runs.
    journal: Option<JournalFlush>,
}

enum SaveWork {
    /// Write the output, then record it.
    Write { encoded: EncodedOutput, output: PathBuf, record: Option<Record> },
    /// Record again a save written with `code`, without writing it again.
    Record { record: Option<Record>, code: ExitCode },
}

/// What a flush of the journal needs: the inputs hash here if they are still hashing.
struct JournalFlush {
    recovery: Recovery,
    inputs: Inputs,
    start: Start,
}

/// A save's outcome, and the journal it carried.
pub struct Saved {
    outcome: Result<ExitCode, SaveError>,
    recovery: Option<Recovery>,
}

impl Saved {
    #[must_use]
    pub fn outcome(&self) -> &Result<ExitCode, SaveError> {
        &self.outcome
    }
}

impl SaveJob {
    /// Write the journal, then write and record, or record again.
    #[must_use]
    pub fn run(self) -> Saved {
        let recovery = self.journal.map(|JournalFlush { mut recovery, inputs, start }| {
            recovery.flush(|| input_hashes(&inputs), start);
            recovery
        });
        let outcome = match self.work {
            SaveWork::Write { encoded, output, record } => {
                encoded.save(&output, record.as_ref(), None)
            }
            SaveWork::Record { record, code } => {
                record.as_ref().map_or(Ok(code), |record| record.resave(code))
            }
        };
        Saved { outcome, recovery }
    }
}

/// A dry run of a save into a jj commit ([`Session::preview`]). It holds no borrow of the
/// session.
pub struct PreviewJob {
    encoded: EncodedOutput,
    record: Record,
}

impl PreviewJob {
    /// What the save would do, written nothing: the rewrite's report. None when the save
    /// would write nothing, as a pair's result with conflicts does.
    ///
    /// # Errors
    ///
    /// [`VcsError`] when the repository refuses the write.
    pub fn run(self) -> Result<Option<Rewrite>, VcsError> {
        let resolved = self.encoded.code() == ExitCode::Resolved;
        self.record.preview(self.encoded.bytes(), resolved)
    }
}

impl Session {
    /// A session on `doc` that saves to `target`, journalled in `recovery` when given.
    #[must_use]
    pub fn new(doc: MergeDocument, target: SaveTarget, recovery: Option<Recovery>) -> Self {
        Self { doc, recovery, target, saving: false }
    }

    #[must_use]
    pub fn document(&self) -> &MergeDocument {
        &self.doc
    }

    #[must_use]
    pub fn output(&self) -> Option<&Path> {
        self.target.output.as_deref()
    }

    #[must_use]
    pub fn line_ending(&self) -> OutputEnding {
        self.target.line_ending
    }

    /// Whether a save is writing the merge.
    #[must_use]
    pub fn saving(&self) -> bool {
        self.saving
    }

    /// Whether the session takes commands now.
    ///
    /// # Errors
    ///
    /// [`Refused::Saving`] while a save runs.
    pub fn accepts(&self) -> Result<(), Refused> {
        if self.saving { Err(Refused::Saving) } else { Ok(()) }
    }

    /// Apply `command` to the document, and record it in the journal when it applies.
    ///
    /// # Errors
    ///
    /// [`Refused::Saving`] while a save runs, and [`Refused::Core`] for a command the
    /// document refuses. The document and the journal are then unchanged.
    pub fn apply(&mut self, command: Command) -> Result<Applied, Refused> {
        self.accepts()?;
        // The journal keeps its own copy. An edit's batch is usually one keystroke.
        let recorded = self.recovery.is_some().then(|| command.clone());
        let applied = self.doc.apply(command)?;
        if let (Some(recovery), Some(command)) = (&mut self.recovery, recorded) {
            recovery.record(command);
        }
        Ok(applied)
    }

    /// Show `doc`, the replay of `journal`, in place of the session's document, and carry on
    /// the journal from it.
    ///
    /// # Errors
    ///
    /// [`Refused::Saving`] while a save runs, which writes the document as it stood. The
    /// session is then unchanged.
    pub fn restore(&mut self, doc: MergeDocument, journal: JournalFile) -> Result<(), Refused> {
        self.accepts()?;
        if let Some(recovery) = &mut self.recovery {
            recovery.restored(journal);
        }
        self.doc = doc;
        Ok(())
    }

    #[must_use]
    pub fn recovery(&self) -> Option<&Recovery> {
        self.recovery.as_ref()
    }

    pub fn recovery_mut(&mut self) -> Option<&mut Recovery> {
        self.recovery.as_mut()
    }

    /// Write the journal now, on this thread, hashing the inputs if they are still hashing.
    /// The front end cancels its journal tasks first.
    pub fn flush_journal(&mut self) {
        if let Some(recovery) = &mut self.recovery {
            let inputs = self.doc.inputs();
            recovery.flush(|| input_hashes(inputs), self.doc.start());
        }
    }

    /// Remove the journal, on this thread, after any write running, and keep none from here
    /// on. The front end cancels its journal tasks first.
    pub fn end_journal(&mut self) {
        let removal = self.recovery.take().and_then(|mut recovery| recovery.discard());
        if let Some(removal) = removal
            && let Err(error) = removal.run()
        {
            // The next session for these inputs offers it, and the owner can start over.
            tracing::warn!(%error, path = %removal.path.display(), "cannot remove the journal");
        }
    }

    /// Do what the end of a session asks of the journal.
    pub fn finish_journal(&mut self, journal: Journal) {
        match journal {
            Journal::Remove => self.end_journal(),
            Journal::Flush => self.flush_journal(),
            Journal::Keep => {}
        }
    }

    /// What Save & Exit does now (spec §6.2).
    #[must_use]
    pub fn save_requested(&self) -> Next {
        let pair = self.target.record.as_ref().and_then(Record::pair);
        ending::save_requested(
            self.saving,
            self.doc.unresolved(),
            self.target.unresolved_save,
            pair,
        )
    }

    /// What Close does now (spec §6.2).
    #[must_use]
    pub fn close_requested(&self) -> Next {
        ending::close_requested(self.saving, self.doc.changed())
    }

    /// Start Save & Exit, and lock the session until [`Session::saved`]. The job writes the
    /// journal before the output, so the journal holds the work while the save runs, and
    /// after it fails.
    ///
    /// # Errors
    ///
    /// [`CannotSave::NoOutput`] without an output, and [`CannotSave::Saving`] while a save
    /// runs. The session is then unchanged.
    pub fn start_save(&mut self) -> Result<SaveJob, CannotSave> {
        if self.saving {
            return Err(CannotSave::Saving);
        }
        let output = self.target.output.clone().ok_or(CannotSave::NoOutput)?;
        self.saving = true;
        let journal = self.recovery.take().map(|recovery| JournalFlush {
            recovery,
            // Texts share their content, so the copy is O(1).
            inputs: self.doc.inputs().clone(),
            start: self.doc.start(),
        });
        let work = SaveWork::Write {
            encoded: EncodedOutput::new(&self.doc, self.target.line_ending),
            output,
            record: self.target.record.clone(),
        };
        Ok(SaveJob { work, journal })
    }

    /// Start recording again a save whose output is written with `code`, and lock the
    /// session until [`Session::saved`].
    ///
    /// # Errors
    ///
    /// [`CannotSave::Saving`] while a save runs.
    pub fn start_record(&mut self, code: ExitCode) -> Result<SaveJob, CannotSave> {
        if self.saving {
            return Err(CannotSave::Saving);
        }
        self.saving = true;
        Ok(SaveJob {
            work: SaveWork::Record { record: self.target.record.clone(), code },
            journal: None,
        })
    }

    /// End a save, take back its journal, unlock the session, and give the next step.
    pub fn saved(&mut self, saved: Saved) -> Next {
        self.saving = false;
        if let Some(recovery) = saved.recovery {
            self.recovery = Some(recovery);
        }
        ending::saved(&saved.outcome)
    }

    /// What a save would write into the jj commit `resolve -r` opened, to run on a thread of
    /// its own: the save's output and its record. None for a merge whose saves go to no
    /// commit (spec §7.5.1).
    #[must_use]
    pub fn preview(&self) -> Option<PreviewJob> {
        let record = self.target.record.clone().filter(Record::in_commit)?;
        let encoded = EncodedOutput::new(&self.doc, self.target.line_ending);
        Some(PreviewJob { encoded, record })
    }

    /// The output as Save Copy As writes it, and the conflicts it holds as markers.
    #[must_use]
    pub fn copy(&self) -> (EncodedOutput, usize) {
        (EncodedOutput::new(&self.doc, self.target.line_ending), self.doc.unresolved())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use merge_core::document::Resolution;
    use merge_core::index::RegionIx;
    use merge_core::merge3::{LEFT, Source};
    use merge_core::testing::doc;

    use crate::recovery::WriteMode;

    const CONFLICT_REGION: RegionIx = RegionIx::new(1);
    const MARKERS: &str = "a\n<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n";

    /// One conflict at line 2.
    fn conflict() -> MergeDocument {
        doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n")
    }

    fn pick_left() -> Command {
        Command::Pick { region: CONFLICT_REGION, sources: vec![Source::Side(LEFT)] }
    }

    fn target(output: Option<PathBuf>, unresolved_save: UnresolvedSave) -> SaveTarget {
        SaveTarget { output, unresolved_save, ..SaveTarget::default() }
    }

    /// A session on [`conflict`] writing to `dir/out`, journalled in `dir/journal` with the
    /// inputs hashed.
    fn journalled(dir: &Path) -> Session {
        let mut recovery = Recovery::new(dir.join("journal"), Some(dir.join("out")));
        let doc = conflict();
        recovery.hashed(input_hashes(doc.inputs()), doc.start());
        Session::new(doc, target(Some(dir.join("out")), UnresolvedSave::Write), Some(recovery))
    }

    #[test]
    fn a_merge_saved_to_no_commit_has_no_preview() {
        let session = Session::new(conflict(), SaveTarget::default(), None);
        assert!(session.preview().is_none());
    }

    fn journal_commands(session: &Session) -> usize {
        let path = session.recovery().and_then(Recovery::path).expect("a named journal");
        JournalFile::parse(&std::fs::read(path).expect("written")).expect("parses").commands().len()
    }

    #[test]
    fn a_command_applies_and_is_journalled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.apply(pick_left()).expect("applies");
        assert_eq!(session.document().resolutions()[1], Resolution::Pick(vec![Source::Side(0)]));
        session.flush_journal();
        assert_eq!(journal_commands(&session), 1);
    }

    #[test]
    fn a_command_the_core_refuses_is_not_journalled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let missing = Command::Clear { region: RegionIx::new(9) };
        assert!(matches!(session.apply(missing), Err(Refused::Core(_))));
        session.apply(pick_left()).expect("applies");
        session.flush_journal();
        assert_eq!(journal_commands(&session), 1);
    }

    #[test]
    fn while_a_save_runs_every_command_is_refused_and_nothing_is_journalled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let _job = session.start_save().expect("starts");
        for command in [pick_left(), Command::Undo, Command::Clear { region: CONFLICT_REGION }] {
            assert!(matches!(session.apply(command), Err(Refused::Saving)));
        }
        assert!(matches!(session.accepts(), Err(Refused::Saving)));
        assert_eq!(session.document().resolutions()[1], Resolution::Unresolved);
        assert_eq!(session.recovery().and_then(Recovery::next_write).map(|(_, n)| n), None);
    }

    #[test]
    fn a_save_writes_the_merge_as_it_stood_when_it_started() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let job = session.start_save().expect("starts");
        assert_eq!(*job.run().outcome().as_ref().expect("writes"), ExitCode::Conflicts);
        assert_eq!(std::fs::read_to_string(dir.path().join("out")).expect("written"), MARKERS);
    }

    #[test]
    fn a_save_writes_the_journal_in_its_job_before_the_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        // The output cannot be written, so a journal on disk was written before it.
        session.target.output = Some(dir.path().join("none").join("out"));
        session.apply(pick_left()).expect("applies");
        let path = session.recovery().and_then(Recovery::path).expect("named").to_owned();
        let job = session.start_save().expect("starts");
        assert!(!path.exists(), "the UI thread writes nothing");
        let saved = job.run();
        assert!(saved.outcome().is_err());
        let file = JournalFile::parse(&std::fs::read(&path).expect("written")).expect("parses");
        assert_eq!(file.commands().len(), 1);
    }

    #[test]
    fn a_save_while_the_inputs_hash_hashes_them_in_its_job() {
        let dir = tempfile::tempdir().expect("tempdir");
        let recovery = Recovery::new(dir.path().join("journal"), Some(dir.path().join("out")));
        let target = target(Some(dir.path().join("out")), UnresolvedSave::Write);
        let mut session = Session::new(conflict(), target, Some(recovery));
        session.apply(pick_left()).expect("applies");
        let job = session.start_save().expect("starts");
        let saved = job.run();
        assert_eq!(*saved.outcome().as_ref().expect("writes"), ExitCode::Resolved);
        session.saved(saved);
        assert_eq!(journal_commands(&session), 1);
    }

    #[test]
    fn a_failed_save_unlocks_the_session_and_journals_on_from_its_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.target.output = Some(dir.path().join("none").join("out"));
        session.apply(pick_left()).expect("applies");
        let saved = session.start_save().expect("starts").run();
        let next = session.saved(saved);
        assert!(matches!(next, Next::Failed { undone: ending::Undone::Write, .. }), "{next:?}");
        assert!(!session.saving());
        session.apply(Command::Undo).expect("applies after the failure");
        let (write, upto) = session.recovery().and_then(Recovery::next_write).expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Append, 2));
    }

    #[test]
    fn a_record_job_leaves_the_journal_in_the_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.apply(pick_left()).expect("applies");
        let job = session.start_record(ExitCode::Resolved).expect("starts");
        assert_eq!(session.recovery().and_then(Recovery::next_write).map(|(_, n)| n), Some(1));
        let saved = job.run();
        assert_eq!(*saved.outcome().as_ref().expect("nothing to record"), ExitCode::Resolved);
    }

    /// A journal of `conflict` holding `pick_left`, and the document it replays to.
    fn picked_journal() -> (MergeDocument, JournalFile) {
        let mut doc = conflict();
        let mut journal = JournalFile::with_hashes(input_hashes(doc.inputs()), doc.start());
        doc.apply(pick_left()).expect("applies");
        journal.record(pick_left());
        (doc, journal)
    }

    #[test]
    fn a_restore_replaces_the_document_and_the_journal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let (doc, journal) = picked_journal();
        session.restore(doc, journal).expect("restores");
        assert_eq!(session.document().resolutions()[1], Resolution::Pick(vec![Source::Side(0)]));
        session.apply(Command::Undo).expect("applies");
        let (write, upto) = session.recovery().and_then(Recovery::next_write).expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 2));
    }

    #[test]
    fn a_restore_while_a_save_runs_is_refused_and_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.target.output = Some(dir.path().join("none").join("out"));
        session.apply(Command::Clear { region: CONFLICT_REGION }).expect("applies");
        let job = session.start_save().expect("starts");
        let (doc, journal) = picked_journal();
        assert!(matches!(session.restore(doc, journal), Err(Refused::Saving)));
        assert_eq!(session.document().resolutions()[1], Resolution::Pick(vec![]), "as cleared");
        assert!(matches!(session.saved(job.run()), Next::Failed { .. }));
        session.apply(Command::Undo).expect("applies after the failure");
        let (write, upto) = session.recovery().and_then(Recovery::next_write).expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Append, 2), "the journal the save wrote");
    }

    #[test]
    fn a_save_cannot_start_twice_or_without_an_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let _job = session.start_save().expect("starts");
        assert_eq!(session.start_save().map(drop), Err(CannotSave::Saving));
        assert_eq!(session.start_record(ExitCode::Resolved).map(drop), Err(CannotSave::Saving));
        let mut unsaved = Session::new(conflict(), SaveTarget::default(), None);
        assert_eq!(unsaved.start_save().map(drop), Err(CannotSave::NoOutput));
        assert!(!unsaved.saving(), "a save that cannot start leaves the session unlocked");
    }

    #[test]
    fn save_and_exit_asks_about_unresolved_conflicts_and_waits_for_a_running_save() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut confirm = Session::new(
            conflict(),
            target(Some(dir.path().join("out")), UnresolvedSave::Confirm),
            None,
        );
        assert_eq!(confirm.save_requested(), Next::ConfirmUnresolved { unresolved: 1 });
        confirm.apply(pick_left()).expect("applies");
        assert_eq!(confirm.save_requested(), Next::Write);
        let _job = confirm.start_save().expect("starts");
        assert_eq!(confirm.save_requested(), Next::Stay);
        assert_eq!(confirm.close_requested(), Next::Stay);
    }

    #[test]
    fn close_cancels_an_unchanged_merge_and_asks_about_a_changed_one() {
        let mut session = Session::new(conflict(), SaveTarget::default(), None);
        let cancel = Next::Finish { code: ExitCode::Cancelled, journal: Journal::Remove };
        assert_eq!(session.close_requested(), cancel);
        session.apply(pick_left()).expect("applies");
        assert_eq!(session.close_requested(), Next::ConfirmClose);
        session.apply(Command::Undo).expect("undoes");
        assert_eq!(session.close_requested(), cancel);
    }

    #[test]
    fn ending_the_journal_removes_its_file_and_journals_nothing_after() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.apply(pick_left()).expect("applies");
        session.flush_journal();
        let path = session.recovery().and_then(Recovery::path).expect("named").to_owned();
        session.finish_journal(Journal::Remove);
        assert!(!path.exists());
        assert!(session.recovery().is_none());
        session.apply(Command::Undo).expect("applies");
        assert!(!path.exists());
    }

    #[test]
    fn a_copy_holds_the_current_merge_and_counts_its_conflicts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let (copy, unresolved) = session.copy();
        assert_eq!(unresolved, 1);
        assert_eq!(copy.code(), ExitCode::Conflicts);
        session.apply(pick_left()).expect("applies");
        let (copy, unresolved) = session.copy();
        assert_eq!(unresolved, 0);
        let path = dir.path().join("copy");
        assert_eq!(copy.write(&path).expect("writes"), ExitCode::Resolved);
        assert_eq!(std::fs::read_to_string(path).expect("written"), "a\nL\nc\n");
    }
}
