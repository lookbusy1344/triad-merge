# gpui-kit issues

Known problems in `gpui-kit` 0.8.1 (upstream commit `99576db58ec8103417c41a092e891b4022d24e7e`) that affect triad, and triad's answer to each. `vendor/gpui-base/PATCHES.md` lists the patch.

## Range fills cover only the text extent

`RangeDecorationStyle::Fill` paints the glyphs' extent, so a short line gets a short band, and the editor has no gutter API. triad's vendored `gpui-base` adds `RangeDecorationStyle::FullLine` and `GutterMark` ([PATCHES.md](../vendor/gpui-base/PATCHES.md)).

## Published crate tests do not compile

A `gpui-base` test in `src/text/text_view.rs` includes `../../../../README.md`, a file outside the published crate. The vendored copy points it at the crate's own README.

## Popup menus have one size

`gpui-component`'s `PopupMenu` has a `size` field and 20 px rows for `Size::Small`, but no setter, and every row sets `text_sm()` (14 px). A toolbar menu is then larger than the toolbar's labels. Custom element rows can set their own text, but lose their accessible names. triad draws its toolbar menus itself on gpui-base's `Popover` (`merge_app::split_menu`).

## Whole-text copy per frame

`gpui-component`'s `Input::render` (`src/input/input.rs`) calls `state.text(cx).to_string()` for the accessibility value when an accessibility client such as VoiceOver is active, or when `test-support` is on. At 1M lines this copies about 50 MB per frame, about 35 ms. Large files drop below the refresh rate for VoiceOver users, and headless measurements read slow. triad leaves it; PLAN.md §A6.7 decides from measurement.

## Soft wrap at start-up

Soft wrap is on by default in `EditorState` and wraps every line before the first frame:

| Lines | First frame, wrap on | Memory, wrap on | First frame, wrap off |
|---|---|---|---|
| 100k | 2.7 s | 570 MB | 202 ms |
| 1M | 26.6 s | 4.8 GB | 687 ms |

triad turns soft wrap off, and allows it only up to `SOFT_WRAP_MAX_LINES` (PLAN.md §A6.2).

## Line layout at start-up

With soft wrap off, `TextWrapper` still builds one `LineItem` per buffer line before the first frame. Upstream looks each line up with `Rope::get_line`, an O(log n) tree walk, and does so twice: when the element first sets the font, and in `prepare_if_need`. At 1M lines the two passes cost about 400 ms. triad's vendored `gpui-base` builds the lines in one pass, reads an unwrapped line by its length alone from a scan of the rope's chunks for `\n`, and rebuilds on a font or indent change only when the lines wrap and have been prepared ([PATCHES.md](../vendor/gpui-base/PATCHES.md)). At 1M lines the scan and tree take 21 ms of a 29 ms first frame. The frame after it waits about 45 ms at any size while macOS activates the new window; the main thread is idle in the run loop then.

## A tab can end as a sliver

`gpui-base` 0.8.1 moves the glyphs after each tab to the next stop. A tab that starts a hair short of a stop ends on it, a fraction of a pixel wide: shaped glyph positions and the stop width round differently. triad's vendored `gpui-base` lets a tab that starts less than 0.01 px short of a stop take the next one ([PATCHES.md](../vendor/gpui-base/PATCHES.md)).

## Scroll bounce

`gpui-base` 0.8.1 keeps rounding from stretching the scroll edge and throws the edge only when it is stretched. triad's tests of both stay in the vendored copy ([PATCHES.md](../vendor/gpui-base/PATCHES.md)). [scroll-debugging.md](scroll-debugging.md) gives the failure modes and how to reproduce them.

## Behaviour triad relies on

Facts about GPUI and the editor that are easy to trip over. Each was checked against `gpui-kit` with a test or a measurement.

