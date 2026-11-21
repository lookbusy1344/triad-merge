//! Headless tests of the directory window (spec §6.1): the files that differ between two
//! directories, each text file opening in the diff window, a refresh after the diff window
//! closes, and the exit code.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::common::tool_window::{LIST_TOP, UNDER_HEADER, assert_frame};
use crate::common::{act, bounds, is_selected};
use gpui_kit::base::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, App, Entity, TestAppContext, VisualTestContext};
use merge_app::diff_window::{DiffWindow, copy_id};
use merge_app::dir_window::{
    self, DirStart, DirWindow, EMPTY_ID, NOTE_ID, REFRESH_ID, SUMMARY_ID, Source, file_id,
};
use merge_app::{ExitCode, WindowSettings};
use merge_core::diff::DiffOptions;
use merge_core::diff2::Side;
use merge_core::index::RegionIx;
use merge_session::dir_diff::{Change, Content};
use merge_session::repository::Discovery;

/// `left/` and `right/` below one temporary directory.
struct Dirs {
    root: tempfile::TempDir,
}

impl Dirs {
    fn new(left: &[(&str, &[u8])], right: &[(&str, &[u8])]) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        for (side, files) in [("left", left), ("right", right)] {
            let dir = root.path().join(side);
            std::fs::create_dir(&dir).expect("dir");
            for (name, bytes) in files {
                let path = dir.join(name);
                std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
                std::fs::write(path, bytes).expect("written");
            }
        }
        Self { root }
    }

    fn side(&self, side: &str) -> PathBuf {
        self.root.path().join(side)
    }

    fn start(&self) -> DirStart {
        self.start_with(self.side("left"), [false; 2])
    }

    fn start_with(&self, left: PathBuf, readonly: [bool; 2]) -> DirStart {
        dirs_start(self.root.path(), left, self.side("right"), readonly)
    }
}

/// The window on directories `left` and `right`, from the working directory `cwd`.
fn dirs_start(cwd: &Path, left: PathBuf, right: PathBuf, readonly: [bool; 2]) -> DirStart {
    start_on(cwd, Source::Dirs { left, right, readonly })
}

fn start_on(cwd: &Path, source: Source) -> DirStart {
    DirStart {
        cwd: cwd.to_owned(),
        source,
        discovery: Discovery::default(),
        title: None,
        diff: DiffOptions::default(),
        settings: WindowSettings::default(),
        watch: false,
    }
}

/// A modified text file, an added text file, a deleted text file and an added binary file.
fn mixed() -> Dirs {
    Dirs::new(
        &[("same.txt", b"s\n"), ("src/a.rs", b"1\n"), ("gone.txt", b"g\n")],
        &[("same.txt", b"s\n"), ("src/a.rs", b"2\n"), ("new.md", b"n\n"), ("img.png", b"\x00p")],
    )
}

/// The rows of [`mixed`], by path.
const GONE: usize = 0;
const IMG: usize = 1;
const NEW: usize = 2;
const A_RS: usize = 3;

struct Started {
    window: AnyWindowHandle,
    view: Entity<DirWindow>,
    exits: Rc<RefCell<Vec<ExitCode>>>,
}

/// The directory window on `start`, recording the codes it exits with, once its list has
/// landed.
fn open(cx: &mut TestAppContext, start: DirStart) -> Started {
    let exits = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&exits);
    let (window, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        dir_window::start(start, move |code, _: &mut App| record.borrow_mut().push(code), cx)
            .expect("the window opens")
    });
    cx.run_until_parked();
    Started { window, view, exits }
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: gpui_kit::ElementId) {
    act(cx, window, TestWindowExt::render_frame);
    act(cx, window, |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
    cx.run_until_parked();
}

/// Each open diff window.
fn diff_windows(cx: &mut TestAppContext) -> Vec<(AnyWindowHandle, Entity<DiffWindow>)> {
    cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|w| {
                let root = w.downcast::<Root>()?.read(cx).ok()?;
                let view = root.view().clone().downcast::<DiffWindow>().ok()?;
                Some((w, view))
            })
            .collect()
    })
}

