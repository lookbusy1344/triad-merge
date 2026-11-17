//! The go-to-line field (spec §6.3), after Zed's go-to-line modal (`crates/go_to_line`):
//! a raised card over the top of the result, with the field over a border and a muted line
//! that names the current line, or the line Enter goes to.

use gpui_kit::component::input::{Escape, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::elevation::{self, Elevation};
use crate::strings;

/// The card's width, Zed's 24 rem.
const WIDTH: Pixels = px(384.);
/// The space between the card and the top of the result.
const INSET: Pixels = px(8.);
/// Chrome text (`docs/STYLE.md` § Type) on Zed's 16 px lines.
const TEXT_SIZE: Pixels = px(12.);
const LINE_HEIGHT: Pixels = px(16.);
/// Zed's padding round the field and the help line: 8 px at the sides, 4 px above and below.
const PADDING_X: Pixels = px(8.);
const PADDING_Y: Pixels = px(4.);
const CONTEXT: &str = "GoToLine";

/// Element ids, also used by tests.
pub const GO_TO_LINE_ID: &str = "go-to-line";
pub const GO_TO_LINE_FIELD_ID: &str = "go-to-line-field";
pub const GO_TO_LINE_HELP_ID: &str = "go-to-line-help";

/// The field and whether it shows.
pub(crate) struct Field {
    input: Entity<InputState>,
    open: bool,
}

impl Field {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(strings::go_to_line_field()));
        Self { input, open: false }
    }

    /// The text field, whose Enter the window subscribes to.
    pub(crate) fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the field, empty and focused.
    pub(crate) fn open(&mut self, window: &mut Window, cx: &mut App) {
        self.open = true;
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    /// The result line, from 0, that the field names in a result of `lines` lines.
    pub(crate) fn line(&self, lines: usize, cx: &App) -> Option<usize> {
        parse(&self.input.read(cx).value(), lines)
    }

    /// The card over the top of the result, for a result of `lines` lines with the caret on
    /// line `current`, from 0: the field, then the line Enter goes to, else the current line.
    /// `on_escape` closes it.
    pub(crate) fn render<F>(
        &self,
        lines: usize,
        current: usize,
        on_escape: F,
        cx: &App,
    ) -> impl IntoElement + use<F>
    where
        F: Fn(&Escape, &mut Window, &mut App) + 'static,
    {
        let theme = cx.theme();
        let help: gpui_kit::SharedString = match self.line(lines, cx) {
            Some(line) => strings::go_to_line_target(line + 1),
            None => strings::go_to_line_current(current + 1, lines),
        }
        .into();
        let card = v_flex()
            .id(GO_TO_LINE_ID)
            .test_support()
            .role(Role::Group)
            .aria_label(strings::go_to_line_prompt(lines))
            .key_context(CONTEXT)
            .on_action(on_escape)
            .w(WIDTH)
            .font_family(theme.font_family.clone())
            .text_size(TEXT_SIZE)
            .line_height(LINE_HEIGHT)
            .child(
                div().px(PADDING_X).py(PADDING_Y).border_b_1().border_color(theme.border).child(
                    Input::new(&self.input)
                        .id(GO_TO_LINE_FIELD_ID)
                        .aria_label(strings::go_to_line_field())
                        .appearance(false)
                        .small()
                        // The card's padding places the text, in line with the help.
                        .px_0(),
                ),
            )
            .child(
                div()
                    .id(GO_TO_LINE_HELP_ID)
                    .test_support()
                    .role(Role::Status)
                    .aria_label(help.clone())
                    .px(PADDING_X)
                    .py(PADDING_Y)
                    .text_color(theme.muted_foreground)
                    .child(help),
            );
        // Across the result's top, so the card centres over it and the text stays put.
        div()
            .absolute()
            .top(INSET)
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(elevation::surface(card, Elevation::Raised, theme))
    }
}

/// The result line, from 0, that `text` names: a whole number from 1, held to the last of
/// `lines` lines. `None` for anything else, which leaves the field open.
fn parse(text: &str, lines: usize) -> Option<usize> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Only digits remain, so a parse can fail only by overflow: past every line.
    let line = text.parse::<usize>().unwrap_or(usize::MAX);
    (line > 0).then(|| line.min(lines.max(1)) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: usize = 100;

    #[test]
    fn a_line_number_counts_from_one() {
        assert_eq!(parse("1", LINES), Some(0));
        assert_eq!(parse("42", LINES), Some(41));
        assert_eq!(parse("100", LINES), Some(99));
    }

    #[test]
    fn surrounding_spaces_are_ignored() {
        assert_eq!(parse("  7 ", LINES), Some(6));
    }

    #[test]
    fn a_line_past_the_end_holds_to_the_last_line() {
        assert_eq!(parse("101", LINES), Some(99));
        assert_eq!(parse("99999999999999999999999999", LINES), Some(99));
    }

    #[test]
    fn an_empty_result_has_only_line_one() {
        assert_eq!(parse("1", 0), Some(0));
        assert_eq!(parse("5", 0), Some(0));
    }

    #[test]
    fn anything_but_a_whole_number_from_one_is_refused() {
        for text in ["", " ", "0", "00", "-3", "+3", "1.5", "3a", "a", "1 2", "١"] {
            assert_eq!(parse(text, LINES), None, "{text:?}");
        }
    }
}
