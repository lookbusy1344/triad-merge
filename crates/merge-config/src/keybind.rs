//! Key bindings (`docs/PLAN.md` §E9): the `keybind` lines of the config file and the bindings
//! they leave. This module knows no GUI; `merge-app` turns the bindings into GPUI's.

use std::collections::BTreeSet;
use std::fmt;

use strum::{EnumMessage, IntoStaticStr, VariantArray};

use crate::named::Named;

use crate::settings::name;

/// The word that drops the defaults and every earlier line.
const CLEAR: &str = "clear";
/// The word in an action's place that removes a chord's binding.
const UNBIND: &str = "unbind";
/// Joins a chord's modifiers and key.
const CHORD_SEPARATOR: char = '+';
/// Joins a chord and its action. No action holds it, so the last one splits a line, and `=`
/// stays a key.
const ACTION_SEPARATOR: char = '=';
/// The chord of the `+` key, which holds the separator, after its modifiers.
const PLUS_AFTER_MODIFIERS: &str = "++";
const PLUS_KEY: &str = "+";
/// The names of the keys that print nothing (spec §6.3).
const NAMED_KEYS: &[&str] = &[
    "enter",
    "tab",
    "space",
    "backspace",
    "delete",
    "insert",
    "escape",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
];
const FUNCTION_KEY_PREFIX: char = 'f';
/// The function keys a chord names: `f1` to `f35` (spec §6.3).
const FUNCTION_KEYS: std::ops::RangeInclusive<u8> = 1..=35;

/// The default bindings in macOS's form; elsewhere `cmd` is `ctrl`. Where one action has two
/// chords, the menu shows the last.
const DEFAULT_BINDINGS: &[(&str, KeyAction)] = &[
    ("cmd+z", KeyAction::Undo),
    ("cmd+shift+z", KeyAction::Redo),
    ("cmd+down", KeyAction::NextDifference),
    ("cmd+up", KeyAction::PreviousDifference),
    ("cmd+alt+down", KeyAction::NextConflict),
    ("cmd+alt+up", KeyAction::PreviousConflict),
    ("cmd+]", KeyAction::NextConflict),
    ("cmd+[", KeyAction::PreviousConflict),
    ("cmd+1", KeyAction::ToggleLeft),
    ("cmd+2", KeyAction::ToggleBase),
    ("cmd+3", KeyAction::ToggleRight),
    ("cmd+0", KeyAction::ClearChoice),
    ("cmd+4", KeyAction::TakeLeftThenRight),
    ("cmd+5", KeyAction::TakeRightThenLeft),
    ("cmd+b", KeyAction::ToggleBasePane),
    ("cmd+i", KeyAction::ToggleOriginColumn),
    ("cmd+y", KeyAction::ToggleSyntax),
    ("cmd+r", KeyAction::ToggleSoftWrap),
    ("cmd+k", KeyAction::ToggleScrollSync),
    ("cmd+f", KeyAction::Find),
    ("cmd+l", KeyAction::GoToLine),
    ("cmd+s", KeyAction::SaveAndExit),
    ("cmd+s", KeyAction::Save),
    ("cmd+shift+s", KeyAction::SaveCopyAs),
    ("cmd+q", KeyAction::Cancel),
    ("cmd+w", KeyAction::Cancel),
    ("cmd+,", KeyAction::OpenSettings),
];

/// The platform whose modifiers a chord may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    /// Linux and Windows.
    Other,
}

impl Platform {
    /// The platform this build runs on.
    pub const CURRENT: Self = if cfg!(target_os = "macos") { Self::MacOs } else { Self::Other };
}

/// A chord's modifier, in the order a chord prints them.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, VariantArray, IntoStaticStr, EnumMessage,
)]
#[strum(serialize_all = "kebab-case")]
pub enum Modifier {
    Cmd,
    Super,
    Ctrl,
    Alt,
    Shift,
}

impl Modifier {
    fn exists_on(self, platform: Platform) -> bool {
        match self {
            Self::Cmd => platform == Platform::MacOs,
            Self::Super => platform == Platform::Other,
            Self::Ctrl | Self::Alt | Self::Shift => true,
        }
    }
}

