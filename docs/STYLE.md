# UI style

triad's windows are compact and quiet, in the manner of Zed. This file sets the defaults for type, size, spacing and colour in `merge-app`. A view that departs from them names the reason in a comment.

The reference is Zed, checked out at `~/Documents/dev/other/zed`. Its `ui` crate sets the scale (`crates/ui/src/styles/`: `TextSize`, `IconSize`, `DynamicSpacing`) and the components (`crates/ui/src/components/`: `list/list_item.rs`, `context_menu.rs`, `button/`). Its panels set the layout of a window that lists things: `crates/git_ui/src/git_panel.rs` and `crates/project_panel/`.

## Working from Zed

Before building or changing a view, find the Zed view that does the same job and read how it draws: its heights, padding, colours, icons and keys. Copy that, in triad's sizes below. Take the job, not the look of one theme: a colour comes from the theme, never from a literal Zed shows. Where Zed and this file differ, this file wins; where this file says nothing, Zed decides, and the choice goes into this file in the same change. A view that departs from both says why in a comment.

| triad view | Zed view to read |
|---|---|
| A tool window: header, list, status line, footer | git panel (`git_panel.rs`), project panel |
| A list row, a heading | `ListItem`, `ListHeader`, the git panel's entries |
| A file's state | `GitStatusIcon` (`crates/git_ui/src/git_ui.rs`), the git panel's `path_formatted` |
| A menu | `ContextMenu` |
| A dialog, a prompt | the modals in `crates/ui/src/components/modal.rs` |
| The toolbar | the editor's toolbar items, `crates/editor/src/items.rs` |

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
- A pill over one end of a moved block is a `move_pill::move_pill`, in both the diff window and the merge window's top panes: one row tall, 4 px padding, 16 px in from the pane's right edge, translucent pale blue with bold deep blue text at the chip text size.
- A menu is a `menu_button::MenuButton`: 20 px rows, a 12 px icon, the label and the action's key, in the text size it is given. A row that toggles a state is a checkbox row, with a tick after its label while the state is on. gpui-component's `PopupMenu` has 26 px rows with 14 px text, which triad cannot shrink (`docs/gpui-kit-issues.md`).
- Toolbar and header buttons are `.ghost()`. A dialog has one `.primary()` button, the one that finishes it; a tool window has at most one, the one that writes, such as absorb's Apply. A button that only shows something, such as Preview Save, stays ghost.
- Heights: a small control 24 px, a list row 24 px, a tool window header 32 px, a menu row 20 px, a tab 22 px inside a 2 px inset.
- Button icons take the button's size.
- Corners take `theme.radius`. An element inside a rounded one takes the outer radius less the inset.

## Tool windows

The directory, stack, history and absorb windows are built from `tool_window` and `pick_list`, after Zed's git panel. A new window that lists things uses them too.

- **Frame.** `tool_window::frame` fills the window in the theme's background, at 12 px text, and holds the `ToolWindow` key context. Escape in a list and Cancel (⌘W, ⌘Q) close the window through the same check as its close button.
- **Header.** 32 px tall over a 1 px `border`, 12 px in from each side. At the left, a muted summary that counts what the list holds: `10 files`, `4 commits`. At the right, the buttons, 4 px apart: Refresh first, then the window's own.
- **Status line.** Under the header, an outcome or an error, muted, 12 px in from the sides with 4 px above and below; it wraps.
- **Lists.** Fill the space under the header, inset 4 px so a selected row's rounded corners show. Two lists side by side, as in absorb, stand on a 1 px `border` between them. The list takes the focus when its window opens: ↑ and ↓ move the selection, Enter opens the selected row, and the selection scrolls into view.
- **Rows.** `pick_list::row`: 24 px tall, 8 px side padding, so its text lines up with the header's at 12 px. Then, 8 px apart: a status icon, a muted lead such as IDs, the main text, which ends in an ellipsis, and a muted trail such as a state or a count. A heading under which rows are indented is semibold.
- **Paths.** A row whose text is a path shows the file name, then its directory muted, which is cut first. A deleted file's name and directory are struck through and muted.
- **Status icons.** 12 px, Zed's squares in the theme's status colours: added `SquarePlus` in `success`, modified `SquareDot` in `warning`, renamed `SquareArrowRight` in `info`, deleted `SquareMinus` in `danger`, conflicted `TriangleAlert` in `danger`. The row's accessible name says the state in words.
- **Footer.** A result that belongs to the whole window, such as Preview Save's, sits under the lists over a 1 px `border`, laid out as the status line.
- **Empty list.** One muted line where the first row would be: `No files differ.`

## Spacing

| Between | Size | Helper |
|---|---|---|
| A dialog's edge and its content | 12 px | `p_3()` |
| A tool window's header or status line and the window's sides | 12 px | `px_3()` |
| A tool window's list and its rows | 4 px | `p_1()` |
| Rows of a dialog, a label and its field | 8 px | `gap_2()` |
| Controls in one toolbar group | 4 px | `gap_1()` |
| Toolbar groups | 16 px | `gap_4()` |
| A toolbar's ends and the window edge | 8 px | `px_2()` |

## Colour

Take every colour from the theme: `foreground`, `muted_foreground`, `background`, `muted`, `border`, and the status colours `success`, `warning`, `info` and `danger`. Merge colours come from `tones.rs` and the settings, syntax colours from `syntax_colours.rs`. Removed and added lines shown outside an editor, as in the absorb window, take the editor font and size and the left and right tones' bands, as the diff window's do. A new colour literal in a view is a named constant with a comment giving the reason.

## Constants

A size that the helpers above do not cover is a named `Pixels` or `Rems` constant at the top of its module, such as `open_dialog::TAB_HEIGHT`. A test that checks a size takes its expected value from this file.
