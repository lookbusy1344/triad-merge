//! The pill over one end of a moved block, in the diff window and the merge window's top panes
//! (spec §5): translucent pale blue with bold deep blue text, naming the other end's line. The
//! inherited pill over an inherited conflict in the result takes the same shape in the
//! inherited colour.

use gpui_kit::base::ObservedElement;
use gpui_kit::{
    Div, ElementId, FontWeight, Hsla, InteractiveElement as _, ParentElement as _, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
    hsla, px,
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
    pill(id, text, label, Role::Button, height, text_size, [MOVE_PILL_FILL, MOVE_PILL_TEXT])
        .cursor_pointer()
}

/// A pill one row tall showing `text` in `colours`, fill then text, read as a label. The
/// caller places it.
pub(crate) fn label_pill(
    id: ElementId,
    text: SharedString,
    height: Pixels,
    text_size: Pixels,
    colours: [Hsla; 2],
) -> ObservedElement<Stateful<Div>> {
    pill(id, text.clone(), text, Role::Label, height, text_size, colours)
}

fn pill(
    id: ElementId,
    text: impl Into<SharedString>,
    label: impl Into<SharedString>,
    role: Role,
    height: Pixels,
    text_size: Pixels,
    [fill, colour]: [Hsla; 2],
) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .test_support()
        .role(role)
        .aria_label(label.into())
        .h(height)
        .px(MOVE_PILL_PADDING)
        .flex()
        .items_center()
        .rounded_full()
        .bg(fill)
        .text_color(colour)
        .font_weight(FontWeight::BOLD)
        .text_size(text_size)
        .child(text.into())
}
