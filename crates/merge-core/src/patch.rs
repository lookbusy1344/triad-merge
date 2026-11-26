//! Unified diffs of a two-way diff's regions (spec §4.6), as `git diff` prints them.
//!
//! A hunk holds one or more changed regions, with up to `context` unchanged lines before and
//! after each. Two changes join into one hunk when at most twice `context` lines lie between
//! them. Context comes only from unchanged regions. A change the comparison ignores starts no
//! hunk; inside one it prints as a change. Context lines print from the right side, as
//! `git diff -w` prints them.

use std::ops::Range;

use crate::diff2::{Kind2, Region2};
use crate::index::RegionIx;
use crate::text::{Encoding, Text};

/// `git diff`'s context size.
pub const DEFAULT_CONTEXT: usize = 3;

/// The longest hunk-header function name, in bytes, as git's `def_ff` keeps it.
const FUNC_NAME_MAX: usize = 80;

const NO_NEWLINE: &str = "\\ No newline at end of file\n";

/// Which changes a patch holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every changed region.
    File,
    /// One region, which gives an empty patch unless it is changed.
    Region(RegionIx),
}

/// The names the `---` and `+++` headers give the two sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Titles<'a> {
    pub left: &'a str,
    pub right: &'a str,
}

/// The unified diff of `left` and `right` over `regions`, which must cover both texts in
/// order, as [`crate::diff2::regions`] gives them. Empty when `scope` holds no change.
#[must_use]
pub fn unified(
    left: &Text,
    right: &Text,
    regions: &[Region2],
    context: usize,
    scope: Scope,
    titles: Titles<'_>,
) -> String {
    let Unified { mut head, hunks, .. } =
        unified_patch(left, right, regions, context, scope, titles);
    head.push_str(&hunks);
    head
}

/// A unified diff in two parts: the `---` and `+++` lines, which name files, and the hunks,
/// which hold the texts' lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unified {
    head: String,
    hunks: String,
    /// The encoding the hunks' bytes take: Windows-1252 when both texts read one character
    /// per byte.
    encoding: Encoding,
}

impl Unified {
    /// The patch as `git apply` reads it: the head in UTF-8, the hunks in the texts' shared
    /// 8-bit encoding, so each line holds the bytes of its file.
    ///
    /// # Panics
    ///
    /// Never: the hunks hold ASCII and the two texts' characters, which their shared
    /// encoding holds.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        #[expect(clippy::expect_used, reason = "invariant: see the message")]
        let hunks = self.encoding.encode_chunk(&self.hunks).expect(
            "the hunks hold ASCII and the texts' characters, which their shared encoding holds",
        );
        [self.head.as_bytes(), &hunks].concat()
    }

    /// The patch as text.
    #[must_use]
    pub fn text(&self) -> String {
        [self.head.as_str(), &self.hunks].concat()
    }
}

/// [`unified`] in its two parts.
#[must_use]
pub fn unified_patch(
    left: &Text,
    right: &Text,
    regions: &[Region2],
    context: usize,
    scope: Scope,
    titles: Titles<'_>,
) -> Unified {
    let byte_per_char = left.encoding().one_byte_per_char() && right.encoding().one_byte_per_char();
    let encoding = if byte_per_char { Encoding::Windows1252 } else { Encoding::Utf8 };
    let hunks = hunks(regions, context, scope);
    if hunks.is_empty() {
        return Unified { head: String::new(), hunks: String::new(), encoding };
    }
    let mut head = String::new();
    for (sign, title) in [("--- ", titles.left), ("+++ ", titles.right)] {
        head.push_str(sign);
        head.push_str(title);
        head.push('\n');
    }
    let mut out = String::new();
    for hunk in &hunks {
        write_hunk(&mut out, left, right, &regions[hunk.regions.clone()], hunk);
    }
    Unified { head, hunks: out, encoding }
}

/// One hunk: the regions it spans, and the context it takes from the unchanged regions at
/// either end.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk {
    regions: Range<usize>,
    lead: usize,
    trail: usize,
}

