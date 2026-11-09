//! Headless tests of the two-way diff window (spec §6.1; PLAN.md §A6.5).

use crate::common;
use merge_core::index::RegionIx;

use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use common::{Finished, act};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Entity, Pixels, Role, ScrollDelta, Size, TestAppContext, point, px, size,
};
use merge_app::ExitCode;
use merge_app::diff_window::{
    DIFF_DEBOUNCE, DIFF_RULER_ID, DiffFile, DiffSettings, DiffWindow, Origin, copy_id, pane_id,
};
use merge_app::toolbar::Tool;
use merge_app::window::TOOLBAR_ID;
use merge_config::keybind::{KeyBindings, Keybind, Platform};
use merge_core::diff2::{Kind2, Region2, Side};
use merge_core::text::Text;
use tempfile::TempDir;
use vcs::RepoKind;

const JUST_BEFORE: Duration = Duration::from_millis(99);
const ONE_MS: Duration = Duration::from_millis(1);
const SAVE: &str = "Save";
const DISCARD: &str = "Discard";
const KEEP_EDITING: &str = "Keep Editing";
const MODIFIED: &str = " ●";
const SIDES_OF_TEST: [Side; 2] = [Side::Left, Side::Right];
const RELOAD: &str = "Reload";
const KEEP_MINE: &str = "Keep Mine";
const UTF16_LE_BOM: [u8; 2] = [0xFF, 0xFE];

fn utf16(s: &str) -> Vec<u8> {
    UTF16_LE_BOM.into_iter().chain(s.encode_utf16().flat_map(u16::to_le_bytes)).collect()
}

struct Files {
    dir: TempDir,
}

impl Files {
    fn new(left: &str, right: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("left.txt"), left).expect("left");
        std::fs::write(dir.path().join("right.txt"), right).expect("right");
        Self { dir }
    }

    fn path(&self, side: Side) -> PathBuf {
        self.dir.path().join(match side {
            Side::Left => "left.txt",
            Side::Right => "right.txt",
        })
    }

    fn read(&self, side: Side) -> String {
        std::fs::read_to_string(self.path(side)).expect("readable")
    }

    fn file(&self, side: Side, read_only: bool) -> DiffFile {
        let bytes = std::fs::read(self.path(side)).expect("readable");
        DiffFile {
            origin: Origin::File(self.path(side)),
            text: Text::decode(bytes).expect("decodes"),
            read_only,
        }
    }
}

struct Opened {
    window: AnyWindowHandle,
    view: Entity<DiffWindow>,
    finished: Finished,
}

fn open_with(cx: &mut TestAppContext, files: &Files, read_only: [bool; 2]) -> Opened {
    open_files(cx, files.file(Side::Left, read_only[0]), files.file(Side::Right, read_only[1]))
}

fn open_files(cx: &mut TestAppContext, left: DiffFile, right: DiffFile) -> Opened {
    open_files_with(cx, left, right, &DiffSettings::default())
}

fn open_files_with(
    cx: &mut TestAppContext,
    left: DiffFile,
    right: DiffFile,
    settings: &DiffSettings,
) -> Opened {
    let (window, view, finished) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        let finished = Finished::install(cx);
        let (window, view) =
            merge_app::diff_window::open(left, right, settings, cx).expect("window opens");
        (window, view, finished)
    });
    act(cx, window, TestWindowExt::render_frame);
    Opened { window, view, finished }
}

fn open(cx: &mut TestAppContext, files: &Files) -> Opened {
    open_with(cx, files, [false, false])
}

/// The size the window opened at.
fn window_size(cx: &mut TestAppContext, o: &Opened) -> Size<Pixels> {
    let mut opened = None;
    act(cx, o.window, |window, _| {
        opened = Some(window.bounds().size);
    });
    opened.expect("the window reports its bounds")
}

#[gpui_kit::test]
fn the_window_opens_at_1400_by_900_by_default(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new("a\n", "b\n"));
    assert_eq!(window_size(cx, &o), size(px(1400.), px(900.)));
}

#[gpui_kit::test]
fn the_window_opens_at_the_settings_size_and_no_taller_than_its_display(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let open_at = |cx: &mut TestAppContext, window_size| {
        let settings = DiffSettings { window_size, ..DiffSettings::default() };
        open_files_with(
            cx,
            files.file(Side::Left, false),
            files.file(Side::Right, false),
            &settings,
        )
    };
    let o = open_at(cx, size(px(1024.), px(768.)));
    assert_eq!(window_size(cx, &o), size(px(1024.), px(768.)));
    // GPUI's test display is 1920 × 1080.
    let o = open_at(cx, size(px(1000.), px(2000.)));
    assert_eq!(window_size(cx, &o), size(px(1000.), px(1080.)));
}

/// Click into `side`'s pane, put the caret at `at` and type `text`.
fn type_at(cx: &mut TestAppContext, o: &Opened, side: Side, at: usize, text: &str) {
    act(cx, o.window, |window, cx| window.click(pane_id(side), cx));
    o.view.update(cx, |view, cx| {
        view.editor(side).update(cx, |state, cx| state.set_selected_range(at..at, cx));
    });
    act(cx, o.window, |window, cx| window.input(text, cx));
}

fn press(cx: &mut TestAppContext, o: &Opened, key: &str) {
    act(cx, o.window, |window, cx| window.press(key, cx));
}

/// Save, and let the writes land.
fn save(cx: &mut TestAppContext, o: &Opened) {
    press(cx, o, "secondary-s");
    cx.run_until_parked();
}

fn wait(cx: &mut TestAppContext, by: Duration) {
    cx.executor().advance_clock(by);
    cx.run_until_parked();
}

/// The core's text and the editor's text of `side`.
fn texts(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side) -> (String, String) {
    cx.read(|cx| {
        let view = view.read(cx);
        (view.document().text(side).to_string(), view.editor(side).read(cx).text().to_string())
    })
}

fn regions(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> Vec<Region2> {
    cx.read(|cx| view.read(cx).document().regions().to_vec())
}

fn status(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> String {
    cx.read(|cx| view.read(cx).status_text().to_string())
}

fn title(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side) -> String {
    cx.read(|cx| view.read(cx).title(side).to_string())
}

fn region(kind: Kind2, left: std::ops::Range<usize>, right: std::ops::Range<usize>) -> Region2 {
    Region2 { kind, left, right }
}

#[gpui_kit::test]
fn the_panes_show_both_files_and_their_names(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\n", "a\nc\n");
    let o = open(cx, &files);
    assert_eq!(texts(cx, &o.view, Side::Left).1, "a\nb\n");
    assert_eq!(texts(cx, &o.view, Side::Right).1, "a\nc\n");
    assert_eq!(title(cx, &o.view, Side::Left), files.path(Side::Left).display().to_string());
    assert_eq!(status(cx, &o.view), "no repo · 1 difference · whitespace significant");
}

#[gpui_kit::test]
fn the_status_names_the_repository_before_and_after_a_recompute(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "a\nc\n");
    let settings = DiffSettings { repo: Some(RepoKind::Git), ..DiffSettings::default() };
    let o = open_files_with(
        cx,
        files.file(Side::Left, false),
        files.file(Side::Right, false),
        &settings,
    );
    assert_eq!(status(cx, &o.view), "git repo · 1 difference · whitespace significant");
    type_at(cx, &o, Side::Left, "a\n".len(), "c\n");
    wait(cx, DIFF_DEBOUNCE);
    assert_eq!(
        status(cx, &o.view),
        "git repo · 0 differences · whitespace significant",
        "the recompute keeps the repository"
    );
}

/// `diff PATH`: the left side is a revision, the right the working-copy file (spec §6.1).
fn open_revision(cx: &mut TestAppContext, files: &Files, left: &str, title: &str) -> Opened {
    let revision = DiffFile::revision(Text::decode(left.as_bytes()).expect("decodes"), title);
    open_files(cx, revision, files.file(Side::Right, false))
}

#[gpui_kit::test]
fn a_revision_side_shows_its_title_and_refuses_typing(cx: &mut TestAppContext) {
    let files = Files::new("unused\n", "a\nc\n");
    let o = open_revision(cx, &files, "a\nb\n", "main");
    assert_eq!(texts(cx, &o.view, Side::Left).1, "a\nb\n");
    assert_eq!(title(cx, &o.view, Side::Left), "main");
    type_at(cx, &o, Side::Left, 0, "x");
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\nb\n");
    assert!(!has_copy(cx, &o, RegionIx::new(1), Side::Left), "nothing copies into a revision");
    assert!(has_copy(cx, &o, RegionIx::new(1), Side::Right));
    let editor = cx.read(|cx| o.view.read(cx).editor(Side::Left).entity_id());
    assert_eq!(named(cx, &o, ("input", editor)).1.as_deref(), Some("Left: main"));
}

#[gpui_kit::test]
fn a_revision_side_ignores_disk_changes_and_saves_only_the_file(cx: &mut TestAppContext) {
    let files = Files::new("on disk\n", "a\n");
    // Titled with a real file's path, which the side must never read or write.
    let named_like_a_file = files.path(Side::Left).display().to_string();
    let o = open_revision(cx, &files, "a\n", &named_like_a_file);
    disk_changed(cx, &o, Side::Left);
    assert!(!cx.has_pending_prompt());
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\n");
    type_at(cx, &o, Side::Right, 0, "z");
    save(cx, &o);
    assert_eq!(files.read(Side::Right), "za\n");
    assert_eq!(files.read(Side::Left), "on disk\n");
}

#[gpui_kit::test]
fn typing_edits_that_side_in_the_core(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\n", "a\nc\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Right, 0, "x");
    assert_eq!(texts(cx, &o.view, Side::Right), ("xa\nc\n".to_owned(), "xa\nc\n".to_owned()));
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\nb\n");
    let expected = files.path(Side::Right).display().to_string() + MODIFIED;
    assert_eq!(title(cx, &o.view, Side::Right), expected);
}

#[gpui_kit::test]
fn a_newline_typed_in_a_crlf_file_takes_its_ending(cx: &mut TestAppContext) {
    let files = Files::new("a\r\nb\r\n", "a\r\nb\r\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x\n");
    assert_eq!(
        texts(cx, &o.view, Side::Left),
        ("x\r\na\r\nb\r\n".to_owned(), "x\r\na\r\nb\r\n".to_owned())
    );
}

#[gpui_kit::test]
fn regions_recompute_once_typing_pauses(cx: &mut TestAppContext) {
    // a / a b: one insertion. Typing "b\n" at the end of left makes the files equal.
    let files = Files::new("a\n", "a\nb\n");
    assert_eq!(JUST_BEFORE + ONE_MS, DIFF_DEBOUNCE, "100 ms, from the spec");
    let o = open(cx, &files);
    let before = vec![region(Kind2::Unchanged, 0..1, 0..1), region(Kind2::Changed, 1..1, 1..2)];
    assert_eq!(regions(cx, &o.view), before);
    type_at(cx, &o, Side::Left, 2, "b");
    wait(cx, JUST_BEFORE);
    // A second edit restarts the wait.
    act(cx, o.window, |window, cx| window.input("\n", cx));
    wait(cx, JUST_BEFORE);
    assert_eq!(regions(cx, &o.view), before, "not 100 ms after the last edit");
    wait(cx, ONE_MS);
    assert_eq!(regions(cx, &o.view), [region(Kind2::Unchanged, 0..2, 0..2)]);
    assert_eq!(status(cx, &o.view), "no repo · 0 differences · whitespace significant");
}

