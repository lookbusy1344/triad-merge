//! The ways to fill a region from the two sides, offered by the result pane's right-click
//! menu (spec §5).

use gpui_kit::Action;
use merge_core::document::Resolution;
use merge_core::index::RegionIx;
use merge_core::merge3::Source;
use merge_core::suggest::Suggestion;

use crate::{
    CarryMovedEdit, ClearChoice, TakeBase, TakeLeft, TakeLeftThenRight, TakeRight,
    TakeRightThenLeft, TakeSortedUnion,
};

const LEFT: Source = Source::Side(0);
const RIGHT: Source = Source::Side(1);
const BASE: Source = Source::Base(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    Nothing,
    Base,
    Left,
    Right,
    LeftThenRight,
    RightThenLeft,
}

impl Choice {
    /// The menu's items, in order.
    pub(crate) const ALL: [Choice; 6] = [
        Choice::Nothing,
        Choice::Base,
        Choice::Left,
        Choice::Right,
        Choice::LeftThenRight,
        Choice::RightThenLeft,
    ];

    /// The pick this choice makes.
    pub(crate) fn sources(self) -> &'static [Source] {
        match self {
            Choice::Nothing => &[],
            Choice::Base => &[BASE],
            Choice::Left => &[LEFT],
            Choice::Right => &[RIGHT],
            Choice::LeftThenRight => &[LEFT, RIGHT],
            Choice::RightThenLeft => &[RIGHT, LEFT],
        }
    }

    /// The window action that makes this choice in the current region.
    pub(crate) fn action(self) -> Box<dyn Action> {
        match self {
            Choice::Nothing => Box::new(ClearChoice),
            Choice::Base => Box::new(TakeBase),
            Choice::Left => Box::new(TakeLeft),
            Choice::Right => Box::new(TakeRight),
            Choice::LeftThenRight => Box::new(TakeLeftThenRight),
            Choice::RightThenLeft => Box::new(TakeRightThenLeft),
        }
    }

    /// The choice `resolution` holds, when it is one of these.
    pub(crate) fn of(resolution: &Resolution) -> Option<Choice> {
        match resolution {
            Resolution::Pick(sources) => {
                Self::ALL.into_iter().find(|choice| choice.sources() == sources.as_slice())
            }
            Resolution::Auto | Resolution::Edited | Resolution::Unresolved => None,
        }
    }
}

/// What a right-click menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    Take(Choice),
    /// Replace the region's text with its sorted union (spec §4.4).
    SortedUnion,
    /// Carry `from`'s edit of a block the other side moved to the block's new place, which
    /// starts on result line `line`, counted from 1 (spec §4.4).
    CarryEdit {
        from: Choice,
        line: usize,
    },
}

impl MenuAction {
    /// The window action that does this in the current region.
    pub(crate) fn action(self) -> Box<dyn Action> {
        match self {
            MenuAction::Take(choice) => choice.action(),
            MenuAction::SortedUnion => Box::new(TakeSortedUnion),
            MenuAction::CarryEdit { .. } => Box::new(CarryMovedEdit),
        }
    }

    /// The item that takes `suggestion` in a three-way merge. `line_of` gives the result
    /// line, counted from 1, where a region starts.
    pub(crate) fn suggested(
        suggestion: &Suggestion,
        line_of: impl Fn(RegionIx) -> usize,
    ) -> MenuAction {
        match suggestion {
            Suggestion::EverySide => MenuAction::Take(Choice::LeftThenRight),
            Suggestion::SortedUnion(_) => MenuAction::SortedUnion,
            Suggestion::CarryEdit { side, to, .. } => MenuAction::CarryEdit {
                from: if Source::Side(*side) == LEFT { Choice::Left } else { Choice::Right },
                line: line_of(*to),
            },
        }
    }
}

/// An item of the right-click menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MenuChoice {
    pub(crate) action: MenuAction,
    /// The region holds this choice.
    pub(crate) checked: bool,
    /// One of the region's suggestions (spec §4.4), listed above the choices.
    pub(crate) suggested: bool,
}

