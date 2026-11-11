//! The two-way diff window's view model (spec §5, two-way mode): where each side's changed
//! regions sit, and how many there are.

use std::ops::Range;

use merge_core::diff2::{DiffDocument, Kind2, Region2, Side};
use merge_core::index::RegionIx;

use crate::marks::Tone;

/// The region navigation last made current on `side`, and the caret it left. While the caret
/// stays there, that region is current: an empty region shares its start with the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pin {
    pub side: Side,
    pub region: RegionIx,
    pub caret: usize,
}

/// The tone of `side`'s changed regions: left blue, right pink, as in the merge window.
#[must_use]
pub fn tone(side: Side) -> Tone {
    match side {
        Side::Left => Tone::Left,
        Side::Right => Tone::Right,
    }
}

/// The byte ranges of `side`'s changed regions, in its current text. A region with no lines
/// on this side, a pure insertion on the other, has no range.
/// Regions lag the text until the next recompute, so their lines are clamped to the text.
pub fn changed_bytes(doc: &DiffDocument, side: Side) -> impl Iterator<Item = Range<usize>> + '_ {
    changed(doc).map(move |region| bytes(doc, side, region)).filter(|bytes| !bytes.is_empty())
}

fn bytes(doc: &DiffDocument, side: Side, region: &Region2) -> Range<usize> {
    let text = doc.text(side);
    let byte = |line: usize| text.line_to_byte(line.min(text.len_lines()));
    let lines = region.lines(side);
    byte(lines.start)..byte(lines.end)
}

/// The byte range of `region` in `side`'s current text.
#[must_use]
pub fn region_bytes(doc: &DiffDocument, side: Side, region: RegionIx) -> Range<usize> {
    bytes(doc, side, &doc.regions()[region.get()])
}

/// The region holding `line` of `side`: the last one starting at or before it. An empty
/// region shares its start with the next one, so it is never current by line alone.
#[must_use]
pub fn current_region(doc: &DiffDocument, side: Side, line: usize) -> RegionIx {
    let starting = doc.regions().partition_point(|r| r.lines(side).start <= line);
    RegionIx::new(starting.saturating_sub(1))
}

/// The current region for `side`'s caret at byte `caret`: the pinned one while the caret
/// stays on the pin and the document has that region, else the one holding the caret. A caret
/// past the text counts as its end, since regions and carets lag each other around an edit.
#[must_use]
pub fn held_region(doc: &DiffDocument, pin: Option<Pin>, side: Side, caret: usize) -> RegionIx {
    match pin {
        Some(pin)
            if pin.side == side && pin.caret == caret && pin.region.get() < doc.regions().len() =>
        {
            pin.region
        }
        _ => {
            let text = doc.text(side);
            current_region(doc, side, text.byte_to_line(caret.min(text.len_bytes())))
        }
    }
}

/// The first changed region.
#[must_use]
pub fn first_change(doc: &DiffDocument) -> Option<RegionIx> {
    doc.regions().iter().position(|r| r.kind == Kind2::Changed).map(RegionIx::new)
}

/// The first changed region after `from`.
#[must_use]
pub fn next_change(doc: &DiffDocument, from: RegionIx) -> Option<RegionIx> {
    let after = from.next().get();
    doc.regions()
        .get(after..)?
        .iter()
        .position(|r| r.kind == Kind2::Changed)
        .map(|i| RegionIx::new(after + i))
}

/// The last changed region before `from`.
#[must_use]
pub fn previous_change(doc: &DiffDocument, from: RegionIx) -> Option<RegionIx> {
    doc.regions()
        .get(..from.get())?
        .iter()
        .rposition(|r| r.kind == Kind2::Changed)
        .map(RegionIx::new)
}

/// The spacer rows that level `side` with the other: `(line, rows)` for each changed region
/// with fewer lines here, set above the line after it. A region at the end puts its rows
/// after the last line.
pub fn spacers(doc: &DiffDocument, side: Side) -> impl Iterator<Item = (usize, usize)> + '_ {
    spacers_by(doc, side, |_, lines| lines.len())
}

