//! A top-pane cell's text as runs: syntax colours with the intra-line marks and search
//! matches over them (`docs/PLAN.md` §F4). The syntax style is generic, so this stays free of GPUI.

use std::borrow::Cow;
use std::ops::Range;

use crate::highlight::CellSpans;
use crate::tabs;

/// One run of a cell's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run<S> {
    pub range: Range<usize>,
    /// The syntax style; none where no syntax span reaches.
    pub style: Option<S>,
    /// The run differs from its partner line.
    pub changed: bool,
    /// The run lies in a search match.
    pub matched: Option<Matched>,
}

/// Which search match a run lies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matched {
    Other,
    Current,
}

/// Split `syntax`, `changed` and `matched` at every boundary of any, in byte order. Each is
/// sorted and disjoint. Every byte one covers lies in one run; bytes none covers lie in none.
/// Empty ranges are dropped.
#[must_use]
pub fn runs<S: Copy>(
    syntax: &[(Range<usize>, S)],
    changed: &[Range<usize>],
    matched: &[(Range<usize>, Matched)],
) -> Vec<Run<S>> {
    let mut bounds: Vec<usize> = syntax
        .iter()
        .map(|(range, _)| range)
        .chain(changed)
        .chain(matched.iter().map(|(range, _)| range))
        .filter(|range| !range.is_empty())
        .flat_map(|range| [range.start, range.end])
        .collect();
    bounds.sort_unstable();
    bounds.dedup();
    let mut styles = syntax.iter().peekable();
    let mut marks = changed.iter().peekable();
    let mut found = matched.iter().peekable();
    bounds
        .windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            let style = covering(&mut styles, |(r, _)| r, range.start).map(|&(_, s)| s);
            let changed = covering(&mut marks, |r| r, range.start).is_some();
            let matched = covering(&mut found, |(r, _)| r, range.start).map(|&(_, m)| m);
            (style.is_some() || changed || matched.is_some()).then_some(Run {
                range,
                style,
                changed,
                matched,
            })
        })
        .collect()
}

/// A cell's text as it draws, and its runs in bytes of that text: the line without its
/// terminator, tabs expanded to `tab_width`, with `syntax`, `spans` and `matched` (all in
/// bytes of the line) moved onto the expanded text. Ranges past the line stop at its end.
#[must_use]
pub fn cell<'a, S: Copy>(
    line: &'a str,
    spans: &CellSpans,
    syntax: &[(Range<usize>, S)],
    matched: &[(Range<usize>, Matched)],
    tab_width: usize,
) -> (Cow<'a, str>, Vec<Run<S>>) {
    let line = line.trim_end_matches(['\n', '\r']);
    let expanded = tabs::expand(line, tab_width);
    let at = |offset: usize| expanded.offset(offset.min(line.len()));
    let syntax: Vec<_> =
        syntax.iter().map(|(range, style)| (at(range.start)..at(range.end), *style)).collect();
    let changed: Vec<_> = match spans {
        CellSpans::None => Vec::new(),
        CellSpans::WholeLine => std::iter::once(0..expanded.text().len()).collect(),
        CellSpans::Ranges(ranges) => {
            ranges.iter().map(|r| at(r.start)..at(r.end)).filter(|r| !r.is_empty()).collect()
        }
    };
    let matched: Vec<_> = matched
        .iter()
        .map(|(range, kind)| (at(range.start)..at(range.end), *kind))
        .filter(|(range, _)| !range.is_empty())
        .collect();
    (expanded.into_text(), runs(&syntax, &changed, &matched))
}

