//! triad's light syntax colours (`docs/DESIGN.md` §F6.2). gpui-component's light theme
//! draws comments in `#007fff` and attributes in `#957931`, under 3:1 on some bands. triad
//! swaps in darker shades of the same hues, which hold 4:1 on every default band.

use std::rc::Rc;

use gpui_kit::App;
use gpui_kit::component::highlighter::SyntaxColors;
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeConfig};
use serde_json::Value;

/// The capture names triad recolours in the light theme, with their colours.
const LIGHT: [(&str, &str); 3] =
    [("comment", "#0066cc"), ("comment.doc", "#0066cc"), ("attribute", "#7d6629")];
const COLOR: &str = "color";

#[derive(Debug, thiserror::Error)]
pub(crate) enum ColourError {
    #[error("the light theme has no syntax colours")]
    NoHighlight,
    #[error("the light syntax colours do not round-trip")]
    Serde(#[source] serde_json::Error),
}

/// `syntax` with each of [`LIGHT`]'s captures in its colour; the capture's other fields stay.
fn recolour(syntax: &SyntaxColors) -> Result<SyntaxColors, ColourError> {
    let mut value = serde_json::to_value(syntax).map_err(ColourError::Serde)?;
    if let Value::Object(captures) = &mut value {
        for (name, colour) in LIGHT {
            let style =
                captures.entry(name).or_insert_with(|| Value::Object(serde_json::Map::new()));
            if !style.is_object() {
                *style = Value::Object(serde_json::Map::new());
            }
            if let Value::Object(fields) = style {
                fields.insert(COLOR.to_owned(), Value::String(colour.to_owned()));
            }
        }
    }
    serde_json::from_value(value).map_err(ColourError::Serde)
}

/// The light theme `config` with triad's syntax colours.
fn readable(config: &ThemeConfig) -> Result<ThemeConfig, ColourError> {
    let mut highlight = config.highlight.clone().ok_or(ColourError::NoHighlight)?;
    highlight.syntax = recolour(&highlight.syntax)?;
    Ok(ThemeConfig { highlight: Some(highlight), ..config.clone() })
}

/// Give the light theme triad's syntax colours and redraw in the current mode.
pub(crate) fn install(cx: &mut App) {
    match readable(&cx.theme().light_theme) {
        Ok(light) => {
            Theme::global_mut(cx).light_theme = Rc::new(light);
            let mode = cx.theme().mode;
            Theme::change(mode, None, cx);
        }
        // The theme's own colours still draw every capture, only with less contrast.
        Err(error) => tracing::warn!(%error, "keeping the light theme's syntax colours"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tones::Palette;
    use gpui_kit::component::ThemeMode;
    use gpui_kit::component::highlighter::HighlightTheme;
    use gpui_kit::{Hsla, Rgba, TestAppContext};
    use merge_view::marks::Tone;

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
    /// WCAG's floor for large text and user-interface parts, the user's target for captures.
    const MIN_CONTRAST: f32 = 3.0;
    /// WCAG relative luminance: the linear cut-off, the offset and the channel weights.
    const LINEAR_CUT: f32 = 0.039_28;
    const LINEAR_DIVISOR: f32 = 12.92;
    const GAMMA_OFFSET: f32 = 0.055;
    const GAMMA: f32 = 2.4;
    const WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];
    const FLARE: f32 = 0.05;

    fn channel(value: f32) -> f32 {
        if value <= LINEAR_CUT {
            value / LINEAR_DIVISOR
        } else {
            ((value + GAMMA_OFFSET) / (1.0 + GAMMA_OFFSET)).powf(GAMMA)
        }
    }

    fn luminance(colour: Rgba) -> f32 {
        [colour.r, colour.g, colour.b]
            .into_iter()
            .zip(WEIGHTS)
            .map(|(value, weight)| channel(value) * weight)
            .sum()
    }

    fn contrast(a: Rgba, b: Rgba) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + FLARE) / (a.min(b) + FLARE)
    }

