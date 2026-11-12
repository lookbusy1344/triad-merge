//! Caret placement after the core's patches reach the editor. Patches are sorted and
//! disjoint, with offsets in the text before any of them.

use merge_core::document::TextPatch;

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

/// The one patch that turns `before` into `after`: everything between their common prefix
/// and common suffix. `None` when they are equal.
pub(crate) fn difference(before: &str, after: &str) -> Option<TextPatch> {
    let common = |a: &mut dyn Iterator<Item = (u8, u8)>| a.take_while(|(x, y)| x == y).count();
    let mut prefix = common(&mut before.bytes().zip(after.bytes()));
    while !before.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let room = before.len().min(after.len()) - prefix;
    let mut suffix = common(&mut before.bytes().rev().zip(after.bytes().rev())).min(room);
    while !before.is_char_boundary(before.len() - suffix)
        || !after.is_char_boundary(after.len() - suffix)
    {
        suffix -= 1;
    }
    let range = prefix..before.len() - suffix;
    let text = &after[prefix..after.len() - suffix];
    (!range.is_empty() || !text.is_empty()).then(|| TextPatch { range, text: text.to_owned() })
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
    use std::ops::Range;

    use super::*;

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
