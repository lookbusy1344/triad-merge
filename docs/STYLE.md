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
| A dialog, a prompt | the modals in `crates/ui/src/components/modal.rs`, `crates/ui_prompt` |
| A tooltip | `Tooltip` (`crates/ui/src/components/tooltip.rs`) |
| A button | `Button` and `ButtonLike` (`crates/ui/src/components/button/`), the settings window's buttons (`crates/settings_ui`) |
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

Chrome takes the theme's UI font, as Zed's does: the toolbar, the headings, the status bar, the dialogs and the tool windows. The panes and editors take the `font` setting, else the theme's monospace font (`Typography::editor_font`); so do commit IDs. Zed's scale is 14, 12 and 10 px; triad's chrome takes 12 px. Two settings size text: `font-size` the editors and panes, `toolbar-font-size` the toolbar, its badge and its menus, which take one value so they change together. Other chrome sizes are constants. Secondary text, such as field labels, takes `muted_foreground`. Text is normal weight. Semibold marks a heading: a pane's heading, a prompt's message, a list's group heading. Italic marks deletion notes and spacer notes.

## Controls

- A text button is `button::outlined`, after Zed's `Button` at `ButtonSize::Default` in `ButtonStyle::Outlined` with a `LabelSize::Small` label: 22 px tall, 4 px side padding, a 12 px label, and a faint 1 px edge: `border` 85 % of the way from the background, a step lighter than a divider. Zed's `border.variant` sits a third of the way; triad's is stronger, so the edge holds on a white dialog. On hover the edge turns `input` over `input` at half strength. gpui-kit's default variant at `.xsmall()` gives the hover, label and padding; the module sets the height and the resting edge.
- The button that finishes a dialog or writes from a tool window, which Enter takes in a prompt, is `button::default_action`: the outlined button on a light grey fill, `muted` at rest, `secondary` on hover and `secondary_active` while pressed, with the label in the text colour. Zed tints a prompt's default action in its accent; triad keeps it grey, as the accent reads too strong on a small dialog. A dialog or tool window has at most one. A button that only shows something, such as Preview Save, is outlined.
- An icon button and a toolbar button are `.ghost()` and `.small()`: transparent, with the theme's `accent` on hover, as Zed's `IconButton`s are subtle. Every `Input` is `.small()`.
- A pill over one end of a moved block is a `move_pill::move_pill`; § Moved blocks gives where it goes.
- A menu is a `menu_button::MenuButton`: 20 px rows, a 12 px icon, the label and the action's key, in the text size it is given. A row that toggles a state is a checkbox row, with a tick after its label while the state is on. gpui-component's `PopupMenu` has 26 px rows with 14 px text, which triad cannot shrink (`docs/gpui-kit-issues.md`).
- Heights: a text button 22 px, an icon button or input 24 px, a list row 24 px, a tool window header 32 px, a menu row 20 px, a tab 22 px inside a 2 px inset.
- Button icons take the button's size.
- A tooltip is `tooltip::tooltip`, after Zed's: 12 px text on 16 px lines, 4 px above and below, 8 px at the sides, at most 384 px wide, on gpui-kit's popover surface with its border and shadow. Zed stops at 288 px; triad's widest line is a full commit ID. An element takes it through `tooltip::build`, or `build_with_action` to show an action's keys. A gpui-kit button builds its own tooltip, which cannot be styled (`docs/gpui-kit-issues.md`), so a button with a tooltip sits in `tooltip::wrap` and never calls `Button::tooltip`.
- A heading whose text stands in for missing content, such as `(no description set)`, shows it muted at normal weight.
- A heading's mark, such as the stack window's `@`, sits before its lead on `pick_list::MARKED_BAND`, lime at 35 % strength, with 4 px either side and the theme's radius. The rest of the row stays plain.
- Corners take `theme.radius`, 4 px, Zed's `rounded_sm` on its buttons, list items and inputs. `corners::install` writes it into both theme files, over gpui-component's 6 px. Dialogs, menus and other raised surfaces take `theme.radius_lg`, 8 px, Zed's `rounded_lg` (§ Elevation). An element inside a rounded one takes the outer radius less the inset: a menu row, 4 px inside the menu's edge, takes 4 px.

## Tool windows

The directory, stack, history and absorb windows are built from `tool_window` and `pick_list`, after Zed's git panel. A new window that lists things uses them too.

