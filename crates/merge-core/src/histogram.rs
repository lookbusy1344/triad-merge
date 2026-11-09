//! git's histogram line diff with git's slider compaction, for the merge (spec §4.2).
//!
//! The merge must place hunks as git `merge-file` does, or conflicts form in other places.
//! imara-diff's histogram strips the common prefix and suffix first and picks anchors in
//! another order, so its hunks differ from git's on repetitive input. This module follows
//! the histogram design from `JGit`, which git's `xhistogram.c` also follows: no prefix or suffix
//! stripping; scan `after` in order, skip past each matched run, and keep a new run only when
//! it is longer or its rarest line is rarer than the best so far.

use std::ops::Range;

use imara_diff::{Algorithm, Diff, Token};

use crate::diff::{Hunk, widen_one};

/// A line that occurs more often than this in a region of `before` is too common to anchor on.
/// When every line the two regions share is that common, the region falls back to Myers.
const MAX_CHAIN_LENGTH: usize = 64;

/// The hunks that turn `before` into `after`, as git's histogram diff places them without
/// the indent heuristic.
pub(crate) fn diff(before: &[Token], after: &[Token], num_tokens: u32) -> Vec<Hunk> {
    let mut removed = vec![false; before.len()];
    let mut added = vec![false; after.len()];
    let mut regions = vec![(0..before.len(), 0..after.len())];
    let mut table = Occurrences::new(num_tokens, before.len());
    while let Some((a, b)) = regions.pop() {
        if a.is_empty() || b.is_empty() {
            removed[a].fill(true);
            added[b].fill(true);
            continue;
        }
        match anchor(&mut table, before, after, &a, &b) {
            Anchor::Run { a: run_a, b: run_b } => {
                regions.push((a.start..run_a.start, b.start..run_b.start));
                regions.push((run_a.end..a.end, run_b.end..b.end));
            }
            Anchor::None => {
                removed[a].fill(true);
                added[b].fill(true);
            }
            Anchor::TooCommon => {
                myers(
                    (&mut removed[a.clone()], &before[a]),
                    (&mut added[b.clone()], &after[b]),
                    num_tokens,
                );
            }
        }
    }
    compact((&mut removed, before), (&mut added, after), num_tokens);
    compact((&mut added, after), (&mut removed, before), num_tokens);
    hunks(&removed, &added)
}

enum Anchor {
    /// A run of equal lines to split the regions around.
    Run { a: Range<usize>, b: Range<usize> },
    /// The regions share no line.
    None,
    /// Every shared line occurs more than [`MAX_CHAIN_LENGTH`] times in `before`'s region.
    TooCommon,
}

/// Each line's positions in one region of `before`, in order: a counting sort into one flat
/// array. The arrays indexed by token are allocated once per diff; filling the table for a
/// region and clearing it again touch only that region's lines.
struct Occurrences {
    /// Occurrences of each token in the region.
    count: Vec<u32>,
    /// Where each token's positions start in `positions`.
    offset: Vec<u32>,
    positions: Vec<usize>,
    /// The tokens with a non-zero count.
    tokens: Vec<Token>,
}

impl Occurrences {
    fn new(num_tokens: u32, lines: usize) -> Self {
        let tokens = widen_one(num_tokens);
        Self {
            count: vec![0; tokens],
            offset: vec![0; tokens],
            positions: Vec::with_capacity(lines),
            tokens: Vec::new(),
        }
    }

    fn fill(&mut self, lines: &[Token], region: &Range<usize>) {
        for t in self.tokens.drain(..) {
            self.count[index(t)] = 0;
        }
        for &t in &lines[region.clone()] {
            let count = &mut self.count[index(t)];
            if *count == 0 {
                self.tokens.push(t);
            }
            *count += 1;
        }
        let mut next = 0;
        for &t in &self.tokens {
            self.offset[index(t)] = next;
            next += self.count[index(t)];
        }
        self.positions.clear();
        self.positions.resize(region.len(), 0);
        // Fill each token's slots in order, using `offset` as a cursor, then wind it back.
        for i in region.clone() {
            let at = &mut self.offset[index(lines[i])];
            self.positions[index_u32(*at)] = i;
            *at += 1;
        }
        for &t in &self.tokens {
            self.offset[index(t)] -= self.count[index(t)];
        }
    }

