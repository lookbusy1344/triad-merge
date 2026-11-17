//! A line as it draws outside an editor, such as in the merge window's side panes: each tab
//! becomes the spaces up to the next stop, counted in characters (spec §6.4), and each
//! invisible character becomes its code point, `U+202E`, which draws as a badge (spec §5).
//! With whitespace shown, spaces, tabs, carriage returns and a missing final newline draw as
//! symbols (spec §5).

use std::borrow::Cow;
use std::ops::Range;

use merge_core::unusual::{self, CodePoint, Unusual};
use smallvec::SmallVec;

/// Replacements a line usually holds before the list spills to the heap.
const INLINE_REPLACEMENTS: usize = 4;

/// A space, with whitespace shown.
pub const SPACE_SYMBOL: char = '·';
/// The first column of a tab, with whitespace shown.
pub const TAB_SYMBOL: char = '→';
/// A carriage return in a line ending, with whitespace shown.
pub const CARRIAGE_RETURN_SYMBOL: char = '␍';
/// The end of a last line without a newline, with whitespace shown.
pub const NO_NEWLINE_SYMBOL: char = '∅';

/// How a line draws: its tab stops, and whether its whitespace shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawing {
    /// Columns between tab stops; 0 is taken as 1.
    pub tab_width: usize,
    pub whitespace: bool,
}

/// A mark on drawn text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// An invisible character's code point, or a space other than U+0020.
    Unusual(Unusual),
    /// A whitespace symbol.
    Whitespace,
}

/// One character the drawn text replaces: a tab, an invisible character or a whitespace
/// symbol. The mark of a missing final newline replaces nothing at the line's end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Replaced {
    /// The character's byte offset in the line.
    at: usize,
    /// The character's length in bytes.
    len: usize,
    /// The bytes added by this replacement and every one before it.
    added: usize,
}

/// A line as it draws, and the map from its byte offsets to the drawn text's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded<'a> {
    text: Cow<'a, str>,
    replaced: SmallVec<[Replaced; INLINE_REPLACEMENTS]>,
    /// The marks, in bytes of the drawn text.
    marks: SmallVec<[(Range<usize>, Mark); INLINE_REPLACEMENTS]>,
    /// The length of the line without its ending.
    body: usize,
}

/// `line` split into its text and its ending: the `\n` and `\r` at its end.
fn split_ending(line: &str) -> (&str, &str) {
    let body = line.trim_end_matches(['\n', '\r']);
    (body, &line[body.len()..])
}

/// The symbols `ending` draws as with whitespace shown: one per carriage return, then the
/// mark of a missing newline. An empty line, which no text holds, draws none.
fn ending_symbols(line: &str, ending: &str) -> impl Iterator<Item = (usize, char)> {
    let returns = ending.char_indices().filter(|&(_, c)| c == '\r');
    let missing =
        (!line.is_empty() && !ending.ends_with('\n')).then_some((ending.len(), NO_NEWLINE_SYMBOL));
    returns.map(|(at, _)| (at, CARRIAGE_RETURN_SYMBOL)).chain(missing)
}

