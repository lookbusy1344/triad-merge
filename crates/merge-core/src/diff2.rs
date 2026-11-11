//! Two-way diff (spec §4.6): regions, and a document of two editable buffers.

use std::ops::Range;

/// The rope that holds each side, which [`DiffDocument::text`] and [`DiffDocument::snapshot`]
/// return. Front ends name it here, so the core alone picks the rope crate.
pub use ropey::{Rope, RopeSlice};

use crate::diff::{DiffOptions, diff_lines};
pub use crate::edit::{BatchError, EditBatch, TextPatch};
use crate::edit::{apply, caret_after, inverse, prepare};
use crate::index::RegionIx;
use crate::text::{Encoding, LineEnding, Text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind2 {
    Unchanged,
    /// Either range may be empty, for a pure insertion or deletion.
    Changed,
}

/// One stretch of both files, as line ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region2 {
    pub kind: Kind2,
    pub left: Range<usize>,
    pub right: Range<usize>,
}

/// The regions that cover both files in order, from one diff left→right. Equal stretches
/// between changes are `Unchanged`. Identical files, empty ones included, give one
/// `Unchanged` region.
#[must_use]
pub fn regions(left: &Text, right: &Text, opts: DiffOptions) -> Vec<Region2> {
    let hunks = diff_lines(left, right, opts);
    let mut regions = Vec::with_capacity(2 * hunks.len() + 1);
    let mut at = (0, 0);
    for hunk in hunks {
        let start = (hunk.before.start, hunk.after.start);
        if start != at {
            regions.push(unchanged(at, start));
        }
        at = (hunk.before.end, hunk.after.end);
        regions.push(Region2 { kind: Kind2::Changed, left: hunk.before, right: hunk.after });
    }
    let end = (left.line_count(), right.line_count());
    if end != at || regions.is_empty() {
        regions.push(unchanged(at, end));
    }
    regions
}