/// `side`'s text, title and read-only flag in `view`.
fn side(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side) -> (String, String, bool) {
    cx.read(|cx| {
        let view = view.read(cx);
        let read_only = view.editor(side).read(cx).presentation().is_readonly();
        (view.document().text(side).to_string(), view.title(side).to_string(), read_only)
    })
}

fn listed(cx: &mut TestAppContext, view: &Entity<DirWindow>) -> Vec<(String, Change, Content)> {
    cx.read(|cx| {
        view.read(cx)
            .files()
            .iter()
            .map(|file| (file.display_path(), file.change.clone(), file.content))
            .collect()
    })
}

fn note(cx: &mut TestAppContext, view: &Entity<DirWindow>) -> Option<String> {
    cx.read(|cx| view.read(cx).note().map(str::to_owned))
}

fn label(cx: &mut TestAppContext, window: AnyWindowHandle, id: gpui_kit::ElementId) -> String {
    let mut found = String::new();
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        found = window.find(id).label().map_or_else(String::new, str::to_owned);
    });
    found
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[gpui_kit::test]
fn the_window_lists_the_files_that_differ_by_path(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    assert_eq!(
        listed(cx, &started.view),
        [
            ("gone.txt".to_owned(), Change::Deleted, Content::Text),
            ("img.png".to_owned(), Change::Added, Content::Binary),
            ("new.md".to_owned(), Change::Added, Content::Text),
            ("src/a.rs".to_owned(), Change::Modified, Content::Text),
        ]
    );
    assert_eq!(label(cx, started.window, file_id(GONE)), "gone.txt deleted");
    assert_eq!(label(cx, started.window, file_id(IMG)), "img.png added, binary");
    assert_eq!(label(cx, started.window, file_id(A_RS)), "src/a.rs modified");
    assert_eq!(label(cx, started.window, SUMMARY_ID.into()), "4 files");
    assert_eq!(diff_windows(cx).len(), 0);
    assert_eq!(note(cx, &started.view), None);
}

#[gpui_kit::test]
fn a_renamed_file_opens_its_old_file_on_the_left(cx: &mut TestAppContext) {
    let dirs = Dirs::new(&[("old.rs", b"1\n2\n3\n4\n")], &[("lib/new.rs", b"1\n2\n3\n5\n")]);
    let started = open(cx, dirs.start());
    assert_eq!(label(cx, started.window, file_id(0)), "lib/new.rs renamed from old.rs");
    click(cx, started.window, file_id(0));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    let left = path_text(&dirs.side("left").join("old.rs"));
    let right = path_text(&dirs.side("right").join("lib").join("new.rs"));
    assert_eq!(side(cx, view, Side::Left), ("1\n2\n3\n4\n".to_owned(), left, false));
    assert_eq!(side(cx, view, Side::Right), ("1\n2\n3\n5\n".to_owned(), right, false));
}

#[gpui_kit::test]
fn a_renamed_file_opens_against_its_old_path_at_the_revision(cx: &mut TestAppContext) {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[("old.txt", b"1\n2\n3\n4\n")]);
    fixture.stage(&[("new.txt", b"1\n2\n3\n4\n")]);
    fixture.write("new.txt", b"1\n2\n3\n5\n");
    let started = open(cx, changes_start(fixture.root(), None));
    assert_eq!(label(cx, started.window, file_id(0)), "new.txt renamed from old.txt");
    click(cx, started.window, file_id(0));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    let working = dunce::canonicalize(fixture.root()).expect("the root").join("new.txt");
    assert_eq!(side(cx, view, Side::Left), ("1\n2\n3\n4\n".to_owned(), "HEAD".to_owned(), true));
    assert_eq!(
        side(cx, view, Side::Right),
        ("1\n2\n3\n5\n".to_owned(), path_text(&working), false)
    );
}

