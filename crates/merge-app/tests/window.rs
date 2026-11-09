//! Headless tests of the merge window (PLAN.md §A6.2).

use crate::common;
use merge_core::index::RegionIx;

use common::{act, caret, doc, open, open_with, set_caret, texts};
use gpui_kit::Action as _;
use gpui_kit::base::input::AddCursorBelow;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Pixels, Size, TestAppContext, px, size};
use merge_app::WindowSettings;
use merge_app::window::RESULT_ID;
use merge_core::document::Labels;
use merge_view::status::FileNames;
use std::path::Path;
use std::time::Duration;
use vcs::RepoKind;

fn titles(cx: &mut TestAppContext, view: &gpui_kit::Entity<merge_app::MergeWindow>) -> Vec<String> {
    cx.read(|cx| view.read(cx).pane_titles().map(ToString::to_string).collect())
}

/// git's temporary names, as `git mergetool` passes them.
fn git_names() -> WindowSettings {
    WindowSettings {
        file_names: FileNames::three_way(
            Path::new("./foo_BASE_1234.c"),
            Path::new("./foo_LOCAL_1234.c"),
            Path::new("./foo_REMOTE_1234.c"),
        ),
        ..WindowSettings::default()
    }
}

/// The size `window` opened at.
fn window_size(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) -> Size<Pixels> {
    let mut opened = None;
    act(cx, window, |window, _| opened = Some(window.bounds().size));
    opened.expect("the window reports its bounds")
}

#[gpui_kit::test]
fn the_window_opens_at_1400_by_900_by_default(cx: &mut TestAppContext) {
    let (window, _) = open(cx, doc("a\n", "b\n", "c\n"));
    assert_eq!(window_size(cx, window), size(px(1400.), px(900.)));
}

#[gpui_kit::test]
fn the_window_opens_at_the_settings_size(cx: &mut TestAppContext) {
    let settings =
        WindowSettings { window_size: size(px(1024.), px(768.)), ..WindowSettings::default() };
    let (window, _, _) = open_with(cx, doc("a\n", "b\n", "c\n"), &settings);
    assert_eq!(window_size(cx, window), size(px(1024.), px(768.)));
}

#[gpui_kit::test]
fn a_window_wider_than_its_display_opens_as_wide_as_the_display(cx: &mut TestAppContext) {
    // GPUI's test display is 1920 × 1080.
    let settings =
        WindowSettings { window_size: size(px(3000.), px(800.)), ..WindowSettings::default() };
    let (window, _, _) = open_with(cx, doc("a\n", "b\n", "c\n"), &settings);
    assert_eq!(window_size(cx, window), size(px(1920.), px(800.)));
}

#[gpui_kit::test]
fn panes_without_labels_show_their_file_names(cx: &mut TestAppContext) {
    let (window, view, _) = open_with(cx, doc("a\n", "b\n", "c\n"), &git_names());
    assert_eq!(titles(cx, &view), ["foo_LOCAL_1234.c", "foo_REMOTE_1234.c"]);
    // The base pane, shown later, takes its name too.
    act(cx, window, |window, cx| window.press("secondary-b", cx));
    assert_eq!(titles(cx, &view), ["foo_LOCAL_1234.c", "foo_BASE_1234.c", "foo_REMOTE_1234.c"]);
}

#[gpui_kit::test]
fn labels_title_their_panes_and_markers_ignore_file_names(cx: &mut TestAppContext) {
    let labelled = merge_core::document::MergeDocument::new(
        merge_core::merge3::Inputs::three_way(
            common::text("a\n"),
            common::text("b\n"),
            common::text("c\n"),
        ),
        merge_core::document::MergeOptions::default(),
        Labels::three_way("ours", "", ""),
    )
    .expect("three labels");
    let (_, view, _) = open_with(cx, labelled, &git_names());
    assert_eq!(titles(cx, &view), ["ours", "foo_REMOTE_1234.c"]);
    let (_, core) = texts(cx, &view);
    assert_eq!(core, "b\nc\n");
    let saved = cx.read(|cx| view.read(cx).document().output().into_text());
    assert_eq!(saved, "<<<<<<< ours\nb\n|||||||\na\n=======\nc\n>>>>>>>\n");
}

