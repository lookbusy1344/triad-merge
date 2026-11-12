//! Headless tests of `resolve` without a path: each conflicted file opens in turn
//! (spec §7.4, `DESIGN.md` §D5.4).

use crate::common;
use merge_view::rows::RowIx;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use common::act;
use gpui_kit::base::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, App, Entity, TestAppContext};
use merge_app::{ExitCode, MergeWindow, WindowSettings};
use merge_core::document::MergeOptions;
use vcs::testing::Entry;
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Held, Op};

const SAVE_WITH_CONFLICTS: &str = "Save with Conflicts";
const DISCARD: &str = "Discard";
const TRY_AGAIN: &str = "Try Again";
const QUIT: &str = "Quit";
const CANNOT_RECORD: &str = "The repository did not record the save";

/// A git merge stopped on a conflict in each of `paths`: `a` against `left` and `right`.
fn stopped(paths: &[&str]) -> vcs_git::testing::Fixture {
    stopped_with(paths, &[])
}

/// As [`stopped`], with `deleted` deleted on every side.
fn stopped_with(paths: &[&str], deleted: &[&str]) -> vcs_git::testing::Fixture {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[("x.txt", b"x\n")]);
    fixture.commit("left", &[("x.txt", b"x\n")]);
    let theirs = fixture.commit_on(&[one], "right", &[("x.txt", b"x\n")]);
    let terms: [&[u8]; 3] = [b"a\n", b"left\n", b"right\n"];
    let conflicts: Vec<(&str, [Option<Entry>; 3])> = paths
        .iter()
        .map(|&path| (path, terms.map(|t| Some(Entry::File(t)))))
        .chain(deleted.iter().map(|&path| (path, [Some(Entry::File(terms[0])), None, None])))
        .collect();
    fixture.stage_conflicts(&[("x.txt", b"x\n")], &conflicts);
    fixture.write_pseudo_ref("MERGE_HEAD", theirs);
    fixture
}

fn canonical(path: &Path) -> PathBuf {
    dunce::canonicalize(path).expect("the path exists")
}

/// Start the queue at `fixture`'s root, recording the codes it exits with.
fn start(
    cx: &mut TestAppContext,
    fixture: &vcs_git::testing::Fixture,
) -> Rc<RefCell<Vec<ExitCode>>> {
    start_at(cx, fixture.root().as_std_path())
}

/// Start the queue at `root`, recording the codes it exits with.
fn start_at(cx: &mut TestAppContext, root: &Path) -> Rc<RefCell<Vec<ExitCode>>> {
    let exits = start_unparked(cx, root);
    cx.run_until_parked();
    exits
}

/// As [`start_at`], letting nothing run after the start: work in the background has not
/// landed.
fn start_unparked(cx: &mut TestAppContext, root: &Path) -> Rc<RefCell<Vec<ExitCode>>> {
    let exits = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&exits);
    let root = root.to_owned();
    cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::resolve_queue::start(
            merge_app::resolve_queue::Queue::new(
                root,
                MergeOptions::default(),
                WindowSettings::default(),
                move |code, _: &mut App| record.borrow_mut().push(code),
            )
            .with_jj_config(ConfigEnv::default()),
            cx,
        );
    });
    exits
}

/// Each open merge window and its view.
fn views(cx: &mut TestAppContext) -> Vec<(AnyWindowHandle, Entity<MergeWindow>)> {
    cx.update(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|w| Some((w, w.downcast::<Root>()?)))
            .filter_map(|(w, root)| {
                let view = root.read(cx).ok()?.view().clone();
                Some((w, view.downcast::<MergeWindow>().ok()?))
            })
            .collect()
    })
}

/// Each open merge window and the file it writes.
fn windows(cx: &mut TestAppContext) -> Vec<(AnyWindowHandle, PathBuf)> {
    views(cx)
        .into_iter()
        .filter_map(|(w, view)| {
            let output = cx.read(|cx| view.read(cx).output().map(Path::to_path_buf))?;
            Some((w, output))
        })
        .collect()
}

/// The summary of the commit the origin column names for `pane`'s `row`: the hover's last
/// line.
fn origin(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
    pane: usize,
    row: usize,
) -> Option<String> {
    let cell = cx.read(|cx| view.read(cx).origin_cell(pane, RowIx::new(row)))?;
    cell.hover.lines().last().map(str::to_owned)
}

