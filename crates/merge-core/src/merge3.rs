//! Alignment of the sides of a merge against its bases, and region classification
//! (spec §4.3).
//!
//! A merge has n sides and n − 1 bases, in jj's order: side 0, base 0, side 1, base 1, …,
//! side n − 1. A three-way merge has left = side 0, base = base 0 and right = side 1.
//! Base 0 is the reference: every other input is diffed against it, and regions are found in
//! its line coordinates.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::ops::{Range, RangeInclusive};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::boundaries::{Boundaries, Cuts};
use crate::diff::{
    Comparison, DiffOptions, Hunk, diff_line_ranges, diff_lines_for_merge, line_key,
};
use crate::jj::{self, HunkKind, LineDiff};
use crate::text::Text;

/// Side index of left in a three-way merge.
pub const LEFT: usize = 0;
/// Side index of right in a three-way merge.
pub const RIGHT: usize = 1;

/// The base that every other input is diffed against.
const REFERENCE: usize = 0;
const MIN_SIDES: usize = 2;

/// Whose merge the alignment follows (`docs/PLAN.md` §B1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Semantics {
    /// git `merge-file`: a diff from the reference base to each input (spec §4.2).
    #[default]
    Git,
    /// jj's file merge: one diff of every input against the reference base.
    Jj,
}

/// Whether a region where every side made the same change resolves to it: jj's
/// `merge.same-change`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SameChange {
    #[default]
    Accept,
    /// Leave it a conflict.
    Keep,
}

/// Whether conflicts merge word by word: jj's `merge.hunk-level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkLevel {
    #[default]
    Line,
    /// Merge each conflict's words; keep the conflict unless every word resolves.
    Word,
}

/// How [`align`] finds and classifies regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alignment {
    pub semantics: Semantics,
    /// The algorithm applies to git semantics only; the whitespace mode to both.
    pub diff: DiffOptions,
    pub same_change: SameChange,
    pub hunk_level: HunkLevel,
    pub refine: Refine,
}

/// How far [`align`] refines conflicts (spec §4.3). Reads `true` as [`Refine::Ends`] and
/// `false` as [`Refine::Off`], the settings file's form and that of older journals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", try_from = "RefineValue")]
pub enum Refine {
    /// Conflicts stay as the diff gives them.
    Off,
    /// Lines every side shares at a conflict's start and end split off, as in git's
    /// `--zdiff3`.
    #[default]
    Ends,
    /// As [`Refine::Ends`], and a three-way conflict also splits at each block of lines both
    /// sides share inside it.
    Deep,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RefineValue {
    Flag(bool),
    Name(String),
}

impl TryFrom<RefineValue> for Refine {
    type Error = String;

    fn try_from(value: RefineValue) -> Result<Self, String> {
        match value {
            RefineValue::Flag(true) => Ok(Refine::Ends),
            RefineValue::Flag(false) => Ok(Refine::Off),
            RefineValue::Name(name) => match name.as_str() {
                "off" => Ok(Refine::Off),
                "ends" => Ok(Refine::Ends),
                "deep" => Ok(Refine::Deep),
                _ => Err(format!("unknown refinement {name:?}: expected true, false or \"deep\"")),
            },
        }
    }
}

/// One input of a merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    Side(usize),
    Base(usize),
}

/// The texts of a merge: n sides and n − 1 bases, with n at least 2.
#[derive(Debug, Clone)]
pub struct Inputs {
    sides: Vec<Text>,
    bases: Vec<Text>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "a merge needs at least two sides and one base fewer than sides, got {sides} sides and {bases} bases"
)]
pub struct ShapeError {
    pub sides: usize,
    pub bases: usize,
}

impl Inputs {
    /// # Errors
    ///
    /// [`ShapeError`] unless there are at least two sides and one base fewer than sides.
    pub fn new(sides: Vec<Text>, bases: Vec<Text>) -> Result<Self, ShapeError> {
        if sides.len() >= MIN_SIDES && sides.len() == bases.len() + 1 {
            Ok(Self { sides, bases })
        } else {
            Err(ShapeError { sides: sides.len(), bases: bases.len() })
        }
    }

    #[must_use]
    pub fn three_way(base: Text, left: Text, right: Text) -> Self {
        Self { sides: vec![left, right], bases: vec![base] }
    }

    #[must_use]
    pub fn sides(&self) -> &[Text] {
        &self.sides
    }

    /// Whether the merge has two sides and one base: left, base and right.
    #[must_use]
    pub fn is_three_way(&self) -> bool {
        self.sides.len() == MIN_SIDES
    }

    #[must_use]
    pub fn bases(&self) -> &[Text] {
        &self.bases
    }

    /// # Panics
    ///
    /// If the index is out of range.
    #[must_use]
    pub fn text(&self, source: Source) -> &Text {
        match source {
            Source::Side(i) => &self.sides[i],
            Source::Base(i) => &self.bases[i],
        }
    }
}

/// How a region resolves automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No input changed the reference base's lines.
    Unchanged,
    /// The merge resolves to this side's text. With several sides holding that text, the
    /// lowest index. In a three-way merge, `OneSide(LEFT)` is `LeftOnly` and
    /// `OneSide(RIGHT)` is `RightOnly`.
    OneSide(usize),
    /// Every side holds the same text: `BothSame` in a three-way merge.
    AllSame,
    /// The sides conflict line by line, and every word resolves: [`word_merge`] gives the
    /// text.
    WordMerged,
    /// The sides differ and no trivial resolution exists.
    Conflict,
}

/// One aligned stretch of every input, as line ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub kind: Kind,
    /// One range per base.
    pub bases: Vec<Range<usize>>,
    /// One range per side.
    pub sides: Vec<Range<usize>>,
}

/// Align every input against the reference base and classify each region.
///
/// Under git semantics, changes whose reference ranges overlap or touch form one region, as in
/// git `merge-file`. Under jj semantics, each hunk of jj's diff is a region. With `refine`, the
/// lines that every side shares at the start and end of a conflict split off into
/// [`Kind::AllSame`] regions with empty base ranges, as in git's `--zdiff3`. The conflict keeps
/// its whole base ranges. With [`HunkLevel::Word`], a conflict whose words all resolve is
/// [`Kind::WordMerged`] instead, before refinement.
///
/// The regions cover every input's lines in order, without gaps or overlaps.
#[must_use]
pub fn align(inputs: &Inputs, opts: Alignment) -> Vec<Region> {
    align_merged(inputs, opts).regions
}

/// [`align`]'s regions, with the text of each [`Kind::WordMerged`] region.
pub struct Aligned {
    pub regions: Vec<Region>,
    /// [`word_merge`]'s text of each word-merged region, in the regions' order.
    pub word_merged: Vec<String>,
}

/// [`align`], keeping the text of each word-merged region, which classifying it computes.
#[must_use]
pub fn align_merged(inputs: &Inputs, opts: Alignment) -> Aligned {
    align_bounded(inputs, opts, None)
}

