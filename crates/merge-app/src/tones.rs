//! Colours for region tones (spec §5): left blue, right pink, base yellow, conflict orange,
//! edited purple, word-merged green by default. Bands are translucent so text and the active
//! line show through.
//!
//! A window builds its [`Palette`] from the settings' colours (spec §6.4) when it opens.

use gpui_kit::{Hsla, hsla, rgb};
use merge_config::settings::{Colour, Colours, DEFAULT_COLOURS, Themed};
use merge_view::marks::Tone;

const OPAQUE: f32 = 1.;
/// The grey of the cleared tone, which has no setting.
const CLEARED_LIGHTNESS: f32 = 0.45;
const CLEARED_LIGHTNESS_DARK: f32 = 0.65;

/// The colours of one theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tones {
    left: Hsla,
    right: Hsla,
    base: Hsla,
    conflict: Hsla,
    edited: Hsla,
    both: Hsla,
    word_merged: Hsla,
    cleared: Hsla,
    current: Hsla,
    deleted: Hsla,
    resolved: Hsla,
    band_opacity: f32,
}

impl Tones {
    fn new(colours: &Colours, dark: bool) -> Self {
        let pick = |colour: Themed<Colour>| Hsla::from(rgb(colour.get(dark).hex()));
        let cleared = if dark { CLEARED_LIGHTNESS_DARK } else { CLEARED_LIGHTNESS };
        Self {
            left: pick(colours.left),
            right: pick(colours.right),
            base: pick(colours.base),
            conflict: pick(colours.conflict),
            edited: pick(colours.edited),
            both: pick(colours.both),
            word_merged: pick(colours.word_merged),
            cleared: hsla(0., 0., cleared, OPAQUE),
            current: pick(colours.current),
            deleted: pick(colours.deleted),
            resolved: pick(colours.resolved),
            band_opacity: colours.band_opacity.get(dark),
        }
    }

    /// The opaque colour of a tone's gutter marks, labels and changed words.
    pub(crate) const fn mark(&self, tone: Tone) -> Hsla {
        match tone {
            Tone::Left => self.left,
            Tone::Right => self.right,
            Tone::Base => self.base,
            Tone::Conflict => self.conflict,
            Tone::Edited => self.edited,
            Tone::Both => self.both,
            Tone::WordMerged => self.word_merged,
            Tone::Cleared => self.cleared,
        }
    }

    /// The translucent band behind a tone's rows: its mark at the band opacity.
    pub(crate) const fn band(&self, tone: Tone) -> Hsla {
        self.translucent(self.mark(tone))
    }

    /// The colour of the current region's bars in the top panes and frame in the result.
    pub(crate) const fn current(&self) -> Hsla {
        self.current
    }

    /// The colour of a deletion's note and marks in the input panes.
    pub(crate) const fn deleted(&self) -> Hsla {
        self.deleted
    }

    /// The conflict badge's background: the conflict band while any conflict is unresolved,
    /// the resolved colour's band once none is.
    pub(crate) const fn conflict_badge(&self, resolved: bool) -> Hsla {
        if resolved { self.translucent(self.resolved) } else { self.band(Tone::Conflict) }
    }

    const fn translucent(&self, colour: Hsla) -> Hsla {
        Hsla { a: self.band_opacity, ..colour }
    }
}

/// The colours of both themes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    light: Tones,
    dark: Tones,
}

impl Palette {
    pub(crate) fn new(colours: &Colours) -> Self {
        Self { light: Tones::new(colours, false), dark: Tones::new(colours, true) }
    }

