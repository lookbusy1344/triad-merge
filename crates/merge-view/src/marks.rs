//! Each region's mark in the result gutter (spec §5, §6.5): a label and a tone, so colour is
//! never the only signal. Unchanged regions with their automatic text have none.

use std::ops::Range;

use merge_core::document::{MergeDocument, Resolution};
use merge_core::index::RegionIx;
use merge_core::merge3::{Kind, Source};

/// The colour family of a mark, which also colours its band in the result pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Left,
    Right,
    Base,
    /// Text from more than one input.
    Both,
    Conflict,
    Edited,
    /// The region contributes no text.
    Cleared,
    /// Merged word by word: text from more than one side within its lines.
    WordMerged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub label: &'static str,
    pub tone: Tone,
}

const LEFT: Mark = Mark { label: "L", tone: Tone::Left };
const RIGHT: Mark = Mark { label: "R", tone: Tone::Right };
const BASE: Mark = Mark { label: "B", tone: Tone::Base };
/// Every side made the same change.
const SAME: Mark = Mark { label: "=", tone: Tone::Both };
const SEVERAL: Mark = Mark { label: "+", tone: Tone::Both };
const UNRESOLVED: Mark = Mark { label: "!", tone: Tone::Conflict };
const EDITED: Mark = Mark { label: "E", tone: Tone::Edited };
const CLEARED: Mark = Mark { label: "-", tone: Tone::Cleared };
const WORD_MERGED: Mark = Mark { label: "W", tone: Tone::WordMerged };

/// Labels for the sides of a merge with more than two, numbered from 1.
const SIDE_NUMBERS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// A run of the result pane's text: its bytes, the tone behind them, the gutter mark and the
/// tone of a border around them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub bytes: Range<usize>,
    pub tone: Tone,
    pub mark: Mark,
    pub border: Option<Tone>,
}

/// Whether `bands` paint as `shown` does once the editor has moved `shown` to `tracked`: as
/// many bands, each with the same looks and bytes equal to its tracked range. The editor moves
/// its decorations with each edit, so then they need no rebuild.
#[must_use]
pub fn bands_unchanged(shown: &[Band], tracked: &[Range<usize>], bands: &[Band]) -> bool {
    shown.len() == bands.len()
        && tracked.len() == bands.len()
        && shown.iter().zip(tracked).zip(bands).all(|((old, at), new)| {
            new.bytes == *at
                && new.tone == old.tone
                && new.mark == old.mark
                && new.border == old.border
        })
}

/// The result pane's bands, in order, skipping regions without text. A picked or unresolved
/// region gets one band per source, in that source's tone; an unresolved one marks each with
/// [`UNRESOLVED`] and borders it in the conflict tone. Other regions get one band.
#[must_use]
pub fn bands(doc: &MergeDocument) -> Vec<Band> {
    let three_way = doc.inputs().is_three_way();
    doc.region_ixs()
        .flat_map(|region| {
            let unresolved = *doc.resolution(region) == Resolution::Unresolved;
            let blocks = doc.blocks(region);
            let whole = blocks.is_empty().then(|| {
                mark(doc, region).map(|mark| Band {
                    bytes: doc.region_bytes(region),
                    tone: mark.tone,
                    mark,
                    border: None,
                })
            });
            let parts = blocks.into_iter().map(move |(source, bytes)| {
                let own = source_mark(source, three_way);
                Band {
                    bytes,
                    tone: own.tone,
                    mark: if unresolved { UNRESOLVED } else { own },
                    border: unresolved.then_some(Tone::Conflict),
                }
            });
            whole.flatten().into_iter().chain(parts)
        })
        .filter(|band| !band.bytes.is_empty())
        .collect()
}

/// The look of a result placeholder: the band's tone, the gutter mark and the border tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceholderMark {
    pub tone: Tone,
    pub mark: Mark,
    pub border: Option<Tone>,
}

