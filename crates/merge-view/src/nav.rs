//! Navigation between regions (spec §5). Navigation stops at the first and last target: it
//! never wraps around. A scan visits the regions between the start and the target, and
//! differences and unchanged regions alternate, so a step to the next difference is short.

use merge_core::document::{MergeDocument, Resolution};
use merge_core::index::{ByteOffset, RegionIx};
use merge_core::merge3::{Kind, simplified_sides};

/// The region navigation or a pick last made current, and the caret it left. While the caret
/// stays there, that region is current: an empty region holds no byte, so the caret alone
/// cannot name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub region: RegionIx,
    pub caret: ByteOffset,
}

/// What navigation moves between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A region some input changed. Whitespace-only changes stay `Unchanged` and are not
    /// differences (spec §4.2).
    Difference,
    /// A conflict still showing its markers.
    Unresolved,
}

impl Target {
    fn matches(self, doc: &MergeDocument, region: RegionIx) -> bool {
        match self {
            Target::Difference => doc.region(region).kind != Kind::Unchanged,
            Target::Unresolved => *doc.resolution(region) == Resolution::Unresolved,
        }
    }
}

/// The first target after region `from`.
#[must_use]
pub fn next(doc: &MergeDocument, from: RegionIx, target: Target) -> Option<RegionIx> {
    doc.region_ixs().skip(from.next().get()).find(|&r| target.matches(doc, r))
}

/// The last target before region `from`.
#[must_use]
pub fn previous(doc: &MergeDocument, from: RegionIx, target: Target) -> Option<RegionIx> {
    doc.region_ixs().take(from.get()).rev().find(|&r| target.matches(doc, r))
}

#[must_use]
pub fn first(doc: &MergeDocument, target: Target) -> Option<RegionIx> {
    doc.region_ixs().find(|&r| target.matches(doc, r))
}

#[must_use]
pub fn last(doc: &MergeDocument, target: Target) -> Option<RegionIx> {
    doc.region_ixs().rev().find(|&r| target.matches(doc, r))
}

/// The difference at or before the middle region, or the first after it; the middle region
/// in a merge without differences.
#[must_use]
pub fn middle_difference(doc: &MergeDocument) -> RegionIx {
    let middle = RegionIx::new(doc.regions().len() / 2);
    previous(doc, middle.next(), Target::Difference)
        .or_else(|| next(doc, middle, Target::Difference))
        .unwrap_or(middle)
}

/// The region the result cursor at byte `cursor` sits in: at a boundary, the region that
/// starts there; at or past the end of the text, the last region.
#[must_use]
pub fn current_region(doc: &MergeDocument, cursor: ByteOffset) -> RegionIx {
    doc.region_at(cursor).unwrap_or_else(|| RegionIx::new(doc.regions().len() - 1))
}

/// The current region for a caret at `caret`: the pinned one while the caret stays on the pin
/// and the document has that region, else the one holding the caret. Always a region of `doc`.
#[must_use]
pub fn held_region(doc: &MergeDocument, pin: Option<Pin>, caret: ByteOffset) -> RegionIx {
    match pin {
        Some(pin) if pin.caret == caret && pin.region.get() < doc.regions().len() => pin.region,
        _ => current_region(doc, caret),
    }
}

/// Which conflict `region` is, counting from 1 in file order, or `None` if it is not a
/// conflict or past the last region. A resolved conflict keeps its number.
#[must_use]
pub fn conflict_number(doc: &MergeDocument, region: RegionIx) -> Option<usize> {
    let regions = doc.regions();
    let region = region.get();
    (regions.get(region)?.kind == Kind::Conflict)
        .then(|| regions[..=region].iter().filter(|r| r.kind == Kind::Conflict).count())
}

/// How many sides a conflict keeps after jj's simplification, out of the merge's sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sides {
    pub kept: usize,
    pub of: usize,
}