#[gpui_kit::test]
fn a_modified_file_opens_with_both_sides_editable(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(A_RS));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    let [left, right] =
        ["left", "right"].map(|s| path_text(&dirs.side(s).join("src").join("a.rs")));
    assert_eq!(side(cx, view, Side::Left), ("1\n".to_owned(), left, false));
    assert_eq!(side(cx, view, Side::Right), ("2\n".to_owned(), right, false));
}

#[gpui_kit::test]
fn an_added_file_opens_with_an_empty_read_only_left_side(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(NEW));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    assert_eq!(side(cx, view, Side::Left), (String::new(), "new.md (no file)".to_owned(), true));
    let right = path_text(&dirs.side("right").join("new.md"));
    assert_eq!(side(cx, view, Side::Right), ("n\n".to_owned(), right, false));
}

#[gpui_kit::test]
fn a_deleted_file_opens_with_an_empty_read_only_right_side(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(GONE));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    let left = path_text(&dirs.side("left").join("gone.txt"));
    assert_eq!(side(cx, view, Side::Left), ("g\n".to_owned(), left, false));
    assert_eq!(side(cx, view, Side::Right), (String::new(), "gone.txt (no file)".to_owned(), true));
}

#[gpui_kit::test]
fn the_readonly_flags_hold_for_every_file(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start_with(dirs.side("left"), [true, false]));
    click(cx, started.window, file_id(A_RS));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    assert!(side(cx, view, Side::Left).2);
    assert!(!side(cx, view, Side::Right).2);
}

#[gpui_kit::test]
fn a_binary_file_opens_nothing(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(IMG));
    assert_eq!(diff_windows(cx).len(), 0);
    press(cx, started.window, "down");
    press(cx, started.window, "down");
    assert!(is_selected(cx, started.window, file_id(IMG)));
    press(cx, started.window, "enter");
    assert_eq!(diff_windows(cx).len(), 0);
    // Nothing was read: a read would fail to decode and say so.
    assert_eq!(note(cx, &started.view), None);
}

#[gpui_kit::test]
fn a_file_copied_across_and_saved_drops_out_of_the_list(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(A_RS));
    let [(diff, _)] = diff_windows(cx)[..] else { panic!("one diff window") };
    click(cx, diff, copy_id(RegionIx::new(0), Side::Right));
    press(cx, diff, "secondary-s");
    press(cx, diff, "secondary-w");
    assert_eq!(diff_windows(cx).len(), 0);
    let on_disk = std::fs::read(dirs.side("right").join("src/a.rs")).expect("a.rs reads");
    assert_eq!(on_disk, b"1\n");
    let paths: Vec<_> = listed(cx, &started.view).into_iter().map(|(path, ..)| path).collect();
    assert_eq!(paths, ["gone.txt", "img.png", "new.md"]);
    assert_eq!(started.exits.borrow().as_slice(), [], "the run goes on");
}

#[gpui_kit::test]
fn a_second_file_while_a_diff_window_is_open_opens_nothing(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(A_RS));
    let open_now: Vec<_> = diff_windows(cx).into_iter().map(|(w, _)| w).collect();
    assert_eq!(open_now.len(), 1);
    click(cx, started.window, file_id(NEW));
    let now: Vec<_> = diff_windows(cx).into_iter().map(|(w, _)| w).collect();
    assert_eq!(now, open_now);
}

#[gpui_kit::test]
fn a_file_gone_from_disk_says_why_and_opens_nothing(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    let file = dirs.side("right").join("new.md");
    std::fs::remove_file(&file).expect("new.md is removed");
    click(cx, started.window, file_id(NEW));
    assert_eq!(diff_windows(cx).len(), 0);
    let said = note(cx, &started.view).expect("a note");
    assert!(said.starts_with(&format!("cannot read {}: ", file.display())), "{said}");
    // The window is free to open another file.
    click(cx, started.window, file_id(A_RS));
    assert_eq!(diff_windows(cx).len(), 1);
    assert_eq!(note(cx, &started.view), None);
}

