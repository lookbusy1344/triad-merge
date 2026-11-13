//! Scroll sync between the top panes and the result pane (spec §5). Each pane has an anchor:
//! a content position at the middle of its view, which moves to the top or bottom edge as the
//! pane nears its top or end. The panes line their anchors up by region: an anchor some share
//! of the way through a region's rows meets the same share of its result rows, its lines and
//! its placeholder rows.

use std::ops::Range;

use merge_core::document::MergeDocument;
use merge_core::index::{Line, RegionIx};

use crate::heights::Heights;
use crate::placeholder::PLACEHOLDER_ROWS;
use crate::rows::{RowIx, RowMap};

/// Scroll tops closer than this, in pixels, are equal, so a synced pane does not echo back.
pub const TOP_EPSILON: f32 = 0.5;

/// The scroll top a sync gives the pane that follows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Follow {
    Rows(f32),
    Result(f32),
}

/// The pane a sync moved, or both when the caller placed them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Follower {
    Rows,
    Result,
    Both,
}

/// The scroll tops of the rows and the result after the last sync.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScrollSync {
    rows: f32,
    result: f32,
    /// The pane the last sync moved. It holds its target from its next prepaint on, clamped
    /// to its own range; the next sync takes that top as synced.
    follower: Option<Follower>,
    /// The next step brings the rows to the result, wherever both stand.
    resync: bool,
    /// The next step brings the result to the rows, wherever both stand.
    rows_lead: bool,
}

impl ScrollSync {
    /// A sync whose first step brings the rows to the result: scroll sync turned on with the
    /// panes apart.
    #[must_use]
    pub fn resync() -> Self {
        Self { resync: true, ..Self::default() }
    }

    /// Take `rows` and `result` as synced: the caller scrolled each pane to its own top. Each
    /// holds its top from its next prepaint on, clamped to its own range, and the next step
    /// takes those tops as synced.
    pub fn place(&mut self, rows: f32, result: f32) {
        *self = Self { rows, result, follower: Some(Follower::Both), ..Self::default() };
    }

    /// Make the next step bring the result to the rows: the rows' display rows changed
    /// under them, and they kept their place.
    pub fn bring_result(&mut self) {
        *self = Self { rows_lead: true, ..Self::default() };
    }

    /// The move that keeps the panes together, given their tops now. After [`Self::resync`]
    /// the result leads; otherwise the pane that moved since the last sync leads, the rows
    /// first when both did; `to_result` and `to_rows`
    /// map its top to the other pane's.
    pub fn step(
        &mut self,
        rows: f32,
        result: f32,
        to_result: impl FnOnce(f32) -> f32,
        to_rows: impl FnOnce(f32) -> f32,
    ) -> Option<Follow> {
        // A follower that stopped short of its target hit its end; it did not move.
        match self.follower.take() {
            Some(Follower::Rows) => self.rows = rows,
            Some(Follower::Result) => self.result = result,
            Some(Follower::Both) => (self.rows, self.result) = (rows, result),
            None => {}
        }
        let moved = |now: f32, then: f32| (now - then).abs() > TOP_EPSILON;
        let rows_moved = std::mem::take(&mut self.rows_lead) || moved(rows, self.rows);
        let result_leads =
            std::mem::take(&mut self.resync) || !rows_moved && moved(result, self.result);
        if result_leads {
            let target = to_rows(result);
            *self =
                Self { rows: target, result, follower: Some(Follower::Rows), ..Self::default() };
            Some(Follow::Rows(target))
        } else if rows_moved {
            let target = to_result(rows);
            *self =
                Self { rows, result: target, follower: Some(Follower::Result), ..Self::default() };
            Some(Follow::Result(target))
        } else {
            None
        }
    }
}

/// `left` held within the scroll range of `content` pixels of text in a view `view` wide:
/// from 0 to the offset that shows the content's end at the view's right edge.
#[must_use]
pub fn clamp_left(left: f32, content: f32, view: f32) -> f32 {
    left.clamp(0.0, Pane { content, view }.max_top())
}

/// One pane's scroll extent, in pixels: the height of its content and of its view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pane {
    pub content: f32,
    pub view: f32,
}

impl Pane {
    /// The furthest the content scrolls to show its end at the bottom of the view.
    fn max_top(self) -> f32 {
        (self.content - self.view).max(0.0)
    }

    /// The scroll distance over which the anchor moves from an edge of the view to its
    /// middle: half the view, or half the scroll range when that is shorter.
    fn ramp(self) -> f32 {
        self.view.min(self.max_top()) / 2.0
    }