/// The spacer rows that level `side` with the other when `rows` gives the display rows a
/// run of one side's lines takes, as under soft wrap: `(line, rows)` for each region that
/// takes fewer rows here, set above the line after it. Unchanged regions count too, since
/// panes of different widths wrap the same line differently. Two regions may name one line;
/// their rows add up.
pub fn spacers_by<'a>(
    doc: &'a DiffDocument,
    side: Side,
    rows: impl Fn(Side, Range<usize>) -> usize + 'a,
) -> impl Iterator<Item = (usize, usize)> + 'a {
    doc.regions().iter().filter_map(move |region| {
        let here = region.lines(side);
        let short =
            rows(side.other(), region.lines(side.other())).checked_sub(rows(side, here.clone()))?;
        (short > 0).then_some((here.end, short))
    })
}

/// The rows `region` spans in either pane. Spacer rows level the panes, so a region takes as
/// many rows as its longer side, and the rows above it are those of the regions before it.
#[must_use]
pub fn region_rows(doc: &DiffDocument, region: RegionIx) -> Range<usize> {
    region_rows_by(doc, region, |_, lines| lines.len())
}

/// [`region_rows`] in display rows, `rows` giving those of a run of one side's lines, as
/// [`spacers_by`] takes them.
#[must_use]
pub fn region_rows_by(
    doc: &DiffDocument,
    region: RegionIx,
    rows: impl Fn(Side, Range<usize>) -> usize,
) -> Range<usize> {
    let taller = |r: &Region2| {
        rows(Side::Left, r.lines(Side::Left)).max(rows(Side::Right, r.lines(Side::Right)))
    };
    let start = doc.regions()[..region.get()].iter().map(taller).sum();
    start..start + taller(&doc.regions()[region.get()])
}

/// The number of changed regions.
#[must_use]
pub fn differences(doc: &DiffDocument) -> usize {
    changed(doc).count()
}

fn changed(doc: &DiffDocument) -> impl Iterator<Item = &Region2> {
    doc.regions().iter().filter(|r| r.kind == Kind2::Changed)
}

#[cfg(test)]
#[expect(
    clippy::single_range_in_vec_init,
    reason = "expected values are lists of byte ranges, some with one range"
)]
mod tests {
    use merge_core::diff::DiffOptions;
    use merge_core::diff2::EditBatch;
    use merge_core::testing::text;

    use super::*;

    /// Typed: `jj-core`'s regex dependencies add `PartialEq<Span>` impls for `Range`.
    const NO_BYTES: [Range<usize>; 0] = [];

    fn doc(left: &str, right: &str) -> DiffDocument {
        DiffDocument::new(&text(left), &text(right), DiffOptions::default())
    }

    #[test]
    fn each_side_takes_its_own_tone() {
        assert_eq!(tone(Side::Left), Tone::Left);
        assert_eq!(tone(Side::Right), Tone::Right);
    }

    fn bytes(doc: &DiffDocument, side: Side) -> Vec<Range<usize>> {
        changed_bytes(doc, side).collect()
    }

    #[test]
    fn a_change_covers_its_lines_on_both_sides() {
        // Regions: a unchanged, b→cc changed, d unchanged.
        let d = doc("a\nb\nd\n", "a\ncc\nd\n");
        assert_eq!(bytes(&d, Side::Left), [2..4]);
        assert_eq!(bytes(&d, Side::Right), [2..5]);
        assert_eq!(differences(&d), 1);
    }

    #[test]
    fn an_insertion_has_no_range_on_the_side_without_it() {
        let d = doc("a\n", "a\nb\nc\n");
        assert_eq!(bytes(&d, Side::Left), NO_BYTES);
        assert_eq!(bytes(&d, Side::Right), [2..6]);
        assert_eq!(differences(&d), 1);
    }

    #[test]
    fn equal_texts_have_no_differences() {
        let d = doc("a\nb\n", "a\nb\n");
        assert_eq!(bytes(&d, Side::Left), NO_BYTES);
        assert_eq!(differences(&d), 0);
    }

    #[test]
    fn several_changes_each_have_a_range() {
        // a→A, b same, c→C: two changes.
        let d = doc("a\nb\nc\n", "A\nb\nC\n");
        assert_eq!(bytes(&d, Side::Left), [0..2, 4..6]);
        assert_eq!(differences(&d), 2);
    }

