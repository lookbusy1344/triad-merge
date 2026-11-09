//! Tab stops for text drawn outside an editor, such as the merge window's side panes (spec
//! §6.4). Each tab becomes the spaces up to the next stop, counted in characters.

use std::borrow::Cow;

use smallvec::SmallVec;

/// Tabs a line usually holds before the list spills to the heap.
const INLINE_TABS: usize = 4;

/// A line with its tabs expanded, and the map from its byte offsets to the expanded text's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded<'a> {
    text: Cow<'a, str>,
    /// Each tab's byte offset in the line, and the bytes added by it and every tab before.
    tabs: SmallVec<[(usize, usize); INLINE_TABS]>,
}

/// Expand the tabs in `line` to stops every `tab_width` characters. A width of 0 is taken as 1.
#[must_use]
pub fn expand(line: &str, tab_width: usize) -> Expanded<'_> {
    if !line.contains('\t') {
        return Expanded { text: Cow::Borrowed(line), tabs: SmallVec::new() };
    }
    let width = tab_width.max(1);
    let mut text = String::with_capacity(line.len() + width * line.matches('\t').count());
    let mut tabs = SmallVec::new();
    let mut column = 0;
    for (at, c) in line.char_indices() {
        if c == '\t' {
            let spaces = width - column % width;
            text.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
            tabs.push((at, text.len() - at - 1));
        } else {
            text.push(c);
            column += 1;
        }
    }
    Expanded { text: Cow::Owned(text), tabs }
}

/// The columns `line` fills with its tabs expanded to stops every `tab_width` characters,
/// without its line ending. A width of 0 is taken as 1.
#[must_use]
pub fn columns(line: &str, tab_width: usize) -> usize {
    let width = tab_width.max(1);
    line.trim_end_matches(['\n', '\r'])
        .chars()
        .fold(0, |column, c| if c == '\t' { column + width - column % width } else { column + 1 })
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

    /// Where byte `offset` of the line lands in the expanded text. A tab's own offset is the
    /// start of its spaces.
    #[must_use]
    pub fn offset(&self, offset: usize) -> usize {
        match self.tabs.partition_point(|&(at, _)| at < offset) {
            0 => offset,
            n => offset + self.tabs[n - 1].1,
        }
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
    fn a_second_tab_counts_the_columns_the_first_one_filled() {
        // a at column 0, the first tab fills 1-3, b at 4, the second tab fills 5-7.
        let line = expand("a\tb\tc", WIDTH);
        assert_eq!(line.text(), "a   b   c");
        assert_eq!(line.offset(4), 8);
        assert_eq!(line.into_text(), "a   b   c");
    }
}