    /// The positions of `token` in the region, in order; empty if it does not occur.
    fn of(&self, token: Token) -> &[usize] {
        let (start, count) = (self.offset[index(token)], self.count[index(token)]);
        if count == 0 {
            return &[];
        }
        &self.positions[index_u32(start)..index_u32(start + count)]
    }
}

fn index(token: Token) -> usize {
    index_u32(token.0)
}

fn index_u32(n: u32) -> usize {
    widen_one(n)
}

fn anchor(
    table: &mut Occurrences,
    before: &[Token],
    after: &[Token],
    a: &Range<usize>,
    b: &Range<usize>,
) -> Anchor {
    table.fill(before, a);
    let table = &*table;
    let count = |i: usize| table.of(before[i]).len();

    let mut best: Option<(Range<usize>, Range<usize>)> = None;
    // git starts from an empty run that counts as length 1 and a limit one past the cap.
    let mut best_len = 1;
    let mut best_count = MAX_CHAIN_LENGTH + 1;
    let mut shared = false;
    let mut b_at = b.start;
    while b_at < b.end {
        let mut b_next = b_at + 1;
        let positions = table.of(after[b_at]);
        if !positions.is_empty() {
            shared = true;
            if positions.len() <= best_count {
                let mut k = 0;
                while let Some(&start) = positions.get(k) {
                    let mut rarest = positions.len();
                    let note = |rarest: &mut usize, i: usize| {
                        if *rarest > 1 {
                            *rarest = (*rarest).min(count(i));
                        }
                    };
                    let (mut a_start, mut b_start) = (start, b_at);
                    while a_start > a.start
                        && b_start > b.start
                        && before[a_start - 1] == after[b_start - 1]
                    {
                        a_start -= 1;
                        b_start -= 1;
                        note(&mut rarest, a_start);
                    }
                    let (mut a_end, mut b_end) = (start + 1, b_at + 1);
                    while a_end < a.end && b_end < b.end && before[a_end] == after[b_end] {
                        note(&mut rarest, a_end);
                        a_end += 1;
                        b_end += 1;
                    }
                    b_next = b_next.max(b_end);
                    if best_len < a_end - a_start || rarest < best_count {
                        best = Some((a_start..a_end, b_start..b_end));
                        best_len = a_end - a_start;
                        best_count = rarest;
                    }
                    // Resume at the first occurrence past this run.
                    k += positions[k..].iter().take_while(|&&p| p < a_end).count();
                }
            }
        }
        b_at = b_next;
    }
    match best {
        _ if shared && best_count > MAX_CHAIN_LENGTH => Anchor::TooCommon,
        Some((a, b)) => Anchor::Run { a, b },
        None => Anchor::None,
    }
}

/// A run of changed lines, possibly empty, between two unchanged lines or a file edge. The
/// n-th group of one file pairs with the n-th group of the other.
struct Group {
    start: usize,
    end: usize,
}

impl Group {
    fn first(changed: &[bool]) -> Self {
        let mut group = Self { start: 0, end: 0 };
        group.extend_down(changed);
        group
    }

    fn is_empty(&self) -> bool {
        self.start == self.end
    }

    fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    fn extend_down(&mut self, changed: &[bool]) {
        while self.end < changed.len() && changed[self.end] {
            self.end += 1;
        }
    }

    fn extend_up(&mut self, changed: &[bool]) {
        while self.start > 0 && changed[self.start - 1] {
            self.start -= 1;
        }
    }