#[gpui_kit::test]
fn undo_and_redo_act_on_the_focused_side(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    type_at(cx, &o, Side::Right, 0, "y");
    act(cx, o.window, |window, cx| {
        window.click(pane_id(Side::Left), cx);
    });
    press(cx, &o, "secondary-z");
    assert_eq!(texts(cx, &o.view, Side::Left), ("a\n".to_owned(), "a\n".to_owned()));
    assert_eq!(texts(cx, &o.view, Side::Right).0, "yb\n");
    press(cx, &o, "secondary-shift-z");
    assert_eq!(texts(cx, &o.view, Side::Left), ("xa\n".to_owned(), "xa\n".to_owned()));
    act(cx, o.window, |window, cx| {
        window.click(pane_id(Side::Right), cx);
    });
    press(cx, &o, "secondary-z");
    assert_eq!(texts(cx, &o.view, Side::Right), ("b\n".to_owned(), "b\n".to_owned()));
    assert_eq!(texts(cx, &o.view, Side::Left).0, "xa\n");
}

#[gpui_kit::test]
fn a_read_only_side_refuses_typing(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open_with(cx, &files, [true, false]);
    type_at(cx, &o, Side::Left, 0, "x");
    assert_eq!(texts(cx, &o.view, Side::Left), ("a\n".to_owned(), "a\n".to_owned()));
    type_at(cx, &o, Side::Right, 0, "y");
    assert_eq!(texts(cx, &o.view, Side::Right).0, "yb\n");
    // The editor itself refuses, so no edit reaches the core to be turned back.
    let read_only = cx.read(|cx| {
        let view = o.view.read(cx);
        SIDES_OF_TEST.map(|side| view.editor(side).read(cx).presentation().is_readonly())
    });
    assert_eq!(read_only, [true, false]);
}

#[gpui_kit::test]
fn save_writes_only_the_edited_side_and_stays_open(cx: &mut TestAppContext) {
    let files = Files::new("a\r\nb\r\n", "a\nc\n");
    let o = open(cx, &files);
    // Left changes on disk; left has no edits, so saving must not overwrite it.
    std::fs::write(files.path(Side::Left), "on disk\n").expect("left");
    type_at(cx, &o, Side::Right, 0, "x");
    save(cx, &o);
    assert_eq!(files.read(Side::Right), "xa\nc\n");
    assert_eq!(files.read(Side::Left), "on disk\n");
    assert_eq!(o.finished.codes(), []);
    assert_eq!(title(cx, &o.view, Side::Right), files.path(Side::Right).display().to_string());
}

#[gpui_kit::test]
fn save_keeps_a_utf16_file_utf16(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "a\n");
    std::fs::write(files.path(Side::Right), utf16("b\n")).expect("right");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Right, 0, "x");
    save(cx, &o);
    assert_eq!(std::fs::read(files.path(Side::Right)).expect("readable"), utf16("xb\n"));
}

#[gpui_kit::test]
fn a_failed_save_says_why_and_keeps_the_edits(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Right, 0, "x");
    // A directory where the file was: the write's rename cannot replace it.
    std::fs::remove_file(files.path(Side::Right)).expect("removed");
    std::fs::create_dir(files.path(Side::Right)).expect("a directory");
    save(cx, &o);
    assert!(cx.has_pending_prompt(), "an alert says why");
    cx.simulate_prompt_answer("OK");
    cx.run_until_parked();
    assert_eq!(o.finished.codes(), []);
    let expected = files.path(Side::Right).display().to_string() + MODIFIED;
    assert_eq!(title(cx, &o.view, Side::Right), expected);
}

#[gpui_kit::test]
fn a_failed_save_on_closing_keeps_the_window_open(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Right, 0, "x");
    std::fs::remove_file(files.path(Side::Right)).expect("removed");
    std::fs::create_dir(files.path(Side::Right)).expect("a directory");
    press(cx, &o, "secondary-w");
    cx.simulate_prompt_answer(SAVE);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "an alert says why");
    cx.simulate_prompt_answer("OK");
    cx.run_until_parked();
    assert_eq!(o.finished.codes(), []);
}

#[gpui_kit::test]
fn closing_without_edits_exits_with_0(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    press(cx, &o, "secondary-w");
    assert!(!cx.has_pending_prompt());
    assert_eq!(o.finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn closing_with_edits_asks_and_keep_editing_stays(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-w");
    cx.simulate_prompt_answer(KEEP_EDITING);
    cx.run_until_parked();
    assert_eq!(o.finished.codes(), []);
    assert_eq!(files.read(Side::Left), "a\n");
}

#[gpui_kit::test]
fn closing_with_edits_and_discard_writes_nothing(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-w");
    cx.simulate_prompt_answer(DISCARD);
    cx.run_until_parked();
    assert_eq!(o.finished.codes(), [ExitCode::Resolved]);
    assert_eq!(files.read(Side::Left), "a\n");
}

#[gpui_kit::test]
fn closing_with_edits_and_save_writes_then_exits(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-w");
    cx.simulate_prompt_answer(SAVE);
    cx.run_until_parked();
    assert_eq!(files.read(Side::Left), "xa\n");
    assert_eq!(o.finished.codes(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn undo_to_the_saved_text_still_counts_as_an_edit(cx: &mut TestAppContext) {
    // The marker tracks saves, not content: undo after a save leaves the side unsaved.
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    save(cx, &o);
    press(cx, &o, "secondary-z");
    let expected = files.path(Side::Left).display().to_string() + MODIFIED;
    assert_eq!(title(cx, &o.view, Side::Left), expected);
    save(cx, &o);
    assert_eq!(files.read(Side::Left), "a\n");
}

/// Tell the window that `side`'s file changed on disk, as its watcher does.
fn disk_changed(cx: &mut TestAppContext, o: &Opened, side: Side) {
    let view = o.view.clone();
    act(cx, o.window, |window, cx| {
        view.update(cx, |view, cx| view.disk_changed(side, window, cx));
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_pane_without_edits_reloads_a_change_on_disk(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "a\nb\n");
    let o = open(cx, &files);
    std::fs::write(files.path(Side::Left), "a\nb\n").expect("left");
    disk_changed(cx, &o, Side::Left);
    assert!(!cx.has_pending_prompt());
    assert_eq!(texts(cx, &o.view, Side::Left), ("a\nb\n".to_owned(), "a\nb\n".to_owned()));
    let path = files.path(Side::Left).display().to_string();
    assert_eq!(title(cx, &o.view, Side::Left), path);
    wait(cx, DIFF_DEBOUNCE);
    assert_eq!(status(cx, &o.view), "no repo · 0 differences · whitespace significant");
}

#[gpui_kit::test]
fn a_change_on_disk_is_read_off_the_ui_thread(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    std::fs::write(files.path(Side::Left), "a\nb\n").expect("left");
    let view = o.view.clone();
    act(cx, o.window, |window, cx| {
        view.update(cx, |view, cx| view.disk_changed(Side::Left, window, cx));
    });
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\n", "not read yet");
    cx.run_until_parked();
    assert_eq!(texts(cx, &o.view, Side::Left), ("a\nb\n".to_owned(), "a\nb\n".to_owned()));
}

#[gpui_kit::test]
fn a_pane_with_edits_asks_and_reload_takes_the_disk(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    std::fs::write(files.path(Side::Left), "new\n").expect("left");
    disk_changed(cx, &o, Side::Left);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer(RELOAD);
    cx.run_until_parked();
    assert_eq!(texts(cx, &o.view, Side::Left), ("new\n".to_owned(), "new\n".to_owned()));
    let path = files.path(Side::Left).display().to_string();
    assert_eq!(title(cx, &o.view, Side::Left), path);
}

#[gpui_kit::test]
fn keep_mine_keeps_the_edits_and_the_marker(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    std::fs::write(files.path(Side::Left), "new\n").expect("left");
    disk_changed(cx, &o, Side::Left);
    cx.simulate_prompt_answer(KEEP_MINE);
    cx.run_until_parked();
    assert_eq!(texts(cx, &o.view, Side::Left).0, "xa\n");
    let expected = files.path(Side::Left).display().to_string() + MODIFIED;
    assert_eq!(title(cx, &o.view, Side::Left), expected);
    assert_eq!(files.read(Side::Left), "new\n");
}

#[gpui_kit::test]
fn a_second_change_while_asking_asks_once(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    std::fs::write(files.path(Side::Left), "new\n").expect("left");
    disk_changed(cx, &o, Side::Left);
    std::fs::write(files.path(Side::Left), "newer\n").expect("left");
    disk_changed(cx, &o, Side::Left);
    cx.simulate_prompt_answer(RELOAD);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "one prompt for both changes");
    assert_eq!(texts(cx, &o.view, Side::Left).0, "newer\n");
}

#[gpui_kit::test]
fn the_windows_own_save_changes_nothing(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    save(cx, &o);
    disk_changed(cx, &o, Side::Left);
    assert!(!cx.has_pending_prompt());
    // A reload would have dropped the history.
    press(cx, &o, "secondary-z");
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\n");
}

#[gpui_kit::test]
fn a_file_gone_from_disk_is_left_as_it_is(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    std::fs::remove_file(files.path(Side::Left)).expect("removed");
    disk_changed(cx, &o, Side::Left);
    assert!(!cx.has_pending_prompt());
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\n");
}

#[gpui_kit::test]
fn a_reload_takes_the_files_new_encoding(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    std::fs::write(files.path(Side::Right), utf16("c\n")).expect("right");
    disk_changed(cx, &o, Side::Right);
    type_at(cx, &o, Side::Right, 0, "x");
    save(cx, &o);
    assert_eq!(std::fs::read(files.path(Side::Right)).expect("readable"), utf16("xc\n"));
}

#[gpui_kit::test]
fn the_same_text_in_a_new_encoding_reloads(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    std::fs::write(files.path(Side::Right), utf16("b\n")).expect("right");
    disk_changed(cx, &o, Side::Right);
    type_at(cx, &o, Side::Right, 0, "x");
    save(cx, &o);
    assert_eq!(std::fs::read(files.path(Side::Right)).expect("readable"), utf16("xb\n"));
}

/// Regions of these files: 0 a, 1 b→B, 2 c, 3 d→(none), 4 e. Bytes: left lines start at
/// 0 2 4 6 8, right lines at 0 2 4 6.
const FIVE_LEFT: &str = "a\nb\nc\nd\ne\n";
const FIVE_RIGHT: &str = "a\nB\nc\ne\n";

fn carets(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> (usize, usize) {
    cx.read(|cx| {
        let view = view.read(cx);
        let caret = |side| view.editor(side).read(cx).cursor();
        (caret(Side::Left), caret(Side::Right))
    })
}

/// Focus `side`'s pane with the caret at `at`.
fn focus_at(cx: &mut TestAppContext, o: &Opened, side: Side, at: usize) {
    act(cx, o.window, |window, cx| window.click(pane_id(side), cx));
    o.view.update(cx, |view, cx| {
        view.editor(side).update(cx, |state, cx| state.set_selected_range(at..at, cx));
    });
}

#[gpui_kit::test]
fn next_and_previous_difference_move_both_carets(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), (2, 2));
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), (6, 6));
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), (6, 6), "no change after the last");
    press(cx, &o, "secondary-up");
    assert_eq!(carets(cx, &o.view), (2, 2));
    press(cx, &o, "secondary-up");
    assert_eq!(carets(cx, &o.view), (2, 2), "no change before the first");
}

#[gpui_kit::test]
fn the_bracket_chords_go_to_differences(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-]");
    assert_eq!(carets(cx, &o.view), (2, 2));
    press(cx, &o, "secondary-]");
    assert_eq!(carets(cx, &o.view), (6, 6));
    press(cx, &o, "secondary-[");
    assert_eq!(carets(cx, &o.view), (2, 2));
}

#[gpui_kit::test]
fn an_empty_region_reached_by_navigation_stays_current(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Right, 0);
    press(cx, &o, "secondary-down");
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), (6, 6));
    // Right byte 6 starts line e, in region 4 by line; the pin keeps region 3 current.
    press(cx, &o, "secondary-up");
    assert_eq!(carets(cx, &o.view), (2, 2));
}