/// Expand `line`, as its text holds it with its ending, for `drawing`: tabs run to stops
/// every `tab_width` characters, each invisible character is written as its code point,
/// which takes a column per character, and the ending is left out. With whitespace shown,
/// each space draws as [`SPACE_SYMBOL`], each tab starts with [`TAB_SYMBOL`], and the ending
/// draws as its symbols.
#[must_use]
pub fn expand(line: &str, drawing: Drawing) -> Expanded<'_> {
    let (body, ending) = split_ending(line);
    let shown = drawing.whitespace;
    let plain = |c: char| c != '\t' && !(shown && c == ' ') && unusual::classify(c).is_none();
    let mut endings = ending_symbols(line, ending).filter(|_| shown).peekable();
    if body.chars().all(plain) && endings.peek().is_none() {
        return Expanded {
            text: Cow::Borrowed(body),
            replaced: SmallVec::new(),
            marks: SmallVec::new(),
            body: body.len(),
        };
    }
    let width = drawing.tab_width.max(1);
    let mut text = String::with_capacity(line.len() + width * body.matches('\t').count());
    let mut replaced = SmallVec::new();
    let mut marks = SmallVec::new();
    let mut column = 0;
    for (at, c) in body.char_indices() {
        let start = text.len();
        match (c, unusual::classify(c)) {
            ('\t', _) => {
                let spaces = width - column % width;
                let blanks = if shown {
                    text.push(TAB_SYMBOL);
                    marks.push((start..text.len(), Mark::Whitespace));
                    spaces - 1
                } else {
                    spaces
                };
                text.extend(std::iter::repeat_n(' ', blanks));
                column += spaces;
            }
            (' ', _) if shown => {
                text.push(SPACE_SYMBOL);
                marks.push((start..text.len(), Mark::Whitespace));
                column += 1;
            }
            (_, Some(Unusual::Invisible)) => {
                let label = CodePoint(c);
                // Writing to a `String` cannot fail.
                let _ = std::fmt::Write::write_fmt(&mut text, format_args!("{label}"));
                column += label.width();
                marks.push((start..text.len(), Mark::Unusual(Unusual::Invisible)));
            }
            (_, kind) => {
                text.push(c);
                column += 1;
                if let Some(kind) = kind {
                    marks.push((start..text.len(), Mark::Unusual(kind)));
                }
                continue;
            }
        }
        replaced.push(Replaced { at, len: c.len_utf8(), added: text.len() - at - c.len_utf8() });
    }
    for (offset, symbol) in endings {
        let start = text.len();
        text.push(symbol);
        marks.push((start..text.len(), Mark::Whitespace));
        let at = body.len() + offset;
        let len = usize::from(symbol == CARRIAGE_RETURN_SYMBOL);
        replaced.push(Replaced { at, len, added: text.len() - at - len });
    }
    Expanded { text: Cow::Owned(text), replaced, marks, body: body.len() }
}

/// The columns `line` fills as [`expand`] draws it for `drawing`.
#[must_use]
pub fn columns(line: &str, drawing: Drawing) -> usize {
    let width = drawing.tab_width.max(1);
    let (body, ending) = split_ending(line);
    let text = body.chars().fold(0, |column, c| match (c, unusual::classify(c)) {
        ('\t', _) => column + width - column % width,
        (_, Some(Unusual::Invisible)) => column + CodePoint(c).width(),
        _ => column + 1,
    });
    let symbols = if drawing.whitespace { ending_symbols(line, ending).count() } else { 0 };
    text + symbols
}

/// The most [`columns`] of any of `lines`, each with its ending; 0 without lines.
#[must_use]
pub fn widest<'a>(lines: impl Iterator<Item = &'a str>, drawing: Drawing) -> usize {
    lines.map(|line| columns(line, drawing)).max().unwrap_or(0)
}