    fn next(&mut self, changed: &[bool]) -> bool {
        if self.end == changed.len() {
            return false;
        }
        self.start = self.end + 1;
        self.end = self.start;
        self.extend_down(changed);
        true
    }

    fn previous(&mut self, changed: &[bool]) -> bool {
        if self.start == 0 {
            return false;
        }
        self.end = self.start - 1;
        self.start = self.end;
        self.extend_up(changed);
        true
    }

    /// Move the group up one line when the line above equals its last line, merging with a
    /// group it reaches.
    fn slide_up(&mut self, changed: &mut [bool], lines: &[Token]) -> bool {
        if self.start == 0 || lines[self.start - 1] != lines[self.end - 1] {
            return false;
        }
        self.start -= 1;
        changed[self.start] = true;
        self.end -= 1;
        changed[self.end] = false;
        self.extend_up(changed);
        true
    }

    /// Move the group down one line when the line below equals its first line, merging with a
    /// group it reaches.
    fn slide_down(&mut self, changed: &mut [bool], lines: &[Token]) -> bool {
        if self.end == changed.len() || lines[self.start] != lines[self.end] {
            return false;
        }
        changed[self.start] = false;
        self.start += 1;
        changed[self.end] = true;
        self.end += 1;
        self.extend_down(changed);
        true
    }
}

/// Lines of one file with their changed marks.
type Marked<'a> = (&'a mut [bool], &'a [Token]);

/// Mark the changes between `before` and `after` with git's Myers diff. imara-diff's Myers is a
/// port of git's, with the same prefix and suffix trimming and discarding of unmatched lines.
fn myers(before: Marked<'_>, after: Marked<'_>, num_tokens: u32) {
    let ((removed, before), (added, after)) = (before, after);
    let mut diff = Diff::default();
    diff.compute_with(Algorithm::Myers, before, after, num_tokens);
    for (mark, n) in removed.iter_mut().zip(0..) {
        *mark = diff.is_removed(n);
    }
    for (mark, n) in added.iter_mut().zip(0..) {
        *mark = diff.is_added(n);
    }
}

/// git's `xdl_change_compact` without the indent heuristic, for a histogram diff. Each group of
/// changed lines slides up and down as far as it goes, merging with groups it meets, and ends
/// as low as possible, or at the lowest place where it lines up with a change in the other
/// file. A group that moved, facing a changed group in the other file, is diffed again with
/// Myers, which can find lines the two now share (git 2.54 and later).
fn compact(this: Marked<'_>, other: Marked<'_>, num_tokens: u32) {
    // The asserts below check git's invariant: both files have the same number of
    // unchanged lines, so the groups between them pair up one to one, and a group that
    // slides moves its partner in the other file with it.
    const IN_STEP: &str = "groups in the two files stay in step";
    let (changed, lines) = this;
    let (other_changed, other_lines) = other;
    let mut g = Group::first(changed);
    let mut go = Group::first(other_changed);
    loop {
        if !g.is_empty() {
            let original = g.range();
            let (mut top_end, mut meets_other);
            loop {
                let size = g.end - g.start;
                while g.slide_up(changed, lines) {
                    assert!(go.previous(other_changed), "{IN_STEP}");
                }
                top_end = g.end;
                meets_other = !go.is_empty();
                while g.slide_down(changed, lines) {
                    assert!(go.next(other_changed), "{IN_STEP}");
                    meets_other |= !go.is_empty();
                }
                if size == g.end - g.start {
                    break;
                }
            }
            if g.end != top_end && meets_other {
                while go.is_empty() {
                    assert!(
                        g.slide_up(changed, lines),
                        "a group that met a change can reach it again"
                    );
                    assert!(go.previous(other_changed), "{IN_STEP}");
                }
            }
            if !go.is_empty() && g.range() != original {
                myers(
                    (&mut changed[g.range()], &lines[g.range()]),
                    (&mut other_changed[go.range()], &other_lines[go.range()]),
                    num_tokens,
                );
            }
        }
        if !g.next(changed) {
            break;
        }
        assert!(go.next(other_changed), "{IN_STEP}");
    }
}

