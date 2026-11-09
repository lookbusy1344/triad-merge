//! Search in the input panes (spec §6.3): the result's search drives every input pane, and
//! each pane steps through its own matches from the controls in its heading.

use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex};
use gpui_kit::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div, px,
};
use merge_core::merge3::Source;
use merge_view::rows::RowIx;
use merge_view::runs::Matched;
use merge_view::search::{self, Case, Hit};

use crate::strings;

/// A step button's square side: a compact inline button, whose icon takes three quarters.
const STEP_BUTTON: Pixels = px(18.);

/// Pane `pane`'s previous-match button, also used by tests.
#[must_use]
pub fn input_match_previous_id(pane: usize) -> ElementId {
    ElementId::Name(format!("input-match-previous-{pane}").into())
}

/// Pane `pane`'s next-match button, also used by tests.
#[must_use]
pub fn input_match_next_id(pane: usize) -> ElementId {
    ElementId::Name(format!("input-match-next-{pane}").into())
}

/// Pane `pane`'s match count, also used by tests.
#[must_use]
pub fn input_match_count_id(pane: usize) -> ElementId {
    ElementId::Name(format!("input-match-count-{pane}").into())
}

/// A step through one pane's matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Next,
    Previous,
}

/// Where a match starts: its input, line and first byte. It names a pane's current match
/// across a search, whose hits renumber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Start {
    source: Source,
    line: usize,
    byte: usize,
}

impl Start {
    fn of(hit: &Hit) -> Self {
        Self { source: hit.source, line: hit.line, byte: hit.bytes.start }
    }
}

/// One pane's matches, in row and byte order, and its current one.
#[derive(Debug)]
struct PaneHits {
    source: Source,
    hits: Vec<Hit>,
    current: Option<usize>,
}

/// What following the result's search asks of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Follow {
    /// The query and case stand; nothing to do.
    Unchanged,
    /// A new query or case: search the panes.
    Search,
    /// The search closed or emptied: the matches are gone.
    Cleared,
}

/// The input panes' matches of the result search's query.
#[derive(Debug, Default)]
pub(crate) struct Found {
    /// The query and case the panes are searched for; none while the result's search is
    /// closed or empty.
    query: Option<(String, Case)>,
    /// One entry per visible input pane, in pane order, once a search has landed.
    panes: Vec<PaneHits>,
    /// Counts the searches started; a result from an older one is dropped.
    searches: u64,
}

impl Found {
    /// Follow the result's search to `wanted`, its query and case while it is open and not
    /// empty.
    pub(crate) fn follow(&mut self, wanted: Option<(String, Case)>) -> Follow {
        if wanted == self.query {
            return Follow::Unchanged;
        }
        self.query = wanted;
        if self.query.is_some() {
            Follow::Search
        } else {
            self.clear();
            Follow::Cleared
        }
    }

    /// The query and case the panes are searched for.
    pub(crate) fn query(&self) -> Option<&(String, Case)> {
        self.query.as_ref()
    }

    /// Start a search; its result lands with the number returned.
    pub(crate) fn begin(&mut self) -> u64 {
        self.searches += 1;
        self.searches
    }

    /// Drop the matches, and the result of any search still running.
    fn clear(&mut self) {
        self.panes = Vec::new();
        self.searches += 1;
    }

    /// Take the hits of search `search` for the panes showing `sources`, unless a later search
    /// started. Each pane keeps its current match when `keep` names one still found;
    /// otherwise its first hit on or below `from` is current. Returns whether the hits were
    /// taken.
    pub(crate) fn land(
        &mut self,
        search: u64,
        hits: Vec<Hit>,
        sources: &[Source],
        keep: &[Start],
        from: RowIx,
    ) -> bool {
        if search != self.searches {
            return false;
        }
        let mut panes: Vec<PaneHits> = sources
            .iter()
            .map(|&source| PaneHits { source, hits: Vec::new(), current: None })
            .collect();
        for hit in hits {
            if let Some(pane) = panes.iter_mut().find(|p| p.source == hit.source) {
                pane.hits.push(hit);
            }
        }
        for pane in &mut panes {
            pane.current = keep
                .iter()
                .filter(|at| at.source == pane.source)
                .find_map(|at| search::position_of(&pane.hits, at.source, at.line, at.byte))
                .or_else(|| search::first_from(&pane.hits, from));
        }
        self.panes = panes;
        true
    }

    /// Where each pane's current match starts.
    pub(crate) fn starts(&self) -> Vec<Start> {
        (0..self.panes.len()).filter_map(|pane| self.current(pane).map(Start::of)).collect()
    }