#[gpui_kit::test]
fn the_result_pane_shows_the_output_file_name(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let settings = WindowSettings { output: Some(dir.path().join("foo.c")), ..git_names() };
    let (_, view, _) = open_with(cx, doc("a\n", "b\n", "c\n"), &settings);
    let title = cx.read(|cx| view.read(cx).result_title().to_string());
    assert_eq!(title, "foo.c");
}

#[gpui_kit::test]
fn the_result_pane_without_an_output_is_titled_result(cx: &mut TestAppContext) {
    let (_, view) = open(cx, doc("a\n", "b\n", "c\n"));
    let title = cx.read(|cx| view.read(cx).result_title().to_string());
    assert_eq!(title, "Result");
}

#[gpui_kit::test]
fn the_result_starts_as_the_automatic_merge(cx: &mut TestAppContext) {
    let (window, view) = open(cx, doc("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n"));
    act(cx, window, TestWindowExt::render_frame);
    let (editor, core) = texts(cx, &view);
    assert_eq!(editor, "a\nB\nc\n");
    assert_eq!(core, "a\nB\nc\n", "the first parse records no edit");
    let status = cx.read(|cx| view.read(cx).status_text().clone());
    assert_eq!(
        status.as_ref(),
        "no repo · git merge · 1 difference · 0 conflicts · 0 unresolved · UTF-8 · LF · whitespace significant"
    );
}

#[gpui_kit::test]
fn the_status_names_the_repository_before_and_after_an_edit(cx: &mut TestAppContext) {
    let settings = WindowSettings { repo: Some(RepoKind::Jj), ..WindowSettings::default() };
    let (window, view, _) = open_with(cx, doc("a\nb\n", "a\nB\n", "a\nb\n"), &settings);
    let status = |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).status_text().to_string());
    assert_eq!(
        status(cx),
        "jj repo · git merge · 1 difference · 0 conflicts · 0 unresolved · UTF-8 · LF · \
         whitespace significant"
    );
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.input("x", cx));
    assert_eq!(texts(cx, &view).1, "xa\nB\n", "the edit reached the core");
    assert!(status(cx).starts_with("jj repo · git merge · "), "{}", status(cx));
}

#[gpui_kit::test]
fn typing_in_a_crlf_file_inserts_crlf(cx: &mut TestAppContext) {
    let crlf = "one\r\ntwo\r\n";
    let (window, view) = open(cx, doc(crlf, crlf, crlf));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    // After "one".
    set_caret(cx, &view, 3);
    // Each keystroke is its own event, as in the running app.
    act(cx, window, |window, cx| window.press("enter", cx));
    act(cx, window, |window, cx| window.input("x", cx));
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "one\r\nx\r\ntwo\r\n");
    assert_eq!(editor, core);
    assert_eq!(caret(cx, &view), "one\r\nx".len());
}

#[gpui_kit::test]
fn a_three_cursor_edit_undoes_in_one_step(cx: &mut TestAppContext) {
    let lines = "a\nb\nc\nd\n";
    let (window, view) = open(cx, doc(lines, lines, lines));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    // The editor's own key for this is ⌘⌥↓ on macOS, which triad takes for navigation.
    for _ in 0..2 {
        act(cx, window, |window, cx| {
            window.dispatch_action(AddCursorBelow.boxed_clone(), cx);
        });
    }
    act(cx, window, |window, cx| window.input("x", cx));
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "xa\nxb\nxc\nd\n");
    assert_eq!(editor, core);
    act(cx, window, |window, cx| window.press("secondary-z", cx));
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, lines, "one undo removes all three");
    assert_eq!(editor, lines);
    act(cx, window, |window, cx| {
        window.press("secondary-shift-z", cx);
    });
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "xa\nxb\nxc\nd\n");
    assert_eq!(editor, core);
}

#[gpui_kit::test]
fn a_patch_after_a_multibyte_character_lands_at_its_utf16_offset(cx: &mut TestAppContext) {
    // "é" is two bytes and one UTF-16 unit; "𝄞" is four bytes and two units.
    let start = "é𝄞\nz\n";
    let (window, view) = open(cx, doc(start, start, start));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    let after = "é𝄞".len();
    set_caret(cx, &view, after);
    act(cx, window, |window, cx| window.input("x", cx));
    assert_eq!(texts(cx, &view).1, "é𝄞x\nz\n");
    // Undo's patch removes bytes 6..7, which are UTF-16 units 3..4.
    act(cx, window, |window, cx| window.press("secondary-z", cx));
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, start);
    assert_eq!(editor, start);
    assert_eq!(caret(cx, &view), after);
    act(cx, window, |window, cx| {
        window.press("secondary-shift-z", cx);
    });
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "é𝄞x\nz\n");
    assert_eq!(editor, core);
    assert_eq!(caret(cx, &view), after + 1);
}

