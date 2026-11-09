//! Headless tests of the open dialog (spec §6.1; PLAN.md §A6.2).

use crate::common;

use std::path::{Path, PathBuf};

use common::act;
use gpui_kit::base::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyView, AnyWindowHandle, App, Entity, TestAppContext, VisualTestContext};
use merge_app::diff_window::DiffWindow;
use merge_app::open_dialog::{Mode, OpenDialog, START_ID, Slot};
use merge_app::{ExitCode, MergeWindow, WindowSettings};
use merge_core::diff2::Side;
use tempfile::TempDir;

/// Diff windows the dialog opens leave the disk alone: tests call `disk_changed` instead.
const NO_WATCH: bool = false;

/// Base, left and right of a merge with no conflict, whose result is `A b C`.
fn inputs() -> TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, text) in
        [("base.txt", "a\nb\nc\n"), ("left.txt", "A\nb\nc\n"), ("right.txt", "a\nb\nC\n")]
    {
        std::fs::write(dir.path().join(name), text).expect("the input is written");
    }
    dir
}

fn opened(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<OpenDialog>) {
    let opened = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::open_dialog::open(WindowSettings::default(), NO_WATCH, cx)
            .expect("the dialog opens")
    });
    act(cx, opened.0, TestWindowExt::render_frame);
    opened
}

fn fill(cx: &mut TestAppContext, window: AnyWindowHandle, slot: Slot, path: &Path) {
    act(cx, window, |window, cx| {
        window.click(slot.id(), cx);
        window.input(&path.to_string_lossy(), cx);
    });
}

fn fill_all(cx: &mut TestAppContext, window: AnyWindowHandle, dir: &Path) {
    for (slot, name) in [
        (Slot::Base, "base.txt"),
        (Slot::Left, "left.txt"),
        (Slot::Right, "right.txt"),
        (Slot::Output, "out.txt"),
    ] {
        fill(cx, window, slot, &dir.join(name));
    }
}

/// The content view of each open window.
fn views(cx: &App) -> Vec<AnyView> {
    cx.windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Root>())
        .map(|root| root.read(cx).expect("the window is open").view().clone())
        .collect()
}

/// The merge windows open now, with each one's result text and output path.
fn merges(cx: &mut TestAppContext) -> Vec<(String, Option<PathBuf>)> {
    cx.update(|cx| {
        views(cx)
            .into_iter()
            .filter_map(|view| view.downcast::<MergeWindow>().ok())
            .map(|view| {
                let view = view.read(cx);
                (view.document().output().into_text(), view.output().map(Path::to_path_buf))
            })
            .collect()
    })
}

fn dialog_open(cx: &mut TestAppContext) -> bool {
    cx.update(|cx| views(cx).into_iter().any(|view| view.downcast::<OpenDialog>().is_ok()))
}

fn error(cx: &mut TestAppContext, view: &Entity<OpenDialog>) -> Option<String> {
    cx.read(|cx| view.read(cx).error().map(ToString::to_string))
}

#[gpui_kit::test]
fn the_dialog_fills_the_slots_and_starts_a_merge(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, _view) = opened(cx);
    fill_all(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(merges(cx), [("A\nb\nC\n".to_owned(), Some(dir.path().join("out.txt")))]);
    assert!(!dialog_open(cx), "the dialog closes");
    assert!(!dir.path().join("out.txt").exists(), "nothing is written yet");
}

#[gpui_kit::test]
fn the_merge_titles_its_panes_with_the_file_names(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, _view) = opened(cx);
    fill_all(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let titles: Vec<Vec<String>> = cx.update(|cx| {
        views(cx)
            .into_iter()
            .filter_map(|view| view.downcast::<MergeWindow>().ok())
            .map(|view| {
                let titles = view.read(cx).pane_titles();
                titles.map(ToString::to_string).collect()
            })
            .collect()
    });
    assert_eq!(titles, [["left.txt", "right.txt"]]);
}

