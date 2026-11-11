//! Line and character diffs over decoded text (spec §4.2).
//!
//! Whitespace modes change only the key a line is interned under, so they change which lines
//! compare equal and never the bytes. Whitespace means space, tab and `\r`, as in git's
//! `isspace`; `\n` ends the line and is never part of a key except under [`Whitespace::None`]
//! with [`LineEndingRule::Compare`].

use std::borrow::Cow;
use std::ops::Range;

use imara_diff::{Diff, InternedInput};
use serde::{Deserialize, Serialize};

use crate::histogram;
use crate::text::Text;

/// Lines with more characters than this get no character-level diff; the view highlights
/// the whole line instead (spec §5).
pub const MAX_INTRALINE_DIFF_CHARS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Algorithm {
    #[default]
    Histogram,
    Myers,
    MyersMinimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Whitespace {
    /// Lines compare byte for byte, terminators included.
    #[default]
    None,
    /// Ignore trailing whitespace and the line terminator.
    Trailing,
    /// As `Trailing`, and treat every run of whitespace as one space.
    Amount,
    /// Ignore all whitespace.
    All,
}

/// Whether line terminators take part in comparison (spec §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineEndingRule {
    /// `\r\n`, `\n` and a missing final newline differ, unless the whitespace mode drops them.
    #[default]
    Compare,
    /// `\r\n`, `\n` and a missing final newline compare equal.
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiffOptions {
    pub algorithm: Algorithm,
    pub whitespace: Whitespace,
    /// Journals written before the field compared line endings.
    #[serde(default)]
    pub line_endings: LineEndingRule,
}

impl DiffOptions {
    /// The rule lines compare by under these options.
    #[must_use]
    pub fn comparison(self) -> Comparison {
        Comparison { whitespace: self.whitespace, line_endings: self.line_endings }
    }
}

/// How two lines compare: by their [`line_key`] under a whitespace mode and a line-ending rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Comparison {
    pub whitespace: Whitespace,
    pub line_endings: LineEndingRule,
}

impl Comparison {
    /// Lines compare byte for byte.
    #[must_use]
    pub fn is_exact(self) -> bool {
        self == Self::default()
    }
}

impl From<Whitespace> for Comparison {
    fn from(whitespace: Whitespace) -> Self {
        Self { whitespace, line_endings: LineEndingRule::Compare }
    }
}

/// One change: `before` is replaced by `after`. Line ranges from [`diff_lines`], byte ranges
/// from [`diff_chars`]. Either range may be empty, for a pure insertion or deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub before: Range<usize>,
    pub after: Range<usize>,
}

/// The changes that turn `before` into `after`, in order, as line ranges. Ambiguous hunks
/// are placed by the indent heuristic, as in git `diff`.
#[must_use]
pub fn diff_lines(before: &Text, after: &Text, opts: DiffOptions) -> Vec<Hunk> {
    diff_lines_by(before, after, opts, Sliders::IndentHeuristic)
}

/// As [`diff_lines`], placing hunks as git `merge-file` does: no indent heuristic, and
/// git's own histogram algorithm.
pub(crate) fn diff_lines_for_merge(before: &Text, after: &Text, opts: DiffOptions) -> Vec<Hunk> {
    match opts.algorithm {
        Algorithm::Histogram => {
            let input = interned(before, after, opts.comparison());
            histogram::diff(&input.before, &input.after, input.interner.num_tokens())
        }
        Algorithm::Myers | Algorithm::MyersMinimal => {
            diff_lines_by(before, after, opts, Sliders::Down)
        }
    }
}

/// The changes that turn lines `before_lines` of `before` into lines `after_lines` of
/// `after`, as line ranges counted from each range's start, by git's histogram.
pub(crate) fn diff_line_ranges(
    before: &Text,
    before_lines: Range<usize>,
    after: &Text,
    after_lines: Range<usize>,
    mode: Comparison,
) -> Vec<Hunk> {
    let mut input = InternedInput::default();
    input.update_before(before_lines.map(|n| Key(line_key(before.line(n), mode))));
    input.update_after(after_lines.map(|n| Key(line_key(after.line(n), mode))));
    histogram::diff(&input.before, &input.after, input.interner.num_tokens())
}

