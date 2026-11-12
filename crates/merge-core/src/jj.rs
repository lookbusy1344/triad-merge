//! jj's line diff over a merge's inputs, in line coordinates (`DESIGN.md` §B2).
//!
//! `jj_core::diff::ContentDiff` diffs base 0 against every other input in one pass. A hunk is
//! `Matching` where every input agrees and `Different` elsewhere. jj orders the inputs as every
//! base, then every side; this module hands them back as bases and sides.
//!
//! [`classify`] resolves a hunk with jj's `trivial_merge`, as jj-lib's `files.rs` does in
//! `resolve_diff_hunks` (Apache-2.0, `jj-lib` 0.46).

use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use jj_core::conflicts::choose_materialized_conflict_marker_len;
use jj_core::diff::{CompareBytes, ContentDiff, DiffHunkKind, find_line_ranges};
use jj_core::merge::{Merge, SameChange as JjSameChange};

use crate::diff::{Comparison, line_key};
use crate::merge3::{Inputs, Kind, SameChange};
use crate::text::Text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkKind {
    /// Every input holds the same lines.
    Matching,
    /// At least one input differs from the others.
    Different,
}

/// One hunk: a line range in each base and each side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub kind: HunkKind,
    pub bases: Vec<Range<usize>>,
    pub sides: Vec<Range<usize>>,
}

/// jj's line diff of a merge's inputs. It borrows their texts.
pub struct LineDiff<'a> {
    inputs: &'a Inputs,
    diff: ContentDiff<'a>,
}

impl<'a> LineDiff<'a> {
    /// Lines compare by `comparison` as in git semantics: by [`line_key`].
    #[must_use]
    pub fn new(inputs: &'a Inputs, comparison: Comparison) -> Self {
        let texts = || inputs.bases().iter().chain(inputs.sides()).map(Text::content);
        let diff = if comparison.is_exact() {
            ContentDiff::by_line(texts())
        } else {
            ContentDiff::for_tokenizer(texts(), find_line_ranges, LineKeys(comparison))
        };
        Self { inputs, diff }
    }

    /// The hunks in order. They cover every input without gaps or overlaps.
    pub fn hunks(&self) -> impl Iterator<Item = Hunk> + '_ {
        let bases = self.inputs.bases();
        let sides = self.inputs.sides();
        self.diff.hunk_ranges().map(move |hunk| {
            let (base_bytes, side_bytes) = hunk.ranges.split_at(bases.len());
            Hunk {
                kind: match hunk.kind {
                    DiffHunkKind::Matching => HunkKind::Matching,
                    DiffHunkKind::Different => HunkKind::Different,
                },
                bases: bases
                    .iter()
                    .zip(base_bytes)
                    .map(|(text, bytes)| text.lines_at(bytes))
                    .collect(),
                sides: sides
                    .iter()
                    .zip(side_bytes)
                    .map(|(text, bytes)| text.lines_at(bytes))
                    .collect(),
            }
        })
    }
}

/// Compares lines by their [`line_key`] under a [`Comparison`].
struct LineKeys(Comparison);

impl LineKeys {
    #[expect(clippy::expect_used, reason = "invariant: jj-lib splits a `str` into lines at `\\n`")]
    fn key<'t>(&self, line: &'t [u8]) -> Cow<'t, str> {
        let line = std::str::from_utf8(line).expect("a line of a `str` is UTF-8: it ends at `\\n`");
        line_key(line, self.0)
    }
}

impl CompareBytes for LineKeys {
    fn eq(&self, left: &[u8], right: &[u8]) -> bool {
        self.key(left) == self.key(right)
    }

    fn hash<H: Hasher>(&self, text: &[u8], state: &mut H) {
        self.key(text).hash(state);
    }
}

/// The texts of one line hunk merged word by word, as jj-lib's `merge_hunk_by_word` does
/// (`files.rs`): jj's word diff, each `Different` word hunk resolved trivially. None unless
/// every word hunk resolves; jj then keeps the line hunk whole. With `clean`, also none
/// when a resolved word hunk starts or ends where `clean(side, offset)` is false, in a side
/// that holds its text; the offset counts from the start of that side's text.
pub(crate) fn merge_words(
    bases: &[&str],
    sides: &[&str],
    same_change: SameChange,
    clean: Option<&dyn Fn(usize, usize) -> bool>,
) -> Option<String> {
    let same_change = jj_same_change(same_change);
    let diff = ContentDiff::by_word(bases.iter().chain(sides));
    let mut merged = String::with_capacity(sides.first().map_or(0, |side| side.len()));
    // Each input's offset at the hunk's start.
    let mut at = vec![0; bases.len() + sides.len()];
    for hunk in diff.hunks() {
        let text = match hunk.kind {
            DiffHunkKind::Matching => hunk.contents[0],
            DiffHunkKind::Different => {
                let (removes, adds) = hunk.contents.split_at(bases.len());
                let resolved =
                    *Merge::from_removes_adds(removes.iter().copied(), adds.iter().copied())
                        .resolve_trivial(same_change)?;
                if let Some(clean) = clean {
                    let starts = &at[bases.len()..];
                    let cut_clean =
                        adds.iter().zip(starts).enumerate().all(|(side, (add, &start))| {
                            *add != resolved || clean(side, start) && clean(side, start + add.len())
                        });
                    if !cut_clean {
                        return None;
                    }
                }
                resolved
            }
        };
        for (offset, content) in at.iter_mut().zip(&hunk.contents) {
            *offset += content.len();
        }
        // The word tokenizer splits at ASCII bytes only, so each piece of a `str` is UTF-8.
        #[expect(
            clippy::expect_used,
            reason = "invariant: the word tokenizer splits a `str` at ASCII bytes"
        )]
        merged.push_str(std::str::from_utf8(text).expect("a word hunk of a `str` is UTF-8"));
    }
    Some(merged)
}