#[gpui_kit::test]
fn the_editors_own_undo_does_not_run(cx: &mut TestAppContext) {
    let lines = "a\n";
    let (window, view) = open(cx, doc(lines, lines, lines));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 1);
    act(cx, window, |window, cx| window.input("x", cx));
    act(cx, window, |window, cx| window.input("y", cx));
    // The core joins typing into one step; the editor would undo one character.
    act(cx, window, |window, cx| window.press("secondary-z", cx));
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, lines);
    assert_eq!(editor, lines);
    // Nothing is left to undo, so a second press changes nothing.
    act(cx, window, |window, cx| window.press("secondary-z", cx));
    assert_eq!(texts(cx, &view), (lines.to_owned(), lines.to_owned()));
}

#[gpui_kit::test]
fn edits_that_arrive_together_keep_the_editor_and_core_equal(cx: &mut TestAppContext) {
    let crlf = "one\r\ntwo\r\n";
    let (window, view) = open(cx, doc(crlf, crlf, crlf));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 3);
    // Both keystrokes reach the window in one batch; the first needs a correction.
    act(cx, window, |window, cx| {
        window.press("enter", cx);
        window.input("x", cx);
    });
    let (editor, core) = texts(cx, &view);
    assert_eq!(editor, core);
    assert_eq!(core.matches("\r\n").count(), 3, "every newline is CRLF: {core:?}");
    assert!(!core.contains("\n\r"), "no bare LF before a CR: {core:?}");
}

#[gpui_kit::test]
fn undo_sends_waiting_edits_first(cx: &mut TestAppContext) {
    let lines = "a\n";
    let (window, view) = open(cx, doc(lines, lines, lines));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 1);
    act(cx, window, |window, cx| {
        window.input("x", cx);
        window.press("secondary-z", cx);
    });
    assert_eq!(texts(cx, &view), (lines.to_owned(), lines.to_owned()));
}

#[gpui_kit::test]
fn theme_settings_hold_the_merge_window_light_or_dark(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    use merge_config::settings::Theme;
    let lines = "a\n";
    for (theme, dark) in [(Theme::Dark, true), (Theme::Light, false)] {
        let settings = merge_app::WindowSettings { theme, ..merge_app::WindowSettings::default() };
        let _opened = common::open_with(cx, doc(lines, lines, lines), &settings);
        assert_eq!(cx.read(|cx| cx.theme().is_dark()), dark, "{theme:?}");
    }
}

#[gpui_kit::test]
fn the_tab_key_types_the_tab_width_in_spaces(cx: &mut TestAppContext) {
    const TAB_WIDTH: usize = 6;
    let settings = merge_app::WindowSettings {
        typography: merge_app::Typography {
            tab_width: TAB_WIDTH,
            ..merge_app::Typography::default()
        },
        ..merge_app::WindowSettings::default()
    };
    let lines = "a\n";
    let (window, view, _) = common::open_with(cx, doc(lines, lines, lines), &settings);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.press("tab", cx));
    let (editor, core) = texts(cx, &view);
    assert_eq!(core, "      a\n");
    assert_eq!(editor, core);
}

#[gpui_kit::test]
fn the_tab_key_types_four_spaces_by_default(cx: &mut TestAppContext) {
    let lines = "a\n";
    let (window, view) = open(cx, doc(lines, lines, lines));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.press("tab", cx));
    assert_eq!(texts(cx, &view).1, "    a\n");
}

