//! A merge window's session (spec §6.2): the document, its recovery journal, where a save
//! writes and records, and whether a save is running. The front end holds the tasks that run
//! a save and the journal's writes, and asks the session what each step does.

use std::path::Path;

use merge_core::document::{Applied, Command, CommandError, MergeDocument, OutputEnding, Start};
use merge_core::journal::{Journal as JournalFile, input_hashes};
use merge_core::merge3::Inputs;
use merge_core::text::EncodeError;

use crate::ending::{self, After, Closing, Journal, Next, UnresolvedSave};
use crate::recovery::Recovery;
use crate::resolve::{Destination, InCommit, Record};
use crate::{EncodedOutput, ExitCode, SaveError};
use vcs::VcsError;
use vcs_jj::Rewrite;

/// Why the session did not apply a command.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    /// A save is writing the merge as it stood when it started; a later change would be
    /// lost when the save ends the session, or missing from the journal it carries.
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

/// Why a preview of a save into a jj commit gave no report.
#[derive(Debug, thiserror::Error)]
pub enum PreviewError {
    /// The output encoding cannot hold a character of the result.
    #[error(transparent)]
    Encode(#[from] EncodeError),
    #[error(transparent)]
    Vcs(#[from] VcsError),
}

/// The output of the document as it stood, to build and encode on a thread of its own: at
/// 1M lines that takes 18 ms, 52 ms with line endings made. It holds no borrow of the
/// session.
pub struct PendingOutput {
    /// Boxed: a document is 1 KB, and the jobs that carry it move it between threads.
    doc: Box<MergeDocument>,
    ending: OutputEnding,
}

impl PendingOutput {
    /// Build and encode the output.
    ///
    /// # Errors
    ///
    /// [`EncodeError`] when the output encoding cannot hold a character of the result.
    pub fn encode(&self) -> Result<EncodedOutput, EncodeError> {
        EncodedOutput::new(&self.doc, self.ending)
    }

    /// Build, encode and write the output to `path` atomically, and nothing else.
    ///
    /// # Errors
    ///
    /// [`SaveError::Encode`] when the output encoding cannot hold the result, and
    /// [`SaveError::Write`] when `path` cannot be written. `path` is then unchanged.
    pub fn write(&self, path: &Path) -> Result<ExitCode, SaveError> {
        Ok(self.encode()?.write(path)?)
    }
}

/// What a session saves to and how.
#[derive(Debug, Clone, Default)]
pub struct SaveTarget {
    /// Where Save & Exit saves; none for a merge that only saves copies.
    pub destination: Option<Destination>,
    pub line_ending: OutputEnding,
    pub unresolved_save: UnresolvedSave,
}

/// One merge, from open to its end.
pub struct Session {
    doc: MergeDocument,
    recovery: Option<Recovery>,
    target: SaveTarget,
    saving: bool,
    /// Commands applied and restores, which mark a change since the last Save.
    changes: u64,
    /// The last Save that wrote and recorded the output; none before any.
    last_saved: Option<LastSaved>,
}

/// A Save that wrote and recorded the output.
#[derive(Debug, Clone, Copy)]
struct LastSaved {
    code: ExitCode,
    /// [`Session::changes`] when it started.
    changes: u64,
}

/// A save's write and record, to run on a thread of its own. It holds no borrow of the
/// session.
pub struct SaveJob {
    work: SaveWork,
    after: After,
    /// The session's journal, written before the output. The session takes it back in
    /// [`Session::saved`]; it takes no commands while the save runs.
    journal: Option<JournalFlush>,
}

enum SaveWork {
    /// Build and write the output, then record it.
    Write { pending: PendingOutput, destination: Destination },
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
    after: After,
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
            SaveWork::Write { pending, destination } => pending
                .encode()
                .map_err(SaveError::from)
                .and_then(|encoded| encoded.save(&destination, None)),
            SaveWork::Record { record, code } => {
                record.as_ref().map_or(Ok(code), |record| record.resave(code))
            }
        };
        Saved { outcome, after: self.after, recovery }
    }
}

/// A dry run of a save into a jj commit ([`Session::preview`]). It holds no borrow of the
/// session.
pub struct PreviewJob {
    pending: PendingOutput,
    commit: InCommit,
}

