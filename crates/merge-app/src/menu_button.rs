//! A menu button: a button that opens a compact menu of related actions (spec §6.3,
//! `docs/STYLE.md`). gpui-base's `Popover` owns the open state, Escape, a click outside and
//! giving the focus back; this module draws the rows and moves the selection.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::Popover;
use gpui_kit::component::button::Button;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    Action, AnyElement, AnyView, App, ClickEvent, Entity, EntityId, FocusHandle,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Pixels, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Toggled,
    Window, actions, div, prelude::FluentBuilder as _, px, transparent_black,
};

use crate::elevation::{self, Elevation};

actions!(menu_button, [SelectNext, SelectPrevious, Activate]);

/// The key context of an open menu.
const CONTEXT: &str = "SplitMenu";
/// A menu row (`docs/STYLE.md`).
const ROW_HEIGHT: Pixels = px(20.);
/// The slot of a toggling row's tick: a 12 px icon.
const CHECK_WIDTH: Pixels = px(12.);
/// The gap between the trigger and the menu.
const MENU_OFFSET: Pixels = px(2.);
/// The space between the menu's edge and its rows.
const INSET: Pixels = px(4.);

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
    pub label: SharedString,
    pub action: Box<dyn Action>,
    /// A row that toggles a state shows a tick while it is on; `None` for a plain row.
    pub checked: Option<bool>,
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

/// A button that opens a menu of related actions. Each row shows the key of its action in
/// `key_context`.
#[derive(IntoElement)]
pub(crate) struct MenuButton {
    id: &'static str,
    trigger: Button,
    label: SharedString,
    key_context: &'static str,
    text_size: Pixels,
    items: Rc<[Item]>,
    tooltip: Option<TooltipBuilder>,
}

/// A builder of a tooltip, as an element's `tooltip` takes it.
pub(crate) type TooltipBuilder = Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

impl MenuButton {
    /// `trigger`, which opens a menu named `label` of `items`, whose text is `text_size`.
    pub(crate) fn new(
        id: &'static str,
        trigger: Button,
        label: SharedString,
        key_context: &'static str,
        text_size: Pixels,
        items: impl IntoIterator<Item = Item>,
    ) -> Self {
        Self {
            id,
            trigger,
            label,
            key_context,
            text_size,
            items: items.into_iter().collect(),
            tooltip: None,
        }
    }

    /// Show `build`'s tooltip over the trigger while the menu is closed.
    pub(crate) fn tooltip(self, build: TooltipBuilder) -> Self {
        Self { tooltip: Some(build), ..self }
    }

    /// The id of one piece of the menu button's element state.
    fn state_id(&self, piece: &str) -> SharedString {
        format!("{}-{piece}", self.id).into()
    }
}

impl gpui_kit::RenderOnce for MenuButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let selected = window.use_keyed_state(self.state_id("selected"), cx, |_, _| None);
        let focus = window
            .use_keyed_state(self.state_id("focus"), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();
        let view = window.current_view();
        let popover = Popover::new(self.state_id("popover"))
            .anchor(gpui_kit::Anchor::TopLeft)
            .offset(MENU_OFFSET)
            .track_focus(&focus)
            .on_open_change({
                let selected = selected.clone();
                move |_, _, cx| selected.update(cx, |selected, _| *selected = None)
            })
            .trigger_with({
                let (id, trigger, tooltip) = (self.id, self.trigger, self.tooltip);
                move |open, _, _| {
                    let trigger = trigger.open(open);
                    match tooltip.filter(|_| !open) {
                        Some(build) => {
                            crate::tooltip::wrap(id, trigger, move |window, cx| build(window, cx))
                                .into_any_element()
                        }
                        None => trigger.into_any_element(),
                    }
                }
            });
        let menu = Menu {
            label: self.label,
            key_context: self.key_context,
            text_size: self.text_size,
            items: self.items,
            selected,
            focus,
            view,
        };
        popover.content(move |_, window, cx| menu.render(cx.entity(), window, cx))
    }
}

