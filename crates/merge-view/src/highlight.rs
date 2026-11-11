//! Intra-line highlights for the top panes (spec §5), computed for a row when it is first
//! shown and kept by region in a small LRU cache.
//!
//! In a changed region the n-th line of each side pairs with the n-th line of the reference
//! base. A side's cell marks the characters that differ from base; the base cell marks the
//! union of its differences from every side. Spans depend only on the inputs and the regions,
//! so edits that keep the regions keep the cache, and a merge or split of regions clears it.

use std::collections::HashMap;
use std::ops::Range;

use merge_core::diff::{Hunk, intraline_hunks};
use merge_core::document::{Applied, MergeDocument};
use merge_core::index::RegionIx;
use merge_core::merge3::{Kind, Region, Source};
use smallvec::SmallVec;

use crate::rows::{PaneSet, RowIx, RowMap};

/// Regions whose spans the cache keeps.
pub const INTRALINE_CACHE_REGIONS: usize = 256;
const REFERENCE_BASE: usize = 0;
const PANES: usize = 3;

/// The highlight inside one cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellSpans {
    /// Nothing inside the line: no partner, an unchanged region, or equal lines.
    None,
    /// Byte ranges within the line, sorted and disjoint.
    Ranges(Vec<Range<usize>>),
    /// A line too long for a character diff (`MAX_INTRALINE_DIFF_CHARS`).
    WholeLine,
}

type RowSpans = SmallVec<[CellSpans; PANES]>;

/// Spans per row, cached by region.
#[derive(Debug)]
pub struct Highlights {
    sources: SmallVec<[Source; PANES]>,
    /// Most recently used last. Each region maps its row offsets to their spans.
    cache: Vec<(RegionIx, HashMap<usize, RowSpans>)>,
    /// Rows computed, for tests of the cache.
    computed: usize,
}

impl Highlights {
    #[must_use]
    pub fn new(panes: PaneSet) -> Self {
        Self {
            sources: panes.sources(),
            cache: Vec::with_capacity(INTRALINE_CACHE_REGIONS),
            computed: 0,
        }
    }

    /// The spans of row `index`, one per pane.
    ///
    /// # Panics
    ///
    /// If `index` is not below the row count.
    pub fn row(&mut self, doc: &MergeDocument, rows: &RowMap, index: RowIx) -> &[CellSpans] {
        let region = rows.region_of_row(index);
        let offset = index.since(rows.first_row(region));
        let entry = if let Some(at) = self.cache.iter().position(|(r, _)| *r == region) {
            self.cache.remove(at)
        } else {
            if self.cache.len() == INTRALINE_CACHE_REGIONS {
                self.cache.remove(0);
            }
            (region, HashMap::new())
        };
        let (_, offsets) = self.cache.push_mut(entry);
        let (sources, computed) = (&self.sources, &mut self.computed);
        offsets.entry(offset).or_insert_with(|| {
            *computed += 1;
            spans(doc, doc.region(region), sources, offset)
        })
    }

    /// Follow a command: a merge or split of regions renumbers them, so the cache empties.
    pub fn update(&mut self, applied: &Applied) {
        if applied.regions_changed {
            self.cache.clear();
        }
    }

    #[cfg(test)]
    fn computed(&self) -> usize {
        self.computed
    }
}

fn spans(doc: &MergeDocument, region: &Region, sources: &[Source], offset: usize) -> RowSpans {
    let inputs = doc.inputs();
    let line = |source: Source| {
        let lines = range(region, source);
        (offset < lines.len()).then(|| inputs.text(source).line(lines.start + offset))
    };
    // Each side's character diff from base; `None` inside when a line is too long.
    let diffs: Vec<(usize, Option<Vec<Hunk>>)> =
        match (region.kind, line(Source::Base(REFERENCE_BASE))) {
            (Kind::Unchanged, _) | (_, None) => Vec::new(),
            (_, Some(base)) => (0..region.sides.len())
                .filter_map(|i| {
                    line(Source::Side(i)).map(|side| {
                        let hunks = if side == base {
                            Some(Vec::new())
                        } else {
                            intraline_hunks(base, side)
                        };
                        (i, hunks)
                    })
                })
                .collect(),
        };
    sources
        .iter()
        .map(|&source| match source {
            Source::Side(i) => match diffs.iter().find(|(side, _)| *side == i) {
                None => CellSpans::None,
                Some((_, None)) => CellSpans::WholeLine,
                Some((_, Some(hunks))) => ranges(hunks.iter().map(|h| h.after.clone())),
            },
            Source::Base(_) => {
                if diffs.iter().any(|(_, hunks)| hunks.is_none()) {
                    CellSpans::WholeLine
                } else {
                    let mut all: Vec<Range<usize>> = diffs
                        .iter()
                        .flat_map(|(_, hunks)| hunks.iter().flatten().map(|h| h.before.clone()))
                        .collect();
                    all.sort_by_key(|r| (r.start, r.end));
                    ranges(union(all))
                }
            }
        })
        .collect()
}

