//! Text buttons, after Zed's `Button` (`crates/ui/src/components/button/`) at
//! `ButtonSize::Default` with a `LabelSize::Small` label: 22 px tall, 4 px side padding, a 12 px
//! label, a faint 1 px edge and a grey hover, as Zed's `ButtonStyle::Outlined` draws them. Icon
//! buttons stay ghost, as Zed's `IconButton`s are subtle (`docs/STYLE.md` § Controls).

use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Colorize as _, Sizable as _};
use gpui_kit::{App, ElementId, Hsla, Pixels, SharedString, Styled as _, px};

/// Zed's `ButtonSize::Default`.
pub(crate) const HEIGHT: Pixels = px(22.);

/// How far a resting edge goes from the background to `border`. Zed's outlined edge,
/// `border.variant`, sits a third of the way from the button's fill to `border` in its One
/// themes: light `#ebebec`, `#dfdfe0`, `#c9c9ca`; dark `#2e343e`, `#363c46`, `#464b57`. triad
/// goes 85 % of the way, a step lighter than a divider, so the edge holds on a white dialog.
const EDGE_SHARE: f32 = 0.85;
/// A text button: gpui-kit's default variant at extra small gives the 12 px label, the 4 px
/// side padding and the grey hover with an `input` edge; the height and the faint resting edge
/// are Zed's.
pub(crate) fn outlined(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> Button {
    sized(id, label).border_color(faint_edge(cx))
}

/// The button that finishes a dialog or writes from a tool window, which Enter takes in a
/// prompt: the outlined button on a light grey fill, `muted` at rest, `secondary` under the
/// pointer and `secondary_active` while pressed. The label keeps the text colour.
pub(crate) fn default_action(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> Button {
    let fill = grey_fill(cx);
    // A custom variant draws its resting fill at a fifth of its colour; `bg` sets it whole.
    sized(id, label).custom(fill.variant).bg(fill.rest).border_color(faint_edge(cx))
}

/// The default action's fill.
#[derive(Debug, Clone, Copy, PartialEq)]
struct GreyFill {
    rest: Hsla,
    variant: ButtonCustomVariant,
}

fn grey_fill(cx: &App) -> GreyFill {
    let theme = cx.theme();
    GreyFill {
        rest: theme.muted,
        variant: ButtonCustomVariant::new(cx)
            .color(theme.muted)
            .foreground(theme.foreground)
            .hover(theme.secondary)
            .active(theme.secondary_active),
    }
}

/// gpui-component borders only its default variant and outlined buttons, so the edge is set
/// here for every text button.
fn sized(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).xsmall().h(HEIGHT).border_1().label(label)
}

fn faint_edge(cx: &App) -> Hsla {
    let theme = cx.theme();
    theme.border.mix_oklab(theme.background, EDGE_SHARE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::{Theme, ThemeMode};
    use gpui_kit::{AbsoluteLength, TestAppContext};

    fn in_mode(cx: &mut TestAppContext, mode: ThemeMode, check: impl FnOnce(&App)) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            Theme::change(mode, None, cx);
            check(cx);
        });
    }

    /// Whether `colour` is `#rrggbb` to within one step a channel: the theme builds
    /// Tailwind's neutrals from oklch, which rounds a step either way.
    fn near(colour: Hsla, rrggbb: u32) -> bool {
        const STEP: f32 = 1. / 255.;
        let rgba = colour.to_rgb();
        let channel =
            |shift: u32| f32::from(u8::try_from((rrggbb >> shift) & 0xff).expect("a byte")) / 255.;
        [(rgba.r, 16), (rgba.g, 8), (rgba.b, 0)]
            .into_iter()
            .all(|(value, shift)| (value - channel(shift)).abs() <= STEP + f32::EPSILON)
    }

    /// gpui-component's light theme: `muted` is neutral-100, `secondary` neutral-200,
    /// `secondary_active` neutral-300 (Tailwind's neutral scale).
    #[gpui_kit::test]
    fn the_default_action_sits_on_light_grey(cx: &mut TestAppContext) {
        in_mode(cx, ThemeMode::Light, |cx| {
            let theme = cx.theme();
            let fill = grey_fill(cx);
            assert!(near(fill.rest, 0xf5_f5_f5), "rest {fill:?}");
            assert!(near(theme.secondary, 0xe5_e5_e5), "hover {:?}", theme.secondary);
            assert!(
                near(theme.secondary_active, 0xd4_d4_d4),
                "pressed {:?}",
                theme.secondary_active
            );
            let expected = ButtonCustomVariant::new(cx)
                .color(theme.muted)
                .foreground(theme.foreground)
                .hover(theme.secondary)
                .active(theme.secondary_active);
            assert_eq!(fill.variant, expected);
        });
    }

    /// Both buttons draw a 1 px edge on every side. gpui-component draws a border only for
    /// its default variant or an outlined button; the default action's grey is a custom one.
    #[gpui_kit::test]
    fn both_buttons_draw_a_one_pixel_edge(cx: &mut TestAppContext) {
        in_mode(cx, ThemeMode::Light, |cx| {
            let one = Some(AbsoluteLength::from(px(1.)));
            for (name, mut button) in
                [("outlined", outlined("o", "O", cx)), ("default", default_action("d", "D", cx))]
            {
                let widths = button.style().border_widths.clone();
                let sides = [widths.top, widths.right, widths.bottom, widths.left];
                assert_eq!(sides, [one; 4], "{name}");
            }
        });
    }

    /// The edge sits 85 % of the way from the background to `border`, measured on lightness.
    #[gpui_kit::test]
    fn the_resting_edge_is_most_of_the_way_to_border(cx: &mut TestAppContext) {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            in_mode(cx, mode, |cx| {
                let (border, background) = (cx.theme().border, cx.theme().background);
                let share = (faint_edge(cx).l - background.l) / (border.l - background.l);
                assert!((share - 0.85).abs() < 0.05, "{mode:?}: share {share}");
            });
        }
    }
}
