//! The top panes' syntax colours as capture spans per line (`docs/PLAN.md` §F4, §F6.3).
//!
//! A tree-sitter tree costs about 90 bytes per byte of text, so a whole 100k-line input does
//! not fit the memory target. The text parses in chunks of about [`CHUNK_LINES`] lines
//! instead, each cut before a top-level line, and only the spans stay. A span keeps every
//! capture over it, outermost first, up to [`CAPTURE_DEPTH`]; the theme merges their styles
//! in that order, so an inner capture the theme leaves unstyled keeps its outer colour. Of
//! two captures of the same node, the first counts. Captures merge in order of where they
//! start, the text's own before injected ones.
//!
//! A grammar's injections parse too, one level deep: Rust's macro bodies parse as Rust, and
//! HTML's scripts as JavaScript once [`Grammar::inject`] gives it that grammar. In an injected
//! parse, a capture that starts inside the previous one is skipped, unless its pattern sets
//! `highlight.allow-overlap`. Injections of languages without a grammar are left plain, as
//! are combined injections.

use std::ops::Range;
use std::sync::Arc;

use merge_core::text::Text;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator as _};

/// Lines per chunk, before the cut moves on to the next top-level line.
pub const CHUNK_LINES: usize = 2000;
/// Captures a span keeps, innermost last. A deeper nest drops its outermost captures.
pub const CAPTURE_DEPTH: usize = 3;
/// An empty capture slot.
const NO_CAPTURE: u16 = u16::MAX;
const NO_CAPTURES: [u16; CAPTURE_DEPTH] = [NO_CAPTURE; CAPTURE_DEPTH];

/// A grammar and its compiled highlight query.
#[derive(Debug)]
pub struct Grammar {
    language: Language,
    name: Box<str>,
    query: Query,
    injections: Option<Injections>,
    /// The grammars of other languages its injections name. Their captures follow the
    /// grammar's own in [`Span::captures`], in this order.
    injected: Vec<Injected>,
}

/// The injection patterns and their content capture.
#[derive(Debug)]
struct Injections {
    query: Query,
    content: u32,
}

/// Another language's grammar, and the index its first capture takes.
#[derive(Debug)]
struct Injected {
    name: Box<str>,
    grammar: Arc<Grammar>,
    first: u16,
}

const CONTENT: &str = "injection.content";
const LANGUAGE: &str = "injection.language";
const COMBINED: &str = "injection.combined";
const ALLOW_OVERLAP: &str = "highlight.allow-overlap";

