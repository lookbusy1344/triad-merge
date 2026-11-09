//! Headless tests of search in the input panes: the result's search drives every pane, and
//! each input pane steps through its own matches (spec §6.3).

use crate::common;

use std::fmt::Write as _;
use std::time::Duration;

use common::{act, doc, open};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, HighlightStyle, Hsla, TestAppContext, rgb};
use merge_app::MergeWindow;
use merge_app::window::{RESULT_ID, cell_id, input_match_next_id, input_match_previous_id};
use merge_config::settings::DEFAULT_COLOURS;
use merge_core::document::MergeDocument;
use merge_core::merge3::Source;
use merge_view::rows::RowIx;

const NO_MATCHES: &str = "No matches";
const LEFT_PANE: usize = 0;
const RIGHT_PANE: usize = 1;

/// Rows: 0 `a`, 1 the conflict `bb|B`, 2 `c`. Ignoring case, `b` matches twice on the left
/// and once on the right.
fn uneven() -> MergeDocument {
    doc("a\nb\nc\n", "a\nbb\nc\n", "a\nB\nc\n")
}

const NUMBERED_LINES: usize = 200;

/// 200 unchanged lines, `l1` to `l200`: more rows than the window shows.
fn numbered() -> MergeDocument {
    let text = (1..=NUMBERED_LINES).fold(String::new(), |mut text, n| {
        writeln!(text, "l{n}").expect("a String takes any write");
        text
    });
    doc(&text, &text, &text)
}

fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    act(cx, window, |window, cx| window.press(key, cx));
    cx.run_until_parked();
}

fn type_text(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    act(cx, window, |window, cx| window.input(text, cx));
    cx.run_until_parked();
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: gpui_kit::ElementId) {
    act(cx, window, |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

/// Open `doc`, focus the result and open its search with ⌘F.
fn searching(
    cx: &mut TestAppContext,
    doc: MergeDocument,
) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let (window, view) = open(cx, doc);
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    press(cx, window, "secondary-f");
    (window, view)
}

/// Set the result search's query and case, as its panel does.
fn set_query(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
    query: &str,
    case_insensitive: bool,
) {
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| {
            state.set_search_query(query, case_insensitive, cx);
        });
    });
    cx.run_until_parked();
}

fn close_search(cx: &mut TestAppContext, view: &Entity<MergeWindow>) {
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| {
            state.close_search(cx);
        });
    });
    cx.run_until_parked();
}

/// Pane `pane`'s match count; `None` while no search runs.
fn count(cx: &mut TestAppContext, view: &Entity<MergeWindow>, pane: usize) -> Option<String> {
    cx.read(|cx| view.read(cx).input_match_count(pane))
}

/// Pane `pane`'s current match: its row, input, line and bytes in the line.
fn current(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
    pane: usize,
) -> Option<(usize, Source, usize, std::ops::Range<usize>)> {
    cx.read(|cx| {
        view.read(cx)
            .input_match_current(pane)
            .map(|hit| (hit.row.get(), hit.source, hit.line, hit.bytes.clone()))
    })
}

fn counts(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> [Option<String>; 2] {
    [LEFT_PANE, RIGHT_PANE].map(|pane| count(cx, view, pane))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "builds the expected value of a count, which is an Option"
)]
fn some(text: &str) -> Option<String> {
    Some(text.to_owned())
}

fn result_query(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> String {
    cx.read(|cx| view.read(cx).result_editor().read(cx).search_session().query.clone())
}

/// Type `text` one key at a time, as a person does.
fn type_keys(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
    for key in text.chars() {
        type_text(cx, window, key.encode_utf8(&mut [0; 4]));
    }
}

#[gpui_kit::test]
fn each_key_typed_in_the_results_search_adds_to_the_query(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, doc("base\n", "base\n", "base\n"));
    type_keys(cx, window, "base");
    assert_eq!(result_query(cx, &view), "base");
}

