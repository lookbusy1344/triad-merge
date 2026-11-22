//! Earlier resolutions of a conflict (spec §7.5.5): an earlier version of the conflict and
//! the file that resolved it give each of the earlier conflict's regions the lines the file
//! holds between the region's neighbours. A conflict region of the current merge whose every
//! side and base holds the text of such a region is offered that resolution.

use std::collections::HashMap;
use std::ops::Range;

use merge_core::diff::{DiffOptions, Hunk, diff_lines};
use merge_core::document::{Labels, MergeDocument, MergeOptions};
use merge_core::index::RegionIx;
use merge_core::merge3::{Inputs, Kind};
use merge_core::text::Text;

use crate::inherited::key;

/// One earlier resolution of a region: its text, and the index of the pair it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub text: String,
    pub pair: usize,
}

/// The conflict regions of one document with earlier resolutions, in region order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Earlier(Vec<(RegionIx, Vec<Offer>)>);

impl Earlier {
    /// No earlier resolutions: a merge outside a jj repository.
    pub const NONE: Self = Self(Vec::new());

    /// The earlier resolutions of `doc`'s conflict regions from `pairs`, newest pair first:
    /// each an earlier version of the conflict and the file the version that resolved it
    /// held.
    /// Each pair's conflict merges with `doc`'s options; a pair of another number of sides
    /// or bases offers nothing, since its keys have another length. A region's offers keep the order of `pairs`, each text once,
    /// under the newest pair that gives it.
    #[must_use]
    pub fn find<'p>(
        doc: &MergeDocument,
        pairs: impl IntoIterator<Item = (&'p Inputs, &'p Text)>,
    ) -> Self {
        let inputs = doc.inputs();
        let mut held: HashMap<Vec<String>, Vec<Offer>> = HashMap::new();
        for (ix, (conflict, resolved)) in pairs.into_iter().enumerate() {
            for (region_key, text) in resolutions(conflict, resolved, doc.options()) {
                let offers = held.entry(region_key).or_default();
                if offers.iter().all(|offer| offer.text != text) {
                    offers.push(Offer { text, pair: ix });
                }
            }
        }
        if held.is_empty() {
            return Self::default();
        }
        Self(
            doc.region_ixs()
                .filter(|&ix| doc.region(ix).kind == Kind::Conflict)
                .filter_map(|ix| {
                    let region_key: Vec<String> =
                        key(inputs, doc.region(ix)).into_iter().map(str::to_owned).collect();
                    held.get(&region_key).map(|offers| (ix, offers.clone()))
                })
                .collect(),
        )
    }

    /// The earlier resolutions of `region`, newest first; none when it has none.
    #[must_use]
    pub fn offers(&self, region: RegionIx) -> &[Offer] {
        self.0
            .binary_search_by_key(&region, |(ix, _)| *ix)
            .map_or(&[], |found| self.0[found].1.as_slice())
    }

    /// The regions with earlier resolutions, in order.
    pub fn regions(&self) -> impl Iterator<Item = RegionIx> + '_ {
        self.0.iter().map(|(ix, _)| *ix)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Each conflict region of `conflict`, merged with `options`, keyed by its text, and the
/// lines `resolved` holds between the region's neighbours. The merge's result pane
/// shows an unresolved region as its sides' lines; a line diff of that text against the
/// resolved file finds the neighbours. A region whose neighbour the diff changes has none.
fn resolutions(
    conflict: &Inputs,
    resolved: &Text,
    options: &MergeOptions,
) -> Vec<(Vec<String>, String)> {
    #[expect(clippy::expect_used, reason = "invariant: a merge without labels takes any shape")]
    let earlier = MergeDocument::new(conflict.clone(), options.clone(), Labels::none())
        .expect("a merge without labels takes any shape");
    let shown = Text::from_utf8(earlier.text());
    let hunks = diff_lines(&shown, resolved, &DiffOptions::default());
    let resolved_lines = resolved.line_count();
    earlier
        .region_ixs()
        .filter(|&ix| earlier.region(ix).kind == Kind::Conflict)
        .filter_map(|ix| {
            let lines = earlier.region_lines(ix);
            let Range { start, end } = lines.start.get()..lines.end.get();
            let from = match start.checked_sub(1) {
                Some(before) => image(&hunks, before)? + 1,
                None => 0,
            };
            let to = if end < shown.line_count() { image(&hunks, end)? } else { resolved_lines };
            let region_key = key(earlier.inputs(), earlier.region(ix));
            Some((
                region_key.into_iter().map(str::to_owned).collect(),
                resolved.span(from..to).to_owned(),
            ))
        })
        .collect()
}

/// The line of the diff's `after` text that line `line` of its `before` text stays as; none
/// when a hunk replaces or removes it.
fn image(hunks: &[Hunk], line: usize) -> Option<usize> {
    let mut shift: isize = 0;
    for hunk in hunks {
        if hunk.before.contains(&line) {
            return None;
        }
        // A hunk that starts past the line leaves it; an insertion right before it moves it.
        if hunk.before.start > line {
            break;
        }
        shift += hunk.after.len().cast_signed() - hunk.before.len().cast_signed();
    }
    line.checked_add_signed(shift)
}

