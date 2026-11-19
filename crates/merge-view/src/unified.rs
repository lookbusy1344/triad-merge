//! The diff window's unified view (spec §5, two-way mode): one read-only text holding the
//! unchanged lines, and for each change its left lines, deleted, then its right lines,
//! inserted, with both sides' line numbers.
//!
//! Unchanged lines show the right side's text, as a unified diff's context does. A side's
//! last line without a terminator gets one when more lines follow it, so no two lines join.

use std::ops::Range;

use merge_core::diff2::{Kind2, Region2, RopeSlice, Side};
use merge_core::index::RegionIx;

/// The fewest digits a line number column holds, as the editors' own line numbers.
const MIN_NUMBER_DIGITS: usize = 3;

/// What a unified line shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Unchanged,
    /// A left line a change replaces or removes.
    Deleted,
    /// A right line a change puts in.
    Inserted,
}

/// A run of unified lines of one kind from one region, up to the next run's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    line: usize,
    byte: usize,
    kind: LineKind,
    /// The left and right line, from 0, that the run's first line stands for. Only the
    /// sides the kind shows are read.
    left: usize,
    right: usize,
    region: RegionIx,
}

/// The unified view of one version of both texts. It keeps the runs, O(regions); the text
/// goes to the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unified {
    runs: Vec<Run>,
    lines: usize,
    bytes: usize,
    digits: usize,
}

impl Unified {
    /// The unified view of `left` and `right` over `regions`, which cover both texts in
    /// order, and its text.
    #[must_use]
    pub fn build(left: RopeSlice<'_>, right: RopeSlice<'_>, regions: &[Region2]) -> (Self, String) {
        let mut text = String::with_capacity(left.len_bytes() + right.len_bytes());
        let mut runs = Vec::with_capacity(2 * regions.len());
        let mut lines = 0;
        for (index, region) in regions.iter().enumerate() {
            let region_ix = RegionIx::new(index);
            let parts: &[(LineKind, Side)] = match region.kind {
                Kind2::Unchanged => &[(LineKind::Unchanged, Side::Right)],
                Kind2::Changed | Kind2::Ignored => {
                    &[(LineKind::Deleted, Side::Left), (LineKind::Inserted, Side::Right)]
                }
            };
            for &(kind, side) in parts {
                let span = region.lines(side);
                if span.is_empty() {
                    continue;
                }
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                runs.push(Run {
                    line: lines,
                    byte: text.len(),
                    kind,
                    left: region.left.start,
                    right: region.right.start,
                    region: region_ix,
                });
                let source = match side {
                    Side::Left => left,
                    Side::Right => right,
                };
                let bytes = source.line_to_byte(span.start)..source.line_to_byte(span.end);
                source.byte_slice(bytes).chunks().for_each(|chunk| text.push_str(chunk));
                lines += span.len();
            }
        }
        let longest = left.len_lines().max(right.len_lines());
        let digits = (longest.max(1).ilog10() as usize + 1).max(MIN_NUMBER_DIGITS);
        (Self { runs, lines, bytes: text.len(), digits }, text)
    }

    /// The number of unified lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines
    }

    /// The run holding unified line `line`, if any.
    fn run(&self, line: usize) -> Option<&Run> {
        if line >= self.lines {
            return None;
        }
        let at = self.runs.partition_point(|run| run.line <= line);
        at.checked_sub(1).map(|i| &self.runs[i])
    }

    #[must_use]
    pub fn kind(&self, line: usize) -> Option<LineKind> {
        self.run(line).map(|run| run.kind)
    }

    /// The left and right line numbers, from 1, of unified line `line`: a deleted line has
    /// no right number, an inserted one no left number.
    #[must_use]
    pub fn numbers(&self, line: usize) -> [Option<usize>; 2] {
        let Some(run) = self.run(line) else {
            return [None, None];
        };
        let offset = line - run.line;
        let left = Some(run.left + offset + 1);
        let right = Some(run.right + offset + 1);
        match run.kind {
            LineKind::Unchanged => [left, right],
            LineKind::Deleted => [left, None],
            LineKind::Inserted => [None, right],
        }
    }

    /// The width of [`Unified::label`], in characters: two columns and a space between.
    #[must_use]
    pub fn label_width(&self) -> usize {
        2 * self.digits + 1
    }

    /// Unified line `line`'s numbers as the gutter draws them, each right-aligned in its
    /// column, an absent one blank.
    #[must_use]
    pub fn label(&self, line: usize) -> String {
        let column = |number: Option<usize>| number.map_or_else(String::new, |n| n.to_string());
        let [left, right] = self.numbers(line).map(column);
        let digits = self.digits;
        format!("{left:>digits$} {right:>digits$}")
    }

    /// The byte ranges of the unified text's lines of `kind`, a range per run, in order.
    pub fn bytes_of(&self, kind: LineKind) -> impl Iterator<Item = Range<usize>> + '_ {
        let ends = self.runs.iter().skip(1).map(|run| run.byte).chain([self.bytes]);
        self.runs
            .iter()
            .zip(ends)
            .filter(move |(run, _)| run.kind == kind)
            .map(|(run, end)| run.byte..end)
    }

    /// The region unified line `line` belongs to.
    #[must_use]
    pub fn region_at(&self, line: usize) -> Option<RegionIx> {
        self.run(line).map(|run| run.region)
    }

    /// The first unified line of `region`; none for a region without lines.
    #[must_use]
    pub fn first_line(&self, region: RegionIx) -> Option<usize> {
        self.runs.iter().find(|run| run.region == region).map(|run| run.line)
    }
}

