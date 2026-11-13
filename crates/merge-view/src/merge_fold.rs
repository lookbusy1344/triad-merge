//! Collapsed unchanged regions in the merge window (spec §5): the aligned rows and result
//! lines each fold hides, and which regions the user opened.
//!
//! A region folds while every input leaves it unchanged and the result holds its automatic
//! text, so the top panes and the result have as many lines in it as each other. It keeps
//! `context` lines at each end that faces another region, as in the two-way diff
//! ([`crate::fold`]).

use std::ops::Range;

use merge_core::document::{MergeDocument, Resolution};
use merge_core::index::RegionIx;
use merge_core::merge3::{Kind, Region};

use crate::fold::MIN_FOLDED_LINES;
use crate::rows::RowMap;
use crate::scroll::result_lines;

/// The aligned rows and the result lines one unchanged region hides; as many of each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeFold {
    pub region: RegionIx,
    pub rows: Range<usize>,
    pub result: Range<usize>,
}

impl MergeFold {
    /// The lines hidden in each pane.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Always false: a fold hides at least [`MIN_FOLDED_LINES`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The unchanged regions the user opened, by their lines in the first side. The inputs never
/// change, so these stay put while edits and picks renumber the regions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeOpened {
    lines: Vec<Range<usize>>,
}

impl MergeOpened {
    /// Open the region `fold` hides lines of.
    pub fn open(&mut self, doc: &MergeDocument, fold: &MergeFold) {
        if let Some(lines) = doc.regions().get(fold.region.get()).and_then(first_side) {
            self.lines.push(lines);
        }
    }

    /// Close every region.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    fn holds(&self, region: &Region) -> bool {
        first_side(region).is_some_and(|region| {
            self.lines.iter().any(|open| open.start < region.end && region.start < open.end)
        })
    }
}

fn first_side(region: &Region) -> Option<Range<usize>> {
    region.sides.first().cloned()
}

/// The folds of `doc`'s unchanged regions whose result holds the automatic text, with
/// `context` lines kept at each end that faces another region, leaving out the regions in
/// `opened`. `rows` gives each region's aligned rows. Sorted by row and by result line.
#[must_use]
pub fn folds(
    doc: &MergeDocument,
    rows: &RowMap,
    context: usize,
    opened: &MergeOpened,
) -> Vec<MergeFold> {
    let last = doc.regions().len().saturating_sub(1);
    doc.region_ixs()
        .zip(doc.regions())
        .filter(|&(ix, region)| {
            region.kind == Kind::Unchanged
                && *doc.resolution(ix) == Resolution::Auto
                && !opened.holds(region)
        })
        .filter_map(|(ix, _)| {
            let (first, count) = (rows.first_row(ix).get(), rows.rows_in(ix));
            let result = result_lines(doc, ix);
            if result.len() != count {
                return None;
            }
            let top = if ix.get() == 0 { 0 } else { context };
            let bottom = if ix.get() == last { 0 } else { context };
            let hidden = count.checked_sub(top + bottom)?;
            (hidden >= MIN_FOLDED_LINES).then(|| MergeFold {
                region: ix,
                rows: first + top..first + top + hidden,
                result: result.start + top..result.start + top + hidden,
            })
        })
        .collect()
}

/// The fold that hides aligned row `row`; its first row is the fold row.
#[must_use]
pub fn fold_at_row(folds: &[MergeFold], row: usize) -> Option<&MergeFold> {
    let after = folds.partition_point(|fold| fold.rows.end <= row);
    folds.get(after).filter(|fold| fold.rows.contains(&row))
}

/// The fold that hides result line `line`.
#[must_use]
pub fn fold_over_line(folds: &[MergeFold], line: usize) -> Option<&MergeFold> {
    let after = folds.partition_point(|fold| fold.result.end <= line);
    folds.get(after).filter(|fold| fold.result.contains(&line))
}

/// The fold whose row sits above result line `line`: the one whose hidden lines end there.
#[must_use]
pub fn fold_above_line(folds: &[MergeFold], line: usize) -> Option<&MergeFold> {
    folds.iter().find(|fold| fold.result.end == line)
}

#[cfg(test)]
mod tests {
    use merge_core::document::{Command, EditBatch, TextPatch};
    use merge_core::testing::doc;

    use crate::rows::PaneSet;

    use super::*;

    const CONTEXT: usize = 3;

    fn lines(range: Range<usize>) -> String {
        range.flat_map(|i| ["l".to_owned(), i.to_string(), "\n".to_owned()]).collect()
    }

    /// Twenty base lines; the left puts four lines in after line 4, the right keeps them.
    /// Regions: 0 unchanged (lines 0-4), 1 the left's insertion, 2 unchanged (base 5-19).
    /// Rows: region 0 at 0-4, region 1 at 5-8, region 2 at 9-23.
    fn inserted() -> MergeDocument {
        let base = lines(0..20);
        let left = [lines(0..5), "x1\nx2\nx3\nx4\n".to_owned(), lines(5..20)].concat();
        doc(&base, &left, &base)
    }

