//! Decoding input files into text, splitting lines and encoding output (spec §4.1).
//!
//! Lines end at `\n` and include it. A lone `\r` is an ordinary character inside a line,
//! as in git `merge-file` and the result editor.

use std::borrow::Cow;
use std::iter;
use std::ops::Range;
use std::sync::Arc;

/// A file's encoding and whether it starts with a byte order mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

impl Encoding {
    /// The byte order mark a file in this encoding starts with; empty for plain UTF-8.
    #[must_use]
    pub fn bom(self) -> &'static [u8] {
        match self {
            Encoding::Utf8 => &[],
            Encoding::Utf8Bom => &UTF8_BOM,
            Encoding::Utf16Le => &UTF16LE_BOM,
            Encoding::Utf16Be => &UTF16BE_BOM,
        }
    }

    /// `text` in this encoding without the byte order mark, so a text split at `char`
    /// boundaries encodes piece by piece. UTF-8 borrows it.
    #[must_use]
    pub fn encode_chunk(self, text: &str) -> Cow<'_, [u8]> {
        match self {
            Encoding::Utf8 | Encoding::Utf8Bom => Cow::Borrowed(text.as_bytes()),
            Encoding::Utf16Le => {
                text.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>().into()
            }
            Encoding::Utf16Be => {
                text.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>().into()
            }
        }
    }

    /// `text` in this encoding, after its byte order mark if it has one. UTF-8 borrows it.
    #[must_use]
    pub fn encode(self, text: &str) -> Cow<'_, [u8]> {
        match self {
            Encoding::Utf8 => Cow::Borrowed(text.as_bytes()),
            Encoding::Utf8Bom => Cow::Owned([&UTF8_BOM[..], text.as_bytes()].concat()),
            Encoding::Utf16Le => UTF16LE_BOM
                .into_iter()
                .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
                .collect::<Vec<u8>>()
                .into(),
            Encoding::Utf16Be => UTF16BE_BOM
                .into_iter()
                .chain(text.encode_utf16().flat_map(u16::to_be_bytes))
                .collect::<Vec<u8>>()
                .into(),
        }
    }
}

/// The ending of a line that has a `\n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// Why a file could not be decoded. Offsets count bytes from the start of the file,
/// including any byte order mark.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("binary file: a NUL byte within the first {BINARY_SNIFF_BYTES} bytes")]
    Binary,
    #[error("invalid UTF-8 at byte {offset}")]
    InvalidUtf8 { offset: usize },
    #[error("unpaired UTF-16 surrogate at byte {offset}")]
    InvalidUtf16 { offset: usize },
    #[error("UTF-16 file with an odd number of bytes")]
    OddUtf16Length,
}

/// git's binary rule: a NUL byte within this many leading bytes makes a file binary.
pub const BINARY_SNIFF_BYTES: usize = 8000;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const UTF16BE_BOM: [u8; 2] = [0xFE, 0xFF];
const UTF16_BOM_LEN: usize = 2;
const UTF16_UNIT_BYTES: usize = 2;

/// Decoded, immutable file text with its encoding and line index. A clone shares both.
#[derive(Debug, Clone)]
pub struct Text {
    encoding: Encoding,
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    content: String,
    line_starts: Vec<usize>,
}

