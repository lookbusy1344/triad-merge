//! Headless tests of `TRIAD_MEASURE` in the merge window (DESIGN.md §A6.7): each mode acts once per
//! frame, reports, and finishes with 2 without writing. The test platform draws a window more
//! than once per `render_frame`, so tests draw until the run finishes and check where it
//! ended.

#![expect(clippy::panic, reason = "test helpers outside `#[test]` panic on a failed check")]

use crate::common;
use merge_core::index::RegionIx;
use merge_view::rows::RowIx;

use std::time::Instant;

use common::{Finished, act, doc, open_with, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use merge_app::measure::{Measure, Mode, SCROLL_STEP};
use merge_app::{ExitCode, MergeWindow, WindowSettings};
use merge_core::document::Resolution;
use merge_core::merge3::Source;
use merge_view::status::FileNames;

const LINES: usize = 200;
const CONFLICT_LINE: usize = 100;
/// The region of the conflict: the unchanged lines before it are region 0.
const CONFLICT: usize = 1;
const FRAMES: usize = 3;
const FRAMES_F32: f32 = 3.;

fn numbered(edit: Option<&str>) -> String {
    (0..LINES)
        .map(|n| match edit {
            Some(text) if n == CONFLICT_LINE => format!("{text}\n"),
            _ => format!("l{n}\n"),
        })
        .collect()
}

struct Opened {
    window: AnyWindowHandle,
    view: Entity<MergeWindow>,
    finished: Finished,
    output: std::path::PathBuf,
    /// The result before the run, and where the conflict starts in it.
    start: (String, usize),
    dir: tempfile::TempDir,
}

/// A merge with one conflict in the middle, measured in `mode` for [`FRAMES`] frames.
fn measured(cx: &mut TestAppContext, mode: Mode, journal: bool) -> Opened {
    let dir = tempfile::tempdir().expect("temporary directory");
    let output = dir.path().join("out.txt");
    let settings = WindowSettings {
        output: Some(output.clone()),
        journal_dir: journal.then(|| dir.path().join("journal")),
        measure: Some(Measure { mode, frames: FRAMES, started: Instant::now() }),
        ..WindowSettings::default()
    };
    let doc = doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R")));
    let start = (doc.text(), doc.region_bytes(RegionIx::new(CONFLICT)).start);
    let (window, view, finished) = open_with(cx, doc, &settings);
    Opened { window, view, finished, output, start, dir }
}

fn frames(cx: &mut TestAppContext, window: AnyWindowHandle, count: usize) {
    for _ in 0..count {
        act(cx, window, TestWindowExt::render_frame);
    }
}

/// Draws enough for any run of [`FRAMES`] frames.
const MAX_DRAWS: usize = 10 * FRAMES;

fn run_to_end(cx: &mut TestAppContext, window: AnyWindowHandle, finished: &Finished) {
    for _ in 0..MAX_DRAWS {
        if !finished.codes().is_empty() {
            return;
        }
        frames(cx, window, 1);
    }
    panic!("the run did not finish in {MAX_DRAWS} draws");
}

fn assert_reported(cx: &mut TestAppContext, opened: &Opened, mode: Mode, actions: usize) {
    let report = cx.read(|cx| opened.view.read(cx).measured().cloned()).expect("the run reported");
    assert_eq!(report.mode, mode);
    assert_eq!(report.frames.count, FRAMES);
    assert_eq!(report.work.count, actions);
    assert_eq!(opened.finished.codes(), [ExitCode::Cancelled]);
    assert!(!opened.output.exists(), "a measured run writes nothing");
}

#[gpui_kit::test]
fn scroll_rows_moves_the_rows_one_step_a_frame(cx: &mut TestAppContext) {
    let opened = measured(cx, Mode::ScrollRows, false);
    run_to_end(cx, opened.window, &opened.finished);
    let top = cx.read(|cx| opened.view.read(cx).rows_scroll_top());
    assert_eq!(top, SCROLL_STEP * FRAMES_F32);
    assert_reported(cx, &opened, Mode::ScrollRows, FRAMES);
}

#[gpui_kit::test]
fn scroll_result_moves_the_result_one_step_a_frame(cx: &mut TestAppContext) {
    let opened = measured(cx, Mode::ScrollResult, false);
    run_to_end(cx, opened.window, &opened.finished);
    let top = cx.read(|cx| -opened.view.read(cx).result_editor().read(cx).scroll_offset().y);
    assert_eq!(top, SCROLL_STEP * FRAMES_F32);
    assert_reported(cx, &opened, Mode::ScrollResult, FRAMES);
}

#[gpui_kit::test]
fn type_inserts_one_character_a_frame_at_the_middle_difference(cx: &mut TestAppContext) {
    let opened = measured(cx, Mode::Type, false);
    let (start, at) = &opened.start;
    let at = *at;
    run_to_end(cx, opened.window, &opened.finished);
    let typed = "x".repeat(FRAMES);
    let expected = format!("{}{typed}{}", &start[..at], &start[at..]);
    assert_eq!(texts(cx, &opened.view), (expected.clone(), expected));
    assert_reported(cx, &opened, Mode::Type, FRAMES);
}

#[gpui_kit::test]
fn pick_toggles_left_and_right_in_turn(cx: &mut TestAppContext) {
    let opened = measured(cx, Mode::Pick, false);
    run_to_end(cx, opened.window, &opened.finished);
    // Left on, right on, left off.
    let resolution = cx.read(|cx| opened.view.read(cx).document().resolutions()[CONFLICT].clone());
    assert_eq!(resolution, Resolution::Pick(vec![Source::Side(1)]));
    let (editor, core) = texts(cx, &opened.view);
    assert_eq!(editor, core);
    assert_reported(cx, &opened, Mode::Pick, FRAMES);
}

#[gpui_kit::test]
fn open_finishes_after_the_frame_after_the_first(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let settings = WindowSettings {
        output: Some(dir.path().join("out.txt")),
        measure: Some(Measure::new(Mode::Open, Instant::now())),
        ..WindowSettings::default()
    };
    let doc = doc(&numbered(None), &numbered(Some("L")), &numbered(Some("R")));
    let (window, view, finished) = open_with(cx, doc, &settings);
    run_to_end(cx, window, &finished);
    assert_eq!(finished.codes(), [ExitCode::Cancelled]);
    let report = cx.read(|cx| view.read(cx).measured().cloned()).expect("reported");
    assert_eq!((report.frames.count, report.work.count), (1, 0));
}

#[gpui_kit::test]
fn a_measured_run_leaves_no_journal(cx: &mut TestAppContext) {
    let opened = measured(cx, Mode::Type, true);
    run_to_end(cx, opened.window, &opened.finished);
    assert_eq!(opened.finished.codes(), [ExitCode::Cancelled]);
    // The journal write that waits for a pause in the commands never comes.
    cx.executor().advance_clock(merge_app::window::JOURNAL_DEBOUNCE * 2);
    cx.run_until_parked();
    let journal = opened.dir.path().join("journal");
    assert_eq!(std::fs::read_dir(&journal).map_or(0, Iterator::count), 0);
}

#[gpui_kit::test]
fn no_measure_draws_no_report(cx: &mut TestAppContext) {
    // No difference, so the window opens at the top, as a measured run does.
    let opened_doc = doc(&numbered(None), &numbered(None), &numbered(None));
    let (window, view, finished) = open_with(cx, opened_doc, &WindowSettings::default());
    frames(cx, window, FRAMES + 1);
    cx.run_until_parked();
    assert!(cx.read(|cx| view.read(cx).measured().is_none()));
    assert_eq!(finished.codes(), []);
    assert_eq!(cx.read(|cx| view.read(cx).rows_scroll_top()), gpui_kit::px(0.));
}

/// A Rust merge measured in [`Mode::Syntax`], its inputs and output named `name`.
fn measured_syntax(cx: &mut TestAppContext, name: &str) -> Opened {
    let dir = tempfile::tempdir().expect("temporary directory");
    let output = dir.path().join(name);
    let named = std::path::Path::new(name);
    let settings = WindowSettings {
        output: Some(output.clone()),
        file_names: FileNames::three_way(named, named, named),
        measure: Some(Measure::new(Mode::Syntax, Instant::now())),
        ..WindowSettings::default()
    };
    let doc = doc("fn a() {}\n", "fn b() {}\n", "fn a() {}\n");
    let start = (doc.text(), 0);
    let (window, view, finished) = open_with(cx, doc, &settings);
    Opened { window, view, finished, output, start, dir }
}

/// Whether the left pane's first line carries a syntax colour.
fn left_coloured(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> bool {
    const LEFT_PANE: usize = 0;
    const FIRST_ROW: usize = 0;
    cx.update(|cx| {
        view.update(cx, |view, cx| view.cell_highlights(LEFT_PANE, RowIx::new(FIRST_ROW), cx))
            .expect("the left pane has a line")
            .1
            .iter()
            .any(|(_, style)| style.color.is_some())
    })
}

#[gpui_kit::test]
fn syntax_finishes_only_once_the_chunks_on_screen_land(cx: &mut TestAppContext) {
    let opened = measured_syntax(cx, "a.rs");
    // Drawing alone runs no background work, so the parses stay out.
    frames(cx, opened.window, MAX_DRAWS);
    assert_eq!(opened.finished.codes(), []);
    assert!(!left_coloured(cx, &opened.view));
    cx.run_until_parked();
    run_to_end(cx, opened.window, &opened.finished);
    assert_eq!(opened.finished.codes(), [ExitCode::Cancelled]);
    assert!(left_coloured(cx, &opened.view));
    let report = cx.read(|cx| opened.view.read(cx).measured().cloned()).expect("the run reported");
    assert_eq!(report.mode, Mode::Syntax);
    assert_eq!((report.frames.count, report.work.count), (1, 0));
    assert!(!opened.output.exists(), "a measured run writes nothing");
}

#[gpui_kit::test]
fn syntax_on_plain_text_finishes_as_open_does(cx: &mut TestAppContext) {
    let opened = measured_syntax(cx, "a.txt");
    run_to_end(cx, opened.window, &opened.finished);
    assert_eq!(opened.finished.codes(), [ExitCode::Cancelled]);
    let report = cx.read(|cx| opened.view.read(cx).measured().cloned()).expect("the run reported");
    assert_eq!((report.mode, report.frames.count), (Mode::Syntax, 1));
}
