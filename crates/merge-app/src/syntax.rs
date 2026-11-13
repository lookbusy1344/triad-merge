//! The language each editor highlights its text as (`docs/DESIGN.md` §F5), and the
//! inputs' capture spans that colour the top panes (§4).

use std::ops::Range;

use std::sync::Arc;

use gpui_kit::HighlightStyle;
use gpui_kit::base::input::InputHighlighterFactory;
use gpui_kit::component::highlighter::{GrammarConfig, HighlightTheme, LanguageRegistry};
use merge_core::merge3::Source;
use merge_core::text::Text;
use merge_syntax::language::Language;
use merge_syntax::syntax::{self, Grammar, LineSpans};
use merge_view::chunks::{ChunkId, ChunkedSpans, Limits};

use crate::window::Syntax;

/// The language name of an editor that highlights nothing.
const PLAIN: &str = "plain";

/// The name gpui-component's language registry knows `language`'s grammar by.
pub(crate) const fn registry_name(language: Language) -> &'static str {
    match language {
        Language::Rust => "rust",
        Language::Go => "go",
        Language::CSharp => "csharp",
        Language::Python => "python",
        Language::Swift => "swift",
        Language::Html => "html",
        Language::JavaScript => "javascript",
        Language::TypeScript => "typescript",
        Language::Tsx => "tsx",
        Language::Json => "json",
        Language::Toml => "toml",
        Language::Yaml => "yaml",
    }
}

/// The name an editor of `language`'s text is created with.
pub(crate) fn editor_language(language: Option<Language>) -> &'static str {
    language.map_or(PLAIN, registry_name)
}

/// A window's syntax colours: the language its file names give, whether the colours show,
/// and the longest text its editors colour (spec §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Colouring {
    language: Option<Language>,
    shown: Syntax,
    max_lines: usize,
}

impl Colouring {
    pub(crate) const fn new(language: Option<Language>, shown: Syntax, max_lines: usize) -> Self {
        Self { language, shown, max_lines }
    }

    /// The language the panes colour by; none while the colours are off.
    pub(crate) fn language(self) -> Option<Language> {
        self.language.filter(|_| self.shown == Syntax::Highlighted)
    }

    /// Whether the colours are on, whether or not a language was found.
    pub(crate) fn shown(self) -> bool {
        self.shown == Syntax::Highlighted
    }

    /// The language the file names give, whether or not the colours show.
    pub(crate) const fn detected(self) -> Option<Language> {
        self.language
    }

    /// The name an editor takes: the language while the colours show, plain otherwise.
    pub(crate) fn editor_language(self) -> &'static str {
        editor_language(self.language())
    }

    /// Whether an editor whose longest text has `lines` lines colours a chunk at a time,
    /// past `syntax-whole-file-max-lines` (spec §6.4).
    pub(crate) const fn chunked(self, lines: usize) -> bool {
        lines > self.max_lines
    }

    /// The chunked highlighter for an editor whose longest text has `lines` lines, past
    /// `syntax-whole-file-max-lines`, of the language the file names give whether or not the colours
    /// show; none within the limit, without a language, or when its grammar does not load.
    pub(crate) fn chunked_factory(self, lines: usize) -> Option<InputHighlighterFactory> {
        let language = self.language.filter(|_| self.chunked(lines))?;
        let grammar = grammar(language)
            .inspect_err(|error| tracing::warn!(%error, "the editors show plain"))
            .ok()?;
        Some(crate::editor_syntax::factory(registry_name(language), grammar))
    }

    /// The syntax styles of `source`'s line `line` in `inputs`; none while the colours are off.
    pub(crate) fn line(
        self,
        inputs: &InputSyntax,
        source: Source,
        line: usize,
        theme: &HighlightTheme,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        if self.language().is_some() { inputs.line(source, line, theme) } else { Vec::new() }
    }

    /// Turn the colours off when on, on when off.
    pub(crate) const fn toggle(&mut self) {
        self.shown = match self.shown {
            Syntax::Highlighted => Syntax::Plain,
            Syntax::Plain => Syntax::Highlighted,
        };
    }
}

