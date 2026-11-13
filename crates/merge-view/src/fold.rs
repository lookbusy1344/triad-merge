//! Collapsed unchanged regions in the two-way diff (spec §5): which lines each side hides, the
//! fold row that stands in for them, and which folds the user opened.
//!
//! An unchanged region keeps `context` lines at each end that faces a change and hides the
//! rest, when that hides at least [`MIN_FOLDED_LINES`]. The first region keeps no lines at its
//! top and the last none at its bottom. Both sides of an unchanged region have the same number
//! of lines, so each side hides as many lines as the other and the panes stay level.

use std::ops::Range;

use merge_core::diff2::{DiffDocument, Kind2, Region2, Side};

/// The fewest lines a fold hides: one hidden line takes no less room than its fold row.
pub const MIN_FOLDED_LINES: usize = 2;

/// The default number of lines an unchanged region keeps at each end that faces a change.
pub const DEFAULT_CONTEXT_LINES: usize = 3;

/// The lines one unchanged region hides on each side; as many on each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    pub left: Range<usize>,
    pub right: Range<usize>,
}

impl Fold {
    #[must_use]
    pub fn lines(&self, side: Side) -> Range<usize> {
        match side {
            Side::Left => self.left.clone(),
            Side::Right => self.right.clone(),
        }
    }

    /// The number of lines hidden on each side.
    #[must_use]
    pub fn len(&self) -> usize {
        self.left.len()
    }

    /// Always false: a fold hides at least [`MIN_FOLDED_LINES`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
}

/// The unchanged regions the user opened, by their lines on each side. Edits move them with
/// the text, so they survive a recompute that renumbers the regions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opened {
    regions: Vec<Fold>,
}

impl Opened {
    /// Open the unchanged region that `fold` hides lines of.
    pub fn open(&mut self, fold: &Fold) {
        self.regions.push(fold.clone());
    }

    /// Close every region.
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Whether the user opened an unchanged region that shares lines with `region`.
    #[must_use]
    pub fn holds(&self, region: &Region2) -> bool {
        self.regions
            .iter()
            .any(|open| overlaps(&open.left, &region.left) || overlaps(&open.right, &region.right))
    }

    /// Move the opened lines on `side` that lie past an edit ending on line `end`, which
    /// changed the line count by `delta`.
    pub fn shift(&mut self, side: Side, end: usize, delta: isize) {
        for open in &mut self.regions {
            let lines = match side {
                Side::Left => &mut open.left,
                Side::Right => &mut open.right,
            };
            if lines.start > end {
                lines.start = lines.start.saturating_add_signed(delta);
            }
            if lines.end > end {
                lines.end = lines.end.saturating_add_signed(delta);
            }
            lines.end = lines.end.max(lines.start);
        }
    }

    /// Keep only the regions `doc` still has, by their current lines: after a recompute.
    pub fn renew(&mut self, doc: &DiffDocument) {
        let regions = doc
            .regions()
            .iter()
            .filter(|region| region.kind == Kind2::Unchanged && self.holds(region))
            .map(|region| Fold { left: region.left.clone(), right: region.right.clone() })
            .collect();
        self.regions = regions;
    }
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// The folds of `doc`'s unchanged regions with `context` lines kept at each end that faces a
/// change, leaving out the regions in `opened`. Sorted by line.
#[must_use]
pub fn folds(doc: &DiffDocument, context: usize, opened: &Opened) -> Vec<Fold> {
    let regions = doc.regions();
    let last = regions.len().saturating_sub(1);
    regions
        .iter()
        .enumerate()
        .filter(|(_, region)| region.kind == Kind2::Unchanged && !opened.holds(region))
        .filter_map(|(ix, region)| {
            let top = if ix == 0 { 0 } else { context };
            let bottom = if ix == last { 0 } else { context };
            let hidden = region.left.len().checked_sub(top + bottom)?;
            (hidden >= MIN_FOLDED_LINES).then(|| {
                let at = |lines: &Range<usize>| lines.start + top..lines.start + top + hidden;
                Fold { left: at(&region.left), right: at(&region.right) }
            })
        })
        .collect()
}

/// The fold whose row sits above `line` on `side`: the one whose hidden lines end there.
#[must_use]
pub fn fold_above(folds: &[Fold], side: Side, line: usize) -> Option<&Fold> {
    folds.iter().find(|fold| fold.lines(side).end == line)
}

/// The folds that hide any line of `lines` on `side`.
pub fn folds_over(
    folds: &[Fold],
    side: Side,
    lines: Range<usize>,
) -> impl Iterator<Item = &Fold> + '_ {
    folds.iter().filter(move |fold| {
        let hidden = fold.lines(side);
        hidden.start < lines.end.max(lines.start + 1) && lines.start < hidden.end
    })
}

