//! The recovery journal (spec §6.2): enough to rebuild a session after a crash. It holds a
//! hash of each input, the options the merge was aligned with, and every command applied.
//! Restoring checks the hashes, aligns with the recorded options and replays the commands.
//!
//! The file is JSON Lines: a header line, then one line per command, each ending in `\n`, so
//! new commands append to it. A last line without its `\n` is a write cut short; parsing
//! drops it.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::boundaries::Boundaries;
use crate::document::{
    Command, CommandError, LabelCountError, Labels, MergeDocument, MergeOptions, Start,
};
use crate::merge3::{Inputs, Kind, Region};

/// The format of the journal. A journal of another version is not restored.
pub const JOURNAL_VERSION: u32 = 10;

const HEX_DIGITS_PER_BYTE: usize = 2;

/// SHA-256 of each input's decoded text, in hex: the sides, then the bases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputHashes(Vec<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Journal {
    header: Header,
    commands: Vec<Command>,
}

/// The journal's first line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Header {
    version: u32,
    /// SHA-256 of each input's decoded text, in hex: the sides, then the bases.
    inputs: Vec<String>,
    options: MergeOptions,
    /// [`Start::alignment`] of the document the commands applied to. Absent from a version-9
    /// header, which restore then refuses by its version.
    #[serde(default)]
    alignment: String,
}

const LINE_END: u8 = b'\n';
pub const JOURNAL_EXTENSION: &str = "jsonl";

/// A journal file that cannot be read back.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("the journal has no whole header line")]
    NoHeader,
    #[error("line {index} of the journal does not parse: {source}")]
    Line {
        index: usize,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("the journal has format {found}, not {JOURNAL_VERSION}")]
    Version { found: u32 },
    #[error("the journal was written for other inputs")]
    Inputs,
    #[error(
        "the journal was written for another alignment of these inputs, such as one by another version of triad"
    )]
    Alignment,
    #[error(transparent)]
    Labels(#[from] LabelCountError),
    #[error("command {index} of the journal does not apply: {source}")]
    Command {
        index: usize,
        #[source]
        source: CommandError,
    },
}

impl Journal {
    /// An empty journal for `doc` as it was aligned.
    #[must_use]
    pub fn new(doc: &MergeDocument) -> Self {
        Self::with_hashes(input_hashes(doc.inputs()), doc.start())
    }

    /// An empty journal for inputs with `hashes`, of a document that started from `start`.
    #[must_use]
    pub fn with_hashes(hashes: InputHashes, start: Start) -> Self {
        let Start { options, alignment } = start;
        Self {
            header: Header { version: JOURNAL_VERSION, inputs: hashes.0, options, alignment },
            commands: Vec::new(),
        }
    }

    /// Read a journal file. A last line without its `\n` is dropped.
    ///
    /// # Errors
    ///
    /// [`ParseError::NoHeader`] without a whole first line, [`ParseError::Line`] for a whole
    /// line that does not parse.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        // Every whole line ends in `\n`; what follows the last one is cut short.
        let Some(end) = bytes.iter().rposition(|&b| b == LINE_END) else {
            return Err(ParseError::NoHeader);
        };
        let mut lines = bytes[..end].split(|&b| b == LINE_END).enumerate();
        let Some((index, header)) = lines.next() else {
            return Err(ParseError::NoHeader);
        };
        let header =
            serde_json::from_slice(header).map_err(|source| ParseError::Line { index, source })?;
        let commands = lines
            .map(|(index, line)| {
                serde_json::from_slice(line).map_err(|source| ParseError::Line { index, source })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { header, commands })
    }

    /// The whole file: the header line and every command's line.
    ///
    /// # Errors
    ///
    /// [`serde_json::Error`] when a value does not serialise.
    pub fn to_lines(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut out = Vec::new();
        push_line(&mut out, &self.header)?;
        self.push_commands(&mut out, 0)?;
        Ok(out)
    }

    /// The lines of the commands from `from` on, to append to a file that holds the ones
    /// before.
    ///
    /// # Errors
    ///
    /// [`serde_json::Error`] when a command does not serialise.
    pub fn lines_from(&self, from: usize) -> Result<Vec<u8>, serde_json::Error> {
        let mut out = Vec::new();
        self.push_commands(&mut out, from)?;
        Ok(out)
    }

