//! Decoding and conflict-marker parsing on input triad did not write. These read files from
//! disk, so any bytes can reach them. Each failure is a typed error, never a panic.

use std::cell::Cell;

use merge_core::markers::{self, MarkerError};
use merge_core::text::{DecodeError, Encoding, Text};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed, TestRunner};

const PROPTEST_SEED: u64 = 0xBAD_F11E;
const BYTE_CASES: u32 = 4096;
const SOUP_CASES: u32 = 8192;
const MAX_BYTES: usize = 256;
const MAX_LINES: usize = 20;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16BE_BOM: [u8; 2] = [0xFE, 0xFF];
const BYTE_ORDER_MARK: char = '\u{FEFF}';
/// The shortest run of a marker character that counts as a marker.
const MIN_MARKER_LENGTH: usize = 7;
const START: char = '<';
/// Each outcome a soup run must reach many times, or it tests nothing.
const MIN_OUTCOMES: u32 = 1000;

fn config(cases: u32) -> Config {
    Config {
        cases,
        rng_seed: RngSeed::Fixed(PROPTEST_SEED),
        failure_persistence: None,
        ..Config::default()
    }
}

/// Bytes, often behind a byte order mark, where the decoder's branches start.
fn hostile_bytes() -> impl Strategy<Value = Vec<u8>> {
    (
        prop::sample::select(vec![&[][..], &UTF8_BOM[..], &UTF16LE_BOM[..], &UTF16BE_BOM[..]]),
        prop::collection::vec(any::<u8>(), 0..MAX_BYTES),
    )
        .prop_map(|(bom, rest)| [bom, &rest].concat())
}

/// A marker-like line: every marker character, at the usual length and around it, with text
/// after it, and the lines jj's diff style reads.
fn soup_line() -> impl Strategy<Value = String> {
    let lengths = prop::sample::select(vec![6_usize, 7, 7, 7, 8, 9]);
    let markers = prop::sample::select(vec!['<', '>', '|', '=', '+', '-', '%', '\\']);
    let tails = prop::sample::select(vec![
        "",
        " ",
        " x",
        "x",
        " side #1",
        " base",
        " conflict 1 of 1",
        " (no terminating newline)",
        " é",
    ]);
    let endings = prop::sample::select(vec!["\n", "\n", "\r\n", ""]);
    let bodies = prop::sample::select(vec!["a", " a", "-a", "+a", "", "é", "\r", "\t"]);
    prop_oneof![
        3 => (markers, lengths, tails, endings.clone())
            .prop_map(|(c, n, tail, end)| format!("{}{tail}{end}", c.to_string().repeat(n))),
        1 => (bodies, endings).prop_map(|(body, end)| format!("{body}{end}")),
    ]
}

/// Whole conflicts in git's and jj's styles, so that soup around them often parses.
fn whole_conflict() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "<<<<<<< a\nL\n=======\nR\n>>>>>>> b\n",
        "<<<<<<< a\nL\n||||||| c\nB\n=======\nR\n>>>>>>> b\n",
        "<<<<<<< x\n+++++++ a\nL\n------- c\nB\n+++++++ b\nR\n>>>>>>> x\n",
        "<<<<<<< x\n%%%%%%% a\n-B\n+L\n+++++++ b\nR\n>>>>>>> x\n",
    ])
    .prop_map(str::to_owned)
}

fn soup() -> impl Strategy<Value = String> {
    let piece = prop_oneof![4 => soup_line(), 1 => whole_conflict()];
    prop::collection::vec(piece, 0..MAX_LINES).prop_map(|lines| lines.concat())
}

/// Any text the decoder takes as UTF-8: no NUL, and no byte order mark to read as an encoding.
fn plain_text() -> impl Strategy<Value = String> {
    any::<String>().prop_map(|s| s.replace(['\0', BYTE_ORDER_MARK], ""))
}

/// The line an error names, when it names one.
fn error_line(error: &MarkerError) -> Option<usize> {
    match *error {
        MarkerError::NoMarkers => None,
        MarkerError::Unclosed { line }
        | MarkerError::Nested { line }
        | MarkerError::OutOfOrder { line }
        | MarkerError::NoSeparator { line }
        | MarkerError::BadDiffLine { line }
        | MarkerError::Shape { line, .. }
        | MarkerError::MixedArity { line, .. } => Some(line),
    }
}