/// How a placeholder for `sources`' deletion in `region` looks: as the block those sources
/// would fill. In an unresolved conflict, the first source's tone with [`UNRESOLVED`] and a
/// conflict border. In a pick or an automatic region, the source's own mark, the same-change
/// mark for every side's automatic deletion, or the several-sources mark. With no sources,
/// the cleared mark.
///
/// # Panics
///
/// If the region does not exist.
#[must_use]
pub fn placeholder_mark(
    doc: &MergeDocument,
    region: RegionIx,
    sources: &[Source],
) -> PlaceholderMark {
    let three_way = doc.inputs().is_three_way();
    let first = sources.first().map(|&s| source_mark(s, three_way));
    let unresolved = *doc.resolution(region) == Resolution::Unresolved;
    let same =
        *doc.resolution(region) == Resolution::Auto && doc.region(region).kind == Kind::AllSame;
    let (tone, mark, border) = match (first, sources.len()) {
        (None, _) => (Tone::Cleared, CLEARED, None),
        (Some(own), _) if unresolved => (own.tone, UNRESOLVED, Some(Tone::Conflict)),
        (Some(own), 1) => (own.tone, own, None),
        (Some(_), _) if same => (Tone::Both, SAME, None),
        (Some(_), _) => (Tone::Both, SEVERAL, None),
    };
    PlaceholderMark { tone, mark, border }
}

/// The mark of `region`.
///
/// # Panics
///
/// If the region does not exist.
#[must_use]
pub fn mark(doc: &MergeDocument, region: RegionIx) -> Option<Mark> {
    let three_way = doc.inputs().is_three_way();
    match doc.resolution(region) {
        Resolution::Auto => match doc.region(region).kind {
            Kind::Unchanged => None,
            Kind::OneSide(i) => Some(source_mark(Source::Side(i), three_way)),
            Kind::AllSame => Some(SAME),
            Kind::WordMerged => Some(WORD_MERGED),
            Kind::Conflict => Some(UNRESOLVED),
        },
        Resolution::Pick(sources) => Some(match sources.as_slice() {
            [] => CLEARED,
            [source] => source_mark(*source, three_way),
            _ => SEVERAL,
        }),
        Resolution::Edited => Some(EDITED),
        Resolution::Unresolved => Some(UNRESOLVED),
    }
}

/// The tone of `source`'s rows in a region of `kind` in the input panes, which show the
/// inputs and not the resolution. A side takes its own tone where it changed; the base takes
/// the base tone wherever any side changed.
#[must_use]
pub fn input_tone(kind: Kind, source: Source) -> Option<Tone> {
    match (kind, source) {
        (Kind::Unchanged, _) => None,
        (_, Source::Base(_)) => Some(Tone::Base),
        (Kind::OneSide(changed), Source::Side(i)) if changed != i => None,
        (_, Source::Side(i)) => Some(side_tone(i)),
    }
}

/// The mark in the input panes' gutters: conflicts only, since each pane's tone already
/// shows which sides changed.
#[must_use]
pub fn input_mark(kind: Kind) -> Option<Mark> {
    (kind == Kind::Conflict).then_some(UNRESOLVED)
}

/// The tone of a pane's heading: the tone its bands take.
#[must_use]
pub fn pane_tone(source: Source) -> Tone {
    match source {
        Source::Base(_) => Tone::Base,
        Source::Side(i) => side_tone(i),
    }
}

fn side_tone(side: usize) -> Tone {
    if side == 0 { Tone::Left } else { Tone::Right }
}

fn source_mark(source: Source, three_way: bool) -> Mark {
    match (source, three_way) {
        (Source::Base(_), _) => BASE,
        (Source::Side(0), true) => LEFT,
        (Source::Side(_), true) => RIGHT,
        (Source::Side(i), false) => Mark {
            label: SIDE_NUMBERS.get(i).copied().unwrap_or(SEVERAL.label),
            tone: side_tone(i),
        },
    }
}

#[cfg(test)]
mod tests {
    use merge_core::document::{Command, EditBatch, Labels, MergeOptions, TextPatch};
    use merge_core::merge3::Inputs;
    use merge_core::text::Text;