    fn push_commands(&self, out: &mut Vec<u8>, from: usize) -> Result<(), serde_json::Error> {
        self.commands[from..].iter().try_for_each(|command| push_line(out, command))
    }

    /// Add a command the document has applied.
    pub fn record(&mut self, command: Command) {
        self.commands.push(command);
    }

    #[must_use]
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    #[must_use]
    pub fn options(&self) -> MergeOptions {
        self.header.options
    }

    /// The journal's file name: its [key](Journal::key) with the JSON Lines extension.
    #[must_use]
    pub fn file_name(&self, output: Option<&Path>) -> String {
        format!("{}.{JOURNAL_EXTENSION}", self.key(output))
    }

    /// The name of the journal for its inputs and the output it saves to: SHA-256 of the
    /// inputs' hashes, then the output path's bytes, in hex.
    #[must_use]
    pub fn key(&self, output: Option<&Path>) -> String {
        let mut hasher = Sha256::new();
        for hash in &self.header.inputs {
            hasher.update(hash.as_bytes());
        }
        if let Some(output) = output {
            hasher.update(output.as_os_str().as_encoded_bytes());
        }
        hex(&hasher.finalize())
    }

    /// Whether this journal has the format [`Journal::restore`] takes.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.header.version == JOURNAL_VERSION
    }

    /// Rebuild the session on `inputs`, with the recorded options.
    ///
    /// # Errors
    ///
    /// [`RestoreError`] when the journal has another version, `inputs` are not the ones it was
    /// written for, they align otherwise than they did, `labels` do not fit, or a command
    /// does not apply.
    pub fn restore(&self, inputs: Inputs, labels: Labels) -> Result<MergeDocument, RestoreError> {
        self.restore_bounded(inputs, labels, None)
    }

    /// [`Journal::restore`], aligned with `boundaries` as [`MergeDocument::new_bounded`] is.
    ///
    /// # Errors
    ///
    /// As [`Journal::restore`].
    pub fn restore_bounded(
        &self,
        inputs: Inputs,
        labels: Labels,
        boundaries: Option<&dyn Boundaries>,
    ) -> Result<MergeDocument, RestoreError> {
        if self.header.version != JOURNAL_VERSION {
            return Err(RestoreError::Version { found: self.header.version });
        }
        if input_hashes(&inputs).0 != self.header.inputs {
            return Err(RestoreError::Inputs);
        }
        let mut doc = MergeDocument::new_bounded(inputs, self.header.options, labels, boundaries)?;
        // The commands name regions and byte offsets of the alignment they were made on.
        if doc.start().alignment != self.header.alignment {
            return Err(RestoreError::Alignment);
        }
        for (index, command) in self.commands.iter().enumerate() {
            doc.apply(command.clone()).map_err(|source| RestoreError::Command { index, source })?;
        }
        Ok(doc)
    }
}

/// Each input's hash: the sides, then the bases. Each input hashes on its own thread: at 1M
/// lines one input takes about 100 ms.
#[must_use]
pub fn input_hashes(inputs: &Inputs) -> InputHashes {
    InputHashes(std::thread::scope(|scope| {
        let hashing: Vec<_> = inputs
            .sides()
            .iter()
            .chain(inputs.bases())
            .map(|text| scope.spawn(|| hex(&Sha256::digest(text.content().as_bytes()))))
            .collect();
        hashing
            .into_iter()
            // A hashing thread that panicked passes its panic on; this adds none of its own.
            .map(|hash| hash.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)))
            .collect()
    }))
}

/// SHA-256, in hex, of `regions` with the byte length of each one's text: per region a kind
/// tag, the side for [`Kind::OneSide`], each base's and each side's line range, then the
/// length, every number a little-endian `u64`.
pub(crate) fn alignment_hash(
    regions: &[Region],
    lengths: impl IntoIterator<Item = usize>,
) -> String {
    const UNCHANGED: u8 = 0;
    const ONE_SIDE: u8 = 1;
    const ALL_SAME: u8 = 2;
    const WORD_MERGED: u8 = 3;
    const CONFLICT: u8 = 4;
    let number = |hasher: &mut Sha256, n: usize| {
        hasher.update(u64::try_from(n).expect("a usize fits a u64").to_le_bytes());
    };
    let mut hasher = Sha256::new();
    for (region, length) in regions.iter().zip(lengths) {
        match region.kind {
            Kind::Unchanged => hasher.update([UNCHANGED]),
            Kind::OneSide(side) => {
                hasher.update([ONE_SIDE]);
                number(&mut hasher, side);
            }
            Kind::AllSame => hasher.update([ALL_SAME]),
            Kind::WordMerged => hasher.update([WORD_MERGED]),
            Kind::Conflict => hasher.update([CONFLICT]),
        }
        for range in region.bases.iter().chain(&region.sides) {
            number(&mut hasher, range.start);
            number(&mut hasher, range.end);
        }
        number(&mut hasher, length);
    }
    hex(&hasher.finalize())
}