#[gpui_kit::test]
fn a_directory_that_cannot_be_read_says_why(cx: &mut TestAppContext) {
    let dirs = mixed();
    let missing = dirs.side("missing");
    let started = open(cx, dirs.start_with(missing.clone(), [false; 2]));
    assert_eq!(listed(cx, &started.view), []);
    assert_eq!(note(cx, &started.view), Some(format!("cannot read {}", missing.display())));
    assert_eq!(bounds(cx, started.window, NOTE_ID).top(), UNDER_HEADER);
    assert_eq!(crate::common::role(cx, started.window, NOTE_ID), Some(gpui_kit::Role::Alert));
}

#[gpui_kit::test]
fn refresh_lists_again(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    std::fs::write(dirs.side("right").join("same.txt"), b"changed\n").expect("written");
    click(cx, started.window, REFRESH_ID.into());
    let paths: Vec<_> = listed(cx, &started.view).into_iter().map(|(path, ..)| path).collect();
    assert_eq!(paths, ["gone.txt", "img.png", "new.md", "same.txt", "src/a.rs"]);
}

#[gpui_kit::test]
fn the_selection_follows_its_file_when_the_list_changes(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    press(cx, started.window, "up");
    assert!(is_selected(cx, started.window, file_id(A_RS)));
    // `same.txt` now differs and lists before `src/a.rs`.
    std::fs::write(dirs.side("right").join("same.txt"), b"changed\n").expect("written");
    click(cx, started.window, REFRESH_ID.into());
    assert!(is_selected(cx, started.window, file_id(A_RS + 1)), "src/a.rs stays selected");
    assert!(!is_selected(cx, started.window, file_id(A_RS)));
    // The selected file drops out: nothing is selected.
    std::fs::write(dirs.side("right").join("src/a.rs"), b"1\n").expect("written");
    click(cx, started.window, REFRESH_ID.into());
    let rows = listed(cx, &started.view).len();
    assert_eq!(rows, 4);
    for row in 0..rows {
        assert!(!is_selected(cx, started.window, file_id(row)), "row {row} is selected");
    }
}

#[gpui_kit::test]
fn a_diff_window_closed_while_a_listing_runs_lists_again_once_it_lands(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(A_RS));
    let [(diff, _)] = diff_windows(cx)[..] else { panic!("one diff window") };
    act(cx, started.window, TestWindowExt::render_frame);
    act(cx, started.window, |window, cx| window.click(REFRESH_ID, cx));
    // The listing reads the directories; its result has not reached the window.
    while cx.dispatcher.tick(true) {}
    std::fs::write(dirs.side("right").join("same.txt"), b"changed\n").expect("written");
    press(cx, diff, "secondary-w");
    assert_eq!(diff_windows(cx).len(), 0);
    let paths: Vec<_> = listed(cx, &started.view).into_iter().map(|(path, ..)| path).collect();
    assert_eq!(paths, ["gone.txt", "img.png", "new.md", "same.txt", "src/a.rs"]);
}

