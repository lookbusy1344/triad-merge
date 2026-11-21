//! Find in the input panes.

use gpui_kit::component::input::RopeExt as _;
use gpui_kit::{AnyElement, AppContext as _, ClickEvent, Context, IntoElement, Window};
use merge_core::merge3::Source;
use merge_core::text::Text;
use merge_view::merge_fold;
use merge_view::rows::RowIx;
use merge_view::search::{Case, Hit, InputMatches};

use crate::input_find::{self, Step};
use crate::{Find, ignore_closed_view};

use super::{Found, Keep, MergeWindow};

impl MergeWindow {
    /// Open the result's search, from anywhere in the window.
    pub(super) fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        self.result.update(cx, |state, cx| {
            state.focus(window, cx);
            state.open_search(false, cx);
        });
    }

    /// Follow the result's search into the input panes: search them when its query or case
    /// changes, and drop their matches when it closes or empties.
    pub(super) fn follow_search(&mut self, cx: &mut Context<Self>) {
        let session = self.result.read(cx).search_session();
        if let Some(bytes) = self.result_steps.stepped(session) {
            self.start_found(Found::Result(bytes.start, bytes.end), cx);
        }
        self.open_searched_fold(cx);
        let session = self.result.read(cx).search_session();
        let wanted = (session.is_active() && !session.query.is_empty()).then(|| {
            let case = if session.case_insensitive { Case::Insensitive } else { Case::Sensitive };
            (session.query.clone(), case)
        });
        match self.input_find.follow(wanted) {
            input_find::Follow::Unchanged => {}
            input_find::Follow::Search => self.search_inputs(Keep::No, cx),
            input_find::Follow::Cleared => cx.notify(),
        }
    }

    /// Open the region whose hidden lines hold the result's current search match, once for
    /// each match, so a collapse after it stays collapsed (spec §5).
    pub(super) fn open_searched_fold(&mut self, cx: &mut Context<Self>) {
        let state = self.result.read(cx);
        let session = state.search_session();
        let current = session
            .is_active()
            .then(|| session.matcher.current())
            .flatten()
            .and_then(|ix| session.matcher.matched_ranges().get(ix).cloned());
        if current == self.collapse.searched {
            return;
        }
        let line = current.as_ref().map(|bytes| state.text().offset_to_point(bytes.start).row);
        self.collapse.searched = current;
        if let Some(line) = line {
            self.open_fold_over_line(line, cx);
        }
    }

    /// Search the input panes again for the same query after the rows or panes changed,
    /// keeping each pane's current match.
    pub(super) fn search_inputs_again(&mut self, cx: &mut Context<Self>) {
        if self.input_find.query().is_some() {
            self.search_inputs(Keep::Yes, cx);
        }
    }

    /// Search the visible inputs off the UI thread (`docs/DESIGN.md` §A14 measures why). With
    /// `keep`, each pane's current match stays if the search still finds it; otherwise a
    /// pane's first match from the top row is current.
    pub(super) fn search_inputs(&mut self, keep: Keep, cx: &mut Context<Self>) {
        // The search runs on another thread; the query is short.
        let Some((query, case)) = self.input_find.query().cloned() else {
            return;
        };
        let search = self.input_find.begin();
        let keep = match keep {
            Keep::Yes => self.input_find.starts(),
            Keep::No => Vec::new(),
        };
        let inputs = self.session.document().inputs();
        let sources = self.panes.sources();
        // `Text` clones share their buffer.
        let texts: Vec<(Source, Text)> =
            sources.iter().map(|&source| (source, inputs.text(source).clone())).collect();
        // A copy of the line ranges, one per region, so the rows can be found off the UI
        // thread.
        let regions = self.session.document().regions().to_vec();
        let rows = self.rows.clone();
        let from = self.top_row();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    InputMatches::find_in(texts.iter().map(|(s, t)| (*s, t)), &query, case)
                        .map(|matches| matches.ordered(&regions, &rows))
                })
                .await;
            ignore_closed_view(this.update(cx, |this, cx| {
                let hits = found
                    .inspect_err(|error| tracing::warn!(%error, "the input search found nothing"))
                    .unwrap_or_default();
                this.input_find.land(search, hits, &sources, &keep, from);
                cx.notify();
            }));
        })
        .detach();
    }

    /// Step pane `pane` to its next or previous match and scroll that row to the middle.
    pub(super) fn step_input_match(&mut self, pane: usize, step: Step, cx: &mut Context<Self>) {
        if let Some(row) = self.input_find.step(pane, step).map(|hit| hit.row) {
            if let Some(fold) = merge_fold::fold_at_row(&self.collapse.folds, row.get()).cloned() {
                self.open_fold(&fold, cx);
            }
            self.scroll.centre(row);
            self.start_found(Found::Input(pane), cx);
        }
        cx.notify();
    }

    /// Each input pane's search controls, while the result's search runs.
    pub(super) fn input_controls(&self, cx: &mut Context<Self>) -> Vec<Option<AnyElement>> {
        (0..self.headings.len())
            .map(|pane| {
                let title = &self.headings[pane].title;
                let previous = cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.step_input_match(pane, Step::Previous, cx);
                });
                let next = cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.step_input_match(pane, Step::Next, cx);
                });
                input_find::controls(&self.input_find, pane, title, previous, next)
                    .map(IntoElement::into_any_element)
            })
            .collect()
    }

    /// The first row the input panes show.
    pub(super) fn top_row(&self) -> RowIx {
        self.scroll.top_row(self.typography.line_height())
    }

    /// Input pane `pane`'s match count, `n of m` or `No matches`; `None` while the result's
    /// search is closed or empty.
    #[must_use]
    pub fn input_match_count(&self, pane: usize) -> Option<String> {
        self.input_find.count(pane)
    }

    /// Input pane `pane`'s current match.
    #[must_use]
    pub fn input_match_current(&self, pane: usize) -> Option<&Hit> {
        self.input_find.current(pane)
    }
}
