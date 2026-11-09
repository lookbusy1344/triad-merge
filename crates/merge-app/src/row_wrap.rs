//! The top panes' rows under soft wrap (spec §5): each aligned row takes as many display rows
//! as its tallest cell wraps to, so the panes stay level. Each display row is one editor line
//! tall, and the list scrolls by them.

use std::collections::HashMap;
use std::ops::Range;

use gpui_kit::Pixels;
use merge_view::heights::Heights;
use merge_view::rows::PaneSet;

/// Where a wrapped cell's continuation rows start, in its tab-expanded text.
type Breaks = Vec<usize>;

/// What the rows were wrapped for. Another width, pane set or row map wraps them again.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fit {
    /// Each shown pane's text width.
    pub widths: Vec<Pixels>,
    pub panes: PaneSet,
    pub rows: usize,
}

pub(crate) struct RowWrap {
    heights: Heights,
    /// The breaks of each cell that wraps, by row and pane.
    breaks: HashMap<(usize, usize), Breaks>,
    /// What the heights were built for; `None` while unwrapped, and after the rows change.
    fitted: Option<Fit>,
}

impl RowWrap {
    pub(crate) fn new(rows: usize) -> Self {
        Self { heights: Heights::Uniform(rows), breaks: HashMap::new(), fitted: None }
    }

    pub(crate) fn heights(&self) -> &Heights {
        &self.heights
    }

    /// The rows changed: wrap them again at the next fit. Heights for another row count
    /// cannot stand in meanwhile, so the rows show one display row each until then.
    pub(crate) fn rows_changed(&mut self, rows: usize) {
        self.fitted = None;
        if self.heights.items() != rows {
            self.heights = Heights::Uniform(rows);
            self.breaks.clear();
        }
    }

    /// Show every row on one display row. Whether that changed the heights.
    pub(crate) fn unwrap(&mut self, rows: usize) -> bool {
        self.fitted = None;
        self.breaks.clear();
        let changed = self.heights != Heights::Uniform(rows);
        self.heights = Heights::Uniform(rows);
        changed
    }

    /// Wrap the rows for `fit`, unless they are wrapped for it already. `cells` gives each
    /// row's tab-expanded text in each pane, `None` on a filler; `breaks` wraps one text at
    /// one width. Whether the heights changed.
    pub(crate) fn fit<'a, C>(
        &mut self,
        fit: Fit,
        cells: impl Fn(usize) -> C,
        mut breaks: impl FnMut(&str, Pixels) -> Breaks,
    ) -> bool
    where
        C: IntoIterator<Item = Option<std::borrow::Cow<'a, str>>>,
    {
        if self.fitted.as_ref() == Some(&fit) {
            return false;
        }
        self.breaks.clear();
        let rows = (0..fit.rows).map(|row| {
            let mut tallest = 1;
            for (pane, text) in cells(row).into_iter().enumerate() {
                let Some((text, &width)) = text.zip(fit.widths.get(pane)) else {
                    continue;
                };
                let cell = breaks(&text, width);
                tallest = tallest.max(cell.len() + 1);
                if !cell.is_empty() {
                    self.breaks.insert((row, pane), cell);
                }
            }
            tallest
        });
        let heights = Heights::from_rows(rows);
        self.fitted = Some(fit);
        let changed = heights != self.heights;
        self.heights = heights;
        changed
    }

    /// The bytes of display row `sub` of `pane`'s cell on `row`, whose tab-expanded text is
    /// `text`; `None` past the cell's last segment.
    pub(crate) fn segment(
        &self,
        row: usize,
        pane: usize,
        sub: usize,
        text: &str,
    ) -> Option<Range<usize>> {
        let breaks = self.breaks.get(&(row, pane)).map_or(&[][..], |b| &b[..]);
        segment(text, breaks, sub)
    }
}