    fn folds_of(doc: &MergeDocument, context: usize, opened: &MergeOpened) -> Vec<MergeFold> {
        let rows = RowMap::new(doc, PaneSet::LeftBaseRight).expect("three panes");
        folds(doc, &rows, context, opened)
    }

    fn fold(region: usize, rows: Range<usize>, result: Range<usize>) -> MergeFold {
        MergeFold { region: RegionIx::new(region), rows, result }
    }

    #[test]
    fn a_region_keeps_context_only_towards_another_region() {
        let doc = inserted();
        assert_eq!(doc.regions().len(), 3);
        // Region 0 hides 5 - 3 = 2 lines at its top; region 2 hides 15 - 3 = 12 at its bottom.
        assert_eq!(
            folds_of(&doc, CONTEXT, &MergeOpened::default()),
            [fold(0, 0..2, 0..2), fold(2, 12..24, 12..24)]
        );
    }

    #[test]
    fn rows_and_result_lines_part_where_the_result_drops_a_change() {
        let mut doc = inserted();
        doc.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        // Region 1 keeps its four rows but no result lines: region 2's lines start at 5.
        assert_eq!(
            folds_of(&doc, CONTEXT, &MergeOpened::default()),
            [fold(0, 0..2, 0..2), fold(2, 12..24, 8..20)]
        );
    }

    #[test]
    fn a_region_folds_from_two_hidden_lines() {
        // Region 0 has five lines: two hidden at context 3, one at context 4, none at 5.
        let doc = inserted();
        let first = |context| folds_of(&doc, context, &MergeOpened::default()).first().cloned();
        assert_eq!(first(3), Some(fold(0, 0..2, 0..2)));
        assert_eq!(first(4).map(|fold| fold.region.get()), Some(2), "region 0 hides one line");
        assert_eq!(first(0), Some(fold(0, 0..5, 0..5)));
    }

    #[test]
    fn a_merge_without_changes_folds_whole() {
        let text = lines(0..10);
        let doc = doc(&text, &text, &text);
        assert_eq!(folds_of(&doc, CONTEXT, &MergeOpened::default()), [fold(0, 0..10, 0..10)]);
    }

    #[test]
    fn a_region_the_result_edits_or_picks_shows_whole() {
        let mut edited = inserted();
        // "l1" becomes "L1" in the result: region 0 holds typed text.
        let patch = TextPatch { range: 3..4, text: "L".to_owned() };
        edited.apply(Command::Edit(EditBatch(vec![patch]))).expect("edit");
        let regions: Vec<_> = folds_of(&edited, CONTEXT, &MergeOpened::default())
            .iter()
            .map(|fold| fold.region.get())
            .collect();
        assert_eq!(regions, [2]);

        let mut picked = inserted();
        picked.apply(Command::Clear { region: RegionIx::new(2) }).expect("clear");
        let regions: Vec<_> = folds_of(&picked, CONTEXT, &MergeOpened::default())
            .iter()
            .map(|fold| fold.region.get())
            .collect();
        assert_eq!(regions, [0]);
    }

    #[test]
    fn an_opened_region_stays_open_when_the_regions_renumber() {
        let mut doc = inserted();
        let mut opened = MergeOpened::default();
        let folds = folds_of(&doc, CONTEXT, &opened);
        opened.open(&doc, &folds[1]);
        assert_eq!(folds_of(&doc, CONTEXT, &opened), [fold(0, 0..2, 0..2)]);
        // Deleting "l4\nx1\n" spans regions 0 and 1 and joins them into one edited region of
        // nine rows and seven result lines; region 2 becomes region 1 and stays open.
        let patch = TextPatch { range: 12..18, text: String::new() };
        doc.apply(Command::Edit(EditBatch(vec![patch]))).expect("edit");
        assert_eq!(doc.regions().len(), 2);
        assert_eq!(folds_of(&doc, CONTEXT, &opened), []);
        opened.clear();
        assert_eq!(folds_of(&doc, CONTEXT, &opened), [fold(1, 12..24, 10..22)]);
    }

    #[test]
    fn a_row_or_line_finds_its_fold() {
        let folds = [fold(0, 0..2, 0..2), fold(2, 12..24, 8..20)];
        assert_eq!(fold_at_row(&folds, 0), Some(&folds[0]));
        assert_eq!(fold_at_row(&folds, 1), Some(&folds[0]));
        assert_eq!(fold_at_row(&folds, 2), None);
        assert_eq!(fold_at_row(&folds, 12), Some(&folds[1]));
        assert_eq!(fold_at_row(&folds, 24), None);
        assert_eq!(fold_over_line(&folds, 7), None);
        assert_eq!(fold_over_line(&folds, 8), Some(&folds[1]));
        assert_eq!(fold_over_line(&folds, 19), Some(&folds[1]));
        assert_eq!(fold_over_line(&folds, 20), None);
        assert_eq!(fold_above_line(&folds, 2), Some(&folds[0]));
        assert_eq!(fold_above_line(&folds, 20), Some(&folds[1]));
        assert_eq!(fold_above_line(&folds, 8), None);
    }
}