/// A fold row: a spacer row above `line` that names how many lines the fold hides. `before`
/// spacer rows sit above `line` ahead of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldRow {
    pub line: usize,
    pub before: usize,
    pub hidden: usize,
}

/// `side`'s spacers with `folds` collapsed, and the fold rows among them.
///
/// A spacer above a hidden line moves to the line after the fold, since the editor drops
/// spacers on hidden lines; it levels the change above the fold, so it stays ahead of the
/// fold row. Each fold adds one spacer row above the line after it, after the last line for a
/// fold at the end. `spacers` are `(line, rows)` pairs as the editor takes them.
#[must_use]
pub fn fold_spacers(
    spacers: impl IntoIterator<Item = (usize, usize)>,
    folds: &[Fold],
    side: Side,
) -> (Vec<(usize, usize)>, Vec<FoldRow>) {
    let after_fold = |line: usize| {
        folds
            .iter()
            .map(|fold| fold.lines(side))
            .find(|hidden| hidden.contains(&line))
            .map_or(line, |hidden| hidden.end)
    };
    let mut moved: Vec<(usize, usize)> =
        spacers.into_iter().map(|(line, rows)| (after_fold(line), rows)).collect();
    let rows = folds
        .iter()
        .map(|fold| {
            let line = fold.lines(side).end;
            let before = moved.iter().filter(|&&(at, _)| at == line).map(|&(_, rows)| rows).sum();
            FoldRow { line, before, hidden: fold.len() }
        })
        .collect();
    moved.extend(folds.iter().map(|fold| (fold.lines(side).end, 1)));
    (moved, rows)
}

#[cfg(test)]
#[expect(
    clippy::single_range_in_vec_init,
    reason = "expected values are lists of line ranges, some with one range"
)]
mod tests {
    use merge_core::diff::DiffOptions;
    use merge_core::testing::text;

    use super::*;

    const CONTEXT: usize = 3;

    /// Typed: `jj-core`'s regex dependencies add `PartialEq<Span>` impls for `Range`.
    const NO_LINES: [Range<usize>; 0] = [];
    const NO_FOLDS: [Fold; 0] = [];

    fn doc(left: &str, right: &str) -> DiffDocument {
        DiffDocument::new(&text(left), &text(right), DiffOptions::default())
    }

