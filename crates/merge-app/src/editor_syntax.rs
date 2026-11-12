//! The editors' syntax colours above `syntax-whole-file-max-lines` (`docs/DESIGN.md` §F4.1). The
//! highlighter cuts the text into chunks as the top panes do and parses the chunks on screen,
//! both off the UI thread. An edit parses its chunk again; until that lands, the chunk keeps
//! its old spans, with none on the edited lines.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::input::{
    FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter, InputHighlighterFactory, Rope,
    RopeExt as _,
};
use gpui_kit::component::input::EditorState;
use gpui_kit::{AsyncApp, Context, HighlightStyle, SharedString, WeakEntity, Window};
use merge_syntax::syntax::{self, CHUNK_LINES, Cut, Grammar, LineSpans};
use merge_view::chunks::{ChunkId, ChunkedSpans, Limits};

/// A chunk that an edit makes longer than this is cut again.
const MAX_CHUNK_LINES: usize = 2 * CHUNK_LINES;

/// A factory that gives editors of language `name` a chunked highlighter of `grammar`, and
/// editors of any other language none.
pub(crate) fn factory(name: &'static str, grammar: Arc<Grammar>) -> InputHighlighterFactory {
    Rc::new(move |language: &str| {
        (language == name).then(|| {
            let highlighter: Box<dyn InputHighlighter> =
                Box::new(ChunkedHighlighter::new(name, Arc::clone(&grammar)));
            highlighter
        })
    })
}

/// The first line of each chunk of lines `lines` of `text`, `chunk_lines` lines or a little
/// more each, as `syntax::chunk_starts` cuts a [`merge_core::text::Text`]. One pass over the
/// text's bytes.
pub(crate) fn cut(text: &Rope, lines: Range<usize>, chunk_lines: usize) -> Vec<usize> {
    let count = text.lines_len();
    let lines = lines.start.min(count)..lines.end.min(count);
    let offset = |row: usize| {
        if row >= count { text.len() } else { text.line_start_offset(row) }
    };
    let mut cut = Cut::new(lines.clone(), chunk_lines);
    let mut first = true;
    let mut after_blank = false;
    let mut line = |head: (Option<char>, Option<char>), blank: bool| {
        if !first {
            cut.line(syntax::starts_item(head.0, head.1) && after_blank);
        }
        first = false;
        after_blank = blank;
    };
    let mut scan = LineScan::new();
    for chunk in text.slice(offset(lines.start)..offset(lines.end)).chunks() {
        scan.feed(chunk, &mut line);
    }
    scan.end(&mut line);
    cut.finish()
}

/// What [`cut`] needs of each line, read across the rope's chunks: its first two characters
/// and whether it is blank.
struct LineScan {
    head: (Option<char>, Option<char>),
    blank: bool,
    /// The line is decided; the rest of it up to its `\n` is skipped.
    decided: bool,
}

impl LineScan {
    const fn new() -> Self {
        Self { head: (None, None), blank: true, decided: false }
    }

    fn feed(&mut self, chunk: &str, line: &mut impl FnMut((Option<char>, Option<char>), bool)) {
        let mut rest = chunk;
        while !rest.is_empty() {
            if self.decided {
                let Some(at) = rest.find('\n') else {
                    return;
                };
                self.end(line);
                rest = &rest[at + 1..];
                continue;
            }
            let mut taken = rest.len();
            for (at, c) in rest.char_indices() {
                if c == '\n' {
                    self.end(line);
                    taken = at + 1;
                    break;
                }
                match self.head {
                    (None, _) => self.head.0 = Some(c),
                    (Some(_), None) => self.head.1 = Some(c),
                    (Some(_), Some(_)) => {}
                }
                self.blank &= c.is_whitespace();
                if self.head.1.is_some() && !self.blank {
                    self.decided = true;
                    taken = at + c.len_utf8();
                    break;
                }
            }
            rest = &rest[taken..];
        }
    }

