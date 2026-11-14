//! The toolbar (spec §5, §6.3): a button for each window action, in groups.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    Action, AnyElement, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    Pixels, Role, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};

use crate::split_menu::{Item, SplitMenu};
use crate::window::TOOLBAR_ID;
use crate::{
    About, ClearChoice, Find, GoToLine, NextConflict, NextDifference, PreviousConflict,
    PreviousDifference, Redo, TakeLeftThenRight, TakeRightThenLeft, ToggleBase, ToggleBasePane,
    ToggleLeft, ToggleRight, ToggleSoftWrap, ToggleSyntax, Undo,
};
use crate::{logo, menu_bar, strings};

/// A toolbar button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    PreviousDifference,
    NextDifference,
    PreviousConflict,
    NextConflict,
    Left,
    Base,
    Right,
    Clear,
    LeftThenRight,
    RightThenLeft,
    Undo,
    Redo,
    BasePane,
    Find,
    GoToLine,
    Syntax,
    SoftWrap,
}

/// The mark that opens the About window, also used by tests.
pub const MARK_ID: &str = "mark";

/// The split button that takes both sides, also used by tests.
pub const BOTH_ID: &str = "tool-both";
pub use crate::split_menu::{MENU_TRIGGER_ID, POPUP_MENU_ID};
/// The orders the Both menu offers, at their positions in the menu.
const BOTH_MENU: [(Tool, &str); 2] = [
    (Tool::LeftThenRight, strings::LEFT_THEN_RIGHT),
    (Tool::RightThenLeft, strings::RIGHT_THEN_LEFT),
];
pub const BOTH_MENU_LEFT_THEN_RIGHT: usize = 0;
pub const BOTH_MENU_RIGHT_THEN_LEFT: usize = 1;

/// The narrowest window whose merge toolbar shows its labels at the default text size; a
/// narrower one shows icons alone. It fits [`REFERENCE_BADGE`].
pub const LABELS_MIN_WIDTH: Pixels = px(1052.);
/// The narrowest window whose merge toolbar fits without its labels, at the default text size.
pub const COMPACT_MIN_WIDTH: Pixels = px(788.);
/// The longest conflict badge at two-digit counts, which the label threshold makes room for.
const REFERENCE_BADGE: &str = "Conflict 10 of 12 · 12 unresolved";
/// [`REFERENCE_BADGE`] in a merge of more than two sides, with the sides a conflict keeps.
const REFERENCE_BADGE_SIDES: &str = "Conflict 10 of 12 · 3 of 3 sides · 12 unresolved";

/// What the conflict badge shows, which sets the room the toolbar keeps for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// The counts of a three-way merge.
    Counts,
    /// The counts and, in a conflict, the sides it keeps: a merge of more than two sides.
    Sides,
}

impl Badge {
    const fn reference(self) -> &'static str {
        match self {
            Self::Counts => REFERENCE_BADGE,
            Self::Sides => REFERENCE_BADGE_SIDES,
        }
    }
}

/// A toolbar control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    /// One button, with a label when the tool has one.
    Button(Tool),
    /// Previous and next as icons, with the label between them.
    Pair(Tool, Tool),
    /// Left then right, with a menu of both orders.
    Both,
}

impl Control {
    /// The tool whose label the control shows.
    fn labelled(self) -> Tool {
        match self {
            Self::Button(tool) | Self::Pair(tool, _) => tool,
            Self::Both => Tool::LeftThenRight,
        }
    }

    /// The tools the control runs.
    #[cfg(test)]
    fn tools(self) -> impl Iterator<Item = Tool> {
        let [(first, _), (second, _)] = BOTH_MENU;
        let (first, second) = match self {
            Self::Button(tool) => (tool, None),
            Self::Pair(previous, next) => (previous, Some(next)),
            Self::Both => (first, Some(second)),
        };
        std::iter::once(first).chain(second)
    }
}

/// Whether the toolbar shows text labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Labels {
    Shown,
    Hidden,
}

impl Labels {
    /// The labels a merge window of `width` shows, when labels need `min_width`.
    #[must_use]
    pub(crate) fn for_width(width: Pixels, min_width: Pixels) -> Self {
        if width >= min_width { Self::Shown } else { Self::Hidden }
    }
}

/// The merge window's controls, in groups, from left to right.
pub(crate) const GROUPS: [&[Control]; 4] = [
    &[
        Control::Pair(Tool::PreviousDifference, Tool::NextDifference),
        Control::Pair(Tool::PreviousConflict, Tool::NextConflict),
    ],
    &[
        Control::Button(Tool::Left),
        Control::Button(Tool::Base),
        Control::Button(Tool::Right),
        Control::Button(Tool::Clear),
        Control::Both,
    ],
    &[Control::Button(Tool::Undo), Control::Button(Tool::Redo)],
    &[
        Control::Button(Tool::BasePane),
        Control::Button(Tool::Find),
        Control::Button(Tool::GoToLine),
        Control::Button(Tool::Syntax),
        Control::Button(Tool::SoftWrap),
    ],
];