/// Register the highlight queries gpui-component leaves out (`docs/gpui-kit-issues.md`). Its
/// C# and Swift grammars come with empty queries, which colour nothing. C# has no
/// injections; Swift's inject only grammars triad does not build. Its TSX grammar comes with
/// only the lines `tree-sitter-typescript` adds to JavaScript's query, so TSX takes
/// TypeScript's whole query and JavaScript's injections, as TypeScript has them.
pub(crate) fn register_queries() {
    let registry = LanguageRegistry::singleton();
    let queries = [
        (Language::CSharp, tree_sitter_c_sharp::HIGHLIGHTS_QUERY),
        (Language::Swift, tree_sitter_swift::HIGHLIGHTS_QUERY),
    ];
    for (language, highlights) in queries {
        let name = registry_name(language);
        registry.register(
            name,
            &GrammarConfig::new(name, language.grammar(), Vec::new(), highlights, "", ""),
        );
    }
    let tsx = registry_name(Language::Tsx);
    let typescript = registry.language(registry_name(Language::TypeScript));
    if let (Some(config), Some(typescript)) = (registry.language(tsx), typescript) {
        let config = GrammarConfig {
            highlights: typescript.highlights,
            injections: typescript.injections,
            ..config
        };
        registry.register(tsx, &config);
    } else {
        tracing::error!("the TSX or TypeScript grammar is not built in; TSX shows plain");
    }
}

/// A language's grammar is missing.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GrammarError {
    #[error("no grammar for {0} is built in")]
    Missing(&'static str),
    #[error("the grammar for {name} does not load")]
    Load {
        name: &'static str,
        #[source]
        source: syntax::GrammarError,
    },
}

/// `language`'s grammar and highlight query, as the editors' highlighter has them, with the
/// grammars of the other languages its injections name. As in the editors, an injected
/// grammar's own injections do not run, and a language that is not built in shows plain.
pub(crate) fn grammar(language: Language) -> Result<Arc<Grammar>, GrammarError> {
    let name = registry_name(language);
    let registry = LanguageRegistry::singleton();
    let config = registry.language(name).ok_or(GrammarError::Missing(name))?;
    let host = language.grammar();
    // The editors' query runs the locals patterns before the highlights, so a node's
    // unstyled local capture wins over its highlight. Injected layers take highlights alone.
    let highlights = [config.locals.as_ref(), config.highlights.as_ref()].concat();
    let mut grammar = Grammar::new(host, name, &highlights, &config.injections)
        .map_err(|source| GrammarError::Load { name, source })?;
    let others: Vec<Box<str>> = grammar.injected_languages().map(Box::from).collect();
    for other in others {
        let Some(config) = registry.language(&other) else {
            continue;
        };
        let Some(language) = config.language else {
            continue;
        };
        let injected = Grammar::new(language, &other, &config.highlights, "")
            .and_then(|injected| grammar.inject(&other, Arc::new(injected)));
        if let Err(error) = injected {
            // The host's own captures do not depend on it.
            tracing::warn!(%error, language = &*other, host = name, "an injected grammar does not load; its injections show plain");
        }
    }
    Ok(Arc::new(grammar))
}

/// One input's text and its spans, parsed a chunk at a time as its rows show.
struct Input {
    source: Source,
    text: Text,
    chunks: ChunkedSpans,
}

/// The inputs' syntax: each input's spans, chunk by chunk, and the grammar they index.
#[derive(Default)]
pub(crate) struct InputSyntax {
    grammar: Option<Arc<Grammar>>,
    inputs: Vec<Input>,
}

/// The parse of one chunk of one input, to run off the UI thread.
pub(crate) struct ChunkParse {
    pub(crate) source: Source,
    pub(crate) chunk: ChunkId,
    grammar: Arc<Grammar>,
    text: Text,
    lines: Range<usize>,
}

impl ChunkParse {
    /// The chunk's spans. Slow, so it runs off the UI thread.
    pub(crate) fn run(&self) -> LineSpans {
        syntax::highlight_lines(&self.grammar, &self.text, self.lines.clone())
    }
}

