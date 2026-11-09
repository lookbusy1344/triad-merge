//! The merge window's scroll (spec §5): the top panes' rows, down by display row while long
//! lines wrap and sideways through their text; soft wrap; the region `go_to` brings into
//! view; and the syncs that tie the rows to the result.

use gpui_kit::{Pixels, ScrollStrategy, UniformListScrollHandle, point, px};
use merge_core::index::RegionIx;
use merge_core::merge3::Source;
use merge_view::heights::Heights;
use merge_view::rows::RowIx;
use merge_view::scroll::{Follow, ScrollSync};

use crate::row_wrap::RowWrap;
use crate::sideways::Sideways;
use crate::wrap::Wrap;

/// Scroll sync between the rows and the result, down and sideways.
#[derive(Default)]
struct ScrollSyncs {
    down: ScrollSync,
    sideways: ScrollSync,
}

impl ScrollSyncs {
    /// Syncs whose first step brings the rows to the result.
    fn resync() -> Self {
        Self { down: ScrollSync::resync(), sideways: ScrollSync::resync() }
    }
}

/// What a sync step asks of the result, and whether the rows moved.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Synced {
    /// The result's new scroll top; `None` leaves it.
    pub(crate) result_top: Option<f32>,
    /// The result's new scroll left; `None` leaves it.
    pub(crate) result_left: Option<f32>,
    /// The rows moved, down or sideways, after they drew this frame.
    pub(crate) rows_moved: bool,
}

/// The merge window's scroll state.
pub(crate) struct PaneScroll {
    list: UniformListScrollHandle,
    /// The rows' sideways scroll and each pane's text width.
    sideways: Sideways,
    /// The display rows each aligned row takes: one, more while it wraps.
    row_wrap: RowWrap,
    /// Whether the result and the rows wrap long lines.
    wrap: Wrap,
    /// Where the panes stood at the last sync; `None` with scroll sync off.
    syncs: Option<ScrollSyncs>,
    /// The region `go_to` moved to, which both panes bring into view once laid out.
    reveal: Option<RegionIx>,
}

impl PaneScroll {
    /// The scroll of `rows` aligned rows, at the top and left.
    pub(crate) fn new(rows: usize, wrap: Wrap, sync: bool) -> Self {
        Self {
            list: UniformListScrollHandle::new(),
            sideways: Sideways::default(),
            row_wrap: RowWrap::new(rows),
            wrap,
            syncs: sync.then(ScrollSyncs::default),
            reveal: None,
        }
    }

    /// The list's handle, which the rows' list tracks.
    pub(crate) fn list(&self) -> &UniformListScrollHandle {
        &self.list
    }

    pub(crate) fn sideways(&self) -> &Sideways {
        &self.sideways
    }

    pub(crate) fn sideways_mut(&mut self) -> &mut Sideways {
        &mut self.sideways
    }

    pub(crate) fn row_wrap(&self) -> &RowWrap {
        &self.row_wrap
    }

    pub(crate) fn row_wrap_mut(&mut self) -> &mut RowWrap {
        &mut self.row_wrap
    }

    /// The display rows each aligned row takes.
    pub(crate) fn heights(&self) -> &Heights {
        self.row_wrap.heights()
    }

    pub(crate) fn wrap(&self) -> Wrap {
        self.wrap
    }

    /// Turn soft wrap on or off; inputs too long to wrap stay unwrapped. The new state.
    pub(crate) fn toggle_wrap(&mut self) -> Wrap {
        self.wrap = self.wrap.toggled();
        self.wrap
    }

    pub(crate) fn syncing(&self) -> bool {
        self.syncs.is_some()
    }

    /// Turn scroll sync off or on. Turned on, its first step brings the rows to the result.
    pub(crate) fn toggle_sync(&mut self) {
        self.syncs = self.syncs.is_none().then(ScrollSyncs::resync);
    }

    /// How far the rows are scrolled down.
    pub(crate) fn top(&self) -> Pixels {
        -self.list.0.borrow().base_handle.offset().y
    }

    /// Scroll the rows down to `top`, keeping their sideways offset.
    pub(crate) fn set_top(&self, top: f32) {
        let handle = &self.list.0.borrow().base_handle;
        handle.set_offset(point(handle.offset().x, -px(top)));
    }

    /// The height of the rows' view.
    pub(crate) fn view_height(&self) -> Pixels {
        self.list.0.borrow().base_handle.bounds().size.height
    }

    /// Scroll aligned row `row`'s first display row to the middle of the view.
    pub(crate) fn centre(&self, row: RowIx) {
        let display = self.heights().of(row.get()).start;
        self.list.scroll_to_item(display, ScrollStrategy::Center);
    }

