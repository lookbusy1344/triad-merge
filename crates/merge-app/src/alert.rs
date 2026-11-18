//! The text of an error or a warning, alike in every window (`docs/STYLE.md` § Errors): 12 px
//! chrome text on 16 px lines that wraps, in a status colour.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role,
    SharedString, StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};

/// The height of a line of alert text: a tooltip's and the status bar's.
pub(crate) const LINE_HEIGHT: Pixels = px(16.);

/// `element` styled as alert text in `colour`.
pub(crate) fn style<E: Styled>(element: E, colour: Hsla) -> E {
    element.text_xs().line_height(LINE_HEIGHT).whitespace_normal().text_color(colour)
}

/// A warning across the window above its panes, with element id `id`.
pub(crate) fn warning_banner(
    id: &'static str,
    text: SharedString,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    style(div(), theme.warning_foreground)
        .id(id)
        .test_support()
        .role(Role::Alert)
        .aria_label(text.clone())
        .px_2()
        .bg(theme.warning)
        .child(text)
}
