//! Headless tests of line origins in the merge and diff windows (spec §5, §7.3; PLAN.md §C5.4).

use crate::common;
use merge_view::rows::RowIx;

use common::{Finished, act, doc, open_with};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, OwnedMenuItem, TestAppContext};
use merge_app::diff_window::{
    DIFF_DEBOUNCE, DiffFile, DiffLookup, DiffSettings, DiffWindow, Origin as DiffOrigin, pane_id,
};
use merge_app::origin_column::{OriginCell, Visibility};
use merge_app::window::{OriginLookup, WindowSettings};
use merge_app::{MergeWindow, ToggleOriginColumn};
use std::ops::Range;
use vcs::IdKind;

use gpui_kit::component::input::EditorState;
use merge_core::diff2::Side;
use merge_core::document::MergeDocument;
use merge_core::testing::text;
use merge_session::repository::{Handoff, RunRepository};

const PATH: &str = "a.txt";
const BASE: &str = "a\nb\nc\n";
const LEFT: &str = "A\nb\nc\n";
const RIGHT: &str = "a\nb\nC\n";
/// The git fixture's author time, the epoch, as the hover shows it.
const FIXTURE_TIME: &str = "triad, 1970-01-01 00:00 +00:00";

fn merge() -> MergeDocument {
    doc(BASE, LEFT, RIGHT)
}

/// A git repository stopped in a merge of `left` into `HEAD` at `right`, as `git mergetool`
/// finds it, with the full IDs of the left and right commits.
fn stopped_merge() -> (vcs_git::testing::Fixture, String, String) {
    let fixture = vcs_git::testing::Fixture::new();
    let one = fixture.commit("one", &[(PATH, BASE.as_bytes())]);
    let left = fixture.commit("left", &[(PATH, LEFT.as_bytes())]);
    let right = fixture.commit_on(&[one], "right", &[(PATH, RIGHT.as_bytes())]);
    fixture.write_pseudo_ref("MERGE_HEAD", right);
    fixture.write(PATH, b"<<<<<<< ours\n");
    (fixture, left.to_string(), right.to_string())
}

fn lookup(fixture: &vcs_git::testing::Fixture) -> OriginLookup {
    let output = fixture.root().join(PATH).into_std_path_buf();
    let cwd = fixture.root().as_std_path().to_owned();
    OriginLookup {
        repo: Handoff::new(RunRepository::of_merge(Some(&cwd), Some(&output))),
        output,
        title: None,
        cwd,
    }
}

fn cell(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
    pane: usize,
    row: usize,
) -> Option<OriginCell> {
    cx.read(|cx| view.read(cx).origin_cell(pane, RowIx::new(row)))
}

fn column(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> bool {
    cx.read(|cx| view.read(cx).origin_column())
}

#[gpui_kit::test]
fn origins_arrive_after_the_window_shows(cx: &mut TestAppContext) {
    let (fixture, left, right) = stopped_merge();
    let settings = WindowSettings { origins: Some(lookup(&fixture)), ..WindowSettings::default() };
    let (_window, view, _) = open_with(cx, merge(), &settings);
    assert_eq!(cell(cx, &view, 0, 0), None, "nothing before the load finishes");

    cx.run_until_parked();
    // Rows: A | b | C. Left: A from `left`, b and c from `one`. Right: a, b from `one`, C
    // from `right`.
    let OriginCell { label, hover } = cell(cx, &view, 0, 0).expect("the left side has origins");
    let label = label.expect("a run starts here").to_string();
    assert!(left.starts_with(&label), "{label} shortens {left}");
    assert_eq!(hover.as_ref(), format!("{label}\n{FIXTURE_TIME}\nleft"));

    let one = cell(cx, &view, 0, 1).expect("b has an origin");
    let one_label = one.label.expect("a new run starts at b").to_string();
    assert_eq!(one.hover.as_ref(), format!("{one_label}\n{FIXTURE_TIME}\none"));
    assert_eq!(cell(cx, &view, 0, 2).map(|c| c.label), Some(None), "c continues b's run");

    let right_c = cell(cx, &view, 1, 2).expect("C has an origin");
    let right_label = right_c.label.expect("a new run starts at C").to_string();
    assert!(right.starts_with(&right_label), "{right_label} shortens {right}");
    assert_eq!(
        cell(cx, &view, 1, 0).map(|c| c.label),
        Some(Some(one_label.into())),
        "right's a starts a run from `one`"
    );
    assert_eq!(cell(cx, &view, 1, 1).map(|c| c.label), Some(None), "right's b continues it");
}

#[gpui_kit::test]
fn the_merge_window_reads_origins_from_the_repository_handed_to_it(cx: &mut TestAppContext) {
    let (fixture, left, _) = stopped_merge();
    let handoff = Handoff::new(RunRepository::find([fixture.root().as_std_path()]));
    let settings = WindowSettings {
        origins: Some(OriginLookup { repo: handoff.clone(), ..lookup(&fixture) }),
        ..WindowSettings::default()
    };
    let (_window, view, _) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    assert!(handoff.is_taken(), "the window took the repository");
    let label = cell(cx, &view, 0, 0).and_then(|cell| cell.label).expect("A has an origin");
    assert!(left.starts_with(label.as_ref()), "{label} shortens {left}");
}

#[gpui_kit::test]
fn a_run_outside_any_repository_shows_no_origins(cx: &mut TestAppContext) {
    let outside = tempfile::tempdir().expect("a temporary directory");
    let settings = WindowSettings {
        origins: Some(OriginLookup {
            repo: Handoff::new(RunRepository::of_merge(
                Some(outside.path()),
                Some(&outside.path().join(PATH)),
            )),
            output: outside.path().join(PATH),
            title: None,
            cwd: outside.path().to_owned(),
        }),
        ..WindowSettings::default()
    };
    let (_window, view, finished) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    assert_eq!(cell(cx, &view, 0, 0), None);
    assert_eq!(cell(cx, &view, 1, 2), None);
    assert_eq!(finished.codes(), [], "the window stays open");
}

#[gpui_kit::test]
fn a_filler_row_and_a_missing_pane_have_no_origin(cx: &mut TestAppContext) {
    let (fixture, _, _) = stopped_merge();
    let settings = WindowSettings { origins: Some(lookup(&fixture)), ..WindowSettings::default() };
    let (_window, view, _) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    assert_eq!(cell(cx, &view, 2, 0), None, "two panes show");
    assert_eq!(cell(cx, &view, 0, 3), None, "three rows");
}

fn toggle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(ToggleOriginColumn), cx);
    });
}

