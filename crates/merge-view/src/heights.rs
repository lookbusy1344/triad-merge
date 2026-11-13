//! The display rows of a pane whose items each take one or more of them: the top panes'
//! aligned rows when they wrap, and the result's rows when its editor wraps (spec §5). Scroll
//! sync and jumps work in item positions; a pane scrolls by display rows. A fold (spec §5)
//! gives its items one display row together: its first item takes the row, the rest none.

use std::ops::Range;

/// How many display rows each item of a pane takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heights {
    /// This many items, one display row each.
    Uniform(usize),
    /// The first display row of each item, then the display row count.
    Starts(Vec<usize>),
    /// One display row per item, apart from the folds.
    Folded(Folds),
}

/// Items of one display row each, with sorted, disjoint folds of items that share one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folds {
    items: usize,
    ranges: Vec<Range<usize>>,
    /// The rows hidden by the folds before each fold: one less than each fold's items.
    hidden_before: Vec<usize>,
}

impl Folds {
    /// The rows the folds hide in all.
    fn hidden(&self) -> usize {
        match (self.ranges.last(), self.hidden_before.last()) {
            (Some(last), Some(&before)) => before + hidden_by(last),
            _ => 0,
        }
    }

    fn start(&self, item: usize) -> usize {
        // The folds starting before `item`; the last of them may still run on past it.
        let before = self.ranges.partition_point(|fold| fold.start < item);
        let Some(fold) = before.checked_sub(1).map(|last| &self.ranges[last]) else {
            return item;
        };
        let partial = item.min(fold.end).saturating_sub(fold.start + 1);
        item - self.hidden_before[before - 1] - partial
    }

    fn item_at(&self, row: usize) -> usize {
        // The folds whose row is at or above `row`.
        let above = self.ranges.partition_point(|fold| self.row_of(fold) <= row);
        let Some(last) = above.checked_sub(1) else {
            return row;
        };
        let fold = &self.ranges[last];
        if self.row_of(fold) == row {
            fold.start
        } else {
            row + self.hidden_before[last] + hidden_by(fold)
        }
    }

    /// The display row of `fold`'s row.
    fn row_of(&self, fold: &Range<usize>) -> usize {
        self.start(fold.start)
    }
}

/// The rows `fold` hides: all its items but the first.
fn hidden_by(fold: &Range<usize>) -> usize {
    fold.len().saturating_sub(1)
}

impl Heights {
    /// Items taking `rows` display rows each, in order. An item takes at least one row.
    #[must_use]
    pub fn from_rows(rows: impl IntoIterator<Item = usize>) -> Self {
        let rows = rows.into_iter();
        let mut starts = Vec::with_capacity(rows.size_hint().0 + 1);
        let end = rows.fold(0, |start, rows| {
            starts.push(start);
            start + rows.max(1)
        });
        starts.push(end);
        Self::Starts(starts)
    }

    /// These heights with each of `folds`, ranges of items sorted and apart, taking one
    /// display row: its first item takes it, the rest none. Folds past the items are cut off.
    #[must_use]
    pub fn fold(self, folds: &[Range<usize>]) -> Self {
        let items = self.items();
        let folds: Vec<_> = folds
            .iter()
            .map(|fold| fold.start.min(items)..fold.end.min(items))
            .filter(|fold| !fold.is_empty())
            .collect();
        if folds.is_empty() {
            return self;
        }
        match self {
            Self::Uniform(items) => {
                let hidden_before = folds
                    .iter()
                    .scan(0, |hidden, fold| {
                        let before = *hidden;
                        *hidden += hidden_by(fold);
                        Some(before)
                    })
                    .collect();
                Self::Folded(Folds { items, ranges: folds, hidden_before })
            }
            Self::Starts(_) | Self::Folded(_) => {
                let mut rows: Vec<usize> = (0..items).map(|item| self.of(item).len()).collect();
                for fold in &folds {
                    rows[fold.clone()].fill(0);
                    rows[fold.start] = 1;
                }
                let mut starts = Vec::with_capacity(items + 1);
                let end = rows.iter().fold(0, |start, rows| {
                    starts.push(start);
                    start + rows
                });
                starts.push(end);
                Self::Starts(starts)
            }
        }
    }

    #[must_use]
    pub fn items(&self) -> usize {
        match self {
            Self::Uniform(items) => *items,
            Self::Starts(starts) => starts.len() - 1,
            Self::Folded(folds) => folds.items,
        }
    }