    /// The line ends; the next starts.
    fn end(&mut self, line: &mut impl FnMut((Option<char>, Option<char>), bool)) {
        line(self.head, self.blank);
        *self = Self::new();
    }
}

/// A cut of one version of an editor's text, to run off the UI thread.
pub(crate) struct EditorCut {
    version: u64,
    text: Rope,
}

impl EditorCut {
    /// The chunks' first lines and the text's line count.
    pub(crate) fn run(&self) -> (Vec<usize>, usize) {
        let count = self.text.lines_len();
        (cut(&self.text, 0..count, CHUNK_LINES), count)
    }
}

/// One chunk's lines, to parse off the UI thread.
pub(crate) struct EditorParse {
    id: ChunkId,
    grammar: Arc<Grammar>,
    source: String,
}

impl EditorParse {
    pub(crate) fn run(&self) -> LineSpans {
        syntax::highlight_source(&self.grammar, &self.source)
    }
}

/// An editor's text and its spans, chunk by chunk.
pub(crate) struct EditorSyntax {
    grammar: Arc<Grammar>,
    /// The text as of the last update; a clone is O(1).
    text: Rope,
    /// Counts the updates, so a cut of an older text is known.
    version: u64,
    /// None until the cut of the text lands.
    chunks: Option<ChunkedSpans>,
    cut_wanted: bool,
    cutting: bool,
}

impl EditorSyntax {
    pub(crate) fn new(grammar: Arc<Grammar>) -> Self {
        Self {
            grammar,
            text: Rope::new(),
            version: 0,
            chunks: None,
            cut_wanted: false,
            cutting: false,
        }
    }

    /// The text is `text` after `edit`. Until the first cut lands, and when the edit's rows
    /// do not match the chunks, the text cuts afresh.
    pub(crate) fn update(&mut self, edit: &InputEdit, text: &Rope) {
        self.text = text.clone();
        self.version += 1;
        let Some(chunks) = &mut self.chunks else {
            self.cut_wanted = true;
            return;
        };
        let start = edit.start_position.row;
        let old = start..edit.old_end_position.row + 1;
        let count = edit.new_end_position.row.saturating_sub(start) + 1;
        let id = chunks.edit(old, count);
        chunks.split(id, MAX_CHUNK_LINES, |lines| cut(text, lines, CHUNK_LINES));
        if chunks.line_count() != text.lines_len() {
            tracing::debug!("an edit's rows do not match the text; the chunks cut afresh");
            self.reset(text);
        }
    }

    /// The text is `text`, to cut afresh; it draws plain until the cut lands.
    pub(crate) fn reset(&mut self, text: &Rope) {
        self.text = text.clone();
        self.version += 1;
        self.chunks = None;
        self.cut_wanted = true;
    }

    /// The cut to run, when one is wanted and none runs.
    pub(crate) fn take_cut(&mut self) -> Option<EditorCut> {
        if !self.cut_wanted || self.cutting {
            return None;
        }
        self.cut_wanted = false;
        self.cutting = true;
        Some(EditorCut { version: self.version, text: self.text.clone() })
    }

    /// The cut of version `version` has landed. A cut of an older text is dropped, and the
    /// text cuts again.
    pub(crate) fn cut_landed(&mut self, version: u64, starts: Vec<usize>, line_count: usize) {
        self.cutting = false;
        if version == self.version {
            self.chunks = Some(ChunkedSpans::new(starts, line_count, Limits::DEFAULT));
        } else {
            self.cut_wanted = true;
        }
    }