    /// The content position at the pane's anchor for scroll top `top`. The anchor sits at
    /// the middle of the view, and moves to the top edge as the pane nears its top and to
    /// the bottom edge as it nears its end, so the panes' tops and ends meet. A top past the
    /// end, into the space below the last line, anchors at the end.
    #[must_use]
    pub fn anchor(self, top: f32) -> f32 {
        let (end, ramp, view) = (self.max_top(), self.ramp(), self.view);
        if ramp <= 0.0 {
            return top.clamp(0.0, self.content);
        }
        if top >= end {
            return self.content;
        }
        let top = top.max(0.0);
        let share = if top < ramp {
            top / (2.0 * ramp)
        } else if top <= end - ramp {
            0.5
        } else {
            0.5 + (top - (end - ramp)) / (2.0 * ramp)
        };
        top + view * share
    }

    /// The scroll top whose anchor is at content position `anchor`: the inverse of
    /// [`Pane::anchor`], held to the scroll range.
    #[must_use]
    pub fn top_for(self, anchor: f32) -> f32 {
        let (end, ramp, view) = (self.max_top(), self.ramp(), self.view);
        if ramp <= 0.0 {
            return anchor.clamp(0.0, end);
        }
        let anchor = anchor.clamp(0.0, self.content);
        // Over a ramp, the anchor moves this much faster than the top.
        let rate = 1.0 + view / (2.0 * ramp);
        if anchor <= ramp * rate {
            anchor / rate
        } else if anchor <= end - ramp + view / 2.0 {
            anchor - view / 2.0
        } else {
            (anchor - view / 2.0 + view * (end - ramp) / (2.0 * ramp)) / rate
        }
    }
}

/// The result pane's display rows: its lines, and the placeholder rows among them.
#[derive(Debug, Clone, Copy)]
pub struct ResultRows<'a> {
    doc: &'a MergeDocument,
    /// The region of each placeholder, in order.
    placeholders: &'a [RegionIx],
}

impl<'a> ResultRows<'a> {
    /// The rows of `doc`'s result with `placeholders`, the region of each placeholder in
    /// order, as [`crate::placeholder::placeholders`] gives them.
    #[must_use]
    pub fn new(doc: &'a MergeDocument, placeholders: &'a [RegionIx]) -> Self {
        Self { doc, placeholders }
    }

    /// The display rows in the result.
    #[must_use]
    pub fn count(self) -> usize {
        self.doc.line_count() + self.placeholders.len() * PLACEHOLDER_ROWS
    }

    /// The display rows of `region`: its result lines and its placeholders.
    #[must_use]
    pub fn span(self, region: RegionIx) -> Range<usize> {
        let before = self.placeholders_before(region);
        let own = self.placeholders_before(region.next()) - before;
        let lines = result_lines(self.doc, region);
        let start = lines.start + before;
        start..start + lines.len() + own
    }

    /// The rows of `lines`, result lines of `region`.
    #[must_use]
    pub fn rows_of_lines(self, region: RegionIx, lines: &Range<usize>) -> Range<usize> {
        let shift = self.placeholders_before(region);
        lines.start + shift..lines.end + shift
    }

    /// The display rows of each row, in order: `line_rows` for each result line, and one
    /// for each placeholder row.
    #[must_use]
    pub fn heights(self, line_rows: impl Fn(usize) -> usize) -> Heights {
        let regions = (0..self.doc.regions().len()).map(RegionIx::new);
        let rows = regions.flat_map(|region| {
            let own = self.placeholders_before(region.next()) - self.placeholders_before(region);
            result_lines(self.doc, region).map(&line_rows).chain(std::iter::repeat_n(1, own))
        });
        Heights::from_rows(rows)
    }

    fn placeholders_before(self, region: RegionIx) -> usize {
        self.placeholders.partition_point(|&r| r < region) * PLACEHOLDER_ROWS
    }

    /// The region whose span holds display row position `row`, below [`ResultRows::count`].
    fn region_at(self, row: f32) -> RegionIx {
        // The last region starting at or before `row`; one with an empty span shares its
        // start with the next, so only the last region could be empty, and `row` is past it.
        let (mut low, mut high) = (0, self.doc.regions().len());
        while low < high {
            let middle = low + (high - low) / 2;
            if to_f32(self.span(RegionIx::new(middle)).start) <= row {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        RegionIx::new(low.saturating_sub(1))
    }
}

/// The display rows of the top panes' rows and of the result's rows.
#[derive(Debug, Clone, Copy)]
pub struct Displayed<'a> {
    pub rows: &'a Heights,
    pub result: &'a Heights,
}

/// Row heights and view heights of the two panes, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub row_h: f32,
    pub rows_view: f32,
    pub line_h: f32,
    pub result_view: f32,
}

impl Geometry {
    fn panes(self, displayed: Displayed) -> (Pane, Pane) {
        (
            Pane { content: to_f32(displayed.rows.rows()) * self.row_h, view: self.rows_view },
            Pane { content: to_f32(displayed.result.rows()) * self.line_h, view: self.result_view },
        )
    }
}

