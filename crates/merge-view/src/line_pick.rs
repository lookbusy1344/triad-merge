//! The lines a press or a selection in a top pane picks (spec §4.4, `PLAN.md` §G8): an input
//! line's region and its place there.

use std::ops::Range;

use merge_core::document::MergeDocument;
use merge_core::index::RegionIx;
use merge_core::line_pick::PickedLines;
use merge_core::merge3::{Kind, Region, Source};

use crate::selection::InputSelection;

/// Lines of one input in one region, counted from the region's first line of that input, as
/// [`merge_core::document::Command::ToggleLines`] takes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineTarget {
    pub region: RegionIx,
    pub source: Source,
    pub lines: Range<usize>,
}

fn source_lines(region: &Region, source: Source) -> &Range<usize> {
    match source {
        Source::Side(i) => &region.sides[i],
        Source::Base(i) => &region.bases[i],
    }
}

/// The target of `source`'s input line `line`: its region and its place there. `None` in an
/// unchanged region, which has no pick targets, or past the input's last line.
#[must_use]
pub fn line_target(doc: &MergeDocument, source: Source, line: usize) -> Option<LineTarget> {
    let regions = doc.regions();
    let index = regions.partition_point(|r| source_lines(r, source).end <= line);
    let region = regions.get(index).filter(|r| r.kind != Kind::Unchanged)?;
    let start = source_lines(region, source).start;
    let at = line.checked_sub(start)?;
    Some(LineTarget { region: RegionIx::new(index), source, lines: at..at + 1 })
}

/// The lines a top pane's selection spans, inside the region of its first line: a selection
/// that ends at a line's start leaves that line out, and an empty one takes the line it is
/// on. `None` when the first line has no target.
#[must_use]
pub fn selected_lines(doc: &MergeDocument, selection: InputSelection) -> Option<LineTarget> {
    let source = selection.source();
    let text = doc.inputs().text(source);
    let bytes = selection.range();
    let first = text.line_of_byte(bytes.start);
    let last = if bytes.is_empty() { first } else { text.line_of_byte(bytes.end - 1) };
    let target = line_target(doc, source, first)?;
    let region = doc.region(target.region);
    let end = (last + 1).min(source_lines(region, source).end) - source_lines(region, source).start;
    Some(LineTarget { lines: target.lines.start..end, ..target })
}

/// The run from `anchor` to `to`, both ends included, when both lie in one region of one
/// input, as a shift-click extends a pick; otherwise `to` alone.
#[must_use]
pub fn extend(anchor: &LineTarget, to: LineTarget) -> LineTarget {
    if (anchor.region, anchor.source) != (to.region, to.source) {
        return to;
    }
    let start = anchor.lines.start.min(to.lines.start);
    let end = anchor.lines.end.max(to.lines.end);
    LineTarget { lines: start..end, ..to }
}

/// Whether line `line` of `source`, counted from the region's first line of it, is in
/// `selection`.
#[must_use]
pub fn is_picked(selection: &[PickedLines], source: Source, line: usize) -> bool {
    selection.iter().any(|p| p.source == source && p.lines.contains(&line))
}

#[cfg(test)]
mod tests {
    use merge_core::document::Command;
    use merge_core::testing::doc;

    use super::*;

    const L: Source = Source::Side(0);
    const R: Source = Source::Side(1);
    const B: Source = Source::Base(0);

    /// Regions: 0 `a`, 1 a conflict of two left lines, two base lines and three right
    /// lines, 2 `c`. Left's text: `a\n` at 0..2, `L1\n` at 2..5, `L2\n` at 5..8, `c\n` at
    /// 8..10.
    fn conflict() -> MergeDocument {
        let d = doc("a\nb1\nb2\nc\n", "a\nL1\nL2\nc\n", "a\nR1\nR2\nR3\nc\n");
        assert_eq!(d.regions().len(), 3);
        assert_eq!(d.regions()[1].sides, [1..3, 1..4]);
        d
    }

    fn target(region: usize, source: Source, lines: Range<usize>) -> LineTarget {
        LineTarget { region: RegionIx::new(region), source, lines }
    }

    #[test]
    fn an_input_line_targets_its_place_in_its_region() {
        let d = conflict();
        assert_eq!(line_target(&d, L, 2), Some(target(1, L, 1..2)));
        assert_eq!(line_target(&d, R, 3), Some(target(1, R, 2..3)));
        assert_eq!(line_target(&d, B, 1), Some(target(1, B, 0..1)));
    }

    #[test]
    fn unchanged_lines_and_lines_past_the_end_have_no_target() {
        let d = conflict();
        assert_eq!(line_target(&d, L, 0), None);
        assert_eq!(line_target(&d, L, 3), None);
        assert_eq!(line_target(&d, L, 4), None);
    }

    #[test]
    fn a_selection_takes_the_lines_it_touches() {
        let d = conflict();
        // From inside `L1` to inside `L2`.
        assert_eq!(selected_lines(&d, InputSelection::new(L, 3, 6)), Some(target(1, L, 0..2)));
        // Backwards: the head before the anchor.
        assert_eq!(selected_lines(&d, InputSelection::new(L, 6, 3)), Some(target(1, L, 0..2)));
    }

    #[test]
    fn a_selection_ending_at_a_line_start_leaves_that_line_out() {
        let d = conflict();
        assert_eq!(selected_lines(&d, InputSelection::new(L, 2, 5)), Some(target(1, L, 0..1)));
    }

    #[test]
    fn an_empty_selection_takes_its_line() {
        let d = conflict();
        assert_eq!(selected_lines(&d, InputSelection::caret(L, 6)), Some(target(1, L, 1..2)));
    }

    #[test]
    fn a_selection_stops_at_the_end_of_its_first_lines_region() {
        let d = conflict();
        assert_eq!(selected_lines(&d, InputSelection::new(L, 3, 10)), Some(target(1, L, 0..2)));
    }

    #[test]
    fn a_selection_starting_in_an_unchanged_region_has_no_target() {
        let d = conflict();
        assert_eq!(selected_lines(&d, InputSelection::new(L, 0, 6)), None);
    }

    #[test]
    fn extend_runs_from_the_anchor_in_either_direction() {
        assert_eq!(extend(&target(1, L, 3..4), target(1, L, 0..1)), target(1, L, 0..4));
        assert_eq!(extend(&target(1, L, 0..1), target(1, L, 2..3)), target(1, L, 0..3));
    }

    #[test]
    fn extend_across_regions_or_inputs_takes_the_new_line_alone() {
        assert_eq!(extend(&target(1, L, 0..1), target(3, L, 2..3)), target(3, L, 2..3));
        assert_eq!(extend(&target(1, L, 0..1), target(1, R, 2..3)), target(1, R, 2..3));
    }

    #[test]
    fn is_picked_reads_the_selection_by_input_and_line() {
        let mut d = conflict();
        d.apply(Command::ToggleLines { region: RegionIx::new(1), source: R, lines: 1..2 })
            .expect("the line exists");
        let selection = d.line_selection(RegionIx::new(1));
        assert_eq!([0, 1, 2].map(|n| is_picked(&selection, R, n)), [false, true, false]);
        assert!(!is_picked(&selection, L, 1));
    }
}