#[gpui_kit::test]
fn closing_the_window_ends_the_run_with_0(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    assert!(VisualTestContext::from_window(started.window, cx).simulate_close());
    cx.run_until_parked();
    assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn closing_while_a_refresh_runs_waits_for_it_then_ends_the_run(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    act(cx, started.window, TestWindowExt::render_frame);
    act(cx, started.window, |window, cx| window.click(REFRESH_ID, cx));
    assert!(!VisualTestContext::from_window(started.window, cx).simulate_close());
    assert_eq!(started.exits.borrow().as_slice(), []);
    cx.run_until_parked();
    assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn the_window_stays_open_while_its_diff_window_is(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(A_RS));
    assert!(!VisualTestContext::from_window(started.window, cx).simulate_close());
    cx.run_until_parked();
    assert_eq!(started.exits.borrow().as_slice(), []);
    assert_eq!(diff_windows(cx).len(), 1);
}

#[gpui_kit::test]
fn the_window_has_the_tool_window_frame(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    assert_frame(cx, started.window, SUMMARY_ID, REFRESH_ID, file_id(0));
    let [first, second] = [0, 1].map(|ix| bounds(cx, started.window, file_id(ix)));
    assert_eq!(second.top(), first.bottom());
}

#[gpui_kit::test]
fn escape_closes_the_window_and_ends_the_run_with_0(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    press(cx, started.window, "escape");
    assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
    assert!(!cx.update(|cx| cx.windows().contains(&started.window)), "the window closed");
}

#[gpui_kit::test]
fn cancel_closes_the_window_as_its_key_does(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    act(cx, started.window, |window, cx| window.dispatch_action(Box::new(merge_app::Cancel), cx));
    cx.run_until_parked();
    assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
    assert!(!cx.update(|cx| cx.windows().contains(&started.window)), "the window closed");
}

#[gpui_kit::test]
fn escape_while_a_diff_window_is_open_keeps_the_window(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    click(cx, started.window, file_id(A_RS));
    press(cx, started.window, "escape");
    assert_eq!(started.exits.borrow().as_slice(), []);
    assert!(cx.update(|cx| cx.windows().contains(&started.window)), "the window stays");
    assert_eq!(diff_windows(cx).len(), 1);
}

#[gpui_kit::test]
fn up_and_enter_open_the_selected_file(cx: &mut TestAppContext) {
    let dirs = mixed();
    let started = open(cx, dirs.start());
    press(cx, started.window, "up");
    assert!(is_selected(cx, started.window, file_id(A_RS)));
    press(cx, started.window, "enter");
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    assert_eq!(side(cx, view, Side::Left).0, "1\n");
}

#[gpui_kit::test]
fn equal_directories_say_so_where_the_list_starts(cx: &mut TestAppContext) {
    let dirs = Dirs::new(&[("a", b"1\n")], &[("a", b"1\n")]);
    let started = open(cx, dirs.start());
    assert_eq!(listed(cx, &started.view), []);
    assert_eq!(bounds(cx, started.window, EMPTY_ID).top(), LIST_TOP);
}

/// The window on the working copy's changes in the repository at `root`, against `rev`.
fn changes_start(root: impl AsRef<Path>, rev: Option<&str>) -> DirStart {
    start_on(root.as_ref(), Source::Changes { rev: rev.map(str::to_owned) })
}

/// A git repository whose `HEAD` holds `a.txt` and `gone.txt`; on disk `a.txt` is edited,
/// `gone.txt` is absent, and `new.txt` is staged.
fn git_changes() -> vcs_git::testing::Fixture {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[("a.txt", b"1\n"), ("gone.txt", b"g\n")]);
    fixture.stage(&[("a.txt", b"1\n"), ("gone.txt", b"g\n"), ("new.txt", b"n\n")]);
    fixture.write("a.txt", b"2\n");
    fixture.write("new.txt", b"n\n");
    fixture
}

/// The rows of [`git_changes`], by path.
const CHANGED_A: usize = 0;
const CHANGED_GONE: usize = 1;
const CHANGED_NEW: usize = 2;

#[gpui_kit::test]
fn the_working_copy_s_changes_are_listed(cx: &mut TestAppContext) {
    let fixture = git_changes();
    let started = open(cx, changes_start(fixture.root(), None));
    assert_eq!(
        listed(cx, &started.view),
        [
            ("a.txt".to_owned(), Change::Modified, Content::Text),
            ("gone.txt".to_owned(), Change::Deleted, Content::Text),
            ("new.txt".to_owned(), Change::Added, Content::Text),
        ]
    );
    assert_eq!(label(cx, started.window, SUMMARY_ID.into()), "3 files against HEAD");
    assert_eq!(note(cx, &started.view), None);
}

