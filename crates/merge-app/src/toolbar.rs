//! The toolbar (spec §5, §6.3): a button for each window action, in groups.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Selectable as _, Sizable as _, Theme, h_flex};
use gpui_kit::{
    Action, AnyElement, ClickEvent, Div, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, Role, Stateful, StatefulInteractiveElement as _, Styled,
    TestSupportExt as _, Window, div, prelude::FluentBuilder as _, px,
};
use merge_core::merge3::{LEFT, RIGHT, Source};

use crate::menu_button::{Item, MenuButton};
use crate::window::TOOLBAR_ID;
use crate::{
    About, ClearChoice, Find, GoToLine, NextConflict, NextDifference, PreviousConflict,
    PreviousDifference, Redo, TakeLeftThenRight, TakeRightThenLeft, ToggleBase, ToggleBasePane,
    ToggleLeft, ToggleRight, ToggleSoftWrap, ToggleSyntax, Undo,
};
use crate::{ViewTicks, logo, menu_bar, strings, tooltip};

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
    LeftThenRight,
    RightThenLeft,
    Clear,
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

/// The menu button of the picks beyond Left and Right, and its label; also used by tests.
pub const MORE_ID: &str = "tool-more";
pub const MORE_LABEL_ID: &str = "tool-label-more";
pub use crate::menu_button::POPUP_MENU_ID;
/// The More button's icon.
pub(crate) const MORE_ICON: IconName = IconName::SquareMenu;
/// The picks the More menu offers, at their positions in the menu.
const MORE_MENU: [(Tool, &str); 4] = [
    (Tool::Base, strings::BASE),
    (Tool::LeftThenRight, strings::LEFT_THEN_RIGHT),
    (Tool::RightThenLeft, strings::RIGHT_THEN_LEFT),
    (Tool::Clear, strings::NONE),
];
pub const MORE_MENU_BASE: usize = 0;
pub const MORE_MENU_LEFT_THEN_RIGHT: usize = 1;
pub const MORE_MENU_RIGHT_THEN_LEFT: usize = 2;
pub const MORE_MENU_NONE: usize = 3;

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
    /// A menu of the picks beyond Left and Right.
    More,
}

