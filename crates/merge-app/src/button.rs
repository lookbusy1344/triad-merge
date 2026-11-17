//! Text buttons, after Zed's `Button` (`crates/ui/src/components/button/`) at
//! `ButtonSize::Default` with a `LabelSize::Small` label: 22 px tall, 4 px side padding, a 12 px
//! label, a 1 px border and a grey hover, as Zed's `ButtonStyle::Outlined` draws them. Icon
//! buttons stay ghost, as Zed's `IconButton`s are subtle (`docs/STYLE.md` § Controls).

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{ElementId, Pixels, SharedString, Styled as _, px};

/// Zed's `ButtonSize::Default`.
pub(crate) const HEIGHT: Pixels = px(22.);

/// A text button: gpui-kit's default variant has Zed's outlined look, a 1 px `input` border
/// over the background and `input` at half strength on hover. Extra small gives the 12 px label
/// and 4 px side padding; the height is Zed's.
pub(crate) fn outlined(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).xsmall().h(HEIGHT).label(label)
}

/// The button that finishes a dialog or writes from a tool window, which Enter takes in a
/// prompt: outlined in the primary colour, as Zed tints a prompt's default action.
pub(crate) fn default_action(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    outlined(id, label).primary().outline()
}
