//! Characters that draw as nothing or as a plain space, and so can hide what a text holds
//! (spec §5): controls, format and bidirectional controls, zero-width characters, tag
//! characters, variation selectors, blank fillers, and the spaces other than U+0020.

use std::borrow::Cow;
use std::fmt::{self, Write as _};

/// Why a character is unusual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unusual {
    /// Draws nothing, or changes how the text around it draws: a control other than tab and
    /// line feed, a format or bidirectional control, a zero-width character, a tag character,
    /// a variation selector other than VS15 and VS16, or a blank filler.
    Invisible,
    /// Draws as a space but is not U+0020: no-break, ideographic and typographic spaces.
    Space,
}

/// Characters that draw nothing or reorder the text, in ascending order, as inclusive ranges.
/// Controls come from [`char::is_control`].
const INVISIBLE: &[(char, char)] = &[
    ('\u{00AD}', '\u{00AD}'),   // soft hyphen
    ('\u{034F}', '\u{034F}'),   // combining grapheme joiner
    ('\u{061C}', '\u{061C}'),   // Arabic letter mark
    ('\u{115F}', '\u{1160}'),   // Hangul choseong and jungseong fillers
    ('\u{17B4}', '\u{17B5}'),   // Khmer inherent vowels
    ('\u{180B}', '\u{180F}'),   // Mongolian variation selectors and vowel separator
    ('\u{200B}', '\u{200F}'),   // zero-width space, non-joiner, joiner; LRM, RLM
    ('\u{2028}', '\u{202E}'),   // line and paragraph separators; LRE, RLE, PDF, LRO, RLO
    ('\u{2060}', '\u{2064}'),   // word joiner and invisible operators
    ('\u{2066}', '\u{206F}'),   // LRI, RLI, FSI, PDI; deprecated format characters
    ('\u{2800}', '\u{2800}'),   // braille pattern blank
    ('\u{3164}', '\u{3164}'),   // Hangul filler
    ('\u{FE00}', '\u{FE0D}'),   // variation selectors 1–14
    ('\u{FEFF}', '\u{FEFF}'),   // zero-width no-break space, byte order mark
    ('\u{FFA0}', '\u{FFA0}'),   // halfwidth Hangul filler
    ('\u{FFF9}', '\u{FFFB}'),   // interlinear annotation controls
    ('\u{13430}', '\u{1343F}'), // Egyptian hieroglyph format controls
    ('\u{1BCA0}', '\u{1BCA3}'), // shorthand format controls
    ('\u{1D173}', '\u{1D17A}'), // musical symbol format controls
    ('\u{E0000}', '\u{E007F}'), // tag characters
    ('\u{E0100}', '\u{E01EF}'), // variation selectors 17–256
];

/// The spaces other than U+0020, in ascending order, as inclusive ranges.
const SPACES: &[(char, char)] = &[
    ('\u{00A0}', '\u{00A0}'), // no-break space
    ('\u{1680}', '\u{1680}'), // Ogham space mark
    ('\u{2000}', '\u{200A}'), // en quad to hair space
    ('\u{202F}', '\u{202F}'), // narrow no-break space
    ('\u{205F}', '\u{205F}'), // medium mathematical space
    ('\u{3000}', '\u{3000}'), // ideographic space
];

/// Whether `c` is unusual, and why. Tab and line feed are not.
#[must_use]
pub fn classify(c: char) -> Option<Unusual> {
    if c.is_ascii() {
        return (c.is_ascii_control() && !matches!(c, '\t' | '\n')).then_some(Unusual::Invisible);
    }
    if c.is_control() || within(INVISIBLE, c) {
        Some(Unusual::Invisible)
    } else if within(SPACES, c) {
        Some(Unusual::Space)
    } else {
        None
    }
}

fn within(ranges: &[(char, char)], c: char) -> bool {
    let after = ranges.partition_point(|&(_, last)| last < c);
    ranges.get(after).is_some_and(|&(first, _)| first <= c)
}

/// A character's code point as `U+` and at least four upper-case hex digits: `U+202E`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodePoint(pub char);

/// Fewest hex digits a code point is written with.
const MIN_HEX_DIGITS: u32 = 4;
/// Bits one hex digit holds.
const BITS_PER_HEX_DIGIT: u32 = 4;
/// The `U+` before the digits.
const PREFIX_LEN: usize = 2;

impl CodePoint {
    /// The characters the code point is written with, all ASCII: 6 for `U+202E`.
    #[must_use]
    pub fn width(self) -> usize {
        let bits = u32::from(self.0).bit_width();
        let digits = bits.div_ceil(BITS_PER_HEX_DIGIT).max(MIN_HEX_DIGITS);
        PREFIX_LEN + digits as usize
    }
}

impl fmt::Display for CodePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U+{:04X}", u32::from(self.0))
    }
}

