//! `inspect` and `diff --inspect`: print regions as text or as the JSON of
//! `docs/json-schema.md`.

use std::ops::Range;
use std::path::Path;

use merge_core::diff::DiffOptions;
use merge_core::diff2::{self, Kind2};
use merge_core::document::{Labels, MergeOptions, OutputEnding};
use merge_core::merge3::{Kind, LEFT, RIGHT, word_merge};
use merge_core::text::{Encoding, output_encoding};
use serde::Serialize;

use merge_session::invocation::MergeFiles;
use merge_session::semantics::Mode;
use merge_session::{load, read_text};
use merge_syntax::language::Language;

use crate::{CliError, ExitCode};

/// The version in every JSON object. `docs/json-schema.md` says when it changes.
const SCHEMA: u32 = 1;
const ENCODING_MISMATCH: &str = "encoding-mismatch";

#[derive(Serialize)]
struct MergeReport {
    schema: u32,
    command: &'static str,
    encodings: MergeEncodings,
    output_encoding: &'static str,
    /// The saved merge's line endings: `lf`, `crlf`, `mixed` or `none`.
    line_endings: &'static str,
    warnings: Vec<&'static str>,
    conflicts: usize,
    regions: Vec<MergeRegion>,
}

#[derive(Serialize)]
struct MergeEncodings {
    base: &'static str,
    left: &'static str,
    right: &'static str,
}

#[derive(Serialize)]
struct MergeRegion {
    kind: &'static str,
    base: [usize; 2],
    left: [usize; 2],
    right: [usize; 2],
    /// A word-merged region's text.
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Serialize)]
struct DiffReport {
    schema: u32,
    command: &'static str,
    encodings: DiffEncodings,
    warnings: Vec<&'static str>,
    changes: usize,
    regions: Vec<DiffRegion>,
}

#[derive(Serialize)]
struct DiffEncodings {
    left: &'static str,
    right: &'static str,
}

#[derive(Serialize)]
struct DiffRegion {
    kind: &'static str,
    left: [usize; 2],
    right: [usize; 2],
}

/// The merge's regions under `options` and the semantics of `mode`, and the exit code: 0
/// without conflicts, 1 with. The regions are the ones a headless merge with the same
/// options numbers its commands by: syntax boundaries included, in the language the inputs'
/// names give.
///
/// # Errors
///
/// [`CliError`] when an input cannot be read or decoded.
pub fn inspect(
    files: &MergeFiles,
    options: MergeOptions,
    mode: Mode,
    ending: OutputEnding,
    json: bool,
) -> Result<(String, ExitCode), CliError> {
    let options = MergeOptions {
        semantics: mode.semantics,
        same_change: mode.same_change,
        hunk_level: mode.hunk_level,
        ..options
    };
    let language = Language::of_run(files.paths(), None, None);
    let document = load(files, options, Labels::none(), language)?;
    let inputs = document.inputs();
    let (base, left, right) = (&inputs.bases()[0], &inputs.sides()[LEFT], &inputs.sides()[RIGHT]);
    let encodings = [base.encoding(), left.encoding(), right.encoding()];
    let output = output_encoding(base, left);
    let regions: Vec<MergeRegion> = document
        .regions()
        .iter()
        .map(|r| MergeRegion {
            kind: merge_kind(r.kind),
            base: span(&r.bases[0]),
            left: span(&r.sides[LEFT]),
            right: span(&r.sides[RIGHT]),
            text: (r.kind == Kind::WordMerged)
                .then(|| word_merge(document.inputs(), r, mode.same_change))
                .flatten(),
        })
        .collect();
    let report = MergeReport {
        schema: SCHEMA,
        command: "inspect",
        encodings: MergeEncodings {
            base: encoding_name(encodings[0]),
            left: encoding_name(encodings[1]),
            right: encoding_name(encodings[2]),
        },
        output_encoding: encoding_name(output),
        line_endings: document.saved_endings(ending).name(),
        warnings: warnings(&encodings),
        conflicts: regions.iter().filter(|r| r.kind == CONFLICT).count(),
        regions,
    };
    let code = exit_code(report.conflicts);
    let text = if json {
        to_json(&report)?
    } else {
        let lines = report.regions.iter().map(|r| {
            format!(
                "{} base {} left {} right {}",
                r.kind,
                dots(r.base),
                dots(r.left),
                dots(r.right)
            )
        });
        to_text(&report.warnings, lines, &format!("conflicts: {}", report.conflicts))
    };
    Ok((text, code))
}