/// The non-empty `ranges`, or [`CellSpans::None`] when there are none.
fn ranges(ranges: impl IntoIterator<Item = Range<usize>>) -> CellSpans {
    let ranges: Vec<Range<usize>> = ranges.into_iter().filter(|r| !r.is_empty()).collect();
    if ranges.is_empty() { CellSpans::None } else { CellSpans::Ranges(ranges) }
}

/// Sorted ranges with overlapping or touching ones joined.
fn union(sorted: Vec<Range<usize>>) -> Vec<Range<usize>> {
    sorted.into_iter().fold(Vec::new(), |mut out: Vec<Range<usize>>, r| {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
        out
    })
}

fn range(region: &Region, source: Source) -> Range<usize> {
    match source {
        Source::Side(i) => region.sides[i].clone(),
        Source::Base(i) => region.bases[i].clone(),
    }
}

#[cfg(test)]
#[expect(clippy::single_range_in_vec_init, reason = "spans are lists of ranges, often of one")]
mod tests {
    use merge_core::testing::{doc, doc_with};
    use std::fmt::Write as _;

    use merge_core::document::{Command, EditBatch, MergeOptions, TextPatch};

    use super::*;

    const LRU_LINES: usize = 600;

    fn row(doc: &MergeDocument, panes: PaneSet, index: usize) -> Vec<CellSpans> {
        let rows = RowMap::new(doc, panes).expect("panes fit");
        Highlights::new(panes).row(doc, &rows, RowIx::new(index)).to_vec()
    }

    fn at(ranges: &[Range<usize>]) -> CellSpans {
        CellSpans::Ranges(ranges.to_vec())
    }

    #[test]
    fn a_replaced_word_is_marked_in_the_side_and_in_base() {
        let d = doc("let x = 1;\n", "let y = 1;\n", "let x = 1;\n");
        assert_eq!(row(&d, PaneSet::LeftBaseRight, 0), [at(&[4..5]), at(&[4..5]), CellSpans::None]);
    }

    #[test]
    fn base_marks_the_union_of_both_sides_changes() {
        let d = doc("a b c\n", "A b c\n", "a b C\n");
        assert_eq!(
            row(&d, PaneSet::LeftBaseRight, 0),
            [at(&[0..1]), at(&[0..1, 4..5]), at(&[4..5])]
        );
    }

    #[test]
    fn base_joins_overlapping_and_touching_changes() {
        let overlapping = doc("abcd\n", "XYcd\n", "aZcd\n");
        assert_eq!(row(&overlapping, PaneSet::LeftBaseRight, 0)[1], at(&[0..2]));
        let touching = doc("abcd\n", "Xbcd\n", "aZcd\n");
        assert_eq!(row(&touching, PaneSet::LeftBaseRight, 0)[1], at(&[0..2]));
    }

    #[test]
    fn an_insertion_inside_a_line_is_marked_only_in_the_side() {
        let d = doc("a x\n", "abc x\n", "a x\n");
        assert_eq!(
            row(&d, PaneSet::LeftBaseRight, 0),
            [at(&[1..3]), CellSpans::None, CellSpans::None]
        );
    }

    #[test]
    fn a_whitespace_only_change_has_no_spans() {
        let options = MergeOptions {
            diff: merge_core::diff::DiffOptions {
                whitespace: merge_core::diff::Whitespace::Trailing,
                ..Default::default()
            },
            ..MergeOptions::default()
        };
        let d = doc_with("a\n", "a \n", "a\n", options);
        assert_eq!(row(&d, PaneSet::LeftBaseRight, 0), vec![CellSpans::None; 3]);
    }

    #[test]
    fn a_line_over_the_limit_is_marked_whole() {
        let long =
            |c: &str| format!("{}\n", c.repeat(merge_core::diff::MAX_INTRALINE_DIFF_CHARS + 1));
        let d = doc(&long("a"), &long("b"), &long("a"));
        assert_eq!(
            row(&d, PaneSet::LeftBaseRight, 0),
            [CellSpans::WholeLine, CellSpans::WholeLine, CellSpans::None]
        );
    }

