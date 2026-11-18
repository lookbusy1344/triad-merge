//! Lines picked one by one inside a region (spec §4.4, `PLAN.md` §G8). Expected texts are
//! written by hand from the inputs.

use std::ops::Range;

use merge_core::document::{
    Applied, Command, CommandError, Labels, MergeDocument, Output, Resolution,
};
use merge_core::edit::{EditBatch, TextPatch};
use merge_core::index::RegionIx;
use merge_core::journal::Journal;
use merge_core::line_pick::PickedLines;
use merge_core::merge3::{Kind, Source};
use merge_core::testing::doc;

const L: Source = Source::Side(0);
const R: Source = Source::Side(1);
const B: Source = Source::Base(0);

/// The conflict in [`two_line_conflict`].
const CONFLICT: usize = 1;

/// Unchanged `a`, a conflict of two lines on each input, unchanged `c`.
fn two_line_conflict() -> MergeDocument {
    let d = doc("a\nb1\nb2\nc\n", "a\nL1\nL2\nc\n", "a\nR1\nR2\nc\n");
    assert_eq!(d.regions()[CONFLICT].kind, Kind::Conflict);
    assert_eq!(d.regions()[CONFLICT].sides, [1..3, 1..3]);
    d
}

fn toggle_lines(region: usize, source: Source, lines: Range<usize>) -> Command {
    Command::ToggleLines { region: RegionIx::new(region), source, lines }
}

fn picked(source: Source, lines: Range<usize>) -> PickedLines {
    PickedLines::new(source, lines)
}

fn apply(d: &mut MergeDocument, command: Command) {
    d.apply(command).expect("applies");
}

#[test]
fn a_line_picked_in_an_unresolved_conflict_is_its_only_text() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, R, 0..1));
    assert_eq!(d.text(), "a\nR1\nc\n");
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Lines(vec![picked(R, 0..1)]));
    assert_eq!(d.output(), Output::Resolved("a\nR1\nc\n".to_owned()));
}

#[test]
fn lines_go_in_click_order_across_the_sides() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, R, 0..1));
    apply(&mut d, toggle_lines(CONFLICT, L, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, L, 0..1));
    assert_eq!(d.text(), "a\nR1\nL2\nR2\nL1\nc\n");
}

#[test]
fn a_pick_target_replaces_line_picks_with_its_whole_side() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, L, 0..1));
    apply(&mut d, Command::Toggle { region: RegionIx::new(CONFLICT), source: L });
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Pick(vec![L]));
    assert_eq!(d.text(), "a\nL1\nL2\nc\n");
}

#[test]
fn base_lines_picked_first_go_first() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, B, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, L, 0..1));
    assert_eq!(d.text(), "a\nb2\nL1\nc\n");
}

#[test]
fn a_line_pick_adds_to_a_whole_side_pick() {
    let mut d = two_line_conflict();
    apply(&mut d, Command::Pick { region: RegionIx::new(CONFLICT), sources: vec![R] });
    apply(&mut d, toggle_lines(CONFLICT, L, 0..1));
    assert_eq!(d.text(), "a\nR1\nR2\nL1\nc\n");
    assert_eq!(
        d.resolutions()[CONFLICT],
        Resolution::Lines(vec![picked(R, 0..2), picked(L, 0..1)])
    );
}

#[test]
fn a_line_pick_on_a_reordered_pick_keeps_its_order() {
    let mut d = two_line_conflict();
    apply(&mut d, Command::Pick { region: RegionIx::new(CONFLICT), sources: vec![R, L] });
    assert_eq!(d.text(), "a\nR1\nR2\nL1\nL2\nc\n");
    apply(&mut d, toggle_lines(CONFLICT, L, 0..1));
    assert_eq!(d.text(), "a\nR1\nR2\nL2\nc\n");
}

#[test]
fn lines_that_cover_whole_sources_collapse_to_a_pick_in_their_order() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, R, 0..1));
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, L, 0..2));
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Pick(vec![R, L]));
    assert_eq!(d.text(), "a\nR1\nR2\nL1\nL2\nc\n");
}

#[test]
fn every_line_of_a_side_in_reverse_order_stays_lines() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, R, 0..1));
    assert_eq!(
        d.resolutions()[CONFLICT],
        Resolution::Lines(vec![picked(R, 1..2), picked(R, 0..1)])
    );
    assert_eq!(d.text(), "a\nR2\nR1\nc\n");
}

#[test]
fn turning_off_a_conflicts_last_line_leaves_it_unresolved() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, L, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, L, 1..2));
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Unresolved);
    assert_eq!(d.text(), "a\nL1\nL2\nR1\nR2\nc\n");
    assert!(!d.output().is_resolved());
}

#[test]
fn turning_off_a_one_side_regions_lines_leaves_an_empty_pick() {
    let mut d = doc("a\nc\n", "a\nc\n", "a\nR1\nR2\nc\n");
    assert_eq!(d.regions()[1].kind, Kind::OneSide(1));
    apply(&mut d, toggle_lines(1, R, 0..1));
    assert_eq!(d.text(), "a\nR2\nc\n");
    assert_eq!(d.resolutions()[1], Resolution::Lines(vec![picked(R, 1..2)]));
    apply(&mut d, toggle_lines(1, R, 1..2));
    assert_eq!(d.resolutions()[1], Resolution::Pick(vec![]));
    assert_eq!(d.text(), "a\nc\n");
}