/// The two-way regions and the exit code: 0 if the files are equal under the whitespace mode,
/// 1 if they differ.
///
/// # Errors
///
/// [`CliError`] when an input cannot be read or decoded.
pub fn diff_inspect(
    left: &Path,
    right: &Path,
    diff: &DiffOptions,
    json: bool,
) -> Result<(String, ExitCode), CliError> {
    let left = read_text(left)?;
    let right = read_text(right)?;
    let encodings = [left.encoding(), right.encoding()];
    let regions: Vec<DiffRegion> = diff2::regions(&left, &right, diff)
        .iter()
        .map(|r| DiffRegion {
            kind: match r.kind {
                Kind2::Unchanged => "unchanged",
                Kind2::Changed => "changed",
            },
            left: span(&r.left),
            right: span(&r.right),
        })
        .collect();
    let report = DiffReport {
        schema: SCHEMA,
        command: "diff",
        encodings: DiffEncodings {
            left: encoding_name(encodings[0]),
            right: encoding_name(encodings[1]),
        },
        warnings: warnings(&encodings),
        changes: regions.iter().filter(|r| r.kind == "changed").count(),
        regions,
    };
    let code = exit_code(report.changes);
    let text = if json {
        to_json(&report)?
    } else {
        let lines = report
            .regions
            .iter()
            .map(|r| format!("{} left {} right {}", r.kind, dots(r.left), dots(r.right)));
        to_text(&report.warnings, lines, &format!("changes: {}", report.changes))
    };
    Ok((text, code))
}

const CONFLICT: &str = "conflict";

fn merge_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Unchanged => "unchanged",
        Kind::OneSide(LEFT) => "left-only",
        // A three-way merge has no other side.
        Kind::OneSide(_) => "right-only",
        Kind::AllSame => "both-same",
        Kind::WordMerged => "word-merged",
        Kind::Conflict => CONFLICT,
    }
}

fn encoding_name(encoding: Encoding) -> &'static str {
    match encoding {
        Encoding::Utf8 => "utf-8",
        Encoding::Utf8Bom => "utf-8-bom",
        Encoding::Utf16Le => "utf-16le",
        Encoding::Utf16Be => "utf-16be",
    }
}

fn warnings(encodings: &[Encoding]) -> Vec<&'static str> {
    let mixed = encodings.iter().any(|&e| e != encodings[0]);
    mixed.then_some(ENCODING_MISMATCH).into_iter().collect()
}

/// 1 when anything differs or conflicts, as in `diff(1)`.
fn exit_code(count: usize) -> ExitCode {
    if count == 0 { ExitCode::Resolved } else { ExitCode::Conflicts }
}

fn span(lines: &Range<usize>) -> [usize; 2] {
    [lines.start, lines.end]
}

fn dots([start, end]: [usize; 2]) -> String {
    format!("{start}..{end}")
}

fn to_json(report: &impl Serialize) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report).map(|json| json + "\n")
}

