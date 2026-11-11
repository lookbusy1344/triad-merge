//! Resolutions triad offers for a conflict and never applies itself (spec §4.4,
//! `docs/PLAN.md` §B5.7). The user takes one from the region's menu.

use std::cmp::Ordering;
use std::ops::Range;

use crate::document::MergeDocument;
use crate::edit::{EditBatch, TextPatch};
use crate::index::RegionIx;
use crate::merge3::{Inputs, Kind, Region};
use crate::text::Text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggestion {
    /// Every side inserts lines where base has none: take every side, in order. In a
    /// three-way merge, left then right.
    EverySide,
    /// Every side adds lines to a block that is sorted in every input: the sorted union of
    /// the sides' lines, each line as often as the side that holds it most. Offered only
    /// when it differs from every side in order.
    SortedUnion(String),
    /// In a three-way merge, one side deletes a block that the other side edits, and inserts
    /// the block unchanged in exactly one other region, which resolves to the moving side.
    /// Carry `side`'s edit there: region `to` takes `text`, and the conflict takes the moving
    /// side's deletion. A block of blank lines is never matched.
    CarryEdit {
        /// The side that edits the block.
        side: usize,
        /// The region the block moves to.
        to: RegionIx,
        /// Region `to`'s text with the edited block in place of the moved one.
        text: String,
    },
}

/// A suggestion the user takes as an edit of the result; [`Suggestion::EverySide`] is a pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditSuggestion {
    /// [`Suggestion::SortedUnion`].
    SortedUnion,
    /// [`Suggestion::CarryEdit`].
    CarryEdit,
}

/// The edit that takes `wanted` for region `region` of `doc`, in the result's current
/// offsets; none when the region does not offer it. Every region the edit touches becomes
/// `Edited` when the batch is applied, in one undo step.
///
/// # Panics
///
/// If `region` is out of range for `doc`.
#[must_use]
pub fn edit(doc: &MergeDocument, region: RegionIx, wanted: EditSuggestion) -> Option<EditBatch> {
    let patches = suggestions(doc.inputs(), doc.regions(), region).into_iter().find_map(
        |suggestion| match (wanted, suggestion) {
            (EditSuggestion::SortedUnion, Suggestion::SortedUnion(text)) => {
                Some(vec![TextPatch { range: doc.region_bytes(region), text }])
            }
            (EditSuggestion::CarryEdit, Suggestion::CarryEdit { to, text, .. }) => {
                let deletion = TextPatch { range: doc.region_bytes(region), text: String::new() };
                let carried = TextPatch { range: doc.region_bytes(to), text };
                let mut patches: Vec<TextPatch> = [deletion, carried]
                    .into_iter()
                    .filter(|patch| !(patch.range.is_empty() && patch.text.is_empty()))
                    .collect();
                patches.sort_by_key(|patch| patch.range.start);
                Some(patches)
            }
            _ => None,
        },
    )?;
    Some(EditBatch(patches))
}

/// The suggestions for region `index` of `regions`, none unless it is a conflict.
///
/// # Panics
///
/// If `index` is out of range for `regions`.
#[must_use]
pub fn suggestions(inputs: &Inputs, regions: &[Region], index: RegionIx) -> Vec<Suggestion> {
    let index = index.get();
    let region = &regions[index];
    if region.kind != Kind::Conflict {
        return Vec::new();
    }
    let inserts_only = region.bases.iter().all(Range::is_empty)
        && region.sides.iter().all(|side| !side.is_empty());
    let every_side = inserts_only.then_some(Suggestion::EverySide);
    let union = sorted_union(inputs, region).map(Suggestion::SortedUnion);
    let carry = carried_edit(inputs, regions, index);
    every_side.into_iter().chain(union).chain(carry).collect()
}

