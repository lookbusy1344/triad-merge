//! The overview ruler (`docs/DESIGN.md` §E4): a strip beside the result pane, and beside
//! the diff window's right pane, with a mark for every region that is not unchanged, placed
//! by its lines' share of the whole text. Marks are visual only; the navigation commands reach
//! every region from the keyboard.

use std::ops::Range;

use merge_core::diff2::{DiffDocument, Kind2, Side};
use merge_core::document::MergeDocument;
use merge_core::index::RegionIx;
use merge_core::merge3::Kind;

use crate::diff::tone;
use crate::marks::{Tone, mark};

/// How a mark paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// The full width in the tone.
    Plain(Tone),
    /// An unresolved conflict: the full width in the conflict tone.
    Unresolved,
    /// A resolved conflict: half the width in the conflict tone, half in the resolution's.
    Resolved(Tone),
}

/// One region's mark: the lines it covers in the text the ruler stands beside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulerMark {
    pub region: RegionIx,
    pub lines: Range<usize>,
    pub style: Style,
}

/// The marks beside the result pane, in region order.
pub fn marks(doc: &MergeDocument) -> impl Iterator<Item = RulerMark> + '_ {
    doc.region_ixs().filter_map(move |region| {
        let shown = mark(doc, region)?;
        let style = match shown.tone {
            Tone::Conflict => Style::Unresolved,
            tone if doc.region(region).kind == Kind::Conflict => Style::Resolved(tone),
            tone => Style::Plain(tone),
        };
        Some(RulerMark {
            region,
            lines: {
                let lines = doc.region_lines(region);
                lines.start.get()..lines.end.get()
            },
            style,
        })
    })
}

/// The lines in the result, the total a [`marks`] range is a share of.
#[must_use]
pub fn total(doc: &MergeDocument) -> usize {
    doc.line_count()
}

/// The marks beside `side`'s pane of a diff: each changed region in that side's tone. Regions
/// lag the text until the next recompute, so their lines are clamped to the text.
pub fn diff_marks(doc: &DiffDocument, side: Side) -> impl Iterator<Item = RulerMark> + '_ {
    let total = diff_total(doc, side);
    doc.regions().iter().enumerate().filter(|(_, region)| region.kind == Kind2::Changed).map(
        move |(index, region)| {
            let lines = region.lines(side);
            RulerMark {
                region: RegionIx::new(index),
                lines: lines.start.min(total)..lines.end.min(total),
                style: Style::Plain(tone(side)),
            }
        },
    )
}

/// The lines in `side`'s text, the total a [`diff_marks`] range is a share of. An
/// unterminated last line counts; an empty text has none.
#[must_use]
pub fn diff_total(doc: &DiffDocument, side: Side) -> usize {
    let text = doc.text(side);
    let len = text.len_bytes();
    let terminated = len == 0 || text.byte(len - 1) == b'\n';
    text.len_lines() - usize::from(terminated)
}

/// Where `lines` of `total` paint on a strip `height` tall: its share of the height, at least
/// `min` tall, moved up from the bottom edge when the minimum would cross it.
#[must_use]
pub fn extent(lines: &Range<usize>, total: usize, height: f32, min: f32) -> Range<f32> {
    let at = |line: usize| share(line, total) * height;
    let start = at(lines.start);
    let end = at(lines.end).max(start + min);
    if end > height { (height - min).max(0.)..height } else { start..end }
}

/// The line of `total` at `y` on a strip `height` tall, held to the text.
#[must_use]
pub fn line_at(y: f32, total: usize, height: f32) -> usize {
    let fraction = (y / height).clamp(0., 1.);
    let line = (fraction * count(total)).floor();
    // The fraction is in 0..=1, so the line is in 0..=total and fits.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the line is a whole number in 0..=total"
    )]
    let line = line as usize;
    line.min(total.saturating_sub(1))
}

/// The region of the mark painted at `y`: the last of `marks` whose [`extent`] holds it, since
/// later marks paint over earlier ones.
#[must_use]
pub fn mark_at(
    marks: &[RulerMark],
    y: f32,
    total: usize,
    height: f32,
    min: f32,
) -> Option<RegionIx> {
    marks
        .iter()
        .rev()
        .find(|mark| extent(&mark.lines, total, height, min).contains(&y))
        .map(|mark| mark.region)
}

