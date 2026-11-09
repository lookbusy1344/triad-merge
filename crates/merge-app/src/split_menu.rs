//! A split button: a button, and beside it a compact menu of related actions (spec §6.3,
//! `docs/STYLE.md`). gpui-base's `Popover` owns the open state, Escape, a click outside and
//! giving the focus back; this module draws the rows and moves the selection.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::Popover;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Action, AnyElement, App, ClickEvent, Entity, EntityId, FocusHandle, InteractiveElement as _,
    IntoElement, KeyBinding, ParentElement as _, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, actions, div,
    prelude::FluentBuilder as _, px, transparent_black,
};

actions!(split_menu, [SelectNext, SelectPrevious, Activate]);

/// The key context of an open menu.
const CONTEXT: &str = "SplitMenu";
/// A menu row (`docs/STYLE.md`).
const ROW_HEIGHT: Pixels = px(20.);
/// The gap between the trigger and the menu.
const MENU_OFFSET: Pixels = px(2.);

/// The menu's trigger, inside its split button; also used by tests.
pub const MENU_TRIGGER_ID: &str = "popup";
/// An open menu; its rows' ids are their positions. Also used by tests.
pub const POPUP_MENU_ID: &str = "popup-menu";

/// Bind the keys of an open menu: ↓ and ↑ move the selection, Enter runs it.
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(CONTEXT)),
        KeyBinding::new("enter", Activate, Some(CONTEXT)),
    ]);
}

/// A menu row.
pub(crate) struct Item {
    pub icon: IconName,
    pub label: &'static str,
    pub action: Box<dyn Action>,
}

/// The row after `selected`, wrapping; the first when none is selected.
fn next(selected: Option<usize>, len: usize) -> usize {
    selected.map_or(0, |ix| (ix + 1) % len)
}

/// The row before `selected`, wrapping; the last when none is selected.
fn previous(selected: Option<usize>, len: usize) -> usize {
    match selected {
        Some(0) | None => len - 1,
        Some(ix) => ix - 1,
    }
}

/// A button with a menu of related actions beside it. Each row shows the key of its action
/// in `key_context`.
#[derive(IntoElement)]
pub(crate) struct SplitMenu {
    id: &'static str,
    button: Button,
    label: &'static str,
    key_context: &'static str,
    text_size: Pixels,
    items: Rc<[Item]>,
}

impl SplitMenu {
    /// `button`, and a menu named `label` of `items`, whose text is `text_size`.
    pub(crate) fn new(
        id: &'static str,
        button: Button,
        label: &'static str,
        key_context: &'static str,
        text_size: Pixels,
        items: impl IntoIterator<Item = Item>,
    ) -> Self {
        Self { id, button, label, key_context, text_size, items: items.into_iter().collect() }
    }

    /// The id of one piece of the split button's element state.
    fn state_id(&self, piece: &str) -> SharedString {
        format!("{}-{piece}", self.id).into()
    }
}

impl gpui_kit::RenderOnce for SplitMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let selected = window.use_keyed_state(self.state_id("selected"), cx, |_, _| None);
        let focus = window
            .use_keyed_state(self.state_id("focus"), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let view = window.current_view();
        let trigger = Button::new(MENU_TRIGGER_ID)
            .ghost()
            .small()
            .icon(IconName::ChevronDown)
            .accessibility_label(self.label)
            .tooltip(self.label);
        let popover = Popover::new(self.state_id("popover"))
            .anchor(gpui_kit::Anchor::TopLeft)
            .offset(MENU_OFFSET)
            .track_focus(&focus)
            .on_open_change({
                let selected = selected.clone();
                move |_, _, cx| selected.update(cx, |selected, _| *selected = None)
            })
            .trigger(trigger);
        let menu = Menu {
            label: self.label,
            key_context: self.key_context,
            text_size: self.text_size,
            items: self.items,
            selected,
            focus,
            view,
        };
        h_flex()
            .id(self.id)
            .child(self.button)
            .child(popover.content(move |_, window, cx| menu.render(cx.entity(), window, cx)))
    }
}

/// An open menu.
struct Menu {
    label: &'static str,
    key_context: &'static str,
    text_size: Pixels,
    items: Rc<[Item]>,
    selected: Entity<Option<usize>>,
    focus: FocusHandle,
    /// The view that draws the split button.
    view: EntityId,
}

type RunRow = Rc<dyn Fn(usize, &mut Window, &mut App)>;

impl Menu {
    fn render(
        self,
        popover: Entity<gpui_kit::base::PopoverState>,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let Self { label, key_context, text_size, items, selected, focus, view } = self;
        let len = items.len();
        let current = *selected.read(cx);
        // Close the menu, which gives the focus back, then run the row's action there.
        let activate: RunRow = {
            let items = Rc::clone(&items);
            Rc::new(move |ix, window, cx| {
                popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                window.dispatch_action(items[ix].action.boxed_clone(), cx);
                cx.notify(view);
            })
        };
        let step = |choose: fn(Option<usize>, usize) -> usize| {
            let selected = selected.clone();
            move |cx: &mut App| {
                selected.update(cx, |selected, _| *selected = Some(choose(*selected, len)));
                cx.notify(view);
            }
        };
        let (down, up) = (step(next), step(previous));
        let enter = {
            let activate = Rc::clone(&activate);
            move |_: &Activate, window: &mut Window, cx: &mut App| {
                if let Some(ix) = *selected.read(cx) {
                    activate(ix, window, cx);
                }
            }
        };
        let theme = cx.theme();
        let (accent, accent_text) = (theme.accent, theme.accent_foreground);
        let row_radius = theme.radius / 2.;
        let rows = items.iter().enumerate().map(|(ix, item)| {
            let key = Kbd::binding_for_action(item.action.as_ref(), Some(key_context), window)
                .map(|key| key.p_0().border_0().bg(transparent_black()).text_size(text_size));
            let activate = Rc::clone(&activate);
            h_flex()
                .id(ix)
                .test_support()
                .role(Role::MenuItem)
                .aria_label(item.label)
                .h(ROW_HEIGHT)
                .px_2()
                .gap_1p5()
                .rounded(row_radius)
                .whitespace_nowrap()
                .when(current == Some(ix), |row| row.bg(accent).text_color(accent_text))
                .hover(|style| style.bg(accent).text_color(accent_text))
                .child(Icon::new(item.icon).xsmall())
                .child(div().flex_1().child(item.label))
                .children(key.map(|key| div().ml_3().child(key)))
                .on_click(move |_: &ClickEvent, window, cx| activate(ix, window, cx))
        });
        v_flex()
            .id(POPUP_MENU_ID)
            .test_support()
            .role(Role::Menu)
            .aria_label(label)
            .track_focus(&focus)
            .key_context(CONTEXT)
            .on_action(move |_: &SelectNext, _, cx| down(cx))
            .on_action(move |_: &SelectPrevious, _, cx| up(cx))
            .on_action(enter)
            .p_1()
            .text_size(text_size)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .rounded(theme.radius)
            .shadow_md()
            .children(rows)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: usize = 3;

    #[test]
    fn next_starts_at_the_first_row_and_wraps() {
        assert_eq!(next(None, ROWS), 0);
        assert_eq!(next(Some(0), ROWS), 1);
        assert_eq!(next(Some(2), ROWS), 0);
    }

    #[test]
    fn previous_starts_at_the_last_row_and_wraps() {
        assert_eq!(previous(None, ROWS), 2);
        assert_eq!(previous(Some(2), ROWS), 1);
        assert_eq!(previous(Some(0), ROWS), 2);
    }
}