#[test]
fn a_range_toggles_as_one_command() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, L, 0..2));
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Pick(vec![L]));
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, L, 0..2));
    assert_eq!(d.text(), "a\nR2\nc\n", "every line of left was picked, so the range turns off");
}

#[test]
fn a_last_line_without_a_newline_takes_the_typed_ending_before_another_line() {
    let mut d = doc("a\nb", "a\nL1\nL2", "a\nR1\nR2");
    assert_eq!(d.regions()[CONFLICT].kind, Kind::Conflict);
    apply(&mut d, toggle_lines(CONFLICT, L, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    assert_eq!(d.text(), "a\nL2\nR2");
}

#[test]
fn a_line_pick_replaces_an_edit_and_undo_restores_it() {
    let mut d = two_line_conflict();
    // Result "a\nL1\nL2\nR1\nR2\nc\n": replace the conflict's bytes 2..14.
    let edit = EditBatch(vec![TextPatch { range: 2..14, text: "X\n".to_owned() }]);
    apply(&mut d, Command::Edit(edit));
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Edited);
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    assert_eq!(d.text(), "a\nR2\nc\n");
    apply(&mut d, Command::Undo);
    assert_eq!(d.text(), "a\nX\nc\n");
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Edited);
}

#[test]
fn undo_and_redo_step_through_line_picks() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, L, 0..1));
    apply(&mut d, toggle_lines(CONFLICT, R, 1..2));
    apply(&mut d, Command::Undo);
    assert_eq!(d.text(), "a\nL1\nc\n");
    apply(&mut d, Command::Undo);
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Unresolved);
    assert!(!d.changed());
    apply(&mut d, Command::Redo);
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Lines(vec![picked(L, 0..1)]));
    assert!(d.changed());
}

#[test]
fn blocks_give_each_run_of_picked_lines_its_bytes() {
    let mut d = two_line_conflict();
    apply(&mut d, toggle_lines(CONFLICT, L, 1..2));
    apply(&mut d, toggle_lines(CONFLICT, R, 0..1));
    apply(&mut d, toggle_lines(CONFLICT, B, 0..1));
    // In pick order: "a\n" then "L2\n" at 2..5, "R1\n" at 5..8, "b1\n" at 8..11.
    assert_eq!(d.blocks(RegionIx::new(CONFLICT)), [(L, 2..5), (R, 5..8), (B, 8..11)]);
}

#[test]
fn line_selection_reads_every_resolution_as_lines() {
    let mut d = two_line_conflict();
    let selection = |d: &MergeDocument| d.line_selection(RegionIx::new(CONFLICT));
    assert_eq!(selection(&d), [], "an unresolved conflict picks no line");
    apply(&mut d, Command::Pick { region: RegionIx::new(CONFLICT), sources: vec![R, L] });
    assert_eq!(selection(&d), [picked(R, 0..2), picked(L, 0..2)]);
    apply(&mut d, toggle_lines(CONFLICT, R, 0..1));
    assert_eq!(selection(&d), [picked(R, 1..2), picked(L, 0..2)]);
    let one_side = doc("a\n", "a\n", "a\nR1\n");
    assert_eq!(one_side.line_selection(RegionIx::new(1)), [picked(R, 0..1)]);
}

#[test]
fn lines_outside_the_region_are_refused_and_change_nothing() {
    let mut d = two_line_conflict();
    let before = d.text();
    for lines in [1..3, 2..3, 1..1] {
        assert_eq!(
            d.apply(toggle_lines(CONFLICT, L, lines.clone())),
            Err(CommandError::NoLines { input: L, lines, available: 2 })
        );
    }
    assert_eq!(
        d.apply(toggle_lines(CONFLICT, Source::Side(2), 0..1)),
        Err(CommandError::NoInput { input: Source::Side(2) })
    );
    assert_eq!(
        d.apply(toggle_lines(3, L, 0..1)),
        Err(CommandError::NoRegion { region: RegionIx::new(3), regions: 3 })
    );
    assert_eq!(d.text(), before);
    assert_eq!(d.resolutions()[CONFLICT], Resolution::Unresolved);
    assert_eq!(d.apply(Command::Undo), Ok(Applied::default()), "no step was recorded");
}

#[test]
fn a_journal_replays_line_picks() {
    let mut d = two_line_conflict();
    let mut journal = Journal::new(&d);
    for command in [toggle_lines(CONFLICT, R, 1..2), toggle_lines(CONFLICT, L, 0..1)] {
        apply(&mut d, command.clone());
        journal.record(command);
    }
    let bytes = journal.to_lines().expect("serialises");
    let restored = Journal::parse(&bytes)
        .expect("parses")
        .restore(d.inputs().clone(), Labels::none())
        .expect("restores");
    assert_eq!(restored.text(), "a\nR2\nL1\nc\n");
    assert_eq!(restored.resolutions(), d.resolutions());
}