/// A command a key can run: each item of the menus, by the kebab-case name of its action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum KeyAction {
    Undo,
    Redo,
    NextDifference,
    PreviousDifference,
    NextConflict,
    PreviousConflict,
    ToggleLeft,
    ToggleBase,
    ToggleRight,
    ClearChoice,
    TakeBase,
    TakeLeft,
    TakeRight,
    TakeLeftThenRight,
    TakeRightThenLeft,
    TakeLeftAllUnresolved,
    TakeRightAllUnresolved,
    TakeWholeLeft,
    TakeWholeRight,
    TakeSortedUnion,
    CarryMovedEdit,
    ToggleBasePane,
    ToggleOriginColumn,
    ToggleSyntax,
    ToggleSoftWrap,
    ToggleScrollSync,
    Find,
    GoToLine,
    SaveAndExit,
    SaveCopyAs,
    Save,
    Cancel,
    OpenSettings,
    About,
}

impl KeyAction {
    /// Whether the merge window has this command.
    #[must_use]
    pub const fn in_merge_window(self) -> bool {
        !matches!(self, Self::Save)
    }

    /// Whether the diff window has this command.
    #[must_use]
    pub const fn in_diff_window(self) -> bool {
        matches!(
            self,
            Self::Undo
                | Self::Redo
                | Self::NextDifference
                | Self::PreviousDifference
                | Self::Find
                | Self::ToggleOriginColumn
                | Self::ToggleSyntax
                | Self::ToggleSoftWrap
                | Self::Save
                | Self::Cancel
                | Self::OpenSettings
                | Self::About
        )
    }

    /// Whether a window has both commands, so one chord cannot run both.
    const fn clashes_with(self, other: Self) -> bool {
        (self.in_merge_window() && other.in_merge_window())
            || (self.in_diff_window() && other.in_diff_window())
    }
}

/// Modifiers and a key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Chord {
    modifiers: BTreeSet<Modifier>,
    key: Box<str>,
}

impl Chord {
    /// Read a chord as the file writes it, such as `cmd+shift+j`.
    ///
    /// # Errors
    ///
    /// [`KeybindError::Chord`] for a chord of the wrong form, a modifier named twice or a key
    /// spec §6.3 does not name; [`KeybindError::Modifier`] for a modifier `platform` lacks.
    pub fn parse(text: &str, platform: Platform) -> Result<Self, KeybindError> {
        let malformed = || KeybindError::Chord(text.to_owned());
        let (names, key) = match text.strip_suffix(PLUS_AFTER_MODIFIERS) {
            Some(names) => (Some(names), PLUS_KEY),
            None if text == PLUS_KEY => (None, PLUS_KEY),
            None => text
                .rsplit_once(CHORD_SEPARATOR)
                .map_or((None, text), |(names, key)| (Some(names), key)),
        };
        if !is_key(key) {
            return Err(malformed());
        }
        let mut modifiers = BTreeSet::new();
        for modifier_name in names.into_iter().flat_map(|names| names.split(CHORD_SEPARATOR)) {
            let modifier = Modifier::from_name(modifier_name).ok_or_else(malformed)?;
            if !modifier.exists_on(platform) {
                return Err(KeybindError::Modifier {
                    modifier: modifier_name.to_owned(),
                    chord: text.to_owned(),
                });
            }
            if !modifiers.insert(modifier) {
                return Err(malformed());
            }
        }
        Ok(Self { modifiers, key: key.into() })
    }

    /// The chord's modifiers, in a fixed order: cmd, super, ctrl, alt, shift.
    pub fn modifiers(&self) -> impl Iterator<Item = Modifier> + '_ {
        self.modifiers.iter().copied()
    }

    /// The chord's key: one printing character, or a name of spec §6.3.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// This chord of the default table, on `platform`.
    fn ported(mut self, platform: Platform) -> Self {
        if platform == Platform::Other && self.modifiers.remove(&Modifier::Cmd) {
            self.modifiers.insert(Modifier::Ctrl);
        }
        self
    }
}