/// The result pane's scroll top for the top panes' `top` (spec §5): the result's anchor
/// goes to the place in its region that the rows' anchor holds in theirs. Each pane scrolls
/// by its display rows; a share of a row is the same share of its display rows.
#[must_use]
pub fn rows_to_result(
    rows: &RowMap,
    result: ResultRows,
    displayed: Displayed,
    geometry: Geometry,
    top: f32,
) -> f32 {
    let (rows_pane, result_pane) = geometry.panes(displayed);
    let row = displayed.rows.to_items(rows_pane.anchor(top) / geometry.row_h);
    let result_row = displayed.result.to_rows(result_row_at(rows, result, row));
    result_pane.top_for(result_row * geometry.line_h)
}

/// The top panes' scroll top for the result pane's `top`: the inverse of
/// [`rows_to_result`].
#[must_use]
pub fn result_to_rows(
    rows: &RowMap,
    result: ResultRows,
    displayed: Displayed,
    geometry: Geometry,
    top: f32,
) -> f32 {
    let (rows_pane, result_pane) = geometry.panes(displayed);
    let line = displayed.result.to_items(result_pane.anchor(top) / geometry.line_h);
    let row = displayed.rows.to_rows(row_at(rows, result, line));
    rows_pane.top_for(row * geometry.row_h)
}

/// The result display row position at the same share of its region as row position `row`.
/// The end of the rows maps to the end of the result.
#[must_use]
pub fn result_row_at(rows: &RowMap, result: ResultRows, row: f32) -> f32 {
    let count = rows.row_count();
    if row >= to_f32(count) {
        return to_f32(result.count());
    }
    let (whole, _) = split(row, count);
    let region = rows.region_of_row(RowIx::new(whole));
    let span = result.span(region);
    // A region holding a row has at least one. Multiplying first keeps whole shares exact.
    let within = row.max(0.0) - to_f32(rows.first_row(region).get());
    to_f32(span.start) + within * to_f32(span.len()) / to_f32(rows.rows_in(region))
}

/// The row position at the same share of its region as result display row position `row`.
/// A region with no rows holds the rows at the place it would start. The end of the result
/// maps to the end of the rows.
#[must_use]
pub fn row_at(rows: &RowMap, result: ResultRows, row: f32) -> f32 {
    if row >= to_f32(result.count()) {
        return to_f32(rows.row_count());
    }
    let row = row.max(0.0);
    let region = result.region_at(row);
    let span = result.span(region);
    // Below the count, the region found holds `row`, so its span is not empty.
    let within = row - to_f32(span.start);
    to_f32(rows.first_row(region).get())
        + within * to_f32(rows.rows_in(region)) / to_f32(span.len())
}

/// The row of result `line`, held to the last line; `None` without rows or lines.
#[must_use]
pub fn row_of_line(rows: &RowMap, doc: &MergeDocument, line: Line) -> Option<RowIx> {
    let (count, lines) = (rows.row_count(), doc.line_count());
    if count == 0 || lines == 0 {
        return None;
    }
    let line = line.min(Line::new(lines - 1));
    let region = doc.region_at_byte(doc.byte_of_line(line));
    let within = line.since(first_line(doc, region)).min(rows.rows_in(region).saturating_sub(1));
    Some(rows.first_row(region).plus(within).min(RowIx::new(count - 1)))
}

/// The scroll top that brings `rows`, each `row_h` tall, into a view `view` tall: centred
/// when they fit, else the first row at the top. Held at the top of the content; the pane
/// holds it at the bottom.
#[must_use]
pub fn reveal_top(rows: Range<usize>, row_h: f32, view: f32) -> f32 {
    let start = to_f32(rows.start) * row_h;
    let height = to_f32(rows.len()) * row_h;
    let top = if height <= view { start - (view - height) / 2.0 } else { start };
    top.max(0.0)
}

/// The whole and fractional parts of `position`, with the whole part held in `0..count`.
fn split(position: f32, count: usize) -> (usize, f32) {
    let position = position.max(0.0);
    let whole = position.floor();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a non-negative whole number, held below `count` next"
    )]
    let index = (whole as usize).min(count - 1);
    (index, position - whole)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "row and line counts stay far below 2^24 in practice; pixels need no more"
)]
fn to_f32(n: usize) -> f32 {
    n as f32
}

fn first_line(doc: &MergeDocument, region: RegionIx) -> Line {
    doc.line_of_byte(doc.region_start(region))
}

/// The result lines of `region`: from its first line up to the next region's first line, or
/// the end for the last.
#[must_use]
pub fn result_lines(doc: &MergeDocument, region: RegionIx) -> Range<usize> {
    let start = first_line(doc, region).get();
    let end = if region.next().get() < doc.regions().len() {
        first_line(doc, region.next()).get()
    } else {
        doc.line_count()
    };
    start..end.max(start)
}

#[cfg(test)]
mod tests {
    use merge_core::document::Command;
    use merge_core::merge3::Source;
    use merge_core::testing::doc;

    use super::*;