fn to_text(warnings: &[&str], lines: impl Iterator<Item = String>, total: &str) -> String {
    warnings
        .iter()
        .map(|w| format!("warning: {w}"))
        .chain(lines)
        .chain(std::iter::once(total.to_owned()))
        .map(|line| line + "\n")
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use merge_core::diff::Whitespace;
    use serde_json::{Value, json};

    use super::*;

    /// git semantics, the same change accepted, line by line.
    const GIT: Mode = Mode {
        semantics: merge_core::merge3::Semantics::Git,
        same_change: merge_core::merge3::SameChange::Accept,
        hunk_level: merge_core::merge3::HunkLevel::Line,
    };

    const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];

    struct Files {
        dir: tempfile::TempDir,
    }

    impl Files {
        fn new(files: &[(&str, &[u8])]) -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            for (name, bytes) in files {
                fs::write(dir.path().join(name), bytes).expect("input");
            }
            Self { dir }
        }

        fn path(&self, name: &str) -> PathBuf {
            self.dir.path().join(name)
        }

        fn merge(&self) -> MergeFiles {
            MergeFiles {
                base: self.path("base"),
                left: self.path("left"),
                right: self.path("right"),
            }
        }
    }

    fn three(base: &[u8], left: &[u8], right: &[u8]) -> Files {
        Files::new(&[("base", base), ("left", left), ("right", right)])
    }

    fn utf16le(s: &str) -> Vec<u8> {
        UTF16LE_BOM.into_iter().chain(s.encode_utf16().flat_map(u16::to_le_bytes)).collect()
    }

    fn json_of(output: &str) -> Value {
        assert!(output.ends_with('\n'), "{output:?}");
        serde_json::from_str(output).expect("valid JSON")
    }

    fn conflict() -> Files {
        three(b"a\nb\nc\n", b"a\nL\nc\n", b"a\nR\nc\n")
    }

    const KEEP: OutputEnding = OutputEnding::Keep;

    #[test]
    fn inspect_json_gives_the_saved_line_endings() {
        let files = three(b"a\r\nb\r\n", b"a\r\nB\n", b"a\r\nb\r\n");
        let endings = |ending| {
            let (out, _) =
                inspect(&files.merge(), MergeOptions::default(), GIT, ending, true).expect("ok");
            json_of(&out)["line_endings"].clone()
        };
        assert_eq!(endings(KEEP), "mixed");
        assert_eq!(endings(OutputEnding::Lf), "lf");
        assert_eq!(endings(OutputEnding::Base), "crlf");
        let unbroken = three(b"a", b"a", b"a");
        let (out, _) =
            inspect(&unbroken.merge(), MergeOptions::default(), GIT, KEEP, true).expect("ok");
        assert_eq!(json_of(&out)["line_endings"], "none");
    }

    #[test]
    fn inspect_json_lists_regions_and_conflicts() {
        let (out, code) =
            inspect(&conflict().merge(), MergeOptions::default(), GIT, KEEP, true).expect("ok");
        assert_eq!(code, ExitCode::Conflicts);
        assert_eq!(
            json_of(&out),
            json!({
                "schema": 1,
                "command": "inspect",
                "encodings": { "base": "utf-8", "left": "utf-8", "right": "utf-8" },
                "output_encoding": "utf-8",
                "line_endings": "lf",
                "warnings": [],
                "conflicts": 1,
                "regions": [
                    { "kind": "unchanged", "base": [0, 1], "left": [0, 1], "right": [0, 1] },
                    { "kind": "conflict", "base": [1, 2], "left": [1, 2], "right": [1, 2] },
                    { "kind": "unchanged", "base": [2, 3], "left": [2, 3], "right": [2, 3] }
                ]
            })
        );
    }

    #[test]
    fn inspect_json_gives_a_word_merged_regions_text() {
        let files = three(b"a\nx = 1;\n", b"a\ny = 1;\n", b"a\nx = 2;\n");
        let by_word = Mode { hunk_level: merge_core::merge3::HunkLevel::Word, ..GIT };
        let (out, code) =
            inspect(&files.merge(), MergeOptions::default(), by_word, KEEP, true).expect("ok");
        assert_eq!(code, ExitCode::Resolved);
        assert_eq!(
            json_of(&out)["regions"],
            json!([
                { "kind": "unchanged", "base": [0, 1], "left": [0, 1], "right": [0, 1] },
                { "kind": "word-merged", "base": [1, 2], "left": [1, 2], "right": [1, 2],
                  "text": "y = 2;\n" }
            ])
        );
        let (line, code) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, true).expect("ok");
        assert_eq!(code, ExitCode::Conflicts);
        assert_eq!(json_of(&line)["regions"][1]["kind"], "conflict");
    }

    #[test]
    fn inspect_keeps_the_same_change_a_conflict_when_the_mode_says_so() {
        let files = three(b"a\nb\n", b"a\nX\n", b"a\nX\n");
        let keep = Mode { same_change: merge_core::merge3::SameChange::Keep, ..GIT };
        let (out, code) =
            inspect(&files.merge(), MergeOptions::default(), keep, KEEP, false).expect("ok");
        assert_eq!(code, ExitCode::Conflicts);
        assert_eq!(
            out,
            "unchanged base 0..1 left 0..1 right 0..1\n\
             conflict base 1..2 left 1..2 right 1..2\n\
             conflicts: 1\n"
        );
        let (_, code) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, false).expect("ok");
        assert_eq!(code, ExitCode::Resolved, "accepted by default");
    }

    #[test]
    fn inspect_text_prints_one_line_per_region() {
        let (out, code) =
            inspect(&conflict().merge(), MergeOptions::default(), GIT, KEEP, false).expect("ok");
        assert_eq!(code, ExitCode::Conflicts);
        assert_eq!(
            out,
            "unchanged base 0..1 left 0..1 right 0..1\n\
             conflict base 1..2 left 1..2 right 1..2\n\
             unchanged base 2..3 left 2..3 right 2..3\n\
             conflicts: 1\n"
        );
    }

    #[test]
    fn inspect_names_every_kind_and_exits_0_without_conflicts() {
        let files = three(b"a\nb\nc\nd\ne\n", b"A\nb\nC\nd\ne\n", b"a\nb\nC\nd\nE\n");
        let (out, code) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, false).expect("ok");
        assert_eq!(code, ExitCode::Resolved);
        assert_eq!(
            out,
            "left-only base 0..1 left 0..1 right 0..1\n\
             unchanged base 1..2 left 1..2 right 1..2\n\
             both-same base 2..3 left 2..3 right 2..3\n\
             unchanged base 3..4 left 3..4 right 3..4\n\
             right-only base 4..5 left 4..5 right 4..5\n\
             conflicts: 0\n"
        );
    }

    #[test]
    fn inspect_ranges_follow_each_input() {
        let files = three(b"a\nb\n", b"a\nX\nY\nb\n", b"a\nb\n");
        let (out, _) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, false).expect("ok");
        assert_eq!(
            out,
            "unchanged base 0..1 left 0..1 right 0..1\n\
             left-only base 1..1 left 1..3 right 1..1\n\
             unchanged base 1..2 left 3..4 right 1..2\n\
             conflicts: 0\n"
        );
    }

    #[test]
    fn inspect_uses_the_whitespace_mode() {
        // Adjacent changes conflict, unless left's change is whitespace only under the mode.
        let files = three(b"a\nb\n", b"a \nb\n", b"a\nB\n");
        let trailing = DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() };
        let (out, code) = inspect(
            &files.merge(),
            MergeOptions { diff: trailing, ..MergeOptions::default() },
            GIT,
            KEEP,
            false,
        )
        .expect("ok");
        assert_eq!(code, ExitCode::Resolved);
        assert_eq!(
            out,
            "unchanged base 0..1 left 0..1 right 0..1\n\
             right-only base 1..2 left 1..2 right 1..2\n\
             conflicts: 0\n"
        );
        let (_, code) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, false).expect("ok");
        assert_eq!(code, ExitCode::Conflicts);
    }

    #[test]
    fn mixed_encodings_give_a_warning() {
        let files = three(b"a\n", &utf16le("a\n"), b"\xEF\xBB\xBFa\n");
        let (out, code) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, true).expect("ok");
        assert_eq!(code, ExitCode::Resolved);
        let value = json_of(&out);
        assert_eq!(
            value["encodings"],
            json!({ "base": "utf-8", "left": "utf-16le", "right": "utf-8-bom" })
        );
        assert_eq!(value["output_encoding"], json!("utf-8"));
        assert_eq!(value["warnings"], json!(["encoding-mismatch"]));
        let (text, _) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, false).expect("ok");
        assert_eq!(
            text,
            "warning: encoding-mismatch\nunchanged base 0..1 left 0..1 right 0..1\nconflicts: 0\n"
        );
    }

    #[test]
    fn an_empty_base_takes_lefts_output_encoding() {
        let files = three(b"", &utf16le("x\n"), b"x\n");
        let (out, _) =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, true).expect("ok");
        assert_eq!(json_of(&out)["output_encoding"], json!("utf-16le"));
    }

    #[test]
    fn inspect_reports_a_bad_input() {
        let files = three(b"a\n", b"a\0\n", b"a\n");
        let error =
            inspect(&files.merge(), MergeOptions::default(), GIT, KEEP, true).expect_err("binary");
        assert_eq!(
            error.to_string(),
            format!(
                "{}: binary file: a NUL byte within the first 8000 bytes",
                files.path("left").display()
            )
        );
    }

    fn two(left: &[u8], right: &[u8]) -> Files {
        Files::new(&[("left", left), ("right", right)])
    }

    #[test]
    fn diff_json_lists_regions_and_changes() {
        let files = two(b"a\nb\nc\n", b"a\nB\nc\n");
        let (out, code) =
            diff_inspect(&files.path("left"), &files.path("right"), &DiffOptions::default(), true)
                .expect("ok");
        assert_eq!(code, ExitCode::Conflicts);
        assert_eq!(
            json_of(&out),
            json!({
                "schema": 1,
                "command": "diff",
                "encodings": { "left": "utf-8", "right": "utf-8" },
                "warnings": [],
                "changes": 1,
                "regions": [
                    { "kind": "unchanged", "left": [0, 1], "right": [0, 1] },
                    { "kind": "changed", "left": [1, 2], "right": [1, 2] },
                    { "kind": "unchanged", "left": [2, 3], "right": [2, 3] }
                ]
            })
        );
    }

    #[test]
    fn diff_text_and_equal_files_exit_0() {
        let files = two(b"a\n", b"x\na\n");
        let (out, code) =
            diff_inspect(&files.path("left"), &files.path("right"), &DiffOptions::default(), false)
                .expect("ok");
        assert_eq!(code, ExitCode::Conflicts);
        assert_eq!(
            out,
            "changed left 0..0 right 0..1\nunchanged left 0..1 right 1..2\nchanges: 1\n"
        );
        let equal = two(b"a \n", b"a\r\n");
        let trailing = DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() };
        let (out, code) =
            diff_inspect(&equal.path("left"), &equal.path("right"), &trailing, false).expect("ok");
        assert_eq!(code, ExitCode::Resolved);
        assert_eq!(out, "unchanged left 0..1 right 0..1\nchanges: 0\n");
    }

    #[test]
    fn diff_warns_on_mixed_encodings() {
        let files = two(b"a\n", &utf16le("a\n"));
        let (out, code) =
            diff_inspect(&files.path("left"), &files.path("right"), &DiffOptions::default(), true)
                .expect("ok");
        assert_eq!(code, ExitCode::Resolved);
        let value = json_of(&out);
        assert_eq!(value["encodings"], json!({ "left": "utf-8", "right": "utf-16le" }));
        assert_eq!(value["warnings"], json!(["encoding-mismatch"]));
    }
}
