//! A selection in one of the top panes (spec §5): two byte offsets in the text of the input
//! the pane shows. The inputs never change, so the offsets hold across rows, wrap and panes.

use std::ops::Range;

use merge_core::merge3::Source;
use merge_core::text::Text;

/// A selection in `source`'s text, from `anchor`, where it started, to `head`, where it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSelection {
    source: Source,
    anchor: usize,
    head: usize,
}

impl InputSelection {
    /// A selection from `anchor` to `head` in `source`'s text.
    #[must_use]
    pub const fn new(source: Source, anchor: usize, head: usize) -> Self {
        Self { source, anchor, head }
    }

    /// An empty selection at `at`.
    #[must_use]
    pub const fn caret(source: Source, at: usize) -> Self {
        Self::new(source, at, at)
    }

    #[must_use]
    pub const fn source(self) -> Source {
        self.source
    }

    #[must_use]
    pub const fn anchor(self) -> usize {
        self.anchor
    }

    /// This selection with its end moved to `head`.
    #[must_use]
    pub const fn with_head(self, head: usize) -> Self {
        Self { head, ..self }
    }

    /// The selected bytes, in order.
    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// The selected text of `content`, the input's text. Ends past it stop at its end.
    #[must_use]
    pub fn text(self, content: &str) -> &str {
        let Range { start, end } = self.range();
        content.get(start.min(content.len())..end.min(content.len())).unwrap_or_default()
    }

    /// The selected part of the line of `source` at `line`, the line's bytes in the input
    /// without its terminator, in bytes of the line. `None` for another input or where the
    /// selection leaves the line's text out.
    #[must_use]
    pub fn in_line(self, source: Source, line: Range<usize>) -> Option<Range<usize>> {
        let selected = self.range();
        let (start, end) = (selected.start.max(line.start), selected.end.min(line.end));
        (source == self.source && start < end).then(|| start - line.start..end - line.start)
    }
}

/// The byte of `text` at `column` of `line`, or at the start of line `line` when `line` is
/// `Err`, as [`crate::rows::RowMap::line_at_row`] gives it for a filler row. A column past
/// the line's text is its end, before the terminator.
#[must_use]
pub fn offset(text: &Text, line: Result<usize, usize>, column: usize) -> usize {
    match line {
        Ok(n) => {
            let start = line_bytes(text, n).start;
            start + column.min(text.line(n).trim_end_matches(['\n', '\r']).len())
        }
        Err(n) => text.span(0..n.min(text.line_count())).len(),
    }
}

/// The bytes of `text`'s line `line` with its terminator.
#[must_use]
pub fn line_bytes(text: &Text, line: usize) -> Range<usize> {
    let start = text.span(0..line).len();
    start..start + text.line(line).len()
}

/// The word of `line`, without its terminator, at byte `column`: the run of letters, digits
/// and `_` holding the character there, else that one character. At the end of the line, the
/// character before it; an empty line gives an empty range.
#[must_use]
pub fn word_at(line: &str, column: usize) -> Range<usize> {
    let line = line.trim_end_matches(['\n', '\r']);
    let column = line.floor_char_boundary(column.min(line.len()));
    let under = line[column..].chars().next().map(|c| (column, c));
    let Some((at, c)) = under.or_else(|| line[..column].char_indices().next_back()) else {
        return column..column;
    };
    if !is_word(c) {
        return at..at + c.len_utf8();
    }
    let start = line[..at]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_word(c))
        .last()
        .map_or(at, |(i, _)| i);
    let end =
        line[at..].char_indices().find(|&(_, c)| !is_word(c)).map_or(line.len(), |(i, _)| at + i);
    start..end
}

