//! One two-way diff, from open to its end: the document, each side's file and encoding,
//! which sides hold unsaved edits, and the saves that write them (spec §4.6, §6.1). The diff
//! window drives it; it holds no GPUI state.

use std::path::{Path, PathBuf};

use merge_core::diff2::{DiffDocument, DiffEditError, Diffed, EditBatch, Edited, Marks, Side};
use merge_core::index::RegionIx;
use merge_core::text::{Encoding, Text};
use vcs::VcsError;

use crate::SessionError;
use crate::diff_tool::{self, SideSave};
use crate::own_writes::OwnWrites;
use crate::repository::WorkingCopy;

const SIDES: [Side; 2] = [Side::Left, Side::Right];

/// A side as the diff opens it.
#[derive(Debug, Clone)]
pub struct DiffSide {
    /// The file a save writes; none for a revision, which is read-only.
    pub path: Option<PathBuf>,
    /// The file's encoding, which a save writes it back in.
    pub encoding: Encoding,
    pub read_only: bool,
}

/// One side's file and its edits.
#[derive(Debug)]
struct SideState {
    path: Option<PathBuf>,
    encoding: Encoding,
    /// Edited since it was opened, reloaded or last saved.
    modified: bool,
    /// Counts the edits and reloads, so a save that lands knows whether the text it wrote is
    /// still the side's.
    changes: u64,
    /// The texts the session wrote or is writing to the file.
    writes: OwnWrites,
}

/// A two-way diff and the files behind its sides.
#[derive(Debug)]
pub struct DiffSession {
    doc: DiffDocument,
    sides: [SideState; 2],
    /// The jj working copy each save snapshots; none outside triad's own jj runs.
    working_copy: Option<WorkingCopy>,
    /// The last save to land failed to write a side.
    save_failed: bool,
}

/// A save's writes, to run off the UI thread. It holds no borrow of the session.
#[derive(Debug)]
pub struct DiffSaveJob {
    /// Each side written, with its change count when its text was taken.
    writes: Vec<(Side, u64)>,
    saves: Vec<SideSave>,
    working_copy: Option<WorkingCopy>,
}

/// A save's outcome, for [`DiffSession::saved`].
#[derive(Debug)]
pub struct DiffSaved {
    writes: Vec<(Side, u64)>,
    saved: diff_tool::Saved,
}

/// What a landed save tells the window.
#[derive(Debug)]
pub struct SaveReport {
    /// Each side whose write failed, and why. Those sides keep their unsaved edits.
    pub failed: Vec<(Side, SessionError)>,
    /// The snapshot of jj's working copy; none when the save took none.
    pub snapshot: Option<Result<(), VcsError>>,
}

impl DiffSession {
    #[must_use]
    pub fn new(
        mut doc: DiffDocument,
        sides: [DiffSide; 2],
        working_copy: Option<WorkingCopy>,
    ) -> Self {
        let [left, right] = sides;
        let sides = [(Side::Left, left), (Side::Right, right)].map(|(side, open)| {
            doc.set_read_only(side, open.read_only);
            SideState {
                path: open.path,
                encoding: open.encoding,
                modified: false,
                changes: 0,
                writes: OwnWrites::default(),
            }
        });
        Self { doc, sides, working_copy, save_failed: false }
    }

    #[must_use]
    pub fn document(&self) -> &DiffDocument {
        &self.doc
    }

    /// The file `side` saves to; none for a revision.
    #[must_use]
    pub fn path(&self, side: Side) -> Option<&Path> {
        self.side(side).path.as_deref()
    }

    #[must_use]
    pub fn encoding(&self, side: Side) -> Encoding {
        self.side(side).encoding
    }

    /// Whether `side` holds edits no save has written.
    #[must_use]
    pub fn modified(&self, side: Side) -> bool {
        self.side(side).modified
    }

    #[must_use]
    pub fn any_modified(&self) -> bool {
        self.sides.iter().any(|side| side.modified)
    }

    /// Whether the last save to land failed to write a side.
    #[must_use]
    pub fn save_failed(&self) -> bool {
        self.save_failed
    }

    /// Apply the editor's `batch` to `side`.
    ///
    /// # Errors
    ///
    /// As [`DiffDocument::edit`]; the side is then as it was.
    pub fn edit(&mut self, side: Side, batch: EditBatch) -> Result<Edited, DiffEditError> {
        let before = self.doc.version();
        let edited = self.doc.edit(side, batch)?;
        Ok(self.note(before, edited))
    }

