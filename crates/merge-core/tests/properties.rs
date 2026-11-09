//! Properties of spec §8.2 through the public API: merge output, two-way regions, the edit
//! model, and reading triad's own conflict markers back. Coverage and swapped kinds of the alignment itself are in `merge3`'s tests.
//!
//! Small alphabets make lines repeat, which is where merges go wrong. ASCII keeps every byte
//! offset a character boundary, so edits can land anywhere.

use merge_core::index::RegionIx;
use std::ops::Range;

use merge_core::diff::DiffOptions;
use merge_core::diff2;
use merge_core::document::{
    Command, EditBatch, Labels, MergeDocument, MergeOptions, Output, Resolution, TextPatch,
};
use merge_core::markers;
use merge_core::merge3::{Inputs, Kind, LEFT, RIGHT, Refine, Region, Semantics, Source};
use merge_core::text::Text;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

const PROPTEST_SEED: u64 = 0x8_2000;
const PROPTEST_CASES: u32 = 512;
const EDIT_CASES: u32 = 256;
const MAX_OPS: usize = 24;
/// Sides in the many-sided marker property: jj's snapshot layout.
const MANY_SIDES: usize = 3;

fn config(cases: u32) -> Config {
    Config { cases, rng_seed: RngSeed::Fixed(PROPTEST_SEED), ..Config::default() }
}

fn file_strategy() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(
            prop::sample::select(vec!["a\n", "b\n", "c\n", "}\n", "a \n", "\n", "a\r\n"]),
            0..12,
        ),
        prop::sample::select(vec!["", "a", "b"]),
    )
        .prop_map(|(lines, last)| lines.concat() + last)
}

/// Files whose lines include marker-like ones, which the writer must step around with longer
/// markers.
fn marked_file_strategy() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(
            prop::sample::select(vec![
                "a\n",
                "b\n",
                "a\r\n",
                "\n",
                "<<<<<<< x\n",
                "|||||||\n",
                "=======\n",
                ">>>>>>>\n",
                "+++++++\n",
                "-------\n",
                "%%%%%%%\n",
            ]),
            0..10,
        ),
        prop::sample::select(vec!["", "a", "======="]),
    )
        .prop_map(|(lines, last)| lines.concat() + last)
}

fn text(s: &str) -> Text {
    Text::decode(s.as_bytes()).expect("generated text decodes")
}

fn semantics() -> impl Strategy<Value = Semantics> {
    prop::sample::select(vec![Semantics::Git, Semantics::Jj])
}

fn refines() -> impl Strategy<Value = Refine> {
    prop::sample::select(vec![Refine::Off, Refine::Ends, Refine::Deep])
}

/// Deep refinement diffs left against right, and a diff's tie-break depends on direction, so
/// swapping the sides need not mirror its splits.
fn mirrored_refines() -> impl Strategy<Value = Refine> {
    prop::sample::select(vec![Refine::Off, Refine::Ends])
}

fn document(
    base: &str,
    left: &str,
    right: &str,
    refine: Refine,
    semantics: Semantics,
) -> MergeDocument {
    let options = MergeOptions { semantics, refine, ..MergeOptions::default() };
    MergeDocument::new(
        Inputs::three_way(text(base), text(left), text(right)),
        options,
        Labels::three_way("left", "base", "right"),
    )
    .expect("three labels fit")
}

/// Each input's ranges start at 0, follow on from each other and end at its line count.
fn covers(texts: &[Text], ranges: impl Fn(&Region) -> &[Range<usize>], regions: &[Region]) -> bool {
    texts.iter().enumerate().all(|(i, t)| {
        regions.iter().try_fold(0, |at, r| {
            let range = &ranges(r)[i];
            (range.start == at && range.end >= range.start).then_some(range.end)
        }) == Some(t.line_count())
    })
}

/// The region byte ranges follow on from each other and end at the text's length.
fn bytes_cover(doc: &MergeDocument) -> bool {
    let total = doc.text().len();
    doc.region_ixs().try_fold(0, |at, r| {
        let bytes = doc.region_bytes(r);
        (bytes.start == at).then_some(bytes.end)
    }) == Some(total)
}

fn concat_lines(t: &Text, ranges: impl Iterator<Item = Range<usize>>) -> String {
    ranges.map(|r| t.span(r)).collect()
}

