//! triad's conflict-marker parser against jj-lib's (`PLAN.md` §D5.7): jj
//! materialises seeded merges in each of its styles, and triad's parse of the bytes gives
//! the texts and labels jj's own parse and labels give.

#![expect(clippy::panic, reason = "test helpers outside `#[test]` panic on a failed check")]

use jj_lib::conflict_labels::ConflictLabels;
use jj_lib::conflicts::{self, ConflictMarkerStyle, ConflictMaterializeOptions};
use jj_lib::files::{FileMergeHunkLevel, MergeOptions};
use jj_lib::merge::{Merge, SameChange};
use merge_core::document::Labels;
use merge_core::markers;
use merge_core::text::Text;

/// Merges per side count and line ending.
const CASES: usize = 150;
const SEED: u64 = 0x5EED_1234_ABCD_0001;
const MAX_LINES: usize = 8;
/// Lines the generator draws from: plain text, marker-like lines jj must step around, and
/// lines a jj diff could misread.
const LINES: [&str; 10] =
    ["a", "b", "c", "d", "<<<<<<< x", "=======", "-------", "+++ y", " z", ""];
const STYLES: [ConflictMarkerStyle; 4] = [
    ConflictMarkerStyle::Diff,
    ConflictMarkerStyle::DiffExperimental,
    ConflictMarkerStyle::Snapshot,
    ConflictMarkerStyle::Git,
];

/// xorshift64: a fixed seed gives the same cases on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("below n")
    }
}

fn file(rng: &mut Rng, eol: &str) -> String {
    let lines = rng.below(MAX_LINES) + 1;
    let mut text = String::new();
    for _ in 0..lines {
        text.push_str(LINES[rng.below(LINES.len())]);
        text.push_str(eol);
    }
    // One file in four ends without a newline.
    if rng.below(4) == 0 {
        text.truncate(text.len() - eol.len());
    }
    text
}

/// A side: the base with some lines changed, dropped or added.
fn edit(rng: &mut Rng, base: &str, eol: &str) -> String {
    base.split_inclusive('\n')
        .flat_map(|line| match rng.below(4) {
            0 => vec![format!("{}{eol}", LINES[rng.below(LINES.len())])],
            1 => vec![],
            2 => vec![line.to_owned(), format!("{}{eol}", LINES[rng.below(LINES.len())])],
            _ => vec![line.to_owned()],
        })
        .collect()
}

/// jj's parse of `bytes` as whole texts: sides then bases.
fn jj_texts(bytes: &[u8], sides: usize, marker_len: usize) -> (Vec<String>, Vec<String>) {
    let hunks = conflicts::parse_conflict(bytes, sides, marker_len).expect("jj parses its own");
    let mut out_sides = vec![String::new(); sides];
    let mut out_bases = vec![String::new(); sides - 1];
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).expect("UTF-8");
    for hunk in &hunks {
        if let Some(resolved) = hunk.as_resolved() {
            for t in out_sides.iter_mut().chain(out_bases.iter_mut()) {
                t.push_str(&text(resolved));
            }
        } else {
            for (t, add) in out_sides.iter_mut().zip(hunk.adds()) {
                t.push_str(&text(add));
            }
            for (t, remove) in out_bases.iter_mut().zip(hunk.removes()) {
                t.push_str(&text(remove));
            }
        }
    }
    (out_sides, out_bases)
}

fn contents(texts: &[Text]) -> Vec<String> {
    texts.iter().map(|t| t.content().to_owned()).collect()
}

/// The labels triad should read: jj's own labels, or none, as panes title them.
fn expected_labels(given: Option<&[String]>, sides: usize) -> Labels {
    match given {
        Some(labels) => Labels::new(
            labels.iter().step_by(2).cloned().collect(),
            labels.iter().skip(1).step_by(2).cloned().collect(),
        ),
        None => Labels::new(vec![String::new(); sides], vec![String::new(); sides - 1]),
    }
}

fn check(sides: usize, eol: &str) {
    let mut rng = Rng(SEED ^ (sides as u64) ^ (eol.len() as u64) << 8);
    let mut checked = 0;
    for case in 0..CASES {
        let bases: Vec<String> = (0..sides - 1).map(|_| file(&mut rng, eol)).collect();
        let adds: Vec<String> = (0..sides)
            .map(|i| edit(&mut rng, &bases[i.saturating_sub(1).min(bases.len() - 1)], eol))
            .collect();
        let merge = Merge::from_removes_adds(bases.clone(), adds.clone());
        let given: Vec<String> =
            (0..2 * sides - 1).map(|i| format!("term {i} \"label\"")).collect();
        for (labelled, style) in [false, true].into_iter().flat_map(|l| STYLES.map(|s| (l, s))) {
            let labels = if labelled {
                ConflictLabels::from_vec(given.clone())
            } else {
                ConflictLabels::unlabeled()
            };
            let options = ConflictMaterializeOptions {
                marker_style: style,
                marker_len: None,
                merge: MergeOptions {
                    hunk_level: FileMergeHunkLevel::Line,
                    same_change: SameChange::Accept,
                },
            };
            let bytes = conflicts::materialize_merge_result_to_bytes(&merge, &labels, &options);
            let marker_len = conflicts::choose_materialized_conflict_marker_len(&merge);
            if conflicts::parse_conflict(&bytes, sides, marker_len).is_none() {
                continue; // resolved cleanly: no markers to read
            }
            let context = || {
                format!(
                    "case {case}, {style:?}, labelled {labelled}:\n{}",
                    String::from_utf8_lossy(&bytes)
                )
            };
            let parsed = markers::parse(&Text::decode(bytes.to_vec()).expect("text"))
                .unwrap_or_else(|e| panic!("{e}\n{}", context()));
            let (jj_sides, jj_bases) = jj_texts(&bytes, sides, marker_len);
            assert_eq!(contents(parsed.inputs.sides()), jj_sides, "sides, {}", context());
            assert_eq!(contents(parsed.inputs.bases()), jj_bases, "bases, {}", context());
            assert_eq!(parsed.marker_length, marker_len, "{}", context());
            assert_eq!(
                parsed.labels,
                expected_labels(labelled.then_some(given.as_slice()), sides),
                "labels, {}",
                context()
            );
            checked += 1;
        }
    }
    assert!(checked >= CASES, "only {checked} conflicted materialisations");
}

#[test]
fn two_sides_lf() {
    check(2, "\n");
}

#[test]
fn two_sides_crlf() {
    check(2, "\r\n");
}

#[test]
fn three_sides_lf() {
    check(3, "\n");
}

#[test]
fn four_sides_crlf() {
    check(4, "\r\n");
}