    /// Runs that cover `range` in order, each line's syntax styles resolved by `resolver` and
    /// default styles between them. Its lines' chunks queue.
    pub(crate) fn styles(
        &mut self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut runs = Vec::new();
        let mut at = range.start;
        let Some(chunks) = &mut self.chunks else {
            return vec![(range.clone(), HighlightStyle::default())];
        };
        let first = self.text.offset_to_point(range.start).row;
        let last = self.text.offset_to_point(range.end).row;
        for row in first..=last.min(self.text.lines_len().saturating_sub(1)) {
            chunks.want(row);
            let Some(spans) = chunks.line(row) else {
                continue;
            };
            let line_start = self.text.line_start_offset(row);
            let line_end = line_start + self.text.slice_line(row).len();
            for span in spans {
                let start = (line_start + span.range().start).max(at);
                let end = (line_start + span.range().end).min(line_end).min(range.end);
                if start >= end {
                    continue;
                }
                let style = span
                    .captures()
                    .filter_map(|capture| resolver.style(self.grammar.capture_name(capture)?))
                    .reduce(HighlightStyle::highlight);
                let Some(style) = style else {
                    continue;
                };
                if start > at {
                    runs.push((at..start, HighlightStyle::default()));
                }
                runs.push((start..end, style));
                at = end;
            }
        }
        if at < range.end {
            runs.push((at..range.end, HighlightStyle::default()));
        }
        runs
    }

    /// A queued chunk to parse now, while a parse is free.
    pub(crate) fn next_parse(&mut self) -> Option<EditorParse> {
        let (id, lines) = self.chunks.as_mut()?.next_parse()?;
        let count = self.text.lines_len();
        let offset = |row: usize| {
            if row >= count { self.text.len() } else { self.text.line_start_offset(row) }
        };
        let source = self.text.slice(offset(lines.start)..offset(lines.end)).to_string();
        Some(EditorParse { id, grammar: Arc::clone(&self.grammar), source })
    }

    pub(crate) fn landed(&mut self, id: ChunkId, spans: Option<LineSpans>) {
        if let Some(chunks) = &mut self.chunks {
            chunks.landed(id, spans);
        }
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.chunks.as_ref().map_or(0, ChunkedSpans::pending)
    }
}

/// Where a highlighter's parses run, and the editor they redraw.
#[derive(Clone)]
struct Spawner {
    cx: AsyncApp,
    editor: WeakEntity<EditorState>,
}

/// An editor highlighter over [`EditorSyntax`].
struct ChunkedHighlighter {
    language: SharedString,
    syntax: Rc<RefCell<EditorSyntax>>,
    spawner: Option<Spawner>,
}

impl ChunkedHighlighter {
    fn new(language: &'static str, grammar: Arc<Grammar>) -> Self {
        Self {
            language: SharedString::new_static(language),
            syntax: Rc::new(RefCell::new(EditorSyntax::new(grammar))),
            spawner: None,
        }
    }

    fn start(&self) {
        if let Some(spawner) = &self.spawner {
            start(&self.syntax, spawner);
        }
    }
}

/// Cut the text and parse the queued chunks off the UI thread; the editor redraws as each
/// lands.
fn start(syntax: &Rc<RefCell<EditorSyntax>>, spawner: &Spawner) {
    let job = syntax.borrow_mut().take_cut();
    if let Some(job) = job {
        let syntax = Rc::clone(syntax);
        let spawner = spawner.clone();
        spawner
            .cx
            .clone()
            .spawn(async move |cx| {
                let version = job.version;
                let (starts, count) =
                    cx.background_executor().spawn(async move { job.run() }).await;
                syntax.borrow_mut().cut_landed(version, starts, count);
                start(&syntax, &spawner);
                if spawner.editor.update(cx, |_, cx| cx.notify()).is_err() {
                    // The editor has closed; nothing shows the colours.
                    tracing::trace!("a cut landed after its editor closed");
                }
            })
            .detach();
    }
    loop {
        let Some(parse) = syntax.borrow_mut().next_parse() else {
            return;
        };
        let syntax = Rc::clone(syntax);
        let spawner = spawner.clone();
        spawner
            .cx
            .clone()
            .spawn(async move |cx| {
                let id = parse.id;
                let spans = cx.background_executor().spawn(async move { parse.run() }).await;
                syntax.borrow_mut().landed(id, Some(spans));
                start(&syntax, &spawner);
                if spawner.editor.update(cx, |_, cx| cx.notify()).is_err() {
                    // The editor has closed; nothing shows the colours.
                    tracing::trace!("a chunk landed after its editor closed");
                }
            })
            .detach();
    }
}