fn hunks(regions: &[Region2], context: usize, scope: Scope) -> Vec<Hunk> {
    let changed = |i: &usize| regions[*i].kind == Kind2::Changed;
    let starts: Vec<usize> = match scope {
        Scope::File => (0..regions.len()).filter(changed).collect(),
        Scope::Region(region) => {
            Some(region.get()).filter(|i| *i < regions.len()).filter(changed).into_iter().collect()
        }
    };
    let mut groups: Vec<Range<usize>> = Vec::new();
    for i in starts {
        match groups.last_mut() {
            Some(group) if gap(&regions[group.end..i]) <= 2 * context => group.end = i + 1,
            _ => groups.push(i..i + 1),
        }
    }
    groups
        .into_iter()
        .map(|group| {
            let lead = group.start.checked_sub(1).map_or(0, |i| unchanged_len(&regions[i]));
            let trail = regions.get(group.end).map_or(0, unchanged_len);
            Hunk { lead: lead.min(context), trail: trail.min(context), regions: group }
        })
        .collect()
}

/// The lines between two changes: the longer side of each region between them.
fn gap(between: &[Region2]) -> usize {
    between.iter().map(|r| r.left.len().max(r.right.len())).sum()
}

/// The lines a region gives as context: all of an unchanged one, none of another.
fn unchanged_len(region: &Region2) -> usize {
    if region.kind == Kind2::Unchanged { region.right.len() } else { 0 }
}

fn write_hunk(out: &mut String, left: &Text, right: &Text, spanned: &[Region2], hunk: &Hunk) {
    let (Some(first), Some(last)) = (spanned.first(), spanned.last()) else {
        return;
    };
    let left_lines = first.left.start - hunk.lead..last.left.end + hunk.trail;
    let right_lines = first.right.start - hunk.lead..last.right.end + hunk.trail;
    out.push_str("@@ -");
    out.push_str(&range(&left_lines));
    out.push_str(" +");
    out.push_str(&range(&right_lines));
    out.push_str(" @@");
    if let Some(name) = func_name(left, left_lines.start) {
        out.push(' ');
        out.push_str(name);
    }
    out.push('\n');
    let context_before = first.right.start - hunk.lead..first.right.start;
    let context_after = last.right.end..last.right.end + hunk.trail;
    write_lines(out, ' ', right, context_before);
    for region in spanned {
        if region.kind == Kind2::Unchanged {
            write_lines(out, ' ', right, region.right.clone());
        } else {
            write_lines(out, '-', left, region.left.clone());
            write_lines(out, '+', right, region.right.clone());
        }
    }
    write_lines(out, ' ', right, context_after);
}

/// A hunk header's range: from 1, with the count left out when it is 1. An empty range
/// names the line before it, as `diff -u` does.
fn range(lines: &Range<usize>) -> String {
    match lines.len() {
        0 => format!("{},0", lines.start),
        1 => format!("{}", lines.start + 1),
        count => format!("{},{count}", lines.start + 1),
    }
}

fn write_lines(out: &mut String, sign: char, text: &Text, lines: Range<usize>) {
    for n in lines {
        let line = text.line(n);
        out.push(sign);
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push('\n');
            out.push_str(NO_NEWLINE);
        }
    }
}

/// git's default function name for a hunk that starts at line `start` of the left side: the
/// nearest line above it that starts with an ASCII letter, `_` or `$`, cut to
/// [`FUNC_NAME_MAX`] bytes on a character boundary, without trailing whitespace.
fn func_name(left: &Text, start: usize) -> Option<&str> {
    (0..start).rev().map(|n| left.line(n)).find(|line| starts_name(line)).map(|line| {
        let cut = line.floor_char_boundary(FUNC_NAME_MAX.min(line.len()));
        line[..cut].trim_end_matches([' ', '\t', '\n', '\r'])
    })
}

