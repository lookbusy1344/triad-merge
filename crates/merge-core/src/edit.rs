//! Text patches and edit batches, shared by the merge result and the two-way diff buffers.

use std::ops::Range;

use ropey::Rope;
use serde::{Deserialize, Serialize};

use crate::text::LineEnding;

/// Replace `range` of a text, in byte offsets before the change, with `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPatch {
    pub range: Range<usize>,
    pub text: String,
}

/// One user action's patches: sorted, non-overlapping, with offsets in the text before any of
/// them. Insertions at the same offset land in batch order.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EditBatch(pub Vec<TextPatch>);

/// A batch that does not fit the text it edits.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BatchError {
    #[error("edit {index} starts before the edit ahead of it ends")]
    OutOfOrder { index: usize },
    #[error("edit range {range:?} does not fit a text of {len} bytes")]
    BadRange { range: Range<usize>, len: usize },
    #[error("byte {offset} is inside a character")]
    NotCharBoundary { offset: usize },
}

/// A checked batch with its line breaks given the typed ending (spec §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prepared {
    /// The batch as the text must hold it, with offsets in the text before the user's edit.
    pub patches: Vec<TextPatch>,
    /// The fixes to the text the user's edit left, with offsets in that text.
    pub corrections: Vec<TextPatch>,
    /// One insertion without a newline, which may join the undo step before it.
    pub typing: bool,
}

/// Check `batch` against `text`, drop patches that change nothing, and give inserted line
/// breaks the `ending`. `None` when nothing is left.
pub(crate) fn prepare(
    text: &Rope,
    EditBatch(patches): EditBatch,
    ending: LineEnding,
) -> Result<Option<Prepared>, BatchError> {
    check(text, &patches)?;
    let user: Vec<TextPatch> =
        patches.into_iter().filter(|p| !p.range.is_empty() || !p.text.is_empty()).collect();
    if user.is_empty() {
        return Ok(None);
    }
    let starts = starts_after(user.iter().map(|p| (p.range.clone(), p.text.len())));
    let corrections = user
        .iter()
        .zip(starts)
        .flat_map(|(p, start)| {
            ending_fixes(&p.text, ending).into_iter().map(move |fix| TextPatch {
                range: start + fix.range.start..start + fix.range.end,
                text: fix.text,
            })
        })
        .collect();
    let patches: Vec<TextPatch> = user
        .into_iter()
        .map(|TextPatch { range, text }| {
            let fixes = ending_fixes(&text, ending);
            let text = if fixes.is_empty() { text } else { patched(&text, &fixes) };
            TextPatch { range, text }
        })
        .collect();
    let typing = match patches.as_slice() {
        [p] => p.range.is_empty() && !p.text.contains('\n'),
        _ => false,
    };
    Ok(Some(Prepared { patches, corrections, typing }))
}

fn check(text: &Rope, patches: &[TextPatch]) -> Result<(), BatchError> {
    let len = text.len_bytes();
    let is_char_boundary = |offset| text.char_to_byte(text.byte_to_char(offset)) == offset;
    patches
        .iter()
        .enumerate()
        .try_fold(0, |previous_end, (index, p)| {
            let range = p.range.clone();
            if range.start > range.end || range.end > len {
                Err(BatchError::BadRange { range, len })
            } else if range.start < previous_end {
                Err(BatchError::OutOfOrder { index })
            } else if let Some(offset) =
                [range.start, range.end].into_iter().find(|&o| !is_char_boundary(o))
            {
                Err(BatchError::NotCharBoundary { offset })
            } else {
                Ok(range.end)
            }
        })
        .map(drop)
}

/// Make sorted `patches`, with offsets in `text` before any of them.
pub(crate) fn apply(text: &mut Rope, patches: &[TextPatch]) {
    for p in patches.iter().rev() {
        let start = text.byte_to_char(p.range.start);
        let end = text.byte_to_char(p.range.end);
        text.remove(start..end);
        text.insert(start, &p.text);
    }
}

/// The patches that take the text after sorted `patches` back to `text`.
pub(crate) fn inverse(patches: &[TextPatch], text: &Rope) -> Vec<TextPatch> {
    let starts = starts_after(patches.iter().map(|p| (p.range.clone(), p.text.len())));
    patches
        .iter()
        .zip(starts)
        .map(|(p, start)| TextPatch {
            range: start..start + p.text.len(),
            text: text.byte_slice(p.range.clone()).to_string(),
        })
        .collect()
}

/// When `undo` only removes one insertion, the insertion's end: where typing would go on.
pub(crate) fn caret_after(undo: &[TextPatch]) -> Option<usize> {
    match undo {
        [p] if p.text.is_empty() && !p.range.is_empty() => Some(p.range.end),
        _ => None,
    }
}

/// Where each of a sorted run of replacements starts once all are made. Each replaces `range`
/// with `len` units.
pub(crate) fn starts_after(
    replacements: impl IntoIterator<Item = (Range<usize>, usize)>,
) -> Vec<usize> {
    replacements
        .into_iter()
        .scan((0, 0), |(old_end, new_end), (range, len)| {
            let start = *new_end + (range.start - *old_end);
            (*old_end, *new_end) = (range.end, start + len);
            Some(start)
        })
        .collect()
}

/// Patches, with offsets in `text`, that give each line break in it the `ending`. A lone `\r`
/// is not a line break.
fn ending_fixes(text: &str, ending: LineEnding) -> Vec<TextPatch> {
    text.match_indices('\n')
        .filter_map(|(i, _)| {
            let crlf = text[..i].ends_with('\r');
            match (ending, crlf) {
                (LineEnding::CrLf, false) => Some(TextPatch { range: i..i, text: "\r".to_owned() }),
                (LineEnding::Lf, true) => Some(TextPatch { range: i - 1..i, text: String::new() }),
                _ => None,
            }
        })
        .collect()
}

/// `text` with sorted, non-overlapping `patches` applied.
fn patched(text: &str, patches: &[TextPatch]) -> String {
    let mut out = String::with_capacity(text.len() + patches.len());
    let end = patches.iter().fold(0, |at, p| {
        out.push_str(&text[at..p.range.start]);
        out.push_str(&p.text);
        p.range.end
    });
    out.push_str(&text[end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(range: Range<usize>, text: &str) -> TextPatch {
        TextPatch { range, text: text.to_owned() }
    }

    #[test]
    fn the_caret_follows_only_an_undo_that_removes_one_insertion() {
        assert_eq!(caret_after(&[patch(2..5, "")]), Some(5), "removes an insertion");
        assert_eq!(caret_after(&[patch(2..5, "x")]), None, "restores replaced text");
        assert_eq!(caret_after(&[patch(2..2, "x")]), None, "restores deleted text");
        assert_eq!(caret_after(&[patch(2..2, "")]), None, "changes nothing");
        assert_eq!(caret_after(&[patch(0..1, ""), patch(3..4, "")]), None, "two patches");
        assert_eq!(caret_after(&[]), None, "no patches");
    }
}