- **Frames.** `cx.notify()` inside `render` schedules no new frame. Use `window.request_animation_frame()`.
- **Edits.** The editor reports every edit to its highlighter as an `InputEdit`: `start_byte`, `old_end_byte`, `new_end_byte` and row/column points. It also reports one empty edit on its first parse; ignore it (`merge-app` `recorder` drops empty edits). triad reads edits through the patch's `EditObserver`, so the highlighter slot stays free for syntax.
- **Range decorations follow edits** by Monaco's rules: an insertion at an edge does not grow the range. A decoration whose range becomes empty is dropped. The core keeps its own region list and never counts decorations.
- **Undo.** A binding for ⌘Z in the result's key context, registered after gpui-component's own, wins over the editor's `Undo`; the editor's undo never runs, and the core's history takes every undo (spec §4.4).
- **Blocking.** Running `triad.app/Contents/MacOS/triad-merge` directly blocks the caller until the window closes, which git and jj need. `open triad.app` does not.
- **No unsafe from macros.** GPUI and gpui-component macro output (`actions!`, element builders, `#[gpui_kit::test]`) compiles under `unsafe_code = "forbid"`.
- **Headless tests** run under nextest without a display, through gpui-kit's `test-support` feature.
- **Linux build packages:** `pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev libvulkan-dev libfontconfig-dev libfreetype-dev libasound2-dev libzstd-dev`.
- **gpui-component's `diff` module** is a read-only, virtualized unified-patch viewer with no three-way model and no editing. It shows how gpui-component renders diffs; it does not fit triad's panes.
- **Start-up stall at 1M lines.** A stall of about 195 ms near the start of a 1M-line run showed in early GPUI measurements and was never explained. If one shows in `script/measure-app`, profile the first frames before blaming triad's code.

## Accessibility gaps

- No live regions. GPUI's element API sets no AccessKit `Live` property, so a screen reader does not announce a change to the status bar; it reads the new text when the user moves to it.
- Range decorations are visual only. The editors expose their text, but not the region bands or gutter marks, so a screen reader cannot tell which result lines belong to a conflict except through the conflict markers in the text. The diff window's change bands are likewise invisible to it.
- `uniform_list` takes no role and builds only the rows on screen. triad wraps the merge window's rows in a table that gives the total row count, but a screen reader sees only the visible rows and cannot scroll the list itself.
- The test platform has no AccessKit adapter, so `Window::debug_a11y_tree_json` stays empty in headless tests. triad's tests read the role, label and toggled state that gpui-kit records for elements marked with `test_support()`.

## The load-time constructor

`gpui-pre-macos` runs `build_classes` as a `#[ctor]` at process load, in every binary that links GPUI. It touches `NSView`, whose `+initialize` reads user defaults. On macOS 27.2 it costs no visible time: `triad-merge --version` takes 7.0 ms (debug build, hyperfine, 20 runs).

## A tab-size change rebuilds every line

`gpui-base` 0.8.1's `TextWrapper::set_tab_size` rebuilds the line tree whenever the tab size changes, wrapped or not, and before the first layout. The wrapper starts at a tab size of 2 and triad's editors use 4, so upstream's first frame builds the tree for every line twice and frees one: at 1M lines about 125 MB of small allocations, and 91 MB more peak RSS. triad's vendored `gpui-base` rebuilds on a tab-size change only when the lines wrap and have been prepared, as for a font change ([PATCHES.md](../vendor/gpui-base/PATCHES.md)).

## Key bindings

`KeyBinding::new` takes a `&'static str` and does not resolve `secondary-`. triad builds the bindings from the settings with `KeyBinding::load` (`merge-app` `keys::binding`).

## 0.9 is not adopted

`gpui-kit` 0.9 pins `gpui-pre` 0.3.11. triad has not built or measured it.

## The C# and Swift grammars have no highlight query; TSX has part of one