#[gpui_kit::test]
fn the_region_holding_the_caret_is_highlighted_unless_unchanged(cx: &mut TestAppContext) {
    // Regions: 0 a, 1 conflict, 2 c, 3 D (left only), 4 e.
    // Result bytes: a\n 0..2, L\nR\n 2..6, c\n 6..8, D\n 8..10, e\n 10..12.
    let (window, view) = open(cx, doc("a\nb\nc\nd\ne\n", "a\nL\nc\nD\ne\n", "a\nR\nc\nd\ne\n"));
    // The window opens on the conflict; start in unchanged `a`.
    set_caret(cx, &view, 0);
    let highlighted = |cx: &mut TestAppContext| cx.read(|cx| view.read(cx).highlighted());
    assert_eq!(highlighted(cx), None, "the caret starts in unchanged `a`");
    act(cx, window, |window, cx| {
        window.press("secondary-alt-down", cx);
    });
    assert_eq!(highlighted(cx), Some((RegionIx::new(1), 2..6)));
    // Picking left shrinks the conflict to L\n, and the highlight with it.
    act(cx, window, |window, cx| window.press("secondary-1", cx));
    assert_eq!(highlighted(cx), Some((RegionIx::new(1), 2..4)));
    // Now: a\n 0..2, L\n 2..4, c\n 4..6, D\n 6..8, e\n 8..10.
    set_caret(cx, &view, 6);
    assert_eq!(highlighted(cx), Some((RegionIx::new(3), 6..8)), "a one-sided change");
    set_caret(cx, &view, 4);
    assert_eq!(highlighted(cx), None, "unchanged `c`");
}

/// After a jump the highlight blinks for 500 ms (spec §6): hidden and shown in turn, each
/// for `BLINK`, starting hidden and ending shown.
const BLINK: Duration = Duration::from_millis(100);

#[gpui_kit::test]
fn a_jump_blinks_the_highlight_for_half_a_second(cx: &mut TestAppContext) {
    // Regions: 0 a, 1 conflict, 2 c, 3 conflict, 4 e.
    let (window, view) = open(cx, doc("a\nb\nc\nd\ne\n", "a\nL\nc\nM\ne\n", "a\nR\nc\nN\ne\n"));
    // The window opens on the first conflict; start in unchanged `a`.
    set_caret(cx, &view, 0);
    // (flashing, frame shown)
    let state = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let view = view.read(cx);
            (view.flashing(), view.highlight_shown())
        })
    };
    let wait = |cx: &mut TestAppContext| {
        cx.executor().advance_clock(BLINK);
        cx.run_until_parked();
    };
    let press = |cx: &mut TestAppContext, key: &str| {
        act(cx, window, |window, cx| window.press(key, cx));
    };
    assert_eq!(state(cx), (false, false), "no region highlighted yet");
    press(cx, "secondary-alt-down");
    let mut seen = vec![state(cx)];
    for _ in 0..5 {
        wait(cx);
        seen.push(state(cx));
    }
    assert_eq!(
        seen,
        [(true, false), (true, true), (true, false), (true, true), (true, false), (false, true),]
    );
    // A jump to conflict 2, then back to conflict 1 mid-blink, starts the half second
    // again: four blinks later the first would be over.
    press(cx, "secondary-alt-down");
    wait(cx);
    wait(cx);
    press(cx, "secondary-alt-up");
    assert_eq!(state(cx), (true, false));
    for _ in 0..4 {
        wait(cx);
    }
    assert_eq!(state(cx), (true, false), "the jump back restarted the blink");
    wait(cx);
    assert_eq!(state(cx), (false, true));
    // No earlier conflict: the caret stays and nothing blinks.
    press(cx, "secondary-alt-up");
    assert_eq!(state(cx), (false, true));
}

const BAD_LINES: [&str; 2] = [
    "/c/config.triad:2: tab-width 0 is outside 1–16",
    "/c/config.triad:5: unknown setting `scrol-sync`",
];

/// The role and accessible label of the element `id`; none when it is not drawn.
fn element(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    id: &'static str,
) -> Option<(Option<gpui_kit::Role>, Option<String>)> {
    let mut found = None;
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        found = window.try_find(id).map(|e| (e.role(), e.label().map(str::to_owned)));
    });
    found
}

fn set_errors(
    cx: &mut TestAppContext,
    view: &gpui_kit::Entity<merge_app::MergeWindow>,
    lines: &[&str],
) {
    let lines: Vec<String> = lines.iter().map(|&l| l.to_owned()).collect();
    view.update(cx, |view, cx| view.set_settings_errors(&lines, cx));
}