impl Text {
    /// Decode a file's bytes: byte order mark first, then the binary check, then UTF-8.
    /// UTF-8 decodes in place, in the buffer read from the file.
    ///
    /// # Errors
    ///
    /// Binary input, invalid UTF-8, an unpaired UTF-16 surrogate or an odd UTF-16 length.
    pub fn decode(bytes: impl Into<Vec<u8>>) -> Result<Text, DecodeError> {
        let mut bytes = bytes.into();
        let (encoding, content) = if bytes.starts_with(&UTF8_BOM) {
            bytes.drain(..UTF8_BOM.len());
            (Encoding::Utf8Bom, decode_utf8(bytes, UTF8_BOM.len())?)
        } else if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
            (Encoding::Utf16Le, decode_utf16(rest, u16::from_le_bytes)?)
        } else if let Some(rest) = bytes.strip_prefix(&UTF16BE_BOM) {
            (Encoding::Utf16Be, decode_utf16(rest, u16::from_be_bytes)?)
        } else if bytes.iter().take(BINARY_SNIFF_BYTES).any(|&byte| byte == 0) {
            return Err(DecodeError::Binary);
        } else {
            (Encoding::Utf8, decode_utf8(bytes, 0)?)
        };
        Ok(Text::new(encoding, content))
    }

    pub(crate) fn new(encoding: Encoding, content: String) -> Text {
        let line_starts = if content.is_empty() {
            Vec::new()
        } else {
            iter::once(0)
                .chain(
                    content
                        .match_indices('\n')
                        .map(|(at, _)| at + 1)
                        .filter(|&start| start < content.len()),
                )
                .collect()
        };
        Text { encoding, shared: Arc::new(Shared { content, line_starts }) }
    }

    /// Encode `text` in this file's encoding, with its byte order mark if it has one.
    #[must_use]
    pub fn encode<'a>(&self, text: &'a str) -> Cow<'a, [u8]> {
        self.encoding.encode(text)
    }

    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.shared.content
    }

    /// Line `n`, including its terminator.
    ///
    /// # Panics
    ///
    /// If `n` is not below [`Text::line_count`].
    #[must_use]
    pub fn line(&self, n: usize) -> &str {
        let count = self.line_count();
        // An index past the end is a caller's bug, as with slice indexing.
        assert!(n < count, "line {n} out of range for {count} lines");
        let Shared { content, line_starts } = &*self.shared;
        let end = line_starts.get(n + 1).copied().unwrap_or(content.len());
        &content[line_starts[n]..end]
    }

    /// Lines `lines.start..lines.end` as one slice, terminators included.
    ///
    /// # Panics
    ///
    /// If the range is reversed or ends past [`Text::line_count`].
    #[must_use]
    pub fn span(&self, lines: Range<usize>) -> &str {
        let count = self.line_count();
        // A range past the end is a caller's bug, as with slice indexing.
        assert!(
            lines.start <= lines.end && lines.end <= count,
            "lines {lines:?} out of range for {count} lines"
        );
        let Shared { content, line_starts } = &*self.shared;
        let at = |n: usize| line_starts.get(n).copied().unwrap_or(content.len());
        &content[at(lines.start)..at(lines.end)]
    }

    /// The lines a byte range spans. Both ends must lie on line boundaries.
    #[must_use]
    pub(crate) fn lines_at(&self, bytes: &Range<usize>) -> Range<usize> {
        self.line_at(bytes.start)..self.line_at(bytes.end)
    }

    /// The line starting at byte `at`, or the line count at the end of the text.
    fn line_at(&self, at: usize) -> usize {
        let Shared { content, line_starts } = &*self.shared;
        let line = line_starts.partition_point(|&start| start < at);
        debug_assert!(
            at == content.len() || line_starts.get(line) == Some(&at),
            "byte {at} is not a line boundary"
        );
        line
    }

    /// Every line in order, each with its terminator.
    pub fn lines(&self) -> impl ExactSizeIterator<Item = &str> + '_ {
        (0..self.line_count()).map(|n| self.line(n))
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.shared.line_starts.len()
    }

    /// The ending most lines use. Ties go to [`LineEnding::Lf`]. `None` when no line ends in `\n`.
    #[must_use]
    pub fn dominant_ending(&self) -> Option<LineEnding> {
        let (lf, crlf) = (0..self.line_count())
            .map(|n| self.line(n))
            .filter(|line| line.ends_with('\n'))
            .fold((0_usize, 0_usize), |(lf, crlf), line| {
                if line.ends_with("\r\n") { (lf, crlf + 1) } else { (lf + 1, crlf) }
            });
        match (lf, crlf) {
            (0, 0) => None,
            (lf, crlf) if crlf > lf => Some(LineEnding::CrLf),
            _ => Some(LineEnding::Lf),
        }
    }
}

/// `bytes` start `offset` bytes into the file, after any byte order mark.
fn decode_utf8(bytes: Vec<u8>, offset: usize) -> Result<String, DecodeError> {
    String::from_utf8(bytes).map_err(|error| DecodeError::InvalidUtf8 {
        offset: offset + error.utf8_error().valid_up_to(),
    })
}