/// The sides conflict `region` keeps once jj drops each pair of a side and a base that hold
/// the same lines there (spec §7.5.2). `None` in a three-way merge, where every conflict has
/// two, and for a region that is not a conflict or is past the last.
#[must_use]
pub fn conflict_sides(doc: &MergeDocument, region: RegionIx) -> Option<Sides> {
    let of = doc.inputs().sides().len();
    let region = doc.regions().get(region.get())?;
    (of > THREE_WAY_SIDES && region.kind == Kind::Conflict)
        .then(|| Sides { kept: simplified_sides(doc.inputs(), region, doc.options().diff), of })
}

/// The sides of a three-way merge.
const THREE_WAY_SIDES: usize = 2;

#[cfg(test)]
mod tests {
    use merge_core::document::Labels;
    use merge_core::document::{Command, EditBatch, MergeOptions, TextPatch};
    use merge_core::merge3::{Inputs, Source};
    use merge_core::testing::{doc, doc_with, text};

    use super::*;

    /// Regions: 0 a, 1 B (left only), 2 c, 3 D (right only), 4 e, 5 f (conflict), 6 g.
    fn mixed() -> MergeDocument {
        doc("a\nb\nc\nd\ne\nf\ng\n", "a\nB\nc\nd\ne\nF\ng\n", "a\nb\nc\nD\ne\nX\ng\n")
    }

    /// Regions: 0 a, 1 conflict, 2 c, 3 D (right only), 4 e, 5 conflict, 6 g.
    fn two_conflicts() -> MergeDocument {
        doc("a\nb\nc\nd\ne\nf\ng\n", "a\nL\nc\nd\ne\nF\ng\n", "a\nR\nc\nD\ne\nX\ng\n")
    }

    #[test]
    fn conflicts_are_numbered_in_file_order() {
        let d = two_conflicts();
        let numbers: Vec<_> = (0..7).map(|r| conflict_number(&d, RegionIx::new(r))).collect();
        assert_eq!(numbers, [None, Some(1), None, None, None, Some(2), None]);
    }

    #[test]
    fn a_resolved_conflict_keeps_its_number() {
        let mut d = two_conflicts();
        d.apply(Command::Pick { region: RegionIx::new(1), sources: vec![Source::Side(0)] })
            .expect("pick");
        assert_eq!(
            (conflict_number(&d, RegionIx::new(1)), conflict_number(&d, RegionIx::new(5))),
            (Some(1), Some(2))
        );
    }

    #[test]
    fn differences_step_forward_and_back_without_wrapping() {
        let d = mixed();
        let steps: Vec<_> = [0, 1, 3, 5, 6]
            .map(|from| next(&d, RegionIx::new(from), Target::Difference).map(RegionIx::get))
            .into();
        assert_eq!(steps, [Some(1), Some(3), Some(5), None, None]);
        let back: Vec<_> = [6, 5, 3, 1, 0]
            .map(|from| previous(&d, RegionIx::new(from), Target::Difference).map(RegionIx::get))
            .into();
        assert_eq!(back, [Some(5), Some(3), Some(1), None, None]);
        assert_eq!(
            (first(&d, Target::Difference), last(&d, Target::Difference)),
            (Some(RegionIx::new(1)), Some(RegionIx::new(5)))
        );
    }

    #[test]
    fn unresolved_conflicts_skip_other_differences_and_leave_once_resolved() {
        let mut d = mixed();
        assert_eq!(next(&d, RegionIx::new(0), Target::Unresolved), Some(RegionIx::new(5)));
        assert_eq!(previous(&d, RegionIx::new(6), Target::Unresolved), Some(RegionIx::new(5)));
        assert_eq!(previous(&d, RegionIx::new(5), Target::Unresolved), None);
        d.apply(Command::Toggle { region: RegionIx::new(5), source: Source::Side(0) })
            .expect("pick left");
        assert_eq!((first(&d, Target::Unresolved), last(&d, Target::Unresolved)), (None, None));
        assert_eq!(next(&d, RegionIx::new(3), Target::Difference), Some(RegionIx::new(5)));
    }