impl PreviewJob {
    /// What the save would do, written nothing: the rewrite's report.
    ///
    /// # Errors
    ///
    /// [`PreviewError::Encode`] when the output encoding cannot hold the result, and
    /// [`PreviewError::Vcs`] when the repository refuses the write.
    pub fn run(self) -> Result<Rewrite, PreviewError> {
        const DRY_RUN: bool = true;
        let encoded = self.pending.encode()?;
        Ok(self.commit.write(encoded.bytes(), DRY_RUN)?)
    }
}

impl Session {
    /// A session on `doc` that saves to `target`, journalled in `recovery` when given.
    #[must_use]
    pub fn new(doc: MergeDocument, target: SaveTarget, recovery: Option<Recovery>) -> Self {
        Self { doc, recovery, target, saving: false, changes: 0, last_saved: None }
    }

    #[must_use]
    pub fn document(&self) -> &MergeDocument {
        &self.doc
    }

    /// The file a save writes; for a jj commit's conflict, the temporary file the window
    /// names.
    #[must_use]
    pub fn output(&self) -> Option<&Path> {
        self.target.destination.as_ref().map(Destination::path)
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
        self.changes += 1;
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
        self.changes += 1;
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
        ending::save_requested(self.saving, self.doc.unresolved(), self.target.unresolved_save)
    }

    /// What Save does now (spec §6.2).
    #[must_use]
    pub fn save_in_place_requested(&self) -> Next {
        ending::save_in_place_requested(self.saving, self.target.destination.is_some())
    }

    /// What Close does now (spec §6.2): before any Save, whether the merge changed from its
    /// automatic resolution; after one, whether any command applied since.
    #[must_use]
    pub fn close_requested(&self) -> Next {
        let changed = match self.last_saved {
            Some(last) => last.changes != self.changes,
            None => self.doc.changed(),
        };
        ending::close_requested(self.saving, changed, self.last_saved_code())
    }

    /// What the answer to the close question does now (spec §6.2).
    #[must_use]
    pub fn close_answered(&self, answer: Option<Closing>) -> Next {
        ending::close_answered(answer, self.last_saved_code())
    }

    fn last_saved_code(&self) -> Option<ExitCode> {
        self.last_saved.map(|last| last.code)
    }

    /// Start a save that goes on as `after` says, and lock the session until
    /// [`Session::saved`]. The job writes the journal before the output, so the journal
    /// holds the work while the save runs, and after it fails.
    ///
    /// # Errors
    ///
    /// [`CannotSave::NoOutput`] without an output, and [`CannotSave::Saving`] while a save
    /// runs. The session is then unchanged. An output the encoding cannot hold fails in the
    /// job, with [`SaveError::Encode`].
    pub fn start_save(&mut self, after: After) -> Result<SaveJob, CannotSave> {
        if self.saving {
            return Err(CannotSave::Saving);
        }
        let destination = self.target.destination.clone().ok_or(CannotSave::NoOutput)?;
        let pending = self.pending_output();
        self.saving = true;
        let journal = self.recovery.take().map(|recovery| JournalFlush {
            recovery,
            // Texts share their content, so the copy is O(1).
            inputs: self.doc.inputs().clone(),
            start: self.doc.start(),
        });
        let work = SaveWork::Write { pending, destination };
        Ok(SaveJob { work, after, journal })
    }

    /// Start recording again a save whose output is written with `code`, which goes on as
    /// `after` says, and lock the session until [`Session::saved`].
    ///
    /// # Errors
    ///
    /// [`CannotSave::Saving`] while a save runs.
    pub fn start_record(&mut self, code: ExitCode, after: After) -> Result<SaveJob, CannotSave> {
        if self.saving {
            return Err(CannotSave::Saving);
        }
        self.saving = true;
        Ok(SaveJob {
            work: SaveWork::Record { record: self.record().cloned(), code },
            after,
            journal: None,
        })
    }

    /// End a save, take back its journal, unlock the session, and give the next step. A
    /// Save that wrote and recorded the output is the one Close and Discard end with.
    pub fn saved(&mut self, saved: Saved) -> Next {
        self.saving = false;
        if let Some(recovery) = saved.recovery {
            self.recovery = Some(recovery);
        }
        // A save whose record failed still wrote the output. The session took no command
        // while the save ran.
        if let Ok(&code) | Err(&SaveError::Record { code, .. }) = saved.outcome.as_ref() {
            self.last_saved = Some(LastSaved { code, changes: self.changes });
        }
        ending::saved(saved.outcome, saved.after)
    }