/// The [`Suggestion::CarryEdit`] for conflict `index`, when one side moved its block.
fn carried_edit(inputs: &Inputs, regions: &[Region], index: usize) -> Option<Suggestion> {
    let ([left, right], [base]) = (inputs.sides(), inputs.bases()) else {
        return None;
    };
    let region = &regions[index];
    let block = base.span(region.bases[0].clone());
    if block.trim().is_empty() {
        return None;
    }
    // The mover deletes the block; the editor keeps something in its place.
    let (mover, side) = match (region.sides[0].is_empty(), region.sides[1].is_empty()) {
        (true, false) => (0, 1),
        (false, true) => (1, 0),
        _ => return None,
    };
    let texts = [left, right];
    let mover_text = texts[mover];
    let edited = texts[side].span(region.sides[side].clone());
    let block_lines = region.bases[0].len();
    // Every place the block starts in a region that takes the mover's text.
    let mut places =
        regions.iter().enumerate().filter(|(_, r)| r.kind == Kind::OneSide(mover)).flat_map(
            |(to, r)| {
                let end = r.sides[mover].end;
                r.sides[mover]
                    .clone()
                    .filter(move |&at| at + block_lines <= end)
                    .filter(move |&at| mover_text.span(at..at + block_lines) == block)
                    .map(move |at| (to, r, at))
            },
        );
    let (to, r, at) = places.next()?;
    let lines = &r.sides[mover];
    if places.next().is_some() {
        return None;
    }
    let text =
        [mover_text.span(lines.start..at), edited, mover_text.span(at + block_lines..lines.end)]
            .concat();
    Some(Suggestion::CarryEdit { side, to: RegionIx::new(to), text })
}

/// The sorted union of the sides' lines, when every side adds lines to a block sorted in
/// every input, and it differs from every side in order. Each line must end in a newline, so
/// the union can put any line anywhere.
fn sorted_union(inputs: &Inputs, region: &Region) -> Option<String> {
    let sides: Vec<Vec<&str>> = inputs.sides().iter().zip(&region.sides).map(block).collect();
    let bases: Vec<Vec<&str>> = inputs.bases().iter().zip(&region.bases).map(block).collect();
    let blocks = || sides.iter().chain(&bases);
    let complete = blocks().flatten().all(|line| line.ends_with('\n'));
    let sorted = blocks().all(|lines| lines.is_sorted_by_key(|line| text_of(line)));
    // A side holds a base's lines when adding them to it adds nothing.
    let adds = sides.iter().all(|side| {
        bases.iter().all(|base| side.len() > base.len() && union(side, base).len() == side.len())
    });
    if !(complete && sorted && adds) {
        return None;
    }
    let (first, rest) = sides.split_first()?;
    let merged = rest.iter().fold(first.clone(), |merged, side| union(&merged, side));
    (merged != sides.concat()).then(|| merged.concat())
}

/// The lines of `text` in `lines`, with their endings.
fn block<'t>((text, lines): (&'t Text, &Range<usize>)) -> Vec<&'t str> {
    lines.clone().map(|n| text.line(n)).collect()
}

/// A line without its ending.
fn text_of(line: &str) -> &str {
    let line = line.strip_suffix('\n').unwrap_or(line);
    line.strip_suffix('\r').unwrap_or(line)
}

