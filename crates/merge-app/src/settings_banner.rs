//! The banner that lists the config file's bad lines (`DESIGN.md` §E3.2). It shows above
//! the panes of the merge and diff windows until its close button or a clean reload closes it.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{
    App, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};

use crate::{ignore_closed_view, strings};

/// The banner's element id.
pub const BANNER_ID: &str = "settings-banner";
/// The close button's element id.
pub const CLOSE_ID: &str = "settings-banner-close";

/// The banner's text for `errors`, one per line; none for no errors.
pub(crate) fn text(errors: &[String]) -> Option<SharedString> {
    (!errors.is_empty()).then(|| errors.join("\n").into())
}

/// The banner showing `text`, if any, in a view that keeps it in `field`. Its close button
/// clears the field.
pub(crate) fn render<V: 'static>(
    text: Option<&SharedString>,
    field: fn(&mut V) -> &mut Option<SharedString>,
    cx: &mut Context<V>,
) -> Option<impl IntoElement + use<V>> {
    let text = text?;
    let view = cx.entity().downgrade();
    let close = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
        ignore_closed_view(view.update(cx, |view, cx| {
            *field(view) = None;
            cx.notify();
        }));
    };
    let theme = cx.theme();
    let banner = h_flex()
        .id(BANNER_ID)
        .test_support()
        .role(Role::Alert)
        .aria_label(strings::settings_errors_label(text))
        .w_full()
        .px_2()
        .gap_2()
        .items_start()
        .bg(theme.danger)
        .text_color(theme.danger_foreground)
        .child(div().flex_1().child(text.clone()))
        .child(
            Button::new(CLOSE_ID)
                .ghost()
                .xsmall()
                .icon(IconName::Close)
                .accessibility_label(strings::CLOSE)
                .on_click(close),
        );
    Some(banner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_go_one_per_line_and_none_give_no_banner() {
        assert_eq!(text(&[]), None);
        assert_eq!(
            text(&["a:1: x".to_owned(), "b:2: y".to_owned()]),
            Some("a:1: x\nb:2: y".into())
        );
    }
}
