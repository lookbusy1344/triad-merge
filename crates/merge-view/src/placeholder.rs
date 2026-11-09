//! Placeholder rows in the result pane (spec §5): one hatched row where a region's input
//! lines leave no text, so a deletion or an empty pick stays visible.

use merge_core::document::{MergeDocument, Resolution};
use merge_core::index::{ByteOffset, Line, RegionIx};
use merge_core::merge3::{Kind, Source};
use smallvec::SmallVec;

use crate::rows::deleted_lines;

/// The display rows each placeholder takes in the result pane.
pub const PLACEHOLDER_ROWS: usize = 1;

/// A result-pane row that stands for input lines the result does not show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// The result line it sits above. The line count puts it after the last line.
    pub line: Line,
    pub region: RegionIx,
    /// The sides whose deletion it shows, in pick order. Empty for a region picked as nothing.
    pub sources: SmallVec<[Source; 2]>,
    /// The base lines the sides deleted, or for an empty pick the region's longest input.
    pub lines: usize,
}

/// The result pane's placeholders, in order.
///
/// A picked, unresolved or one-side region gets one where each shown side deleted base lines,
/// at that side's place among the region's blocks. Neighbouring deleting sides share one. A
/// region picked as nothing gets one when any input has lines in it. Unchanged, word-merged
/// and edited regions get none.
#[must_use]
pub fn placeholders(doc: &MergeDocument) -> Vec<Placeholder> {
    doc.region_ixs().flat_map(|region| in_region(doc, region)).collect()
}

