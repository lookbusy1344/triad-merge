//! Aligned rows for the top panes (spec §5): each region takes as many rows as its longest
//! visible pane, and shorter panes get filler rows at the region's end.

use merge_core::document::{Applied, MergeDocument};
use merge_core::index::RegionIx;
use merge_core::lengths::RegionLengths;
use merge_core::merge3::{Region, Source};
use smallvec::SmallVec;
use std::fmt;

/// The inputs shown in the top panes, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneSet {
    LeftRight,
    LeftBaseRight,
    /// The first n sides of an n-way merge, without a base.
    Sides(usize),
}

impl PaneSet {
    /// The input each pane shows, in order.
    #[must_use]
    pub fn sources(self) -> SmallVec<[Source; PANES]> {
        match self {
            PaneSet::LeftRight => [Source::Side(0), Source::Side(1)].into_iter().collect(),
            PaneSet::LeftBaseRight => {
                [Source::Side(0), Source::Base(0), Source::Side(1)].into_iter().collect()
            }
            PaneSet::Sides(n) => (0..n).map(Source::Side).collect(),
        }
    }
}

/// Panes held inline in a [`Row`]; more spill to the heap.
const PANES: usize = 3;

/// One row of the top panes: per pane, the input line it shows, or `None` for a filler row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub region: RegionIx,
    pub cells: SmallVec<[Option<usize>; PANES]>,
}

/// The document lacks an input a pane shows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no {input:?} to show: the merge has {sides} sides and {bases} bases")]
pub struct PaneError {
    pub input: Source,
    pub sides: usize,
    pub bases: usize,
}

/// The index of a row of the top panes. A row is not a result line: a region can hold more of
/// one than of the other.
///
/// ```
/// # use merge_core::index::RegionIx;
/// # use merge_view::rows::{RowIx, RowMap};
/// fn region(rows: &RowMap, row: RowIx) -> RegionIx {
///     rows.region_of_row(row)
/// }
/// ```
///
/// ```compile_fail,E0308
/// # use merge_core::index::{Line, RegionIx};
/// # use merge_view::rows::RowMap;
/// fn region(rows: &RowMap, line: Line) -> RegionIx {
///     rows.region_of_row(line)
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowIx(usize);

impl RowIx {
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// The row `rows` below this one.
    #[must_use]
    pub const fn plus(self, rows: usize) -> Self {
        Self(self.0 + rows)
    }

    /// How many rows this one is below `first`.
    ///
    /// # Panics
    ///
    /// If this row is above `first`.
    #[must_use]
    pub const fn since(self, first: Self) -> usize {
        self.0 - first.0
    }
}

impl fmt::Display for RowIx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Row counts per region. Finding a row's region and a region's first row cost O(log n).
#[derive(Debug, Clone)]
pub struct RowMap {
    sources: SmallVec<[Source; PANES]>,
    rows: RegionLengths,
}

impl RowMap {
    /// # Errors
    ///
    /// [`PaneError`] when `panes` shows an input `doc` does not have.
    pub fn new(doc: &MergeDocument, panes: PaneSet) -> Result<Self, PaneError> {
        let sources = panes.sources();
        let (sides, bases) =
            doc.regions().first().map_or((0, 0), |r| (r.sides.len(), r.bases.len()));
        if let Some(&input) = sources.iter().find(|&&s| match s {
            Source::Side(i) => i >= sides,
            Source::Base(i) => i >= bases,
        }) {
            return Err(PaneError { input, sides, bases });
        }
        let rows = RegionLengths::new(Self::counts(&sources, doc.regions()));
        Ok(Self { sources, rows })
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.total()
    }

    /// [`Self::row`] for a row held across commands: none past the last row.
    #[must_use]
    pub fn get_row(&self, doc: &MergeDocument, index: RowIx) -> Option<Row> {
        (index.get() < self.row_count()).then(|| self.row(doc, index))
    }

    /// # Panics
    ///
    /// If `index` is not below [`RowMap::row_count`].
    #[must_use]
    pub fn row(&self, doc: &MergeDocument, index: RowIx) -> Row {
        let region = self.region_of_row(index);
        let k = index.since(self.first_row(region));
        let r = doc.region(region);
        Row {
            region,
            cells: self
                .sources
                .iter()
                .map(|&s| {
                    let lines = range(r, s);
                    (k < lines.len()).then_some(lines.start + k)
                })
                .collect(),
        }
    }