#[gpui_kit::test]
fn a_moved_caret_drops_the_pin(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Right, 0);
    press(cx, &o, "secondary-down");
    press(cx, &o, "secondary-down");
    // Caret on right line 0: region 0, so the next change is region 1.
    focus_at(cx, &o, Side::Right, 0);
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), (2, 2));
}

#[gpui_kit::test]
fn navigation_sees_edits_not_yet_recomputed(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nb\nc\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 4, "x");
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    cx.run_until_parked();
    assert_eq!(carets(cx, &o.view), (4, 4));
}

#[gpui_kit::test]
fn navigation_after_an_edit_waits_for_the_recompute_off_the_ui_thread(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nb\nc\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 4, "x");
    focus_at(cx, &o, Side::Left, 0);
    let before = carets(cx, &o.view);
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), before, "no diff ran on the UI thread");
    // No clock advance: navigation does not wait out the debounce.
    cx.run_until_parked();
    assert_eq!(carets(cx, &o.view), (4, 4));
    assert_eq!(
        regions(cx, &o.view),
        [region(Kind2::Unchanged, 0..2, 0..2), region(Kind2::Changed, 2..3, 2..3),]
    );
}

#[gpui_kit::test]
fn navigations_waiting_for_a_recompute_run_in_turn(cx: &mut TestAppContext) {
    // Left becomes a b xc d xe: changes on lines 2 and 4.
    let files = Files::new("a\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 8, "x");
    type_at(cx, &o, Side::Left, 4, "x");
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    press(cx, &o, "secondary-down");
    cx.run_until_parked();
    // Line 4 starts at left byte 9 (a b xc d) and right byte 8 (a b c d).
    assert_eq!(carets(cx, &o.view), (9, 8));
}

#[gpui_kit::test]
fn an_edit_drops_a_navigation_waiting_for_the_recompute(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nb\nc\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 4, "x");
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    type_at(cx, &o, Side::Left, 0, "y");
    wait(cx, DIFF_DEBOUNCE);
    let left = cx.read(|cx| o.view.read(cx).editor(Side::Left).read(cx).cursor());
    assert_eq!(left, 1, "the caret stays after the typed y");
}

const JJ_BANNER: &str =
    "These are temporary copies from jj diff --tool. Edits here are lost when the window closes.";

fn banner(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> Option<String> {
    cx.read(|cx| view.read(cx).banner().map(ToString::to_string))
}

#[gpui_kit::test]
fn files_from_jj_diff_tool_show_a_banner(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let jj = dir.path().join("jj-diff-x7Yq");
    let [left, right] = ["left", "right"].map(|side| {
        let path = jj.join(side).join("a.txt");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(&path, side).expect("written");
        DiffFile {
            origin: Origin::File(path),
            text: Text::decode(side.as_bytes()).expect("decodes"),
            read_only: false,
        }
    });
    let view = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::diff_window::open(left, right, &DiffSettings::default(), cx)
            .expect("window opens")
            .1
    });
    assert_eq!(banner(cx, &view).as_deref(), Some(JJ_BANNER));
}

/// A relative path resolves against the run's working directory, which the window is given,
/// not the process's.
#[gpui_kit::test]
fn a_relative_jj_copy_resolves_against_the_runs_directory(cx: &mut TestAppContext) {
    // jj runs the tool from its temporary directory's left side: only the working directory
    // makes `a.txt` a jj copy.
    let dir = tempfile::tempdir().expect("tempdir");
    let jj = dir.path().join("jj-diff-x7Yq");
    let [left, right] = [("left", "a.txt"), ("right", "../right/a.txt")].map(|(side, relative)| {
        let path = jj.join(side).join("a.txt");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(&path, side).expect("written");
        DiffFile {
            origin: Origin::File(PathBuf::from(relative)),
            text: Text::decode(side.as_bytes()).expect("decodes"),
            read_only: true,
        }
    });
    let settings =
        DiffSettings { cwd: Some(jj.join("left")), watch: false, ..DiffSettings::default() };
    let view = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::diff_window::open(left, right, &settings, cx).expect("window opens").1
    });
    assert_eq!(banner(cx, &view).as_deref(), Some(JJ_BANNER));
}

#[gpui_kit::test]
fn other_files_show_no_banner(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    assert_eq!(banner(cx, &o.view), None);
}

/// `side`'s spacer requests and display row count.
fn rows(
    cx: &mut TestAppContext,
    view: &Entity<DiffWindow>,
    side: Side,
) -> (Vec<(usize, usize)>, usize) {
    cx.read(|cx| {
        let map = view.read(cx).editor(side).read(cx).display_map();
        (map.spacers().to_vec(), map.display_row_count())
    })
}

#[gpui_kit::test]
fn the_shorter_side_of_a_change_gets_spacer_rows(cx: &mut TestAppContext) {
    // b against x y: left needs one row above c. Each editor has a last, empty line.
    let files = Files::new("a\nb\nc\n", "a\nx\ny\nc\n");
    let o = open(cx, &files);
    assert_eq!(rows(cx, &o.view, Side::Left), (vec![(2, 1)], 5));
    assert_eq!(rows(cx, &o.view, Side::Right), (vec![], 5));
}

#[gpui_kit::test]
fn spacers_follow_the_regions_once_typing_pauses(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nx\ny\nc\n");
    let o = open(cx, &files);
    // A line typed before c levels the change: a b q c against a x y c.
    type_at(cx, &o, Side::Left, 4, "q\n");
    wait(cx, JUST_BEFORE);
    assert_eq!(rows(cx, &o.view, Side::Left), (vec![(2, 1)], 6), "not yet");
    wait(cx, ONE_MS);
    assert_eq!(rows(cx, &o.view, Side::Left), (vec![], 5));
    assert_eq!(rows(cx, &o.view, Side::Right), (vec![], 5));
    // Two more lines on the left: the right is now short by two above c.
    type_at(cx, &o, Side::Left, 6, "r\ns\n");
    wait(cx, DIFF_DEBOUNCE);
    assert_eq!(rows(cx, &o.view, Side::Left), (vec![], 7));
    assert_eq!(rows(cx, &o.view, Side::Right), (vec![(3, 2)], 7));
}

#[gpui_kit::test]
fn an_insertion_at_the_end_puts_spacers_after_the_last_line(cx: &mut TestAppContext) {
    // Left "a" has one line and no newline; right has three.
    let files = Files::new("a", "a\nb\nc");
    let o = open(cx, &files);
    assert_eq!(rows(cx, &o.view, Side::Left), (vec![(1, 2)], 3));
    assert_eq!(rows(cx, &o.view, Side::Right), (vec![], 3));
}

#[gpui_kit::test]
fn the_panes_use_the_default_text_size(cx: &mut TestAppContext) {
    // 10 pt text at 1.5 lines (spec §6.4).
    const DEFAULT_ROW: gpui_kit::Pixels = gpui_kit::px(15.);
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    for side in SIDES_OF_TEST {
        let line = cx.read(|cx| o.view.read(cx).editor(side).read(cx).line_height());
        assert_eq!(line, Some(DEFAULT_ROW), "{side:?}");
    }
}

/// Frames drawn after a scroll, so a change made in one frame shows in the next.
const SETTLE_FRAMES: usize = 3;
const SCROLL_LINES: usize = 200;
/// Lines the left has above the right's first line, so the right starts with spacers.
const LEFT_ONLY: usize = 3;

fn numbered(from: usize) -> String {
    use std::fmt::Write as _;
    (from..SCROLL_LINES).fold(String::new(), |mut text, n| {
        writeln!(text, "l{n}").expect("a String takes any write");
        text
    })
}

/// Scroll the pane of `side` down by `by`, and draw until both panes settle.
fn wheel(cx: &mut TestAppContext, o: &Opened, side: Side, by: gpui_kit::Pixels) {
    use gpui_kit::{ScrollDelta, point, px};
    act(cx, o.window, |window, cx| {
        window.scroll(pane_id(side), ScrollDelta::Pixels(point(px(0.), -by)), cx);
    });
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
}

/// How far each pane is scrolled down, and the line height.
fn scroll_tops(
    cx: &mut TestAppContext,
    view: &Entity<DiffWindow>,
) -> (gpui_kit::Pixels, gpui_kit::Pixels, gpui_kit::Pixels) {
    cx.read(|cx| {
        let view = view.read(cx);
        let top = |side| -view.editor(side).read(cx).scroll_offset().y;
        let line = view.editor(Side::Left).read(cx).line_height().expect("laid out");
        (top(Side::Left), top(Side::Right), line)
    })
}

#[gpui_kit::test]
fn scrolling_one_pane_scrolls_the_other_to_the_same_row(cx: &mut TestAppContext) {
    let files = Files::new(&numbered(0), &numbered(LEFT_ONLY));
    let o = open(cx, &files);
    let (_, _, line) = scroll_tops(cx, &o.view);
    wheel(cx, &o, Side::Left, line * 10.);
    assert_eq!(scroll_tops(cx, &o.view), (line * 10., line * 10., line));
    wheel(cx, &o, Side::Right, line * 4.);
    assert_eq!(scroll_tops(cx, &o.view), (line * 14., line * 14., line));
    // Back up past the top: both stop at 0.
    wheel(cx, &o, Side::Right, line * -100.);
    assert_eq!(scroll_tops(cx, &o.view).0, gpui_kit::px(0.));
    assert_eq!(scroll_tops(cx, &o.view).1, gpui_kit::px(0.));
}

#[gpui_kit::test]
fn a_caret_that_scrolls_its_pane_scrolls_the_other(cx: &mut TestAppContext) {
    let files = Files::new(&numbered(0), &numbered(LEFT_ONLY));
    let o = open(cx, &files);
    // The right's last line, then one step left so the editor scrolls to the caret.
    let end = numbered(LEFT_ONLY).len();
    focus_at(cx, &o, Side::Right, end);
    press(cx, &o, "left");
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    let (left, right, _) = scroll_tops(cx, &o.view);
    assert_ne!(right, gpui_kit::px(0.), "the right scrolled to its caret");
    assert_eq!(left, right);
}

/// One line, the editors' line height (spec §6.4).
const LINE: gpui_kit::Pixels = gpui_kit::px(15.);

fn copy(cx: &mut TestAppContext, o: &Opened, region: RegionIx, to: Side) {
    act(cx, o.window, |window, cx| {
        window.click(copy_id(region, to), cx);
    });
}

fn has_copy(cx: &mut TestAppContext, o: &Opened, region: RegionIx, to: Side) -> bool {
    let mut found = false;
    act(cx, o.window, |window, _| {
        found = window.try_find(copy_id(region, to)).is_some();
    });
    found
}

/// The top of the copy button of `region` that copies to `to`.
fn copy_top(cx: &mut TestAppContext, o: &Opened, region: RegionIx, to: Side) -> gpui_kit::Pixels {
    let mut top = gpui_kit::px(0.);
    act(cx, o.window, |window, _| {
        top = window.find(copy_id(region, to)).bounds().top();
    });
    top
}

#[gpui_kit::test]
fn copy_to_the_left_takes_the_rights_lines(cx: &mut TestAppContext) {
    // Region 1: b against x y.
    let files = Files::new("a\nb\nc\n", "a\nx\ny\nc\n");
    let o = open(cx, &files);
    copy(cx, &o, RegionIx::new(1), Side::Left);
    let left = texts(cx, &o.view, Side::Left);
    assert_eq!(left, ("a\nx\ny\nc\n".to_owned(), "a\nx\ny\nc\n".to_owned()));
    assert_eq!(texts(cx, &o.view, Side::Right).0, "a\nx\ny\nc\n");
    let left_title = files.path(Side::Left).display().to_string() + MODIFIED;
    assert_eq!(title(cx, &o.view, Side::Left), left_title);
    let right_title = files.path(Side::Right).display().to_string();
    assert_eq!(title(cx, &o.view, Side::Right), right_title);
    wait(cx, DIFF_DEBOUNCE);
    assert_eq!(status(cx, &o.view), "no repo · 0 differences · whitespace significant");
    assert!(!has_copy(cx, &o, RegionIx::new(1), Side::Left), "no change left to copy");
}

