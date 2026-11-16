//! The frame of the tool windows (directory, stack, history, absorb), after the panels of
//! Zed's git panel and project panel and `docs/STYLE.md`: a header across the top with a
//! summary at the left and the window's buttons at the right, a status line under it, the
//! window's lists, and an optional footer. Escape in a list and Cancel (⌘W, ⌘Q) close the
//! window as its close button does.

use gpui_kit::base::ObservedElement;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, Div, ElementId, InteractiveElement as _, ParentElement as _, Pixels, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

/// The key context of a tool window, in which Cancel binds.
pub(crate) const CONTEXT: &str = "ToolWindow";
/// The header: a small control with 4 px above and below (`docs/STYLE.md`).
const HEADER_HEIGHT: Pixels = px(32.);
/// The height of the header's summary: a small control's, so it lines up with the buttons.
const SUMMARY_HEIGHT: Pixels = px(24.);
/// The space between the header's or status line's text and the window's sides. With the
/// list's inset and a row's padding, a row's text lines up with it.
const SIDE: Pixels = px(12.);

/// A tool window's root: it fills the window in the theme's colours at the chrome's text
/// size, holds the key context, and closes the window on Cancel or a list's Escape when
/// `close` allows it.
pub(crate) fn frame(
    cx: &App,
    close: impl Fn(&mut Window, &mut App) -> bool + Clone + 'static,
) -> Stateful<Div> {
    let theme = cx.theme();
    let leave = close.clone();
    v_flex()
        .id("tool-window")
        .key_context(CONTEXT)
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_xs()
        .on_action(move |_: &crate::Cancel, window, cx| close_if(&close, window, cx))
        .on_action(move |_: &crate::pick_list::Leave, window, cx| close_if(&leave, window, cx))
}

fn close_if(close: &impl Fn(&mut Window, &mut App) -> bool, window: &mut Window, cx: &mut App) {
    if close(window, cx) {
        window.remove_window();
    }
}

/// The header: `summary` at the left, muted, then `buttons` at the right, over a border
/// outside its height.
pub(crate) fn header(
    id: &'static str,
    summary: Option<SharedString>,
    buttons: impl IntoIterator<Item = Button>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let summary = summary.unwrap_or_default();
    let bar = h_flex().h(HEADER_HEIGHT).px(SIDE).gap_2().justify_between();
    div().flex_none().border_b_1().border_color(theme.border).child(
        bar.child(
            div()
                .id(id)
                .test_support()
                .role(Role::Status)
                .aria_label(summary.clone())
                .flex()
                .items_center()
                .h(SUMMARY_HEIGHT)
                .min_w_0()
                .text_color(theme.muted_foreground)
                .child(div().min_w_0().truncate().child(summary)),
        )
        .child(h_flex().flex_none().gap_1().children(buttons)),
    )
}

/// The header's button that lists again: ghost, as Zed's panel headers draw theirs.
pub(crate) fn refresh_button(id: &'static str, disabled: bool) -> Button {
    Button::new(id).small().ghost().label(crate::strings::REFRESH).disabled(disabled)
}

/// A header button that changes nothing: ghost.
pub(crate) fn button(id: &'static str, label: &'static str, disabled: bool) -> Button {
    Button::new(id).small().ghost().label(label).disabled(disabled)
}

/// The header button that writes: the window's one primary button.
pub(crate) fn write_button(id: &'static str, label: &'static str, disabled: bool) -> Button {
    Button::new(id).small().primary().label(label).disabled(disabled)
}

/// The status line under the header: an outcome or an error, muted, wrapping.
pub(crate) fn status(
    id: &'static str,
    text: Option<&SharedString>,
    cx: &App,
) -> Option<ObservedElement<Stateful<Div>>> {
    let muted = cx.theme().muted_foreground;
    text.map(|text| line(id, text.clone()).text_color(muted))
}

/// The footer: a line of text over the window's bottom edge, such as a preview's result.
pub(crate) fn footer(
    id: &'static str,
    text: SharedString,
    cx: &App,
) -> ObservedElement<Stateful<Div>> {
    line(id, text).flex_none().border_t_1().border_color(cx.theme().border)
}

fn line(id: &'static str, text: SharedString) -> ObservedElement<Stateful<Div>> {
    div()
        .id(ElementId::Name(id.into()))
        .test_support()
        .role(Role::Status)
        .aria_label(text.clone())
        .px(SIDE)
        .py_1()
        .whitespace_normal()
        .child(text)
}

/// The space under the header that holds a window's lists.
pub(crate) fn body() -> Div {
    div().flex().flex_1().min_h_0()
}
