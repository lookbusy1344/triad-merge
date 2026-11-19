//! Two-way diff (spec §4.6): regions, and a document of two editable buffers.

use std::iter::Peekable;
use std::ops::Range;

/// The rope that holds each side, which [`DiffDocument::text`] and [`DiffDocument::snapshot`]
/// return. Front ends name it here, so the core alone picks the rope crate.
pub use ropey::{Rope, RopeSlice};

use crate::diff::{BlankLineRule, DiffOptions, Hunk, diff_lines, intraline_hunks, is_blank_line};
pub use crate::edit::{BatchError, EditBatch, TextPatch};
use crate::edit::{apply, caret_after, inverse, prepare};
use crate::index::RegionIx;
use crate::patch::{self, Scope, Titles};
use crate::text::{Encoding, LineEnding, Text};

mod moves;
pub use moves::{MOVE_MIN_LINES, Move, moves};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind2 {
    Unchanged,
    /// Either range may be empty, for a pure insertion or deletion.
    Changed,
    /// A change the comparison ignores: under [`BlankLineRule::Ignore`], one whose lines on
    /// both sides are all blank. It is no difference, and either range may be empty.
    Ignored,
}

/// One stretch of both files, as line ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region2 {
    pub kind: Kind2,
    pub left: Range<usize>,
    pub right: Range<usize>,
}

/// The regions that cover both files in order, from one diff left→right. Equal stretches
/// between changes are `Unchanged`, and a change the options ignore is `Ignored`. Identical
/// files, empty ones included, give one `Unchanged` region.
#[must_use]
pub fn regions(left: &Text, right: &Text, opts: &DiffOptions) -> Vec<Region2> {
    let hunks = diff_lines(left, right, opts);
    let mut regions = Vec::with_capacity(2 * hunks.len() + 1);
    let mut at = (0, 0);
    for hunk in hunks {
        let start = (hunk.before.start, hunk.after.start);
        if start != at {
            regions.push(unchanged(at, start));
        }
        at = (hunk.before.end, hunk.after.end);
        let kind = if ignores(&hunk, left, right, opts) { Kind2::Ignored } else { Kind2::Changed };
        regions.push(Region2 { kind, left: hunk.before, right: hunk.after });
    }
    let end = (left.line_count(), right.line_count());
    if end != at || regions.is_empty() {
        regions.push(unchanged(at, end));
    }
    regions
}

/// Whether `opts` ignore `hunk`: every line it holds on either side is blank.
fn ignores(hunk: &Hunk, left: &Text, right: &Text, opts: &DiffOptions) -> bool {
    let blank = |text: &Text, lines: &Range<usize>| {
        lines.clone().all(|n| is_blank_line(text.line(n), &opts.patterns))
    };
    opts.blank_lines == BlankLineRule::Ignore
        && blank(left, &hunk.before)
        && blank(right, &hunk.after)
}

/// The characters that differ within paired lines of the changed regions (spec §5,
/// intra-line highlights), as byte ranges into each side's text, sorted and disjoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    pub left: Vec<Range<usize>>,
    pub right: Vec<Range<usize>>,
}

impl Marks {
    /// `side`'s marks.
    #[must_use]
    pub fn side(&self, side: Side) -> &[Range<usize>] {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }
}

/// The [`Marks`] of `regions`. In each changed region the lines outside `moves`' ends pair in
/// order, the n-th left line with the n-th right line. A pair marks the characters that
/// differ, terminators left out; a line over [`MAX_INTRALINE_DIFF_CHARS`] characters is
/// marked whole. A line without a partner gets no marks.
///
/// [`MAX_INTRALINE_DIFF_CHARS`]: crate::diff::MAX_INTRALINE_DIFF_CHARS
#[must_use]
pub fn marks(left: &Text, right: &Text, regions: &[Region2], moves: &[Move]) -> Marks {
    let ends = |end: fn(&Move) -> (RegionIx, Range<usize>)| {
        let mut ends: Vec<_> = moves.iter().map(end).collect();
        ends.sort_by_key(|(region, lines)| (*region, lines.start));
        ends.into_iter().peekable()
    };
    let mut left_ends = ends(|m| (m.from, m.left.clone()));
    let mut right_ends = ends(|m| (m.to, m.right.clone()));
    let mut marks = Marks::default();
    for (index, region) in regions.iter().enumerate() {
        let here = RegionIx::new(index);
        let (left_out, right_out) = (ends_in(&mut left_ends, here), ends_in(&mut right_ends, here));
        if region.kind != Kind2::Changed {
            continue;
        }
        let pairs =
            kept(region.left.clone(), &left_out).zip(kept(region.right.clone(), &right_out));
        for (l, r) in pairs {
            mark_pair(&Pair::of(left, l), &Pair::of(right, r), &mut marks);
        }
    }
    marks
}

