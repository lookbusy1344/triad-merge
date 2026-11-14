//! The lists of the tool windows (stack, history, absorb), after Zed's `ListItem`
//! (`crates/ui/src/components/list/list_item.rs`) and `docs/STYLE.md`: rows across the
//! list, text that ends in an ellipsis, IDs and states muted, and a selection that ↑ and ↓
//! move and Enter opens.

use gpui_kit::base::ObservedElement;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::list::ListItem;
use gpui_kit::{
    App, Div, ElementId, FocusHandle, FontWeight, Hsla, InteractiveElement as _, KeyBinding,
    ParentElement as _, Pixels, Role, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, actions, div, px,
};

actions!(pick_list, [SelectNext, SelectPrevious, Activate, Leave]);

/// The key context of a focused list.
const CONTEXT: &str = "PickList";
/// A list row (`docs/STYLE.md`).
const ROW_HEIGHT: Pixels = px(24.);
/// A row's side padding (`docs/STYLE.md`).
const ROW_PADDING: Pixels = px(8.);
/// The extra left padding of a row under a heading, as Zed's `ListItem` indents a level.
const INDENT: Pixels = px(12.);

/// Bind a focused list's keys: ↓ and ↑ move the selection, Enter opens it, Escape leaves.
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(CONTEXT)),
        KeyBinding::new("enter", Activate, Some(CONTEXT)),
        KeyBinding::new("escape", Leave, Some(CONTEXT)),
    ]);
}

/// The row after `selected`, held at the last; the first when none is selected. None in an
/// empty list.
pub(crate) fn next(selected: Option<usize>, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(selected.map_or(0, |ix| ix.saturating_add(1).min(last)))
}

/// The row before `selected`, held at the first; the last when none is selected. None in
/// an empty list.
pub(crate) fn previous(selected: Option<usize>, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(selected.map_or(last, |ix| ix.saturating_sub(1).min(last)))
}

/// A row's text: a muted lead such as IDs, the main text, which takes the room left and
/// ends in an ellipsis, and a muted trail such as a state or a count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Row {
    pub lead: Option<SharedString>,
    pub text: SharedString,
    pub trail: Option<SharedString>,
}

impl Row {
    /// The row's accessible name: its parts, space-separated.
    pub(crate) fn label(&self) -> SharedString {
        [self.lead.as_ref(), Some(&self.text), self.trail.as_ref()]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .map(SharedString::as_ref)
            .collect::<Vec<&str>>()
            .join(" ")
            .into()
    }
}

/// How a row stands in its list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct State {
    pub selected: bool,
    pub disabled: bool,
    /// Under a heading.
    pub indented: bool,
}

/// A row across the list, 24 px tall.
pub(crate) fn row(id: impl Into<ElementId>, content: &Row, state: State, cx: &App) -> ListItem {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let part = |text: &SharedString| div().flex_none().text_color(muted).child(text.clone());
    let left = if state.indented { ROW_PADDING + INDENT } else { ROW_PADDING };
    ListItem::new(id)
        .role(Role::ListBoxOption)
        .aria_selected(state.selected)
        .accessibility_label(content.label())
        .selected(state.selected)
        .disabled(state.disabled)
        .h(ROW_HEIGHT)
        .py_0()
        .pl(left)
        .pr(ROW_PADDING)
        .rounded(theme.radius)
        .text_xs()
        .child(
            div()
                .flex()
                .items_center()
                .w_full()
                .min_w_0()
                .gap_2()
                .children(content.lead.as_ref().map(part))
                .child(div().flex_1().min_w_0().truncate().child(content.text.clone()))
                .children(content.trail.as_ref().map(part)),
        )
}

/// A heading over a group of rows: a row's height, semibold (`docs/STYLE.md`).
pub(crate) fn heading(
    id: impl Into<ElementId>,
    content: &Row,
    cx: &App,
) -> ObservedElement<Stateful<Div>> {
    let muted = cx.theme().muted_foreground;
    let part = |text: &SharedString| div().flex_none().text_color(muted).child(text.clone());
    div()
        .id(id)
        .test_support()
        .role(Role::Heading)
        .aria_label(content.label())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .h(ROW_HEIGHT)
        .px(ROW_PADDING)
        .gap_2()
        .text_xs()
        .children(content.lead.as_ref().map(part))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(content.text.clone()),
        )
        .children(content.trail.as_ref().map(part))
}

/// The list: it scrolls, takes the list keys while `focus` holds the focus, and scrolls
/// to a child by its index through `scroll`.
pub(crate) fn list(
    id: impl Into<ElementId>,
    focus: &FocusHandle,
    scroll: &ScrollHandle,
) -> ObservedElement<Stateful<Div>> {
    div()
        .id(id)
        .test_support()
        .role(Role::ListBox)
        .key_context(CONTEXT)
        .track_focus(focus)
        .track_scroll(scroll)
        .flex()
        .flex_col()
        .overflow_y_scroll()
}

/// A line in a list in `muted`, the theme's muted text colour, such as the empty list's note.
pub(crate) fn note(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    muted: Hsla,
) -> ObservedElement<Stateful<Div>> {
    let text = text.into();
    div()
        .id(id)
        .test_support()
        .role(Role::Status)
        .aria_label(text.clone())
        .px(ROW_PADDING)
        .text_color(muted)
        .whitespace_normal()
        .child(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_starts_at_the_first_and_holds_at_the_last() {
        assert_eq!(next(None, 3), Some(0));
        assert_eq!(next(Some(0), 3), Some(1));
        assert_eq!(next(Some(2), 3), Some(2));
        assert_eq!(next(None, 0), None);
        assert_eq!(next(Some(1), 0), None);
    }

    #[test]
    fn next_and_previous_bring_a_stale_selection_into_the_list() {
        // The list shrank under the selection after a refresh.
        assert_eq!(next(Some(7), 3), Some(2));
        assert_eq!(previous(Some(7), 3), Some(2));
    }

    #[test]
    fn previous_starts_at_the_last_and_holds_at_the_first() {
        assert_eq!(previous(None, 3), Some(2));
        assert_eq!(previous(Some(2), 3), Some(1));
        assert_eq!(previous(Some(0), 3), Some(0));
        assert_eq!(previous(None, 0), None);
    }

    #[test]
    fn a_label_joins_the_parts_present() {
        let row = Row { lead: Some("abc".into()), text: "a.txt".into(), trail: None };
        assert_eq!(row.label(), "abc a.txt");
        let row = Row { lead: None, text: "a.txt".into(), trail: Some("(inherited)".into()) };
        assert_eq!(row.label(), "a.txt (inherited)");
        let row = Row { lead: Some("op".into()), text: "".into(), trail: Some("(absent)".into()) };
        assert_eq!(row.label(), "op (absent)");
    }
}