#[gpui_kit::test]
fn copy_to_the_right_takes_the_lefts_lines(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nx\ny\nc\n");
    let o = open(cx, &files);
    copy(cx, &o, RegionIx::new(1), Side::Right);
    let right = texts(cx, &o.view, Side::Right);
    assert_eq!(right, ("a\nb\nc\n".to_owned(), "a\nb\nc\n".to_owned()));
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\nb\nc\n");
}

#[gpui_kit::test]
fn undo_reverses_a_copy(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nx\ny\nc\n");
    let o = open(cx, &files);
    copy(cx, &o, RegionIx::new(1), Side::Left);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-z");
    let left = texts(cx, &o.view, Side::Left);
    assert_eq!(left, ("a\nb\nc\n".to_owned(), "a\nb\nc\n".to_owned()));
}

#[gpui_kit::test]
fn only_changes_have_copy_buttons_and_never_into_a_read_only_side(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nx\ny\nc\n");
    let o = open_with(cx, &files, [true, false]);
    assert!(!has_copy(cx, &o, RegionIx::new(0), Side::Right), "region 0 is unchanged");
    assert!(!has_copy(cx, &o, RegionIx::new(2), Side::Right), "region 2 is unchanged");
    assert!(has_copy(cx, &o, RegionIx::new(1), Side::Right));
    assert!(!has_copy(cx, &o, RegionIx::new(1), Side::Left), "the left is read-only");
}

#[gpui_kit::test]
fn copy_buttons_sit_on_their_regions_first_row(cx: &mut TestAppContext) {
    // Rows: 0 a, 1 b|x (region 1), 2 spacer|y, 3 c, 4 d|D (region 3), 5 e.
    let files = Files::new("a\nb\nc\nd\ne\n", "a\nx\ny\nc\nD\ne\n");
    let o = open(cx, &files);
    let first = copy_top(cx, &o, RegionIx::new(1), Side::Left);
    assert_eq!(first, rows_top(cx, &o.view, Side::Left) + LINE);
    assert_eq!(copy_top(cx, &o, RegionIx::new(1), Side::Right), first);
    assert_eq!(copy_top(cx, &o, RegionIx::new(3), Side::Left) - first, LINE * 3.);
}

#[gpui_kit::test]
fn copy_buttons_follow_the_scroll(cx: &mut TestAppContext) {
    // A change at line 50: the button moves up by what the panes scroll.
    let mut right = numbered(0);
    right.replace_range(
        right.find("l50\n").expect("line 50")..right.find("l51\n").expect("line 51"),
        "X\n",
    );
    let files = Files::new(&numbered(0), &right);
    let o = open(cx, &files);
    let before = copy_top(cx, &o, RegionIx::new(1), Side::Left);
    wheel(cx, &o, Side::Left, LINE * 10.);
    assert_eq!(copy_top(cx, &o, RegionIx::new(1), Side::Left), before - LINE * 10.);
}

/// The top of `side`'s first row, unscrolled.
fn rows_top(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side) -> gpui_kit::Pixels {
    cx.read(|cx| view.read(cx).editor(side).read(cx).input_bounds().top())
}

#[gpui_kit::test]
fn an_insertions_button_sits_on_its_first_line_not_the_spacer(cx: &mut TestAppContext) {
    // Region 1 is b, on the right only. The left's spacer sits above its line 1, c.
    let files = Files::new("a\nc\n", "a\nb\nc\n");
    let o = open(cx, &files);
    let first = rows_top(cx, &o.view, Side::Left);
    assert_eq!(copy_top(cx, &o, RegionIx::new(1), Side::Left), first + LINE);
    assert_eq!(copy_top(cx, &o, RegionIx::new(1), Side::Right), first + LINE);
}

#[gpui_kit::test]
fn copy_buttons_wait_for_the_recompute_after_an_edit(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nx\ny\nc\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "z");
    act(cx, o.window, TestWindowExt::render_frame);
    assert!(!has_copy(cx, &o, RegionIx::new(1), Side::Left), "regions lag the text");
    wait(cx, DIFF_DEBOUNCE);
    act(cx, o.window, TestWindowExt::render_frame);
    // za against a is a change now: a b against x y joins it as region 0.
    assert!(has_copy(cx, &o, RegionIx::new(0), Side::Left));
}

#[gpui_kit::test]
fn copy_buttons_off_screen_are_not_built(cx: &mut TestAppContext) {
    const FAR: usize = 150;
    const FAR_LINES: f32 = 150.;
    let mut right = numbered(0);
    let at = right.find(&format!("l{FAR}\n")).expect("line");
    right.insert_str(at, "X\n");
    let files = Files::new(&numbered(0), &right);
    let o = open(cx, &files);
    // The window opens on the far change, so go back to the top first.
    wheel(cx, &o, Side::Left, -LINE * FAR_LINES);
    assert!(!has_copy(cx, &o, RegionIx::new(1), Side::Left));
    wheel(cx, &o, Side::Left, LINE * FAR_LINES);
    assert!(has_copy(cx, &o, RegionIx::new(1), Side::Left));
}

#[gpui_kit::test]
fn copy_buttons_sit_on_their_rows_below_a_banner(cx: &mut TestAppContext) {
    // jj's temporary directory brings the banner, which pushes the panes down.
    let dir = tempfile::tempdir().expect("tempdir");
    let jj = dir.path().join("jj-diff-x7Yq");
    let [left, right] = ["left", "right"].map(|side| {
        let path = jj.join(side).join("a.txt");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(&path, format!("same\n{side}\n")).expect("written");
        DiffFile {
            origin: Origin::File(path),
            text: Text::decode(format!("same\n{side}\n").as_bytes()).expect("decodes"),
            read_only: false,
        }
    });
    let (window, view, finished) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        let finished = Finished::install(cx);
        let (window, view) =
            merge_app::diff_window::open(left, right, &DiffSettings::default(), cx)
                .expect("window opens");
        (window, view, finished)
    });
    act(cx, window, TestWindowExt::render_frame);
    let o = Opened { window, view, finished };
    assert_eq!(banner(cx, &o.view).as_deref(), Some(JJ_BANNER));
    act(cx, o.window, TestWindowExt::render_frame);
    let first = rows_top(cx, &o.view, Side::Left);
    assert_eq!(copy_top(cx, &o, RegionIx::new(1), Side::Left), first + LINE);
}

fn open_set(cx: &mut TestAppContext, files: &Files, settings: &DiffSettings) -> Opened {
    let (left, right) = (files.file(Side::Left, false), files.file(Side::Right, false));
    let (window, view, finished) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        let finished = Finished::install(cx);
        let (window, view) =
            merge_app::diff_window::open(left, right, settings, cx).expect("window opens");
        (window, view, finished)
    });
    act(cx, window, TestWindowExt::render_frame);
    Opened { window, view, finished }
}

fn toggle_scroll_sync(cx: &mut TestAppContext, o: &Opened) {
    act(cx, o.window, |window, cx| {
        window.dispatch_action(Box::new(merge_app::ToggleScrollSync), cx);
    });
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
}

#[gpui_kit::test]
fn the_scroll_sync_toggle_leaves_the_diff_panes_scrolling_together(cx: &mut TestAppContext) {
    let o = open_set(cx, &Files::new(&numbered(0), &numbered(LEFT_ONLY)), &DiffSettings::default());
    // Spacer rows face each other only while the panes share a top (spec §5).
    toggle_scroll_sync(cx, &o);
    wheel(cx, &o, Side::Left, LINE * 10.);
    assert_eq!(scroll_tops(cx, &o.view), (LINE * 10., LINE * 10., LINE));
    toggle_scroll_sync(cx, &o);
    wheel(cx, &o, Side::Right, LINE * 4.);
    assert_eq!(scroll_tops(cx, &o.view), (LINE * 14., LINE * 14., LINE));
}

/// A sideways scroll well inside a long line.
const SIDEWAYS: Pixels = px(40.);
/// A sideways scroll past the end of any line.
const FAR: Pixels = px(100_000.);

/// `width` characters on line 1, after a line that differs on each side.
fn wide_file(first: &str, width: usize) -> String {
    format!("{first}\n{}\nend\n", "x".repeat(width))
}

/// How far `side`'s pane is scrolled right.
fn pane_left(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side) -> Pixels {
    cx.read(|cx| -view.read(cx).editor(side).read(cx).scroll_offset().x)
}

/// Scroll `side`'s pane `by` to the right, as its scrollbar does, and let the panes settle.
fn scroll_right(cx: &mut TestAppContext, o: &Opened, side: Side, by: Pixels) {
    o.view.update(cx, |view, cx| {
        view.editor(side).update(cx, |state, cx| {
            let offset = state.scroll_offset();
            state.set_scroll_offset(point(offset.x - by, offset.y), cx);
        });
    });
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
}

fn toggle_soft_wrap(cx: &mut TestAppContext, o: &Opened) {
    act(cx, o.window, |window, cx| {
        window.dispatch_action(Box::new(merge_app::ToggleSoftWrap), cx);
    });
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
}

#[gpui_kit::test]
fn the_panes_scroll_sideways_by_default(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(&wide_file("a", 400), &wide_file("b", 400)));
    scroll_right(cx, &o, Side::Left, SIDEWAYS);
    assert_eq!(pane_left(cx, &o.view, Side::Left), SIDEWAYS);
}

#[gpui_kit::test]
fn the_soft_wrap_setting_wraps_both_panes(cx: &mut TestAppContext) {
    let settings = DiffSettings { soft_wrap: true, ..DiffSettings::default() };
    let o = open_set(cx, &Files::new(&wide_file("a", 400), &wide_file("b", 400)), &settings);
    for side in SIDES_OF_TEST {
        scroll_right(cx, &o, side, SIDEWAYS);
        assert_eq!(pane_left(cx, &o.view, side), px(0.), "{side:?}");
    }
}

#[gpui_kit::test]
fn the_action_turns_soft_wrap_on_and_off_in_both_panes(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(&wide_file("a", 400), &wide_file("b", 400)));
    toggle_soft_wrap(cx, &o);
    for side in SIDES_OF_TEST {
        scroll_right(cx, &o, side, SIDEWAYS);
        assert_eq!(pane_left(cx, &o.view, side), px(0.), "{side:?} wrapped");
    }
    toggle_soft_wrap(cx, &o);
    scroll_right(cx, &o, Side::Right, SIDEWAYS);
    assert_eq!(pane_left(cx, &o.view, Side::Right), SIDEWAYS, "unwrapped");
}

#[gpui_kit::test]
fn scrolling_one_pane_sideways_moves_the_other(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(&wide_file("a", 400), &wide_file("b", 400)));
    scroll_right(cx, &o, Side::Left, SIDEWAYS);
    assert_eq!(pane_left(cx, &o.view, Side::Right), SIDEWAYS);
    scroll_right(cx, &o, Side::Right, SIDEWAYS);
    assert_eq!(pane_left(cx, &o.view, Side::Left), SIDEWAYS * 2.);
}

#[gpui_kit::test]
fn a_pane_held_at_its_end_does_not_pull_the_other_back(cx: &mut TestAppContext) {
    // The right side is a quarter as wide: it stops well short of the left's end.
    let o = open(cx, &Files::new(&wide_file("a", 400), &wide_file("b", 100)));
    scroll_right(cx, &o, Side::Left, FAR);
    let left = pane_left(cx, &o.view, Side::Left);
    let right = pane_left(cx, &o.view, Side::Right);
    assert!(right < left, "the right stops first: {right:?} < {left:?}");
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    assert_eq!(
        (pane_left(cx, &o.view, Side::Left), pane_left(cx, &o.view, Side::Right)),
        (left, right)
    );
}