#[derive(Debug, thiserror::Error)]
pub enum GrammarError {
    #[error("the highlight query does not compile")]
    Query(#[source] tree_sitter::QueryError),
    #[error("the injection query does not compile")]
    Injections(#[source] tree_sitter::QueryError),
    #[error("the highlight query has {0} captures, more than a span can name")]
    Captures(usize),
    #[error("the grammar does not suit the linked tree-sitter")]
    Version(#[source] tree_sitter::LanguageError),
}

impl Grammar {
    /// The grammar `name`, with its highlight and injection queries. An empty injection
    /// query injects nothing.
    ///
    /// # Errors
    ///
    /// `language` does not suit the linked tree-sitter, a query does not compile for it, or
    /// the highlight query has more captures than a [`Span`] names.
    pub fn new(
        language: Language,
        name: &str,
        highlights: &str,
        injections: &str,
    ) -> Result<Self, GrammarError> {
        Parser::new().set_language(&language).map_err(GrammarError::Version)?;
        let query = Query::new(&language, highlights).map_err(GrammarError::Query)?;
        let count = query.capture_names().len();
        if count >= usize::from(NO_CAPTURE) {
            return Err(GrammarError::Captures(count));
        }
        let injections = if injections.trim().is_empty() {
            None
        } else {
            let query = Query::new(&language, injections).map_err(GrammarError::Injections)?;
            query.capture_index_for_name(CONTENT).map(|content| Injections { query, content })
        };
        Ok(Self { language, name: name.into(), query, injections, injected: Vec::new() })
    }

    /// Parse the injections of `name` with `grammar`'s highlight query. `grammar`'s own
    /// injections do not run.
    ///
    /// # Errors
    ///
    /// With `grammar`'s captures, the grammar has more than a [`Span`] names. It keeps the
    /// grammars injected before.
    pub fn inject(&mut self, name: &str, grammar: Arc<Grammar>) -> Result<(), GrammarError> {
        let first = self.capture_count();
        let total = first + grammar.query.capture_names().len();
        if total >= usize::from(NO_CAPTURE) {
            return Err(GrammarError::Captures(total));
        }
        self.injected.push(Injected {
            name: name.into(),
            grammar,
            first: u16::try_from(first).map_err(|_| GrammarError::Captures(total))?,
        });
        Ok(())
    }

    /// The other languages the injection patterns name, each once, in pattern order.
    pub fn injected_languages(&self) -> impl Iterator<Item = &str> {
        let query = self.injections.as_ref().map(|injections| &injections.query);
        let patterns = query.map_or(0, Query::pattern_count);
        let mut seen = Vec::new();
        (0..patterns)
            .filter_map(move |pattern| injected_language(query?, pattern))
            .filter(|&name| name != &*self.name)
            .filter(move |&name| {
                let first = !seen.contains(&name);
                seen.push(name);
                first
            })
    }

    /// The name of capture `capture`, as a [`Span::captures`] gives it.
    #[must_use]
    pub fn capture_name(&self, capture: usize) -> Option<&str> {
        let own = self.query.capture_names();
        if let Some(&name) = own.get(capture) {
            return Some(name);
        }
        let injected =
            self.injected.iter().rev().find(|injected| usize::from(injected.first) <= capture)?;
        let names = injected.grammar.query.capture_names();
        names.get(capture - usize::from(injected.first)).copied()
    }

    /// Captures of the grammar and its injected grammars.
    fn capture_count(&self) -> usize {
        self.injected.last().map_or(self.query.capture_names().len(), |last| {
            usize::from(last.first) + last.grammar.query.capture_names().len()
        })
    }

    /// The grammar that parses an injection of `name`, and the index its first capture takes.
    fn injecting(&self, name: &str) -> Option<(&Self, u16)> {
        if name == &*self.name {
            return Some((self, 0));
        }
        self.injected
            .iter()
            .find(|injected| &*injected.name == name)
            .map(|injected| (&*injected.grammar, injected.first))
    }
}

/// A run of one set of captures in bytes of its line, terminator excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    stack: [u16; CAPTURE_DEPTH],
}

impl Span {
    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    /// The captures over the span, outermost first.
    pub fn captures(self) -> impl Iterator<Item = usize> {
        self.stack.into_iter().take_while(|&capture| capture != NO_CAPTURE).map(usize::from)
    }
}

/// Put `capture` innermost on a byte's stack, dropping the outermost when it is full.
fn push(stack: &mut [u16; CAPTURE_DEPTH], capture: u16) {
    if let Some(free) = stack.iter().position(|&slot| slot == NO_CAPTURE) {
        stack[free] = capture;
    } else {
        stack.rotate_left(1);
        stack[CAPTURE_DEPTH - 1] = capture;
    }
}

/// Every line's spans, in one buffer.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LineSpans {
    /// Line n's spans end at `ends[n]`.
    ends: Vec<u32>,
    spans: Vec<Span>,
}

impl LineSpans {
    /// Line `n`'s spans, sorted and disjoint; none past the last line.
    #[must_use]
    pub fn line(&self, n: usize) -> &[Span] {
        let Some(&end) = self.ends.get(n) else {
            return &[];
        };
        let start = n.checked_sub(1).map_or(0, |before| self.ends[before]);
        &self.spans[start as usize..end as usize]
    }

    /// Lines `lines` become `count` lines without spans; the lines after them follow. Lines
    /// past the last line count as having none.
    pub fn splice(&mut self, lines: Range<usize>, count: usize) {
        let total = self.ends.len();
        let lines = lines.start.min(total)..lines.end.min(total).max(lines.start.min(total));
        let span_start = |n: usize| n.checked_sub(1).map_or(0, |before| self.ends[before]);
        let (first, last) = (span_start(lines.start), span_start(lines.end));
        self.spans.drain(first as usize..last as usize);
        self.ends.splice(lines.clone(), std::iter::repeat_n(first, count));
        let removed = last - first;
        self.ends[lines.start + count..].iter_mut().for_each(|end| *end -= removed);
    }

    /// `lines` lines with one span, of no capture, at `bytes` of line `line`.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn with_span_on(lines: usize, line: usize, bytes: Range<u32>) -> Self {
        let mut spans = Self::default();
        for n in 0..lines {
            if n == line {
                spans.spans.push(Span { start: bytes.start, end: bytes.end, stack: NO_CAPTURES });
            }
            spans.end_line();
        }
        spans
    }

    fn end_line(&mut self) {
        let end = u32::try_from(self.spans.len()).unwrap_or(u32::MAX);
        self.ends.push(end);
    }
}

/// The spans of every line of `text` under `grammar`, parsed `chunk_lines` lines at a time.
#[must_use]
pub fn highlight(grammar: &Grammar, text: &Text, chunk_lines: usize) -> LineSpans {
    let mut parser = parser(grammar);
    let count = text.line_count();
    let mut spans = LineSpans { ends: Vec::with_capacity(count), spans: Vec::new() };
    let starts = chunk_starts(text, chunk_lines);
    let ends = starts.iter().skip(1).copied().chain([count]);
    for (start, end) in starts.iter().copied().zip(ends) {
        highlight_chunk(grammar, &mut parser, text.span(start..end), &mut spans);
    }
    spans
}

/// The first line of each chunk of `text`, `chunk_lines` lines or a little more each, as
/// [`highlight`] cuts them.
#[must_use]
pub fn chunk_starts(text: &Text, chunk_lines: usize) -> Vec<usize> {
    let count = text.line_count();
    cut(0..count, chunk_lines, (1..count).map(|n| top_level(text, n)))
}