/// A run of the strip painted in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Painted {
    pub px: Range<f32>,
    pub style: Style,
}

/// What the strip paints for `marks`: each mark's [`extent`] out to whole pixels, each pixel in
/// the style of the last mark on it, since later marks paint over earlier ones, and pixels of
/// one style that meet joined. The runs do not overlap, so the strip paints no more runs than
/// it has pixels.
#[must_use]
pub fn painted(marks: &[RulerMark], total: usize, height: f32, min: f32) -> Vec<Painted> {
    let pixels = whole_pixels(height.ceil());
    let mut top: Vec<Option<Style>> = vec![None; pixels];
    for mark in marks {
        let at = extent(&mark.lines, total, height, min);
        let end = whole_pixels(at.end.ceil()).min(pixels);
        let start = whole_pixels(at.start.floor()).min(end);
        top[start..end].fill(Some(mark.style));
    }
    let mut runs: Vec<(Range<usize>, Style)> = Vec::new();
    for (y, style) in top.into_iter().enumerate() {
        let Some(style) = style else { continue };
        match runs.last_mut() {
            Some((px, last)) if *last == style && px.end == y => px.end = y + 1,
            _ => runs.push((y..y + 1, style)),
        }
    }
    runs.into_iter()
        .map(|(px, style)| Painted { px: count(px.start)..count(px.end), style })
        .collect()
}

/// A whole number of pixels on the strip as a count; below zero is none.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "callers pass whole pixels of a strip a window tall"
)]
fn whole_pixels(px: f32) -> usize {
    px.max(0.) as usize
}

/// A pane's scroll down its text, in pixels: how far it is scrolled, how tall its view is,
/// and how tall its content is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scrolled {
    pub offset: f32,
    pub viewport: f32,
    pub content: f32,
}

impl Scrolled {
    /// How far the pane scrolls: none when it shows all its content.
    fn range(self) -> f32 {
        (self.content - self.viewport).max(0.)
    }
}

/// Where the scrollbar thumb paints on a strip `height` tall: the view's share of the content,
/// at least `min` tall, placed along the rest of the strip by the share of the range scrolled.
/// A pane that shows all its content fills the strip.
#[must_use]
pub fn thumb(scrolled: Scrolled, height: f32, min: f32) -> Range<f32> {
    let range = scrolled.range();
    if range <= 0. {
        return 0.0..height;
    }
    let length = thumb_length(scrolled, height, min);
    let top = (scrolled.offset / range).clamp(0., 1.) * (height - length);
    top..top + length
}

/// The scroll offset that puts the thumb's top at `top` on a strip `height` tall, held to the
/// pane's range.
#[must_use]
pub fn drag_offset(top: f32, scrolled: Scrolled, height: f32, min: f32) -> f32 {
    let travel = height - thumb_length(scrolled, height, min);
    if travel <= 0. {
        return 0.;
    }
    (top / travel).clamp(0., 1.) * scrolled.range()
}

fn thumb_length(scrolled: Scrolled, height: f32, min: f32) -> f32 {
    if scrolled.content <= 0. {
        return height;
    }
    (scrolled.viewport / scrolled.content * height).clamp(min.min(height), height)
}

/// The scroll top that puts `line` in the middle of a view `view` tall, with lines
/// `line_height` tall, held at the top of the text.
#[must_use]
pub fn centred_top(line: usize, line_height: f32, view: f32) -> f32 {
    (count(line) * line_height - view / 2.).max(0.)
}

fn share(line: usize, total: usize) -> f32 {
    if total == 0 { 0. } else { count(line) / count(total) }
}

/// A line count as a float. Exact below 2^24 lines, and a pixel's share of the strip beyond.
#[expect(
    clippy::cast_precision_loss,
    reason = "a ruler places marks to the pixel; 2^24 lines is far past one"
)]
fn count(lines: usize) -> f32 {
    lines as f32
}

#[cfg(test)]
mod tests {
    use merge_core::diff::DiffOptions;
    use merge_core::document::{Command, EditBatch, Labels, MergeOptions, TextPatch};
    use merge_core::merge3::Source;
    use merge_core::testing::{text, three};

    use super::*;

    const HEIGHT: f32 = 500.;
    const MIN: f32 = 2.;

    fn doc(base: &str, left: &str, right: &str) -> MergeDocument {
        let inputs = three(base, left, right);
        MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels")
    }

