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
    /// The fallback for text that is not UTF-8 and has no byte order mark: one byte per
    /// character, each of the 256 bytes mapping to one character and back (spec §4.1).
    Windows1252,
}

/// A character `encoding` cannot hold, on the 1-based `line` of the text being encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{} cannot hold {character:?} (U+{:04X}) on line {line}", .encoding.name(), u32::from(*.character))]
pub struct EncodeError {
    pub encoding: Encoding,
    pub character: char,
    pub line: usize,
}

impl Encoding {
    /// The encoding's name, as an error names it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 with BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Windows1252 => "Windows-1252",
        }
    }

    /// The byte order mark a file in this encoding starts with; empty for plain UTF-8.
    #[must_use]
    pub fn bom(self) -> &'static [u8] {
        match self {
            Encoding::Utf8 | Encoding::Windows1252 => &[],
            Encoding::Utf8Bom => &UTF8_BOM,
            Encoding::Utf16Le => &UTF16LE_BOM,
            Encoding::Utf16Be => &UTF16BE_BOM,
        }
    }

    /// `text` in this encoding without the byte order mark, so a text split at `char`
    /// boundaries encodes piece by piece. UTF-8 borrows it.
    ///
    /// # Errors
    ///
    /// [`EncodeError`] for the first character the encoding cannot hold, its line counted
    /// from the start of `text`.
    pub fn encode_chunk(self, text: &str) -> Result<Cow<'_, [u8]>, EncodeError> {
        Ok(match self {
            Encoding::Utf8 | Encoding::Utf8Bom => Cow::Borrowed(text.as_bytes()),
            Encoding::Utf16Le => {
                text.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>().into()
            }
            Encoding::Utf16Be => {
                text.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>().into()
            }
            Encoding::Windows1252 => text
                .char_indices()
                .map(|(at, c)| {
                    windows_1252_byte(c).ok_or_else(|| EncodeError {
                        encoding: self,
                        character: c,
                        line: line_number(text, at),
                    })
                })
                .collect::<Result<Vec<u8>, _>>()?
                .into(),
        })
    }

    /// Whether this encoding holds every character of `chunks`, pieces of one text in order,
    /// without encoding them.
    ///
    /// # Errors
    ///
    /// [`EncodeError`] for the first character it cannot hold, its line counted over all
    /// the pieces.
    pub fn holds<'t>(self, chunks: impl IntoIterator<Item = &'t str>) -> Result<(), EncodeError> {
        if self != Encoding::Windows1252 {
            return Ok(());
        }
        let mut lines_before = 0;
        for chunk in chunks {
            if let Some((at, c)) =
                chunk.char_indices().find(|&(_, c)| windows_1252_byte(c).is_none())
            {
                let line = lines_before + line_number(chunk, at);
                return Err(EncodeError { encoding: self, character: c, line });
            }
            lines_before += chunk.bytes().filter(|&byte| byte == b'\n').count();
        }
        Ok(())
    }

    /// `text` in this encoding, after its byte order mark if it has one. UTF-8 borrows it.
    ///
    /// # Errors
    ///
    /// [`EncodeError`] for the first character the encoding cannot hold.
    pub fn encode(self, text: &str) -> Result<Cow<'_, [u8]>, EncodeError> {
        let bom = self.bom();
        let body = self.encode_chunk(text)?;
        Ok(if bom.is_empty() { body } else { Cow::Owned([bom, &body].concat()) })
    }
}

/// The 1-based line of `text` that byte `at` lies on.
fn line_number(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// Windows-1252's characters for the bytes 0x80–0x9F, from WHATWG's index: the five bytes the
/// code page leaves unassigned (0x81, 0x8D, 0x8F, 0x90, 0x9D) take the C1 control of the
/// same value. Every other byte is the character of the same value, as in Latin-1.
const WINDOWS_1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];
/// The first byte [`WINDOWS_1252_HIGH`] covers.
const WINDOWS_1252_HIGH_START: u8 = 0x80;

/// The character Windows-1252 gives `byte`.
fn windows_1252_char(byte: u8) -> char {
    match byte.checked_sub(WINDOWS_1252_HIGH_START) {
        Some(at) if usize::from(at) < WINDOWS_1252_HIGH.len() => WINDOWS_1252_HIGH[usize::from(at)],
        _ => char::from(byte),
    }
}