impl<'a> Expanded<'a> {
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn into_text(self) -> Cow<'a, str> {
        self.text
    }

    /// The marks, in bytes of the drawn text, in order.
    #[must_use]
    pub fn marks(&self) -> &[(Range<usize>, Mark)] {
        &self.marks
    }

    /// Where byte `offset` of the line lands in the drawn text. A replaced character's own
    /// offset is the start of its replacement; the end of the text is the start of the
    /// ending's symbols.
    #[must_use]
    pub fn offset(&self, offset: usize) -> usize {
        match self.replaced.partition_point(|r| r.at < offset) {
            0 => offset,
            n => offset + self.replaced[n - 1].added,
        }
    }

    /// The byte of the line that byte `at` of the drawn text shows: [`Self::offset`] reversed.
    /// A byte inside a replacement goes to the nearer side of its character, the start on a
    /// tie. A byte in the ending's symbols or past the end is the end of the text.
    #[must_use]
    pub fn line_offset(&self, at: usize) -> usize {
        let at = at.min(self.text.len());
        let mut added = 0;
        for r in &self.replaced {
            let start = r.at + added;
            if at <= start {
                break;
            }
            let end = r.at + r.len + r.added;
            if at < end {
                let side = if at - start <= end - at { r.at } else { r.at + r.len };
                return side.min(self.body);
            }
            added = r.added;
        }
        (at - added).min(self.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Drawing = Drawing { tab_width: 4, whitespace: false };
    const SHOWN: Drawing = Drawing { tab_width: 4, whitespace: true };

    #[test]
    fn a_tab_after_text_runs_to_the_next_stop() {
        let line = expand("a\tb", PLAIN);
        assert_eq!(line.text(), "a   b");
        assert_eq!((0..=3).map(|b| line.offset(b)).collect::<Vec<_>>(), [0, 1, 4, 5]);
    }

    #[test]
    fn a_tab_at_a_stop_is_a_whole_stop() {
        assert_eq!(expand("abcd\tx", PLAIN).text(), "abcd    x");
        assert_eq!(expand("\tx", PLAIN).text(), "    x");
    }

    #[test]
    fn tabs_in_a_row_each_take_a_stop() {
        let line = expand("\t\tx", PLAIN);
        assert_eq!(line.text(), "        x");
        assert_eq!(line.offset(1), 4);
        assert_eq!(line.offset(2), 8);
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let line = expand("é\tx", PLAIN);
        assert_eq!(line.text(), "é   x");
        assert_eq!(line.offset(3), 5);
    }

    #[test]
    fn the_width_sets_the_stops() {
        assert_eq!(expand("a\tb", Drawing { tab_width: 8, whitespace: false }).text(), "a       b");
        assert_eq!(expand("a\tb", Drawing { tab_width: 1, whitespace: false }).text(), "a b");
        assert_eq!(expand("a\tb", Drawing { tab_width: 0, whitespace: false }).text(), "a b");
    }

    #[test]
    fn a_line_without_tabs_is_borrowed() {
        let line = expand("ab", PLAIN);
        assert!(matches!(line.text, Cow::Borrowed("ab")));
        assert_eq!(line.offset(2), 2);
        assert!(matches!(line.into_text(), Cow::Borrowed("ab")));
    }

    #[test]
    fn columns_expand_tabs_and_count_characters() {
        assert_eq!(columns("", PLAIN), 0);
        assert_eq!(columns("abc", PLAIN), 3);
        // a, the tab to column 4, then é and x.
        assert_eq!(columns("a\téx", PLAIN), 6);
        assert_eq!(columns("\t\t", PLAIN), 8);
        assert_eq!(columns("abcd\t", PLAIN), 8);
        assert_eq!(columns("a\t", Drawing { tab_width: 0, whitespace: false }), 2);
    }

    #[test]
    fn columns_leave_out_the_line_ending() {
        assert_eq!(columns("ab\n", PLAIN), 2);
        assert_eq!(columns("ab\r\n", PLAIN), 2);
        assert_eq!(columns("\r\n", PLAIN), 0);
    }

    #[test]
    fn widest_takes_the_most_columns_of_any_line() {
        assert_eq!(widest(std::iter::empty(), PLAIN), 0);
        assert_eq!(widest(["abcdef\n", "\tab\n", "x"].into_iter(), PLAIN), 6);
        assert_eq!(widest(["abcdef\n", "\t\tab\n"].into_iter(), PLAIN), 10);
    }

    #[test]
    fn a_byte_of_the_expanded_text_maps_back_to_its_line_byte() {
        // a at 0, the tab's spaces 1..4, b at 4.
        let line = expand("a\tb", PLAIN);
        assert_eq!((0..=6).map(|b| line.line_offset(b)).collect::<Vec<_>>(), [0, 1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn a_tie_inside_a_tab_goes_to_its_start() {
        // Two tabs of four spaces each: 0..4 and 4..8; 2 and 6 sit midway.
        let line = expand("\t\tx", PLAIN);
        assert_eq!(
            (0..=9).map(|b| line.line_offset(b)).collect::<Vec<_>>(),
            [0, 0, 0, 1, 1, 1, 1, 2, 2, 3]
        );
    }

    #[test]
    fn line_offset_reverses_offset() {
        let line = "é\tx\ty";
        let expanded = expand(line, PLAIN);
        let boundaries = (0..=line.len()).filter(|&b| line.is_char_boundary(b));
        assert!(boundaries.into_iter().all(|b| expanded.line_offset(expanded.offset(b)) == b));
    }

    #[test]
    fn past_the_end_is_the_lines_end() {
        assert_eq!(expand("ab", PLAIN).line_offset(1), 1);
        assert_eq!(expand("ab", PLAIN).line_offset(9), 2);
        assert_eq!(expand("a\t", PLAIN).line_offset(9), 2);
    }

    #[test]
    fn a_second_tab_counts_the_columns_the_first_one_filled() {
        // a at column 0, the first tab fills 1-3, b at 4, the second tab fills 5-7.
        let line = expand("a\tb\tc", PLAIN);
        assert_eq!(line.text(), "a   b   c");
        assert_eq!(line.offset(4), 8);
        assert_eq!(line.into_text(), "a   b   c");
    }

    const RLO: char = '\u{202E}';

    #[test]
    fn an_invisible_character_draws_as_its_code_point() {
        let line = format!("a{RLO}b");
        let drawn = expand(&line, PLAIN);
        assert_eq!(drawn.text(), "aU+202Eb");
        assert_eq!(drawn.marks(), [(1..7, Mark::Unusual(Unusual::Invisible))]);
        // a at 0, RLO's three bytes at 1, b at 4: b draws at 1 + 6.
        assert_eq!([0, 1, 4, 5].map(|b| drawn.offset(b)), [0, 1, 7, 8]);
    }

    #[test]
    fn a_byte_inside_a_code_point_goes_to_the_nearer_side_of_its_character() {
        // The label U+202E fills 1..7; its start is line byte 1, its end line byte 4.
        let line = format!("a{RLO}b");
        let drawn = expand(&line, PLAIN);
        assert_eq!(
            (0..=8).map(|b| drawn.line_offset(b)).collect::<Vec<_>>(),
            [0, 1, 1, 1, 1, 4, 4, 4, 5]
        );
    }

    #[test]
    fn a_code_point_fills_a_column_per_character_before_a_tab() {
        // U+200B fills columns 0-5, so the tab runs from 6 to the stop at 8.
        let drawn = expand("\u{200B}\tx", PLAIN);
        assert_eq!(drawn.text(), "U+200B  x");
        assert_eq!(drawn.marks(), [(0..6, Mark::Unusual(Unusual::Invisible))]);
        assert_eq!(columns("\u{200B}\tx", PLAIN), 9);
    }

    #[test]
    fn a_character_past_the_basic_plane_takes_its_whole_code_point() {
        let drawn = expand("\u{E0041}z", PLAIN);
        assert_eq!(drawn.text(), "U+E0041z");
        assert_eq!(drawn.offset(4), 7);
        assert_eq!(drawn.line_offset(7), 4);
        assert_eq!(columns("\u{E0041}z", PLAIN), 8);
    }

    #[test]
    fn an_odd_space_stays_and_is_marked() {
        let drawn = expand("a\u{A0}b", PLAIN);
        assert_eq!(drawn.text(), "a\u{A0}b");
        assert_eq!(drawn.marks(), [(1..3, Mark::Unusual(Unusual::Space))]);
        assert_eq!(drawn.offset(3), 3);
        assert_eq!(columns("a\u{A0}b", PLAIN), 3);
    }

    #[test]
    fn a_carriage_return_inside_a_line_is_invisible_but_one_at_its_end_is_the_ending() {
        assert_eq!(expand("a\rb", PLAIN).text(), "aU+000Db");
        assert_eq!(columns("a\rb\r\n", PLAIN), 8);
    }

    #[test]
    fn line_offset_reverses_offset_with_code_points_and_tabs() {
        let line = format!("é\t{RLO}x\u{E0041}\ty\u{A0}");
        let drawn = expand(&line, PLAIN);
        let boundaries = (0..=line.len()).filter(|&b| line.is_char_boundary(b));
        assert!(boundaries.into_iter().all(|b| drawn.line_offset(drawn.offset(b)) == b));
    }
    #[test]
    fn a_space_shows_as_a_middle_dot() {
        let drawn = expand("a b\n", SHOWN);
        assert_eq!(drawn.text(), "a·b");
        assert_eq!(drawn.marks(), [(1..3, Mark::Whitespace)]);
        // b, line byte 2, draws after a and the two bytes of the dot.
        assert_eq!(drawn.offset(2), 3);
        assert_eq!(drawn.line_offset(3), 2);
    }

    #[test]
    fn a_tab_shows_as_an_arrow_then_spaces_to_the_stop() {
        // a at column 0; the tab fills 1-3: the arrow, then two spaces.
        let drawn = expand("a\tb\n", SHOWN);
        assert_eq!(drawn.text(), "a→  b");
        assert_eq!(drawn.marks(), [(1..4, Mark::Whitespace)]);
        assert_eq!(drawn.offset(2), 6);
        // A tab one column wide is the arrow alone.
        assert_eq!(expand("abc\tx\n", SHOWN).text(), "abc→x");
    }

    #[test]
    fn a_carriage_return_in_the_ending_shows_after_the_text() {
        let drawn = expand("a\r\n", SHOWN);
        assert_eq!(drawn.text(), "a␍");
        assert_eq!(drawn.marks(), [(1..4, Mark::Whitespace)]);
        assert_eq!(drawn.offset(1), 1);
    }

    #[test]
    fn a_last_line_without_a_newline_ends_in_the_mark() {
        let drawn = expand("a", SHOWN);
        assert_eq!(drawn.text(), "a∅");
        assert_eq!(drawn.marks(), [(1..4, Mark::Whitespace)]);
        assert_eq!(expand("a\r", SHOWN).text(), "a␍∅");
        assert_eq!(expand("\r\n", SHOWN).text(), "␍");
        assert_eq!(expand("\n", SHOWN).text(), "");
    }

    #[test]
    fn a_byte_in_the_ending_marks_maps_to_the_end_of_the_text() {
        // a, then ␍ at 1..4 and ∅ at 4..7: every byte past a is line byte 1.
        let drawn = expand("a\r", SHOWN);
        assert_eq!(
            (0..=9).map(|b| drawn.line_offset(b)).collect::<Vec<_>>(),
            [0, 1, 1, 1, 1, 1, 1, 1, 1, 1]
        );
        assert_eq!(expand("a\r\n", PLAIN).line_offset(9), 1);
    }

    #[test]
    fn hidden_whitespace_draws_as_before() {
        assert_eq!(expand("a b\tc\r\n", PLAIN).text(), "a b c");
        assert_eq!(expand("a b\tc\r\n", PLAIN).marks(), []);
        assert!(matches!(expand("ab", PLAIN).into_text(), Cow::Borrowed("ab")));
        assert!(matches!(expand("ab\n", SHOWN).into_text(), Cow::Borrowed("ab")));
    }

    #[test]
    fn an_odd_space_keeps_its_shade_with_whitespace_shown() {
        let drawn = expand("a\u{A0}\n", SHOWN);
        assert_eq!(drawn.text(), "a\u{A0}");
        assert_eq!(drawn.marks(), [(1..3, Mark::Unusual(Unusual::Space))]);
    }

    #[test]
    fn shown_whitespace_counts_its_ending_marks_as_columns() {
        // a, the tab to column 4, b, then ␍.
        assert_eq!(columns("a\tb\r\n", SHOWN), 6);
        assert_eq!(columns("ab", SHOWN), 3);
        assert_eq!(columns("ab\n", SHOWN), 2);
        assert_eq!(columns("\r\n", SHOWN), 1);
        assert_eq!(columns("a\r", SHOWN), 3);
        assert_eq!(widest(["ab\n", "abc"].into_iter(), SHOWN), 4);
    }

    #[test]
    fn line_offset_reverses_offset_with_whitespace_shown() {
        for line in ["é \tx\u{202E} y\r\n", "a\t \t", " \r"] {
            let drawn = expand(line, SHOWN);
            let body = line.trim_end_matches(['\n', '\r']).len();
            let boundaries = (0..=body).filter(|&b| line.is_char_boundary(b));
            assert!(
                boundaries.into_iter().all(|b| drawn.line_offset(drawn.offset(b)) == b),
                "{line:?}"
            );
        }
    }
}
