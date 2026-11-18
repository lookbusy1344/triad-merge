//! Where the lines of a text land in a new version of it, for a reload from disk that keeps
//! the caret and the view (spec §6.1).
//!
//! The map is an exact line diff of the old text against the new. A line outside every
//! change keeps its place among the unchanged lines; a line a change replaced or removed
//! lands on the first line the change put in its place. A change that only inserts lines
//! puts them above the line it sits before.

use ropey::{Rope, RopeSlice};

use crate::diff::{DiffOptions, Hunk, diff_lines};
use crate::text::{Encoding, Text};

/// The changes from one text to another, by line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMap {
    hunks: Vec<Hunk>,
}

impl LineMap {
    #[must_use]
    pub fn new(before: &Text, after: &Text) -> Self {
        Self { hunks: diff_lines(before, after, &DiffOptions::default()) }
    }

    /// The map from `before`, a pane's text, to `after`. The diff needs `before` as one
    /// string, so this copies it; run it off the UI thread.
    #[must_use]
    pub fn from_rope(before: &Rope, after: &Text) -> Self {
        // The encoding plays no part in diffing.
        Self::new(&Text::new(Encoding::Utf8, before.to_string()), after)
    }

    /// Where line `line` of the old text sits in the new.
    #[must_use]
    pub fn line(&self, line: usize) -> usize {
        self.place(line).line()
    }

    /// Where byte `offset` of `before` sits in `after`, the texts the map was built from. On
    /// an unchanged line it keeps its column; on a changed one it goes to the start of the
    /// change's new lines. An offset past either end is cut to it.
    #[must_use]
    pub fn offset(&self, before: RopeSlice<'_>, after: RopeSlice<'_>, offset: usize) -> usize {
        let offset = offset.min(before.len_bytes());
        let line = before.byte_to_line(offset);
        let (line, column) = match self.place(line) {
            Place::Kept(to) => (to, offset - before.line_to_byte(line)),
            Place::Changed(to) => (to, 0),
        };
        let start = after.line_to_byte(line.min(after.len_lines()));
        (start + column).min(after.len_bytes())
    }

    fn place(&self, line: usize) -> Place {
        let passed = self.hunks.partition_point(|hunk| hunk.before.end <= line);
        if let Some(hunk) = self.hunks.get(passed)
            && hunk.before.start <= line
        {
            return Place::Changed(hunk.after.start);
        }
        match passed.checked_sub(1).and_then(|last| self.hunks.get(last)) {
            Some(last) => Place::Kept(last.after.end + (line - last.before.end)),
            None => Place::Kept(line),
        }
    }
}

/// Where a line of the old text lands in the new.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// An unchanged line, at this line of the new text.
    Kept(usize),
    /// A line a change replaced or removed; the change's new lines start here.
    Changed(usize),
}

impl Place {
    fn line(self) -> usize {
        match self {
            Self::Kept(line) | Self::Changed(line) => line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes()).unwrap()
    }

    fn map(before: &str, after: &str) -> LineMap {
        LineMap::new(&text(before), &text(after))
    }

    /// Where `offset` of `before` lands in `after`.
    fn offset(before: &str, after: &str, offset: usize) -> usize {
        let (from, to) = (Rope::from(before), Rope::from(after));
        map(before, after).offset(from.slice(..), to.slice(..), offset)
    }

    #[test]
    fn the_same_text_maps_each_line_to_itself() {
        let m = map("a\nb\nc\n", "a\nb\nc\n");
        assert_eq!([0, 1, 2, 3].map(|l| m.line(l)), [0, 1, 2, 3]);
    }

    #[test]
    fn lines_inserted_above_push_a_line_down() {
        let m = map("a\nb\nc\n", "x\ny\na\nb\nc\n");
        assert_eq!([0, 1, 2].map(|l| m.line(l)), [2, 3, 4]);
    }

    #[test]
    fn lines_removed_above_pull_a_line_up() {
        let m = map("a\nb\nc\nd\n", "c\nd\n");
        assert_eq!([2, 3].map(|l| m.line(l)), [0, 1]);
    }

    #[test]
    fn a_change_below_leaves_a_line_in_place() {
        let m = map("a\nb\nc\n", "a\nb\nz\nw\n");
        assert_eq!([0, 1].map(|l| m.line(l)), [0, 1]);
    }

    #[test]
    fn a_replaced_line_goes_to_the_start_of_its_replacement() {
        // b and c become x, y and z: both land on x, line 1.
        let m = map("a\nb\nc\nd\n", "a\nx\ny\nz\nd\n");
        assert_eq!([1, 2].map(|l| m.line(l)), [1, 1]);
        assert_eq!(m.line(3), 4, "d sits after the three new lines");
    }

    #[test]
    fn a_removed_line_goes_to_the_line_that_follows_the_removal() {
        let m = map("a\nb\nc\n", "a\nc\n");
        assert_eq!(m.line(1), 1, "b's place is now c's");
        assert_eq!(m.line(2), 1);
    }

    #[test]
    fn an_insertion_before_a_line_puts_the_new_lines_above_it() {
        let m = map("a\nb\n", "a\nx\nb\n");
        assert_eq!(m.line(1), 2);
    }

    #[test]
    fn the_caret_keeps_its_column_on_a_line_that_moved() {
        // Byte 4 sits after "ab" on line 1; that line moves down two.
        assert_eq!(offset("q\nabc\n", "n\nn\nq\nabc\n", 4), 8);
    }

    #[test]
    fn the_caret_keeps_a_column_after_multibyte_characters() {
        // "é" is two bytes: the caret after "éa" on line 1 sits at byte 2 + 3 = 5.
        assert_eq!(offset("x\néab\n", "new\nx\néab\n", 5), 9);
    }

    #[test]
    fn the_caret_on_a_changed_line_goes_to_the_start_of_the_change() {
        assert_eq!(offset("a\nbbbb\nc\n", "a\nzz\nyy\nc\n", 4), 2);
    }

    #[test]
    fn the_caret_on_a_removed_last_line_goes_to_the_end() {
        assert_eq!(offset("a\nbcd\n", "a\n", 4), 2);
    }

    #[test]
    fn the_caret_at_the_end_follows_lines_added_at_the_end() {
        assert_eq!(offset("a\nb\n", "a\nb\nc\n", 4), 6);
    }

    #[test]
    fn the_caret_at_the_end_stays_at_the_end_when_the_final_newline_goes() {
        assert_eq!(offset("a\nb\n", "a\nb", 4), 3);
    }

    #[test]
    fn the_caret_keeps_its_column_on_crlf_lines() {
        assert_eq!(offset("a\r\nbc\r\n", "z\r\na\r\nbc\r\n", 4), 7);
    }

    #[test]
    fn a_caret_past_the_end_is_cut_to_the_end() {
        assert_eq!(offset("ab\n", "x\nab\n", 99), 5);
    }

    #[test]
    fn an_emptied_text_puts_the_caret_at_its_start() {
        assert_eq!(offset("a\nb\n", "", 3), 0);
    }

    #[test]
    fn from_rope_matches_the_map_of_its_text() {
        let after = text("x\na\nb\n");
        assert_eq!(LineMap::from_rope(&Rope::from("a\nb\n"), &after), map("a\nb\n", "x\na\nb\n"));
    }
}