    /// Replace region `region`'s lines in `to` with the other side's (spec §4.6).
    ///
    /// # Errors
    ///
    /// As [`DiffDocument::copy_region`].
    pub fn copy_region(&mut self, region: RegionIx, to: Side) -> Result<Edited, DiffEditError> {
        let before = self.doc.version();
        let edited = self.doc.copy_region(region, to)?;
        Ok(self.note(before, edited))
    }

    pub fn undo(&mut self, side: Side) -> Edited {
        let before = self.doc.version();
        let edited = self.doc.undo(side);
        self.note(before, edited)
    }

    pub fn redo(&mut self, side: Side) -> Edited {
        let before = self.doc.version();
        let edited = self.doc.redo(side);
        self.note(before, edited)
    }

    /// Mark `edited`'s side modified when the change moved the document past `before`.
    fn note(&mut self, before: u64, edited: Edited) -> Edited {
        if edited.version != before {
            let state = self.side_mut(edited.side);
            state.changes += 1;
            state.modified = true;
        }
        edited
    }

    /// Install a recompute's regions, unless an edit made them stale.
    pub fn install(&mut self, diffed: Diffed) -> bool {
        self.doc.install(diffed)
    }

    /// Install the intra-line marks computed at `version`, unless an edit made them stale.
    pub fn install_marks(&mut self, version: u64, marks: Marks) -> bool {
        self.doc.install_marks(version, marks)
    }

    /// Start a save of every side with unsaved edits and a file, as it stands now. Each
    /// write counts as the session's own from here, since the file watcher can report it
    /// before it lands.
    pub fn start_save(&mut self) -> DiffSaveJob {
        let (writes, saves): (Vec<_>, Vec<_>) = SIDES
            .into_iter()
            .filter_map(|side| {
                let state = self.side(side);
                let path = state.path.as_ref().filter(|_| state.modified)?;
                let save = SideSave {
                    path: path.clone(),
                    encoding: state.encoding,
                    text: self.doc.snapshot(side),
                };
                Some(((side, state.changes), save))
            })
            .unzip();
        for (&(side, changes), save) in writes.iter().zip(&saves) {
            // A rope clone shares its chunks.
            self.side_mut(side).writes.start(changes, save.text.clone(), save.encoding);
        }
        DiffSaveJob { writes, saves, working_copy: self.working_copy.clone() }
    }

    /// Take the outcome of a save. A side edited while its write ran keeps its edits marked
    /// unsaved.
    pub fn saved(&mut self, saved: DiffSaved) -> SaveReport {
        let DiffSaved { writes, saved: diff_tool::Saved { written, snapshot } } = saved;
        let mut failed = Vec::new();
        for ((side, changes), result) in writes.into_iter().zip(written) {
            let state = self.side_mut(side);
            state.writes.ended(changes, result.is_ok());
            match result {
                Ok(()) if state.changes == changes => state.modified = false,
                Ok(()) => {}
                Err(error) => failed.push((side, error)),
            }
        }
        self.save_failed = !failed.is_empty();
        SaveReport { failed, snapshot }
    }

    /// Whether `side`'s file holding `text`, in the side's encoding, is the session's own
    /// save: one written, or one being written.
    #[must_use]
    pub fn is_own_write(&self, side: Side, text: &str) -> bool {
        let state = self.side(side);
        state.writes.holds(text, state.encoding)
    }

    /// The text `read` from `side`'s file after a change on disk, when it is news: none when
    /// the read failed, the text matches the side, or it is the session's own save. Beside a
    /// side read one character per byte, the text reads that way too (spec §4.1).
    #[must_use]
    pub fn disk_text(&self, side: Side, read: Result<Text, SessionError>) -> Option<Text> {
        let text = match read {
            Ok(text) => text,
            Err(error) => {
                tracing::info!(%error, "cannot read a changed file; the side keeps its text");
                return None;
            }
        };
        let fallback = self.sides.iter().any(|state| state.encoding.one_byte_per_char());
        let text = if fallback { text.to_windows_1252().unwrap_or(text) } else { text };
        let state = self.side(side);
        let same = text.encoding() == state.encoding && self.doc.text(side) == text.content();
        let own = state.writes.holds(text.content(), text.encoding());
        (!same && !own).then_some(text)
    }