#[gpui_kit::test]
fn enter_in_a_slot_starts_the_merge(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, _view) = opened(cx);
    fill_all(cx, window, dir.path());
    act(cx, window, |window, cx| window.press("enter", cx));
    assert_eq!(merges(cx).len(), 1);
}

#[gpui_kit::test]
fn an_empty_slot_is_named_and_starts_nothing(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, view) = opened(cx);
    fill(cx, window, Slot::Base, &dir.path().join("base.txt"));
    fill(cx, window, Slot::Left, &dir.path().join("left.txt"));
    fill(cx, window, Slot::Output, &dir.path().join("out.txt"));
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(error(cx, &view).as_deref(), Some("Choose a file for Right."));
    assert_eq!(merges(cx), []);
    assert!(dialog_open(cx));
}

#[gpui_kit::test]
fn an_unreadable_input_is_named_and_the_slots_are_kept(cx: &mut TestAppContext) {
    let dir = inputs();
    std::fs::remove_file(dir.path().join("left.txt")).expect("left is removed");
    let (window, view) = opened(cx);
    fill_all(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let left = dir.path().join("left.txt");
    let message = error(cx, &view).expect("an error shows");
    assert!(message.starts_with(&format!("cannot read {}: ", left.display())), "{message}");
    assert_eq!(merges(cx), []);
    // Once the file is there, the same slots start the merge.
    std::fs::write(&left, "A\nb\nc\n").expect("left is written");
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(merges(cx).len(), 1);
}

#[gpui_kit::test]
fn a_long_error_leaves_start_in_view(cx: &mut TestAppContext) {
    // Windows' temporary paths and error texts are long enough to wrap over several lines.
    const DEPTH: usize = 6;
    let dir = inputs();
    let deep = (0..DEPTH).fold(dir.path().to_path_buf(), |path, n| {
        path.join(format!("a-directory-with-a-long-name-{n}"))
    });
    let (window, view) = opened(cx);
    fill(cx, window, Slot::Base, &deep.join("base.txt"));
    fill(cx, window, Slot::Left, &dir.path().join("left.txt"));
    fill(cx, window, Slot::Right, &dir.path().join("right.txt"));
    fill(cx, window, Slot::Output, &dir.path().join("out.txt"));
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert!(error(cx, &view).is_some());
    // Fixing the slot and starting again works with the error still showing.
    act(cx, window, |window, cx| {
        window.click(Slot::Base.id(), cx);
        window.press("secondary-a", cx);
        window.input(&dir.path().join("base.txt").to_string_lossy(), cx);
        window.click(START_ID, cx);
    });
    assert_eq!(merges(cx).len(), 1);
}

#[gpui_kit::test]
fn a_binary_input_is_refused(cx: &mut TestAppContext) {
    let dir = inputs();
    std::fs::write(dir.path().join("base.txt"), b"a\0b\n").expect("base is written");
    let (window, view) = opened(cx);
    fill_all(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let message = error(cx, &view).expect("an error shows");
    let base = dir.path().join("base.txt");
    assert!(message.starts_with(&format!("{}: ", base.display())), "{message}");
    assert_eq!(merges(cx), []);
}

#[gpui_kit::test]
fn choose_fills_a_slot_from_the_platform_picker(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, view) = opened(cx);
    let pick = |cx: &mut TestAppContext, slot: Slot, name: &str| {
        act(cx, window, |window, cx| window.click(slot.choose_id(), cx));
        let path = dir.path().join(name);
        cx.simulate_path_prompt_response(|options| {
            assert!(options.files && !options.directories && !options.multiple);
            Some(vec![path])
        });
        cx.run_until_parked();
    };
    pick(cx, Slot::Base, "base.txt");
    pick(cx, Slot::Left, "left.txt");
    pick(cx, Slot::Right, "right.txt");
    act(cx, window, |window, cx| {
        window.click(Slot::Output.choose_id(), cx);
    });
    let out = dir.path().join("out.txt");
    let expected_dir = dir.path().to_path_buf();
    cx.simulate_new_path_selection(|directory| {
        assert_eq!(directory, expected_dir, "the picker starts in base's directory");
        Some(out)
    });
    cx.run_until_parked();
    let slots = cx.read(|cx| view.read(cx).paths(cx));
    assert_eq!(
        slots,
        ["base.txt", "left.txt", "right.txt", "out.txt"].map(|n| dir
            .path()
            .join(n)
            .display()
            .to_string())
    );
}

#[gpui_kit::test]
fn closing_the_dialog_cancels(cx: &mut TestAppContext) {
    let (window, _view) = opened(cx);
    let finished = cx.update(common::Finished::install);
    let mut visual = VisualTestContext::from_window(window, cx);
    assert!(visual.simulate_close());
    assert_eq!(finished.codes(), [ExitCode::Cancelled]);
}

#[gpui_kit::test]
fn a_cancelled_picker_keeps_the_slot(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, view) = opened(cx);
    let typed = dir.path().join("base.txt");
    fill(cx, window, Slot::Base, &typed);
    act(cx, window, |window, cx| {
        window.click(Slot::Base.choose_id(), cx);
    });
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    let slots = cx.read(|cx| view.read(cx).paths(cx));
    assert_eq!(slots[0], typed.display().to_string());
}

