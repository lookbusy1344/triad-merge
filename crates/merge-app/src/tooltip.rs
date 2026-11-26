//! Tooltips, after Zed's (`crates/ui/src/components/tooltip.rs`): the chrome's 12 px text on
//! 16 px lines, 4 px above and below, 8 px at the sides, on gpui-kit's popover surface with
//! its border and shadow. gpui-kit's own tooltip draws 14 px text on the default line height.
//!
//! gpui-kit's `Button::tooltip` builds that tooltip and offers no way to restyle it, so a
//! button with a tooltip sits in [`wrap`], which holds the tooltip in its place.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    Action, AnyView, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

/// The chrome's text size (`docs/STYLE.md` § Type).
pub(crate) const TEXT_SIZE: Pixels = px(12.);
/// A line of tooltip text, on the 4 px grid.
pub(crate) const LINE_HEIGHT: Pixels = px(16.);
/// Zed's 4 px above and below the text.
pub(crate) const PADDING_Y: Pixels = px(4.);
/// Zed's 8 px either side of the text.
pub(crate) const PADDING_X: Pixels = px(8.);
/// Zed's tooltips stop at 288 px (`max_w_72`). triad's widest line is a full commit ID with
/// its label, `Commit ID: ` and 40 hex digits, which a break inside would make unreadable.
pub(crate) const MAX_WIDTH: Pixels = px(384.);

/// A tooltip showing `text`, which may hold several lines.
pub(crate) fn tooltip(text: impl Into<SharedString>) -> Tooltip {
    // A tooltip shows commit descriptions and paths: their hidden characters show (spec §5).
    let text = crate::strings::visible(&text.into());
    Tooltip::new(text)
        .text_size(TEXT_SIZE)
        .line_height(LINE_HEIGHT)
        .py(PADDING_Y)
        .px(PADDING_X)
        .max_w(MAX_WIDTH)
}

/// A builder of [`tooltip`] for `text`, as an element's `tooltip` takes it.
pub(crate) fn build(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |window, cx| tooltip(text.clone()).build(window, cx)
}

/// A builder of [`tooltip`] for `text` with the keys bound to `action` in `context`.
pub(crate) fn build_with_action(
    text: impl Into<SharedString>,
    action: Box<dyn Action>,
    context: &'static str,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = text.into();
    move |window, cx| tooltip(text.clone()).action(action.as_ref(), Some(context)).build(window, cx)
}

/// `child`, a control whose own tooltip cannot be styled, in a box as large as it that shows
/// `build`'s tooltip. The box's id is `id`'s child, so `id` still names the control.
pub(crate) fn wrap(
    id: impl Into<ElementId>,
    child: impl IntoElement,
    build: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
) -> Stateful<Div> {
    let id = ElementId::NamedChild(std::sync::Arc::new(id.into()), WRAP_SUFFIX.into());
    div().id(id).flex_none().child(child).tooltip(build)
}

/// The name of a [`wrap`] box under its control's id.
const WRAP_SUFFIX: &str = "tooltip";

#[cfg(test)]
mod tests {
    use gpui_kit::{AbsoluteLength, DefiniteLength, Length};

    use super::*;

    #[test]
    fn a_tooltip_takes_zed_s_text_spacing_and_triad_s_width() {
        let mut tip = tooltip("Change ID: x\nCommit ID: y");
        let style = tip.style();
        assert_eq!(style.text.font_size, Some(AbsoluteLength::Pixels(px(12.))));
        assert_eq!(
            style.text.line_height,
            Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(16.))))
        );
        let edge = Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(4.))));
        let side = Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.))));
        assert_eq!((style.padding.top, style.padding.bottom), (edge, edge));
        assert_eq!((style.padding.left, style.padding.right), (side, side));
        assert_eq!(
            style.max_size.width,
            Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(384.)))))
        );
    }
}