    /// Replace `side` with `text`, read from its file: it holds no unsaved edits now, and the
    /// session's earlier writes no longer describe the file.
    pub fn reloaded(&mut self, side: Side, text: &Text) {
        self.doc.reload(side, text);
        let state = self.side_mut(side);
        state.encoding = text.encoding();
        state.changes += 1;
        state.writes.reloaded();
        state.modified = false;
    }

    /// Read `side`, UTF-8 text `utf8`, one character per byte beside the other side's
    /// Windows-1252 text (spec §4.1). Its unsaved edits stay; its undo history goes. False,
    /// and nothing changes, unless the side is UTF-8 without a byte order mark.
    pub fn read_bytewise(&mut self, side: Side, utf8: String) -> bool {
        if self.side(side).encoding != Encoding::Utf8 {
            return false;
        }
        let Some(text) = Text::from_utf8(utf8).to_windows_1252() else {
            return false;
        };
        self.doc.reload(side, &text);
        let state = self.side_mut(side);
        state.encoding = text.encoding();
        state.changes += 1;
        true
    }

    fn side(&self, side: Side) -> &SideState {
        &self.sides[index(side)]
    }

    fn side_mut(&mut self, side: Side) -> &mut SideState {
        &mut self.sides[index(side)]
    }
}

impl DiffSaveJob {
    /// Write each side, then snapshot jj's working copy when a write landed.
    #[must_use]
    pub fn run(self) -> DiffSaved {
        let saved = diff_tool::save(&self.saves, self.working_copy.as_ref());
        DiffSaved { writes: self.writes, saved }
    }
}

const fn index(side: Side) -> usize {
    match side {
        Side::Left => 0,
        Side::Right => 1,
    }
}

#[cfg(test)]
mod tests {
    use merge_core::diff::DiffOptions;
    use merge_core::diff2::TextPatch;

    use super::*;

    const RIGHT: &str = "r.txt";

    fn text(content: &str) -> Text {
        Text::from_utf8(content.to_owned())
    }

    /// A diff of a revision holding `left` against the file `right` in `dir`, saved at
    /// `file`, the revision in `left`'s encoding.
    fn session_at(left: &Text, right: &str, file: PathBuf) -> DiffSession {
        let doc = DiffDocument::new(left, &text(right), DiffOptions::default());
        let revision = DiffSide { path: None, encoding: left.encoding(), read_only: true };
        let file = DiffSide { path: Some(file), encoding: Encoding::Utf8, read_only: false };
        DiffSession::new(doc, [revision, file], None)
    }