#[gpui_kit::test]
fn a_changed_file_opens_against_the_revision_and_saves_to_the_working_copy(
    cx: &mut TestAppContext,
) {
    let fixture = git_changes();
    let started = open(cx, changes_start(fixture.root(), None));
    click(cx, started.window, file_id(CHANGED_A));
    let [(diff, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    let working = dunce::canonicalize(fixture.root()).expect("the root").join("a.txt");
    assert_eq!(side(cx, view, Side::Left), ("1\n".to_owned(), "HEAD".to_owned(), true));
    assert_eq!(side(cx, view, Side::Right), ("2\n".to_owned(), path_text(&working), false));
    click(cx, *diff, copy_id(RegionIx::new(0), Side::Right));
    press(cx, *diff, "secondary-s");
    assert_eq!(std::fs::read(&working).expect("a.txt reads"), b"1\n");
    // Closing the diff window lists again: `a.txt` no longer differs.
    press(cx, *diff, "secondary-w");
    assert_eq!(diff_windows(cx).len(), 0);
    assert_eq!(
        listed(cx, &started.view),
        [
            ("gone.txt".to_owned(), Change::Deleted, Content::Text),
            ("new.txt".to_owned(), Change::Added, Content::Text),
        ]
    );
}

#[gpui_kit::test]
fn a_file_deleted_on_disk_opens_against_an_empty_read_only_right_side(cx: &mut TestAppContext) {
    let fixture = git_changes();
    let started = open(cx, changes_start(fixture.root(), Some("HEAD")));
    click(cx, started.window, file_id(CHANGED_GONE));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    assert_eq!(side(cx, view, Side::Left), ("g\n".to_owned(), "HEAD".to_owned(), true));
    assert_eq!(side(cx, view, Side::Right), (String::new(), "gone.txt (no file)".to_owned(), true));
}

#[gpui_kit::test]
fn a_file_new_since_the_revision_opens_against_an_empty_left_side(cx: &mut TestAppContext) {
    let fixture = git_changes();
    let started = open(cx, changes_start(fixture.root(), None));
    click(cx, started.window, file_id(CHANGED_NEW));
    let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
    assert_eq!(side(cx, view, Side::Left), (String::new(), "HEAD (no file)".to_owned(), true));
    assert_eq!(side(cx, view, Side::Right).0, "n\n");
}

#[gpui_kit::test]
fn an_unknown_revision_says_why(cx: &mut TestAppContext) {
    let fixture = git_changes();
    let started = open(cx, changes_start(fixture.root(), Some("nope")));
    assert_eq!(listed(cx, &started.view), []);
    assert_eq!(note(cx, &started.view), Some("unknown revision nope".to_owned()));
}

mod jj {
    use super::*;
    use vcs_jj::testing::Fixture;

    /// A working copy on disk whose `a.txt` holds `2\n`, snapshotted.
    fn working_copy() -> Fixture {
        let fixture = Fixture::new();
        fixture.check_out_to_disk();
        std::fs::write(fixture.root().join("a.txt"), b"2\n").expect("a.txt is written");
        let mut repo = vcs_jj::JjRepository::discover(fixture.root()).expect("the repository");
        vcs::Repository::snapshot(&mut repo).expect("the snapshot");
        fixture
    }

    #[gpui_kit::test]
    fn a_right_copy_of_the_working_copy_saves_to_the_working_copy_file(cx: &mut TestAppContext) {
        let fixture = working_copy();
        let tool = tempfile::Builder::new().prefix("jj-diff-").tempdir().expect("tempdir");
        let [left, right] = ["left", "right"].map(|s| tool.path().join(s));
        for (dir, bytes) in [(&left, b"1\n"), (&right, b"2\n")] {
            std::fs::create_dir(dir).expect("dir");
            std::fs::write(dir.join("a.txt"), bytes).expect("written");
        }
        let start = dirs_start(fixture.root().as_std_path(), left, right.clone(), [false; 2]);
        let started = open(cx, start);
        click(cx, started.window, file_id(0));
        let [(diff, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
        let working = dunce::canonicalize(fixture.root()).expect("the root").join("a.txt");
        assert!(side(cx, view, Side::Left).2, "the left copy turns read-only");
        assert_eq!(side(cx, view, Side::Right).1, path_text(&working));
        click(cx, *diff, copy_id(RegionIx::new(0), Side::Right));
        let operations = fixture.operation_count();
        press(cx, *diff, "secondary-s");
        assert_eq!(std::fs::read(&working).expect("a.txt reads"), b"1\n");
        assert_eq!(std::fs::read(right.join("a.txt")).expect("the copy reads"), b"2\n");
        assert_eq!(fixture.operation_count(), operations, "jj's own command takes the save in");
    }

    #[gpui_kit::test]
    fn the_listing_snapshots_and_opens_against_at_minus(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"1\n")]);
        fixture.check_out_to_disk();
        fixture.write("a.txt", b"2\n");
        let started = open(cx, changes_start(fixture.root(), None));
        assert_eq!(
            listed(cx, &started.view),
            [("a.txt".to_owned(), Change::Modified, Content::Text)]
        );
        click(cx, started.window, file_id(0));
        let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
        let working = dunce::canonicalize(fixture.root()).expect("the root").join("a.txt");
        assert_eq!(side(cx, view, Side::Left), ("1\n".to_owned(), "@-".to_owned(), true));
        assert_eq!(side(cx, view, Side::Right), ("2\n".to_owned(), path_text(&working), false));
    }

    #[gpui_kit::test]
    fn a_save_from_the_listing_snapshots_the_working_copy(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        fixture.commit("one", &[("a.txt", b"1\n")]);
        fixture.check_out_to_disk();
        fixture.write("a.txt", b"2\n");
        let started = open(cx, changes_start(fixture.root(), None));
        click(cx, started.window, file_id(0));
        let [(diff, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
        click(cx, *diff, copy_id(RegionIx::new(0), Side::Right));
        let operations = fixture.operation_count();
        press(cx, *diff, "secondary-s");
        assert_eq!(fixture.operation_count(), operations + 1, "one snapshot operation");
        let tree = fixture.tree_text(&fixture.working_copy_commit());
        assert_eq!(tree.get("a.txt").map(String::as_str), Some("1\n"));
        assert_eq!(cx.read(|cx| view.read(cx).snapshot_failed().cloned()), None);
    }

    #[gpui_kit::test]
    fn a_save_jj_cannot_snapshot_stays_written_and_a_banner_says_why(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let one = fixture.commit_on(&[], "one", &[("a.txt", b"1\n")]);
        let other = fixture.commit_on(&[], "other", &[("a.txt", b"o\n")]);
        fixture.check_out(&[&one]);
        fixture.check_out_to_disk();
        fixture.write("a.txt", b"2\n");
        let started = open(cx, changes_start(fixture.root(), None));
        click(cx, started.window, file_id(0));
        let [(diff, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
        // The view moves on; the files on disk stay at `one`, so the working copy is stale.
        fixture.check_out(&[&other]);
        click(cx, *diff, copy_id(RegionIx::new(0), Side::Right));
        let operations = fixture.operations();
        press(cx, *diff, "secondary-s");
        assert!(!cx.has_pending_prompt(), "a failed snapshot asks nothing");
        let working = dunce::canonicalize(fixture.root()).expect("the root").join("a.txt");
        assert_eq!(std::fs::read(&working).expect("a.txt reads"), b"1\n");
        assert_eq!(fixture.operations(), operations, "jj wrote nothing");
        let banner = cx.read(|cx| view.read(cx).snapshot_failed().cloned()).expect("a banner");
        assert!(banner.starts_with("Saved, but jj did not record the save: "), "{banner}");
    }
}
