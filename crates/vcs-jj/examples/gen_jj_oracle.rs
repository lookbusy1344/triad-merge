//! Writes `fixtures/oracle/jj-corpus.jsonl` from jj-lib's file merge (`PLAN.md`
//! §B5.2). Dev-time only; `script/gen-jj-oracle` runs it.
//!
//! Each case is a triple of the git corpus or a hand seed, merged by
//! `jj_lib::files::merge_hunks` under each combination of `same_change` and `hunk_level`. A
//! case records jj's hunks: resolved text, and the terms of each conflict.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use jj_lib::files::{FileMergeHunkLevel, MergeOptions, MergeResult, merge_hunks};
use jj_lib::merge::{Merge, SameChange};
use serde::{Deserialize, Serialize};

const MODES: [(&str, SameChange, FileMergeHunkLevel); 4] = [
    ("accept-line", SameChange::Accept, FileMergeHunkLevel::Line),
    ("accept-word", SameChange::Accept, FileMergeHunkLevel::Word),
    ("keep-line", SameChange::Keep, FileMergeHunkLevel::Line),
    ("keep-word", SameChange::Keep, FileMergeHunkLevel::Word),
];
/// jj stops anchoring on lines that occur more often than this (`jj-core` `diff.rs`).
const MAX_OCCURRENCES: usize = 100;

#[derive(Deserialize)]
struct GitCase {
    id: String,
    base: String,
    left: String,
    right: String,
}

#[derive(Serialize)]
struct Case {
    id: String,
    sides: Vec<String>,
    bases: Vec<String>,
    merges: BTreeMap<&'static str, Outcome>,
}

#[derive(Serialize)]
struct Outcome {
    conflicts: usize,
    hunks: Vec<Hunk>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Hunk {
    Resolved(String),
    Conflict { sides: Vec<String>, bases: Vec<String> },
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("jj splits UTF-8 input only at ASCII bytes")
}

fn hunk(merge: &Merge<impl AsRef<[u8]>>) -> Hunk {
    match merge.as_resolved() {
        Some(resolved) => Hunk::Resolved(text(resolved.as_ref())),
        None => Hunk::Conflict {
            sides: merge.adds().map(|t| text(t.as_ref())).collect(),
            bases: merge.removes().map(|t| text(t.as_ref())).collect(),
        },
    }
}

fn outcome(
    sides: &[String],
    bases: &[String],
    same_change: SameChange,
    level: FileMergeHunkLevel,
) -> Outcome {
    let inputs = Merge::from_removes_adds(bases, sides);
    let options = MergeOptions { hunk_level: level, same_change };
    let hunks: Vec<Hunk> = match merge_hunks(&inputs, &options) {
        MergeResult::Resolved(resolved) => vec![Hunk::Resolved(text(&resolved))],
        MergeResult::Conflict(hunks) => hunks.iter().map(hunk).collect(),
    };
    let conflicts = hunks.iter().filter(|h| matches!(h, Hunk::Conflict { .. })).count();
    Outcome { conflicts, hunks }
}

fn case(id: String, sides: Vec<String>, bases: Vec<String>) -> Case {
    let merges = MODES
        .iter()
        .map(|&(name, same_change, level)| (name, outcome(&sides, &bases, same_change, level)))
        .collect();
    Case { id, sides, bases, merges }
}

fn owned(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|&t| t.to_owned()).collect()
}

/// A three-way seed: base, left, right.
fn triple(name: &str, base: &str, left: &str, right: &str) -> Case {
    case(format!("jj-{name}"), owned(&[left, right]), owned(&[base]))
}

/// `x` and `y` lines, `count` of each, with `z` added at both ends on left and the last `y`
/// changed on right. Up to `MAX_OCCURRENCES` they anchor; beyond it jj trims only the common
/// prefix and suffix.
fn repeated(name: &str, count: usize) -> Case {
    let base = "x\n".repeat(count) + &"y\n".repeat(count);
    let left = format!("z\n{base}z\n");
    let right = "x\n".repeat(count) + &"y\n".repeat(count - 1) + "Y\n";
    triple(name, &base, &left, &right)
}

/// Seeds where jj's merge differs from git's or has no git counterpart (PLAN.md §B5.2).
fn seeds() -> Vec<Case> {
    const ABC: &str = "a\nb\nc\n";
    vec![
        triple("same-change", ABC, "a\nX\nc\n", "a\nX\nc\n"),
        triple("touching-changes", "a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nb\nC\nd\n"),
        triple(
            "braces-insert-block",
            "f {\n}\ng {\n}\n",
            "f {\n}\nh {\n}\ng {\n}\n",
            "f {\n}\ng {\n  y\n}\n",
        ),
        triple("blank-lines", "a\n\nb\n\nc\n", "a\n\nB\n\nc\n", "a\n\nb\n\n\nc\n"),
        triple(
            "braces-repeated",
            "{\n}\n{\n}\n{\n}\n",
            "{\n}\n{\nx\n}\n{\n}\n",
            "{\n}\n{\n}\n{\ny\n}\n",
        ),
        triple("word-resolvable", "let x = 1;\n", "let y = 1;\n", "let x = 2;\n"),
        triple("word-partial", "let x = 1;\n", "let y = 1;\n", "let z = 2;\n"),
        triple("word-utf8", "naïve x\n", "naïve y\n", "naïf x\n"),
        triple("word-same-change", "let x = 1;\n", "let y = 1;\n", "let y = 1;\n"),
        // Both sides change `1` to `2`; left also changes `x` to `y`.
        triple("word-same-word", "x = 1;\n", "y = 2;\n", "x = 2;\n"),
        triple("word-two-lines", "a b\nc d\n", "A b\nc d\n", "a b\nc D\n"),
        triple("word-spaces", "f(a, b)\n", "f(a,  B)\n", "g(a, b)\n"),
        repeated("repeated-anchor-limit", MAX_OCCURRENCES),
        repeated("repeated-over-limit", MAX_OCCURRENCES + 1),
        case("jj-nway-resolves".to_owned(), owned(&["a\nX\nc\n", ABC, ABC]), owned(&[ABC, ABC])),
        case(
            "jj-nway-conflict".to_owned(),
            owned(&["a\nX\nc\n", "a\nY\nc\n", ABC]),
            owned(&[ABC, ABC]),
        ),
        case(
            "jj-nway-same-change".to_owned(),
            owned(&["a\nX\nc\n", "a\nX\nc\n", "a\nX\nc\n"]),
            owned(&[ABC, ABC]),
        ),
        // jj's same-change rule with more sides: Y cancels, two X outnumber base b by one.
        case(
            "jj-nway-same-change-cancelled".to_owned(),
            owned(&["a\nX\nc\n", "a\nX\nc\n", "a\nY\nc\n"]),
            owned(&["a\nY\nc\n", ABC]),
        ),
        case(
            "jj-nway-four-sides".to_owned(),
            owned(&["a\nX\nc\n", ABC, ABC, ABC]),
            owned(&[ABC, ABC, ABC]),
        ),
    ]
}

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/oracle");
    let git = fs::read_to_string(dir.join("corpus.jsonl")).expect("the git corpus exists");
    let from_git = git.lines().map(|line| {
        let c: GitCase = serde_json::from_str(line).expect("git corpus line parses");
        case(format!("git-{}", c.id), vec![c.left, c.right], vec![c.base])
    });
    let lines: String = from_git
        .chain(seeds())
        .map(|c| serde_json::to_string(&c).expect("a case serialises") + "\n")
        .collect();
    fs::write(dir.join("jj-corpus.jsonl"), lines).expect("the fixtures directory takes the corpus");
}