/// Where a hunk goes when it could slide over a run of equal lines.
#[derive(Clone, Copy)]
enum Sliders {
    IndentHeuristic,
    Down,
}

fn diff_lines_by(before: &Text, after: &Text, opts: DiffOptions, sliders: Sliders) -> Vec<Hunk> {
    let input = interned(before, after, opts.comparison());
    let mut diff = Diff::compute(opts.algorithm.into(), &input);
    match sliders {
        Sliders::IndentHeuristic => diff.postprocess_lines(&input),
        Sliders::Down => diff.postprocess_no_heuristic(&input),
    }
    diff.hunks().map(|h| Hunk { before: widen(h.before), after: widen(h.after) }).collect()
}

fn interned<'a>(before: &'a Text, after: &'a Text, mode: Comparison) -> InternedInput<Key<'a>> {
    let mut input = InternedInput::default();
    input.update_before(keys(before, mode));
    input.update_after(keys(after, mode));
    input
}

fn keys(text: &Text, mode: Comparison) -> impl Iterator<Item = Key<'_>> {
    text.lines().map(move |line| Key(line_key(line, mode)))
}

/// The character-level changes that turn `before` into `after`, as byte ranges on UTF-8
/// boundaries.
#[must_use]
pub fn diff_chars(before: &str, after: &str) -> Vec<Hunk> {
    let mut input = InternedInput::default();
    input.update_before(before.chars());
    input.update_after(after.chars());
    // Histogram does badly on small alphabets such as characters, so this is always Myers.
    let mut diff = Diff::compute(imara_diff::Algorithm::Myers, &input);
    diff.postprocess_no_heuristic(&input);
    let (before_at, after_at) = (char_offsets(before), char_offsets(after));
    diff.hunks()
        .map(|h| Hunk {
            before: before_at[widen_one(h.before.start)]..before_at[widen_one(h.before.end)],
            after: after_at[widen_one(h.after.start)]..after_at[widen_one(h.after.end)],
        })
        .collect()
}

/// [`diff_chars`] for a changed line pair, or `None` when either line is longer than
/// [`MAX_INTRALINE_DIFF_CHARS`] characters.
#[must_use]
pub fn intraline_hunks(before: &str, after: &str) -> Option<Vec<Hunk>> {
    let within = |line: &str| line.chars().count() <= MAX_INTRALINE_DIFF_CHARS;
    (within(before) && within(after)).then(|| diff_chars(before, after))
}

/// The key a line compares under by `mode`.
#[must_use]
pub fn line_key(line: &str, mode: Comparison) -> Cow<'_, str> {
    let body = || line.strip_suffix('\n').unwrap_or(line).trim_end_matches(is_blank);
    match (mode.whitespace, mode.line_endings) {
        (Whitespace::None, LineEndingRule::Compare) => Cow::Borrowed(line),
        (Whitespace::None, LineEndingRule::Ignore) => Cow::Borrowed(
            line.strip_suffix('\n').map_or(line, |l| l.strip_suffix('\r').unwrap_or(l)),
        ),
        (Whitespace::Trailing, _) => Cow::Borrowed(body()),
        (Whitespace::Amount, _) => Cow::Owned(collapse_blanks(body())),
        (Whitespace::All, _) => Cow::Owned(body().chars().filter(|&c| !is_blank(c)).collect()),
    }
}

fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r')
}

/// Replace each run of whitespace with one space.
fn collapse_blanks(text: &str) -> String {
    text.chars()
        .fold((String::with_capacity(text.len()), false), |(mut out, in_run), c| {
            let blank = is_blank(c);
            match (blank, in_run) {
                (true, true) => {}
                (true, false) => out.push(' '),
                (false, _) => out.push(c),
            }
            (out, blank)
        })
        .0
}

/// A line's comparison key. `postprocess_lines` reads keys as bytes for its indent heuristic.
#[derive(Default, PartialEq, Eq, Hash)]
struct Key<'a>(Cow<'a, str>);

