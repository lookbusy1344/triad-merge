//! Test inputs: a seeded generator for large base/left/right triples, for benchmarks and
//! tests, and builders for small texts and documents.
#![expect(
    clippy::expect_used,
    reason = "test support: a failed setup fails the test, named by its message"
)]

use crate::document::{Labels, MergeDocument, MergeOptions};
use crate::merge3::Inputs;
use crate::text::Text;

/// Lines in each generated block. Each block gets one kind of change.
const BLOCK_LINES: usize = 20;
/// One block in this many is changed; the rest stay unchanged.
const CHANGE_EVERY: u64 = 4;
/// Number of edit kinds that a changed block rotates through.
const EDIT_KINDS: u64 = 5;

/// `SplitMix64`: small, fast and deterministic. Enough for fixtures.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

pub struct Triple {
    pub base: String,
    pub left: String,
    pub right: String,
}

/// Generate a triple with `lines` base lines. Changed blocks rotate through
/// left-only, right-only, both-same, conflict and right-insertion edits.
#[must_use]
pub fn generate(lines: usize, seed: u64) -> Triple {
    let mut rng = SplitMix64(seed);
    let mut triple = Triple { base: String::new(), left: String::new(), right: String::new() };
    for block_start in (0..lines).step_by(BLOCK_LINES) {
        let block_end = (block_start + BLOCK_LINES).min(lines);
        let edit = rng.next().is_multiple_of(CHANGE_EVERY).then(|| rng.next() % EDIT_KINDS);
        for n in block_start..block_end {
            let line = base_line(n);
            let edited = n == block_start + BLOCK_LINES / 2;
            triple.base.push_str(&line);
            let (left, right) = match (edit, edited) {
                (Some(0), true) => (format!("{n:>7}  // left edit\n"), line.clone()),
                (Some(1), true) => (line.clone(), format!("{n:>7}  // right edit\n")),
                (Some(2), true) => {
                    let same = format!("{n:>7}  // same edit\n");
                    (same.clone(), same)
                }
                (Some(3), true) => {
                    (format!("{n:>7}  // left conflict\n"), format!("{n:>7}  // right conflict\n"))
                }
                (Some(4), true) => (
                    line.clone(),
                    format!(
                        "{line}{n:>7}  // right insertion one\n{n:>7}  // right insertion two\n"
                    ),
                ),
                _ => (line.clone(), line),
            };
            triple.left.push_str(&left);
            triple.right.push_str(&right);
        }
    }
    triple
}

/// Generate a triple with `lines` base lines, of which every `spacing`th, from the first,
/// conflicts line by line and resolves word by word: left renames the function, right turns
/// its `*` into `+`. A `spacing` of 1 makes the whole file one conflict.
#[must_use]
pub fn generate_word_conflicts(lines: usize, spacing: usize) -> Triple {
    let mut triple = Triple { base: String::new(), left: String::new(), right: String::new() };
    for n in 0..lines {
        let line = base_line(n);
        if n.is_multiple_of(spacing) {
            triple.left.push_str(&line.replacen("fn item_", "fn left_", 1));
            triple.right.push_str(&line.replacen(" * ", " + ", 1));
        } else {
            triple.left.push_str(&line);
            triple.right.push_str(&line);
        }
        triple.base.push_str(&line);
    }
    triple
}

fn base_line(n: usize) -> String {
    format!("{n:>7}  fn item_{n}(value: u64) -> u64 {{ value * {n} }}\n")
}

/// `s` as a text. Test input is UTF-8 and always decodes.
///
/// # Panics
/// Never: UTF-8 always decodes.
#[must_use]
pub fn text(s: &str) -> Text {
    Text::decode(s.as_bytes()).expect("UTF-8 decodes")
}

/// Three-way inputs from `base`, `left` and `right`.
#[must_use]
pub fn three(base: &str, left: &str, right: &str) -> Inputs {
    Inputs::three_way(text(base), text(left), text(right))
}

/// The three-way merge of `base`, `left` and `right` under `options`, without labels.
///
/// # Panics
/// Never: no labels fit any number of sides.
#[must_use]
pub fn doc_with(base: &str, left: &str, right: &str, options: MergeOptions) -> MergeDocument {
    MergeDocument::new(three(base, left, right), options, Labels::none()).expect("no labels fit")
}