impl InputSyntax {
    /// `inputs` under `grammar`, cut into chunks of about `chunk_lines` lines, none parsed.
    /// Texts share their content, so a clone is O(1).
    pub(crate) fn new<'a>(
        grammar: Arc<Grammar>,
        inputs: impl IntoIterator<Item = (Source, &'a Text)>,
        chunk_lines: usize,
        limits: Limits,
    ) -> Self {
        let inputs = inputs
            .into_iter()
            .map(|(source, text)| Input {
                source,
                text: text.clone(),
                chunks: ChunkedSpans::new(
                    syntax::chunk_starts(text, chunk_lines),
                    text.line_count(),
                    limits,
                ),
            })
            .collect();
        Self { grammar: Some(grammar), inputs }
    }

    fn input(&self, source: Source) -> Option<&Input> {
        self.inputs.iter().find(|input| input.source == source)
    }

    fn input_mut(&mut self, source: Source) -> Option<&mut Input> {
        self.inputs.iter_mut().find(|input| input.source == source)
    }

    /// `source`'s line `line` shows; its chunk queues unless it is parsed.
    pub(crate) fn want(&mut self, source: Source, line: usize) {
        if let Some(input) = self.input_mut(source) {
            input.chunks.want(line);
        }
    }

    /// A queued chunk to parse now, the inputs in order, while each has a parse free.
    pub(crate) fn next_parse(&mut self) -> Option<ChunkParse> {
        let grammar = self.grammar.as_ref()?;
        self.inputs.iter_mut().find_map(|input| {
            let (chunk, lines) = input.chunks.next_parse()?;
            Some(ChunkParse {
                source: input.source,
                chunk,
                grammar: Arc::clone(grammar),
                text: input.text.clone(),
                lines,
            })
        })
    }

    /// Chunk `chunk` of `source` has landed with its spans, or failed with none.
    pub(crate) fn landed(&mut self, source: Source, chunk: ChunkId, spans: Option<LineSpans>) {
        if let Some(input) = self.input_mut(source) {
            input.chunks.landed(chunk, spans);
        }
    }

    /// Chunks queued or parsing, over every input.
    pub(crate) fn pending(&self) -> usize {
        self.inputs.iter().map(|input| input.chunks.pending()).sum()
    }