/// Each row's origin in the left and right panes of the queue's one window.
fn pane_origins(cx: &mut TestAppContext) -> [Vec<Option<String>>; 2] {
    let open = views(cx);
    assert_eq!(open.len(), 1, "one window");
    let view = &open[0].1;
    [0, 1].map(|pane| (0..3).map(|row| origin(cx, view, pane, row)).collect())
}

fn owned(summaries: [Option<&str>; 3]) -> Vec<Option<String>> {
    summaries.iter().map(|s| s.map(str::to_owned)).collect()
}

const ONE: &[u8] = b"a\nb\nc\n";
const LEFT_ONE: &[u8] = b"L\nb\nc\n";
const RIGHT_ONE: &[u8] = b"R\nb\nc\n";

#[gpui_kit::test]
fn a_git_conflict_shows_each_sides_origins(cx: &mut TestAppContext) {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[("a.txt", ONE)]);
    fixture.commit("left", &[("a.txt", LEFT_ONE)]);
    let theirs = fixture.commit_on(&[one], "right", &[("a.txt", RIGHT_ONE)]);
    let terms = [ONE, LEFT_ONE, RIGHT_ONE].map(|t| Some(Entry::File(t)));
    fixture.stage_conflicts(&[], &[("a.txt", terms)]);
    fixture.write_pseudo_ref("MERGE_HEAD", theirs);
    start(cx, &fixture);
    assert_eq!(
        pane_origins(cx),
        [
            owned([Some("left"), Some("one"), Some("one")]),
            owned([Some("right"), Some("one"), Some("one")])
        ]
    );
}

#[gpui_kit::test]
fn a_git_conflicts_headings_read_title_then_chip_on_every_side(cx: &mut TestAppContext) {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[("a.txt", ONE)]);
    let ours = fixture.commit("left", &[("a.txt", LEFT_ONE)]).to_string();
    let theirs = fixture.commit_on(&[one], "right", &[("a.txt", RIGHT_ONE)]);
    let terms = [ONE, LEFT_ONE, RIGHT_ONE].map(|t| Some(Entry::File(t)));
    fixture.stage_conflicts(&[], &[("a.txt", terms)]);
    fixture.write_pseudo_ref("MERGE_HEAD", theirs);
    start(cx, &fixture);
    let open = views(cx);
    let view = &open[0].1;
    let heading = |cx: &mut TestAppContext, pane| {
        let text = cx.read(|cx| view.read(cx).heading_text(pane)).expect("the pane exists");
        let id = text.chip.and_then(|chip| chip.label).expect("every side names its commit");
        (text.title.to_string(), id.to_string())
    };
    // Ours is titled by its branch; theirs by `<commit> <summary>`, whose commit the chip
    // takes.
    let (left_title, left_id) = heading(cx, 0);
    assert_eq!(left_title, "main");
    assert!(ours.starts_with(&left_id), "{left_id} shortens {ours}");
    let (right_title, right_id) = heading(cx, 1);
    assert_eq!(right_title, "right");
    let theirs = theirs.to_string();
    assert!(theirs.starts_with(&right_id), "{right_id} shortens {theirs}");
}

fn the_window(cx: &mut TestAppContext) -> (AnyWindowHandle, PathBuf) {
    let open = windows(cx);
    assert_eq!(open.len(), 1, "one window at a time: {open:?}");
    open.into_iter().next().expect("one window")
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
    cx.run_until_parked();
}

/// Save the window's conflict with markers.
fn save_with_markers(cx: &mut TestAppContext, window: AnyWindowHandle) {
    press(cx, window, "secondary-s");
    cx.simulate_prompt_answer(SAVE_WITH_CONFLICTS);
    cx.run_until_parked();
}

/// Pick the left side of the window's only conflict, then save.
fn save_resolved(cx: &mut TestAppContext, window: AnyWindowHandle) {
    press(cx, window, "secondary-alt-down");
    press(cx, window, "secondary-1");
    press(cx, window, "secondary-s");
}

#[gpui_kit::test]
fn each_save_opens_the_next_conflicted_file(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt", "b.txt"]);
    let root = canonical(fixture.root().as_std_path());
    let exits = start(cx, &fixture);
    let (first, file) = the_window(cx);
    assert_eq!(file, root.join("a.txt"));
    save_with_markers(cx, first);
    // git keeps a.txt's stages, so only the files already opened tell it apart.
    let (second, file) = the_window(cx);
    assert_eq!(file, root.join("b.txt"));
    assert_eq!(exits.borrow().as_slice(), []);
    save_resolved(cx, second);
    assert_eq!(windows(cx), []);
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Conflicts]);
    assert!(
        std::fs::read_to_string(root.join("a.txt")).expect("written").starts_with("<<<<<<< main\n")
    );
    assert_eq!(std::fs::read_to_string(root.join("b.txt")).expect("written"), "left\n");
    // The resolved file is staged, as `git add` stages it; the one with markers is not.
    assert_eq!(vcs::Repository::conflicts(&fixture.open()).expect("listed"), ["a.txt"]);
}