/// The three-way merge of `base`, `left` and `right` under the default options.
#[must_use]
pub fn doc(base: &str, left: &str, right: &str) -> MergeDocument {
    doc_with(base, left, right, MergeOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Output;
    use crate::merge3::{HunkLevel, Kind, Semantics};

    fn jj_doc(triple: &Triple, hunk_level: HunkLevel) -> MergeDocument {
        let options =
            MergeOptions { semantics: Semantics::Jj, hunk_level, ..MergeOptions::default() };
        let inputs = three(&triple.base, &triple.left, &triple.right);
        MergeDocument::new(inputs, options, Labels::three_way("l", "b", "r")).unwrap()
    }

    fn kinds(doc: &MergeDocument) -> Vec<Kind> {
        doc.regions().iter().map(|r| r.kind).collect()
    }

    #[test]
    fn same_seed_gives_same_triple() {
        let (a, b) = (generate(500, 7), generate(500, 7));
        assert_eq!((a.base, a.left, a.right), (b.base, b.left, b.right));
    }

    #[test]
    fn different_seeds_change_different_blocks() {
        let (a, b) = (generate(2_000, 1), generate(2_000, 2));
        assert_eq!(a.base, b.base);
        assert_ne!((a.left, a.right), (b.left, b.right));
    }

    #[test]
    fn base_has_the_requested_line_count_with_a_partial_last_block() {
        assert_eq!(generate(1_234, 1).base.lines().count(), 1_234);
        assert_eq!(generate(25, 1).base.lines().count(), 25);
    }

    #[test]
    fn zero_lines_gives_empty_inputs() {
        let triple = generate(0, 3);
        assert_eq!(
            (triple.base, triple.left, triple.right),
            (String::new(), String::new(), String::new())
        );
    }

    #[test]
    fn base_lines_are_numbered_items() {
        let base = generate(3, 9).base;
        assert_eq!(
            base,
            "      0  fn item_0(value: u64) -> u64 { value * 0 }\n\
             \x20     1  fn item_1(value: u64) -> u64 { value * 1 }\n\
             \x20     2  fn item_2(value: u64) -> u64 { value * 2 }\n"
        );
    }

    #[test]
    fn word_conflicts_edit_every_spacingth_line_from_the_first() {
        let triple = generate_word_conflicts(3, 2);
        assert_eq!(
            triple.base,
            "      0  fn item_0(value: u64) -> u64 { value * 0 }\n\
             \x20     1  fn item_1(value: u64) -> u64 { value * 1 }\n\
             \x20     2  fn item_2(value: u64) -> u64 { value * 2 }\n"
        );
        assert_eq!(
            triple.left,
            "      0  fn left_0(value: u64) -> u64 { value * 0 }\n\
             \x20     1  fn item_1(value: u64) -> u64 { value * 1 }\n\
             \x20     2  fn left_2(value: u64) -> u64 { value * 2 }\n"
        );
        assert_eq!(
            triple.right,
            "      0  fn item_0(value: u64) -> u64 { value + 0 }\n\
             \x20     1  fn item_1(value: u64) -> u64 { value * 1 }\n\
             \x20     2  fn item_2(value: u64) -> u64 { value + 2 }\n"
        );
    }

    #[test]
    fn word_conflicts_with_zero_lines_are_empty() {
        let triple = generate_word_conflicts(0, 1);
        assert_eq!(
            (triple.base, triple.left, triple.right),
            (String::new(), String::new(), String::new())
        );
    }

    #[test]
    fn spaced_word_conflicts_are_separate_conflicts_that_all_resolve_by_word() {
        // Lines 0, 20 and 40 of 50 conflict; the unchanged lines between keep them apart.
        let triple = generate_word_conflicts(50, 20);
        let conflicts =
            |doc: &MergeDocument, kind| kinds(doc).iter().filter(|k| **k == kind).count();
        let line = jj_doc(&triple, HunkLevel::Line);
        assert_eq!(conflicts(&line, Kind::Conflict), 3);
        let word = jj_doc(&triple, HunkLevel::Word);
        assert_eq!((conflicts(&word, Kind::Conflict), conflicts(&word, Kind::WordMerged)), (0, 3));
    }

    #[test]
    fn word_conflicts_with_spacing_one_are_one_conflict_that_resolves_by_word() {
        let triple = generate_word_conflicts(2, 1);
        assert_eq!(kinds(&jj_doc(&triple, HunkLevel::Line)), [Kind::Conflict]);
        let word = jj_doc(&triple, HunkLevel::Word);
        assert_eq!(kinds(&word), [Kind::WordMerged]);
        assert_eq!(
            word.output(),
            Output::Resolved(
                "      0  fn left_0(value: u64) -> u64 { value + 0 }\n\
                 \x20     1  fn left_1(value: u64) -> u64 { value + 1 }\n"
                    .to_owned()
            )
        );
    }
}