/// The move ends in region `here`, taken from `ends`, which are sorted by region.
fn ends_in(
    ends: &mut Peekable<impl Iterator<Item = (RegionIx, Range<usize>)>>,
    here: RegionIx,
) -> Vec<Range<usize>> {
    std::iter::from_fn(|| ends.next_if(|(region, _)| *region <= here))
        .filter_map(|(region, lines)| (region == here).then_some(lines))
        .collect()
}

/// `lines` without those in `out`.
fn kept(lines: Range<usize>, out: &[Range<usize>]) -> impl Iterator<Item = usize> + '_ {
    lines.filter(|n| !out.iter().any(|lines| lines.contains(n)))
}

/// One line of a pair: its text without the terminator, and where it starts in its file.
struct Pair<'t> {
    content: &'t str,
    at: usize,
}

impl<'t> Pair<'t> {
    fn of(text: &'t Text, line: usize) -> Self {
        let whole = text.line(line);
        let content = whole.strip_suffix('\n').map_or(whole, |l| l.strip_suffix('\r').unwrap_or(l));
        Self { content, at: text.span(0..line).len() }
    }

    fn shifted(&self, bytes: Range<usize>) -> Range<usize> {
        self.at + bytes.start..self.at + bytes.end
    }
}

fn mark_pair(left: &Pair<'_>, right: &Pair<'_>, marks: &mut Marks) {
    if left.content == right.content {
        return;
    }
    let whole = || Hunk { before: 0..left.content.len(), after: 0..right.content.len() };
    let hunks = intraline_hunks(left.content, right.content).unwrap_or_else(|| vec![whole()]);
    for Hunk { before, after } in hunks {
        if !before.is_empty() {
            marks.left.push(left.shifted(before));
        }
        if !after.is_empty() {
            marks.right.push(right.shifted(after));
        }
    }
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
    moves: Vec<Move>,
    marks: Marks,
    regions_version: u64,
    version: u64,
}

/// The regions, moves and intra-line marks of one version of both texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diffed {
    pub version: u64,
    pub regions: Vec<Region2>,
    pub moves: Vec<Move>,
    pub marks: Marks,
}

/// A snapshot of both texts to diff off the UI thread. Cloning a rope is O(1).
#[derive(Debug, Clone)]
pub struct Recompute {
    version: u64,
    left: Rope,
    right: Rope,
    options: DiffOptions,
}

/// A snapshot of both texts and their regions, to compute their marks off the UI thread.
/// Cloning a rope is O(1).
#[derive(Debug, Clone)]
pub struct Marking {
    version: u64,
    left: Rope,
    right: Rope,
    regions: Vec<Region2>,
    moves: Vec<Move>,
}

impl Marking {
    /// The marks, and the version they belong to.
    #[must_use]
    pub fn run(self) -> (u64, Marks) {
        let text = |rope: Rope| Text::new(Encoding::Utf8, rope.to_string());
        let (left, right) = (text(self.left), text(self.right));
        (self.version, marks(&left, &right, &self.regions, &self.moves))
    }
}

/// A snapshot of both texts and their regions, to write a patch off the UI thread. Cloning a
/// rope is O(1).
#[derive(Debug, Clone)]
pub struct Patching {
    left: Rope,
    right: Rope,
    regions: Vec<Region2>,
    scope: Scope,
}