#[gpui_kit::test]
fn the_queue_lists_and_loads_off_the_ui_thread(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt", "b.txt"]);
    let root = canonical(fixture.root().as_std_path());
    let exits = start_unparked(cx, fixture.root().as_std_path());
    assert_eq!(windows(cx), [], "the first file waits for the background executor");
    cx.run_until_parked();
    let (first, file) = the_window(cx);
    assert_eq!(file, root.join("a.txt"));
    act(cx, first, |window, cx| window.press("secondary-s", cx));
    cx.simulate_prompt_answer(SAVE_WITH_CONFLICTS);
    // The save, the close and the next file's list and load each wait for the executor.
    cx.run_until_parked();
    let (_, file) = the_window(cx);
    assert_eq!(file, root.join("b.txt"));
    assert_eq!(exits.borrow().as_slice(), []);
}

#[gpui_kit::test]
fn a_save_closes_its_merge_window_and_leaves_the_about_window_open(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt", "b.txt"]);
    let root = canonical(fixture.root().as_std_path());
    start(cx, &fixture);
    let (first, _) = the_window(cx);
    cx.update(|cx| cx.dispatch_action(&merge_app::About));
    cx.run_until_parked();
    let about = cx
        .update(|cx| cx.windows().into_iter().find(|w| w.window_id() != first.window_id()))
        .expect("the About window is open");
    save_with_markers(cx, first);
    let (_, file) = the_window(cx);
    assert_eq!(file, root.join("b.txt"));
    let open: Vec<_> =
        cx.update(|cx| cx.windows().iter().map(AnyWindowHandle::window_id).collect());
    assert!(open.contains(&about.window_id()), "the About window stays: {open:?}");
    assert!(!open.contains(&first.window_id()), "the saved window closes: {open:?}");
}

#[gpui_kit::test]
fn a_save_git_cannot_stage_offers_to_try_again(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt"]);
    let lock = fixture.write(".git/index.lock", b"");
    let exits = start(cx, &fixture);
    let (window, _) = the_window(cx);
    save_resolved(cx, window);
    assert!(cx.has_pending_prompt(), "an alert says why");
    assert_eq!(exits.borrow().as_slice(), []);
    std::fs::remove_file(lock).expect("the lock is removed");
    cx.simulate_prompt_answer(TRY_AGAIN);
    cx.run_until_parked();
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Resolved]);
    assert!(
        vcs::Repository::conflicts(&fixture.open()).expect("listed").is_empty(),
        "a.txt is staged"
    );
}

/// The queue on one git conflict, `a.txt`, saved resolved to left while another git command
/// holds the index, with the alert that follows still open. Returns the run's exit codes
/// and the lock.
fn save_unrecorded(
    cx: &mut TestAppContext,
    fixture: &vcs_git::testing::Fixture,
) -> (Rc<RefCell<Vec<ExitCode>>>, PathBuf) {
    let lock = fixture.write(".git/index.lock", b"").into_std_path_buf();
    let exits = start(cx, fixture);
    let (window, _) = the_window(cx);
    save_resolved(cx, window);
    let (message, detail) = cx.pending_prompt().expect("an alert says why");
    assert_eq!(message, CANNOT_RECORD);
    assert!(
        detail.starts_with("a.txt is written, but the repository did not record the save: "),
        "{detail}"
    );
    assert_eq!(std::fs::read(fixture.root().join("a.txt")).expect("written"), b"left\n");
    assert_eq!(exits.borrow().as_slice(), []);
    (exits, lock)
}

#[gpui_kit::test]
fn try_again_after_an_unrecorded_save_records_it_without_writing_again(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt"]);
    let (exits, lock) = save_unrecorded(cx, &fixture);
    std::fs::remove_file(lock).expect("the lock is removed");
    // A write would put back `left`; the record stages what the file holds.
    fixture.write("a.txt", b"by hand\n");
    cx.simulate_prompt_answer(TRY_AGAIN);
    cx.run_until_parked();
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Resolved]);
    assert_eq!(std::fs::read(fixture.root().join("a.txt")).expect("kept"), b"by hand\n");
    assert_eq!(vcs::Repository::conflicts(&fixture.open()).expect("listed"), [""; 0]);
}

