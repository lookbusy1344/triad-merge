//! Caret placement after the core's patches reach the editor. Patches are sorted and
//! disjoint, with offsets in the text before any of them.

use std::ops::Range;

use gpui_kit::component::input::Rope;
use merge_core::document::{EditBatch, TextPatch};

/// Where `offset` lands once `patches` are applied. An offset inside a replaced range, or
/// at its end, moves to the end of the new text, so a correction before the caret keeps
/// the caret after it.
pub(crate) fn map_offset(offset: usize, patches: &[TextPatch]) -> usize {
    let mut shift = 0isize;
    for patch in patches {
        if patch.range.end <= offset {
            shift += growth(patch);
        } else if patch.range.start < offset {
            return shifted(patch.range.start, shift) + patch.text.len();
        } else {
            break;
        }
    }
    shifted(offset, shift)
}

/// The end of the last patch's new text, once every patch is applied: where undo and redo
/// leave the caret.
pub(crate) fn end_of_last(patches: &[TextPatch]) -> Option<usize> {
    let (last, before) = patches.split_last()?;
    let shift = before.iter().map(growth).sum();
    Some(shifted(last.range.start, shift) + last.text.len())
}

/// A text the edit difference reads in place, byte by byte, so only a patch's new text is
/// copied: the core's rope, the editor's rope, and `str`.
pub(crate) trait ByteText {
    fn byte_len(&self) -> usize;
    fn is_char_boundary(&self, at: usize) -> bool;
    /// The bytes from `at` to the end.
    fn bytes_from(&self, at: usize) -> impl Iterator<Item = u8>;
    /// The bytes before `at`, nearest first.
    fn bytes_before(&self, at: usize) -> impl Iterator<Item = u8>;
    /// A copy of the bytes in `range`, which starts and ends on character boundaries.
    fn text(&self, range: Range<usize>) -> String;
}

impl ByteText for str {
    fn byte_len(&self) -> usize {
        self.len()
    }

    fn is_char_boundary(&self, at: usize) -> bool {
        str::is_char_boundary(self, at)
    }

    fn bytes_from(&self, at: usize) -> impl Iterator<Item = u8> {
        self.as_bytes()[at..].iter().copied()
    }

    fn bytes_before(&self, at: usize) -> impl Iterator<Item = u8> {
        self.as_bytes()[..at].iter().rev().copied()
    }

    fn text(&self, range: Range<usize>) -> String {
        self[range].to_owned()
    }
}

impl ByteText for ropey::RopeSlice<'_> {
    fn byte_len(&self) -> usize {
        self.len_bytes()
    }

    fn is_char_boundary(&self, at: usize) -> bool {
        at == self.len_bytes() || starts_char(self.byte(at))
    }

    fn bytes_from(&self, at: usize) -> impl Iterator<Item = u8> {
        self.bytes_at(at)
    }

    fn bytes_before(&self, at: usize) -> impl Iterator<Item = u8> {
        self.bytes_at(at).reversed()
    }

    fn text(&self, range: Range<usize>) -> String {
        self.byte_slice(range).to_string()
    }
}

impl ByteText for Rope {
    fn byte_len(&self) -> usize {
        self.len()
    }

    fn is_char_boundary(&self, at: usize) -> bool {
        Rope::is_char_boundary(self, at)
    }

    fn bytes_from(&self, at: usize) -> impl Iterator<Item = u8> {
        self.bytes_at(at)
    }

    fn bytes_before(&self, at: usize) -> impl Iterator<Item = u8> {
        self.bytes_at(at).reversed()
    }

    fn text(&self, range: Range<usize>) -> String {
        self.slice(range).to_string()
    }
}

/// UTF-8 continuation bytes, `10xxxxxx`: every byte of a character but its first.
const CONTINUATION: Range<u8> = 0x80..0xC0;

/// Whether `byte` starts a character in UTF-8.
fn starts_char(byte: u8) -> bool {
    !CONTINUATION.contains(&byte)
}