#[gpui_kit::test]
fn the_view_menu_toggles_the_blame_column(cx: &mut TestAppContext) {
    let (fixture, _, _) = stopped_merge();
    let settings = WindowSettings { origins: Some(lookup(&fixture)), ..WindowSettings::default() };
    let (window, view, _) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    assert!(!column(cx, &view), "off by default");
    toggle(cx, window);
    assert!(column(cx, &view));
    toggle(cx, window);
    assert!(!column(cx, &view));
}

#[gpui_kit::test]
fn the_setting_shows_the_column_once_the_origins_load(cx: &mut TestAppContext) {
    let (fixture, _, _) = stopped_merge();
    let settings = WindowSettings {
        origins: Some(lookup(&fixture)),
        origin_column: Visibility::Shown,
        ..WindowSettings::default()
    };
    let (_window, view, _) = open_with(cx, merge(), &settings);
    assert!(!column(cx, &view), "hidden before the load finishes");
    assert_eq!(result_labels(cx, &view), None, "no empty result gutter");
    cx.run_until_parked();
    assert!(column(cx, &view));
    assert_eq!(result_labels(cx, &view).map(|labels| labels.len()), Some(3));
}

#[gpui_kit::test]
fn a_merge_without_origins_hides_the_column_and_ignores_the_toggle(cx: &mut TestAppContext) {
    let settings = WindowSettings { origin_column: Visibility::Shown, ..WindowSettings::default() };
    let (window, view, _) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    assert!(!column(cx, &view));
    assert_eq!(result_labels(cx, &view), None);
    toggle(cx, window);
    assert!(!column(cx, &view));
    assert_eq!(result_labels(cx, &view), None);
}

#[gpui_kit::test]
fn a_merge_outside_any_repository_hides_the_column(cx: &mut TestAppContext) {
    let outside = tempfile::tempdir().expect("a temporary directory");
    let settings = WindowSettings {
        origins: Some(OriginLookup {
            repo: Handoff::new(RunRepository::of_merge(
                Some(outside.path()),
                Some(&outside.path().join(PATH)),
            )),
            output: outside.path().join(PATH),
            title: None,
            cwd: outside.path().to_owned(),
        }),
        origin_column: Visibility::Shown,
        ..WindowSettings::default()
    };
    let (_window, view, _) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    assert!(!column(cx, &view));
    assert_eq!(result_labels(cx, &view), None);
}

/// The View menu's Blame item, as whether it is ticked; none when the menu has no such item.
fn blame_item(cx: &mut TestAppContext) -> Option<bool> {
    let menus = cx.update(|cx| cx.get_menus()).expect("the test platform keeps menus");
    menus
        .into_iter()
        .find(|menu| menu.name.as_ref() == "View")
        .expect("a View menu")
        .items
        .into_iter()
        .find_map(|item| match item {
            OwnedMenuItem::Action { name, checked, .. } if name == "Blame" => Some(checked),
            _ => None,
        })
}

/// Make `window` the active window and let it draw. Activation takes effect once the app
/// parks, so the origins load too.
fn activate(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, _| window.activate_window());
    cx.run_until_parked();
    act(cx, window, TestWindowExt::render_frame);
}

#[gpui_kit::test]
fn the_blame_item_appears_once_the_merge_origins_load(cx: &mut TestAppContext) {
    let (fixture, _, _) = stopped_merge();
    let settings = WindowSettings {
        origins: Some(lookup(&fixture)),
        origin_column: Visibility::Shown,
        ..WindowSettings::default()
    };
    let (window, _view, _) = open_with(cx, merge(), &settings);
    activate(cx, window);
    assert_eq!(blame_item(cx), Some(true), "ticked");
    toggle(cx, window);
    act(cx, window, TestWindowExt::render_frame);
    assert_eq!(blame_item(cx), Some(false), "unticked");
}

