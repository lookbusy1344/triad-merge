//! Headless tests of the `keybind` setting in the windows (`docs/PLAN.md` §E9).

use crate::common;

use common::{Finished, act, caret, doc, set_caret, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Action, AnyWindowHandle, Entity, OwnedMenuItem, TestAppContext};
use merge_app::{
    Cancel, Find, MergeWindow, NextConflict, Save, SaveAndExit, TakeLeft, ToggleLeft,
    WindowSettings,
};
use merge_config::keybind::{KeyBindings, Keybind, Platform};
use merge_core::document::MergeDocument;

/// The modifier of the default chords.
const SECONDARY: &str = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };
/// The number of commands the menus hold in a window without origins.
const MENU_COMMANDS: usize = 33;
/// The command of View → Blame, which the menus hold only once origins load (spec §7.3).
const BLAME_COMMAND: &str = "toggle-origin-column";
const ACTION_NAMESPACE: &str = "triad::";
const NO_CHORDS: [String; 0] = [];
const BASE: &str = "a\nb\nc\n";
const LEFT: &str = "a\nL\nc\n";
const RIGHT: &str = "a\nR\nc\n";

/// One conflict at line 2.
fn conflict() -> MergeDocument {
    doc(BASE, LEFT, RIGHT)
}

fn bindings(lines: &[String]) -> KeyBindings {
    let mut bindings = KeyBindings::default();
    for line in lines {
        bindings.apply(Keybind::parse(line, Platform::CURRENT).expect("the test's lines read"));
    }
    bindings
}

struct Opened {
    window: AnyWindowHandle,
    view: Entity<MergeWindow>,
    _finished: Finished,
}

impl Opened {
    fn new(cx: &mut TestAppContext, lines: &[String]) -> Self {
        let bindings = bindings(lines);
        let (window, view, finished) = cx.update(|cx| {
            gpui_kit::init(cx);
            merge_app::init_with(&bindings, cx);
            let finished = Finished::install(cx);
            let (window, view) =
                merge_app::open(conflict(), &WindowSettings::default(), cx).expect("window opens");
            (window, view, finished)
        });
        act(cx, window, TestWindowExt::render_frame);
        Self { window, view, _finished: finished }
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        act(cx, self.window, |window, cx| window.press(key, cx));
        cx.run_until_parked();
    }

    fn output(&self, cx: &mut TestAppContext) -> String {
        texts(cx, &self.view).0
    }
}

/// The chords of `action`, in the order the menus look through them.
fn chords(cx: &mut TestAppContext, action: &dyn Action) -> Vec<String> {
    cx.update(|cx| {
        let os = cx.platform_info().os;
        cx.key_bindings()
            .borrow()
            .bindings_for_action(action)
            .flat_map(|binding| binding.keystrokes().iter().map(move |key| key.unparse(os)))
            .collect()
    })
}

/// `triad::TakeLeft` as `take-left`.
fn kebab(action: &str) -> String {
    let name = action.strip_prefix(ACTION_NAMESPACE).expect("a triad action");
    name.chars().enumerate().fold(String::new(), |mut text, (i, c)| {
        if c.is_uppercase() && i > 0 {
            text.push('-');
        }
        text.push(c.to_ascii_lowercase());
        text
    })
}

#[gpui_kit::test]
fn a_binding_replaces_a_default_chord(cx: &mut TestAppContext) {
    let opened = Opened::new(cx, &[format!("{SECONDARY}+3=toggle-left")]);
    opened.press(cx, &format!("{SECONDARY}-alt-down"));
    opened.press(cx, &format!("{SECONDARY}-3"));
    assert_eq!(opened.output(cx), LEFT);
}

#[gpui_kit::test]
fn a_new_chord_runs_its_command(cx: &mut TestAppContext) {
    let lines = ["ctrl+j=next-conflict".to_owned(), "ctrl+k=toggle-right".to_owned()];
    let opened = Opened::new(cx, &lines);
    opened.press(cx, "ctrl-j");
    opened.press(cx, "ctrl-k");
    assert_eq!(opened.output(cx), RIGHT);
}