/// `bytes` follow a two-byte byte order mark.
fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Result<String, DecodeError> {
    let (pairs, []) = bytes.as_chunks::<UTF16_UNIT_BYTES>() else {
        return Err(DecodeError::OddUtf16Length);
    };
    char::decode_utf16(pairs.iter().copied().map(unit))
        .try_fold((String::with_capacity(pairs.len()), 0), |(mut text, units_read), decoded| {
            match decoded {
                Ok(c) => {
                    text.push(c);
                    Ok((text, units_read + c.len_utf16()))
                }
                Err(_) => Err(DecodeError::InvalidUtf16 {
                    offset: UTF16_BOM_LEN + units_read * UTF16_UNIT_BYTES,
                }),
            }
        })
        .map(|(text, _)| text)
}

/// The output's encoding: base's, or left's when base is empty (spec §4.1).
#[must_use]
pub fn output_encoding(base: &Text, left: &Text) -> Encoding {
    if base.content().is_empty() { left.encoding() } else { base.encoding() }
}

/// The ending for newlines typed into the result: base's dominant ending, then left's, then LF.
#[must_use]
pub fn typed_ending(base: &Text, left: &Text) -> LineEnding {
    base.dominant_ending().or_else(|| left.dominant_ending()).unwrap_or(LineEnding::Lf)
}

/// The line endings a text holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndings {
    /// No line ends in `\n`.
    None,
    Lf,
    CrLf,
    Mixed,
}

impl LineEndings {
    /// From whether some line ends in `\n` alone, and whether some ends in `\r\n`.
    #[must_use]
    pub const fn from_presence(lf: bool, crlf: bool) -> Self {
        match (lf, crlf) {
            (false, false) => Self::None,
            (true, false) => Self::Lf,
            (false, true) => Self::CrLf,
            (true, true) => Self::Mixed,
        }
    }

    /// The endings of `text`.
    #[must_use]
    pub fn of(text: &str) -> Self {
        let counts = EndingCounts::of(text.as_bytes(), None);
        Self::from_presence(counts.lf > 0, counts.crlf > 0)
    }

    /// The name `inspect --json` gives: `none`, `lf`, `crlf` or `mixed`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Lf => "lf",
            Self::CrLf => "crlf",
            Self::Mixed => "mixed",
        }
    }
}

/// How many lines end in `\n` alone, and how many in `\r\n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndingCounts {
    pub lf: usize,
    pub crlf: usize,
}

impl EndingCounts {
    /// The endings of the `\n`s in `bytes`, where `before` is the byte just before them,
    /// if any, which a `\n` at the start pairs with.
    #[must_use]
    pub fn of(bytes: &[u8], before: Option<u8>) -> Self {
        let mut counts = Self::default();
        counts.add(bytes, before);
        counts
    }

    /// Count the `\n`s in `bytes`, as [`EndingCounts::of`] does, onto these.
    pub fn add(&mut self, bytes: &[u8], before: Option<u8>) {
        let mut previous = before;
        for &byte in bytes {
            if byte == b'\n' {
                if previous == Some(b'\r') {
                    self.crlf += 1;
                } else {
                    self.lf += 1;
                }
            }
            previous = Some(byte);
        }
    }
}

