//! The lists of the tool windows (directory, stack, history, absorb), after Zed's
//! `ListItem` (`crates/ui/src/components/list/list_item.rs`), its git panel's entries and
//! `docs/STYLE.md`: rows across the list, a coloured status icon, a path's file name before
//! its muted directory, text that ends in an ellipsis, IDs and states muted, and a selection
//! that ↑ and ↓ move and Enter opens.

use gpui_kit::assets::IconName;
use gpui_kit::base::ObservedElement;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Theme};
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
/// The space between the list's edge and its rows, so a selected row's corners show.
pub(crate) const LIST_INSET: Pixels = px(4.);
/// The separator of the paths a row shows: `dir_diff::display` and jj's repository paths.
const SEPARATOR: char = '/';

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

/// A file's state, shown as Zed's git panel shows it: a square icon in the theme's colour
/// for the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
    Conflict,
}

impl Status {
    const fn icon(self) -> IconName {
        match self {
            Self::Added => IconName::SquarePlus,
            Self::Modified => IconName::SquareDot,
            Self::Deleted => IconName::SquareMinus,
            Self::Renamed => IconName::SquareArrowRight,
            Self::Conflict => IconName::TriangleAlert,
        }
    }

    /// Zed's version-control colours, from the theme's status colours.
    fn colour(self, theme: &Theme) -> Hsla {
        match self {
            Self::Added => theme.success,
            Self::Modified => theme.warning,
            Self::Renamed => theme.info,
            Self::Deleted | Self::Conflict => theme.danger,
        }
    }
}

/// How a row draws its main text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Shown {
    #[default]
    Text,
    /// A path: the file name, then its directory muted, as Zed's git panel shows one.
    Path,
    /// A deleted file's path: struck through and muted.
    Deleted,
}

/// A row's text: a status icon, a muted lead such as IDs, the main text, which takes the
/// room left and ends in an ellipsis, and a muted trail such as a state or a count.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Row {
    pub status: Option<Status>,
    pub lead: Option<SharedString>,
    pub text: SharedString,
    pub shown: Shown,
    pub trail: Option<SharedString>,
    /// The accessible name, where the parts do not read as one.
    pub label: Option<SharedString>,
}

/// `path`'s file name and the directory holding it; none at the top.
pub(crate) fn split_path(path: &str) -> (&str, Option<&str>) {
    match path.rsplit_once(SEPARATOR) {
        Some((dir, name)) => (name, Some(dir)),
        None => (path, None),
    }
}

impl Row {
    /// The row's accessible name: its label, else its parts, space-separated.
    pub(crate) fn label(&self) -> SharedString {
        if let Some(label) = &self.label {
            return label.clone();
        }
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
    let icon = content.status.map(|status| {
        Icon::new(status.icon()).xsmall().flex_none().text_color(status.colour(theme))
    });
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
                .children(icon)
                .children(content.lead.as_ref().map(part))
                .child(main_text(content, muted))
                .children(content.trail.as_ref().map(part)),
        )
}

/// A row's main text, which takes the room left: a path's file name keeps its width and its
/// directory ends in an ellipsis, as in Zed's git panel.
fn main_text(content: &Row, muted: Hsla) -> Div {
    let text = div().flex_1().min_w_0();
    match content.shown {
        Shown::Text => text.truncate().child(content.text.clone()),
        Shown::Path | Shown::Deleted => {
            let (name, dir) = split_path(&content.text);
            let deleted = content.shown == Shown::Deleted;
            let styled = |part: Div| {
                if deleted { part.line_through().text_color(muted) } else { part }
            };
            text.flex()
                .items_center()
                .gap_1()
                .overflow_hidden()
                .child(styled(div().flex_none().child(SharedString::from(name.to_owned()))))
                .children(dir.map(|dir| {
                    styled(div().min_w_0().truncate().text_color(muted))
                        .child(SharedString::from(dir.to_owned()))
                }))
        }
    }
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
        .min_w_0()
        .p(LIST_INSET)
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
    fn a_path_splits_into_its_file_name_and_directory() {
        assert_eq!(split_path("a.rs"), ("a.rs", None));
        assert_eq!(split_path("src/a.rs"), ("a.rs", Some("src")));
        assert_eq!(split_path("invoice/io/parser.py"), ("parser.py", Some("invoice/io")));
    }

    #[test]
    fn a_label_wins_over_the_parts() {
        let row = Row {
            text: "a.rs".into(),
            trail: Some("from b.rs".into()),
            label: Some("a.rs renamed from b.rs".into()),
            ..Row::default()
        };
        assert_eq!(row.label(), "a.rs renamed from b.rs");
    }

    #[test]
    fn a_label_joins_the_parts_present() {
        let row = Row { lead: Some("abc".into()), text: "a.txt".into(), ..Row::default() };
        assert_eq!(row.label(), "abc a.txt");
        let row = Row { text: "a.txt".into(), trail: Some("(inherited)".into()), ..Row::default() };
        assert_eq!(row.label(), "a.txt (inherited)");
        let row = Row { lead: Some("op".into()), trail: Some("(absent)".into()), ..Row::default() };
        assert_eq!(row.label(), "op (absent)");
    }
}