    /// Pane `pane`'s current match.
    pub(crate) fn current(&self, pane: usize) -> Option<&Hit> {
        let pane = self.panes.get(pane)?;
        pane.current.and_then(|i| pane.hits.get(i))
    }

    /// Move pane `pane` to its next or previous match, wrapping; the match moved to.
    pub(crate) fn step(&mut self, pane: usize, step: Step) -> Option<&Hit> {
        let hits = self.panes.get_mut(pane)?;
        let current = hits.current?;
        let len = hits.hits.len();
        hits.current = Some(match step {
            Step::Next => search::next(len, current),
            Step::Previous => search::previous(len, current),
        });
        self.current(pane)
    }

    /// Pane `pane`'s count: `n of m` or `No matches`; `None` before a search lands.
    pub(crate) fn count(&self, pane: usize) -> Option<String> {
        let hits = self.panes.get(pane)?;
        Some(hits.current.map_or_else(
            || strings::NO_MATCHES.to_owned(),
            |current| strings::match_count(current + 1, hits.hits.len()),
        ))
    }

    /// The matches top pane `pane` shows on `row`, in bytes of the line, each marked as the
    /// pane's current match or not.
    pub(crate) fn in_cell(
        &self,
        row: RowIx,
        pane: usize,
    ) -> impl Iterator<Item = (Range<usize>, Matched)> + '_ {
        let current = self.current(pane);
        let hits = self.panes.get(pane).map_or(&[][..], |p| p.hits.as_slice());
        search::in_row(hits, row).iter().map(move |hit| {
            let kind = if current == Some(hit) { Matched::Current } else { Matched::Other };
            (hit.bytes.clone(), kind)
        })
    }

    /// Whether `pane` has a match to step to.
    fn has_hits(&self, pane: usize) -> bool {
        self.panes.get(pane).is_some_and(|p| !p.hits.is_empty())
    }
}

