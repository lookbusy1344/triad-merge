//! The display rows of a pane whose items each take one or more of them: the top panes'
//! aligned rows when they wrap, and the result's rows when its editor wraps (spec §5). Scroll
//! sync and jumps work in item positions; a pane scrolls by display rows.

use std::ops::Range;

/// How many display rows each item of a pane takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heights {
    /// This many items, one display row each.
    Uniform(usize),
    /// The first display row of each item, then the display row count.
    Starts(Vec<usize>),
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

    #[must_use]
    pub fn items(&self) -> usize {
        match self {
            Self::Uniform(items) => *items,
            Self::Starts(starts) => starts.len() - 1,
        }
    }

    /// The display rows of every item.
    #[must_use]
    pub fn rows(&self) -> usize {
        match self {
            Self::Uniform(items) => *items,
            Self::Starts(starts) => starts[starts.len() - 1],
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
            Self::Starts(starts) => starts.partition_point(|&start| start <= row) - 1,
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
}