impl Control {
    /// The label the control shows, when labels show.
    fn label(self) -> Option<&'static str> {
        match self {
            Self::Button(tool) | Self::Pair(tool, _) => strings::tool_label(tool),
            Self::More => Some(strings::MORE),
        }
    }

    /// The tools the control runs.
    #[cfg(test)]
    fn tools(self) -> Vec<Tool> {
        match self {
            Self::Button(tool) => vec![tool],
            Self::Pair(previous, next) => vec![previous, next],
            Self::More => MORE_MENU.map(|(tool, _)| tool).to_vec(),
        }
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

/// The tools that toggle a source in the current region's pick.
pub(crate) const PICKS: [Tool; 3] = [Tool::Left, Tool::Base, Tool::Right];

/// The view toggles that `ticks` turns on, as the View menu ticks them.
pub(crate) fn views_on(ticks: ViewTicks) -> impl Iterator<Item = Tool> {
    [
        (Tool::BasePane, ticks.base_pane),
        (Tool::Syntax, ticks.syntax),
        (Tool::SoftWrap, ticks.soft_wrap),
    ]
    .into_iter()
    .filter_map(|(tool, on)| on.then_some(tool))
}

/// The merge window's controls, in groups, from left to right.
pub(crate) const GROUPS: [&[Control]; 4] = [
    &[
        Control::Pair(Tool::PreviousDifference, Tool::NextDifference),
        Control::Pair(Tool::PreviousConflict, Tool::NextConflict),
    ],
    &[Control::Button(Tool::Left), Control::Button(Tool::Right), Control::More],
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
            .filter_map(|&control| control.label())
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
    groups.iter().flat_map(|group| group.iter()).flat_map(|&control| control.tools())
}

/// The mark, as the button that opens the About window.
pub(crate) fn mark(dark: bool) -> Stateful<Div> {
    let button = Button::new(MARK_ID)
        .ghost()
        .small()
        .child(logo::mark(dark))
        .accessibility_label(strings::ABOUT_TITLE)
        .on_click(|_: &ClickEvent, window, cx| window.dispatch_action(Box::new(About), cx));
    tooltip::wrap(MARK_ID, button, tooltip::build(strings::ABOUT_TITLE))
}

/// A toolbar of `groups`, labelled `label`, whose tooltips show the keys in `context`, with
/// text of `text_size`. The toggles in `pressed` show pressed.
pub(crate) fn bar(
    label: &'static str,
    groups: &[&[Control]],
    context: &'static str,
    theme: &Theme,
    labels: Labels,
    text_size: Pixels,
    pressed: &[Tool],
) -> impl IntoElement + ParentElement + Styled + use<> {
    // Linux and Windows show the mark in the title bar.
    let mark = (!menu_bar::IN_WINDOW).then(|| mark(theme.is_dark()));
    h_flex()
        .id(TOOLBAR_ID)
        .test_support()
        .role(Role::Toolbar)
        .aria_label(label)
        .w_full()
        .px_2()
        .gap_4()
        .font_family(theme.font_family.clone())
        .children(mark)
        .children(groups.iter().map(|group| {
            h_flex().gap_1().children(
                group.iter().map(|&control| render(control, context, labels, text_size, pressed)),
            )
        }))
}

fn render(
    control: Control,
    context: &'static str,
    labels: Labels,
    text_size: Pixels,
    pressed: &[Tool],
) -> AnyElement {
    match control {
        Control::Button(tool) => {
            let button = tool.button(labels, text_size).when(tool.toggles(), |button| {
                let on = pressed.contains(&tool);
                button.selected(on).toggled(on)
            });
            tool.with_tooltip(button, context).into_any_element()
        }
        Control::Pair(previous, next) => h_flex()
            .child(previous.with_tooltip(previous.button(Labels::Hidden, text_size), context))
            .children(
                strings::tool_label(previous)
                    .filter(|_| labels == Labels::Shown)
                    .map(|text| label(previous, text, text_size)),
            )
            .child(next.with_tooltip(next.button(Labels::Hidden, text_size), context))
            .into_any_element(),
        // The menu's text matches the toolbar's labels. Base ticks while the region takes it.
        Control::More => MenuButton::new(
            MORE_ID,
            more_trigger(labels, text_size),
            strings::MORE_MENU,
            context,
            text_size,
            MORE_MENU.iter().map(|&(tool, label)| Item {
                icon: tool.icon(),
                label,
                action: tool.action(),
                checked: tool.toggles().then(|| pressed.contains(&tool)),
            }),
        )
        .tooltip(Rc::new(tooltip::build(strings::MORE_MENU)))
        .into_any_element(),
    }
}

/// The button that opens the More menu, with its label when `labels` shows it.
fn more_trigger(labels: Labels, text_size: Pixels) -> Button {
    Button::new(MORE_ID)
        .ghost()
        .small()
        .icon(MORE_ICON)
        .children((labels == Labels::Shown).then(|| {
            div().id(MORE_LABEL_ID).test_support().text_size(text_size).child(strings::MORE)
        }))
        .accessibility_label(strings::MORE_MENU)
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

    /// `button` with the tool's tooltip, which shows the keys bound in `context`.
    fn with_tooltip(self, button: Button, context: &'static str) -> Stateful<Div> {
        let build = tooltip::build_with_action(strings::tool_name(self), self.action(), context);
        tooltip::wrap(self.id(), button, build)
    }

    /// The button: its icon, and its label when it has one and `labels` shows it. A click
    /// leaves the focus where it was.
    fn button(self, labels: Labels, text_size: Pixels) -> Button {
        let action = self.action();
        let name = strings::tool_name(self);
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
            .accessibility_label(name)
            .on_click(move |_: &ClickEvent, window, cx| {
                window.dispatch_action(action.boxed_clone(), cx);
            })
    }

    /// Whether the button is a toggle, shown pressed while its state is on: a source in the
    /// current region's pick, or a view.
    #[must_use]
    pub const fn toggles(self) -> bool {
        matches!(
            self,
            Self::Left | Self::Base | Self::Right | Self::BasePane | Self::Syntax | Self::SoftWrap
        )
    }

    /// The source a pick button toggles in the current region.
    #[must_use]
    pub const fn source(self) -> Option<Source> {
        match self {
            Self::Left => Some(Source::Side(LEFT)),
            Self::Base => Some(Source::Base(0)),
            Self::Right => Some(Source::Side(RIGHT)),
            _ => None,
        }
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
            Self::LeftThenRight => Box::new(TakeLeftThenRight),
            Self::RightThenLeft => Box::new(TakeRightThenLeft),
            Self::Clear => Box::new(ClearChoice),
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
            Self::LeftThenRight => IconName::ArrowLeftRight,
            Self::RightThenLeft => IconName::ArrowRightLeft,
            Self::Clear => IconName::Eraser,
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
    fn the_toolbar_takes_the_ui_font() {
        let theme =
            Theme { font_family: "UI".into(), mono_font_family: "Mono".into(), ..Theme::default() };
        let mut bar = bar("Toolbar", &DIFF_GROUPS, "Diff", &theme, Labels::Shown, px(8.4), &[]);
        assert_eq!(bar.style().text.font_family, Some("UI".into()));
    }

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