fn starts_name(line: &str) -> bool {
    line.bytes().next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b == b'$')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{BlankLineRule, DiffOptions, Whitespace};
    use crate::diff2::regions;

    const TITLES: Titles<'static> = Titles { left: "old", right: "new" };

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes()).unwrap()
    }

    fn patch_with(
        left: &str,
        right: &str,
        opts: &DiffOptions,
        context: usize,
        scope: Scope,
    ) -> String {
        let (left, right) = (text(left), text(right));
        unified(&left, &right, &regions(&left, &right, opts), context, scope, TITLES)
    }

    fn patch(left: &str, right: &str, context: usize) -> String {
        patch_with(left, right, &DiffOptions::default(), context, Scope::File)
    }

    /// Ten numbered lines that start with digits, so no line names a function.
    fn numbered() -> String {
        (1..=10).map(|n| n.to_string() + "\n").collect()
    }

    #[test]
    fn equal_texts_give_an_empty_patch() {
        assert_eq!(patch("a\nb\n", "a\nb\n", DEFAULT_CONTEXT), "");
        assert_eq!(patch("", "", DEFAULT_CONTEXT), "");
    }

    #[test]
    fn one_changed_line_takes_context_on_both_sides() {
        let right = numbered().replace("5\n", "five\n");
        assert_eq!(
            patch(&numbered(), &right, DEFAULT_CONTEXT),
            "--- old\n+++ new\n@@ -2,7 +2,7 @@\n 2\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n"
        );
    }

    #[test]
    fn context_stops_at_either_end_of_the_file() {
        let right = numbered().replace("1\n", "one\n").replace("10\n", "ten\n");
        assert_eq!(
            patch(&numbered(), &right, 1),
            "--- old\n+++ new\n@@ -1,2 +1,2 @@\n-1\n+one\n 2\n@@ -9,2 +9,2 @@\n 9\n-10\n+ten\n"
        );
    }

    #[test]
    fn changes_join_when_twice_the_context_or_fewer_lines_lie_between() {
        // Lines 3 and 6 changed: 2 lines between, joined at context 1, apart at context 0.
        let right = numbered().replace("3\n", "c\n").replace("6\n", "f\n");
        assert_eq!(
            patch(&numbered(), &right, 1),
            "--- old\n+++ new\n@@ -2,6 +2,6 @@\n 2\n-3\n+c\n 4\n 5\n-6\n+f\n 7\n"
        );
        assert_eq!(
            patch(&numbered(), &right, 0),
            "--- old\n+++ new\n@@ -3 +3 @@\n-3\n+c\n@@ -6 +6 @@\n-6\n+f\n"
        );
    }

    #[test]
    fn changes_one_line_too_far_apart_stay_in_two_hunks() {
        // Lines 2 and 6 changed: 3 lines between, more than 2 × 1.
        let right = numbered().replace("2\n", "b\n").replace("6\n", "f\n");
        assert_eq!(
            patch(&numbered(), &right, 1),
            "--- old\n+++ new\n@@ -1,3 +1,3 @@\n 1\n-2\n+b\n 3\n@@ -5,3 +5,3 @@\n 5\n-6\n+f\n 7\n"
        );
    }

    #[test]
    fn an_empty_range_names_the_line_before_it() {
        assert_eq!(patch("a\n", "x\na\n", 0), "--- old\n+++ new\n@@ -0,0 +1 @@\n+x\n");
        assert_eq!(
            patch("1\n2\n", "1\n2\n3\n4\n", 0),
            "--- old\n+++ new\n@@ -2,0 +3,2 @@\n+3\n+4\n"
        );
        assert_eq!(patch("1\n2\n3\n", "1\n3\n", 0), "--- old\n+++ new\n@@ -2 +1,0 @@\n-2\n");
    }

    #[test]
    fn a_line_without_a_newline_is_marked() {
        assert_eq!(
            patch("1\n2", "1\n3", 1),
            "--- old\n+++ new\n@@ -1,2 +1,2 @@\n 1\n-2\n\\ No newline at end of file\n+3\n\\ No newline at end of file\n"
        );
        assert_eq!(
            patch("1\n2\n", "1\n2", 0),
            "--- old\n+++ new\n@@ -2 +2 @@\n-2\n+2\n\\ No newline at end of file\n"
        );
    }

    #[test]
    fn crlf_lines_keep_their_endings() {
        assert_eq!(
            patch("a\r\nb\r\n", "a\r\nc\r\n", 1),
            "--- old\n+++ new\n@@ -1,2 +1,2 @@\n a\r\n-b\r\n+c\r\n"
        );
    }

    #[test]
    fn the_header_names_the_nearest_line_above_that_starts_a_name() {
        let left = "fn f() {\n  1\n  2\n}\n_g\n  3\n  4\n  5\n";
        let right = left.replace("  5\n", "  five\n");
        // The hunk starts at line 6 ("  3"); line 5 ("_g") is the nearest name above it.
        assert_eq!(
            patch(left, &right, 2),
            "--- old\n+++ new\n@@ -6,3 +6,3 @@ _g\n   3\n   4\n-  5\n+  five\n"
        );
        let right = left.replace("  2\n", "  two\n");
        assert_eq!(
            patch(left, &right, 0),
            "--- old\n+++ new\n@@ -3 +3 @@ fn f() {\n-  2\n+  two\n"
        );
    }

    #[test]
    fn a_name_is_cut_to_80_bytes_on_a_character_boundary_then_trimmed() {
        // 79 ASCII bytes, then `é` (2 bytes) across the cut at 80.
        let long = format!("{}é tail\n", "a".repeat(79));
        let left = format!("{long}x\n");
        let right = format!("{long}y\n");
        let expected = format!("--- old\n+++ new\n@@ -2 +2 @@ {}\n-x\n+y\n", "a".repeat(79));
        assert_eq!(patch(&left, &right, 0), expected);
        // A name that ends in spaces within the cut loses them.
        assert_eq!(
            patch("$v  \r\nx\n", "$v  \r\ny\n", 0),
            "--- old\n+++ new\n@@ -2 +2 @@ $v\n-x\n+y\n"
        );
    }

    #[test]
    fn a_line_starting_with_another_letter_names_no_function() {
        // `é` is no ASCII letter, and an indented line starts with a space.
        assert_eq!(patch("é\n x\n1\n", "é\n x\n2\n", 0), "--- old\n+++ new\n@@ -3 +3 @@\n-1\n+2\n");
    }

    #[test]
    fn a_region_scope_holds_that_change_alone() {
        let right = numbered().replace("3\n", "c\n").replace("6\n", "f\n");
        let (l, r) = (text(&numbered()), text(&right));
        let found = regions(&l, &r, &DiffOptions::default());
        // Regions: 1–2 unchanged, 3 changed, 4–5 unchanged, 6 changed, 7–10 unchanged.
        assert_eq!(
            unified(&l, &r, &found, DEFAULT_CONTEXT, Scope::Region(RegionIx::new(3)), TITLES),
            "--- old\n+++ new\n@@ -4,6 +4,6 @@\n 4\n 5\n-6\n+f\n 7\n 8\n 9\n"
        );
    }

    #[test]
    fn a_region_scope_on_an_unchanged_or_missing_region_is_empty() {
        let right = numbered().replace("3\n", "c\n");
        let (l, r) = (text(&numbered()), text(&right));
        let found = regions(&l, &r, &DiffOptions::default());
        assert_eq!(unified(&l, &r, &found, 1, Scope::Region(RegionIx::new(0)), TITLES), "");
        assert_eq!(
            unified(&l, &r, &found, 1, Scope::Region(RegionIx::new(found.len())), TITLES),
            ""
        );
    }

    #[test]
    fn context_lines_print_from_the_right_under_a_whitespace_mode() {
        let all = DiffOptions { whitespace: Whitespace::All, ..DiffOptions::default() };
        assert_eq!(
            patch_with("a \nb\nc\nd\n", "a\nb\nC\nd \n", &all, DEFAULT_CONTEXT, Scope::File),
            "--- old\n+++ new\n@@ -1,4 +1,4 @@\n a\n b\n-c\n+C\n d \n"
        );
    }

    #[test]
    fn an_ignored_change_starts_no_hunk_and_prints_as_a_change_inside_one() {
        let blank = DiffOptions { blank_lines: BlankLineRule::Ignore, ..DiffOptions::default() };
        // Only a blank line added: no difference.
        assert_eq!(patch_with("1\n2\n", "1\n\n2\n", &blank, 1, Scope::File), "");
        // A blank line added between two changes: 1 + 1 + 1 lines lie between them, which
        // context 2 reaches across and context 1 does not.
        let (left, right) = ("1\n2\n3\n4\n", "one\n2\n\n3\nfour\n");
        assert_eq!(
            patch_with(left, right, &blank, 2, Scope::File),
            "--- old\n+++ new\n@@ -1,4 +1,5 @@\n-1\n+one\n 2\n+\n 3\n-4\n+four\n"
        );
        // Context stops at the ignored change on either side.
        assert_eq!(
            patch_with(left, right, &blank, 1, Scope::File),
            "--- old\n+++ new\n@@ -1,2 +1,2 @@\n-1\n+one\n 2\n@@ -3,2 +4,2 @@\n 3\n-4\n+four\n"
        );
    }
}
