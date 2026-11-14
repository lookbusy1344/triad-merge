# UI style

triad's windows are compact and quiet, in the manner of Zed. This file sets the defaults for type, size, spacing and colour in `merge-app`. A view that departs from them names the reason in a comment.

The reference is Zed's `ui` crate (`crates/ui/src/styles/`): `TextSize`, `IconSize` and `DynamicSpacing`, and `components/context_menu.rs`.

## Units

One rem is the theme's `font_size`, 16 px; gpui-component's `Root` sets it on every window. Spacing helpers follow Tailwind: `gap_1` is 4 px, `gap_2` 8 px, `p_3` 12 px, `gap_4` 16 px. Sizes sit on a 4 px grid, with 2 px for insets.

## Type

| Use | Size | Helper |
|---|---|---|
| Dialog and menu text, tab labels, field labels, errors | 12 px | `text_xs()` |
| A window's base text, under its controls | 14 px | `text_sm()` |
| Toolbar labels, the conflict badge, and the menus the toolbar opens | the `toolbar-font-size` setting, 8.4 by default | `Typography::toolbar_size` |
| Editor and pane text | the `font-size` setting, 10 by default (spec §6.4) | `Typography` |

Zed's scale is 14, 12 and 10 px; triad's chrome takes 12 px. Two settings size text: `font-size` the editors and panes, `toolbar-font-size` the toolbar, its badge and its menus, which take one value so they change together. Other chrome sizes are constants. Secondary text, such as field labels, takes `muted_foreground`. Text is normal weight. Semibold marks a heading: a pane's heading, a prompt's message, a list's group heading. Italic marks deletion notes and spacer notes.

## Controls

- Every `Button` and `Input` is `.small()`.
- A menu is a `split_menu::SplitMenu`: 20 px rows, a 12 px icon, the label and the action's key, in the text size it is given. gpui-component's `PopupMenu` has 26 px rows with 14 px text, which triad cannot shrink (`docs/gpui-kit-issues.md`).
- Toolbar buttons are `.ghost()`. A dialog has one `.primary()` button, the one that finishes it.
- A list in a tool window (stack, history, absorb) is built from `pick_list`, after Zed's `ListItem` (`crates/ui/src/components/list/list_item.rs`). A row spans the list, 24 px tall with 8 px side padding. Its text ends in an ellipsis when it does not fit; IDs, counts and states take `muted_foreground`. The list takes the focus when its window opens: ↑ and ↓ move the selection, Enter opens the selected row, and the selection scrolls into view.
- Heights: a small control 24 px, a list row 24 px, a menu row 20 px, a tab 22 px inside a 2 px inset.
- Button icons take the button's size.
- Corners take `theme.radius`. An element inside a rounded one takes the outer radius less the inset.

## Spacing

| Between | Size | Helper |
|---|---|---|
| A dialog's or tool window's edge and its content | 12 px | `p_3()` |
| Rows of a dialog or tool window, a label and its field | 8 px | `gap_2()` |
| Controls in one toolbar group | 4 px | `gap_1()` |
| Toolbar groups | 16 px | `gap_4()` |
| A toolbar's ends and the window edge | 8 px | `px_2()` |

## Colour

Take every colour from the theme: `foreground`, `muted_foreground`, `background`, `muted`, `border`, `danger`. Merge colours come from `tones.rs` and the settings, syntax colours from `syntax_colours.rs`. Removed and added lines shown outside an editor, as in the absorb window, take the editor font and size and the left and right tones' bands, as the diff window's do. A new colour literal in a view is a named constant with a comment giving the reason.

## Constants

A size that the helpers above do not cover is a named `Pixels` or `Rems` constant at the top of its module, such as `open_dialog::TAB_HEIGHT`. A test that checks a size takes its expected value from this file.