fn push_line(out: &mut Vec<u8>, value: &impl Serialize) -> Result<(), serde_json::Error> {
    serde_json::to_writer(&mut *out, value)?;
    out.push(LINE_END);
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    const LOW_NIBBLE: u8 = 0x0f;
    const NIBBLE_BITS: u32 = 4;
    let mut out = String::with_capacity(bytes.len() * HEX_DIGITS_PER_BYTE);
    for &b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> NIBBLE_BITS)]));
        out.push(char::from(DIGITS[usize::from(b & LOW_NIBBLE)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::boundaries::{Marked, node};
    use crate::diff::{DiffOptions, Whitespace};
    use crate::document::{EditBatch, Output, PickScope, Resolution, TextPatch};
    use crate::index::RegionIx;
    use crate::merge3::Region;
    use crate::merge3::{HunkLevel, Kind, Refine, SameChange, Semantics, Source};
    use crate::testing::{doc_with, three};

    use std::ops::Range;
    use std::path::Path;

    use super::*;

    /// SHA-256 of "a\n", from `shasum -a 256`.
    const SHA256_A_NEWLINE: &str =
        "87428fc522803d31065e7bce3cf03fe475096631e5e07bbd7a0fde60c4cf25c7";

    /// SHA-256 of "l\n", "r\n" and "b\n", from `shasum -a 256`.
    const SHA256_L_NEWLINE: &str =
        "6d7ebc44c5bc26207e62f4f628f912e1a0f41ed11764891aa7dd99eab83228e7";
    const SHA256_R_NEWLINE: &str =
        "8e54b0ca18020275e4aef1ca0eb5e197e066c065c1864817652a8a39c55402cd";
    const SHA256_B_NEWLINE: &str =
        "0263829989b6fd954f72baaf2fc64bc2e2f01d692d4de72986ea808f6e99813f";

    /// SHA-256 of `SHA256_A_NEWLINE` three times over, from `shasum -a 256`.
    const KEY_OF_THREE_A_NEWLINES: &str =
        "b310e8e2b1fea2b5f4acd701f58fb5bf342aa4f38c408c90aaa737abbddca375";
    /// SHA-256 of `SHA256_A_NEWLINE` three times over, then `OUTPUT`, from `shasum -a 256`.
    const KEY_OF_THREE_A_NEWLINES_AND_OUTPUT: &str =
        "8adb091683e7305037b1e7b6dfd810a385c4a9f85ea7c4779b2562afdb1d850e";
    const OUTPUT: &str = "/work/out.txt";

    fn inputs(left: &str) -> Inputs {
        three("a\nb\nc\n", left, "a\nR\nc\n")
    }

    fn doc(left: &str, options: MergeOptions) -> MergeDocument {
        MergeDocument::new(inputs(left), options, Labels::none()).expect("no labels")
    }

    /// Apply `command` to `doc` and record it, as the window does.
    fn run(doc: &mut MergeDocument, journal: &mut Journal, command: Command) {
        doc.apply(command.clone()).expect("applies");
        journal.record(command);
    }

    fn session() -> (MergeDocument, Journal) {
        let mut d = doc("a\nL\nc\n", MergeOptions::default());
        let mut journal = Journal::new(&d);
        let right = Command::Toggle { region: RegionIx::new(1), source: Source::Side(1) };
        run(&mut d, &mut journal, right);
        let typed = Command::Edit(EditBatch(vec![TextPatch { range: 0..0, text: "x".to_owned() }]));
        run(&mut d, &mut journal, typed);
        run(&mut d, &mut journal, Command::Undo);
        run(&mut d, &mut journal, Command::Redo);
        (d, journal)
    }

    #[test]
    fn replay_rebuilds_the_session() {
        let (d, journal) = session();
        assert_eq!(journal.commands().len(), 4);
        let restored = journal.restore(inputs("a\nL\nc\n"), Labels::none()).expect("restores");
        assert_eq!(restored.output(), Output::Resolved("xa\nR\nc\n".to_owned()));
        assert_eq!(restored.output(), d.output());
        assert_eq!(restored.resolutions(), d.resolutions());
        // Undo still reaches back through the replayed commands.
        let mut restored = restored;
        restored.apply(Command::Undo).expect("undo the typing");
        restored.apply(Command::Undo).expect("undo the pick");
        assert_eq!(restored.resolutions()[1], Resolution::Unresolved);
    }

    fn bytes(journal: &Journal) -> Vec<u8> {
        journal.to_lines().expect("serialises")
    }

    #[test]
    fn a_bulk_pick_replays_from_its_line() {
        let mut d = doc("a\nL\nc\n", MergeOptions::default());
        let mut journal = Journal::new(&d);
        let everywhere = Command::PickAll { scope: PickScope::All, sources: vec![Source::Side(1)] };
        run(&mut d, &mut journal, everywhere);
        let all = bytes(&journal);
        let text = std::str::from_utf8(&all).expect("JSON is UTF-8");
        assert_eq!(
            text.lines().nth(1),
            Some(r#"{"PickAll":{"scope":"All","sources":[{"Side":1}]}}"#)
        );
        let restored = Journal::parse(&all)
            .expect("parses")
            .restore(inputs("a\nL\nc\n"), Labels::none())
            .expect("restores");
        assert_eq!(restored.output(), Output::Resolved("a\nR\nc\n".to_owned()));
    }

    #[test]
    fn the_journal_survives_its_file_format() {
        let (_, journal) = session();
        assert_eq!(Journal::parse(&bytes(&journal)).expect("parses"), journal);
    }

    #[test]
    fn the_file_is_a_header_line_then_one_line_per_command() {
        let (_, journal) = session();
        let all = bytes(&journal);
        let text = std::str::from_utf8(&all).expect("JSON is UTF-8");
        assert_eq!(text.matches('\n').count(), 1 + 4);
        assert!(text.ends_with('\n'));
        let undo = text.lines().nth(3).expect("the third command");
        assert_eq!(undo, r#""Undo""#);
    }

    #[test]
    fn appended_lines_extend_the_written_file() {
        let (_, mut journal) = session();
        let written = bytes(&journal);
        let from = journal.commands().len();
        journal.record(Command::Clear { region: RegionIx::new(1) });
        journal.record(Command::Undo);
        let appended = journal.lines_from(from).expect("serialises");
        assert_eq!([written, appended].concat(), bytes(&journal));
    }

    #[test]
    fn nothing_new_appends_nothing() {
        let (_, journal) = session();
        let all = journal.commands().len();
        assert_eq!(journal.lines_from(all).expect("serialises"), b"");
    }

    #[test]
    fn a_torn_last_line_is_dropped() {
        let (_, journal) = session();
        let mut file = bytes(&journal);
        file.extend_from_slice(br#"{"Clear":{"reg"#);
        assert_eq!(Journal::parse(&file).expect("parses"), journal);
    }

    #[test]
    fn a_whole_last_line_without_its_newline_is_dropped() {
        let (_, journal) = session();
        let mut file = bytes(&journal);
        file.extend_from_slice(br#""Undo""#);
        assert_eq!(Journal::parse(&file).expect("parses"), journal);
    }

    #[test]
    fn a_broken_line_before_the_end_is_refused() {
        let (_, journal) = session();
        let file =
            [bytes(&journal), b"{\n".to_vec(), br#""Undo""#.to_vec(), b"\n".to_vec()].concat();
        let error = Journal::parse(&file).expect_err("the line after the commands is broken");
        assert!(matches!(error, ParseError::Line { index: 5, .. }), "{error:?}");
    }

    #[test]
    fn a_file_without_a_whole_header_is_refused() {
        let error = Journal::parse(br#"{"version":2"#).expect_err("no header line");
        assert!(matches!(error, ParseError::NoHeader), "{error:?}");
        let error = Journal::parse(b"").expect_err("empty");
        assert!(matches!(error, ParseError::NoHeader), "{error:?}");
    }

    #[test]
    fn a_header_that_does_not_parse_is_refused() {
        let error = Journal::parse(b"[]\n").expect_err("not a header");
        assert!(matches!(error, ParseError::Line { index: 0, .. }), "{error:?}");
    }

    #[test]
    fn the_format_version_is_ten() {
        assert_eq!(JOURNAL_VERSION, 10);
        let (d, journal) = session();
        let all = bytes(&journal);
        let header =
            std::str::from_utf8(&all).expect("JSON is UTF-8").lines().next().expect("a header");
        assert!(header.starts_with(r#"{"version":10,"#), "{header}");
        let alignment = format!(r#""alignment":"{}""#, d.start().alignment);
        assert!(header.contains(&alignment), "{header}");
        assert!(header.contains(r#""refine":"ends""#), "{header}");
        assert!(header.contains(r#""syntax_boundaries":false"#), "{header}");
        assert!(header.contains(r#""line_endings":"Compare""#), "{header}");
    }

    /// The session's journal written as `version`, with each `(from, to)` replaced once, is
    /// refused by its version.
    fn assert_refused_by_version(version: u32, edits: &[(&str, &str)]) {
        let (_, journal) = session();
        let lines = String::from_utf8(bytes(&journal)).expect("JSON is UTF-8");
        let header = format!(r#"{{"version":{version},"#);
        let current = format!(r#"{{"version":{JOURNAL_VERSION},"#);
        let old = edits.iter().fold(lines.replacen(&current, &header, 1), |old, (from, to)| {
            assert!(old.contains(from), "{from} in {old}");
            old.replacen(from, to, 1)
        });
        let parsed = Journal::parse(old.as_bytes()).expect("an older header parses");
        let error =
            parsed.restore(inputs("a\nL\nc\n"), Labels::none()).expect_err("an older format");
        assert!(matches!(error, RestoreError::Version { found } if found == version), "{error:?}");
    }

    #[test]
    fn a_version_nine_journal_without_an_alignment_is_refused_by_its_version() {
        let (d, _) = session();
        let alignment = format!(r#","alignment":"{}""#, d.start().alignment);
        assert_refused_by_version(9, &[(&alignment, "")]);
    }

    #[test]
    fn a_journal_from_another_alignment_is_refused() {
        // A node across left's shared `m` keeps the conflict whole: three regions. Without
        // it, as after a grammar changes, the conflict splits into five, and the pick of
        // region 1 would land on another conflict.
        let left = "a\nX\nm\nY\nz\n";
        let marked = Marked(vec![(left, node(3..5))]);
        let deep = || three("a\np\nq\nz\n", left, "a\nZ\nm\nW\nz\n");
        let options = MergeOptions {
            refine: Refine::Deep,
            syntax_boundaries: true,
            ..MergeOptions::default()
        };
        let mut d = MergeDocument::new_bounded(deep(), options, Labels::none(), Some(&marked))
            .expect("no labels");
        let mut journal = Journal::new(&d);
        let pick = Command::Toggle { region: RegionIx::new(1), source: Source::Side(0) };
        run(&mut d, &mut journal, pick);
        let error =
            journal.restore_bounded(deep(), Labels::none(), None).expect_err("five regions");
        assert!(matches!(error, RestoreError::Alignment), "{error:?}");
    }

    fn region(kind: Kind, bases: Range<usize>, sides: [Range<usize>; 2]) -> Region {
        Region { kind, bases: vec![bases], sides: Vec::from(sides) }
    }

    /// SHA-256 of `00`, then `0, 1`, `0, 1, 0, 1` and `2` as little-endian `u64`s, from
    /// Python's `hashlib`.
    const ONE_UNCHANGED_REGION: &str =
        "66c778557a6d128974108420f1cf3a211a821c0890ed4b0943e6d01c88658b64";
    /// As [`ONE_UNCHANGED_REGION`], then `01` and `1` for `OneSide(1)`, then `1, 2`,
    /// `1, 1, 1, 3` and `5`.
    const AND_A_RIGHT_ONLY_REGION: &str =
        "cf7b6da6fe83cb7fb96ba3deb54e640f55751a27d8b9a2213f93d25551546c0d";

    #[test]
    fn the_alignment_hash_covers_each_regions_kind_ranges_and_text_length() {
        let unchanged = region(Kind::Unchanged, 0..1, [0..1, 0..1]);
        let right = region(Kind::OneSide(1), 1..2, [1..1, 1..3]);
        assert_eq!(alignment_hash(std::slice::from_ref(&unchanged), [2]), ONE_UNCHANGED_REGION);
        assert_eq!(
            alignment_hash(&[unchanged.clone(), right.clone()], [2, 5]),
            AND_A_RIGHT_ONLY_REGION
        );
        let left = region(Kind::OneSide(0), 1..2, [1..1, 1..3]);
        let longer = alignment_hash(&[unchanged.clone(), right.clone()], [2, 6]);
        let other_side = alignment_hash(&[unchanged.clone(), left], [2, 5]);
        let other_range =
            alignment_hash(&[unchanged, region(Kind::OneSide(1), 1..2, [1..2, 1..3])], [2, 5]);
        for changed in [longer, other_side, other_range] {
            assert_ne!(changed, AND_A_RIGHT_ONLY_REGION);
        }
    }

    #[test]
    fn a_document_starts_with_its_options_and_the_hash_of_its_alignment() {
        let (d, journal) = session();
        let start = d.start();
        assert_eq!(start.options, MergeOptions::default());
        assert_eq!(journal.options(), start.options);
        // `a`, the conflict `b`/`L`/`R`, `c`; the conflict shows `L` then `R`.
        let regions = [
            region(Kind::Unchanged, 0..1, [0..1, 0..1]),
            region(Kind::Conflict, 1..2, [1..2, 1..2]),
            region(Kind::Unchanged, 2..3, [2..3, 2..3]),
        ];
        assert_eq!(start.alignment, alignment_hash(&regions, [2, 4, 2]));
    }

    #[test]
    fn a_version_seven_journal_without_a_line_ending_rule_is_refused_by_its_version() {
        assert_refused_by_version(7, &[(r#","line_endings":"Compare""#, "")]);
    }

    #[test]
    fn other_inputs_are_refused() {
        let (_, journal) = session();
        let error = journal.restore(inputs("a\nM\nc\n"), Labels::none()).expect_err("left changed");
        assert!(matches!(error, RestoreError::Inputs), "{error:?}");
    }

    #[test]
    fn another_version_is_refused() {
        let (_, mut journal) = session();
        journal.header.version = JOURNAL_VERSION + 1;
        let error =
            journal.restore(inputs("a\nL\nc\n"), Labels::none()).expect_err("a newer format");
        assert!(
            matches!(error, RestoreError::Version { found } if found == JOURNAL_VERSION + 1),
            "{error:?}"
        );
    }

    #[test]
    fn a_journal_of_this_format_is_current() {
        let (_, journal) = session();
        assert!(journal.is_current());
        assert!(Journal::parse(&bytes(&journal)).expect("parses").is_current());
    }

    #[test]
    fn a_journal_of_another_format_is_not_current() {
        let (_, journal) = session();
        for found in [JOURNAL_VERSION - 1, JOURNAL_VERSION + 1] {
            let lines = String::from_utf8(bytes(&journal)).expect("JSON is UTF-8");
            let other = lines.replacen(
                &format!(r#"{{"version":{JOURNAL_VERSION},"#),
                &format!(r#"{{"version":{found},"#),
                1,
            );
            let parsed = Journal::parse(other.as_bytes()).expect("the header parses");
            assert!(!parsed.is_current(), "version {found}");
        }
    }

    #[test]
    fn a_command_that_does_not_apply_is_named() {
        let (_, mut journal) = session();
        journal.record(Command::Clear { region: RegionIx::new(99) });
        let error = journal.restore(inputs("a\nL\nc\n"), Labels::none()).expect_err("no region 99");
        assert!(matches!(error, RestoreError::Command { index: 4, .. }), "{error:?}");
    }

    #[test]
    fn restore_aligns_with_the_recorded_options() {
        let all = MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::All, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        // Left changes only whitespace: one region when whitespace is ignored, three when not.
        let d = doc_with("a\nb\nc\n", "a\nb \nc\n", "a\nb\nc\n", all);
        let journal = Journal::new(&d);
        let restored = journal
            .restore(three("a\nb\nc\n", "a\nb \nc\n", "a\nb\nc\n"), Labels::none())
            .expect("restores");
        assert_eq!(restored.options(), all);
        assert_eq!(d.regions().len(), 1);
        assert_eq!(restored.regions().len(), 1);
    }

    #[test]
    fn restore_aligns_with_the_recorded_semantics_and_same_change() {
        let jj_keep = MergeOptions {
            semantics: Semantics::Jj,
            same_change: SameChange::Keep,
            ..MergeOptions::default()
        };
        let same = || three("a\nb\n", "a\nB\n", "a\nB\n");
        let d = MergeDocument::new(same(), jj_keep, Labels::none()).expect("no labels");
        let restored = Journal::new(&d).restore(same(), Labels::none()).expect("restores");
        assert_eq!(restored.options(), jj_keep);
        // Accepted, the same change would resolve; kept, it stays a conflict.
        assert_eq!(restored.regions()[1].kind, Kind::Conflict);
        assert_eq!(restored.regions(), d.regions());
    }

    /// Word-resolvable inputs: left changes `x`, right changes `1`.
    fn words() -> Inputs {
        three("a\nlet x = 1;\n", "a\nlet y = 1;\n", "a\nlet x = 2;\n")
    }

    const BY_WORD: MergeOptions = MergeOptions {
        hunk_level: HunkLevel::Word,
        semantics: Semantics::Jj,
        same_change: SameChange::Accept,
        diff: DiffOptions {
            algorithm: crate::diff::Algorithm::Histogram,
            whitespace: Whitespace::None,
            line_endings: crate::diff::LineEndingRule::Compare,
        },
        refine: crate::merge3::Refine::Ends,
        syntax_boundaries: false,
        marker_length: crate::document::DEFAULT_MARKER_LENGTH,
        marker_style: crate::document::MarkerStyle::Merge,
    };

    #[test]
    fn restore_aligns_with_the_recorded_hunk_level() {
        let d = MergeDocument::new(words(), BY_WORD, Labels::none()).expect("no labels");
        assert_eq!(d.regions()[1].kind, Kind::WordMerged);
        let restored = Journal::new(&d).restore(words(), Labels::none()).expect("restores");
        assert_eq!(restored.options(), BY_WORD);
        assert_eq!(restored.regions(), d.regions());
        assert_eq!(restored.text(), "a\nlet y = 2;\n");
    }

    #[test]
    fn a_session_with_a_word_merged_region_replays() {
        let mut d = MergeDocument::new(words(), BY_WORD, Labels::none()).expect("no labels");
        let mut journal = Journal::new(&d);
        let left = Command::Pick { region: RegionIx::new(1), sources: vec![Source::Side(0)] };
        run(&mut d, &mut journal, left);
        run(&mut d, &mut journal, Command::Undo);
        run(&mut d, &mut journal, Command::Redo);
        assert_eq!(d.text(), "a\nlet y = 1;\n");
        let parsed = Journal::parse(&bytes(&journal)).expect("parses");
        let restored = parsed.restore(words(), Labels::none()).expect("restores");
        assert_eq!(restored.text(), d.text());
        assert_eq!(restored.resolutions(), d.resolutions());
    }

    #[test]
    fn a_journal_with_syntax_boundaries_restores_with_the_same_boundaries() {
        // Left's shared m is at bytes 4..6; a node across its start keeps the conflict whole.
        let left = "a\nX\nm\nY\nz\n";
        let marked = Marked(vec![(left, node(3..5))]);
        let deep = || three("a\np\nq\nz\n", left, "a\nZ\nm\nW\nz\n");
        let options = MergeOptions {
            refine: Refine::Deep,
            syntax_boundaries: true,
            ..MergeOptions::default()
        };
        let mut d = MergeDocument::new_bounded(deep(), options, Labels::none(), Some(&marked))
            .expect("no labels");
        let mut journal = Journal::new(&d);
        let pick = Command::Toggle { region: RegionIx::new(1), source: Source::Side(0) };
        run(&mut d, &mut journal, pick);
        let parsed = Journal::parse(&bytes(&journal)).expect("parses");
        let header = String::from_utf8(bytes(&journal)).expect("JSON is UTF-8");
        assert!(header.contains(r#""syntax_boundaries":true"#), "{header}");
        let restored =
            parsed.restore_bounded(deep(), Labels::none(), Some(&marked)).expect("restores");
        assert_eq!(restored.regions(), d.regions());
        assert_eq!(restored.text(), d.text());
        assert_eq!(d.regions().len(), 3, "the conflict stays whole");
    }

    #[test]
    fn a_version_six_journal_without_syntax_boundaries_is_refused_by_its_version() {
        assert_refused_by_version(6, &[(r#","syntax_boundaries":false"#, "")]);
    }

    #[test]
    fn a_version_five_journal_is_refused_since_its_toggles_could_clear_a_conflict() {
        assert_refused_by_version(5, &[]);
    }

    #[test]
    fn a_version_four_journal_with_refinement_as_a_flag_is_refused_by_its_version() {
        assert_refused_by_version(4, &[(r#""refine":"ends""#, r#""refine":true"#)]);
    }

    #[test]
    fn a_version_three_journal_without_the_hunk_level_is_refused_by_its_version() {
        assert_refused_by_version(3, &[(r#""hunk_level":"line","#, "")]);
    }

    #[test]
    fn a_header_without_a_marker_style_restores_diff3() {
        let (_, journal) = session();
        let lines = String::from_utf8(bytes(&journal)).expect("JSON is UTF-8");
        let old = lines.replacen(r#","marker_style":"diff3""#, "", 1);
        assert!(!old.contains("marker_style"), "{old}");
        let restored = Journal::parse(old.as_bytes())
            .expect("the header parses")
            .restore(inputs("a\nL\nc\n"), Labels::none())
            .expect("restores");
        assert_eq!(restored.options().marker_style, crate::document::MarkerStyle::Diff3);
    }

    #[test]
    fn a_version_two_journal_without_the_new_options_is_refused_by_its_version() {
        assert_refused_by_version(
            2,
            &[
                (r#""semantics":"git","#, ""),
                (r#""same_change":"accept","#, ""),
                (r#""hunk_level":"line","#, ""),
            ],
        );
    }

    #[test]
    fn the_key_hashes_every_input() {
        let same = crate::testing::doc("a\n", "a\n", "a\n");
        assert_eq!(Journal::new(&same).key(None), KEY_OF_THREE_A_NEWLINES);
        let key = |left| Journal::new(&doc(left, MergeOptions::default())).key(None);
        assert_ne!(key("a\nL\nc\n"), key("a\nM\nc\n"));
    }

    #[test]
    fn the_file_name_is_the_key_with_the_json_lines_extension() {
        let same = crate::testing::doc("a\n", "a\n", "a\n");
        assert_eq!(Journal::new(&same).file_name(None), format!("{KEY_OF_THREE_A_NEWLINES}.jsonl"));
    }

    #[test]
    fn the_key_hashes_the_output_path_after_the_inputs() {
        let same = crate::testing::doc("a\n", "a\n", "a\n");
        let journal = Journal::new(&same);
        assert_eq!(journal.key(Some(Path::new(OUTPUT))), KEY_OF_THREE_A_NEWLINES_AND_OUTPUT);
        assert_ne!(
            journal.key(Some(Path::new("/work/other.txt"))),
            KEY_OF_THREE_A_NEWLINES_AND_OUTPUT
        );
    }

    #[test]
    fn input_hashes_go_left_right_then_base() {
        let d = crate::testing::doc("b\n", "l\n", "r\n");
        assert_eq!(
            Journal::new(&d).header.inputs,
            [SHA256_L_NEWLINE, SHA256_R_NEWLINE, SHA256_B_NEWLINE]
        );
    }

    #[test]
    fn an_input_hash_is_the_sha256_of_its_text() {
        let d = crate::testing::doc("a\n", "a\n", "a\n");
        assert_eq!(Journal::new(&d).header.inputs, [SHA256_A_NEWLINE; 3]);
    }

    #[test]
    fn a_journal_from_hashes_holds_those_hashes_and_the_start() {
        let all = MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::All, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        let d = doc_with("b\n", "l\n", "r\n", all);
        let journal = Journal::with_hashes(input_hashes(d.inputs()), d.start());
        assert_eq!(journal.header.inputs, [SHA256_L_NEWLINE, SHA256_R_NEWLINE, SHA256_B_NEWLINE]);
        assert_eq!(journal.options(), all);
        assert_eq!(journal.header.alignment, d.start().alignment);
        assert_eq!(journal.commands(), []);
    }
}