/// `text` for a terminal: each unusual character as a JSON escape, `\u202e`, a character
/// past U+FFFF as its UTF-16 pair, `\udb40\udc41`. A terminal then shows what the text
/// holds, and runs no escape sequence it carries. A JSON document keeps its value, since
/// JSON reads the escape inside a string as the character, and these characters appear
/// nowhere else in it.
#[must_use]
pub fn escaped(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| classify(c).is_some()) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut units = [0; 2];
    for c in text.chars() {
        if classify(c).is_some() {
            for unit in c.encode_utf16(&mut units) {
                // Writing to a `String` cannot fail.
                let _ = write!(out, "\\u{unit:04x}");
            }
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// [`escaped`] for an owned `text`, without a copy when it holds nothing to escape.
#[must_use]
pub fn escaped_owned(text: String) -> String {
    match escaped(&text) {
        Cow::Borrowed(_) => text,
        Cow::Owned(escaped) => escaped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_borrowed() {
        let text = "src/a b.rs\tx\nline two é 中\n";
        assert!(matches!(escaped(text), Cow::Borrowed(t) if t == text));
    }

    #[test]
    fn escape_sequences_and_carriage_returns_are_escaped() {
        assert_eq!(escaped("a\u{1B}]8;;x\u{7}b\rc"), r"a\u001b]8;;x\u0007b\u000dc");
        assert_eq!(escaped("\u{9B}31m"), r"\u009b31m");
    }

    #[test]
    fn bidirectional_controls_and_odd_spaces_are_escaped() {
        assert_eq!(escaped("if a\u{202E} {\u{2066}"), r"if a\u202e {\u2066");
        assert_eq!(escaped("x\u{00A0}y"), r"x\u00a0y");
    }

    #[test]
    fn a_character_past_the_basic_plane_becomes_its_utf16_pair() {
        // U+E0041 is D800 + (0xE0041 - 0x10000) >> 10 = DB40, DC00 + 0x41 = DC41.
        assert_eq!(escaped("a\u{E0041}"), r"a\udb40\udc41");
    }

    #[test]
    fn an_escaped_json_string_keeps_its_value() {
        let value = "x\u{202E}\u{E0041}\u{A0}";
        let json = serde_json::to_string(value).expect("serialises");
        let parsed: String = serde_json::from_str(&escaped(&json)).expect("still JSON");
        assert_eq!(parsed, value);
    }

    fn all(chars: &[char]) -> Vec<Option<Unusual>> {
        chars.iter().map(|&c| classify(c)).collect()
    }

    #[test]
    fn printable_ascii_tab_and_line_feed_are_usual() {
        let usual: Vec<char> = (' '..='~').chain(['\t', '\n']).collect();
        assert_eq!(all(&usual), vec![None; usual.len()]);
    }

    #[test]
    fn every_other_control_is_invisible() {
        let controls =
            ['\0', '\u{7}', '\r', '\u{1B}', '\u{1F}', '\u{7F}', '\u{80}', '\u{9B}', '\u{9F}'];
        assert_eq!(all(&controls), vec![Some(Unusual::Invisible); controls.len()]);
    }

    #[test]
    fn bidirectional_controls_are_invisible() {
        // The Trojan Source characters: LRE, RLE, PDF, LRO, RLO, LRI, RLI, FSI, PDI, and the
        // marks LRM, RLM and ALM.
        let bidi = [
            '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}',
            '\u{2068}', '\u{2069}', '\u{200E}', '\u{200F}', '\u{061C}',
        ];
        assert_eq!(all(&bidi), vec![Some(Unusual::Invisible); bidi.len()]);
    }

    #[test]
    fn zero_width_and_filler_characters_are_invisible() {
        let hidden = [
            '\u{200B}',
            '\u{200C}',
            '\u{200D}',
            '\u{2060}',
            '\u{FEFF}',
            '\u{00AD}',
            '\u{3164}',
            '\u{115F}',
            '\u{FFA0}',
            '\u{E0001}',
            '\u{E0041}',
            '\u{E007F}',
            '\u{FE00}',
            '\u{E0100}',
            '\u{E01EF}',
            '\u{2028}',
            '\u{2029}',
            '\u{2800}',
        ];
        assert_eq!(all(&hidden), vec![Some(Unusual::Invisible); hidden.len()]);
    }

    #[test]
    fn spaces_other_than_u0020_are_spaces() {
        let spaces = [
            '\u{00A0}', '\u{1680}', '\u{2000}', '\u{2007}', '\u{200A}', '\u{202F}', '\u{205F}',
            '\u{3000}',
        ];
        assert_eq!(all(&spaces), vec![Some(Unusual::Space); spaces.len()]);
    }

    #[test]
    fn the_neighbours_of_each_range_are_usual() {
        // Hyphen after the zero-width range, per mille after the narrow no-break space, the
        // unassigned U+2065 between the invisible operators and the isolates, the first
        // character past the tag and variation selector blocks, and the code points either
        // side of the Hangul filler.
        let usual = [
            '\u{2010}',
            '\u{2030}',
            '\u{2065}',
            '\u{E0080}',
            '\u{E01F0}',
            '\u{3163}',
            '\u{3165}',
            '\u{1FFF}',
            '\u{27FF}',
            '\u{2801}',
        ];
        assert_eq!(all(&usual), vec![None; usual.len()]);
    }

    #[test]
    fn letters_emoji_and_emoji_presentation_selectors_are_usual() {
        // é, Cyrillic а, a CJK ideograph, an emoji, VS15 and VS16.
        let usual = ['é', 'а', '中', '😀', '\u{FE0E}', '\u{FE0F}'];
        assert_eq!(all(&usual), vec![None; usual.len()]);
    }

    #[test]
    fn a_code_points_width_is_the_length_it_is_written_with() {
        let chars =
            ['\0', '\u{1B}', '\u{202E}', '\u{FFFF}', '\u{10000}', '\u{E0041}', '\u{10FFFF}'];
        let widths: Vec<usize> = chars.iter().map(|&c| CodePoint(c).width()).collect();
        assert_eq!(widths, [6, 6, 6, 6, 7, 7, 8]);
        assert!(chars.iter().all(|&c| CodePoint(c).width() == CodePoint(c).to_string().len()));
    }

    #[test]
    fn a_code_point_has_at_least_four_hex_digits() {
        assert_eq!(CodePoint('\u{202E}').to_string(), "U+202E");
        assert_eq!(CodePoint('\u{1B}').to_string(), "U+001B");
        assert_eq!(CodePoint('\u{E0041}').to_string(), "U+E0041");
    }
}