/// Whether a chord may name this key: one printing character, or a named key (spec §6.3).
fn is_key(key: &str) -> bool {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(only), None) => !only.is_whitespace() && !only.is_uppercase(),
        _ => {
            NAMED_KEYS.contains(&key)
                || FUNCTION_KEYS
                    .clone()
                    .any(|number| key == format!("{FUNCTION_KEY_PREFIX}{number}"))
        }
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in &self.modifiers {
            write!(f, "{}{CHORD_SEPARATOR}", name(*modifier))?;
        }
        f.write_str(&self.key)
    }
}

/// A chord and the command it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub chord: Chord,
    pub action: KeyAction,
}

/// What one `keybind` line says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keybind {
    Bind(Binding),
    Unbind(Chord),
    Clear,
}

impl Keybind {
    /// Read the value of a `keybind` line.
    ///
    /// # Errors
    ///
    /// [`KeybindError`] for a line with no action, an unknown action or a bad chord.
    pub fn parse(value: &str, platform: Platform) -> Result<Self, KeybindError> {
        if value == CLEAR {
            return Ok(Self::Clear);
        }
        let (chord, action) = value
            .rsplit_once(ACTION_SEPARATOR)
            .ok_or_else(|| KeybindError::Form(value.to_owned()))?;
        let chord = Chord::parse(chord, platform)?;
        if action == UNBIND {
            return Ok(Self::Unbind(chord));
        }
        let action =
            KeyAction::from_name(action).ok_or_else(|| KeybindError::Action(action.to_owned()))?;
        Ok(Self::Bind(Binding { chord, action }))
    }
}

/// Why a `keybind` line does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeybindError {
    #[error("keybind = {0}: expected CHORD=ACTION, CHORD=unbind or clear")]
    Form(String),
    #[error("keybind: unknown action `{0}`")]
    Action(String),
    #[error("keybind: malformed chord `{0}`")]
    Chord(String),
    #[error("keybind: this platform has no `{modifier}` key, in `{chord}`")]
    Modifier { modifier: String, chord: String },
}

/// The bindings in effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    bound: Vec<Binding>,
    /// Chords that do nothing, which also keeps the editors' own bindings for them from
    /// running.
    unbound: BTreeSet<Chord>,
}

impl KeyBindings {
    /// The default bindings on `platform`.
    ///
    /// # Panics
    ///
    /// Never: the default table holds valid chords, which a test reads.
    #[must_use]
    pub fn for_platform(platform: Platform) -> Self {
        let bound = DEFAULT_BINDINGS
            .iter()
            .map(|&(chord, action)| Binding {
                chord: Chord::parse(chord, Platform::MacOs)
                    .expect("the default table holds valid macOS chords")
                    .ported(platform),
                action,
            })
            .collect();
        Self { bound, unbound: BTreeSet::new() }
    }

    /// Apply one line: a later binding for a chord replaces an earlier one in each window
    /// that has both commands.
    pub fn apply(&mut self, line: Keybind) {
        match line {
            Keybind::Bind(binding) => {
                self.unbound.remove(&binding.chord);
                self.bound.retain(|old| {
                    old.chord != binding.chord || !old.action.clashes_with(binding.action)
                });
                self.bound.push(binding);
            }
            Keybind::Unbind(chord) => {
                self.bound.retain(|old| old.chord != chord);
                self.unbound.insert(chord);
            }
            Keybind::Clear => {
                let dropped = std::mem::take(&mut self.bound);
                self.unbound.extend(dropped.into_iter().map(|old| old.chord));
            }
        }
    }

    /// The bindings, oldest first.
    #[must_use]
    pub fn bound(&self) -> &[Binding] {
        &self.bound
    }

    /// The chords that do nothing.
    pub fn unbound(&self) -> impl Iterator<Item = &Chord> {
        self.unbound.iter()
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::for_platform(Platform::CURRENT)
    }
}

