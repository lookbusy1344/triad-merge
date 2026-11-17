//! The text of an error or a warning, alike in every window (`docs/STYLE.md` § Errors): 12 px
//! chrome text on 16 px lines that wraps, in a status colour.

use gpui_kit::{Hsla, Pixels, Styled, px};

/// The height of a line of alert text: a tooltip's and the status bar's.
pub(crate) const LINE_HEIGHT: Pixels = px(16.);

/// `element` styled as alert text in `colour`.
pub(crate) fn style<E: Styled>(element: E, colour: Hsla) -> E {
    element.text_xs().line_height(LINE_HEIGHT).whitespace_normal().text_color(colour)
}
