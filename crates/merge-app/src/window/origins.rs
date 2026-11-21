//! Line origins: loading them, the origin column and the result's labels (spec §7.3).

use std::time::Duration;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AppContext as _, Context, SharedString, Window};
use merge_core::document::MergeDocument;
use merge_core::index::Line;
use merge_core::merge3::Source;
use merge_core::text::Text;
use merge_session::origin;
use merge_view::origin::hover_text;
use merge_view::rows::RowIx;
use vcs::VcsError;

use crate::origin_column::{Annotated, Described, OriginCell};
use crate::{ToggleOriginColumn, ignore_closed_view};

use super::{MergeWindow, OriginLookup};

impl MergeWindow {
    /// Find and annotate the inputs off the UI thread; they show their origins when that
    /// finishes. Only a three-way merge has the base, left and right a VCS run names.
    pub(super) fn load_origins(
        lookup: OriginLookup,
        doc: &MergeDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Texts share their content, so the copy is O(1).
        let inputs = doc.inputs().clone();
        cx.spawn_in(window, async move |this, cx| {
            let annotations = cx
                .background_spawn(async move {
                    let found = lookup.find(&inputs)?;
                    let texts: Vec<&Text> = origin::merge_inputs(&inputs).map(|(_, t)| t).collect();
                    Ok::<_, VcsError>((found.annotate(&texts), found.describe()))
                })
                .await;
            ignore_closed_view(this.update(cx, |this, cx| this.loaded_origins(annotations, cx)));
        })
        .detach();
    }

    pub(super) fn loaded_origins(
        &mut self,
        found: Result<(Annotated, Described), VcsError>,
        cx: &mut Context<Self>,
    ) {
        let (annotations, described) = match found {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(%error, "cannot find the inputs' line origins; none show");
                return;
            }
        };
        let sources = || origin::merge_inputs(self.session.document().inputs()).map(|(s, _)| s);
        let sources: Vec<Source> = sources().collect();
        self.origin_column.described(sources.iter().copied(), described);
        self.origin_column.annotated(sources, annotations);
        self.refresh_result_labels(None, cx);
        cx.notify();
    }

    /// The revision chip of `pane`'s heading: none for a pane whose input shows no commit.
    #[must_use]
    pub fn revision_chip(&self, pane: usize) -> Option<OriginCell> {
        self.heading_text(pane)?.chip
    }

    /// The origin column of `pane` on `row`: none for a filler row, a row or pane the window
    /// lacks, or a line without an origin.
    #[must_use]
    pub fn origin_cell(&self, pane: usize, row: RowIx) -> Option<OriginCell> {
        let source = *self.panes.sources().get(pane)?;
        let cell = *self.rows.get_row(self.session.document(), row)?.cells.get(pane)?;
        self.origin_column.cell(source, cell)
    }

    /// The origin column's cells of one row, pane by pane; none while the column is hidden.
    pub(super) fn row_origins(
        &self,
        cells: &[Option<usize>],
    ) -> Option<std::vec::IntoIter<Option<OriginCell>>> {
        self.origin_column.shown().then(|| {
            self.panes
                .sources()
                .into_iter()
                .zip(cells)
                .map(|(source, &line)| self.origin_column.cell(source, line))
                .collect::<Vec<_>>()
                .into_iter()
        })
    }

    /// The origin of result line `line`, as its gutter shows it; none for a line without one.
    #[must_use]
    pub fn result_origin(&self, line: usize) -> Option<OriginCell> {
        let doc = self.session.document();
        let origins = self.origin_column.origins();
        let origin = origins.result_line(doc, Line::new(line))?;
        Some(OriginCell {
            label: origins
                .result_label(doc, line, self.origin_column.kind())
                .map(SharedString::new),
            hover: hover_text(origin).into(),
        })
    }

    /// Give the result pane's gutter the origins of the current result after `delay`, while
    /// the column shows. Finding them visits every line, 240 ms at 1M, so it runs off the UI
    /// thread on a snapshot; a newer call cancels it. Without the column the gutter clears
    /// at once.
    pub(super) fn refresh_result_labels(
        &mut self,
        delay: Option<Duration>,
        cx: &mut Context<Self>,
    ) {
        if !self.origin_column.shown() {
            self.result_labels = None;
            self.result.update(cx, |state, cx| state.set_line_labels(None, cx));
            return;
        }
        self.result_labels = Some(cx.spawn(async move |this, cx| {
            if let Some(delay) = delay {
                cx.background_executor().timer(delay).await;
            }
            let Ok((doc, origins)) = this.update(cx, |this, _| {
                (this.session.document().snapshot(), this.origin_column.shared())
            }) else {
                return;
            };
            let runs = cx.background_spawn(async move { origins.result_runs(&doc) }).await;
            ignore_closed_view(this.update(cx, |this, cx| {
                this.result_labels = None;
                let labels = this.origin_column.run_labels(runs, cx.theme().muted_foreground);
                this.result.update(cx, |state, cx| state.set_line_labels(labels, cx));
            }));
        }));
    }

    /// Whether the panes show the origin column: it is turned on and the origins have
    /// loaded. A window without origins draws no empty column (spec §7.3).
    #[must_use]
    pub fn origin_column(&self) -> bool {
        self.origin_column.shown()
    }

    pub(super) fn toggle_origin_column(
        &mut self,
        _: &ToggleOriginColumn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.origin_column.toggle();
        self.refresh_result_labels(None, cx);
        cx.notify();
    }
}