/// The item of a sorted, disjoint sequence whose range holds byte `at`, skipping the items
/// that end at or before it. Calls come in rising `at`.
fn covering<'a, T>(
    items: &mut std::iter::Peekable<std::slice::Iter<'a, T>>,
    range: impl Fn(&T) -> &Range<usize>,
    at: usize,
) -> Option<&'a T> {
    while items.next_if(|item| range(item).end <= at).is_some() {}
    items.peek().copied().filter(|item| range(item).start <= at)
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: char = 'k';
    const S: char = 's';

    /// One mark; a one-range array literal reads as a mistake to clippy.
    fn one(range: Range<usize>) -> [Range<usize>; 1] {
        [range]
    }

    fn run(range: Range<usize>, style: Option<char>, changed: bool) -> Run<char> {
        Run { range, style, changed, matched: None }
    }

    fn found(run: Run<char>, matched: Matched) -> Run<char> {
        Run { matched: Some(matched), ..run }
    }

    #[test]
    fn a_mark_across_two_spans_splits_both() {
        assert_eq!(
            runs(&[(0..2, K), (3..5, S)], &one(1..4), &[]),
            [
                run(0..1, Some(K), false),
                run(1..2, Some(K), true),
                run(2..3, None, true),
                run(3..4, Some(S), true),
                run(4..5, Some(S), false),
            ]
        );
    }

    #[test]
    fn without_marks_the_spans_pass_through() {
        assert_eq!(
            runs(&[(0..2, K), (2..4, S)], &[], &[]),
            [run(0..2, Some(K), false), run(2..4, Some(S), false)]
        );
    }

    #[test]
    fn without_spans_the_marks_pass_through() {
        assert_eq!(
            runs::<char>(&[], &[1..2, 4..6], &[]),
            [run(1..2, None, true), run(4..6, None, true)]
        );
    }

    #[test]
    fn nothing_gives_no_runs() {
        assert_eq!(runs::<char>(&[], &[], &[]), []);
    }

    #[test]
    fn a_mark_on_a_span_makes_one_run() {
        assert_eq!(runs(&[(2..5, K)], &one(2..5), &[]), [run(2..5, Some(K), true)]);
    }

    #[test]
    fn a_gap_between_a_span_and_a_mark_has_no_run() {
        assert_eq!(
            runs(&[(0..1, K)], &one(5..6), &[]),
            [run(0..1, Some(K), false), run(5..6, None, true)]
        );
    }

    #[test]
    fn empty_ranges_drop() {
        assert_eq!(runs(&[(2..2, K)], &one(3..3), &[]), []);
        assert_eq!(runs(&[(0..0, S), (0..2, K)], &one(1..1), &[]), [run(0..2, Some(K), false)]);
    }

    #[test]
    fn a_mark_inside_a_span_splits_it_in_three() {
        assert_eq!(
            runs(&[(0..6, K)], &one(2..3), &[]),
            [run(0..2, Some(K), false), run(2..3, Some(K), true), run(3..6, Some(K), false),]
        );
    }

    const TAB_WIDTH: usize = 4;

    /// A cell's text and the ranges its marks cover, without syntax.
    fn marks<'a>(line: &'a str, spans: &CellSpans) -> (Cow<'a, str>, Vec<Range<usize>>) {
        let (text, runs) = cell::<()>(line, spans, &[], &[], TAB_WIDTH);
        assert!(runs.iter().all(|run| run.changed && run.style.is_none()));
        (text, runs.into_iter().map(|run| run.range).collect())
    }

    #[test]
    fn syntax_after_a_tab_moves_with_the_text() {
        // `"b"` is bytes 2..5 of the line, 4..7 once the tab runs to column 4.
        let syntax = [(0..1, K), (2..5, S)];
        let (text, runs) = cell("a\t\"b\"\n", &CellSpans::None, &syntax, &[], TAB_WIDTH);
        assert_eq!(text, "a   \"b\"");
        assert_eq!(runs, [run(0..1, Some(K), false), run(4..7, Some(S), false)]);
    }

    #[test]
    fn a_mark_splits_the_syntax_under_it() {
        let spans = CellSpans::Ranges(one(1..3).to_vec());
        let (_, runs) = cell("abcd\n", &spans, &[(0..4, K)], &[], TAB_WIDTH);
        assert_eq!(
            runs,
            [run(0..1, Some(K), false), run(1..3, Some(K), true), run(3..4, Some(K), false),]
        );
    }

    #[test]
    fn syntax_past_the_line_stops_at_its_end() {
        let (_, runs) = cell("ab\r\n", &CellSpans::None, &[(0..4, K)], &[], TAB_WIDTH);
        assert_eq!(runs, [run(0..2, Some(K), false)]);
    }

    #[test]
    fn a_cell_expands_tabs_and_drops_the_terminator() {
        let (text, ranges) = marks("a\tb\r\n", &CellSpans::WholeLine);
        assert_eq!(text, "a   b");
        assert_eq!(ranges, one(0..5));
    }

    #[test]
    fn marks_after_a_tab_move_with_the_text() {
        // "b" is bytes 2..3 of the line, 4..5 once the tab runs to column 4.
        let spans = CellSpans::Ranges(vec![0..2, 2..3]);
        let (text, ranges) = marks("a\tb\n", &spans);
        assert_eq!(text, "a   b");
        assert_eq!(ranges, [0..4, 4..5]);
    }

    #[test]
    fn marks_past_the_line_stop_at_their_end() {
        let spans = CellSpans::Ranges(vec![1..9, 9..12]);
        let (text, ranges) = marks("\tb\n", &spans);
        assert_eq!(text, "    b");
        assert_eq!(ranges, one(4..5));
    }

    #[test]
    fn a_match_splits_the_spans_and_marks_under_it() {
        assert_eq!(
            runs(&[(0..2, K)], &one(1..4), &[(3..5, Matched::Current)]),
            [
                run(0..1, Some(K), false),
                run(1..2, Some(K), true),
                run(2..3, None, true),
                found(run(3..4, None, true), Matched::Current),
                found(run(4..5, None, false), Matched::Current),
            ]
        );
    }

    #[test]
    fn matches_alone_pass_through_with_their_kind() {
        let matched = [(0..1, Matched::Other), (2..4, Matched::Current)];
        assert_eq!(
            runs::<char>(&[], &[], &matched),
            [
                found(run(0..1, None, false), Matched::Other),
                found(run(2..4, None, false), Matched::Current),
            ]
        );
    }

    #[test]
    fn a_match_after_a_tab_moves_with_the_text_and_stops_at_the_line_end() {
        // `b` is byte 2 of the line, 4 once the tab runs to column 4; `c\n` stops at `c`.
        let matched = [(2..3, Matched::Current), (3..5, Matched::Other)];
        let (text, runs) = cell::<char>("a\tbc\n", &CellSpans::None, &[], &matched, TAB_WIDTH);
        assert_eq!(text, "a   bc");
        assert_eq!(
            runs,
            [
                found(run(4..5, None, false), Matched::Current),
                found(run(5..6, None, false), Matched::Other),
            ]
        );
    }
}