    /// The region showing row `index`.
    ///
    /// # Panics
    ///
    /// If `index` is not below [`RowMap::row_count`].
    #[must_use]
    #[expect(
        clippy::panic,
        reason = "a row past the end is a caller's bug, as with slice indexing"
    )]
    pub fn region_of_row(&self, index: RowIx) -> RegionIx {
        let region = self
            .rows
            .containing(index.get())
            .unwrap_or_else(|| panic!("row {index} of {}", self.row_count()));
        RegionIx::new(region)
    }

    /// The first row of `region`. A region with no rows starts where the next one does.
    ///
    /// # Panics
    ///
    /// If `region` is past the region count.
    #[must_use]
    pub fn first_row(&self, region: RegionIx) -> RowIx {
        RowIx::new(self.rows.start(region.get()))
    }

    /// The row showing `source`'s line `line` among `regions`, the document's; `None` when no pane shows `source` or the line
    /// is past its end.
    #[must_use]
    pub fn row_of_input_line(
        &self,
        regions: &[Region],
        source: Source,
        line: usize,
    ) -> Option<RowIx> {
        if !self.sources.contains(&source) {
            return None;
        }
        // Each input's lines run through the regions in order, so the first region ending
        // after `line` holds it.
        let region = regions.partition_point(|r| range(r, source).end <= line);
        let lines = range(regions.get(region)?, source);
        lines
            .contains(&line)
            .then(|| self.first_row(RegionIx::new(region)).plus(line - lines.start))
    }

    /// The line of `source` row `index` shows: `Ok` with the line, or `Err` with the line after
    /// the row on a filler row, which is the line count past the input's last line.
    ///
    /// # Errors
    ///
    /// The line after a filler row, as above.
    ///
    /// # Panics
    ///
    /// If `index` is not below [`RowMap::row_count`].
    pub fn line_at_row(
        &self,
        doc: &MergeDocument,
        index: RowIx,
        source: Source,
    ) -> Result<usize, usize> {
        let region = self.region_of_row(index);
        let k = index.since(self.first_row(region));
        let lines = range(doc.region(region), source);
        if k < lines.len() { Ok(lines.start + k) } else { Err(lines.end) }
    }

    /// Rows of `region`.
    #[must_use]
    pub fn rows_in(&self, region: RegionIx) -> usize {
        self.rows.len(region.get())
    }

    /// Follow a command. Rows change only when regions merged or split.
    pub fn update(&mut self, doc: &MergeDocument, applied: &Applied) {
        if applied.regions_changed {
            self.rows = RegionLengths::new(Self::counts(&self.sources, doc.regions()));
        }
    }

    fn counts(sources: &[Source], regions: &[Region]) -> Vec<usize> {
        regions
            .iter()
            .map(|r| sources.iter().map(|&s| range(r, s).len()).max().unwrap_or(0))
            .collect()
    }
}

/// The base lines `source` deleted in `region`: some when `source` is a side with no lines in
/// it and the first base has lines. Its filler rows mark a deletion,
/// not lines another input added.
#[must_use]
pub fn deleted_lines(region: &Region, source: Source) -> Option<usize> {
    let base = region.bases.first().map_or(0, ExactSizeIterator::len);
    (matches!(source, Source::Side(_)) && range(region, source).is_empty() && base > 0)
        .then_some(base)
}

fn range(region: &Region, source: Source) -> std::ops::Range<usize> {
    match source {
        Source::Side(i) => region.sides[i].clone(),
        Source::Base(i) => region.bases[i].clone(),
    }
}

#[cfg(test)]
mod tests {
    use merge_core::document::{Command, EditBatch, Labels, MergeOptions, TextPatch};
    use merge_core::merge3::{Inputs, Kind};
    use merge_core::testing::{doc, text};

    use super::*;

    fn rows(doc: &MergeDocument, panes: PaneSet) -> Vec<(usize, Vec<Option<usize>>)> {
        let map = RowMap::new(doc, panes).expect("panes fit");
        (0..map.row_count())
            .map(|i| {
                let row = map.row(doc, RowIx::new(i));
                (row.region.get(), row.cells.to_vec())
            })
            .collect()
    }

    #[test]
    fn a_row_past_the_last_is_absent() {
        // One unchanged region of two lines: rows 0 and 1.
        let doc = doc("a\nb\n", "a\nb\n", "a\nb\n");
        let map = RowMap::new(&doc, PaneSet::Sides(2)).expect("two sides");
        let cells = |row| map.get_row(&doc, RowIx::new(row)).map(|row| row.cells.to_vec());
        assert_eq!(cells(1), Some(vec![Some(1), Some(1)]));
        assert_eq!(cells(2), None);
    }