impl Patching {
    /// The unified diff of the snapshot's `scope`, with `context` lines around each change.
    #[must_use]
    pub fn run(self, context: usize, titles: Titles<'_>) -> String {
        let text = |rope: Rope| Text::new(Encoding::Utf8, rope.to_string());
        let (left, right) = (text(self.left), text(self.right));
        patch::unified(&left, &right, &self.regions, context, self.scope, titles)
    }
}

impl Recompute {
    /// The regions, moves and marks, and the version they belong to.
    #[must_use]
    pub fn run(self) -> Diffed {
        self.run_with(|_| ()).0
    }

    /// [`Recompute::run`], also handing the snapshot's texts, left then right, to `also`. Its
    /// result belongs to the same version.
    #[must_use]
    pub fn run_with<T>(self, also: impl FnOnce([&Text; 2]) -> T) -> (Diffed, T) {
        // The encoding plays no part in diffing.
        let text = |rope: Rope| Text::new(Encoding::Utf8, rope.to_string());
        let (left, right) = (text(self.left), text(self.right));
        let regions = regions(&left, &right, &self.options);
        let moves = moves(&left, &right, &regions, &self.options);
        let marks = marks(&left, &right, &regions, &moves);
        (Diffed { version: self.version, regions, moves, marks }, also([&left, &right]))
    }
}

