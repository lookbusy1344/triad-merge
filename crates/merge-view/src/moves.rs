//! The ends of each side's moved blocks in the merge window's top panes (`PLAN.md` §G16).

use std::ops::Range;

use merge_core::document::MergeDocument;
use merge_core::index::RegionIx;
use merge_core::merge3::{Region, SideMove, Source};

use crate::rows::{RowIx, RowMap};

/// Which end of a move a [`MoveEnd`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndKind {
    /// Where the side deleted the block: the rows facing its base lines.
    Origin,
    /// Where the side inserted the block: the rows of its lines.
    Destination,
}

/// One end of a side's move, in that side's pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveEnd {
    /// The pane of the side that moved the block.
    pub pane: usize,
    pub kind: EndKind,
    /// The region holding this end.
    pub region: RegionIx,
    /// This end's rows in the pane, one per block line, within its region's rows.
    pub rows: Range<RowIx>,
    /// The input and line the other end starts at: the side's line at an origin, base's line
    /// at a destination.
    pub other: Source,
    pub other_line: usize,
    /// The region holding the other end.
    pub other_region: RegionIx,
}

impl MoveEnd {
    /// Whether the other end lies above this one, in an earlier region.
    #[must_use]
    pub fn other_is_above(&self) -> bool {
        self.other_region < self.region
    }
}

/// The ends of `doc`'s moves that have rows in a pane `rows` shows, ordered by pane, then row.
/// An origin takes the rows of its region from the block's first base line, as many as the
/// block has lines and the region has rows from there; a region with no rows from there, as
/// a deletion by both sides has while the base pane hides, gives no origin.
#[must_use]
pub fn move_ends(doc: &MergeDocument, rows: &RowMap) -> Vec<MoveEnd> {
    let regions = doc.regions();
    let mut ends: Vec<MoveEnd> = doc
        .moves()
        .iter()
        .filter_map(|m| {
            let side = Source::Side(m.side);
            let pane = rows.pane_of(side)?;
            let origin_region = region_of(regions, Source::Base(0), m.base.start);
            let region = region_of(regions, side, m.lines.start);
            let first = rows.row_of_input_line(regions, side, m.lines.start)?;
            let origin_rows = origin_rows(rows, &regions[origin_region.get()], origin_region, m);
            let destination = MoveEnd {
                pane,
                kind: EndKind::Destination,
                region,
                rows: first..first.plus(m.lines.len()),
                other: Source::Base(0),
                other_line: m.base.start,
                other_region: origin_region,
            };
            let origin = origin_rows.map(|rows| MoveEnd {
                pane,
                kind: EndKind::Origin,
                region: origin_region,
                rows,
                other: side,
                other_line: m.lines.start,
                other_region: region,
            });
            Some(origin.into_iter().chain([destination]))
        })
        .flatten()
        .collect();
    ends.sort_by_key(|end| (end.pane, end.rows.start));
    ends
}

/// The first end held by `region`, for Go to Other End of Move.
#[must_use]
pub fn end_in(ends: &[MoveEnd], region: RegionIx) -> Option<&MoveEnd> {
    ends.iter().find(|end| end.region == region)
}

/// The rows of `m`'s origin in `region`, numbered `index`: from the row facing the block's
/// first base line, one per block line while the region has rows. None when it has none
/// from there.
fn origin_rows(
    rows: &RowMap,
    region: &Region,
    index: RegionIx,
    m: &SideMove,
) -> Option<Range<RowIx>> {
    let offset = m.base.start - region.bases[0].start;
    let count = m.base.len().min(rows.rows_in(index).saturating_sub(offset));
    let start = rows.first_row(index).plus(offset);
    (count > 0).then(|| start..start.plus(count))
}