    // Regions of "a b c d e" against "a B c e": 0 a, 1 b→B, 2 c, 3 d→(none), 4 e.
    fn five() -> DiffDocument {
        doc("a\nb\nc\nd\ne\n", "a\nB\nc\ne\n")
    }

    #[test]
    fn region_bytes_cover_the_regions_lines() {
        let d = five();
        assert_eq!(region_bytes(&d, Side::Left, RegionIx::new(1)), 2..4);
        assert_eq!(region_bytes(&d, Side::Left, RegionIx::new(3)), 6..8);
        assert_eq!(region_bytes(&d, Side::Right, RegionIx::new(3)), 6..6);
        assert_eq!(region_bytes(&d, Side::Right, RegionIx::new(4)), 6..8);
    }

    #[test]
    fn the_current_region_holds_the_line() {
        let d = five();
        assert_eq!(current_region(&d, Side::Left, 0), RegionIx::new(0));
        assert_eq!(current_region(&d, Side::Left, 3), RegionIx::new(3));
        assert_eq!(current_region(&d, Side::Left, 4), RegionIx::new(4));
        // Right line 3 is e: the empty region 3 starts there too, and region 4 holds it.
        assert_eq!(current_region(&d, Side::Right, 3), RegionIx::new(4));
        // Past the end, and the line after the last newline: the last region.
        assert_eq!(current_region(&d, Side::Right, 4), RegionIx::new(4));
        assert_eq!(current_region(&d, Side::Right, 99), RegionIx::new(4));
    }

    fn pin(side: Side, region: usize, caret: usize) -> Pin {
        Pin { side, region: RegionIx::new(region), caret }
    }

    #[test]
    fn a_pin_holds_for_its_side_and_caret() {
        // Left byte 6 is line 3, region 3; right byte 6 is line 3, region 4.
        let d = five();
        let held = |pin, side, caret| held_region(&d, pin, side, caret).get();
        assert_eq!(held(Some(pin(Side::Left, 1, 6)), Side::Left, 6), 1);
        assert_eq!(held(Some(pin(Side::Left, 1, 6)), Side::Left, 8), 4);
        assert_eq!(held(Some(pin(Side::Left, 1, 6)), Side::Right, 6), 4);
        assert_eq!(held(None, Side::Left, 6), 3);
    }

    #[test]
    fn a_pin_past_the_last_region_is_dropped() {
        // Five regions, so 4 is the last.
        let d = five();
        assert_eq!(held_region(&d, Some(pin(Side::Left, 4, 6)), Side::Left, 6), RegionIx::new(4));
        assert_eq!(held_region(&d, Some(pin(Side::Left, 5, 6)), Side::Left, 6), RegionIx::new(3));
    }

    #[test]
    fn a_caret_past_the_text_is_in_the_last_region() {
        assert_eq!(held_region(&five(), None, Side::Left, 99), RegionIx::new(4));
    }

    #[test]
    fn the_first_change_is_the_first_changed_region() {
        let inner = doc("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n");
        assert_eq!(first_change(&inner), Some(RegionIx::new(1)));
        let at_the_start = doc("a\nb\nc\n", "A\nb\nC\n");
        assert_eq!(first_change(&at_the_start), Some(RegionIx::new(0)));
        let inserted_at_the_end = doc("a\n", "a\nb\n");
        assert_eq!(first_change(&inserted_at_the_end), Some(RegionIx::new(1)));
    }

    #[test]
    fn identical_and_empty_texts_have_no_first_change() {
        assert_eq!(first_change(&doc("a\nb\n", "a\nb\n")), None);
        assert_eq!(first_change(&doc("", "")), None);
    }

    #[test]
    fn next_and_previous_skip_unchanged_regions() {
        let d = five();
        assert_eq!(next_change(&d, RegionIx::new(0)), Some(RegionIx::new(1)));
        assert_eq!(next_change(&d, RegionIx::new(1)), Some(RegionIx::new(3)));
        assert_eq!(next_change(&d, RegionIx::new(3)), None);
        assert_eq!(previous_change(&d, RegionIx::new(4)), Some(RegionIx::new(3)));
        assert_eq!(previous_change(&d, RegionIx::new(3)), Some(RegionIx::new(1)));
        assert_eq!(previous_change(&d, RegionIx::new(1)), None);
    }

