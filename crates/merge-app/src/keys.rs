//! The windows' key bindings (spec §6.3): `merge-cli`'s [`KeyBindings`] as GPUI's.

use gpui_kit::base::input::{AddCursorAbove, AddCursorBelow};
use std::rc::Rc;

use gpui_kit::{Action, App, KeyBinding, KeyBindingContextPredicate, NoAction};
use merge_config::keybind::{Chord, KeyAction, KeyBindings, Modifier, Windows};

use crate::input_select::INPUT_PANES_CONTEXT;
use crate::{CopyInput, SelectAllInput, diff_window, window};

/// The editor's own key context.
const EDITOR_CONTEXT: &str = "Input";

/// The key contexts a command binds in. Each window's command binds for the window and again
/// for its editor, whose own bindings for these keys would win: ⌘↑/↓ move to the start and
/// end, ⌘[/] indent, ⌥⌘↑/↓ add cursors.
struct Contexts {
    result: String,
    pane: String,
}

impl Contexts {
    fn new() -> Self {
        Self {
            result: format!("{} > {EDITOR_CONTEXT}", window::RESULT_CONTEXT),
            pane: format!("{} > {EDITOR_CONTEXT}", diff_window::PANE_CONTEXT),
        }
    }

    /// The contexts `action` binds in, given the `windows` its chord runs it in.
    fn of(&self, action: KeyAction, windows: Windows) -> Vec<Option<&str>> {
        match action {
            KeyAction::OpenSettings | KeyAction::About => vec![None],
            // Inside an editor, the editor's own ⌘F opens its search.
            KeyAction::Find => vec![Some(window::WINDOW_CONTEXT), Some(diff_window::DIFF_CONTEXT)],
            _ => {
                let merge = [window::WINDOW_CONTEXT, self.result.as_str()];
                let diff = [diff_window::DIFF_CONTEXT, self.pane.as_str()];
                [(windows.merge, merge), (windows.diff, diff)]
                    .into_iter()
                    .filter(|(runs, _)| *runs)
                    .flat_map(|(_, contexts)| contexts.map(Some))
                    .collect()
            }
        }
    }
}

/// The top panes' copy and select all: ⌘ on macOS, Ctrl elsewhere.
const COPY_KEY: &str = "secondary-c";
const SELECT_ALL_KEY: &str = "secondary-a";

/// Joins the parts of a GPUI keystroke.
const KEYSTROKE_SEPARATOR: &str = "-";

/// `chord` as GPUI writes a keystroke, such as `cmd-shift-j`. The key names of spec §6.3 are
/// GPUI's.
fn keystroke(chord: &Chord) -> String {
    let modifiers = chord.modifiers().map(|modifier| match modifier {
        Modifier::Cmd => "cmd",
        Modifier::Super => "super",
        Modifier::Ctrl => "ctrl",
        Modifier::Alt => "alt",
        Modifier::Shift => "shift",
    });
    modifiers.chain([chord.key()]).collect::<Vec<_>>().join(KEYSTROKE_SEPARATOR)
}

/// Register `bindings`. ⌥⇧↑/↓ add cursors and ⌘C and ⌘A copy and select in the top panes;
/// no setting names them, so a `keybind` line for those chords wins. The menus show a command's first binding, so the newest goes first.
pub(crate) fn bind(bindings: &KeyBindings, cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("shift-alt-down", AddCursorBelow, Some(EDITOR_CONTEXT)),
        KeyBinding::new("shift-alt-up", AddCursorAbove, Some(EDITOR_CONTEXT)),
    ]);
    cx.bind_keys([
        binding(COPY_KEY, Box::new(CopyInput), Some(INPUT_PANES_CONTEXT), cx),
        binding(SELECT_ALL_KEY, Box::new(SelectAllInput), Some(INPUT_PANES_CONTEXT), cx),
    ]);
    let contexts = Contexts::new();
    let effective: Vec<_> = bindings.effective().collect();
    let bound: Vec<_> = effective
        .into_iter()
        .rev()
        .flat_map(|(bound, windows)| {
            let chord = keystroke(&bound.chord);
            contexts
                .of(bound.action, windows)
                .into_iter()
                .map(|context| binding(&chord, crate::action(bound.action), context, cx))
                .collect::<Vec<_>>()
        })
        .collect();
    cx.bind_keys(bound);
    let unbound: Vec<_> = bindings
        .unbound()
        .map(|chord| binding(&keystroke(chord), Box::new(NoAction {}), None, cx))
        .collect();
    cx.bind_keys(unbound);
}

/// A binding of `keystroke` to `action` in `context`. `KeyBinding::new` takes only static
/// keystrokes; these come from the settings.
fn binding(
    keystroke: &str,
    action: Box<dyn Action>,
    context: Option<&str>,
    cx: &App,
) -> KeyBinding {
    let predicate = context.map(|context| {
        Rc::new(
            KeyBindingContextPredicate::parse(context)
                .expect("the key contexts are triad's constants, which parse"),
        )
    });
    KeyBinding::load(keystroke, action, predicate, false, None, cx)
        .expect("a parsed chord writes a keystroke GPUI parses: spec §6.3 uses GPUI's key names")
}

#[cfg(test)]
mod tests {
    use merge_config::keybind::Platform;

    use super::*;

    fn chord(text: &str, platform: Platform) -> Chord {
        Chord::parse(text, platform).expect("a chord")
    }

    #[test]
    fn a_chord_gives_gpuis_keystroke() {
        let mac = Platform::MacOs;
        assert_eq!(keystroke(&chord("cmd+shift+j", mac)), "cmd-shift-j");
        assert_eq!(keystroke(&chord("cmd+alt+down", mac)), "cmd-alt-down");
        assert_eq!(keystroke(&chord("f5", mac)), "f5");
        assert_eq!(keystroke(&chord("cmd+-", mac)), "cmd--");
        assert_eq!(keystroke(&chord("ctrl+cmd++", mac)), "cmd-ctrl-+");
        assert_eq!(keystroke(&chord("super+j", Platform::Other)), "super-j");
    }
}