    use super::*;

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes()).expect("decodes")
    }

    fn doc(inputs: Inputs) -> MergeDocument {
        MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels")
    }

    /// Regions: 0 a, 1 B (left only), 2 c, 3 D (right only), 4 e, 5 F (both same), 6 g,
    /// 7 h (conflict), 8 i.
    fn mixed() -> MergeDocument {
        doc(Inputs::three_way(
            text("a\nb\nc\nd\ne\nf\ng\nh\ni\n"),
            text("a\nB\nc\nd\ne\nF\ng\nH\ni\n"),
            text("a\nb\nc\nD\ne\nF\ng\nX\ni\n"),
        ))
    }

    fn toggle(doc: &mut MergeDocument, region: RegionIx, source: Source) {
        doc.apply(Command::Toggle { region, source }).expect("region and source exist");
    }

    #[test]
    fn a_word_merged_region_shows_its_own_mark_until_picked() {
        let options = MergeOptions {
            hunk_level: merge_core::merge3::HunkLevel::Word,
            ..MergeOptions::default()
        };
        let inputs = Inputs::three_way(text("x = 1;\n"), text("y = 1;\n"), text("x = 2;\n"));
        let mut d = MergeDocument::new(inputs, options, Labels::none()).expect("no labels");
        assert_eq!(mark(&d, RegionIx::new(0)), Some(WORD_MERGED));
        assert_eq!(WORD_MERGED.tone, Tone::WordMerged);
        assert_eq!(
            bands(&d),
            [band(0..7, Tone::WordMerged, WORD_MERGED)],
            "one band over the merged text"
        );
        toggle(&mut d, RegionIx::new(0), Source::Side(0));
        assert_eq!(mark(&d, RegionIx::new(0)), Some(LEFT));
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(mark(&d, RegionIx::new(0)), Some(WORD_MERGED));
        assert_eq!(input_mark(Kind::WordMerged), None);
    }

    #[test]
    fn automatic_regions_show_their_kind() {
        let d = mixed();
        let marks: Vec<Option<Mark>> = d.region_ixs().map(|r| mark(&d, r)).collect();
        assert_eq!(
            marks,
            vec![
                None,
                Some(LEFT),
                None,
                Some(RIGHT),
                None,
                Some(SAME),
                None,
                Some(UNRESOLVED),
                None
            ]
        );
    }

    fn band(bytes: Range<usize>, tone: Tone, mark: Mark) -> Band {
        Band { bytes, tone, mark, border: None }
    }

    fn unresolved(bytes: Range<usize>, tone: Tone) -> Band {
        Band { border: Some(Tone::Conflict), ..band(bytes, tone, UNRESOLVED) }
    }

    // `mixed`'s result: a B c D e F g, then H X for the conflict, then i, two bytes each.

    #[test]
    fn an_unresolved_conflict_shows_each_side_in_its_tone_with_the_unresolved_mark() {
        assert_eq!(
            bands(&mixed()),
            [
                band(2..4, Tone::Left, LEFT),
                band(6..8, Tone::Right, RIGHT),
                band(10..12, Tone::Both, SAME),
                unresolved(14..16, Tone::Left),
                unresolved(16..18, Tone::Right),
            ]
        );
    }

    #[test]
    fn a_pick_of_several_sources_shows_each_in_its_own_tone_and_order() {
        let mut d = mixed();
        d.apply(Command::Pick {
            region: RegionIx::new(7),
            sources: vec![Source::Side(1), Source::Side(0)],
        })
        .expect("right then left");
        assert_eq!(
            bands(&d)[3..],
            [band(14..16, Tone::Right, RIGHT), band(16..18, Tone::Left, LEFT)]
        );
        d.apply(Command::Pick { region: RegionIx::new(7), sources: vec![Source::Base(0)] })
            .expect("base");
        assert_eq!(bands(&d)[3..], [band(14..16, Tone::Base, BASE)]);
    }

    #[test]
    fn cleared_regions_have_no_band_and_edited_ones_one() {
        let mut d = mixed();
        d.apply(Command::Clear { region: RegionIx::new(7) }).expect("clear");
        assert_eq!(bands(&d).len(), 3);
        d.apply(Command::Undo).expect("undo");
        // Type after the conflict's H: the region becomes one edited band.
        d.apply(Command::Edit(EditBatch(vec![TextPatch { range: 15..15, text: "x".to_owned() }])))
            .expect("type");
        assert_eq!(bands(&d)[3..], [band(14..19, Tone::Edited, EDITED)]);
    }

    #[test]
    fn input_panes_tone_only_the_sides_that_changed() {
        let d = mixed();
        let tones = |region: RegionIx| {
            let kind = d.region(region).kind;
            [Source::Side(0), Source::Base(0), Source::Side(1)].map(|s| input_tone(kind, s))
        };
        let (l, b, r) = (Some(Tone::Left), Some(Tone::Base), Some(Tone::Right));
        assert_eq!(tones(RegionIx::new(0)), [None, None, None]);
        assert_eq!(tones(RegionIx::new(1)), [l, b, None]);
        assert_eq!(tones(RegionIx::new(3)), [None, b, r]);
        assert_eq!(tones(RegionIx::new(5)), [l, b, r]);
        assert_eq!(tones(RegionIx::new(7)), [l, b, r]);
    }

    #[test]
    fn input_tones_ignore_resolutions() {
        let mut d = mixed();
        toggle(&mut d, RegionIx::new(7), Source::Side(1));
        assert_eq!(input_tone(d.regions()[7].kind, Source::Side(0)), Some(Tone::Left));
        assert_eq!(input_tone(d.regions()[7].kind, Source::Side(1)), Some(Tone::Right));
    }

    #[test]
    fn later_sides_of_wider_merges_tone_as_right() {
        assert_eq!(input_tone(Kind::OneSide(2), Source::Side(2)), Some(Tone::Right));
        assert_eq!(input_tone(Kind::OneSide(2), Source::Side(1)), None);
        assert_eq!(input_tone(Kind::OneSide(2), Source::Side(0)), None);
        assert_eq!(input_tone(Kind::Conflict, Source::Side(2)), Some(Tone::Right));
        assert_eq!(input_tone(Kind::Conflict, Source::Base(1)), Some(Tone::Base));
    }

    #[test]
    fn only_conflicts_get_an_input_mark() {
        let d = mixed();
        let marks: Vec<Option<Mark>> = d.regions().iter().map(|r| input_mark(r.kind)).collect();
        let mut expected = vec![None; 9];
        expected[7] = Some(UNRESOLVED);
        assert_eq!(marks, expected);
    }

    #[test]
    fn picks_show_their_source_in_three_way_merges() {
        let mut d = mixed();
        toggle(&mut d, RegionIx::new(7), Source::Side(1));
        assert_eq!(mark(&d, RegionIx::new(7)), Some(RIGHT));
        toggle(&mut d, RegionIx::new(7), Source::Side(1));
        toggle(&mut d, RegionIx::new(7), Source::Side(0));
        assert_eq!(mark(&d, RegionIx::new(7)), Some(LEFT));
        toggle(&mut d, RegionIx::new(7), Source::Side(0));
        toggle(&mut d, RegionIx::new(7), Source::Base(0));
        assert_eq!(mark(&d, RegionIx::new(7)), Some(BASE));
        toggle(&mut d, RegionIx::new(7), Source::Side(0));
        assert_eq!(mark(&d, RegionIx::new(7)), Some(SEVERAL));
        // An unchanged region with a pick gets a mark too.
        toggle(&mut d, RegionIx::new(0), Source::Side(1));
        assert_eq!(mark(&d, RegionIx::new(0)), Some(RIGHT));
    }

    #[test]
    fn removing_the_only_pick_clears_the_region() {
        let mut d = mixed();
        // An automatic left-only region picks left, so the first toggle removes it.
        toggle(&mut d, RegionIx::new(1), Source::Side(0));
        assert_eq!(mark(&d, RegionIx::new(1)), Some(CLEARED));
        toggle(&mut d, RegionIx::new(1), Source::Side(0));
        assert_eq!(mark(&d, RegionIx::new(1)), Some(LEFT));
        toggle(&mut d, RegionIx::new(1), Source::Side(0));
        assert_eq!(mark(&d, RegionIx::new(1)), Some(CLEARED));
        d.apply(Command::Clear { region: RegionIx::new(7) }).expect("region 7");
        assert_eq!(mark(&d, RegionIx::new(7)), Some(CLEARED));
    }

    #[test]
    fn edits_show_as_edited_and_undo_restores_the_mark() {
        let mut d = mixed();
        // Region 1 holds "B\n" at bytes 2..4.
        d.apply(Command::Edit(EditBatch(vec![TextPatch { range: 3..3, text: "x".to_owned() }])))
            .expect("type");
        assert_eq!(mark(&d, RegionIx::new(1)), Some(EDITED));
        d.apply(Command::Undo).expect("undo");
        assert_eq!(mark(&d, RegionIx::new(1)), Some(LEFT));
    }

    #[test]
    fn sides_of_wider_merges_are_numbered() {
        let mut d = doc(Inputs::new(
            vec![text("a\n"), text("b\n"), text("c\n")],
            vec![text("x\n"), text("x\n")],
        )
        .expect("three sides, two bases"));
        assert_eq!(d.regions()[0].kind, Kind::Conflict);
        toggle(&mut d, RegionIx::new(0), Source::Side(2));
        assert_eq!(mark(&d, RegionIx::new(0)), Some(Mark { label: "3", tone: Tone::Right }));
        toggle(&mut d, RegionIx::new(0), Source::Side(2));
        toggle(&mut d, RegionIx::new(0), Source::Side(0));
        assert_eq!(mark(&d, RegionIx::new(0)), Some(Mark { label: "1", tone: Tone::Left }));
        toggle(&mut d, RegionIx::new(0), Source::Side(0));
        toggle(&mut d, RegionIx::new(0), Source::Base(1));
        assert_eq!(mark(&d, RegionIx::new(0)), Some(BASE));
    }

    fn placeholder(tone: Tone, mark: Mark, border: Option<Tone>) -> PlaceholderMark {
        PlaceholderMark { tone, mark, border }
    }

    const L: Source = Source::Side(0);
    const R: Source = Source::Side(1);

    /// Left deletes b, c; right changes them. Region 1 is the conflict.
    fn left_deletes() -> MergeDocument {
        doc(Inputs::three_way(text("a\nb\nc\nz\n"), text("a\nz\n"), text("a\nB\nC\nz\n")))
    }

    #[test]
    fn an_unresolved_placeholder_looks_like_a_conflict_block_in_its_sides_tone() {
        let d = left_deletes();
        assert_eq!(
            placeholder_mark(&d, RegionIx::new(1), &[L]),
            placeholder(Tone::Left, UNRESOLVED, Some(Tone::Conflict))
        );
        let d = doc(Inputs::three_way(text("a\nb\nz\n"), text("a\nB\nz\n"), text("a\nz\n")));
        assert_eq!(
            placeholder_mark(&d, RegionIx::new(1), &[R]),
            placeholder(Tone::Right, UNRESOLVED, Some(Tone::Conflict))
        );
    }

    #[test]
    fn a_picked_placeholder_takes_its_sources_mark() {
        let mut d = left_deletes();
        d.apply(Command::Pick { region: RegionIx::new(1), sources: vec![R, L] }).expect("pick");
        assert_eq!(
            placeholder_mark(&d, RegionIx::new(1), &[L]),
            placeholder(Tone::Left, LEFT, None)
        );
    }

    #[test]
    fn an_automatic_placeholder_takes_its_kinds_mark() {
        let one = doc(Inputs::three_way(text("a\nb\nz\n"), text("a\nb\nz\n"), text("a\nz\n")));
        assert_eq!(
            placeholder_mark(&one, RegionIx::new(1), &[R]),
            placeholder(Tone::Right, RIGHT, None)
        );
        let both = doc(Inputs::three_way(text("a\nb\nz\n"), text("a\nz\n"), text("a\nz\n")));
        assert_eq!(
            placeholder_mark(&both, RegionIx::new(1), &[L, R]),
            placeholder(Tone::Both, SAME, None)
        );
    }

    #[test]
    fn a_placeholder_for_several_picked_sources_takes_the_several_mark() {
        let mut both = doc(Inputs::three_way(text("a\nb\nz\n"), text("a\nz\n"), text("a\nz\n")));
        both.apply(Command::Pick { region: RegionIx::new(1), sources: vec![R, L] }).expect("pick");
        assert_eq!(
            placeholder_mark(&both, RegionIx::new(1), &[R, L]),
            placeholder(Tone::Both, SEVERAL, None)
        );
    }

    #[test]
    fn an_empty_pick_placeholder_is_cleared() {
        let mut d = left_deletes();
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        assert_eq!(
            placeholder_mark(&d, RegionIx::new(1), &[]),
            placeholder(Tone::Cleared, CLEARED, None)
        );
    }

    fn bordered(bytes: Range<usize>, tone: Tone, mark: Mark, border: Option<Tone>) -> Band {
        Band { bytes, tone, mark, border }
    }

    #[test]
    fn bands_that_match_the_tracked_ranges_and_looks_need_no_rebuild() {
        let shown = [
            bordered(0..4, Tone::Left, LEFT, None),
            bordered(4..8, Tone::Conflict, UNRESOLVED, Some(Tone::Conflict)),
            bordered(10..12, Tone::Edited, EDITED, None),
        ];
        // Nothing moved.
        let tracked = shown.each_ref().map(|b| b.bytes.clone());
        assert!(bands_unchanged(&shown, &tracked, &shown));
        // A key typed inside the edited band: the editor grew it and the core agrees.
        let tracked = [0..4, 4..8, 10..13];
        let typed = [
            bordered(0..4, Tone::Left, LEFT, None),
            bordered(4..8, Tone::Conflict, UNRESOLVED, Some(Tone::Conflict)),
            bordered(10..13, Tone::Edited, EDITED, None),
        ];
        assert!(bands_unchanged(&shown, &tracked, &typed));
    }

    #[test]
    fn bands_that_differ_from_the_tracked_ranges_or_looks_need_a_rebuild() {
        let shown = [
            bordered(0..4, Tone::Left, LEFT, None),
            bordered(4..8, Tone::Conflict, UNRESOLVED, Some(Tone::Conflict)),
        ];
        let tracked = [0..4, 4..8];
        let with = |b: Band| [shown[0].clone(), b];
        // A key typed at a band's edge: the editor kept it out, the core took it in.
        assert!(!bands_unchanged(
            &shown,
            &tracked,
            &with(bordered(4..9, Tone::Conflict, UNRESOLVED, Some(Tone::Conflict)))
        ));
        // Typing into a conflict makes it edited: same bytes, another tone, mark and border.
        assert!(!bands_unchanged(
            &shown,
            &tracked,
            &with(bordered(4..8, Tone::Edited, UNRESOLVED, Some(Tone::Conflict)))
        ));
        assert!(!bands_unchanged(
            &shown,
            &tracked,
            &with(bordered(4..8, Tone::Conflict, EDITED, Some(Tone::Conflict)))
        ));
        assert!(!bands_unchanged(
            &shown,
            &tracked,
            &with(bordered(4..8, Tone::Conflict, UNRESOLVED, None))
        ));
        // The editor dropped a band whose text went.
        assert!(!bands_unchanged(&shown, &tracked[..1], &shown[..1]));
        // A new band.
        assert!(!bands_unchanged(&shown[..1], &tracked[..1], &shown));
    }
}