/// Pane `pane`'s controls in its heading, for the pane titled `title`: previous, the count,
/// next. `None` before a search lands.
pub(crate) fn controls<P, N>(
    found: &Found,
    pane: usize,
    title: &str,
    previous: P,
    next: N,
) -> Option<impl IntoElement + use<P, N>>
where
    P: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    N: Fn(&ClickEvent, &mut Window, &mut App) + 'static,
{
    let count = SharedString::from(found.count(pane)?);
    let stepping = found.has_hits(pane);
    let button = |id, icon, label: String| {
        Button::new(id)
            .ghost()
            .with_size(STEP_BUTTON)
            .icon(icon)
            .disabled(!stepping)
            .accessibility_label(label.clone())
            .tooltip(label)
    };
    Some(
        h_flex()
            .flex_none()
            .ml_auto()
            .child(
                button(
                    input_match_previous_id(pane),
                    IconName::ChevronUp,
                    strings::previous_match(title),
                )
                .on_click(previous),
            )
            .child(
                div()
                    .id(input_match_count_id(pane))
                    .test_support()
                    .role(Role::Status)
                    .aria_label(count.clone())
                    .px_1()
                    .whitespace_nowrap()
                    .font_weight(gpui_kit::FontWeight::NORMAL)
                    .child(count),
            )
            .child(
                button(
                    input_match_next_id(pane),
                    IconName::ChevronDown,
                    strings::next_match(title),
                )
                .on_click(next),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Source = Source::Side(0);
    const RIGHT: Source = Source::Side(1);
    const SOURCES: [Source; 2] = [LEFT, RIGHT];

    fn hit(row: usize, source: Source, line: usize, start: usize) -> Hit {
        Hit {
            row: RowIx::new(row),
            pane: usize::from(source == RIGHT),
            source,
            line,
            bytes: start..start + 1,
        }
    }

    /// Left hits on rows 2 and 9, right on row 5, in row order.
    fn hits() -> Vec<Hit> {
        vec![hit(2, LEFT, 2, 0), hit(5, RIGHT, 4, 0), hit(9, LEFT, 9, 0)]
    }

    fn landed(from: usize) -> Found {
        let mut found = Found::default();
        let search = found.begin();
        assert!(found.land(search, hits(), &SOURCES, &[], RowIx::new(from)));
        found
    }

    fn query(text: &str) -> (String, Case) {
        (text.to_owned(), Case::Insensitive)
    }

    #[test]
    fn a_new_query_searches_and_the_same_one_does_not() {
        let mut found = Found::default();
        assert_eq!(found.follow(None), Follow::Unchanged, "no search, none wanted");
        assert_eq!(found.follow(Some(query("a"))), Follow::Search);
        assert_eq!(found.query(), Some(&query("a")));
        assert_eq!(found.follow(Some(query("a"))), Follow::Unchanged);
        assert_eq!(found.follow(Some(("a".to_owned(), Case::Sensitive))), Follow::Search);
        assert_eq!(found.follow(Some(query("ab"))), Follow::Search);
    }

    #[test]
    fn closing_the_search_drops_the_matches_and_any_search_running() {
        let mut found = landed(0);
        assert_eq!(found.follow(Some(query("a"))), Follow::Search);
        let running = found.begin();
        assert_eq!(found.follow(None), Follow::Cleared);
        assert_eq!((found.query(), found.count(0)), (None, None));
        assert!(!found.land(running, hits(), &SOURCES, &[], RowIx::new(0)), "dropped");
    }

    #[test]
    fn each_pane_takes_its_own_hits_and_its_first_from_the_row() {
        let found = landed(3);
        assert_eq!(found.current(0), Some(&hit(9, LEFT, 9, 0)));
        assert_eq!(found.count(0).as_deref(), Some("2 of 2"));
        assert_eq!(found.current(1), Some(&hit(5, RIGHT, 4, 0)));
        assert_eq!(found.count(1).as_deref(), Some("1 of 1"));
    }

    #[test]
    fn a_pane_without_hits_says_so() {
        let mut found = Found::default();
        let search = found.begin();
        found.land(search, vec![hit(2, LEFT, 2, 0)], &SOURCES, &[], RowIx::new(0));
        assert_eq!(found.count(1).as_deref(), Some("No matches"));
        assert_eq!(found.step(1, Step::Next), None);
        assert_eq!(found.count(2), None, "no third pane");
    }

    #[test]
    fn nothing_counts_before_a_search_lands() {
        let found = Found::default();
        assert_eq!(found.count(0), None);
        assert_eq!(found.current(0), None);
    }

    #[test]
    fn an_older_search_is_dropped() {
        let mut found = Found::default();
        let older = found.begin();
        let newer = found.begin();
        assert!(!found.land(older, hits(), &SOURCES, &[], RowIx::new(0)));
        assert_eq!(found.count(0), None);
        assert!(found.land(newer, hits(), &SOURCES, &[], RowIx::new(0)));
        assert_eq!(found.count(0).as_deref(), Some("1 of 2"));
    }

    #[test]
    fn a_search_running_when_cleared_is_dropped() {
        let mut found = Found::default();
        let search = found.begin();
        found.clear();
        assert!(!found.land(search, hits(), &SOURCES, &[], RowIx::new(0)));
        assert_eq!(found.count(0), None);
    }

    #[test]
    fn steps_move_one_pane_and_wrap() {
        let mut found = landed(0);
        assert_eq!(found.step(0, Step::Next), Some(&hit(9, LEFT, 9, 0)));
        assert_eq!(found.current(1), Some(&hit(5, RIGHT, 4, 0)), "right stays");
        assert_eq!(found.step(0, Step::Next), Some(&hit(2, LEFT, 2, 0)));
        assert_eq!(found.step(0, Step::Previous), Some(&hit(9, LEFT, 9, 0)));
    }

    #[test]
    fn each_panes_current_match_stays_when_found_again() {
        let mut found = landed(9);
        let keep = found.starts();
        let search = found.begin();
        // Rows moved: left's current match is now on row 12, behind a new first hit, and the
        // panes are now right then left.
        let moved =
            vec![hit(1, LEFT, 0, 0), hit(2, LEFT, 2, 0), hit(7, RIGHT, 4, 0), hit(12, LEFT, 9, 0)];
        found.land(search, moved, &[RIGHT, LEFT], &keep, RowIx::new(0));
        assert_eq!(found.current(0), Some(&hit(7, RIGHT, 4, 0)));
        assert_eq!(found.current(1), Some(&hit(12, LEFT, 9, 0)));
        assert_eq!(found.count(1).as_deref(), Some("3 of 3"));
    }

    #[test]
    fn a_cell_marks_only_its_panes_current_match() {
        let mut found = Found::default();
        let search = found.begin();
        let row = vec![hit(2, LEFT, 2, 0), hit(2, LEFT, 2, 3), hit(2, RIGHT, 2, 1)];
        found.land(search, row, &SOURCES, &[], RowIx::new(0));
        let cell = |found: &Found, pane| found.in_cell(RowIx::new(2), pane).collect::<Vec<_>>();
        assert_eq!(cell(&found, 0), [(0..1, Matched::Current), (3..4, Matched::Other)]);
        assert_eq!(cell(&found, 1), [(1..2, Matched::Current)]);
        found.step(0, Step::Next);
        assert_eq!(cell(&found, 0), [(0..1, Matched::Other), (3..4, Matched::Current)]);
        assert_eq!(found.in_cell(RowIx::new(3), 0).count(), 0);
    }
}