#[gpui_kit::test]
fn a_merge_without_origins_has_no_blame_item(cx: &mut TestAppContext) {
    let settings = WindowSettings { origin_column: Visibility::Shown, ..WindowSettings::default() };
    let (window, _view, _) = open_with(cx, merge(), &settings);
    activate(cx, window);
    assert_eq!(blame_item(cx), None);
}

// The diff window (PLAN.md §C5.4, decision 3).

const ONE: &str = "a\nb\n";
const TWO: &str = "a\nb\nc\n";
/// The working copy: b changed, so it has no origin; a from `one`, c from `two`.
const WORKING: &str = "a\nB\nc\n";

/// A git repository whose `HEAD` and index hold [`TWO`], with [`WORKING`] in the working tree.
fn worked_on() -> vcs_git::testing::Fixture {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[(PATH, ONE.as_bytes())]);
    fixture.commit("two", &[(PATH, TWO.as_bytes())]);
    fixture.stage(&[(PATH, TWO.as_bytes())]);
    fixture.write(PATH, WORKING.as_bytes());
    fixture
}

struct Diff {
    window: AnyWindowHandle,
    view: Entity<DiffWindow>,
    finished: Finished,
}

fn open_diff(cx: &mut TestAppContext, left: DiffFile, right: DiffFile, lookup: DiffLookup) -> Diff {
    let settings = DiffSettings { origins: Some(lookup), ..DiffSettings::default() };
    open_diff_with(cx, left, right, &settings)
}

fn open_diff_with(
    cx: &mut TestAppContext,
    left: DiffFile,
    right: DiffFile,
    settings: &DiffSettings,
) -> Diff {
    let (window, view, finished) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        let finished = Finished::install(cx);
        let (window, view) =
            merge_app::diff_window::open(left, right, settings, cx).expect("window opens");
        (window, view, finished)
    });
    act(cx, window, TestWindowExt::render_frame);
    Diff { window, view, finished }
}

/// `diff PATH` on [`worked_on`]: `HEAD` against the working-tree file.
fn open_native(cx: &mut TestAppContext, fixture: &vcs_git::testing::Fixture) -> Diff {
    let path = fixture.root().join(PATH).into_std_path_buf();
    let right =
        DiffFile { origin: DiffOrigin::File(path.clone()), text: text(WORKING), read_only: false };
    let lookup =
        DiffLookup::Native { repo: Handoff::new(RunRepository::of_file(&path)), path, rev: None };
    open_diff(cx, DiffFile::revision(text(TWO), "HEAD"), right, lookup)
}

/// Let the origins load and the regions recompute.
fn settle(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(DIFF_DEBOUNCE);
    cx.run_until_parked();
}

/// The summary of each line's origin in `side`, from its hover card; `None` for a line
/// without one. Each label must shorten the commit the hover names.
fn summaries(
    cx: &mut TestAppContext,
    view: &Entity<DiffWindow>,
    side: Side,
) -> Vec<Option<String>> {
    let lines = cx.read(|cx| view.read(cx).document().text(side).len_lines() - 1);
    (0..lines)
        .map(|line| {
            cx.read(|cx| view.read(cx).origin_cell(side, line)).map(|cell| {
                let hover = cell.hover.to_string();
                let (id, rest) = hover.split_once('\n').expect("the hover names the commit");
                if let Some(label) = cell.label {
                    assert!(id.starts_with(label.as_ref()), "{label} shortens {id}");
                }
                let (time, summary) = rest.split_once('\n').expect("time, then summary");
                assert_eq!(time, FIXTURE_TIME);
                summary.to_owned()
            })
        })
        .collect()
}

fn owned(summaries: &[Option<&str>]) -> Vec<Option<String>> {
    summaries.iter().map(|s| s.map(str::to_owned)).collect()
}

fn labelled(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side) -> Vec<bool> {
    let lines = cx.read(|cx| view.read(cx).document().text(side).len_lines() - 1);
    (0..lines)
        .map(|line| {
            cx.read(|cx| view.read(cx).origin_cell(side, line))
                .is_some_and(|cell| cell.label.is_some())
        })
        .collect()
}

fn type_at(cx: &mut TestAppContext, d: &Diff, side: Side, at: usize, typed: &str) {
    act(cx, d.window, |window, cx| window.click(pane_id(side), cx));
    d.view.update(cx, |view, cx| {
        view.editor(side).update(cx, |state, cx| state.set_selected_range(at..at, cx));
    });
    act(cx, d.window, |window, cx| window.input(typed, cx));
}

#[gpui_kit::test]
fn a_diff_shows_the_revisions_origins_and_none_on_changed_lines(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let d = open_native(cx, &fixture);
    assert_eq!(summaries(cx, &d.view, Side::Left), [None, None, None]);
    settle(cx);
    assert_eq!(summaries(cx, &d.view, Side::Left), owned(&[Some("one"), Some("one"), Some("two")]));
    assert_eq!(labelled(cx, &d.view, Side::Left), [true, false, true]);
    assert_eq!(summaries(cx, &d.view, Side::Right), owned(&[Some("one"), None, Some("two")]));
    assert_eq!(labelled(cx, &d.view, Side::Right), [true, false, true]);
}

