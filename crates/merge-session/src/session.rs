//! A merge window's session (spec §6.2): the document, its recovery journal, where a save
//! writes and records, and whether a save is running. The front end holds the tasks that run
//! a save and the journal's writes, and asks the session what each step does.

use std::path::{Path, PathBuf};

use merge_core::document::{Applied, Command, CommandError, MergeDocument, OutputEnding};
use merge_core::journal::input_hashes;

use crate::ending::{self, Journal, Next, UnresolvedSave};
use crate::recovery::Recovery;
use crate::resolve::Record;
use crate::{EncodedOutput, ExitCode, SaveError};

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
#[derive(Debug)]
pub enum SaveJob {
    /// Write the output, then record it.
    Write { encoded: EncodedOutput, output: PathBuf, record: Option<Record> },
    /// Record again a save written with `code`, without writing it again.
    Record { record: Option<Record>, code: ExitCode },
}

impl SaveJob {
    /// Write and record, or record again.
    ///
    /// # Errors
    ///
    /// As [`EncodedOutput::save`] and [`Record::resave`].
    pub fn run(self) -> Result<ExitCode, SaveError> {
        match self {
            Self::Write { encoded, output, record } => encoded.save(&output, record.as_ref(), None),
            Self::Record { record, code } => {
                record.as_ref().map_or(Ok(code), |record| record.resave(code))
            }
        }
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

    /// Show `doc` in place of the session's document: a journal's replay.
    pub fn replace(&mut self, doc: MergeDocument) {
        self.doc = doc;
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

    /// Remove the journal and keep none from here on. The front end cancels its journal
    /// tasks first.
    pub fn end_journal(&mut self) {
        if let Some(recovery) = &mut self.recovery {
            recovery.discard();
        }
        self.recovery = None;
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

    /// What Close does now (spec §6.2).
    #[must_use]
    pub fn close_requested(&self) -> Next {
        ending::close_requested(self.saving, self.doc.changed())
    }

    /// Start Save & Exit: write the journal, then lock the session until [`Session::saved`].
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
        // The journal holds the work while the save runs, and after it fails.
        self.flush_journal();
        self.saving = true;
        Ok(SaveJob::Write {
            encoded: EncodedOutput::new(&self.doc, self.target.line_ending),
            output,
            record: self.target.record.clone(),
        })
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
        Ok(SaveJob::Record { record: self.target.record.clone(), code })
    }

    /// End a save with `outcome`, unlock the session, and give the next step.
    pub fn saved(&mut self, outcome: &Result<ExitCode, SaveError>) -> Next {
        self.saving = false;
        ending::saved(outcome)
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
    use merge_core::document::{Labels, MergeOptions, Resolution};
    use merge_core::index::RegionIx;
    use merge_core::journal::Journal as JournalFile;
    use merge_core::merge3::{Inputs, LEFT, Source};
    use merge_core::text::Text;

    const CONFLICT_REGION: RegionIx = RegionIx::new(1);
    const MARKERS: &str = "a\n<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n";

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes()).expect("decodes")
    }

    /// One conflict at line 2.
    fn conflict() -> MergeDocument {
        MergeDocument::new(
            Inputs::three_way(text("a\nb\nc\n"), text("a\nL\nc\n"), text("a\nR\nc\n")),
            MergeOptions::default(),
            Labels::none(),
        )
        .expect("no labels")
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
        assert_eq!(job.run().expect("writes"), ExitCode::Conflicts);
        assert_eq!(std::fs::read_to_string(dir.path().join("out")).expect("written"), MARKERS);
    }

    #[test]
    fn a_save_writes_the_journal_before_it_starts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        session.apply(pick_left()).expect("applies");
        let _job = session.start_save().expect("starts");
        assert_eq!(journal_commands(&session), 1);
    }

    #[test]
    fn a_failed_save_unlocks_the_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = journalled(dir.path());
        let missing = dir.path().join("none").join("out");
        session.target.output = Some(missing);
        let outcome = session.start_save().expect("starts").run();
        let next = session.saved(&outcome);
        assert!(matches!(next, Next::Failed { undone: ending::Undone::Write, .. }), "{next:?}");
        assert!(!session.saving());
        session.apply(pick_left()).expect("applies after the failure");
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