/// A character a word holds.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use merge_core::testing::text;

    const LEFT: Source = Source::Side(0);
    const RIGHT: Source = Source::Side(1);

    #[test]
    fn the_range_runs_from_the_lower_end_either_way() {
        assert_eq!(InputSelection::new(LEFT, 2, 7).range(), 2..7);
        assert_eq!(InputSelection::new(LEFT, 7, 2).range(), 2..7);
        assert_eq!(InputSelection::caret(LEFT, 4).range(), 4..4);
        assert_eq!(InputSelection::new(LEFT, 7, 2).with_head(9), InputSelection::new(LEFT, 7, 9));
    }

    #[test]
    fn the_text_keeps_terminators_and_tabs() {
        let content = "ab\r\n\tc\nd";
        assert_eq!(InputSelection::new(LEFT, 1, 6).text(content), "b\r\n\tc");
        assert_eq!(InputSelection::new(LEFT, 6, 1).text(content), "b\r\n\tc");
        assert_eq!(InputSelection::caret(LEFT, 3).text(content), "");
    }

    #[test]
    fn the_text_stops_at_the_end_of_the_input() {
        assert_eq!(InputSelection::new(LEFT, 1, 40).text("abc"), "bc");
        assert_eq!(InputSelection::new(LEFT, 40, 50).text("abc"), "");
    }

    #[test]
    fn a_line_shows_the_part_of_the_selection_inside_its_text() {
        // The line `cdef` at bytes 3..7 of the input.
        let line = 3..7;
        assert_eq!(InputSelection::new(LEFT, 4, 6).in_line(LEFT, line.clone()), Some(1..3));
        assert_eq!(InputSelection::new(LEFT, 0, 5).in_line(LEFT, line.clone()), Some(0..2));
        assert_eq!(InputSelection::new(LEFT, 9, 5).in_line(LEFT, line.clone()), Some(2..4));
        assert_eq!(InputSelection::new(LEFT, 0, 20).in_line(LEFT, line.clone()), Some(0..4));
    }

    #[test]
    fn a_line_outside_the_selection_or_of_another_input_shows_none() {
        let line = 3..7;
        assert_eq!(InputSelection::new(LEFT, 0, 3).in_line(LEFT, line.clone()), None);
        assert_eq!(InputSelection::new(LEFT, 7, 9).in_line(LEFT, line.clone()), None);
        assert_eq!(InputSelection::caret(LEFT, 5).in_line(LEFT, line.clone()), None);
        assert_eq!(InputSelection::new(LEFT, 0, 9).in_line(RIGHT, line), None);
        // An empty line inside the selection has no text to show.
        assert_eq!(InputSelection::new(LEFT, 0, 9).in_line(LEFT, 4..4), None);
    }

    #[test]
    fn an_offset_counts_the_lines_before_it() {
        let input = text("ab\r\ncde\nf");
        assert_eq!(offset(&input, Ok(0), 0), 0);
        assert_eq!(offset(&input, Ok(0), 1), 1);
        assert_eq!(offset(&input, Ok(1), 2), 6);
        assert_eq!(offset(&input, Ok(2), 1), 9);
    }

    #[test]
    fn a_column_past_the_text_is_the_end_before_the_terminator() {
        let input = text("ab\r\ncde\nf");
        assert_eq!(offset(&input, Ok(0), 9), 2);
        assert_eq!(offset(&input, Ok(1), 9), 7);
        assert_eq!(offset(&input, Ok(2), 9), 9);
    }

    #[test]
    fn a_filler_row_is_the_start_of_the_next_line() {
        let input = text("ab\r\ncde\nf");
        assert_eq!(offset(&input, Err(1), 2), 4);
        assert_eq!(offset(&input, Err(3), 0), 9);
        assert_eq!(offset(&text(""), Err(0), 5), 0);
    }

    #[test]
    fn line_bytes_include_the_terminator() {
        let input = text("ab\r\ncde\nf");
        assert_eq!(line_bytes(&input, 0), 0..4);
        assert_eq!(line_bytes(&input, 1), 4..8);
        assert_eq!(line_bytes(&input, 2), 8..9);
    }

    #[test]
    fn a_word_runs_over_letters_digits_and_underscores() {
        let line = "let foo_2 = bar;";
        assert_eq!(word_at(line, 4), 4..9);
        assert_eq!(word_at(line, 6), 4..9);
        assert_eq!(word_at(line, 8), 4..9);
        assert_eq!(word_at(line, 0), 0..3);
        assert_eq!(word_at(line, 12), 12..15);
    }

    #[test]
    fn another_character_is_a_word_alone() {
        let line = "a  = b;";
        assert_eq!(word_at(line, 1), 1..2);
        assert_eq!(word_at(line, 3), 3..4);
        assert_eq!(word_at(line, 6), 6..7);
    }

    #[test]
    fn at_the_end_the_word_is_the_one_before() {
        assert_eq!(word_at("ab cd", 5), 3..5);
        assert_eq!(word_at("ab;", 3), 2..3);
        assert_eq!(word_at("", 0), 0..0);
        assert_eq!(word_at("ab", 9), 0..2);
    }

    #[test]
    fn a_word_takes_whole_characters() {
        // é is two bytes; byte 2 is inside it.
        assert_eq!(word_at("xé y", 2), 0..3);
        assert_eq!(word_at("x→y", 2), 1..4);
    }
}