#[gpui_kit::test]
fn an_edit_drops_the_changed_lines_origins_and_moves_the_rest(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let d = open_native(cx, &fixture);
    settle(cx);
    type_at(cx, &d, Side::Right, 0, "new\n");
    settle(cx);
    assert_eq!(summaries(cx, &d.view, Side::Right), owned(&[None, Some("one"), None, Some("two")]));
    // Undo brings the text back, and with it the origins.
    act(cx, d.window, |window, cx| window.press("secondary-z", cx));
    settle(cx);
    assert_eq!(summaries(cx, &d.view, Side::Right), owned(&[Some("one"), None, Some("two")]));
}

#[gpui_kit::test]
fn a_reload_keeps_the_origins_of_the_lines_the_file_still_holds(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let d = open_native(cx, &fixture);
    settle(cx);
    fixture.write(PATH, b"a\nc\nlast\n");
    let view = d.view.clone();
    act(cx, d.window, |window, cx| {
        view.update(cx, |view, cx| view.disk_changed(Side::Right, window, cx));
    });
    settle(cx);
    assert_eq!(summaries(cx, &d.view, Side::Right), owned(&[Some("one"), Some("two"), None]));
}

#[gpui_kit::test]
fn the_diff_window_reads_origins_from_the_repository_handed_to_it(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let path = fixture.root().join(PATH).into_std_path_buf();
    let handoff = Handoff::new(RunRepository::find([fixture.root().as_std_path()]));
    let right =
        DiffFile { origin: DiffOrigin::File(path.clone()), text: text(WORKING), read_only: false };
    let lookup = DiffLookup::Native { path, rev: None, repo: handoff.clone() };
    let d = open_diff(cx, DiffFile::revision(text(TWO), "HEAD"), right, lookup);
    settle(cx);
    assert!(handoff.is_taken(), "the window took the repository");
    assert_eq!(summaries(cx, &d.view, Side::Left), owned(&[Some("one"), Some("one"), Some("two")]));
}

#[gpui_kit::test]
fn a_diff_outside_any_repository_shows_no_origins(cx: &mut TestAppContext) {
    let outside = tempfile::tempdir().expect("a temporary directory");
    let paths = ["left.txt", "right.txt"].map(|name| outside.path().join(name));
    let files = paths.each_ref().map(|path| {
        std::fs::write(path, ONE).expect("written");
        DiffFile { origin: DiffOrigin::File(path.clone()), text: text(ONE), read_only: false }
    });
    let [left, right] = files;
    let lookup = DiffLookup::Tool {
        repo: Handoff::new(RunRepository::of_diff_tool(Some(outside.path()), &paths[1])),
        paths,
        title: None,
        cwd: outside.path().to_owned(),
    };
    let d = open_diff(cx, left, right, lookup);
    settle(cx);
    assert_eq!(summaries(cx, &d.view, Side::Left), [None, None]);
    assert_eq!(summaries(cx, &d.view, Side::Right), [None, None]);
    assert_eq!(d.finished.codes(), [], "the window stays open");
}

#[gpui_kit::test]
fn a_difftool_run_finds_its_origins_by_the_copies_bytes(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let copies = tempfile::tempdir().expect("a temporary directory");
    let local = copies.path().join("a_LOCAL_1234.txt");
    std::fs::write(&local, TWO).expect("written");
    let worktree = fixture.root().join(PATH).into_std_path_buf();
    let left =
        DiffFile { origin: DiffOrigin::File(local.clone()), text: text(TWO), read_only: false };
    let right = DiffFile {
        origin: DiffOrigin::File(worktree.clone()),
        text: text(WORKING),
        read_only: false,
    };
    let lookup = DiffLookup::Tool {
        repo: Handoff::new(RunRepository::of_diff_tool(
            Some(fixture.root().as_std_path()),
            &worktree,
        )),
        paths: [local, worktree],
        title: None,
        cwd: fixture.root().as_std_path().to_owned(),
    };
    let d = open_diff(cx, left, right, lookup);
    settle(cx);
    assert_eq!(summaries(cx, &d.view, Side::Left), owned(&[Some("one"), Some("one"), Some("two")]));
    assert_eq!(summaries(cx, &d.view, Side::Right), owned(&[Some("one"), None, Some("two")]));
}

#[gpui_kit::test]
fn navigation_before_the_recompute_carries_the_origins_too(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let d = open_native(cx, &fixture);
    settle(cx);
    type_at(cx, &d, Side::Right, 0, "new\n");
    // Navigation recomputes at once, without the debounce, in place of the recompute that
    // was waiting.
    act(cx, d.window, |window, cx| {
        window.press("secondary-down", cx);
    });
    cx.run_until_parked();
    assert_eq!(summaries(cx, &d.view, Side::Right), owned(&[None, Some("one"), None, Some("two")]));
    settle(cx);
    assert_eq!(summaries(cx, &d.view, Side::Right), owned(&[None, Some("one"), None, Some("two")]));
}

// Labels in the editors' gutters: the result pane and the diff panes.

