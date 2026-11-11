//! `--headless --commands FILE`: apply a file of merge commands before writing the output
//! (spec §6.1). The file is JSON Lines, one [`Command`] per line, in the recovery journal's
//! encoding (`docs/json-schema.md`).

use std::path::Path;

use merge_core::document::{Command, CommandError, MergeDocument};

use crate::SessionError;

/// A command file line that does not parse or does not apply. Lines count from 1.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("line {line} is not a merge command: {source}")]
    Parse {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("line {line} does not apply: {source}")]
    Apply {
        line: usize,
        #[source]
        source: CommandError,
    },
}

/// Apply the commands in the file at `path` to `document`, in order.
///
/// # Errors
///
/// [`SessionError::Read`] when the file cannot be read, [`SessionError::Commands`] for the first line
/// that does not parse or apply. The commands before it stay applied.
pub fn apply_file(document: &mut MergeDocument, path: &Path) -> Result<(), SessionError> {
    let bytes = std::fs::read(path)
        .map_err(|source| SessionError::Read { path: path.to_owned(), source })?;
    apply(document, &bytes)
        .map_err(|source| SessionError::Commands { path: path.to_owned(), source })
}

/// Apply each line of `lines` as a [`Command`]. Lines of only whitespace are skipped.
///
/// # Errors
///
/// [`ReplayError`] for the first line that does not parse or apply.
pub fn apply(document: &mut MergeDocument, lines: &[u8]) -> Result<(), ReplayError> {
    lines
        .split(|&b| b == LINE_END)
        .zip(FIRST_LINE..)
        .filter(|(json, _)| !json.trim_ascii().is_empty())
        .try_for_each(|(json, line)| {
            let command: Command = serde_json::from_slice(json)
                .map_err(|source| ReplayError::Parse { line, source })?;
            document.apply(command).map(drop).map_err(|source| ReplayError::Apply { line, source })
        })
}

const LINE_END: u8 = b'\n';
/// Error messages count lines from 1, as editors do.
const FIRST_LINE: usize = 1;

#[cfg(test)]
mod tests {
    use merge_core::document::Output;
    use merge_core::index::RegionIx;
    use merge_core::merge3::Source;
    use merge_core::testing::doc;

    use super::*;

    /// Regions: `a` unchanged, the L/R conflict, `c` unchanged.
    fn conflict() -> MergeDocument {
        doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n")
    }

    fn applied(lines: &str) -> Output {
        let mut doc = conflict();
        apply(&mut doc, lines.as_bytes()).expect("the commands apply");
        doc.output()
    }

    #[test]
    fn a_pick_resolves_the_conflict() {
        assert_eq!(
            applied(r#"{"Pick":{"region":1,"sources":[{"Side":1}]}}"#),
            Output::Resolved("a\nR\nc\n".to_owned())
        );
    }

    #[test]
    fn a_pick_keeps_the_order_given() {
        assert_eq!(
            applied(r#"{"Pick":{"region":1,"sources":[{"Side":1},{"Side":0}]}}"#),
            Output::Resolved("a\nR\nL\nc\n".to_owned())
        );
    }

    #[test]
    fn commands_apply_in_order_one_per_line() {
        let lines = concat!(
            r#"{"Toggle":{"region":1,"source":{"Base":0}}}"#,
            "\n",
            r#"{"Toggle":{"region":1,"source":{"Side":0}}}"#,
            "\n",
            "\"Undo\"\n",
        );
        assert_eq!(applied(lines), Output::Resolved("a\nb\nc\n".to_owned()));
    }

    #[test]
    fn undo_back_to_the_start_leaves_the_conflict() {
        let lines = concat!(r#"{"Clear":{"region":1}}"#, "\n\"Undo\"\n");
        assert_eq!(
            applied(lines),
            Output::Conflicted("a\n<<<<<<<\nL\n|||||||\nb\n=======\nR\n>>>>>>>\nc\n".to_owned())
        );
    }

    #[test]
    fn redo_repeats_the_undone_command() {
        let lines = concat!(r#"{"Clear":{"region":1}}"#, "\n\"Undo\"\n\"Redo\"\n");
        assert_eq!(applied(lines), Output::Resolved("a\nc\n".to_owned()));
    }

    #[test]
    fn an_edit_patches_the_result_text_by_byte() {
        let mut doc = doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n");
        let edit = r#"{"Edit":[{"range":{"start":2,"end":3},"text":"B"}]}"#;
        apply(&mut doc, edit.as_bytes()).expect("the edit applies");
        assert_eq!(doc.output(), Output::Resolved("A\nB\nC\n".to_owned()));
    }

    #[test]
    fn blank_lines_and_crlf_line_ends_are_accepted() {
        let lines = concat!("\r\n", r#"{"Pick":{"region":1,"sources":[{"Side":0}]}}"#, "\r\n  \n");
        assert_eq!(applied(lines), Output::Resolved("a\nL\nc\n".to_owned()));
    }

    #[test]
    fn no_commands_leave_the_automatic_merge() {
        assert_eq!(applied(""), conflict().output());
    }

    #[test]
    fn a_line_that_is_not_a_command_names_its_line() {
        let mut doc = conflict();
        let lines = concat!("\n", r#"{"Clear":{"region":1}}"#, "\n", r#"{"Take":1}"#, "\n");
        let error = apply(&mut doc, lines.as_bytes()).expect_err("Take is no command");
        assert!(matches!(error, ReplayError::Parse { line: 3, .. }), "{error:?}");
        assert!(error.to_string().starts_with("line 3 is not a merge command: "), "{error}");
    }

    #[test]
    fn invalid_utf8_on_a_line_is_a_parse_error() {
        let mut doc = conflict();
        let error = apply(&mut doc, b"\"Undo\xff\"\n").expect_err("not UTF-8");
        assert!(matches!(error, ReplayError::Parse { line: 1, .. }), "{error:?}");
    }

    #[test]
    fn a_command_that_does_not_apply_names_its_line_and_cause() {
        let mut doc = conflict();
        let lines = concat!("\"Undo\"\n", r#"{"Clear":{"region":3}}"#, "\n");
        let error = apply(&mut doc, lines.as_bytes()).expect_err("three regions");
        assert!(
            matches!(
                error,
                ReplayError::Apply {
                    line: 2,
                    source: CommandError::NoRegion { region, regions: 3 }
                } if region == RegionIx::new(3)
            ),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            "line 2 does not apply: no region 3: the merge has 3 regions"
        );
    }

    #[test]
    fn a_source_the_merge_lacks_does_not_apply() {
        let mut doc = conflict();
        let pick = r#"{"Pick":{"region":1,"sources":[{"Side":2}]}}"#;
        let error = apply(&mut doc, pick.as_bytes()).expect_err("two sides");
        assert!(
            matches!(
                error,
                ReplayError::Apply {
                    line: 1,
                    source: CommandError::NoInput { input: Source::Side(2) }
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn processing_stops_at_the_first_bad_line() {
        let mut doc = conflict();
        let lines = concat!(
            r#"{"Clear":{"region":9}}"#,
            "\n",
            r#"{"Pick":{"region":1,"sources":[{"Side":0}]}}"#,
            "\n"
        );
        apply(&mut doc, lines.as_bytes()).expect_err("region 9");
        assert_eq!(doc.output(), conflict().output());
    }
}
