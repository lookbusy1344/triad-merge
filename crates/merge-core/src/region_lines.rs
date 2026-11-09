//! Each region's lines in the merge result, in a Fenwick tree of newline counts, so the line a
//! region starts on costs O(log n) and no lookup in the text. Only `\n` ends a line.

use std::ops::Range;

use ropey::Rope;
use ropey::str_utils::byte_to_line_idx;

use crate::lengths::RegionLengths;

/// The lines one region's text holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LineCount {
    /// The `\n` bytes in the text.
    newlines: usize,
    /// The lines the text touches: none when empty, else one more than its newlines unless
    /// its last byte is one.
    spanned: usize,
}

impl LineCount {
    pub(crate) fn of(text: &str) -> Self {
        let newlines = byte_to_line_idx(text, text.len());
        let terminated = text.ends_with('\n');
        Self::new(newlines, text.is_empty(), terminated)
    }

    /// The lines of `bytes` in `rope`, at O(log n) in the rope's length.
    pub(crate) fn in_rope(rope: &Rope, bytes: Range<usize>) -> Self {
        let slice = rope.byte_slice(bytes);
        let len = slice.len_bytes();
        // ropey counts a line after the last newline, empty or not.
        let newlines = slice.len_lines() - 1;
        let terminated = len > 0 && slice.byte(len - 1) == b'\n';
        Self::new(newlines, len == 0, terminated)
    }

    fn new(newlines: usize, empty: bool, terminated: bool) -> Self {
        let spanned = if empty { 0 } else { newlines + usize::from(!terminated) };
        Self { newlines, spanned }
    }
}

/// The [`LineCount`] of each region, in order.
#[derive(Debug, Clone)]
pub(crate) struct RegionLines {
    newlines: RegionLengths,
    counts: Vec<LineCount>,
}

impl Default for RegionLines {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl RegionLines {
    pub(crate) fn new(counts: Vec<LineCount>) -> Self {
        Self { newlines: RegionLengths::new(counts.iter().map(|c| c.newlines).collect()), counts }
    }

    /// The lines `region` covers: from the line its first byte is on to the line after its
    /// last byte, or none at the line it would start on.
    pub(crate) fn of(&self, region: usize) -> Range<usize> {
        let start = self.newlines.start(region);
        start..start + self.counts[region].spanned
    }

    pub(crate) fn set(&mut self, region: usize, count: LineCount) {
        self.newlines.set(region, count.newlines);
        self.counts[region] = count;
    }

    /// Replace the counts of the regions in `range` with `counts`: O(n).
    pub(crate) fn splice(
        &mut self,
        range: Range<usize>,
        counts: impl IntoIterator<Item = LineCount>,
    ) {
        let counts: Vec<LineCount> = counts.into_iter().collect();
        self.newlines.splice(range.clone(), counts.iter().map(|c| c.newlines));
        self.counts.splice(range, counts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_spans_its_lines_and_an_unterminated_last_one() {
        assert_eq!(LineCount::of(""), LineCount { newlines: 0, spanned: 0 });
        assert_eq!(LineCount::of("a"), LineCount { newlines: 0, spanned: 1 });
        assert_eq!(LineCount::of("\n"), LineCount { newlines: 1, spanned: 1 });
        assert_eq!(LineCount::of("a\nb"), LineCount { newlines: 1, spanned: 2 });
        assert_eq!(LineCount::of("a\r\nb\r\n"), LineCount { newlines: 2, spanned: 2 });
        // A lone `\r` ends no line.
        assert_eq!(LineCount::of("a\rb"), LineCount { newlines: 0, spanned: 1 });
    }

    #[test]
    fn a_rope_slice_counts_as_its_text_does() {
        const TEXT: &str = "a\nb\rc\r\nd";
        let rope = Rope::from_str(TEXT);
        let len = TEXT.len();
        for start in 0..=len {
            for end in start..=len {
                let text = &TEXT[start..end];
                assert_eq!(
                    LineCount::in_rope(&rope, start..end),
                    LineCount::of(text),
                    "{start}..{end}"
                );
            }
        }
    }

    #[test]
    fn regions_start_after_the_newlines_before_them() {
        // `a\n`, empty, `b`, `\nc\n`: lines 0..1, 1..1, 1..2, 1..3.
        let mut lines =
            RegionLines::new(["a\n", "", "b", "\nc\n"].into_iter().map(LineCount::of).collect());
        let all = |lines: &RegionLines| (0..4).map(|r| lines.of(r)).collect::<Vec<_>>();
        assert_eq!(all(&lines), [0..1, 1..1, 1..2, 1..3]);
        // The empty region takes `x\ny\n`: two lines, and the rest move down two.
        lines.set(1, LineCount::of("x\ny\n"));
        assert_eq!(all(&lines), [0..1, 1..3, 3..4, 3..5]);
        // Regions 1 and 2 become one `z`.
        lines.splice(1..3, [LineCount::of("z")]);
        assert_eq!((0..3).map(|r| lines.of(r)).collect::<Vec<_>>(), [0..1, 1..2, 1..3]);
    }
}