    /// What a save would write into the jj commit `resolve -r` opened, to run on a thread of
    /// its own: the save's output and its record. None for a merge whose saves go to no
    /// commit (spec §7.5.1).
    #[must_use]
    pub fn preview(&self) -> Option<PreviewJob> {
        let commit = self.target.destination.as_ref().and_then(Destination::in_commit)?;
        Some(PreviewJob { pending: self.pending_output(), commit: commit.clone() })
    }

    /// The output as Save Copy As writes it, and the conflicts it holds as markers.
    #[must_use]
    pub fn copy(&self) -> (PendingOutput, usize) {
        (self.pending_output(), self.doc.unresolved())
    }

    /// The VCS conflict a save to the file is recorded in.
    fn record(&self) -> Option<&Record> {
        self.target.destination.as_ref().and_then(Destination::record)
    }

    /// The output as a save writes it now, to build off the UI thread.
    fn pending_output(&self) -> PendingOutput {
        PendingOutput { doc: Box::new(self.doc.snapshot()), ending: self.target.line_ending }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use merge_core::document::Resolution;
    use merge_core::index::RegionIx;
    use merge_core::merge3::{LEFT, Source};
    use merge_core::testing::doc;

    use crate::ending::After;
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
        let destination = output.map(|path| Destination::File { path, record: None });
        SaveTarget { destination, unresolved_save, ..SaveTarget::default() }
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
        let _job = session.start_save(After::Exit).expect("starts");
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
        let job = session.start_save(After::Exit).expect("starts");
        assert_eq!(*job.run().outcome().as_ref().expect("writes"), ExitCode::Conflicts);
        assert_eq!(std::fs::read_to_string(dir.path().join("out")).expect("written"), MARKERS);
    }