#[gpui_kit::test]
fn the_scroll_sync_toggle_leaves_the_diff_panes_scrolling_sideways_together(
    cx: &mut TestAppContext,
) {
    let o = open_set(
        cx,
        &Files::new(&wide_file("a", 400), &wide_file("b", 400)),
        &DiffSettings::default(),
    );
    toggle_scroll_sync(cx, &o);
    scroll_right(cx, &o, Side::Left, SIDEWAYS);
    assert_eq!(
        (pane_left(cx, &o.view, Side::Left), pane_left(cx, &o.view, Side::Right)),
        (SIDEWAYS, SIDEWAYS)
    );
}

#[gpui_kit::test]
fn a_larger_text_size_makes_taller_lines(cx: &mut TestAppContext) {
    // 13 pt at 1.5 lines is 19.5, which GPUI rounds to 20.
    let settings = DiffSettings {
        typography: merge_app::Typography {
            size: gpui_kit::px(13.),
            ..merge_app::Typography::default()
        },
        ..DiffSettings::default()
    };
    let o = open_set(cx, &Files::new("a\n", "b\n"), &settings);
    for side in SIDES_OF_TEST {
        let line = cx.read(|cx| o.view.read(cx).editor(side).read(cx).line_height());
        assert_eq!(line, Some(gpui_kit::px(20.)), "{side:?}");
    }
}

#[gpui_kit::test]
fn a_line_height_of_2_makes_lines_twice_the_text_size(cx: &mut TestAppContext) {
    let settings = DiffSettings {
        typography: merge_app::Typography { spacing: 2., ..merge_app::Typography::default() },
        ..DiffSettings::default()
    };
    let o = open_set(cx, &Files::new("a\n", "b\n"), &settings);
    for side in SIDES_OF_TEST {
        let line = cx.read(|cx| o.view.read(cx).editor(side).read(cx).line_height());
        assert_eq!(line, Some(gpui_kit::px(20.)), "{side:?}");
    }
}

#[gpui_kit::test]
fn a_dark_theme_setting_opens_the_diff_window_dark(cx: &mut TestAppContext) {
    use gpui_kit::component::ActiveTheme as _;
    let settings =
        DiffSettings { theme: merge_config::settings::Theme::Dark, ..DiffSettings::default() };
    let _o = open_set(cx, &Files::new("a\n", "b\n"), &settings);
    assert!(cx.read(|cx| cx.theme().is_dark()));
}

#[gpui_kit::test]
fn the_tab_key_types_the_tab_width_in_spaces(cx: &mut TestAppContext) {
    const TAB_WIDTH: usize = 6;
    let settings = DiffSettings {
        typography: merge_app::Typography {
            tab_width: TAB_WIDTH,
            ..merge_app::Typography::default()
        },
        ..DiffSettings::default()
    };
    let o = open_set(cx, &Files::new("a\n", "b\n"), &settings);
    for side in SIDES_OF_TEST {
        focus_at(cx, &o, side, 0);
        press(cx, &o, "tab");
    }
    assert_eq!(texts(cx, &o.view, Side::Left), ("      a\n".to_owned(), "      a\n".to_owned()));
    assert_eq!(texts(cx, &o.view, Side::Right), ("      b\n".to_owned(), "      b\n".to_owned()));
}

/// The role and label of the element `id`.
fn named(
    cx: &mut TestAppContext,
    o: &Opened,
    id: impl Into<gpui_kit::ElementId>,
) -> (Option<gpui_kit::Role>, Option<String>) {
    let id = id.into();
    let mut found = None;
    act(cx, o.window, |window, _| {
        let element = window.find(id);
        found = Some((element.role(), element.label().map(str::to_owned)));
    });
    found.expect("the window ran")
}

#[gpui_kit::test]
fn the_panes_copy_buttons_and_status_are_named(cx: &mut TestAppContext) {
    use merge_app::diff_window::STATUS_ID;

    let files = Files::new("a\nb\n", "a\nB\n");
    let o = open(cx, &files);
    for side in SIDES_OF_TEST {
        let editor = cx.read(|cx| o.view.read(cx).editor(side).entity_id());
        let path = files.path(side);
        assert_eq!(
            named(cx, &o, ("input", editor)),
            (Some(Role::MultilineTextInput), Some(format!("{side:?}: {}", path.display())))
        );
    }
    assert_eq!(
        named(cx, &o, copy_id(RegionIx::new(1), Side::Right)),
        (Some(Role::Button), Some("Copy this difference to the right".to_owned()))
    );
    assert_eq!(
        named(cx, &o, copy_id(RegionIx::new(1), Side::Left)),
        (Some(Role::Button), Some("Copy this difference to the left".to_owned()))
    );
    assert_eq!(
        named(cx, &o, STATUS_ID),
        (Some(Role::Status), Some("no repo · 1 difference · whitespace significant".to_owned()))
    );
}

#[gpui_kit::test]
fn the_jj_banner_is_an_alert(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let jj = dir.path().join("jj-diff-x7Yq");
    let [left, right] = ["left", "right"].map(|side| {
        let path = jj.join(side).join("a.txt");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        std::fs::write(&path, side).expect("written");
        DiffFile {
            origin: Origin::File(path),
            text: Text::decode(side.as_bytes()).expect("decodes"),
            read_only: false,
        }
    });
    let (window, view, finished) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        let finished = Finished::install(cx);
        let (window, view) =
            merge_app::diff_window::open(left, right, &DiffSettings::default(), cx)
                .expect("window opens");
        (window, view, finished)
    });
    act(cx, window, TestWindowExt::render_frame);
    let o = Opened { window, view, finished };
    assert_eq!(
        named(cx, &o, merge_app::diff_window::BANNER_ID),
        (Some(gpui_kit::Role::Alert), Some(JJ_BANNER.to_owned()))
    );
}

const SHRINK_LEFT: &str = "a\nb\nc\nd\ne\n";
const SHRINK_RIGHT: &str = "a\nB\nc\nD\ne\n";

#[gpui_kit::test]
fn regions_left_past_the_end_by_a_deletion_are_harmless_until_the_recompute(
    cx: &mut TestAppContext,
) {
    let o = open(cx, &Files::new(SHRINK_LEFT, SHRINK_RIGHT));
    // Delete left's lines b to e, bytes 2..10. Regions 3 and 4 now lie past left's end.
    focus_at(cx, &o, Side::Left, 2);
    o.view.update(cx, |view, cx| {
        view.editor(Side::Left).update(cx, |state, cx| state.set_selected_range(2..10, cx));
    });
    press(cx, &o, "backspace");
    act(cx, o.window, TestWindowExt::render_frame);
    assert_eq!(texts(cx, &o.view, Side::Left), ("a\n".into(), "a\n".into()));
    assert!(
        !has_copy(cx, &o, RegionIx::new(1), Side::Left)
            && !has_copy(cx, &o, RegionIx::new(3), Side::Left),
        "regions that lag the text offer no copy"
    );
    wait(cx, DIFF_DEBOUNCE);
    assert_eq!(
        regions(cx, &o.view),
        [region(Kind2::Unchanged, 0..1, 0..1), region(Kind2::Changed, 1..1, 1..5),]
    );
    copy(cx, &o, RegionIx::new(1), Side::Left);
    assert_eq!(texts(cx, &o.view, Side::Left), (SHRINK_RIGHT.into(), SHRINK_RIGHT.into()));
}

#[gpui_kit::test]
fn navigation_from_a_pin_past_the_end_after_an_undo(cx: &mut TestAppContext) {
    // Typing two lines at the end of left adds region 5; navigation pins it. Undo removes
    // the lines, so the pin's region and caret lie past the text until the recompute.
    let o = open(cx, &Files::new(SHRINK_LEFT, SHRINK_RIGHT));
    type_at(cx, &o, Side::Left, 10, "f\ng\n");
    wait(cx, DIFF_DEBOUNCE);
    focus_at(cx, &o, Side::Left, 0);
    for _ in 0..3 {
        press(cx, &o, "secondary-down");
    }
    assert_eq!(carets(cx, &o.view), (10, 10), "the added lines' region");
    press(cx, &o, "secondary-z");
    assert_eq!(texts(cx, &o.view, Side::Left), (SHRINK_LEFT.into(), SHRINK_LEFT.into()));
    press(cx, &o, "secondary-up");
    cx.run_until_parked();
    assert_eq!(carets(cx, &o.view), (6, 6), "region 3, the last change before the end");
}

#[gpui_kit::test]
fn navigation_keeps_typing_in_the_pane_it_started_from(cx: &mut TestAppContext) {
    for side in SIDES_OF_TEST {
        let o = open(cx, &Files::new(SHRINK_LEFT, SHRINK_RIGHT));
        focus_at(cx, &o, side, 0);
        press(cx, &o, "secondary-down");
        assert_eq!(carets(cx, &o.view), (2, 2));
        act(cx, o.window, |window, cx| window.input("z", cx));
        let (typed, other) = match side {
            Side::Left => ("a\nzb\nc\nd\ne\n", SHRINK_RIGHT),
            Side::Right => ("a\nzB\nc\nD\ne\n", SHRINK_LEFT),
        };
        assert_eq!(texts(cx, &o.view, side).0, typed, "{side:?} takes the typing");
        assert_eq!(texts(cx, &o.view, side.other()).0, other);
        press(cx, &o, "secondary-z");
        let original = match side {
            Side::Left => SHRINK_LEFT,
            Side::Right => SHRINK_RIGHT,
        };
        assert_eq!(texts(cx, &o.view, side).0, original, "undo acts on {side:?}");
    }
}

#[gpui_kit::test]
fn the_window_opens_on_the_first_difference(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(FIVE_LEFT, FIVE_RIGHT));
    assert_eq!(carets(cx, &o.view), (2, 2));
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), (6, 6), "the next is the second");
}

#[gpui_kit::test]
fn the_window_opens_with_the_focus_in_the_left_pane(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SHRINK_LEFT, SHRINK_RIGHT));
    act(cx, o.window, |window, cx| window.input("z", cx));
    assert_eq!(texts(cx, &o.view, Side::Left).0, "a\nzb\nc\nd\ne\n");
    assert_eq!(texts(cx, &o.view, Side::Right).0, SHRINK_RIGHT);
}

#[gpui_kit::test]
fn a_change_on_the_first_line_is_the_first_difference(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new("x\nb\nc\n", "y\nb\nC\n"));
    assert_eq!(carets(cx, &o.view), (0, 0));
    press(cx, &o, "secondary-down");
    assert_eq!(carets(cx, &o.view), (4, 4), "region 0 is current");
}

#[gpui_kit::test]
fn an_insertion_opens_with_the_caret_where_it_goes_on_the_other_side(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new("a\nc\n", "a\nb\nc\n"));
    assert_eq!(carets(cx, &o.view), (2, 2));
    press(cx, &o, "secondary-up");
    assert_eq!(carets(cx, &o.view), (2, 2), "nothing changes before it");
}

#[gpui_kit::test]
fn identical_files_open_at_the_start(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new("a\nb\n", "a\nb\n"));
    assert_eq!(carets(cx, &o.view), (0, 0));
}

/// `numbered`'s text with line `line` replaced.
fn numbered_with_change(line: u16) -> String {
    numbered(0).replace(&format!("l{line}\n"), "changed\n")
}

/// Line 150 lies well below the first window of lines.
const FAR_LINE: u16 = 150;

#[gpui_kit::test]
fn a_far_first_difference_is_scrolled_into_view_in_both_panes(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(&numbered(0), &numbered_with_change(FAR_LINE)));
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    let (left, right, line) = scroll_tops(cx, &o.view);
    assert_eq!(left, right, "the panes show the same rows");
    let height = cx.read(|cx| {
        let view = o.view.read(cx);
        view.editor(Side::Left).read(cx).input_bounds().size.height
    });
    let row = line * f32::from(FAR_LINE);
    assert!(left <= row && row + line <= left + height, "row at {row:?} in {left:?} + {height:?}");
}

