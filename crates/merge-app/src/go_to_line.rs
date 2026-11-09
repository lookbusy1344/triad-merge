//! The go-to-line field (spec §6.3).

use gpui_kit::component::input::{Escape, Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::strings;

const WIDTH: Pixels = px(120.);
const CONTEXT: &str = "GoToLine";

/// Element ids, also used by tests.
pub const GO_TO_LINE_ID: &str = "go-to-line";
pub const GO_TO_LINE_FIELD_ID: &str = "go-to-line-field";

/// The field and whether it shows.
pub(crate) struct Field {
    input: Entity<InputState>,
    open: bool,
}

impl Field {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self { input: cx.new(|cx| InputState::new(window, cx)), open: false }
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

    /// The prompt and the field, for a result of `lines` lines. `on_escape` closes it.
    pub(crate) fn render<F>(&self, lines: usize, on_escape: F) -> impl IntoElement + use<F>
    where
        F: Fn(&Escape, &mut Window, &mut App) + 'static,
    {
        let prompt = strings::go_to_line_prompt(lines);
        h_flex()
            .id(GO_TO_LINE_ID)
            .test_support()
            .role(Role::Group)
            .aria_label(prompt.clone())
            .key_context(CONTEXT)
            .on_action(on_escape)
            .w_full()
            .px_2()
            .gap_2()
            .child(prompt)
            .child(
                div().w(WIDTH).child(
                    Input::new(&self.input)
                        .id(GO_TO_LINE_FIELD_ID)
                        .aria_label(strings::GO_TO_LINE_FIELD)
                        .small(),
                ),
            )
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
