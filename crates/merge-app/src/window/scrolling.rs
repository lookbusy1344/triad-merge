//! Scrolling, scroll sync, wrapping and folds of the rows.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, Context, IndentAdjustment, LineFragment, Pixels, ScrollEvent, Window, point, px,
};
use merge_view::heights::Heights;
use merge_view::merge_fold::{self, MergeFold};
use merge_view::rows::{RowIx, RowMap};
use merge_view::scroll;
use merge_view::scroll::{Displayed, Geometry, ResultRows};
use merge_view::tabs;

use crate::row_wrap::Fit;
use crate::wrap::Wrap;
use crate::{ToggleCollapseUnchanged, ToggleScrollSync, ToggleSoftWrap, ToggleWhitespace};

use super::{MergeWindow, SOFT_WRAP_MAX_LINES, result_heights, result_spacers};

impl MergeWindow {
    /// How far the rows are scrolled down.
    #[must_use]
    pub fn rows_scroll_top(&self) -> Pixels {
        self.scroll.top()
    }

    /// The height of the rows' view.
    #[must_use]
    pub fn rows_view_height(&self) -> Pixels {
        self.scroll.view_height()
    }

    /// Scroll the rows and the result each to its own lines of the region `go_to` set aside:
    /// centred when they fit, else their first line at the top (spec §5). Runs in prepaint,
    /// after the rows lay out and before the result does, so the rows' height is known and
    /// the result applies its top this frame. Before the result's first layout it waits a
    /// frame. Returns whether it placed the panes, which then need no sync this frame.
    pub(super) fn apply_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(region) = self.scroll.revealing(self.session.document().regions().len()) else {
            return false;
        };
        let (line_h, result_view, result_span) = {
            let state = self.result.read(cx);
            let Some(line_h) = state.line_height() else {
                window.refresh();
                return false;
            };
            let result = ResultRows::new(self.session.document(), &self.placeholders);
            let span = result_heights(result, self.scroll.wrap(), state, &self.collapse.folds)
                .span(result.span(region));
            (line_h, state.input_bounds().size.height, span)
        };
        let first = self.rows.first_row(region).get();
        let rows_top = scroll::reveal_top(
            self.scroll.heights().span(first..first + self.rows.rows_in(region)),
            f32::from(self.typography.line_height()),
            f32::from(self.rows_view_height()),
        );
        let result_top = scroll::reveal_top(result_span, f32::from(line_h), f32::from(result_view));
        // The caret sits at the start of a line, so the result goes back to the left edge, as
        // the caret's own scroll would take it.
        self.result.update(cx, |state, cx| {
            state.set_scroll_offset(point(Pixels::ZERO, -px(result_top)), cx);
        });
        self.scroll.revealed(rows_top, result_top, &self.panes.sources());
        // The rows drew this frame already.
        window.refresh();
        true
    }

    /// Copy the scroll position from the pane that moved since the last sync to the other,
    /// mapped by region (spec §5). Runs in prepaint on every frame, so it sees every source
    /// of scrolling: wheel, scrollbar and caret.
    pub(super) fn sync_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.scroll.syncing() {
            return;
        }
        let state = self.result.read(cx);
        let Some(line_h) = state.line_height() else {
            return;
        };
        let geometry = Geometry {
            row_h: f32::from(self.typography.line_height()),
            rows_view: f32::from(self.rows_view_height()),
            line_h: f32::from(line_h),
            result_view: f32::from(state.input_bounds().size.height),
        };
        let result_offset = state.scroll_offset();
        let (rows, result) =
            (&self.rows, ResultRows::new(self.session.document(), &self.placeholders));
        let sources = self.panes.sources();
        let wrap = self.scroll.wrap();
        let folds = &self.collapse.folds;
        // Built only when a pane moved: a wrapped result reads every line's display rows.
        let mapped = |map: fn(&RowMap, ResultRows, Displayed, Geometry, f32) -> f32| {
            move |row_heights: &Heights, top| {
                let displayed = Displayed {
                    rows: row_heights,
                    result: &result_heights(result, wrap, state, folds),
                };
                map(rows, result, displayed, geometry, top)
            }
        };
        let synced = self.scroll.sync(
            f32::from(-result_offset.y),
            f32::from(-result_offset.x),
            mapped(scroll::rows_to_result),
            mapped(scroll::result_to_rows),
            &sources,
        );
        if synced.result_left.is_some() || synced.result_top.is_some() {
            let offset = point(
                synced.result_left.map_or(result_offset.x, |left| -px(left)),
                synced.result_top.map_or(result_offset.y, |top| -px(top)),
            );
            self.result.update(cx, |state, cx| state.set_scroll_offset(offset, cx));
        }
        if synced.rows_moved {
            // The rows drew this frame already.
            window.refresh();
        }
    }

    /// Measure the top panes' columns, wrap their rows to the widths they laid out at while
    /// soft wrap is on, and hold their sideways scroll within those widths. Runs in prepaint,
    /// after the rows lay out.
    pub(super) fn fit_rows(&mut self, window: &mut Window, cx: &App) {
        let font = gpui_kit::font(self.typography.editor_font(cx.theme()));
        self.scroll.sideways_mut().measure(&font, self.typography.size, window);
        let old = self.scroll.heights().clone();
        let changed = if self.scroll.wrap().wraps() {
            self.wrap_rows(font, cx)
        } else {
            self.scroll.unwrap(self.rows.row_count())
        };
        if changed {
            let result_top = f32::from(-self.result.read(cx).scroll_offset().y);
            self.scroll.keep_place(&old, f32::from(self.typography.line_height()), result_top);
        }
        let left = self.scroll.sideways().left();
        if self.scroll.scroll_sideways_to(left, &self.panes.sources()) || changed {
            window.refresh();
        }
    }

    /// Wrap the top panes' rows to the widths their text laid out at in the last frame.
    /// Whether their heights changed.
    pub(super) fn wrap_rows(&mut self, font: gpui_kit::Font, cx: &App) -> bool {
        let panes = self.panes.sources().len();
        let Some(widths) = self.scroll.sideways().views().get(..panes).map(<[Pixels]>::to_vec)
        else {
            // Not laid out yet.
            return false;
        };
        let fit = Fit {
            widths,
            panes: self.panes,
            rows: self.rows.row_count(),
            whitespace: self.whitespace.shown(),
        };
        let (doc, rows, sources) = (self.session.document(), &self.rows, self.panes.sources());
        let drawing = self.drawing();
        let cells = |row: usize| {
            let cells = rows.row(doc, RowIx::new(row)).cells;
            sources
                .iter()
                .zip(cells)
                .map(|(&source, line)| {
                    line.map(|n| {
                        tabs::expand(doc.inputs().text(source).line(n), drawing).into_text()
                    })
                })
                .collect::<Vec<_>>()
        };
        let mut wrapper = cx.text_system().line_wrapper(font, self.typography.size);
        self.scroll.fit(fit, cells, |text, width| {
            if width <= Pixels::ZERO {
                return Vec::new();
            }
            wrapper
                .wrap_line(&[LineFragment::text(text)], width, IndentAdjustment::NoIndent)
                .map(|boundary| boundary.ix)
                .collect()
        })
    }

    /// How far the top panes' text is scrolled right.
    #[must_use]
    pub fn rows_scroll_left(&self) -> Pixels {
        self.scroll.sideways().left()
    }

    /// The top panes' display rows: how many each aligned row takes.
    #[must_use]
    pub fn row_heights(&self) -> &Heights {
        self.scroll.heights()
    }

    /// Scroll the top panes' text sideways by a wheel's horizontal move, unless the move is
    /// mostly vertical: the list scrolls by that.
    pub(super) fn wheel_sideways(
        &mut self,
        event: &ScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.x.abs() <= delta.y.abs() {
            return;
        }
        let left = self.scroll.sideways().left() - delta.x;
        if self.scroll.scroll_sideways_to(left, &self.panes.sources()) {
            cx.notify();
        }
    }

    /// Turn soft wrap in the result and the top panes on or off (spec §5, §6.4). Inputs past
    /// [`SOFT_WRAP_MAX_LINES`] never wrap.
    pub(super) fn toggle_soft_wrap(
        &mut self,
        _: &ToggleSoftWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wrap = self.scroll.toggle_wrap();
        if wrap == Wrap::TooLong {
            tracing::info!(max_lines = SOFT_WRAP_MAX_LINES, "the inputs are too long to wrap");
            return;
        }
        self.result.update(cx, |state, cx| state.set_soft_wrap(wrap.wraps(), window, cx));
        // The top panes wrap at the next prepaint.
        cx.notify();
    }

    /// Show or hide the whitespace in every pane (spec §5). The sideways extent counts the
    /// line endings' symbols, so it counts again.
    pub(super) fn toggle_whitespace(
        &mut self,
        _: &ToggleWhitespace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.whitespace = self.whitespace.toggled();
        let shown = self.whitespace.shown();
        self.result.update(cx, |state, cx| state.set_show_whitespaces(shown, window, cx));
        Self::count_widest(self.session.document(), self.drawing(), cx);
        cx.notify();
    }

    /// Collapse every unchanged region, or show them all (spec §5).
    pub(super) fn toggle_collapse_unchanged(
        &mut self,
        _: &ToggleCollapseUnchanged,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.collapse.on = !self.collapse.on;
        self.collapse.opened.clear();
        self.show_folds(cx);
        cx.notify();
    }

    /// Open the region `fold` hides lines of, in every pane.
    pub(super) fn open_fold(&mut self, fold: &MergeFold, cx: &mut Context<Self>) {
        self.collapse.opened.open(self.session.document(), fold);
        self.show_folds(cx);
        cx.notify();
    }

    /// Open the region that hides result line `line`, if one does.
    pub(super) fn open_fold_over_line(&mut self, line: usize, cx: &mut Context<Self>) {
        if let Some(fold) = merge_fold::fold_over_line(&self.collapse.folds, line).cloned() {
            self.open_fold(&fold, cx);
        }
    }

    /// Fold the regions the collapse gives: the result hides their lines behind a spacer row
    /// each, beside the placeholders, and the top panes their rows behind a fold row each.
    pub(super) fn show_folds(&mut self, cx: &mut Context<Self>) {
        let doc = self.session.document();
        let folds = self.collapse.folds(doc, &self.rows);
        let tones = *self.tones(cx);
        let (spacers, notes, placeholders) =
            result_spacers(doc, &self.inherited, &tones, &folds, cx.theme().muted_foreground);
        self.placeholders = placeholders;
        let hidden: Vec<_> = folds.iter().map(|fold| fold.result.clone()).collect();
        let changed = folds != self.collapse.folds;
        self.result.update(cx, |state, cx| {
            if changed {
                state.set_hidden_lines(hidden, cx);
            }
            state.set_spacers(spacers, cx);
            state.set_spacer_notes(notes, cx);
        });
        self.scroll.set_folds(
            folds.iter().map(|fold| fold.rows.clone()).collect(),
            f32::from(self.typography.line_height()),
        );
        self.collapse.folds = folds;
    }

    /// Turn scroll sync off or on (spec §5). Turned on, it brings the top panes to the
    /// result, down and sideways.
    pub(super) fn toggle_scroll_sync(
        &mut self,
        _: &ToggleScrollSync,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.scroll.toggle_sync();
        window.refresh();
    }
}
