//! Moved blocks in the two-way diff (spec §4.6, `PLAN.md` §G5).

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use super::{Kind2, Region2};
use crate::diff::{Comparison, DiffOptions, is_blank_line, line_key};
use crate::index::RegionIx;
use crate::text::Text;

/// The fewest lines, blank lines not counted, a deleted block holds to count as moved. Carry
/// Edit's rule (`DESIGN.md` §B5.7, item 4): one line not blank.
pub const MOVE_MIN_LINES: usize = 1;

/// A block of left lines that a change deletes and another change inserts once, as lines
/// compare under the diff's options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    /// The changed region whose left lines moved.
    pub from: RegionIx,
    /// The changed region whose right lines hold the block.
    pub to: RegionIx,
    /// Region `from`'s left lines.
    pub left: Range<usize>,
    /// The block's lines in region `to`'s right lines.
    pub right: Range<usize>,
}

/// One inserted line, or the place a block could start: the line, and its region's right
/// lines.
#[derive(Debug, Clone, Copy)]
struct Place {
    region: usize,
    line: usize,
    start: usize,
    end: usize,
}

/// The moves among `regions` of `left` and `right`, ordered by `from`. Each changed region's
/// left lines, with at least [`MOVE_MIN_LINES`] lines not blank, are matched against the
/// right lines of every changed region. A block moves when it occurs at exactly one
/// place there and no other block's place overlaps that place.
#[must_use]
pub fn moves(left: &Text, right: &Text, regions: &[Region2], opts: &DiffOptions) -> Vec<Move> {
    let mode = opts.comparison();
    let changed = || regions.iter().enumerate().filter(|(_, r)| r.kind == Kind2::Changed);
    let blocks: Vec<(usize, &Region2)> = changed()
        .filter(|(_, r)| {
            let solid = r.left.clone().filter(|&n| !is_blank_line(left.line(n), &opts.patterns));
            solid.count() >= MOVE_MIN_LINES
        })
        .collect();
    if blocks.is_empty() {
        return Vec::new();
    }
    let mut inserted: HashMap<Cow<'_, str>, Vec<Place>> = HashMap::new();
    for (region, r) in changed() {
        for line in r.right.clone() {
            let place = Place { region, line, start: r.right.start, end: r.right.end };
            inserted.entry(line_key(right.line(line), mode)).or_default().push(place);
        }
    }
    let mut found: Vec<Move> = blocks
        .into_iter()
        .filter_map(|(from, r)| {
            // A block sits only where each of its lines does, so its line with the fewest
            // inserted places bounds the places to check.
            let (offset, places) = r
                .left
                .clone()
                .enumerate()
                .map(|(offset, n)| {
                    let places = inserted.get(&line_key(left.line(n), mode));
                    (offset, places.map_or(&[][..], Vec::as_slice))
                })
                .min_by_key(|(_, places)| places.len())?;
            let mut matches = places
                .iter()
                .filter_map(|p| {
                    let line = p.line.checked_sub(offset).filter(|&line| line >= p.start)?;
                    Some(Place { line, ..*p })
                })
                .filter(|p| holds(left, r.left.clone(), right, *p, mode));
            let place = matches.next()?;
            matches.next().is_none().then(|| Move {
                from: RegionIx::new(from),
                to: RegionIx::new(place.region),
                left: r.left.clone(),
                right: place.line..place.line + r.left.len(),
            })
        })
        .collect();
    found.sort_by_key(|m| m.right.start);
    // Sorted by start, a place overlaps a later one when the next starts before it ends, and
    // an earlier one when any earlier place ends after it starts.
    let ends_before =
        found.iter().scan(0, |end, m| Some(std::mem::replace(end, (*end).max(m.right.end))));
    let shared: Vec<bool> = ends_before
        .zip(&found)
        .enumerate()
        .map(|(i, (end_before, m))| {
            end_before > m.right.start
                || found.get(i + 1).is_some_and(|next| next.right.start < m.right.end)
        })
        .collect();
    let mut kept: Vec<Move> =
        found.into_iter().zip(shared).filter(|(_, shared)| !shared).map(|(m, _)| m).collect();
    kept.sort_by_key(|m| m.from);
    kept
}