/// `text` with every line ending, `\n` or `\r\n`, made `to`. A lone `\r` stays, and a last
/// line without an ending stays without one.
#[must_use]
pub fn with_endings(text: &str, to: LineEnding) -> Cow<'_, str> {
    let counts = EndingCounts::of(text.as_bytes(), None);
    let done = match to {
        LineEnding::Lf => counts.crlf == 0,
        LineEnding::CrLf => counts.lf == 0,
    };
    if done {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + counts.lf);
    for line in text.split_inclusive('\n') {
        match line.strip_suffix('\n') {
            Some(body) => {
                out.push_str(body.strip_suffix('\r').unwrap_or(body));
                out.push_str(to.as_str());
            }
            None => out.push_str(line),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_encoding_names_its_byte_order_mark() {
        assert_eq!(Encoding::Utf8.bom(), b"");
        assert_eq!(Encoding::Utf8Bom.bom(), [0xEF, 0xBB, 0xBF]);
        assert_eq!(Encoding::Utf16Le.bom(), [0xFF, 0xFE]);
        assert_eq!(Encoding::Utf16Be.bom(), [0xFE, 0xFF]);
    }

    #[test]
    fn a_chunk_encodes_without_the_byte_order_mark() {
        // `é` is U+00E9: C3 A9 in UTF-8, E9 00 in UTF-16LE.
        assert_eq!(&*Encoding::Utf8.encode_chunk("é\n"), [0xC3, 0xA9, 0x0A]);
        assert_eq!(&*Encoding::Utf8Bom.encode_chunk("é\n"), [0xC3, 0xA9, 0x0A]);
        assert_eq!(&*Encoding::Utf16Le.encode_chunk("é\n"), [0xE9, 0x00, 0x0A, 0x00]);
        assert_eq!(&*Encoding::Utf16Be.encode_chunk("é\n"), [0x00, 0xE9, 0x00, 0x0A]);
        // U+1F600 is the surrogate pair D83D DE00.
        assert_eq!(&*Encoding::Utf16Le.encode_chunk("😀"), [0x3D, 0xD8, 0x00, 0xDE]);
        assert!(matches!(Encoding::Utf8Bom.encode_chunk("a"), Cow::Borrowed(_)));
    }

    #[test]
    fn a_text_encodes_as_its_mark_then_its_chunks() {
        for encoding in [Encoding::Utf8, Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be] {
            let pieces =
                [encoding.bom(), &encoding.encode_chunk("ab"), &encoding.encode_chunk("é\n")]
                    .concat();
            assert_eq!(&*encoding.encode("abé\n"), pieces, "{encoding:?}");
        }
    }

    #[test]
    fn endings_are_lf_crlf_mixed_or_none() {
        assert_eq!(LineEndings::of("a\nb\n"), LineEndings::Lf);
        assert_eq!(LineEndings::of("a\r\nb\r\n"), LineEndings::CrLf);
        assert_eq!(LineEndings::of("a\r\nb\n"), LineEndings::Mixed);
        assert_eq!(LineEndings::of("a\rb"), LineEndings::None);
        assert_eq!(LineEndings::of(""), LineEndings::None);
        assert_eq!(LineEndings::of("\n"), LineEndings::Lf);
    }

    #[test]
    fn a_newline_at_the_start_pairs_with_the_byte_before() {
        assert_eq!(EndingCounts::of(b"\nx\n", Some(b'\r')), EndingCounts { lf: 1, crlf: 1 });
        assert_eq!(EndingCounts::of(b"\nx\r\n", Some(b'x')), EndingCounts { lf: 1, crlf: 1 });
        assert_eq!(EndingCounts::of(b"\n", None), EndingCounts { lf: 1, crlf: 0 });
    }

    #[test]
    fn converting_endings_keeps_a_lone_cr_and_a_missing_last_newline() {
        assert_eq!(with_endings("a\r\nb\nc", LineEnding::Lf), "a\nb\nc");
        assert_eq!(with_endings("a\r\nb\nc", LineEnding::CrLf), "a\r\nb\r\nc");
        assert_eq!(with_endings("a\rb\n", LineEnding::CrLf), "a\rb\r\n");
        assert_eq!(with_endings("a\rb\r\n", LineEnding::Lf), "a\rb\n");
        assert_eq!(with_endings("\r\n\r\n", LineEnding::Lf), "\n\n");
        assert_eq!(with_endings("", LineEnding::CrLf), "");
        assert!(matches!(with_endings("a\nb\n", LineEnding::Lf), Cow::Borrowed(_)));
    }

    fn decoded(bytes: impl Into<Vec<u8>>) -> Text {
        Text::decode(bytes).expect("test input decodes")
    }

    fn with_prefix(prefix: &[u8], rest: &[u8]) -> Vec<u8> {
        [prefix, rest].concat()
    }

    #[test]
    fn utf8_decodes_in_the_read_buffer() {
        let bytes = b"a\nb\n".to_vec();
        let at = bytes.as_ptr();
        let text = decoded(bytes);
        assert_eq!(text.content(), "a\nb\n");
        assert_eq!(text.content().as_ptr(), at);
    }

    #[test]
    fn utf8_with_bom_decodes_in_the_read_buffer() {
        let bytes = with_prefix(&UTF8_BOM, b"a\nb\n");
        let at = bytes.as_ptr();
        let text = decoded(bytes);
        assert_eq!(text.content(), "a\nb\n");
        assert_eq!(text.content().as_ptr(), at);
    }

    #[test]
    fn clones_share_the_content_and_line_index() {
        let text = decoded(b"a\n");
        assert!(Arc::ptr_eq(&text.clone().shared, &text.shared));
    }

    #[test]
    fn line_endings_spell_their_bytes() {
        assert_eq!((LineEnding::Lf.as_str(), LineEnding::CrLf.as_str()), ("\n", "\r\n"));
    }

    #[test]
    fn span_joins_whole_lines() {
        let t = decoded(b"a\nb\r\nc");
        assert_eq!(t.span(0..0), "");
        assert_eq!(t.span(1..2), "b\r\n");
        assert_eq!(t.span(1..3), "b\r\nc");
        assert_eq!(t.span(0..3), "a\nb\r\nc");
        assert_eq!(t.span(3..3), "");
        assert_eq!(decoded(b"").span(0..0), "");
    }

    #[test]
    #[should_panic(expected = "lines 2..4 out of range for 3 lines")]
    fn span_past_the_end_panics() {
        let _ = decoded(b"a\nb\nc\n").span(2..4);
    }

    fn lines(text: &Text) -> Vec<&str> {
        (0..text.line_count()).map(|n| text.line(n)).collect()
    }

    // Encoding detection

    #[test]
    fn plain_bytes_are_utf8() {
        let text = decoded(b"a\n");
        assert_eq!((text.encoding(), text.content()), (Encoding::Utf8, "a\n"));
    }

    #[test]
    fn utf8_bom_is_detected_and_stripped() {
        let text = decoded(with_prefix(&UTF8_BOM, b"a"));
        assert_eq!((text.encoding(), text.content()), (Encoding::Utf8Bom, "a"));
    }

    #[test]
    fn utf16le_with_its_nul_bytes_is_text() {
        let text = decoded(with_prefix(&UTF16LE_BOM, &[b'a', 0, b'\n', 0]));
        assert_eq!((text.encoding(), text.content()), (Encoding::Utf16Le, "a\n"));
    }

    #[test]
    fn utf16be_is_detected() {
        let text = decoded(with_prefix(&UTF16BE_BOM, &[0, b'a', 0, 0xE9]));
        assert_eq!((text.encoding(), text.content()), (Encoding::Utf16Be, "aé"));
    }

    #[test]
    fn utf16_surrogate_pair_decodes_to_one_character() {
        // U+1F600 is D83D DE00.
        let text = decoded(with_prefix(&UTF16LE_BOM, &[0x3D, 0xD8, 0x00, 0xDE]));
        assert_eq!(text.content(), "\u{1F600}");
    }

    #[test]
    fn bom_only_files_are_empty() {
        let text = decoded(UTF16BE_BOM);
        assert_eq!(
            (text.encoding(), text.content(), text.line_count()),
            (Encoding::Utf16Be, "", 0)
        );
    }

    #[test]
    fn empty_input_is_empty_utf8() {
        let text = decoded(b"");
        assert_eq!((text.encoding(), text.content(), text.line_count()), (Encoding::Utf8, "", 0));
    }

    // Binary detection

    fn with_nul_at(index: usize, len: usize) -> Vec<u8> {
        let mut bytes = vec![b'a'; len];
        bytes[index] = 0;
        bytes
    }

    #[test]
    fn nul_at_the_first_byte_is_binary() {
        assert_eq!(Text::decode(with_nul_at(0, 10)).unwrap_err(), DecodeError::Binary);
    }

    #[test]
    fn nul_at_the_last_sniffed_byte_is_binary() {
        let bytes = with_nul_at(BINARY_SNIFF_BYTES - 1, 9000);
        assert_eq!(Text::decode(bytes).unwrap_err(), DecodeError::Binary);
    }

    #[test]
    fn nul_just_past_the_sniffed_bytes_is_text() {
        let bytes = with_nul_at(BINARY_SNIFF_BYTES, 9000);
        let text = decoded(bytes.clone());
        assert_eq!(text.content().as_bytes(), bytes.as_slice());
    }

    #[test]
    fn utf8_bom_skips_the_binary_check() {
        let text = decoded(with_prefix(&UTF8_BOM, b"a\0b"));
        assert_eq!((text.encoding(), text.content()), (Encoding::Utf8Bom, "a\0b"));
    }

    #[test]
    fn binary_check_runs_before_utf8_validation() {
        assert_eq!(Text::decode(b"\xFF\x00").unwrap_err(), DecodeError::Binary);
    }

    // Decoding errors

    #[test]
    fn invalid_utf8_reports_the_first_bad_byte() {
        assert_eq!(Text::decode(b"ab\xFFcd").unwrap_err(), DecodeError::InvalidUtf8 { offset: 2 });
    }

    #[test]
    fn invalid_utf8_offset_counts_the_bom() {
        assert_eq!(
            Text::decode(with_prefix(&UTF8_BOM, b"a\xFF")).unwrap_err(),
            DecodeError::InvalidUtf8 { offset: 4 }
        );
    }

    #[test]
    fn truncated_utf8_sequence_at_the_end_is_invalid() {
        assert_eq!(Text::decode(b"a\xE2\x82").unwrap_err(), DecodeError::InvalidUtf8 { offset: 1 });
    }

    #[test]
    fn odd_utf16_length_is_an_error() {
        assert_eq!(
            Text::decode(with_prefix(&UTF16LE_BOM, b"a")).unwrap_err(),
            DecodeError::OddUtf16Length
        );
    }

    #[test]
    fn unpaired_high_surrogate_at_the_end_is_an_error() {
        let bytes = with_prefix(&UTF16LE_BOM, &[b'a', 0, 0x3D, 0xD8]);
        assert_eq!(Text::decode(bytes).unwrap_err(), DecodeError::InvalidUtf16 { offset: 4 });
    }

    #[test]
    fn error_offset_counts_both_units_of_an_earlier_surrogate_pair() {
        // U+1F600 (D83D DE00) takes bytes 2..6; the lone DE00 starts at byte 6.
        let bytes = with_prefix(&UTF16LE_BOM, &[0x3D, 0xD8, 0x00, 0xDE, 0x00, 0xDE]);
        assert_eq!(Text::decode(bytes).unwrap_err(), DecodeError::InvalidUtf16 { offset: 6 });
    }

    #[test]
    fn lone_low_surrogate_is_an_error() {
        let bytes = with_prefix(&UTF16LE_BOM, &[0x00, 0xDE]);
        assert_eq!(Text::decode(bytes).unwrap_err(), DecodeError::InvalidUtf16 { offset: 2 });
    }

    #[test]
    fn high_surrogate_followed_by_a_plain_unit_is_an_error() {
        let bytes = with_prefix(&UTF16BE_BOM, &[0xD8, 0x3D, 0, b'a']);
        assert_eq!(Text::decode(bytes).unwrap_err(), DecodeError::InvalidUtf16 { offset: 2 });
    }

    // Line splitting

    #[test]
    fn single_newline_is_one_line() {
        assert_eq!(lines(&decoded(b"\n")), ["\n"]);
    }

    #[test]
    fn missing_final_newline_is_part_of_the_last_line() {
        assert_eq!(lines(&decoded(b"a\nb")), ["a\n", "b"]);
    }

    #[test]
    fn crlf_stays_with_its_line() {
        assert_eq!(lines(&decoded(b"a\r\nb\r\n")), ["a\r\n", "b\r\n"]);
    }

    #[test]
    fn lone_cr_does_not_end_a_line() {
        assert_eq!(lines(&decoded(b"a\rb\n")), ["a\rb\n"]);
        assert_eq!(lines(&decoded(b"\r")), ["\r"]);
    }

    #[test]
    fn mixed_endings_and_empty_lines_split_at_each_newline() {
        assert_eq!(lines(&decoded(b"a\r\nb\n\nc\r")), ["a\r\n", "b\n", "\n", "c\r"]);
    }

    #[test]
    fn utf16_lines_index_the_decoded_text() {
        let text = decoded(with_prefix(&UTF16LE_BOM, &[0xE9, 0, b'\n', 0, b'x', 0]));
        assert_eq!(lines(&text), ["é\n", "x"]);
    }

    #[test]
    fn lines_iterates_every_line_with_its_terminator() {
        let text = decoded(b"a\r\n\nb");
        assert_eq!(text.lines().collect::<Vec<_>>(), ["a\r\n", "\n", "b"]);
        assert_eq!(text.lines().len(), 3);
        assert_eq!(decoded(b"").lines().len(), 0);
    }

    #[test]
    #[should_panic(expected = "line 2 out of range for 2 lines")]
    fn line_past_the_end_panics() {
        let _ = decoded(b"a\nb\n").line(2);
    }

    // Dominant and typed line endings

    #[test]
    fn more_crlf_lines_make_crlf_dominant() {
        assert_eq!(decoded(b"a\r\nb\r\nc\n").dominant_ending(), Some(LineEnding::CrLf));
    }

    #[test]
    fn a_tie_goes_to_lf() {
        assert_eq!(decoded(b"a\r\nb\n").dominant_ending(), Some(LineEnding::Lf));
    }

    #[test]
    fn last_line_without_newline_does_not_count() {
        assert_eq!(decoded(b"a\r\nb").dominant_ending(), Some(LineEnding::CrLf));
    }

    #[test]
    fn lone_cr_is_not_crlf() {
        assert_eq!(decoded(b"a\rb\n").dominant_ending(), Some(LineEnding::Lf));
    }

    #[test]
    fn no_newline_has_no_dominant_ending() {
        assert_eq!(decoded(b"abc").dominant_ending(), None);
        assert_eq!(decoded(b"").dominant_ending(), None);
    }

    #[test]
    fn typed_ending_comes_from_base_first() {
        assert_eq!(typed_ending(&decoded(b"a\n"), &decoded(b"a\r\n")), LineEnding::Lf);
        assert_eq!(typed_ending(&decoded(b"a\r\n"), &decoded(b"a\n")), LineEnding::CrLf);
    }

    #[test]
    fn typed_ending_falls_back_to_left_then_lf() {
        assert_eq!(typed_ending(&decoded(b"abc"), &decoded(b"x\r\n")), LineEnding::CrLf);
        assert_eq!(typed_ending(&decoded(b""), &decoded(b"x")), LineEnding::Lf);
    }

    // Output encoding

    #[test]
    fn output_uses_base_encoding() {
        let base = decoded(with_prefix(&UTF16LE_BOM, &[b'a', 0]));
        assert_eq!(output_encoding(&base, &decoded(b"a")), Encoding::Utf16Le);
    }

    #[test]
    fn empty_base_uses_left_encoding() {
        let left = decoded(with_prefix(&UTF8_BOM, b"a"));
        assert_eq!(output_encoding(&decoded(b""), &left), Encoding::Utf8Bom);
        assert_eq!(output_encoding(&decoded(UTF16LE_BOM), &decoded(b"a")), Encoding::Utf8);
    }

    // Encoding output

    #[test]
    fn decode_then_encode_gives_the_input_bytes() {
        let inputs: [Vec<u8>; 5] = [
            b"a\r\nb\rc\nno newline".to_vec(),
            with_prefix(&UTF8_BOM, "é\n".as_bytes()),
            with_prefix(&UTF16LE_BOM, &[b'a', 0, b'\r', 0, b'\n', 0, 0x3D, 0xD8, 0x00, 0xDE]),
            with_prefix(&UTF16BE_BOM, &[0, b'a', 0, b'\n', 0xD8, 0x3D, 0xDE, 0x00]),
            Vec::new(),
        ];
        for input in inputs {
            let text = decoded(input.clone());
            assert_eq!(*text.encode(text.content()), input, "{input:?}");
        }
    }

    #[test]
    fn an_encoding_writes_its_byte_order_mark_and_code_units() {
        assert!(
            matches!(Encoding::Utf8.encode("é"), Cow::Borrowed(b) if b == [0xC3, 0xA9]),
            "UTF-8 output borrows the text"
        );
        assert_eq!(*Encoding::Utf8Bom.encode(""), UTF8_BOM);
        assert_eq!(*Encoding::Utf16Le.encode("a"), [0xFF, 0xFE, b'a', 0]);
        assert_eq!(*Encoding::Utf16Be.encode("a"), [0xFE, 0xFF, 0, b'a']);
    }

    #[test]
    fn encode_writes_new_text_in_the_file_encoding() {
        assert_eq!(*decoded(UTF16BE_BOM).encode("é\n"), [0xFE, 0xFF, 0x00, 0xE9, 0x00, 0x0A]);
        assert_eq!(*decoded(UTF8_BOM).encode("x"), [0xEF, 0xBB, 0xBF, b'x']);
        assert_eq!(*decoded(UTF16LE_BOM).encode("\u{1F600}"), [0xFF, 0xFE, 0x3D, 0xD8, 0x00, 0xDE]);
    }
}