/// [`align_merged`], where `boundaries`, when given, drops each deep-refinement split at a cut
/// inside a syntax node, and each word merge with a hunk inside a token
/// (`docs/PLAN.md` §B5.8).
#[must_use]
pub fn align_bounded(
    inputs: &Inputs,
    opts: Alignment,
    boundaries: Option<&dyn Boundaries>,
) -> Aligned {
    let mut classified = Vec::new();
    let mut add = |changed: bool, bases: Vec<Range<usize>>, sides: Vec<Range<usize>>| {
        let kind = if changed { classify(inputs, &bases, &sides, opts) } else { Kind::Unchanged };
        classified.push(Region { kind, bases, sides });
    };
    match opts.semantics {
        Semantics::Git => git_regions(inputs, opts.diff, &mut add),
        Semantics::Jj => LineDiff::new(inputs, opts.diff.comparison())
            .hunks()
            .for_each(|hunk| add(hunk.kind == HunkKind::Different, hunk.bases, hunk.sides)),
    }
    // The word diff dominates a word-level load, and each conflict's merge stands alone.
    let merged: Vec<Option<String>> = if opts.hunk_level == HunkLevel::Word {
        classified
            .par_iter()
            .map(|region| {
                (region.kind == Kind::Conflict)
                    .then(|| word_merge_bounded(inputs, region, opts.same_change, boundaries))
                    .flatten()
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut merged = merged.into_iter();
    let mut regions = Vec::with_capacity(classified.len());
    let mut word_merged = Vec::new();
    for mut region in classified {
        if let Some(text) = merged.next().flatten() {
            region.kind = Kind::WordMerged;
            word_merged.push(text);
        }
        if region.kind != Kind::Conflict || opts.refine == Refine::Off {
            regions.push(region);
            continue;
        }
        let mode = opts.diff.comparison();
        let trimmed = split_shared_ends(inputs, &region, mode);
        if opts.refine == Refine::Deep {
            regions.extend(trimmed.into_iter().flat_map(|piece| match piece.kind {
                Kind::Conflict => split_shared_blocks(inputs, &piece, mode, boundaries),
                _ => vec![piece],
            }));
        } else {
            regions.extend(trimmed);
        }
    }
    Aligned { regions, word_merged }
}

/// The text of `region` merged word by word, as jj-lib's `merge_hunk_by_word` does: jj's word
/// diff of the region's texts, each word hunk resolved trivially. None unless every word hunk
/// resolves. Words compare as bytes, whatever the whitespace mode.
#[must_use]
pub fn word_merge(inputs: &Inputs, region: &Region, same_change: SameChange) -> Option<String> {
    word_merge_bounded(inputs, region, same_change, None)
}

/// [`word_merge`], where `boundaries`, when given, also gives none when a word hunk resolved
/// from a side starts or ends inside a token of a side that holds its text
/// (`docs/PLAN.md` §B5.8). The text, when there is one, is [`word_merge`]'s.
#[must_use]
pub fn word_merge_bounded(
    inputs: &Inputs,
    region: &Region,
    same_change: SameChange,
    boundaries: Option<&dyn Boundaries>,
) -> Option<String> {
    let bases = spans(&inputs.bases, &region.bases);
    let sides = spans(&inputs.sides, &region.sides);
    let Some(boundaries) = boundaries else {
        return jj::merge_words(&bases, &sides, same_change, None);
    };
    // A side parses only when a hunk resolves from it.
    let cuts: Vec<OnceCell<(Cuts, usize)>> = region.sides.iter().map(|_| OnceCell::new()).collect();
    let clean = |side: usize, at: usize| {
        let (cuts, start) = cuts[side].get_or_init(|| {
            let (text, lines) = (&inputs.sides[side], &region.sides[side]);
            (boundaries.token_cuts(text, lines.clone()), text.span(0..lines.start).len())
        });
        cuts.clean(start + at)
    };
    jj::merge_words(&bases, &sides, same_change, Some(&clean))
}

fn spans<'t>(texts: &'t [Text], ranges: &[Range<usize>]) -> Vec<&'t str> {
    texts.iter().zip(ranges).map(|(text, range)| text.span(range.clone())).collect()
}

/// git `merge-file`'s regions in order, as `add(changed, bases, sides)`.
fn git_regions(
    inputs: &Inputs,
    opts: DiffOptions,
    add: &mut impl FnMut(bool, Vec<Range<usize>>, Vec<Range<usize>>),
) {
    let mut shift = Shift::new(inputs);
    let mut at = 0;
    for cluster in clusters(changes(inputs, opts)) {
        if let Some(Region { bases, sides, .. }) = shift.unchanged(at..cluster.base.start) {
            add(false, bases, sides);
        }
        let starts = shift.place(cluster.base.start);
        cluster.changes.iter().for_each(|c| shift.apply(c));
        let ends = shift.place(cluster.base.end);
        add(true, zip_ranges(starts.bases, ends.bases), zip_ranges(starts.sides, ends.sides));
        at = cluster.base.end;
    }
    if let Some(Region { bases, sides, .. }) =
        shift.unchanged(at..inputs.bases[REFERENCE].line_count())
    {
        add(false, bases, sides);
    }
}

/// One hunk of the diff from the reference base to `source`.
struct Change {
    source: Source,
    hunk: Hunk,
}

/// Changes whose reference ranges overlap or touch.
struct Cluster {
    base: Range<usize>,
    changes: Vec<Change>,
}

/// Every hunk from the reference base to each other input, ordered by reference range.
fn changes(inputs: &Inputs, opts: DiffOptions) -> Vec<Change> {
    let reference = &inputs.bases[REFERENCE];
    let others = (0..inputs.sides.len())
        .map(Source::Side)
        .chain((0..inputs.bases.len()).map(Source::Base))
        .filter(|&source| source != Source::Base(REFERENCE));
    let mut all: Vec<Change> = others
        .flat_map(|source| {
            diff_lines_for_merge(reference, inputs.text(source), opts)
                .into_iter()
                .map(move |hunk| Change { source, hunk })
        })
        .collect();
    all.sort_by_key(|c| (c.hunk.before.start, c.hunk.before.end));
    all
}

fn clusters(changes: Vec<Change>) -> Vec<Cluster> {
    changes.into_iter().fold(Vec::new(), |mut clusters, change| {
        match clusters.last_mut() {
            Some(cluster) if change.hunk.before.start <= cluster.base.end => {
                cluster.base.end = cluster.base.end.max(change.hunk.before.end);
                cluster.changes.push(change);
            }
            _ => clusters.push(Cluster { base: change.hunk.before.clone(), changes: vec![change] }),
        }
        clusters
    })
}

/// Each input's line offset from the reference base, outside the changes applied so far.
struct Shift {
    sides: Vec<isize>,
    bases: Vec<isize>,
}

/// One line position per input.
struct Positions {
    sides: Vec<usize>,
    bases: Vec<usize>,
}

impl Shift {
    fn new(inputs: &Inputs) -> Self {
        Self { sides: vec![0; inputs.sides.len()], bases: vec![0; inputs.bases.len()] }
    }

    fn apply(&mut self, change: &Change) {
        let delta = change.hunk.after.len().cast_signed() - change.hunk.before.len().cast_signed();
        match change.source {
            Source::Side(i) => self.sides[i] += delta,
            Source::Base(i) => self.bases[i] += delta,
        }
    }

    /// Where reference line `line` falls in every input.
    fn place(&self, line: usize) -> Positions {
        // Panics only on a broken diff: the changes applied so far lie before `line`, so
        // they remove at most the lines before it.
        let at = |delta: &isize| {
            line.checked_add_signed(*delta).expect("diff offsets stay inside the file")
        };
        Positions {
            sides: self.sides.iter().map(at).collect(),
            bases: self.bases.iter().map(at).collect(),
        }
    }

    /// The unchanged region over reference lines `base`; none when it is empty.
    fn unchanged(&self, base: Range<usize>) -> Option<Region> {
        (!base.is_empty()).then(|| {
            let (starts, ends) = (self.place(base.start), self.place(base.end));
            Region {
                kind: Kind::Unchanged,
                bases: zip_ranges(starts.bases, ends.bases),
                sides: zip_ranges(starts.sides, ends.sides),
            }
        })
    }
}

fn zip_ranges(starts: Vec<usize>, ends: Vec<usize>) -> Vec<Range<usize>> {
    starts.into_iter().zip(ends).map(|(s, e)| s..e).collect()
}

/// A range of lines as comparison keys.
type Keys<'a> = Vec<Cow<'a, str>>;

fn keys<'a>(text: &'a Text, range: &Range<usize>, mode: Comparison) -> Keys<'a> {
    range.clone().map(|n| line_key(text.line(n), mode)).collect()
}

fn keys_each<'a>(texts: &'a [Text], ranges: &[Range<usize>], mode: Comparison) -> Vec<Keys<'a>> {
    texts.iter().zip(ranges).map(|(t, r)| keys(t, r, mode)).collect()
}

/// Classify a changed region by jj's trivial resolution, in both semantics (spec §4.3). In a
/// three-way merge it is git's rule: equal sides are [`Kind::AllSame`] unless `same_change`
/// keeps them, and a side equal to base gives the other.
fn classify(
    inputs: &Inputs,
    bases: &[Range<usize>],
    sides: &[Range<usize>],
    opts: Alignment,
) -> Kind {
    let mode = opts.diff.comparison();
    let sides = keys_each(&inputs.sides, sides, mode);
    let bases = keys_each(&inputs.bases, bases, mode);
    jj::classify(&sides, &bases, opts.same_change)
}

fn all_equal<T: PartialEq>(items: &[T]) -> bool {
    items.windows(2).all(|pair| pair[0] == pair[1])
}

/// Split the lines every side shares at the start and end of a conflict into
/// [`Kind::AllSame`] regions with empty base ranges. Shared lines further in stay. A conflict
/// whose sides are all equal, which [`SameChange::Keep`] leaves, stays whole.
fn split_shared_ends(inputs: &Inputs, conflict: &Region, mode: Comparison) -> Vec<Region> {
    if all_equal(&keys_each(&inputs.sides, &conflict.sides, mode)) {
        return vec![conflict.clone()];
    }
    let shortest = conflict.sides.iter().map(ExactSizeIterator::len).min().unwrap_or(0);
    let same_at = |line: &dyn Fn(&Range<usize>) -> usize| {
        let mut lines =
            inputs.sides.iter().zip(&conflict.sides).map(|(t, r)| line_key(t.line(line(r)), mode));
        let first = lines.next();
        lines.all(|l| Some(&l) == first.as_ref())
    };
    let prefix = (0..shortest).take_while(|&p| same_at(&|r| r.start + p)).count();
    let suffix = (0..shortest - prefix).take_while(|&s| same_at(&|r| r.end - 1 - s)).count();

    let shared = |sides: Vec<Range<usize>>, base_at: &dyn Fn(&Range<usize>) -> usize| Region {
        kind: Kind::AllSame,
        bases: conflict.bases.iter().map(|b| base_at(b)..base_at(b)).collect(),
        sides,
    };
    let head = (prefix > 0).then(|| {
        let sides = conflict.sides.iter().map(|r| r.start..r.start + prefix).collect();
        shared(sides, &|b| b.start)
    });
    let tail = (suffix > 0).then(|| {
        let sides = conflict.sides.iter().map(|r| r.end - suffix..r.end).collect();
        shared(sides, &|b| b.end)
    });
    let middle = Region {
        kind: Kind::Conflict,
        bases: conflict.bases.clone(),
        sides: conflict.sides.iter().map(|r| r.start + prefix..r.end - suffix).collect(),
    };
    head.into_iter().chain([middle]).chain(tail).collect()
}