#[gpui_kit::test]
fn the_dialog_aligns_with_the_settings_options(cx: &mut TestAppContext) {
    use merge_core::diff::{DiffOptions, Whitespace};
    use merge_core::document::MergeOptions;
    // Left only adds a trailing space: ignored, right's b is the merge; seen, a conflict.
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, text) in [("base.txt", "a\n"), ("left.txt", "a \n"), ("right.txt", "b\n")] {
        std::fs::write(dir.path().join(name), text).expect("the input is written");
    }
    let settings = WindowSettings {
        options: MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() },
            ..MergeOptions::default()
        },
        ..WindowSettings::default()
    };
    let (window, _) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::open_dialog::open(settings, NO_WATCH, cx).expect("the dialog opens")
    });
    act(cx, window, TestWindowExt::render_frame);
    fill_all(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(merges(cx)[0].0, "b\n");
}

/// The role and label of the element `id`.
fn named(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> (Option<gpui_kit::Role>, Option<String>) {
    let id = id.into();
    let mut found = None;
    act(cx, window, |window, _| {
        let element = window.find(id);
        found = Some((element.role(), element.label().map(str::to_owned)));
    });
    found.expect("the window ran")
}

#[gpui_kit::test]
fn every_slot_names_its_field_and_choose_button(cx: &mut TestAppContext) {
    use gpui_kit::Role;

    let (window, _) = opened(cx);
    let slots = [
        (Slot::Base, "Base", "Choose the base file"),
        (Slot::Left, "Left", "Choose the left file"),
        (Slot::Right, "Right", "Choose the right file"),
        (Slot::Output, "Output", "Choose the output file"),
    ];
    for (slot, field, choose) in slots {
        assert_eq!(
            named(cx, window, slot.id()),
            (Some(Role::TextInput), Some(field.to_owned())),
            "{slot:?}"
        );
        assert_eq!(
            named(cx, window, slot.choose_id()),
            (Some(Role::Button), Some(choose.to_owned())),
            "{slot:?}"
        );
    }
    assert_eq!(named(cx, window, START_ID), (Some(Role::Button), Some("Merge".to_owned())));
}

#[gpui_kit::test]
fn the_mark_heads_the_dialog_beside_the_tabs_and_names_the_app(cx: &mut TestAppContext) {
    use gpui_kit::{Role, px};
    use merge_app::open_dialog::MARK_ID;

    /// The mark's side in a title bar or toolbar, as `logo::MARK_SIDE` draws it.
    const MARK_SIDE: f32 = 18.;

    let (window, _) = opened(cx);
    assert_eq!(named(cx, window, MARK_ID), (Some(Role::Image), Some("triad".to_owned())));
    let mut bounds = None;
    act(cx, window, |window, _| {
        bounds = Some((
            window.find(MARK_ID).bounds(),
            window.find(Mode::Merge.id()).bounds(),
            window.find(Slot::Base.id()).bounds(),
        ));
    });
    let (mark, tab, base) = bounds.expect("the window ran");
    assert_eq!(mark.size, gpui_kit::size(px(MARK_SIDE), px(MARK_SIDE)));
    assert!(mark.right() <= tab.left(), "the mark sits left of the tabs");
    assert!(mark.bottom() <= base.top(), "the mark heads the slots");
}

#[gpui_kit::test]
fn the_error_is_an_alert_only_while_it_shows(cx: &mut TestAppContext) {
    use gpui_kit::Role;
    use merge_app::open_dialog::ERROR_ID;

    let (window, _) = opened(cx);
    assert_eq!(named(cx, window, ERROR_ID), (None, None));
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(
        named(cx, window, ERROR_ID),
        (Some(Role::Alert), Some("Choose a file for Base.".to_owned()))
    );
}

/// Inputs without an extension and an output named `out.rs` merge as Rust, as they colour:
/// the output names the language (spec §4.3). Deep refinement would split off the lines both
/// sides share, whose first edge falls inside the `if`; Rust's syntax boundaries keep the
/// conflict whole.
#[gpui_kit::test]
fn the_output_names_the_language_the_merge_keeps_syntax_boundaries_in(cx: &mut TestAppContext) {
    use merge_core::document::MergeOptions;
    use merge_core::merge3::{Kind, Refine};
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, text) in [
        ("base", "fn f() {\n    p();\n    q();\n}\n"),
        ("left", "fn f() {\n    if a {\n        m();\n    }\n    x();\n}\n"),
        ("right", "fn f() {\n    if b {\n        m();\n    }\n    y();\n}\n"),
    ] {
        std::fs::write(dir.path().join(name), text).expect("the input is written");
    }
    let settings = WindowSettings {
        options: MergeOptions {
            refine: Refine::Deep,
            syntax_boundaries: true,
            ..MergeOptions::default()
        },
        ..WindowSettings::default()
    };
    let (window, _) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::open_dialog::open(settings, NO_WATCH, cx).expect("the dialog opens")
    });
    act(cx, window, TestWindowExt::render_frame);
    for (slot, name) in [
        (Slot::Base, "base"),
        (Slot::Left, "left"),
        (Slot::Right, "right"),
        (Slot::Output, "out.rs"),
    ] {
        fill(cx, window, slot, &dir.path().join(name));
    }
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let kinds = cx.update(|cx| {
        views(cx)
            .into_iter()
            .find_map(|view| view.downcast::<MergeWindow>().ok())
            .expect("a merge window")
            .read(cx)
            .document()
            .regions()
            .iter()
            .map(|region| region.kind)
            .collect::<Vec<_>>()
    });
    assert_eq!(kinds, [Kind::Unchanged, Kind::Conflict, Kind::Unchanged]);
}