/// The byte Windows-1252 gives `c`, if it holds it.
fn windows_1252_byte(c: char) -> Option<u8> {
    let high = WINDOWS_1252_HIGH_START..WINDOWS_1252_HIGH_START + 32;
    match u8::try_from(u32::from(c)) {
        Ok(byte) if !high.contains(&byte) => Some(byte),
        _ => WINDOWS_1252_HIGH
            .iter()
            .position(|&high| high == c)
            .and_then(|at| u8::try_from(at).ok())
            .map(|at| WINDOWS_1252_HIGH_START + at),
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
    /// Decode a file's bytes: byte order mark first, then the binary check, then UTF-8, then
    /// Windows-1252, which takes any bytes. UTF-8 decodes in place, in the buffer read from
    /// the file.
    ///
    /// # Errors
    ///
    /// Binary input; after a byte order mark, invalid UTF-8, an unpaired UTF-16 surrogate or
    /// an odd UTF-16 length.
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
            match String::from_utf8(bytes) {
                Ok(content) => (Encoding::Utf8, content),
                Err(error) => (Encoding::Windows1252, decode_windows_1252(error.as_bytes())),
            }
        };
        Ok(Text::new(encoding, content))
    }

    /// What [`Text::decode`] would make of `bytes`, without building the text: the encoding,
    /// or the same error.
    ///
    /// # Errors
    ///
    /// As [`Text::decode`].
    pub fn check(bytes: &[u8]) -> Result<Encoding, DecodeError> {
        if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
            check_utf8(rest, UTF8_BOM.len()).map(|()| Encoding::Utf8Bom)
        } else if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
            check_utf16(rest, u16::from_le_bytes).map(|()| Encoding::Utf16Le)
        } else if let Some(rest) = bytes.strip_prefix(&UTF16BE_BOM) {
            check_utf16(rest, u16::from_be_bytes).map(|()| Encoding::Utf16Be)
        } else if bytes.iter().take(BINARY_SNIFF_BYTES).any(|&byte| byte == 0) {
            Err(DecodeError::Binary)
        } else if std::str::from_utf8(bytes).is_ok() {
            Ok(Encoding::Utf8)
        } else {
            Ok(Encoding::Windows1252)
        }
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
    ///
    /// # Errors
    ///
    /// [`EncodeError`] for the first character the encoding cannot hold.
    pub fn encode<'a>(&self, text: &'a str) -> Result<Cow<'a, [u8]>, EncodeError> {
        self.encoding.encode(text)
    }

    /// The file's bytes: its content encoded again, with its byte order mark.
    ///
    /// # Panics
    ///
    /// Never: decoding gave every character of the content, so its encoding holds each.
    #[must_use]
    pub fn bytes(&self) -> Cow<'_, [u8]> {
        #[expect(
            clippy::expect_used,
            reason = "invariant: decoding gave every character of the content, so its encoding holds each"
        )]
        self.encoding.encode(self.content()).expect("a decoded text encodes in its own encoding")
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