/// The bytes at each end of a text that a run of edits leaves as they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Untouched {
    pub(crate) head: usize,
    pub(crate) tail: usize,
}

impl Untouched {
    /// Nothing known to be equal: compare the whole texts.
    #[cfg(test)]
    pub(crate) const NONE: Self = Self { head: 0, tail: 0 };
}

/// The ends of a text of `len` bytes that `batches`, applied in order, leave as they were.
/// Each batch's patches are in the text before that batch.
pub(crate) fn untouched(batches: &[EditBatch], len: usize) -> Untouched {
    let mut len = len;
    let mut ends = Untouched { head: len, tail: len };
    for EditBatch(patches) in batches {
        for patch in patches {
            ends.head = ends.head.min(patch.range.start);
            ends.tail = ends.tail.min(len.saturating_sub(patch.range.end));
        }
        len = shifted(len, patches.iter().map(growth).sum());
    }
    ends
}

/// The one patch that turns the core's text into the editor's after `batches`. The texts are
/// read in place from the ends the batches left as they were; only the patch's new text is
/// copied.
pub(crate) fn batched_difference(
    batches: &[EditBatch],
    core: &(impl ByteText + ?Sized),
    editor: &(impl ByteText + ?Sized),
) -> Option<TextPatch> {
    difference_between(core, editor, untouched(batches, core.byte_len()))
}

/// The one patch that turns `before` into `after`: everything between their common prefix
/// and common suffix, whole characters only. The common prefix runs at least to `known.head`
/// and the suffix at least `known.tail` bytes, so the comparison starts there. `None` when
/// the texts are equal.
fn difference_between(
    before: &(impl ByteText + ?Sized),
    after: &(impl ByteText + ?Sized),
    known: Untouched,
) -> Option<TextPatch> {
    let (before_len, after_len) = (before.byte_len(), after.byte_len());
    let shortest = before_len.min(after_len);
    let head = known.head.min(shortest);
    let mut prefix = head + common(before.bytes_from(head), after.bytes_from(head));
    while !before.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let room = shortest - prefix;
    let tail = known.tail.min(room);
    let rest = before.bytes_before(before_len - tail).take(room - tail);
    let mut suffix = tail + common(rest, after.bytes_before(after_len - tail));
    while !before.is_char_boundary(before_len - suffix)
        || !after.is_char_boundary(after_len - suffix)
    {
        suffix -= 1;
    }
    let range = prefix..before_len - suffix;
    let new = prefix..after_len - suffix;
    (!range.is_empty() || !new.is_empty()).then(|| TextPatch { range, text: after.text(new) })
}

/// How many bytes `a` and `b` share before they first differ.
fn common(a: impl Iterator<Item = u8>, b: impl Iterator<Item = u8>) -> usize {
    a.zip(b).take_while(|(x, y)| x == y).count()
}

fn growth(patch: &TextPatch) -> isize {
    // A patch's lengths fit in memory, so they fit in isize.
    patch.text.len().cast_signed() - patch.range.len().cast_signed()
}

