//! The pill over one end of a moved block, in the diff window and the merge window's top panes
//! (spec §5): translucent pale blue with bold deep blue text, naming the other end's line.

use gpui_kit::base::ObservedElement;
use gpui_kit::{
    Div, ElementId, FontWeight, Hsla, InteractiveElement as _, ParentElement as _, Pixels, Role,
    Stateful, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, hsla, px,
};

/// How far a pill stands in from its pane's right edge: past the 12 px ruler and the
/// scrollbar, on the 4 px grid.
pub(crate) const MOVE_PILL_INSET: Pixels = px(16.);
/// A pill's padding either side of its text, on the 4 px grid.
const MOVE_PILL_PADDING: Pixels = px(4.);
/// Degrees in a turn, for hues given in degrees.
const DEGREES: f32 = 360.;
/// A pill's fill: pale blue, translucent so the text under it shows through.
const MOVE_PILL_FILL: Hsla = hsla(210. / DEGREES, 0.9, 0.85, 0.7);
/// A pill's text: deep blue, bold against the pale fill.
const MOVE_PILL_TEXT: Hsla = hsla(220. / DEGREES, 0.9, 0.3, 1.);

/// A pill one row tall showing `text`, a button named `label`. The caller places it and
/// gives it its click.
pub(crate) fn move_pill(
    id: ElementId,
    text: String,
    label: String,
    height: Pixels,
    text_size: Pixels,
) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .test_support()
        .role(Role::Button)
        .aria_label(label)
        .h(height)
        .px(MOVE_PILL_PADDING)
        .flex()
        .items_center()
        .rounded_full()
        .bg(MOVE_PILL_FILL)
        .text_color(MOVE_PILL_TEXT)
        .font_weight(FontWeight::BOLD)
        .text_size(text_size)
        .cursor_pointer()
        .child(text)
}