/// Switch the dialog to `mode`'s tab.
fn select(cx: &mut TestAppContext, window: AnyWindowHandle, mode: Mode) {
    act(cx, window, |window, cx| window.click(mode.id(), cx));
}

/// The diff windows open now, with each one's left and right text and pane titles.
fn diffs(cx: &mut TestAppContext) -> Vec<[String; 4]> {
    cx.update(|cx| {
        views(cx)
            .into_iter()
            .filter_map(|view| view.downcast::<DiffWindow>().ok())
            .map(|view| {
                let view = view.read(cx);
                [
                    view.document().text(Side::Left).to_string(),
                    view.document().text(Side::Right).to_string(),
                    view.title(Side::Left).to_string(),
                    view.title(Side::Right).to_string(),
                ]
            })
            .collect()
    })
}

fn fill_diff(cx: &mut TestAppContext, window: AnyWindowHandle, dir: &Path) {
    fill(cx, window, Slot::Left, &dir.join("left.txt"));
    fill(cx, window, Slot::Right, &dir.join("right.txt"));
}

#[gpui_kit::test]
fn the_diff_tab_compares_left_and_right_in_a_diff_window(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, _view) = opened(cx);
    select(cx, window, Mode::Diff);
    fill_diff(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let [left, right] = ["left.txt", "right.txt"].map(|n| dir.path().join(n).display().to_string());
    assert_eq!(diffs(cx), [["A\nb\nc\n".to_owned(), "a\nb\nC\n".to_owned(), left, right]]);
    assert_eq!(merges(cx), []);
    assert!(!dialog_open(cx), "the dialog closes");
}