/// Whether `block` of `left` sits at `place` of `right`, inside its region.
fn holds(
    left: &Text,
    block: Range<usize>,
    right: &Text,
    place: Place,
    mode: Comparison<'_>,
) -> bool {
    place.line + block.len() <= place.end
        && block
            .zip(place.line..)
            .all(|(l, r)| line_key(left.line(l), mode) == line_key(right.line(r), mode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{BlankLineRule, CaseRule, Whitespace};
    use crate::diff2::regions;
    use crate::pattern::Patterns;
    use crate::testing::text;

    fn with(left: &str, right: &str, opts: &DiffOptions) -> (Vec<Region2>, Vec<Move>) {
        let (left, right) = (text(left), text(right));
        let regions = regions(&left, &right, opts);
        let moves = moves(&left, &right, &regions, opts);
        (regions, moves)
    }

    fn of(left: &str, right: &str) -> Vec<Move> {
        with(left, right, &DiffOptions::default()).1
    }

    fn moved(from: usize, to: usize, left: Range<usize>, right: Range<usize>) -> Move {
        Move { from: RegionIx::new(from), to: RegionIx::new(to), left, right }
    }

    fn changed(left: Range<usize>, right: Range<usize>) -> Region2 {
        Region2 { kind: Kind2::Changed, left, right }
    }

    fn same(left: Range<usize>, right: Range<usize>) -> Region2 {
        Region2 { kind: Kind2::Unchanged, left, right }
    }

    /// `m1 m2` sits before `a b c`, which outweighs it in every diff.
    const BLOCK_FIRST: &str = "m1\nm2\na\nb\nc\n";
    const BLOCK_LAST: &str = "a\nb\nc\nm1\nm2\n";

    #[test]
    fn a_block_moved_down_is_a_move_to_the_later_insertion() {
        let (regions, moves) = with(BLOCK_FIRST, BLOCK_LAST, &DiffOptions::default());
        assert_eq!(regions, [changed(0..2, 0..0), same(2..5, 0..3), changed(5..5, 3..5)]);
        assert_eq!(moves, [moved(0, 2, 0..2, 3..5)]);
    }

    #[test]
    fn a_block_moved_up_is_a_move_to_the_earlier_insertion() {
        let (regions, moves) = with(BLOCK_LAST, BLOCK_FIRST, &DiffOptions::default());
        assert_eq!(regions, [changed(0..0, 0..2), same(0..3, 2..5), changed(3..5, 5..5)]);
        assert_eq!(moves, [moved(2, 0, 3..5, 0..2)]);
    }

    #[test]
    fn a_block_moved_among_new_lines_names_its_own_lines() {
        assert_eq!(of(BLOCK_FIRST, "a\nb\nc\nn\nm1\nm2\no\n"), [moved(0, 2, 0..2, 4..6)]);
    }

    #[test]
    fn a_block_replaced_by_other_lines_still_moves() {
        let (regions, moves) =
            with("a\nm1\nm2\nb\nc\nd\n", "a\nX\nb\nc\nd\nm1\nm2\n", &DiffOptions::default());
        assert_eq!(regions[1], changed(1..3, 1..2));
        assert_eq!(moves, [moved(1, 3, 1..3, 5..7)]);
    }

    #[test]
    fn a_block_with_two_candidate_places_is_no_move() {
        assert_eq!(of(BLOCK_FIRST, "a\nb\nc\nm1\nm2\nx\nm1\nm2\n"), []);
    }

    #[test]
    fn overlapping_candidate_places_are_two_places() {
        assert_eq!(of("m\nm\na\nb\nc\n", "a\nb\nc\nm\nm\nm\n"), []);
    }

    #[test]
    fn two_blocks_that_claim_one_place_are_no_move() {
        let (regions, moves) =
            with("x1\na\nb\nc\nx1\nd\ne\nf\n", "a\nb\nc\nd\ne\nf\nx1\n", &DiffOptions::default());
        assert_eq!(
            regions,
            [
                changed(0..1, 0..0),
                same(1..4, 0..3),
                changed(4..5, 3..3),
                same(5..8, 3..6),
                changed(8..8, 6..7),
            ]
        );
        assert_eq!(moves, []);
    }

    #[test]
    fn a_place_inside_a_longer_one_two_places_back_is_shared() {
        // `x y z`, `y` and `z` each occur once in the insertion, at 9..12, 10..11 and 11..12.
        let (regions, moves) = with(
            "x\ny\nz\na\nb\nc\ny\nd\ne\nf\nz\ng\nh\ni\n",
            "a\nb\nc\nd\ne\nf\ng\nh\ni\nx\ny\nz\n",
            &DiffOptions::default(),
        );
        assert_eq!(regions[6], changed(14..14, 9..12));
        assert_eq!(moves, []);
    }

    #[test]
    fn a_block_changed_on_the_way_is_no_move() {
        assert_eq!(of(BLOCK_FIRST, "a\nb\nc\nm1\nm2!\n"), []);
    }

    #[test]
    fn a_block_cut_short_at_the_end_of_its_region_is_no_move() {
        assert_eq!(of(BLOCK_FIRST, "a\nb\nc\nm1\n"), []);
    }

    #[test]
    fn a_deleted_block_found_nowhere_is_no_move() {
        assert_eq!(of(BLOCK_FIRST, "a\nb\nc\n"), []);
    }

    #[test]
    fn a_block_of_blank_lines_is_no_move() {
        assert_eq!(of("\n \t\na\nb\nc\n", "a\nb\nc\n\n \t\n"), []);
    }

    #[test]
    fn one_line_not_blank_is_enough() {
        assert_eq!(MOVE_MIN_LINES, 1);
        assert_eq!(of("\nm\na\nb\nc\n", "a\nb\nc\n\nm\n"), [moved(0, 2, 0..2, 3..5)]);
    }

    #[test]
    fn a_block_a_pattern_empties_is_blank_and_no_move() {
        let opts = DiffOptions {
            patterns: Patterns::new(["//.*"]).expect("valid"),
            ..DiffOptions::default()
        };
        assert_eq!(with("// note\na\nb\nc\n", "a\nb\nc\n// note\n", &opts).1, []);
    }

    #[test]
    fn a_move_under_ignore_whitespace_all_matches_respaced_lines() {
        let respaced = "a\nb\nc\n  m1\nm 2\n";
        assert_eq!(of(BLOCK_FIRST, respaced), [], "lines compare byte for byte by default");
        let all = DiffOptions { whitespace: Whitespace::All, ..DiffOptions::default() };
        assert_eq!(with(BLOCK_FIRST, respaced, &all).1, [moved(0, 2, 0..2, 3..5)]);
    }

    #[test]
    fn a_move_under_ignore_case_matches_recased_lines() {
        let recased = "a\nb\nc\nM1\nm2\n";
        assert_eq!(of(BLOCK_FIRST, recased), []);
        let fold = DiffOptions { case: CaseRule::Ignore, ..DiffOptions::default() };
        assert_eq!(with(BLOCK_FIRST, recased, &fold).1, [moved(0, 2, 0..2, 3..5)]);
    }

    #[test]
    fn ignored_regions_neither_move_nor_take_moves() {
        let opts = DiffOptions { blank_lines: BlankLineRule::Ignore, ..DiffOptions::default() };
        let (regions, moves) = with("\n\na\nb\nc\n", "a\nb\nc\n\n\n", &opts);
        assert!(regions.iter().all(|r| r.kind != Kind2::Changed));
        assert_eq!(moves, []);
    }

    #[test]
    fn a_region_can_be_both_ends_of_two_moves() {
        let (regions, moves) =
            with("p\na\nb\nc\nq\nd\ne\nf\n", "q\na\nb\nc\nd\ne\nf\np\n", &DiffOptions::default());
        assert_eq!(
            regions,
            [
                changed(0..1, 0..1),
                same(1..4, 1..4),
                changed(4..5, 4..4),
                same(5..8, 4..7),
                changed(8..8, 7..8),
            ]
        );
        assert_eq!(moves, [moved(0, 4, 0..1, 7..8), moved(2, 0, 4..5, 0..1)]);
    }

    #[test]
    fn braced_blocks_move_only_where_the_generator_moves_them() {
        // 40 units of 5 lines: units 0 and 20 delete `} moved N`, which right inserts in
        // units 11 and 31. Every other block starts with `}` too and moves nowhere.
        let (left, right) = crate::testing::generate_braced_changes(200);
        let (left, right) = (text(&left), text(&right));
        let found = moves(
            &left,
            &right,
            &regions(&left, &right, &DiffOptions::default()),
            &DiffOptions::default(),
        );
        let ends: Vec<(&str, &str)> = found
            .iter()
            .map(|m| (left.span(m.left.clone()), right.span(m.right.clone())))
            .collect();
        assert_eq!(ends, [("}\nmoved 0\n", "}\nmoved 0\n"), ("}\nmoved 20\n", "}\nmoved 20\n")]);
    }

    #[test]
    fn a_block_anchored_on_a_later_line_still_needs_room_above_it() {
        // `x` is rarer among the inserted lines than `m`, and its only place starts the
        // insertion. The `m` above it is unchanged, outside the region, so `m x` has no place.
        let (regions, moves) =
            with("m\nx\na\nb\nc\nm\nz\n", "a\nb\nc\nm\nx\nm\nm\nz\n", &DiffOptions::default());
        assert_eq!(regions[1], same(2..6, 0..4));
        assert_eq!(regions[2], changed(6..6, 4..7));
        assert_eq!(moves, []);
    }

    #[test]
    fn files_without_changes_have_no_moves() {
        assert_eq!(of("a\nb\n", "a\nb\n"), []);
        assert_eq!(of("", ""), []);
    }
}