/// The first line of each chunk of lines `lines`, `chunk_lines` lines or a little more each.
/// `tops` says, for each line after the first in order, whether it starts an item at the top
/// level ([`top_level_line`]).
#[must_use]
pub fn cut(
    lines: Range<usize>,
    chunk_lines: usize,
    tops: impl IntoIterator<Item = bool>,
) -> Vec<usize> {
    let mut cut = Cut::new(lines, chunk_lines);
    tops.into_iter().for_each(|top| cut.line(top));
    cut.finish()
}

/// [`cut`] one line at a time: each chunk ends before the first top-level line at or after
/// its `chunk_lines`th line, within another `chunk_lines` lines; without one, at its
/// `chunk_lines`th line.
#[derive(Debug)]
pub struct Cut {
    end: usize,
    size: usize,
    /// The line [`Cut::line`] gives next.
    next: usize,
    /// Where the chunk being read starts.
    start: usize,
    starts: Vec<usize>,
}

impl Cut {
    #[must_use]
    pub fn new(lines: Range<usize>, chunk_lines: usize) -> Self {
        let size = chunk_lines.max(1);
        let mut starts = Vec::with_capacity(lines.len().div_ceil(size));
        if !lines.is_empty() {
            starts.push(lines.start);
        }
        Self { end: lines.end, size, next: lines.start + 1, start: lines.start, starts }
    }

    /// The next line, after the first, starts an item at the top level or not.
    pub fn line(&mut self, top: bool) {
        let n = self.next;
        self.next += 1;
        let want = self.start + self.size;
        if n >= self.end || n < want {
            return;
        }
        if n == want + self.size {
            // No top-level line in the chunk's window: it ends at its `size`th line, and
            // the next chunk's window starts here.
            self.start = want;
            self.starts.push(want);
        }
        if top {
            self.start = n;
            self.starts.push(n);
        }
    }

    /// The chunks' first lines.
    #[must_use]
    pub fn finish(mut self) -> Vec<usize> {
        let want = self.start + self.size;
        if !self.starts.is_empty() && want < self.end {
            // The window ran to the end without a top-level line.
            self.starts.push(want);
        }
        self.starts
    }
}

/// The spans of lines `lines` of `text`, parsed alone; line 0 of the result is
/// `lines.start`. Lines past the end of `text` have none.
#[must_use]
pub fn highlight_lines(grammar: &Grammar, text: &Text, lines: Range<usize>) -> LineSpans {
    let count = text.line_count();
    let lines = lines.start.min(count)..lines.end.min(count);
    highlight_source(grammar, text.span(lines))
}

/// The spans of each line of `source`, parsed alone. Lines end at `\n`.
#[must_use]
pub fn highlight_source(grammar: &Grammar, source: &str) -> LineSpans {
    let mut parser = parser(grammar);
    let mut spans = LineSpans::default();
    highlight_chunk(grammar, &mut parser, source, &mut spans);
    spans
}

fn parser(grammar: &Grammar) -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&grammar.language).expect("Grammar::new loaded the language into a parser");
    parser
}

/// Where the chunk of `text` from line `start` ends, as [`chunk_end_by`] finds it.
#[cfg(test)]
fn chunk_end(text: &Text, start: usize, lines: usize) -> usize {
    chunk_end_by(text.line_count(), start, lines, |n| top_level(text, n))
}

/// Where the chunk from line `start` ends, in lines that end at `count`: the first
/// top-level line at or after `start + lines`, within another `lines` lines; else
/// `start + lines`. The rule [`Cut`] follows a line at a time.
#[cfg(test)]
fn chunk_end_by(
    count: usize,
    start: usize,
    lines: usize,
    top_level: impl Fn(usize) -> bool,
) -> usize {
    let want = start + lines;
    if want >= count {
        return count;
    }
    (want..count.min(want + lines)).find(|&n| top_level(n)).unwrap_or(want)
}

/// Whether a line whose first characters are `first` and `second` starts an item: it starts
/// at column 0 and does not close a block (`}`, `)`, `]`) or an element (`</`).
#[must_use]
pub fn starts_item(first: Option<char>, second: Option<char>) -> bool {
    match (first, second) {
        (None | Some('}' | ')' | ']'), _) | (Some('<'), Some('/')) => false,
        (Some(first), _) => !first.is_whitespace(),
    }
}

/// Whether a line that starts with `head` starts an item at the top level: it starts an item
/// ([`starts_item`]) and follows a blank line.
#[must_use]
pub fn top_level_line(head: &str, after_blank: bool) -> bool {
    let mut chars = head.chars();
    starts_item(chars.next(), chars.next()) && after_blank
}

/// Line `n` of `text` starts an item at the top level; `n` is past the first line.
pub(crate) fn top_level(text: &Text, n: usize) -> bool {
    top_level_line(text.line(n), text.line(n - 1).trim().is_empty())
}

/// The language pattern `pattern` injects into each match on its own; none for a combined
/// injection.
fn injected_language(query: &Query, pattern: usize) -> Option<&str> {
    let settings = query.property_settings(pattern);
    if settings.iter().any(|setting| &*setting.key == COMBINED) {
        return None;
    }
    settings.iter().find(|setting| &*setting.key == LANGUAGE)?.value.as_deref()
}