#[cfg(test)]
mod tests {
    use merge_core::testing::{text, three};

    use super::*;

    fn doc(inputs: Inputs) -> MergeDocument {
        MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels")
    }

    fn pair(conflict: Inputs, resolved: &str) -> (Inputs, Text) {
        (conflict, text(resolved))
    }

    /// Each region with offers, and the offers' texts and pairs.
    fn offered(
        doc: &MergeDocument,
        pairs: &[(Inputs, Text)],
    ) -> Vec<(usize, Vec<(String, usize)>)> {
        let earlier =
            Earlier::find(doc, pairs.iter().map(|(conflict, resolved)| (conflict, resolved)));
        earlier
            .regions()
            .map(|ix| {
                let offers = earlier.offers(ix).iter().map(|o| (o.text.clone(), o.pair)).collect();
                (ix.get(), offers)
            })
            .collect()
    }

    fn middle() -> Inputs {
        three("a\nb\nc\n", "a\nX\nc\n", "a\nY\nc\n")
    }

    #[test]
    fn a_conflict_resolved_before_is_offered_the_lines_that_replaced_it() {
        let offers = offered(&doc(middle()), &[pair(middle(), "a\nXY\nZ\nc\n")]);
        assert_eq!(offers, [(1, vec![("XY\nZ\n".to_owned(), 0)])]);
    }

    #[test]
    fn a_resolution_that_deletes_the_conflict_offers_no_lines() {
        let offers = offered(&doc(middle()), &[pair(middle(), "a\nc\n")]);
        assert_eq!(offers, [(1, vec![(String::new(), 0)])]);
    }

    #[test]
    fn a_resolution_that_keeps_both_sides_and_adds_lines_offers_all_of_them() {
        let offers = offered(&doc(middle()), &[pair(middle(), "a\nX\nY\nnew\nc\n")]);
        assert_eq!(offers, [(1, vec![("X\nY\nnew\n".to_owned(), 0)])]);
    }

    #[test]
    fn a_resolution_that_keeps_one_side_offers_that_side() {
        let offers = offered(&doc(middle()), &[pair(middle(), "a\nY\nc\n")]);
        assert_eq!(offers, [(1, vec![("Y\n".to_owned(), 0)])]);
    }

    #[test]
    fn lines_added_around_the_conflict_since_leave_it_matched() {
        let now = three("top\na\nb\nc\nend\n", "top\na\nX\nc\nend\n", "top\na\nY\nc\nend\n");
        let offers = offered(&doc(now), &[pair(middle(), "a\nW\nc\n")]);
        assert_eq!(offers, [(1, vec![("W\n".to_owned(), 0)])]);
    }

    #[test]
    fn a_resolution_that_edits_a_neighbour_offers_nothing() {
        assert_eq!(offered(&doc(middle()), &[pair(middle(), "a\nW\nC\n")]), []);
        assert_eq!(offered(&doc(middle()), &[pair(middle(), "A\nW\nc\n")]), []);
    }

    #[test]
    fn a_conflict_at_the_start_or_end_of_the_file_reaches_that_end() {
        let start = three("b\nc\n", "X\nc\n", "Y\nc\n");
        assert_eq!(
            offered(&doc(start.clone()), &[pair(start, "new\nW\nc\n")]),
            [(0, vec![("new\nW\n".to_owned(), 0)])]
        );
        let end = three("a\nb\n", "a\nX\n", "a\nY\n");
        assert_eq!(
            offered(&doc(end.clone()), &[pair(end, "a\nW\nmore\n")]),
            [(1, vec![("W\nmore\n".to_owned(), 0)])]
        );
    }

    #[test]
    fn each_of_two_conflicts_takes_its_own_lines() {
        let two = three("a\nb\nc\nd\ne\n", "a\nX\nc\nd\nP\n", "a\nY\nc\nd\nQ\n");
        let offers = offered(&doc(two.clone()), &[pair(two, "a\nXY\nc\nd\nPQ\n")]);
        assert_eq!(offers, [(1, vec![("XY\n".to_owned(), 0)]), (3, vec![("PQ\n".to_owned(), 0)])]);
    }

    #[test]
    fn several_resolutions_come_newest_first_each_text_once() {
        let pairs = [
            pair(middle(), "a\nnew\nc\n"),
            pair(middle(), "a\nold\nc\n"),
            pair(middle(), "a\nnew\nc\n"),
        ];
        assert_eq!(
            offered(&doc(middle()), &pairs),
            [(1, vec![("new\n".to_owned(), 0), ("old\n".to_owned(), 1)])]
        );
    }

    #[test]
    fn a_conflict_of_other_text_or_shape_offers_nothing() {
        let other = three("a\nb\nc\n", "a\nX\nc\n", "a\nZ\nc\n");
        assert_eq!(offered(&doc(middle()), &[pair(other, "a\nW\nc\n")]), []);
        let four = Inputs::new(
            vec![text("a\nX\nc\n"), text("a\nY\nc\n"), text("a\nZ\nc\n")],
            vec![text("a\nb\nc\n"), text("a\nb\nc\n")],
        )
        .expect("three sides, two bases");
        assert_eq!(offered(&doc(middle()), &[pair(four, "a\nW\nc\n")]), []);
    }
}