fn in_region(doc: &MergeDocument, region: RegionIx) -> Vec<Placeholder> {
    let r = doc.region(region);
    let all_sides = || (0..r.sides.len()).map(Source::Side).collect::<Vec<_>>();
    let shown = match (doc.resolution(region), r.kind) {
        (Resolution::Edited, _) | (Resolution::Auto, Kind::Unchanged | Kind::WordMerged) => {
            return Vec::new();
        }
        (Resolution::Pick(sources), _) if sources.is_empty() => {
            let longest = r.bases.iter().chain(&r.sides).map(ExactSizeIterator::len).max();
            return longest
                .filter(|&lines| lines > 0)
                .map(|lines| Placeholder {
                    line: doc.line_of_byte(doc.region_start(region)),
                    region,
                    sources: SmallVec::new(),
                    lines,
                })
                .into_iter()
                .collect();
        }
        (Resolution::Pick(sources), _) => sources.clone(),
        (Resolution::Auto, Kind::OneSide(i)) => vec![Source::Side(i)],
        (Resolution::Auto, Kind::AllSame | Kind::Conflict) | (Resolution::Unresolved, _) => {
            all_sides()
        }
    };
    let blocks = doc.blocks(region);
    let mut at = doc.region_start(region);
    let mut found: Vec<Placeholder> = Vec::new();
    for source in shown {
        if let Some((_, bytes)) = blocks.iter().find(|(s, _)| *s == source) {
            at = ByteOffset::new(bytes.end);
        } else if let Some(lines) = deleted_lines(r, source) {
            let line = doc.line_of_byte(at);
            match found.last_mut() {
                Some(last) if last.line == line => last.sources.push(source),
                _ => found.push(Placeholder {
                    line,
                    region,
                    sources: [source].into_iter().collect(),
                    lines,
                }),
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use merge_core::document::{Command, Labels, MergeOptions};
    use merge_core::merge3::Inputs;
    use merge_core::text::Text;
    use smallvec::smallvec;

    use super::*;

    const LEFT: Source = Source::Side(merge_core::merge3::LEFT);
    const RIGHT: Source = Source::Side(merge_core::merge3::RIGHT);

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes()).expect("utf-8")
    }

    fn three(base: &str, left: &str, right: &str) -> MergeDocument {
        MergeDocument::new(
            Inputs::three_way(text(base), text(left), text(right)),
            MergeOptions::default(),
            Labels::none(),
        )
        .expect("no labels")
    }

    fn pick(doc: &mut MergeDocument, region: RegionIx, sources: Vec<Source>) {
        doc.apply(Command::Pick { region, sources }).expect("pick");
    }

    fn at(line: usize, region: RegionIx, sources: &[Source], lines: usize) -> Placeholder {
        Placeholder {
            line: Line::new(line),
            region,
            sources: sources.iter().copied().collect(),
            lines,
        }
    }

    /// Left deletes b, c; right changes them to B, C. Region 1 is the conflict.
    fn left_deletes() -> MergeDocument {
        three("a\nb\nc\nz\n", "a\nz\n", "a\nB\nC\nz\n")
    }

    #[test]
    fn an_unresolved_conflict_shows_the_deleting_side_before_the_other() {
        let doc = left_deletes();
        assert_eq!(doc.text(), "a\nB\nC\nz\n");
        assert_eq!(placeholders(&doc), [at(1, RegionIx::new(1), &[LEFT], 2)]);
    }

    #[test]
    fn a_deleting_right_side_follows_the_left_block() {
        let doc = three("a\nb\nc\nz\n", "a\nB\nC\nz\n", "a\nz\n");
        // Result a, B, C, z: right's place is after left's block, above z.
        assert_eq!(placeholders(&doc), [at(3, RegionIx::new(1), &[RIGHT], 2)]);
    }

    #[test]
    fn picking_the_deleting_side_keeps_its_placeholder() {
        let mut doc = left_deletes();
        pick(&mut doc, RegionIx::new(1), vec![LEFT]);
        assert_eq!(doc.text(), "a\nz\n");
        assert_eq!(placeholders(&doc), [at(1, RegionIx::new(1), &[LEFT], 2)]);
    }

    #[test]
    fn picking_the_other_side_has_none() {
        let mut doc = left_deletes();
        pick(&mut doc, RegionIx::new(1), vec![RIGHT]);
        assert_eq!(placeholders(&doc), []);
    }

    #[test]
    fn a_pick_order_puts_the_deletion_after_the_block_before_it() {
        let mut doc = left_deletes();
        pick(&mut doc, RegionIx::new(1), vec![RIGHT, LEFT]);
        assert_eq!(doc.text(), "a\nB\nC\nz\n");
        assert_eq!(placeholders(&doc), [at(3, RegionIx::new(1), &[LEFT], 2)]);
    }

    #[test]
    fn a_one_side_deletion_shows_its_side() {
        let doc = three("a\nb\nz\n", "a\nb\nz\n", "a\nz\n");
        assert_eq!(doc.regions()[1].kind, Kind::OneSide(1));
        assert_eq!(placeholders(&doc), [at(1, RegionIx::new(1), &[RIGHT], 1)]);
    }

    #[test]
    fn a_deletion_on_both_sides_shares_one_placeholder() {
        let doc = three("a\nb\nc\nz\n", "a\nz\n", "a\nz\n");
        assert_eq!(doc.regions()[1].kind, Kind::AllSame);
        let both: SmallVec<[Source; 2]> = smallvec![LEFT, RIGHT];
        assert_eq!(placeholders(&doc), [at(1, RegionIx::new(1), &both, 2)]);
    }

    #[test]
    fn deletions_on_either_side_of_a_block_get_a_placeholder_each() {
        // Both sides delete b; picking left, base, right puts base's b between them.
        let mut doc = three("a\nb\nz\n", "a\nz\n", "a\nz\n");
        pick(&mut doc, RegionIx::new(1), vec![LEFT, Source::Base(0), RIGHT]);
        assert_eq!(doc.text(), "a\nb\nz\n");
        assert_eq!(
            placeholders(&doc),
            [at(1, RegionIx::new(1), &[LEFT], 1), at(2, RegionIx::new(1), &[RIGHT], 1)]
        );
    }

    #[test]
    fn an_empty_pick_counts_the_longest_input() {
        // Base has b, left B, right C and C2: right is longest.
        let mut doc = three("a\nb\nz\n", "a\nB\nz\n", "a\nC\nC2\nz\n");
        doc.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        assert_eq!(doc.text(), "a\nz\n");
        assert_eq!(placeholders(&doc), [at(1, RegionIx::new(1), &[], 2)]);
    }

    #[test]
    fn a_deletion_at_the_end_follows_the_last_line() {
        let mut doc = three("a\nb\n", "a\n", "a\nB\n");
        pick(&mut doc, RegionIx::new(1), vec![LEFT]);
        assert_eq!((doc.text().as_str(), doc.line_count()), ("a\n", 1));
        assert_eq!(placeholders(&doc), [at(1, RegionIx::new(1), &[LEFT], 1)]);
    }

    #[test]
    fn a_deletion_after_an_unterminated_last_line_follows_it() {
        // Right ends without a newline; left deletes the last line.
        let mut doc = three("a\nb", "a\n", "a\nB");
        pick(&mut doc, RegionIx::new(1), vec![LEFT]);
        assert_eq!(placeholders(&doc), [at(doc.line_count(), RegionIx::new(1), &[LEFT], 1)]);
    }

    #[test]
    fn skipping_an_insertion_is_no_deletion() {
        // Left inserts X; picking right, which has no lines there, drops no base line.
        let mut doc = three("a\nz\n", "a\nX\nz\n", "a\nz\n");
        pick(&mut doc, RegionIx::new(1), vec![RIGHT]);
        assert_eq!(doc.text(), "a\nz\n");
        assert_eq!(placeholders(&doc), []);
    }

    #[test]
    fn an_empty_pick_of_an_insertion_counts_the_inserted_lines() {
        // Left inserts X; clearing the region drops it.
        let mut doc = three("a\nz\n", "a\nX\nz\n", "a\nz\n");
        doc.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        assert_eq!(placeholders(&doc), [at(1, RegionIx::new(1), &[], 1)]);
    }

    #[test]
    fn an_edited_region_has_none() {
        // Typing over the conflict's B, C (bytes 2..6) leaves an edited region without text.
        let mut doc = left_deletes();
        let delete = merge_core::document::TextPatch { range: 2..6, text: String::new() };
        doc.apply(Command::Edit(merge_core::document::EditBatch(vec![delete]))).expect("edit");
        assert_eq!(doc.text(), "a\nz\n");
        assert_eq!(doc.resolutions()[1], Resolution::Edited);
        assert_eq!(placeholders(&doc), []);
    }

    #[test]
    fn unchanged_and_changed_regions_with_text_have_none() {
        let doc = three("a\nb\nz\n", "a\nB\nz\n", "a\nb\nz\n");
        assert_eq!(placeholders(&doc), []);
    }

    #[test]
    fn placeholders_follow_each_region_in_order() {
        // Region 1: right deletes b. Region 3: left deletes y. Result a, m, z.
        let doc = three("a\nb\nm\ny\nz\n", "a\nb\nm\nz\n", "a\nm\ny\nz\n");
        assert_eq!(doc.text(), "a\nm\nz\n");
        assert_eq!(
            placeholders(&doc),
            [at(1, RegionIx::new(1), &[RIGHT], 1), at(2, RegionIx::new(3), &[LEFT], 1)]
        );
    }
}
