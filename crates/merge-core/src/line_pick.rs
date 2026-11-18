//! Lines picked one by one inside a region (spec §4.4, `PLAN.md` §G8).
//!
//! A selection is a list of [`PickedLines`] in the order they were picked, which is the
//! order the result takes them in. In normal form each range is non-empty, no line is picked
//! twice, and no run ends where the next one, of the same source, starts: those join.

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

/// `selection` with `lines` of `source` toggled. When every one of them is picked, they are
/// removed where they stand; otherwise any of them already picked are removed and the whole
/// range goes last. `selection` is in normal form, and so is the result. `lines` is not empty.
pub(crate) fn toggle(
    selection: &[PickedLines],
    source: Source,
    lines: &Range<usize>,
) -> Vec<PickedLines> {
    let picked = |n: &usize| selection.iter().any(|p| p.source == source && p.lines.contains(n));
    let covered = lines.clone().all(|n| picked(&n));
    let kept = selection.iter().flat_map(|p| {
        if p.source != source {
            return [Some(p.clone()), None];
        }
        let before = p.lines.start..p.lines.end.min(lines.start);
        let after = p.lines.start.max(lines.end)..p.lines.end;
        [before, after].map(|lines| (!lines.is_empty()).then_some(PickedLines { source, lines }))
    });
    let added = (!covered).then(|| PickedLines { source, lines: lines.clone() });
    kept.flatten().chain(added).fold(Vec::new(), |mut runs, run| {
        match runs.last_mut() {
            Some(last) if last.source == run.source && last.lines.end == run.lines.start => {
                last.lines.end = run.lines.end;
            }
            _ => runs.push(run),
        }
        runs
    })
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
    fn a_line_after_its_neighbour_joins_it() {
        assert_eq!(toggle(&[p(L, 0..1)], L, &(1..2)), [p(L, 0..2)]);
    }

    #[test]
    fn a_line_before_its_neighbour_goes_after_it() {
        assert_eq!(toggle(&[p(L, 2..4)], L, &(1..2)), [p(L, 2..4), p(L, 1..2)]);
    }

    #[test]
    fn clicks_interleave_the_sides_in_click_order() {
        let clicks = [(R, 60), (L, 61), (R, 61), (L, 60)];
        let picked = clicks.iter().fold(Vec::new(), |s, &(src, n)| toggle(&s, src, &(n..n + 1)));
        assert_eq!(picked, [p(R, 60..61), p(L, 61..62), p(R, 61..62), p(L, 60..61)]);
    }

    #[test]
    fn a_range_partly_picked_moves_to_the_end_whole() {
        let picked = [p(L, 0..2), p(R, 0..1), p(L, 5..6)];
        assert_eq!(toggle(&picked, L, &(1..4)), [p(L, 0..1), p(R, 0..1), p(L, 5..6), p(L, 1..4)]);
    }

    #[test]
    fn a_range_picked_in_scattered_runs_turns_off() {
        let picked = [p(R, 0..1), p(L, 3..4), p(R, 1..2)];
        assert_eq!(toggle(&picked, R, &(0..2)), [p(L, 3..4)]);
    }

    #[test]
    fn turning_off_the_lines_between_two_runs_joins_them() {
        let picked = [p(R, 0..1), p(L, 3..4), p(R, 1..2)];
        assert_eq!(toggle(&picked, L, &(3..4)), [p(R, 0..2)]);
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