    #[test]
    fn a_filler_row_names_the_line_after_it() {
        // Rows: 0 a|a, 1 X|-, 2 Y|-, 3 b|b.
        let doc = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        let map = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        let line = |row, source| map.line_at_row(&doc, RowIx::new(row), source);
        let (left, right) = (Source::Side(0), Source::Side(1));
        assert_eq!([0, 1, 2, 3].map(|row| line(row, left)), [Ok(0), Ok(1), Ok(2), Ok(3)]);
        assert_eq!([0, 1, 2, 3].map(|row| line(row, right)), [Ok(0), Err(1), Err(1), Ok(1)]);
    }

    #[test]
    fn a_filler_row_after_the_last_line_names_the_line_count() {
        // Rows: 0 a|a, 1 X|-.
        let doc = doc("a\n", "a\nX\n", "a\n");
        let map = RowMap::new(&doc, PaneSet::LeftBaseRight).expect("panes fit");
        assert_eq!(map.line_at_row(&doc, RowIx::new(1), Source::Base(0)), Err(1));
        assert_eq!(map.line_at_row(&doc, RowIx::new(1), Source::Side(1)), Err(1));
        assert_eq!(map.line_at_row(&doc, RowIx::new(1), Source::Side(0)), Ok(1));
    }

    #[test]
    fn row_distances_count_rows_between() {
        assert_eq!(RowIx::new(5).plus(2), RowIx::new(7));
        assert_eq!(RowIx::new(7).since(RowIx::new(5)), 2);
        assert_eq!(RowIx::new(5).since(RowIx::new(5)), 0);
    }

    #[test]
    fn an_insertion_gets_filler_rows_on_the_other_side() {
        let doc = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        assert_eq!(
            rows(&doc, PaneSet::LeftRight),
            [
                (0, vec![Some(0), Some(0)]),
                (1, vec![Some(1), None]),
                (1, vec![Some(2), None]),
                (2, vec![Some(3), Some(1)]),
            ]
        );
    }

    #[test]
    fn the_base_pane_adds_its_own_cells() {
        let doc = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        assert_eq!(
            rows(&doc, PaneSet::LeftBaseRight),
            [
                (0, vec![Some(0), Some(0), Some(0)]),
                (1, vec![Some(1), None, None]),
                (1, vec![Some(2), None, None]),
                (2, vec![Some(3), Some(1), Some(1)]),
            ]
        );
    }

    #[test]
    fn a_conflict_takes_its_longest_side() {
        let doc = doc("a\nb\nc\n", "a\nL\nc\n", "a\nR1\nR2\nc\n");
        assert_eq!(
            rows(&doc, PaneSet::LeftBaseRight),
            [
                (0, vec![Some(0), Some(0), Some(0)]),
                (1, vec![Some(1), Some(1), Some(1)]),
                (1, vec![None, None, Some(2)]),
                (2, vec![Some(2), Some(2), Some(3)]),
            ]
        );
    }

    #[test]
    fn a_deletion_on_both_sides_has_rows_only_with_the_base_pane() {
        let doc = doc("a\nb\nc\n", "a\nc\n", "a\nc\n");
        let map = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        assert_eq!(
            rows(&doc, PaneSet::LeftRight),
            [(0, vec![Some(0), Some(0)]), (2, vec![Some(1), Some(1)])]
        );
        assert_eq!(
            (
                map.first_row(RegionIx::new(1)).get(),
                map.first_row(RegionIx::new(2)).get(),
                map.rows_in(RegionIx::new(1))
            ),
            (1, 1, 0)
        );
        assert_eq!(rows(&doc, PaneSet::LeftBaseRight)[1], (1, vec![None, Some(1), None]));
    }

    fn deleted(doc: &MergeDocument, region: RegionIx) -> Vec<Option<usize>> {
        PaneSet::LeftBaseRight
            .sources()
            .into_iter()
            .map(|s| deleted_lines(doc.region(region), s))
            .collect()
    }

    #[test]
    fn a_side_that_deletes_base_lines_in_a_conflict_marks_the_deletion() {
        // Left deletes b, c; right changes them. One conflict: base 1..3, left 1..1, right 1..3.
        let doc = doc("a\nb\nc\nz\n", "a\nz\n", "a\nB\nC\nz\n");
        assert_eq!(doc.regions()[1].kind, Kind::Conflict);
        assert_eq!(deleted(&doc, RegionIx::new(1)), [Some(2), None, None]);
    }