    /// The diff of "a\n" against the file "b\n".
    fn open() -> (tempfile::TempDir, DiffSession) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(RIGHT);
        std::fs::write(&file, "b\n").unwrap();
        let session = session_at(&text("a\n"), "b\n", file);
        (dir, session)
    }

    fn type_at(session: &mut DiffSession, side: Side, at: usize, typed: &str) {
        let patch = TextPatch { range: at..at, text: typed.to_owned() };
        session.edit(side, EditBatch(vec![patch])).unwrap();
    }

    fn save(session: &mut DiffSession) -> SaveReport {
        let job = session.start_save();
        session.saved(job.run())
    }

    fn on_disk(dir: &tempfile::TempDir) -> String {
        std::fs::read_to_string(dir.path().join(RIGHT)).unwrap()
    }

    #[test]
    fn an_edit_marks_its_side_and_a_save_writes_and_clears_it() {
        let (dir, mut session) = open();
        type_at(&mut session, Side::Right, 0, "x");
        assert!(session.modified(Side::Right));
        assert!(!session.modified(Side::Left));
        let report = save(&mut session);
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert!(report.snapshot.is_none(), "no working copy to snapshot");
        assert_eq!(on_disk(&dir), "xb\n");
        assert!(!session.any_modified());
        assert!(!session.save_failed());
    }

    #[test]
    fn a_revision_takes_no_edit_and_is_never_marked() {
        let (_dir, mut session) = open();
        let patch = TextPatch { range: 0..0, text: "x".to_owned() };
        let refused = session.edit(Side::Left, EditBatch(vec![patch]));
        assert!(matches!(refused, Err(DiffEditError::ReadOnly { side: Side::Left })));
        assert!(!session.modified(Side::Left));
    }

    #[test]
    fn a_save_without_edits_writes_nothing() {
        let (dir, mut session) = open();
        std::fs::remove_file(dir.path().join(RIGHT)).unwrap();
        let report = save(&mut session);
        assert!(report.failed.is_empty());
        assert!(!dir.path().join(RIGHT).exists());
    }

    #[test]
    fn an_edit_made_while_a_save_runs_stays_unsaved() {
        let (dir, mut session) = open();
        type_at(&mut session, Side::Right, 0, "x");
        let job = session.start_save();
        type_at(&mut session, Side::Right, 0, "y");
        session.saved(job.run());
        assert_eq!(on_disk(&dir), "xb\n");
        assert!(session.modified(Side::Right));
        save(&mut session);
        assert_eq!(on_disk(&dir), "yxb\n");
        assert!(!session.modified(Side::Right));
    }

    #[test]
    fn a_failed_write_keeps_the_edit_until_a_save_lands() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone");
        let mut session = session_at(&text("a\n"), "b\n", missing.join(RIGHT));
        type_at(&mut session, Side::Right, 0, "x");
        let report = save(&mut session);
        let sides: Vec<Side> = report.failed.iter().map(|(side, _)| *side).collect();
        assert_eq!(sides, [Side::Right]);
        assert!(session.modified(Side::Right));
        assert!(session.save_failed());
        std::fs::create_dir(&missing).unwrap();
        assert!(save(&mut session).failed.is_empty());
        assert!(!session.save_failed());
        assert!(!session.modified(Side::Right));
        assert_eq!(std::fs::read_to_string(missing.join(RIGHT)).unwrap(), "xb\n");
    }

    #[test]
    fn a_disk_read_is_news_unless_it_is_the_side_or_an_own_write() {
        let (dir, mut session) = open();
        assert!(session.disk_text(Side::Right, Ok(text("b\n"))).is_none(), "the side's text");
        let news = session.disk_text(Side::Right, Ok(text("c\n"))).expect("news");
        assert_eq!(news.content(), "c\n");
        let failed = SessionError::Read {
            path: dir.path().join(RIGHT),
            source: std::io::ErrorKind::PermissionDenied.into(),
        };
        assert!(session.disk_text(Side::Right, Err(failed)).is_none());
        type_at(&mut session, Side::Right, 0, "x");
        save(&mut session);
        assert!(session.is_own_write(Side::Right, "xb\n"));
        type_at(&mut session, Side::Right, 0, "y");
        assert!(session.disk_text(Side::Right, Ok(text("xb\n"))).is_none(), "the own write");
    }

    #[test]
    fn a_reload_clears_the_edits_and_forgets_the_own_writes() {
        let (_dir, mut session) = open();
        type_at(&mut session, Side::Right, 0, "x");
        save(&mut session);
        type_at(&mut session, Side::Right, 0, "y");
        session.reloaded(Side::Right, &text("z\n"));
        assert!(!session.modified(Side::Right));
        assert_eq!(session.document().text(Side::Right), "z\n");
        assert!(!session.is_own_write(Side::Right, "xb\n"));
        assert!(session.disk_text(Side::Right, Ok(text("xb\n"))).is_some());
    }

    #[test]
    fn beside_a_windows_1252_side_a_disk_read_is_one_character_per_byte() {
        let dir = tempfile::tempdir().unwrap();
        let left = text("\u{e9}\n").to_windows_1252().unwrap();
        let session = session_at(&left, "b\n", dir.path().join(RIGHT));
        let read = session.disk_text(Side::Right, Ok(text("\u{e9}\n"))).expect("news");
        // U+00E9 is C3 A9 in UTF-8: Windows-1252's Ã and ©. The bytes save back unchanged.
        assert_eq!(read.encoding(), Encoding::Utf8AsWindows1252);
        assert_eq!(read.content(), "\u{c3}\u{a9}\n");
    }

    #[test]
    fn a_utf8_side_reads_bytewise_once_and_keeps_its_edits_marked() {
        let (_dir, mut session) = open();
        type_at(&mut session, Side::Right, 0, "\u{e9}");
        assert!(session.read_bytewise(Side::Right, "\u{e9}b\n".to_owned()));
        assert_eq!(session.encoding(Side::Right), Encoding::Utf8AsWindows1252);
        assert_eq!(session.document().text(Side::Right), "\u{c3}\u{a9}b\n");
        assert!(session.modified(Side::Right));
        assert!(!session.read_bytewise(Side::Right, "b\n".to_owned()), "no longer UTF-8");
    }
}