    /// The first aligned row in view, with display rows `row_h` high.
    pub(crate) fn top_row(&self, row_h: Pixels) -> RowIx {
        let top = self.heights().to_items(f32::from(self.top()) / f32::from(row_h));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a non-negative whole number of rows"
        )]
        RowIx::new(top.max(0.).floor() as usize)
    }

    /// Move the text sideways to `left`, held within the range of the panes showing
    /// `sources`; wrapped text stays at the left edge. Whether it moved.
    pub(crate) fn scroll_sideways_to(&mut self, left: Pixels, sources: &[Source]) -> bool {
        let left = if self.wrap.wraps() { Pixels::ZERO } else { left };
        self.sideways.scroll_to(left, sources)
    }

    /// Bring `region` into view once the panes lay out.
    pub(crate) fn reveal(&mut self, region: RegionIx) {
        self.reveal = Some(region);
    }

    /// The region waiting to come into view, of a document with `regions` regions. A region
    /// an edit merged away is dropped.
    pub(crate) fn revealing(&mut self, regions: usize) -> Option<RegionIx> {
        self.reveal = self.reveal.filter(|region| region.get() < regions);
        self.reveal
    }

    /// The waiting region is in view: the rows at `rows_top` and the left edge of the panes
    /// showing `sources`, the result at `result_top`. The syncs take both as they stand, so
    /// neither pane follows the other.
    pub(crate) fn revealed(&mut self, rows_top: f32, result_top: f32, sources: &[Source]) {
        self.reveal = None;
        self.set_top(rows_top);
        self.sideways.scroll_to(Pixels::ZERO, sources);
        if let Some(syncs) = &mut self.syncs {
            syncs.down.place(rows_top, result_top);
            syncs.sideways.place(0.0, 0.0);
        }
    }

    /// Copy the scroll position from the pane that moved since the last step to the other
    /// (spec §5). The result stands at `result_top` and `result_left`; `to_result` and
    /// `to_rows` map a top between the panes, given the rows' display rows. The rows move here; the result's moves come
    /// back. Sideways both take the same offset, the rows' held within the range of the
    /// panes showing `sources`, and at the left edge while they wrap.
    pub(crate) fn sync(
        &mut self,
        result_top: f32,
        result_left: f32,
        to_result: impl FnOnce(&Heights, f32) -> f32,
        to_rows: impl FnOnce(&Heights, f32) -> f32,
        sources: &[Source],
    ) -> Synced {
        let rows_top = f32::from(self.top());
        let Some(syncs) = &mut self.syncs else {
            return Synced::default();
        };
        let heights = self.row_wrap.heights();
        let follow = syncs.down.step(
            rows_top,
            result_top,
            |top| to_result(heights, top),
            |top| to_rows(heights, top),
        );
        // The rows take the result's left held within their range; the sync finds where
        // they stopped at its next step.
        let follow_left =
            syncs.sideways.step(f32::from(self.sideways.left()), result_left, |l| l, |l| l);
        let mut synced = Synced::default();
        match follow {
            Some(Follow::Result(top)) => synced.result_top = Some(top),
            Some(Follow::Rows(top)) => {
                self.set_top(top);
                synced.rows_moved = true;
            }
            None => {}
        }
        match follow_left {
            Some(Follow::Result(left)) => synced.result_left = Some(left),
            Some(Follow::Rows(left)) => {
                synced.rows_moved |= self.scroll_sideways_to(px(left), sources);
            }
            None => {}
        }
        synced
    }

    /// Keep the rows on the same aligned row after their display rows changed from `old`,
    /// each `row_h` high. The down sync takes the new top with the result at `result_top`,
    /// so the result does not follow.
    pub(crate) fn keep_place(&mut self, old: &Heights, row_h: f32, result_top: f32) {
        let item = old.to_items(f32::from(self.top()) / row_h);
        let top = self.heights().to_rows(item) * row_h;
        self.set_top(top);
        if let Some(syncs) = &mut self.syncs {
            syncs.down.place(top, result_top);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW_H: Pixels = px(20.);
    const LEFT: Source = Source::Side(0);
    const SOURCES: [Source; 1] = [LEFT];
    /// One column's width.
    const ADVANCE: Pixels = px(10.);
    /// A pane 100 px wide over a 30-column line scrolls up to 200 px.
    const VIEW: Pixels = px(100.);
    const COLUMNS: usize = 30;
    const MAX_LEFT: f32 = 200.;

    fn double(_: &Heights, top: f32) -> f32 {
        top * 2.
    }

    fn half(_: &Heights, top: f32) -> f32 {
        top / 2.
    }

    /// Ten rows, synced, with a measured pane; both panes at the top and left.
    fn synced(wrap: Wrap) -> PaneScroll {
        let mut scroll = PaneScroll::new(10, wrap, true);
        *scroll.sideways_mut() = Sideways::measured(vec![(LEFT, COLUMNS)], ADVANCE, vec![VIEW]);
        assert_eq!(scroll.sync(0., 0., double, half, &SOURCES), Synced::default());
        scroll
    }

    #[test]
    fn the_top_row_counts_the_whole_rows_scrolled_past() {
        let scroll = PaneScroll::new(10, Wrap::Off, true);
        assert_eq!(scroll.top_row(ROW_H), RowIx::new(0));
        scroll.set_top(45.);
        assert_eq!(scroll.top(), px(45.));
        assert_eq!(scroll.top_row(ROW_H), RowIx::new(2));
        scroll.set_top(55.);
        assert_eq!(scroll.top_row(ROW_H), RowIx::new(2), "row 2 is three quarters past");
        scroll.set_top(60.);
        assert_eq!(scroll.top_row(ROW_H), RowIx::new(3), "row 3 starts at the top");
    }

    #[test]
    fn the_result_follows_rows_that_moved() {
        let mut scroll = synced(Wrap::Off);
        scroll.set_top(30.);
        let synced = scroll.sync(0., 0., double, half, &SOURCES);
        assert_eq!(synced, Synced { result_top: Some(60.), ..Synced::default() });
        assert_eq!(scroll.sync(60., 0., double, half, &SOURCES), Synced::default(), "settled");
    }

    #[test]
    fn the_rows_follow_a_result_that_moved_down_and_sideways() {
        let mut scroll = synced(Wrap::Off);
        let synced = scroll.sync(80., 50., double, half, &SOURCES);
        assert_eq!(synced, Synced { rows_moved: true, ..Synced::default() });
        assert_eq!((scroll.top(), scroll.sideways().left()), (px(40.), px(50.)));
    }

    #[test]
    fn the_rows_follow_sideways_only_within_their_range() {
        let mut scroll = synced(Wrap::Off);
        scroll.sync(0., 500., double, half, &SOURCES);
        assert_eq!(scroll.sideways().left(), px(MAX_LEFT));
    }

    #[test]
    fn wrapped_rows_stay_at_the_left_edge() {
        let mut scroll = synced(Wrap::On);
        let synced = scroll.sync(0., 50., double, half, &SOURCES);
        assert_eq!(synced, Synced::default());
        assert_eq!(scroll.sideways().left(), Pixels::ZERO);
    }

    #[test]
    fn without_sync_neither_pane_follows() {
        let mut scroll = synced(Wrap::Off);
        scroll.toggle_sync();
        assert!(!scroll.syncing());
        scroll.set_top(30.);
        assert_eq!(scroll.sync(80., 50., double, half, &SOURCES), Synced::default());
        assert_eq!((scroll.top(), scroll.sideways().left()), (px(30.), Pixels::ZERO));
    }

    #[test]
    fn sync_turned_on_brings_the_rows_to_the_result() {
        let mut scroll = PaneScroll::new(10, Wrap::Off, false);
        *scroll.sideways_mut() = Sideways::measured(vec![(LEFT, COLUMNS)], ADVANCE, vec![VIEW]);
        scroll.set_top(30.);
        scroll.toggle_sync();
        let synced = scroll.sync(80., 50., double, half, &SOURCES);
        assert_eq!(synced, Synced { rows_moved: true, ..Synced::default() });
        assert_eq!((scroll.top(), scroll.sideways().left()), (px(40.), px(50.)));
    }

    #[test]
    fn a_revealed_region_leaves_both_panes_where_it_put_them() {
        let mut scroll = synced(Wrap::Off);
        scroll.scroll_sideways_to(px(50.), &SOURCES);
        scroll.sync(0., 50., double, half, &SOURCES);
        scroll.reveal(RegionIx::new(3));
        assert_eq!(scroll.revealing(4), Some(RegionIx::new(3)));
        scroll.revealed(100., 70., &SOURCES);
        assert_eq!(scroll.revealing(4), None);
        assert_eq!((scroll.top(), scroll.sideways().left()), (px(100.), Pixels::ZERO));
        let synced = scroll.sync(70., 0., double, half, &SOURCES);
        assert_eq!(synced, Synced::default(), "neither pane follows");
    }

    #[test]
    fn a_region_merged_away_is_not_revealed() {
        let mut scroll = synced(Wrap::Off);
        scroll.reveal(RegionIx::new(3));
        assert_eq!(scroll.revealing(3), None);
        assert_eq!(scroll.revealing(4), None, "dropped, not waiting");
    }

    #[test]
    fn rows_keep_their_place_when_their_display_rows_change() {
        let mut scroll = synced(Wrap::Off);
        // Row 0 took three display rows; the rows stood at display row 3, aligned row 1.
        let old = Heights::from_rows([3, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
        scroll.set_top(60.);
        scroll.keep_place(&old, f32::from(ROW_H), 0.);
        assert_eq!(scroll.top(), px(20.));
        assert_eq!(scroll.sync(0., 0., double, half, &SOURCES), Synced::default(), "no follow");
    }

    #[test]
    fn too_long_inputs_never_wrap() {
        let mut scroll = PaneScroll::new(10, Wrap::TooLong, true);
        assert_eq!(scroll.toggle_wrap(), Wrap::TooLong);
        let mut scroll = PaneScroll::new(10, Wrap::Off, true);
        assert_eq!(scroll.toggle_wrap(), Wrap::On);
        assert_eq!(scroll.toggle_wrap(), Wrap::Off);
    }
}
