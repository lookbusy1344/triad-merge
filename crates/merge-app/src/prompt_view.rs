//! The prompts of Linux, which has no platform dialog. GPUI's fallback cuts off long details
//! and ignores the theme and the keyboard. Enter takes the first button, as a native alert
//! does. Escape takes the button that keeps the user's work, and no other: `Start Over` throws
//! a recovery journal away.

use gpui_kit::component::{ActiveTheme as _, Theme, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Div, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Pixels, PromptButton,
    PromptHandle, PromptLevel, PromptResponse, Render, RenderablePromptHandle, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, black, div, px,
};

use crate::elevation::{self, Elevation};
use crate::strings::prompt;

const ENTER: &str = "enter";
const ESCAPE: &str = "escape";
const ID: &str = "prompt";
const WIDTH: Pixels = px(380.);
/// A dialog's edge and its content (`docs/STYLE.md` § Spacing).
const PADDING: Pixels = px(12.);
/// Rows of a dialog (`docs/STYLE.md` § Spacing).
const GAP: Pixels = px(8.);
/// Dialog text (`docs/STYLE.md` § Type) on Zed's 16 px lines.
const TEXT_SIZE: Pixels = px(12.);
const LINE_HEIGHT: Pixels = px(16.);
/// Zed's scrim under a prompt: black at 20 %.
const SCRIM_ALPHA: f32 = 0.2;

/// Use these prompts in place of GPUI's fallback, on the platform that needs them. Headless
/// tests answer the test platform's prompts, so `run` calls this and `init` does not.
pub(crate) fn install(cx: &mut App) {
    if cfg!(target_os = "linux") {
        cx.set_prompt_builder(build);
    }
}

fn build(
    _level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    buttons: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let view = cx.new(|cx| PromptView {
        message: SharedString::from(message.to_owned()),
        detail: detail.map(|detail| SharedString::from(detail.to_owned())),
        buttons: buttons.to_vec(),
        focus: cx.focus_handle(),
    });
    handle.with_view(view, window, cx)
}

/// The index of the button `key` takes.
fn answer(key: &str, buttons: &[PromptButton]) -> Option<usize> {
    match key {
        ENTER => (!buttons.is_empty()).then_some(0),
        ESCAPE => buttons
            .iter()
            .position(|button| {
                button.is_cancel()
                    || [prompt::keep_editing(), prompt::keep_mine()].contains(button.label())
            })
            .or((buttons.len() == 1).then_some(0)),
        _ => None,
    }
}

struct PromptView {
    message: SharedString,
    detail: Option<SharedString>,
    buttons: Vec<PromptButton>,
    focus: FocusHandle,
}

impl PromptView {
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = answer(&event.keystroke.key, &self.buttons) {
            cx.emit(PromptResponse(index));
            cx.stop_propagation();
        }
    }
}

impl EventEmitter<PromptResponse> for PromptView {}

impl Focusable for PromptView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PromptView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let buttons = self.buttons.iter().enumerate().map(|(index, button)| {
            let label = button.label().clone();
            let button = if index == 0 {
                crate::button::default_action(index, label, cx)
            } else {
                crate::button::outlined(index, label, cx)
            };
            button.on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(PromptResponse(index));
                cx.stop_propagation();
            }))
        });
        let card = card(theme)
            .id(ID)
            .test_support()
            .role(Role::AlertDialog)
            .aria_label(self.message.clone())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(self.message.clone()))
            .children(self.detail.clone().map(|detail| div().text_color(muted).child(detail)))
            .child(h_flex().pt_2().gap_2().justify_end().flex_wrap().children(buttons));
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(black().opacity(SCRIM_ALPHA))
            .child(card)
    }
}

/// The dialog's card: dialog text in the UI font, on a modal surface.
fn card(theme: &Theme) -> Div {
    let card = v_flex()
        .w(WIDTH)
        .p(PADDING)
        .gap(GAP)
        .font_family(theme.font_family.clone())
        .text_size(TEXT_SIZE)
        .line_height(LINE_HEIGHT);
    elevation::surface(card, Elevation::Modal, theme)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{AbsoluteLength, DefiniteLength};

    use super::*;

    #[test]
    fn the_card_takes_dialog_spacing_and_text_on_a_modal_surface() {
        let theme = Theme { font_family: "UI".into(), ..Theme::default() };
        let mut card = card(&theme);
        let style = card.style();
        let twelve = Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(12.))));
        assert_eq!((style.padding.top, style.padding.left), (twelve, twelve));
        assert_eq!(
            style.gap.height,
            Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.))))
        );
        assert_eq!(style.text.font_size, Some(AbsoluteLength::Pixels(px(12.))));
        assert_eq!(style.text.font_family, Some("UI".into()));
        assert_eq!(style.box_shadow, Some(Elevation::Modal.shadow(false)));
    }

    fn buttons(labels: &[SharedString]) -> Vec<PromptButton> {
        labels.iter().cloned().map(PromptButton::new).collect()
    }

    #[test]
    fn enter_takes_the_first_button() {
        let save = buttons(&[prompt::save(), prompt::discard(), prompt::keep_editing()]);
        assert_eq!(answer(ENTER, &save), Some(0));
    }

    #[test]
    fn enter_without_buttons_takes_none() {
        assert_eq!(answer(ENTER, &[]), None);
    }

    #[test]
    fn escape_keeps_editing_wherever_that_button_sits() {
        let last = buttons(&[prompt::save(), prompt::discard(), prompt::keep_editing()]);
        assert_eq!(answer(ESCAPE, &last), Some(2));
        let first = buttons(&[prompt::keep_editing(), prompt::discard()]);
        assert_eq!(answer(ESCAPE, &first), Some(0));
    }

    #[test]
    fn escape_keeps_mine_on_a_change_on_disk() {
        let changed = buttons(&[prompt::keep_mine(), prompt::reload()]);
        assert_eq!(answer(ESCAPE, &changed), Some(0));
        let reversed = buttons(&[prompt::reload(), prompt::keep_mine()]);
        assert_eq!(answer(ESCAPE, &reversed), Some(1));
    }

    #[test]
    fn escape_takes_a_cancel_button() {
        let cancel = [
            PromptButton::new(prompt::save()),
            PromptButton::cancel(crate::strings::menu::cancel()),
        ];
        assert_eq!(answer(ESCAPE, &cancel), Some(1));
    }

    #[test]
    fn escape_closes_an_alert_with_one_button() {
        assert_eq!(answer(ESCAPE, &buttons(&[prompt::quit()])), Some(0));
    }

    #[test]
    fn escape_does_not_choose_between_a_restore_and_starting_over() {
        let restore = buttons(&[prompt::restore(), prompt::start_over()]);
        assert_eq!(answer(ESCAPE, &restore), None);
    }

    #[test]
    fn escape_without_buttons_takes_none() {
        assert_eq!(answer(ESCAPE, &[]), None);
    }

    #[test]
    fn another_key_takes_none() {
        assert_eq!(answer("a", &buttons(&[prompt::quit()])), None);
    }
}