    /// The display rows of every item.
    #[must_use]
    pub fn rows(&self) -> usize {
        match self {
            Self::Uniform(items) => *items,
            Self::Starts(starts) => starts[starts.len() - 1],
            Self::Folded(folds) => folds.items - folds.hidden(),
        }
    }

    /// The display rows of `items`, held to the items there are.
    #[must_use]
    pub fn span(&self, items: Range<usize>) -> Range<usize> {
        let last = self.items();
        let start = self.start(items.start.min(last));
        start..self.start(items.end.clamp(items.start.min(last), last))
    }

    /// The display rows of `item`.
    #[must_use]
    pub fn of(&self, item: usize) -> Range<usize> {
        self.span(item..item + 1)
    }

    /// The item display row `row` belongs to, and the row's place within it; `None` past
    /// the last row.
    #[must_use]
    pub fn at(&self, row: usize) -> Option<(usize, usize)> {
        if row >= self.rows() {
            return None;
        }
        let item = match self {
            Self::Uniform(_) => row,
            // The last item starting at `row`: items before it that start there take no rows.
            Self::Starts(starts) => starts.partition_point(|&start| start <= row) - 1,
            Self::Folded(folds) => folds.item_at(row),
        };
        Some((item, row - self.start(item)))
    }

    /// The display row position at item position `position`: a share of an item is the same
    /// share of its rows. The end of the items is the end of the rows.
    #[must_use]
    pub fn to_rows(&self, position: f32) -> f32 {
        let Some((item, share)) = split(position, self.items()) else {
            return to_f32(self.rows());
        };
        let rows = self.of(item);
        to_f32(rows.start) + share * to_f32(rows.len())
    }

    /// The item position at display row position `row`: the inverse of [`Self::to_rows`].
    #[must_use]
    pub fn to_items(&self, row: f32) -> f32 {
        let Some(((item, within), share)) =
            split(row, self.rows()).and_then(|(whole, share)| Some((self.at(whole)?, share)))
        else {
            return to_f32(self.items());
        };
        let rows = self.of(item).len();
        to_f32(item) + (to_f32(within) + share) / to_f32(rows)
    }

    fn start(&self, item: usize) -> usize {
        match self {
            Self::Uniform(_) => item,
            Self::Starts(starts) => starts[item],
            Self::Folded(folds) => folds.start(item),
        }
    }
}

/// The whole and fractional parts of `position`, at least 0; `None` at or past `count`.
fn split(position: f32, count: usize) -> Option<(usize, f32)> {
    let position = position.max(0.0);
    if position >= to_f32(count) {
        return None;
    }
    let whole = position.floor();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a non-negative whole number below `count`"
    )]
    let index = whole as usize;
    Some((index, position - whole))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "row counts stay far below 2^24 in practice; pixels need no more"
)]
fn to_f32(n: usize) -> f32 {
    n as f32
}

#[cfg(test)]
#[expect(
    clippy::single_range_in_vec_init,
    reason = "folds are lists of item ranges, some with one range"
)]
mod tests {
    use super::*;

    /// Items of 1, 3 and 2 rows: rows 0, 1-3 and 4-5.
    fn mixed() -> Heights {
        Heights::from_rows([1, 3, 2])
    }

    #[test]
    fn rows_add_up_and_each_item_takes_at_least_one() {
        assert_eq!(mixed(), Heights::Starts(vec![0, 1, 4, 6]));
        assert_eq!((mixed().items(), mixed().rows()), (3, 6));
        assert_eq!(Heights::from_rows([0, 2]), Heights::Starts(vec![0, 1, 3]));
        assert_eq!(Heights::from_rows([]), Heights::Starts(vec![0]));
        assert_eq!((Heights::Uniform(5).items(), Heights::Uniform(5).rows()), (5, 5));
    }

    #[test]
    fn an_item_spans_its_rows() {
        assert_eq!(mixed().of(1), 1..4);
        assert_eq!(mixed().span(1..3), 1..6);
        assert_eq!(mixed().span(2..2), 4..4);
        // Held to the items there are.
        assert_eq!(mixed().span(2..9), 4..6);
        assert_eq!(mixed().of(7), 6..6);
        assert_eq!(Heights::Uniform(5).span(1..3), 1..3);
    }

    #[test]
    fn a_row_names_its_item_and_place() {
        assert_eq!(mixed().at(0), Some((0, 0)));
        assert_eq!(mixed().at(1), Some((1, 0)));
        assert_eq!(mixed().at(3), Some((1, 2)));
        assert_eq!(mixed().at(5), Some((2, 1)));
        assert_eq!(mixed().at(6), None);
        assert_eq!(Heights::Uniform(5).at(4), Some((4, 0)));
        assert_eq!(Heights::Uniform(5).at(5), None);
    }