fn hunks(removed: &[bool], added: &[bool]) -> Vec<Hunk> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    loop {
        while i < removed.len() && j < added.len() && !removed[i] && !added[j] {
            i += 1;
            j += 1;
        }
        let (before_start, after_start) = (i, j);
        while i < removed.len() && removed[i] {
            i += 1;
        }
        while j < added.len() && added[j] {
            j += 1;
        }
        if (i, j) == (before_start, after_start) {
            return out;
        }
        out.push(Hunk { before: before_start..i, after: after_start..j });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use imara_diff::InternedInput;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    fn lines(before: &str, after: &str) -> Vec<Hunk> {
        let mut input = InternedInput::default();
        input.update_before(before.split_inclusive('\n'));
        input.update_after(after.split_inclusive('\n'));
        diff(&input.before, &input.after, input.interner.num_tokens())
    }

    fn hunk(before: Range<usize>, after: Range<usize>) -> Hunk {
        Hunk { before, after }
    }

    fn repeat(line: &str, times: usize) -> String {
        line.repeat(times)
    }

    // Expected hunks from `git diff --no-index --histogram --no-indent-heuristic -U0`, git 2.56.

    #[test]
    fn empty_and_identical_inputs() {
        assert_eq!(lines("a\nb\nc\n", "a\nb\nc\n"), []);
        assert_eq!(lines("", ""), []);
        assert_eq!(lines("", "a\nb\n"), [hunk(0..0, 0..2)]);
        assert_eq!(lines("a\nb\n", ""), [hunk(0..2, 0..0)]);
    }

    #[test]
    fn anchors_on_the_first_rarest_run_without_stripping_the_common_prefix() {
        // Stripping the shared `a` first would anchor on `a b` at lines 2..4 of both and
        // replace `R` with `b`. git anchors `a b` at base 2..4 on `a b` at 0..2.
        assert_eq!(lines("a\nR\na\nb\n", "a\nb\na\nb\n a\n"), [hunk(1..3, 1..1), hunk(4..4, 2..5)]);
    }

    #[test]
    fn a_group_merged_while_sliding_is_diffed_again() {
        assert_eq!(lines("a\nb\n", "c\nb\na\nd\ne\nb\n"), [hunk(0..0, 0..2), hunk(1..1, 3..5)]);
    }

    #[test]
    fn lines_commoner_than_the_best_anchor_are_not_tried() {
        // After anchoring on `a` (once in before), `c` and `b` (twice) are skipped, although
        // `c b` would be a longer run.
        assert_eq!(
            lines("a\nb\nc\nc\nb\n", "x\nc\na\nc\nb\n"),
            [hunk(0..0, 0..2), hunk(1..3, 3..3)]
        );
    }

    #[test]
    fn occurrences_inside_a_matched_run_are_not_tried() {
        assert_eq!(
            lines("u\nb\nb\nb\nu\n", "x\nb\nb\nu\nb\nb\n"),
            [hunk(0..0, 0..3), hunk(3..5, 6..6)]
        );
    }

    #[test]
    fn an_occurrence_right_after_a_matched_run_is_tried() {
        // From `git diff --histogram --no-indent-heuristic`. The run on the first `b` ends where
        // the second `b` starts, and that `b` holds the better anchor.
        assert_eq!(lines("a\nb\nb\na\n", "b\na\nb\n"), [hunk(0..2, 0..0), hunk(4..4, 2..3)]);
    }

    #[test]
    fn the_scan_of_after_resumes_past_each_matched_run() {
        assert_eq!(
            lines("b\na\nu\na\nu\nu\na\n", "b\nu\nu\nu\na\nu\n"),
            [hunk(1..2, 1..3), hunk(5..7, 6..6)]
        );
    }

    #[test]
    fn swapped_lines() {
        assert_eq!(lines("p\nq\nr\n", "r\nq\np\n"), [hunk(0..2, 0..0), hunk(3..3, 1..3)]);
    }

    #[test]
    fn insertions_and_deletions_slide_as_far_down_as_they_go() {
        assert_eq!(lines("x\na\nb\n", "x\na\nb\na\nb\n"), [hunk(3..3, 3..5)]);
        assert_eq!(lines("a\nb\nc\nb\nc\n", "a\nb\nc\nX\nb\nc\nb\nc\n"), [hunk(3..3, 3..6)]);
        assert_eq!(lines("c\na\nb\na\nb\nd\n", "c\na\nb\nd\n"), [hunk(3..5, 3..3)]);
    }

    #[test]
    fn a_sliding_group_stops_where_it_lines_up_with_a_change_on_the_other_side() {
        // The deletion of one `x` could sit at line 0 or 1; at 0 it pairs with the insertion
        // of `y` and the two form one change.
        assert_eq!(lines("x\nx\n", "y\nx\n"), [hunk(0..1, 0..1)]);
    }

    #[test]
    fn a_moved_group_is_diffed_again() {
        // Without the re-diff (git before 2.54) the result is one hunk 2..6 -> 2..6.
        assert_eq!(
            lines("x\na\na\nb\nc\na\nb\nc\n", "x\na\n a\n b\nc\n a\nb\nc\n"),
            [hunk(2..4, 2..4), hunk(5..6, 5..6)]
        );
    }

    #[test]
    fn lines_up_to_the_chain_limit_still_anchor() {
        // 64 copies of x on each side.
        let before = format!("{}p\n{}", repeat("x\n", 32), repeat("x\n", 32));
        let after = format!("{}q\n{}", repeat("x\n", 31), repeat("x\n", 33));
        assert_eq!(lines(&before, &after), [hunk(31..31, 31..63), hunk(32..64, 64..64)]);
    }

    #[test]
    fn lines_above_the_chain_limit_fall_back_to_myers() {
        // 65 copies of x on each side: no line is rare enough to anchor on.
        let before = format!("{}p\n{}", repeat("x\n", 33), repeat("x\n", 32));
        let after = format!("{}q\n{}", repeat("x\n", 32), repeat("x\n", 33));
        assert_eq!(lines(&before, &after), [hunk(32..32, 32..33), hunk(33..34, 34..34)]);
    }

    // Properties

    const PROPTEST_SEED: u64 = 0x4157_0000;
    const PROPTEST_CASES: u32 = 512;

    fn file_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(prop::sample::select(vec!["a\n", "b\n", "c\n", "}\n", "\n"]), 0..24)
            .prop_map(|lines| lines.concat())
    }

    proptest! {
        #![proptest_config(Config {
            cases: PROPTEST_CASES,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            ..Config::default()
        })]

        #[test]
        fn hunks_are_ordered_non_empty_and_rebuild_after(
            before in file_strategy(),
            after in file_strategy(),
        ) {
            let (b, a): (Vec<&str>, Vec<&str>) =
                (before.split_inclusive('\n').collect(), after.split_inclusive('\n').collect());
            let mut out = String::new();
            let (mut at_b, mut at_a) = (0, 0);
            for h in lines(&before, &after) {
                prop_assert!(!h.before.is_empty() || !h.after.is_empty(), "empty hunk");
                prop_assert!(h.before.start >= at_b && h.after.start >= at_a, "out of order");
                prop_assert_eq!(h.before.start - at_b, h.after.start - at_a);
                prop_assert_eq!(&b[at_b..h.before.start], &a[at_a..h.after.start]);
                out.extend(b[at_b..h.before.start].iter().copied());
                out.extend(a[h.after.clone()].iter().copied());
                (at_b, at_a) = (h.before.end, h.after.end);
            }
            prop_assert_eq!(&b[at_b..], &a[at_a..]);
            out.extend(b[at_b..].iter().copied());
            prop_assert_eq!(out, after);
        }
    }
}