/// The merge with `source` picked in every conflict, as conflict markers hold it: the writer
/// ends a section without a final newline with its conflict's marker line ending, `endings`
/// in conflict order.
fn picked_in_conflicts(doc: &MergeDocument, source: Source, endings: &[&str]) -> String {
    let mut doc = doc.clone();
    let conflicts: Vec<usize> =
        (0..doc.regions().len()).filter(|&r| doc.regions()[r].kind == Kind::Conflict).collect();
    for &region in &conflicts {
        doc.apply(Command::Pick { region: RegionIx::new(region), sources: vec![source] })
            .expect("pick");
    }
    let text = doc.text();
    let mut endings = endings.iter().copied();
    doc.region_ixs()
        .flat_map(|r| {
            let region = &text[doc.region_bytes(r)];
            let unterminated = !region.is_empty() && !region.ends_with('\n');
            let ending = if conflicts.contains(&r.get()) {
                let eol = endings.next().expect("one ending per conflict");
                if unterminated { eol } else { "" }
            } else {
                ""
            };
            [region, ending]
        })
        .collect()
}

/// Each conflict's line ending, from its close marker: a run of exactly `len` `>`.
fn close_marker_endings(file: &str, len: usize) -> Vec<&'static str> {
    file.split_inclusive('\n')
        .filter(|line| {
            let rest = line.trim_start_matches('>');
            line.len() - rest.len() == len && rest.chars().next().is_none_or(char::is_whitespace)
        })
        .map(|line| if line.ends_with("\r\n") { "\r\n" } else { "\n" })
        .collect()
}

/// Write `doc`'s conflicts as markers, parse them back, and compare with each input picked in
/// every conflict. A merge without conflicts writes no markers and checks nothing.
fn markers_read_back(doc: &MergeDocument) -> Result<(), TestCaseError> {
    let Output::Conflicted(file) = doc.output() else {
        return Ok(());
    };
    let endings = close_marker_endings(&file, doc.marker_length());
    let parsed =
        markers::parse(&text(&file)).map_err(|e| TestCaseError::fail(format!("{e}: {file:?}")))?;
    let picked = |sources: &mut dyn Iterator<Item = Source>| -> Vec<String> {
        sources.map(|s| picked_in_conflicts(doc, s, &endings)).collect()
    };
    let content =
        |t: &[Text]| -> Vec<String> { t.iter().map(|t| t.content().to_owned()).collect() };
    let (sides, bases) = (doc.inputs().sides().len(), doc.inputs().bases().len());
    prop_assert_eq!(
        content(parsed.inputs.sides()),
        picked(&mut (0..sides).map(Source::Side)),
        "{:?}",
        file
    );
    prop_assert_eq!(
        content(parsed.inputs.bases()),
        picked(&mut (0..bases).map(Source::Base)),
        "{:?}",
        file
    );
    prop_assert_eq!(&parsed.labels, doc.labels());
    prop_assert_eq!(parsed.marker_length, doc.marker_length());
    Ok(())
}