    #[test]
    fn a_share_of_an_item_is_the_same_share_of_its_rows() {
        let heights = mixed();
        assert_eq!(heights.to_rows(0.5), 0.5);
        // Half way through item 1's three rows.
        assert_eq!(heights.to_rows(1.5), 2.5);
        assert_eq!(heights.to_rows(2.25), 4.5);
        assert_eq!(heights.to_rows(3.0), 6.0);
        assert_eq!(heights.to_rows(-1.0), 0.0);
        assert_eq!(heights.to_rows(9.0), 6.0);
    }

    #[test]
    fn rows_map_back_to_items() {
        let heights = mixed();
        assert_eq!(heights.to_items(2.5), 1.5);
        assert_eq!(heights.to_items(4.5), 2.25);
        assert_eq!(heights.to_items(1.0), 1.0);
        assert_eq!(heights.to_items(6.0), 3.0);
        assert_eq!(heights.to_items(-2.0), 0.0);
        assert_eq!(Heights::Uniform(4).to_items(2.5), 2.5);
        assert_eq!(Heights::Uniform(4).to_rows(2.5), 2.5);
    }

    /// Ten items, items 2-4 and 7-8 folded: item 2 is a fold row, 3 and 4 take no rows;
    /// item 7 is a fold row, 8 takes none. Display rows: 0 1 [2] 5 6 [7] 9.
    fn folded() -> Heights {
        Heights::Uniform(10).fold(&[2..5, 7..9])
    }

    #[test]
    fn a_fold_takes_one_row_for_all_its_items() {
        let heights = folded();
        assert_eq!((heights.items(), heights.rows()), (10, 7));
        assert_eq!(heights.of(1), 1..2);
        assert_eq!(heights.of(2), 2..3);
        assert_eq!(heights.of(3), 3..3);
        assert_eq!(heights.of(4), 3..3);
        assert_eq!(heights.of(5), 3..4);
        assert_eq!(heights.of(7), 5..6);
        assert_eq!(heights.of(8), 6..6);
        assert_eq!(heights.of(9), 6..7);
        assert_eq!(heights.span(0..10), 0..7);
        assert_eq!(heights.span(3..6), 3..4);
    }

    #[test]
    fn a_row_names_the_fold_or_the_item_after_it() {
        let heights = folded();
        let at = |row| heights.at(row);
        assert_eq!([0, 1, 2, 3, 4, 5, 6].map(at), [0, 1, 2, 5, 6, 7, 9].map(|i| Some((i, 0))));
        assert_eq!(at(7), None);
    }

    #[test]
    fn positions_through_a_fold_map_both_ways() {
        let heights = folded();
        assert_eq!(heights.to_rows(2.5), 2.5, "half way through the fold row");
        assert_eq!(heights.to_rows(3.5), 3.0, "a hidden item sits at the row after the fold");
        assert_eq!(heights.to_rows(5.5), 3.5);
        assert_eq!(heights.to_rows(10.0), 7.0);
        assert_eq!(heights.to_items(2.5), 2.5);
        assert_eq!(heights.to_items(3.5), 5.5);
        assert_eq!(heights.to_items(6.5), 9.5);
        assert_eq!(heights.to_items(7.0), 10.0);
    }

    #[test]
    fn a_fold_at_the_end_or_the_start_holds_its_first_item() {
        let end = Heights::Uniform(5).fold(&[3..5]);
        assert_eq!((end.rows(), end.at(3), end.at(4), end.of(4)), (4, Some((3, 0)), None, 4..4));
        let start = Heights::Uniform(5).fold(&[0..4]);
        assert_eq!((start.rows(), start.at(0), start.at(1)), (2, Some((0, 0)), Some((4, 0))));
    }

    #[test]
    fn a_wrapped_items_fold_takes_one_row() {
        // Items of 1, 3, 2 and 1 rows; items 1 and 2 fold into one row.
        let heights = Heights::from_rows([1, 3, 2, 1]).fold(&[1..3]);
        assert_eq!(heights, Heights::Starts(vec![0, 1, 2, 2, 3]));
        assert_eq!((heights.at(1), heights.at(2)), (Some((1, 0)), Some((3, 0))));
    }

    #[test]
    fn no_folds_leave_the_heights() {
        assert_eq!(Heights::Uniform(4).fold(&[]), Heights::Uniform(4));
        assert_eq!(mixed().fold(&[]), mixed());
    }
}