/// The tools the diff window has, with their accessible labels.
const DIFF_TOOLS: [(Tool, &str); 7] = [
    (Tool::PreviousDifference, "Previous difference"),
    (Tool::NextDifference, "Next difference"),
    (Tool::Undo, "Undo"),
    (Tool::Redo, "Redo"),
    (Tool::Find, "Find"),
    (Tool::Syntax, "Turn syntax colours on or off"),
    (Tool::SoftWrap, "Turn soft wrap on or off"),
];

#[gpui_kit::test]
fn the_toolbar_names_its_tools(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(FIVE_LEFT, FIVE_RIGHT));
    assert_eq!(named(cx, &o, TOOLBAR_ID), (Some(Role::Toolbar), Some("Diff tools".to_owned())));
    for (tool, label) in DIFF_TOOLS {
        assert_eq!(named(cx, &o, tool.id()), (Some(Role::Button), Some(label.to_owned())));
    }
}

#[gpui_kit::test]
fn the_toolbar_labels_only_the_navigation_pair(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(FIVE_LEFT, FIVE_RIGHT));
    let shown = |cx: &mut TestAppContext, tool: Tool| {
        let mut shown = false;
        act(cx, o.window, |window, _| {
            shown = window.try_find(tool.label_id()).is_some();
        });
        shown
    };
    assert!(shown(cx, Tool::PreviousDifference));
    for tool in
        [Tool::NextDifference, Tool::Undo, Tool::Redo, Tool::Find, Tool::Syntax, Tool::SoftWrap]
    {
        assert!(!shown(cx, tool), "{tool:?}");
    }
}

#[gpui_kit::test]
fn toolbar_navigation_moves_the_carets_and_keeps_the_focus(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SHRINK_LEFT, SHRINK_RIGHT));
    focus_at(cx, &o, Side::Right, 2);
    let click = |cx: &mut TestAppContext, tool: Tool| {
        act(cx, o.window, |window, cx| window.click(tool.id(), cx));
    };
    click(cx, Tool::NextDifference);
    assert_eq!(carets(cx, &o.view), (6, 6));
    click(cx, Tool::PreviousDifference);
    assert_eq!(carets(cx, &o.view), (2, 2));
    click(cx, Tool::NextDifference);
    act(cx, o.window, |window, cx| window.input("z", cx));
    assert_eq!(texts(cx, &o.view, Side::Right).0, "a\nB\nc\nzD\ne\n");
    assert_eq!(texts(cx, &o.view, Side::Left).0, SHRINK_LEFT);
}

#[gpui_kit::test]
fn toolbar_undo_and_redo_act_on_the_focused_pane(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SHRINK_LEFT, SHRINK_RIGHT));
    type_at(cx, &o, Side::Right, 0, "z");
    act(cx, o.window, |window, cx| {
        window.click(Tool::Undo.id(), cx);
    });
    assert_eq!(texts(cx, &o.view, Side::Right).0, SHRINK_RIGHT);
    act(cx, o.window, |window, cx| {
        window.click(Tool::Redo.id(), cx);
    });
    assert_eq!(texts(cx, &o.view, Side::Right).0, "za\nB\nc\nD\ne\n");
    assert_eq!(texts(cx, &o.view, Side::Left).0, SHRINK_LEFT);
}

#[gpui_kit::test]
fn settings_errors_show_in_the_diff_window_and_close(cx: &mut TestAppContext) {
    use merge_app::settings_banner::{BANNER_ID, CLOSE_ID};
    let o = open(cx, &Files::new("a\n", "b\n"));
    let errors = vec!["/c/config.triad:1: expected `key = value`".to_owned()];
    o.view.update(cx, |view, cx| view.set_settings_errors(&errors, cx));
    act(cx, o.window, TestWindowExt::render_frame);
    assert_eq!(
        named(cx, &o, BANNER_ID),
        (Some(gpui_kit::Role::Alert), Some(format!("The config file has errors: {}", errors[0])))
    );
    assert_eq!(
        cx.read(|cx| o.view.read(cx).settings_banner().map(ToString::to_string)),
        Some(errors[0].clone())
    );
    act(cx, o.window, |window, cx| window.click(CLOSE_ID, cx));
    assert_eq!(cx.read(|cx| o.view.read(cx).settings_banner().cloned()), None);
    o.view.update(cx, |view, cx| view.set_settings_errors(&errors, cx));
    o.view.update(cx, |view, cx| view.set_settings_errors(&[], cx));
    assert_eq!(cx.read(|cx| o.view.read(cx).settings_banner().cloned()), None);
}

/// `l0` to `l999`, with line 500 changed on the right.
fn thousand(changed: bool) -> String {
    (0..1000)
        .map(|n| if changed && n == 500 { "X\n".to_owned() } else { format!("l{n}\n") })
        .collect()
}

/// Across the ruler, inside its 12 px.
const ACROSS: Pixels = px(6.);
/// 10 pt text at 1.5 lines (spec §6.4).
const DIFF_LINE_HEIGHT: f32 = 15.;

/// Click the right pane's ruler `line` lines of its 1000 down.
fn click_ruler(cx: &mut TestAppContext, o: &Opened, line: f32) {
    act(cx, o.window, |window, cx| {
        let height = f32::from(window.find(DIFF_RULER_ID).bounds().size.height);
        window.click_at(DIFF_RULER_ID, point(ACROSS, px(line / 1000. * height)), cx);
    });
    for _ in 0..3 {
        act(cx, o.window, TestWindowExt::render_frame);
    }
}

#[gpui_kit::test]
fn the_ruler_goes_to_a_marked_region_and_scrolls_elsewhere(cx: &mut TestAppContext) {
    let files = Files::new(&thousand(false), &thousand(true));
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Right, 0);
    click_ruler(cx, &o, 500.5);
    // Lines l0-l9 take 3 bytes, l10-l99 4 and l100-l499 5: line 500 starts at byte 2390.
    assert_eq!(carets(cx, &o.view), (2390, 2390));
    focus_at(cx, &o, Side::Right, 0);
    click_ruler(cx, &o, 100.5);
    let (top, view) = cx.read(|cx| {
        let state = o.view.read(cx).editor(Side::Right).read(cx);
        (-state.scroll_offset().y, f32::from(state.input_bounds().size.height))
    });
    // Line 100 in the middle of the view; the carets stay.
    assert_eq!(top, px(100. * DIFF_LINE_HEIGHT - view / 2.));
    assert_eq!(carets(cx, &o.view).1, 0);
}

/// Inside the left pane's vertical bar: its thumb is 6 px wide, 4 px in from the pane's edge.
const BAR_FROM_RIGHT: Pixels = px(7.);

/// Click `side`'s pane `from_right` in from its right edge and `from_bottom` up from its foot.
fn click_pane(
    cx: &mut TestAppContext,
    o: &Opened,
    side: Side,
    from_right: Pixels,
    from_bottom: Pixels,
) {
    act(cx, o.window, |window, cx| {
        let pane = window.find(pane_id(side)).bounds().size;
        window.click_at(
            pane_id(side),
            point(pane.width - from_right, pane.height - from_bottom),
            cx,
        );
    });
    for _ in 0..3 {
        act(cx, o.window, TestWindowExt::render_frame);
    }
}

fn pane_top_and_view(cx: &mut TestAppContext, o: &Opened, side: Side) -> (Pixels, Pixels) {
    cx.read(|cx| {
        let state = o.view.read(cx).editor(side).read(cx);
        (-state.scroll_offset().y, state.input_bounds().size.height)
    })
}