/// Segment `sub` of `text` broken before each of `breaks`. Breaks that do not fit the text,
/// as after an edit before the rows wrap again, leave it whole.
pub(crate) fn segment(text: &str, breaks: &[usize], sub: usize) -> Option<Range<usize>> {
    let fits = breaks.windows(2).all(|pair| pair[0] < pair[1])
        && breaks.iter().all(|&at| at > 0 && at < text.len() && text.is_char_boundary(at));
    let breaks = if fits { breaks } else { &[] };
    let start = match sub {
        0 => 0,
        n => *breaks.get(n - 1)?,
    };
    let end = breaks.get(sub).copied().unwrap_or(text.len());
    Some(start..end)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use gpui_kit::px;

    use super::*;

    const WIDTH: Pixels = px(100.);

    #[test]
    fn segments_run_between_the_breaks() {
        assert_eq!(segment("abcdefgh", &[3, 6], 0), Some(0..3));
        assert_eq!(segment("abcdefgh", &[3, 6], 1), Some(3..6));
        assert_eq!(segment("abcdefgh", &[3, 6], 2), Some(6..8));
        assert_eq!(segment("abcdefgh", &[3, 6], 3), None);
        assert_eq!(segment("abc", &[], 0), Some(0..3));
        assert_eq!(segment("", &[], 0), Some(0..0));
        assert_eq!(segment("abc", &[], 1), None);
    }

    #[test]
    fn breaks_that_do_not_fit_the_text_leave_it_whole() {
        assert_eq!(segment("abc", &[5], 0), Some(0..3));
        assert_eq!(segment("abc", &[5], 1), None);
        // Inside the two bytes of é.
        assert_eq!(segment("aéb", &[2], 0), Some(0..4));
        assert_eq!(segment("abcd", &[3, 2], 0), Some(0..4));
        assert_eq!(segment("abcd", &[0], 0), Some(0..4));
    }

    /// Breaks every `per` bytes, as a monospace wrap of `per` columns.
    fn every(per: usize) -> impl FnMut(&str, Pixels) -> Breaks {
        move |text, _| (per..text.len()).step_by(per).collect()
    }

    fn fit(rows: usize) -> Fit {
        Fit { widths: vec![WIDTH, WIDTH], panes: PaneSet::LeftRight, rows }
    }

    /// Two panes: row 0 is 5 and 12 bytes, row 1 a filler and 9, row 2 is 3 and 3.
    fn cells(row: usize) -> [Option<Cow<'static, str>>; 2] {
        let text = |n: usize| Some(Cow::Owned("x".repeat(n)));
        match row {
            0 => [text(5), text(12)],
            1 => [None, text(9)],
            _ => [text(3), text(3)],
        }
    }

    #[test]
    fn a_row_takes_the_rows_of_its_tallest_cell() {
        let mut wrap = RowWrap::new(3);
        assert!(wrap.fit(fit(3), cells, every(4)));
        // Row 0: 2 and 3 segments; row 1: 3; row 2: 1.
        assert_eq!(wrap.heights(), &Heights::Starts(vec![0, 3, 6, 7]));
        assert_eq!(wrap.segment(0, 1, 2, &"x".repeat(12)), Some(8..12));
        assert_eq!(wrap.segment(0, 0, 2, &"x".repeat(5)), None);
        assert_eq!(wrap.segment(2, 0, 0, "xxx"), Some(0..3));
    }

    #[test]
    fn the_same_fit_wraps_once_and_a_new_width_wraps_again() {
        let mut wrap = RowWrap::new(3);
        assert!(wrap.fit(fit(3), cells, every(4)));
        let mut calls = 0;
        let counting = |text: &str, width| {
            calls += 1;
            every(4)(text, width)
        };
        assert!(!wrap.fit(fit(3), cells, counting));
        assert_eq!(calls, 0);
        let wider = Fit { widths: vec![WIDTH * 2., WIDTH * 2.], ..fit(3) };
        assert!(wrap.fit(wider, cells, every(6)));
        // Row 0: 1 and 2 segments; row 1: 2; row 2: 1.
        assert_eq!(wrap.heights(), &Heights::Starts(vec![0, 2, 4, 5]));
    }

    #[test]
    fn changed_rows_unwrap_until_the_next_fit_when_their_count_changes() {
        let mut wrap = RowWrap::new(3);
        wrap.fit(fit(3), cells, every(4));
        wrap.rows_changed(3);
        assert_eq!(wrap.heights(), &Heights::Starts(vec![0, 3, 6, 7]), "same count");
        assert!(wrap.fit(fit(3), cells, every(100)));
        wrap.rows_changed(2);
        assert_eq!(wrap.heights(), &Heights::Uniform(2));
        assert_eq!(wrap.segment(0, 1, 1, &"x".repeat(12)), None);
    }

    #[test]
    fn unwrapping_gives_each_row_one_display_row() {
        let mut wrap = RowWrap::new(3);
        wrap.fit(fit(3), cells, every(4));
        assert!(wrap.unwrap(3));
        assert_eq!(wrap.heights(), &Heights::Uniform(3));
        assert!(!wrap.unwrap(3));
        // Unwrapped, the next fit wraps again.
        assert!(wrap.fit(fit(3), cells, every(4)));
    }
}