/// The region holding `source`'s line `line`: the first whose lines of `source` end after it.
fn region_of(regions: &[Region], source: Source, line: usize) -> RegionIx {
    let lines = |r: &Region| match source {
        Source::Side(i) => r.sides[i].end,
        Source::Base(i) => r.bases[i].end,
    };
    let found = regions.partition_point(|r| lines(r) <= line);
    RegionIx::new(found.min(regions.len().saturating_sub(1)))
}

#[cfg(test)]
mod tests {
    use merge_core::testing::doc;

    use super::*;
    use crate::rows::PaneSet;

    const BLOCK_FIRST: &str = "m1\nm2\na\nb\nc\n";
    const BLOCK_LAST: &str = "a\nb\nc\nm1\nm2\n";
    /// Both sides move `m1 m2` from the top: left to the end, right before `d`.
    const TWICE: [&str; 3] =
        ["m1\nm2\na\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\nm1\nm2\n", "a\nb\nc\nm1\nm2\nd\ne\n"];

    fn ends(base: &str, left: &str, right: &str, panes: PaneSet) -> Vec<MoveEnd> {
        let d = doc(base, left, right);
        move_ends(&d, &RowMap::new(&d, panes).expect("panes fit"))
    }

    fn rows(r: Range<usize>) -> Range<RowIx> {
        RowIx::new(r.start)..RowIx::new(r.end)
    }

    fn end(
        pane: usize,
        kind: EndKind,
        region: usize,
        at: Range<usize>,
        other: Source,
        other_line: usize,
        other_region: usize,
    ) -> MoveEnd {
        MoveEnd {
            pane,
            kind,
            region: RegionIx::new(region),
            rows: rows(at),
            other,
            other_line,
            other_region: RegionIx::new(other_region),
        }
    }

    #[test]
    fn a_left_move_has_both_ends_in_the_left_pane() {
        // Regions: left deletes `m1 m2` (rows 0–1, facing right's lines), `a b c` (rows
        // 2–4), left inserts `m1 m2` (rows 5–6).
        let expected = [
            end(0, EndKind::Origin, 0, 0..2, Source::Side(0), 3, 2),
            end(0, EndKind::Destination, 2, 5..7, Source::Base(0), 0, 0),
        ];
        assert_eq!(ends(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST, PaneSet::LeftRight), expected);
        assert_eq!(ends(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST, PaneSet::LeftBaseRight), expected);
    }

    #[test]
    fn a_right_move_has_its_ends_in_the_right_pane() {
        // Right moves `m1 m2` up: right inserts it (rows 0–1), `a b c` (rows 2–4), right
        // deletes it (rows 5–6). Right is pane 1 without base and pane 2 with it.
        let expected = |pane| {
            [
                end(pane, EndKind::Destination, 0, 0..2, Source::Base(0), 3, 2),
                end(pane, EndKind::Origin, 2, 5..7, Source::Side(1), 0, 0),
            ]
        };
        assert_eq!(ends(BLOCK_LAST, BLOCK_LAST, BLOCK_FIRST, PaneSet::LeftRight), expected(1));
        assert_eq!(ends(BLOCK_LAST, BLOCK_LAST, BLOCK_FIRST, PaneSet::LeftBaseRight), expected(2));
    }

    #[test]
    fn a_deletion_by_both_sides_has_no_origin_while_base_hides() {
        // Region 0 deletes `m1 m2` on both sides: no rows without base. Then `a b c` (rows
        // 0–2), right's insert (region 2, rows 3–4), `d e` (rows 5–6), left's insert (region
        // 4, rows 7–8).
        let [base, left, right] = TWICE;
        assert_eq!(
            ends(base, left, right, PaneSet::LeftRight),
            [
                end(0, EndKind::Destination, 4, 7..9, Source::Base(0), 0, 0),
                end(1, EndKind::Destination, 2, 3..5, Source::Base(0), 0, 0),
            ]
        );
    }

