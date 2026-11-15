// Triad: new file. Unusual characters as badges and shaded spaces. See PATCHES.md.
//! Characters an application marks: a badge character draws as a box showing its code point,
//! `U+202E`, and a shaded one draws on a grey background.
//!
//! A badge character is shaped as spaces of its byte length, so the shaper gives it a width
//! and no bidirectional control reorders the text around it. Its glyphs then become one,
//! as wide as the badge, by moving every glyph after it to the right, as tab stops do. The
//! caret, hit testing and selection read glyph positions, so they follow.

use std::borrow::Cow;
use std::sync::Arc;

use gpui::{
    App, Corners, Hsla, LineLayout, Pixels, Point, ShapedGlyph, ShapedLine, ShapedRun,
    TextAlign, Window, fill, point, px, size,
};

use crate::theme::ActiveTheme as _;

/// How an application marks a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharMark {
    /// Draw a box showing the code point in place of the character.
    Badge,
    /// Draw the character on a grey background.
    Shade,
}

/// Which characters an application marks, and how.
pub type CharMarker = fn(char) -> Option<CharMark>;

/// Space between a badge's edge and its label.
const BADGE_PADDING: Pixels = px(2.);
/// Space between a badge or shade and the top and bottom of its row.
const MARK_INSET: Pixels = px(1.);
/// Corner radius of a badge.
const BADGE_RADIUS: Pixels = px(3.);
/// The opacity of the muted text colour behind a marked character.
const MARK_FILL_OPACITY: f32 = 0.25;

/// The fill behind a marked character and the colour of a badge's label: the muted text
/// colour at 25 % opacity, and the muted text colour.
pub fn char_mark_colours(cx: &App) -> (Hsla, Hsla) {
    let text = cx.theme().tokens.colors.muted_foreground;
    (text.opacity(MARK_FILL_OPACITY), text)
}

/// The label of `c`'s badge: `U+` and at least four upper-case hex digits.
pub(crate) fn badge_label(c: char) -> String {
    format!("U+{:04X}", u32::from(c))
}

/// The width of `c`'s badge, with each label character `digit` wide.
fn badge_width(c: char, digit: Pixels) -> Pixels {
    digit * badge_label(c).len() as f32 + BADGE_PADDING * 2.
}

/// The part of `text` whose characters are marked: all of it, less a final `\r` when `text`
/// ends its line, since that `\r` belongs to the line ending.
fn marked_part(text: &str, ends_line: bool) -> &str {
    if ends_line {
        text.strip_suffix('\r').unwrap_or(text)
    } else {
        text
    }
}

/// `text` as it is shaped: each badge character as spaces of its byte length. `ends_line`
/// says whether `text` ends its line.
pub(crate) fn masked(text: &str, marker: Option<CharMarker>, ends_line: bool) -> Cow<'_, str> {
    let Some(marker) = marker else {
        return Cow::Borrowed(text);
    };
    let part = marked_part(text, ends_line);
    let is_badge = |c: char| marker(c) == Some(CharMark::Badge);
    if !part.chars().any(is_badge) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for c in part.chars() {
        if is_badge(c) {
            out.extend(std::iter::repeat_n(' ', c.len_utf8()));
        } else {
            out.push(c);
        }
    }
    out.push_str(&text[part.len()..]);
    Cow::Owned(out)
}

/// Each badge character of `text`: its byte offset and the extra width its badge takes in
/// `layout`, shaped from [`masked`] text, with label characters `digit` wide.
fn badge_shifts(
    layout: &LineLayout,
    text: &str,
    marker: CharMarker,
    digit: Pixels,
) -> Vec<(usize, usize, Pixels)> {
    text.char_indices()
        .filter(|&(_, c)| marker(c) == Some(CharMark::Badge))
        .map(|(ix, c)| {
            let end = ix + c.len_utf8();
            let shaped = layout.x_for_index(end) - layout.x_for_index(ix);
            (ix, end, badge_width(c, digit) - shaped)
        })
        .collect()
}