/// Split a three-way conflict at each block of lines both sides share inside it, where one
/// base position fits the block on both sides. The blocks become [`Kind::AllSame`] regions
/// with empty base ranges; the pieces between stay conflicts, since where base lines go
/// between them is often open. An open base line goes to the later piece. Conflicts with more
/// sides, and conflicts whose sides are all equal, stay whole. With `boundaries`, a block
/// whose first or last line edge lies inside a syntax node of either side does not split.
fn split_shared_blocks(
    inputs: &Inputs,
    conflict: &Region,
    mode: Comparison,
    boundaries: Option<&dyn Boundaries>,
) -> Vec<Region> {
    let ([base_text], [left_text, right_text], [base_lines], [left_lines, right_lines]) = (
        inputs.bases.as_slice(),
        inputs.sides.as_slice(),
        conflict.bases.as_slice(),
        conflict.sides.as_slice(),
    ) else {
        return vec![conflict.clone()];
    };
    if all_equal(&keys_each(&inputs.sides, &conflict.sides, mode)) {
        return vec![conflict.clone()];
    }
    let diff = |(a, a_lines): (&Text, &Range<usize>), (b, b_lines): (&Text, &Range<usize>)| {
        diff_line_ranges(a, a_lines.clone(), b, b_lines.clone(), mode)
    };
    let (base, left, right) =
        ((base_text, base_lines), (left_text, left_lines), (right_text, right_lines));
    let (to_left, to_right) = (diff(base, left), diff(base, right));
    let shared = matched_runs(&diff(left, right), left_lines.len());
    if shared.is_empty() {
        return vec![conflict.clone()];
    }

    let piece = |kind, b: Range<usize>, l: Range<usize>, r: Range<usize>| {
        let base = base_lines.start + b.start..base_lines.start + b.end;
        Region {
            kind,
            bases: vec![base],
            sides: vec![
                left_lines.start + l.start..left_lines.start + l.end,
                right_lines.start + r.start..right_lines.start + r.end,
            ],
        }
    };
    let differ = |l: &Range<usize>, r: &Range<usize>| {
        let ranges = [
            left_lines.start + l.start..left_lines.start + l.end,
            right_lines.start + r.start..right_lines.start + r.end,
        ];
        if all_equal(&keys_each(&inputs.sides, &ranges, mode)) {
            Kind::AllSame
        } else {
            Kind::Conflict
        }
    };
    let cuts = boundaries.map(|boundaries| {
        (
            boundaries.node_cuts(left_text, left_lines.clone()),
            boundaries.node_cuts(right_text, right_lines.clone()),
        )
    });
    let clean = |run: &SharedRun| {
        let edge = |text: &Text, cuts: &Cuts, line: usize| cuts.clean(text.span(0..line).len());
        let (left_end, right_end) = (run.left + run.len, run.right + run.len);
        cuts.as_ref().is_none_or(|(in_left, in_right)| {
            edge(left_text, in_left, left_lines.start + run.left)
                && edge(left_text, in_left, left_lines.start + left_end)
                && edge(right_text, in_right, right_lines.start + run.right)
                && edge(right_text, in_right, right_lines.start + right_end)
        })
    };
    let mut pieces = Vec::new();
    let (mut b, mut l, mut r) = (0, 0, 0);
    for run in shared.into_iter().filter(clean) {
        let Some(at) = split_point(&to_left, &to_right, run, b) else {
            continue;
        };
        let (left_end, right_end) = (run.left + run.len, run.right + run.len);
        pieces.push(piece(
            differ(&(l..run.left), &(r..run.right)),
            b..at,
            l..run.left,
            r..run.right,
        ));
        pieces.push(piece(Kind::AllSame, at..at, run.left..left_end, run.right..right_end));
        (b, l, r) = (at, left_end, right_end);
    }
    if pieces.is_empty() {
        return vec![conflict.clone()];
    }
    let ends = (base_lines.len(), left_lines.len(), right_lines.len());
    pieces.push(piece(differ(&(l..ends.1), &(r..ends.2)), b..ends.0, l..ends.1, r..ends.2));
    pieces
}

/// A block of lines left and right share: `len` lines from `left` in one, from `right` in
/// the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SharedRun {
    left: usize,
    right: usize,
    len: usize,
}

/// The blocks between `hunks` of a diff from `left_len` lines on.
fn matched_runs(hunks: &[Hunk], left_len: usize) -> Vec<SharedRun> {
    let mut runs = Vec::with_capacity(hunks.len() + 1);
    let (mut left, mut right) = (0, 0);
    for hunk in hunks {
        if hunk.before.start > left {
            runs.push(SharedRun { left, right, len: hunk.before.start - left });
        }
        (left, right) = (hunk.before.end, hunk.after.end);
    }
    if left_len > left {
        runs.push(SharedRun { left, right, len: left_len - left });
    }
    runs
}

/// The base position, no lower than `from`, that fits `run` on both sides: a cut before and
/// after it that each side's diff from base allows. The lowest, when several fit.
fn split_point(to_left: &[Hunk], to_right: &[Hunk], run: SharedRun, from: usize) -> Option<usize> {
    let fits = [
        base_cuts(to_left, run.left),
        base_cuts(to_right, run.right),
        base_cuts(to_left, run.left + run.len),
        base_cuts(to_right, run.right + run.len),
        from..=usize::MAX,
    ];
    let start = fits.iter().map(|cuts| *cuts.start()).max()?;
    let end = fits.iter().map(|cuts| *cuts.end()).min()?;
    (start <= end).then_some(start)
}