fn jj_same_change(same_change: SameChange) -> JjSameChange {
    match same_change {
        SameChange::Accept => JjSameChange::Accept,
        SameChange::Keep => JjSameChange::Keep,
    }
}

/// jj's trivial resolution of a `Different` hunk, given each input's text or comparison keys.
/// [`Kind::AllSame`] when it resolves to a text every side holds, else [`Kind::OneSide`] with
/// the lowest side holding it, else [`Kind::Conflict`].
#[expect(clippy::expect_used, reason = "invariant: a trivial merge resolves to a side's term")]
pub(crate) fn classify<T: Eq + Hash>(sides: &[T], bases: &[T], same_change: SameChange) -> Kind {
    let merge = Merge::from_removes_adds(bases, sides);
    let same_change = jj_same_change(same_change);
    let Some(&resolved) = merge.resolve_trivial(same_change) else {
        return Kind::Conflict;
    };
    if sides.iter().all(|side| side == resolved) {
        Kind::AllSame
    } else {
        // trivial_merge keeps a text with more sides than bases, so a side holds it.
        Kind::OneSide(
            sides.iter().position(|side| side == resolved).expect("jj resolves to a side's text"),
        )
    }
}

/// The conflict marker length jj's materialiser writes a conflict of `inputs` with, and the
/// shortest markers it parses back: four past the longest marker-like line of any input, and
/// at least seven (spec §4.4).
#[must_use]
pub fn marker_length(inputs: &Inputs) -> usize {
    let merge = Merge::from_removes_adds(
        inputs.bases().iter().map(Text::content),
        inputs.sides().iter().map(Text::content),
    );
    choose_materialized_conflict_marker_len(&merge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{text, three};

    /// jj's marker length for a conflict of `sides` over `base`.
    fn jj_markers([left, right]: [&str; 2], base: &str) -> usize {
        marker_length(&three(base, left, right))
    }

    #[test]
    fn jj_writes_markers_of_seven_or_four_past_the_longest_marker_like_line() {
        assert_eq!(jj_markers(["left\n", "right\n"], "a\n"), 7);
        // A run of 3 gives 3 + 4, no more than the least length of 7.
        assert_eq!(jj_markers(["=== left\n", "right\n"], "a\n"), 7);
        // A run of 4 gives 4 + 4.
        assert_eq!(jj_markers(["left\n", "<<<<\n"], "a\n"), 8);
        // A run of 7 gives 7 + 4.
        assert_eq!(jj_markers(["<<<<<<< left\n", "right\n"], "a\n"), 11);
        // A run of 9 in the base counts too.
        assert_eq!(jj_markers(["left\n", "right\n"], ">>>>>>>>> a\n"), 13);
        // A run followed by other than whitespace is no marker.
        assert_eq!(jj_markers(["<<<<<<<<<x\n", "right\n"], "a\n"), 7);
    }

    #[test]
    fn jj_counts_snapshot_markers_in_any_style() {
        // `+`, `-`, `%` and `\` runs are markers to jj in every style (spec §4.4).
        assert_eq!(jj_markers(["+++++ a\n", "right\n"], "a\n"), 9);
        assert_eq!(jj_markers(["left\n", "%%%%%%%%\n"], "a\n"), 12);
    }

    fn hunks(base: &str, left: &str, right: &str) -> Vec<Hunk> {
        let inputs = three(base, left, right);
        LineDiff::new(&inputs, Comparison::default()).hunks().collect()
    }

    fn hunk(kind: HunkKind, base: Range<usize>, left: Range<usize>, right: Range<usize>) -> Hunk {
        Hunk { kind, bases: vec![base], sides: vec![left, right] }
    }

    fn same(base: Range<usize>, left: Range<usize>, right: Range<usize>) -> Hunk {
        hunk(HunkKind::Matching, base, left, right)
    }

    fn differ(base: Range<usize>, left: Range<usize>, right: Range<usize>) -> Hunk {
        hunk(HunkKind::Different, base, left, right)
    }

    #[test]
    fn identical_inputs_are_one_matching_hunk() {
        assert_eq!(hunks("a\nb\nc\n", "a\nb\nc\n", "a\nb\nc\n"), [same(0..3, 0..3, 0..3)]);
    }

    #[test]
    fn three_empty_inputs_have_no_hunks() {
        assert_eq!(hunks("", "", ""), []);
    }

    #[test]
    fn a_change_on_both_sides_is_one_different_hunk() {
        assert_eq!(
            hunks("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n"),
            [same(0..1, 0..1, 0..1), differ(1..2, 1..2, 1..2), same(2..3, 2..3, 2..3)]
        );
    }

    #[test]
    fn a_change_on_one_side_still_spans_every_input() {
        assert_eq!(
            hunks("a\nb\nc\n", "a\nB\nB\nc\n", "a\nb\nc\n"),
            [same(0..1, 0..1, 0..1), differ(1..2, 1..3, 1..2), same(2..3, 3..4, 2..3)]
        );
    }

    #[test]
    fn insertion_is_an_empty_base_range() {
        assert_eq!(
            hunks("a\nb\n", "a\nx\nb\n", "a\nb\n"),
            [same(0..1, 0..1, 0..1), differ(1..1, 1..2, 1..1), same(1..2, 2..3, 1..2)]
        );
    }

    #[test]
    fn a_missing_final_newline_makes_the_last_line_differ() {
        assert_eq!(
            hunks("a\nb", "a\nb\n", "a\nb"),
            [same(0..1, 0..1, 0..1), differ(1..2, 1..2, 1..2)]
        );
    }

    #[test]
    fn crlf_and_lf_lines_differ() {
        assert_eq!(
            hunks("a\r\nb\n", "a\nb\n", "a\r\nb\n"),
            [differ(0..1, 0..1, 0..1), same(1..2, 1..2, 1..2)]
        );
    }

    #[test]
    fn a_lone_cr_does_not_end_a_line() {
        assert_eq!(
            hunks("a\rb\nc\n", "a\rB\nc\n", "a\rb\nc\n"),
            [differ(0..1, 0..1, 0..1), same(1..2, 1..2, 1..2)]
        );
    }

    #[test]
    fn an_empty_base_makes_the_whole_file_one_hunk() {
        // jj tokenizes nothing when any input is empty, so the shared line is not matched.
        assert_eq!(hunks("", "a\nb\n", "a\nc\n"), [differ(0..0, 0..2, 0..2)]);
    }

    #[test]
    fn an_empty_side_makes_the_whole_file_one_hunk() {
        assert_eq!(hunks("a\nb\n", "", "a\nb\n"), [differ(0..2, 0..0, 0..2)]);
    }

    fn repeated(line: &str, count: usize) -> String {
        line.repeat(count)
    }

    #[test]
    fn lines_occurring_up_to_100_times_anchor() {
        // Every base line occurs 100 times, the limit, and as often in left: all anchor.
        let base = repeated("x\n", 100) + &repeated("y\n", 100);
        let left = format!("z\n{base}z\n");
        assert_eq!(
            hunks(&base, &left, &base),
            [
                differ(0..0, 0..1, 0..0),
                same(0..200, 1..201, 0..200),
                differ(200..200, 201..202, 200..200),
            ]
        );
    }

    #[test]
    fn lines_occurring_over_100_times_do_not_anchor() {
        // 101 occurrences: jj gives up anchoring, and no common prefix or suffix remains.
        let base = repeated("x\n", 101) + &repeated("y\n", 101);
        let left = format!("z\n{base}z\n");
        assert_eq!(hunks(&base, &left, &base), [differ(0..202, 0..204, 0..202)]);
    }

    #[test]
    fn n_way_hunks_return_bases_and_sides_in_their_own_order() {
        let inputs = Inputs::new(
            vec![text("a\nX\nY\n"), text("a\nb\n"), text("a\nb\n")],
            vec![text("a\nb\n"), text("a\nb\n")],
        )
        .unwrap();
        let mixed = |kind, bases: [Range<usize>; 2], sides: [Range<usize>; 3]| Hunk {
            kind,
            bases: Vec::from(bases),
            sides: Vec::from(sides),
        };
        assert_eq!(
            LineDiff::new(&inputs, Comparison::default()).hunks().collect::<Vec<_>>(),
            [
                mixed(HunkKind::Matching, [0..1, 0..1], [0..1, 0..1, 0..1]),
                mixed(HunkKind::Different, [1..2, 1..2], [1..3, 1..2, 1..2]),
            ]
        );
    }
}