#[gpui_kit::test]
fn each_key_adds_to_the_query_when_the_result_scrolls_to_a_match(cx: &mut TestAppContext) {
    let mut text = (1..=NUMBERED_LINES).fold(String::new(), |mut text, n| {
        writeln!(text, "l{n}").expect("a String takes any write");
        text
    });
    text.push_str("bxxx\nbaxx\nbasx\nbase\n");
    let (window, view) = searching(cx, doc(&text, &text, &text));
    type_keys(cx, window, "base");
    assert_eq!(result_query(cx, &view), "base");
}

#[gpui_kit::test]
fn typing_in_the_results_search_searches_every_input_pane(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    type_text(cx, window, "b");
    assert_eq!(
        cx.read(|cx| view.read(cx).result_editor().read(cx).search_session().query.clone()),
        "b",
        "the typing went to the result's search field"
    );
    assert_eq!(counts(cx, &view), [some("1 of 2"), some("1 of 1")]);
    assert_eq!(current(cx, &view, LEFT_PANE), Some((1, Source::Side(0), 1, 0..1)));
    assert_eq!(current(cx, &view, RIGHT_PANE), Some((1, Source::Side(1), 1, 0..1)));
}

#[gpui_kit::test]
fn the_case_setting_reaches_the_input_panes(cx: &mut TestAppContext) {
    let (_window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", false);
    assert_eq!(counts(cx, &view), [some("1 of 2"), some(NO_MATCHES)]);
    set_query(cx, &view, "B", false);
    assert_eq!(counts(cx, &view), [some(NO_MATCHES), some("1 of 1")]);
    set_query(cx, &view, "B", true);
    assert_eq!(counts(cx, &view), [some("1 of 2"), some("1 of 1")]);
}

#[gpui_kit::test]
fn each_pane_steps_through_its_own_matches_and_wraps(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", true);
    click(cx, window, input_match_next_id(LEFT_PANE));
    assert_eq!(counts(cx, &view), [some("2 of 2"), some("1 of 1")]);
    assert_eq!(current(cx, &view, LEFT_PANE), Some((1, Source::Side(0), 1, 1..2)));
    click(cx, window, input_match_next_id(LEFT_PANE));
    assert_eq!(count(cx, &view, LEFT_PANE), some("1 of 2"));
    click(cx, window, input_match_previous_id(LEFT_PANE));
    assert_eq!(count(cx, &view, LEFT_PANE), some("2 of 2"));
    click(cx, window, input_match_next_id(RIGHT_PANE));
    assert_eq!(counts(cx, &view), [some("2 of 2"), some("1 of 1")]);
}

#[gpui_kit::test]
fn a_step_scrolls_its_row_into_view(cx: &mut TestAppContext) {
    // `l18` matches l18 and l180 to l189 in each pane: 11 matches, the first on row 17.
    let (window, view) = searching(cx, numbered());
    set_query(cx, &view, "l18", false);
    assert_eq!(count(cx, &view, LEFT_PANE), some("1 of 11"));
    click(cx, window, input_match_next_id(LEFT_PANE));
    assert_eq!(current(cx, &view, LEFT_PANE), Some((179, Source::Side(0), 179, 0..3)));
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(cell_id(LEFT_PANE, RowIx::new(179))).is_some(), "row 179 shows");
        assert!(
            window.try_find(cell_id(LEFT_PANE, RowIx::new(0))).is_none(),
            "row 0 is scrolled away"
        );
    });
}