gpui-component 0.8.1 registers `csharp` with `tree_sitter_c_sharp::LANGUAGE` and empty highlight, injection and locals queries (`src/highlighter/languages.rs`). Every C# editor parses its text and colours nothing. `tree-sitter-c-sharp` 0.23.5 ships `HIGHLIGHTS_QUERY` (MIT). triad registers `csharp` again with that query at start-up (`merge-app` `syntax::register_queries`). `swift` is the same: empty queries in gpui-component, `HIGHLIGHTS_QUERY` (MIT) in `tree-sitter-swift` 0.7.4, registered again at start-up.

`tsx` comes with `tree_sitter_typescript::HIGHLIGHTS_QUERY` alone: the 35 lines that `tree-sitter-typescript` layers over JavaScript's query. Comments, strings and most keywords colour nothing. triad registers `tsx` again at start-up with the highlights and injections gpui-component gives `typescript`.

The editor's highlighter builds one query from the injections, locals and highlights, and disables only the injection patterns (`build_for_language`). The locals patterns stay on and capture before the highlights, so a JavaScript or TypeScript identifier takes `@local.reference`, which the theme leaves unstyled, in place of `@variable`. triad's top panes run the same query, so the panes match the editors.

## Injection parses have a wall-clock budget

gpui-component 0.8.1 gives each injection parse 20 ms (`INJECTION_PARSE_TIMEOUT`, `src/highlighter/highlighter.rs`) and drops the layer on timeout. Rust's macro bodies are injections, so their styles depend on machine load. The tests that compare triad's chunked highlighter against `SyntaxHighlighter` fail 7 runs in 8 with the CPU saturated, each at a line inside a macro body. `.config/nextest.toml` runs them alone, which removes the load of other tests. Load from outside the suite can still fail them.

## Linux: a Wayland compositor without a seat

`gpui-pre-linux` 0.3.10 returns an error from the Wayland client when the compositor offers no `wl_seat`. triad has not run it against such a compositor. Weston's headless backend offers no seat. Desktop compositors offer one, so this shows only in headless test setups; `script/linux/smoke` uses sway.

## Linux: modifiers before a keymap panic

`gpui-pre-linux` 0.3.10 unwraps `keymap_state` on `wl_keyboard::Event::Modifiers` (`src/linux/wayland/client.rs:2124`). A virtual keyboard without a keymap, as `wtype -s` holds open, sends modifiers first. Desktop compositors send the keymap first, so this shows only in test setups.

## Linux: the fallback prompt

GPUI's `FallbackPromptRenderer` cuts off a detail wider than its card, draws a white card in every theme and takes no keys. `merge_app::prompt_view` replaces it through `App::set_prompt_builder`.

## macOS: a mixed toggle reads as on

`accesskit_macos` 0.26.3 gives `accessibilityValue` as `toggled != Toggled::False` (`src/node.rs:325`), so `Toggled::Mixed` reads as 1, the same as on. VoiceOver speaks a mixed toggle as selected. triad reports off for the pick targets of an unresolved conflict and names the state in the label.

## macOS: a menu item's disabled flag has no effect

`gpui-fast-macos` 0.1.12 sets `setEnabled(!disabled)` on each menu item (`src/platform.rs:429`), but the menus keep AppKit's autoenabling, so `validateMenuItem:` (`src/platform.rs:1523`) decides. It asks whether the focused window takes the item's action and ignores the flag. The title-bar menu on Linux and Windows honours the flag. triad leaves out an item that cannot run, as View → Blame without line origins (spec §7.3), and registers its action only while it can.

## The search panel selects its field on every key

`gpui-base` 0.8.1's `set_search_query` writes the session's `anchor_offset`: the top of the viewport when the query changes, `None` when it does not. `gpui-component` 0.8.1's overlay (`src/input/overlay.rs`, `InputOverlayHost::sync`) re-shows the search panel when the anchor changes, which selects the field. The panel calls `set_search_query` on every key and once more on re-show, so the anchor flips each key and each key replaces the one before. triad's patch to `vendor/gpui-base` leaves the anchor to `open_search` (`PATCHES.md`).
