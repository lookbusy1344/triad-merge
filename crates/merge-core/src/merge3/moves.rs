//! Blocks a side moved from base, in a three-way merge (`PLAN.md` §G16).

use std::ops::Range;

use super::{Inputs, Kind, Region};
use crate::diff::{Comparison, DiffOptions, line_key};
use crate::diff2::{self, Kind2, Region2};
use crate::text::Text;

/// A block of base lines that one side deletes and inserts at one other place, as lines
/// compare under the merge's options. Lines index the inputs, which never change, so a move
/// outlives the edits that join the result's regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideMove {
    /// The side that moved the block.
    pub side: usize,
    /// The block's lines in base.
    pub base: Range<usize>,
    /// The block's lines in the side.
    pub lines: Range<usize>,
}

/// The moves each side of a three-way merge makes from base, ordered by side, then by base
/// line. A side's changes against base are the regions where its lines differ from base's,
/// less the lines at either end it shares with base; `diff2::moves` matches them under
/// `opts`. A merge of more than two sides has none.
#[must_use]
pub fn side_moves(inputs: &Inputs, regions: &[Region], opts: &DiffOptions) -> Vec<SideMove> {
    let ([_, _], [base]) = (inputs.sides(), inputs.bases()) else {
        return Vec::new();
    };
    let mode = opts.comparison();
    inputs
        .sides()
        .iter()
        .enumerate()
        .flat_map(|(side, text)| {
            let changes: Vec<Region2> = regions
                .iter()
                .filter(|r| r.kind != Kind::Unchanged)
                .filter_map(|r| {
                    let (left, right) =
                        trimmed(base, r.bases[0].clone(), text, r.sides[side].clone(), mode);
                    (!(left.is_empty() && right.is_empty())).then_some(Region2 {
                        kind: Kind2::Changed,
                        left,
                        right,
                    })
                })
                .collect();
            diff2::moves(base, text, &changes, opts).into_iter().map(move |m| SideMove {
                side,
                base: m.left,
                lines: m.right,
            })
        })
        .collect()
}