    #[test]
    fn a_left_offset_stays_within_the_content_past_the_view() {
        // 300 px of text in a 200 px view scrolls 0-100 px.
        assert_eq!(clamp_left(50.0, 300.0, 200.0), 50.0);
        assert_eq!(clamp_left(150.0, 300.0, 200.0), 100.0);
        assert_eq!(clamp_left(-5.0, 300.0, 200.0), 0.0);
        // Text that fits does not scroll.
        assert_eq!(clamp_left(20.0, 150.0, 200.0), 0.0);
        assert_eq!(clamp_left(20.0, 200.0, 200.0), 0.0);
    }

    #[test]
    fn a_resync_brings_the_rows_to_the_result_wherever_both_stand() {
        const ROWS_TOP: f32 = 40.;
        const RESULT_TOP: f32 = 90.;
        const RESULT_TO_ROWS: f32 = 3.;
        let mut sync = ScrollSync::resync();
        let to_rows = |top: f32| top / RESULT_TO_ROWS;
        let unused = |_: f32| -> f32 { panic!("the rows do not lead") };
        assert_eq!(
            sync.step(ROWS_TOP, RESULT_TOP, unused, to_rows),
            Some(Follow::Rows(RESULT_TOP / RESULT_TO_ROWS))
        );
        // The rows hold their target from their next prepaint; nothing moved since.
        let unmoved = |_: f32| -> f32 { panic!("no pane moved") };
        assert_eq!(sync.step(RESULT_TOP / RESULT_TO_ROWS, RESULT_TOP, unmoved, unmoved), None);
    }

    #[test]
    fn bring_result_makes_the_rows_lead_wherever_both_stand() {
        const ROWS_TOP: f32 = 40.;
        const RESULT_TOP: f32 = 90.;
        const ROWS_TO_RESULT: f32 = 2.;
        let mut sync = ScrollSync::default();
        sync.place(ROWS_TOP, RESULT_TOP);
        let unmoved = |_: f32| -> f32 { panic!("no pane moved") };
        assert_eq!(sync.step(ROWS_TOP, RESULT_TOP, unmoved, unmoved), None);
        sync.bring_result();
        let to_result = |top: f32| top * ROWS_TO_RESULT;
        let unused = |_: f32| -> f32 { panic!("the result does not lead") };
        assert_eq!(
            sync.step(ROWS_TOP, RESULT_TOP, to_result, unused),
            Some(Follow::Result(ROWS_TOP * ROWS_TO_RESULT))
        );
        assert_eq!(sync.step(ROWS_TOP, ROWS_TOP * ROWS_TO_RESULT, unmoved, unmoved), None);
    }

    #[test]
    fn a_follower_held_at_its_end_does_not_pull_the_leader_back() {
        // Sideways, both panes take the same offset; the result's text is shorter.
        let mut sync = ScrollSync::default();
        let same = |x| x;
        assert_eq!(sync.step(80.0, 0.0, same, same), Some(Follow::Result(80.0)));
        // The result stopped at its end, 30 px; the rows keep 80.
        assert_eq!(sync.step(80.0, 30.0, same, same), None);
        assert_eq!(sync.step(80.0, 30.0, same, same), None);
        // The result moving on its own leads again.
        assert_eq!(sync.step(80.0, 10.0, same, same), Some(Follow::Rows(10.0)));
    }

    /// The result is twice as tall as the rows.
    fn step(sync: &mut ScrollSync, rows: f32, result: f32) -> Option<Follow> {
        sync.step(rows, result, |top| top * 2.0, |top| top / 2.0)
    }

    /// The row height of the reveal tests.
    const ROW: f32 = 10.0;

    #[test]
    fn rows_that_fit_are_centred() {
        // 12 rows, 120 px, leave 80 px: 40 above and 40 below.
        assert_eq!(reveal_top(30..42, ROW, 200.0), 260.0);
    }

    #[test]
    fn rows_as_tall_as_the_view_start_at_the_top() {
        assert_eq!(reveal_top(30..50, ROW, 200.0), 300.0);
    }

    #[test]
    fn rows_taller_than_the_view_start_at_the_top() {
        assert_eq!(reveal_top(30..51, ROW, 200.0), 300.0);
        assert_eq!(reveal_top(30..530, ROW, 200.0), 300.0);
    }

    #[test]
    fn rows_near_the_top_hold_the_view_at_the_top() {
        assert_eq!(reveal_top(3..5, ROW, 200.0), 0.0);
    }

    #[test]
    fn no_rows_centre_as_a_point() {
        assert_eq!(reveal_top(30..30, ROW, 200.0), 200.0);
    }

    #[test]
    fn a_view_with_no_height_puts_the_start_at_the_top() {
        assert_eq!(reveal_top(30..31, ROW, 0.0), 300.0);
    }