impl AsRef<[u8]> for Key<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl From<Algorithm> for imara_diff::Algorithm {
    fn from(algorithm: Algorithm) -> Self {
        match algorithm {
            Algorithm::Histogram => imara_diff::Algorithm::Histogram,
            Algorithm::Myers => imara_diff::Algorithm::Myers,
            Algorithm::MyersMinimal => imara_diff::Algorithm::MyersMinimal,
        }
    }
}

/// Byte offset of each character, then the string's length, indexed by character position.
fn char_offsets(text: &str) -> Vec<usize> {
    text.char_indices().map(|(at, _)| at).chain(std::iter::once(text.len())).collect()
}

// Every supported target has a `usize` of at least 32 bits, so `widen_one` loses nothing.
const _: () = assert!(usize::BITS >= u32::BITS);

/// A `u32` index from `imara-diff` as a `usize`.
pub(crate) fn widen_one(index: u32) -> usize {
    index as usize
}

fn widen(range: Range<u32>) -> Range<usize> {
    widen_one(range.start)..widen_one(range.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::text;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};
    use std::iter;

    const ALGORITHMS: [Algorithm; 3] =
        [Algorithm::Histogram, Algorithm::Myers, Algorithm::MyersMinimal];
    const MODES: [Whitespace; 4] =
        [Whitespace::None, Whitespace::Trailing, Whitespace::Amount, Whitespace::All];

    fn hunk(before: Range<usize>, after: Range<usize>) -> Hunk {
        Hunk { before, after }
    }

    fn lines_with(
        before: &str,
        after: &str,
        algorithm: Algorithm,
        whitespace: Whitespace,
    ) -> Vec<Hunk> {
        diff_lines(
            &text(before),
            &text(after),
            DiffOptions { algorithm, whitespace, ..DiffOptions::default() },
        )
    }

    fn lines_all_algorithms(before: &str, after: &str) -> Vec<Vec<Hunk>> {
        ALGORITHMS
            .iter()
            .map(|&algorithm| lines_with(before, after, algorithm, Whitespace::None))
            .collect()
    }

    fn same_for_every_algorithm(before: &str, after: &str, expected: &[Hunk]) {
        for (algorithm, hunks) in ALGORITHMS.iter().zip(lines_all_algorithms(before, after)) {
            assert_eq!(hunks, expected, "{algorithm:?}");
        }
    }

    // Line diffs that every algorithm agrees on

    #[test]
    fn identical_inputs_have_no_hunks() {
        same_for_every_algorithm("a\nb\n", "a\nb\n", &[]);
        same_for_every_algorithm("", "", &[]);
    }

    #[test]
    fn one_changed_line_in_the_middle() {
        same_for_every_algorithm("a\nb\nc\n", "a\nX\nc\n", &[hunk(1..2, 1..2)]);
    }

    #[test]
    fn insertion_at_the_start() {
        same_for_every_algorithm("b\n", "a\nb\n", &[hunk(0..0, 0..1)]);
    }

    #[test]
    fn deletion_at_the_end() {
        same_for_every_algorithm("a\nb\n", "a\n", &[hunk(1..2, 1..1)]);
    }

    #[test]
    fn everything_inserted_into_an_empty_file() {
        same_for_every_algorithm("", "a\nb\n", &[hunk(0..0, 0..2)]);
    }

    #[test]
    fn everything_deleted() {
        same_for_every_algorithm("a\nb\n", "", &[hunk(0..2, 0..0)]);
    }

    #[test]
    fn two_separate_changes_are_two_hunks() {
        same_for_every_algorithm(
            "a\nb\nc\nd\ne\n",
            "A\nb\nc\nd\nE\n",
            &[hunk(0..1, 0..1), hunk(4..5, 4..5)],
        );
    }

    #[test]
    fn missing_final_newline_is_a_change_without_a_whitespace_mode() {
        same_for_every_algorithm("a\nb", "a\nb\n", &[hunk(1..2, 1..2)]);
    }

    #[test]
    fn crlf_and_lf_lines_differ_without_a_whitespace_mode() {
        same_for_every_algorithm("a\r\nb\n", "a\nb\n", &[hunk(0..1, 0..1)]);
    }

    // Algorithm choice. Expected hunks from `git diff --no-index -U0` with
    // `--histogram` and `--minimal`: before a,b,c,a,b; after b,a,b,c.

    const BEFORE_ALGO: &str = "a\nb\nc\na\nb\n";
    const AFTER_ALGO: &str = "b\na\nb\nc\n";

    #[test]
    fn histogram_matches_git_histogram() {
        assert_eq!(
            lines_with(BEFORE_ALGO, AFTER_ALGO, Algorithm::Histogram, Whitespace::None),
            [hunk(0..0, 0..1), hunk(3..5, 4..4)]
        );
    }

    #[test]
    fn myers_minimal_matches_git_minimal() {
        assert_eq!(
            lines_with(BEFORE_ALGO, AFTER_ALGO, Algorithm::MyersMinimal, Whitespace::None),
            [hunk(0..1, 0..0), hunk(2..3, 1..1), hunk(5..5, 3..4)]
        );
    }

    #[test]
    fn myers_matches_minimal_on_small_input() {
        assert_eq!(
            lines_with(BEFORE_ALGO, AFTER_ALGO, Algorithm::Myers, Whitespace::None),
            [hunk(0..1, 0..0), hunk(2..3, 1..1), hunk(5..5, 3..4)]
        );
    }

    #[test]
    fn indent_heuristic_places_an_ambiguous_insertion_as_git_does() {
        // `git diff --no-index --histogram -U0` gives `@@ -0,0 +1,3 @@` with the indent
        // heuristic and `@@ -1,0 +2,3 @@` without it.
        assert_eq!(
            lines_with(
                "{\n  a\n\n}\n  b\n",
                "{\n}\n    c\n{\n  a\n\n}\n  b\n",
                Algorithm::Histogram,
                Whitespace::None
            ),
            [hunk(0..0, 0..3)]
        );
    }

    #[test]
    fn merge_diff_places_an_ambiguous_insertion_as_git_merge_file_does() {
        // git merge-file runs no indent heuristic: `git diff --no-index --histogram -U0
        // --no-indent-heuristic` gives `@@ -1,0 +2,3 @@`.
        let hunks = diff_lines_for_merge(
            &text("{\n  a\n\n}\n  b\n"),
            &text("{\n}\n    c\n{\n  a\n\n}\n  b\n"),
            DiffOptions::default(),
        );
        assert_eq!(hunks, [hunk(1..1, 1..4)]);
    }

    #[test]
    fn merge_diff_with_myers_places_the_insertion_as_git_does_without_the_heuristic() {
        // `git diff --no-index --diff-algorithm=myers -U0 --no-indent-heuristic`.
        let hunks = diff_lines_for_merge(
            &text("{\n  a\n\n}\n  b\n"),
            &text("{\n}\n    c\n{\n  a\n\n}\n  b\n"),
            DiffOptions {
                algorithm: Algorithm::Myers,
                whitespace: Whitespace::None,
                ..DiffOptions::default()
            },
        );
        assert_eq!(hunks, [hunk(1..1, 1..4)]);
    }

    #[test]
    fn merge_diff_compares_lines_under_the_whitespace_mode() {
        let hunks = diff_lines_for_merge(
            &text("a\nb \nc\n"),
            &text("a\nb\r\nC\n"),
            DiffOptions {
                algorithm: Algorithm::Histogram,
                whitespace: Whitespace::Trailing,
                ..DiffOptions::default()
            },
        );
        assert_eq!(hunks, [hunk(2..3, 2..3)]);
    }

    // Line keys

    #[test]
    fn keys_without_a_mode_are_the_whole_line() {
        assert_eq!(line_key(" a \r\n", Whitespace::None.into()), " a \r\n");
    }

    #[test]
    fn trailing_keys_drop_trailing_whitespace_and_the_terminator() {
        assert_eq!(line_key("  a b \t\r\n", Whitespace::Trailing.into()), "  a b");
        assert_eq!(line_key("a", Whitespace::Trailing.into()), "a");
        assert_eq!(line_key(" \t\n", Whitespace::Trailing.into()), "");
    }

    #[test]
    fn trailing_keys_keep_a_cr_inside_the_line() {
        assert_eq!(line_key("a\rb\n", Whitespace::Trailing.into()), "a\rb");
    }

    #[test]
    fn amount_keys_collapse_each_run_to_one_space() {
        assert_eq!(line_key("  a \t b\r\n", Whitespace::Amount.into()), " a b");
        assert_eq!(line_key("a\r\rb\n", Whitespace::Amount.into()), "a b");
    }

    #[test]
    fn all_keys_remove_every_whitespace_character() {
        assert_eq!(line_key(" a \t b\r\n", Whitespace::All.into()), "ab");
    }

    const IGNORE_ENDINGS: Comparison =
        Comparison { whitespace: Whitespace::None, line_endings: LineEndingRule::Ignore };

    #[test]
    fn keys_ignoring_line_endings_drop_the_terminator_only() {
        assert_eq!(line_key(" a \r\n", IGNORE_ENDINGS), " a ");
        assert_eq!(line_key(" a \n", IGNORE_ENDINGS), " a ");
        assert_eq!(line_key(" a ", IGNORE_ENDINGS), " a ");
        assert_eq!(line_key("\n", IGNORE_ENDINGS), "");
    }

    #[test]
    fn keys_ignoring_line_endings_keep_a_cr_inside_the_line() {
        assert_eq!(line_key("a\rb\n", IGNORE_ENDINGS), "a\rb");
        assert_eq!(line_key("a\r\r\n", IGNORE_ENDINGS), "a\r");
        assert_eq!(line_key("a\r", IGNORE_ENDINGS), "a\r");
    }

    #[test]
    fn ignoring_line_endings_changes_no_whitespace_mode() {
        for whitespace in [Whitespace::Trailing, Whitespace::Amount, Whitespace::All] {
            let ignoring = Comparison { whitespace, line_endings: LineEndingRule::Ignore };
            assert_eq!(
                line_key(" a  b \r\n", ignoring),
                line_key(" a  b \r\n", whitespace.into()),
                "{whitespace:?}"
            );
        }
    }

    #[test]
    fn crlf_against_lf_differs_on_every_line_unless_line_endings_are_ignored() {
        let (crlf, lf) = (text("a\r\nb\r\nc\r\n"), text("a\nB\nc"));
        let with = |line_endings| DiffOptions { line_endings, ..DiffOptions::default() };
        assert_eq!(diff_lines(&crlf, &lf, with(LineEndingRule::Compare)), [hunk(0..3, 0..3)]);
        assert_eq!(diff_lines(&crlf, &lf, with(LineEndingRule::Ignore)), [hunk(1..2, 1..2)]);
    }

    #[test]
    fn keys_leave_other_unicode_spaces_alone() {
        // U+00A0 NO-BREAK SPACE is not whitespace under git's rule.
        assert_eq!(line_key("a\u{A0}\n", Whitespace::All.into()), "a\u{A0}");
    }

    // Whitespace modes in line diffs: no hunk when lines differ only in the mode's whitespace,
    // one hunk for a real change.

    fn every_algorithm(before: &str, after: &str, mode: Whitespace) -> Vec<Vec<Hunk>> {
        ALGORITHMS.iter().map(|&algorithm| lines_with(before, after, algorithm, mode)).collect()
    }

    fn assert_hunks(before: &str, after: &str, mode: Whitespace, expected: &[Hunk]) {
        for hunks in every_algorithm(before, after, mode) {
            assert_eq!(hunks, expected, "{mode:?}: {before:?} -> {after:?}");
        }
    }

    #[test]
    fn trailing_mode() {
        assert_hunks("a  \nb\n", "a\nb\n", Whitespace::Trailing, &[]);
        assert_hunks("a\r\nb\r\n", "a\nb\n", Whitespace::Trailing, &[]);
        assert_hunks("a\nb", "a\nb\n", Whitespace::Trailing, &[]);
        assert_hunks("  \n", "\n", Whitespace::Trailing, &[]);
        assert_hunks(" a\n", "a\n", Whitespace::Trailing, &[hunk(0..1, 0..1)]);
        assert_hunks("x\na \n", "x\nb\n", Whitespace::Trailing, &[hunk(1..2, 1..2)]);
    }

    #[test]
    fn amount_mode() {
        assert_hunks("a  b\n", "a b\n", Whitespace::Amount, &[]);
        assert_hunks("a\tb\n", "a b \n", Whitespace::Amount, &[]);
        assert_hunks("  a\n", " a\n", Whitespace::Amount, &[]);
        assert_hunks(" a\n", "a\n", Whitespace::Amount, &[hunk(0..1, 0..1)]);
        assert_hunks("ab\n", "a b\n", Whitespace::Amount, &[hunk(0..1, 0..1)]);
    }

    #[test]
    fn all_mode() {
        assert_hunks("a b\n", "ab\n", Whitespace::All, &[]);
        assert_hunks(" a\t\r\n", "a\n", Whitespace::All, &[]);
        assert_hunks("a\n", "b\n", Whitespace::All, &[hunk(0..1, 0..1)]);
    }

    #[test]
    fn no_mode_sees_whitespace_changes() {
        assert_hunks("a \n", "a\n", Whitespace::None, &[hunk(0..1, 0..1)]);
    }

    // Character diffs

    #[test]
    fn identical_strings_have_no_char_hunks() {
        assert_eq!(diff_chars("abc", "abc"), []);
        assert_eq!(diff_chars("", ""), []);
    }

    #[test]
    fn pure_char_insertion() {
        assert_eq!(diff_chars("ac", "abc"), [hunk(1..1, 1..2)]);
        assert_eq!(diff_chars("", "ab"), [hunk(0..0, 0..2)]);
    }

    #[test]
    fn pure_char_deletion() {
        assert_eq!(diff_chars("abc", "ac"), [hunk(1..2, 1..1)]);
    }

    #[test]
    fn replacement_in_the_middle() {
        assert_eq!(diff_chars("a cat sat", "a dog sat"), [hunk(2..5, 2..5)]);
    }

    #[test]
    fn multibyte_characters_give_byte_ranges_on_char_boundaries() {
        // é and ü are two bytes each; the emoji is four.
        assert_eq!(diff_chars("aéb", "aüb"), [hunk(1..3, 1..3)]);
        assert_eq!(diff_chars("x\u{1F600}", "x"), [hunk(1..5, 1..1)]);
        assert_eq!(diff_chars("é", "éé"), [hunk(2..2, 2..4)]);
    }

    #[test]
    fn intraline_runs_up_to_the_limit() {
        let at_limit = "a".repeat(MAX_INTRALINE_DIFF_CHARS);
        let changed = format!("{}b", "a".repeat(MAX_INTRALINE_DIFF_CHARS - 1));
        assert_eq!(
            intraline_hunks(&at_limit, &changed),
            Some(vec![hunk(
                MAX_INTRALINE_DIFF_CHARS - 1..MAX_INTRALINE_DIFF_CHARS,
                MAX_INTRALINE_DIFF_CHARS - 1..MAX_INTRALINE_DIFF_CHARS
            )])
        );
    }

    #[test]
    fn intraline_stops_above_the_limit_on_either_side() {
        let short = "a";
        let long = "a".repeat(MAX_INTRALINE_DIFF_CHARS + 1);
        assert_eq!(intraline_hunks(&long, short), None);
        assert_eq!(intraline_hunks(short, &long), None);
    }

    #[test]
    fn intraline_limit_counts_characters_not_bytes() {
        // 10 000 two-byte characters: 20 000 bytes but within the limit.
        let wide = "é".repeat(MAX_INTRALINE_DIFF_CHARS);
        assert_eq!(intraline_hunks(&wide, &wide), Some(vec![]));
    }

    // Properties. Small alphabets make lines repeat, which is where diffs go wrong.

    const PROPTEST_SEED: u64 = 0x7121_AD00;
    const PROPTEST_CASES: u32 = 512;

    fn config() -> Config {
        Config {
            cases: PROPTEST_CASES,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            ..Config::default()
        }
    }

    fn line_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("a\n".to_owned()),
            Just("b\n".to_owned()),
            Just("}\n".to_owned()),
            Just("a \n".to_owned()),
            Just(" a\n".to_owned()),
            Just("a\r\n".to_owned()),
            Just("a\tb\n".to_owned()),
            Just("a  b\n".to_owned()),
            Just("\n".to_owned()),
        ]
    }

    fn file_strategy() -> impl Strategy<Value = String> {
        (
            prop::collection::vec(line_strategy(), 0..24),
            prop_oneof![Just(""), Just("a"), Just("b ")],
        )
            .prop_map(|(lines, last)| lines.concat() + last)
    }

    fn algorithm_strategy() -> impl Strategy<Value = Algorithm> {
        prop::sample::select(ALGORITHMS.to_vec())
    }

    fn mode_strategy() -> impl Strategy<Value = Whitespace> {
        prop::sample::select(MODES.to_vec())
    }

    /// Hunks are ordered, disjoint, in bounds and non-empty, and every line outside them
    /// pairs with an equal-keyed line on the other side.
    fn check_line_hunks(before: &Text, after: &Text, hunks: &[Hunk], mode: Whitespace) {
        for h in hunks {
            assert!(!h.before.is_empty() || !h.after.is_empty(), "empty hunk {h:?}");
        }
        // Each unchanged stretch runs from one hunk's end to the next hunk's start.
        let ends = iter::once((0, 0)).chain(hunks.iter().map(|h| (h.before.end, h.after.end)));
        let starts = hunks
            .iter()
            .map(|h| (h.before.start, h.after.start))
            .chain(iter::once((before.line_count(), after.line_count())));
        for ((b0, a0), (b1, a1)) in ends.zip(starts) {
            assert!(b1 >= b0 && a1 >= a0, "hunks out of order or out of bounds");
            assert_eq!(b1 - b0, a1 - a0, "unchanged stretch has equal length");
            for (x, y) in (b0..b1).zip(a0..a1) {
                assert_eq!(
                    line_key(before.line(x), mode.into()),
                    line_key(after.line(y), mode.into())
                );
            }
        }
    }

    /// Replacing each hunk's `before` lines with its `after` lines gives `after` exactly.
    fn apply_line_hunks(before: &Text, after: &Text, hunks: &[Hunk]) -> String {
        let mut out = String::new();
        let mut at = 0;
        for h in hunks {
            out.extend((at..h.before.start).map(|n| before.line(n)));
            out.extend(h.after.clone().map(|n| after.line(n)));
            at = h.before.end;
        }
        out.extend((at..before.line_count()).map(|n| before.line(n)));
        out
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn line_hunks_rebuild_after_without_a_mode(
            before in file_strategy(),
            after in file_strategy(),
            algorithm in algorithm_strategy(),
        ) {
            let (b, a) = (text(&before), text(&after));
            let hunks = diff_lines(&b, &a, DiffOptions { algorithm, ..DiffOptions::default() });
            check_line_hunks(&b, &a, &hunks, Whitespace::None);
            prop_assert_eq!(apply_line_hunks(&b, &a, &hunks), after);
        }

        #[test]
        fn line_hunks_pair_equal_keys_in_every_mode(
            before in file_strategy(),
            after in file_strategy(),
            algorithm in algorithm_strategy(),
            mode in mode_strategy(),
        ) {
            let (b, a) = (text(&before), text(&after));
            let hunks = diff_lines(&b, &a, DiffOptions { algorithm, whitespace: mode, ..DiffOptions::default() });
            check_line_hunks(&b, &a, &hunks, mode);
        }

        #[test]
        fn identical_files_have_no_hunks_in_any_mode(
            file in file_strategy(),
            algorithm in algorithm_strategy(),
            mode in mode_strategy(),
        ) {
            let t = text(&file);
            prop_assert_eq!(diff_lines(&t, &t, DiffOptions { algorithm, whitespace: mode, ..DiffOptions::default() }), vec![]);
        }

        #[test]
        fn char_hunks_rebuild_after(
            before in "[ab é😀]{0,16}",
            after in "[ab é😀]{0,16}",
        ) {
            let hunks = diff_chars(&before, &after);
            let mut out = String::new();
            let mut at = 0;
            for h in &hunks {
                prop_assert!(h.before.start >= at, "char hunks out of order");
                prop_assert!(!h.before.is_empty() || !h.after.is_empty(), "empty hunk");
                out.push_str(&before[at..h.before.start]);
                out.push_str(&after[h.after.clone()]);
                at = h.before.end;
            }
            out.push_str(&before[at..]);
            prop_assert_eq!(out, after);
        }
    }
}