#[gpui_kit::test]
fn the_diff_tab_ignores_line_endings_by_default(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("left.txt"), "a\r\nb\r\nc\r\n").expect("written");
    std::fs::write(dir.path().join("right.txt"), "a\nB\nc\n").expect("written");
    let (window, _view) = opened(cx);
    select(cx, window, Mode::Diff);
    fill_diff(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let status = cx.update(|cx| {
        views(cx)
            .into_iter()
            .find_map(|view| view.downcast::<DiffWindow>().ok())
            .expect("a diff window")
            .read(cx)
            .status_text()
            .to_string()
    });
    assert!(status.ends_with(" · 1 difference · line endings ignored"), "{status}");
}

#[gpui_kit::test]
fn enter_in_a_diff_slot_starts_the_diff(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, _view) = opened(cx);
    select(cx, window, Mode::Diff);
    fill_diff(cx, window, dir.path());
    act(cx, window, |window, cx| window.press("enter", cx));
    assert_eq!(diffs(cx).len(), 1);
    assert_eq!(merges(cx), []);
}

#[gpui_kit::test]
fn the_diff_tab_shows_left_and_right_only(cx: &mut TestAppContext) {
    let (window, _view) = opened(cx);
    let shown = |cx: &mut TestAppContext| {
        let mut shown = Vec::new();
        act(cx, window, |window, _| {
            shown = [Slot::Base, Slot::Left, Slot::Right, Slot::Output]
                .into_iter()
                .filter(|slot| window.try_find(slot.id()).is_some())
                .collect();
        });
        shown
    };
    assert_eq!(shown(cx), [Slot::Base, Slot::Left, Slot::Right, Slot::Output]);
    select(cx, window, Mode::Diff);
    assert_eq!(shown(cx), [Slot::Left, Slot::Right]);
    select(cx, window, Mode::Merge);
    assert_eq!(shown(cx), [Slot::Base, Slot::Left, Slot::Right, Slot::Output]);
}

#[gpui_kit::test]
fn the_slots_keep_their_paths_across_tabs(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, view) = opened(cx);
    fill_all(cx, window, dir.path());
    select(cx, window, Mode::Diff);
    select(cx, window, Mode::Merge);
    let slots = cx.read(|cx| view.read(cx).paths(cx));
    assert_eq!(
        slots,
        ["base.txt", "left.txt", "right.txt", "out.txt"].map(|n| dir
            .path()
            .join(n)
            .display()
            .to_string())
    );
    // Left and right typed for the merge serve the diff.
    select(cx, window, Mode::Diff);
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(diffs(cx).len(), 1);
}

#[gpui_kit::test]
fn an_empty_diff_slot_is_named_and_starts_nothing(cx: &mut TestAppContext) {
    let dir = inputs();
    let (window, view) = opened(cx);
    select(cx, window, Mode::Diff);
    fill(cx, window, Slot::Left, &dir.path().join("left.txt"));
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(error(cx, &view).as_deref(), Some("Choose a file for Right."));
    assert_eq!(diffs(cx), Vec::<[String; 4]>::new());
    assert!(dialog_open(cx));
}