    /// 100 lines, `l0` to `l99`, with lines 40..50 changed by `change`.
    fn hundred(change: impl Fn(usize) -> Option<String>) -> String {
        (0..100)
            .map(|n| match change(n) {
                Some(line) => line,
                None => format!("l{n}\n"),
            })
            .collect()
    }

    fn in_forties(prefix: &'static str) -> impl Fn(usize) -> Option<String> {
        move |n| (40..50).contains(&n).then(|| format!("{prefix}{n}\n"))
    }

    fn mark(region: RegionIx, lines: Range<usize>, style: Style) -> RulerMark {
        RulerMark { region, lines, style }
    }

    fn all(doc: &MergeDocument) -> Vec<RulerMark> {
        marks(doc).collect()
    }

    #[test]
    fn an_unresolved_conflict_covers_both_sides_lines() {
        let d = doc(&hundred(|_| None), &hundred(in_forties("L")), &hundred(in_forties("R")));
        // The result shows left's ten lines, then right's: 110 lines.
        assert_eq!(total(&d), 110);
        assert_eq!(all(&d), [mark(RegionIx::new(1), 40..60, Style::Unresolved)]);
    }

    #[test]
    fn a_picked_conflict_is_resolved_in_the_picks_tone() {
        let mut d = doc(&hundred(|_| None), &hundred(in_forties("L")), &hundred(in_forties("R")));
        d.apply(Command::Toggle { region: RegionIx::new(1), source: Source::Side(0) })
            .expect("picks");
        assert_eq!(total(&d), 100);
        assert_eq!(all(&d), [mark(RegionIx::new(1), 40..50, Style::Resolved(Tone::Left))]);
    }

    #[test]
    fn an_edited_conflict_is_resolved_in_the_edited_tone() {
        let mut d = doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n");
        // The result is `a L R c`; the edit types into L.
        let typed = TextPatch { range: 2..2, text: "x".to_owned() };
        d.apply(Command::Edit(EditBatch(vec![typed]))).expect("edits");
        assert_eq!(all(&d), [mark(RegionIx::new(1), 1..3, Style::Resolved(Tone::Edited))]);
    }

    #[test]
    fn automatic_regions_take_their_kinds_tone() {
        // Regions: 0 a, 1 B (left), 2 c, 3 D (right), 4 e, 5 F (both), 6 g, 7 conflict, 8 i.
        // The result is a B c D e F g H X i: the conflict shows H and X on lines 7 and 8.
        let d = doc(
            "a\nb\nc\nd\ne\nf\ng\nh\ni\n",
            "a\nB\nc\nd\ne\nF\ng\nH\ni\n",
            "a\nb\nc\nD\ne\nF\ng\nX\ni\n",
        );
        assert_eq!(
            all(&d),
            [
                mark(RegionIx::new(1), 1..2, Style::Plain(Tone::Left)),
                mark(RegionIx::new(3), 3..4, Style::Plain(Tone::Right)),
                mark(RegionIx::new(5), 5..6, Style::Plain(Tone::Both)),
                mark(RegionIx::new(7), 7..9, Style::Unresolved),
            ]
        );
        assert_eq!(total(&d), 10);
    }

    #[test]
    fn an_empty_or_unchanged_file_has_no_marks() {
        assert_eq!(all(&doc("", "", "")), []);
        let same = hundred(|_| None);
        assert_eq!(all(&doc(&same, &same, &same)), []);
    }

    #[test]
    fn deleting_every_line_leaves_a_mark_at_the_top() {
        let d = doc("a\n", "", "");
        assert_eq!(
            (total(&d), all(&d)),
            (0, vec![mark(RegionIx::new(0), 0..0, Style::Plain(Tone::Both))])
        );
        assert_eq!(extent(&(0..0), 0, HEIGHT, MIN), 0.0..2.0);
        let diff = DiffDocument::new(&text("a\n"), &text(""), DiffOptions::default());
        let right: Vec<RulerMark> = diff_marks(&diff, Side::Right).collect();
        assert_eq!(right, [mark(RegionIx::new(0), 0..0, Style::Plain(Tone::Right))]);
    }