/// Each label of `editor`'s gutter column, as its lines and text; none without the column.
fn editor_labels(
    cx: &mut TestAppContext,
    editor: &Entity<EditorState>,
) -> Option<Vec<(Range<usize>, String)>> {
    cx.read(|cx| {
        editor.read(cx).line_labels().map(|labels| {
            labels
                .labels()
                .iter()
                .map(|label| (label.lines().clone(), label.label().to_string()))
                .collect()
        })
    })
}

fn result_labels(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
) -> Option<Vec<(Range<usize>, String)>> {
    let editor = cx.read(|cx| view.read(cx).result_editor().clone());
    editor_labels(cx, &editor)
}

/// The summary of each result line's origin; `None` for a line without one.
fn result_summaries(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Vec<Option<String>> {
    let lines = cx.read(|cx| view.read(cx).document().line_count());
    (0..lines)
        .map(|line| {
            cx.read(|cx| view.read(cx).result_origin(line)).map(|cell| {
                let hover = cell.hover.to_string();
                hover.rsplit('\n').next().expect("a summary").to_owned()
            })
        })
        .collect()
}

/// The short ID `view` labels result line `line` with.
fn result_label(cx: &mut TestAppContext, view: &Entity<MergeWindow>, line: usize) -> String {
    cx.read(|cx| view.read(cx).result_origin(line))
        .and_then(|cell| cell.label)
        .expect("a labelled line")
        .to_string()
}

fn merge_with_column(
    cx: &mut TestAppContext,
) -> (vcs_git::testing::Fixture, AnyWindowHandle, Entity<MergeWindow>) {
    let (fixture, _, _) = stopped_merge();
    let settings = WindowSettings {
        origins: Some(lookup(&fixture)),
        origin_column: Visibility::Shown,
        ..WindowSettings::default()
    };
    let (window, view, _) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    (fixture, window, view)
}

#[gpui_kit::test]
fn the_result_pane_labels_each_run_of_one_commit(cx: &mut TestAppContext) {
    let (_fixture, _window, view) = merge_with_column(cx);
    // The result A, b, C takes A from the left, b from the base, C from the right.
    assert_eq!(result_summaries(cx, &view), owned(&[Some("left"), Some("one"), Some("right")]));
    let expected =
        (0..3).map(|line| (line..line + 1, result_label(cx, &view, line))).collect::<Vec<_>>();
    assert_eq!(result_labels(cx, &view), Some(expected));
}

#[gpui_kit::test]
fn an_edited_result_line_loses_its_label(cx: &mut TestAppContext) {
    let (_fixture, window, view) = merge_with_column(cx);
    let (one, right) = (result_label(cx, &view, 1), result_label(cx, &view, 2));
    common::set_caret(cx, &view, 0);
    act(cx, window, |window, cx| window.input("x", cx));
    assert_eq!(result_summaries(cx, &view), owned(&[None, Some("one"), Some("right")]));
    assert_eq!(result_labels(cx, &view), Some(vec![(1..2, one), (2..3, right)]));
}

#[gpui_kit::test]
fn the_result_pane_shows_labels_only_with_the_column(cx: &mut TestAppContext) {
    let (_fixture, window, view) = merge_with_column(cx);
    toggle(cx, window);
    assert_eq!(result_labels(cx, &view), None);
    toggle(cx, window);
    assert_eq!(result_labels(cx, &view).map(|labels| labels.len()), Some(3));
}

fn pane_labels(
    cx: &mut TestAppContext,
    view: &Entity<DiffWindow>,
    side: Side,
) -> Option<Vec<Range<usize>>> {
    let editor = cx.read(|cx| view.read(cx).editor(side).clone());
    editor_labels(cx, &editor).map(|labels| labels.into_iter().map(|(lines, _)| lines).collect())
}

#[gpui_kit::test]
fn the_diff_panes_label_runs_and_follow_edits(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let path = fixture.root().join(PATH).into_std_path_buf();
    let right =
        DiffFile { origin: DiffOrigin::File(path.clone()), text: text(WORKING), read_only: false };
    let settings = DiffSettings {
        origins: Some(DiffLookup::Native {
            repo: Handoff::new(RunRepository::of_file(&path)),
            path,
            rev: None,
        }),
        origin_column: Visibility::Shown,
        ..DiffSettings::default()
    };
    let d = open_diff_with(cx, DiffFile::revision(text(TWO), "HEAD"), right, &settings);
    settle(cx);
    assert_eq!(pane_labels(cx, &d.view, Side::Left), Some(vec![0..2, 2..3]));
    assert_eq!(pane_labels(cx, &d.view, Side::Right), Some(vec![0..1, 2..3]));
    let label = cx
        .read(|cx| d.view.read(cx).origin_cell(Side::Right, 0))
        .and_then(|cell| cell.label)
        .expect("a labelled line");
    let editor = cx.read(|cx| d.view.read(cx).editor(Side::Right).clone());
    assert_eq!(
        editor_labels(cx, &editor).map(|labels| labels[0].1.clone()),
        Some(label.to_string())
    );
    type_at(cx, &d, Side::Right, 0, "new\n");
    settle(cx);
    assert_eq!(pane_labels(cx, &d.view, Side::Right), Some(vec![1..2, 3..4]));
    act(cx, d.window, |window, cx| {
        window.dispatch_action(Box::new(ToggleOriginColumn), cx);
    });
    assert_eq!(pane_labels(cx, &d.view, Side::Right), None);
}

#[gpui_kit::test]
fn the_diff_panes_have_no_label_column_by_default(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let d = open_native(cx, &fixture);
    settle(cx);
    assert_eq!(pane_labels(cx, &d.view, Side::Left), None);
    assert_eq!(pane_labels(cx, &d.view, Side::Right), None);
}

/// jj shows IDs with at least this many characters.
const JJ_SHORT_ID: usize = 8;

/// The first label of the left pane of `diff PATH` in a jj repository whose `@-` holds
/// [`TWO`], under `revision_id`, and that commit.
fn jj_left_label(
    cx: &mut TestAppContext,
    revision_id: IdKind,
) -> (String, vcs_jj::testing::Committed) {
    let fixture = vcs_jj::testing::Fixture::new();
    let two = fixture.commit("two", &[(PATH, TWO.as_bytes())]);
    fixture.write(PATH, WORKING.as_bytes());
    let path = fixture.root().join(PATH).into_std_path_buf();
    let right =
        DiffFile { origin: DiffOrigin::File(path.clone()), text: text(WORKING), read_only: false };
    let settings = DiffSettings {
        origins: Some(DiffLookup::Native {
            repo: Handoff::new(RunRepository::of_file(&path)),
            path,
            rev: None,
        }),
        revision_id,
        ..DiffSettings::default()
    };
    let d = open_diff_with(cx, DiffFile::revision(text(TWO), "@-"), right, &settings);
    settle(cx);
    let label = cx
        .read(|cx| d.view.read(cx).origin_cell(Side::Left, 0))
        .and_then(|cell| cell.label)
        .expect("the first line is labelled");
    (label.to_string(), two)
}

#[gpui_kit::test]
fn a_jj_origin_column_shows_change_ids_by_default(cx: &mut TestAppContext) {
    let (label, two) = jj_left_label(cx, IdKind::default());
    assert!(
        label.len() >= JJ_SHORT_ID && two.change_id.starts_with(&label),
        "{label} shortens the change ID {}",
        two.change_id
    );
}

#[gpui_kit::test]
fn a_jj_origin_column_shows_commits_under_the_commit_setting(cx: &mut TestAppContext) {
    let (label, two) = jj_left_label(cx, IdKind::Commit);
    assert!(
        label.len() >= JJ_SHORT_ID && two.commit.starts_with(&label),
        "{label} shortens the commit {}",
        two.commit
    );
}

// The revision chips of the pane headings (spec §5).

/// Each pane's chip, in pane order, as its ID and the summary its hover ends with.
fn merge_chips(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
) -> Vec<Option<(String, String)>> {
    let panes = cx.read(|cx| view.read(cx).pane_titles().count());
    (0..panes).map(|pane| cx.read(|cx| view.read(cx).revision_chip(pane)).map(chip_parts)).collect()
}

fn chip_parts(chip: OriginCell) -> (String, String) {
    let id = chip.label.expect("a chip names its commit").to_string();
    let summary = chip.hover.lines().last().expect("the hover has lines").to_owned();
    (id, summary)
}

#[gpui_kit::test]
fn a_git_merges_headings_name_each_inputs_commit_once_loaded(cx: &mut TestAppContext) {
    let (fixture, left, right) = stopped_merge();
    let settings = WindowSettings {
        origins: Some(lookup(&fixture)),
        base_pane: true,
        ..WindowSettings::default()
    };
    let (_window, view, _) = open_with(cx, merge(), &settings);
    assert_eq!(merge_chips(cx, &view), [None, None, None], "nothing before the load");
    cx.run_until_parked();
    let chips = merge_chips(cx, &view);
    let [Some((l, l_summary)), Some((b, b_summary)), Some((r, r_summary))] = chips.as_slice()
    else {
        panic!("every pane names its commit: {chips:?}");
    };
    assert!(left.starts_with(l.as_str()), "{l} shortens {left}");
    assert!(right.starts_with(r.as_str()), "{r} shortens {right}");
    assert!(b.len() >= GIT_SHORT_ID && b != l && b != r, "{b}");
    assert_eq!([l_summary, b_summary, r_summary].map(String::as_str), ["left", "one", "right"]);
}

#[gpui_kit::test]
fn a_merge_run_outside_any_repository_names_no_commit(cx: &mut TestAppContext) {
    let outside = tempfile::tempdir().expect("a temporary directory");
    let settings = WindowSettings {
        origins: Some(OriginLookup {
            repo: Handoff::new(RunRepository::of_merge(
                Some(outside.path()),
                Some(&outside.path().join(PATH)),
            )),
            output: outside.path().join(PATH),
            title: None,
            cwd: outside.path().to_owned(),
        }),
        ..WindowSettings::default()
    };
    let (_window, view, _) = open_with(cx, merge(), &settings);
    cx.run_until_parked();
    assert_eq!(merge_chips(cx, &view), [None, None]);
}

#[gpui_kit::test]
fn a_three_sided_jj_merges_headings_name_each_sides_commit(cx: &mut TestAppContext) {
    use merge_core::document::{Labels, MergeOptions};
    use merge_core::merge3::Inputs;
    const THIRD: &str = "a\nB\nc\n";
    let fixture = vcs_jj::testing::Fixture::new();
    let base = fixture.commit_on(&[], "base", &[(PATH, BASE.as_bytes())]);
    let sides = [("one", LEFT), ("two", RIGHT), ("three", THIRD)]
        .map(|(name, body)| fixture.commit_on(&[&base], name, &[(PATH, body.as_bytes())]));
    fixture.check_out(&sides.each_ref());
    let inputs =
        Inputs::new([LEFT, RIGHT, THIRD].map(text).to_vec(), [BASE, BASE].map(text).to_vec())
            .expect("three sides, two bases");
    let doc =
        MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
    let cwd = fixture.root().as_std_path().to_owned();
    let output = cwd.join(PATH);
    let settings = WindowSettings {
        origins: Some(OriginLookup {
            repo: Handoff::new(RunRepository::of_merge(Some(&cwd), Some(&output))),
            output,
            title: Some(PATH.to_owned()),
            cwd,
        }),
        ..WindowSettings::default()
    };
    let (_window, view, _) = open_with(cx, doc, &settings);
    cx.run_until_parked();
    let summaries: Vec<Option<String>> =
        merge_chips(cx, &view).into_iter().map(|chip| chip.map(|(_, summary)| summary)).collect();
    assert_eq!(summaries, owned(&[Some("one"), Some("two"), Some("three")]));
}

/// git shortens commit IDs to at least this many characters.
const GIT_SHORT_ID: usize = 7;

fn diff_chip(cx: &mut TestAppContext, d: &Diff, side: Side) -> Option<(String, String)> {
    cx.read(|cx| d.view.read(cx).revision_chip(side)).map(chip_parts)
}

#[gpui_kit::test]
fn a_git_diff_names_heads_commit_and_none_for_the_working_tree(cx: &mut TestAppContext) {
    let fixture = vcs_git::testing::Fixture::new();
    fixture.commit("one", &[(PATH, ONE.as_bytes())]);
    let head = fixture.commit("two", &[(PATH, TWO.as_bytes())]).to_string();
    fixture.stage(&[(PATH, TWO.as_bytes())]);
    fixture.write(PATH, WORKING.as_bytes());
    let d = open_native(cx, &fixture);
    assert_eq!(diff_chip(cx, &d, Side::Left), None, "nothing before the load");
    settle(cx);
    let (id, summary) = diff_chip(cx, &d, Side::Left).expect("HEAD is a commit");
    assert!(id.len() >= GIT_SHORT_ID && head.starts_with(&id), "{id} shortens {head}");
    assert_eq!(summary, "two");
    assert_eq!(diff_chip(cx, &d, Side::Right), None);
}

/// `diff PATH` in a jj repository whose `@-` holds [`TWO`], its left pane titled as `title`
/// gives for that commit, under `revision_id`; and that commit.
fn open_jj_native(
    cx: &mut TestAppContext,
    title: impl FnOnce(&vcs_jj::testing::Committed) -> String,
    revision_id: IdKind,
) -> (Diff, vcs_jj::testing::Fixture, vcs_jj::testing::Committed) {
    let fixture = vcs_jj::testing::Fixture::new();
    let two = fixture.commit("two", &[(PATH, TWO.as_bytes())]);
    let title = title(&two);
    fixture.write(PATH, WORKING.as_bytes());
    let path = fixture.root().join(PATH).into_std_path_buf();
    let right =
        DiffFile { origin: DiffOrigin::File(path.clone()), text: text(WORKING), read_only: false };
    let settings = DiffSettings {
        origins: Some(DiffLookup::Native {
            repo: Handoff::new(RunRepository::of_file(&path)),
            path,
            rev: None,
        }),
        revision_id,
        ..DiffSettings::default()
    };
    let d = open_diff_with(cx, DiffFile::revision(text(TWO), title), right, &settings);
    settle(cx);
    (d, fixture, two)
}

#[gpui_kit::test]
fn a_jj_diff_names_the_parent_and_the_working_copy_by_change_id(cx: &mut TestAppContext) {
    let (d, _fixture, two) = open_jj_native(cx, |_| "@-".to_owned(), IdKind::Change);
    let (left, summary) = diff_chip(cx, &d, Side::Left).expect("@- is a commit");
    assert!(
        left.len() >= JJ_SHORT_ID && two.change_id.starts_with(&left),
        "{left} shortens {}",
        two.change_id
    );
    assert_eq!(summary, "two");
    let (right, _) = diff_chip(cx, &d, Side::Right).expect("the working copy is @");
    assert!(right.len() >= JJ_SHORT_ID, "{right}");
    assert!(!two.change_id.starts_with(&right), "@ is another change: {right}");
}

#[gpui_kit::test]
fn a_jj_diff_names_commits_under_the_commit_setting(cx: &mut TestAppContext) {
    let (d, _fixture, two) = open_jj_native(cx, |_| "@-".to_owned(), IdKind::Commit);
    let (left, _) = diff_chip(cx, &d, Side::Left).expect("@- is a commit");
    assert!(
        left.len() >= JJ_SHORT_ID && two.commit.starts_with(&left),
        "{left} shortens {}",
        two.commit
    );
}

/// The title of a jj term without a label: its change ID and summary (`vcs::Term::title`).
fn jj_term_title(two: &vcs_jj::testing::Committed) -> String {
    format!("{} two", &two.change_id[..JJ_SHORT_ID])
}

#[gpui_kit::test]
fn a_title_that_starts_with_the_change_id_gives_it_to_the_chip(cx: &mut TestAppContext) {
    let (d, _fixture, two) = open_jj_native(cx, jj_term_title, IdKind::Change);
    let text = cx.read(|cx| d.view.read(cx).heading_text(Side::Left));
    assert_eq!(text.title.as_ref(), "two");
    let (left, _) = text.chip.map(chip_parts).expect("the chip names the change");
    assert_eq!(left, two.change_id[..JJ_SHORT_ID]);
}

#[gpui_kit::test]
fn a_title_that_names_the_change_loses_it_beside_the_commit_chip(cx: &mut TestAppContext) {
    let (d, _fixture, two) = open_jj_native(cx, jj_term_title, IdKind::Commit);
    let text = cx.read(|cx| d.view.read(cx).heading_text(Side::Left));
    assert_eq!(text.title.as_ref(), "two", "the title names neither ID");
    let (left, _) = text.chip.map(chip_parts).expect("the chip names the commit");
    assert!(two.commit.starts_with(&left), "{left} shortens {}", two.commit);
}

// The diff window without origins (spec §7.3).

fn diff_with_column(cx: &mut TestAppContext, origins: Option<DiffLookup>) -> Diff {
    let settings =
        DiffSettings { origins, origin_column: Visibility::Shown, ..DiffSettings::default() };
    open_diff_with(
        cx,
        DiffFile::revision(text(TWO), "HEAD"),
        DiffFile::revision(text(WORKING), "working copy"),
        &settings,
    )
}

#[gpui_kit::test]
fn a_diff_without_origins_hides_the_column_and_ignores_the_toggle(cx: &mut TestAppContext) {
    let d = diff_with_column(cx, None);
    settle(cx);
    assert_eq!(pane_labels(cx, &d.view, Side::Left), None);
    assert_eq!(pane_labels(cx, &d.view, Side::Right), None);
    act(cx, d.window, |window, cx| {
        window.dispatch_action(Box::new(ToggleOriginColumn), cx);
    });
    assert_eq!(pane_labels(cx, &d.view, Side::Left), None);
    activate(cx, d.window);
    assert_eq!(blame_item(cx), None);
}

#[gpui_kit::test]
fn the_diff_column_shows_once_the_origins_load(cx: &mut TestAppContext) {
    let fixture = worked_on();
    let path = fixture.root().join(PATH).into_std_path_buf();
    let lookup =
        DiffLookup::Native { repo: Handoff::new(RunRepository::of_file(&path)), path, rev: None };
    let d = diff_with_column(cx, Some(lookup));
    assert_eq!(pane_labels(cx, &d.view, Side::Left), None, "hidden before the load");
    activate(cx, d.window);
    settle(cx);
    act(cx, d.window, TestWindowExt::render_frame);
    assert_eq!(pane_labels(cx, &d.view, Side::Left), Some(vec![0..2, 2..3]));
    assert_eq!(blame_item(cx), Some(true), "ticked");
}

/// Whether `window` takes Blame, which its key binding needs.
fn blame_available(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    let mut available = false;
    act(cx, window, |window, cx| {
        available = window.is_action_available(&ToggleOriginColumn, cx);
    });
    available
}

#[gpui_kit::test]
fn a_merge_takes_blame_only_once_its_origins_load(cx: &mut TestAppContext) {
    let (fixture, _, _) = stopped_merge();
    let settings = WindowSettings { origins: Some(lookup(&fixture)), ..WindowSettings::default() };
    let (window, _view, _) = open_with(cx, merge(), &settings);
    assert!(!blame_available(cx, window), "not before the load");
    cx.run_until_parked();
    act(cx, window, TestWindowExt::render_frame);
    assert!(blame_available(cx, window));
}

#[gpui_kit::test]
fn a_merge_without_origins_never_takes_blame(cx: &mut TestAppContext) {
    let (window, _view, _) = open_with(cx, merge(), &WindowSettings::default());
    cx.run_until_parked();
    act(cx, window, TestWindowExt::render_frame);
    assert!(!blame_available(cx, window));
}

#[gpui_kit::test]
fn a_diff_takes_blame_only_once_its_origins_load(cx: &mut TestAppContext) {
    let without = diff_with_column(cx, None);
    settle(cx);
    act(cx, without.window, TestWindowExt::render_frame);
    assert!(!blame_available(cx, without.window), "no origins");

    let fixture = worked_on();
    let d = open_native(cx, &fixture);
    assert!(!blame_available(cx, d.window), "not before the load");
    settle(cx);
    act(cx, d.window, TestWindowExt::render_frame);
    assert!(blame_available(cx, d.window));
}