    #[test]
    fn a_difference_in_the_last_region_is_the_last() {
        let d = doc("a\nb\n", "a\nB\n", "a\nb\n");
        assert_eq!(last(&d, Target::Difference), Some(RegionIx::new(1)));
    }

    #[test]
    fn a_document_without_differences_has_nowhere_to_go() {
        let d = doc("a\nb\n", "a\nb\n", "a\nb\n");
        assert_eq!(first(&d, Target::Difference), None);
        assert_eq!(last(&d, Target::Difference), None);
        assert_eq!(next(&d, RegionIx::new(0), Target::Difference), None);
        assert_eq!(previous(&d, RegionIx::new(0), Target::Difference), None);
    }

    #[test]
    fn a_whitespace_only_change_is_not_a_difference() {
        let options = MergeOptions {
            diff: merge_core::diff::DiffOptions {
                whitespace: merge_core::diff::Whitespace::Trailing,
                ..Default::default()
            },
            ..MergeOptions::default()
        };
        let d = doc_with("a\nb\n", "a \nb\n", "a\nb\n", options);
        assert_eq!(first(&d, Target::Difference), None);
    }

    #[test]
    fn the_current_region_follows_the_cursor() {
        // Result: "a\n" 0..2, "B\n" 2..4, "c\n" 4..6, "D\n" 6..8, ...
        let d = mixed();
        let regions: Vec<_> =
            [0, 1, 2, 3, 7, 8].map(|at| current_region(&d, ByteOffset::new(at)).get()).into();
        assert_eq!(regions, [0, 0, 1, 1, 3, 4]);
        let end = d.text().len();
        assert_eq!(current_region(&d, ByteOffset::new(end)), RegionIx::new(6));
    }

    #[test]
    fn a_caret_past_the_end_is_in_the_last_region() {
        let d = mixed();
        let past = ByteOffset::new(d.text().len() + 1);
        assert_eq!(current_region(&d, past), RegionIx::new(6));
    }

    #[test]
    fn a_pin_holds_only_while_the_caret_stays_on_it() {
        // Byte 2 starts region 1; byte 3 is inside it.
        let d = mixed();
        let pin = Pin { region: RegionIx::new(0), caret: ByteOffset::new(2) };
        assert_eq!(held_region(&d, Some(pin), ByteOffset::new(2)), RegionIx::new(0));
        assert_eq!(held_region(&d, Some(pin), ByteOffset::new(3)), RegionIx::new(1));
        assert_eq!(held_region(&d, None, ByteOffset::new(2)), RegionIx::new(1));
    }

    #[test]
    fn a_pin_on_the_last_region_holds_and_one_past_it_is_dropped() {
        // Seven regions, so 6 is the last. The caret alone says region 1.
        let d = mixed();
        let caret = ByteOffset::new(2);
        let pinned =
            |region| held_region(&d, Some(Pin { region: RegionIx::new(region), caret }), caret);
        assert_eq!(pinned(6), RegionIx::new(6));
        assert_eq!(pinned(7), RegionIx::new(1));
    }

    #[test]
    fn a_stale_pin_and_a_stale_caret_give_the_last_region() {
        let d = mixed();
        let caret = ByteOffset::new(d.text().len() + 1);
        let pin = Pin { region: RegionIx::new(99), caret };
        assert_eq!(held_region(&d, Some(pin), caret), RegionIx::new(6));
    }

    /// A merge of three sides in jj's order s0 b0 s1 b1 s2. Regions: 0 a, 1 conflict, 2 c.
    fn three_sides(s0: &str, b0: &str, s1: &str, b1: &str, s2: &str) -> MergeDocument {
        let inputs = Inputs::new(vec![text(s0), text(s1), text(s2)], vec![text(b0), text(b1)])
            .expect("three sides, two bases");
        MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels")
    }

