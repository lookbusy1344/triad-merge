//! Where a cut through a region's text falls between syntax nodes
//! (`docs/PLAN.md` §B5.8). `merge-core` knows no grammar: a caller that has one
//! gives [`Boundaries`], and deep refinement and the word merge drop the cuts it marks.

use std::fmt;
use std::ops::Range;

use crate::text::Text;

/// The byte offsets of a text where a cut lies inside a syntax node: every offset strictly
/// inside one of its ranges. Offsets count from the start of the whole text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cuts {
    /// Sorted, and no two overlap; two may touch, and a cut where they touch is clean.
    inside: Vec<Range<usize>>,
}

impl Cuts {
    /// Every cut is clean: no grammar, or a parse with an error in the region.
    #[must_use]
    pub const fn all_clean() -> Self {
        Self { inside: Vec::new() }
    }

    /// The cuts inside any of `nodes`, in any order.
    #[must_use]
    pub fn inside(mut nodes: Vec<Range<usize>>) -> Self {
        nodes.retain(|node| node.end - node.start > 1);
        nodes.sort_unstable_by_key(|node| node.start);
        let mut inside: Vec<Range<usize>> = Vec::with_capacity(nodes.len());
        for node in nodes {
            match inside.last_mut() {
                Some(last) if node.start < last.end => last.end = last.end.max(node.end),
                _ => inside.push(node),
            }
        }
        Self { inside }
    }

    /// A cut at `offset` lies inside no node.
    #[must_use]
    pub fn clean(&self, offset: usize) -> bool {
        let at = self.inside.partition_point(|node| node.end <= offset);
        self.inside.get(at).is_none_or(|node| node.start >= offset)
    }
}

/// A grammar's view of where a region may be cut.
pub trait Boundaries: Sync + fmt::Debug {
    /// The cuts of `text`'s lines `lines` that lie inside a syntax node within them: a node
    /// that crosses the cut and does not hold all of the lines' text, without the whitespace
    /// around it. For deep refinement's splits.
    fn node_cuts(&self, text: &Text, lines: Range<usize>) -> Cuts;

    /// The cuts of `text`'s lines `lines` that lie inside one token: a leaf of the tree, such
    /// as a string's content, a comment or `->`. For the word merge's hunks.
    fn token_cuts(&self, text: &Text, lines: Range<usize>) -> Cuts;
}

/// Hand-marked syntax nodes for tests: each text's node byte ranges, by its content. Node
/// and token cuts are the same.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct Marked(pub(crate) Vec<(&'static str, Vec<Range<usize>>)>);

#[cfg(test)]
impl Boundaries for Marked {
    fn node_cuts(&self, text: &Text, _: Range<usize>) -> Cuts {
        self.0
            .iter()
            .find(|(content, _)| *content == text.content())
            .map_or_else(Cuts::all_clean, |(_, nodes)| Cuts::inside(nodes.clone()))
    }

    fn token_cuts(&self, text: &Text, lines: Range<usize>) -> Cuts {
        self.node_cuts(text, lines)
    }
}

/// One node's byte range, for [`Marked`].
#[cfg(test)]
pub(crate) fn node(range: Range<usize>) -> Vec<Range<usize>> {
    vec![range]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_strictly_inside_a_node_is_not_clean() {
        let node = 4..8;
        let cuts = Cuts::inside(vec![node]);
        assert_eq!(
            (0..=10).map(|at| cuts.clean(at)).collect::<Vec<_>>(),
            [true, true, true, true, true, false, false, false, true, true, true]
        );
    }

    #[test]
    fn overlapping_nodes_join_and_touching_ones_keep_their_seam() {
        let cuts = Cuts::inside(vec![10..14, 2..6, 4..9, 14..16]);
        assert_eq!(cuts.inside, [2..9, 10..14, 14..16]);
        let unclean: Vec<usize> = (0..20).filter(|&at| !cuts.clean(at)).collect();
        assert_eq!(unclean, [3, 4, 5, 6, 7, 8, 11, 12, 13, 15]);
    }

    #[test]
    fn a_node_of_one_byte_or_none_has_no_inside() {
        let cuts = Cuts::inside(vec![3..4, 6..6]);
        assert!((0..10).all(|at| cuts.clean(at)));
        assert_eq!(cuts, Cuts::all_clean());
    }

    #[test]
    fn a_nested_node_adds_nothing() {
        let outer = 0..10;
        assert_eq!(Cuts::inside(vec![outer.clone(), 2..5]).inside, [outer]);
    }
}