#[gpui_kit::test]
fn a_new_chord_leaves_the_commands_default_chord_working(cx: &mut TestAppContext) {
    let opened = Opened::new(cx, &["ctrl+j=next-conflict".to_owned()]);
    opened.press(cx, &format!("{SECONDARY}-alt-down"));
    opened.press(cx, &format!("{SECONDARY}-3"));
    assert_eq!(opened.output(cx), RIGHT);
}

#[gpui_kit::test]
fn unbind_leaves_the_chord_doing_nothing(cx: &mut TestAppContext) {
    let opened = Opened::new(cx, &[format!("{SECONDARY}+z=unbind")]);
    opened.press(cx, &format!("{SECONDARY}-alt-down"));
    opened.press(cx, &format!("{SECONDARY}-3"));
    opened.press(cx, &format!("{SECONDARY}-z"));
    assert_eq!(opened.output(cx), RIGHT);
}

#[gpui_kit::test]
fn unbind_also_stops_the_editors_own_binding_of_the_chord(cx: &mut TestAppContext) {
    let opened = Opened::new(cx, &[format!("{SECONDARY}+up=unbind")]);
    let end = LEFT.len();
    set_caret(cx, &opened.view, end);
    opened.press(cx, &format!("{SECONDARY}-up"));
    assert_eq!(caret(cx, &opened.view), end);
}

#[gpui_kit::test]
fn clear_leaves_only_the_lines_after_it(cx: &mut TestAppContext) {
    let lines =
        ["clear".to_owned(), "ctrl+j=next-conflict".to_owned(), "ctrl+k=toggle-right".to_owned()];
    let opened = Opened::new(cx, &lines);
    opened.press(cx, "ctrl-j");
    let before = opened.output(cx);
    opened.press(cx, &format!("{SECONDARY}-1"));
    assert_eq!(opened.output(cx), before);
    assert_eq!(chords(cx, &ToggleLeft), NO_CHORDS);
    opened.press(cx, "ctrl-k");
    assert_eq!(opened.output(cx), RIGHT);
}

#[gpui_kit::test]
fn a_merge_only_command_leaves_the_diff_windows_save_on_the_chord(cx: &mut TestAppContext) {
    let _opened = Opened::new(cx, &[format!("{SECONDARY}+s=take-left")]);
    let chord = format!("{SECONDARY}-s");
    assert_eq!(chords(cx, &SaveAndExit), NO_CHORDS);
    assert_eq!(chords(cx, &Save), [chord.clone(), chord.clone()]);
    assert_eq!(chords(cx, &TakeLeft), [chord.clone(), chord]);
}

#[gpui_kit::test]
fn the_menus_show_the_latest_chord_of_a_command(cx: &mut TestAppContext) {
    let _opened = Opened::new(cx, &["ctrl+j=next-conflict".to_owned()]);
    assert_eq!(chords(cx, &NextConflict)[0], "ctrl-j");
}

#[gpui_kit::test]
fn the_menus_show_the_same_default_chord_as_spec_6_3(cx: &mut TestAppContext) {
    let _opened = Opened::new(cx, &[]);
    assert_eq!(chords(cx, &Cancel)[0], format!("{SECONDARY}-w"));
}

#[gpui_kit::test]
fn every_action_in_the_menus_has_a_name(cx: &mut TestAppContext) {
    let _opened = Opened::new(cx, &[]);
    let menus = cx.update(|cx| cx.get_menus()).expect("the test platform keeps menus");
    let names: Vec<String> = menus
        .iter()
        .flat_map(|menu| &menu.items)
        .filter_map(|item| match item {
            OwnedMenuItem::Action { action, .. } => Some(kebab(action.name())),
            _ => None,
        })
        .collect();
    assert_eq!(names.len(), MENU_COMMANDS);
    assert!(!names.iter().any(|name| name == BLAME_COMMAND));
    for name in names.iter().map(String::as_str).chain([BLAME_COMMAND]) {
        let line = format!("f5={name}");
        assert!(matches!(Keybind::parse(&line, Platform::CURRENT), Ok(Keybind::Bind(_))), "{name}");
    }
}

#[gpui_kit::test]
fn find_binds_in_both_windows(cx: &mut TestAppContext) {
    let _opened = Opened::new(cx, &[]);
    let chord = format!("{SECONDARY}-f");
    assert_eq!(chords(cx, &Find), [chord.clone(), chord]);
}