#[gpui_kit::test]
fn an_unreadable_diff_input_is_named_and_the_slots_are_kept(cx: &mut TestAppContext) {
    let dir = inputs();
    let right = dir.path().join("right.txt");
    std::fs::remove_file(&right).expect("right is removed");
    let (window, view) = opened(cx);
    select(cx, window, Mode::Diff);
    fill_diff(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let message = error(cx, &view).expect("an error shows");
    assert!(message.starts_with(&format!("cannot read {}: ", right.display())), "{message}");
    assert_eq!(diffs(cx), Vec::<[String; 4]>::new());
    std::fs::write(&right, "a\nb\nC\n").expect("right is written");
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(diffs(cx).len(), 1);
}

#[gpui_kit::test]
fn a_binary_diff_input_is_refused(cx: &mut TestAppContext) {
    let dir = inputs();
    let left = dir.path().join("left.txt");
    std::fs::write(&left, b"a\0b\n").expect("left is written");
    let (window, view) = opened(cx);
    select(cx, window, Mode::Diff);
    fill_diff(cx, window, dir.path());
    act(cx, window, |window, cx| window.click(START_ID, cx));
    let message = error(cx, &view).expect("an error shows");
    assert!(message.starts_with(&format!("{}: ", left.display())), "{message}");
    assert_eq!(diffs(cx), Vec::<[String; 4]>::new());
}

#[gpui_kit::test]
fn switching_tabs_clears_the_error(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    act(cx, window, |window, cx| window.click(START_ID, cx));
    assert_eq!(error(cx, &view).as_deref(), Some("Choose a file for Base."));
    select(cx, window, Mode::Diff);
    assert_eq!(error(cx, &view), None);
}

#[gpui_kit::test]
fn the_tabs_are_named_and_selected_and_start_names_its_action(cx: &mut TestAppContext) {
    use gpui_kit::Role;

    let (window, view) = opened(cx);
    let state = |cx: &mut TestAppContext| {
        let mut found = Vec::new();
        act(cx, window, |window, _| {
            found = [Mode::Merge.id(), Mode::Diff.id(), START_ID.into()]
                .into_iter()
                .map(|id| {
                    let element = window.find(id);
                    (element.role(), element.label().map(str::to_owned), element.selected())
                })
                .collect();
        });
        found
    };
    let tab = |label: &str, selected| (Some(Role::Tab), Some(label.to_owned()), Some(selected));
    let start = |label: &str| (Some(Role::Button), Some(label.to_owned()), None);
    assert_eq!(state(cx), [tab("Merge", true), tab("Diff", false), start("Merge")]);
    assert_eq!(cx.read(|cx| view.read(cx).mode()), Mode::Merge);
    select(cx, window, Mode::Diff);
    assert_eq!(state(cx), [tab("Merge", false), tab("Diff", true), start("Compare")]);
    assert_eq!(cx.read(|cx| view.read(cx).mode()), Mode::Diff);
}

#[gpui_kit::test]
fn every_control_fits_the_window_in_both_tabs(cx: &mut TestAppContext) {
    let (window, _view) = opened(cx);
    for mode in [Mode::Merge, Mode::Diff] {
        select(cx, window, mode);
        let mut fits = None;
        act(cx, window, |window, _| {
            let viewport = window.viewport_size();
            let start = window.find(START_ID).bounds();
            let right = window.find(Slot::Right.choose_id()).bounds();
            fits = Some(
                start.bottom() <= viewport.height
                    && start.right() <= viewport.width
                    && right.right() <= viewport.width,
            );
        });
        assert_eq!(fits, Some(true), "{mode:?}");
    }
}