/// An open menu.
struct Menu {
    label: SharedString,
    key_context: &'static str,
    text_size: Pixels,
    items: Rc<[Item]>,
    selected: Entity<Option<usize>>,
    focus: FocusHandle,
    /// The view that draws the menu button.
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
        // Inside the menu's large radius, less the inset (`docs/STYLE.md` § Controls).
        let row_radius = theme.radius_lg - INSET;
        let rows = items.iter().enumerate().map(|(ix, item)| {
            let key = Kbd::binding_for_action(item.action.as_ref(), Some(key_context), window)
                .map(|key| key.p_0().border_0().bg(transparent_black()).text_size(text_size));
            let activate = Rc::clone(&activate);
            let role = if item.checked.is_some() { Role::MenuItemCheckBox } else { Role::MenuItem };
            h_flex()
                .id(ix)
                .test_support()
                .role(role)
                .aria_label(item.label.clone())
                .when_some(item.checked, |row, on| {
                    row.aria_toggled(if on { Toggled::True } else { Toggled::False })
                })
                .h(ROW_HEIGHT)
                .px_2()
                .gap_1p5()
                .rounded(row_radius)
                .whitespace_nowrap()
                .when(current == Some(ix), |row| row.bg(accent).text_color(accent_text))
                .hover(|style| style.bg(accent).text_color(accent_text))
                .child(Icon::new(item.icon).xsmall())
                .child(div().flex_1().child(item.label.clone()))
                .children(item.checked.map(|on| {
                    div()
                        .w(CHECK_WIDTH)
                        .when(on, |slot| slot.child(Icon::new(IconName::Check).xsmall()))
                }))
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
            .p(INSET)
            .text_size(text_size)
            .map(|menu| elevation::surface(menu, Elevation::Raised, theme))
            .children(rows)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AnyView, AppContext as _, Context, Render, TestAppContext};

    use super::*;

    const ROWS: usize = 3;
    /// The menu button of the probe window.
    const ID: &str = "probe";
    /// Longer than GPUI's tooltip delay.
    const PAST_TOOLTIP_DELAY: Duration = Duration::from_secs(1);

    /// A window with one menu button, whose tooltip counts how often it is built.
    struct Probe {
        builds: Rc<Cell<usize>>,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let builds = Rc::clone(&self.builds);
            let item = Item {
                icon: IconName::Check,
                label: "Row".into(),
                action: Box::new(Activate),
                checked: None,
            };
            div().size_full().child(
                MenuButton::new(
                    ID,
                    Button::new(ID).label("Menu"),
                    "Menu".into(),
                    "Probe",
                    px(12.),
                    [item],
                )
                .tooltip(Rc::new(
                    move |window: &mut Window, cx: &mut App| -> AnyView {
                        builds.set(builds.get() + 1);
                        crate::tooltip::tooltip("Menu").build(window, cx)
                    },
                )),
            )
        }
    }

    /// Open the probe; with `open_menu`, open its menu. Then hover the button past the
    /// tooltip's delay and give the number of tooltips built.
    fn tooltips_on_hover(cx: &mut TestAppContext, open_menu: bool) -> usize {
        let builds = Rc::new(Cell::new(0));
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_reduce_motion(true);
            let builds = Rc::clone(&builds);
            cx.open_window(gpui_kit::WindowOptions::default(), |_, cx| cx.new(|_| Probe { builds }))
                .expect("window opens")
        });
        let act = |cx: &mut TestAppContext, f: &dyn Fn(&mut Window, &mut App)| {
            cx.update_window(window.into(), |_, window, cx| f(window, cx)).expect("window open");
        };
        act(cx, &|window, cx| window.render_frame(cx));
        if open_menu {
            act(cx, &|window, cx| window.click(ID, cx));
            act(cx, &|window, _| assert!(window.try_find(POPUP_MENU_ID).is_some(), "menu open"));
        }
        act(cx, &|window, cx| window.hover(ID, cx));
        cx.executor().advance_clock(PAST_TOOLTIP_DELAY);
        cx.run_until_parked();
        act(cx, &|window, cx| window.render_frame(cx));
        builds.get()
    }

    #[gpui_kit::test]
    fn a_closed_menu_button_shows_its_tooltip(cx: &mut TestAppContext) {
        assert_eq!(tooltips_on_hover(cx, false), 1);
    }

    /// Zed hides a button's tooltip while its menu is open, so the tooltip never covers it.
    #[gpui_kit::test]
    fn an_open_menu_button_shows_no_tooltip(cx: &mut TestAppContext) {
        assert_eq!(tooltips_on_hover(cx, true), 0);
    }

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