/// The `keybind` values that give these bindings, one per line.
impl fmt::Display for KeyBindings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(CLEAR)?;
        for Binding { chord, action } in &self.bound {
            write!(f, "\n{chord}{ACTION_SEPARATOR}{}", name(*action))?;
        }
        for chord in &self.unbound {
            write!(f, "\n{chord}{ACTION_SEPARATOR}{UNBIND}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Platform::{MacOs, Other};

    fn chord(text: &str) -> Chord {
        Chord::parse(text, MacOs).unwrap()
    }

    fn line(text: &str) -> Keybind {
        Keybind::parse(text, MacOs).unwrap()
    }

    fn apply(lines: &[&str]) -> KeyBindings {
        let mut bindings = KeyBindings::for_platform(MacOs);
        for text in lines {
            bindings.apply(line(text));
        }
        bindings
    }

    /// The actions `text` runs, in binding order.
    fn actions_of(bindings: &KeyBindings, text: &str) -> Vec<KeyAction> {
        let wanted = chord(text);
        bindings
            .bound()
            .iter()
            .filter(|binding| binding.chord == wanted)
            .map(|binding| binding.action)
            .collect()
    }

    fn bad_chord(text: &str) -> KeybindError {
        KeybindError::Chord(text.to_owned())
    }

    #[test]
    fn a_chord_prints_its_modifiers_in_one_order() {
        assert_eq!(chord("shift+cmd+j").to_string(), "cmd+shift+j");
        assert_eq!(chord("shift+alt+ctrl+cmd+j").to_string(), "cmd+ctrl+alt+shift+j");
        assert_eq!(chord("f5").to_string(), "f5");
    }

    #[test]
    fn a_chord_gives_its_modifiers_in_order_and_its_key() {
        let parsed = chord("shift+alt+cmd+down");
        assert_eq!(
            parsed.modifiers().collect::<Vec<_>>(),
            [Modifier::Cmd, Modifier::Alt, Modifier::Shift]
        );
        assert_eq!(parsed.key(), "down");
        assert_eq!(chord("cmd++").key(), "+");
    }

    #[test]
    fn keys_that_look_like_separators_read() {
        assert_eq!(chord("cmd++").to_string(), "cmd++");
        assert_eq!(chord("+").to_string(), "+");
        assert_eq!(chord("cmd+=").to_string(), "cmd+=");
        assert_eq!(chord("cmd+,").to_string(), "cmd+,");
    }

    #[test]
    fn every_named_key_and_function_key_bound_reads() {
        for key in NAMED_KEYS {
            assert_eq!(chord(key).to_string(), *key);
        }
        assert_eq!(chord("f1").to_string(), "f1");
        assert_eq!(chord("f35").to_string(), "f35");
    }

    #[test]
    fn malformed_chords_are_refused() {
        for text in [
            "",
            "cmd+",
            "+j",
            "cmd++j",
            "cmd+cmd+j",
            "cmd+shift",
            "hyper+j",
            "cmd+nonsense",
            "cmd+J",
            "CMD+j",
            "cmd+Down",
            "f0",
            "f36",
            "cmd+ ",
            "cmd+f1x",
        ] {
            assert_eq!(Chord::parse(text, MacOs), Err(bad_chord(text)), "{text:?}");
        }
    }

    #[test]
    fn a_modifier_the_platform_lacks_is_refused() {
        assert_eq!(
            Chord::parse("super+j", MacOs),
            Err(KeybindError::Modifier {
                modifier: "super".to_owned(),
                chord: "super+j".to_owned(),
            })
        );
        assert_eq!(
            Chord::parse("cmd+j", Other),
            Err(KeybindError::Modifier { modifier: "cmd".to_owned(), chord: "cmd+j".to_owned() })
        );
    }

    #[test]
    fn a_line_reads_as_a_binding_an_unbind_or_a_clear() {
        assert_eq!(
            line("cmd+j=next-conflict"),
            Keybind::Bind(Binding { chord: chord("cmd+j"), action: KeyAction::NextConflict })
        );
        assert_eq!(line("cmd+k=unbind"), Keybind::Unbind(chord("cmd+k")));
        assert_eq!(line("clear"), Keybind::Clear);
    }

    #[test]
    fn the_equals_key_binds_because_the_last_equals_splits_the_line() {
        assert_eq!(
            line("cmd+==take-left"),
            Keybind::Bind(Binding { chord: chord("cmd+="), action: KeyAction::TakeLeft })
        );
    }

    #[test]
    fn bad_lines_are_refused_with_their_cause() {
        let parse = |text| Keybind::parse(text, MacOs);
        assert_eq!(parse("cmd+j=nonsense"), Err(KeybindError::Action("nonsense".to_owned())));
        assert_eq!(parse("cmd+=next-conflict"), Err(bad_chord("cmd+")));
        assert_eq!(parse("=next-conflict"), Err(bad_chord("")));
        assert_eq!(parse("cmd+j"), Err(KeybindError::Form("cmd+j".to_owned())));
        assert_eq!(parse("cmd+j="), Err(KeybindError::Action(String::new())));
        assert_eq!(parse("Clear"), Err(KeybindError::Form("Clear".to_owned())));
    }

    #[test]
    fn every_action_has_the_name_of_its_menu_command() {
        for (name, action) in [
            ("take-left", KeyAction::TakeLeft),
            ("take-base", KeyAction::TakeBase),
            ("next-conflict", KeyAction::NextConflict),
            ("save-and-exit", KeyAction::SaveAndExit),
            ("take-left-then-right", KeyAction::TakeLeftThenRight),
            ("toggle-origin-column", KeyAction::ToggleOriginColumn),
            ("toggle-soft-wrap", KeyAction::ToggleSoftWrap),
        ] {
            assert_eq!(KeyAction::from_name(name), Some(action));
        }
    }

    #[test]
    fn the_defaults_are_spec_6_3s_keys() {
        let bindings = KeyBindings::for_platform(MacOs);
        assert_eq!(actions_of(&bindings, "cmd+z"), [KeyAction::Undo]);
        assert_eq!(actions_of(&bindings, "cmd+alt+up"), [KeyAction::PreviousConflict]);
        assert_eq!(actions_of(&bindings, "cmd+]"), [KeyAction::NextConflict]);
        assert_eq!(actions_of(&bindings, "cmd+["), [KeyAction::PreviousConflict]);
        assert_eq!(actions_of(&bindings, "cmd+down"), [KeyAction::NextDifference]);
        assert_eq!(actions_of(&bindings, "cmd+r"), [KeyAction::ToggleSoftWrap]);
        assert_eq!(actions_of(&bindings, "cmd+y"), [KeyAction::ToggleSyntax]);
        assert_eq!(actions_of(&bindings, "cmd+i"), [KeyAction::ToggleOriginColumn]);
        assert_eq!(actions_of(&bindings, "cmd+k"), [KeyAction::ToggleScrollSync]);
        assert_eq!(actions_of(&bindings, "cmd+q"), [KeyAction::Cancel]);
        assert_eq!(actions_of(&bindings, "cmd+w"), [KeyAction::Cancel]);
        assert_eq!(actions_of(&bindings, "cmd+s"), [KeyAction::SaveAndExit, KeyAction::Save]);
        assert_eq!(bindings.bound().len(), 27);
        assert_eq!(bindings.unbound().count(), 0);
    }

    #[test]
    fn the_defaults_use_ctrl_off_macos() {
        let bindings = KeyBindings::for_platform(Other);
        let first = &bindings.bound()[0];
        assert_eq!(first.chord.to_string(), "ctrl+z");
        assert_eq!(first.action, KeyAction::Undo);
        assert!(bindings.bound().iter().all(|binding| !binding.chord.to_string().contains("cmd")));
    }

    #[test]
    fn a_binding_adds_to_the_chords_an_action_has() {
        let bindings = apply(&["cmd+j=next-conflict"]);
        assert_eq!(actions_of(&bindings, "cmd+j"), [KeyAction::NextConflict]);
        assert_eq!(actions_of(&bindings, "cmd+alt+down"), [KeyAction::NextConflict]);
    }

    #[test]
    fn a_later_line_for_a_chord_wins() {
        let bindings = apply(&["cmd+j=next-conflict", "cmd+j=take-left"]);
        assert_eq!(actions_of(&bindings, "cmd+j"), [KeyAction::TakeLeft]);
    }

    #[test]
    fn a_binding_replaces_a_default_chord() {
        let bindings = apply(&["cmd+z=next-conflict"]);
        assert_eq!(actions_of(&bindings, "cmd+z"), [KeyAction::NextConflict]);
    }

    #[test]
    fn the_merge_window_alone_toggles_scroll_sync_by_name() {
        assert_eq!(KeyAction::from_name("toggle-scroll-sync"), Some(KeyAction::ToggleScrollSync));
        assert!(KeyAction::ToggleScrollSync.in_merge_window());
        // The diff panes always scroll together, so their spacer rows face each other.
        assert!(!KeyAction::ToggleScrollSync.in_diff_window());
        assert_eq!(
            actions_of(&KeyBindings::for_platform(MacOs), "cmd+k"),
            [KeyAction::ToggleScrollSync]
        );
    }

    #[test]
    fn both_windows_toggle_soft_wrap() {
        assert!(KeyAction::ToggleSoftWrap.in_merge_window());
        assert!(KeyAction::ToggleSoftWrap.in_diff_window());
    }

    #[test]
    fn a_merge_only_command_leaves_the_diff_windows_binding_of_the_chord() {
        let bindings = apply(&["cmd+s=take-left"]);
        assert_eq!(actions_of(&bindings, "cmd+s"), [KeyAction::Save, KeyAction::TakeLeft]);
    }

    #[test]
    fn a_diff_only_command_leaves_the_merge_windows_binding_of_the_chord() {
        let bindings = apply(&["cmd+s=save"]);
        assert_eq!(actions_of(&bindings, "cmd+s"), [KeyAction::SaveAndExit, KeyAction::Save]);
    }

    #[test]
    fn a_command_of_both_windows_replaces_the_chord_in_both() {
        let bindings = apply(&["cmd+s=cancel"]);
        assert_eq!(actions_of(&bindings, "cmd+s"), [KeyAction::Cancel]);
    }

    #[test]
    fn unbind_removes_every_binding_of_the_chord_and_records_it() {
        let bindings = apply(&["cmd+s=unbind", "cmd+k=unbind"]);
        assert_eq!(actions_of(&bindings, "cmd+s"), []);
        assert_eq!(bindings.unbound().collect::<Vec<_>>(), [&chord("cmd+k"), &chord("cmd+s")]);
    }

    #[test]
    fn a_binding_after_an_unbind_takes_the_chord_back() {
        let bindings = apply(&["cmd+k=unbind", "cmd+k=take-left"]);
        assert_eq!(actions_of(&bindings, "cmd+k"), [KeyAction::TakeLeft]);
        assert_eq!(bindings.unbound().count(), 0);
    }

    #[test]
    fn clear_then_one_line_leaves_one_binding() {
        let bindings = apply(&["cmd+j=next-conflict", "clear", "cmd+k=take-left"]);
        assert_eq!(
            bindings.bound(),
            [Binding { chord: chord("cmd+k"), action: KeyAction::TakeLeft }]
        );
    }

    #[test]
    fn clear_leaves_the_dropped_chords_doing_nothing() {
        let bindings = apply(&["clear"]);
        assert!(bindings.unbound().any(|unbound| *unbound == chord("cmd+z")));
        assert!(bindings.unbound().any(|unbound| *unbound == chord("cmd+s")));
    }

    #[test]
    fn the_text_starts_with_clear_and_lists_bindings_then_unbinds() {
        let mut bindings = apply(&["clear", "cmd+j=next-conflict", "cmd+k=unbind"]);
        bindings.unbound.retain(|unbound| *unbound == chord("cmd+k"));
        assert_eq!(bindings.to_string(), "clear\ncmd+j=next-conflict\ncmd+k=unbind");
    }

    #[test]
    fn the_text_reads_back_as_the_same_bindings() {
        for lines in [
            &[][..],
            &["cmd+j=next-conflict"],
            &["cmd+s=take-left", "cmd+z=unbind", "cmd+k=unbind"],
            &["clear", "cmd+j=next-conflict"],
        ] {
            let bindings = apply(lines);
            let text = bindings.to_string();
            let reread = apply(&text.lines().collect::<Vec<_>>());
            assert_eq!(reread, bindings, "{lines:?}");
        }
    }
}