/// `base` and `side` without the lines at either end that compare equal.
fn trimmed(
    base_text: &Text,
    base: Range<usize>,
    side_text: &Text,
    side: Range<usize>,
    mode: Comparison<'_>,
) -> (Range<usize>, Range<usize>) {
    let same =
        |b: usize, s: usize| line_key(base_text.line(b), mode) == line_key(side_text.line(s), mode);
    let shortest = base.len().min(side.len());
    let head = (0..shortest).take_while(|&i| same(base.start + i, side.start + i)).count();
    let tail =
        (0..shortest - head).take_while(|&i| same(base.end - 1 - i, side.end - 1 - i)).count();
    (base.start + head..base.end - tail, side.start + head..side.end - tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::Whitespace;
    use crate::document::MergeOptions;
    use crate::merge3::{Semantics, align};
    use crate::testing::{text, three};

    /// The moves under `diff`, in both semantics, which must agree.
    fn moves_with(base: &str, left: &str, right: &str, diff: &DiffOptions) -> Vec<SideMove> {
        let inputs = three(base, left, right);
        let [git, jj] = [Semantics::Git, Semantics::Jj].map(|semantics| {
            let options = MergeOptions { semantics, diff: diff.clone(), ..MergeOptions::default() };
            side_moves(&inputs, &align(&inputs, &options.alignment()), &options.diff)
        });
        assert_eq!(git, jj);
        git
    }

    fn moves(base: &str, left: &str, right: &str) -> Vec<SideMove> {
        moves_with(base, left, right, &DiffOptions::default())
    }

    fn moved(side: usize, base: Range<usize>, lines: Range<usize>) -> SideMove {
        SideMove { side, base, lines }
    }

    /// `m1 m2` before `a b c`, which outweighs it in every diff.
    const BLOCK_FIRST: &str = "m1\nm2\na\nb\nc\n";
    const BLOCK_LAST: &str = "a\nb\nc\nm1\nm2\n";

    #[test]
    fn a_block_left_moves_down_is_a_left_move() {
        assert_eq!(moves(BLOCK_FIRST, BLOCK_LAST, BLOCK_FIRST), [moved(0, 0..2, 3..5)]);
    }

    #[test]
    fn a_block_right_moves_up_is_a_right_move() {
        assert_eq!(moves(BLOCK_LAST, BLOCK_LAST, BLOCK_FIRST), [moved(1, 3..5, 0..2)]);
    }

    #[test]
    fn a_block_both_sides_move_to_one_place_is_a_move_on_each() {
        assert_eq!(
            moves(BLOCK_FIRST, BLOCK_LAST, BLOCK_LAST),
            [moved(0, 0..2, 3..5), moved(1, 0..2, 3..5)]
        );
    }

    #[test]
    fn a_block_the_sides_move_to_two_places_is_a_move_on_each() {
        // Both delete `m1 m2` at the top; left puts it at the end, right before `d`.
        assert_eq!(
            moves("m1\nm2\na\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\nm1\nm2\n", "a\nb\nc\nm1\nm2\nd\ne\n"),
            [moved(0, 0..2, 5..7), moved(1, 0..2, 3..5)]
        );
    }

    #[test]
    fn a_block_at_two_places_on_its_side_is_no_move() {
        assert_eq!(moves(BLOCK_FIRST, "a\nb\nc\nm1\nm2\nx\nm1\nm2\n", BLOCK_FIRST), []);
    }

    #[test]
    fn a_block_of_blank_lines_is_no_move() {
        assert_eq!(moves("\n\na\nb\nc\n", "a\nb\nc\n\n\n", "\n\na\nb\nc\n"), []);
    }

    #[test]
    fn a_move_next_to_the_other_sides_change_names_the_moved_lines_alone() {
        // One conflict over base `m1 m2 z`: left keeps `z`, right edits it. Left's block is
        // `m1 m2`; right's change is `z` to `Z`, which moves nowhere.
        assert_eq!(
            moves("m1\nm2\nz\na\nb\nc\n", "z\na\nb\nc\nm1\nm2\n", "m1\nm2\nZ\na\nb\nc\n"),
            [moved(0, 0..2, 4..6)]
        );
    }

    #[test]
    fn a_block_moved_on_one_side_and_edited_on_the_other_is_a_move() {
        // Carry Edit's case: the move shows beside the suggestion.
        assert_eq!(moves(BLOCK_FIRST, BLOCK_LAST, "m1\nM2\na\nb\nc\n"), [moved(0, 0..2, 3..5)]);
    }

    #[test]
    fn a_block_moved_with_other_spacing_moves_only_when_whitespace_is_ignored() {
        let spaced = "a\nb\nc\nm1 \nm2\n";
        assert_eq!(moves(BLOCK_FIRST, spaced, BLOCK_FIRST), []);
        let trailing = DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() };
        assert_eq!(moves_with(BLOCK_FIRST, spaced, BLOCK_FIRST, &trailing), [moved(0, 0..2, 3..5)]);
    }

    #[test]
    fn unchanged_inputs_move_nothing() {
        assert_eq!(moves(BLOCK_FIRST, BLOCK_FIRST, BLOCK_FIRST), []);
        assert_eq!(moves("", "", ""), []);
    }

    #[test]
    fn a_merge_of_three_sides_has_no_moves() {
        let inputs = Inputs::new(
            vec![text(BLOCK_LAST), text(BLOCK_FIRST), text(BLOCK_FIRST)],
            vec![text(BLOCK_FIRST), text(BLOCK_FIRST)],
        )
        .unwrap();
        let options = MergeOptions::default();
        let regions = align(&inputs, &options.alignment());
        assert_eq!(side_moves(&inputs, &regions, &options.diff), []);
    }
}