/// The diff window's controls, in groups, from left to right. It has no merge regions.
pub(crate) const DIFF_GROUPS: [&[Control]; 4] = [
    &[Control::Pair(Tool::PreviousDifference, Tool::NextDifference)],
    &[Control::Button(Tool::Undo), Control::Button(Tool::Redo)],
    &[Control::Button(Tool::Find)],
    &[Control::Button(Tool::Syntax), Control::Button(Tool::SoftWrap)],
];

/// The narrowest window whose merge toolbar shows its labels in text of `text_size` with
/// `badge`: [`LABELS_MIN_WIDTH`], less the labels' and [`REFERENCE_BADGE`]'s text at the
/// default size, plus the labels' and `badge`'s reference text at `text_size`. The buttons,
/// gaps and padding stay the same.
#[must_use]
pub fn labels_min_width(text_size: Pixels, badge: Badge, window: &Window) -> Pixels {
    let style = window.text_style();
    let width = |badge: Badge, size| {
        GROUPS
            .iter()
            .flat_map(|group| group.iter())
            .filter_map(|&control| strings::tool_label(control.labelled()))
            .chain([badge.reference()])
            .map(|text| {
                let run = style.to_run(text.len());
                window.text_system().shape_line(text.into(), size, &[run], None).width
            })
            .fold(Pixels::ZERO, |total, width| total + width)
    };
    LABELS_MIN_WIDTH + width(badge, text_size) - width(Badge::Counts, crate::TOOLBAR_FONT_SIZE)
}

/// Every tool `groups` runs, in order.
#[cfg(test)]
pub(crate) fn tools(groups: &[&[Control]]) -> impl Iterator<Item = Tool> {
    groups.iter().flat_map(|group| group.iter()).flat_map(|control| control.tools())
}

/// The mark, as the button that opens the About window.
pub(crate) fn mark(dark: bool) -> Button {
    Button::new(MARK_ID)
        .ghost()
        .small()
        .child(logo::mark(dark))
        .accessibility_label(strings::ABOUT_TITLE)
        .tooltip(strings::ABOUT_TITLE)
        .on_click(|_: &ClickEvent, window, cx| window.dispatch_action(Box::new(About), cx))
}

/// A toolbar of `groups`, labelled `label`, whose tooltips show the keys in `context`, with
/// text of `text_size`.
pub(crate) fn bar(
    label: &'static str,
    groups: &[&[Control]],
    context: &'static str,
    dark: bool,
    labels: Labels,
    text_size: Pixels,
) -> impl IntoElement + ParentElement + use<> {
    // Linux and Windows show the mark in the title bar.
    let mark = (!menu_bar::IN_WINDOW).then(|| mark(dark));
    h_flex()
        .id(TOOLBAR_ID)
        .test_support()
        .role(Role::Toolbar)
        .aria_label(label)
        .w_full()
        .px_2()
        .gap_4()
        .children(mark)
        .children(groups.iter().map(|group| {
            h_flex()
                .gap_1()
                .children(group.iter().map(|&control| render(control, context, labels, text_size)))
        }))
}

fn render(
    control: Control,
    context: &'static str,
    labels: Labels,
    text_size: Pixels,
) -> AnyElement {
    match control {
        Control::Button(tool) => tool.button(context, labels, text_size).into_any_element(),
        Control::Pair(previous, next) => h_flex()
            .child(previous.button(context, Labels::Hidden, text_size))
            .children(
                strings::tool_label(previous)
                    .filter(|_| labels == Labels::Shown)
                    .map(|text| label(previous, text, text_size)),
            )
            .child(next.button(context, Labels::Hidden, text_size))
            .into_any_element(),
        // The menu's text matches the toolbar's labels.
        Control::Both => SplitMenu::new(
            BOTH_ID,
            Tool::LeftThenRight.button(context, labels, text_size),
            strings::BOTH_MENU,
            context,
            text_size,
            BOTH_MENU.iter().map(|&(tool, label)| Item {
                icon: tool.icon(),
                label,
                action: tool.action(),
            }),
        )
        .into_any_element(),
    }
}

/// A tool's text label, in the toolbar's text.
fn label(tool: Tool, text: &'static str, text_size: Pixels) -> impl IntoElement {
    div().id(tool.label_id()).test_support().text_size(text_size).child(text)
}

impl Tool {
    /// The button's element id, also used by tests.
    #[must_use]
    pub fn id(self) -> ElementId {
        ElementId::Name(format!("tool-{self:?}").into())
    }

    /// The id of the button's text label, also used by tests.
    #[must_use]
    pub fn label_id(self) -> ElementId {
        ElementId::Name(format!("tool-label-{self:?}").into())
    }