#[gpui_kit::test]
fn closing_the_search_hides_the_counts(cx: &mut TestAppContext) {
    let (_window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", true);
    close_search(cx, &view);
    assert_eq!(counts(cx, &view), [None, None]);
    assert_eq!(current(cx, &view, LEFT_PANE), None);
}

#[gpui_kit::test]
fn escape_in_the_search_field_hides_the_counts(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    type_text(cx, window, "b");
    press(cx, window, "escape");
    assert_eq!(counts(cx, &view), [None, None]);
}

#[gpui_kit::test]
fn an_empty_query_shows_no_counts(cx: &mut TestAppContext) {
    let (_window, view) = searching(cx, uneven());
    assert_eq!(counts(cx, &view), [None, None]);
    set_query(cx, &view, "b", true);
    set_query(cx, &view, "", true);
    assert_eq!(counts(cx, &view), [None, None]);
}

#[gpui_kit::test]
fn the_base_pane_is_searched_while_it_shows(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", false);
    press(cx, window, "secondary-b");
    // Panes: left, base, right.
    assert_eq!(count(cx, &view, 1), some("1 of 1"));
    assert_eq!(current(cx, &view, 1), Some((1, Source::Base(0), 1, 0..1)));
    assert_eq!(count(cx, &view, 2), some(NO_MATCHES));
    press(cx, window, "secondary-b");
    assert_eq!(counts(cx, &view), [some("1 of 2"), some(NO_MATCHES)]);
}

#[gpui_kit::test]
fn a_pick_keeps_each_panes_current_match(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", true);
    click(cx, window, input_match_next_id(LEFT_PANE));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    press(cx, window, "secondary-1");
    assert_eq!(counts(cx, &view), [some("2 of 2"), some("1 of 1")]);
}

/// The backgrounds of the runs top pane `pane` draws on row `row`.
fn backgrounds(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
    pane: usize,
    row: usize,
) -> Vec<(std::ops::Range<usize>, Option<Hsla>)> {
    view.update(cx, |view, cx| {
        view.cell_highlights(pane, RowIx::new(row), cx)
            .expect("the cell has a line")
            .1
            .into_iter()
            .map(|(range, style): (_, HighlightStyle)| (range, style.background_color))
            .collect()
    })
}

/// The fills of a pane's current match and its others: the editors' selection colour, and
/// that colour greyed to a saturation of 0.1, as the editors' search paints them.
fn fills(cx: &mut TestAppContext) -> (Hsla, Hsla) {
    cx.read(|cx| {
        let selection = cx.theme().selection;
        (selection, Hsla { s: 0.1, ..selection })
    })
}

#[gpui_kit::test]
fn each_panes_current_match_stands_out(cx: &mut TestAppContext) {
    // Row 1 is `cc|c`. The fills cover every matched byte, over any change mark.
    let (window, view) = searching(cx, doc("a\nc\n", "a\ncc\n", "a\nc\n"));
    let plain = backgrounds(cx, &view, LEFT_PANE, 1);
    set_query(cx, &view, "c", true);
    let (current, other) = fills(cx);
    assert_eq!(backgrounds(cx, &view, LEFT_PANE, 1), [(0..1, Some(current)), (1..2, Some(other))]);
    assert_eq!(backgrounds(cx, &view, RIGHT_PANE, 1), [(0..1, Some(current))]);
    click(cx, window, input_match_next_id(LEFT_PANE));
    wait(cx, BLINK * 5);
    assert_eq!(
        backgrounds(cx, &view, LEFT_PANE, 1),
        [(0..1, Some(other)), (1..2, Some(current))],
        "after the step's blink"
    );
    close_search(cx, &view);
    assert_eq!(backgrounds(cx, &view, LEFT_PANE, 1), plain, "the change mark alone");
}

#[gpui_kit::test]
fn showing_the_base_pane_keeps_each_panes_current_match(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", true);
    click(cx, window, input_match_next_id(LEFT_PANE));
    assert_eq!(count(cx, &view, LEFT_PANE), some("2 of 2"));
    press(cx, window, "secondary-b");
    assert_eq!(count(cx, &view, LEFT_PANE), some("2 of 2"));
    press(cx, window, "secondary-b");
    assert_eq!(counts(cx, &view), [some("2 of 2"), some("1 of 1")]);
}

/// A step blinks for 500 ms: shown and hidden in turn, each for `BLINK`, starting shown and
/// ending hidden, as a jump to a region blinks (spec §5).
const BLINK: Duration = Duration::from_millis(100);
/// The blink's samples: at the step, then after each `BLINK`.
const BLINK_SAMPLES: usize = 6;

fn wait(cx: &mut TestAppContext, by: Duration) {
    cx.executor().advance_clock(by);
    cx.run_until_parked();
}

/// The colour of the current region's frame, which a blinking match takes.
fn flash_colour(cx: &mut TestAppContext) -> Hsla {
    let dark = cx.read(|cx| cx.theme().is_dark());
    Hsla::from(rgb(DEFAULT_COLOURS.current.get(dark).hex()))
}

#[gpui_kit::test]
fn a_step_blinks_its_panes_current_match(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", true);
    let (current, other) = fills(cx);
    let flash = flash_colour(cx);
    let right = backgrounds(cx, &view, RIGHT_PANE, 1);
    // The left pane's matches are 0..1 and 1..2 of `bb`; the step goes to the second.
    click(cx, window, input_match_next_id(LEFT_PANE));
    let on = vec![(0..1, Some(other)), (1..2, Some(flash))];
    let off = vec![(0..1, Some(other)), (1..2, Some(current))];
    let mut seen = vec![backgrounds(cx, &view, LEFT_PANE, 1)];
    for _ in 1..BLINK_SAMPLES {
        wait(cx, BLINK);
        seen.push(backgrounds(cx, &view, LEFT_PANE, 1));
    }
    assert_eq!(seen, [on.clone(), off.clone(), on.clone(), off.clone(), on, off]);
    assert_eq!(backgrounds(cx, &view, RIGHT_PANE, 1), right, "the right pane did not step");
}

#[gpui_kit::test]
fn typing_a_query_blinks_no_match(cx: &mut TestAppContext) {
    let (_, view) = searching(cx, uneven());
    set_query(cx, &view, "b", true);
    let (current, other) = fills(cx);
    assert_eq!(backgrounds(cx, &view, LEFT_PANE, 1), [(0..1, Some(current)), (1..2, Some(other))]);
    assert_eq!(result_find_flash(cx, &view), None);
}

#[gpui_kit::test]
fn a_new_step_restarts_the_blink_on_its_pane(cx: &mut TestAppContext) {
    let (window, view) = searching(cx, uneven());
    set_query(cx, &view, "b", true);
    let (current, other) = fills(cx);
    let flash = flash_colour(cx);
    click(cx, window, input_match_next_id(LEFT_PANE));
    wait(cx, BLINK * 3);
    click(cx, window, input_match_next_id(RIGHT_PANE));
    assert_eq!(
        backgrounds(cx, &view, LEFT_PANE, 1),
        [(0..1, Some(other)), (1..2, Some(current))],
        "the left pane's blink stopped"
    );
    assert_eq!(backgrounds(cx, &view, RIGHT_PANE, 1), [(0..1, Some(flash))]);
}

/// The match flash the result shows now; it holds one at most.
fn result_find_flash(
    cx: &mut TestAppContext,
    view: &Entity<MergeWindow>,
) -> Option<std::ops::Range<usize>> {
    let flashes = cx.read(|cx| view.read(cx).find_flashes(cx));
    assert!(flashes.len() <= 1, "the result has flashes {flashes:?}");
    flashes.into_iter().next()
}

#[gpui_kit::test]
fn a_step_in_the_results_search_blinks_the_match(cx: &mut TestAppContext) {
    // The result is `a\nb\nb\n`: `b` at 2..3 and 4..5. The step goes to the second.
    let (_, view) = searching(cx, doc("a\nb\nb\n", "a\nb\nb\n", "a\nb\nb\n"));
    set_query(cx, &view, "b", true);
    assert_eq!(result_find_flash(cx, &view), None);
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| {
            state.next_search_match(cx);
        });
    });
    cx.run_until_parked();
    let mut seen = vec![result_find_flash(cx, &view)];
    for _ in 1..BLINK_SAMPLES {
        wait(cx, BLINK);
        seen.push(result_find_flash(cx, &view));
    }
    let (on, off) = (Some(4..5), None);
    assert_eq!(seen, [on.clone(), off.clone(), on.clone(), off.clone(), on, off]);
}