impl InputHighlighter for ChunkedHighlighter {
    fn language(&self) -> SharedString {
        self.language.clone()
    }

    fn update(
        &mut self,
        edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        cx: &mut Context<EditorState>,
    ) {
        self.spawner.get_or_insert_with(|| Spawner { cx: cx.to_async(), editor: cx.weak_entity() });
        match edit {
            Some(edit) => self.syntax.borrow_mut().update(&edit, text),
            None => self.syntax.borrow_mut().reset(text),
        }
        self.start();
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let styles = self.syntax.borrow_mut().styles(range, resolver);
        self.start();
        styles
    }

    /// Folding is off in triad's editors.
    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::base::input::Point;
    use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};
    use merge_syntax::language::Language;

    use super::*;
    use std::fmt::Write as _;

    const DEMO: &str = include_str!("../../../fixtures/demo/base.txt");

    fn rust() -> Arc<Grammar> {
        crate::syntax::grammar(Language::Rust).expect("Rust is built in")
    }

    /// An edit that says nothing of the text, as an editor's first update may.
    fn any_edit() -> InputEdit {
        InputEdit {
            start_byte: 0,
            old_end_byte: 0,
            new_end_byte: 0,
            start_position: Point::new(0, 0),
            old_end_position: Point::new(0, 0),
            new_end_position: Point::new(0, 0),
        }
    }

    /// The cut that is wanted, run and landed.
    fn finish_cut(syntax: &mut EditorSyntax) {
        while let Some(cut) = syntax.take_cut() {
            let (starts, count) = cut.run();
            syntax.cut_landed(cut.version, starts, count);
        }
    }

    fn opened(text: &str) -> EditorSyntax {
        let mut syntax = EditorSyntax::new(rust());
        syntax.update(&any_edit(), &Rope::from_str(text));
        finish_cut(&mut syntax);
        syntax
    }

    /// Every chunk queued, parsed and landed, as styles over each line ask. Returns the
    /// parses run.
    fn parse_all(syntax: &mut EditorSyntax) -> usize {
        let theme = HighlightTheme::default_light();
        finish_cut(syntax);
        let mut parses = 0;
        for row in 0..syntax.text.lines_len() {
            let start = syntax.text.line_start_offset(row);
            syntax.styles(&(start..start), &*theme);
            while let Some(parse) = syntax.next_parse() {
                syntax.landed(parse.id, Some(parse.run()));
                parses += 1;
            }
        }
        parses
    }

    /// `s` inserted at the start of row `row` of `old`: the edit and the text after it.
    fn insert(old: &Rope, row: usize, s: &str) -> (InputEdit, Rope) {
        let at = old.line_start_offset(row);
        let mut new = old.clone();
        new.replace(at..at, s);
        let edit = InputEdit {
            start_byte: at,
            old_end_byte: at,
            new_end_byte: at + s.len(),
            start_position: old.offset_to_point(at),
            old_end_position: old.offset_to_point(at),
            new_end_position: new.offset_to_point(at + s.len()),
        };
        (edit, new)
    }

    /// The style of each byte of `length`.
    fn per_byte(length: usize, styles: &[(Range<usize>, HighlightStyle)]) -> Vec<HighlightStyle> {
        let mut bytes = vec![HighlightStyle::default(); length];
        for (range, style) in styles {
            bytes[range.clone()].fill(*style);
        }
        bytes
    }

    /// The first line where `syntax`'s styles differ from gpui-component's highlighter over
    /// the whole text.
    fn first_difference(syntax: &mut EditorSyntax) -> Option<String> {
        let theme = HighlightTheme::default_light();
        let text = syntax.text.clone();
        let mut editor = SyntaxHighlighter::new("rust");
        editor.update(None, &text, None);
        let whole = 0..text.len();
        let ours = per_byte(whole.end, &syntax.styles(&whole, &*theme));
        let theirs = per_byte(whole.end, &editor.styles(&whole, &*theme));
        (0..text.lines_len()).find_map(|row| {
            let start = text.line_start_offset(row);
            let end = start + text.slice_line(row).len();
            (ours[start..end] != theirs[start..end])
                .then(|| format!("line {row}: {:?}", text.slice_line(row).to_string()))
        })
    }

    #[test]
    fn the_styles_match_the_editors_highlighter_over_two_chunks() {
        crate::syntax::register_queries();
        let mut syntax = opened(&DEMO.repeat(3));
        assert_eq!(parse_all(&mut syntax), 2, "2 940 lines cut once");
        assert_eq!(first_difference(&mut syntax), None);
    }

    #[test]
    fn before_its_chunk_lands_a_range_is_one_plain_run() {
        let theme = HighlightTheme::default_light();
        let mut syntax = opened("fn a() {}\n");
        assert_eq!(syntax.styles(&(0..9), &*theme), [(0..9, HighlightStyle::default())]);
        assert_eq!(syntax.pending(), 1);
        parse_all(&mut syntax);
        let styles = syntax.styles(&(0..9), &*theme);
        assert_eq!(styles.first().map(|(range, _)| range.clone()), Some(0..2));
        assert_eq!(styles.last().map(|(range, _)| range.end), Some(9));
        assert!(styles.windows(2).all(|pair| pair[0].0.end == pair[1].0.start));
    }

    #[test]
    fn an_edit_keeps_the_old_colours_until_its_chunk_parses_again() {
        let theme = HighlightTheme::default_light();
        let mut syntax = opened(DEMO);
        parse_all(&mut syntax);
        let old = syntax.text.clone();
        let line = |text: &Rope, row: usize| {
            let start = text.line_start_offset(row);
            start..start + text.slice_line(row).len()
        };
        let row12 = line(&old, 12);
        let before: Vec<_> = syntax
            .styles(&row12, &*theme)
            .into_iter()
            .map(|(range, style)| (range.start - row12.start..range.end - row12.start, style))
            .collect();
        assert!(
            before.iter().any(|(_, style)| *style != HighlightStyle::default()),
            "line 12 is coloured"
        );
        let (edit, new) = insert(&old, 11, "/* x\n*/\n");
        syntax.update(&edit, &new);
        // The edit's lines, 11 to 13 now, have no colours yet. Old line 12,
        // `use crate::text::Text;`, is now line 14, its colours kept.
        let row14 = line(&new, 14);
        let kept: Vec<_> = syntax
            .styles(&row14, &*theme)
            .into_iter()
            .map(|(range, style)| (range.start - row14.start..range.end - row14.start, style))
            .collect();
        assert_eq!(kept, before);
        assert_eq!(
            syntax.styles(&line(&new, 13), &*theme),
            [(line(&new, 13), HighlightStyle::default())]
        );
        assert_eq!(parse_all(&mut syntax), 1, "only the edited chunk parses again");
        assert_eq!(first_difference(&mut syntax), None);
    }

    #[test]
    fn an_edit_that_opens_a_comment_recolours_the_lines_after_it() {
        let mut syntax = opened(DEMO);
        parse_all(&mut syntax);
        let (edit, new) = insert(&syntax.text.clone(), 5, "/*\n");
        syntax.update(&edit, &new);
        parse_all(&mut syntax);
        assert_eq!(first_difference(&mut syntax), None);
    }

    #[test]
    fn a_paste_of_many_lines_cuts_its_chunk_again() {
        let mut syntax = opened(&"fn a() {}\n".repeat(10));
        parse_all(&mut syntax);
        let pasted = (0..5000).fold(String::new(), |mut pasted, n| {
            write!(pasted, "\nfn b{n}() {{}}\n").expect("a String takes any write");
            pasted
        });
        let (edit, new) = insert(&syntax.text.clone(), 5, &pasted);
        syntax.update(&edit, &new);
        let theme = HighlightTheme::default_light();
        let far = new.line_start_offset(8000);
        syntax.styles(&(far..far), &*theme);
        let parse = syntax.next_parse().expect("the far chunk queues");
        let lines = parse.source.lines().count();
        assert!((CHUNK_LINES..=MAX_CHUNK_LINES).contains(&lines), "{lines} lines");
    }

    #[test]
    fn rows_that_do_not_match_the_text_cut_afresh() {
        let mut syntax = opened("fn a() {}\n");
        parse_all(&mut syntax);
        // The edit says nothing changed, but the text has four more lines.
        let new = Rope::from_str("fn a() {}\nfn b() {}\n\n// c\nlet\n");
        syntax.update(&any_edit(), &new);
        parse_all(&mut syntax);
        assert_eq!(first_difference(&mut syntax), None);
        let theme = HighlightTheme::default_light();
        let comment = new.line_start_offset(3);
        let styles = syntax.styles(&(comment..comment + 4), &*theme);
        assert_eq!(styles.len(), 1);
        assert_ne!(styles[0].1, HighlightStyle::default(), "the comment is coloured");
    }

    #[test]
    fn a_rope_cuts_as_a_text_of_the_same_lines_does() {
        let samples = [
            DEMO.repeat(3),
            "a\n\nb\n\n}\n\n</c>\n\n<d>\n  \ne\r\n\r\nf\n\n\u{e9}g\n\n".repeat(40),
            "\n".repeat(30),
            "x".repeat(5000),
            String::new(),
        ];
        for sample in &samples {
            let text = merge_core::text::Text::decode(sample.as_bytes().to_vec()).expect("decodes");
            let rope = Rope::from_str(sample);
            for chunk_lines in [1, 3, 7, 64, CHUNK_LINES] {
                assert_eq!(
                    cut(&rope, 0..text.line_count(), chunk_lines),
                    syntax::chunk_starts(&text, chunk_lines),
                    "{} lines, chunks of {chunk_lines}",
                    text.line_count()
                );
            }
        }
    }

    #[test]
    fn a_cut_of_some_lines_reads_only_them() {
        let sample = "a\n\nb\n\nc\n\nd\n\ne\n";
        let rope = Rope::from_str(sample);
        let text = merge_core::text::Text::decode(sample.as_bytes().to_vec()).expect("decodes");
        // Chunks of 2 over lines 2..9: from 2, line 4 (`c`) is top-level, then 6, then 8.
        assert_eq!(cut(&rope, 2..9, 2), [2, 4, 6, 8]);
        assert_eq!(
            cut(&rope, 2..9, 2),
            syntax::cut(2..9, 2, (3..9).map(|n| n % 2 == 0)),
            "every even line after a blank one is top-level"
        );
        assert_eq!(cut(&rope, 0..text.line_count(), 2), syntax::chunk_starts(&text, 2));
        assert_eq!(cut(&rope, 5..5, 2), [0; 0]);
    }

    #[test]
    fn a_cut_of_an_older_text_is_dropped_and_the_text_cuts_again() {
        let mut syntax = EditorSyntax::new(rust());
        let first = Rope::from_str("fn a() {}\n");
        syntax.update(&any_edit(), &first);
        let stale = syntax.take_cut().expect("the first text cuts");
        assert!(syntax.take_cut().is_none(), "one cut at a time");
        let (edit, second) = insert(&first, 1, "// b\n");
        syntax.update(&edit, &second);
        let (starts, count) = stale.run();
        syntax.cut_landed(stale.version, starts, count);
        let theme = HighlightTheme::default_light();
        assert_eq!(
            syntax.styles(&(0..second.len()), &*theme),
            [(0..second.len(), HighlightStyle::default())],
            "no chunks from the older text"
        );
        assert_eq!(syntax.pending(), 0, "nothing queues without chunks");
        let again = syntax.take_cut().expect("the newer text cuts");
        assert_eq!(again.text.to_string(), second.to_string());
        let (starts, count) = again.run();
        syntax.cut_landed(again.version, starts, count);
        parse_all(&mut syntax);
        assert_eq!(first_difference(&mut syntax), None);
    }
}