/// The sorted union of two sorted blocks, ordered by [`text_of`]: each line as often as the
/// block that holds it most. Of two lines with the same text, `a`'s bytes stay.
fn union<'t>(a: &[&'t str], b: &[&'t str]) -> Vec<&'t str> {
    let mut merged = Vec::with_capacity(a.len().max(b.len()));
    let (mut i, mut j) = (0, 0);
    while let (Some(&x), Some(&y)) = (a.get(i), b.get(j)) {
        match text_of(x).cmp(text_of(y)) {
            Ordering::Less => {
                merged.push(x);
                i += 1;
            }
            Ordering::Greater => {
                merged.push(y);
                j += 1;
            }
            Ordering::Equal => {
                merged.push(x);
                i += 1;
                j += 1;
            }
        }
    }
    merged.extend(&a[i..]);
    merged.extend(&b[j..]);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::MergeOptions;
    use crate::merge3::{Inputs, Semantics, align};
    use crate::testing::{text, three};

    /// Each region's suggestions, in both semantics, which must agree.
    fn suggested(base: &str, left: &str, right: &str) -> Vec<Vec<Suggestion>> {
        let inputs = three(base, left, right);
        let [git, jj] = [Semantics::Git, Semantics::Jj].map(|semantics| {
            let options = MergeOptions { semantics, ..MergeOptions::default() };
            let regions = align(&inputs, options.alignment());
            (0..regions.len())
                .map(|region| suggestions(&inputs, &regions, RegionIx::new(region)))
                .collect::<Vec<_>>()
        });
        assert_eq!(git, jj);
        git
    }

    const NONE: Vec<Suggestion> = Vec::new();

    #[test]
    fn two_inserts_at_one_place_suggest_every_side() {
        assert_eq!(
            suggested("a\nb\n", "a\nx\nb\n", "a\ny\nz\nb\n"),
            [NONE, vec![Suggestion::EverySide], NONE]
        );
    }

    #[test]
    fn two_inserts_at_the_start_and_at_the_end_suggest_every_side() {
        assert_eq!(suggested("a\n", "x\na\n", "y\na\n"), [vec![Suggestion::EverySide], NONE]);
        assert_eq!(suggested("a\n", "a\nx\n", "a\ny\n"), [NONE, vec![Suggestion::EverySide]]);
    }

    #[test]
    fn two_inserts_into_empty_base_suggest_every_side() {
        assert_eq!(suggested("", "x\n", "y\n"), [vec![Suggestion::EverySide]]);
    }

    #[test]
    fn inserts_sharing_a_first_line_suggest_every_side_for_what_differs() {
        // Refinement splits the shared `s` off; the conflict left inserts x against y.
        assert_eq!(
            suggested("a\nb\n", "a\ns\nx\nb\n", "a\ns\ny\nb\n"),
            [NONE, NONE, vec![Suggestion::EverySide], NONE]
        );
    }

    #[test]
    fn a_conflict_that_changes_base_lines_suggests_nothing() {
        assert_eq!(suggested("a\nb\nc\n", "a\nx\nc\n", "a\ny\nc\n"), [NONE, NONE, NONE]);
    }

    #[test]
    fn an_insert_against_a_deletion_suggests_nothing() {
        // Left inserts after `a`, right deletes it: one conflict over `a`, then `b`.
        assert_eq!(suggested("a\nb\n", "a\nx\nb\n", "b\n"), [NONE, NONE]);
    }

    #[test]
    fn non_conflicts_suggest_nothing() {
        // One side inserts; both insert the same; nothing changes.
        assert_eq!(suggested("a\n", "a\nx\n", "a\n"), [NONE, NONE]);
        assert_eq!(suggested("a\n", "a\nx\n", "a\nx\n"), [NONE, NONE]);
        assert_eq!(suggested("a\n", "a\n", "a\n"), [NONE]);
    }

    #[test]
    fn a_side_that_inserts_nothing_among_three_suggests_nothing() {
        // Sides x, (nothing), y over two empty bases: not every side inserts.
        let inputs = Inputs::new(
            vec![text("a\nx\n"), text("a\n"), text("a\ny\n")],
            vec![text("a\n"), text("a\n")],
        )
        .unwrap();
        let regions = align(&inputs, MergeOptions::default().alignment());
        assert_eq!(regions[1].kind, Kind::Conflict);
        assert_eq!(suggestions(&inputs, &regions, RegionIx::new(1)), NONE);
    }

    #[test]
    fn three_sides_that_all_insert_suggest_every_side() {
        let inputs = Inputs::new(
            // Already in order, so the sorted union adds nothing to every side.
            vec![text("a\nw\n"), text("a\nx\n"), text("a\ny\n")],
            vec![text("a\n"), text("a\n")],
        )
        .unwrap();
        let regions = align(&inputs, MergeOptions::default().alignment());
        assert_eq!(regions[1].kind, Kind::Conflict);
        assert_eq!(suggestions(&inputs, &regions, RegionIx::new(1)), [Suggestion::EverySide]);
    }

    // Sorted union

    fn union(text: &str) -> Suggestion {
        Suggestion::SortedUnion(text.to_owned())
    }

    #[test]
    fn imports_added_out_of_order_suggest_their_sorted_union() {
        assert_eq!(
            suggested("use a;\nuse d;\n", "use a;\nuse c;\nuse d;\n", "use a;\nuse b;\nuse d;\n"),
            [NONE, vec![Suggestion::EverySide, union("use b;\nuse c;\n")], NONE]
        );
    }

    #[test]
    fn a_union_equal_to_every_side_in_order_is_not_offered_again() {
        assert_eq!(
            suggested("a\nd\n", "a\nb\nd\n", "a\nc\nd\n"),
            [NONE, vec![Suggestion::EverySide], NONE]
        );
    }

    #[test]
    fn lines_both_sides_add_appear_once_in_the_union() {
        assert_eq!(
            suggested("a\nz\n", "a\nc\nd\nz\n", "a\nb\nc\nz\n"),
            [NONE, vec![Suggestion::EverySide, union("b\nc\nd\n")], NONE]
        );
    }

    #[test]
    fn a_line_one_side_repeats_appears_as_often_as_there() {
        assert_eq!(
            suggested("a\nz\n", "a\nc\nc\nd\nz\n", "a\nb\nc\ne\nz\n"),
            [NONE, vec![Suggestion::EverySide, union("b\nc\nc\nd\ne\n")], NONE]
        );
    }

    #[test]
    fn the_union_keeps_each_lines_bytes() {
        assert_eq!(
            suggested("a\r\nd\r\n", "a\r\nc\r\nd\r\n", "a\r\nb\r\nd\r\n"),
            [NONE, vec![Suggestion::EverySide, union("b\r\nc\r\n")], NONE]
        );
    }

    #[test]
    fn the_union_orders_lines_by_their_text_without_the_ending() {
        // By text, `b` sorts before `b\tx`. With the ending, `b\n` would sort after it, since
        // `\n` (0x0a) follows `\t` (0x09), and the union would equal left then right.
        assert_eq!(
            suggested("a\nz\n", "a\nb\tx\nz\n", "a\nb\nz\n"),
            [NONE, vec![Suggestion::EverySide, union("b\nb\tx\n")], NONE]
        );
    }

    #[test]
    fn an_unsorted_side_suggests_no_union() {
        assert_eq!(
            suggested("a\nz\n", "a\ny\nx\nz\n", "a\nb\nz\n"),
            [NONE, vec![Suggestion::EverySide], NONE]
        );
    }

    #[test]
    fn a_side_that_drops_a_base_line_suggests_no_union() {
        // Left deletes `b`; right adds `c` after it. One conflict over `b`.
        assert_eq!(suggested("a\nb\nd\n", "a\nd\n", "a\nb\nc\nd\n"), [NONE, NONE, NONE]);
    }

    #[test]
    fn a_line_without_a_newline_suggests_no_union() {
        assert_eq!(suggested("a\n", "a\nc", "a\nb"), [NONE, vec![Suggestion::EverySide]]);
    }

    #[test]
    fn taking_the_union_as_an_edit_resolves_the_region_to_it() {
        use crate::document::{
            Command, EditBatch, Labels, MergeDocument, Output, Resolution, TextPatch,
        };
        let inputs = three("a\nd\n", "a\nc\nd\n", "a\nb\nd\n");
        let mut doc = MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).unwrap();
        let Some(Suggestion::SortedUnion(union)) =
            suggestions(doc.inputs(), doc.regions(), RegionIx::new(1)).pop()
        else {
            panic!("the conflict offers a sorted union");
        };
        let patch = TextPatch { range: doc.region_bytes(RegionIx::new(1)), text: union };
        doc.apply(Command::Edit(EditBatch(vec![patch]))).unwrap();
        assert_eq!(doc.resolutions()[1], Resolution::Edited);
        assert_eq!(doc.output(), Output::Resolved("a\nb\nc\nd\n".to_owned()));
    }

    #[test]
    fn the_sorted_union_edit_resolves_the_region_to_it() {
        use crate::document::{Command, Labels, Output, Resolution};
        let inputs = three("a\nd\n", "a\nc\nd\n", "a\nb\nd\n");
        let mut doc = MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).unwrap();
        let conflict = RegionIx::new(1);
        let batch =
            edit(&doc, conflict, EditSuggestion::SortedUnion).expect("the union is offered");
        assert_eq!(batch.0.len(), 1);
        assert_eq!(batch.0[0].text, "b\nc\n");
        assert_eq!(edit(&doc, conflict, EditSuggestion::CarryEdit), None);
        assert_eq!(edit(&doc, RegionIx::new(0), EditSuggestion::SortedUnion), None);
        doc.apply(Command::Edit(batch)).unwrap();
        assert_eq!(doc.resolutions()[1], Resolution::Edited);
        assert_eq!(doc.output(), Output::Resolved("a\nb\nc\nd\n".to_owned()));
    }

    #[test]
    fn the_carried_edit_moves_the_edit_and_resolves_the_conflict_to_the_deletion() {
        use crate::document::{Command, Labels, Output, Resolution};
        // Regions: the conflict over `m1 m2`, `a b c`, left's insert of `m1 m2`.
        let inputs = Inputs::three_way(
            text(MOVE_BASE),
            text("a\nb\nc\nm1\nm2\n"),
            text("m1\nM2\na\nb\nc\n"),
        );
        let mut doc = MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).unwrap();
        let conflict = RegionIx::new(0);
        assert_eq!(edit(&doc, conflict, EditSuggestion::SortedUnion), None);
        let batch = edit(&doc, conflict, EditSuggestion::CarryEdit).expect("the carry is offered");
        let starts: Vec<usize> = batch.0.iter().map(|patch| patch.range.start).collect();
        assert!(starts.is_sorted(), "the patches are sorted: {starts:?}");
        doc.apply(Command::Edit(batch)).unwrap();
        assert_eq!(doc.resolutions()[0], Resolution::Edited);
        assert_eq!(doc.resolutions()[2], Resolution::Edited);
        assert_eq!(doc.output(), Output::Resolved("a\nb\nc\nm1\nM2\n".to_owned()));
        doc.apply(Command::Undo).unwrap();
        assert_eq!(doc.resolutions()[0], Resolution::Unresolved, "one undo reverts both");
        assert_ne!(doc.resolutions()[2], Resolution::Edited);
    }

    // Moved blocks

    /// Each region's suggestions, as `suggested` gives them, but only the carried edits.
    fn carried(base: &str, left: &str, right: &str) -> Vec<Vec<Suggestion>> {
        suggested(base, left, right)
            .into_iter()
            .map(|region| {
                region.into_iter().filter(|s| matches!(s, Suggestion::CarryEdit { .. })).collect()
            })
            .collect()
    }

    fn carry(side: usize, to: usize, text: &str) -> Suggestion {
        Suggestion::CarryEdit { side, to: RegionIx::new(to), text: text.to_owned() }
    }

    /// Base's block `m1 m2` sits before `a b c`, which outweighs it in every diff.
    const MOVE_BASE: &str = "m1\nm2\na\nb\nc\n";

    #[test]
    fn a_block_left_moves_and_right_edits_suggests_carrying_the_edit() {
        // Regions: the conflict over `m1 m2`, `a b c`, left's insert of `m1 m2`.
        assert_eq!(
            carried(MOVE_BASE, "a\nb\nc\nm1\nm2\n", "m1\nM2\na\nb\nc\n"),
            [vec![carry(1, 2, "m1\nM2\n")], NONE, NONE]
        );
    }

    #[test]
    fn a_block_right_moves_and_left_edits_suggests_carrying_the_edit() {
        assert_eq!(
            carried(MOVE_BASE, "M1\nm2\na\nb\nc\n", "a\nb\nc\nm1\nm2\n"),
            [vec![carry(0, 2, "M1\nm2\n")], NONE, NONE]
        );
    }

    #[test]
    fn a_block_moved_up_carries_the_edit_to_an_earlier_region() {
        // Regions: left's insert of `m1 m2`, `a b c`, the conflict over `m1 m2`.
        assert_eq!(
            carried("a\nb\nc\nm1\nm2\n", "m1\nm2\na\nb\nc\n", "a\nb\nc\nm1\nX\n"),
            [NONE, NONE, vec![carry(1, 0, "m1\nX\n")]]
        );
    }

    #[test]
    fn a_block_moved_among_new_lines_keeps_them_around_the_edit() {
        assert_eq!(
            carried(MOVE_BASE, "a\nb\nc\nn\nm1\nm2\no\n", "m1\nM2\na\nb\nc\n"),
            [vec![carry(1, 2, "n\nm1\nM2\no\n")], NONE, NONE]
        );
    }

    #[test]
    fn a_deleted_block_found_nowhere_else_suggests_nothing() {
        assert_eq!(carried(MOVE_BASE, "a\nb\nc\n", "m1\nM2\na\nb\nc\n"), [NONE, NONE]);
    }

    #[test]
    fn a_block_changed_on_the_way_suggests_nothing() {
        assert_eq!(
            carried(MOVE_BASE, "a\nb\nc\nm1\nm2!\n", "m1\nM2\na\nb\nc\n"),
            [NONE, NONE, NONE]
        );
    }

    #[test]
    fn a_block_inserted_twice_suggests_nothing() {
        assert_eq!(
            carried(MOVE_BASE, "a\nb\nc\nm1\nm2\nm1\nm2\n", "m1\nM2\na\nb\nc\n"),
            [NONE, NONE, NONE]
        );
    }

    #[test]
    fn a_moved_block_of_blank_lines_suggests_nothing() {
        assert_eq!(
            carried("\n \na\nb\nc\n", "a\nb\nc\n\n \n", "x\n \na\nb\nc\n"),
            [NONE, NONE, NONE]
        );
    }

    #[test]
    fn a_move_among_three_sides_suggests_nothing() {
        let base = text(MOVE_BASE);
        let inputs = Inputs::new(
            vec![text("a\nb\nc\nm1\nm2\n"), text("m1\nM2\na\nb\nc\n"), text(MOVE_BASE)],
            vec![base.clone(), base],
        )
        .unwrap();
        let regions = align(&inputs, MergeOptions::default().alignment());
        assert_eq!(regions[0].kind, Kind::Conflict);
        assert_eq!(suggestions(&inputs, &regions, RegionIx::new(0)), NONE);
    }

    #[test]
    fn carrying_the_edit_moves_it_in_one_undoable_step() {
        use crate::document::{
            Command, EditBatch, Labels, MergeDocument, Output, Resolution, TextPatch,
        };
        let inputs = Inputs::three_way(
            text(MOVE_BASE),
            text("a\nb\nc\nm1\nm2\n"),
            text("m1\nM2\na\nb\nc\n"),
        );
        let mut doc = MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).unwrap();
        let Some(Suggestion::CarryEdit { to, text, .. }) =
            suggestions(doc.inputs(), doc.regions(), RegionIx::new(0)).pop()
        else {
            panic!("the conflict offers to carry right's edit");
        };
        let patches = vec![
            TextPatch { range: doc.region_bytes(RegionIx::new(0)), text: String::new() },
            TextPatch { range: doc.region_bytes(to), text },
        ];
        doc.apply(Command::Edit(EditBatch(patches))).unwrap();
        assert_eq!(doc.output(), Output::Resolved("a\nb\nc\nm1\nM2\n".to_owned()));
        assert_eq!(doc.resolutions(), [Resolution::Edited, Resolution::Auto, Resolution::Edited]);
        doc.apply(Command::Undo).unwrap();
        assert_eq!(doc.resolutions(), [Resolution::Unresolved, Resolution::Auto, Resolution::Auto]);
    }
}