#[gpui_kit::test]
fn the_left_panes_bar_shows_by_default_so_a_click_at_its_foot_scrolls_to_the_end(
    cx: &mut TestAppContext,
) {
    let files = Files::new(&thousand(false), &thousand(false));
    let o = open(cx, &files);
    // The end of the scroll, where the wheel stops.
    act(cx, o.window, |window, cx| {
        window.scroll(pane_id(Side::Left), ScrollDelta::Pixels(point(px(0.), px(-1_000_000.))), cx);
    });
    for _ in 0..3 {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    let end = pane_top_and_view(cx, &o, Side::Left).0;
    o.view.update(cx, |view, cx| {
        view.editor(Side::Left).update(cx, |state, cx| {
            state.set_scroll_offset(point(Pixels::ZERO, Pixels::ZERO), cx);
        });
    });
    for _ in 0..3 {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    assert_eq!(pane_top_and_view(cx, &o, Side::Left).0, Pixels::ZERO);
    click_pane(cx, &o, Side::Left, BAR_FROM_RIGHT, px(1.));
    assert_eq!(pane_top_and_view(cx, &o, Side::Left).0, end);
}

#[gpui_kit::test]
fn the_right_panes_vertical_bar_is_its_ruler(cx: &mut TestAppContext) {
    let files = Files::new(&thousand(false), &thousand(false));
    let o = open(cx, &files);
    // Where the right pane's own vertical bar would take a click halfway down; the caret
    // lands in view, so nothing else scrolls.
    let half = pane_top_and_view(cx, &o, Side::Right).1 / 2.;
    click_pane(cx, &o, Side::Right, BAR_FROM_RIGHT, half);
    assert_eq!(pane_top_and_view(cx, &o, Side::Right).0, Pixels::ZERO);
}

/// After a jump the region's frame blinks for 500 ms in each pane where the region has
/// lines: shown and hidden in turn, each for `BLINK`, starting shown and ending hidden.
const BLINK: Duration = Duration::from_millis(100);
/// The blink's samples: at the jump, then after each `BLINK`.
const BLINK_SAMPLES: usize = 6;
/// No frame in either pane.
const NO_FRAMES: [Option<Range<usize>>; 2] = [None, None];

/// The framed byte range of the left and right panes. A pane holds one frame at most.
fn frames(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> [Option<Range<usize>>; 2] {
    cx.read(|cx| {
        SIDES_OF_TEST.map(|side| {
            let frames = view.read(cx).flash_frames(side, cx);
            assert!(frames.len() <= 1, "{side:?} has frames {frames:?}");
            frames.into_iter().next()
        })
    })
}

fn flashing(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> bool {
    cx.read(|cx| view.read(cx).flashing())
}

/// The frames at the jump and after each `BLINK` that follows.
fn blink_samples(
    cx: &mut TestAppContext,
    view: &Entity<DiffWindow>,
) -> Vec<[Option<Range<usize>>; 2]> {
    let mut seen = vec![frames(cx, view)];
    for _ in 1..BLINK_SAMPLES {
        wait(cx, BLINK);
        seen.push(frames(cx, view));
    }
    seen
}

#[gpui_kit::test]
fn next_difference_blinks_the_region_in_both_panes(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    assert_eq!(frames(cx, &o.view), NO_FRAMES, "no jump yet");
    assert!(!flashing(cx, &o.view));
    press(cx, &o, "secondary-down");
    assert!(flashing(cx, &o.view));
    // Region 1: b\n at left 2..4, B\n at right 2..4.
    let on = [Some(2..4), Some(2..4)];
    assert_eq!(
        blink_samples(cx, &o.view),
        [on.clone(), NO_FRAMES, on.clone(), NO_FRAMES, on, NO_FRAMES]
    );
    assert!(!flashing(cx, &o.view), "the blink ends after 500 ms");
}

#[gpui_kit::test]
fn previous_difference_blinks_the_region_it_reaches(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    press(cx, &o, "secondary-down");
    wait(cx, BLINK * 5);
    press(cx, &o, "secondary-up");
    assert_eq!(frames(cx, &o.view), [Some(2..4), Some(2..4)]);
    assert!(flashing(cx, &o.view));
}

#[gpui_kit::test]
fn a_region_empty_on_one_side_blinks_on_the_other_alone(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    press(cx, &o, "secondary-down");
    // Region 3: d\n at left 6..8, nothing on the right.
    assert_eq!(frames(cx, &o.view), [Some(6..8), None]);
}

#[gpui_kit::test]
fn a_second_jump_restarts_the_blink(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    wait(cx, BLINK);
    press(cx, &o, "secondary-down");
    // 500 ms after the first jump, 400 ms after the second: the second's last shown step.
    wait(cx, BLINK * 4);
    assert!(flashing(cx, &o.view), "the first jump's timers are gone");
    assert_eq!(frames(cx, &o.view), [Some(6..8), None]);
    wait(cx, BLINK);
    assert!(!flashing(cx, &o.view));
    assert_eq!(frames(cx, &o.view), NO_FRAMES);
}

#[gpui_kit::test]
fn a_step_from_an_unchanged_region_that_finds_no_difference_does_not_blink(
    cx: &mut TestAppContext,
) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-up");
    assert!(!flashing(cx, &o.view), "nothing before a");
    assert_eq!(frames(cx, &o.view), NO_FRAMES);
    // Left byte 8 starts e, the unchanged region after the last difference.
    focus_at(cx, &o, Side::Left, 8);
    press(cx, &o, "secondary-down");
    assert!(!flashing(cx, &o.view), "nothing after e");
    assert_eq!(frames(cx, &o.view), NO_FRAMES);
}

#[gpui_kit::test]
fn a_step_past_the_last_difference_blinks_the_current_one(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    press(cx, &o, "secondary-down");
    press(cx, &o, "secondary-down");
    wait(cx, BLINK * 5);
    press(cx, &o, "secondary-down");
    assert!(flashing(cx, &o.view));
    assert_eq!(frames(cx, &o.view), [Some(6..8), None]);
    assert_eq!(carets(cx, &o.view), (6, 6), "the carets stay on it");
}

#[gpui_kit::test]
fn with_one_difference_both_steps_blink_it(cx: &mut TestAppContext) {
    let files = Files::new("a\nb\nc\n", "a\nB\nc\n");
    // The window opens with both carets on the difference.
    let o = open(cx, &files);
    press(cx, &o, "secondary-down");
    assert_eq!(frames(cx, &o.view), [Some(2..4), Some(2..4)]);
    wait(cx, BLINK * 5);
    assert!(!flashing(cx, &o.view));
    press(cx, &o, "secondary-up");
    assert_eq!(frames(cx, &o.view), [Some(2..4), Some(2..4)]);
}

#[gpui_kit::test]
fn the_toolbars_next_button_blinks_the_region(cx: &mut TestAppContext) {
    let files = Files::new(FIVE_LEFT, FIVE_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Left, 0);
    act(cx, o.window, |window, cx| {
        window.click(Tool::NextDifference.id(), cx);
    });
    assert!(flashing(cx, &o.view));
    assert_eq!(frames(cx, &o.view), [Some(2..4), Some(2..4)]);
}

#[gpui_kit::test]
fn a_ruler_mark_blinks_its_region(cx: &mut TestAppContext) {
    let files = Files::new(&thousand(false), &thousand(true));
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Right, 0);
    click_ruler(cx, &o, 500.5);
    assert!(flashing(cx, &o.view));
    // Line 500 starts at byte 2390: l500\n on the left, X\n on the right.
    assert_eq!(frames(cx, &o.view), [Some(2390..2395), Some(2390..2392)]);
}

// Search and replace (spec §5, Two-way mode): search in both panes, replace in the right.

const SEARCH_LEFT: &str = "a\nb\nb\n";
const SEARCH_RIGHT: &str = "b\nc\nb\n";

/// Replace every match of `query` in `side`'s pane with `with`; how many it replaced.
fn replace_all(
    cx: &mut TestAppContext,
    o: &Opened,
    side: Side,
    query: &str,
    case_insensitive: bool,
    with: &str,
) -> usize {
    let editor = cx.read(|cx| o.view.read(cx).editor(side).clone());
    let mut replaced = 0;
    act(cx, o.window, |window, cx| {
        editor.update(cx, |state, cx| {
            state.set_search_query(query, case_insensitive, cx);
            replaced = state.replace_all_search_matches(with, window, cx);
        });
    });
    replaced
}

fn replaceable(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> [bool; 2] {
    cx.read(|cx| {
        let view = view.read(cx);
        SIDES_OF_TEST.map(|side| view.editor(side).read(cx).is_replaceable())
    })
}

#[gpui_kit::test]
fn the_left_pane_refuses_a_replacement(cx: &mut TestAppContext) {
    let files = Files::new(SEARCH_LEFT, SEARCH_RIGHT);
    let o = open(cx, &files);
    assert_eq!(replaceable(cx, &o.view), [false, true]);
    assert_eq!(replace_all(cx, &o, Side::Left, "b", false, "Z"), 0);
    assert_eq!(texts(cx, &o.view, Side::Left), (SEARCH_LEFT.to_owned(), SEARCH_LEFT.to_owned()));
    assert_eq!(title(cx, &o.view, Side::Left), files.path(Side::Left).display().to_string());
}

#[gpui_kit::test]
fn the_right_pane_replaces_through_the_core_and_undo_restores(cx: &mut TestAppContext) {
    let files = Files::new(SEARCH_LEFT, SEARCH_RIGHT);
    let o = open(cx, &files);
    focus_at(cx, &o, Side::Right, 0);
    assert_eq!(replace_all(cx, &o, Side::Right, "b", false, "Z"), 2);
    assert_eq!(texts(cx, &o.view, Side::Right), ("Z\nc\nZ\n".to_owned(), "Z\nc\nZ\n".to_owned()));
    assert_eq!(texts(cx, &o.view, Side::Left).0, SEARCH_LEFT);
    press(cx, &o, "secondary-z");
    assert_eq!(texts(cx, &o.view, Side::Right), (SEARCH_RIGHT.to_owned(), SEARCH_RIGHT.to_owned()));
}

#[gpui_kit::test]
fn a_replacement_follows_the_case_setting(cx: &mut TestAppContext) {
    let files = Files::new(SEARCH_LEFT, SEARCH_RIGHT);
    let o = open(cx, &files);
    assert_eq!(replace_all(cx, &o, Side::Right, "B", false, "Z"), 0);
    assert_eq!(texts(cx, &o.view, Side::Right).0, SEARCH_RIGHT);
    assert_eq!(replace_all(cx, &o, Side::Right, "B", true, "Z"), 2);
    assert_eq!(texts(cx, &o.view, Side::Right).0, "Z\nc\nZ\n");
}

#[gpui_kit::test]
fn a_read_only_right_pane_refuses_a_replacement(cx: &mut TestAppContext) {
    let files = Files::new(SEARCH_LEFT, SEARCH_RIGHT);
    let o = open_with(cx, &files, [false, true]);
    assert_eq!(replaceable(cx, &o.view), [false, false]);
    assert_eq!(replace_all(cx, &o, Side::Right, "b", false, "Z"), 0);
    assert_eq!(texts(cx, &o.view, Side::Right).0, SEARCH_RIGHT);
}

/// Whether each pane's search is open.
fn searching(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> [bool; 2] {
    cx.read(|cx| {
        let view = view.read(cx);
        SIDES_OF_TEST.map(|side| view.editor(side).read(cx).search_session().open)
    })
}

#[gpui_kit::test]
fn find_opens_the_focused_panes_search(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SEARCH_LEFT, SEARCH_RIGHT));
    focus_at(cx, &o, Side::Right, 0);
    press(cx, &o, "secondary-f");
    assert_eq!(searching(cx, &o.view), [false, true]);
}

#[gpui_kit::test]
fn the_toolbars_find_opens_the_focused_panes_search(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SEARCH_LEFT, SEARCH_RIGHT));
    focus_at(cx, &o, Side::Right, 0);
    act(cx, o.window, |window, cx| window.click(Tool::Find.id(), cx));
    assert_eq!(searching(cx, &o.view), [false, true]);
    focus_at(cx, &o, Side::Left, 0);
    act(cx, o.window, |window, cx| window.click(Tool::Find.id(), cx));
    assert_eq!(searching(cx, &o.view), [true, true]);
}

#[gpui_kit::test]
fn a_chord_bound_to_find_opens_the_focused_panes_search(cx: &mut TestAppContext) {
    let mut bindings = KeyBindings::default();
    bindings.apply(Keybind::parse("ctrl+j=find", Platform::CURRENT).expect("the line reads"));
    let files = Files::new(SEARCH_LEFT, SEARCH_RIGHT);
    let (left, right) = (files.file(Side::Left, false), files.file(Side::Right, false));
    let (window, view, _finished) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init_with(&bindings, cx);
        let finished = Finished::install(cx);
        let (window, view) =
            merge_app::diff_window::open(left, right, &DiffSettings::default(), cx)
                .expect("window opens");
        (window, view, finished)
    });
    act(cx, window, TestWindowExt::render_frame);
    act(cx, window, |window, cx| {
        window.click(pane_id(Side::Right), cx);
    });
    act(cx, window, |window, cx| window.press("ctrl-j", cx));
    assert_eq!(searching(cx, &view), [false, true]);
}

#[gpui_kit::test]
fn each_key_typed_in_a_panes_search_adds_to_the_query(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SEARCH_LEFT, SEARCH_RIGHT));
    focus_at(cx, &o, Side::Right, 0);
    press(cx, &o, "secondary-f");
    for key in ["b", "a", "s", "e"] {
        act(cx, o.window, |window, cx| window.input(key, cx));
        cx.run_until_parked();
    }
    let query =
        cx.read(|cx| o.view.read(cx).editor(Side::Right).read(cx).search_session().query.clone());
    assert_eq!(query, "base");
}

/// The match flashes of the left and right panes. A pane holds one at most.
fn find_flashes(cx: &mut TestAppContext, view: &Entity<DiffWindow>) -> [Option<Range<usize>>; 2] {
    cx.read(|cx| {
        SIDES_OF_TEST.map(|side| {
            let flashes = view.read(cx).find_flashes(side, cx);
            assert!(flashes.len() <= 1, "{side:?} has flashes {flashes:?}");
            flashes.into_iter().next()
        })
    })
}

/// Set `side`'s search query, as its panel does on each key.
fn search_for(cx: &mut TestAppContext, o: &Opened, side: Side, query: &str) {
    act(cx, o.window, |_, cx| {
        o.view.read(cx).editor(side).clone().update(cx, |state, cx| {
            state.set_search_query(query, true, cx);
        });
    });
    cx.run_until_parked();
}

/// Step `side`'s search to its next match, as its panel's next button does.
fn next_match(cx: &mut TestAppContext, o: &Opened, side: Side) {
    act(cx, o.window, |_, cx| {
        o.view.read(cx).editor(side).clone().update(cx, |state, cx| {
            state.next_search_match(cx);
        });
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_search_step_blinks_the_match(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SEARCH_LEFT, SEARCH_RIGHT));
    focus_at(cx, &o, Side::Right, 0);
    press(cx, &o, "secondary-f");
    search_for(cx, &o, Side::Right, "b");
    assert_eq!(find_flashes(cx, &o.view), NO_FRAMES, "typing blinks nothing");
    // `b` lies at 0..1 and 4..5 on the right; the step goes to the second.
    next_match(cx, &o, Side::Right);
    let on = [None, Some(4..5)];
    let mut seen = vec![find_flashes(cx, &o.view)];
    for _ in 1..BLINK_SAMPLES {
        wait(cx, BLINK);
        seen.push(find_flashes(cx, &o.view));
    }
    assert_eq!(seen, [on.clone(), NO_FRAMES, on.clone(), NO_FRAMES, on, NO_FRAMES]);
}

#[gpui_kit::test]
fn an_edit_that_moves_the_current_match_blinks_nothing(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new(SEARCH_LEFT, SEARCH_RIGHT));
    focus_at(cx, &o, Side::Right, 0);
    press(cx, &o, "secondary-f");
    search_for(cx, &o, Side::Right, "b");
    next_match(cx, &o, Side::Right);
    wait(cx, BLINK * 5);
    // Typing `b` at the start adds a match before the current one.
    type_at(cx, &o, Side::Right, 0, "b");
    cx.run_until_parked();
    assert_eq!(find_flashes(cx, &o.view), NO_FRAMES);
}