    #[test]
    fn a_deletion_by_both_sides_has_an_origin_in_each_side_with_base_shown() {
        // With base, region 0 has two rows; every later row moves down two.
        let [base, left, right] = TWICE;
        assert_eq!(
            ends(base, left, right, PaneSet::LeftBaseRight),
            [
                end(0, EndKind::Origin, 0, 0..2, Source::Side(0), 5, 4),
                end(0, EndKind::Destination, 4, 9..11, Source::Base(0), 0, 0),
                end(2, EndKind::Origin, 0, 0..2, Source::Side(1), 3, 2),
                end(2, EndKind::Destination, 2, 5..7, Source::Base(0), 0, 0),
            ]
        );
    }

    #[test]
    fn an_origin_inside_a_larger_region_starts_at_its_first_base_line() {
        // One conflict over base `m1 m2 z`; left keeps `z`, right edits it. Left's block is
        // base lines 0–1, the conflict's first two rows; left inserts it at region 2, rows 6–7.
        assert_eq!(
            ends(
                "m1\nm2\nz\na\nb\nc\n",
                "z\na\nb\nc\nm1\nm2\n",
                "m1\nm2\nZ\na\nb\nc\n",
                PaneSet::LeftRight
            ),
            [
                end(0, EndKind::Origin, 0, 0..2, Source::Side(0), 4, 2),
                end(0, EndKind::Destination, 2, 6..8, Source::Base(0), 0, 0),
            ]
        );
    }

    #[test]
    fn an_origin_after_the_regions_first_base_line_starts_further_down() {
        // One conflict over base `z m1 m2` (rows 0–2): left keeps `z`, right edits it. Left's
        // block is base lines 1–2, rows 1–2; left inserts it at region 2, rows 6–7.
        assert_eq!(
            ends(
                "z\nm1\nm2\na\nb\nc\n",
                "z\na\nb\nc\nm1\nm2\n",
                "Z\nm1\nm2\na\nb\nc\n",
                PaneSet::LeftRight
            ),
            [
                end(0, EndKind::Origin, 0, 1..3, Source::Side(0), 4, 2),
                end(0, EndKind::Destination, 2, 6..8, Source::Base(0), 1, 0),
            ]
        );
    }

    #[test]
    fn an_origin_in_a_region_with_fewer_rows_takes_the_rows_there_are() {
        // Both sides replace `m1 m2` with one line: the conflict has one row without base.
        // Left inserts the block at region 2, rows 4–5.
        assert_eq!(
            ends(BLOCK_FIRST, "X\na\nb\nc\nm1\nm2\n", "Y\na\nb\nc\n", PaneSet::LeftRight),
            [
                end(0, EndKind::Origin, 0, 0..1, Source::Side(0), 4, 2),
                end(0, EndKind::Destination, 2, 4..6, Source::Base(0), 0, 0),
            ]
        );
    }

    #[test]
    fn an_origin_above_its_destination_points_down_and_the_destination_up() {
        let found = ends(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST, PaneSet::LeftRight);
        let above: Vec<bool> = found.iter().map(MoveEnd::other_is_above).collect();
        assert_eq!(above, [false, true]);
        let found = ends(BLOCK_LAST, BLOCK_LAST, BLOCK_FIRST, PaneSet::LeftRight);
        let above: Vec<bool> = found.iter().map(MoveEnd::other_is_above).collect();
        assert_eq!(above, [false, true], "right's destination first, then its origin");
    }

    #[test]
    fn no_moves_no_ends() {
        assert_eq!(ends(BLOCK_FIRST, BLOCK_FIRST, BLOCK_FIRST, PaneSet::LeftRight), []);
    }

    #[test]
    fn the_end_in_a_region_is_its_first() {
        let found = ends(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST, PaneSet::LeftRight);
        assert_eq!(end_in(&found, RegionIx::new(2)), Some(&found[1]));
        assert_eq!(end_in(&found, RegionIx::new(0)), Some(&found[0]));
        assert_eq!(end_in(&found, RegionIx::new(1)), None);
    }
}