#[cfg(test)]
mod tests {
    use merge_core::diff::DiffOptions;
    use merge_core::diff2::{Rope, regions};
    use merge_core::testing::text;

    use super::*;
    use LineKind::{Deleted, Inserted, Unchanged};

    fn unified(left: &str, right: &str) -> (Unified, String) {
        let found = regions(&text(left), &text(right), &DiffOptions::default());
        let (left, right) = (Rope::from_str(left), Rope::from_str(right));
        Unified::build(left.slice(..), right.slice(..), &found)
    }

    fn kinds(view: &Unified) -> Vec<Option<LineKind>> {
        (0..view.line_count()).map(|line| view.kind(line)).collect()
    }

    fn numbers(view: &Unified) -> Vec<[Option<usize>; 2]> {
        (0..view.line_count()).map(|line| view.numbers(line)).collect()
    }

    #[test]
    fn a_change_shows_its_left_lines_then_its_right_lines() {
        let (view, text) = unified("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(text, "a\nb\nB\nc\n");
        assert_eq!(kinds(&view), [Some(Unchanged), Some(Deleted), Some(Inserted), Some(Unchanged)]);
        assert_eq!(
            numbers(&view),
            [[Some(1), Some(1)], [Some(2), None], [None, Some(2)], [Some(3), Some(3)]]
        );
        assert_eq!(view.kind(4), None);
        assert_eq!(view.numbers(4), [None, None]);
    }

    #[test]
    fn labels_right_align_each_column_in_at_least_three_digits() {
        let (view, _) = unified("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(view.label_width(), 7);
        let labels: Vec<String> = (0..4).map(|line| view.label(line)).collect();
        assert_eq!(labels, ["  1   1", "  2    ", "      2", "  3   3"]);
        // 1,000 lines take four digits a column.
        let long: String = (0..1000).map(|n| n.to_string() + "\n").collect();
        let (view, _) = unified(&long, &long);
        assert_eq!(view.label_width(), 9);
        assert_eq!(view.label(999), "1000 1000");
    }

    #[test]
    fn numbers_count_each_side_through_insertions_and_deletions() {
        let (view, text) = unified("a\nx\ny\nb\n", "a\nb\nz\n");
        assert_eq!(text, "a\nx\ny\nb\nz\n");
        assert_eq!(
            numbers(&view),
            [
                [Some(1), Some(1)],
                [Some(2), None],
                [Some(3), None],
                [Some(4), Some(2)],
                [None, Some(3)],
            ]
        );
    }

    #[test]
    fn unchanged_lines_show_the_right_sides_text() {
        let all =
            DiffOptions { whitespace: merge_core::diff::Whitespace::All, ..DiffOptions::default() };
        let (l, r) = ("a \nb\n", "a\nB\n");
        let found = regions(&text(l), &text(r), &all);
        let (lr, rr) = (Rope::from_str(l), Rope::from_str(r));
        let (_, unified_text) = Unified::build(lr.slice(..), rr.slice(..), &found);
        assert_eq!(unified_text, "a\nb\nB\n");
    }

    #[test]
    fn a_last_line_without_a_terminator_gets_one_when_lines_follow() {
        let (view, text) = unified("a\nb", "a\nc");
        assert_eq!(text, "a\nb\nc");
        assert_eq!(view.bytes_of(Deleted).collect::<Vec<_>>(), vec![2..4]);
        assert_eq!(view.bytes_of(Inserted).collect::<Vec<_>>(), vec![4..5]);
        let (_, text) = unified("a\nb\n", "a\nb");
        assert_eq!(text, "a\nb\nb");
    }

    #[test]
    fn bytes_of_a_kind_give_a_range_per_run() {
        let (view, text) = unified("1\n2\n3\n4\n5\n", "1\nB\n3\n4\nE\n");
        assert_eq!(text, "1\n2\nB\n3\n4\n5\nE\n");
        assert_eq!(view.bytes_of(Deleted).collect::<Vec<_>>(), [2..4, 10..12]);
        assert_eq!(view.bytes_of(Inserted).collect::<Vec<_>>(), [4..6, 12..14]);
        assert_eq!(view.bytes_of(Unchanged).collect::<Vec<_>>(), [0..2, 6..10]);
    }

    #[test]
    fn regions_map_to_their_first_unified_line_and_back() {
        // Regions: 0 unchanged (1), 1 changed (2 → B), 2 unchanged (3 4), 3 changed (5 → E).
        let (view, _) = unified("1\n2\n3\n4\n5\n", "1\nB\n3\n4\nE\n");
        let at: Vec<Option<usize>> =
            (0..view.line_count()).map(|line| view.region_at(line).map(RegionIx::get)).collect();
        assert_eq!(at, [Some(0), Some(1), Some(1), Some(2), Some(2), Some(3), Some(3)]);
        assert_eq!(view.first_line(RegionIx::new(3)), Some(5));
        assert_eq!(view.first_line(RegionIx::new(4)), None);
        assert_eq!(view.region_at(7), None);
    }

    #[test]
    fn empty_texts_give_an_empty_view() {
        let (view, text) = unified("", "");
        assert_eq!((view.line_count(), text.as_str()), (0, ""));
        assert_eq!(view.region_at(0), None);
        assert_eq!(view.first_line(RegionIx::new(0)), None);
    }

    #[test]
    fn an_insertion_into_an_empty_file_is_all_inserted() {
        let (view, text) = unified("", "a\nb\n");
        assert_eq!(text, "a\nb\n");
        assert_eq!(numbers(&view), [[None, Some(1)], [None, Some(2)]]);
    }
}