    #[test]
    fn a_conflict_of_three_sides_counts_the_sides_jj_keeps() {
        // s1 = b0 = b cancel: X and Y are left over b.
        let abc = "a\nb\nc\n";
        let d = three_sides("a\nX\nc\n", abc, abc, abc, "a\nY\nc\n");
        assert_eq!(d.regions()[1].kind, Kind::Conflict);
        assert_eq!(conflict_sides(&d, RegionIx::new(1)), Some(Sides { kept: 2, of: 3 }));
    }

    #[test]
    fn a_conflict_whose_sides_all_changed_keeps_every_side() {
        let d = three_sides("a\nX\nc\n", "a\nb\nc\n", "a\nY\nc\n", "a\nd\nc\n", "a\nZ\nc\n");
        assert_eq!(conflict_sides(&d, RegionIx::new(1)), Some(Sides { kept: 3, of: 3 }));
    }

    #[test]
    fn only_a_conflict_of_more_than_two_sides_counts_its_sides() {
        let abc = "a\nb\nc\n";
        let d = three_sides("a\nX\nc\n", abc, abc, abc, "a\nY\nc\n");
        assert_eq!(conflict_sides(&d, RegionIx::new(0)), None);
        assert_eq!(conflict_sides(&d, RegionIx::new(3)), None);
        let d = two_conflicts();
        assert_eq!(conflict_sides(&d, RegionIx::new(1)), None);
    }

    #[test]
    fn a_region_past_the_last_has_no_conflict_number() {
        let d = two_conflicts();
        assert_eq!(conflict_number(&d, RegionIx::new(5)), Some(2));
        assert_eq!(conflict_number(&d, RegionIx::new(7)), None);
    }

    #[test]
    fn a_merging_edit_moves_the_current_region_and_targets() {
        // Deleting "\nc\nD" joins B, c and D: regions 0 a, 1 B..D, 2 e, 3 conflict, 4 g.
        let mut d = mixed();
        d.apply(Command::Edit(EditBatch(vec![TextPatch { range: 3..7, text: String::new() }])))
            .expect("edit");
        assert_eq!(d.regions().len(), 5);
        assert_eq!(current_region(&d, ByteOffset::new(3)), RegionIx::new(1));
        assert_eq!(next(&d, RegionIx::new(1), Target::Difference), Some(RegionIx::new(3)));
        assert_eq!(next(&d, RegionIx::new(0), Target::Unresolved), Some(RegionIx::new(3)));
    }

    #[test]
    fn the_middle_difference_is_the_middle_region_when_it_differs() {
        // Seven regions; the middle, 3, is D.
        assert_eq!(middle_difference(&mixed()), RegionIx::new(3));
    }

    #[test]
    fn the_middle_difference_is_the_last_one_before_an_unchanged_middle() {
        // Regions: 0 a, 1 B, 2 c, 3 D. The middle, 2, is unchanged; 1 comes before it.
        let d = doc("a\nb\nc\nd\n", "a\nB\nc\nD\n", "a\nb\nc\nd\n");
        assert_eq!(d.regions().len(), 4);
        assert_eq!(middle_difference(&d), RegionIx::new(1));
        // Regions: 0 A, 1 b, 2 C. The middle, 1, is unchanged; 0 comes before it.
        let d = doc("a\nb\nc\n", "A\nb\nC\n", "a\nb\nc\n");
        assert_eq!(d.regions().len(), 3);
        assert_eq!(middle_difference(&d), RegionIx::new(0));
    }

    #[test]
    fn without_differences_the_middle_difference_is_the_middle_region() {
        let d = doc("a\nb\n", "a\nb\n", "a\nb\n");
        assert_eq!(d.regions().len(), 1);
        assert_eq!(middle_difference(&d), RegionIx::new(0));
    }
}