/// `layout`, shaped from [`masked`] `text`, with each badge character one glyph as wide as
/// its badge. None when `text` has no badge character.
fn widened_layout(
    layout: &LineLayout,
    text: &str,
    marker: CharMarker,
    digit: Pixels,
) -> Option<LineLayout> {
    let badges = badge_shifts(layout, text, marker, digit);
    if badges.is_empty() {
        return None;
    }
    // The glyph at a badge's start moves by the badges before it; a glyph at or past its end
    // also by the badge itself. Glyphs inside a badge go.
    let shift_at = |index: usize| -> Option<Pixels> {
        let mut shift = px(0.);
        for &(start, end, extra) in &badges {
            if index < start || index == start {
                break;
            }
            if index < end {
                return None;
            }
            shift += extra;
        }
        Some(shift)
    };
    let total = badges.iter().map(|&(_, _, extra)| extra).fold(px(0.), |a, b| a + b);
    let runs = layout
        .runs
        .iter()
        .map(|run| ShapedRun {
            font_id: run.font_id,
            glyphs: run
                .glyphs
                .iter()
                .filter_map(|glyph| {
                    shift_at(glyph.index).map(|shift| ShapedGlyph {
                        position: point(glyph.position.x + shift, glyph.position.y),
                        ..glyph.clone()
                    })
                })
                .collect(),
        })
        .collect();
    Some(LineLayout {
        font_size: layout.font_size,
        width: layout.width + total,
        ascent: layout.ascent,
        descent: layout.descent,
        runs,
        len: layout.len,
    })
}

/// Widen each badge character of `line`, shaped from [`masked`] `text`, to its badge.
pub(crate) fn widen(
    line: &mut ShapedLine,
    text: &str,
    marker: Option<CharMarker>,
    digit: Pixels,
    ends_line: bool,
) {
    let text = marked_part(text, ends_line);
    if let Some(layout) = marker.and_then(|marker| widened_layout(line, text, marker, digit)) {
        **line = Arc::new(layout);
    }
}

/// The layout of `text`, shaped from its [`masked`] text, with its badges widened.
pub(crate) fn widened(
    layout: Arc<LineLayout>,
    text: &str,
    marker: Option<CharMarker>,
    digit: Pixels,
    ends_line: bool,
) -> Arc<LineLayout> {
    let text = marked_part(text, ends_line);
    marker
        .and_then(|marker| widened_layout(&layout, text, marker, digit))
        .map_or(layout, Arc::new)
}

/// A marked character as it paints on one visual line.
#[derive(Clone)]
pub(crate) struct MarkPaint {
    /// The visual line of the logical line it sits on.
    pub(crate) line_index: usize,
    pub(crate) x: Pixels,
    pub(crate) width: Pixels,
    /// A badge's shaped label; none for a shaded character.
    pub(crate) label: Option<ShapedLine>,
}

/// The marked characters of visual line `line_index`, `text` laid out as `line`.
pub(crate) fn marks_of(
    line: &ShapedLine,
    text: &str,
    line_index: usize,
    marker: Option<CharMarker>,
    ends_line: bool,
    mut shape_label: impl FnMut(String) -> ShapedLine,
) -> Vec<MarkPaint> {
    let Some(marker) = marker else {
        return Vec::new();
    };
    marked_part(text, ends_line)
        .char_indices()
        .filter_map(|(ix, c)| {
            let mark = marker(c)?;
            let x = line.x_for_index(ix);
            let width = line.x_for_index(ix + c.len_utf8()) - x;
            let label = (mark == CharMark::Badge).then(|| shape_label(badge_label(c)));
            Some(MarkPaint { line_index, x, width, label })
        })
        .collect()
}

/// Paint the fills of `marks`, each visual line `line_height` high and offset by `indent`.
pub(crate) fn paint_fills(
    marks: &[MarkPaint],
    pos: Point<Pixels>,
    line_height: Pixels,
    indent: impl Fn(usize) -> Pixels,
    colour: Hsla,
    window: &mut Window,
) {
    for mark in marks {
        let origin = point(
            pos.x + indent(mark.line_index) + mark.x,
            pos.y + line_height * mark.line_index as f32 + MARK_INSET,
        );
        let radius = if mark.label.is_some() { BADGE_RADIUS } else { px(0.) };
        let bounds = gpui::Bounds::new(origin, size(mark.width, line_height - MARK_INSET * 2.));
        window.paint_quad(fill(bounds, colour).corner_radii(Corners::all(radius)));
    }
}