    #[test]
    fn a_save_writes_the_journal_in_its_job_before_the_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        // The output cannot be written, so a journal on disk was written before it.
        session.target.destination = Some(Destination::file(dir.path().join("none").join("out")));
        session.apply(pick_left()).expect("applies");
        let path = session.recovery().and_then(Recovery::path).expect("named").to_owned();
        let job = session.start_save(After::Exit).expect("starts");
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
        let job = session.start_save(After::Exit).expect("starts");
        let saved = job.run();
        assert_eq!(*saved.outcome().as_ref().expect("writes"), ExitCode::Resolved);
        session.saved(saved);
        assert_eq!(journal_commands(&session), 1);
    }

    #[test]
    fn a_failed_save_unlocks_the_session_and_journals_on_from_its_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.target.destination = Some(Destination::file(dir.path().join("none").join("out")));
        session.apply(pick_left()).expect("applies");
        let saved = session.start_save(After::Exit).expect("starts").run();
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
        let job = session.start_record(ExitCode::Resolved, After::Exit).expect("starts");
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
        session.target.destination = Some(Destination::file(dir.path().join("none").join("out")));
        session.apply(Command::Clear { region: CONFLICT_REGION }).expect("applies");
        let job = session.start_save(After::Exit).expect("starts");
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
        let _job = session.start_save(After::Exit).expect("starts");
        assert_eq!(session.start_save(After::Exit).map(drop), Err(CannotSave::Saving));
        assert_eq!(
            session.start_record(ExitCode::Resolved, After::Exit).map(drop),
            Err(CannotSave::Saving)
        );
        let mut unsaved = Session::new(conflict(), SaveTarget::default(), None);
        assert_eq!(unsaved.start_save(After::Exit).map(drop), Err(CannotSave::NoOutput));
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
        assert_eq!(confirm.save_requested(), Next::Write(After::Exit));
        let _job = confirm.start_save(After::Exit).expect("starts");
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
    fn save_writes_markers_unlocks_and_journals_on() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        assert_eq!(session.save_in_place_requested(), Next::Write(After::Stay));
        let saved = session.start_save(After::Stay).expect("starts").run();
        assert_eq!(session.saved(saved), Next::Saved(ExitCode::Conflicts));
        assert_eq!(std::fs::read_to_string(dir.path().join("out")).expect("written"), MARKERS);
        assert!(!session.saving());
        session.apply(pick_left()).expect("applies after the save");
        let (_, upto) = session.recovery().and_then(Recovery::next_write).expect("a write");
        assert_eq!(upto, 1, "the journal goes on");
    }

    #[test]
    fn save_without_an_output_asks_for_a_path() {
        let session = Session::new(conflict(), SaveTarget::default(), None);
        assert_eq!(session.save_in_place_requested(), Next::SaveCopyAs);
    }

    #[test]
    fn close_after_a_save_ends_with_its_code_until_the_next_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.apply(pick_left()).expect("applies");
        let saved = session.start_save(After::Stay).expect("starts").run();
        assert_eq!(session.saved(saved), Next::Saved(ExitCode::Resolved));
        let resolved = Next::Finish { code: ExitCode::Resolved, journal: Journal::Remove };
        assert_eq!(session.close_requested(), resolved);
        session.apply(Command::Undo).expect("undoes");
        assert_eq!(session.close_requested(), Next::ConfirmClose, "changed since the save");
        assert_eq!(session.close_answered(Some(ending::Closing::Discard)), resolved);
    }

    #[test]
    fn a_failed_save_leaves_the_last_saves_code() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let saved = session.start_save(After::Stay).expect("starts").run();
        assert_eq!(session.saved(saved), Next::Saved(ExitCode::Conflicts));
        session.target.destination = Some(Destination::file(dir.path().join("none").join("out")));
        session.apply(pick_left()).expect("applies");
        let saved = session.start_save(After::Stay).expect("starts").run();
        assert!(matches!(session.saved(saved), Next::Failed { after: After::Stay, .. }));
        let conflicts = Next::Finish { code: ExitCode::Conflicts, journal: Journal::Remove };
        assert_eq!(session.close_answered(Some(ending::Closing::Discard)), conflicts);
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
        assert_eq!(copy.encode().expect("encodes").code(), ExitCode::Conflicts);
        session.apply(pick_left()).expect("applies");
        let (copy, unresolved) = session.copy();
        assert_eq!(unresolved, 0);
        let path = dir.path().join("copy");
        assert_eq!(copy.write(&path).expect("writes"), ExitCode::Resolved);
        assert_eq!(std::fs::read_to_string(path).expect("written"), "a\nL\nc\n");
    }

    #[test]
    fn a_copy_writes_the_merge_as_it_stood_when_taken() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let (copy, _) = session.copy();
        session.apply(pick_left()).expect("applies");
        let path = dir.path().join("copy");
        assert_eq!(copy.write(&path).expect("writes"), ExitCode::Conflicts);
        assert_eq!(std::fs::read_to_string(path).expect("written"), MARKERS);
    }

    /// A clean merge of Windows-1252 inputs with `ł` typed at the start of line 1:
    /// Windows-1252 has no byte for it.
    fn unencodable() -> MergeDocument {
        use merge_core::document::{Labels, MergeOptions};
        use merge_core::edit::{EditBatch, TextPatch};
        use merge_core::text::Text;
        let text = || Text::decode(b"caf\xe9\n".to_vec()).expect("Windows-1252 decodes");
        let inputs = Inputs::three_way(text(), text(), text());
        let mut doc = MergeDocument::new(inputs, MergeOptions::default(), Labels::none())
            .expect("no labels fit");
        let typed = TextPatch { range: 0..0, text: "ł".to_owned() };
        doc.apply(Command::Edit(EditBatch(vec![typed]))).expect("the edit applies");
        doc
    }

    const UNENCODABLE: EncodeError =
        EncodeError { encoding: merge_core::text::Encoding::Windows1252, character: 'ł', line: 1 };

    #[test]
    fn a_save_the_encoding_cannot_hold_writes_nothing_and_the_merge_goes_on() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = Recovery::new(dir.path().join("journal"), Some(dir.path().join("out")));
        let doc = unencodable();
        recovery.hashed(input_hashes(doc.inputs()), doc.start());
        let target = target(Some(dir.path().join("out")), UnresolvedSave::Write);
        let mut session = Session::new(doc, target, Some(recovery));
        let saved = session.start_save(After::Exit).expect("starts").run();
        assert!(
            matches!(saved.outcome(), Err(SaveError::Encode(error)) if *error == UNENCODABLE),
            "{:?}",
            saved.outcome()
        );
        assert!(!dir.path().join("out").exists());
        assert_eq!(session.saved(saved), Next::Refused { error: UNENCODABLE });
        assert!(!session.saving());
        assert!(session.recovery().is_some(), "the journal goes on");
        session.apply(Command::Undo).expect("applies after the refusal");
    }

    #[test]
    fn a_copy_the_encoding_cannot_hold_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = Session::new(unencodable(), SaveTarget::default(), None);
        let path = dir.path().join("copy");
        let (copy, _) = session.copy();
        let written = copy.write(&path);
        assert!(
            matches!(written, Err(SaveError::Encode(error)) if error == UNENCODABLE),
            "{written:?}"
        );
        assert!(!path.exists());
    }
}