    #[test]
    fn equal_texts_have_nowhere_to_go() {
        let d = doc("a\n", "a\n");
        assert_eq!(next_change(&d, RegionIx::new(0)), None);
        assert_eq!(previous_change(&d, RegionIx::new(0)), None);
    }

    fn spacer_rows(d: &DiffDocument, side: Side) -> Vec<(usize, usize)> {
        spacers(d, side).collect()
    }

    #[test]
    fn the_shorter_side_of_a_change_gets_the_difference_below_it() {
        // b (left 1..2) against x y (right 1..3): left needs one row above c, its line 2.
        let d = doc("a\nb\nc\n", "a\nx\ny\nc\n");
        assert_eq!(spacer_rows(&d, Side::Left), [(2, 1)]);
        assert_eq!(spacer_rows(&d, Side::Right), []);
    }

    #[test]
    fn a_deletion_at_the_start_puts_rows_above_line_0() {
        let d = doc("x\ny\na\n", "a\n");
        assert_eq!(spacer_rows(&d, Side::Left), []);
        assert_eq!(spacer_rows(&d, Side::Right), [(0, 2)]);
    }

    #[test]
    fn an_insertion_at_the_end_puts_rows_after_the_last_line() {
        // Left has one line; its rows go above line 1, the end.
        let d = doc("a\n", "a\nb\nc\n");
        assert_eq!(spacer_rows(&d, Side::Left), [(1, 2)]);
        assert_eq!(spacer_rows(&d, Side::Right), []);
    }

    #[test]
    fn each_change_levels_on_its_own_side() {
        // Regions of five(): 1 b→B (level), 3 d→(none): right needs one row above e, line 3.
        assert_eq!(spacer_rows(&five(), Side::Left), []);
        assert_eq!(spacer_rows(&five(), Side::Right), [(3, 1)]);
        // a→A1 A2 then c d→C: left short by one at line 1, right short by one at line 4.
        let d = doc("a\nb\nc\nd\n", "A1\nA2\nb\nC\n");
        assert_eq!(spacer_rows(&d, Side::Left), [(1, 1)]);
        assert_eq!(spacer_rows(&d, Side::Right), [(4, 1)]);
    }

    #[test]
    fn a_region_spans_the_rows_of_its_longer_side() {
        // Rows: 0 a, 1 b|B, 2 c, 3 d|spacer, 4 e.
        let d = five();
        assert_eq!(region_rows(&d, RegionIx::new(0)), 0..1);
        assert_eq!(region_rows(&d, RegionIx::new(1)), 1..2);
        assert_eq!(region_rows(&d, RegionIx::new(3)), 3..4);
        assert_eq!(region_rows(&d, RegionIx::new(4)), 4..5);
    }

    #[test]
    fn spacers_on_either_side_push_the_rows_below_down() {
        // Regions: 0 a, 1 b|xyz (left gets 2 spacers), 2 c, 3 de|f (right gets 1), 4 g.
        let d = doc("a\nb\nc\nd\ne\ng\n", "a\nx\ny\nz\nc\nf\ng\n");
        assert_eq!(region_rows(&d, RegionIx::new(1)), 1..4);
        assert_eq!(region_rows(&d, RegionIx::new(2)), 4..5);
        assert_eq!(region_rows(&d, RegionIx::new(3)), 5..7);
        assert_eq!(region_rows(&d, RegionIx::new(4)), 7..8);
    }