    /// The dark theme's colours when `dark`, else the light theme's.
    pub(crate) const fn tones(&self, dark: bool) -> &Tones {
        if dark { &self.dark } else { &self.light }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new(&DEFAULT_COLOURS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TONES: [Tone; 8] = [
        Tone::Left,
        Tone::Right,
        Tone::Base,
        Tone::Both,
        Tone::Conflict,
        Tone::Edited,
        Tone::Cleared,
        Tone::WordMerged,
    ];
    const THEMES: [bool; 2] = [false, true];
    const DEGREES: f32 = 360.;
    /// Hues closer than this read as one colour in a translucent band. The default hues
    /// keep this gap before they round to `#rrggbb`, which moves each by up to
    /// [`HEX_HUE_ROUNDING`].
    const MIN_HUE_GAP: f32 = 30.;
    /// Pink, per spec §5: between magenta and red.
    const PINK: std::ops::RangeInclusive<f32> = 300.0..=345.0;
    /// Blue, per spec §5: between cyan and violet.
    const BLUE: std::ops::RangeInclusive<f32> = 190.0..=250.0;
    /// Red: within 10 degrees of 0.
    const RED_MAX_GAP: f32 = 10.;
    /// The most a hue moves when an HSL colour rounds to `#rrggbb`, in degrees.
    const HEX_HUE_ROUNDING: f32 = 0.5;
    /// The most saturation or lightness moves when an HSL colour rounds to `#rrggbb`.
    const HEX_ROUNDING: f32 = 1. / 255.;

    fn hue_of(colour: Hsla) -> f32 {
        colour.h * DEGREES
    }

    fn hue_gap(a: f32, b: f32) -> f32 {
        (a - b).abs().min(DEGREES - (a - b).abs())
    }

    /// Whether `colour` is the HSL colour `hue`, `s`, `l`, rounded to `#rrggbb`.
    fn rounds_from(colour: Hsla, hue: f32, s: f32, l: f32) -> bool {
        hue_gap(hue_of(colour), hue) <= HEX_HUE_ROUNDING
            && (colour.s - s).abs() <= HEX_ROUNDING
            && (colour.l - l).abs() <= HEX_ROUNDING
    }

    #[test]
    fn the_default_light_left_mark_is_2273c3_and_its_band_that_at_0_18() {
        let light = *Palette::default().tones(false);
        let left = Hsla::from(rgb(0x22_73_c3));
        assert_eq!(light.mark(Tone::Left), left);
        assert_eq!(light.band(Tone::Left), Hsla { a: 0.18, ..left });
    }

    #[test]
    fn the_default_marks_are_the_tones_hues_at_the_theme_lightness() {
        let palette = Palette::default();
        for (dark, l) in [(false, 0.45), (true, 0.65)] {
            let tones = palette.tones(dark);
            for (tone, hue) in [
                (Tone::Left, 210.),
                (Tone::Right, 330.),
                (Tone::Base, 55.),
                (Tone::Conflict, 25.),
                (Tone::Edited, 275.),
                (Tone::Both, 180.),
                (Tone::WordMerged, 120.),
            ] {
                let mark = tones.mark(tone);
                assert!(rounds_from(mark, hue, 0.7, l), "{tone:?} {mark:?} dark {dark}");
            }
            assert!(rounds_from(tones.deleted(), 0., 0.7, l), "dark {dark}");
            assert!(rounds_from(tones.resolved, 140., 0.7, l), "dark {dark}");
        }
        assert!(rounds_from(palette.tones(false).current(), 88., 0.85, 0.4));
        assert!(rounds_from(palette.tones(true).current(), 88., 0.75, 0.55));
    }

    #[test]
    fn a_set_colour_gives_its_mark_and_band_at_the_theme_opacity() {
        let colours =
            Colours { conflict: Themed::both(Colour::rgb(0xff_00_00)), ..DEFAULT_COLOURS };
        let palette = Palette::new(&colours);
        let red = hsla(0., 1., 0.5, OPAQUE);
        assert_eq!(palette.tones(false).mark(Tone::Conflict), red);
        assert_eq!(palette.tones(false).band(Tone::Conflict), hsla(0., 1., 0.5, 0.18));
        assert_eq!(palette.tones(true).band(Tone::Conflict), hsla(0., 1., 0.5, 0.28));
        assert_eq!(
            palette.tones(false).mark(Tone::Left),
            Palette::default().tones(false).mark(Tone::Left)
        );
    }

    #[test]
    fn each_theme_takes_its_own_colour_and_opacity() {
        let colours = Colours {
            left: Themed { light: Colour::rgb(0x00_00_ff), dark: Colour::rgb(0x00_ff_00) },
            band_opacity: Themed { light: 0.5, dark: 0.25 },
            ..DEFAULT_COLOURS
        };
        let palette = Palette::new(&colours);
        assert_eq!(palette.tones(false).band(Tone::Left), hsla(240. / DEGREES, 1., 0.5, 0.5));
        assert_eq!(palette.tones(true).band(Tone::Left), hsla(120. / DEGREES, 1., 0.5, 0.25));
    }

    #[test]
    fn every_setting_colours_its_own_tone() {
        let colours = Colours {
            left: Themed::both(Colour::rgb(0x01_00_00)),
            right: Themed::both(Colour::rgb(0x02_00_00)),
            base: Themed::both(Colour::rgb(0x03_00_00)),
            conflict: Themed::both(Colour::rgb(0x04_00_00)),
            edited: Themed::both(Colour::rgb(0x05_00_00)),
            both: Themed::both(Colour::rgb(0x06_00_00)),
            word_merged: Themed::both(Colour::rgb(0x07_00_00)),
            current: Themed::both(Colour::rgb(0x08_00_00)),
            deleted: Themed::both(Colour::rgb(0x09_00_00)),
            resolved: Themed::both(Colour::rgb(0x0a_00_00)),
            band_opacity: Themed::both(0.5),
        };
        let tones = *Palette::new(&colours).tones(false);
        let red = |n: u32| Hsla::from(rgb(n << 16));
        let got = [
            tones.mark(Tone::Left),
            tones.mark(Tone::Right),
            tones.mark(Tone::Base),
            tones.mark(Tone::Conflict),
            tones.mark(Tone::Edited),
            tones.mark(Tone::Both),
            tones.mark(Tone::WordMerged),
            tones.current(),
            tones.deleted(),
        ];
        assert!(got.into_iter().eq((1..=9).map(red)), "{got:?}");
        assert_eq!(tones.conflict_badge(true), Hsla { a: 0.5, ..red(10) });
        assert_eq!(tones.conflict_badge(false), Hsla { a: 0.5, ..red(4) });
    }

    #[test]
    fn the_cleared_tone_is_grey_whatever_the_colours() {
        for dark in THEMES {
            let grey = Palette::default().tones(dark).mark(Tone::Cleared);
            assert_eq!(grey.s, 0., "dark {dark}");
            assert_eq!(grey.a, OPAQUE, "dark {dark}");
        }
        assert_eq!(Palette::default().tones(false).mark(Tone::Cleared).l, 0.45);
        assert_eq!(Palette::default().tones(true).mark(Tone::Cleared).l, 0.65);
    }

    #[test]
    fn a_deletion_is_red_in_both_themes() {
        for dark in THEMES {
            let colour = Palette::default().tones(dark).deleted();
            let hue = hue_of(colour);
            assert!(hue_gap(hue, 0.) <= RED_MAX_GAP, "deletion hue {hue} in dark {dark}");
            assert_eq!(colour.a, 1., "deletion text is opaque in dark {dark}");
        }
    }

    #[test]
    fn the_current_region_highlight_stands_apart_from_every_tone() {
        for dark in THEMES {
            let palette = Palette::default();
            let tones = palette.tones(dark);
            let highlight = hue_of(tones.current());
            for tone in TONES.into_iter().filter(|&t| t != Tone::Cleared) {
                let gap = hue_gap(highlight, hue_of(tones.mark(tone)));
                assert!(
                    gap + 2. * HEX_HUE_ROUNDING >= MIN_HUE_GAP,
                    "highlight and {tone:?} are {gap} degrees apart in dark {dark}"
                );
            }
        }
    }

    #[test]
    fn left_is_blue_and_right_is_pink_in_both_themes() {
        for dark in THEMES {
            let palette = Palette::default();
            let tones = palette.tones(dark);
            let (left, right) = (tones.mark(Tone::Left), tones.mark(Tone::Right));
            assert!(BLUE.contains(&hue_of(left)), "left {left:?} dark {dark}");
            assert!(PINK.contains(&hue_of(right)), "right {right:?} dark {dark}");
        }
    }

    #[test]
    fn marks_are_opaque_and_bands_let_text_through() {
        for dark in THEMES {
            let palette = Palette::default();
            let tones = palette.tones(dark);
            for tone in TONES {
                assert_eq!(tones.mark(tone).a, 1., "{tone:?} dark {dark}");
                let alpha = tones.band(tone).a;
                assert!(alpha > 0. && alpha < 1., "{tone:?} band alpha {alpha}");
            }
        }
    }

    #[test]
    fn tones_are_far_apart_on_the_colour_wheel() {
        for dark in THEMES {
            let palette = Palette::default();
            let tones = palette.tones(dark);
            let hues: Vec<(Tone, f32)> = TONES
                .iter()
                .filter(|&&t| t != Tone::Cleared)
                .map(|&t| (t, hue_of(tones.mark(t))))
                .collect();
            for (i, &(a, ha)) in hues.iter().enumerate() {
                for &(b, hb) in &hues[i + 1..] {
                    let gap = hue_gap(ha, hb);
                    assert!(
                        gap + 2. * HEX_HUE_ROUNDING >= MIN_HUE_GAP,
                        "{a:?} and {b:?} are {gap} degrees apart in dark {dark}"
                    );
                }
            }
        }
    }
}