    #[test]
    fn a_diffs_regions_lagging_a_deletion_are_held_to_the_text() {
        // Right line 3 is an insertion; deleting right's last two lines leaves the region
        // past the text until the recompute.
        let mut d =
            DiffDocument::new(&text("a\nb\nc\n"), &text("a\nB\nc\nd\n"), DiffOptions::default());
        let cut = TextPatch { range: 4..8, text: String::new() };
        d.edit(Side::Right, EditBatch(vec![cut])).expect("edits");
        assert_eq!(diff_total(&d, Side::Right), 2);
        let right: Vec<RulerMark> = diff_marks(&d, Side::Right).collect();
        assert_eq!(
            right,
            [
                mark(RegionIx::new(1), 1..2, Style::Plain(Tone::Right)),
                mark(RegionIx::new(3), 2..2, Style::Plain(Tone::Right)),
            ]
        );
    }

    #[test]
    fn a_deletion_has_a_mark_of_no_lines_where_it_was() {
        let d = doc(
            &hundred(|_| None),
            &hundred(|n| (40..50).contains(&n).then(String::new)),
            &hundred(|_| None),
        );
        assert_eq!(total(&d), 90);
        assert_eq!(all(&d), [mark(RegionIx::new(1), 40..40, Style::Plain(Tone::Left))]);
        // 40 of 90 lines down a 450 px strip is 200 px; the minimum gives it 2 px.
        assert_eq!(extent(&(40..40), 90, 450., MIN), 200.0..202.0);
    }

    #[test]
    fn marks_at_the_first_and_last_line_meet_the_strips_edges() {
        let d = doc(
            &hundred(|_| None),
            &hundred(|n| (n == 0).then(|| "first\n".to_owned())),
            &hundred(|n| (n == 99).then(|| "last\n".to_owned())),
        );
        let marks = all(&d);
        assert_eq!(
            marks,
            [
                mark(RegionIx::new(0), 0..1, Style::Plain(Tone::Left)),
                mark(RegionIx::new(2), 99..100, Style::Plain(Tone::Right)),
            ]
        );
        assert_eq!(extent(&marks[0].lines, 100, HEIGHT, MIN), 0.0..5.0);
        assert_eq!(extent(&marks[1].lines, 100, HEIGHT, MIN), 495.0..500.0);
    }

    #[test]
    fn an_edit_across_two_regions_leaves_one_mark() {
        // The result is a B c D e: the edit replaces `B\nc\nD` with `Q`.
        let mut d = doc("a\nb\nc\nd\ne\n", "a\nB\nc\nd\ne\n", "a\nb\nc\nD\ne\n");
        let edit = TextPatch { range: 2..7, text: "Q".to_owned() };
        d.apply(Command::Edit(EditBatch(vec![edit]))).expect("edits");
        assert_eq!(d.text(), "a\nQ\ne\n");
        assert_eq!(all(&d), [mark(RegionIx::new(1), 1..2, Style::Plain(Tone::Edited))]);
    }

    #[test]
    fn an_extent_is_its_share_of_the_height_and_at_least_the_minimum() {
        assert_eq!(extent(&(40..50), 100, HEIGHT, MIN), 200.0..250.0);
        // One line of 1000 is half a pixel.
        assert_eq!(extent(&(0..1), 1000, HEIGHT, MIN), 0.0..2.0);
        // A mark of no lines at the end moves up to fit.
        assert_eq!(extent(&(100..100), 100, HEIGHT, MIN), 498.0..500.0);
        // A strip shorter than the minimum holds the mark from its top.
        assert_eq!(extent(&(0..1), 1, 1., MIN), 0.0..1.0);
    }

    #[test]
    fn a_place_on_the_strip_is_a_line_held_to_the_text() {
        assert_eq!(line_at(250., 100, HEIGHT), 50);
        assert_eq!(line_at(4.99, 100, HEIGHT), 0);
        assert_eq!(line_at(5., 100, HEIGHT), 1);
        assert_eq!(line_at(HEIGHT, 100, HEIGHT), 99);
        assert_eq!(line_at(-1., 100, HEIGHT), 0);
        assert_eq!(line_at(HEIGHT + 1., 100, HEIGHT), 99);
        assert_eq!(line_at(100., 0, HEIGHT), 0);
    }