    /// Display rows per line, from a table for each side: the rows each line wraps to.
    fn wrapped<'t>(
        left: &'t [usize],
        right: &'t [usize],
    ) -> impl Fn(Side, Range<usize>) -> usize + 't {
        move |side, lines| {
            let table = match side {
                Side::Left => left,
                Side::Right => right,
            };
            table[lines].iter().sum()
        }
    }

    fn wrapped_spacer_rows(
        d: &DiffDocument,
        side: Side,
        left: &[usize],
        right: &[usize],
    ) -> Vec<(usize, usize)> {
        spacers_by(d, side, wrapped(left, right)).collect()
    }

    #[test]
    fn a_wrapped_change_levels_by_display_rows() {
        // five(): b wraps to 3 rows against B's 1, so right needs 2 rows above c, its line 2.
        let (left, right) = ([1, 3, 1, 1, 1], [1, 1, 1, 1]);
        assert_eq!(wrapped_spacer_rows(&five(), Side::Left, &left, &right), []);
        assert_eq!(wrapped_spacer_rows(&five(), Side::Right, &left, &right), [(2, 2), (3, 1)]);
    }

    #[test]
    fn a_side_with_more_lines_can_take_fewer_rows() {
        // b (4 rows) against x y (1 row each): right is short by 2 above c, its line 3.
        let d = doc("a\nb\nc\n", "a\nx\ny\nc\n");
        let (left, right) = ([1, 4, 1], [1, 1, 1, 1]);
        assert_eq!(wrapped_spacer_rows(&d, Side::Left, &left, &right), []);
        assert_eq!(wrapped_spacer_rows(&d, Side::Right, &left, &right), [(3, 2)]);
    }

    #[test]
    fn unchanged_lines_that_wrap_differently_are_levelled() {
        // Panes of different widths: a wraps to 2 rows on the left, 1 on the right.
        let d = doc("a\nb\n", "a\nc\n");
        let (left, right) = ([2, 1], [1, 1]);
        assert_eq!(wrapped_spacer_rows(&d, Side::Left, &left, &right), []);
        assert_eq!(wrapped_spacer_rows(&d, Side::Right, &left, &right), [(1, 1)]);
    }

    #[test]
    fn rows_for_one_line_add_up_from_each_region() {
        // five(): c wraps to 2 rows on the left only, and d, deleted, to 3. Both levels sit
        // above e, right's line 3, one entry per region.
        let (left, right) = ([1, 1, 2, 3, 1], [1, 1, 1, 1]);
        assert_eq!(wrapped_spacer_rows(&five(), Side::Right, &left, &right), [(3, 1), (3, 3)]);
        assert_eq!(wrapped_spacer_rows(&five(), Side::Left, &left, &right), []);
    }

    #[test]
    fn one_row_per_line_gives_the_unwrapped_spacers() {
        let d = doc("a\nb\nc\nd\n", "A1\nA2\nb\nC\n");
        for side in [Side::Left, Side::Right] {
            let lines: Vec<_> = spacers_by(&d, side, |_, lines| lines.len()).collect();
            assert_eq!(lines, spacer_rows(&d, side), "{side:?}");
        }
    }

    #[test]
    fn a_wrapped_region_spans_the_display_rows_of_its_taller_side() {
        // five() with b at 3 rows and d at 2: rows 0 a, 1..4 b|B, 4 c, 5..7 d|spacer, 7 e.
        let rows = wrapped(&[1, 3, 1, 2, 1], &[1, 1, 1, 1]);
        let d = five();
        let spans: Vec<_> = (0..5).map(|r| region_rows_by(&d, RegionIx::new(r), &rows)).collect();
        assert_eq!(spans, [0..1, 1..4, 4..5, 5..7, 7..8]);
    }

    #[test]
    fn equal_texts_need_no_spacers() {
        let d = doc("a\nb\n", "a\nb\n");
        assert_eq!(spacer_rows(&d, Side::Left), []);
        assert_eq!(spacer_rows(&d, Side::Right), []);
    }

    #[test]
    fn ranges_follow_installed_regions_in_the_current_text() {
        let mut d = doc("a\nb\n", "a\nc\n");
        // An unrecomputed edit shifts the text but not the regions: the range is in the
        // old line numbers, mapped through the new text.
        d.edit(
            Side::Left,
            EditBatch(vec![merge_core::diff2::TextPatch { range: 0..0, text: "zz".to_owned() }]),
        )
        .expect("edits");
        assert_eq!(bytes(&d, Side::Left), [4..6]);
        let (version, regions) = d.recompute().run();
        assert!(d.install(version, regions));
        // zza→a changed and b→c changed join into one region over both lines.
        assert_eq!(bytes(&d, Side::Left), [0..6]);
        assert_eq!(differences(&d), 1);
    }
}