    /// `top` drawn over the opaque `under`.
    fn over(top: Hsla, under: Rgba) -> Rgba {
        let top = Rgba::from(top);
        let mix = |t: f32, u: f32| t * top.a + u * (1.0 - top.a);
        Rgba { r: mix(top.r, under.r), g: mix(top.g, under.g), b: mix(top.b, under.b), a: 1.0 }
    }

    /// Every capture the theme colours, by name.
    fn colours(theme: &HighlightTheme) -> Vec<(String, Hsla)> {
        let Value::Object(captures) =
            serde_json::to_value(&theme.style.syntax).expect("syntax colours serialise")
        else {
            panic!("syntax colours serialise to an object");
        };
        captures
            .keys()
            .filter_map(|name| {
                let colour = theme.style.syntax.style(name)?.color?;
                Some((name.clone(), colour))
            })
            .collect()
    }

    fn hsla(hex: &str) -> Hsla {
        Rgba::try_from(hex).expect("a hex colour").into()
    }

    fn init(cx: &mut TestAppContext, mode: ThemeMode) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            Theme::change(mode, None, cx);
            install(cx);
        });
    }

    fn colour_of(cx: &mut TestAppContext, name: &str) -> Option<Hsla> {
        cx.read(|cx| {
            cx.theme().highlight_theme.style.syntax.style(name).and_then(|style| style.color)
        })
    }

    #[gpui_kit::test]
    fn the_light_theme_takes_triads_colours(cx: &mut TestAppContext) {
        init(cx, ThemeMode::Light);
        for (name, hex) in
            [("comment", "#0066cc"), ("comment.doc", "#0066cc"), ("attribute", "#7d6629")]
        {
            assert_eq!(colour_of(cx, name), Some(hsla(hex)), "{name}");
        }
    }

    #[gpui_kit::test]
    fn the_dark_theme_keeps_its_colours(cx: &mut TestAppContext) {
        init(cx, ThemeMode::Dark);
        let dark = HighlightTheme::default_dark();
        for name in ["comment", "attribute", "keyword"] {
            let theirs = dark.style.syntax.style(name).and_then(|style| style.color);
            assert_eq!(colour_of(cx, name), theirs, "{name}");
        }
    }

    #[gpui_kit::test]
    fn the_colours_survive_a_change_of_mode(cx: &mut TestAppContext) {
        init(cx, ThemeMode::Dark);
        cx.update(|cx| Theme::change(ThemeMode::Light, None, cx));
        assert_eq!(colour_of(cx, "comment"), Some(hsla("#0066cc")));
    }

    #[test]
    fn other_captures_and_fields_stay() {
        let light = HighlightTheme::default_light();
        let theirs = &light.style.syntax;
        let ours = recolour(theirs).expect("recolours");
        for name in ["keyword", "string", "function", "type"] {
            assert_eq!(ours.style(name), theirs.style(name), "{name}");
        }
    }

    #[gpui_kit::test]
    fn every_capture_holds_three_to_one_on_every_band(cx: &mut TestAppContext) {
        let palette = Palette::default();
        let mut weak = Vec::new();
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            init(cx, mode);
            let (background, theme) = cx.read(|cx| {
                let theme = cx.theme();
                (Rgba::from(theme.background), std::sync::Arc::clone(&theme.highlight_theme))
            });
            let tones = palette.tones(mode.is_dark());
            for (name, colour) in colours(&theme) {
                let text = Rgba::from(colour);
                for tone in TONES {
                    let ratio = contrast(text, over(tones.band(tone), background));
                    if ratio < MIN_CONTRAST {
                        weak.push(format!("{mode:?} {name} on {tone:?}: {ratio:.2}"));
                    }
                }
            }
        }
        assert!(weak.is_empty(), "{weak:#?}");
    }
}
