//! Search in the top panes (spec §5): a literal query over every visible input, matched as the
//! editors' search matches, with ASCII case folding when the search ignores case.

use std::ops::Range;

use aho_corasick::AhoCorasick;
use merge_core::document::MergeDocument;
use merge_core::merge3::{Region, Source};
use merge_core::text::Text;
use smallvec::SmallVec;

use crate::rows::{PaneSet, RowIx, RowMap};

/// Whether a search tells upper from lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    /// ASCII letters only, as the editors' search folds them.
    Insensitive,
}

/// A match inside one input line: the line and the bytes within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMatch {
    pub line: usize,
    pub bytes: Range<usize>,
}

/// A match where the top panes show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub row: RowIx,
    /// The pane's position among the visible panes.
    pub pane: usize,
    pub source: Source,
    pub line: usize,
    pub bytes: Range<usize>,
}

/// A query the matcher cannot take.
#[derive(Debug, thiserror::Error)]
#[error("cannot search for this query")]
pub struct SearchError(#[from] aho_corasick::BuildError);

/// Every match of a query in the visible inputs, per input, in line and byte order.
#[derive(Debug, Clone, Default)]
pub struct InputMatches {
    sources: SmallVec<[(Source, Vec<LineMatch>); PANES]>,
}

/// Inputs held inline; more spill to the heap.
const PANES: usize = 3;

impl InputMatches {
    /// Every match of `query` in the inputs `panes` shows. An empty query, or one holding a
    /// newline, matches nothing: a row shows one line.
    ///
    /// # Errors
    ///
    /// [`SearchError`] when the query is too large for the matcher.
    pub fn find(
        doc: &MergeDocument,
        panes: PaneSet,
        query: &str,
        case: Case,
    ) -> Result<Self, SearchError> {
        let texts = panes.sources().into_iter().map(|source| (source, doc.inputs().text(source)));
        Self::find_in(texts, query, case)
    }

    /// [`Self::find`] over `texts`, each with the input it is, in pane order.
    ///
    /// # Errors
    ///
    /// As [`Self::find`].
    pub fn find_in<'a>(
        texts: impl IntoIterator<Item = (Source, &'a Text)>,
        query: &str,
        case: Case,
    ) -> Result<Self, SearchError> {
        if query.is_empty() || query.contains('\n') {
            return Ok(Self::default());
        }
        let searcher = AhoCorasick::builder()
            .ascii_case_insensitive(case == Case::Insensitive)
            .build([query])?;
        let sources = texts
            .into_iter()
            .map(|(source, text)| {
                let matches = text
                    .lines()
                    .enumerate()
                    .flat_map(|(line, content)| {
                        searcher
                            .find_iter(content)
                            .map(move |m| LineMatch { line, bytes: m.range() })
                    })
                    .collect();
                (source, matches)
            })
            .collect();
        Ok(Self { sources })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sources.iter().map(|(_, matches)| matches.len()).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The matches in `source`; none when it is not searched.
    #[must_use]
    pub fn in_source(&self, source: Source) -> &[LineMatch] {
        self.sources
            .iter()
            .find(|(s, _)| *s == source)
            .map_or(&[], |(_, matches)| matches.as_slice())
    }

    /// The matches in `source`'s line `line`, in byte order.
    #[must_use]
    pub fn in_cell(&self, source: Source, line: usize) -> &[LineMatch] {
        let matches = self.in_source(source);
        let start = matches.partition_point(|m| m.line < line);
        let end = start + matches[start..].partition_point(|m| m.line == line);
        &matches[start..end]
    }

    /// Every match with its row, ordered by row, then pane, then byte.
    #[must_use]
    pub fn ordered(&self, regions: &[Region], rows: &RowMap) -> Vec<Hit> {
        let mut hits: Vec<Hit> = self
            .sources
            .iter()
            .enumerate()
            .flat_map(|(pane, (source, matches))| {
                matches.iter().filter_map(move |m| {
                    Some(Hit {
                        row: rows.row_of_input_line(regions, *source, m.line)?,
                        pane,
                        source: *source,
                        line: m.line,
                        bytes: m.bytes.clone(),
                    })
                })
            })
            .collect();
        hits.sort_unstable_by_key(|hit| (hit.row, hit.pane, hit.bytes.start));
        hits
    }
}

/// The hits on `row`, in pane and byte order.
#[must_use]
pub fn in_row(hits: &[Hit], row: RowIx) -> &[Hit] {
    let start = hits.partition_point(|hit| hit.row < row);
    let end = start + hits[start..].partition_point(|hit| hit.row == row);
    &hits[start..end]
}

/// The hit after `current` of `len`, wrapping to the first.
#[must_use]
pub fn next(len: usize, current: usize) -> usize {
    (current + 1) % len
}

/// The hit before `current` of `len`, wrapping to the last.
#[must_use]
pub fn previous(len: usize, current: usize) -> usize {
    (current + len - 1) % len
}

/// The first hit on or below `row`, else the first; `None` without hits.
#[must_use]
pub fn first_from(hits: &[Hit], row: RowIx) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    let at = hits.partition_point(|hit| hit.row < row);
    Some(if at == hits.len() { 0 } else { at })
}

/// The hit in `source`'s line `line` that starts at byte `start`.
#[must_use]
pub fn position_of(hits: &[Hit], source: Source, line: usize, start: usize) -> Option<usize> {
    hits.iter().position(|hit| hit.source == source && hit.line == line && hit.bytes.start == start)
}

#[cfg(test)]
mod tests {
    use merge_core::document::{Labels, MergeOptions};
    use merge_core::merge3::Inputs;

    use super::*;

    const LEFT: Source = Source::Side(0);
    const BASE: Source = Source::Base(0);
    const RIGHT: Source = Source::Side(1);

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes()).expect("utf-8")
    }

    fn three(base: &str, left: &str, right: &str) -> MergeDocument {
        MergeDocument::new(
            Inputs::three_way(text(base), text(left), text(right)),
            MergeOptions::default(),
            Labels::none(),
        )
        .expect("no labels")
    }

    fn lm(line: usize, bytes: Range<usize>) -> LineMatch {
        LineMatch { line, bytes }
    }

    fn found(doc: &MergeDocument, panes: PaneSet, query: &str, case: Case) -> InputMatches {
        InputMatches::find(doc, panes, query, case).expect("a short query builds")
    }

    /// Left `AB ab` holds `ab` at 3 and, ignoring case, at 0 too.
    fn cases() -> MergeDocument {
        three("x\nab\nz\n", "x\nAB ab\nz\n", "x\nab\nz\nab\n")
    }

    #[test]
    fn a_case_sensitive_search_matches_exact_bytes() {
        let m = found(&cases(), PaneSet::LeftBaseRight, "ab", Case::Sensitive);
        assert_eq!(m.in_source(LEFT), [lm(1, 3..5)]);
        assert_eq!(m.in_source(BASE), [lm(1, 0..2)]);
        assert_eq!(m.in_source(RIGHT), [lm(1, 0..2), lm(3, 0..2)]);
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn a_case_insensitive_search_folds_ascii_letters() {
        let m = found(&cases(), PaneSet::LeftBaseRight, "ab", Case::Insensitive);
        assert_eq!(m.in_source(LEFT), [lm(1, 0..2), lm(1, 3..5)]);
        assert_eq!(m.len(), 5);
        let upper = found(&cases(), PaneSet::LeftBaseRight, "AB", Case::Sensitive);
        assert_eq!(upper.in_source(LEFT), [lm(1, 0..2)]);
        assert_eq!(upper.len(), 1);
    }

    #[test]
    fn case_folding_leaves_letters_outside_ascii() {
        let doc = three("x\n", "é\n", "x\n");
        let folded = found(&doc, PaneSet::LeftRight, "É", Case::Insensitive);
        assert!(folded.is_empty());
        let exact = found(&doc, PaneSet::LeftRight, "é", Case::Insensitive);
        assert_eq!(exact.in_source(LEFT), [lm(0, 0..2)]);
    }

    #[test]
    fn a_hidden_base_is_not_searched() {
        let m = found(&cases(), PaneSet::LeftRight, "ab", Case::Sensitive);
        assert_eq!(m.in_source(BASE), []);
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn matches_do_not_overlap() {
        let doc = three("x\n", "aaa\n", "x\n");
        let m = found(&doc, PaneSet::LeftRight, "aa", Case::Sensitive);
        assert_eq!(m.in_source(LEFT), [lm(0, 0..2)]);
    }

    #[test]
    fn matches_sit_at_line_starts_and_ends_whatever_the_ending() {
        let doc = three("x\n", "ba\r\nab", "x\n");
        let m = found(&doc, PaneSet::LeftRight, "b", Case::Sensitive);
        assert_eq!(m.in_source(LEFT), [lm(0, 0..1), lm(1, 1..2)]);
        assert_eq!(m.in_cell(LEFT, 1), [lm(1, 1..2)]);
        assert_eq!(m.in_cell(LEFT, 2), []);
    }

    #[test]
    fn a_query_with_a_newline_or_none_finds_nothing() {
        let doc = three("a\nb\n", "a\nb\n", "a\nb\n");
        assert!(found(&doc, PaneSet::LeftRight, "a\nb", Case::Sensitive).is_empty());
        assert!(found(&doc, PaneSet::LeftRight, "", Case::Sensitive).is_empty());
    }

    /// Rows, base hidden: 0 `a|a`, 1 `bXb|-`, 2 `Y|-`, 3 `b|b`. With the base, 3 is `b|b|b`.
    fn inserted() -> MergeDocument {
        three("a\nb\n", "a\nbXb\nY\nb\n", "a\nb\n")
    }

    fn hit(row: usize, pane: usize, source: Source, line: usize, bytes: Range<usize>) -> Hit {
        Hit { row: RowIx::new(row), pane, source, line, bytes }
    }

    #[test]
    fn an_input_line_has_the_row_that_shows_it() {
        let doc = inserted();
        let two = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        assert_eq!(two.row_of_input_line(doc.regions(), LEFT, 3), Some(RowIx::new(3)));
        assert_eq!(two.row_of_input_line(doc.regions(), LEFT, 2), Some(RowIx::new(2)));
        assert_eq!(two.row_of_input_line(doc.regions(), RIGHT, 0), Some(RowIx::new(0)));
        assert_eq!(two.row_of_input_line(doc.regions(), RIGHT, 1), Some(RowIx::new(3)));
        assert_eq!(two.row_of_input_line(doc.regions(), RIGHT, 2), None, "past the end");
        assert_eq!(two.row_of_input_line(doc.regions(), BASE, 1), None, "base hidden");
        let three = RowMap::new(&doc, PaneSet::LeftBaseRight).expect("panes fit");
        assert_eq!(three.row_of_input_line(doc.regions(), BASE, 1), Some(RowIx::new(3)));
    }

    #[test]
    fn hits_go_by_row_then_pane_then_byte() {
        let doc = inserted();
        let rows = RowMap::new(&doc, PaneSet::LeftBaseRight).expect("panes fit");
        let m = found(&doc, PaneSet::LeftBaseRight, "b", Case::Sensitive);
        assert_eq!(
            m.ordered(doc.regions(), &rows),
            [
                hit(1, 0, LEFT, 1, 0..1),
                hit(1, 0, LEFT, 1, 2..3),
                hit(3, 0, LEFT, 3, 0..1),
                hit(3, 1, BASE, 1, 0..1),
                hit(3, 2, RIGHT, 1, 0..1),
            ]
        );
    }

    #[test]
    fn a_left_pane_hit_comes_first_even_further_along_its_line() {
        let doc = three("a\n", "xb\n", "b\n");
        let rows = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        let m = found(&doc, PaneSet::LeftRight, "b", Case::Sensitive);
        assert_eq!(
            m.ordered(doc.regions(), &rows),
            [hit(0, 0, LEFT, 0, 1..2), hit(0, 1, RIGHT, 0, 0..1)]
        );
    }

    #[test]
    fn hits_without_the_base_number_the_panes_anew() {
        let doc = inserted();
        let rows = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        let m = found(&doc, PaneSet::LeftRight, "b", Case::Sensitive);
        let hits = m.ordered(doc.regions(), &rows);
        assert_eq!(hits.len(), 4);
        assert_eq!(hits[3], hit(3, 1, RIGHT, 1, 0..1));
    }

    #[test]
    fn a_rows_hits_are_those_on_it() {
        let doc = inserted();
        let rows = RowMap::new(&doc, PaneSet::LeftBaseRight).expect("panes fit");
        let hits =
            found(&doc, PaneSet::LeftBaseRight, "b", Case::Sensitive).ordered(doc.regions(), &rows);
        assert_eq!(in_row(&hits, RowIx::new(1)), &hits[0..2]);
        assert_eq!(in_row(&hits, RowIx::new(2)), []);
        assert_eq!(in_row(&hits, RowIx::new(3)), &hits[2..5]);
        assert_eq!(in_row(&hits, RowIx::new(9)), []);
    }

    #[test]
    fn stepping_wraps_at_both_ends() {
        assert_eq!(next(4, 0), 1);
        assert_eq!(next(4, 3), 0);
        assert_eq!(previous(4, 3), 2);
        assert_eq!(previous(4, 0), 3);
        assert_eq!(next(1, 0), 0);
        assert_eq!(previous(1, 0), 0);
    }

    #[test]
    fn the_first_hit_from_a_row_wraps_past_the_last() {
        let doc = inserted();
        let rows = RowMap::new(&doc, PaneSet::LeftBaseRight).expect("panes fit");
        let hits =
            found(&doc, PaneSet::LeftBaseRight, "b", Case::Sensitive).ordered(doc.regions(), &rows);
        assert_eq!(first_from(&hits, RowIx::new(0)), Some(0));
        assert_eq!(first_from(&hits, RowIx::new(2)), Some(2));
        assert_eq!(first_from(&hits, RowIx::new(3)), Some(2));
        assert_eq!(first_from(&hits, RowIx::new(4)), Some(0));
        assert_eq!(first_from(&[], RowIx::new(0)), None);
    }

    #[test]
    fn a_hit_is_found_again_by_where_it_starts() {
        let doc = inserted();
        let rows = RowMap::new(&doc, PaneSet::LeftBaseRight).expect("panes fit");
        let hits =
            found(&doc, PaneSet::LeftBaseRight, "b", Case::Sensitive).ordered(doc.regions(), &rows);
        assert_eq!(position_of(&hits, LEFT, 1, 2), Some(1));
        assert_eq!(position_of(&hits, BASE, 1, 0), Some(3));
        assert_eq!(position_of(&hits, LEFT, 1, 1), None);
    }
}
