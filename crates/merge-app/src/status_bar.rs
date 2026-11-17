//! The status bar at the foot of the merge and diff windows, after Zed's `StatusBar`
//! (`crates/workspace/src/status_bar.rs`) and `docs/STYLE.md` § Status bar: one line of
//! muted chrome text in the UI font, 24 px tall under a 1 px border outside its height.

use gpui_kit::component::{Theme, h_flex};
use gpui_kit::{
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};

/// The bar's height under its border, a list row's.
const HEIGHT: Pixels = px(24.);
/// The border above the bar.
const BORDER: Pixels = px(1.);
/// Chrome text (`docs/STYLE.md` § Type).
const TEXT_SIZE: Pixels = px(12.);
/// Chrome text's line, as Zed's 12 px labels and triad's tooltips take.
const LINE_HEIGHT: Pixels = px(16.);
/// The space between the bar's text and the window's sides, a toolbar's ends'.
const SIDE: Pixels = px(8.);

/// The bar `id`, named `label` for assistive technology. Its items sit 8 px apart.
pub(crate) fn bar(
    id: &'static str,
    label: SharedString,
    theme: &Theme,
) -> impl IntoElement + ParentElement + Styled + use<> {
    h_flex()
        .id(id)
        .test_support()
        .role(Role::Status)
        .aria_label(label)
        .flex_none()
        .w_full()
        .h(HEIGHT + BORDER)
        .gap_2()
        .px(SIDE)
        .border_t(BORDER)
        .border_color(theme.border)
        .bg(theme.secondary)
        .font_family(theme.font_family.clone())
        .text_size(TEXT_SIZE)
        .line_height(LINE_HEIGHT)
        .text_color(theme.muted_foreground)
}

/// An item of the bar: `text` as element `id`.
pub(crate) fn item(id: &'static str, text: SharedString) -> impl IntoElement {
    div().id(id).test_support().whitespace_nowrap().child(text)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{AbsoluteLength, DefiniteLength, Length};

    use super::*;

    #[test]
    fn the_bar_takes_chrome_text_in_the_ui_font_over_a_border() {
        let theme =
            Theme { font_family: "UI".into(), mono_font_family: "Mono".into(), ..Theme::default() };
        let mut bar = bar("status", "a status".into(), &theme);
        let style = bar.style();
        assert_eq!(style.text.font_family, Some("UI".into()));
        assert_eq!(style.text.font_size, Some(AbsoluteLength::Pixels(px(12.))));
        assert_eq!(
            style.text.line_height,
            Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(16.))))
        );
        assert_eq!(
            style.size.height,
            Some(Length::Definite(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(25.)))))
        );
        let side = Some(DefiniteLength::Absolute(AbsoluteLength::Pixels(px(8.))));
        assert_eq!((style.padding.left, style.padding.right), (side, side));
        assert_eq!(style.border_widths.top, Some(AbsoluteLength::Pixels(px(1.))));
    }
}