impl DiffDocument {
    #[must_use]
    pub fn new(left: &Text, right: &Text, options: DiffOptions) -> Self {
        let regions = regions(left, right, &options);
        Self {
            moves: moves(left, right, &regions, &options),
            marks: Marks::default(),
            regions,
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

    /// The moves among the regions last installed, ordered by the region each moves from.
    #[must_use]
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// The intra-line marks of the regions last installed, in bytes of the text they were
    /// computed from. A new document has none until [`DiffDocument::marking`]'s are
    /// installed, since they can cost more than the diff.
    #[must_use]
    pub fn marks(&self) -> &Marks {
        &self.marks
    }

    /// A snapshot to compute the marks of the current regions off the UI thread, or `None`
    /// while the regions lag the text: a recompute brings its own.
    #[must_use]
    pub fn marking(&self) -> Option<Marking> {
        self.regions_current().then(|| Marking {
            version: self.version,
            left: self.left.text.clone(),
            right: self.right.text.clone(),
            regions: self.regions.clone(),
            moves: self.moves.clone(),
        })
    }

    /// A snapshot to write `scope`'s patch off the UI thread, or `None` while the regions lag
    /// the text, so a region index names a region of the text the patch shows.
    #[must_use]
    pub fn patching(&self, scope: Scope) -> Option<Patching> {
        self.regions_current().then(|| Patching {
            left: self.left.text.clone(),
            right: self.right.text.clone(),
            regions: self.regions.clone(),
            scope,
        })
    }

    /// Install marks computed for version `version`. Marks for another version are dropped:
    /// returns whether they were installed.
    pub fn install_marks(&mut self, version: u64, marks: Marks) -> bool {
        let current = version == self.version && self.regions_current();
        if current {
            self.marks = marks;
        }
        current
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
            let diffed = self.recompute().run();
            self.install(diffed);
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
            options: self.options.clone(),
        }
    }

    /// Install regions and moves computed for their version. A result for an older version is
    /// dropped: returns whether it was installed.
    pub fn install(&mut self, diffed: Diffed) -> bool {
        let current = diffed.version == self.version;
        if current {
            self.regions = diffed.regions;
            self.moves = diffed.moves;
            self.marks = diffed.marks;
            self.regions_version = diffed.version;
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
#[expect(clippy::single_range_in_vec_init, reason = "marks are lists of ranges, often of one")]
mod tests {
    use super::*;
    use crate::diff::{BlankLineRule, MAX_INTRALINE_DIFF_CHARS, Whitespace};
    use crate::pattern::Patterns;
    use crate::testing::text;

    fn of(left: &str, right: &str) -> Vec<Region2> {
        regions(&text(left), &text(right), &DiffOptions::default())
    }

    fn same(left: Range<usize>, right: Range<usize>) -> Region2 {
        Region2 { kind: Kind2::Unchanged, left, right }
    }

    fn changed(left: Range<usize>, right: Range<usize>) -> Region2 {
        Region2 { kind: Kind2::Changed, left, right }
    }

    const TITLES: Titles<'static> = Titles { left: "l", right: "r" };

    #[test]
    fn patching_writes_the_current_texts_and_waits_for_current_regions() {
        let mut d = doc("a\nb\nc\n", "a\nB\nc\n");
        let whole = d.patching(Scope::File).expect("regions are current").run(1, TITLES);
        assert_eq!(whole, "--- l\n+++ r\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n");
        d.edit(Side::Right, insert(0, "z\n")).expect("edits");
        assert!(d.patching(Scope::File).is_none(), "the regions lag the edit");
        assert!(d.install(d.recompute().run()));
        assert_eq!(
            d.patching(Scope::File).expect("current again").run(0, TITLES),
            "--- l\n+++ r\n@@ -0,0 +1 @@\n+z\n@@ -2 +3 @@ a\n-b\n+B\n"
        );
    }

    #[test]
    fn patching_a_region_writes_that_change_alone() {
        let d = doc("a\nb\nc\nd\n", "A\nb\nc\nD\n");
        // Regions: 0 changed, 1 unchanged, 2 changed.
        let region =
            |n| d.patching(Scope::Region(RegionIx::new(n))).expect("current").run(1, TITLES);
        assert_eq!(region(2), "--- l\n+++ r\n@@ -3,2 +3,2 @@ b\n c\n-d\n+D\n");
        assert_eq!(region(1), "");
    }

    #[test]
    fn regions_are_current_until_an_edit_and_again_once_installed() {
        let mut d = doc("a\n", "b\n");
        assert!(d.regions_current());
        d.edit(Side::Left, insert(0, "x")).expect("edits");
        assert!(!d.regions_current());
        let stale = d.recompute();
        d.edit(Side::Left, insert(0, "y")).expect("edits");
        assert!(!d.install(stale.run()));
        assert!(!d.regions_current(), "a stale result is not installed");
        assert!(d.install(d.recompute().run()));
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
            regions(&text(left), &text(right), &opts)
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

    fn ignored(left: Range<usize>, right: Range<usize>) -> Region2 {
        Region2 { kind: Kind2::Ignored, left, right }
    }

    fn ignoring_blank_lines(left: &str, right: &str, patterns: Patterns) -> Vec<Region2> {
        let opts =
            DiffOptions { blank_lines: BlankLineRule::Ignore, patterns, ..DiffOptions::default() };
        regions(&text(left), &text(right), &opts)
    }

    fn blank(left: &str, right: &str) -> Vec<Region2> {
        ignoring_blank_lines(left, right, Patterns::empty())
    }

    #[test]
    fn blank_lines_differ_unless_ignored() {
        assert_eq!(
            of("a\n\nb\n", "a\nb\n"),
            [same(0..1, 0..1), changed(1..2, 1..1), same(2..3, 1..2)]
        );
        assert_eq!(
            blank("a\n\nb\n", "a\nb\n"),
            [same(0..1, 0..1), ignored(1..2, 1..1), same(2..3, 1..2)]
        );
        assert_eq!(blank("a\n", "\n\na\n"), [ignored(0..0, 0..2), same(0..1, 2..3)]);
    }

    #[test]
    fn a_line_of_spaces_tabs_and_carriage_returns_is_blank() {
        assert_eq!(
            blank("a\n \t\nb\n", "a\nb\n"),
            [same(0..1, 0..1), ignored(1..2, 1..1), same(2..3, 1..2)]
        );
        assert_eq!(
            blank("a\n\r\nb\n", "a\nb\n"),
            [same(0..1, 0..1), ignored(1..2, 1..1), same(2..3, 1..2)]
        );
        assert_eq!(
            blank("a\n  \nb\n", "a\n\nb\n"),
            [same(0..1, 0..1), ignored(1..2, 1..2), same(2..3, 2..3)],
            "a blank line replaced by another"
        );
    }

    #[test]
    fn other_white_space_is_not_blank() {
        assert_eq!(
            blank("a\n\u{a0}\nb\n", "a\nb\n"),
            [same(0..1, 0..1), changed(1..2, 1..1), same(2..3, 1..2)]
        );
        assert_eq!(
            blank("a\n\u{c}\nb\n", "a\nb\n"),
            [same(0..1, 0..1), changed(1..2, 1..1), same(2..3, 1..2)]
        );
    }

    #[test]
    fn a_change_with_one_line_not_blank_stays_a_change() {
        assert_eq!(blank("a\nb\n", "a\n\nc\n"), [same(0..1, 0..1), changed(1..2, 1..3)]);
    }

    #[test]
    fn a_blank_last_line_without_a_newline_is_blank() {
        assert_eq!(blank("a\n", "a\n  "), [same(0..1, 0..1), ignored(1..1, 1..2)]);
    }

    #[test]
    fn a_missing_final_newline_on_a_line_with_text_is_not_blank() {
        assert_eq!(blank("a", "a\n"), [changed(0..1, 0..1)]);
    }

    #[test]
    fn a_line_a_pattern_empties_is_blank() {
        let comments = || Patterns::new(["//.*"]).expect("valid");
        assert_eq!(
            ignoring_blank_lines("a\n  // note\nb\n", "a\nb\n", comments()),
            [same(0..1, 0..1), ignored(1..2, 1..1), same(2..3, 1..2)]
        );
        assert_eq!(
            ignoring_blank_lines("a\nx // note\nb\n", "a\nb\n", comments()),
            [same(0..1, 0..1), changed(1..2, 1..1), same(2..3, 1..2)]
        );
    }

    #[test]
    fn files_that_differ_only_in_blank_lines_have_no_change() {
        let regions = blank("\na\n\n\nb\n", "a\nb\n\n");
        assert_eq!(regions.iter().filter(|r| r.kind == Kind2::Changed).count(), 0);
        assert_eq!(
            regions,
            [
                ignored(0..1, 0..0),
                same(1..2, 0..1),
                ignored(2..4, 1..1),
                same(4..5, 1..2),
                ignored(5..5, 2..3),
            ]
        );
    }

    #[test]
    fn a_whitespace_mode_can_make_files_equal() {
        let opts = DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() };
        assert_eq!(regions(&text("a \nb\n"), &text("a\nb\r\n"), &opts), [same(0..2, 0..2)]);
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

    fn moved(from: usize, to: usize, left: Range<usize>, right: Range<usize>) -> Move {
        Move { from: RegionIx::new(from), to: RegionIx::new(to), left, right }
    }

    #[test]
    fn moves_lag_the_text_with_the_regions_and_install_with_them() {
        let mut d = doc("m1\nm2\na\nb\nc\n", "a\nb\nc\nm1\nm2\n");
        assert_eq!(d.moves(), [moved(0, 2, 0..2, 3..5)]);
        d.edit(Side::Right, insert(11, "!")).expect("edits");
        assert_eq!(d.text(Side::Right), "a\nb\nc\nm1\nm2!\n");
        assert_eq!(d.moves(), [moved(0, 2, 0..2, 3..5)], "moves wait for the recompute");
        let diffed = d.recompute().run();
        assert_eq!(diffed.moves, []);
        assert!(d.install(diffed));
        assert_eq!(d.moves(), []);
    }

    #[test]
    fn a_document_matches_moves_under_its_options() {
        let all = DiffOptions { whitespace: Whitespace::All, ..DiffOptions::default() };
        let mut d = DiffDocument::new(&text("m1\nm2\na\nb\nc\n"), &text("a\nb\nc\nm 1\nm2\n"), all);
        assert_eq!(d.moves(), [moved(0, 2, 0..2, 3..5)]);
        d.edit(Side::Left, insert(12, "q\n")).expect("edits");
        assert_eq!(d.recompute().run().moves, [moved(0, 2, 0..2, 3..5)], "a recompute keeps them");
        assert_eq!(doc("m1\nm2\na\nb\nc\n", "a\nb\nc\nm 1\nm2\n").moves(), []);
    }

    #[test]
    fn an_edit_takes_a_new_version_and_regions_wait_for_the_recompute() {
        let mut d = doc("a\nb\nc\n", "a\nb\nc\n");
        assert_eq!(d.edit(Side::Left, insert(2, "x")), edited(Side::Left, 1, vec![]));
        assert_eq!(d.text(Side::Left), "a\nxb\nc\n");
        assert_eq!(d.regions(), [same(0..3, 0..3)]);
        let diffed = d.recompute().run();
        let regions = vec![same(0..1, 0..1), changed(1..2, 1..2), same(2..3, 2..3)];
        let marks = Marks { left: vec![2..3], right: vec![] };
        assert_eq!(diffed, Diffed { version: 1, regions: regions.clone(), moves: vec![], marks });
        assert!(d.install(diffed));
        assert_eq!(d.regions(), regions);
    }

    #[test]
    fn a_recompute_hands_its_snapshot_of_both_texts_on() {
        let mut d = doc("a\n", "b\n");
        let job = d.recompute();
        d.edit(Side::Left, insert(0, "x")).expect("edit");
        let (diffed, seen) =
            job.run_with(|[left, right]| (left.content().to_owned(), right.content().to_owned()));
        assert_eq!(diffed.version, 0);
        assert_eq!(diffed.regions, [changed(0..1, 0..1)]);
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
        let diffed = job.run();
        assert_eq!(diffed.version, 0);
        assert!(!d.install(diffed));
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

    // Intra-line marks

    fn marks_of(left: &str, right: &str) -> Marks {
        let (left, right) = (text(left), text(right));
        let opts = DiffOptions::default();
        let found = regions(&left, &right, &opts);
        marks(&left, &right, &found, &moves(&left, &right, &found, &opts))
    }

    fn marked(left: &[Range<usize>], right: &[Range<usize>]) -> Marks {
        Marks { left: left.to_vec(), right: right.to_vec() }
    }

    #[test]
    fn a_replaced_word_is_marked_on_both_sides() {
        assert_eq!(marks_of("let x = 1;\n", "let y = 1;\n"), marked(&[4..5], &[4..5]));
    }

    #[test]
    fn marks_are_bytes_into_the_whole_text() {
        // "a\n" is 2 bytes and "foo ba" 6, so the changed letter sits at byte 8.
        assert_eq!(marks_of("a\nfoo bar\n", "a\nfoo baz\n"), marked(&[8..9], &[8..9]));
    }

    #[test]
    fn an_insertion_within_a_line_marks_one_side_only() {
        assert_eq!(marks_of("ab\n", "aXb\n"), marked(&[], &[1..2]));
        assert_eq!(marks_of("aXb\n", "ab\n"), marked(&[1..2], &[]));
    }

    #[test]
    fn multibyte_characters_give_byte_ranges_on_their_boundaries() {
        // "é" is 2 bytes; "ü" is 2 bytes, after "é " at byte 3.
        assert_eq!(marks_of("é x\n", "é ü\n"), marked(&[3..4], &[3..5]));
    }

    #[test]
    fn the_nth_line_of_each_side_pairs_and_extra_lines_get_no_marks() {
        assert_eq!(marks_of("x1\nx2\n", "y1\n"), marked(&[0..1], &[0..1]));
        assert_eq!(marks_of("q\nab\n", "q\nAb\nnew\n"), marked(&[2..3], &[2..3]));
    }

    #[test]
    fn pure_insertions_and_deletions_get_no_marks() {
        assert_eq!(marks_of("a\n", "a\nb\n"), Marks::default());
        assert_eq!(marks_of("a\nb\n", "b\n"), Marks::default());
    }

    #[test]
    fn identical_files_get_no_marks() {
        assert_eq!(marks_of("a b\n", "a b\n"), Marks::default());
    }

    #[test]
    fn line_terminators_are_never_marked() {
        assert_eq!(marks_of("ab", "ab\n"), Marks::default());
        assert_eq!(marks_of("ab\r\n", "ab\n"), Marks::default());
        assert_eq!(marks_of("ab\r\n", "aB\n"), marked(&[1..2], &[1..2]));
    }

    #[test]
    fn ignored_regions_get_no_marks() {
        let (left, right) = (text("a\n \n"), text("a\n\t\n"));
        let opts = DiffOptions { blank_lines: BlankLineRule::Ignore, ..DiffOptions::default() };
        let found = regions(&left, &right, &opts);
        assert_eq!(found, [same(0..1, 0..1), ignored(1..2, 1..2)]);
        assert_eq!(marks(&left, &right, &found, &[]), Marks::default());
    }

    #[test]
    fn a_line_too_long_for_a_character_diff_is_marked_whole_without_its_terminator() {
        let long = "a".repeat(MAX_INTRALINE_DIFF_CHARS + 1);
        let left = format!("x\n{long}\n");
        let right = format!("x\n{long}b\n");
        let line = 2..2 + long.len();
        assert_eq!(
            marks_of(&left, &right),
            marked(std::slice::from_ref(&line), &[2..line.end + 1])
        );
    }

    #[test]
    fn lines_in_a_move_end_get_no_marks() {
        let (left, right) = ("m1\nm2\na\nb\nc\nx\n", "y\na\nb\nc\nm1\nm2\n");
        let found = regions(&text(left), &text(right), &DiffOptions::default());
        let found_moves = moves(&text(left), &text(right), &found, &DiffOptions::default());
        assert_eq!(found_moves, [moved(0, 2, 0..2, 4..6)], "the block m1, m2 moves");
        assert_eq!(marks_of(left, right), Marks::default());
    }

    #[test]
    fn lines_beside_a_move_end_pair_among_themselves() {
        // Region 2 on the right holds "m1", "m2" from the left's top, then "zq" against "zp".
        let (left, right) = ("m1\nm2\na\nb\nc\nzp\n", "a\nb\nc\nm1\nm2\nzq\n");
        let found = regions(&text(left), &text(right), &DiffOptions::default());
        assert_eq!(found, [changed(0..2, 0..0), same(2..5, 0..3), changed(5..6, 3..6)]);
        assert_eq!(
            moves(&text(left), &text(right), &found, &DiffOptions::default()),
            [moved(0, 2, 0..2, 3..5)]
        );
        // "zp" pairs with "zq", past the move end; both start at byte 12.
        assert_eq!(marks_of(left, right), marked(&[13..14], &[13..14]));
    }

    #[test]
    fn a_new_document_has_no_marks_until_its_marking_installs() {
        let mut d = doc("a\nb c\n", "a\nb d\n");
        assert_eq!(d.marks(), &Marks::default());
        let (version, marks) = d.marking().expect("regions are current").run();
        assert_eq!((version, &marks), (0, &marked(&[4..5], &[4..5])));
        assert!(d.install_marks(version, marks));
        assert_eq!(d.marks(), &marked(&[4..5], &[4..5]));
    }

    #[test]
    fn marks_of_an_older_version_are_dropped() {
        let mut d = doc("a\nb c\n", "a\nb d\n");
        let marking = d.marking().expect("regions are current");
        d.edit(Side::Left, insert(0, "zz")).expect("edits");
        assert!(d.marking().is_none(), "regions lag the text");
        let (version, marks) = marking.run();
        assert!(!d.install_marks(version, marks));
        assert_eq!(d.marks(), &Marks::default());
    }

    #[test]
    fn marks_install_with_the_regions_of_a_recompute() {
        let mut d = doc("a\nb c\n", "a\nb d\n");
        let (version, marks) = d.marking().expect("regions are current").run();
        assert!(d.install_marks(version, marks));
        assert_eq!(d.marks(), &marked(&[4..5], &[4..5]));
        assert_eq!(d.marks().side(Side::Left), [4..5]);
        d.edit(Side::Left, insert(0, "zz")).expect("edits");
        assert_eq!(d.marks(), &marked(&[4..5], &[4..5]), "marks wait for the recompute");
        let diffed = d.recompute().run();
        assert_eq!(diffed.marks, marked(&[0..2, 6..7], &[4..5]));
        assert!(d.install(diffed));
        assert_eq!(d.marks().side(Side::Left), [0..2, 6..7]);
    }
}