    #[test]
    fn a_one_side_deletion_marks_only_the_deleting_side() {
        let doc = doc("a\nb\nz\n", "a\nb\nz\n", "a\nz\n");
        assert_eq!(doc.regions()[1].kind, Kind::OneSide(1));
        assert_eq!(deleted(&doc, RegionIx::new(1)), [None, None, Some(1)]);
    }

    #[test]
    fn a_deletion_on_both_sides_marks_both() {
        let doc = doc("a\nb\nc\nz\n", "a\nz\n", "a\nz\n");
        assert_eq!(doc.regions()[1].kind, Kind::AllSame);
        assert_eq!(deleted(&doc, RegionIx::new(1)), [Some(2), None, Some(2)]);
    }

    #[test]
    fn an_insertion_is_no_deletion() {
        // Left inserts X: base and right have no lines in the region.
        let doc = doc("a\nz\n", "a\nX\nz\n", "a\nz\n");
        assert_eq!(deleted(&doc, RegionIx::new(1)), [None, None, None]);
    }

    #[test]
    fn a_shorter_change_is_no_deletion() {
        // Left turns b, c into one line: filler rows, but left still has lines.
        let doc = doc("a\nb\nc\nz\n", "a\nL\nz\n", "a\nb\nc\nz\n");
        assert_eq!(deleted(&doc, RegionIx::new(1)), [None, None, None]);
    }

    #[test]
    fn unchanged_regions_mark_nothing() {
        let doc = doc("a\n", "a\n", "a\n");
        assert_eq!(deleted(&doc, RegionIx::new(0)), [None, None, None]);
    }

    #[test]
    fn three_sides_show_one_cell_each() {
        let inputs = Inputs::new(
            vec![text("a\nb\n"), text("a\nb\n"), text("a\nB\nB2\n")],
            vec![text("a\nb\n"), text("a\nb\n")],
        )
        .expect("three sides, two bases");
        let doc =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
        assert_eq!(
            rows(&doc, PaneSet::Sides(3)),
            [
                (0, vec![Some(0), Some(0), Some(0)]),
                (1, vec![Some(1), Some(1), Some(1)]),
                (1, vec![None, None, Some(2)]),
            ]
        );
    }

    #[test]
    fn a_pane_without_an_input_is_an_error() {
        let doc = doc("a\n", "a\n", "a\n");
        assert_eq!(
            RowMap::new(&doc, PaneSet::Sides(3)).err(),
            Some(PaneError { input: Source::Side(2), sides: 2, bases: 1 })
        );
    }

    #[test]
    fn empty_inputs_have_no_rows() {
        let doc = doc("", "", "");
        let map = RowMap::new(&doc, PaneSet::LeftBaseRight).expect("panes fit");
        assert_eq!((map.row_count(), map.first_row(RegionIx::new(0)).get()), (0, 0));
    }

    #[test]
    fn rows_follow_a_merging_edit_and_its_undo() {
        // Regions: a | L (left only) | m | R (right only) | z. Result "a\nL\nm\nR\nz\n".
        let mut doc = doc("a\nm\nz\n", "a\nL\nm\nz\n", "a\nm\nR\nz\n");
        let mut map = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        assert_eq!((map.row_count(), map.first_row(RegionIx::new(4)).get()), (5, 4));

        // Deleting "\nm\nR" joins L, m and R into one region: left 1..3, right 1..3.
        let delete = TextPatch { range: 3..7, text: String::new() };
        let applied = doc.apply(Command::Edit(EditBatch(vec![delete]))).expect("edit");
        assert_eq!(doc.regions().len(), 3);
        map.update(&doc, &applied);
        assert_eq!(map.row_count(), 4);
        assert_eq!(map.row(&doc, RowIx::new(2)).cells.to_vec(), [Some(2), Some(2)]);
        assert_eq!(
            (map.row(&doc, RowIx::new(3)).region.get(), map.first_row(RegionIx::new(2)).get()),
            (2, 3)
        );

        let applied = doc.apply(Command::Undo).expect("undo");
        map.update(&doc, &applied);
        assert_eq!((map.row_count(), map.first_row(RegionIx::new(4)).get()), (5, 4));
        assert_eq!(map.row(&doc, RowIx::new(3)).cells.to_vec(), [None, Some(2)]);
    }

    #[test]
    fn a_pick_leaves_the_rows_alone() {
        let mut doc = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        let mut map = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        let applied = doc
            .apply(Command::Toggle { region: RegionIx::new(1), source: Source::Side(1) })
            .expect("toggle");
        map.update(&doc, &applied);
        assert_eq!((map.row_count(), map.rows_in(RegionIx::new(1))), (4, 2));
    }
}