/// `bytes`, one character each.
fn decode_windows_1252(bytes: &[u8]) -> String {
    bytes.iter().copied().map(windows_1252_char).collect()
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

fn check_utf8(bytes: &[u8], offset: usize) -> Result<(), DecodeError> {
    std::str::from_utf8(bytes)
        .map(drop)
        .map_err(|error| DecodeError::InvalidUtf8 { offset: offset + error.valid_up_to() })
}

/// `bytes` follow a two-byte byte order mark.
fn check_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Result<(), DecodeError> {
    let (pairs, []) = bytes.as_chunks::<UTF16_UNIT_BYTES>() else {
        return Err(DecodeError::OddUtf16Length);
    };
    char::decode_utf16(pairs.iter().copied().map(unit))
        .try_fold(0, |units_read, decoded| match decoded {
            Ok(c) => Ok(units_read + c.len_utf16()),
            Err(_) => Err(DecodeError::InvalidUtf16 {
                offset: UTF16_BOM_LEN + units_read * UTF16_UNIT_BYTES,
            }),
        })
        .map(drop)
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
        assert_eq!(&*Encoding::Utf8.encode_chunk("é\n").unwrap(), [0xC3, 0xA9, 0x0A]);
        assert_eq!(&*Encoding::Utf8Bom.encode_chunk("é\n").unwrap(), [0xC3, 0xA9, 0x0A]);
        assert_eq!(&*Encoding::Utf16Le.encode_chunk("é\n").unwrap(), [0xE9, 0x00, 0x0A, 0x00]);
        assert_eq!(&*Encoding::Utf16Be.encode_chunk("é\n").unwrap(), [0x00, 0xE9, 0x00, 0x0A]);
        // U+1F600 is the surrogate pair D83D DE00.
        assert_eq!(&*Encoding::Utf16Le.encode_chunk("😀").unwrap(), [0x3D, 0xD8, 0x00, 0xDE]);
        assert!(matches!(Encoding::Utf8Bom.encode_chunk("a").unwrap(), Cow::Borrowed(_)));
    }

    #[test]
    fn a_text_encodes_as_its_mark_then_its_chunks() {
        for encoding in [Encoding::Utf8, Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be] {
            let pieces = [
                encoding.bom(),
                &encoding.encode_chunk("ab").unwrap(),
                &encoding.encode_chunk("é\n").unwrap(),
            ]
            .concat();
            assert_eq!(&*encoding.encode("abé\n").unwrap(), pieces, "{encoding:?}");
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

    #[test]
    fn check_agrees_with_decode() {
        let cases: [&[u8]; 12] = [
            b"",
            b"plain\n",
            b"\xEF\xBB\xBFbom\n",
            b"\xEF\xBB\xBFa\0b",
            b"ab\xFFcd",
            b"\xEF\xBB\xBFab\xFF",
            b"\xFF\xFEa\0b\0",
            b"\xFE\xFF\0a\0b",
            b"\xFF\xFEa\0b",
            b"\xFF\xFEa\0\x00\xD8b\0",
            b"\xFE\xFF\0a\xDC\x00",
            b"a\0b",
        ];
        for bytes in cases {
            let decoded = Text::decode(bytes).map(|text| text.encoding());
            assert_eq!(Text::check(bytes), decoded, "{bytes:?}");
        }
        // The values decode gives, by hand: an error offset counts the byte order mark.
        assert_eq!(Text::check(b"\xEF\xBB\xBFab\xFF"), Err(DecodeError::InvalidUtf8 { offset: 5 }));
        assert_eq!(
            Text::check(b"\xFF\xFEa\0\x00\xD8b\0"),
            Err(DecodeError::InvalidUtf16 { offset: 4 })
        );
        assert_eq!(Text::check(b"\xFF\xFEa\0b"), Err(DecodeError::OddUtf16Length));
        assert_eq!(Text::check(b"\xFE\xFF\0a\0b"), Ok(Encoding::Utf16Be));
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

    // Windows-1252 fallback

    #[test]
    fn text_that_is_not_utf8_decodes_as_windows_1252() {
        // Latin-1's `é` is 0xE9; Windows-1252's curly quotes and euro sign sit in 0x80–0x9F.
        let text = decoded(b"caf\xE9\n\x93quoted\x94 \x80\n".to_vec());
        assert_eq!(text.encoding(), Encoding::Windows1252);
        assert_eq!(text.content(), "café\n\u{201C}quoted\u{201D} \u{20AC}\n");
        assert_eq!(text.line_count(), 2);
        assert_eq!(Text::check(b"caf\xE9\n"), Ok(Encoding::Windows1252));
    }

    #[test]
    fn a_truncated_utf8_sequence_decodes_as_windows_1252() {
        // `€` in UTF-8 is E2 82 AC; without its last byte the file is not UTF-8.
        let text = decoded(b"a\xE2\x82".to_vec());
        assert_eq!((text.encoding(), text.content()), (Encoding::Windows1252, "a\u{E2}\u{201A}"));
    }

    #[test]
    fn valid_utf8_stays_utf8() {
        let text = decoded("café\n".as_bytes().to_vec());
        assert_eq!((text.encoding(), text.content()), (Encoding::Utf8, "café\n"));
    }

    #[test]
    fn every_byte_but_nul_decodes_as_whatwg_windows_1252_and_encodes_back() {
        let bytes: Vec<u8> = (1..=u8::MAX).collect();
        let text = decoded(bytes.clone());
        assert_eq!(text.encoding(), Encoding::Windows1252);
        let (oracle, _, malformed) = encoding_rs::WINDOWS_1252.decode(&bytes);
        assert!(!malformed);
        assert_eq!(text.content(), oracle);
        assert_eq!(text.bytes().as_ref(), bytes.as_slice());
        assert_eq!(Encoding::Windows1252.encode(text.content()).unwrap().as_ref(), bytes);
    }

    #[test]
    fn the_unassigned_bytes_are_c1_controls_and_encode_back() {
        let text = decoded(b"\x81\x8D\x8F\x90\x9D\xFF".to_vec());
        assert_eq!(text.content(), "\u{81}\u{8D}\u{8F}\u{90}\u{9D}\u{FF}");
        assert_eq!(text.bytes().as_ref(), b"\x81\x8D\x8F\x90\x9D\xFF");
    }

    #[test]
    fn windows_1252_refuses_a_character_it_cannot_hold_and_names_its_line() {
        let encoding = Encoding::Windows1252;
        let error = encoding.encode("ok €\nfine\nZdzisław\n").unwrap_err();
        assert_eq!(error, EncodeError { encoding, character: 'ł', line: 3 });
        assert_eq!(error.to_string(), "Windows-1252 cannot hold 'ł' (U+0142) on line 3");
        // A C1 control Windows-1252 assigns elsewhere: 0x80 is `€`, so U+0080 has no byte.
        let error = encoding.encode("\u{80}").unwrap_err();
        assert_eq!(error, EncodeError { encoding, character: '\u{80}', line: 1 });
        // An emoji, past the Basic Multilingual Plane.
        assert_eq!(encoding.encode_chunk("a\n😀").unwrap_err().line, 2);
    }

    #[test]
    fn holds_counts_lines_across_chunks() {
        let encoding = Encoding::Windows1252;
        assert_eq!(encoding.holds(["a\nb", "c\n", "d\n€"]), Ok(()));
        assert_eq!(
            encoding.holds(["a\nb", "c\n", "d\nł"]),
            Err(EncodeError { encoding, character: 'ł', line: 4 })
        );
        assert_eq!(Encoding::Utf16Le.holds(["ł"]), Ok(()));
    }

    #[test]
    fn utf_encodings_hold_every_character() {
        for encoding in [Encoding::Utf8, Encoding::Utf8Bom, Encoding::Utf16Le, Encoding::Utf16Be] {
            let bytes = encoding.encode("ł😀\u{80}").unwrap().into_owned();
            let text = decoded(bytes);
            assert_eq!((text.encoding(), text.content()), (encoding, "ł😀\u{80}"));
        }
    }

    #[test]
    fn invalid_utf8_offset_counts_the_bom() {
        assert_eq!(
            Text::decode(with_prefix(&UTF8_BOM, b"a\xFF")).unwrap_err(),
            DecodeError::InvalidUtf8 { offset: 4 }
        );
    }

    #[test]
    fn invalid_utf8_after_a_utf8_bom_stays_an_error() {
        assert_eq!(
            Text::decode(with_prefix(&UTF8_BOM, b"caf\xE9")).unwrap_err(),
            DecodeError::InvalidUtf8 { offset: 6 }
        );
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
            assert_eq!(*text.encode(text.content()).unwrap(), input, "{input:?}");
        }
    }

    #[test]
    fn an_encoding_writes_its_byte_order_mark_and_code_units() {
        assert!(
            matches!(Encoding::Utf8.encode("é").unwrap(), Cow::Borrowed(b) if b == [0xC3, 0xA9]),
            "UTF-8 output borrows the text"
        );
        assert_eq!(*Encoding::Utf8Bom.encode("").unwrap(), UTF8_BOM);
        assert_eq!(*Encoding::Utf16Le.encode("a").unwrap(), [0xFF, 0xFE, b'a', 0]);
        assert_eq!(*Encoding::Utf16Be.encode("a").unwrap(), [0xFE, 0xFF, 0, b'a']);
    }

    #[test]
    fn encode_writes_new_text_in_the_file_encoding() {
        assert_eq!(
            *decoded(UTF16BE_BOM).encode("é\n").unwrap(),
            [0xFE, 0xFF, 0x00, 0xE9, 0x00, 0x0A]
        );
        assert_eq!(*decoded(UTF8_BOM).encode("x").unwrap(), [0xEF, 0xBB, 0xBF, b'x']);
        assert_eq!(
            *decoded(UTF16LE_BOM).encode("\u{1F600}").unwrap(),
            [0xFF, 0xFE, 0x3D, 0xD8, 0x00, 0xDE]
        );
    }
}