    /// Lines `0..n` as "l0\n", "l1\n", ….
    fn lines(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("l{i}\n")).collect()
    }

    /// `left` lines with line `changed` replaced on the right.
    fn one_change(len: usize, changed: usize) -> DiffDocument {
        let left = lines(len);
        let mut right = left.clone();
        right[changed] = "changed\n".to_owned();
        doc(&left.concat(), &right.concat())
    }

    fn left_folds(doc: &DiffDocument, context: usize) -> Vec<Range<usize>> {
        folds(doc, context, &Opened::default()).into_iter().map(|fold| fold.left).collect()
    }

    /// Changes at lines 0 and `gap + 1`, with `gap` unchanged lines between them.
    fn gap(gap: usize) -> DiffDocument {
        let left = lines(gap + 2);
        let mut right = left.clone();
        right[0] = "x\n".to_owned();
        right[gap + 1] = "y\n".to_owned();
        doc(&left.concat(), &right.concat())
    }

    #[test]
    fn a_middle_region_folds_from_two_hidden_lines() {
        // 2c and 2c + 1 lines between the changes hide 0 and 1: no fold.
        assert_eq!(left_folds(&gap(2 * CONTEXT), CONTEXT), NO_LINES);
        assert_eq!(left_folds(&gap(2 * CONTEXT + 1), CONTEXT), NO_LINES);
        // 2c + 2 hides the two lines after the top context: lines 1 + 3 = 4 and 5.
        assert_eq!(left_folds(&gap(2 * CONTEXT + 2), CONTEXT), [4..6]);
    }

    #[test]
    fn the_first_and_last_regions_keep_context_only_towards_the_change() {
        // 20 lines, line 10 changed: 0..10 unchanged, 10 changed, 11..20 unchanged.
        let d = one_change(20, 10);
        assert_eq!(left_folds(&d, CONTEXT), [0..7, 14..20]);
    }

    #[test]
    fn edge_regions_fold_from_two_hidden_lines() {
        // Line c + 1 changed: the first region has c + 1 lines and hides 1.
        assert_eq!(left_folds(&one_change(CONTEXT + 2, CONTEXT + 1), CONTEXT), NO_LINES);
        // Line c + 2 changed: the first region hides 0..2.
        assert_eq!(left_folds(&one_change(CONTEXT + 3, CONTEXT + 2), CONTEXT), [0..2]);
    }

    #[test]
    fn a_file_with_no_changes_folds_whole() {
        let same = lines(5).concat();
        assert_eq!(left_folds(&doc(&same, &same), CONTEXT), [0..5]);
        assert_eq!(left_folds(&doc("one\n", "one\n"), CONTEXT), NO_LINES);
    }

    #[test]
    fn no_context_hides_the_whole_region() {
        assert_eq!(left_folds(&gap(2), 0), [1..3]);
        assert_eq!(left_folds(&one_change(20, 10), 0), [0..10, 11..20]);
    }

    #[test]
    fn each_side_hides_its_own_lines_and_as_many() {
        // Left line 2 becomes three lines on the right: right lines after it sit two lower.
        let left = lines(20);
        let mut right = left.clone();
        right.splice(2..3, ["x\n".to_owned(), "y\n".to_owned(), "z\n".to_owned()]);
        let d = doc(&left.concat(), &right.concat());
        let folds = folds(&d, CONTEXT, &Opened::default());
        assert_eq!(folds, [Fold { left: 6..20, right: 8..22 }]);
        assert_eq!(folds[0].len(), 14);
    }

    #[test]
    fn an_opened_region_does_not_fold() {
        let d = one_change(20, 10);
        let mut opened = Opened::default();
        let all = folds(&d, CONTEXT, &opened);
        opened.open(&all[1]);
        assert_eq!(folds(&d, CONTEXT, &opened), [all[0].clone()]);
        opened.clear();
        assert_eq!(folds(&d, CONTEXT, &opened), all);
    }

    #[test]
    fn either_side_holds_an_opened_region() {
        // The left lines are stale, as after an edit the shift missed; the right still match.
        let d = one_change(20, 10);
        let mut opened = Opened::default();
        opened.open(&Fold { left: 100..110, right: 15..17 });
        assert_eq!(folds(&d, CONTEXT, &opened), [Fold { left: 0..7, right: 0..7 }]);
    }

    #[test]
    fn an_opened_region_follows_edits_above_it() {
        // Open the last region, 11..20, then add two lines at left line 0.
        let d = one_change(20, 10);
        let mut opened = Opened::default();
        opened.open(&folds(&d, CONTEXT, &opened)[1]);
        // The fold hid 14..20 on each side; the left moves down two.
        opened.shift(Side::Left, 0, 2);
        assert_eq!(opened.regions, [Fold { left: 16..22, right: 14..20 }]);

        let mut left = lines(20);
        left.splice(0..0, ["new\n".to_owned(), "new\n".to_owned()]);
        let mut right = lines(20);
        right[10] = "changed\n".to_owned();
        let d = doc(&left.concat(), &right.concat());
        opened.renew(&d);
        // The edit is a change at the top; the unchanged regions are 2..12 and 13..22 on the
        // left. The second holds the opened lines, and renew keeps it whole.
        assert_eq!(opened.regions, [Fold { left: 13..22, right: 11..20 }]);
        let lefts: Vec<_> = folds(&d, CONTEXT, &opened).into_iter().map(|fold| fold.left).collect();
        assert_eq!(lefts, [5..9]);
    }

    #[test]
    fn a_shift_moves_only_lines_after_the_edit() {
        let mut opened = Opened::default();
        opened.open(&Fold { left: 10..20, right: 10..20 });
        // Inside: the end moves, the start stays.
        opened.shift(Side::Left, 12, -1);
        assert_eq!(opened.regions[0].left, 10..19);
        opened.shift(Side::Left, 9, 1);
        assert_eq!(opened.regions[0].left, 11..20, "the line before moves both");
        opened.shift(Side::Left, 9, -1);
        // An edit on the first line keeps the start: the line is still there.
        opened.shift(Side::Left, 10, 1);
        assert_eq!(opened.regions[0].left, 10..20);
        opened.shift(Side::Left, 10, -1);
        // Below: nothing moves.
        opened.shift(Side::Left, 30, 4);
        assert_eq!(opened.regions[0].left, 10..19);
        // Right side only.
        opened.shift(Side::Right, 0, 3);
        assert_eq!(opened.regions[0], Fold { left: 10..19, right: 13..23 });
        // An end pulled above the start leaves an empty range, not a reversed one.
        let mut short = Opened::default();
        short.open(&Fold { left: 10..12, right: 10..12 });
        short.shift(Side::Left, 11, -5);
        assert_eq!(short.regions[0].left, 10..10);
    }

    #[test]
    fn renew_drops_regions_that_are_gone() {
        let d = one_change(20, 10);
        let mut opened = Opened::default();
        opened.open(&Fold { left: 40..50, right: 40..50 });
        opened.renew(&d);
        assert_eq!(opened, Opened::default());
    }

    #[test]
    fn a_fold_row_is_found_by_the_line_below_it() {
        let d = one_change(20, 10);
        let all = folds(&d, CONTEXT, &Opened::default());
        assert_eq!(fold_above(&all, Side::Right, 7), Some(&all[0]));
        assert_eq!(fold_above(&all, Side::Left, 20), Some(&all[1]));
        assert_eq!(fold_above(&all, Side::Left, 8), None);
    }

    #[test]
    fn folds_over_lines_include_a_caret_line() {
        let d = one_change(20, 10);
        let all = folds(&d, CONTEXT, &Opened::default());
        let over = |lines| folds_over(&all, Side::Left, lines).cloned().collect::<Vec<_>>();
        assert_eq!(over(6..6), [all[0].clone()], "an empty range is its line");
        assert_eq!(over(7..7), NO_FOLDS);
        assert_eq!(over(5..15), all);
        assert_eq!(over(20..20), NO_FOLDS, "the line after the last fold");
    }

    #[test]
    fn a_fold_row_follows_the_spacers_it_moves() {
        // Left line 1 deleted on the right, with no context: the right's filler row sits
        // above right line 1, which the next fold hides.
        let left = lines(10);
        let mut right = left.clone();
        right.remove(1);
        let d = doc(&left.concat(), &right.concat());
        let folds = folds(&d, 0, &Opened::default());
        assert_eq!(folds, [Fold { left: 2..10, right: 1..9 }]);
        let spacers: Vec<_> = crate::diff::spacers(&d, Side::Right).collect();
        assert_eq!(spacers, [(1, 1)]);
        let (moved, rows) = fold_spacers(spacers, &folds, Side::Right);
        assert_eq!(moved, [(9, 1), (9, 1)]);
        assert_eq!(rows, [FoldRow { line: 9, before: 1, hidden: 8 }]);
        // The left has no filler: its fold row is the first spacer above line 10.
        let (moved, rows) = fold_spacers(crate::diff::spacers(&d, Side::Left), &folds, Side::Left);
        assert_eq!(moved, [(10, 1)]);
        assert_eq!(rows, [FoldRow { line: 10, before: 0, hidden: 8 }]);
    }

    #[test]
    fn spacers_outside_folds_stay() {
        let folds = [Fold { left: 4..8, right: 4..8 }];
        let (moved, rows) = fold_spacers([(2, 3), (8, 1), (9, 2)], &folds, Side::Left);
        assert_eq!(moved, [(2, 3), (8, 1), (9, 2), (8, 1)]);
        assert_eq!(rows, [FoldRow { line: 8, before: 1, hidden: 4 }]);
    }
}