    /// The button: its icon, and its label when it has one and `labels` shows it. A click
    /// leaves the focus where it was.
    fn button(self, context: &'static str, labels: Labels, text_size: Pixels) -> Button {
        let action = self.action();
        let tooltip = strings::tool_name(self);
        Button::new(self.id())
            .ghost()
            .small()
            .icon(self.icon())
            // A label takes the button's own text size, so the text goes in as a child.
            .children(
                strings::tool_label(self)
                    .filter(|_| labels == Labels::Shown)
                    .map(|text| label(self, text, text_size)),
            )
            .accessibility_label(tooltip)
            .tooltip_with_action(tooltip, action.as_ref(), Some(context))
            .on_click(move |_: &ClickEvent, window, cx| {
                window.dispatch_action(action.boxed_clone(), cx);
            })
    }

    /// The action the button runs, the same one as its key.
    pub(crate) fn action(self) -> Box<dyn Action> {
        match self {
            Self::PreviousDifference => Box::new(PreviousDifference),
            Self::NextDifference => Box::new(NextDifference),
            Self::PreviousConflict => Box::new(PreviousConflict),
            Self::NextConflict => Box::new(NextConflict),
            Self::Left => Box::new(ToggleLeft),
            Self::Base => Box::new(ToggleBase),
            Self::Right => Box::new(ToggleRight),
            Self::Clear => Box::new(ClearChoice),
            Self::LeftThenRight => Box::new(TakeLeftThenRight),
            Self::RightThenLeft => Box::new(TakeRightThenLeft),
            Self::Undo => Box::new(Undo),
            Self::Redo => Box::new(Redo),
            Self::BasePane => Box::new(ToggleBasePane),
            Self::Find => Box::new(Find),
            Self::GoToLine => Box::new(GoToLine),
            Self::Syntax => Box::new(ToggleSyntax),
            Self::SoftWrap => Box::new(ToggleSoftWrap),
        }
    }

    /// The button's icon. Every tool has its own.
    pub(crate) fn icon(self) -> IconName {
        match self {
            Self::PreviousDifference => IconName::ChevronUp,
            Self::NextDifference => IconName::ChevronDown,
            Self::PreviousConflict => IconName::ChevronsUp,
            Self::NextConflict => IconName::ChevronsDown,
            Self::Left => IconName::SquareArrowLeft,
            Self::Base => IconName::SquareDot,
            Self::Right => IconName::SquareArrowRight,
            Self::Clear => IconName::Eraser,
            Self::LeftThenRight => IconName::ArrowLeftRight,
            Self::RightThenLeft => IconName::ArrowRightLeft,
            Self::Undo => IconName::Undo2,
            Self::Redo => IconName::Redo2,
            Self::BasePane => IconName::Columns3,
            Self::Find => IconName::Search,
            Self::GoToLine => IconName::Hash,
            Self::Syntax => IconName::Highlighter,
            Self::SoftWrap => IconName::TextWrap,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// The number of `Tool` variants.
    const TOOL_COUNT: usize = 17;

    #[test]
    fn every_tool_has_its_own_icon() {
        let icons: HashSet<_> = tools(&GROUPS).map(Tool::icon).collect();
        assert_eq!(icons.len(), TOOL_COUNT);
    }

    #[test]
    fn the_merge_toolbar_runs_every_tool_once() {
        let all: Vec<_> = tools(&GROUPS).collect();
        let distinct: HashSet<_> = all.iter().copied().collect();
        assert_eq!((all.len(), distinct.len()), (TOOL_COUNT, TOOL_COUNT));
    }

    #[test]
    fn both_windows_toggle_soft_wrap_from_the_toolbar() {
        assert!(tools(&GROUPS).any(|tool| tool == Tool::SoftWrap));
        assert!(tools(&DIFF_GROUPS).any(|tool| tool == Tool::SoftWrap));
        assert_eq!(Tool::SoftWrap.icon(), IconName::TextWrap);
    }

    #[test]
    fn labels_show_from_their_minimum_width() {
        let min = LABELS_MIN_WIDTH;
        assert_eq!(Labels::for_width(min, min), Labels::Shown);
        assert_eq!(Labels::for_width(min - px(1.), min), Labels::Hidden);
        assert_eq!(Labels::for_width(COMPACT_MIN_WIDTH, min), Labels::Hidden);
    }

    #[test]
    fn the_navigation_icons_point_the_way_they_go() {
        assert_eq!(Tool::PreviousDifference.icon(), IconName::ChevronUp);
        assert_eq!(Tool::NextDifference.icon(), IconName::ChevronDown);
        assert_eq!(Tool::PreviousConflict.icon(), IconName::ChevronsUp);
        assert_eq!(Tool::NextConflict.icon(), IconName::ChevronsDown);
    }
}