/// The base positions a cut before line `at` of a side may take, given the side's diff from
/// base. Between hunks the lines match and the cut has one place. At or inside a hunk, any
/// split of the hunk is a valid alignment, so the cut may take any place in its base range.
fn base_cuts(hunks: &[Hunk], at: usize) -> RangeInclusive<usize> {
    let (mut base, mut side) = (0, 0);
    for hunk in hunks {
        if at < hunk.after.start {
            break;
        }
        if at <= hunk.after.end {
            return hunk.before.start..=hunk.before.end;
        }
        (base, side) = (hunk.before.end, hunk.after.end);
    }
    let cut = base + (at - side);
    cut..=cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundaries::{Marked, node};
    use crate::diff::Whitespace;
    use crate::testing::text;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    const BASE: usize = 0;
    const LEFT_ONLY: Kind = Kind::OneSide(LEFT);
    const RIGHT_ONLY: Kind = Kind::OneSide(RIGHT);

    fn three(base: &str, left: &str, right: &str) -> Inputs {
        Inputs::three_way(text(base), text(left), text(right))
    }

    fn git(diff: DiffOptions, refine: bool) -> Alignment {
        Alignment {
            semantics: Semantics::Git,
            diff,
            same_change: SameChange::Accept,
            hunk_level: HunkLevel::Line,
            refine: refining(refine),
        }
    }

    /// Refinement at the ends, or none.
    fn refining(on: bool) -> Refine {
        if on { Refine::Ends } else { Refine::Off }
    }

    fn merge_with(
        base: &str,
        left: &str,
        right: &str,
        opts: DiffOptions,
        refine: bool,
    ) -> Vec<Region> {
        align(&three(base, left, right), git(opts, refine))
    }

    /// Refinement on, no whitespace mode.
    fn merge(base: &str, left: &str, right: &str) -> Vec<Region> {
        merge_with(base, left, right, DiffOptions::default(), true)
    }

    fn unrefined(base: &str, left: &str, right: &str) -> Vec<Region> {
        merge_with(base, left, right, DiffOptions::default(), false)
    }

    fn ws(base: &str, left: &str, right: &str, whitespace: Whitespace) -> Vec<Region> {
        let opts = DiffOptions { whitespace, ..DiffOptions::default() };
        merge_with(base, left, right, opts, true)
    }

    fn r3(kind: Kind, base: Range<usize>, left: Range<usize>, right: Range<usize>) -> Region {
        Region { kind, bases: vec![base], sides: vec![left, right] }
    }

    // The spike's seven alignment cases (commit 65d590d).

    #[test]
    fn identical_inputs_are_one_unchanged_region() {
        assert_eq!(merge("a\nb\n", "a\nb\n", "a\nb\n"), [r3(Kind::Unchanged, 0..2, 0..2, 0..2)]);
    }

    #[test]
    fn empty_inputs_have_no_regions() {
        assert_eq!(merge("", "", ""), []);
    }

    #[test]
    fn left_only_change_is_surrounded_by_unchanged_regions() {
        assert_eq!(
            merge("a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(LEFT_ONLY, 1..2, 1..2, 1..2),
                r3(Kind::Unchanged, 2..3, 2..3, 2..3),
            ]
        );
    }

    #[test]
    fn right_insertion_shifts_later_right_ranges() {
        assert_eq!(
            merge("a\nb\n", "a\nb\n", "a\nX\nY\nb\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(RIGHT_ONLY, 1..1, 1..1, 1..3),
                r3(Kind::Unchanged, 1..2, 1..2, 3..4),
            ]
        );
    }

    #[test]
    fn same_change_on_both_sides_is_all_same() {
        assert_eq!(
            merge("a\nb\n", "a\nB\n", "a\nB\n"),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(Kind::AllSame, 1..2, 1..2, 1..2),]
        );
    }

    #[test]
    fn different_changes_to_one_line_conflict() {
        assert_eq!(
            merge("a\nb\n", "a\nL\n", "a\nR\n"),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(Kind::Conflict, 1..2, 1..2, 1..2),]
        );
    }

    #[test]
    fn touching_changes_on_different_sides_conflict() {
        assert_eq!(
            merge("a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nb\nC\nd\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..3, 1..3, 1..3),
                r3(Kind::Unchanged, 3..4, 3..4, 3..4),
            ]
        );
    }

    // Insertions, deletions and empty bases

    #[test]
    fn changes_one_line_apart_stay_separate() {
        assert_eq!(
            merge("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n"),
            [
                r3(LEFT_ONLY, 0..1, 0..1, 0..1),
                r3(Kind::Unchanged, 1..2, 1..2, 1..2),
                r3(RIGHT_ONLY, 2..3, 2..3, 2..3),
            ]
        );
    }

    #[test]
    fn same_insertion_at_the_same_place_is_all_same() {
        assert_eq!(
            merge("a\nb\n", "a\nX\nb\n", "a\nX\nb\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::AllSame, 1..1, 1..2, 1..2),
                r3(Kind::Unchanged, 1..2, 2..3, 2..3),
            ]
        );
    }

    #[test]
    fn different_insertions_at_the_same_place_conflict() {
        assert_eq!(
            merge("a\nb\n", "a\nL\nb\n", "a\nR\nb\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..1, 1..2, 1..2),
                r3(Kind::Unchanged, 1..2, 2..3, 2..3),
            ]
        );
    }

    #[test]
    fn insertion_touching_a_change_on_the_other_side_conflicts() {
        // Left inserts X at base line 2, right changes base line 1: they touch at 2.
        assert_eq!(
            merge("a\nb\nc\n", "a\nb\nX\nc\n", "a\nB\nc\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..2, 1..3, 1..2),
                r3(Kind::Unchanged, 2..3, 3..4, 2..3),
            ]
        );
    }

    #[test]
    fn deletion_against_modification_conflicts() {
        assert_eq!(
            merge("a\nb\nc\n", "a\nc\n", "a\nB\nc\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..2, 1..1, 1..2),
                r3(Kind::Unchanged, 2..3, 1..2, 2..3),
            ]
        );
    }

    #[test]
    fn same_deletion_on_both_sides_is_all_same() {
        assert_eq!(
            merge("a\nb\nc\n", "a\nc\n", "a\nc\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::AllSame, 1..2, 1..1, 1..1),
                r3(Kind::Unchanged, 2..3, 1..2, 1..2),
            ]
        );
    }

    #[test]
    fn add_add_with_an_empty_base_is_one_conflict_without_refinement() {
        assert_eq!(unrefined("", "x\nL\ny\n", "x\nR\ny\n"), [r3(Kind::Conflict, 0..0, 0..3, 0..3)]);
    }

    #[test]
    fn add_add_refinement_splits_off_shared_first_and_last_lines() {
        // git merge-file --zdiff3 keeps x and y outside the markers.
        assert_eq!(
            merge("", "x\nL\ny\n", "x\nR\ny\n"),
            [
                r3(Kind::AllSame, 0..0, 0..1, 0..1),
                r3(Kind::Conflict, 0..0, 1..2, 1..2),
                r3(Kind::AllSame, 0..0, 2..3, 2..3),
            ]
        );
    }

    #[test]
    fn add_add_of_the_same_text_is_all_same() {
        assert_eq!(merge("", "x\n", "x\n"), [r3(Kind::AllSame, 0..0, 0..1, 0..1)]);
    }

    #[test]
    fn one_side_adding_to_an_empty_base_is_one_side() {
        assert_eq!(merge("", "", "x\n"), [r3(RIGHT_ONLY, 0..0, 0..0, 0..1)]);
    }

    #[test]
    fn change_without_a_final_newline_is_a_change() {
        assert_eq!(
            merge("a\nb\n", "a\nb", "a\nb\n"),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(LEFT_ONLY, 1..2, 1..2, 1..2),]
        );
    }

    #[test]
    fn hunks_slide_as_in_git_merge_file_without_the_indent_heuristic() {
        // Right inserts `}`, `    c`, `{` after base line 0; the indent heuristic would place
        // the insertion before line 0, clear of left's change. git merge-file conflicts.
        assert_eq!(
            merge("{\n  a\n\n}\n  b\n", "{\n  A\n\n}\n  b\n", "{\n}\n    c\n{\n  a\n\n}\n  b\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..2, 1..2, 1..5),
                r3(Kind::Unchanged, 2..5, 2..5, 5..8),
            ]
        );
    }

    // Refinement. Expected regions from `git merge-file --zdiff3` and `--diff3`.

    #[test]
    fn refinement_splits_off_two_shared_leading_lines() {
        assert_eq!(
            merge("a\nb\nd\n", "a\nS1\nS2\nL\nd\n", "a\nS1\nS2\nR\nd\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::AllSame, 1..1, 1..3, 1..3),
                r3(Kind::Conflict, 1..2, 3..4, 3..4),
                r3(Kind::Unchanged, 2..3, 4..5, 4..5),
            ]
        );
    }

    #[test]
    fn refinement_splits_off_two_shared_trailing_lines() {
        assert_eq!(
            merge("a\nb\nd\n", "a\nL\nT1\nT2\nd\n", "a\nR\nT1\nT2\nd\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..2, 1..2, 1..2),
                r3(Kind::AllSame, 2..2, 2..4, 2..4),
                r3(Kind::Unchanged, 2..3, 4..5, 4..5),
            ]
        );
    }

    #[test]
    fn without_refinement_shared_lines_stay_in_the_conflict() {
        assert_eq!(
            unrefined("a\nb\nd\n", "a\nS1\nS2\nL\nd\n", "a\nS1\nS2\nR\nd\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..2, 1..4, 1..4),
                r3(Kind::Unchanged, 2..3, 4..5, 4..5),
            ]
        );
    }

    #[test]
    fn refinement_keeps_a_shared_middle_line_in_the_conflict() {
        assert_eq!(
            merge("a\nb\nd\n", "a\nL1\nS\nL2\nd\n", "a\nR1\nS\nR2\nd\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..2, 1..4, 1..4),
                r3(Kind::Unchanged, 2..3, 4..5, 4..5),
            ]
        );
    }

    #[test]
    fn refinement_stops_when_one_side_runs_out() {
        // Left X, right X Y: git leaves an empty left section in the markers.
        assert_eq!(
            merge("a\nb\nc\n", "a\nX\nc\n", "a\nX\nY\nc\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::AllSame, 1..1, 1..2, 1..2),
                r3(Kind::Conflict, 1..2, 2..2, 2..3),
                r3(Kind::Unchanged, 2..3, 2..3, 3..4),
            ]
        );
    }

    #[test]
    fn conflicts_separated_by_a_brace_line_stay_two_conflicts() {
        assert_eq!(
            merge("a\nb\n}\nc\nd\n", "a\nL1\n}\nL2\nd\n", "a\nR1\n}\nR2\nd\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..2, 1..2, 1..2),
                r3(Kind::Unchanged, 2..3, 2..3, 2..3),
                r3(Kind::Conflict, 3..4, 3..4, 3..4),
                r3(Kind::Unchanged, 4..5, 4..5, 4..5),
            ]
        );
    }

    #[test]
    fn refinement_leaves_non_conflicts_alone() {
        assert_eq!(
            merge("a\nb\n", "a\nX\nB\n", "a\nX\nB\n"),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(Kind::AllSame, 1..2, 1..3, 1..3),]
        );
    }

    // Whitespace modes decide equality only.

    #[test]
    fn whitespace_only_change_is_unchanged_under_trailing() {
        assert_eq!(
            ws("a\nb\nc\n", "a\nb  \nc\n", "a\nb\nc\n", Whitespace::Trailing),
            [r3(Kind::Unchanged, 0..3, 0..3, 0..3)]
        );
    }

    #[test]
    fn whitespace_only_change_is_a_change_without_a_mode() {
        assert_eq!(
            ws("a\nb\nc\n", "a\nb  \nc\n", "a\nb\nc\n", Whitespace::None),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(LEFT_ONLY, 1..2, 1..2, 1..2),
                r3(Kind::Unchanged, 2..3, 2..3, 2..3),
            ]
        );
    }

    #[test]
    fn whitespace_only_change_against_a_real_change_is_one_side_under_trailing() {
        assert_eq!(
            ws("a\nb\n", "a\nb \n", "a\nB\n", Whitespace::Trailing),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(RIGHT_ONLY, 1..2, 1..2, 1..2),]
        );
    }

    #[test]
    fn sides_equal_under_the_mode_are_all_same() {
        assert_eq!(
            ws("a\nb\n", "a\nB \n", "a\nB\r\n", Whitespace::Trailing),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(Kind::AllSame, 1..2, 1..2, 1..2),]
        );
    }

    #[test]
    fn refinement_compares_lines_under_the_mode() {
        assert_eq!(
            ws("a\nb\nd\n", "a\nS \nL\nd\n", "a\nS\nR\nd\n", Whitespace::Trailing),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::AllSame, 1..1, 1..2, 1..2),
                r3(Kind::Conflict, 1..2, 2..3, 2..3),
                r3(Kind::Unchanged, 2..3, 3..4, 3..4),
            ]
        );
    }

    // More than two sides. Order: side 0, base 0, side 1, base 1, side 2.

    fn five(s0: &str, b0: &str, s1: &str, b1: &str, s2: &str) -> Inputs {
        Inputs::new(vec![text(s0), text(s1), text(s2)], vec![text(b0), text(b1)])
            .expect("three sides and two bases")
    }

    fn r5(kind: Kind, bases: [Range<usize>; 2], sides: [Range<usize>; 3]) -> Region {
        Region { kind, bases: Vec::from(bases), sides: Vec::from(sides) }
    }

    #[test]
    fn three_sides_with_a_conflict_on_one_line() {
        let abc = "a\nb\nc\n";
        let inputs = five("a\nX\nc\n", abc, abc, abc, "a\nY\nc\n");
        assert_eq!(
            align(&inputs, git(DiffOptions::default(), true)),
            [
                r5(Kind::Unchanged, [0..1, 0..1], [0..1, 0..1, 0..1]),
                r5(Kind::Conflict, [1..2, 1..2], [1..2, 1..2, 1..2]),
                r5(Kind::Unchanged, [2..3, 2..3], [2..3, 2..3, 2..3]),
            ]
        );
    }

    #[test]
    fn three_sides_resolve_when_each_other_side_cancels_a_base() {
        // s0 = b0 = P and s1 = b1 = Q cancel, leaving s2.
        let inputs = five("a\nP\n", "a\nP\n", "a\nQ\n", "a\nQ\n", "a\nR\n");
        assert_eq!(
            align(&inputs, git(DiffOptions::default(), true)),
            [
                r5(Kind::Unchanged, [0..1, 0..1], [0..1, 0..1, 0..1]),
                r5(Kind::OneSide(2), [1..2, 1..2], [1..2, 1..2, 1..2]),
            ]
        );
    }

    #[test]
    fn three_sides_with_a_side_insertion_shift_its_later_ranges() {
        let ab = "a\nb\n";
        let inputs = five(ab, ab, "a\nX\nb\n", ab, ab);
        assert_eq!(
            align(&inputs, git(DiffOptions::default(), true)),
            [
                r5(Kind::Unchanged, [0..1, 0..1], [0..1, 0..1, 0..1]),
                r5(Kind::OneSide(1), [1..1, 1..1], [1..1, 1..2, 1..1]),
                r5(Kind::Unchanged, [1..2, 1..2], [1..2, 2..3, 1..2]),
            ]
        );
    }

    #[test]
    fn three_sides_with_a_base_insertion_shift_its_later_ranges() {
        // Base 1 holds X where base 0 has nothing; every side matches base 0.
        let ab = "a\nb\n";
        let inputs = five(ab, ab, ab, "a\nX\nb\n", ab);
        assert_eq!(
            align(&inputs, git(DiffOptions::default(), true)),
            [
                r5(Kind::Unchanged, [0..1, 0..1], [0..1, 0..1, 0..1]),
                r5(Kind::AllSame, [1..1, 1..2], [1..1, 1..1, 1..1]),
                r5(Kind::Unchanged, [1..2, 2..3], [1..2, 1..2, 1..2]),
            ]
        );
    }

    #[test]
    fn three_sides_refine_lines_every_side_shares() {
        let ab = "a\nb\n";
        let inputs = five("a\nS\nX\n", ab, "a\nS\nY\n", ab, "a\nS\nZ\n");
        assert_eq!(
            align(&inputs, git(DiffOptions::default(), true)),
            [
                r5(Kind::Unchanged, [0..1, 0..1], [0..1, 0..1, 0..1]),
                r5(Kind::AllSame, [1..1, 1..1], [1..2, 1..2, 1..2]),
                r5(Kind::Conflict, [1..2, 1..2], [2..3, 2..3, 2..3]),
            ]
        );
    }

    #[test]
    fn three_sides_resolve_when_every_changed_side_made_the_same_change() {
        // jj's trivial_merge: s2 = b0 = Y cancel; s0 = s1 = X outnumber b1 = b by one, so
        // every changed side made the same change and X wins, in both semantics.
        let inputs = five("a\nX\n", "a\nY\n", "a\nX\n", "a\nb\n", "a\nY\n");
        let expected = [
            r5(Kind::Unchanged, [0..1, 0..1], [0..1, 0..1, 0..1]),
            r5(Kind::OneSide(0), [1..2, 1..2], [1..2, 1..2, 1..2]),
        ];
        assert_eq!(align(&inputs, git(DiffOptions::default(), true)), expected);
        assert_eq!(align(&inputs, jj(Whitespace::None, SameChange::Accept, true)), expected);
        let kept = [expected[0].clone(), r5(Kind::Conflict, [1..2, 1..2], [1..2, 1..2, 1..2])];
        assert_eq!(align(&inputs, jj(Whitespace::None, SameChange::Keep, true)), kept);
    }

    // jj semantics (docs/PLAN.md §B5.3)

    fn jj(whitespace: Whitespace, same_change: SameChange, refine: bool) -> Alignment {
        Alignment {
            semantics: Semantics::Jj,
            diff: DiffOptions { whitespace, ..DiffOptions::default() },
            same_change,
            hunk_level: HunkLevel::Line,
            refine: refining(refine),
        }
    }

    fn jj_merge(base: &str, left: &str, right: &str) -> Vec<Region> {
        align(&three(base, left, right), jj(Whitespace::None, SameChange::Accept, true))
    }

    #[test]
    fn jj_regions_follow_its_hunks() {
        assert_eq!(
            jj_merge("a\nb\nc\nd\n", "a\nB\nc\nd\n", "a\nb\nC\nd\n"),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..3, 1..3, 1..3),
                r3(Kind::Unchanged, 3..4, 3..4, 3..4),
            ]
        );
        assert_eq!(
            jj_merge("a\nb\nc\n", "a\nb\nC\n", "A\nb\nc\n"),
            [
                r3(RIGHT_ONLY, 0..1, 0..1, 0..1),
                r3(Kind::Unchanged, 1..2, 1..2, 1..2),
                r3(LEFT_ONLY, 2..3, 2..3, 2..3),
            ]
        );
    }

    #[test]
    fn jj_has_no_regions_for_empty_inputs() {
        assert_eq!(jj_merge("", "", ""), []);
    }

    // Word-level merge (docs/PLAN.md §B5.4)

    fn by_word(semantics: Semantics, same_change: SameChange) -> Alignment {
        Alignment {
            semantics,
            hunk_level: HunkLevel::Word,
            same_change,
            ..jj(Whitespace::None, same_change, true)
        }
    }

    const SEMANTICS: [Semantics; 2] = [Semantics::Git, Semantics::Jj];

    #[test]
    fn a_conflict_of_separate_words_word_merges_in_both_semantics() {
        let inputs = three("a\nlet x = 1;\nb\n", "a\nlet y = 1;\nb\n", "a\nlet x = 2;\nb\n");
        for semantics in SEMANTICS {
            let regions = align(&inputs, by_word(semantics, SameChange::Accept));
            assert_eq!(
                regions,
                [
                    r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                    r3(Kind::WordMerged, 1..2, 1..2, 1..2),
                    r3(Kind::Unchanged, 2..3, 2..3, 2..3),
                ],
                "{semantics:?}"
            );
            assert_eq!(
                word_merge(&inputs, &regions[1], SameChange::Accept).as_deref(),
                Some("let y = 2;\n"),
                "{semantics:?}"
            );
        }
    }

    // Syntax-aware word merge (docs/PLAN.md §B5.8). Line 1 starts at byte 2:
    // `let y = 1;` has y at 6..7, `let x = 2;` has 2 at 10..11.
    const WORD_BASE: &str = "a\nlet x = 1;\nb\n";
    const WORD_LEFT: &str = "a\nlet y = 1;\nb\n";
    const WORD_RIGHT: &str = "a\nlet x = 2;\nb\n";

    fn word_conflict() -> Region {
        r3(Kind::Conflict, 1..2, 1..2, 1..2)
    }

    #[test]
    fn a_word_merge_whose_hunks_sit_at_clean_cuts_keeps_its_text() {
        let inputs = three(WORD_BASE, WORD_LEFT, WORD_RIGHT);
        // A one-byte node has no inside; `let` does not reach the right's hunk; right's x,
        // at 6..7, is not the text the merge takes.
        let marked = Marked(vec![(WORD_LEFT, node(6..7)), (WORD_RIGHT, vec![2..5, 5..8])]);
        assert_eq!(
            word_merge_bounded(&inputs, &word_conflict(), SameChange::Accept, Some(&marked))
                .as_deref(),
            Some("let y = 2;\n")
        );
    }

    #[test]
    fn a_word_merge_with_a_hunk_inside_a_token_stays_a_conflict() {
        let inputs = three(WORD_BASE, WORD_LEFT, WORD_RIGHT);
        // Across the start or the end of left's y, or of right's 2.
        for marked in [
            Marked(vec![(WORD_LEFT, node(5..7))]),
            Marked(vec![(WORD_LEFT, node(6..8))]),
            Marked(vec![(WORD_RIGHT, node(9..11))]),
            Marked(vec![(WORD_RIGHT, node(10..12))]),
        ] {
            assert_eq!(
                word_merge_bounded(&inputs, &word_conflict(), SameChange::Accept, Some(&marked)),
                None,
                "{marked:?}"
            );
            for semantics in SEMANTICS {
                let aligned =
                    align_bounded(&inputs, by_word(semantics, SameChange::Accept), Some(&marked));
                assert_eq!(
                    aligned.regions,
                    [
                        r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                        word_conflict(),
                        r3(Kind::Unchanged, 2..3, 2..3, 2..3),
                    ],
                    "{semantics:?}, {marked:?}"
                );
                assert!(aligned.word_merged.is_empty(), "no word-merged text");
            }
        }
    }

    #[test]
    fn a_hunk_both_sides_make_needs_clean_cuts_in_each() {
        // Both sides change x to y, at 6..7; right also changes 1 to 2.
        let right = "a\nlet y = 2;\nb\n";
        let inputs = three(WORD_BASE, WORD_LEFT, right);
        let conflict = word_conflict();
        assert_eq!(
            word_merge_bounded(&inputs, &conflict, SameChange::Accept, None).as_deref(),
            Some("let y = 2;\n")
        );
        let marked = Marked(vec![(right, node(5..8))]);
        assert_eq!(word_merge_bounded(&inputs, &conflict, SameChange::Accept, Some(&marked)), None);
    }

    #[test]
    fn align_merged_keeps_each_word_merged_regions_text_in_order() {
        let inputs = three("p q\nk\nr\nk\nu v\n", "P q\nk\ns\nk\nU v\n", "p Q\nk\nt\nk\nu V\n");
        for semantics in SEMANTICS {
            let aligned = align_merged(&inputs, by_word(semantics, SameChange::Accept));
            let kinds: Vec<Kind> = aligned.regions.iter().map(|r| r.kind).collect();
            assert_eq!(
                kinds,
                [
                    Kind::WordMerged,
                    Kind::Unchanged,
                    Kind::Conflict,
                    Kind::Unchanged,
                    Kind::WordMerged,
                ],
                "{semantics:?}"
            );
            assert_eq!(aligned.word_merged, ["P Q\n", "U V\n"], "{semantics:?}");
        }
    }

    #[test]
    fn align_merged_keeps_no_text_at_the_line_level() {
        let inputs = three("p q\n", "P q\n", "p Q\n");
        let aligned = align_merged(&inputs, jj(Whitespace::None, SameChange::Accept, true));
        assert_eq!(aligned.regions, [r3(Kind::Conflict, 0..1, 0..1, 0..1)]);
        assert_eq!(aligned.word_merged, Vec::<String>::new());
    }

    #[test]
    fn a_conflict_over_several_lines_word_merges_whole() {
        let inputs = three("a b\nc d\n", "A b\nc d\n", "a b\nc D\n");
        for semantics in SEMANTICS {
            let regions = align(&inputs, by_word(semantics, SameChange::Accept));
            assert_eq!(regions, [r3(Kind::WordMerged, 0..2, 0..2, 0..2)], "{semantics:?}");
            assert_eq!(
                word_merge(&inputs, &regions[0], SameChange::Accept).as_deref(),
                Some("A b\nc D\n")
            );
        }
    }

    #[test]
    fn a_conflict_that_resolves_only_in_part_stays_a_whole_conflict() {
        let inputs = three("let x = 1;\n", "let y = 1;\n", "let z = 2;\n");
        for semantics in SEMANTICS {
            let regions = align(&inputs, by_word(semantics, SameChange::Accept));
            assert_eq!(regions, [r3(Kind::Conflict, 0..1, 0..1, 0..1)], "{semantics:?}");
            assert_eq!(word_merge(&inputs, &regions[0], SameChange::Accept), None);
        }
    }

    #[test]
    fn keep_leaves_a_same_word_change_a_conflict() {
        let inputs = three("x = 1;\n", "x = 2;\n", "x = 2;\n");
        for semantics in SEMANTICS {
            assert_eq!(
                align(&inputs, by_word(semantics, SameChange::Keep)),
                [r3(Kind::Conflict, 0..1, 0..1, 0..1)],
                "{semantics:?}"
            );
            assert_eq!(
                align(&inputs, by_word(semantics, SameChange::Accept)),
                [r3(Kind::AllSame, 0..1, 0..1, 0..1)],
                "{semantics:?}: the line level resolves it first"
            );
        }
    }

    #[test]
    fn word_merges_accept_a_same_word_change_beside_another() {
        // Both sides change `1` to `2`; left also changes `x` to `y`.
        let inputs = three("x = 1;\n", "y = 2;\n", "x = 2;\n");
        for semantics in SEMANTICS {
            let regions = align(&inputs, by_word(semantics, SameChange::Accept));
            assert_eq!(regions, [r3(Kind::WordMerged, 0..1, 0..1, 0..1)], "{semantics:?}");
            assert_eq!(
                word_merge(&inputs, &regions[0], SameChange::Accept).as_deref(),
                Some("y = 2;\n")
            );
            assert_eq!(
                align(&inputs, by_word(semantics, SameChange::Keep)),
                [r3(Kind::Conflict, 0..1, 0..1, 0..1)],
                "{semantics:?}: keep"
            );
        }
    }

    #[test]
    fn line_level_leaves_word_resolvable_conflicts_alone() {
        let inputs = three("let x = 1;\n", "let y = 1;\n", "let x = 2;\n");
        for semantics in SEMANTICS {
            let line =
                Alignment { hunk_level: HunkLevel::Line, ..by_word(semantics, SameChange::Accept) };
            assert_eq!(
                align(&inputs, line),
                [r3(Kind::Conflict, 0..1, 0..1, 0..1)],
                "{semantics:?}"
            );
        }
    }

    #[test]
    fn word_merging_compares_bytes_whatever_the_whitespace_mode() {
        // Under `all` the lines' keys differ only by the words; the merge keeps every byte.
        let inputs = three("f(a, b)\n", "f(a,  B)\n", "g(a, b)\n");
        let opts = Alignment {
            diff: DiffOptions { whitespace: Whitespace::All, ..DiffOptions::default() },
            ..by_word(Semantics::Jj, SameChange::Accept)
        };
        let regions = align(&inputs, opts);
        assert_eq!(regions, [r3(Kind::WordMerged, 0..1, 0..1, 0..1)]);
        assert_eq!(
            word_merge(&inputs, &regions[0], SameChange::Accept).as_deref(),
            Some("g(a,  B)\n")
        );
    }

    #[test]
    fn jj_and_git_place_a_conflict_differently_on_lines_repeated_past_jjs_limit() {
        // 101 x and 101 y lines. Left adds z at both ends; right changes the last y. jj
        // anchors no line that occurs over 100 times and finds no common prefix or suffix
        // with left, so the whole file is one conflict. git keeps left's first z apart.
        const COUNT: usize = 101;
        let base = "x\n".repeat(COUNT) + &"y\n".repeat(COUNT);
        let left = format!("z\n{base}z\n");
        let right = "x\n".repeat(COUNT) + &"y\n".repeat(COUNT - 1) + "Y\n";
        let (lines, last) = (2 * COUNT, 2 * COUNT - 1);
        assert_eq!(
            merge(&base, &left, &right),
            [
                r3(LEFT_ONLY, 0..0, 0..1, 0..0),
                r3(Kind::Unchanged, 0..last, 1..last + 1, 0..last),
                r3(Kind::Conflict, last..lines, last + 1..lines + 2, last..lines),
            ]
        );
        assert_eq!(
            jj_merge(&base, &left, &right),
            [r3(Kind::Conflict, 0..lines, 0..lines + 2, 0..lines)]
        );
    }

    #[test]
    fn jj_whitespace_modes_compare_lines_by_their_keys() {
        let (base, left, right) = ("a\nb\n", "a \nb\n", "a\nB\n");
        let inputs = three(base, left, right);
        // Byte for byte, left changes line 0 and right line 1: no line is common to all.
        assert_eq!(
            align(&inputs, jj(Whitespace::None, SameChange::Accept, true)),
            [r3(Kind::Conflict, 0..2, 0..2, 0..2)]
        );
        assert_eq!(
            align(&inputs, jj(Whitespace::Trailing, SameChange::Accept, true)),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(RIGHT_ONLY, 1..2, 1..2, 1..2),]
        );
    }

    #[test]
    fn keeping_the_same_change_makes_a_whole_conflict_in_both_semantics() {
        let inputs = three("a\nb\n", "a\nB\n", "a\nB\n");
        let expected =
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(Kind::Conflict, 1..2, 1..2, 1..2)];
        let git_keep =
            Alignment { same_change: SameChange::Keep, ..git(DiffOptions::default(), true) };
        assert_eq!(align(&inputs, git_keep), expected);
        assert_eq!(align(&inputs, jj(Whitespace::None, SameChange::Keep, true)), expected);
        assert_eq!(
            align(&inputs, jj(Whitespace::None, SameChange::Accept, true)),
            [r3(Kind::Unchanged, 0..1, 0..1, 0..1), r3(Kind::AllSame, 1..2, 1..2, 1..2),]
        );
    }

    #[test]
    fn jj_refines_conflicts_as_git_does() {
        let (base, left, right) = ("o\n", "s\nL\nt\n", "s\nR\nt\n");
        assert_eq!(
            jj_merge(base, left, right),
            [
                r3(Kind::AllSame, 0..0, 0..1, 0..1),
                r3(Kind::Conflict, 0..1, 1..2, 1..2),
                r3(Kind::AllSame, 1..1, 2..3, 2..3),
            ]
        );
        assert_eq!(
            align(&three(base, left, right), jj(Whitespace::None, SameChange::Accept, false)),
            [r3(Kind::Conflict, 0..1, 0..3, 0..3)]
        );
    }

    fn utf16le(s: &str) -> Text {
        const BOM: [u8; 2] = [0xFF, 0xFE];
        let bytes: Vec<u8> =
            BOM.into_iter().chain(s.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        Text::decode(bytes).expect("UTF-16 decodes")
    }

    fn utf8_bom(s: &str) -> Text {
        const BOM: &str = "\u{feff}";
        Text::decode(format!("{BOM}{s}").into_bytes()).expect("UTF-8 decodes")
    }

    /// Regions where right changes line 0 and left line 2, around a shared line 1.
    fn apart() -> [Region; 3] {
        [
            r3(RIGHT_ONLY, 0..1, 0..1, 0..1),
            r3(Kind::Unchanged, 1..2, 1..2, 1..2),
            r3(LEFT_ONLY, 2..3, 2..3, 2..3),
        ]
    }

    #[test]
    fn jj_merges_utf16_inputs_as_their_decoded_text() {
        let (base, left, right) = ("a\nb\nc\n", "a\nb\nC\n", "A\nb\nc\n");
        let inputs = Inputs::three_way(utf16le(base), utf16le(left), utf16le(right));
        assert_eq!(align(&inputs, jj(Whitespace::None, SameChange::Accept, true)), apart());
    }

    #[test]
    fn jj_merges_inputs_that_disagree_on_a_byte_order_mark_as_their_decoded_text() {
        // jj itself would see base's and right's first lines start with the mark, and left's
        // not. triad drops the mark on decoding, so every first line is `a` or `A`.
        let inputs =
            Inputs::three_way(utf8_bom("a\nb\nc\n"), text("a\nb\nC\n"), utf8_bom("A\nb\nc\n"));
        assert_eq!(align(&inputs, jj(Whitespace::None, SameChange::Accept, true)), apart());
    }

    // Input shape

    #[test]
    fn inputs_need_one_base_fewer_than_sides() {
        let t = || text("a\n");
        assert_eq!(
            Inputs::new(vec![t(), t()], vec![t(), t()]).err(),
            Some(ShapeError { sides: 2, bases: 2 })
        );
        assert_eq!(
            Inputs::new(vec![t(), t(), t()], vec![t()]).err(),
            Some(ShapeError { sides: 3, bases: 1 })
        );
    }

    #[test]
    fn inputs_need_at_least_two_sides() {
        assert_eq!(
            Inputs::new(vec![text("a\n")], vec![]).err(),
            Some(ShapeError { sides: 1, bases: 0 })
        );
    }

    #[test]
    fn two_sides_and_one_base_are_a_three_way_merge() {
        let t = || text("a\n");
        assert!(Inputs::three_way(t(), t(), t()).is_three_way());
        assert!(Inputs::new(vec![t(), t()], vec![t()]).unwrap().is_three_way());
        let four_way = Inputs::new(vec![t(), t(), t()], vec![t(), t()]).unwrap();
        assert!(!four_way.is_three_way());
    }

    #[test]
    fn three_way_inputs_put_left_and_right_around_the_base() {
        let inputs = three("b\n", "l\n", "r\n");
        assert_eq!(inputs.text(Source::Side(LEFT)).content(), "l\n");
        assert_eq!(inputs.text(Source::Base(BASE)).content(), "b\n");
        assert_eq!(inputs.text(Source::Side(RIGHT)).content(), "r\n");
    }

    // Settings form

    #[test]
    fn refine_reads_its_names_and_the_older_flags() {
        let read = |json: &str| serde_json::from_str::<Refine>(json).map_err(|e| e.to_string());
        assert_eq!(read(r#""off""#), Ok(Refine::Off));
        assert_eq!(read(r#""ends""#), Ok(Refine::Ends));
        assert_eq!(read(r#""deep""#), Ok(Refine::Deep));
        assert_eq!(read("true"), Ok(Refine::Ends));
        assert_eq!(read("false"), Ok(Refine::Off));
        assert_eq!(
            read(r#""Deep""#),
            Err(r#"unknown refinement "Deep": expected true, false or "deep""#.to_owned())
        );
        assert!(read("1").is_err_and(|e| e.contains("untagged")), "a number");
    }

    #[test]
    fn refine_reads_back_what_it_writes() {
        for refine in [Refine::Off, Refine::Ends, Refine::Deep] {
            let json = serde_json::to_string(&refine).expect("serialises");
            assert_eq!(serde_json::from_str::<Refine>(&json).ok(), Some(refine), "{json}");
        }
    }

    // Properties (spec §8.2). Small alphabets make lines repeat, which is where merges go wrong.

    const PROPTEST_SEED: u64 = 0x3E36_E000;
    const PROPTEST_CASES: u32 = 512;

    fn config() -> Config {
        Config {
            cases: PROPTEST_CASES,
            rng_seed: RngSeed::Fixed(PROPTEST_SEED),
            ..Config::default()
        }
    }

    fn file_strategy() -> impl Strategy<Value = String> {
        (
            prop::collection::vec(
                prop::sample::select(vec!["a\n", "b\n", "c\n", "}\n", "a \n", "\n"]),
                0..16,
            ),
            prop::sample::select(vec!["", "a", "b"]),
        )
            .prop_map(|(lines, last)| lines.concat() + last)
    }

    /// Each input's ranges start at 0, follow on from each other and end at its line count.
    fn assert_covers(
        texts: &[Text],
        ranges: impl Fn(&Region) -> &[Range<usize>],
        regions: &[Region],
    ) {
        for (i, t) in texts.iter().enumerate() {
            let end = regions.iter().try_fold(0, |at, r| {
                let range = &ranges(r)[i];
                (range.start == at && range.end >= range.start).then_some(range.end)
            });
            assert_eq!(end, Some(t.line_count()), "input {i} is covered in order");
        }
    }

    fn semantics() -> impl Strategy<Value = Semantics> {
        prop::sample::select(vec![Semantics::Git, Semantics::Jj])
    }

    fn swap(kind: Kind) -> Kind {
        match kind {
            Kind::OneSide(LEFT) => RIGHT_ONLY,
            Kind::OneSide(RIGHT) => LEFT_ONLY,
            other => other,
        }
    }

    fn refines() -> impl Strategy<Value = Refine> {
        prop::sample::select(vec![Refine::Off, Refine::Ends, Refine::Deep])
    }

    // Deep refinement (docs/PLAN.md §B5.7)

    fn deep(semantics: Semantics) -> Alignment {
        Alignment { semantics, refine: Refine::Deep, ..git(DiffOptions::default(), true) }
    }

    const DEEP_BASE: &str = "a\np\nq\nz\n";
    /// Lines start at bytes 0, 2, 4, 6 and 8; the shared m is 4..6.
    const DEEP_LEFT: &str = "a\nX\nm\nY\nz\n";
    const DEEP_RIGHT: &str = "a\nZ\nm\nW\nz\n";

    #[test]
    fn deep_refinement_keeps_a_split_whose_edges_are_clean_cuts() {
        let inputs = three(DEEP_BASE, DEEP_LEFT, DEEP_RIGHT);
        // Nodes that end at m's start, and start at its end.
        let marked = Marked(vec![(DEEP_LEFT, node(1..4)), (DEEP_RIGHT, node(6..9))]);
        assert_eq!(
            align_bounded(&inputs, deep(Semantics::Git), Some(&marked)).regions,
            align(&inputs, deep(Semantics::Git))
        );
    }

    #[test]
    fn deep_refinement_drops_a_split_at_a_cut_inside_a_node_of_either_side() {
        let inputs = three(DEEP_BASE, DEEP_LEFT, DEEP_RIGHT);
        let whole = [
            r3(Kind::Unchanged, 0..1, 0..1, 0..1),
            r3(Kind::Conflict, 1..3, 1..4, 1..4),
            r3(Kind::Unchanged, 3..4, 4..5, 4..5),
        ];
        for marked in [
            // Across m's start or end, in left or in right.
            Marked(vec![(DEEP_LEFT, node(3..5))]),
            Marked(vec![(DEEP_LEFT, node(5..7))]),
            Marked(vec![(DEEP_RIGHT, node(3..5))]),
            Marked(vec![(DEEP_RIGHT, node(5..7))]),
        ] {
            for semantics in SEMANTICS {
                assert_eq!(
                    align_bounded(&inputs, deep(semantics), Some(&marked)).regions,
                    whole,
                    "{semantics:?}, {marked:?}"
                );
            }
        }
    }

    #[test]
    fn deep_refinement_drops_only_the_splits_inside_a_node() {
        // Left's lines start at 0, 2, 5, 7, 10, 12 and 15: m is 5..7, n is 10..12. A node
        // across m's edges drops that split; n's stays.
        let left = "a\nX1\nm\nX2\nn\nX3\nz\n";
        let inputs = three("a\np\nq\nr\nz\n", left, "a\nY1\nm\nY2\nn\nY3\nz\n");
        let marked = Marked(vec![(left, node(4..8))]);
        assert_eq!(
            align_bounded(&inputs, deep(Semantics::Git), Some(&marked)).regions,
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..1, 1..4, 1..4),
                r3(Kind::AllSame, 1..1, 4..5, 4..5),
                r3(Kind::Conflict, 1..4, 5..6, 5..6),
                r3(Kind::Unchanged, 4..5, 6..7, 6..7),
            ]
        );
    }

    #[test]
    fn boundaries_change_nothing_without_deep_refinement() {
        let inputs = three(DEEP_BASE, DEEP_LEFT, DEEP_RIGHT);
        let marked = Marked(vec![(DEEP_LEFT, node(0..10)), (DEEP_RIGHT, node(0..10))]);
        for refine in [Refine::Off, Refine::Ends] {
            let opts = Alignment { refine, ..deep(Semantics::Git) };
            assert_eq!(
                align_bounded(&inputs, opts, Some(&marked)).regions,
                align(&inputs, opts),
                "{refine:?}"
            );
        }
    }

    #[test]
    fn deep_refinement_splits_a_conflict_at_a_line_both_sides_add() {
        // Both sides replace p, q with three lines, the middle one the same. Base is replaced
        // whole on both sides, so its lines may go on either side of m: the later piece.
        let inputs = three("a\np\nq\nz\n", "a\nX\nm\nY\nz\n", "a\nZ\nm\nW\nz\n");
        for semantics in SEMANTICS {
            let ends = Alignment { semantics, ..git(DiffOptions::default(), true) };
            assert_eq!(
                align(&inputs, ends),
                [
                    r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                    r3(Kind::Conflict, 1..3, 1..4, 1..4),
                    r3(Kind::Unchanged, 3..4, 4..5, 4..5),
                ],
                "{semantics:?}: refinement at the ends keeps m inside"
            );
            assert_eq!(
                align(&inputs, deep(semantics)),
                [
                    r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                    r3(Kind::Conflict, 1..1, 1..2, 1..2),
                    r3(Kind::AllSame, 1..1, 2..3, 2..3),
                    r3(Kind::Conflict, 1..3, 3..4, 3..4),
                    r3(Kind::Unchanged, 3..4, 4..5, 4..5),
                ],
                "{semantics:?}"
            );
        }
    }

    #[test]
    fn deep_refinement_splits_at_every_shared_block() {
        let inputs =
            three("a\np\nq\nr\nz\n", "a\nX1\nm\nX2\nn\nX3\nz\n", "a\nY1\nm\nY2\nn\nY3\nz\n");
        assert_eq!(
            align(&inputs, deep(Semantics::Git)),
            [
                r3(Kind::Unchanged, 0..1, 0..1, 0..1),
                r3(Kind::Conflict, 1..1, 1..2, 1..2),
                r3(Kind::AllSame, 1..1, 2..3, 2..3),
                r3(Kind::Conflict, 1..1, 3..4, 3..4),
                r3(Kind::AllSame, 1..1, 4..5, 4..5),
                r3(Kind::Conflict, 1..4, 5..6, 5..6),
                r3(Kind::Unchanged, 4..5, 6..7, 6..7),
            ]
        );
    }

    #[test]
    fn deep_refinement_classifies_pieces_by_their_own_lines_deep_in_the_file() {
        // The conflict starts at line 3. Between m and n, left adds Y and right adds nothing:
        // a piece with one empty side, which is still a conflict.
        let base = "a\nb\nc\np\nq\nz\n";
        let left = "a\nb\nc\nX\nm\nY\nn\nV\nz\n";
        let right = "a\nb\nc\nW\nm\nn\nU\nz\n";
        let expected = [
            r3(Kind::Unchanged, 0..3, 0..3, 0..3),
            r3(Kind::Conflict, 3..3, 3..4, 3..4),
            r3(Kind::AllSame, 3..3, 4..5, 4..5),
            r3(Kind::Conflict, 3..3, 5..6, 5..5),
            r3(Kind::AllSame, 3..3, 6..7, 5..6),
            r3(Kind::Conflict, 3..5, 7..8, 6..7),
            r3(Kind::Unchanged, 5..6, 8..9, 7..8),
        ];
        assert_eq!(align(&three(base, left, right), deep(Semantics::Git)), expected);
        let swapped = expected.map(|Region { kind, bases, sides }| Region {
            kind: swap(kind),
            bases,
            sides: vec![sides[RIGHT].clone(), sides[LEFT].clone()],
        });
        assert_eq!(
            align(&three(base, right, left), deep(Semantics::Git)),
            swapped,
            "with the sides swapped"
        );
    }

    #[test]
    fn deep_refinement_leaves_a_conflict_of_equal_sides_whole() {
        let inputs = three("a\n", "x\n", "x\n");
        let keep = Alignment { same_change: SameChange::Keep, ..deep(Semantics::Git) };
        assert_eq!(align(&inputs, keep), [r3(Kind::Conflict, 0..1, 0..1, 0..1)]);
    }

    #[test]
    fn deep_refinement_leaves_conflicts_of_more_sides_as_at_the_ends() {
        let pq = "p\nq\n";
        let inputs = five("X\nm\nY\n", pq, "Z\nm\nW\n", pq, "V\nm\nU\n");
        let ends = git(DiffOptions::default(), true);
        let regions = align(&inputs, ends);
        assert_eq!(regions, [r5(Kind::Conflict, [0..2, 0..2], [0..3, 0..3, 0..3])]);
        assert_eq!(align(&inputs, deep(Semantics::Git)), regions);
    }

    fn hunk(before: Range<usize>, after: Range<usize>) -> Hunk {
        Hunk { before, after }
    }

    #[test]
    fn a_cut_between_hunks_has_one_place_and_at_or_inside_a_hunk_any_in_its_base() {
        let replace = [hunk(1..3, 1..2)];
        assert_eq!(base_cuts(&replace, 0), 0..=0);
        assert_eq!(base_cuts(&replace, 1), 1..=3);
        assert_eq!(base_cuts(&replace, 2), 1..=3);
        assert_eq!(base_cuts(&replace, 3), 4..=4);
        assert_eq!(base_cuts(&[hunk(2..2, 2..4)], 3), 2..=2, "inside an insertion");
        assert_eq!(base_cuts(&[hunk(1..3, 1..1)], 1), 1..=3, "at a deletion");
        assert_eq!(base_cuts(&[], 5), 5..=5);
    }

    #[test]
    fn a_block_the_sides_place_at_different_base_positions_does_not_split() {
        // Left inserts the block at base 1, right at base 2, each between matching lines.
        let run = SharedRun { left: 1, right: 2, len: 1 };
        assert_eq!(split_point(&[hunk(1..1, 1..2)], &[hunk(2..2, 2..3)], run, 0), None);
    }

    #[test]
    fn a_block_one_side_replaces_around_takes_the_other_sides_place() {
        let run = SharedRun { left: 1, right: 1, len: 1 };
        let (to_left, to_right) = ([hunk(1..1, 1..2)], [hunk(0..2, 0..3)]);
        assert_eq!(split_point(&to_left, &to_right, run, 0), Some(1));
        assert_eq!(
            split_point(&to_left, &to_right, run, 2),
            None,
            "a split never goes before the last one"
        );
    }

    #[test]
    fn a_block_that_ends_on_a_matched_base_line_does_not_split() {
        // The block's first line is inside a side's hunk, so the cut before it may go at base
        // 0 or 1. Its second line matches base line 1 on one side, so the cut after it can
        // only go at base 2. No single base position fits both cuts.
        let run = SharedRun { left: 1, right: 1, len: 2 };
        let (matched, replaced) = ([hunk(0..1, 0..2)], [hunk(0..2, 0..3)]);
        assert_eq!(split_point(&matched, &replaced, run, 0), None, "left");
        assert_eq!(split_point(&replaced, &matched, run, 0), None, "right");
        assert_eq!(
            split_point(&replaced, &replaced, run, 0),
            Some(0),
            "both sides replace around the block"
        );
    }

    #[test]
    fn shared_runs_are_the_blocks_between_hunks() {
        // Left a X b c, right a Y b c Z.
        assert_eq!(
            matched_runs(&[hunk(1..2, 1..2), hunk(4..4, 4..5)], 4),
            [SharedRun { left: 0, right: 0, len: 1 }, SharedRun { left: 2, right: 2, len: 2 },]
        );
        assert_eq!(matched_runs(&[hunk(0..1, 0..2)], 3), [SharedRun { left: 1, right: 2, len: 2 }]);
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn regions_cover_every_input(
            base in file_strategy(), left in file_strategy(), right in file_strategy(),
            refine in refines(), semantics in semantics(),
        ) {
            let inputs = three(&base, &left, &right);
            let regions = align(&inputs, Alignment { semantics, refine, ..git(DiffOptions::default(), true) });
            assert_covers(inputs.bases(), |r| &r.bases, &regions);
            assert_covers(inputs.sides(), |r| &r.sides, &regions);
            prop_assert!(regions.iter().all(|r| r.bases.iter().chain(&r.sides).any(|s| !s.is_empty())));
        }

        #[test]
        fn swapping_sides_swaps_one_side_kinds_and_ranges(
            base in file_strategy(), left in file_strategy(), right in file_strategy(),
            refine in any::<bool>(), semantics in semantics(),
        ) {
            let opts = Alignment { semantics, ..git(DiffOptions::default(), refine) };
            let forward = align(&three(&base, &left, &right), opts);
            let backward = align(&three(&base, &right, &left), opts);
            let swapped: Vec<Region> = forward
                .into_iter()
                .map(|r| Region {
                    kind: swap(r.kind),
                    bases: r.bases,
                    sides: vec![r.sides[RIGHT].clone(), r.sides[LEFT].clone()],
                })
                .collect();
            prop_assert_eq!(backward, swapped);
        }

        #[test]
        fn an_unchanged_side_gives_the_other_side_without_conflicts(
            base in file_strategy(), other in file_strategy(), refine in refines(),
            semantics in semantics(),
        ) {
            let opts = Alignment { semantics, refine, ..git(DiffOptions::default(), true) };
            let regions = align(&three(&base, &base, &other), opts);
            prop_assert!(
                regions.iter().all(|r| matches!(r.kind, Kind::Unchanged | Kind::OneSide(RIGHT))),
                "{regions:?}"
            );
        }
    }
}