/// Equal lines from `from` to `to`, each a (left, right) pair of line numbers.
fn unchanged(from: (usize, usize), to: (usize, usize)) -> Region2 {
    Region2 { kind: Kind2::Unchanged, left: from.0..to.0, right: from.1..to.1 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    #[must_use]
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

impl Region2 {
    /// This region's lines in `side`.
    #[must_use]
    pub fn lines(&self, side: Side) -> Range<usize> {
        match side {
            Side::Left => self.left.clone(),
            Side::Right => self.right.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiffEditError {
    #[error("the {side:?} side is read-only")]
    ReadOnly { side: Side },
    #[error("no region {region}: the diff has {regions} regions")]
    NoRegion { region: RegionIx, regions: usize },
    #[error(transparent)]
    Edit(#[from] BatchError),
}

/// A change to one side and the document version it made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub side: Side,
    pub version: u64,
    /// Sorted, with offsets in that side's editor text before any of them. For an edit the
    /// editor made, the line-ending corrections to it.
    pub patches: Vec<TextPatch>,
}

/// One side's text and its undo history.
#[derive(Debug, Clone)]
struct Buffer {
    text: Rope,
    typed_ending: LineEnding,
    read_only: bool,
    /// Each entry undoes or redoes one step.
    undo: Vec<Vec<TextPatch>>,
    redo: Vec<Vec<TextPatch>>,
    /// The last step typed text without a newline, so typing on at its end joins it.
    typing: bool,
}

impl Buffer {
    fn new(text: &Text) -> Self {
        Self {
            text: Rope::from_str(text.content()),
            typed_ending: text.dominant_ending().unwrap_or(LineEnding::Lf),
            read_only: false,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
        }
    }

    fn commit(&mut self, patches: &[TextPatch], typing: bool) {
        let undo = inverse(patches, &self.text);
        apply(&mut self.text, patches);
        self.redo.clear();
        match self.undo.last_mut() {
            Some(top)
                if typing && self.typing && caret_after(top) == Some(patches[0].range.start) =>
            {
                top[0].range.end = undo[0].range.end;
            }
            _ => self.undo.push(undo),
        }
        self.typing = typing;
    }

    /// Undo or redo one step, recording its inverse on the other stack.
    fn replay(&mut self, undo: bool) -> Vec<TextPatch> {
        self.typing = false;
        let from = if undo { &mut self.undo } else { &mut self.redo };
        let Some(step) = from.pop() else {
            return Vec::new();
        };
        let back = inverse(&step, &self.text);
        apply(&mut self.text, &step);
        if undo {
            self.redo.push(back);
        } else {
            self.undo.push(back);
        }
        step
    }

    fn line_bytes(&self, lines: Range<usize>) -> Range<usize> {
        self.text.line_to_byte(lines.start)..self.text.line_to_byte(lines.end)
    }
}

/// Two editable files and their regions. Every change to either text takes a new version;
/// regions come from [`DiffDocument::recompute`] and are installed for the version they were
/// computed from.
#[derive(Debug, Clone)]
pub struct DiffDocument {
    left: Buffer,
    right: Buffer,
    options: DiffOptions,
    regions: Vec<Region2>,
    regions_version: u64,
    version: u64,
}

/// A snapshot of both texts to diff off the UI thread. Cloning a rope is O(1).
#[derive(Debug, Clone)]
pub struct Recompute {
    version: u64,
    left: Rope,
    right: Rope,
    options: DiffOptions,
}

impl Recompute {
    /// The regions, and the version they belong to.
    #[must_use]
    pub fn run(self) -> (u64, Vec<Region2>) {
        let (version, regions, ()) = self.run_with(|_| ());
        (version, regions)
    }

    /// [`Recompute::run`], also handing the snapshot's texts, left then right, to `also`. Its
    /// result belongs to the same version.
    #[must_use]
    pub fn run_with<T>(self, also: impl FnOnce([&Text; 2]) -> T) -> (u64, Vec<Region2>, T) {
        // The encoding plays no part in diffing.
        let text = |rope: Rope| Text::new(Encoding::Utf8, rope.to_string());
        let (left, right) = (text(self.left), text(self.right));
        let regions = regions(&left, &right, self.options);
        (self.version, regions, also([&left, &right]))
    }
}

impl DiffDocument {
    #[must_use]
    pub fn new(left: &Text, right: &Text, options: DiffOptions) -> Self {
        Self {
            regions: regions(left, right, options),
            left: Buffer::new(left),
            right: Buffer::new(right),
            options,
            regions_version: 0,
            version: 0,
        }
    }

    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether the installed regions belong to the current text.
    #[must_use]
    pub fn regions_current(&self) -> bool {
        self.regions_version == self.version
    }

    /// The regions last installed. They lag the text until the next recompute is installed.
    #[must_use]
    pub fn regions(&self) -> &[Region2] {
        &self.regions
    }

    #[must_use]
    pub fn text(&self, side: Side) -> RopeSlice<'_> {
        self.buffer(side).text.slice(..)
    }

    /// `side`'s text as it stands now, for a write off the UI thread. The copy is O(1): it
    /// shares the rope's nodes, and later edits leave it as it is.
    #[must_use]
    pub fn snapshot(&self, side: Side) -> Rope {
        self.buffer(side).text.clone()
    }

    pub fn set_read_only(&mut self, side: Side, read_only: bool) {
        self.buffer_mut(side).read_only = read_only;
    }

    /// Fold in an edit the editor has made to `side`. Newlines in inserted text take that
    /// side's dominant ending.
    ///
    /// # Errors
    ///
    /// [`DiffEditError::ReadOnly`] for a read-only side, [`DiffEditError::Edit`] for a batch
    /// that does not fit the text. The document is then unchanged.
    pub fn edit(&mut self, side: Side, batch: EditBatch) -> Result<Edited, DiffEditError> {
        let buffer = self.writable(side)?;
        let Some(prepared) = prepare(&buffer.text, batch, buffer.typed_ending)? else {
            return Ok(self.edited(side, Vec::new()));
        };
        buffer.commit(&prepared.patches, prepared.typing);
        self.version += 1;
        Ok(self.edited(side, prepared.corrections))
    }

    /// Replace region `region`'s lines in `to` with the other side's (spec §4.6). Regions are
    /// recomputed first if they lag the text.
    ///
    /// # Errors
    ///
    /// [`DiffEditError::ReadOnly`] when `to` is read-only, [`DiffEditError::NoRegion`] for a
    /// region the current text does not have.
    pub fn copy_region(&mut self, region: RegionIx, to: Side) -> Result<Edited, DiffEditError> {
        self.writable(to)?;
        if !self.regions_current() {
            let (version, regions) = self.recompute().run();
            self.install(version, regions);
        }
        let r = self
            .regions
            .get(region.get())
            .ok_or(DiffEditError::NoRegion { region, regions: self.regions.len() })?;
        let from = self.buffer(to.other());
        let text = from.text.byte_slice(from.line_bytes(r.lines(to.other()))).to_string();
        let patch = TextPatch { range: self.buffer(to).line_bytes(r.lines(to)), text };
        self.buffer_mut(to).commit(std::slice::from_ref(&patch), false);
        self.version += 1;
        Ok(self.edited(to, vec![patch]))
    }

    /// Undo `side`'s last step. With nothing to undo, no patches and the same version.
    pub fn undo(&mut self, side: Side) -> Edited {
        self.replay(side, true)
    }

    /// Redo `side`'s last undone step. With nothing to redo, no patches and the same version.
    pub fn redo(&mut self, side: Side) -> Edited {
        self.replay(side, false)
    }

    /// Replace `side` with `text`, as read from disk again. The side's undo history goes;
    /// its read-only flag stays.
    pub fn reload(&mut self, side: Side, text: &Text) {
        let buffer = self.buffer_mut(side);
        *buffer = Buffer { read_only: buffer.read_only, ..Buffer::new(text) };
        self.version += 1;
    }

    #[must_use]
    pub fn recompute(&self) -> Recompute {
        Recompute {
            version: self.version,
            left: self.left.text.clone(),
            right: self.right.text.clone(),
            options: self.options,
        }
    }

    /// Install regions computed for `version`. A result for an older version is dropped:
    /// returns whether the regions were installed.
    pub fn install(&mut self, version: u64, regions: Vec<Region2>) -> bool {
        let current = version == self.version;
        if current {
            self.regions = regions;
            self.regions_version = version;
        }
        current
    }

    fn replay(&mut self, side: Side, undo: bool) -> Edited {
        let patches = self.buffer_mut(side).replay(undo);
        if !patches.is_empty() {
            self.version += 1;
        }
        self.edited(side, patches)
    }

    fn edited(&self, side: Side, patches: Vec<TextPatch>) -> Edited {
        Edited { side, version: self.version, patches }
    }

    fn writable(&mut self, side: Side) -> Result<&mut Buffer, DiffEditError> {
        let buffer = self.buffer_mut(side);
        if buffer.read_only { Err(DiffEditError::ReadOnly { side }) } else { Ok(buffer) }
    }

    fn buffer(&self, side: Side) -> &Buffer {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn buffer_mut(&mut self, side: Side) -> &mut Buffer {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::Whitespace;
    use crate::testing::text;

    fn of(left: &str, right: &str) -> Vec<Region2> {
        regions(&text(left), &text(right), DiffOptions::default())
    }

    fn same(left: Range<usize>, right: Range<usize>) -> Region2 {
        Region2 { kind: Kind2::Unchanged, left, right }
    }

    fn changed(left: Range<usize>, right: Range<usize>) -> Region2 {
        Region2 { kind: Kind2::Changed, left, right }
    }

    #[test]
    fn regions_are_current_until_an_edit_and_again_once_installed() {
        let mut d = doc("a\n", "b\n");
        assert!(d.regions_current());
        d.edit(Side::Left, insert(0, "x")).expect("edits");
        assert!(!d.regions_current());
        let stale = d.recompute();
        d.edit(Side::Left, insert(0, "y")).expect("edits");
        let (version, regions) = stale.run();
        assert!(!d.install(version, regions));
        assert!(!d.regions_current(), "a stale result is not installed");
        let (version, regions) = d.recompute().run();
        assert!(d.install(version, regions));
        assert!(d.regions_current());
    }

    #[test]
    fn reload_replaces_one_side_and_takes_a_version() {
        let mut d = doc("a\n", "b\n");
        d.edit(Side::Left, insert(0, "x")).expect("edits");
        let version = d.version();
        d.reload(Side::Left, &text("new\n"));
        assert_eq!(d.text(Side::Left).to_string(), "new\n");
        assert_eq!(d.text(Side::Right).to_string(), "b\n");
        assert_eq!(d.version(), version + 1);
        assert_eq!(d.undo(Side::Left).patches, []);
    }

    #[test]
    fn reload_keeps_the_read_only_flag_and_takes_the_new_ending() {
        let mut d = doc("a\n", "b\n");
        d.set_read_only(Side::Left, true);
        d.reload(Side::Left, &text("c\r\n"));
        assert_eq!(
            d.edit(Side::Left, insert(0, "x")),
            Err(DiffEditError::ReadOnly { side: Side::Left })
        );
        d.reload(Side::Right, &text("c\r\n"));
        d.edit(Side::Right, insert(0, "x\n")).expect("edits");
        assert_eq!(d.text(Side::Right).to_string(), "x\r\nc\r\n");
    }

    #[test]
    fn reload_leaves_the_other_sides_history() {
        let mut d = doc("a\n", "b\n");
        d.edit(Side::Right, insert(0, "y")).expect("edits");
        d.reload(Side::Left, &text("new\n"));
        assert_eq!(d.undo(Side::Right).patches.len(), 1);
        assert_eq!(d.text(Side::Right).to_string(), "b\n");
    }

    #[test]
    fn identical_files_give_one_unchanged_region() {
        assert_eq!(of("a\nb\n", "a\nb\n"), [same(0..2, 0..2)]);
        assert_eq!(of("", ""), [same(0..0, 0..0)]);
    }

    #[test]
    fn a_change_in_the_middle_splits_three_ways() {
        assert_eq!(
            of("a\nb\nc\n", "a\nB\nc\n"),
            [same(0..1, 0..1), changed(1..2, 1..2), same(2..3, 2..3)]
        );
    }

    #[test]
    fn insertions_and_deletions_have_an_empty_side() {
        assert_eq!(of("a\n", "x\na\n"), [changed(0..0, 0..1), same(0..1, 1..2)]);
        assert_eq!(of("a\nb\n", "a\n"), [same(0..1, 0..1), changed(1..2, 1..1)]);
        assert_eq!(of("", "x\n"), [changed(0..0, 0..1)]);
    }

    #[test]
    fn one_empty_file_is_one_change() {
        assert_eq!(of("", "a\n"), [changed(0..0, 0..1)]);
        assert_eq!(of("a\n", ""), [changed(0..1, 0..0)]);
    }

    #[test]
    fn insertions_at_the_start_middle_and_end() {
        assert_eq!(of("a\nb\n", "x\na\nb\n"), [changed(0..0, 0..1), same(0..2, 1..3)]);
        assert_eq!(
            of("a\nb\n", "a\nx\nb\n"),
            [same(0..1, 0..1), changed(1..1, 1..2), same(1..2, 2..3)]
        );
        assert_eq!(of("a\nb\n", "a\nb\nx\n"), [same(0..2, 0..2), changed(2..2, 2..3)]);
    }

    #[test]
    fn deletions_at_the_start_middle_and_end() {
        assert_eq!(of("x\na\nb\n", "a\nb\n"), [changed(0..1, 0..0), same(1..3, 0..2)]);
        assert_eq!(
            of("a\nx\nb\n", "a\nb\n"),
            [same(0..1, 0..1), changed(1..2, 1..1), same(2..3, 1..2)]
        );
        assert_eq!(of("a\nb\nx\n", "a\nb\n"), [same(0..2, 0..2), changed(2..3, 2..2)]);
    }

    #[test]
    fn a_missing_final_newline_changes_the_last_line() {
        assert_eq!(of("a\nb", "a\nb\n"), [same(0..1, 0..1), changed(1..2, 1..2)]);
        assert_eq!(of("a\nb\n", "a\nb"), [same(0..1, 0..1), changed(1..2, 1..2)]);
        assert_eq!(of("a\nb", "a\nb"), [same(0..2, 0..2)]);
    }

    #[test]
    fn each_whitespace_mode_decides_equality() {
        let with = |whitespace, left: &str, right: &str| {
            let opts = DiffOptions { whitespace, ..DiffOptions::default() };
            regions(&text(left), &text(right), opts)
        };
        let one_line_differs = [changed(0..1, 0..1)];
        let equal = [same(0..1, 0..1)];
        let cases = [
            (Whitespace::None, "a \n", "a\n", &one_line_differs),
            (Whitespace::Trailing, "a \n", "a\r\n", &equal),
            (Whitespace::Trailing, "a  b\n", "a b\n", &one_line_differs),
            (Whitespace::Amount, "a  b\t\n", "a b\n", &equal),
            (Whitespace::Amount, "ab\n", "a b\n", &one_line_differs),
            (Whitespace::All, "ab\n", " a b\n", &equal),
        ];
        for (mode, left, right, expected) in cases {
            assert_eq!(with(mode, left, right), *expected, "{mode:?} {left:?} {right:?}");
        }
    }

    #[test]
    fn a_whitespace_mode_can_make_files_equal() {
        let opts = DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() };
        assert_eq!(regions(&text("a \nb\n"), &text("a\nb\r\n"), opts), [same(0..2, 0..2)]);
    }

    // DiffDocument

    fn doc(left: &str, right: &str) -> DiffDocument {
        DiffDocument::new(&text(left), &text(right), DiffOptions::default())
    }

    fn patch(range: Range<usize>, text: &str) -> TextPatch {
        TextPatch { range, text: text.to_owned() }
    }

    fn insert(at: usize, text: &str) -> EditBatch {
        EditBatch(vec![patch(at..at, text)])
    }

    #[expect(clippy::unnecessary_wraps, reason = "compared with the document's results")]
    fn edited(side: Side, version: u64, patches: Vec<TextPatch>) -> Result<Edited, DiffEditError> {
        Ok(at(side, version, patches))
    }

    fn at(side: Side, version: u64, patches: Vec<TextPatch>) -> Edited {
        Edited { side, version, patches }
    }

    #[test]
    fn a_new_document_holds_both_texts_and_their_regions() {
        let d = doc("a\nb\n", "a\nB\n");
        assert_eq!(d.version(), 0);
        assert_eq!(d.regions(), [same(0..1, 0..1), changed(1..2, 1..2)]);
        assert_eq!(d.text(Side::Left), "a\nb\n");
        assert_eq!(d.text(Side::Right), "a\nB\n");
    }

    #[test]
    fn an_edit_takes_a_new_version_and_regions_wait_for_the_recompute() {
        let mut d = doc("a\nb\nc\n", "a\nb\nc\n");
        assert_eq!(d.edit(Side::Left, insert(2, "x")), edited(Side::Left, 1, vec![]));
        assert_eq!(d.text(Side::Left), "a\nxb\nc\n");
        assert_eq!(d.regions(), [same(0..3, 0..3)]);
        let (version, regions) = d.recompute().run();
        assert_eq!(
            (version, regions.clone()),
            (1, vec![same(0..1, 0..1), changed(1..2, 1..2), same(2..3, 2..3)])
        );
        assert!(d.install(version, regions.clone()));
        assert_eq!(d.regions(), regions);
    }

    #[test]
    fn a_recompute_hands_its_snapshot_of_both_texts_on() {
        let mut d = doc("a\n", "b\n");
        let job = d.recompute();
        d.edit(Side::Left, insert(0, "x")).expect("edit");
        let (version, regions, seen) =
            job.run_with(|[left, right]| (left.content().to_owned(), right.content().to_owned()));
        assert_eq!(version, 0);
        assert_eq!(regions, [changed(0..1, 0..1)]);
        assert_eq!(seen, ("a\n".to_owned(), "b\n".to_owned()));
    }

    #[test]
    fn a_snapshot_keeps_its_sides_text_through_later_edits() {
        let mut d = doc("a\n", "b\n");
        let left = d.snapshot(Side::Left);
        let right = d.snapshot(Side::Right);
        d.edit(Side::Left, insert(0, "x")).expect("edit");
        assert_eq!(left, "a\n");
        assert_eq!(right, "b\n");
        assert_eq!(d.snapshot(Side::Left), "xa\n");
    }

    #[test]
    fn a_stale_recompute_result_is_rejected() {
        let mut d = doc("a\n", "a\n");
        let job = d.recompute();
        d.edit(Side::Right, insert(0, "x\n")).expect("edit");
        let (version, regions) = job.run();
        assert_eq!(version, 0);
        assert!(!d.install(version, regions));
        assert_eq!(d.regions(), [same(0..1, 0..1)]);
    }

    #[test]
    fn a_read_only_side_refuses_edits_and_copies() {
        let mut d = doc("a\nb\n", "a\nB\n");
        d.set_read_only(Side::Right, true);
        assert_eq!(
            d.edit(Side::Right, insert(0, "x")),
            Err(DiffEditError::ReadOnly { side: Side::Right })
        );
        assert_eq!(
            d.copy_region(RegionIx::new(1), Side::Right),
            Err(DiffEditError::ReadOnly { side: Side::Right })
        );
        assert_eq!(d.text(Side::Right), "a\nB\n");
        assert_eq!(d.version(), 0);
        assert_eq!(d.edit(Side::Left, insert(0, "x")), edited(Side::Left, 1, vec![]));
        d.set_read_only(Side::Right, false);
        assert_eq!(d.edit(Side::Right, insert(0, "y")), edited(Side::Right, 2, vec![]));
    }

    #[test]
    fn copying_left_to_right_is_an_edit_that_undo_reverses() {
        let mut d = doc("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(
            d.copy_region(RegionIx::new(1), Side::Right),
            edited(Side::Right, 1, vec![patch(2..4, "b\n")])
        );
        assert_eq!(d.text(Side::Right), "a\nb\nc\n");
        assert_eq!(d.undo(Side::Right), at(Side::Right, 2, vec![patch(2..4, "B\n")]));
        assert_eq!(d.text(Side::Right), "a\nB\nc\n");
        assert_eq!(d.redo(Side::Right), at(Side::Right, 3, vec![patch(2..4, "b\n")]));
        assert_eq!(d.text(Side::Left), "a\nb\nc\n");
    }

    #[test]
    fn copying_right_to_left_replaces_lefts_lines() {
        let mut d = doc("a\nb\nc\n", "a\nB1\nB2\nc\n");
        assert_eq!(
            d.copy_region(RegionIx::new(1), Side::Left),
            edited(Side::Left, 1, vec![patch(2..4, "B1\nB2\n")])
        );
        assert_eq!(d.text(Side::Left), "a\nB1\nB2\nc\n");
        assert_eq!(d.undo(Side::Left), at(Side::Left, 2, vec![patch(2..8, "b\n")]));
    }

    #[test]
    fn a_copy_uses_regions_of_the_current_text() {
        let mut d = doc("a\nb\n", "a\nB\n");
        d.edit(Side::Left, insert(0, "z\n")).expect("edit");
        // Left is now z a b: z is inserted, b is changed, and that change is region 2.
        assert_eq!(
            d.copy_region(RegionIx::new(2), Side::Right),
            edited(Side::Right, 2, vec![patch(2..4, "b\n")])
        );
        assert_eq!(d.text(Side::Right), "a\nb\n");
    }

    #[test]
    fn copying_a_missing_region_fails() {
        let mut d = doc("a\n", "b\n");
        assert_eq!(
            d.copy_region(RegionIx::new(1), Side::Left),
            Err(DiffEditError::NoRegion { region: RegionIx::new(1), regions: 1 })
        );
    }

    #[test]
    fn each_side_has_its_own_history() {
        let mut d = doc("a\n", "a\n");
        d.edit(Side::Left, insert(0, "L\n")).expect("left");
        d.edit(Side::Right, insert(0, "R\n")).expect("right");
        assert_eq!(d.undo(Side::Left), at(Side::Left, 3, vec![patch(0..2, "")]));
        assert_eq!(d.text(Side::Left), "a\n");
        assert_eq!(d.text(Side::Right), "R\na\n");
        assert_eq!(d.undo(Side::Left), at(Side::Left, 3, vec![]));
    }

    #[test]
    fn typing_at_the_caret_is_one_undo_step() {
        let mut d = doc("", "");
        for (at, c) in [(0, "a"), (1, "b"), (2, "c")] {
            d.edit(Side::Left, insert(at, c)).expect("typed");
        }
        d.edit(Side::Left, insert(3, "\n")).expect("newline");
        assert_eq!(d.undo(Side::Left), at(Side::Left, 5, vec![patch(3..4, "")]));
        assert_eq!(d.undo(Side::Left), at(Side::Left, 6, vec![patch(0..3, "")]));
    }

    #[test]
    fn typing_away_from_the_caret_or_after_another_step_is_a_new_step() {
        let mut d = doc("xy\n", "xy\n");
        d.edit(Side::Left, insert(0, "a")).expect("a");
        d.edit(Side::Left, insert(3, "b")).expect("b away from the caret");
        assert_eq!(d.undo(Side::Left), at(Side::Left, 3, vec![patch(3..4, "")]));
        assert_eq!(d.undo(Side::Left), at(Side::Left, 4, vec![patch(0..1, "")]));

        d.edit(Side::Right, insert(0, "a")).expect("a");
        d.undo(Side::Right);
        d.redo(Side::Right);
        d.edit(Side::Right, insert(1, "b")).expect("b after redo");
        assert_eq!(d.undo(Side::Right), at(Side::Right, 9, vec![patch(1..2, "")]));
        assert_eq!(d.undo(Side::Right), at(Side::Right, 10, vec![patch(0..1, "")]));
    }

    #[test]
    fn a_newline_typed_into_a_crlf_side_gets_a_carriage_return() {
        let mut d = doc("a\r\n", "a\n");
        assert_eq!(
            d.edit(Side::Left, insert(1, "\n")),
            edited(Side::Left, 1, vec![patch(1..1, "\r")])
        );
        assert_eq!(d.text(Side::Left), "a\r\n\r\n");
        assert_eq!(
            d.edit(Side::Right, insert(1, "\r\n")),
            edited(Side::Right, 2, vec![patch(1..2, "")])
        );
        assert_eq!(d.text(Side::Right), "a\n\n");
    }

    #[test]
    fn a_malformed_batch_fails_and_changes_nothing() {
        let mut d = doc("a\n", "a\n");
        assert_eq!(
            d.edit(Side::Left, EditBatch(vec![patch(1..5, "")])),
            Err(DiffEditError::Edit(BatchError::BadRange { range: 1..5, len: 2 }))
        );
        assert_eq!(d.text(Side::Left), "a\n");
        assert_eq!(d.version(), 0);
        assert_eq!(d.edit(Side::Left, EditBatch(vec![])), edited(Side::Left, 0, vec![]));
    }
}