    #[test]
    fn a_click_finds_the_mark_painted_there() {
        let marks = [
            mark(RegionIx::new(1), 40..50, Style::Unresolved),
            mark(RegionIx::new(3), 49..49, Style::Plain(Tone::Left)),
            mark(RegionIx::new(5), 60..60, Style::Plain(Tone::Right)),
        ];
        assert_eq!(mark_at(&marks, 225., 100, HEIGHT, MIN), Some(RegionIx::new(1)));
        // Region 3's mark, 245..247, paints over region 1's.
        assert_eq!(mark_at(&marks, 246., 100, HEIGHT, MIN), Some(RegionIx::new(3)));
        assert_eq!(mark_at(&marks, 300.5, 100, HEIGHT, MIN), Some(RegionIx::new(5)));
        assert_eq!(mark_at(&marks, 302., 100, HEIGHT, MIN), None);
        assert_eq!(mark_at(&marks, 275., 100, HEIGHT, MIN), None);
    }

    #[test]
    fn marks_on_the_same_pixels_paint_once() {
        let left = Style::Plain(Tone::Left);
        // In 1000 lines on 500 px, lines 10, 11 and 12 fall on 5..6 px, widened to 5..7 by
        // the minimum; line 13 starts at 6.5 px and ends at 8.5, out to 6..9. The unresolved
        // marks paint over pixels 6 and 7, so left keeps pixel 5. Line 400 is at 200 px, 2 px
        // tall.
        let marks = [
            mark(RegionIx::new(1), 10..11, left),
            mark(RegionIx::new(3), 11..12, left),
            mark(RegionIx::new(5), 12..13, Style::Unresolved),
            mark(RegionIx::new(7), 13..14, Style::Unresolved),
            mark(RegionIx::new(9), 400..401, Style::Unresolved),
        ];
        assert_eq!(
            painted(&marks, 1000, HEIGHT, MIN),
            [
                Painted { px: 5.0..6.0, style: left },
                Painted { px: 6.0..9.0, style: Style::Unresolved },
                Painted { px: 200.0..202.0, style: Style::Unresolved },
            ]
        );
        // Lines 20 and 24 paint 10..12 and 12..14: runs that meet join.
        let meeting = [mark(RegionIx::new(1), 20..21, left), mark(RegionIx::new(3), 24..25, left)];
        assert_eq!(painted(&meeting, 1000, HEIGHT, MIN), [Painted { px: 10.0..14.0, style: left }]);
        assert_eq!(painted(&[], 1000, HEIGHT, MIN), []);
    }

    #[test]
    fn marks_of_alternating_styles_on_one_pixel_paint_the_last() {
        // In 1M lines on 500 px, lines 0..1000 all start in pixel 0. The last mark, line 999,
        // starts at 0.4995 px and its 2 px minimum ends at 2.4995, out to 0..3; it is right's
        // and paints over the others, none of which reaches past pixel 2.
        let marks: Vec<RulerMark> = (0..1000)
            .map(|n| {
                let tone = if n % 2 == 0 { Tone::Left } else { Tone::Right };
                mark(RegionIx::new(n), n..n + 1, Style::Plain(tone))
            })
            .collect();
        assert_eq!(
            painted(&marks, 1_000_000, HEIGHT, MIN),
            [Painted { px: 0.0..3.0, style: Style::Plain(Tone::Right) }]
        );
    }

    #[test]
    fn a_mark_on_the_last_line_of_a_fractional_strip_ends_at_its_last_pixel() {
        // On 99.5 px, line 99 of 100 starts at 98.505 and is held up to 97.5..99.5, out to
        // whole pixels 97..100.
        let marks = [mark(RegionIx::new(1), 99..100, Style::Unresolved)];
        assert_eq!(
            painted(&marks, 100, 99.5, MIN),
            [Painted { px: 97.0..100.0, style: Style::Unresolved }]
        );
    }

    /// 100 lines of 15 px, 50 of them in view, scrolled `offset` px down.
    fn hundred_lines(offset: f32) -> Scrolled {
        Scrolled { offset, viewport: 750., content: 1500. }
    }

    #[test]
    fn the_thumb_is_the_views_share_of_the_strip() {
        // Lines 25-75 of 100: the middle half of the strip.
        assert_eq!(thumb(hundred_lines(375.), HEIGHT, MIN), 125.0..375.0);
        assert_eq!(thumb(hundred_lines(0.), HEIGHT, MIN), 0.0..250.0);
        assert_eq!(thumb(hundred_lines(750.), HEIGHT, MIN), 250.0..500.0);
    }