    #[test]
    fn unpaired_lines_and_unchanged_regions_have_no_spans() {
        // Rows: a (unchanged), X (left inserts it; no base line to pair with).
        let d = doc("a\n", "a\nX\n", "a\n");
        let none = vec![CellSpans::None; 3];
        assert_eq!(row(&d, PaneSet::LeftBaseRight, 0), none);
        assert_eq!(row(&d, PaneSet::LeftBaseRight, 1), none);
    }

    /// Regions: 0 a, 1 b→B (left), 2 m, 3 c→C (right), 4 n, 5 z→Z (left). One row each.
    /// Result: "a\nB\nm\nC\nn\nZ\n".
    fn six() -> MergeDocument {
        doc("a\nb\nm\nc\nn\nz\n", "a\nB\nm\nc\nn\nZ\n", "a\nb\nm\nC\nn\nz\n")
    }

    #[test]
    fn a_row_is_computed_once_and_kept_through_edits_that_keep_regions() {
        let mut d = six();
        let rows = RowMap::new(&d, PaneSet::LeftRight).expect("panes fit");
        let mut h = Highlights::new(PaneSet::LeftRight);
        assert_eq!(h.row(&d, &rows, RowIx::new(1)), [at(&[0..1]), CellSpans::None]);
        assert_eq!(h.row(&d, &rows, RowIx::new(1)), [at(&[0..1]), CellSpans::None]);
        assert_eq!(h.computed(), 1);

        let applied = d
            .apply(Command::Edit(EditBatch(vec![TextPatch { range: 2..2, text: "x".to_owned() }])))
            .expect("type into region 1");
        assert!(!applied.regions_changed);
        h.update(&applied);
        assert_eq!(h.row(&d, &rows, RowIx::new(1)), [at(&[0..1]), CellSpans::None]);
        assert_eq!(h.computed(), 1);
    }

    #[test]
    fn a_merge_of_regions_empties_the_cache() {
        let mut d = six();
        let mut rows = RowMap::new(&d, PaneSet::LeftRight).expect("panes fit");
        let mut h = Highlights::new(PaneSet::LeftRight);
        assert_eq!(h.row(&d, &rows, RowIx::new(3)), [CellSpans::None, at(&[0..1])]);

        // Deleting "\nm\nC" joins regions 1-3 (rows 1-3); n is region 2, z→Z region 3 at row 5.
        let applied = d
            .apply(Command::Edit(EditBatch(vec![TextPatch { range: 3..7, text: String::new() }])))
            .expect("edit");
        assert_eq!(d.regions().len(), 4);
        rows.update(&d, &applied);
        h.update(&applied);
        assert_eq!(h.row(&d, &rows, RowIx::new(5)), [at(&[0..1]), CellSpans::None]);
        // The joined region's third row pairs base c with right C.
        assert_eq!(h.row(&d, &rows, RowIx::new(3)), [CellSpans::None, at(&[0..1])]);
    }

    #[test]
    fn the_least_recently_used_region_leaves_a_full_cache() {
        // Even lines change on the left: region 2k is line 2k, one row each.
        let base = (0..LRU_LINES).fold(String::new(), |mut s, n| {
            writeln!(s, "l{n}").expect("writing to a String");
            s
        });
        let left: String = (0..LRU_LINES)
            .map(|n| if n % 2 == 0 { format!("L{n}\n") } else { format!("l{n}\n") })
            .collect();
        let d = doc(&base, &left, &base);
        let rows = RowMap::new(&d, PaneSet::LeftRight).expect("panes fit");
        let mut h = Highlights::new(PaneSet::LeftRight);
        let changed = |k: usize| 2 * k;

        let _ = h.row(&d, &rows, RowIx::new(changed(0)));
        for k in 1..INTRALINE_CACHE_REGIONS {
            let _ = h.row(&d, &rows, RowIx::new(changed(k)));
        }
        let _ = h.row(&d, &rows, RowIx::new(changed(0)));
        assert_eq!(h.computed(), INTRALINE_CACHE_REGIONS, "a full cache keeps region 0");

        for k in INTRALINE_CACHE_REGIONS..=INTRALINE_CACHE_REGIONS + 1 {
            let _ = h.row(&d, &rows, RowIx::new(changed(k)));
        }
        let _ = h.row(&d, &rows, RowIx::new(changed(2)));
        assert_eq!(
            h.computed(),
            INTRALINE_CACHE_REGIONS + 3,
            "regions 1 and 2 were least recently used"
        );
    }
}