#[gpui_kit::test]
fn quit_after_an_unrecorded_save_ends_the_run_with_3(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt"]);
    let (exits, _lock) = save_unrecorded(cx, &fixture);
    cx.simulate_prompt_answer(QUIT);
    cx.run_until_parked();
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Error]);
    assert_eq!(std::fs::read(fixture.root().join("a.txt")).expect("written"), b"left\n");
}

#[gpui_kit::test]
fn every_file_resolved_exits_0(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt", "b.txt"]);
    let exits = start(cx, &fixture);
    for _ in 0..2 {
        let (window, _) = the_window(cx);
        save_resolved(cx, window);
    }
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Resolved]);
}

#[gpui_kit::test]
fn a_refused_file_is_skipped_and_the_run_ends_with_1(cx: &mut TestAppContext) {
    // a.txt comes first, and is deleted on every side.
    let fixture = stopped_with(&["b.txt"], &["a.txt"]);
    let root = canonical(fixture.root().as_std_path());
    let exits = start(cx, &fixture);
    let (window, file) = the_window(cx);
    assert_eq!(file, root.join("b.txt"));
    save_resolved(cx, window);
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Conflicts]);
    assert!(!root.join("a.txt").exists());
}

#[gpui_kit::test]
fn every_file_refused_ends_the_run_with_1_and_no_window(cx: &mut TestAppContext) {
    let fixture = stopped_with(&[], &["a.txt"]);
    let exits = start(cx, &fixture);
    assert_eq!(windows(cx), []);
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Conflicts]);
}

#[gpui_kit::test]
fn cancel_ends_the_run_with_2(cx: &mut TestAppContext) {
    let fixture = stopped(&["a.txt", "b.txt"]);
    let exits = start(cx, &fixture);
    let (window, _) = the_window(cx);
    // An edit makes Cancel ask before discarding.
    press(cx, window, "secondary-alt-down");
    press(cx, window, "secondary-1");
    press(cx, window, "secondary-w");
    cx.simulate_prompt_answer(DISCARD);
    cx.run_until_parked();
    assert_eq!(exits.borrow().as_slice(), [ExitCode::Cancelled]);
    assert_eq!(
        std::fs::read(fixture.root().join("b.txt")).ok(),
        None,
        "the next file never opened"
    );
}

mod jj {
    use super::*;

    #[gpui_kit::test]
    fn a_jj_conflict_shows_each_sides_origins(cx: &mut TestAppContext) {
        let fixture = vcs_jj::testing::Fixture::new();
        let one = fixture.commit_on(&[], "one", &[("a.txt", ONE)]);
        let left = fixture.commit_on(&[&one], "left", &[("a.txt", LEFT_ONE)]);
        let right = fixture.commit_on(&[&one], "right", &[("a.txt", RIGHT_ONE)]);
        fixture.check_out(&[&left, &right]);
        fixture.check_out_to_disk();
        start_at(cx, fixture.root().as_std_path());
        assert_eq!(
            pane_origins(cx),
            [
                owned([Some("left"), Some("one"), Some("one")]),
                owned([Some("right"), Some("one"), Some("one")])
            ]
        );
    }

    #[gpui_kit::test]
    fn a_jj_save_is_an_operation_of_its_own_and_ends_the_conflict(cx: &mut TestAppContext) {
        let fixture = vcs_jj::testing::Fixture::new();
        let one = fixture.commit_on(&[], "one", &[("a.txt", ONE)]);
        let left = fixture.commit_on(&[&one], "left", &[("a.txt", LEFT_ONE)]);
        let right = fixture.commit_on(&[&one], "right", &[("a.txt", RIGHT_ONE)]);
        fixture.check_out(&[&left, &right]);
        fixture.check_out_to_disk();
        let exits = start_at(cx, fixture.root().as_std_path());
        let (window, _) = the_window(cx);
        save_resolved(cx, window);
        // jj no longer lists a.txt, so the run ends.
        assert_eq!(windows(cx), []);
        assert_eq!(exits.borrow().as_slice(), [ExitCode::Resolved]);
        assert_eq!(
            fixture.operations()[0],
            Op { description: "save a.txt in triad".to_owned(), is_snapshot: false }
        );
        assert_eq!(fixture.held_at(0, "a.txt"), Held::File(LEFT_ONE.to_vec()));
        assert_eq!(fixture.held_at(1, "a.txt"), Held::Conflict);
    }
}