const REVEAL_LINES: usize = 700;
/// A change shorter than the view and taller than half of it, at rows 100-139.
const SHORT_CHANGE: std::ops::Range<usize> = 100..140;
/// A change taller than the view, at rows 200-599.
const TALL_CHANGE: std::ops::Range<usize> = 200..600;

fn two_changes(changed: &str) -> String {
    use std::fmt::Write as _;
    (0..REVEAL_LINES).fold(String::new(), |mut text, n| {
        let prefix =
            if SHORT_CHANGE.contains(&n) || TALL_CHANGE.contains(&n) { changed } else { "l" };
        writeln!(text, "{prefix}{n}").expect("a String takes any write");
        text
    })
}

fn lines_tall(count: usize) -> gpui_kit::Pixels {
    #[expect(clippy::cast_precision_loss, reason = "a few hundred lines")]
    let count = count as f32;
    LINE * count
}

fn settle(cx: &mut TestAppContext, o: &Opened) {
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
}

#[gpui_kit::test]
fn navigation_centres_a_change_that_fits_and_tops_one_that_does_not(cx: &mut TestAppContext) {
    let files = Files::new(&two_changes("L"), &two_changes("R"));
    let o = open(cx, &files);
    settle(cx, &o);
    let height =
        cx.read(|cx| o.view.read(cx).editor(Side::Left).read(cx).input_bounds().size.height);
    let short = lines_tall(SHORT_CHANGE.len());
    assert!(short <= height && short * 2. > height, "the short change fits, just");
    assert!(lines_tall(TALL_CHANGE.len()) > height, "the tall change does not fit");
    // Opening goes to the first change: centred, with the room left split above and below.
    let centred = lines_tall(SHORT_CHANGE.start) - (height - short) / 2.;
    assert_eq!(scroll_tops(cx, &o.view), (centred, centred, LINE));

    press(cx, &o, "secondary-down");
    settle(cx, &o);
    let top = lines_tall(TALL_CHANGE.start);
    assert_eq!(scroll_tops(cx, &o.view), (top, top, LINE));

    press(cx, &o, "secondary-up");
    settle(cx, &o);
    assert_eq!(scroll_tops(cx, &o.view), (centred, centred, LINE));
}

/// The View menu's items and whether each shows a tick.
fn view_ticks(cx: &mut TestAppContext) -> Vec<(String, bool)> {
    let menus = cx.update(|cx| cx.get_menus()).expect("the test platform keeps menus");
    menus
        .into_iter()
        .find(|menu| menu.name.as_ref() == "View")
        .expect("a View menu")
        .items
        .into_iter()
        .filter_map(|item| match item {
            gpui_kit::OwnedMenuItem::Action { name, checked, .. } => Some((name, checked)),
            _ => None,
        })
        .collect()
}

#[gpui_kit::test]
fn the_view_menu_ticks_what_the_diff_window_shows(cx: &mut TestAppContext) {
    let o = open(cx, &Files::new("a\n", "b\n"));
    act(cx, o.window, |window, _| window.activate_window());
    cx.run_until_parked();
    act(cx, o.window, TestWindowExt::render_frame);
    let ticks = |syntax, wrap| {
        // The diff window has no base pane and no result, and these files have no origins,
        // so no Blame item.
        [
            ("Base Pane", false),
            ("Syntax Highlighting", syntax),
            ("Soft Wrap", wrap),
            ("Sync Result Scrolling", false),
        ]
        .map(|(name, ticked)| (name.to_owned(), ticked))
        .to_vec()
    };
    assert_eq!(view_ticks(cx), ticks(true, false));
    let toggle = |cx: &mut TestAppContext, action: &dyn gpui_kit::Action| {
        act(cx, o.window, |window, cx| {
            window.dispatch_action(action.boxed_clone(), cx);
        });
        act(cx, o.window, TestWindowExt::render_frame);
    };
    toggle(cx, &merge_app::ToggleOriginColumn);
    assert_eq!(view_ticks(cx), ticks(true, false));
    toggle(cx, &merge_app::ToggleSyntax);
    assert_eq!(view_ticks(cx), ticks(false, false));
    toggle(cx, &merge_app::ToggleSoftWrap);
    assert_eq!(view_ticks(cx), ticks(false, true));
}

/// `side`'s display row of buffer line `line`, and its display row count.
fn display_row(
    cx: &mut TestAppContext,
    view: &Entity<DiffWindow>,
    side: Side,
    line: usize,
) -> (usize, usize) {
    cx.read(|cx| {
        let map = view.read(cx).editor(side).read(cx).display_map();
        (map.buffer_line_to_display_row(line), map.display_row_count())
    })
}

/// The display rows `side`'s buffer line `line` wraps to.
fn wrap_rows(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side, line: usize) -> usize {
    cx.read(|cx| {
        view.read(cx)
            .editor(side)
            .read(cx)
            .display_map()
            .visible_wrap_row_count_for_buffer_line(line)
    })
}

/// A long line on the left against a short one on the right, between equal lines: one
/// changed region whose sides have one line each.
fn long_against_short() -> Files {
    Files::new(&wide_file("a", 400), "a\nshort\nend\n")
}

#[gpui_kit::test]
fn wrapped_panes_stay_level_below_a_long_changed_line(cx: &mut TestAppContext) {
    let settings = DiffSettings { soft_wrap: true, ..DiffSettings::default() };
    let o = open_set(cx, &long_against_short(), &settings);
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    let wrapped = wrap_rows(cx, &o.view, Side::Left, 1);
    assert!(wrapped > 1, "the long line wraps: {wrapped} rows");
    // `end`, line 2 on both sides, sits on the same display row; the right pane takes the
    // long line's extra rows as spacers above it.
    assert_eq!(display_row(cx, &o.view, Side::Right, 2), display_row(cx, &o.view, Side::Left, 2));
    assert_eq!(rows(cx, &o.view, Side::Right).0, [(2, wrapped - 1)]);
    assert_eq!(rows(cx, &o.view, Side::Left).0, []);
}

#[gpui_kit::test]
fn turning_wrap_on_and_off_levels_the_panes_each_time(cx: &mut TestAppContext) {
    let o = open(cx, &long_against_short());
    assert_eq!(rows(cx, &o.view, Side::Right).0, [], "one line each, unwrapped");
    toggle_soft_wrap(cx, &o);
    let wrapped = wrap_rows(cx, &o.view, Side::Left, 1);
    assert!(wrapped > 1, "the long line wraps: {wrapped} rows");
    assert_eq!(rows(cx, &o.view, Side::Right).0, [(2, wrapped - 1)]);
    assert_eq!(display_row(cx, &o.view, Side::Right, 2), display_row(cx, &o.view, Side::Left, 2));
    toggle_soft_wrap(cx, &o);
    assert_eq!(rows(cx, &o.view, Side::Right).0, [], "unwrapped again");
    assert_eq!(display_row(cx, &o.view, Side::Right, 2), (2, 4));
}

/// Equal long lines above the panes' one change, so the rows above it wrap.
const WRAPPED_ABOVE: usize = 30;

#[gpui_kit::test]
fn navigation_reveals_a_change_below_wrapped_lines(cx: &mut TestAppContext) {
    let long = format!("{}\n", "x".repeat(400)).repeat(WRAPPED_ABOVE);
    let files = Files::new(&format!("{long}left\nend\n"), &format!("{long}right\nend\n"));
    let settings = DiffSettings { soft_wrap: true, ..DiffSettings::default() };
    let o = open_set(cx, &files, &settings);
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    assert!(wrap_rows(cx, &o.view, Side::Left, 0) > 1, "the lines above wrap");
    act(cx, o.window, |window, cx| {
        window.click(Tool::NextDifference.id(), cx);
    });
    for _ in 0..SETTLE_FRAMES {
        act(cx, o.window, TestWindowExt::render_frame);
    }
    let (row, _) = display_row(cx, &o.view, Side::Left, WRAPPED_ABOVE);
    let change_top = LINE * f32::from(u16::try_from(row).expect("a few hundred rows"));
    let view = cx.read(|cx| o.view.read(cx).editor(Side::Left).read(cx).input_bounds().size.height);
    let (left, right, _) = scroll_tops(cx, &o.view);
    assert_eq!(left, right);
    assert!(
        left <= change_top && change_top + LINE <= left + view,
        "the change, rows {change_top:?}.., shows in the pane, {left:?}..{:?}",
        left + view
    );
}

#[gpui_kit::test]
fn a_save_writes_off_the_ui_thread(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-s");
    assert_eq!(files.read(Side::Left), "a\n", "not written yet");
    let path = files.path(Side::Left).display().to_string();
    assert_eq!(title(cx, &o.view, Side::Left), path.clone() + MODIFIED);
    cx.run_until_parked();
    assert_eq!(files.read(Side::Left), "xa\n");
    assert_eq!(title(cx, &o.view, Side::Left), path);
}

#[gpui_kit::test]
fn an_edit_while_saving_keeps_the_marker(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-s");
    type_at(cx, &o, Side::Left, 0, "y");
    cx.run_until_parked();
    assert_eq!(files.read(Side::Left), "xa\n");
    let expected = files.path(Side::Left).display().to_string() + MODIFIED;
    assert_eq!(title(cx, &o.view, Side::Left), expected);
}

#[gpui_kit::test]
fn the_windows_own_save_changes_nothing_after_an_edit_while_saving(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-s");
    type_at(cx, &o, Side::Left, 0, "y");
    cx.run_until_parked();
    disk_changed(cx, &o, Side::Left);
    assert!(!cx.has_pending_prompt(), "the file holds the window's own save");
    assert_eq!(texts(cx, &o.view, Side::Left).0, "yxa\n");
}

#[gpui_kit::test]
fn a_change_on_disk_after_a_reload_is_not_taken_for_the_windows_save(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    save(cx, &o);
    std::fs::write(files.path(Side::Left), "other\n").expect("left");
    disk_changed(cx, &o, Side::Left);
    assert_eq!(texts(cx, &o.view, Side::Left).0, "other\n");
    // The file goes back to the bytes the window once saved: a change all the same.
    std::fs::write(files.path(Side::Left), "xa\n").expect("left");
    disk_changed(cx, &o, Side::Left);
    assert_eq!(texts(cx, &o.view, Side::Left).0, "xa\n");
}

#[gpui_kit::test]
fn saves_land_in_the_order_asked(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-s");
    type_at(cx, &o, Side::Left, 0, "y");
    press(cx, &o, "secondary-s");
    cx.run_until_parked();
    assert_eq!(files.read(Side::Left), "yxa\n");
    let path = files.path(Side::Left).display().to_string();
    assert_eq!(title(cx, &o.view, Side::Left), path);
}

#[gpui_kit::test]
fn closing_with_save_exits_once_the_write_lands(cx: &mut TestAppContext) {
    let files = Files::new("a\n", "b\n");
    let o = open(cx, &files);
    // What the file holds when the window exits.
    let at_exit = Rc::new(RefCell::new(None));
    let seen = Rc::clone(&at_exit);
    let path = files.path(Side::Left);
    cx.update(|cx| {
        cx.set_global(merge_app::Finish::new(move |_, _| {
            *seen.borrow_mut() = Some(std::fs::read_to_string(&path).expect("left"));
        }));
    });
    type_at(cx, &o, Side::Left, 0, "x");
    press(cx, &o, "secondary-w");
    cx.simulate_prompt_answer(SAVE);
    cx.run_until_parked();
    assert_eq!(at_exit.borrow().as_deref(), Some("xa\n"));
}