    #[test]
    fn panes_at_rest_do_not_move() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 0.0, 0.0), None);
    }

    #[test]
    fn the_rows_lead_the_result() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 100.0, 0.0), Some(Follow::Result(200.0)));
        assert_eq!(step(&mut sync, 100.0, 200.0), None);
    }

    #[test]
    fn the_result_leads_the_rows() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 0.0, 200.0), Some(Follow::Rows(100.0)));
        assert_eq!(step(&mut sync, 100.0, 200.0), None);
    }

    #[test]
    fn the_rows_lead_when_both_moved() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 100.0, 50.0), Some(Follow::Result(200.0)));
    }

    #[test]
    fn a_move_of_the_epsilon_is_no_move_and_just_past_it_is() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, TOP_EPSILON, 0.0), None);
        assert_eq!(step(&mut sync, 0.75, 0.0), Some(Follow::Result(1.5)));
    }

    #[test]
    fn a_result_stopped_at_its_end_does_not_lead_the_rows_back() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 100.0, 0.0), Some(Follow::Result(200.0)));
        assert_eq!(step(&mut sync, 100.0, 150.0), None);
        assert_eq!(step(&mut sync, 100.0, 150.0), None);
    }

    #[test]
    fn rows_stopped_at_their_end_do_not_lead_the_result_back() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 0.0, 200.0), Some(Follow::Rows(100.0)));
        assert_eq!(step(&mut sync, 80.0, 200.0), None);
        assert_eq!(step(&mut sync, 80.0, 200.0), None);
    }

    #[test]
    fn a_follower_leads_again_once_its_stop_is_taken() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 100.0, 0.0), Some(Follow::Result(200.0)));
        assert_eq!(step(&mut sync, 100.0, 150.0), None);
        assert_eq!(step(&mut sync, 100.0, 120.0), Some(Follow::Rows(60.0)));
    }

    #[test]
    fn only_the_last_follower_is_excused_a_move() {
        let mut sync = ScrollSync::default();
        assert_eq!(step(&mut sync, 100.0, 0.0), Some(Follow::Result(200.0)));
        // The result is the follower; the rows moved on their own and still lead.
        assert_eq!(step(&mut sync, 110.0, 200.0), Some(Follow::Result(220.0)));
    }

    #[test]
    fn placed_panes_do_not_lead_each_other() {
        let mut sync = ScrollSync::default();
        sync.place(100.0, 400.0);
        assert_eq!(step(&mut sync, 100.0, 400.0), None);
        assert_eq!(step(&mut sync, 100.0, 400.0), None);
    }

    #[test]
    fn placed_panes_stopped_at_their_ends_take_those_tops() {
        let mut sync = ScrollSync::default();
        sync.place(100.0, 400.0);
        assert_eq!(step(&mut sync, 90.0, 350.0), None);
        assert_eq!(step(&mut sync, 90.0, 350.0), None);
    }

    #[test]
    fn a_pane_moved_after_a_placement_leads_again() {
        let mut sync = ScrollSync::default();
        sync.place(100.0, 400.0);
        assert_eq!(step(&mut sync, 100.0, 400.0), None);
        assert_eq!(step(&mut sync, 110.0, 400.0), Some(Follow::Result(220.0)));
        let mut sync = ScrollSync::default();
        sync.place(100.0, 400.0);
        assert_eq!(step(&mut sync, 100.0, 400.0), None);
        assert_eq!(step(&mut sync, 100.0, 420.0), Some(Follow::Rows(210.0)));
    }

    use crate::rows::PaneSet;

    const LINES: usize = 100;
    const CONFLICT_LINE: usize = 2;

    fn three(base: &str, left: &str, right: &str) -> (MergeDocument, RowMap) {
        let doc = doc(base, left, right);
        let rows = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        (doc, rows)
    }

    fn numbered(edit: Option<&str>) -> String {
        (0..LINES)
            .map(|n| match edit {
                Some(text) if n == CONFLICT_LINE => format!("{text}\n"),
                _ => format!("l{n}\n"),
            })
            .collect()
    }

    /// Line 2 conflicts. Rows: 0-1 unchanged, 2 the conflict, 3-99 unchanged. Result lines:
    /// 0-1, then 2-3 for the conflict (left, then right), then 4-100.
    fn conflict() -> (MergeDocument, RowMap) {
        three(&numbered(None), &numbered(Some("L")), &numbered(Some("R")))
    }

    fn placeholder_regions(doc: &MergeDocument) -> Vec<RegionIx> {
        crate::placeholder::placeholders(doc).iter().map(|p| p.region).collect()
    }

    /// Row position `row` as a result display row position.
    fn to_result(fixture: &(MergeDocument, RowMap), row: f32) -> f32 {
        let placeholders = placeholder_regions(&fixture.0);
        result_row_at(&fixture.1, ResultRows::new(&fixture.0, &placeholders), row)
    }

    /// Result display row position `row` as a row position.
    fn to_rows(fixture: &(MergeDocument, RowMap), row: f32) -> f32 {
        let placeholders = placeholder_regions(&fixture.0);
        row_at(&fixture.1, ResultRows::new(&fixture.0, &placeholders), row)
    }

    fn span(fixture: &(MergeDocument, RowMap), region: usize) -> Range<usize> {
        let placeholders = placeholder_regions(&fixture.0);
        ResultRows::new(&fixture.0, &placeholders).span(RegionIx::new(region))
    }

    #[test]
    fn a_conflicts_result_lines_hold_every_side() {
        // Lines 0-1, then the conflict's 2-3 (left, then right), then 4-100.
        let (doc, _) = conflict();
        assert_eq!(result_lines(&doc, RegionIx::new(0)), 0..2);
        assert_eq!(result_lines(&doc, RegionIx::new(1)), 2..4);
        assert_eq!(result_lines(&doc, RegionIx::new(2)), 4..101);
    }

    #[test]
    fn a_region_without_result_lines_is_empty_where_the_result_goes_on() {
        // Left deletes b. Result: a, c; the region holds no line, at line 1.
        let (doc, _) = three("a\nb\nc\n", "a\nc\n", "a\nb\nc\n");
        assert_eq!(result_lines(&doc, RegionIx::new(1)), 1..1);
    }

    #[test]
    fn rows_past_a_conflict_map_by_their_share_of_the_region() {
        // Region 2 takes rows 3-99 and result lines 4-100: 97 of each.
        assert_eq!(to_result(&conflict(), 5.0), 6.0);
        assert_eq!(to_result(&conflict(), 5.5), 6.5);
        assert_eq!(to_rows(&conflict(), 6.25), 5.25);
    }

    #[test]
    fn a_conflicts_row_spreads_over_its_result_lines() {
        // The conflict's one row, row 2, meets its two result lines, 2-3.
        assert_eq!(to_result(&conflict(), 2.25), 2.5);
        assert_eq!(to_result(&conflict(), 2.5), 3.0);
        assert_eq!(to_rows(&conflict(), 2.5), 2.25);
        assert_eq!(to_rows(&conflict(), 3.0), 2.5);
    }

    #[test]
    fn a_deleted_region_spans_its_placeholder_row() {
        // Left deletes b. Rows: a, b (right only), c. Result rows: a, the placeholder, c.
        let fixture = three("a\nb\nc\n", "a\nc\n", "a\nb\nc\n");
        assert_eq!(span(&fixture, 1), 1..2);
        assert_eq!(span(&fixture, 2), 2..3);
        assert_eq!(to_result(&fixture, 1.5), 1.5);
        assert_eq!(to_result(&fixture, 2.0), 2.0);
        assert_eq!(to_rows(&fixture, 1.5), 1.5);
    }

    #[test]
    fn result_lines_after_a_placeholder_take_the_row_after_it() {
        // Left deletes b. Result rows: a (line 0), the placeholder, c (line 1).
        let (doc, _) = three("a\nb\nc\n", "a\nc\n", "a\nb\nc\n");
        let placeholders = placeholder_regions(&doc);
        let result = ResultRows::new(&doc, &placeholders);
        assert_eq!(result.rows_of_lines(RegionIx::new(0), &(0..1)), 0..1);
        assert_eq!(result.rows_of_lines(RegionIx::new(2), &(1..2)), 2..3);
    }

    #[test]
    fn a_regions_rows_spread_over_fewer_result_lines() {
        // Left turns b, c into X. Region 1: rows 1-2 (X/b, filler/c), result line 1 (X).
        let fixture = three("a\nb\nc\n", "a\nX\n", "a\nb\nc\n");
        assert_eq!(span(&fixture, 1), 1..2);
        assert_eq!(to_result(&fixture, 2.0), 1.5);
        assert_eq!(to_rows(&fixture, 1.5), 2.0);
    }

    #[test]
    fn a_region_with_result_lines_but_no_rows_holds_the_rows_where_it_starts() {
        // Both sides delete b; picking base puts it back. Rows: a. Result rows: a, b and the
        // placeholder.
        let (mut doc, rows) = three("a\nb\n", "a\n", "a\n");
        doc.apply(Command::Toggle { region: RegionIx::new(1), source: Source::Base(0) })
            .expect("pick base");
        // The region keeps the placeholder of the sides' deletion beside base's line.
        assert_eq!(placeholder_regions(&doc), [RegionIx::new(1)]);
        let fixture = (doc, rows);
        assert_eq!(span(&fixture, 1), 1..3);
        assert_eq!(to_rows(&fixture, 1.5), 1.0);
        assert_eq!(to_result(&fixture, 0.5), 0.5);
    }

    #[test]
    fn positions_past_either_end_hold_to_that_end() {
        // 100 rows; 101 result rows.
        assert_eq!(to_result(&conflict(), 100.0), 101.0);
        assert_eq!(to_result(&conflict(), 150.0), 101.0);
        assert_eq!(to_rows(&conflict(), 101.0), 100.0);
        assert_eq!(to_rows(&conflict(), 500.0), 100.0);
        assert_eq!(to_result(&conflict(), -3.0), 0.0);
        assert_eq!(to_rows(&conflict(), -2.5), 0.0);
    }

    #[test]
    fn an_empty_document_stays_at_the_top() {
        let fixture = three("", "", "");
        assert_eq!((to_result(&fixture, 4.0), to_rows(&fixture, 4.0)), (0.0, 0.0));
    }

    /// 1000 px of content in a 100 px view: the top runs 0-900, with ramps 50 px long.
    const LONG: Pane = Pane { content: 1000.0, view: 100.0 };

    #[test]
    fn the_anchor_runs_from_the_top_edge_through_the_middle_to_the_bottom_edge() {
        assert_eq!(LONG.anchor(0.0), 0.0);
        // A quarter of the view down, half way along the top ramp.
        assert_eq!(LONG.anchor(25.0), 50.0);
        assert_eq!(LONG.anchor(50.0), 100.0);
        assert_eq!(LONG.anchor(400.0), 450.0);
        assert_eq!(LONG.anchor(850.0), 900.0);
        // Three quarters of the view down, half way along the bottom ramp.
        assert_eq!(LONG.anchor(875.0), 950.0);
        assert_eq!(LONG.anchor(900.0), 1000.0);
    }

    #[test]
    fn a_top_past_either_end_anchors_at_that_end() {
        assert_eq!(LONG.anchor(-10.0), 0.0);
        assert_eq!(LONG.anchor(950.0), 1000.0);
    }

    #[test]
    fn the_top_for_an_anchor_inverts_it() {
        assert_eq!(LONG.top_for(0.0), 0.0);
        assert_eq!(LONG.top_for(50.0), 25.0);
        assert_eq!(LONG.top_for(100.0), 50.0);
        assert_eq!(LONG.top_for(450.0), 400.0);
        assert_eq!(LONG.top_for(900.0), 850.0);
        assert_eq!(LONG.top_for(950.0), 875.0);
        assert_eq!(LONG.top_for(1000.0), 900.0);
    }

    #[test]
    fn the_top_for_an_anchor_past_either_end_is_that_end() {
        assert_eq!(LONG.top_for(-5.0), 0.0);
        assert_eq!(LONG.top_for(1200.0), 900.0);
    }

    #[test]
    fn content_shorter_than_two_views_ramps_over_its_whole_range() {
        // The top runs 0-50; the anchor runs 0-150, three times as fast.
        let short = Pane { content: 150.0, view: 100.0 };
        assert_eq!(short.anchor(10.0), 30.0);
        assert_eq!(short.anchor(25.0), 75.0);
        assert_eq!(short.anchor(50.0), 150.0);
        assert_eq!(short.top_for(75.0), 25.0);
        assert_eq!(short.top_for(30.0), 10.0);
    }

    #[test]
    fn content_that_fits_stays_at_the_top() {
        let fits = Pane { content: 80.0, view: 100.0 };
        assert_eq!(fits.anchor(0.0), 0.0);
        assert_eq!(fits.top_for(40.0), 0.0);
    }

    #[test]
    fn a_pane_not_laid_out_anchors_at_its_top() {
        let hidden = Pane { content: 1000.0, view: 0.0 };
        assert_eq!(hidden.anchor(30.0), 30.0);
        assert_eq!(hidden.top_for(30.0), 30.0);
    }

    /// Rows 20 px and lines 16 px, each pane 10 of them tall. Rows run 0-1800 px, with
    /// ramps 100 px long; the result runs 0-1456 px, with ramps 80 px long.
    const GEOMETRY: Geometry =
        Geometry { row_h: 20.0, rows_view: 200.0, line_h: 16.0, result_view: 160.0 };

    fn sync_tops(rows_top: f32) -> (f32, f32) {
        let (doc, rows) = conflict();
        let result = ResultRows::new(&doc, &[]);
        let heights = (Heights::Uniform(rows.row_count()), Heights::Uniform(result.count()));
        sync_wrapped(&rows, result, &heights, rows_top)
    }

    /// The result's top for `rows_top`, and the rows' top back from it.
    fn sync_wrapped(
        rows: &RowMap,
        result: ResultRows,
        heights: &(Heights, Heights),
        rows_top: f32,
    ) -> (f32, f32) {
        let displayed = Displayed { rows: &heights.0, result: &heights.1 };
        let to_result = rows_to_result(rows, result, displayed, GEOMETRY, rows_top);
        (to_result, result_to_rows(rows, result, displayed, GEOMETRY, to_result))
    }

    #[test]
    fn wrapped_rows_meet_the_result_by_their_share_of_display_rows() {
        // The conflict's row 2 wraps to four display rows, 2-5. Rows top 40 on the ramp:
        // anchor 80 px, display row 4, half way through row 2. Half way through the
        // conflict's result lines 2-3 is line 3, 48 px, top 24 on the result's ramp.
        let (doc, rows) = conflict();
        let result = ResultRows::new(&doc, &[]);
        let row_heights = (0..rows.row_count()).map(|row| if row == 2 { 4 } else { 1 });
        let heights = (Heights::from_rows(row_heights), Heights::Uniform(result.count()));
        assert_eq!(sync_wrapped(&rows, result, &heights, 40.0), (24.0, 40.0));
    }

    #[test]
    fn a_wrapped_result_meets_the_rows_by_its_share_of_display_rows() {
        // Result line 3 (the conflict's right side) wraps to three display rows, 3-5.
        // Result top 40 on the ramp: anchor 80 px, display row 5, two thirds through line 3,
        // so line 3.67: past the conflict's middle, row 2.83, 56.67 px, rows top 28.33.
        let (doc, rows) = conflict();
        let result = ResultRows::new(&doc, &[]);
        let heights = (
            Heights::Uniform(rows.row_count()),
            result.heights(|line| if line == 3 { 3 } else { 1 }),
        );
        let displayed = Displayed { rows: &heights.0, result: &heights.1 };
        let rows_top = result_to_rows(&rows, result, displayed, GEOMETRY, 40.0);
        assert!((rows_top - 85.0 / 3.0).abs() < 1e-4, "{rows_top}");
        assert_eq!(heights.1.rows(), result.count() + 2);
    }

    #[test]
    fn result_heights_give_placeholders_one_row_each_in_their_place() {
        // Left deletes b. Result items: a, the deletion's placeholder, c.
        let (doc, _) = three("a\nb\nc\n", "a\nc\n", "a\nb\nc\n");
        let placeholders = placeholder_regions(&doc);
        assert_eq!(placeholders, [RegionIx::new(1)]);
        let heights = ResultRows::new(&doc, &placeholders).heights(|line| line + 2);
        // a: 2 rows, the placeholder: 1, c: 3.
        assert_eq!(heights, Heights::Starts(vec![0, 2, 3, 6]));
    }

    #[test]
    fn the_panes_meet_at_their_tops() {
        assert_eq!(sync_tops(0.0), (0.0, 0.0));
    }

    #[test]
    fn the_panes_meet_at_their_anchors_on_the_top_ramp() {
        // Rows top 50: anchor 100, row 5, result row 6, 96 px; on the result's ramp that
        // is top 48.
        assert_eq!(sync_tops(50.0), (48.0, 50.0));
    }

    #[test]
    fn the_panes_meet_at_the_middles_of_their_views() {
        // Rows top 1000: anchor 1100, row 55, result row 56, 896 px; top 896 - 80.
        assert_eq!(sync_tops(1000.0), (816.0, 1000.0));
    }

    #[test]
    fn the_panes_meet_at_their_ends() {
        assert_eq!(sync_tops(1800.0), (1456.0, 1800.0));
    }

    #[test]
    fn a_line_past_the_end_holds_to_the_last_lines_row_before_an_empty_last_region() {
        // Left deletes b. Rows: a, b (right only). Result: a. The last region has no lines.
        let (doc, rows) = three("a\nb\n", "a\n", "a\nb\n");
        assert_eq!((rows.row_count(), doc.line_count()), (2, 1));
        assert_eq!(row_of_line(&rows, &doc, Line::new(5)), Some(RowIx::new(0)));
    }

    #[test]
    fn a_result_line_maps_to_its_row() {
        // Line 0 is row 0; line 3 sits in the conflict's lines 2-3, on row 2; line 11 is
        // the eighth line of the region at row 3 and line 4.
        let (doc, rows) = conflict();
        assert_eq!(row_of_line(&rows, &doc, Line::new(0)), Some(RowIx::new(0)));
        assert_eq!(row_of_line(&rows, &doc, Line::new(3)), Some(RowIx::new(2)));
        assert_eq!(row_of_line(&rows, &doc, Line::new(11)), Some(RowIx::new(10)));
    }

    #[test]
    fn a_line_past_the_end_maps_to_the_last_row() {
        let (doc, rows) = conflict();
        assert_eq!(row_of_line(&rows, &doc, Line::new(100)), Some(RowIx::new(99)));
        assert_eq!(row_of_line(&rows, &doc, Line::new(500)), Some(RowIx::new(99)));
    }

    #[test]
    fn a_line_after_a_deleted_region_maps_past_its_rows() {
        // Rows: a, b (right only), c. Result: a, c. Line 1 is c, on row 2.
        let (doc, rows) = three("a\nb\nc\n", "a\nc\n", "a\nb\nc\n");
        assert_eq!(row_of_line(&rows, &doc, Line::new(1)), Some(RowIx::new(2)));
    }

    #[test]
    fn no_lines_or_no_rows_map_to_no_row() {
        let (doc, rows) = three("", "", "");
        assert_eq!(row_of_line(&rows, &doc, Line::new(0)), None);
        let (doc, rows) = three("a\n", "", "a\n");
        assert_eq!(row_of_line(&rows, &doc, Line::new(0)), None);
    }
}