/// The right-click menu's items: the region's suggestions first, then every choice, with
/// the one the region holds checked.
pub(crate) fn menu_choices(current: Option<Choice>, suggestions: &[MenuAction]) -> Vec<MenuChoice> {
    let suggested =
        suggestions.iter().map(|&action| MenuChoice { action, checked: false, suggested: true });
    let choices = Choice::ALL.into_iter().map(|choice| MenuChoice {
        action: MenuAction::Take(choice),
        checked: current == Some(choice),
        suggested: false,
    });
    suggested.chain(choices).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(choice: Choice, checked: bool) -> MenuChoice {
        MenuChoice { action: MenuAction::Take(choice), checked, suggested: false }
    }

    #[test]
    fn a_pick_of_the_sides_is_its_choice() {
        let of = |sources: &[Source]| Choice::of(&Resolution::Pick(sources.to_vec()));
        assert_eq!(of(&[]), Some(Choice::Nothing));
        assert_eq!(of(&[BASE]), Some(Choice::Base));
        assert_eq!(of(&[LEFT]), Some(Choice::Left));
        assert_eq!(of(&[RIGHT]), Some(Choice::Right));
        assert_eq!(of(&[LEFT, RIGHT]), Some(Choice::LeftThenRight));
        assert_eq!(of(&[RIGHT, LEFT]), Some(Choice::RightThenLeft));
    }

    #[test]
    fn other_picks_and_resolutions_are_no_choice() {
        let of = |sources: &[Source]| Choice::of(&Resolution::Pick(sources.to_vec()));
        assert_eq!(of(&[LEFT, BASE]), None);
        assert_eq!(of(&[BASE, RIGHT]), None);
        assert_eq!(of(&[RIGHT, LEFT, BASE]), None);
        assert_eq!(Choice::of(&Resolution::Unresolved), None);
        assert_eq!(Choice::of(&Resolution::Edited), None);
        assert_eq!(Choice::of(&Resolution::Auto), None);
    }

    #[test]
    fn without_suggestions_the_menu_lists_every_choice_and_checks_the_current_one() {
        assert_eq!(
            menu_choices(Some(Choice::Right), &[]),
            [
                plain(Choice::Nothing, false),
                plain(Choice::Base, false),
                plain(Choice::Left, false),
                plain(Choice::Right, true),
                plain(Choice::LeftThenRight, false),
                plain(Choice::RightThenLeft, false),
            ]
        );
        assert!(menu_choices(None, &[]).iter().all(|item| !item.checked));
    }

    #[test]
    fn every_side_is_suggested_as_left_then_right_above_the_choices() {
        let suggested = MenuChoice {
            action: MenuAction::Take(Choice::LeftThenRight),
            checked: false,
            suggested: true,
        };
        assert_eq!(
            menu_choices(
                Some(Choice::LeftThenRight),
                &[MenuAction::suggested(&Suggestion::EverySide, |_| 0)]
            ),
            [
                suggested,
                plain(Choice::Nothing, false),
                plain(Choice::Base, false),
                plain(Choice::Left, false),
                plain(Choice::Right, false),
                plain(Choice::LeftThenRight, true),
                plain(Choice::RightThenLeft, false),
            ]
        );
    }

    #[test]
    fn a_sorted_union_is_suggested_in_the_order_the_core_gives() {
        let suggestions = [Suggestion::EverySide, Suggestion::SortedUnion("b\nc\n".to_owned())];
        let actions: Vec<MenuAction> =
            suggestions.iter().map(|s| MenuAction::suggested(s, |_| 0)).collect();
        let suggested: Vec<MenuAction> = menu_choices(None, &actions)
            .into_iter()
            .filter(|item| item.suggested)
            .map(|item| item.action)
            .collect();
        assert_eq!(suggested, [MenuAction::Take(Choice::LeftThenRight), MenuAction::SortedUnion]);
    }

    #[test]
    fn a_carried_edit_names_the_editing_side_and_the_line_the_block_moved_to() {
        let carry = |side| Suggestion::CarryEdit {
            side,
            to: RegionIx::new(2),
            text: "m1\nM2\n".to_owned(),
        };
        let line_of = |region: RegionIx| region.get() * 10 + 1;
        assert_eq!(
            MenuAction::suggested(&carry(1), line_of),
            MenuAction::CarryEdit { from: Choice::Right, line: 21 }
        );
        assert_eq!(
            MenuAction::suggested(&carry(0), line_of),
            MenuAction::CarryEdit { from: Choice::Left, line: 21 }
        );
    }
}