/// Whether a capture may start inside the one before it in an injected parse.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Nesting {
    /// The text's own parse: every capture counts.
    All,
    /// An injected parse: a capture inside the previous one is skipped.
    Outermost,
}

/// The captures of `query` under `node`, in match order, as (bytes, capture), each capture
/// index raised by `first`.
fn captures(
    query: &Query,
    node: Node<'_>,
    source: &[u8],
    (nesting, first): (Nesting, u16),
    items: &mut Vec<(Range<usize>, u16)>,
) {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, node, source);
    let mut last = 0..0;
    while let Some(found) = matches.next() {
        let overlap = query
            .property_settings(found.pattern_index)
            .iter()
            .any(|setting| &*setting.key == ALLOW_OVERLAP);
        for capture in found.captures {
            let range = capture.node.byte_range();
            let skip = match nesting {
                Nesting::All => range == last,
                Nesting::Outermost => !overlap && range.start < last.end,
            };
            if skip {
                continue;
            }
            // Fewer than `NO_CAPTURE` captures in all, as `Grammar::inject` checks.
            let index = u16::try_from(capture.index)
                .ok()
                .and_then(|index| index.checked_add(first))
                .unwrap_or(NO_CAPTURE);
            items.push((range.clone(), index));
            if nesting == Nesting::All || !overlap {
                last = range;
            }
        }
    }
}

/// The captures of the injections in `tree` of the languages `grammar` has a grammar for,
/// each parsed alone.
fn injected(
    grammar: &Grammar,
    parser: &mut Parser,
    tree: &tree_sitter::Tree,
    source: &[u8],
    items: &mut Vec<(Range<usize>, u16)>,
) {
    let Some(injections) = &grammar.injections else {
        return;
    };
    let mut layers = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&injections.query, tree.root_node(), source);
    while let Some(found) = matches.next() {
        let Some(injecting) = injected_language(&injections.query, found.pattern_index)
            .and_then(|name| grammar.injecting(name))
        else {
            continue;
        };
        let mut ranges: Vec<_> = found
            .captures
            .iter()
            .filter(|capture| capture.index == injections.content)
            .map(|capture| capture.node.range())
            .collect();
        ranges.sort_by_key(|range| (range.start_byte, range.end_byte));
        if !ranges.is_empty() {
            layers.push((injecting, ranges));
        }
    }
    for ((injecting, first), ranges) in layers {
        if let Err(error) = parser.set_language(&injecting.language) {
            tracing::warn!(%error, name = &*injecting.name, "an injected grammar does not load; it shows plain");
            continue;
        }
        if let Err(error) = parser.set_included_ranges(&ranges) {
            tracing::warn!(%error, "an injection's ranges are out of order; it shows plain");
            continue;
        }
        if let Some(layer) = parser.parse(source, None) {
            captures(
                &injecting.query,
                layer.root_node(),
                source,
                (Nesting::Outermost, first),
                items,
            );
        }
    }
    if let Err(error) = parser.set_included_ranges(&[]) {
        // An empty list is the whole text, which tree-sitter always takes.
        tracing::error!(%error, "the parser keeps an injection's ranges");
    }
    if let Err(error) = parser.set_language(&grammar.language) {
        // `Grammar::new` loaded this language, so the linked tree-sitter takes it.
        tracing::error!(%error, "the parser keeps an injected grammar");
    }
}

/// Push each capture onto the stacks of its bytes, in order of where it starts.
fn paint_items(paint: &mut [[u16; CAPTURE_DEPTH]], mut items: Vec<(Range<usize>, u16)>) {
    // Stable, so captures that start together keep their order.
    items.sort_by_key(|(range, _)| range.start);
    for (range, capture) in items {
        paint[range].iter_mut().for_each(|stack| push(stack, capture));
    }
}

fn highlight_chunk(grammar: &Grammar, parser: &mut Parser, text: &str, spans: &mut LineSpans) {
    let source = text.as_bytes();
    let mut paint = vec![NO_CAPTURES; source.len()];
    if let Some(tree) = parser.parse(source, None) {
        let mut items = Vec::new();
        captures(&grammar.query, tree.root_node(), source, (Nesting::All, 0), &mut items);
        injected(grammar, parser, &tree, source, &mut items);
        paint_items(&mut paint, items);
    } else {
        tracing::warn!("the parser gave no tree; the lines show plain");
    }
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']).len();
        push_runs(&paint[at..at + body], spans);
        spans.end_line();
        at += line.len();
    }
}

