//! Lines picked one by one inside a region (spec §4.4, `PLAN.md` §G8).
//!
//! A selection is a list of [`PickedLines`] in normal form: sorted by source, sides by index
//! then bases, and by line within a source; each range non-empty; no two ranges of one source
//! overlapping or touching. The result takes the lines in that order, whatever order they were
//! picked in.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::merge3::Source;

/// A run of one input's lines, counted from the region's first line of that input.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PickedLines {
    pub source: Source,
    pub lines: Range<usize>,
}

impl PickedLines {
    #[must_use]
    pub fn new(source: Source, lines: Range<usize>) -> Self {
        Self { source, lines }
    }
}

/// `selection` with `lines` of `source` toggled: removed when every one of them is picked,
/// otherwise added. `selection` is in normal form, and so is the result. `lines` is not empty.
pub(crate) fn toggle(
    selection: &[PickedLines],
    source: Source,
    lines: &Range<usize>,
) -> Vec<PickedLines> {
    let of_source = || selection.iter().filter(move |p| p.source == source);
    let covered = of_source().any(|p| p.lines.start <= lines.start && lines.end <= p.lines.end);
    let ranges: Vec<Range<usize>> = if covered {
        of_source()
            .flat_map(|p| {
                let before = p.lines.start..p.lines.end.min(lines.start);
                let after = p.lines.start.max(lines.end)..p.lines.end;
                [before, after]
            })
            .filter(|r| !r.is_empty())
            .collect()
    } else {
        let mut joined = lines.clone();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for p in of_source() {
            if p.lines.end < joined.start || joined.end < p.lines.start {
                ranges.push(p.lines.clone());
            } else {
                joined = joined.start.min(p.lines.start)..joined.end.max(p.lines.end);
            }
        }
        let at = ranges.partition_point(|r| r.start < joined.start);
        ranges.insert(at, joined);
        ranges
    };
    let others = selection.iter().filter(|p| p.source != source).cloned();
    let mut toggled: Vec<PickedLines> =
        others.chain(ranges.into_iter().map(|lines| PickedLines { source, lines })).collect();
    toggled.sort_by_key(|p| (p.source, p.lines.start));
    toggled
}

/// The sources `selection` takes whole, in order, when it takes each of its sources whole:
/// every line of the region, by `count`. `None` when it takes some source in part.
pub(crate) fn whole_sources(
    selection: &[PickedLines],
    count: impl Fn(Source) -> usize,
) -> Option<Vec<Source>> {
    selection.iter().map(|p| (p.lines == (0..count(p.source))).then_some(p.source)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Source = Source::Side(0);
    const R: Source = Source::Side(1);
    const B: Source = Source::Base(0);

    fn p(source: Source, lines: Range<usize>) -> PickedLines {
        PickedLines::new(source, lines)
    }

    #[test]
    fn a_toggle_on_nothing_picks_the_lines() {
        assert_eq!(toggle(&[], R, &(1..3)), [p(R, 1..3)]);
    }

    #[test]
    fn toggling_a_picked_line_inside_a_run_splits_it() {
        assert_eq!(toggle(&[p(L, 0..5)], L, &(2..3)), [p(L, 0..2), p(L, 3..5)]);
    }

    #[test]
    fn toggling_the_ends_of_a_run_shortens_it() {
        assert_eq!(toggle(&[p(L, 0..5)], L, &(0..2)), [p(L, 2..5)]);
        assert_eq!(toggle(&[p(L, 0..5)], L, &(4..5)), [p(L, 0..4)]);
    }

    #[test]
    fn toggling_the_only_picked_lines_leaves_nothing() {
        assert_eq!(toggle(&[p(L, 1..2)], L, &(1..2)), []);
    }

    #[test]
    fn lines_next_to_a_run_join_it() {
        assert_eq!(toggle(&[p(L, 0..1)], L, &(1..2)), [p(L, 0..2)]);
        assert_eq!(toggle(&[p(L, 2..4)], L, &(1..2)), [p(L, 1..4)]);
    }

    #[test]
    fn lines_that_fill_a_gap_join_both_runs() {
        assert_eq!(toggle(&[p(L, 0..1), p(L, 3..4)], L, &(1..3)), [p(L, 0..4)]);
    }

    #[test]
    fn a_range_partly_picked_is_added_whole() {
        assert_eq!(toggle(&[p(L, 0..2), p(L, 5..6)], L, &(1..4)), [p(L, 0..4), p(L, 5..6)]);
    }

    #[test]
    fn a_range_across_two_runs_is_added_as_it_is_not_all_picked() {
        assert_eq!(toggle(&[p(L, 0..2), p(L, 3..5)], L, &(1..4)), [p(L, 0..5)]);
    }

    #[test]
    fn a_new_run_keeps_line_order_among_the_runs_of_its_source() {
        assert_eq!(
            toggle(&[p(L, 0..1), p(L, 6..7)], L, &(3..4)),
            [p(L, 0..1), p(L, 3..4), p(L, 6..7)]
        );
    }

    #[test]
    fn sources_keep_sides_by_index_then_bases_whatever_the_pick_order() {
        let base_first = toggle(&[], B, &(0..1));
        let then_right = toggle(&base_first, R, &(2..3));
        assert_eq!(toggle(&then_right, L, &(1..2)), [p(L, 1..2), p(R, 2..3), p(B, 0..1)]);
    }

    #[test]
    fn a_toggle_leaves_other_sources_as_they_are() {
        assert_eq!(toggle(&[p(L, 0..2), p(R, 0..2)], R, &(0..1)), [p(L, 0..2), p(R, 1..2)]);
    }

    #[test]
    fn whole_sources_names_each_source_taken_whole_in_order() {
        let count = |source| if source == L { 2 } else { 3 };
        assert_eq!(whole_sources(&[p(L, 0..2), p(R, 0..3)], count), Some(vec![L, R]));
        assert_eq!(whole_sources(&[], count), Some(vec![]));
    }

    #[test]
    fn whole_sources_is_none_when_a_source_is_taken_in_part() {
        let count = |_| 3;
        assert_eq!(whole_sources(&[p(L, 0..3), p(R, 0..2)], count), None);
        assert_eq!(whole_sources(&[p(L, 1..3)], count), None);
        assert_eq!(whole_sources(&[p(L, 0..1), p(L, 2..3)], count), None);
    }
}