fn shifted(offset: usize, shift: isize) -> usize {
    // Panics only on a broken caller: the shift sums patches that end at or before `offset`,
    // and disjoint patches remove at most the bytes before it.
    offset.strict_add_signed(shift)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// The one patch between two whole texts.
    fn difference(before: &str, after: &str) -> Option<TextPatch> {
        difference_between(before, after, Untouched::NONE)
    }

    fn patch(range: Range<usize>, text: &str) -> TextPatch {
        TextPatch { range, text: text.to_owned() }
    }

    #[test]
    fn offsets_before_every_patch_stay() {
        assert_eq!(map_offset(2, &[patch(5..6, "xyz")]), 2);
        assert_eq!(map_offset(4, &[patch(5..5, "x")]), 4);
        assert_eq!(
            map_offset(5, &[patch(5..5, "x")]),
            6,
            "an insertion at the caret puts the caret after it"
        );
    }

    #[test]
    fn offsets_after_patches_shift_by_their_growth() {
        // "a\nb|" with "\r" inserted before the "\n" at 1: the caret moves one on.
        assert_eq!(map_offset(3, &[patch(1..1, "\r")]), 4);
        let patches = [patch(0..2, ""), patch(4..4, "xyz")];
        assert_eq!(map_offset(6, &patches), 7);
        assert_eq!(map_offset(4, &patches), 5, "a patch ending at the offset counts");
    }

    #[test]
    fn offsets_inside_a_replacement_move_to_its_end() {
        let patches = [patch(0..1, "ab"), patch(3..8, "x")];
        assert_eq!(map_offset(5, &patches), 3 + 1 + 1);
    }

    #[test]
    fn the_difference_spans_every_change() {
        assert_eq!(difference("abc", "abc"), None);
        assert_eq!(difference("one\ntwo", "one\nx\ntwo"), Some(patch(4..4, "x\n")));
        assert_eq!(difference("abcdef", "aXcdeY"), Some(patch(1..6, "XcdeY")));
        assert_eq!(difference("abc", ""), Some(patch(0..3, "")));
        assert_eq!(difference("", "abc"), Some(patch(0..0, "abc")));
    }

    #[test]
    fn the_difference_overlaps_neither_end_twice() {
        // "aa" to "aaa": the prefix takes both "a"s, so the suffix has no room.
        assert_eq!(difference("aa", "aaa"), Some(patch(2..2, "a")));
        assert_eq!(difference("aaa", "aa"), Some(patch(2..3, "")));
    }

    #[test]
    fn the_difference_keeps_whole_characters() {
        // "é" is C3 A9 and "è" is C3 A8: they share a first byte.
        assert_eq!(difference("é", "è"), Some(patch(0..2, "è")));
        // "€" is E2 82 AC and "₭" is E2 82 AD.
        assert_eq!(difference("a€b", "a₭b"), Some(patch(1..4, "₭")));
        // A shared last byte: "¡" is C2 A1 and "á" is C3 A1.
        assert_eq!(difference("x¡", "xá"), Some(patch(1..3, "á")));
    }

    fn batch(patches: &[TextPatch]) -> EditBatch {
        EditBatch(patches.to_vec())
    }

    #[test]
    fn the_untouched_ends_are_the_narrowest_any_batch_leaves() {
        // On ten bytes: "ab" inserted at 5 leaves 5 and 5; then, on twelve, 7..8 removed
        // leaves 7 and 4.
        let batches = [batch(&[patch(5..5, "ab")]), batch(&[patch(7..8, "")])];
        assert_eq!(untouched(&batches, 10), Untouched { head: 5, tail: 4 });
        assert_eq!(untouched(&[batch(&[patch(8..9, "x")])], 10), Untouched { head: 8, tail: 1 });
        // One action at two places, from the highest offset down, both in the text before it.
        let one_action = [batch(&[patch(8..9, "x"), patch(2..3, "")])];
        assert_eq!(untouched(&one_action, 10), Untouched { head: 2, tail: 1 });
        // A batch that empties the text leaves nothing at either end.
        let batches = [batch(&[patch(0..10, "")]), batch(&[patch(0..0, "z")])];
        assert_eq!(untouched(&batches, 10), Untouched { head: 0, tail: 0 });
    }

    /// A text that records where the difference starts reading and what it copies.
    struct Recording<'a> {
        text: &'a str,
        starts: RefCell<Vec<Read>>,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Read {
        From(usize),
        Before(usize),
        Copy(Range<usize>),
    }

    impl<'a> Recording<'a> {
        fn new(text: &'a str) -> Self {
            Self { text, starts: RefCell::new(Vec::new()) }
        }

        fn reads(&self) -> Vec<Read> {
            self.starts.take()
        }
    }

    impl ByteText for Recording<'_> {
        fn byte_len(&self) -> usize {
            self.text.byte_len()
        }

        fn is_char_boundary(&self, at: usize) -> bool {
            self.text.is_char_boundary(at)
        }

        fn bytes_from(&self, at: usize) -> impl Iterator<Item = u8> {
            self.starts.borrow_mut().push(Read::From(at));
            self.text.bytes_from(at)
        }

        fn bytes_before(&self, at: usize) -> impl Iterator<Item = u8> {
            self.starts.borrow_mut().push(Read::Before(at));
            self.text.bytes_before(at)
        }

        fn text(&self, range: Range<usize>) -> String {
            self.starts.borrow_mut().push(Read::Copy(range.clone()));
            ByteText::text(self.text, range)
        }
    }

    #[test]
    fn a_batched_difference_reads_from_the_untouched_ends_and_copies_only_new_text() {
        // "0123456789", "ab" inserted at 5, then the "5" after it removed: "01234ab6789".
        // The edits leave 5 bytes at the head and 4 at the tail.
        let core = Recording::new("0123456789");
        let editor = Recording::new("01234ab6789");
        let batches = [batch(&[patch(5..5, "ab")]), batch(&[patch(7..8, "")])];
        let joined = batched_difference(&batches, &core, &editor);
        assert_eq!(joined, Some(patch(5..6, "ab")));
        assert_eq!(joined, difference("0123456789", "01234ab6789"));
        assert_eq!(core.reads(), [Read::From(5), Read::Before(6)]);
        assert_eq!(editor.reads(), [Read::From(5), Read::Before(7), Read::Copy(5..7)]);
    }

    #[test]
    fn edits_at_both_ends_copy_the_patch_text_once_and_the_core_never() {
        // "x" typed at the start and "y" at the end: the one patch spans the whole text, so
        // its new text is a copy of the editor's, made once; the core's is only read.
        let middle = "m".repeat(1000);
        let before = format!("a{middle}z");
        let after = format!("xa{middle}zy");
        let core = Recording::new(&before);
        let editor = Recording::new(&after);
        let batches = [batch(&[patch(0..0, "x")]), batch(&[patch(1003..1003, "y")])];
        let joined = batched_difference(&batches, &core, &editor);
        assert_eq!(joined, Some(patch(0..1002, &format!("xa{middle}zy"))));
        let copies: Vec<_> =
            editor.reads().into_iter().filter(|read| matches!(read, Read::Copy(_))).collect();
        assert_eq!(copies, [Read::Copy(0..1004)]);
        assert!(core.reads().iter().all(|read| !matches!(read, Read::Copy(_))));
    }

    #[test]
    fn a_batched_difference_of_edits_that_cancel_is_none() {
        // "x" typed at 3, then removed.
        let batches = [batch(&[patch(3..3, "x")]), batch(&[patch(3..4, "")])];
        assert_eq!(batched_difference(&batches, "abcdef", "abcdef"), None);
    }

    #[test]
    fn both_ropes_read_as_their_text_does() {
        // "€" (E2 82 AC) becomes "₭" (E2 82 AD), then "!" is typed at the end. The shared
        // E2 82 is not a whole character, so the patch starts at "₭".
        let batches = [batch(&[patch(1..4, "₭")]), batch(&[patch(5..5, "!")])];
        let expected = Some(patch(1..5, "₭b!"));
        assert_eq!(batched_difference(&batches, "a€b", "a₭b!"), expected);
        let core = ropey::Rope::from_str("a€b");
        let editor = gpui_kit::component::input::Rope::from("a₭b!");
        assert_eq!(batched_difference(&batches, &core.slice(..), &editor), expected);
    }

    #[test]
    fn the_last_patch_end_counts_the_patches_before_it() {
        assert_eq!(end_of_last(&[]), None);
        assert_eq!(end_of_last(&[patch(4..5, "")]), Some(4));
        assert_eq!(end_of_last(&[patch(4..4, "x")]), Some(5));
        // Three cursors' "x" removed: 0..1, 3..4, 6..7 in the old text.
        let undo = [patch(0..1, ""), patch(3..4, ""), patch(6..7, "")];
        assert_eq!(end_of_last(&undo), Some(4));
    }
}