    /// The syntax styles of `source`'s line `line`, without its terminator, in byte ranges
    /// of the line. Empty until its chunk lands; a capture the theme does not style has
    /// none.
    pub(crate) fn line(
        &self,
        source: Source,
        line: usize,
        theme: &HighlightTheme,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let (Some(grammar), Some(input)) = (&self.grammar, self.input(source)) else {
            return Vec::new();
        };
        let Some(spans) = input.chunks.line(line) else {
            return Vec::new();
        };
        spans
            .iter()
            .filter_map(|span| {
                let style = span
                    .captures()
                    .filter_map(|capture| theme.style.syntax.style(grammar.capture_name(capture)?))
                    .reduce(HighlightStyle::highlight)?;
                Some((span.range(), style))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::highlighter::SyntaxHighlighter;
    use gpui_kit::component::input::Rope;
    use merge_core::testing::text;

    use std::path::Path;

    use super::*;

    const RUST: &str = "fn a() {} // c\r\n// c\n\nlet\n";

    fn style(theme: &HighlightTheme, capture: &str) -> HighlightStyle {
        theme.style.syntax.style(capture).expect("the theme styles the capture")
    }

    /// Every chunk of every input queued, parsed and landed, as rows on every line ask.
    fn parse_all(syntax: &mut InputSyntax, inputs: &[(Source, &Text)]) {
        for &(source, text) in inputs {
            for line in 0..text.line_count() {
                syntax.want(source, line);
                while let Some(parse) = syntax.next_parse() {
                    syntax.landed(parse.source, parse.chunk, Some(parse.run()));
                }
            }
        }
    }

    /// Enough to hold every chunk of a test.
    const ALL: Limits = Limits { cached: usize::MAX, in_flight: 1 };

    fn chunked(language: Language, source: Source, text: &Text, chunk_lines: usize) -> InputSyntax {
        let grammar = grammar(language).expect("the grammar is built in");
        let mut syntax = InputSyntax::new(grammar, [(source, text)], chunk_lines, ALL);
        parse_all(&mut syntax, &[(source, text)]);
        syntax
    }

    fn parsed(source: Source, s: &str) -> (InputSyntax, Text) {
        let text = text(s);
        let syntax = chunked(Language::Rust, source, &text, syntax::CHUNK_LINES);
        (syntax, text)
    }

    #[test]
    fn a_line_takes_its_captures_styles_in_line_bytes() {
        let theme = HighlightTheme::default_light();
        let (syntax, _) = parsed(Source::Side(0), RUST);
        let first = syntax.line(Source::Side(0), 0, &theme);
        assert_eq!(first.first(), Some(&(0..2, style(&theme, "keyword"))));
        assert_eq!(first.last().map(|(range, _)| range.end), Some(14), "no CRLF");
        assert_eq!(syntax.line(Source::Side(0), 1, &theme), [(0..4, style(&theme, "comment"))]);
    }

    #[test]
    fn an_empty_line_has_no_styles() {
        let theme = HighlightTheme::default_light();
        let (syntax, _) = parsed(Source::Side(0), RUST);
        assert_eq!(syntax.line(Source::Side(0), 2, &theme), []);
    }

    #[test]
    fn an_input_without_a_parse_has_no_styles() {
        let theme = HighlightTheme::default_light();
        let (syntax, _) = parsed(Source::Side(0), RUST);
        assert_eq!(syntax.line(Source::Side(1), 0, &theme), []);
        assert_eq!(syntax.line(Source::Base(0), 0, &theme), []);
    }

    #[test]
    fn a_line_has_no_styles_until_its_chunk_lands_and_only_its_input_takes_them() {
        let theme = HighlightTheme::default_light();
        let (left, right) = (text(RUST), text("// c\n"));
        let grammar = grammar(Language::Rust).expect("Rust is built in");
        let inputs = [(Source::Side(0), &left), (Source::Side(1), &right)];
        let mut syntax = InputSyntax::new(grammar, inputs, syntax::CHUNK_LINES, ALL);
        assert_eq!(syntax.line(Source::Side(1), 0, &theme), []);
        syntax.want(Source::Side(1), 0);
        let parse = syntax.next_parse().expect("the right's chunk is queued");
        assert_eq!(parse.source, Source::Side(1));
        assert!(syntax.next_parse().is_none(), "the left is not on screen");
        assert_eq!(syntax.pending(), 1);
        syntax.landed(parse.source, parse.chunk, Some(parse.run()));
        assert_eq!(syntax.pending(), 0);
        assert_eq!(syntax.line(Source::Side(1), 0, &theme), [(0..4, style(&theme, "comment"))]);
        assert_eq!(
            syntax.line(Source::Side(0), 1, &theme),
            [],
            "the left's line 1 is a comment, unparsed"
        );
    }

    #[test]
    fn a_failed_chunk_shows_plain() {
        let theme = HighlightTheme::default_light();
        let left = text(RUST);
        let grammar = grammar(Language::Rust).expect("Rust is built in");
        let mut syntax =
            InputSyntax::new(grammar, [(Source::Side(0), &left)], syntax::CHUNK_LINES, ALL);
        syntax.want(Source::Side(0), 0);
        let parse = syntax.next_parse().expect("queued");
        syntax.landed(parse.source, parse.chunk, None);
        assert_eq!(syntax.line(Source::Side(0), 0, &theme), []);
        assert_eq!(syntax.pending(), 0);
        syntax.want(Source::Side(0), 0);
        assert!(syntax.next_parse().is_none(), "a failed chunk does not parse again");
    }

    /// A line's style at each byte: what a row draws, however it splits the runs.
    fn per_byte(length: usize, styles: &[(Range<usize>, HighlightStyle)]) -> Vec<HighlightStyle> {
        let mut bytes = vec![HighlightStyle::default(); length];
        for (range, style) in styles {
            bytes[range.clone()].fill(*style);
        }
        bytes
    }

    const LANGUAGES: [Language; 12] = [
        Language::Rust,
        Language::Go,
        Language::CSharp,
        Language::Python,
        Language::Swift,
        Language::Html,
        Language::JavaScript,
        Language::TypeScript,
        Language::Tsx,
        Language::Json,
        Language::Yaml,
        Language::Toml,
    ];
    /// Chunks that cut the Go and C# samples only before top-level lines: no shorter than
    /// their longest run of lines without one (34 in Go, 56 in C#), shorter than the sample.
    /// The demo's `mod tests` has none in its last 615 lines, so it parses whole.
    const SAMPLE_CHUNK: usize = 64;
    /// The same for the Python, Swift, JavaScript, TypeScript and TSX samples, whose longest
    /// run is 28 lines. An HTML document is one element, and a JSON document one value,
    /// with no top-level line inside it, so each parses whole.
    const SHORT_SAMPLE_CHUNK: usize = 32;

    #[test]
    fn registry_names_are_gpui_components() {
        assert_eq!(registry_name(Language::Rust), "rust");
        assert_eq!(registry_name(Language::Go), "go");
        assert_eq!(registry_name(Language::CSharp), "csharp");
        assert_eq!(registry_name(Language::Python), "python");
        assert_eq!(registry_name(Language::Swift), "swift");
        assert_eq!(registry_name(Language::Html), "html");
        assert_eq!(registry_name(Language::JavaScript), "javascript");
        assert_eq!(registry_name(Language::TypeScript), "typescript");
        assert_eq!(registry_name(Language::Tsx), "tsx");
        assert_eq!(registry_name(Language::Json), "json");
        assert_eq!(registry_name(Language::Yaml), "yaml");
        assert_eq!(registry_name(Language::Toml), "toml");
    }

    #[test]
    fn the_registry_holds_the_grammars_merge_view_gives() {
        register_queries();
        let registry = LanguageRegistry::singleton();
        for language in LANGUAGES {
            let config = registry.language(registry_name(language));
            let held = config.and_then(|config| config.language);
            assert_eq!(held, Some(language.grammar()), "{language:?}");
        }
    }

    #[test]
    fn every_language_loads_its_grammar() {
        for language in LANGUAGES {
            let loaded = grammar(language);
            assert!(loaded.is_ok(), "{language:?}: {:?}", loaded.err());
        }
    }

    /// The first line, if any, where `language`'s spans of `source`, parsed in chunks of
    /// `chunk_lines`, draw other styles than the editor's highlighter does.
    fn first_difference(language: Language, source: &str, chunk_lines: usize) -> Option<String> {
        let theme = HighlightTheme::default_light();
        let text = text(source);
        let syntax = chunked(language, Source::Base(0), &text, chunk_lines);
        let mut editor = SyntaxHighlighter::new(registry_name(language));
        editor.update(None, &Rope::from_str(source), None);
        let mut start = 0;
        for (n, line) in text.lines().enumerate() {
            let length = line.trim_end_matches(['\n', '\r']).len();
            let theirs: Vec<_> = editor
                .styles(&(start..start + length), &*theme)
                .into_iter()
                .map(|(range, style)| (range.start - start..range.end - start, style))
                .collect();
            let ours = syntax.line(Source::Base(0), n, &theme);
            if per_byte(length, &ours) != per_byte(length, &theirs) {
                return Some(format!("line {n}: {line:?}: ours {ours:?}, the editor's {theirs:?}"));
            }
            start += line.len();
        }
        None
    }

    #[test]
    fn the_spans_draw_what_the_editors_highlighter_draws() {
        register_queries();
        let theme = HighlightTheme::default_light();
        let samples = [
            (
                Language::Rust,
                include_str!("../../../fixtures/demo/base.txt"),
                &[syntax::CHUNK_LINES][..],
            ),
            (
                Language::Go,
                include_str!("../../../fixtures/syntax/sample.go"),
                &[syntax::CHUNK_LINES, SAMPLE_CHUNK][..],
            ),
            (
                Language::CSharp,
                include_str!("../../../fixtures/syntax/sample.cs"),
                &[syntax::CHUNK_LINES, SAMPLE_CHUNK][..],
            ),
            (
                Language::Python,
                include_str!("../../../fixtures/syntax/sample.py"),
                &[syntax::CHUNK_LINES, SHORT_SAMPLE_CHUNK][..],
            ),
            (
                Language::Swift,
                include_str!("../../../fixtures/syntax/sample.swift"),
                &[syntax::CHUNK_LINES, SHORT_SAMPLE_CHUNK][..],
            ),
            (
                Language::Html,
                include_str!("../../../fixtures/syntax/sample.html"),
                &[syntax::CHUNK_LINES][..],
            ),
            (
                Language::JavaScript,
                include_str!("../../../fixtures/syntax/sample.js"),
                &[syntax::CHUNK_LINES, SHORT_SAMPLE_CHUNK][..],
            ),
            (
                Language::TypeScript,
                include_str!("../../../fixtures/syntax/sample.ts"),
                &[syntax::CHUNK_LINES, SHORT_SAMPLE_CHUNK][..],
            ),
            (
                Language::Tsx,
                include_str!("../../../fixtures/syntax/sample.tsx"),
                &[syntax::CHUNK_LINES, SHORT_SAMPLE_CHUNK][..],
            ),
            (
                Language::Json,
                include_str!("../../../fixtures/syntax/sample.json"),
                &[syntax::CHUNK_LINES][..],
            ),
            (
                Language::Yaml,
                include_str!("../../../fixtures/syntax/sample.yaml"),
                &[syntax::CHUNK_LINES, SHORT_SAMPLE_CHUNK][..],
            ),
            (
                Language::Toml,
                include_str!("../../../fixtures/syntax/sample.toml"),
                &[syntax::CHUNK_LINES, SHORT_SAMPLE_CHUNK][..],
            ),
        ];
        for (language, source, chunks) in samples {
            // Each sample opens with a line comment, so a grammar that styles nothing fails.
            let first = source.lines().next().unwrap_or_default();
            let text = text(source);
            let syntax = chunked(language, Source::Base(0), &text, syntax::CHUNK_LINES);
            assert_eq!(
                syntax.line(Source::Base(0), 0, &theme),
                [(0..first.len(), style(&theme, "comment"))],
                "{language:?}"
            );
            for &chunk_lines in chunks {
                assert_eq!(
                    first_difference(language, source, chunk_lines),
                    None,
                    "{language:?}, chunks of {chunk_lines}"
                );
            }
        }
    }

    #[test]
    fn a_toggle_turns_the_language_off_and_on() {
        let mut colouring = Colouring::new(Some(Language::Go), Syntax::Highlighted, 2);
        assert_eq!(colouring.language(), Some(Language::Go));
        colouring.toggle();
        assert_eq!(colouring.language(), None);
        assert_eq!(colouring.editor_language(), "plain");
        colouring.toggle();
        assert_eq!(colouring.language(), Some(Language::Go));
        assert_eq!(colouring.editor_language(), "go");
    }

    #[test]
    fn past_the_line_limit_the_editors_colour_a_chunk_at_a_time() {
        let colouring = Colouring::new(Some(Language::Rust), Syntax::Highlighted, 2);
        assert_eq!(colouring.editor_language(), "rust");
        assert!(!colouring.chunked(2));
        assert!(colouring.chunked(3));
        assert!(colouring.chunked_factory(2).is_none());
        let factory = colouring.chunked_factory(3).expect("Rust is built in");
        let highlighter = factory("rust").expect("a Rust editor takes it");
        assert_eq!(highlighter.language().as_ref(), "rust");
        assert!(factory("plain").is_none(), "an editor turned plain takes none");
        // Off, the factory still serves the language for when the colours turn on.
        let off = Colouring::new(Some(Language::Rust), Syntax::Plain, 2);
        assert!(off.chunked_factory(3).is_some());
        let none = Colouring::new(None, Syntax::Highlighted, 2);
        assert!(none.chunked_factory(3).is_none());
    }

    #[test]
    fn without_a_language_a_toggle_colours_nothing() {
        let mut colouring = Colouring::new(None, Syntax::Plain, usize::MAX);
        colouring.toggle();
        assert_eq!(colouring.language(), None);
        assert_eq!(colouring.editor_language(), "plain");
    }

    #[test]
    fn syntax_off_detects_nothing() {
        let rust = Language::detect([Path::new("a.rs")]);
        assert_eq!(Colouring::new(rust, Syntax::Plain, usize::MAX).language(), None);
        assert_eq!(
            Colouring::new(rust, Syntax::Highlighted, usize::MAX).language(),
            Some(Language::Rust)
        );
    }

    #[test]
    fn an_editor_without_a_language_is_plain() {
        assert_eq!(editor_language(None), "plain");
        assert_eq!(editor_language(Some(Language::Go)), "go");
    }
}