#[gpui_kit::test]
fn settings_errors_show_in_an_alert_banner(cx: &mut TestAppContext) {
    use merge_app::settings_banner::BANNER_ID;
    let lines = "a\n";
    let (window, view) = open(cx, doc(lines, lines, lines));
    assert_eq!(element(cx, window, BANNER_ID), None, "no banner at first");
    set_errors(cx, &view, &BAD_LINES);
    assert_eq!(
        cx.read(|cx| view.read(cx).settings_banner().map(ToString::to_string)),
        Some(BAD_LINES.join("\n"))
    );
    assert_eq!(
        element(cx, window, BANNER_ID),
        Some((
            Some(gpui_kit::Role::Alert),
            Some(format!("The config file has errors: {}", BAD_LINES.join("; ")))
        ))
    );
}

#[gpui_kit::test]
fn the_settings_banner_closes_with_its_button_and_reopens_on_new_errors(cx: &mut TestAppContext) {
    use merge_app::settings_banner::{BANNER_ID, CLOSE_ID};
    let lines = "a\n";
    let (window, view) = open(cx, doc(lines, lines, lines));
    set_errors(cx, &view, &BAD_LINES);
    assert_eq!(
        element(cx, window, CLOSE_ID),
        Some((Some(gpui_kit::Role::Button), Some("Close".to_owned())))
    );
    act(cx, window, |window, cx| window.click(CLOSE_ID, cx));
    assert_eq!(element(cx, window, BANNER_ID), None);
    assert_eq!(cx.read(|cx| view.read(cx).settings_banner().cloned()), None);
    set_errors(cx, &view, &BAD_LINES[..1]);
    assert_eq!(
        cx.read(|cx| view.read(cx).settings_banner().map(ToString::to_string)),
        Some(BAD_LINES[0].to_owned())
    );
}

#[gpui_kit::test]
fn a_clean_reload_closes_the_settings_banner(cx: &mut TestAppContext) {
    use merge_app::settings_banner::BANNER_ID;
    let lines = "a\n";
    let (window, view) = open(cx, doc(lines, lines, lines));
    set_errors(cx, &view, &BAD_LINES);
    set_errors(cx, &view, &[]);
    assert_eq!(element(cx, window, BANNER_ID), None);
}

/// The result's band ranges as the editor tracks them, and as a rebuild from the core gives.
fn bands_and_rebuild(
    cx: &mut TestAppContext,
    view: &gpui_kit::Entity<merge_app::MergeWindow>,
) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    cx.read(|cx| {
        let window = view.read(cx);
        let rebuilt = merge_view::marks::bands(window.document())
            .into_iter()
            .map(|band| band.bytes)
            .collect();
        (window.result_band_ranges(cx), rebuilt)
    })
}

#[gpui_kit::test]
fn the_result_bands_follow_edits_picks_and_undo(cx: &mut TestAppContext) {
    // Left changes line 1; line 3 conflicts. The result reads `a\nB\nc\nD\nE\n`.
    let (window, view) = open(cx, doc("a\nb\nc\nd\n", "a\nB\nc\nD\n", "a\nb\nc\nE\n"));
    let check = |cx: &mut TestAppContext, step: &str| {
        let (tracked, rebuilt) = bands_and_rebuild(cx, &view);
        assert_eq!(tracked, rebuilt, "after {step}");
    };
    check(cx, "opening");
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    // Inside left's band, after `B`, then again at the same place.
    set_caret(cx, &view, 3);
    act(cx, window, |window, cx| window.input("x", cx));
    check(cx, "a key inside a band");
    act(cx, window, |window, cx| window.input("y", cx));
    check(cx, "a second key inside the band");
    act(cx, window, |window, cx| window.press("enter", cx));
    check(cx, "a newline inside the band");
    // At the start of the conflict's first line: the core gives the key to the conflict,
    // which the editor's tracking leaves out.
    let conflict = cx.read(|cx| {
        let doc = view.read(cx).document();
        doc.region_ixs()
            .find(|&r| doc.region(r).kind == merge_core::merge3::Kind::Conflict)
            .map(|r| doc.region_bytes(r).start)
            .expect("one conflict")
    });
    set_caret(cx, &view, conflict);
    act(cx, window, |window, cx| window.input("z", cx));
    check(cx, "a key at the start of a band");
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(merge_app::Undo), cx);
    });
    check(cx, "undo");
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(merge_app::TakeLeft), cx);
    });
    check(cx, "a pick");
}