/// The runs of one capture stack in a line's paint. A line too long for a `u32` offset gets
/// none.
fn push_runs(paint: &[[u16; CAPTURE_DEPTH]], spans: &mut LineSpans) {
    if u32::try_from(paint.len()).is_err() {
        return;
    }
    let mut start = 0;
    for run in paint.chunk_by(|a, b| a == b) {
        let end = start + run.len();
        if run[0] != NO_CAPTURES {
            spans.spans.push(Span {
                start: u32::try_from(start).unwrap_or(u32::MAX),
                end: u32::try_from(end).unwrap_or(u32::MAX),
                stack: run[0],
            });
        }
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn rust() -> Grammar {
        Grammar::new(
            tree_sitter_rust::LANGUAGE.into(),
            RUST,
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
        )
        .expect("the Rust queries compile")
    }

    const RUST: &str = "rust";

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes().to_vec()).expect("decodes")
    }

    /// Line `n`'s spans as (range, innermost capture name).
    fn named<'g>(
        grammar: &'g Grammar,
        spans: &LineSpans,
        n: usize,
    ) -> Vec<(Range<usize>, &'g str)> {
        stacks(grammar, spans, n)
            .into_iter()
            .map(|(range, names)| (range, *names.last().expect("a span has a capture")))
            .collect()
    }

    /// Line `n`'s spans as (range, capture names outermost first).
    fn stacks<'g>(
        grammar: &'g Grammar,
        spans: &LineSpans,
        n: usize,
    ) -> Vec<(Range<usize>, Vec<&'g str>)> {
        spans
            .line(n)
            .iter()
            .map(|span| {
                let names = span
                    .captures()
                    .map(|c| grammar.capture_name(c).expect("a span names the grammar's captures"));
                (span.range(), names.collect())
            })
            .collect()
    }

    /// The host of the injection tests: Rust, which injects `inner` into string contents.
    fn host(injections: &str) -> Grammar {
        Grammar::new(
            tree_sitter_rust::LANGUAGE.into(),
            RUST,
            "(string_literal) @string",
            injections,
        )
        .expect("the host queries compile")
    }

    const INNER: &str = "inner";
    const INJECT_INNER: &str =
        "((string_content) @injection.content (#set! injection.language \"inner\"))";

    /// `inner`: Rust under another name, which captures only identifiers.
    fn inner() -> Arc<Grammar> {
        let grammar =
            Grammar::new(tree_sitter_rust::LANGUAGE.into(), INNER, "(identifier) @inner.name", "")
                .expect("the inner query compiles");
        Arc::new(grammar)
    }

    #[test]
    fn a_function_takes_the_querys_captures() {
        let grammar = rust();
        let spans = highlight(&grammar, &text("fn a() {}\n"), CHUNK_LINES);
        assert_eq!(
            named(&grammar, &spans, 0),
            [
                (0..2, "keyword"),
                (3..4, "function"),
                (4..6, "punctuation.bracket"),
                (7..9, "punctuation.bracket"),
            ]
        );
    }

    #[test]
    fn spans_are_in_bytes_of_their_line_without_the_terminator() {
        let grammar = rust();
        let spans = highlight(&grammar, &text("// a\r\nlet s = \"x\";\r\n"), CHUNK_LINES);
        assert_eq!(named(&grammar, &spans, 0), [(0..4, "comment")]);
        let second = named(&grammar, &spans, 1);
        assert!(second.contains(&(0..3, "keyword")), "{second:?}");
        assert!(second.contains(&(8..11, "string")), "{second:?}");
        assert_eq!(second.last().map(|(range, _)| range.end), Some(12), "{second:?}");
    }

    #[test]
    fn a_comment_across_lines_covers_each() {
        let grammar = rust();
        let spans = highlight(&grammar, &text("/* a\nbc */\n"), CHUNK_LINES);
        assert_eq!(named(&grammar, &spans, 0), [(0..4, "comment")]);
        assert_eq!(named(&grammar, &spans, 1), [(0..5, "comment")]);
    }

    #[test]
    fn blank_lines_and_lines_past_the_end_have_none() {
        let grammar = rust();
        let spans = highlight(&grammar, &text("fn a() {}\n\nfn b() {}\n"), CHUNK_LINES);
        assert_eq!(spans.line(1), []);
        assert_eq!(spans.line(3), []);
        assert_eq!(highlight(&grammar, &text(""), CHUNK_LINES), LineSpans::default());
    }

    const ITEMS: &str = "\
fn a() {
    let x = 1;
}

/// Doc.
fn b() -> u8 {

    2
}

