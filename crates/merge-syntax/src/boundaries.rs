//! [`Boundaries`] from a tree-sitter grammar (`docs/PLAN.md` §B5.8).
//!
//! A region parses with the lines around it, from the top-level line at or before its first
//! line to the next top-level line after it ([`crate::syntax`]'s rule), never the whole file.
//! A parse window longer than [`MAX_WINDOW_LINES`], or an error or missing node across the
//! region's text, leaves every cut clean.

use std::cell::RefCell;
use std::ops::Range;

use merge_core::boundaries::{Boundaries, Cuts};
use merge_core::text::Text;
use tree_sitter::{Node, Parser, Tree};

use crate::language::Language;
use crate::syntax::{CHUNK_LINES, top_level};

/// The most lines a region's parse holds.
pub const MAX_WINDOW_LINES: usize = 10 * CHUNK_LINES;

/// A language's grammar, as [`Boundaries`].
#[derive(Debug, Clone)]
pub struct SyntaxBoundaries {
    grammar: tree_sitter::Language,
}

/// What a cut may not fall inside.
#[derive(Debug, Clone, Copy)]
enum Level {
    /// A node within the region.
    Nodes,
    /// A token.
    Tokens,
}

/// The region's parse has an error or a missing node.
struct Unparsed;

impl SyntaxBoundaries {
    #[must_use]
    pub fn new(language: Language) -> Self {
        Self { grammar: language.grammar() }
    }

    fn cuts(&self, text: &Text, lines: Range<usize>, level: Level) -> Cuts {
        let Some(window) = window(text, lines.clone()) else {
            return Cuts::all_clean();
        };
        let start = text.span(0..lines.start).len();
        let region = text.span(lines);
        let lead = region.len() - region.trim_start().len();
        let core = start + lead..start + region.trim_end().len();
        if core.is_empty() {
            return Cuts::all_clean();
        }
        let offset = text.span(0..window.start).len();
        let Some(tree) = self.parse(text.span(window)) else {
            return Cuts::all_clean();
        };
        let core = core.start - offset..core.end - offset;
        let mut nodes = Vec::new();
        let root = tree.root_node();
        if root.is_error() || collect(root, &core, level, &mut nodes).is_err() {
            return Cuts::all_clean();
        }
        for node in &mut nodes {
            *node = node.start + offset..node.end + offset;
        }
        Cuts::inside(nodes)
    }
}

impl SyntaxBoundaries {
    /// `source`'s tree, by this thread's parser; none when the grammar does not load.
    fn parse(&self, source: &str) -> Option<Tree> {
        PARSER.with_borrow_mut(|(grammar, parser)| {
            if grammar.as_ref() != Some(&self.grammar) {
                if let Err(error) = parser.set_language(&self.grammar) {
                    // A grammar the linked tree-sitter cannot load colours nothing either; the
                    // merge goes on as without the setting.
                    tracing::warn!(%error, "a grammar does not load; syntax boundaries do nothing");
                    return None;
                }
                *grammar = Some(self.grammar.clone());
            }
            parser.parse(source, None)
        })
    }
}

thread_local! {
    /// Each thread's parser, and the grammar it holds. A conflict's parse is short, so a
    /// parser built for each costs as much again.
    static PARSER: RefCell<(Option<tree_sitter::Language>, Parser)> =
        RefCell::new((None, Parser::new()));
}

impl Boundaries for SyntaxBoundaries {
    fn node_cuts(&self, text: &Text, lines: Range<usize>) -> Cuts {
        self.cuts(text, lines, Level::Nodes)
    }

    fn token_cuts(&self, text: &Text, lines: Range<usize>) -> Cuts {
        self.cuts(text, lines, Level::Tokens)
    }
}

/// The lines a region of `lines` parses with: from the top-level line at or before its first
/// line, else the text's start, to the first top-level line after it, else the text's end.
/// None when that is longer than [`MAX_WINDOW_LINES`].
fn window(text: &Text, lines: Range<usize>) -> Option<Range<usize>> {
    let count = text.line_count();
    let start =
        (1..=lines.start).rev().take(MAX_WINDOW_LINES).find(|&n| top_level(text, n)).unwrap_or(0);
    let end = (lines.end.max(1)..count)
        .take(MAX_WINDOW_LINES)
        .find(|&n| top_level(text, n))
        .unwrap_or(count);
    (end - start <= MAX_WINDOW_LINES).then_some(start..end)
}

