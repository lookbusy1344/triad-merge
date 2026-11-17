//! Surfaces raised over the window, after Zed's `ElevationIndex`
//! (`crates/ui/src/styles/elevation.rs`) and `docs/STYLE.md` § Elevation: a popover's fill,
//! its border, the large radius and a shadow for each height.

use gpui_kit::component::Theme;
use gpui_kit::{BoxShadow, Styled, hsla, point, px};

/// How high a surface sits over the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Elevation {
    /// A surface beside its content that leaves the rest of the window working: a menu,
    /// Go to Line. Zed's `ElevatedSurface`.
    Raised,
    /// A dialog that takes the window until it closes. Zed's `ModalSurface`.
    Modal,
}

/// One shadow, as Zed's: black, offset down by `y`, blurred by `blur`, at `light` alpha in
/// a light theme and `dark` alpha in a dark one.
struct Layer {
    y: f32,
    blur: f32,
    light: f32,
    dark: f32,
}

/// Zed's `ElevatedSurface` shadows: a soft one, then a hairline under the edge.
const RAISED: [Layer; 2] = [
    Layer { y: 2., blur: 3., light: 0.12, dark: 0.12 },
    Layer { y: 1., blur: 0., light: 0.03, dark: 0.06 },
];

/// Zed's `ModalSurface` shadows: three soft ones of growing reach, then the hairline.
const MODAL: [Layer; 4] = [
    Layer { y: 2., blur: 3., light: 0.06, dark: 0.12 },
    Layer { y: 3., blur: 6., light: 0.06, dark: 0.08 },
    Layer { y: 6., blur: 12., light: 0.04, dark: 0.04 },
    Layer { y: 1., blur: 0., light: 0.04, dark: 0.12 },
];

impl Elevation {
    /// The shadows of a surface at this height, in a dark theme when `dark`.
    pub(crate) fn shadow(self, dark: bool) -> Vec<BoxShadow> {
        let layers: &[Layer] = match self {
            Self::Raised => &RAISED,
            Self::Modal => &MODAL,
        };
        layers
            .iter()
            .map(|layer| BoxShadow {
                color: hsla(0., 0., 0., if dark { layer.dark } else { layer.light }),
                offset: point(px(0.), px(layer.y)),
                blur_radius: px(layer.blur),
                spread_radius: px(0.),
                inset: false,
            })
            .collect()
    }
}

/// `element` as a surface at `elevation`: the popover's fill and text, a 1 px border, the
/// large radius and the elevation's shadow.
pub(crate) fn surface<E: Styled>(element: E, elevation: Elevation, theme: &Theme) -> E {
    element
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius_lg)
        .shadow(elevation.shadow(theme.is_dark()))
}

#[cfg(test)]
mod tests {
    use gpui_kit::{AbsoluteLength, Div, div};

    use super::*;

    fn layer(alpha: f32, y: f32, blur: f32) -> BoxShadow {
        BoxShadow {
            color: hsla(0., 0., 0., alpha),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.),
            inset: false,
        }
    }

    #[test]
    fn a_raised_surface_takes_zed_s_elevated_surface_shadow() {
        assert_eq!(Elevation::Raised.shadow(false), [layer(0.12, 2., 3.), layer(0.03, 1., 0.)]);
        assert_eq!(Elevation::Raised.shadow(true), [layer(0.12, 2., 3.), layer(0.06, 1., 0.)]);
    }

    #[test]
    fn a_modal_surface_takes_zed_s_modal_surface_shadow() {
        assert_eq!(
            Elevation::Modal.shadow(false),
            [layer(0.06, 2., 3.), layer(0.06, 3., 6.), layer(0.04, 6., 12.), layer(0.04, 1., 0.)]
        );
        assert_eq!(
            Elevation::Modal.shadow(true),
            [layer(0.12, 2., 3.), layer(0.08, 3., 6.), layer(0.04, 6., 12.), layer(0.12, 1., 0.)]
        );
    }

    #[test]
    fn a_surface_takes_the_large_radius_a_border_and_its_shadow() {
        let theme = Theme { radius_lg: px(8.), ..Theme::default() };
        let mut card: Div = surface(div(), Elevation::Raised, &theme);
        let style = card.style();
        let eight = Some(AbsoluteLength::Pixels(px(8.)));
        assert_eq!(style.corner_radii.top_left, eight);
        assert_eq!(style.corner_radii.bottom_right, eight);
        assert_eq!(style.border_widths.top, Some(AbsoluteLength::Pixels(px(1.))));
        assert_eq!(style.box_shadow, Some(Elevation::Raised.shadow(false)));
    }
}