- **Frame.** `tool_window::frame` fills the window in the theme's background, at 12 px text, and holds the `ToolWindow` key context. Escape in a list and Cancel (⌘W, ⌘Q) close the window through the same check as its close button.
- **Header.** 32 px tall over a 1 px `border`, 12 px in from each side. At the left, a muted summary that counts what the list holds: `10 files`, `4 commits`. At the right, the buttons, 4 px apart: Refresh first, then the window's own; Refresh and a button that only shows something are outlined, the one that writes is the default action.
- **Status line.** Under the header, an outcome or an error, muted, 12 px in from the sides with 4 px above and below; it wraps.
- **Lists.** Fill the space under the header, inset 4 px so a selected row's rounded corners show. Two lists side by side, as in absorb, stand on a 1 px `border` between them. The list takes the focus when its window opens: ↑ and ↓ move the selection, Enter opens the selected row, and the selection scrolls into view.
- **Rows.** `pick_list::row`: 24 px tall, 8 px side padding, so its text lines up with the header's at 12 px. Then, 8 px apart: a status icon, a muted lead such as IDs, the main text, which ends in an ellipsis, and a muted trail such as a state or a count. A heading under which rows are indented is semibold.
- **Paths.** A row whose text is a path shows the file name, then its directory muted, which is cut first. A deleted file's name and directory are struck through and muted.
- **Status icons.** 12 px, Zed's squares in the theme's status colours: added `SquarePlus` in `success`, modified `SquareDot` in `warning`, renamed `SquareArrowRight` in `info`, copied as added, since Zed's git panel has no copy state, deleted `SquareMinus` in `danger`, conflicted `TriangleAlert` in `danger`. The row's accessible name says the state in words.
- **Footer.** A result that belongs to the whole window, such as Preview Save's, sits under the lists over a 1 px `border`, laid out as the status line.
- **Empty list.** One muted line where the first row would be: `No files differ.`

## Elevation

A surface raised over the window is `elevation::surface`, after Zed's `ElevationIndex`: the popover's fill and text, a 1 px `border`, `theme.radius_lg` and a shadow for its height. `Elevation::Raised`, Zed's `ElevatedSurface`, holds a surface that leaves the rest of the window working: a menu, Go to Line. `Elevation::Modal`, Zed's `ModalSurface`, holds a dialog that takes the window until it closes: a prompt, over a scrim of black at 20 %. The shadows are Zed's figures, in `elevation.rs`. Inline parts, such as the toolbar, the headings and the status bar, sit flat.

## Go to Line

After Zed's go-to-line modal: a raised card 384 px wide, 8 px under the top of the result and centred over it, so the text does not move. The field sits over a 1 px `border`, then one muted line: `Current line 5 of 1018` while the field names no line, `Go to line 120` once it does. Both are 12 px chrome text, 8 px in from the card's sides with 4 px above and below; the field draws no frame of its own.

## Find

The result's search panel is gpui-component's, laid out as Zed's buffer search bar: the query with a case toggle, the replace toggle, previous and next, the match count and a close button. triad cannot style it (`docs/gpui-kit-issues.md`).

## Status bar

The merge and diff windows end in `status_bar::bar`, after Zed's `StatusBar`: 24 px tall under a 1 px `border` outside its height, across the window, on `secondary`. Its text is 12 px on 16 px lines in the UI font, muted, 8 px in from the sides, its items 8 px apart. An item that warns, such as mixed line endings, takes its own colour.

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

## Moved blocks

One look marks a moved block wherever triad shows one (spec §4.3, §4.6, §5). Build from these parts; a new view of moves uses them, never a variant.

| Part | Look | Where | Source |
|---|---|---|---|
| Band | The move tone, `MOVE_TONE` (`Tone::Moved`): `Tone::Both`'s teal at half the band opacity (`MOVED_BAND_SHARE`), full-line, in place of the region's or side's tone. A move explains where text came from and needs no action, so it reads fainter than a side's change | Both ends in the diff window; both ends in the merge window's side panes. Never base, whose lines keep their region's look, and never the result, whose bands name each line's input | `merge_view::diff::MOVE_TONE` |
| Pill | One row tall, 4 px padding, 16 px in from the pane's right edge, translucent pale blue, bold deep blue text at the chip text size. An arrow toward the other end, then its input's initial and first line: `R5 →`, `↓ L40`, `↑ B1` | The first row of each end in the diff window and the merge window's top panes, base included; never in the result. Several pills on one row stand in move order, 4 px apart (`MOVE_PILL_GAP`) | `move_pill::move_pill` |
| Gutter mark | `M` in the move tone | The diff window only. The merge window's top-pane gutters hold pick targets, and the result's gutter names each line's input | `merge_view::diff::MOVE_MARK` |
| Duplicate on the ruler | The copy's tone inside a 2 px outline in the conflict tone; the conflict tone alone on a mark too short for both | The merge window's ruler, over each copy the result holds | `DUPLICATE_OUTLINE` in `ruler.rs` |

- A pill's press and Go to Other End of Move (⇧⌘M) go to the other end with a jump's blink. An end named by a base line shows the base pane first.
- Initials: `L` left, `R` right, `B` base, as the result gutter's marks.
- The pill names the other end's input and line; the accessible name spells both out: `Moved block: go to its other end, left line 40`.
- The status bar counts duplicates, never moves.

## Constants

A size that the helpers above do not cover is a named `Pixels` or `Rems` constant at the top of its module, such as `open_dialog::TAB_HEIGHT`. A test that checks a size takes its expected value from this file.