/// Push the ranges of `node`'s descendants that a cut may not fall inside, at `level`, for
/// the text `core`.
fn collect(
    node: Node<'_>,
    core: &Range<usize>,
    level: Level,
    nodes: &mut Vec<Range<usize>>,
) -> Result<(), Unparsed> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let range = child.byte_range();
        if child.is_error() || child.is_missing() {
            if range.start < core.end && range.end > core.start || range.is_empty() {
                return Err(Unparsed);
            }
            continue;
        }
        if range.end <= core.start || range.start >= core.end {
            continue;
        }
        match level {
            Level::Nodes if range.start <= core.start && range.end >= core.end => {
                collect(child, core, level, nodes)?;
            }
            Level::Nodes => {
                if child.has_error() {
                    return Err(Unparsed);
                }
                nodes.push(range);
            }
            Level::Tokens if child.child_count() == 0 => nodes.push(range),
            Level::Tokens => collect(child, core, level, nodes)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use merge_core::testing::text;

    /// The offsets of `at` whose cut is clean.
    fn clean(cuts: &Cuts, at: &[usize]) -> Vec<usize> {
        at.iter().copied().filter(|&at| cuts.clean(at)).collect()
    }

    fn nodes(language: Language, source: &str, lines: Range<usize>) -> Cuts {
        SyntaxBoundaries::new(language).node_cuts(&text(source), lines)
    }

    fn tokens(language: Language, source: &str, lines: Range<usize>) -> Cuts {
        SyntaxBoundaries::new(language).token_cuts(&text(source), lines)
    }

    #[test]
    fn in_rust_a_cut_inside_an_if_within_the_region_is_not_clean() {
        // Lines start at 0, 9, 20, 33 and 39. The region is lines 1..4, the whole `if`:
        // bytes 13..38 without the whitespace, which the `if` holds. Its block, 18..38,
        // crosses 20 and 33.
        let source = "fn a() {\n    if x {\n        y();\n    }\n}\n";
        let cuts = nodes(Language::Rust, source, 1..4);
        assert_eq!(clean(&cuts, &[9, 20, 33, 39]), [9, 39]);
    }

    #[test]
    fn in_rust_a_cut_between_two_items_is_clean() {
        // Lines start at 0 and 10; each function ends before its line's `\n`.
        let cuts = nodes(Language::Rust, "fn a() {}\nfn b() {}\n", 0..2);
        assert_eq!(clean(&cuts, &[0, 5, 10, 15, 20]), [0, 10, 20]);
    }

    #[test]
    fn in_python_a_cut_after_a_block_is_clean() {
        // Lines start at 0, 9, 19 and 31; the region is lines 1..4, bytes 13..38 without
        // the whitespace. The function's body holds it; the `if` (13..30) crosses 19, and
        // `d()` starts at 35.
        let source = "def a():\n    if b:\n        c()\n    d()\n";
        let cuts = nodes(Language::Python, source, 1..4);
        assert_eq!(clean(&cuts, &[9, 19, 31, 39]), [9, 31, 39]);
    }

    #[test]
    fn in_go_a_cut_inside_an_if_is_not_clean_and_one_after_it_is() {
        // Lines start at 0, 10, 11, 22, 30, 36, 39 and 44. The region is lines 3..7: the
        // `if` (23..38), crossing 30 and 36, then `e()` (40..43).
        let source = "package a\n\nfunc b() {\n\tif c {\n\t\td()\n\t}\n\te()\n}\n";
        let cuts = nodes(Language::Go, source, 3..7);
        assert_eq!(clean(&cuts, &[22, 30, 36, 39, 44]), [22, 39, 44]);
    }

    #[test]
    fn a_region_with_a_parse_error_has_only_clean_cuts() {
        let source = "fn a( {\n    x();\n}\n";
        let cuts = nodes(Language::Rust, source, 0..3);
        assert_eq!(cuts, Cuts::all_clean());
    }

    #[test]
    fn a_cut_inside_a_string_is_not_clean_between_tokens_it_is() {
        // `let s = "a b";`: the string's quotes at 8 and 12, its content 9..12.
        let cuts = tokens(Language::Rust, "let s = \"a b\";\n", 0..1);
        assert_eq!(clean(&cuts, &[3, 4, 8, 10, 11, 13, 14]), [3, 4, 8, 13, 14]);
    }

    #[test]
    fn a_cut_inside_an_arrow_is_not_clean() {
        // `fn a() -> u8 {}`: `->` at 7..9.
        let cuts = tokens(Language::Rust, "fn a() -> u8 {}\n", 0..1);
        assert_eq!(clean(&cuts, &[6, 7, 8, 9, 10]), [6, 7, 9, 10]);
    }

    /// 3 000 functions of four lines, 21 bytes each with the blank line after it.
    fn many_functions() -> String {
        const FUNCTIONS: usize = 3000;
        "fn f() {\n    x();\n}\n\n".repeat(FUNCTIONS)
    }

    #[test]
    fn a_region_deep_in_a_long_file_parses_with_the_lines_around_it() {
        const FUNCTION: usize = 2500;
        const BYTES: usize = 21;
        const LINES: usize = 4;
        let at = FUNCTION * BYTES;
        // The function's block, 7..19 within it, crosses its lines' starts at 9 and 18.
        let cuts = nodes(Language::Rust, &many_functions(), FUNCTION * LINES..FUNCTION * LINES + 3);
        assert_eq!(clean(&cuts, &[at, at + 9, at + 18, at + 20]), [at, at + 20]);
    }

    #[test]
    fn past_the_longest_window_every_cut_is_clean() {
        // HTML lines with no blank line between: none is top-level, so the window runs from
        // the text's start. The element's text `ab` is one token, at 3..5 of its line.
        let line = "<p>ab</p>\n";
        let near = tokens(Language::Html, &line.repeat(10), 5..6);
        assert_eq!(clean(&near, &[50 + 3, 50 + 4, 50 + 5]), [53, 55]);
        let start = MAX_WINDOW_LINES + 1;
        let far = tokens(Language::Html, &line.repeat(start + 10), start..start + 1);
        assert_eq!(far, Cuts::all_clean());
    }

    #[test]
    fn an_error_inside_a_statement_of_the_region_leaves_every_cut_clean() {
        // `xx(;` parses as a statement holding an error node, inside the function's block.
        // Without the error, `xx` and `yy` would be tokens with an inside.
        let source = "fn a() {\n    xx(;\n    yy();\n}\n";
        assert_eq!(nodes(Language::Rust, source, 1..3), Cuts::all_clean());
        assert_eq!(tokens(Language::Rust, source, 1..3), Cuts::all_clean());
    }

    #[test]
    fn a_window_of_20_000_lines_parses_and_one_more_line_leaves_every_cut_clean() {
        // Spec §4.3: a region parses with at most 20 000 lines around it. HTML lines with no
        // blank line between are never top-level, so the window is the whole text. The
        // element's text `ab` is one token, at 3..5 of line 5, which starts at 50.
        const SPEC_WINDOW_LINES: usize = 20_000;
        let line = "<p>ab</p>\n";
        let fits = tokens(Language::Html, &line.repeat(SPEC_WINDOW_LINES), 5..6);
        assert_eq!(clean(&fits, &[53, 54, 55]), [53, 55]);
        let over = tokens(Language::Html, &line.repeat(SPEC_WINDOW_LINES + 1), 5..6);
        assert_eq!(over, Cuts::all_clean());
    }

    #[test]
    fn an_error_outside_the_region_leaves_its_cuts_as_without_it() {
        // Lines start at 0, 9, 15, 26, 39, 45 and 47. The stray `)` on line 1 is an error in
        // the function's block, before the region. The region is lines 2..5, the `if`:
        // bytes 19..44 without the whitespace. Its block, 24..44, crosses 26 and 39.
        let source = "fn a() {\n    )\n    if x {\n        y();\n    }\n}\n";
        let cuts = nodes(Language::Rust, source, 2..5);
        assert_eq!(clean(&cuts, &[15, 26, 39, 45]), [15, 45]);
    }

    #[test]
    fn a_node_outside_the_region_does_not_bind_its_cuts() {
        // Lines start at 0, 9, 20, 31, 44, 50 and 52. The region is line 1, `foo();` at
        // 13..19; 14 lies inside the token `foo`. The `if` after the region, 24..49, crosses
        // 31 and 44 and holds the token `if` at 24..26; cuts inside it stay clean.
        let source = "fn a() {\n    foo();\n    if y {\n        z();\n    }\n}\n";
        let at = [9, 14, 20, 25, 31, 44, 50];
        let outside = [9, 20, 25, 31, 44, 50];
        assert_eq!(clean(&nodes(Language::Rust, source, 1..2), &at), outside);
        assert_eq!(clean(&tokens(Language::Rust, source, 1..2), &at), outside);
    }

    #[test]
    fn an_empty_or_blank_region_has_only_clean_cuts() {
        let source = "fn a() {}\n\n\nfn b() {}\n";
        assert_eq!(nodes(Language::Rust, source, 1..3), Cuts::all_clean());
        assert_eq!(nodes(Language::Rust, source, 2..2), Cuts::all_clean());
    }
}