    #[test]
    fn a_pane_that_shows_all_its_text_fills_the_strip() {
        assert_eq!(
            thumb(Scrolled { offset: 0., viewport: 750., content: 150. }, HEIGHT, MIN),
            0.0..500.0
        );
        assert_eq!(
            thumb(Scrolled { offset: 0., viewport: 750., content: 750. }, HEIGHT, MIN),
            0.0..500.0
        );
        assert_eq!(
            thumb(Scrolled { offset: 0., viewport: 750., content: 0. }, HEIGHT, MIN),
            0.0..500.0
        );
    }

    #[test]
    fn a_scroll_past_either_end_holds_the_thumb_to_the_strip() {
        assert_eq!(thumb(hundred_lines(-50.), HEIGHT, MIN), 0.0..250.0);
        assert_eq!(thumb(hundred_lines(900.), HEIGHT, MIN), 250.0..500.0);
    }

    #[test]
    fn a_long_text_keeps_the_thumb_at_its_shortest_and_moves_it_along_the_rest() {
        const SHORTEST: f32 = 16.;
        // 10 000 lines of 15 px: 750 px in view is 2.5 px of the strip.
        let at = |offset| Scrolled { offset, viewport: 750., content: 150_000. };
        assert_eq!(thumb(at(0.), HEIGHT, SHORTEST), 0.0..16.0);
        // Half of the 149 250 px range: half of the 484 px the thumb travels.
        assert_eq!(thumb(at(74_625.), HEIGHT, SHORTEST), 242.0..258.0);
        assert_eq!(thumb(at(149_250.), HEIGHT, SHORTEST), 484.0..500.0);
    }

    #[test]
    fn a_dragged_thumb_scrolls_by_its_share_of_its_travel() {
        assert_eq!(drag_offset(125., hundred_lines(0.), HEIGHT, MIN), 375.);
        assert_eq!(drag_offset(250., hundred_lines(0.), HEIGHT, MIN), 750.);
        assert_eq!(drag_offset(0., hundred_lines(375.), HEIGHT, MIN), 0.);
        let long = Scrolled { offset: 0., viewport: 750., content: 150_000. };
        assert_eq!(drag_offset(242., long, HEIGHT, 16.), 74_625.);
    }

    #[test]
    fn a_thumb_dragged_past_either_end_holds_the_scroll_to_its_range() {
        assert_eq!(drag_offset(-10., hundred_lines(0.), HEIGHT, MIN), 0.);
        assert_eq!(drag_offset(400., hundred_lines(0.), HEIGHT, MIN), 750.);
        // Nothing to scroll: a drag stays at the top.
        let short = Scrolled { offset: 0., viewport: 750., content: 150. };
        assert_eq!(drag_offset(100., short, HEIGHT, MIN), 0.);
    }

    #[test]
    fn a_line_scrolls_to_the_middle_of_the_view_or_the_top() {
        // Line 50 of 20 px lines starts at 1000 px; a 400 px view puts it 200 px down.
        assert_eq!(centred_top(50, 20., 400.), 800.);
        assert_eq!(centred_top(5, 20., 400.), 0.);
    }

    #[test]
    fn a_diff_marks_each_changed_region_in_its_sides_tone() {
        // Line 1 changes; right adds line 3.
        let d =
            DiffDocument::new(&text("a\nb\nc\n"), &text("a\nB\nc\nd\n"), DiffOptions::default());
        let right: Vec<RulerMark> = diff_marks(&d, Side::Right).collect();
        assert_eq!(
            right,
            [
                mark(RegionIx::new(1), 1..2, Style::Plain(Tone::Right)),
                mark(RegionIx::new(3), 3..4, Style::Plain(Tone::Right)),
            ]
        );
        assert_eq!(diff_total(&d, Side::Right), 4);
        let left: Vec<RulerMark> = diff_marks(&d, Side::Left).collect();
        assert_eq!(
            left,
            [
                mark(RegionIx::new(1), 1..2, Style::Plain(Tone::Left)),
                mark(RegionIx::new(3), 3..3, Style::Plain(Tone::Left)),
            ]
        );
        let empty = DiffDocument::new(&text(""), &text(""), DiffOptions::default());
        assert_eq!(diff_marks(&empty, Side::Right).count(), 0);
    }
}
