//! The frame of the tool windows (directory, stack, history, absorb), after the panels of
//! Zed's git panel and project panel and `docs/STYLE.md`: a header across the top with a
//! summary at the left and the window's buttons at the right, a status line under it, the
//! window's lists, and an optional footer. Escape in a list, Cancel (⌘W) and Save & Exit (⌘Q)
//! close the window as its close button does: a tool window has nothing to save.

use gpui_kit::base::ObservedElement;

use crate::alert;
use crate::decorations::{self, Fit};
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex};
use gpui_kit::{
    App, Div, ElementId, InteractiveElement as _, ParentElement as _, Pixels, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

/// The key context of a tool window, in which Cancel and Save & Exit bind.
pub(crate) const CONTEXT: &str = "ToolWindow";
/// The header, as tall as Zed's panel headers (`docs/STYLE.md`).
const HEADER_HEIGHT: Pixels = px(32.);
/// The height of the header's summary: a button's, so it lines up with the buttons.
const SUMMARY_HEIGHT: Pixels = crate::button::HEIGHT;
/// The space between the header's or status line's text and the window's sides. With the
/// list's inset and a row's padding, a row's text lines up with it.
const SIDE: Pixels = px(12.);

/// A tool window's root: it fills the window in the theme's colours at the chrome's text
/// size, holds the key context and, where triad draws the frame, the title bar, and closes
/// the window on Cancel, Save & Exit, a list's Escape or the title bar's close button when
/// `close` allows it.
pub(crate) fn frame(
    fit: &Fit,
    title: SharedString,
    window: &Window,
    cx: &App,
    close: impl Fn(&mut Window, &mut App) -> bool + Clone + 'static,
) -> Stateful<Div> {
    let theme = cx.theme();
    let leave = close.clone();
    let quit = close.clone();
    let bar = close.clone();
    let bar = move |window: &mut Window, cx: &mut App| close_if(&bar, window, cx);
    decorations::plain_root("tool-window", fit, title, bar, window, cx)
        .key_context(CONTEXT)
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_xs()
        .on_action(move |_: &crate::Cancel, window, cx| close_if(&close, window, cx))
        .on_action(move |_: &crate::SaveAndExit, window, cx| close_if(&quit, window, cx))
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

/// The header's button that lists again.
pub(crate) fn refresh_button(id: &'static str, disabled: bool, cx: &gpui_kit::App) -> Button {
    button(id, crate::strings::refresh(), disabled, cx)
}

/// A header button that changes nothing.
pub(crate) fn button(
    id: &'static str,
    label: SharedString,
    disabled: bool,
    cx: &gpui_kit::App,
) -> Button {
    crate::button::outlined(id, label, cx).disabled(disabled)
}

/// The header button that writes: the window's one default action.
pub(crate) fn write_button(
    id: &'static str,
    label: SharedString,
    disabled: bool,
    cx: &gpui_kit::App,
) -> Button {
    crate::button::default_action(id, label, cx).disabled(disabled)
}

/// What a tool window's status line says.
#[derive(Debug, Clone)]
pub(crate) enum Note {
    /// The outcome of an action, such as what absorb moved.
    Outcome(SharedString),
    /// Why an action failed.
    Error(SharedString),
}

impl Note {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Outcome(text) | Self::Error(text) => text,
        }
    }
}

/// The status line under the header, wrapping: an outcome muted, an error as an alert in
/// `danger`.
pub(crate) fn status(
    id: &'static str,
    note: Option<&Note>,
    cx: &App,
) -> Option<ObservedElement<Stateful<Div>>> {
    let theme = cx.theme();
    note.map(|note| match note {
        Note::Outcome(text) => {
            line(id, Role::Status, text.clone()).text_color(theme.muted_foreground)
        }
        Note::Error(text) => alert::style(line(id, Role::Alert, text.clone()), theme.danger),
    })
}

/// The footer: a line of text over the window's bottom edge, such as a preview's result.
pub(crate) fn footer(
    id: &'static str,
    text: SharedString,
    cx: &App,
) -> ObservedElement<Stateful<Div>> {
    line(id, Role::Status, text).flex_none().border_t_1().border_color(cx.theme().border)
}

fn line(id: &'static str, role: Role, text: SharedString) -> ObservedElement<Stateful<Div>> {
    div()
        .id(ElementId::Name(id.into()))
        .test_support()
        .role(role)
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
