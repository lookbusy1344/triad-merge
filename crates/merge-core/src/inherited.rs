//! The conflicts a commit inherits from a parent (spec §7.5.1): a conflict region whose every
//! side and base holds the text one of the parent's conflict regions holds, when the parent's
//! conflict at the path is aligned the same way. The merge window draws them in the inherited
//! tone.

use std::collections::HashSet;

use crate::document::MergeDocument;
use crate::index::RegionIx;
use crate::merge3::{self, Inputs, Kind, Region};

/// The inherited conflict regions of one document, in region order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inherited(Vec<RegionIx>);

impl Inherited {
    /// No inherited conflicts: a merge outside a commit.
    pub const NONE: Self = Self(Vec::new());

    /// The conflict regions of `doc` that one of `parents` holds: each parent is the
    /// conflict a parent commit holds at the same path, aligned with `doc`'s options. A
    /// parent of another number of sides or bases holds none of them.
    #[must_use]
    pub fn find(doc: &MergeDocument, parents: &[Inputs]) -> Self {
        let inputs = doc.inputs();
        let alignment = doc.options().alignment();
        let held: HashSet<Vec<&str>> = parents
            .iter()
            .filter(|parent| {
                parent.sides().len() == inputs.sides().len()
                    && parent.bases().len() == inputs.bases().len()
            })
            .flat_map(|parent| {
                merge3::align(parent, &alignment)
                    .into_iter()
                    .filter(|region| region.kind == Kind::Conflict)
                    .map(move |region| key(parent, &region))
            })
            .collect();
        if held.is_empty() {
            return Self::default();
        }
        Self(
            doc.region_ixs()
                .filter(|&ix| {
                    let region = doc.region(ix);
                    region.kind == Kind::Conflict && held.contains(&key(inputs, region))
                })
                .collect(),
        )
    }

    #[must_use]
    pub fn contains(&self, region: RegionIx) -> bool {
        self.0.binary_search(&region).is_ok()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What `region` of `inputs` holds: each base's text, then each side's.
pub(crate) fn key<'a>(inputs: &'a Inputs, region: &Region) -> Vec<&'a str> {
    let bases = inputs.bases().iter().zip(&region.bases);
    let sides = inputs.sides().iter().zip(&region.sides);
    bases.chain(sides).map(|(text, lines)| text.span(lines.clone())).collect()
}

#[cfg(test)]
mod tests {
    use crate::document::{Labels, MergeOptions};
    use crate::merge3::Inputs;
    use crate::testing::{text, three};

    use super::*;

    fn doc(inputs: Inputs) -> MergeDocument {
        MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels")
    }

    /// Two conflicts: line 2 (b against X and Y) and line 6 (f against P and Q).
    fn two_conflicts() -> Inputs {
        three("a\nb\nc\nd\ne\nf\ng\n", "a\nX\nc\nd\ne\nP\ng\n", "a\nY\nc\nd\ne\nQ\ng\n")
    }

    fn conflicts(doc: &MergeDocument) -> Vec<RegionIx> {
        doc.region_ixs().filter(|&r| doc.region(r).kind == Kind::Conflict).collect()
    }

    fn found(doc: &MergeDocument, parents: &[Inputs]) -> Vec<RegionIx> {
        let inherited = Inherited::find(doc, parents);
        doc.region_ixs().filter(|&r| inherited.contains(r)).collect()
    }

    #[test]
    fn a_parent_with_the_same_conflict_makes_every_conflict_inherited() {
        let d = doc(two_conflicts());
        assert_eq!(conflicts(&d).len(), 2);
        assert_eq!(found(&d, &[two_conflicts()]), conflicts(&d));
    }

    #[test]
    fn a_conflict_the_parent_does_not_hold_is_the_commit_s_own() {
        // The parent conflicts at line 2 alone; the commit added the conflict at line 6.
        let parent =
            three("a\nb\nc\nd\ne\nf\ng\n", "a\nX\nc\nd\ne\nf\ng\n", "a\nY\nc\nd\ne\nf\ng\n");
        let d = doc(two_conflicts());
        assert_eq!(found(&d, &[parent]), conflicts(&d)[..1]);
    }

    #[test]
    fn a_conflict_whose_side_changed_is_the_commit_s_own() {
        // The commit edited left's text in the conflict at line 6.
        let parent =
            three("a\nb\nc\nd\ne\nf\ng\n", "a\nX\nc\nd\ne\nP0\ng\n", "a\nY\nc\nd\ne\nQ\ng\n");
        let d = doc(two_conflicts());
        assert_eq!(found(&d, &[parent]), conflicts(&d)[..1]);
    }

    #[test]
    fn a_conflict_moved_by_lines_the_commit_added_above_it_stays_inherited() {
        let parent = two_conflicts();
        let child = three(
            "new\na\nb\nc\nd\ne\nf\ng\n",
            "new\na\nX\nc\nd\ne\nP\ng\n",
            "new\na\nY\nc\nd\ne\nQ\ng\n",
        );
        let d = doc(child);
        assert_eq!(found(&d, &[parent]), conflicts(&d));
    }

    #[test]
    fn a_parent_of_other_arity_holds_none() {
        let parent = Inputs::new(
            vec![text("a\nX\nc\nd\ne\nP\ng\n"), text("a\nY\nc\nd\ne\nQ\ng\n"), text("z\n")],
            vec![text("a\nb\nc\nd\ne\nf\ng\n"), text("y\n")],
        )
        .expect("three sides over two bases");
        assert_eq!(found(&doc(two_conflicts()), &[parent]), []);
    }

    #[test]
    fn of_two_parents_either_may_hold_a_conflict() {
        let first =
            three("a\nb\nc\nd\ne\nf\ng\n", "a\nX\nc\nd\ne\nf\ng\n", "a\nY\nc\nd\ne\nf\ng\n");
        let second =
            three("a\nb\nc\nd\ne\nf\ng\n", "a\nb\nc\nd\ne\nP\ng\n", "a\nb\nc\nd\ne\nQ\ng\n");
        let d = doc(two_conflicts());
        assert_eq!(found(&d, &[first, second]), conflicts(&d));
    }

    #[test]
    fn no_parents_and_changes_without_conflict_hold_none() {
        let d = doc(two_conflicts());
        assert_eq!(found(&d, &[]), []);
        // The same text as a one-side change in the parent is no conflict there.
        let parent =
            three("a\nb\nc\nd\ne\nf\ng\n", "a\nX\nc\nd\ne\nP\ng\n", "a\nb\nc\nd\ne\nf\ng\n");
        assert_eq!(found(&d, &[parent]), []);
        assert!(Inherited::find(&d, &[]).is_empty());
    }
}