struct C;
";

    #[test]
    fn chunks_cut_before_top_level_lines_and_match_one_parse() {
        let grammar = rust();
        let text = text(ITEMS);
        let whole = highlight(&grammar, &text, CHUNK_LINES);
        // Four-line chunks look at lines 4..8, then 8..11. They cut before `/// Doc.` (line
        // 4) and `struct C;` (line 10). The blank line 6 inside `b` comes before the window
        // and is followed by an indented line; line 8 is a closing brace.
        assert_eq!(chunk_end(&text, 0, 4), 4);
        assert_eq!(chunk_end(&text, 4, 4), 10);
        assert_eq!(chunk_end(&text, 10, 4), 11);
        assert_eq!(highlight(&grammar, &text, 4), whole);
    }

    #[test]
    fn the_chunks_start_where_each_one_before_ends() {
        // The cuts of the test above: 0..4, 4..10, 10..11.
        assert_eq!(chunk_starts(&text(ITEMS), 4), [0, 4, 10]);
        // No top-level line: cuts every two lines.
        assert_eq!(chunk_starts(&text("a\nb\nc\nd\ne\nf\n"), 2), [0, 2, 4]);
        assert_eq!(chunk_starts(&text("a\n"), CHUNK_LINES), [0]);
        assert_eq!(chunk_starts(&text(""), CHUNK_LINES), [0; 0]);
        // A chunk of no lines would never end; it takes one.
        assert_eq!(chunk_starts(&text("a\nb\n"), 0), [0, 1]);
    }

    #[test]
    fn each_chunk_parsed_alone_gives_the_lines_of_the_whole_parse() {
        let grammar = rust();
        let text = text(ITEMS);
        let whole = highlight(&grammar, &text, CHUNK_LINES);
        let starts = chunk_starts(&text, 4);
        let ends = starts.iter().skip(1).copied().chain([text.line_count()]);
        for (start, end) in starts.iter().copied().zip(ends) {
            let chunk = highlight_lines(&grammar, &text, start..end);
            for n in start..end {
                assert_eq!(chunk.line(n - start), whole.line(n), "line {n}");
            }
            assert_eq!(chunk.line(end - start), [], "a chunk holds only its own lines");
        }
        // `fn` of `fn b` on line 5 of the text is line 1 of its chunk, at bytes 0..2.
        let chunk = highlight_lines(&grammar, &text, 4..10);
        assert_eq!(named(&grammar, &chunk, 1).first(), Some(&(0..2, "keyword")));
    }

    #[test]
    fn a_source_parses_as_the_same_lines_of_a_text_do() {
        let grammar = rust();
        let crlf = "fn a() {}\r\n// c\r\nlet";
        let from_text = highlight_lines(&grammar, &text(crlf), 0..3);
        let from_source = highlight_source(&grammar, crlf);
        assert_eq!(from_source, from_text);
        // `// c` is line 1 at bytes 0..4: the CRLF is no part of it.
        assert_eq!(named(&grammar, &from_source, 1), [(0..4, "comment")]);
        assert_eq!(highlight_source(&grammar, ""), LineSpans::default());
    }

    #[test]
    fn a_cut_of_some_lines_starts_at_their_first() {
        // Top-level lines at 13 and 25 of lines 10..30, chunks of 4. From 10: none in 14..18,
        // so 14 (13 is before it). From 14: none in 18..22, so 18. From 18: 25 in 22..26. From
        // 25: none in 29..30, so 29. From 29: 33 is past the end.
        let tops = |lines: Range<usize>| (lines.start + 1..lines.end).map(|n| n == 13 || n == 25);
        assert_eq!(cut(10..30, 4, tops(10..30)), [10, 14, 18, 25, 29]);
        assert_eq!(cut(5..5, 4, tops(5..5)), [0; 0]);
        assert!(top_level_line("fn", true));
        assert!(!top_level_line("fn", false), "no blank line before");
        assert!(!top_level_line(" f", true), "indented");
        assert!(!top_level_line("}", true));
        assert!(!top_level_line("</", true));
        assert!(!top_level_line("", true), "a blank line");
    }

    /// The starts [`chunk_end_by`] gives, one chunk after another.
    fn reference_cut(lines: Range<usize>, size: usize, top: impl Fn(usize) -> bool) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut start = lines.start;
        while start < lines.end {
            starts.push(start);
            start = chunk_end_by(lines.end, start, size, &top);
        }
        starts
    }

    #[test]
    fn a_cut_a_line_at_a_time_follows_the_rule_for_every_pattern_of_twelve_lines() {
        const LINES: usize = 12;
        for size in 1..=4 {
            for lines in 0..=LINES {
                for pattern in 0..1_u32 << lines {
                    let top = |n: usize| pattern >> n & 1 == 1;
                    let range = 3..3 + lines;
                    let shifted = |n: usize| top(n - range.start);
                    let tops = (range.start + 1..range.end).map(shifted);
                    assert_eq!(
                        cut(range.clone(), size, tops),
                        reference_cut(range.clone(), size, shifted),
                        "{lines} lines, chunks of {size}, tops {pattern:b}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_line_starts_an_item_unless_it_is_indented_blank_or_closing() {
        assert!(starts_item(Some('f'), Some('n')));
        assert!(starts_item(Some('<'), Some('a')), "an opening tag");
        assert!(starts_item(Some('<'), None));
        assert!(starts_item(Some('/'), Some('/')), "a comment");
        assert!(!starts_item(None, None));
        assert!(!starts_item(Some(' '), Some('f')));
        assert!(!starts_item(Some('\t'), Some('f')));
        assert!(!starts_item(Some('}'), None));
        assert!(!starts_item(Some(')'), Some(';')));
        assert!(!starts_item(Some(']'), None));
        assert!(!starts_item(Some('<'), Some('/')));
    }

    #[test]
    fn a_splice_replaces_lines_with_empty_ones_and_keeps_the_rest() {
        let grammar = rust();
        // Spans on lines 0 (`fn`, `a`, ...), 1 (comment) and 3 (comment).
        let mut spans = highlight_source(&grammar, "fn a() {}\n// b\n\n// d\n");
        let line3 = spans.line(3).to_vec();
        spans.splice(1..3, 3);
        assert_eq!(spans.line(1), []);
        assert_eq!(spans.line(2), []);
        assert_eq!(spans.line(3), []);
        assert_eq!(spans.line(4), line3);
        assert_eq!(named(&grammar, &spans, 0).first(), Some(&(0..2, "keyword")));
        // Removing lines: line 4 moves up to 1.
        spans.splice(1..4, 0);
        assert_eq!(spans.line(1), line3);
        assert_eq!(spans.line(2), []);
        // Past the last line: lines appended.
        spans.splice(9..12, 2);
        assert_eq!(spans.line(1), line3);
        assert_eq!(spans.line(3), []);
    }

    #[test]
    fn without_a_top_level_line_the_chunk_cuts_where_it_must() {
        let text = text("a\nb\nc\nd\ne\nf\n");
        assert_eq!(chunk_end(&text, 0, 2), 2);
        assert_eq!(chunk_end(&text, 4, 2), 6);
    }

    #[test]
    fn a_closing_brace_after_a_blank_line_is_no_cut() {
        // The window is lines 2..4: line 2 is blank, line 3 closes a block.
        let text = text("a\nb\n\n}\n");
        assert_eq!(chunk_end(&text, 0, 2), 2);
    }

    #[test]
    fn a_closing_tag_after_a_blank_line_is_no_cut() {
        // The window is lines 2..4: line 2 is blank, line 3 closes an element.
        let closing = text("<a>\nb\n\n</a>\n");
        assert_eq!(chunk_end(&closing, 0, 2), 2);
        // An opening tag there starts an item.
        let opening = text("<a>\nb\n\n<b>\n");
        assert_eq!(chunk_end(&opening, 0, 2), 3);
    }

    #[test]
    fn a_later_capture_paints_over_an_earlier_one() {
        // `"a\n"` is bytes 8..13; its escape, captured after it, is 10..12.
        let grammar = rust();
        let spans = highlight(&grammar, &text("let s = \"a\\n\";\n"), CHUNK_LINES);
        let line = named(&grammar, &spans, 0);
        let strings: Vec<_> =
            line.iter().filter(|(range, _)| range.start >= 8 && range.end <= 13).cloned().collect();
        assert_eq!(strings, [(8..10, "string"), (10..12, "escape"), (12..13, "string")]);
    }

    #[test]
    fn of_two_captures_of_one_node_the_first_wins() {
        // A doc comment matches `@comment` before `@comment.documentation`.
        let grammar = rust();
        let spans = highlight(&grammar, &text("/// Doc.\nfn a() {}\n"), CHUNK_LINES);
        assert_eq!(named(&grammar, &spans, 0), [(0..8, "comment")]);
    }

    #[test]
    fn a_span_keeps_the_captures_outside_it() {
        // `#[derive(A)]`: the attribute covers 0..12; `[`, `(`, `)` and `]` are brackets in it.
        let grammar = rust();
        let spans = highlight(&grammar, &text("#[derive(A)]\nstruct S;\n"), CHUNK_LINES);
        let line = stacks(&grammar, &spans, 0);
        assert_eq!(line.first().map(|(range, _)| range.clone()), Some(0..1));
        assert!(line.contains(&(1..2, vec!["attribute", "punctuation.bracket"])), "{line:?}");
        assert!(line.iter().all(|(_, names)| names[0] == "attribute"), "{line:?}");
        assert_eq!(line.last().map(|(range, _)| range.end), Some(12));
    }

    #[test]
    fn a_macro_body_parses_as_rust() {
        // `vec![a.len()]`: `len` is bytes 7..10, a method call once the body parses.
        let grammar = rust();
        let spans = highlight(&grammar, &text("let v = vec![a.len()];\n"), CHUNK_LINES);
        let line = named(&grammar, &spans, 0);
        assert!(line.contains(&(15..18, "function.method")), "{line:?}");
        let plain = Grammar::new(
            tree_sitter_rust::LANGUAGE.into(),
            RUST,
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            "",
        )
        .unwrap();
        let spans = highlight(&plain, &text("let v = vec![a.len()];\n"), CHUNK_LINES);
        let line = named(&plain, &spans, 0);
        assert!(!line.iter().any(|(range, _)| *range == (15..18)), "{line:?}");
    }

    #[test]
    fn another_languages_injection_is_left_plain() {
        let grammar = Grammar::new(
            tree_sitter_rust::LANGUAGE.into(),
            "not-rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
        )
        .unwrap();
        let spans = highlight(&grammar, &text("let v = vec![a.len()];\n"), CHUNK_LINES);
        let line = named(&grammar, &spans, 0);
        assert!(!line.iter().any(|(range, _)| *range == (15..18)), "{line:?}");
    }

    #[test]
    fn an_injection_of_another_language_parses_with_its_grammar() {
        // `"a + b"` is bytes 8..15; `a` and `b` parse as identifiers of `inner`.
        let mut grammar = host(INJECT_INNER);
        grammar.inject(INNER, inner()).unwrap();
        let spans = highlight(&grammar, &text("let s = \"a + b\";\n"), CHUNK_LINES);
        assert_eq!(
            stacks(&grammar, &spans, 0),
            [
                (8..9, vec!["string"]),
                (9..10, vec!["string", "inner.name"]),
                (10..13, vec!["string"]),
                (13..14, vec!["string", "inner.name"]),
                (14..15, vec!["string"]),
            ]
        );
    }

    #[test]
    fn without_its_grammar_an_injection_is_plain() {
        let grammar = host(INJECT_INNER);
        let spans = highlight(&grammar, &text("let s = \"a + b\";\n"), CHUNK_LINES);
        assert_eq!(stacks(&grammar, &spans, 0), [(8..15, vec!["string"])]);
    }

    #[test]
    fn an_injected_grammar_injects_nothing_itself() {
        // The string holds `vec![a.len()]`: as Rust with its injections, `len` (bytes
        // 16..19) is a method call. Injected, the macro body stays unparsed.
        let mut grammar = host(INJECT_INNER);
        grammar.inject(INNER, Arc::new(rust())).unwrap();
        let spans = highlight(&grammar, &text("let s = \"vec![a.len()]\";\n"), CHUNK_LINES);
        let line = named(&grammar, &spans, 0);
        assert!(line.contains(&(9..13, "function.macro")), "{line:?}");
        // Innermost, `len` keeps the host's string capture.
        assert!(line.contains(&(16..19, "string")), "{line:?}");
    }

    #[test]
    fn the_injected_languages_are_the_others_each_once() {
        let grammar = host(
            "((string_content) @injection.content (#set! injection.language \"inner\"))
             ((line_comment) @injection.content (#set! injection.language \"inner\"))
             ((block_comment) @injection.content (#set! injection.language \"rust\"))
             ((char_literal) @injection.content (#set! injection.language \"other\"))
             ((raw_string_literal) @injection.content
               (#set! injection.language \"combined\") (#set! injection.combined))",
        );
        assert_eq!(grammar.injected_languages().collect::<Vec<_>>(), ["inner", "other"]);
        assert_eq!(rust().injected_languages().count(), 0);
    }

    #[test]
    fn an_injection_past_the_capture_limit_is_refused() {
        let mut grammar = host(INJECT_INNER);
        let inner = Arc::new(rust());
        let each = inner.query.capture_names().len();
        // The host has one capture. The injection that takes the total to `NO_CAPTURE` or
        // more is refused, and the grammar keeps the ones before it.
        let fits = (usize::from(NO_CAPTURE) - 1 - 1) / each;
        for _ in 0..fits {
            grammar.inject(INNER, Arc::clone(&inner)).unwrap();
        }
        let error =
            grammar.inject(INNER, Arc::clone(&inner)).expect_err("the captures overflow a span");
        assert!(
            matches!(error, GrammarError::Captures(total) if total == 1 + (fits + 1) * each),
            "{error:?}"
        );
        // The last capture of the last injection that fit is the grammar's last.
        assert_eq!(grammar.capture_name(fits * each), inner.query.capture_names().last().copied());
        assert_eq!(grammar.capture_name(1 + fits * each), None);
    }

    #[test]
    fn captures_merge_in_order_of_their_start() {
        // An injected capture over 0..4 comes after a host capture over 1..2 in match order,
        // but starts first, so it is the outer one.
        let mut paint = vec![NO_CAPTURES; 4];
        paint_items(&mut paint, vec![(1..2, 5), (0..4, 9), (1..2, 6)]);
        assert_eq!(
            paint,
            [
                [9, NO_CAPTURE, NO_CAPTURE],
                [9, 5, 6],
                [9, NO_CAPTURE, NO_CAPTURE],
                [9, NO_CAPTURE, NO_CAPTURE],
            ]
        );
    }

    #[test]
    fn a_deep_nest_keeps_its_innermost_captures() {
        let mut stack = NO_CAPTURES;
        for capture in 0..5 {
            push(&mut stack, capture);
        }
        assert_eq!(stack, [2, 3, 4]);
        let mut short = NO_CAPTURES;
        push(&mut short, 7);
        assert_eq!(short, [7, NO_CAPTURE, NO_CAPTURE]);
    }

    #[test]
    fn a_top_level_line_follows_a_blank_one() {
        // The window is lines 2..4: line 2 is indented, line 3 follows a non-blank line.
        let text = text("a\nb\n  c\nd\n\ne\n");
        assert_eq!(chunk_end(&text, 0, 2), 2);
    }

    #[test]
    fn a_bad_query_is_refused() {
        let error = Grammar::new(tree_sitter_rust::LANGUAGE.into(), RUST, "(no_such_node) @x", "")
            .expect_err("the node type is unknown");
        assert!(matches!(error, GrammarError::Query(_)), "{error:?}");
        let error = Grammar::new(
            tree_sitter_rust::LANGUAGE.into(),
            RUST,
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            "(no_such_node) @injection.content",
        )
        .expect_err("the node type is unknown");
        assert!(matches!(error, GrammarError::Injections(_)), "{error:?}");
    }
}