/// Paint the labels of `marks`' badges, as [`paint_fills`] places them.
pub(crate) fn paint_labels(
    marks: &[MarkPaint],
    pos: Point<Pixels>,
    line_height: Pixels,
    indent: impl Fn(usize) -> Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    for mark in marks {
        let Some(label) = &mark.label else {
            continue;
        };
        let origin = point(
            pos.x + indent(mark.line_index) + mark.x + BADGE_PADDING,
            pos.y + line_height * mark.line_index as f32,
        );
        _ = label.paint(origin, line_height, TextAlign::Left, None, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FontId, GlyphId};

    const ADVANCE: f32 = 10.;
    const RLO: char = '\u{202E}';

    fn marker(c: char) -> Option<CharMark> {
        match c {
            '\u{202E}' | '\u{200B}' | '\u{E0041}' => Some(CharMark::Badge),
            '\u{A0}' => Some(CharMark::Shade),
            _ => None,
        }
    }

    /// A line shaped from `text`, every character one glyph `ADVANCE` wide.
    fn layout(text: &str) -> LineLayout {
        let glyphs: Vec<_> = text
            .char_indices()
            .enumerate()
            .map(|(n, (index, _))| ShapedGlyph {
                id: GlyphId(0),
                position: point(px(n as f32 * ADVANCE), px(0.)),
                index,
                is_emoji: false,
            })
            .collect();
        LineLayout {
            font_size: px(10.),
            width: px(glyphs.len() as f32 * ADVANCE),
            ascent: px(8.),
            descent: px(2.),
            runs: vec![ShapedRun { font_id: FontId(0), glyphs }],
            len: text.len(),
        }
    }

    fn glyphs(layout: &LineLayout) -> Vec<(usize, f32)> {
        layout.runs[0]
            .glyphs
            .iter()
            .map(|glyph| (glyph.index, f32::from(glyph.position.x)))
            .collect()
    }

    fn cr_marker(c: char) -> Option<CharMark> {
        (c == '\r').then_some(CharMark::Badge).or_else(|| marker(c))
    }

    #[test]
    fn a_badge_character_is_shaped_as_spaces_of_its_length() {
        let text = format!("a{RLO}b\u{A0}");
        assert_eq!(masked(&text, Some(marker), true), "a   b\u{A0}");
        assert!(matches!(masked("ab\u{A0}", Some(marker), true), Cow::Borrowed(_)));
        assert!(matches!(masked(&text, None, true), Cow::Borrowed(_)));
    }

    #[test]
    fn a_carriage_return_that_ends_the_line_is_not_marked() {
        assert_eq!(masked("a\rb\r", Some(cr_marker), true), "a b\r");
        assert_eq!(masked("a\rb\r", Some(cr_marker), false), "a b ");
        assert!(matches!(masked("ab\r", Some(cr_marker), true), Cow::Borrowed(_)));
    }

    #[test]
    fn a_badge_is_as_wide_as_its_label_and_padding() {
        // U+202E is six characters of 10 px, with 2 px each side.
        assert_eq!(badge_width(RLO, px(ADVANCE)), px(64.));
        assert_eq!(badge_width('\u{E0041}', px(ADVANCE)), px(74.));
    }

    #[test]
    fn a_badge_character_becomes_one_glyph_as_wide_as_its_badge() {
        // a at 0; RLO shaped as three spaces at 10, 20, 30; b at 40. The badge is 64 px, so
        // b moves to 10 + 64.
        let text = format!("a{RLO}b");
        let widened = widened_layout(&layout(&masked(&text, Some(marker), true)), &text, marker, px(ADVANCE))
            .unwrap();
        assert_eq!(glyphs(&widened), [(0, 0.), (1, 10.), (4, 74.)]);
        assert_eq!(widened.width, px(84.));
        assert_eq!(widened.x_for_index(4) - widened.x_for_index(1), px(64.));
    }

    #[test]
    fn badges_in_a_row_each_take_their_width() {
        // Two badges of 64 px from 0, then x at 128.
        let text = format!("{RLO}\u{200B}x");
        let widened = widened_layout(&layout(&masked(&text, Some(marker), true)), &text, marker, px(ADVANCE))
            .unwrap();
        assert_eq!(glyphs(&widened), [(0, 0.), (3, 64.), (6, 128.)]);
    }

    #[test]
    fn a_click_inside_a_badge_lands_at_one_of_its_ends() {
        let text = format!("a{RLO}b");
        let widened = widened_layout(&layout(&masked(&text, Some(marker), true)), &text, marker, px(ADVANCE))
            .unwrap();
        // The badge spans 10..74; 20 is nearer its start, 70 nearer its end.
        assert_eq!(widened.closest_index_for_x(px(20.)), 1);
        assert_eq!(widened.closest_index_for_x(px(70.)), 4);
    }

    #[test]
    fn a_line_without_badges_is_left_alone() {
        let text = "ab\u{A0}";
        assert!(widened_layout(&layout(text), text, marker, px(ADVANCE)).is_none());
    }
}
