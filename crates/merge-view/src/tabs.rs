//! A line as it draws outside an editor, such as in the merge window's side panes: each tab
//! becomes the spaces up to the next stop, counted in characters (spec §6.4), and each
//! invisible character becomes its code point, `U+202E`, which draws as a badge (spec §5).

use std::borrow::Cow;
use std::ops::Range;

use merge_core::unusual::{self, CodePoint, Unusual};
use smallvec::SmallVec;

/// Replacements a line usually holds before the list spills to the heap.
const INLINE_REPLACEMENTS: usize = 4;

/// One character the drawn text replaces: a tab or an invisible character.
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
    /// The unusual characters, in bytes of the drawn text: an invisible character's code
    /// point, or a space other than U+0020.
    marks: SmallVec<[(Range<usize>, Unusual); INLINE_REPLACEMENTS]>,
}

/// Expand the tabs in `line` to stops every `tab_width` characters, and write each invisible
/// character as its code point, which takes a column per character. A width of 0 is taken
/// as 1.
#[must_use]
pub fn expand(line: &str, tab_width: usize) -> Expanded<'_> {
    let plain = |c: char| c != '\t' && unusual::classify(c).is_none();
    if line.chars().all(plain) {
        return Expanded {
            text: Cow::Borrowed(line),
            replaced: SmallVec::new(),
            marks: SmallVec::new(),
        };
    }
    let width = tab_width.max(1);
    let mut text = String::with_capacity(line.len() + width * line.matches('\t').count());
    let mut replaced = SmallVec::new();
    let mut marks = SmallVec::new();
    let mut column = 0;
    for (at, c) in line.char_indices() {
        let start = text.len();
        match (c, unusual::classify(c)) {
            ('\t', _) => {
                let spaces = width - column % width;
                text.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            (_, Some(Unusual::Invisible)) => {
                let label = CodePoint(c);
                // Writing to a `String` cannot fail.
                let _ = std::fmt::Write::write_fmt(&mut text, format_args!("{label}"));
                column += label.width();
                marks.push((start..text.len(), Unusual::Invisible));
            }
            (_, kind) => {
                text.push(c);
                column += 1;
                if let Some(kind) = kind {
                    marks.push((start..text.len(), kind));
                }
                continue;
            }
        }
        replaced.push(Replaced { at, len: c.len_utf8(), added: text.len() - at - c.len_utf8() });
    }
    Expanded { text: Cow::Owned(text), replaced, marks }
}

/// The columns `line` fills as [`expand`] draws it, without its line ending. A width of 0 is
/// taken as 1.
#[must_use]
pub fn columns(line: &str, tab_width: usize) -> usize {
    let width = tab_width.max(1);
    line.trim_end_matches(['\n', '\r']).chars().fold(0, |column, c| {
        match (c, unusual::classify(c)) {
            ('\t', _) => column + width - column % width,
            (_, Some(Unusual::Invisible)) => column + CodePoint(c).width(),
            _ => column + 1,
        }
    })
}