proptest! {
    #![proptest_config(config(BYTE_CASES))]

    #[test]
    fn what_decodes_encodes_back_to_the_same_bytes(bytes in hostile_bytes()) {
        if let Ok(text) = Text::decode(bytes.clone()) {
            let encoded = text.encode(text.content());
            prop_assert_eq!(encoded.as_ref(), bytes.as_slice());
        }
    }

    #[test]
    fn a_utf8_error_names_the_first_invalid_byte(
        bytes in prop::collection::vec(1..=u8::MAX, 0..MAX_BYTES)
    ) {
        prop_assume!(!bytes.starts_with(&UTF8_BOM));
        prop_assume!(!bytes.starts_with(&UTF16LE_BOM) && !bytes.starts_with(&UTF16BE_BOM));
        let expected = std::str::from_utf8(&bytes)
            .err()
            .map(|e| DecodeError::InvalidUtf8 { offset: e.valid_up_to() });
        prop_assert_eq!(Text::decode(bytes).err(), expected);
    }

    #[test]
    fn utf16_of_any_text_decodes_to_it(content in ".*", little in any::<bool>()) {
        let encoding = if little { Encoding::Utf16Le } else { Encoding::Utf16Be };
        let bytes = encoding.encode(&content).into_owned();
        let text = Text::decode(bytes).expect("encoded text decodes");
        prop_assert_eq!(text.content(), content.as_str());
        prop_assert_eq!(text.encoding(), encoding);
    }

    #[test]
    fn lines_add_up_to_the_content(content in plain_text()) {
        let text = Text::decode(content.as_bytes()).expect("UTF-8 without NUL decodes");
        prop_assert_eq!(text.lines().len(), text.line_count());
        prop_assert_eq!(text.lines().collect::<String>(), content.as_str());
        prop_assert_eq!(text.span(0..text.line_count()), content.as_str());
    }
}

/// Check one parse of `file`; whether it parsed.
fn check_parse(file: &str) -> Result<bool, TestCaseError> {
    let text = Text::decode(file.as_bytes()).expect("the soup is UTF-8 without NUL");
    match markers::parse(&text) {
        Ok(parsed) => {
            let (sides, bases) = (parsed.inputs.sides().len(), parsed.inputs.bases().len());
            prop_assert!(sides >= 2, "{sides} sides: {file:?}");
            prop_assert_eq!(sides, bases + 1, "{:?}", file);
            prop_assert!(parsed.marker_length >= MIN_MARKER_LENGTH, "{file:?}");

            // The text before the first conflict reaches every input.
            let run = START.to_string().repeat(parsed.marker_length);
            let starts = |line: &str| {
                line.strip_prefix(run.as_str())
                    .is_some_and(|rest| rest.bytes().next().is_none_or(|b| b.is_ascii_whitespace()))
            };
            let lines: Vec<&str> = file.split_inclusive('\n').collect();
            let first = lines.iter().position(|line| starts(line));
            prop_assert!(first.is_some(), "no start marker: {file:?}");
            let before = lines[..first.unwrap_or_default()].concat();
            for input in parsed.inputs.sides().iter().chain(parsed.inputs.bases()) {
                prop_assert!(input.content().starts_with(&before), "{file:?}");
            }
            Ok(true)
        }
        Err(error) => {
            let line_count = file.split_inclusive('\n').count();
            if let Some(line) = error_line(&error) {
                prop_assert!(
                    (1..=line_count).contains(&line),
                    "{error} of {line_count} lines: {file:?}"
                );
            }
            Ok(false)
        }
    }
}

#[test]
fn marker_soup_parses_to_a_merge_or_names_a_line_in_the_file() {
    let (parsed, refused) = (Cell::new(0_u32), Cell::new(0_u32));
    TestRunner::new(config(SOUP_CASES))
        .run(&soup(), |file| {
            let counter = if check_parse(&file)? { &parsed } else { &refused };
            counter.set(counter.get() + 1);
            Ok(())
        })
        .expect("every parse holds");
    assert!(parsed.get() > MIN_OUTCOMES, "{} parsed", parsed.get());
    assert!(refused.get() > MIN_OUTCOMES, "{} refused", refused.get());
}

proptest! {
    #![proptest_config(config(SOUP_CASES))]

    #[test]
    fn text_without_a_start_marker_has_no_markers(content in plain_text()) {
        let content = content.replace(START, "");
        let text = Text::decode(content.as_bytes()).expect("UTF-8 without NUL decodes");
        prop_assert!(matches!(markers::parse(&text), Err(MarkerError::NoMarkers)));
    }
}