/// One step of an edit sequence. Fractions pick a region or an offset in the current text.
#[derive(Debug, Clone)]
enum Op {
    Toggle(usize, Source),
    Clear(usize),
    Pick(usize, &'static [Source]),
    Insert(usize, &'static str),
    Delete(usize, usize),
    /// Two insertions in one batch, as two cursors.
    TwoCursors(usize, usize, &'static str),
    Undo,
    Redo,
}

const L: Source = Source::Side(LEFT);
const R: Source = Source::Side(RIGHT);
const B: Source = Source::Base(0);

/// A picked or unresolved region's blocks follow on from each other inside its bytes and
/// fill them; other regions have none.
fn blocks_fill_regions(doc: &MergeDocument) -> bool {
    doc.region_ixs().all(|r| {
        let bytes = doc.region_bytes(r);
        let blocks = doc.blocks(r);
        match *doc.resolution(r) {
            Resolution::Pick(_) | Resolution::Unresolved => {
                blocks.iter().try_fold(bytes.start, |at, (_, b)| {
                    (b.start == at && b.end > b.start).then_some(b.end)
                }) == Some(bytes.end)
            }
            Resolution::Auto | Resolution::Edited => blocks.is_empty(),
        }
    })
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let source =
        prop::sample::select(vec![Source::Side(LEFT), Source::Side(RIGHT), Source::Base(0)]);
    let typed = prop::sample::select(vec!["x", "y", "\n", "z\n", "<", "\r\n"]);
    let picks: Vec<&'static [Source]> = vec![&[], &[L], &[R], &[L, R], &[R, L], &[B, R, L]];
    prop_oneof![
        (any::<usize>(), source).prop_map(|(r, s)| Op::Toggle(r, s)),
        any::<usize>().prop_map(Op::Clear),
        (any::<usize>(), prop::sample::select(picks)).prop_map(|(r, s)| Op::Pick(r, s)),
        (any::<usize>(), typed.clone()).prop_map(|(at, t)| Op::Insert(at, t)),
        (any::<usize>(), 1..8usize).prop_map(|(at, len)| Op::Delete(at, len)),
        (any::<usize>(), any::<usize>(), typed).prop_map(|(a, b, t)| Op::TwoCursors(a, b, t)),
        Just(Op::Undo),
        Just(Op::Redo),
    ]
}

fn insert(at: usize, text: &str) -> TextPatch {
    TextPatch { range: at..at, text: text.to_owned() }
}

/// The command for `op` on `doc`'s current text and regions.
fn command(op: &Op, doc: &MergeDocument) -> Command {
    let len = doc.text().len();
    let regions = doc.regions().len();
    match *op {
        Op::Toggle(r, source) => Command::Toggle { region: RegionIx::new(r % regions), source },
        Op::Clear(r) => Command::Clear { region: RegionIx::new(r % regions) },
        Op::Pick(r, sources) => {
            Command::Pick { region: RegionIx::new(r % regions), sources: sources.to_vec() }
        }
        Op::Insert(at, t) => Command::Edit(EditBatch(vec![insert(at % (len + 1), t)])),
        Op::Delete(at, n) => {
            let start = at % (len + 1);
            Command::Edit(EditBatch(vec![TextPatch {
                range: start..(start + n).min(len),
                text: String::new(),
            }]))
        }
        Op::TwoCursors(a, b, t) => {
            let (a, b) = (a % (len + 1), b % (len + 1));
            Command::Edit(EditBatch(vec![insert(a.min(b), t), insert(a.max(b), t)]))
        }
        Op::Undo => Command::Undo,
        Op::Redo => Command::Redo,
    }
}

proptest! {
    #![proptest_config(config(PROPTEST_CASES))]

    #[test]
    fn picking_one_input_everywhere_gives_that_input(
        base in file_strategy(), left in file_strategy(), right in file_strategy(),
        refine in refines(), semantics in semantics(),
    ) {
        for (source, expected) in [
            (Source::Side(LEFT), &left),
            (Source::Side(RIGHT), &right),
            (Source::Base(0), &base),
        ] {
            let mut doc = document(&base, &left, &right, refine, semantics);
            for region in 0..doc.regions().len() {
                doc.apply(Command::Pick { region: RegionIx::new(region), sources: vec![source] }).expect("pick");
            }
            prop_assert_eq!(&doc.output(), &Output::Resolved(expected.clone()), "{:?}", source);
        }
    }

    #[test]
    fn swapping_sides_keeps_the_automatic_text_of_resolved_regions(
        base in file_strategy(), left in file_strategy(), right in file_strategy(),
        refine in mirrored_refines(), semantics in semantics(),
    ) {
        let forward = document(&base, &left, &right, refine, semantics);
        let backward = document(&base, &right, &left, refine, semantics);
        prop_assert_eq!(forward.regions().len(), backward.regions().len());
        let output = |d: &MergeDocument, r: usize| d.text()[d.region_bytes(RegionIx::new(r))].to_owned();
        for r in 0..forward.regions().len() {
            if forward.regions()[r].kind != Kind::Conflict {
                prop_assert_eq!(output(&forward, r), output(&backward, r), "region {}", r);
            }
        }
    }

    #[test]
    fn a_side_equal_to_base_gives_the_other_side(
        base in file_strategy(), other in file_strategy(), refine in refines(),
        semantics in semantics(),
    ) {
        let doc = |left: &str, right: &str| document(&base, left, right, refine, semantics);
        prop_assert_eq!(doc(&base, &other).output(), Output::Resolved(other.clone()));
        prop_assert_eq!(doc(&other, &base).output(), Output::Resolved(other.clone()));
    }

    /// Swapped inputs need not give mirrored regions: where two diffs are equally short, the
    /// tie-break depends on direction, as in `git diff`. Both directions must cover both files.
    #[test]
    fn two_way_regions_cover_both_files_in_either_direction(
        left in file_strategy(), right in file_strategy(),
    ) {
        for (a, b) in [(&left, &right), (&right, &left)] {
            let (ta, tb) = (text(a), text(b));
            let regions = diff2::regions(&ta, &tb, DiffOptions::default());
            prop_assert_eq!(&concat_lines(&ta, regions.iter().map(|x| x.left.clone())), a);
            prop_assert_eq!(&concat_lines(&tb, regions.iter().map(|x| x.right.clone())), b);
            prop_assert!(regions.iter().all(|x| !(x.left.is_empty() && x.right.is_empty())) || regions.len() == 1);
        }
    }
}

proptest! {
    #![proptest_config(config(PROPTEST_CASES))]

    #[test]
    fn triads_two_sided_markers_read_back_as_the_merge(
        base in marked_file_strategy(), left in marked_file_strategy(),
        right in marked_file_strategy(), refine in refines(), semantics in semantics(),
    ) {
        let options = MergeOptions { semantics, refine, ..MergeOptions::default() };
        let doc = MergeDocument::new(
            Inputs::three_way(text(&base), text(&left), text(&right)),
            options,
            Labels::three_way("ours", "anc", "theirs"),
        )
        .expect("three labels fit");
        markers_read_back(&doc)?;
    }

    #[test]
    fn triads_many_sided_markers_read_back_as_the_merge(
        sides in prop::collection::vec(marked_file_strategy(), MANY_SIDES),
        bases in prop::collection::vec(marked_file_strategy(), MANY_SIDES - 1),
        refine in refines(),
    ) {
        let texts = |files: &[String]| files.iter().map(|f| text(f)).collect();
        let options = MergeOptions { semantics: Semantics::Jj, refine, ..MergeOptions::default() };
        let doc = MergeDocument::new(
            Inputs::new(texts(&sides), texts(&bases)).expect("three sides, two bases"),
            options,
            Labels::new(
                vec!["one".to_owned(), "two".to_owned(), "three".to_owned()],
                vec!["b1".to_owned(), "b2".to_owned()],
            ),
        )
        .expect("five labels fit");
        markers_read_back(&doc)?;
    }
}

proptest! {
    #![proptest_config(config(EDIT_CASES))]

    /// After every command the regions' bytes cover the text and their line ranges cover the
    /// inputs; undoing everything restores the starting text and regions.
    #[test]
    fn edit_sequences_keep_regions_and_text_consistent(
        base in file_strategy(), left in file_strategy(), right in file_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..MAX_OPS), semantics in semantics(),
    ) {
        let inputs = Inputs::three_way(text(&base), text(&left), text(&right));
        let mut doc = document(&base, &left, &right, Refine::Ends, semantics);
        let start = (doc.output(), doc.regions().to_vec(), doc.resolutions().to_vec());
        for op in &ops {
            let command = command(op, &doc);
            doc.apply(command.clone()).expect("generated commands fit");
            prop_assert!(bytes_cover(&doc), "after {:?}", command);
            prop_assert!(blocks_fill_regions(&doc), "after {:?}", command);
            prop_assert!(covers(inputs.bases(), |r| &r.bases, doc.regions()), "after {:?}", command);
            prop_assert!(covers(inputs.sides(), |r| &r.sides, doc.regions()), "after {:?}", command);
            prop_assert_eq!(doc.regions().len(), doc.resolutions().len());
            prop_assert_eq!(
                doc.output().is_resolved(),
                !doc.resolutions().contains(&Resolution::Unresolved)
            );
        }
        while !doc.apply(Command::Undo).expect("undo").patches.is_empty() {}
        prop_assert_eq!(
            (doc.output(), doc.regions().to_vec(), doc.resolutions().to_vec()),
            start
        );
    }
}