/// The most [`columns`] of any of `lines`; 0 without lines.
#[must_use]
pub fn widest<'a>(lines: impl Iterator<Item = &'a str>, tab_width: usize) -> usize {
    lines.map(|line| columns(line, tab_width)).max().unwrap_or(0)
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

    /// The unusual characters, in bytes of the drawn text, in order.
    #[must_use]
    pub fn marks(&self) -> &[(Range<usize>, Unusual)] {
        &self.marks
    }

    /// Where byte `offset` of the line lands in the drawn text. A replaced character's own
    /// offset is the start of its replacement.
    #[must_use]
    pub fn offset(&self, offset: usize) -> usize {
        match self.replaced.partition_point(|r| r.at < offset) {
            0 => offset,
            n => offset + self.replaced[n - 1].added,
        }
    }

    /// The byte of the line that byte `at` of the drawn text shows: [`Self::offset`] reversed.
    /// A byte inside a replacement goes to the nearer side of its character, the start on a
    /// tie. A byte past the end is the line's end.
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
                return if at - start <= end - at { r.at } else { r.at + r.len };
            }
            added = r.added;
        }
        at - added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: usize = 4;

    #[test]
    fn a_tab_after_text_runs_to_the_next_stop() {
        let line = expand("a\tb", WIDTH);
        assert_eq!(line.text(), "a   b");
        assert_eq!((0..=3).map(|b| line.offset(b)).collect::<Vec<_>>(), [0, 1, 4, 5]);
    }

    #[test]
    fn a_tab_at_a_stop_is_a_whole_stop() {
        assert_eq!(expand("abcd\tx", WIDTH).text(), "abcd    x");
        assert_eq!(expand("\tx", WIDTH).text(), "    x");
    }

    #[test]
    fn tabs_in_a_row_each_take_a_stop() {
        let line = expand("\t\tx", WIDTH);
        assert_eq!(line.text(), "        x");
        assert_eq!(line.offset(1), 4);
        assert_eq!(line.offset(2), 8);
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let line = expand("é\tx", WIDTH);
        assert_eq!(line.text(), "é   x");
        assert_eq!(line.offset(3), 5);
    }

    #[test]
    fn the_width_sets_the_stops() {
        assert_eq!(expand("a\tb", 8).text(), "a       b");
        assert_eq!(expand("a\tb", 1).text(), "a b");
        assert_eq!(expand("a\tb", 0).text(), "a b");
    }

    #[test]
    fn a_line_without_tabs_is_borrowed() {
        let line = expand("ab", WIDTH);
        assert!(matches!(line.text, Cow::Borrowed("ab")));
        assert_eq!(line.offset(2), 2);
        assert!(matches!(line.into_text(), Cow::Borrowed("ab")));
    }

    #[test]
    fn columns_expand_tabs_and_count_characters() {
        assert_eq!(columns("", WIDTH), 0);
        assert_eq!(columns("abc", WIDTH), 3);
        // a, the tab to column 4, then é and x.
        assert_eq!(columns("a\téx", WIDTH), 6);
        assert_eq!(columns("\t\t", WIDTH), 8);
        assert_eq!(columns("abcd\t", WIDTH), 8);
        assert_eq!(columns("a\t", 0), 2);
    }

    #[test]
    fn columns_leave_out_the_line_ending() {
        assert_eq!(columns("ab\n", WIDTH), 2);
        assert_eq!(columns("ab\r\n", WIDTH), 2);
        assert_eq!(columns("\r\n", WIDTH), 0);
    }

    #[test]
    fn widest_takes_the_most_columns_of_any_line() {
        assert_eq!(widest(std::iter::empty(), WIDTH), 0);
        assert_eq!(widest(["abcdef\n", "\tab\n", "x"].into_iter(), WIDTH), 6);
        assert_eq!(widest(["abcdef\n", "\t\tab\n"].into_iter(), WIDTH), 10);
    }

    #[test]
    fn a_byte_of_the_expanded_text_maps_back_to_its_line_byte() {
        // a at 0, the tab's spaces 1..4, b at 4.
        let line = expand("a\tb", WIDTH);
        assert_eq!((0..=6).map(|b| line.line_offset(b)).collect::<Vec<_>>(), [0, 1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn a_tie_inside_a_tab_goes_to_its_start() {
        // Two tabs of four spaces each: 0..4 and 4..8; 2 and 6 sit midway.
        let line = expand("\t\tx", WIDTH);
        assert_eq!(
            (0..=9).map(|b| line.line_offset(b)).collect::<Vec<_>>(),
            [0, 0, 0, 1, 1, 1, 1, 2, 2, 3]
        );
    }

    #[test]
    fn line_offset_reverses_offset() {
        let line = "é\tx\ty";
        let expanded = expand(line, WIDTH);
        let boundaries = (0..=line.len()).filter(|&b| line.is_char_boundary(b));
        assert!(boundaries.into_iter().all(|b| expanded.line_offset(expanded.offset(b)) == b));
    }

    #[test]
    fn past_the_end_is_the_lines_end() {
        assert_eq!(expand("ab", WIDTH).line_offset(1), 1);
        assert_eq!(expand("ab", WIDTH).line_offset(9), 2);
        assert_eq!(expand("a\t", WIDTH).line_offset(9), 2);
    }

    #[test]
    fn a_second_tab_counts_the_columns_the_first_one_filled() {
        // a at column 0, the first tab fills 1-3, b at 4, the second tab fills 5-7.
        let line = expand("a\tb\tc", WIDTH);
        assert_eq!(line.text(), "a   b   c");
        assert_eq!(line.offset(4), 8);
        assert_eq!(line.into_text(), "a   b   c");
    }

    const RLO: char = '\u{202E}';

    #[test]
    fn an_invisible_character_draws_as_its_code_point() {
        let line = format!("a{RLO}b");
        let drawn = expand(&line, WIDTH);
        assert_eq!(drawn.text(), "aU+202Eb");
        assert_eq!(drawn.marks(), [(1..7, Unusual::Invisible)]);
        // a at 0, RLO's three bytes at 1, b at 4: b draws at 1 + 6.
        assert_eq!([0, 1, 4, 5].map(|b| drawn.offset(b)), [0, 1, 7, 8]);
    }

    #[test]
    fn a_byte_inside_a_code_point_goes_to_the_nearer_side_of_its_character() {
        // The label U+202E fills 1..7; its start is line byte 1, its end line byte 4.
        let line = format!("a{RLO}b");
        let drawn = expand(&line, WIDTH);
        assert_eq!(
            (0..=8).map(|b| drawn.line_offset(b)).collect::<Vec<_>>(),
            [0, 1, 1, 1, 1, 4, 4, 4, 5]
        );
    }

    #[test]
    fn a_code_point_fills_a_column_per_character_before_a_tab() {
        // U+200B fills columns 0-5, so the tab runs from 6 to the stop at 8.
        let drawn = expand("\u{200B}\tx", WIDTH);
        assert_eq!(drawn.text(), "U+200B  x");
        assert_eq!(drawn.marks(), [(0..6, Unusual::Invisible)]);
        assert_eq!(columns("\u{200B}\tx", WIDTH), 9);
    }

    #[test]
    fn a_character_past_the_basic_plane_takes_its_whole_code_point() {
        let drawn = expand("\u{E0041}z", WIDTH);
        assert_eq!(drawn.text(), "U+E0041z");
        assert_eq!(drawn.offset(4), 7);
        assert_eq!(drawn.line_offset(7), 4);
        assert_eq!(columns("\u{E0041}z", WIDTH), 8);
    }

    #[test]
    fn an_odd_space_stays_and_is_marked() {
        let drawn = expand("a\u{A0}b", WIDTH);
        assert_eq!(drawn.text(), "a\u{A0}b");
        assert_eq!(drawn.marks(), [(1..3, Unusual::Space)]);
        assert_eq!(drawn.offset(3), 3);
        assert_eq!(columns("a\u{A0}b", WIDTH), 3);
    }

    #[test]
    fn a_carriage_return_inside_a_line_is_invisible_but_one_at_its_end_is_the_ending() {
        assert_eq!(expand("a\rb", WIDTH).text(), "aU+000Db");
        assert_eq!(columns("a\rb\r\n", WIDTH), 8);
    }

    #[test]
    fn line_offset_reverses_offset_with_code_points_and_tabs() {
        let line = format!("é\t{RLO}x\u{E0041}\ty\u{A0}");
        let drawn = expand(&line, WIDTH);
        let boundaries = (0..=line.len()).filter(|&b| line.is_char_boundary(b));
        assert!(boundaries.into_iter().all(|b| drawn.line_offset(drawn.offset(b)) == b));
    }
}
