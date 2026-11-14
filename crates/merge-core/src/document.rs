//! A merge session: regions, their resolutions, the result text and undo (spec §4.4).
//!
//! The result text is the concatenation of every region's text. The document is its single
//! source of truth: each command returns the patches that turn the old text into the new one,
//! and the view applies them to its editor. Edits typed in the editor reach the document as
//! [`Command::Edit`] after the editor has made them (spec §4.5).

use std::borrow::Cow;
use std::ops::Range;

use ropey::{Rope, RopeBuilder};
use serde::{Deserialize, Serialize};

use crate::boundaries::Boundaries;
use crate::diff::DiffOptions;
pub use crate::edit::{BatchError, EditBatch, TextPatch};
use crate::edit::{apply, caret_after, inverse, prepare, starts_after};
use crate::index::{ByteOffset, Line, RegionIx};
use crate::journal::alignment_hash;
use crate::lengths::RegionLengths;
use crate::merge3::{
    Aligned, Alignment, HunkLevel, Inputs, Kind, Refine, Region, SameChange, Semantics, Source,
    align_bounded, word_merge,
};
use crate::region_lines::{LineCount, RegionLines};
use crate::text::{EndingCounts, LineEnding, LineEndings, Text, typed_ending, with_endings};

/// git's and jj's conflict marker length.
pub const DEFAULT_MARKER_LENGTH: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeOptions {
    /// git's or jj's merge (spec §4.3). Absent from a version-2 journal header, which restore
    /// then refuses by its version.
    #[serde(default)]
    pub semantics: Semantics,
    pub diff: DiffOptions,
    /// Whether a region where every side made the same change resolves (spec §4.3).
    #[serde(default)]
    pub same_change: SameChange,
    /// Whether conflicts merge word by word (spec §4.3). Absent from a version-3 journal
    /// header, which restore then refuses by its version.
    #[serde(default)]
    pub hunk_level: HunkLevel,
    /// How far conflicts are refined (spec §4.3). A version-4 journal header holds a bool,
    /// which still parses, and restore then refuses by its version.
    pub refine: Refine,
    /// Whether deep refinement and the word merge keep off the inside of syntax nodes, where
    /// the document is given a grammar (`docs/DESIGN.md` §B5.8). Absent from a
    /// version-6 journal header, which restore then refuses by its version.
    #[serde(default)]
    pub syntax_boundaries: bool,
    /// The requested marker length. Markers grow past marker-like input lines (spec §4.4).
    pub marker_length: usize,
    /// How a two-sided conflict's markers are written. Absent from older journal headers,
    /// which read as [`MarkerStyle::Diff3`].
    #[serde(default)]
    pub marker_style: MarkerStyle,
}

/// How a document was aligned, for a journal to check its replay against (spec §6.2): the
/// options, and a hash of the alignment they gave before any command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub options: MergeOptions,
    /// SHA-256, in hex, of each region's kind, line ranges and text length as aligned.
    pub alignment: String,
}

/// The markers of a two-sided conflict (spec §4.4). More sides take jj's snapshot style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkerStyle {
    /// git's `diff3`: left, the base after `|||||||`, then right.
    #[default]
    Diff3,
    /// git's `merge`: left and right, without the base section.
    Merge,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            semantics: Semantics::default(),
            diff: DiffOptions::default(),
            same_change: SameChange::default(),
            hunk_level: HunkLevel::default(),
            refine: Refine::default(),
            syntax_boundaries: false,
            marker_length: DEFAULT_MARKER_LENGTH,
            marker_style: MarkerStyle::default(),
        }
    }
}

impl MergeOptions {
    #[must_use]
    pub fn alignment(&self) -> Alignment {
        Alignment {
            semantics: self.semantics,
            diff: self.diff.clone(),
            same_change: self.same_change,
            hunk_level: self.hunk_level,
            refine: self.refine,
        }
    }
}

/// Names written after conflict markers: one per side and one per base, or none at all. An
/// empty name leaves its marker bare, or numbered in jj's snapshot style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Labels {
    sides: Vec<String>,
    bases: Vec<String>,
}

impl Labels {
    /// No names: markers stand alone.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn new(sides: Vec<String>, bases: Vec<String>) -> Self {
        Self { sides, bases }
    }

    /// Side `i`'s label, if it has a non-empty one.
    #[must_use]
    pub fn side(&self, i: usize) -> Option<&str> {
        self.sides.get(i).map(String::as_str).filter(|l| !l.is_empty())
    }

    /// Base `i`'s label, if it has a non-empty one.
    #[must_use]
    pub fn base(&self, i: usize) -> Option<&str> {
        self.bases.get(i).map(String::as_str).filter(|l| !l.is_empty())
    }

    #[must_use]
    pub fn three_way(left: &str, base: &str, right: &str) -> Self {
        Self::new(vec![left.to_owned(), right.to_owned()], vec![base.to_owned()])
    }
}

/// How a region's text is chosen (spec §4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The automatic result. Never held by a conflict.
    Auto,
    /// These sources' texts in this order, each at most once. Empty removes the region's text.
    Pick(Vec<Source>),
    /// Text typed in the result pane. It lives in the result text.
    Edited,
    /// A conflict nobody has decided: the result pane shows every side in order, and the
    /// saved file conflict markers.
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Add `source` to the end of the region's pick, or remove it if present. Removing a
    /// conflict's last source leaves it unresolved.
    Toggle {
        region: RegionIx,
        source: Source,
    },
    /// Pick nothing: the region contributes no text.
    Clear {
        region: RegionIx,
    },
    /// Pick these sources in this order, each at most once.
    Pick {
        region: RegionIx,
        sources: Vec<Source>,
    },
    /// Pick these sources in every region `scope` names, as one step. Regions that already
    /// hold the pick stay as they are.
    PickAll {
        scope: PickScope,
        sources: Vec<Source>,
    },
    /// Patches the editor has already made to its text.
    Edit(EditBatch),
    Undo,
    Redo,
}

/// The regions a [`Command::PickAll`] picks in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickScope {
    /// Every unresolved conflict.
    Unresolved,
    /// Every region, so a single source gives that input's text byte for byte.
    All,
}

/// What a command changed, and whether regions merged or split.
///
/// The patches are sorted and non-overlapping, with offsets in the editor's text before any of
/// them, as in an [`EditBatch`]. For [`Command::Edit`] they are the line-ending corrections to
/// the text the user's edit left.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Applied {
    pub patches: Vec<TextPatch>,
    pub regions_changed: bool,
}

/// The text to save (spec §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Every conflict is resolved: the result pane's text.
    Resolved(String),
    /// Some conflict is unresolved: the text with conflict markers in its place.
    Conflicted(String),
}

/// The line endings of the saved output (`docs/DESIGN.md` §E5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputEnding {
    /// Each region keeps its source's bytes (spec §4.1).
    #[default]
    Keep,
    /// Every line ends in base's dominant ending, else left's, else `\n`.
    Base,
    Lf,
    CrLf,
}

impl Output {
    /// The output with every line ending made `to`; unchanged without one.
    #[must_use]
    pub fn with_ending(self, to: Option<LineEnding>) -> Self {
        let Some(to) = to else {
            return self;
        };
        let convert = |text: String| match with_endings(&text, to) {
            Cow::Borrowed(_) => text,
            Cow::Owned(converted) => converted,
        };
        match self {
            Output::Resolved(text) => Output::Resolved(convert(text)),
            Output::Conflicted(text) => Output::Conflicted(convert(text)),
        }
    }

    #[must_use]
    pub fn is_resolved(&self) -> bool {
        matches!(self, Output::Resolved(_))
    }

    #[must_use]
    pub fn into_text(self) -> String {
        match self {
            Output::Resolved(text) | Output::Conflicted(text) => text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("no region {region}: the merge has {regions} regions")]
    NoRegion { region: RegionIx, regions: usize },
    #[error("the merge has no input {input:?}")]
    NoInput { input: Source },
    #[error("input {input:?} is picked twice")]
    RepeatedInput { input: Source },
    #[error(transparent)]
    Edit(#[from] BatchError),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{label_sides} side labels and {label_bases} base labels do not fit a merge of {sides} sides"
)]
pub struct LabelCountError {
    pub label_sides: usize,
    pub label_bases: usize,
    pub sides: usize,
}

/// A region's state, as history keeps it.
#[derive(Debug, Clone)]
struct Slot {
    region: Region,
    resolution: Resolution,
    len: usize,
}

/// Regions `at..at + before.len()` become `after`.
#[derive(Debug, Clone)]
struct Splice {
    at: usize,
    before: Vec<Slot>,
    after: Vec<Slot>,
}

/// A change to the text and the regions. History keeps the steps that undo and redo commands.
#[derive(Debug, Clone)]
struct Step {
    /// Sorted, with offsets in the text before the step.
    patches: Vec<TextPatch>,
    /// Sorted by `at`, with indices of the regions before the step.
    splices: Vec<Splice>,
}

impl Step {
    /// The step that takes the text and regions after this one back to `text` and the regions
    /// it splices.
    fn inverse(&self, text: &Rope) -> Step {
        let patches = inverse(&self.patches, text);
        let ats =
            starts_after(self.splices.iter().map(|s| (s.at..s.at + s.before.len(), s.after.len())));
        let splices = self
            .splices
            .iter()
            .zip(ats)
            .map(|(s, at)| Splice { at, before: s.after.clone(), after: s.before.clone() })
            .collect();
        Step { patches, splices }
    }
}

#[derive(Debug, Clone)]
pub struct MergeDocument {
    inputs: Inputs,
    labels: Labels,
    options: MergeOptions,
    /// The hash of the alignment as [`MergeDocument::new_bounded`] made it.
    alignment: String,
    regions: Vec<Region>,
    resolutions: Vec<Resolution>,
    result: Rope,
    lengths: RegionLengths,
    /// Each region's lines, kept as steps patch it, so a region's lines cost O(log n).
    lines: RegionLines,
    marker_length: usize,
    typed_ending: LineEnding,
    /// The endings of the result's lines, kept as steps patch it.
    endings: EndingCounts,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The last command typed text without a newline, so typing on at its end joins its step.
    typing: bool,
}

impl MergeDocument {
    /// Align the inputs and start with every conflict unresolved and every other region
    /// automatic. A merge of empty inputs has one empty unchanged region.
    ///
    /// # Errors
    ///
    /// [`LabelCountError`] unless `labels` is empty or has one label per side and per base.
    pub fn new(
        inputs: Inputs,
        options: MergeOptions,
        labels: Labels,
    ) -> Result<Self, LabelCountError> {
        Self::new_bounded(inputs, options, labels, None)
    }

    /// [`MergeDocument::new`], aligned with `boundaries` when
    /// [`MergeOptions::syntax_boundaries`] is on.
    ///
    /// # Errors
    ///
    /// As [`MergeDocument::new`].
    pub fn new_bounded(
        inputs: Inputs,
        options: MergeOptions,
        labels: Labels,
        boundaries: Option<&dyn Boundaries>,
    ) -> Result<Self, LabelCountError> {
        let (sides, bases) = (inputs.sides().len(), inputs.bases().len());
        let fits = (labels.sides.len(), labels.bases.len()) == (sides, bases);
        if !fits && labels != Labels::none() {
            return Err(LabelCountError {
                label_sides: labels.sides.len(),
                label_bases: labels.bases.len(),
                sides,
            });
        }
        let Aligned { mut regions, word_merged } = align_bounded(
            &inputs,
            &options.alignment(),
            boundaries.filter(|_| options.syntax_boundaries),
        );
        if regions.is_empty() {
            regions.push(Region {
                kind: Kind::Unchanged,
                bases: vec![0..0; bases],
                sides: vec![0..0; sides],
            });
        }
        let resolutions: Vec<Resolution> = regions
            .iter()
            .map(|r| match r.kind {
                Kind::Conflict => Resolution::Unresolved,
                _ => Resolution::Auto,
            })
            .collect();
        let mut doc = Self {
            marker_length: marker_length(&inputs, options.marker_length),
            options,
            alignment: String::new(),
            typed_ending: typed_ending(&inputs.bases()[0], &inputs.sides()[0]),
            inputs,
            labels,
            regions,
            resolutions,
            result: Rope::new(),
            endings: EndingCounts::default(),
            lengths: RegionLengths::new(Vec::new()),
            lines: RegionLines::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
        };
        let mut result = RopeBuilder::new();
        let mut word_merged = word_merged.into_iter();
        let (lengths, lines): (Vec<usize>, Vec<LineCount>) = (0..doc.regions.len())
            .map(|r| {
                // align has merged each word-merged region's words; render would merge them again.
                let kept =
                    (doc.regions[r].kind == Kind::WordMerged).then(|| word_merged.next()).flatten();
                let text = kept.map_or_else(|| doc.render(r, &doc.resolutions[r]), Cow::Owned);
                result.append(&text);
                (text.len(), LineCount::of(&text))
            })
            .unzip();
        doc.alignment = alignment_hash(&doc.regions, lengths.iter().copied());
        doc.lengths = RegionLengths::new(lengths);
        doc.lines = RegionLines::new(lines);
        doc.result = result.finish();
        doc.endings = endings_in(&doc.result, 0..doc.result.len_bytes());
        Ok(doc)
    }

    #[must_use]
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    #[must_use]
    pub fn inputs(&self) -> &Inputs {
        &self.inputs
    }

    #[must_use]
    pub fn labels(&self) -> &Labels {
        &self.labels
    }

    #[must_use]
    pub fn options(&self) -> &MergeOptions {
        &self.options
    }

    /// The options and the alignment the document started from. Commands leave it as it is.
    #[must_use]
    pub fn start(&self) -> Start {
        Start { options: self.options.clone(), alignment: self.alignment.clone() }
    }

    #[must_use]
    pub fn resolutions(&self) -> &[Resolution] {
        &self.resolutions
    }

    /// Whether `source` is in `region`'s pick: an explicit pick, or the side an automatic
    /// region takes (spec §4.4).
    #[must_use]
    pub fn is_picked(&self, region: RegionIx, source: Source) -> bool {
        self.current_pick(region.get()).is_some_and(|sources| sources.contains(&source))
    }

    /// The sources `region`'s text comes from, in order. An automatic region that takes one
    /// side's text counts as picking that side. `None` when no pick gives the text: an
    /// unchanged or word-merged region, an edit, or an unresolved conflict.
    fn current_pick(&self, region: usize) -> Option<Cow<'_, [Source]>> {
        match &self.resolutions[region] {
            Resolution::Pick(sources) => Some(Cow::Borrowed(sources)),
            Resolution::Auto => match self.regions[region].kind {
                Kind::OneSide(side) => Some(Cow::Owned(vec![Source::Side(side)])),
                Kind::AllSame => Some(Cow::Owned(vec![Source::Side(0)])),
                Kind::Unchanged | Kind::WordMerged | Kind::Conflict => None,
            },
            Resolution::Edited | Resolution::Unresolved => None,
        }
    }

    /// The marker length in use: the requested one, grown past marker-like input lines.
    #[must_use]
    pub fn marker_length(&self) -> usize {
        self.marker_length
    }

    /// The ending a newline typed into the result takes (spec §4.1).
    #[must_use]
    pub fn typed_ending(&self) -> LineEnding {
        self.typed_ending
    }

    /// # Errors
    ///
    /// [`CommandError`] for a region or input the merge does not have. The document is then
    /// unchanged.
    pub fn apply(&mut self, command: Command) -> Result<Applied, CommandError> {
        match command {
            Command::Toggle { region, source } => {
                self.check_region(region)?;
                self.check_input(source)?;
                let region = region.get();
                let picked = match self.current_pick(region) {
                    Some(sources) if sources.contains(&source) => {
                        sources.iter().copied().filter(|&s| s != source).collect()
                    }
                    Some(sources) => [&*sources, &[source]].concat(),
                    None => vec![source],
                };
                // A conflict never resolves to nothing by a toggle: Clear does that.
                let resolution = if picked.is_empty() && self.regions[region].kind == Kind::Conflict
                {
                    Resolution::Unresolved
                } else {
                    Resolution::Pick(picked)
                };
                Ok(self.resolve(region, resolution))
            }
            Command::Clear { region } => {
                self.check_region(region)?;
                Ok(self.resolve(region.get(), Resolution::Pick(Vec::new())))
            }
            Command::Pick { region, sources } => {
                self.check_region(region)?;
                self.check_pick(&sources)?;
                Ok(self.resolve(region.get(), Resolution::Pick(sources)))
            }
            Command::PickAll { scope, sources } => {
                self.check_pick(&sources)?;
                let regions: Vec<usize> = (0..self.regions.len())
                    .filter(|&r| match scope {
                        PickScope::Unresolved => self.resolutions[r] == Resolution::Unresolved,
                        PickScope::All => self.current_pick(r).as_deref() != Some(&sources),
                    })
                    .collect();
                Ok(self.resolve_all(
                    regions.into_iter().map(|r| (r, Resolution::Pick(sources.clone()))),
                ))
            }
            Command::Edit(batch) => self.edit(batch),
            Command::Undo => Ok(self.replay(History::Undo)),
            Command::Redo => Ok(self.replay(History::Redo)),
        }
    }

    fn check_region(&self, region: RegionIx) -> Result<(), CommandError> {
        let regions = self.regions.len();
        (region.get() < regions).then_some(()).ok_or(CommandError::NoRegion { region, regions })
    }

    /// Every source of a pick is an input of the merge, and none comes twice.
    fn check_pick(&self, sources: &[Source]) -> Result<(), CommandError> {
        for (n, &source) in sources.iter().enumerate() {
            self.check_input(source)?;
            if sources[..n].contains(&source) {
                return Err(CommandError::RepeatedInput { input: source });
            }
        }
        Ok(())
    }

    fn check_input(&self, input: Source) -> Result<(), CommandError> {
        let exists = match input {
            Source::Side(i) => i < self.inputs.sides().len(),
            Source::Base(i) => i < self.inputs.bases().len(),
        };
        exists.then_some(()).ok_or(CommandError::NoInput { input })
    }

    /// Give `region` a new resolution and its text.
    fn resolve(&mut self, region: usize, resolution: Resolution) -> Applied {
        self.resolve_all([(region, resolution)])
    }

    /// Give each region its new resolution and text, as one step. The regions come in order;
    /// none, no step.
    fn resolve_all(
        &mut self,
        resolutions: impl IntoIterator<Item = (usize, Resolution)>,
    ) -> Applied {
        let (patches, splices): (Vec<_>, Vec<_>) = resolutions
            .into_iter()
            .map(|(region, resolution)| {
                let text = self.render(region, &resolution).into_owned();
                let after =
                    Slot { region: self.regions[region].clone(), resolution, len: text.len() };
                let patch = TextPatch { range: self.bytes_of(region), text };
                let splice =
                    Splice { at: region, before: vec![self.slot(region)], after: vec![after] };
                (patch, splice)
            })
            .unzip();
        if splices.is_empty() {
            return Applied::default();
        }
        let step = Step { patches, splices };
        let regions_changed = self.commit(&step, false);
        Applied { patches: step.patches, regions_changed }
    }

    /// Fold an edit the editor has made into the regions (spec §4.5). Newlines in inserted text
    /// take the typed ending; the returned patches make that change in the editor.
    fn edit(&mut self, batch: EditBatch) -> Result<Applied, CommandError> {
        let Some(prepared) = prepare(&self.result, batch, self.typed_ending)? else {
            return Ok(Applied::default());
        };
        let step =
            Step { splices: self.edit_splices(&prepared.patches), patches: prepared.patches };
        Ok(Applied {
            patches: prepared.corrections,
            regions_changed: self.commit(&step, prepared.typing),
        })
    }

    /// One splice for each run of regions the patches touch, turning it into one edited region.
    fn edit_splices(&self, patches: &[TextPatch]) -> Vec<Splice> {
        // Each run of regions, with the bytes its patches insert and remove.
        let mut runs: Vec<(Range<usize>, usize, usize)> = Vec::new();
        for p in patches {
            let touched = self.touched(&p.range);
            match runs.last_mut() {
                Some((run, inserted, removed)) if touched.start < run.end => {
                    run.end = run.end.max(touched.end);
                    *inserted += p.text.len();
                    *removed += p.range.len();
                }
                _ => runs.push((touched, p.text.len(), p.range.len())),
            }
        }
        runs.into_iter()
            .map(|(run, inserted, removed)| {
                let bytes = self.bytes_of(run.start).start..self.bytes_of(run.end - 1).end;
                Splice {
                    at: run.start,
                    before: run.clone().map(|r| self.slot(r)).collect(),
                    after: vec![Slot {
                        region: merged(&self.regions[run]),
                        resolution: Resolution::Edited,
                        len: bytes.len() + inserted - removed,
                    }],
                }
            })
            .collect()
    }

    /// The regions an edit of `range` changes: those holding its bytes, or for an insertion, the
    /// one that takes the text (spec §4.5).
    fn touched(&self, range: &Range<usize>) -> Range<usize> {
        if range.is_empty() {
            let r = self.insertion_region(range.start);
            r..r + 1
        } else {
            self.region_of(range.start)..self.region_of(range.end - 1) + 1
        }
    }

    /// Text inserted mid-line joins the region the line's earlier bytes belong to. At a line
    /// start it goes to the first empty region there, otherwise to the region after.
    fn insertion_region(&self, offset: usize) -> usize {
        if offset > 0 && self.result.byte(offset - 1) != b'\n' {
            return self.region_of(offset - 1);
        }
        let next = self.lengths.first_starting_from(offset);
        let empty_here = next < self.regions.len()
            && self.lengths.len(next) == 0
            && self.lengths.start(next) == offset;
        if empty_here { next } else { self.region_of(offset) }
    }

    fn slot(&self, region: usize) -> Slot {
        Slot {
            region: self.regions[region].clone(),
            resolution: self.resolutions[region].clone(),
            len: self.lengths.len(region),
        }
    }

    /// Make a new step and record its inverse for undo. Typing on at the end of typed text
    /// joins the step before. Returns whether regions merged or split.
    fn commit(&mut self, step: &Step, typing: bool) -> bool {
        let undo = step.inverse(&self.result);
        let regions_changed = self.run(step);
        self.redo.clear();
        let joined = match self.undo.last_mut() {
            Some(top) if typing && self.typing => {
                let caret = caret_after(&top.patches);
                (caret == Some(step.patches[0].range.start)).then(|| {
                    top.patches[0].range.end = undo.patches[0].range.end;
                    top.splices[0].before.clone_from(&undo.splices[0].before);
                })
            }
            _ => None,
        };
        if joined.is_none() {
            self.undo.push(undo);
        }
        self.typing = typing;
        regions_changed
    }

    /// Undo or redo one step, recording its inverse on the other stack.
    fn replay(&mut self, direction: History) -> Applied {
        self.typing = false;
        let from = match direction {
            History::Undo => &mut self.undo,
            History::Redo => &mut self.redo,
        };
        let Some(step) = from.pop() else {
            return Applied::default();
        };
        let inverse = step.inverse(&self.result);
        let regions_changed = self.run(&step);
        match direction {
            History::Undo => self.redo.push(inverse),
            History::Redo => self.undo.push(inverse),
        }
        Applied { patches: step.patches, regions_changed }
    }

    /// Patch the text and splice the regions. Returns whether regions merged or split.
    fn run(&mut self, step: &Step) -> bool {
        // Last patch first, so each one's offsets hold. A patch changes the endings of the
        // newlines it removes and inserts, and of the newline just after it, whose `\r` it
        // may add or take away.
        for patch in step.patches.iter().rev() {
            let reach = |end: usize, text: &Rope| (end + 1).min(text.len_bytes());
            let before =
                endings_in(&self.result, patch.range.start..reach(patch.range.end, &self.result));
            apply(&mut self.result, std::slice::from_ref(patch));
            let inserted = patch.range.start + patch.text.len();
            let after = endings_in(&self.result, patch.range.start..reach(inserted, &self.result));
            self.endings.lf = self.endings.lf - before.lf + after.lf;
            self.endings.crlf = self.endings.crlf - before.crlf + after.crlf;
        }
        let mut regions_changed = false;
        for s in step.splices.iter().rev() {
            let range = s.at..s.at + s.before.len();
            self.regions.splice(range.clone(), s.after.iter().map(|x| x.region.clone()));
            self.resolutions.splice(range.clone(), s.after.iter().map(|x| x.resolution.clone()));
            if s.before.len() == s.after.len() {
                for (region, slot) in range.zip(&s.after) {
                    self.lengths.set(region, slot.len);
                }
            } else {
                regions_changed = true;
                self.lengths.splice(range.clone(), s.after.iter().map(|x| x.len));
                self.lines.splice(range, s.after.iter().map(|_| LineCount::default()));
            }
        }
        // Count the lines of each region the splices wrote, now that every length holds. A
        // splice's regions move by the count change of the splices before it.
        let mut shift = 0_isize;
        for s in &step.splices {
            let start = s.at.strict_add_signed(shift);
            for region in start..start + s.after.len() {
                let bytes = self.bytes_of(region);
                self.lines.set(region, LineCount::in_rope(&self.result, bytes));
            }
            shift += s.after.len().cast_signed() - s.before.len().cast_signed();
        }
        regions_changed
    }

    fn region_text(&self, region: usize) -> String {
        let bytes = self.bytes_of(region);
        self.result.byte_slice(bytes).to_string()
    }

    /// A region's text under `resolution`. An edited region's text is whatever it holds. Text
    /// taken whole from one input borrows it.
    fn render(&self, region: usize, resolution: &Resolution) -> Cow<'_, str> {
        let r = &self.regions[region];
        match resolution {
            // A conflict never holds `Auto`; were it to, it would show its markers.
            Resolution::Auto => auto_text(&self.inputs, r, self.options.same_change)
                .unwrap_or_else(|| self.markers(r).into()),
            Resolution::Pick(sources) => self.pick_text(r, sources),
            Resolution::Edited => self.region_text(region).into(),
            Resolution::Unresolved => self.pick_text(r, &self.sides()),
        }
    }

    /// Every side, in order: what an unresolved conflict shows.
    fn sides(&self) -> Vec<Source> {
        (0..self.inputs.sides().len()).map(Source::Side).collect()
    }

    fn pick_text(&self, region: &Region, sources: &[Source]) -> Cow<'_, str> {
        match self.pieces(region, sources).as_slice() {
            [] => Cow::Borrowed(""),
            &[(_, text, _)] => Cow::Borrowed(text),
            pieces => pieces.iter().flat_map(|&(_, text, joint)| [text, joint]).collect(),
        }
    }

    /// The sources with text, each with that text and the newline joining it to the next: the
    /// typed ending when it lacks one and another source with text follows, otherwise none.
    fn pieces<'a>(
        &'a self,
        region: &Region,
        sources: &[Source],
    ) -> Vec<(Source, &'a str, &'static str)> {
        let texts: Vec<(Source, &str)> = sources
            .iter()
            .map(|&source| (source, self.source_text(region, source)))
            .filter(|(_, t)| !t.is_empty())
            .collect();
        let ending = self.typed_ending.as_str();
        let last = texts.len().saturating_sub(1);
        texts
            .into_iter()
            .enumerate()
            .map(|(n, (source, t))| {
                let joined = n < last && !t.ends_with('\n');
                (source, t, if joined { ending } else { "" })
            })
            .collect()
    }

    fn source_text(&self, region: &Region, source: Source) -> &str {
        let lines = match source {
            Source::Side(i) => region.sides[i].clone(),
            Source::Base(i) => region.bases[i].clone(),
        };
        self.inputs.text(source).span(lines)
    }

    /// Conflict markers: git's diff3 or merge style for two sides, by the options' marker
    /// style; jj's snapshot style, holding every input, for more.
    fn markers(&self, region: &Region) -> String {
        let eol = marker_ending(&self.inputs, region).as_str();
        let line = |c: char, label: Option<&str>| {
            let run = c.to_string().repeat(self.marker_length);
            match label {
                Some(label) => format!("{run} {label}{eol}"),
                None => format!("{run}{eol}"),
            }
        };
        let section = |source: Source| {
            let text = self.source_text(region, source);
            let missing = !text.is_empty() && !text.ends_with('\n');
            format!("{text}{}", if missing { eol } else { "" })
        };
        let (sides, bases) = (&self.labels.sides, &self.labels.bases);
        if self.inputs.is_three_way() {
            let base = match self.options.marker_style {
                MarkerStyle::Diff3 => line('|', label(bases, 0)) + &section(Source::Base(0)),
                MarkerStyle::Merge => String::new(),
            };
            [
                line('<', label(sides, 0)),
                section(Source::Side(0)),
                base,
                line('=', None),
                section(Source::Side(1)),
                line('>', label(sides, 1)),
            ]
            .concat()
        } else {
            let term = |c: char, labels: &[String], i: usize, kind: &str, source: Source| {
                let name =
                    label(labels, i).map_or_else(|| format!("{kind} #{}", i + 1), str::to_owned);
                line(c, Some(&name)) + &section(source)
            };
            let terms = (0..self.inputs.sides().len()).flat_map(|i| {
                let side = term('+', sides, i, "side", Source::Side(i));
                let base = (i < self.inputs.bases().len())
                    .then(|| term('-', bases, i, "base", Source::Base(i)));
                std::iter::once(side).chain(base)
            });
            std::iter::once(line('<', Some("conflict")))
                .chain(terms)
                .chain(std::iter::once(line('>', Some("conflict ends"))))
                .collect()
        }
    }

    /// The result pane's text: every region's text in order.
    #[must_use]
    pub fn text(&self) -> String {
        self.result.to_string()
    }

    /// The bytes each source's text fills in a picked or unresolved region, in order. A
    /// newline joining a source to the next belongs to the first. Sources without text get
    /// none. Automatic and edited regions have none.
    ///
    /// # Panics
    ///
    /// If the region does not exist.
    #[must_use]
    pub fn blocks(&self, region: RegionIx) -> Vec<(Source, Range<usize>)> {
        let region = region.get();
        let sides;
        let sources = match &self.resolutions[region] {
            Resolution::Pick(sources) => sources.as_slice(),
            Resolution::Unresolved => {
                sides = self.sides();
                sides.as_slice()
            }
            Resolution::Auto | Resolution::Edited => return Vec::new(),
        };
        let mut at = self.bytes_of(region).start;
        self.pieces(&self.regions[region], sources)
            .into_iter()
            .map(|(source, text, joint)| {
                let start = at;
                at += text.len() + joint.len();
                (source, start..at)
            })
            .collect()
    }

    /// The text to save.
    #[must_use]
    pub fn output(&self) -> Output {
        if !self.resolutions.contains(&Resolution::Unresolved) {
            return Output::Resolved(self.text());
        }
        let text = self
            .resolutions
            .iter()
            .enumerate()
            .map(|(region, resolution)| match resolution {
                Resolution::Unresolved => self.markers(&self.regions[region]),
                _ => self.region_text(region),
            })
            .collect();
        Output::Conflicted(text)
    }

    /// The text to save with its line endings made as `ending` says.
    #[must_use]
    pub fn output_with(&self, ending: OutputEnding) -> Output {
        let to = match ending {
            OutputEnding::Keep => None,
            OutputEnding::Base => Some(self.typed_ending),
            OutputEnding::Lf => Some(LineEnding::Lf),
            OutputEnding::CrLf => Some(LineEnding::CrLf),
        };
        self.output().with_ending(to)
    }

    /// The endings [`MergeDocument::output_with`] holds under `ending`, without building it.
    #[must_use]
    pub fn saved_endings(&self, ending: OutputEnding) -> LineEndings {
        let to = match ending {
            OutputEnding::Keep => return self.output_endings(),
            OutputEnding::Base => self.typed_ending,
            OutputEnding::Lf => LineEnding::Lf,
            OutputEnding::CrLf => LineEnding::CrLf,
        };
        match (self.output_endings(), to) {
            (LineEndings::None, _) => LineEndings::None,
            (_, LineEnding::Lf) => LineEndings::Lf,
            (_, LineEnding::CrLf) => LineEndings::CrLf,
        }
    }

    /// The endings [`MergeDocument::output`] holds, without building it: the result's, and
    /// for each unresolved conflict, its markers' and base sections'. Its sides' lines are
    /// already in the result.
    #[must_use]
    pub fn output_endings(&self) -> LineEndings {
        let (mut lf, mut crlf) = (self.endings.lf > 0, self.endings.crlf > 0);
        let unresolved = self
            .resolutions
            .iter()
            .zip(&self.regions)
            .filter(|(resolution, _)| **resolution == Resolution::Unresolved);
        for (_, region) in unresolved {
            if lf && crlf {
                break;
            }
            match marker_ending(&self.inputs, region) {
                LineEnding::Lf => lf = true,
                LineEnding::CrLf => crlf = true,
            }
            let three_way = self.inputs.is_three_way();
            let bases = if three_way && self.options.marker_style == MarkerStyle::Merge {
                0
            } else {
                region.bases.len()
            };
            for base in 0..bases {
                let counts =
                    EndingCounts::of(self.source_text(region, Source::Base(base)).as_bytes(), None);
                lf |= counts.lf > 0;
                crlf |= counts.crlf > 0;
            }
        }
        LineEndings::from_presence(lf, crlf)
    }

    /// The region whose text holds byte `offset`. Empty regions hold no bytes. At the end of
    /// the text, the last region.
    ///
    /// # Panics
    ///
    /// If `offset` is past the end of the text.
    #[must_use]
    pub fn region_at_byte(&self, offset: ByteOffset) -> RegionIx {
        RegionIx::new(self.region_of(offset.get()))
    }

    /// [`Self::region_at_byte`] for an offset held across commands: none past the end of the
    /// text.
    #[must_use]
    pub fn region_at(&self, offset: ByteOffset) -> Option<RegionIx> {
        (offset.get() <= self.lengths.total()).then(|| self.region_at_byte(offset))
    }

    fn region_of(&self, offset: usize) -> usize {
        let total = self.lengths.total();
        // An offset past the end is a caller's bug, as with slice indexing.
        assert!(offset <= total, "byte {offset} is past the end of the result ({total} bytes)");
        self.lengths.containing(offset).unwrap_or(self.regions.len() - 1)
    }

    /// # Panics
    ///
    /// If the region does not exist.
    #[must_use]
    pub fn region(&self, region: RegionIx) -> &Region {
        &self.regions[region.get()]
    }

    /// # Panics
    ///
    /// If the region does not exist.
    #[must_use]
    pub fn resolution(&self, region: RegionIx) -> &Resolution {
        &self.resolutions[region.get()]
    }

    /// Every region's index, in order.
    pub fn region_ixs(
        &self,
    ) -> impl DoubleEndedIterator<Item = RegionIx> + ExactSizeIterator + use<> {
        (0..self.regions.len()).map(RegionIx::new)
    }

    /// # Panics
    ///
    /// If the region does not exist.
    #[must_use]
    pub fn region_bytes(&self, region: RegionIx) -> Range<usize> {
        self.bytes_of(region.get())
    }

    /// The offset `region` starts at.
    ///
    /// # Panics
    ///
    /// If the region does not exist.
    #[must_use]
    pub fn region_start(&self, region: RegionIx) -> ByteOffset {
        ByteOffset::new(self.lengths.start(region.get()))
    }

    fn bytes_of(&self, region: usize) -> Range<usize> {
        let start = self.lengths.start(region);
        start..start + self.lengths.len(region)
    }

    /// The line holding byte `offset`. Only `\n` ends a line.
    ///
    /// # Panics
    ///
    /// If `offset` is past the end of the text.
    #[must_use]
    pub fn line_of_byte(&self, offset: ByteOffset) -> Line {
        Line::new(self.result.byte_to_line(offset.get()))
    }

    /// The lines of `region` in the result: from the line its first byte is on to the line
    /// after its last byte. A region without text has none, at the line it would start on.
    /// Costs O(log n) in the regions.
    ///
    /// # Panics
    ///
    /// If the region does not exist.
    #[must_use]
    pub fn region_lines(&self, region: RegionIx) -> Range<Line> {
        let range = self.lines.of(region.get());
        Line::new(range.start)..Line::new(range.end)
    }

    /// The conflicts still unresolved: what a save writes as markers.
    #[must_use]
    pub fn unresolved(&self) -> usize {
        self.resolutions.iter().filter(|r| **r == Resolution::Unresolved).count()
    }

    /// Whether a pick, a clear or an edit has changed any region from its automatic
    /// resolution. Undoing them all leaves nothing changed.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.resolutions.iter().any(|r| matches!(r, Resolution::Pick(_) | Resolution::Edited))
    }

    /// Lines in the result. An unterminated last line counts; an empty text has none.
    #[must_use]
    pub fn line_count(&self) -> usize {
        let len = self.result.len_bytes();
        let terminated = len == 0 || self.result.byte(len - 1) == b'\n';
        self.result.len_lines() - usize::from(terminated)
    }

    /// The byte where `line` starts. The line after the last starts at the end of the text.
    ///
    /// # Panics
    ///
    /// If `line` is past the line after the last.
    #[must_use]
    pub fn byte_of_line(&self, line: Line) -> ByteOffset {
        ByteOffset::new(self.result.line_to_byte(line.get()))
    }
}

#[derive(Clone, Copy)]
enum History {
    Undo,
    Redo,
}

/// One region covering `regions`, which follow each other. It is a conflict if any of them is,
/// otherwise it has the first one's kind.
fn merged(regions: &[Region]) -> Region {
    let (first, last) = (&regions[0], &regions[regions.len() - 1]);
    let kind =
        if regions.iter().any(|r| r.kind == Kind::Conflict) { Kind::Conflict } else { first.kind };
    let join = |a: &[Range<usize>], b: &[Range<usize>]| -> Vec<Range<usize>> {
        a.iter().zip(b).map(|(a, b)| a.start..b.end).collect()
    };
    Region { kind, bases: join(&first.bases, &last.bases), sides: join(&first.sides, &last.sides) }
}

/// Characters that start marker lines in diff3 style and in jj's snapshot style.
const DIFF3_MARKER_CHARS: [char; 4] = ['<', '|', '=', '>'];
const SNAPSHOT_MARKER_CHARS: [char; 5] = ['<', '>', '+', '-', '%'];

/// The requested length, or one more than the longest run of one marker character at the
/// start of any input line when that run is at least the requested length. Under jj, the
/// requested length is jj's `$marker_length` (spec §4.4).
fn marker_length(inputs: &Inputs, requested: usize) -> usize {
    let chars: &[char] =
        if inputs.is_three_way() { &DIFF3_MARKER_CHARS } else { &SNAPSHOT_MARKER_CHARS };
    let run = |line: &str| {
        line.chars()
            .next()
            .filter(|c| chars.contains(c))
            .map_or(0, |first| line.chars().take_while(|&c| c == first).count())
    };
    let longest = inputs
        .sides()
        .iter()
        .chain(inputs.bases())
        .flat_map(Text::lines)
        .map(run)
        .max()
        .unwrap_or(0);
    if longest >= requested { longest + 1 } else { requested }
}

/// git `merge-file`'s rule: CRLF when base's first line ends in CRLF and no side's reference
/// line ends in a bare LF. A side's reference line is the one before the conflict, or its
/// first line when the conflict starts the file. A line without a newline settles nothing.
/// The endings of the newlines in bytes `range` of `text`.
fn endings_in(text: &Rope, range: Range<usize>) -> EndingCounts {
    let mut counts = EndingCounts::default();
    let mut previous = range.start.checked_sub(1).map(|at| text.byte(at));
    for chunk in text.byte_slice(range).chunks() {
        counts.add(chunk.as_bytes(), previous);
        previous = chunk.as_bytes().last().copied().or(previous);
    }
    counts
}

fn marker_ending(inputs: &Inputs, region: &Region) -> LineEnding {
    let ending = |text: &Text, line: usize| -> Option<LineEnding> {
        (line < text.line_count())
            .then(|| text.line(line))
            .filter(|l| l.ends_with('\n'))
            .map(|l| if l.ends_with("\r\n") { LineEnding::CrLf } else { LineEnding::Lf })
    };
    let no_side_lf =
        inputs.sides().iter().zip(&region.sides).all(|(text, range)| {
            ending(text, range.start.saturating_sub(1)) != Some(LineEnding::Lf)
        });
    if no_side_lf && ending(&inputs.bases()[0], 0) == Some(LineEnding::CrLf) {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    }
}

fn label(labels: &[String], i: usize) -> Option<&str> {
    labels.get(i).map(String::as_str).filter(|label| !label.is_empty())
}

/// The automatic text of a region; none for a conflict, which has no automatic text. A
/// word-merged region's text is computed again from the inputs: regions carry no text.
/// Whitespace modes decide equality only: on each line of an unchanged region, the first
/// side whose bytes differ from base gives the bytes (spec §4.2).
#[expect(
    clippy::expect_used,
    reason = "invariant: `align` word-merges a region only after its words merge"
)]
fn auto_text<'a>(
    inputs: &'a Inputs,
    region: &Region,
    same_change: SameChange,
) -> Option<Cow<'a, str>> {
    let side = |i: usize| inputs.sides()[i].span(region.sides[i].clone());
    match region.kind {
        Kind::Unchanged => {
            let base = &inputs.bases()[0];
            let base_text = base.span(region.bases[0].clone());
            let same_bytes = (0..inputs.sides().len()).all(|i| side(i) == base_text);
            if same_bytes {
                return Some(Cow::Borrowed(base_text));
            }
            let start = region.bases[0].start;
            let text = (0..region.bases[0].len())
                .map(|n| {
                    let original = base.line(start + n);
                    inputs
                        .sides()
                        .iter()
                        .zip(&region.sides)
                        .map(|(text, range)| text.line(range.start + n))
                        .find(|&line| line != original)
                        .unwrap_or(original)
                })
                .collect::<String>();
            Some(Cow::Owned(text))
        }
        Kind::OneSide(i) => Some(side(i).into()),
        Kind::AllSame => Some(side(0).into()),
        Kind::WordMerged => Some(Cow::Owned(
            word_merge(inputs, region, same_change)
                .expect("align marks a region word-merged only when its words merge"),
        )),
        Kind::Conflict => None,
    }
}

#[cfg(test)]
#[expect(
    clippy::single_range_in_vec_init,
    reason = "a one-base merge has one base range, and a one-region result one byte range"
)]
mod tests {
    use super::*;
    use crate::boundaries::{Marked, node};
    use crate::diff::{CaseRule, LineEndingRule, Whitespace};
    use crate::merge3::{LEFT, RIGHT};
    use crate::testing::{text, three};

    const L: Source = Source::Side(LEFT);
    const R: Source = Source::Side(RIGHT);
    const B: Source = Source::Base(0);

    fn doc_with(base: &str, left: &str, right: &str, options: MergeOptions) -> MergeDocument {
        let inputs = three(base, left, right);
        MergeDocument::new(inputs, options, Labels::three_way("left", "base", "right"))
            .expect("three labels fit")
    }

    fn doc(base: &str, left: &str, right: &str) -> MergeDocument {
        doc_with(base, left, right, MergeOptions::default())
    }

    fn by_word() -> MergeOptions {
        MergeOptions { hunk_level: HunkLevel::Word, ..MergeOptions::default() }
    }

    #[test]
    fn a_word_merged_region_resolves_to_its_merged_text() {
        let d = doc_with("a\nlet x = 1;\n", "a\nlet y = 1;\n", "a\nlet x = 2;\n", by_word());
        assert_eq!(d.regions()[1].kind, Kind::WordMerged);
        assert_eq!(d.resolutions()[1], Resolution::Auto);
        assert_eq!(d.output(), Output::Resolved("a\nlet y = 2;\n".to_owned()));
    }

    #[test]
    fn word_merged_regions_around_a_conflict_each_take_their_own_text() {
        let mut d = doc_with(
            "p q\nk\nr\nk\nu v\n",
            "P q\nk\ns\nk\nU v\n",
            "p Q\nk\nt\nk\nu V\n",
            by_word(),
        );
        let resolve_conflict =
            Command::Pick { region: RegionIx::new(2), sources: vec![Source::Side(LEFT)] };
        d.apply(resolve_conflict).unwrap();
        assert_eq!(d.output(), Output::Resolved("P Q\nk\ns\nk\nU V\n".to_owned()));
    }

    #[test]
    fn undoing_a_pick_on_a_word_merged_region_returns_to_the_merged_text() {
        let mut d = doc_with("a\nlet x = 1;\n", "a\nlet y = 1;\n", "a\nlet x = 2;\n", by_word());
        d.apply(pick(1, &[R])).expect("applies");
        assert_eq!(text_of(&d), "a\nlet x = 2;\n");
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(text_of(&d), "a\nlet y = 2;\n");
        assert_eq!(d.resolutions()[1], Resolution::Auto);
        d.apply(toggle(1, L)).expect("applies");
        assert_eq!(text_of(&d), "a\nlet y = 1;\n");
        d.apply(toggle(1, L)).expect("applies");
        assert_eq!(text_of(&d), "a\n", "an empty pick removes the text");
    }

    #[test]
    fn a_word_merge_uses_the_documents_same_change_rule() {
        let keep = MergeOptions { same_change: SameChange::Keep, ..by_word() };
        let d = doc_with("x = 1;\n", "y = 2;\n", "x = 2;\n", keep);
        assert_eq!(d.regions()[0].kind, Kind::Conflict);
        let accept = doc_with("x = 1;\n", "y = 2;\n", "x = 2;\n", by_word());
        assert_eq!(accept.output(), Output::Resolved("y = 2;\n".to_owned()));
    }

    #[test]
    fn line_count_counts_an_unterminated_last_line() {
        assert_eq!(doc("", "", "").line_count(), 0);
        assert_eq!(doc("a\n", "a\n", "a\n").line_count(), 1);
        assert_eq!(doc("a\nb", "a\nb", "a\nb").line_count(), 2);
        assert_eq!(doc("a\n\n", "a\n\n", "a\n\n").line_count(), 2);
    }

    #[test]
    fn a_position_past_the_end_is_in_no_region() {
        // Result "a\nB\n": region 0 holds bytes 0..2, region 1 holds 2..4.
        let d = doc("a\nb\n", "a\nB\n", "a\nb\n");
        let at = |offset| d.region_at(ByteOffset::new(offset)).map(RegionIx::get);
        assert_eq!([0, 1, 2, 3, 4, 5].map(at), [Some(0), Some(0), Some(1), Some(1), Some(1), None]);
    }

    #[test]
    fn a_region_starts_after_the_bytes_of_those_before_it() {
        let d = doc("a\nb\n", "a\nB\n", "a\nb\n");
        assert_eq!(d.regions().len(), 2);
        assert_eq!(d.region_start(RegionIx::new(0)), ByteOffset::new(0));
        assert_eq!(d.region_start(RegionIx::new(1)), ByteOffset::new(2));
    }

    #[test]
    fn byte_of_line_gives_line_starts_and_the_end() {
        let d = doc("a\nbc\n", "a\nbc\n", "a\nbc\n");
        assert_eq!([0, 1, 2].map(|line| d.byte_of_line(Line::new(line)).get()), [0, 2, 5]);
    }

    fn patch(range: Range<usize>, text: &str) -> TextPatch {
        TextPatch { range, text: text.to_owned() }
    }

    #[expect(clippy::unnecessary_wraps, reason = "compared with `apply`'s result")]
    fn applied(patches: Vec<TextPatch>) -> Result<Applied, CommandError> {
        Ok(Applied { patches, regions_changed: false })
    }

    fn toggle(region: usize, source: Source) -> Command {
        Command::Toggle { region: RegionIx::new(region), source }
    }

    fn text_of(doc: &MergeDocument) -> String {
        doc.text()
    }

    fn pick(region: usize, sources: &[Source]) -> Command {
        Command::Pick { region: RegionIx::new(region), sources: sources.to_vec() }
    }

    /// Unchanged `a`, a conflict on the middle line, unchanged `c`.
    fn conflict() -> MergeDocument {
        doc("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n")
    }

    /// Region 1 of [`conflict`] in the saved file while it is unresolved.
    const MARKERS: &str = "<<<<<<< left\nL\n||||||| base\nb\n=======\nR\n>>>>>>> right\n";
    /// The end of region 1 of [`conflict`] while it is unresolved: `a\n`, then `L\nR\n`.
    const UNRESOLVED_END: usize = 6;
    /// Region 1 of [`conflict`] in the result pane while it is unresolved.
    const UNRESOLVED: &str = "L\nR\n";

    // Unresolved and changed

    #[test]
    fn a_fresh_merge_counts_its_conflicts_unresolved_and_nothing_changed() {
        let d = doc("a\nb\nc\nd\ne\n", "a\nL\nc\nM\ne\n", "a\nR\nc\nN\ne\n");
        assert_eq!(d.unresolved(), 2);
        assert!(!d.changed());
    }

    #[test]
    fn a_pick_resolves_and_changes_and_its_undo_changes_nothing() {
        let mut d = conflict();
        d.apply(pick(1, &[L])).expect("picks");
        assert_eq!(d.unresolved(), 0);
        assert!(d.changed());
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(d.unresolved(), 1);
        assert!(!d.changed());
    }

    #[test]
    fn a_clear_of_an_automatic_region_changes_it() {
        let mut d = doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n");
        d.apply(Command::Clear { region: RegionIx::new(0) }).expect("clears");
        assert_eq!(d.unresolved(), 0);
        assert!(d.changed());
    }

    #[test]
    fn an_edit_changes_the_merge() {
        let mut d = doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n");
        d.apply(Command::Edit(EditBatch(vec![patch(0..0, "x")]))).expect("applies");
        assert!(d.changed());
    }

    // Starting state and output

    #[test]
    fn resolved_regions_take_their_automatic_text() {
        let d = doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n");
        assert_eq!(d.output(), Output::Resolved("A\nb\nC\n".to_owned()));
        assert_eq!(d.resolutions(), [Resolution::Auto, Resolution::Auto, Resolution::Auto]);
    }

    #[test]
    fn an_unresolved_conflict_shows_left_then_right_and_saves_diff3_markers() {
        let d = conflict();
        assert_eq!(d.text(), "a\nL\nR\nc\n");
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
        assert_eq!(d.resolutions(), [Resolution::Auto, Resolution::Unresolved, Resolution::Auto]);
        assert_eq!(
            (
                d.region_bytes(RegionIx::new(0)),
                d.region_bytes(RegionIx::new(1)),
                d.region_bytes(RegionIx::new(2))
            ),
            (0..2, 2..UNRESOLVED_END, UNRESOLVED_END..UNRESOLVED_END + 2)
        );
    }

    #[test]
    fn an_unresolved_side_without_a_final_newline_gets_the_typed_one_before_the_next() {
        assert_eq!(doc("a\nb", "a\nL", "a\nR").text(), "a\nL\nR");
        assert_eq!(doc("a\r\nb", "a\r\nL", "a\r\nR").text(), "a\r\nL\r\nR");
    }

    #[test]
    fn an_unresolved_conflict_with_a_deleted_side_shows_the_other_side() {
        let d = doc("a\nb\nc\n", "a\nc\n", "a\nR\nc\n");
        assert_eq!(d.regions()[1].kind, Kind::Conflict);
        assert_eq!(d.text(), "a\nR\nc\n");
        assert_eq!(
            d.output().into_text(),
            "a\n<<<<<<< left\n||||||| base\nb\n=======\nR\n>>>>>>> right\nc\n"
        );
    }

    #[test]
    fn an_unresolved_conflict_of_more_sides_shows_every_side_in_order() {
        let inputs = Inputs::new(
            vec![text("a\n"), text("b\n"), text("c\n")],
            vec![text("x\n"), text("x\n")],
        )
        .expect("three sides, two bases");
        let d =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
        assert_eq!(d.text(), "a\nb\nc\n");
        assert_eq!(d.blocks(RegionIx::new(0)), [(L, 0..2), (R, 2..4), (Source::Side(2), 4..6)]);
    }

    // Bulk picks

    fn pick_all(scope: PickScope, sources: &[Source]) -> Command {
        Command::PickAll { scope, sources: sources.to_vec() }
    }

    /// Conflicts at regions 1, 3 and 5; unchanged lines between them.
    fn three_conflicts() -> MergeDocument {
        doc("a\nb\nc\nd\ne\nf\ng\n", "a\nB1\nc\nD1\ne\nF1\ng\n", "a\nB2\nc\nD2\ne\nF2\ng\n")
    }

    #[test]
    fn picking_for_all_unresolved_leaves_picked_and_edited_conflicts_alone() {
        let mut d = three_conflicts();
        d.apply(pick(1, &[R])).expect("applies");
        // Region 3, unresolved, shows "D1\nD2\n" from byte 7.
        d.apply(Command::Edit(EditBatch(vec![patch(7..7, "x")]))).expect("applies");
        d.apply(pick_all(PickScope::Unresolved, &[L])).expect("applies");
        assert_eq!(text_of(&d), "a\nB2\nc\nxD1\nD2\ne\nF1\ng\n");
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![R]));
        assert_eq!(d.resolutions()[3], Resolution::Edited);
        assert_eq!(d.resolutions()[5], Resolution::Pick(vec![L]));
        assert_eq!(d.output(), Output::Resolved("a\nB2\nc\nxD1\nD2\ne\nF1\ng\n".to_owned()));
    }

    #[test]
    fn one_undo_reverts_a_bulk_pick_and_one_redo_makes_it_again() {
        let mut d = three_conflicts();
        let before = text_of(&d);
        d.apply(pick_all(PickScope::Unresolved, &[R])).expect("applies");
        assert_eq!(text_of(&d), "a\nB2\nc\nD2\ne\nF2\ng\n");
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(text_of(&d), before);
        assert!(d.resolutions().iter().skip(1).step_by(2).all(|r| *r == Resolution::Unresolved));
        d.apply(Command::Redo).expect("redoes");
        assert_eq!(text_of(&d), "a\nB2\nc\nD2\ne\nF2\ng\n");
    }

    #[test]
    fn picking_one_side_everywhere_gives_that_sides_bytes() {
        let (base, left, right) =
            ("a\r\nb\r\nc\r\nd\r\ne\r\n", "A\r\nb\r\nL\r\nd\r\ne", "a\r\nb\r\nR\r\nd\r\nE\r\n");
        let mut d = doc(base, left, right);
        // Region 1, unchanged "b\r\n", starts after left's "A\r\n".
        d.apply(Command::Edit(EditBatch(vec![patch(3..3, "x")]))).expect("applies");
        d.apply(pick_all(PickScope::All, &[L])).expect("applies");
        assert_eq!(d.output(), Output::Resolved(left.to_owned()));
        d.apply(pick_all(PickScope::All, &[R])).expect("applies");
        assert_eq!(d.output(), Output::Resolved(right.to_owned()));
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(text_of(&d), left);
        d.apply(pick_all(PickScope::All, &[B])).expect("applies");
        assert_eq!(text_of(&d), base);
    }

    #[test]
    fn a_bulk_pick_with_nothing_to_change_adds_no_undo_step() {
        let mut d = three_conflicts();
        d.apply(pick_all(PickScope::Unresolved, &[L])).expect("applies");
        let after_first = text_of(&d);
        let again = d.apply(pick_all(PickScope::Unresolved, &[R])).expect("applies");
        assert_eq!(again, Applied::default(), "no conflict is unresolved");
        assert_eq!(text_of(&d), after_first);
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(text_of(&d), text_of(&three_conflicts()), "the first pick undone");

        // A left-only region already picks left.
        let mut left_only = doc("a\n", "b\n", "a\n");
        let applied = left_only.apply(pick_all(PickScope::All, &[L])).expect("applies");
        assert_eq!(applied, Applied::default());
        assert_eq!(left_only.resolutions(), [Resolution::Auto]);
    }

    #[test]
    fn a_bulk_pick_of_a_missing_or_repeated_input_changes_nothing() {
        let mut d = three_conflicts();
        let before = text_of(&d);
        let missing = d
            .apply(pick_all(PickScope::Unresolved, &[Source::Side(2)]))
            .expect_err("a three-way merge has two sides");
        assert_eq!(missing, CommandError::NoInput { input: Source::Side(2) });
        let repeated = d.apply(pick_all(PickScope::All, &[L, L])).expect_err("left twice");
        assert_eq!(repeated, CommandError::RepeatedInput { input: L });
        assert_eq!(text_of(&d), before);
        assert_eq!(d.apply(Command::Undo).expect("undoes"), Applied::default());
    }

    #[test]
    fn a_bulk_pick_takes_any_side_of_a_conflict_of_more_sides() {
        let inputs = Inputs::new(
            vec![text("a\n"), text("b\n"), text("c\n")],
            vec![text("x\n"), text("x\n")],
        )
        .expect("three sides, two bases");
        let mut d =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
        d.apply(pick_all(PickScope::Unresolved, &[Source::Side(2)])).expect("applies");
        assert_eq!(d.output(), Output::Resolved("c\n".to_owned()));
    }

    // Blocks

    #[test]
    fn blocks_give_each_source_its_bytes_in_pick_order() {
        let mut d = conflict();
        assert_eq!(d.blocks(RegionIx::new(1)), [(L, 2..4), (R, 4..6)]);
        assert_eq!(d.blocks(RegionIx::new(0)), []);
        d.apply(pick(1, &[R, B, L])).expect("right, base, left");
        assert_eq!(d.blocks(RegionIx::new(1)), [(R, 2..4), (B, 4..6), (L, 6..8)]);
        d.apply(pick(1, &[])).expect("nothing");
        assert_eq!(d.blocks(RegionIx::new(1)), []);
    }

    #[test]
    fn a_joining_newline_belongs_to_the_block_before_it() {
        let mut d = doc("a\nb", "a\nL", "a\nR");
        assert_eq!(d.blocks(RegionIx::new(1)), [(L, 2..4), (R, 4..5)]);
        d.apply(pick(1, &[R, L])).expect("right, left");
        assert_eq!(d.text(), "a\nR\nL");
        assert_eq!(d.blocks(RegionIx::new(1)), [(R, 2..4), (L, 4..5)]);
    }

    #[test]
    fn blocks_skip_sources_without_text() {
        let d = doc("a\nb\nc\n", "a\nc\n", "a\nR\nc\n");
        assert_eq!(d.blocks(RegionIx::new(1)), [(R, 2..4)]);
    }

    #[test]
    fn edited_and_automatic_regions_have_no_blocks() {
        let mut d = doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n");
        assert_eq!(d.blocks(RegionIx::new(0)), []);
        d.apply(pick(2, &[R])).expect("right");
        assert_eq!(d.blocks(RegionIx::new(2)), [(R, 4..6)]);
        let mut c = conflict();
        c.apply(Command::Edit(EditBatch(vec![patch(3..3, "x")]))).expect("type");
        assert_eq!(c.resolutions()[1], Resolution::Edited);
        assert_eq!(c.blocks(RegionIx::new(1)), []);
    }

    // Explicit picks

    #[test]
    fn pick_sets_the_sources_in_the_given_order() {
        let mut d = conflict();
        assert_eq!(d.apply(pick(1, &[R, L])), applied(vec![patch(2..UNRESOLVED_END, "R\nL\n")]));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![R, L]));
        assert_eq!(d.output(), Output::Resolved("a\nR\nL\nc\n".to_owned()));
        assert_eq!(d.apply(pick(1, &[L])), applied(vec![patch(2..UNRESOLVED_END, "L\n")]));
        assert_eq!(d.apply(Command::Undo), applied(vec![patch(2..4, "R\nL\n")]));
        assert_eq!(d.apply(Command::Undo), applied(vec![patch(2..UNRESOLVED_END, "L\nR\n")]));
        assert_eq!(d.resolutions()[1], Resolution::Unresolved);
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
    }

    #[test]
    fn pick_of_the_sides_in_order_resolves_a_conflict_without_changing_its_text() {
        let mut d = conflict();
        assert_eq!(d.apply(pick(1, &[L, R])), applied(vec![patch(2..UNRESOLVED_END, "L\nR\n")]));
        assert_eq!(d.output(), Output::Resolved("a\nL\nR\nc\n".to_owned()));
    }

    #[test]
    fn a_refused_pick_changes_nothing() {
        let mut d = conflict();
        assert_eq!(d.apply(pick(1, &[L, R, L])), Err(CommandError::RepeatedInput { input: L }));
        assert_eq!(
            d.apply(pick(1, &[L, Source::Side(2)])),
            Err(CommandError::NoInput { input: Source::Side(2) })
        );
        assert_eq!(
            d.apply(pick(1, &[Source::Base(1)])),
            Err(CommandError::NoInput { input: Source::Base(1) })
        );
        assert_eq!(
            d.apply(pick(3, &[L])),
            Err(CommandError::NoRegion { region: RegionIx::new(3), regions: 3 })
        );
        assert_eq!(d.text(), "a\nL\nR\nc\n");
        assert_eq!(d.resolutions()[1], Resolution::Unresolved);
        assert_eq!(d.apply(Command::Undo), applied(vec![]));
    }

    #[test]
    fn same_change_on_both_sides_takes_left() {
        let d = doc("a\n", "b\n", "b\n");
        assert_eq!(d.output(), Output::Resolved("b\n".to_owned()));
    }

    #[test]
    fn empty_inputs_have_one_empty_region() {
        let d = doc("", "", "");
        assert_eq!(d.output(), Output::Resolved(String::new()));
        assert_eq!(d.regions().len(), 1);
        assert_eq!(d.regions()[0].kind, Kind::Unchanged);
        assert_eq!(
            (d.region_bytes(RegionIx::new(0)), d.region_at_byte(ByteOffset::new(0)).get()),
            (0..0, 0)
        );
    }

    // Picks

    #[test]
    fn toggling_a_source_on_an_unresolved_conflict_picks_it() {
        let mut d = conflict();
        assert_eq!(d.apply(toggle(1, L)), applied(vec![patch(2..UNRESOLVED_END, "L\n")]));
        assert_eq!(d.output(), Output::Resolved("a\nL\nc\n".to_owned()));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![L]));
        assert_eq!(d.region_bytes(RegionIx::new(2)), 4..6);
    }

    #[test]
    fn toggling_appends_and_removes_in_selection_order() {
        let mut d = conflict();
        d.apply(toggle(1, L)).expect("left");
        assert_eq!(d.apply(toggle(1, R)), applied(vec![patch(2..4, "L\nR\n")]));
        assert_eq!(d.apply(toggle(1, L)), applied(vec![patch(2..6, "R\n")]));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![R]));
        d.apply(toggle(1, L)).expect("left again");
        assert_eq!(text_of(&d), "a\nR\nL\nc\n");
        d.apply(toggle(1, B)).expect("base");
        assert_eq!(text_of(&d), "a\nR\nL\nb\nc\n");
        d.apply(toggle(1, L)).expect("left off");
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![R, B]));
        assert_eq!(text_of(&d), "a\nR\nb\nc\n");
    }

    #[test]
    fn base_alone_can_be_picked() {
        let mut d = conflict();
        d.apply(toggle(1, B)).expect("base");
        assert_eq!(d.output(), Output::Resolved("a\nb\nc\n".to_owned()));
    }

    #[test]
    fn toggling_the_last_source_off_a_conflict_leaves_it_unresolved() {
        let mut d = conflict();
        d.apply(toggle(1, R)).expect("right");
        assert_eq!(d.apply(toggle(1, R)), applied(vec![patch(2..4, UNRESOLVED)]));
        assert_eq!(d.resolutions()[1], Resolution::Unresolved);
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
        d.apply(Command::Undo).expect("undo");
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![R]));
        assert_eq!(text_of(&d), "a\nR\nc\n");
    }

    #[test]
    fn toggling_off_the_last_of_several_sources_leaves_a_conflict_unresolved() {
        let mut d = conflict();
        d.apply(pick(1, &[L, B])).expect("left, base");
        d.apply(toggle(1, L)).expect("left off");
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![B]));
        d.apply(toggle(1, B)).expect("base off");
        assert_eq!(d.resolutions()[1], Resolution::Unresolved);
        assert_eq!(text_of(&d), "a\nL\nR\nc\n");
    }

    #[test]
    fn clear_and_an_empty_pick_still_resolve_a_conflict_to_nothing() {
        let mut d = conflict();
        d.apply(pick(1, &[])).expect("empty pick");
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![]));
        assert_eq!(d.output(), Output::Resolved("a\nc\n".to_owned()));
    }

    #[test]
    fn clear_picks_nothing() {
        let mut d = conflict();
        assert_eq!(
            d.apply(Command::Clear { region: RegionIx::new(1) }),
            applied(vec![patch(2..UNRESOLVED_END, "")])
        );
        assert_eq!(d.output(), Output::Resolved("a\nc\n".to_owned()));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![]));
        assert_eq!(d.region_bytes(RegionIx::new(1)), 2..2);
        assert_eq!(
            d.apply(Command::Clear { region: RegionIx::new(0) }),
            applied(vec![patch(0..2, "")])
        );
        assert_eq!(text_of(&d), "c\n");
    }

    /// Left changed `a`, right changed `c`: `LeftOnly`, `Unchanged`, `RightOnly`.
    fn one_sided() -> MergeDocument {
        doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n")
    }

    /// Whether left, base and right are in `region`'s pick, in that order.
    fn picked(d: &MergeDocument, region: usize) -> [bool; 3] {
        [L, B, R].map(|source| d.is_picked(RegionIx::new(region), source))
    }

    #[test]
    fn toggling_another_source_on_an_automatic_region_follows_the_side_it_takes() {
        let mut d = one_sided();
        assert_eq!(d.apply(toggle(0, R)), applied(vec![patch(0..2, "A\na\n")]));
        assert_eq!(d.resolutions()[0], Resolution::Pick(vec![L, R]));
        assert_eq!(text_of(&d), "A\na\nb\nC\n");
    }

    #[test]
    fn toggling_the_side_an_automatic_region_takes_removes_it() {
        let mut d = one_sided();
        assert_eq!(d.apply(toggle(0, L)), applied(vec![patch(0..2, "")]));
        assert_eq!(d.resolutions()[0], Resolution::Pick(vec![]));
        assert_eq!(d.apply(toggle(2, R)), applied(vec![patch(2..4, "")]));
        assert_eq!(d.resolutions()[2], Resolution::Pick(vec![]));
        assert_eq!(text_of(&d), "b\n");
    }

    #[test]
    fn a_same_change_region_takes_left() {
        let mut d = doc("a\n", "S\n", "S\n");
        assert_eq!(d.regions()[0].kind, Kind::AllSame);
        assert_eq!(picked(&d, 0), [true, false, false]);
        d.apply(toggle(0, R)).expect("right");
        assert_eq!(d.resolutions()[0], Resolution::Pick(vec![L, R]));
        assert_eq!(text_of(&d), "S\nS\n");
    }

    #[test]
    fn an_automatic_region_has_the_side_it_takes_picked() {
        let d = one_sided();
        assert_eq!(picked(&d, 0), [true, false, false]);
        assert_eq!(picked(&d, 1), [false; 3], "an unchanged region has no pick");
        assert_eq!(picked(&d, 2), [false, false, true]);
    }

    #[test]
    fn a_word_merged_region_has_nothing_picked() {
        let d = doc_with("a\nlet x = 1;\n", "a\nlet y = 1;\n", "a\nlet x = 2;\n", by_word());
        assert_eq!(d.regions()[1].kind, Kind::WordMerged);
        assert_eq!(picked(&d, 1), [false; 3]);
    }

    #[test]
    fn picked_follows_an_explicit_pick_and_nothing_else() {
        let mut d = conflict();
        assert_eq!(picked(&d, 1), [false; 3], "unresolved");
        d.apply(pick(1, &[R, B])).expect("applies");
        assert_eq!(picked(&d, 1), [false, true, true]);
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clears");
        assert_eq!(picked(&d, 1), [false; 3], "an empty pick");
        let mut e = conflict();
        e.apply(Command::Edit(EditBatch(vec![patch(3..3, "x")]))).expect("type");
        assert_eq!(e.resolutions()[1], Resolution::Edited);
        assert_eq!(picked(&e, 1), [false; 3], "an edit");
    }

    #[test]
    fn a_picked_source_without_a_final_newline_gets_one_before_the_next() {
        let mut d = doc("a\nb", "a\nL", "a\nR");
        d.apply(toggle(1, L)).expect("left");
        assert_eq!(d.apply(toggle(1, R)), applied(vec![patch(2..3, "L\nR")]));
        let mut crlf = doc("a\r\nb", "a\r\nL", "a\r\nR");
        crlf.apply(toggle(1, L)).expect("left");
        crlf.apply(toggle(1, R)).expect("right");
        assert_eq!(text_of(&crlf), "a\r\nL\r\nR");
    }

    #[test]
    fn an_empty_source_adds_no_newline_after_the_one_before() {
        // At the end of the file base's section is empty, so left's `L` stays last.
        let mut d = doc("a\n", "a\nL", "a\nR");
        d.apply(toggle(1, L)).expect("left");
        d.apply(toggle(1, B)).expect("base");
        assert_eq!(d.output(), Output::Resolved("a\nL".to_owned()));
    }

    // Undo and redo

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut d = conflict();
        d.apply(toggle(1, L)).expect("left");
        d.apply(toggle(1, R)).expect("right");
        assert_eq!(d.apply(Command::Undo), applied(vec![patch(2..6, "L\n")]));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![L]));
        assert_eq!(d.apply(Command::Undo), applied(vec![patch(2..4, UNRESOLVED)]));
        assert_eq!(d.resolutions()[1], Resolution::Unresolved);
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
        assert_eq!(d.apply(Command::Undo), applied(vec![]));
        assert_eq!(d.apply(Command::Redo), applied(vec![patch(2..UNRESOLVED_END, "L\n")]));
        assert_eq!(d.apply(Command::Redo), applied(vec![patch(2..4, "L\nR\n")]));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![L, R]));
        assert_eq!(d.apply(Command::Redo), applied(vec![]));
        assert_eq!(text_of(&d), "a\nL\nR\nc\n");
    }

    #[test]
    fn a_new_command_drops_the_redo_history() {
        let mut d = conflict();
        d.apply(toggle(1, L)).expect("left");
        d.apply(Command::Undo).expect("undo");
        d.apply(toggle(1, R)).expect("right");
        assert_eq!(d.apply(Command::Redo), applied(vec![]));
        assert_eq!(text_of(&d), "a\nR\nc\n");
    }

    #[test]
    fn undo_restores_a_region_of_another_length_and_shifts_later_regions() {
        let mut d = conflict();
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        d.apply(toggle(2, B)).expect("base");
        assert_eq!(d.region_bytes(RegionIx::new(2)), 2..4);
        d.apply(Command::Undo).expect("undo base");
        d.apply(Command::Undo).expect("undo clear");
        assert_eq!(d.region_bytes(RegionIx::new(2)), UNRESOLVED_END..UNRESOLVED_END + 2);
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
    }

    // Errors

    #[test]
    fn commands_on_missing_regions_or_sources_fail_and_change_nothing() {
        let mut d = conflict();
        assert_eq!(
            d.apply(toggle(3, L)),
            Err(CommandError::NoRegion { region: RegionIx::new(3), regions: 3 })
        );
        assert_eq!(
            d.apply(Command::Clear { region: RegionIx::new(9) }),
            Err(CommandError::NoRegion { region: RegionIx::new(9), regions: 3 })
        );
        for source in [Source::Side(2), Source::Base(1)] {
            assert_eq!(d.apply(toggle(1, source)), Err(CommandError::NoInput { input: source }));
        }
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
        assert_eq!(d.apply(Command::Undo), applied(vec![]));
    }

    #[test]
    fn labels_must_fit_the_merge() {
        let inputs = || three("a\n", "b\n", "c\n");
        let labels = Labels::new(vec!["l".to_owned()], vec!["b".to_owned()]);
        assert_eq!(
            MergeDocument::new(inputs(), MergeOptions::default(), labels).err(),
            Some(LabelCountError { label_sides: 1, label_bases: 1, sides: 2 })
        );
        let d = MergeDocument::new(inputs(), MergeOptions::default(), Labels::none())
            .expect("no labels fit");
        assert_eq!(d.output().into_text(), "<<<<<<<\nb\n|||||||\na\n=======\nc\n>>>>>>>\n");
    }

    // Markers (spec §4.4)

    #[test]
    fn an_empty_label_leaves_its_marker_bare() {
        let inputs = three("b\n", "L\n", "R\n");
        let d =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::three_way("ours", "", ""))
                .expect("three labels fit");
        assert_eq!(d.output().into_text(), "<<<<<<< ours\nL\n|||||||\nb\n=======\nR\n>>>>>>>\n");
    }

    #[test]
    fn an_empty_snapshot_label_falls_back_to_a_number() {
        let abc = "a\nb\n";
        let inputs = Inputs::new(
            vec![text("a\nX\n"), text(abc), text("a\nY\n")],
            vec![text(abc), text(abc)],
        )
        .expect("three sides");
        let labels = Labels::new(
            vec![String::new(), "s1".to_owned(), "s2".to_owned()],
            vec!["b0".to_owned(), String::new()],
        );
        let d = MergeDocument::new(inputs, MergeOptions::default(), labels).expect("labels fit");
        assert_eq!(
            d.output().into_text(),
            "a\n<<<<<<< conflict\n+++++++ side #1\nX\n------- b0\nb\n+++++++ s1\nb\n\
             ------- base #2\nb\n+++++++ s2\nY\n>>>>>>> conflict ends\n"
        );
    }

    #[test]
    fn markers_grow_past_a_marker_like_input_line() {
        let d = doc("a\n", "<<<<<<< x\n", "b\n");
        assert_eq!(d.marker_length(), 8);
        assert_eq!(
            d.output().into_text(),
            "<<<<<<<< left\n<<<<<<< x\n|||||||| base\na\n========\nb\n>>>>>>>> right\n"
        );
    }

    #[test]
    fn marker_length_follows_the_longest_run_in_any_input() {
        assert_eq!(doc("=========\na\n", "=========\nL\n", "=========\nR\n").marker_length(), 10);
        assert_eq!(doc("a\n", "<<<<<<\n", ">>>>>>\n").marker_length(), DEFAULT_MARKER_LENGTH);
        assert_eq!(doc("a\n", "|||||||\n", "b\n").marker_length(), 8);
        // A run counts only at the start of a line, and only of one character.
        assert_eq!(doc("a\n", "x<<<<<<<<\n", "<<<<===\n").marker_length(), DEFAULT_MARKER_LENGTH);
    }

    #[test]
    fn a_longer_requested_length_wins_and_a_shorter_one_still_grows() {
        let with = |marker_length| {
            let options = MergeOptions { marker_length, ..MergeOptions::default() };
            doc_with("a\n", "<<<<<<<\n", "<<<<<\n", options).marker_length()
        };
        assert_eq!(with(9), 9);
        assert_eq!(with(5), 8);
        assert_eq!(with(8), 8);
    }

    fn merge_style() -> MergeOptions {
        MergeOptions { marker_style: MarkerStyle::Merge, ..MergeOptions::default() }
    }

    #[test]
    fn merge_style_drops_the_base_section_and_its_marker() {
        let d = doc_with("a\nb\nc\n", "a\nL\nc\n", "a\nR\nc\n", merge_style());
        assert_eq!(
            d.output(),
            Output::Conflicted("a\n<<<<<<< left\nL\n=======\nR\n>>>>>>> right\nc\n".to_owned())
        );
    }

    #[test]
    fn merge_style_keeps_crlf_markers_and_an_empty_side() {
        let d = doc_with("a\r\nb\r\n", "a\r\n", "a\r\nR\r\n", merge_style());
        assert_eq!(
            d.output().into_text(),
            "a\r\n<<<<<<< left\r\n=======\r\nR\r\n>>>>>>> right\r\n"
        );
    }

    #[test]
    fn merge_style_leaves_a_resolved_merge_alone() {
        let d = doc_with("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n", merge_style());
        assert_eq!(d.output(), Output::Resolved("A\nb\nC\n".to_owned()));
    }

    #[test]
    fn the_default_marker_style_is_diff3() {
        assert_eq!(MergeOptions::default().marker_style, MarkerStyle::Diff3);
    }

    #[test]
    fn more_than_two_sides_keep_jj_snapshot_markers_in_merge_style() {
        let abc = "a\nb\n";
        let inputs = Inputs::new(
            vec![text("a\nX\n"), text(abc), text("a\nY\n")],
            vec![text(abc), text(abc)],
        )
        .expect("three sides");
        let d = MergeDocument::new(inputs, merge_style(), Labels::none()).expect("no labels");
        assert_eq!(
            d.output().into_text(),
            "a\n<<<<<<< conflict\n+++++++ side #1\nX\n------- base #1\nb\n+++++++ side #2\nb\n\
             ------- base #2\nb\n+++++++ side #3\nY\n>>>>>>> conflict ends\n"
        );
    }

    #[test]
    fn crlf_file_gets_crlf_markers() {
        let d = doc("a\r\nb\r\n", "a\r\nL\r\n", "a\r\nR\r\n");
        assert_eq!(
            d.output().into_text(),
            "a\r\n<<<<<<< left\r\nL\r\n||||||| base\r\nb\r\n=======\r\nR\r\n>>>>>>> right\r\n"
        );
    }

    #[test]
    fn a_side_whose_line_before_the_conflict_ends_in_lf_gives_lf_markers() {
        // Only a whitespace mode lets the line before a conflict differ between sides. The
        // unchanged first line also takes right's bytes, as right changed it.
        let options = MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        let d = doc_with("a\r\nb\r\n", "a\r\nL\r\n", "a\nR\r\n", options);
        assert_eq!(
            d.output().into_text(),
            "a\n<<<<<<< left\nL\r\n||||||| base\nb\r\n=======\nR\r\n>>>>>>> right\n"
        );
    }

    #[test]
    fn a_conflict_at_the_first_line_reads_each_sides_first_line() {
        let crlf = doc("b\r\n", "L\r\n", "R\r\n");
        assert_eq!(
            crlf.output().into_text(),
            "<<<<<<< left\r\nL\r\n||||||| base\r\nb\r\n=======\r\nR\r\n>>>>>>> right\r\n"
        );
        let lf = doc("b\r\n", "L\r\n", "R\n");
        assert_eq!(
            lf.output().into_text(),
            "<<<<<<< left\nL\r\n||||||| base\nb\r\n=======\nR\n>>>>>>> right\n"
        );
    }

    #[test]
    fn a_line_without_a_newline_settles_nothing() {
        let d = doc("b\r\n", "L", "R\r\n");
        assert_eq!(
            d.output().into_text(),
            "<<<<<<< left\r\nL\r\n||||||| base\r\nb\r\n=======\r\nR\r\n>>>>>>> right\r\n"
        );
    }

    #[test]
    fn add_add_markers_end_in_lf_because_base_has_no_first_line() {
        let d = doc("", "L\r\n", "R\r\n");
        assert_eq!(
            d.output().into_text(),
            "<<<<<<< left\nL\r\n||||||| base\n=======\nR\r\n>>>>>>> right\n"
        );
    }

    #[test]
    fn a_section_without_a_final_newline_gets_one_in_the_markers_ending() {
        let d = doc("a\nb", "a\nL", "a\nR");
        assert_eq!(
            d.output().into_text(),
            "a\n<<<<<<< left\nL\n||||||| base\nb\n=======\nR\n>>>>>>> right\n"
        );
    }

    #[test]
    fn more_than_two_sides_use_jj_snapshot_markers() {
        let abc = "a\nb\n";
        let inputs = Inputs::new(
            vec![text("a\nX\n"), text(abc), text("a\nY\n")],
            vec![text(abc), text(abc)],
        )
        .expect("three sides");
        let mut d =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
        assert_eq!(
            d.output(),
            Output::Conflicted(
                "a\n<<<<<<< conflict\n+++++++ side #1\nX\n------- base #1\nb\n+++++++ side #2\nb\n\
                 ------- base #2\nb\n+++++++ side #3\nY\n>>>>>>> conflict ends\n"
                    .to_owned()
            )
        );
        d.apply(toggle(1, Source::Side(2))).expect("side 2");
        d.apply(toggle(1, Source::Base(1))).expect("base 1");
        assert_eq!(d.output(), Output::Resolved("a\nY\nb\n".to_owned()));
    }

    #[test]
    fn snapshot_markers_grow_past_their_own_characters_and_diff3_markers_do_not() {
        let ab = "a\nb\n";
        let inputs =
            Inputs::new(vec![text("+++++++\nX\n"), text(ab), text(ab)], vec![text(ab), text(ab)])
                .expect("three sides");
        let d =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
        assert_eq!(d.marker_length(), 8);
        assert_eq!(doc("a\n", "+++++++\n", "-------\n").marker_length(), 7);
    }

    #[test]
    fn snapshot_markers_use_labels_when_given() {
        let abc = "a\nb\n";
        let inputs = Inputs::new(
            vec![text("a\nX\n"), text(abc), text("a\nY\n")],
            vec![text(abc), text(abc)],
        )
        .expect("three sides");
        let labels = Labels::new(
            vec!["s0".to_owned(), "s1".to_owned(), "s2".to_owned()],
            vec!["b0".to_owned(), "b1".to_owned()],
        );
        let d = MergeDocument::new(inputs, MergeOptions::default(), labels).expect("labels fit");
        assert_eq!(
            d.output().into_text(),
            "a\n<<<<<<< conflict\n+++++++ s0\nX\n------- b0\nb\n+++++++ s1\nb\n------- b1\nb\n\
             +++++++ s2\nY\n>>>>>>> conflict ends\n"
        );
    }

    // Whitespace modes (spec §4.2): equality only, never bytes.

    #[test]
    fn a_whitespace_only_change_keeps_the_changed_sides_bytes_line_by_line() {
        let options = MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        // Line 0: left changed. Line 1: right changed. Line 2: both changed, left wins.
        let d = doc_with("a\nb\nc\n", "a \nb\nc \n", "a\nb\t\nc\t\n", options);
        assert_eq!(d.regions().len(), 1);
        assert_eq!(d.output(), Output::Resolved("a \nb\t\nc \n".to_owned()));
    }

    #[test]
    fn jj_mode_keeps_the_changed_sides_bytes_as_git_mode_does() {
        let options = MergeOptions {
            semantics: Semantics::Jj,
            diff: DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        let d = doc_with("a\nb\nc\n", "a \nb\nc \n", "a\nb\t\nc\t\n", options.clone());
        assert_eq!(d.regions().len(), 1);
        assert_eq!(d.output(), Output::Resolved("a \nb\t\nc \n".to_owned()));
        // jj would keep base's `a`; triad takes left's changed bytes, then right's change.
        let d = doc_with("a\nb\n", "a \nb\n", "a\nB\n", options);
        assert_eq!(d.output(), Output::Resolved("a \nB\n".to_owned()));
    }

    #[test]
    fn sides_equal_under_the_mode_take_lefts_bytes() {
        let options = MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        let d = doc_with("a\nb\n", "a\nB \n", "a\nB\n", options);
        assert_eq!(d.regions()[1].kind, Kind::AllSame);
        assert_eq!(d.output(), Output::Resolved("a\nB \n".to_owned()));
    }

    #[test]
    fn a_side_converted_to_lf_merges_cleanly_when_line_endings_are_ignored() {
        let (base, left, right) = ("a\r\nb\r\nc\r\n", "a\nb\nc\n", "a\r\nB\r\nc\r\n");
        let compare = doc_with(base, left, right, MergeOptions::default());
        assert!(
            matches!(compare.output(), Output::Conflicted(_)),
            "left changed every line by its ending"
        );
        for semantics in [Semantics::Git, Semantics::Jj] {
            let options = MergeOptions {
                semantics,
                diff: DiffOptions {
                    line_endings: LineEndingRule::Ignore,
                    ..DiffOptions::default()
                },
                ..MergeOptions::default()
            };
            // Left's endings on the lines only left changed; right's change with its own.
            assert_eq!(
                doc_with(base, left, right, options).output(),
                Output::Resolved("a\nB\r\nc\n".to_owned()),
                "{semantics:?}"
            );
        }
    }

    #[test]
    fn sides_that_differ_only_in_case_agree_when_case_is_ignored_and_left_s_bytes_win() {
        let conflicted = doc_with("a\nb\n", "a\nC\n", "a\nc\n", MergeOptions::default());
        assert!(matches!(conflicted.output(), Output::Conflicted(_)), "C and c differ");
        for semantics in [Semantics::Git, Semantics::Jj] {
            let options = MergeOptions {
                semantics,
                diff: DiffOptions { case: CaseRule::Ignore, ..DiffOptions::default() },
                ..MergeOptions::default()
            };
            let d = doc_with("a\nb\n", "a\nC\n", "a\nc\n", options);
            assert_eq!(d.regions()[1].kind, Kind::AllSame, "{semantics:?}");
            assert_eq!(d.output(), Output::Resolved("a\nC\n".to_owned()), "{semantics:?}");
        }
    }

    #[test]
    fn a_side_that_only_changed_case_merges_cleanly_when_case_is_ignored() {
        let (base, left, right) = ("A\nB\nC\n", "a\nb\nc\n", "A\nX\nC\n");
        let compare = doc_with(base, left, right, MergeOptions::default());
        assert!(matches!(compare.output(), Output::Conflicted(_)), "left changed every line");
        for semantics in [Semantics::Git, Semantics::Jj] {
            let options = MergeOptions {
                semantics,
                diff: DiffOptions { case: CaseRule::Ignore, ..DiffOptions::default() },
                ..MergeOptions::default()
            };
            // Left's case on the lines only left changed; right's change on the line it changed.
            assert_eq!(
                doc_with(base, left, right, options).output(),
                Output::Resolved("a\nX\nc\n".to_owned()),
                "{semantics:?}"
            );
        }
    }

    #[test]
    fn sides_that_differ_only_in_pattern_matches_agree_and_left_s_bytes_win() {
        let (base, left, right) = ("v1 // a\nb\n", "v1 // L\nb\n", "v1 // R\nB\n");
        let conflicted = doc_with(base, left, right, MergeOptions::default());
        assert!(matches!(conflicted.output(), Output::Conflicted(_)), "the comments differ");
        for semantics in [Semantics::Git, Semantics::Jj] {
            let options = MergeOptions {
                semantics,
                diff: DiffOptions {
                    patterns: crate::pattern::Patterns::new(["//.*"]).expect("valid"),
                    ..DiffOptions::default()
                },
                ..MergeOptions::default()
            };
            // Line 1 compares equal on every side: left's changed bytes. Line 2: right's change.
            assert_eq!(
                doc_with(base, left, right, options).output(),
                Output::Resolved("v1 // L\nB\n".to_owned()),
                "{semantics:?}"
            );
        }
    }

    // Offsets

    #[test]
    fn region_at_byte_skips_empty_regions_and_ends_on_the_last() {
        let mut d = conflict();
        let at = |d: &MergeDocument, offsets: &[usize]| -> Vec<usize> {
            offsets.iter().map(|&o| d.region_at_byte(ByteOffset::new(o)).get()).collect()
        };
        assert_eq!(at(&d, &[0, 1, 2, 5, 6, 7, 8]), [0, 0, 1, 1, 2, 2, 2]);
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        assert_eq!(at(&d, &[0, 1, 2, 3, 4]), [0, 0, 2, 2, 2]);
    }

    #[test]
    #[should_panic(expected = "byte 9 is past the end of the result (8 bytes)")]
    fn region_at_byte_past_the_end_panics() {
        let _ = conflict().region_at_byte(ByteOffset::new(UNRESOLVED_END + 3));
    }

    #[test]
    fn line_of_byte_counts_only_lf() {
        let d = doc("a\rb\nc\n", "a\rb\nc\n", "a\rb\nc\n");
        let lines: Vec<usize> = (0..=6).map(|o| d.line_of_byte(ByteOffset::new(o)).get()).collect();
        assert_eq!(lines, [0, 0, 0, 0, 1, 1, 2]);
    }

    #[test]
    fn typed_ending_comes_from_base() {
        assert_eq!(doc("a\r\n", "a\n", "a\n").typed_ending(), LineEnding::CrLf);
        assert_eq!(doc("", "a\r\n", "b\r\n").typed_ending(), LineEnding::CrLf);
        assert_eq!(doc("a", "b", "c").typed_ending(), LineEnding::Lf);
    }

    // Labels

    #[test]
    fn labels_give_only_non_empty_names_in_range() {
        let l = Labels::new(vec!["L".to_owned(), String::new()], vec!["B".to_owned()]);
        assert_eq!(
            [l.side(0), l.side(1), l.side(2)],
            [Some("L"), None, None],
            "named, empty, out of range"
        );
        assert_eq!([l.base(0), l.base(1)], [Some("B"), None]);
        let none = Labels::none();
        assert_eq!([none.side(0), none.base(0)], [None, None]);
    }

    // Edits (spec §4.5)

    fn edit(patches: &[(Range<usize>, &str)]) -> Command {
        Command::Edit(EditBatch(patches.iter().map(|(r, t)| patch(r.clone(), t)).collect()))
    }

    fn insert(at: usize, text: &str) -> Command {
        edit(&[(at..at, text)])
    }

    #[expect(clippy::unnecessary_wraps, reason = "compared with `apply`'s result")]
    fn restructured(patches: Vec<TextPatch>) -> Result<Applied, CommandError> {
        Ok(Applied { patches, regions_changed: true })
    }

    fn bytes(d: &MergeDocument) -> Vec<Range<usize>> {
        d.region_ixs().map(|r| d.region_bytes(r)).collect()
    }

    /// Regions `A\n` (left changed), `b\n` (unchanged) and `C\n` (right changed).
    fn three_lines() -> MergeDocument {
        doc("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n")
    }

    const AUTO: Resolution = Resolution::Auto;
    const EDITED: Resolution = Resolution::Edited;

    #[test]
    fn an_edit_inside_one_region_makes_it_edited() {
        let mut d = three_lines();
        assert_eq!(d.apply(insert(3, "x")), applied(vec![]));
        assert_eq!(d.output(), Output::Resolved("A\nbx\nC\n".to_owned()));
        assert_eq!(d.resolutions(), [AUTO, EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..5, 5..7]);
        assert_eq!(d.apply(edit(&[(2..3, "yy")])), applied(vec![]));
        assert_eq!(text_of(&d), "A\nyyx\nC\n");
        assert_eq!(d.regions(), three_lines().regions());
    }

    #[test]
    fn an_edit_across_a_boundary_merges_the_regions_it_touches() {
        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(1..3, "")])), restructured(vec![]));
        assert_eq!(d.output(), Output::Resolved("A\nC\n".to_owned()));
        assert_eq!(
            d.regions(),
            [
                Region { kind: Kind::OneSide(LEFT), bases: vec![0..2], sides: vec![0..2, 0..2] },
                three_lines().regions()[2].clone(),
            ]
        );
        assert_eq!(d.resolutions(), [EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..4]);
    }

    #[test]
    fn a_merge_that_takes_in_a_conflict_is_a_conflict() {
        let mut d = conflict();
        assert_eq!(d.apply(edit(&[(1..3, "")])), restructured(vec![]));
        assert_eq!(d.output(), Output::Resolved("a\nR\nc\n".to_owned()));
        assert_eq!(
            d.regions()[0],
            Region { kind: Kind::Conflict, bases: vec![0..2], sides: vec![0..2, 0..2] }
        );
        assert_eq!(d.resolutions(), [EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..UNRESOLVED_END - 2, UNRESOLVED_END - 2..UNRESOLVED_END]);
    }

    #[test]
    fn patches_that_share_a_region_merge_every_region_they_touch() {
        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(1..3, ""), (3..5, "")])), restructured(vec![]));
        assert_eq!(d.output(), Output::Resolved("A\n".to_owned()));
        assert_eq!(d.resolutions(), [EDITED]);
        assert_eq!(bytes(&d), [0..2]);

        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(1..3, ""), (4..4, "z")])), restructured(vec![]));
        assert_eq!(text_of(&d), "A\nzC\n");
        assert_eq!(d.resolutions(), [EDITED, EDITED]);
        assert_eq!(bytes(&d), [0..2, 2..5]);
    }

    #[test]
    fn an_insertion_at_a_line_start_boundary_goes_to_the_region_after_it() {
        let cases = [
            (0, "xA\nb\nC\n", [EDITED, AUTO, AUTO]),
            (2, "A\nxb\nC\n", [AUTO, EDITED, AUTO]),
            (6, "A\nb\nC\nx", [AUTO, AUTO, EDITED]),
        ];
        for (at, text, resolutions) in cases {
            let mut d = three_lines();
            assert_eq!(d.apply(insert(at, "x")), applied(vec![]), "at {at}");
            assert_eq!(text_of(&d), text);
            assert_eq!(d.resolutions(), resolutions, "at {at}");
        }
    }

    #[test]
    fn typing_into_an_empty_region_between_two_others_stays_in_it() {
        let mut d = conflict();
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        d.apply(insert(2, "x")).expect("x");
        assert_eq!(d.resolutions(), [AUTO, EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..3, 3..5]);
        // Mid-line, the text joins the region the line started in.
        d.apply(insert(3, "y")).expect("y");
        d.apply(insert(4, "\n")).expect("newline");
        assert_eq!(text_of(&d), "a\nxy\nc\n");
        assert_eq!(bytes(&d), [0..2, 2..5, 5..7]);
        // After the newline, the boundary is at a line start again.
        d.apply(insert(5, "z")).expect("z");
        assert_eq!(d.resolutions(), [AUTO, EDITED, EDITED]);
        assert_eq!(bytes(&d), [0..2, 2..5, 5..8]);
    }

    #[test]
    fn an_insertion_goes_to_the_first_of_several_empty_regions() {
        let mut d = conflict();
        d.apply(Command::Clear { region: RegionIx::new(0) }).expect("clear 0");
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear 1");
        d.apply(insert(0, "x")).expect("x");
        assert_eq!(d.resolutions(), [EDITED, Resolution::Pick(vec![]), AUTO]);
        assert_eq!(bytes(&d), [0..1, 1..1, 1..3]);
    }

    #[test]
    fn an_insertion_at_a_line_start_inside_a_region_stays_in_it() {
        let mut d = conflict();
        d.apply(toggle(1, L)).expect("left");
        d.apply(toggle(1, R)).expect("right");
        d.apply(Command::Clear { region: RegionIx::new(2) }).expect("clear");
        // The empty region 2 sits at 6, past the insertion.
        d.apply(insert(4, "x")).expect("x");
        assert_eq!(text_of(&d), "a\nL\nxR\n");
        assert_eq!(d.resolutions(), [AUTO, EDITED, Resolution::Pick(vec![])]);
        assert_eq!(bytes(&d), [0..2, 2..7, 7..7]);
    }

    #[test]
    fn an_empty_last_region_takes_text_typed_at_the_end() {
        let mut d = doc("a\n", "a\nL\n", "a\nR\n");
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        d.apply(insert(2, "x")).expect("x");
        assert_eq!(d.resolutions(), [AUTO, EDITED]);
        assert_eq!(bytes(&d), [0..2, 2..3]);
    }

    #[test]
    fn an_insertion_at_a_mid_line_boundary_joins_the_region_before() {
        let mut d = three_lines();
        d.apply(edit(&[(1..2, "")])).expect("join lines");
        assert_eq!(text_of(&d), "Ab\nC\n");
        d.apply(insert(1, "z")).expect("z");
        assert_eq!(text_of(&d), "Azb\nC\n");
        assert_eq!(d.resolutions(), [EDITED, AUTO, AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..4, 4..6]);
    }

    #[test]
    fn typing_into_an_unresolved_conflict_edits_the_text_it_shows() {
        let mut d = conflict();
        assert_eq!(d.apply(insert(3, "x")), applied(vec![]));
        assert_eq!(d.output(), Output::Resolved("a\nLx\nR\nc\n".to_owned()));
        assert_eq!(d.resolutions(), [AUTO, EDITED, AUTO]);
        assert_eq!(d.region_bytes(RegionIx::new(1)), 2..UNRESOLVED_END + 1);
        d.apply(Command::Undo).expect("undo");
        assert_eq!(d.resolutions()[1], Resolution::Unresolved);
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
    }

    #[test]
    fn deleting_a_whole_region_leaves_an_empty_edited_region() {
        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(2..4, "")])), applied(vec![]));
        assert_eq!(d.resolutions(), [AUTO, EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..2, 2..4]);
        d.apply(insert(2, "q")).expect("q");
        assert_eq!(text_of(&d), "A\nqC\n");
        assert_eq!(bytes(&d), [0..2, 2..3, 3..5]);
    }

    #[test]
    fn a_paste_over_a_whole_region_edits_only_that_region() {
        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(2..4, "p\nq\n")])), applied(vec![]));
        assert_eq!(text_of(&d), "A\np\nq\nC\n");
        assert_eq!(d.resolutions(), [AUTO, EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..6, 6..8]);
    }

    /// `a`, then left's `L` picked, in a CRLF file.
    fn crlf_picked() -> MergeDocument {
        let mut d = doc("a\r\nb\r\n", "a\r\nL\r\n", "a\r\nR\r\n");
        d.apply(toggle(1, L)).expect("left");
        d
    }

    #[test]
    fn a_newline_typed_into_a_crlf_file_gets_a_carriage_return() {
        let mut d = crlf_picked();
        assert_eq!(d.apply(insert(4, "\n")), applied(vec![patch(4..4, "\r")]));
        assert_eq!(text_of(&d), "a\r\nL\r\n\r\n");
        assert_eq!(d.region_bytes(RegionIx::new(1)), 3..8);
        assert_eq!(d.apply(Command::Undo), applied(vec![patch(4..6, "")]));
        assert_eq!(text_of(&d), "a\r\nL\r\n");
        assert_eq!(d.apply(Command::Redo), applied(vec![patch(4..4, "\r\n")]));
        assert_eq!(text_of(&d), "a\r\nL\r\n\r\n");
    }

    #[test]
    fn a_paste_into_a_crlf_file_gets_crlf_endings() {
        let mut d = crlf_picked();
        assert_eq!(
            d.apply(insert(3, "x\ny\n")),
            applied(vec![patch(4..4, "\r"), patch(6..6, "\r")])
        );
        assert_eq!(text_of(&d), "a\r\nx\r\ny\r\nL\r\n");
        assert_eq!(d.apply(insert(3, "u\r\n")), applied(vec![]));
        assert_eq!(text_of(&d), "a\r\nu\r\nx\r\ny\r\nL\r\n");
    }

    #[test]
    fn a_paste_into_an_lf_file_gets_lf_endings_and_keeps_a_lone_cr() {
        let mut d = conflict();
        assert_eq!(d.apply(insert(2, "x\r\ny\r")), applied(vec![patch(3..4, "")]));
        assert_eq!(text_of(&d), format!("a\nx\ny\r{UNRESOLVED}c\n"));
        assert_eq!(d.region_bytes(RegionIx::new(1)), 2..UNRESOLVED_END + 4);
    }

    #[test]
    fn undoing_a_delete_across_three_regions_restores_them() {
        let mut d = three_lines();
        d.apply(toggle(1, R)).expect("right");
        assert_eq!(d.apply(edit(&[(1..5, "")])), restructured(vec![]));
        assert_eq!(d.output(), Output::Resolved("A\n".to_owned()));
        assert_eq!(
            d.regions(),
            [Region { kind: Kind::OneSide(LEFT), bases: vec![0..3], sides: vec![0..3, 0..3] }]
        );
        assert_eq!(d.resolutions(), [EDITED]);
        assert_eq!(d.apply(Command::Undo), restructured(vec![patch(1..1, "\nb\nC")]));
        assert_eq!(d.regions(), three_lines().regions());
        assert_eq!(d.resolutions(), [AUTO, Resolution::Pick(vec![R]), AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..4, 4..6]);
        assert_eq!(d.apply(Command::Redo), restructured(vec![patch(1..5, "")]));
        assert_eq!(d.resolutions(), [EDITED]);
        assert_eq!(bytes(&d), [0..2]);
    }

    #[test]
    fn a_pick_replaces_an_edit_and_undo_restores_it() {
        let mut d = conflict();
        let edited = "Lx\nR\n";
        d.apply(insert(3, "x")).expect("x");
        assert_eq!(d.apply(toggle(1, L)), applied(vec![patch(2..UNRESOLVED_END + 1, "L\n")]));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![L]));
        assert_eq!(d.apply(Command::Undo), applied(vec![patch(2..4, edited)]));
        assert_eq!(d.resolutions()[1], EDITED);
    }

    #[test]
    fn picks_and_edits_share_one_history() {
        let mut d = conflict();
        d.apply(toggle(1, L)).expect("left");
        d.apply(insert(3, "!")).expect("!");
        assert_eq!(d.apply(toggle(1, R)), applied(vec![patch(2..5, "R\n")]));
        let steps = [
            (Command::Undo, patch(2..4, "L!\n"), EDITED),
            (Command::Undo, patch(3..4, ""), Resolution::Pick(vec![L])),
            (Command::Undo, patch(2..4, UNRESOLVED), Resolution::Unresolved),
            (Command::Redo, patch(2..UNRESOLVED_END, "L\n"), Resolution::Pick(vec![L])),
            (Command::Redo, patch(3..3, "!"), EDITED),
            (Command::Redo, patch(2..5, "R\n"), Resolution::Pick(vec![R])),
        ];
        for (n, (command, patch, resolution)) in steps.into_iter().enumerate() {
            assert_eq!(d.apply(command), applied(vec![patch]), "step {n}");
            assert_eq!(d.resolutions()[1], resolution, "step {n}");
        }
        assert_eq!(text_of(&d), "a\nR\nc\n");
    }

    #[test]
    fn an_edit_drops_the_redo_history() {
        let mut d = conflict();
        d.apply(toggle(1, L)).expect("left");
        d.apply(Command::Undo).expect("undo");
        d.apply(insert(2, "x")).expect("x");
        assert_eq!(d.apply(Command::Redo), applied(vec![]));
        assert_eq!(text_of(&d), format!("a\nx{UNRESOLVED}c\n"));
    }

    #[test]
    fn empty_batches_and_empty_patches_change_nothing_and_record_nothing() {
        let mut d = conflict();
        d.apply(toggle(1, L)).expect("left");
        assert_eq!(d.apply(edit(&[])), applied(vec![]));
        assert_eq!(d.apply(edit(&[(3..3, "")])), applied(vec![]));
        assert_eq!(d.resolutions()[1], Resolution::Pick(vec![L]));
        assert_eq!(d.apply(Command::Undo), applied(vec![patch(2..4, UNRESOLVED)]));
    }

    // Undo grouping

    fn undo_all(d: &mut MergeDocument) -> Vec<Vec<TextPatch>> {
        std::iter::from_fn(|| {
            let applied = d.apply(Command::Undo).expect("undo");
            (!applied.patches.is_empty()).then_some(applied.patches)
        })
        .collect()
    }

    #[test]
    fn typing_abc_is_one_undo_step() {
        let mut d = conflict();
        for (at, c) in [(2, "a"), (3, "b"), (4, "c")] {
            d.apply(insert(at, c)).expect("typed");
        }
        assert_eq!(undo_all(&mut d), [vec![patch(2..5, "")]]);
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
        assert_eq!(d.resolutions()[1], Resolution::Unresolved);
    }

    #[test]
    fn a_newline_breaks_typing_into_three_undo_steps() {
        let mut d = conflict();
        for (at, c) in [(2, "a"), (3, "b"), (4, "\n"), (5, "c")] {
            d.apply(insert(at, c)).expect("typed");
        }
        assert_eq!(
            undo_all(&mut d),
            [vec![patch(5..6, "")], vec![patch(4..5, "")], vec![patch(2..4, "")]]
        );
    }

    #[test]
    fn a_paste_is_one_undo_step() {
        let mut d = conflict();
        d.apply(insert(2, "x\ny\n")).expect("paste");
        assert_eq!(undo_all(&mut d), [vec![patch(2..6, "")]]);
    }

    #[test]
    fn typing_away_from_the_caret_starts_a_new_step() {
        let mut d = conflict();
        d.apply(insert(2, "a")).expect("a");
        d.apply(insert(4, "b")).expect("b");
        assert_eq!(undo_all(&mut d), [vec![patch(4..5, "")], vec![patch(2..3, "")]]);
    }

    #[test]
    fn typing_after_another_command_starts_a_new_step() {
        let mut d = conflict();
        d.apply(insert(2, "a")).expect("a");
        d.apply(toggle(2, B)).expect("base");
        d.apply(Command::Undo).expect("undo base");
        d.apply(insert(3, "b")).expect("b");
        assert_eq!(undo_all(&mut d), [vec![patch(3..4, "")], vec![patch(2..3, "")]]);
    }

    #[test]
    fn typing_after_undo_and_redo_starts_a_new_step() {
        let mut d = conflict();
        d.apply(insert(2, "a")).expect("a");
        d.apply(Command::Undo).expect("undo");
        d.apply(Command::Redo).expect("redo");
        d.apply(insert(3, "b")).expect("b");
        assert_eq!(undo_all(&mut d), [vec![patch(3..4, "")], vec![patch(2..3, "")]]);
    }

    #[test]
    fn two_insertions_in_one_region_lengthen_it_by_both() {
        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(2..2, "x"), (3..3, "yz")])), applied(vec![]));
        assert_eq!(text_of(&d), "A\nxbyz\nC\n");
        assert_eq!(d.resolutions(), [AUTO, EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..2, 2..7, 7..9]);
    }

    #[test]
    fn a_merged_run_is_as_long_as_every_patch_in_it_leaves() {
        // The first patch joins regions 0 and 1; the second inserts into region 1.
        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(1..3, "q"), (3..3, "yz")])), restructured(vec![]));
        assert_eq!(text_of(&d), "Aqyz\nC\n");
        assert_eq!(d.resolutions(), [EDITED, AUTO]);
        assert_eq!(bytes(&d), [0..5, 5..7]);
    }

    #[test]
    fn a_three_cursor_batch_is_one_undo_step() {
        let mut d = three_lines();
        assert_eq!(d.apply(edit(&[(0..0, "x"), (2..2, "x"), (4..4, "x")])), applied(vec![]));
        assert_eq!(text_of(&d), "xA\nxb\nxC\n");
        assert_eq!(d.resolutions(), [EDITED, EDITED, EDITED]);
        assert_eq!(undo_all(&mut d), [vec![patch(0..1, ""), patch(3..4, ""), patch(6..7, "")]]);
        assert_eq!(d.output(), Output::Resolved("A\nb\nC\n".to_owned()));
        assert_eq!(d.resolutions(), [AUTO, AUTO, AUTO]);
    }

    // Edit errors

    #[test]
    fn malformed_batches_fail_and_change_nothing() {
        let mut d = conflict();
        let (start, end) = (3, 2);
        let cases = [
            (
                edit(&[(0..2, ""), (1..3, "")]),
                CommandError::Edit(BatchError::OutOfOrder { index: 1 }),
            ),
            (
                edit(&[(4..4, "x"), (2..2, "y")]),
                CommandError::Edit(BatchError::OutOfOrder { index: 1 }),
            ),
            (
                edit(&[(0..0, "x"), (57..59, "")]),
                CommandError::Edit(BatchError::BadRange { range: 57..59, len: UNRESOLVED_END + 2 }),
            ),
            (
                edit(&[(start..end, "")]),
                CommandError::Edit(BatchError::BadRange {
                    range: start..end,
                    len: UNRESOLVED_END + 2,
                }),
            ),
        ];
        for (command, error) in cases {
            assert_eq!(d.apply(command), Err(error));
        }
        assert_eq!(d.output(), Output::Conflicted(format!("a\n{MARKERS}c\n")));
        assert_eq!(d.apply(Command::Undo), applied(vec![]));
    }

    #[test]
    fn an_edit_inside_a_character_fails() {
        let mut d = doc("é\n", "é\nL\n", "é\nR\n");
        let before = text_of(&d);
        assert_eq!(
            d.apply(insert(1, "x")),
            Err(CommandError::Edit(BatchError::NotCharBoundary { offset: 1 }))
        );
        assert_eq!(
            d.apply(edit(&[(0..1, "")])),
            Err(CommandError::Edit(BatchError::NotCharBoundary { offset: 1 }))
        );
        assert_eq!(text_of(&d), before);
    }

    #[test]
    fn offsets_follow_a_sequence_of_edits() {
        let mut d = three_lines();
        d.apply(insert(2, "x\ry")).expect("lone CR stays");
        d.apply(edit(&[(0..2, "")])).expect("empty region 0");
        assert_eq!(text_of(&d), "x\ryb\nC\n");
        assert_eq!(bytes(&d), [0..0, 0..5, 5..7]);
        let at: Vec<usize> = (0..=7).map(|o| d.region_at_byte(ByteOffset::new(o)).get()).collect();
        assert_eq!(at, [1, 1, 1, 1, 1, 2, 2, 2]);
        let lines: Vec<usize> = (0..=7).map(|o| d.line_of_byte(ByteOffset::new(o)).get()).collect();
        assert_eq!(lines, [0, 0, 0, 0, 0, 1, 1, 2]);
        d.apply(insert(0, "w")).expect("into empty region 0");
        assert_eq!(bytes(&d), [0..1, 1..6, 6..8]);
    }

    // Syntax-aware boundaries (docs/DESIGN.md §B5.8). Left's shared m is at
    // bytes 4..6.
    const DEEP_LEFT: &str = "a\nX\nm\nY\nz\n";

    fn deep_inputs() -> Inputs {
        three("a\np\nq\nz\n", DEEP_LEFT, "a\nZ\nm\nW\nz\n")
    }

    fn region(kind: Kind, base: Range<usize>, left: Range<usize>, right: Range<usize>) -> Region {
        Region { kind, bases: vec![base], sides: vec![left, right] }
    }

    #[test]
    fn syntax_boundaries_apply_only_with_the_option_on() {
        let marked = Marked(vec![(DEEP_LEFT, node(3..5))]);
        let on = MergeOptions {
            refine: Refine::Deep,
            syntax_boundaries: true,
            ..MergeOptions::default()
        };
        let off = MergeOptions { syntax_boundaries: false, ..on.clone() };
        let split = [
            region(Kind::Unchanged, 0..1, 0..1, 0..1),
            region(Kind::Conflict, 1..1, 1..2, 1..2),
            region(Kind::AllSame, 1..1, 2..3, 2..3),
            region(Kind::Conflict, 1..3, 3..4, 3..4),
            region(Kind::Unchanged, 3..4, 4..5, 4..5),
        ];
        let whole = [
            region(Kind::Unchanged, 0..1, 0..1, 0..1),
            region(Kind::Conflict, 1..3, 1..4, 1..4),
            region(Kind::Unchanged, 3..4, 4..5, 4..5),
        ];
        let open = |options, boundaries: Option<&dyn Boundaries>| {
            MergeDocument::new_bounded(deep_inputs(), options, Labels::none(), boundaries)
                .expect("no labels")
                .regions()
                .to_vec()
        };
        assert_eq!(open(on.clone(), None), split, "no grammar");
        assert_eq!(open(off, Some(&marked)), split, "the option off");
        assert_eq!(open(on, Some(&marked)), whole);
    }

    // Output line endings (`docs/DESIGN.md` §E5).

    const ENDINGS: [OutputEnding; 4] =
        [OutputEnding::Keep, OutputEnding::Base, OutputEnding::Lf, OutputEnding::CrLf];

    fn saved(d: &MergeDocument, ending: OutputEnding) -> String {
        d.output_with(ending).into_text()
    }

    /// Base in CRLF; left changes `b` to `B` with LF; right keeps base.
    fn crlf_base_lf_edit() -> MergeDocument {
        doc("a\r\nb\r\nc\r\n", "a\r\nB\nc\r\n", "a\r\nb\r\nc\r\n")
    }

    #[test]
    fn keep_takes_each_regions_bytes_and_the_others_make_one_ending() {
        let d = crlf_base_lf_edit();
        assert_eq!(saved(&d, OutputEnding::Keep), "a\r\nB\nc\r\n");
        assert_eq!(saved(&d, OutputEnding::Base), "a\r\nB\r\nc\r\n");
        assert_eq!(saved(&d, OutputEnding::Lf), "a\nB\nc\n");
        assert_eq!(saved(&d, OutputEnding::CrLf), "a\r\nB\r\nc\r\n");
        assert!(d.output_with(OutputEnding::Lf).is_resolved());
    }

    #[test]
    fn markers_and_sections_of_a_conflict_take_the_ending_too() {
        let output = conflict().output_with(OutputEnding::CrLf);
        assert_eq!(
            output,
            Output::Conflicted(
                "a\r\n<<<<<<< left\r\nL\r\n||||||| base\r\nb\r\n=======\r\nR\r\n\
                 >>>>>>> right\r\nc\r\n"
                    .to_owned()
            )
        );
    }

    #[test]
    fn a_last_line_without_a_newline_stays_without_one() {
        let d = doc("a\nb", "a\nb", "a\nb");
        for ending in ENDINGS {
            assert!(saved(&d, ending).ends_with('b'), "{ending:?}");
        }
        assert_eq!(saved(&d, OutputEnding::CrLf), "a\r\nb");
    }

    #[test]
    fn a_lone_cr_stays() {
        let d = doc("a\rb\n", "a\rb\n", "a\rb\n");
        assert_eq!(saved(&d, OutputEnding::Lf), "a\rb\n");
        assert_eq!(saved(&d, OutputEnding::CrLf), "a\rb\r\n");
    }

    #[test]
    fn an_empty_output_is_empty_under_every_ending() {
        let d = doc("", "", "");
        for ending in ENDINGS {
            assert_eq!(saved(&d, ending), "", "{ending:?}");
        }
    }

    #[test]
    fn base_without_line_breaks_takes_lefts_ending_then_lf() {
        // Base is empty; both sides add the same lines, two CRLF and one LF, so left's
        // dominant ending is CRLF.
        let added = "x\r\nz\r\ny\n";
        let d = doc("", added, added);
        assert_eq!(saved(&d, OutputEnding::Keep), added);
        assert_eq!(saved(&d, OutputEnding::Base), "x\r\nz\r\ny\r\n");
        // Neither base nor left breaks a line; right adds CRLF lines.
        let d = doc("a", "a", "a\r\nb\r\n");
        assert_eq!(saved(&d, OutputEnding::Keep), "a\r\nb\r\n");
        assert_eq!(saved(&d, OutputEnding::Base), "a\nb\n");
    }

    #[test]
    fn the_kept_outputs_endings_follow_picks_and_undo() {
        let mut d = crlf_base_lf_edit();
        assert_eq!(d.output_endings(), LineEndings::Mixed);
        d.apply(pick(1, &[Source::Base(0)])).expect("picks base");
        assert_eq!(d.output_endings(), LineEndings::CrLf);
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(d.output_endings(), LineEndings::Mixed);
        assert_eq!(doc("", "", "").output_endings(), LineEndings::None);
        assert_eq!(conflict().output_endings(), LineEndings::Lf);
    }

    #[test]
    fn an_edit_that_drops_the_cr_of_a_crlf_line_makes_it_lf() {
        let mut d = doc("a\r\nb\r\n", "a\r\nb\r\n", "a\r\nb\r\n");
        let drop = TextPatch { range: 1..2, text: String::new() };
        d.apply(Command::Edit(EditBatch(vec![drop]))).expect("edits");
        assert_eq!(d.text(), "a\nb\r\n");
        assert_eq!(d.output_endings(), LineEndings::Mixed);
        // Typing a CR back before the newline pairs them again.
        let cr = TextPatch { range: 1..1, text: "\r".to_owned() };
        d.apply(Command::Edit(EditBatch(vec![cr]))).expect("edits");
        assert_eq!(d.text(), "a\r\nb\r\n");
        assert_eq!(d.output_endings(), LineEndings::CrLf);
        // Deleting the newline of a CRLF line leaves its CR in the line.
        let newline = TextPatch { range: 2..3, text: String::new() };
        d.apply(Command::Edit(EditBatch(vec![newline]))).expect("edits");
        assert_eq!(d.text(), "a\rb\r\n");
        assert_eq!(d.output_endings(), LineEndings::CrLf);
    }

    #[test]
    fn an_unresolved_conflict_counts_its_base_section_and_markers() {
        // The result shows L and R in LF; the saved base section is CRLF.
        let mut d = doc("a\nb\r\nc\n", "a\nL\nc\n", "a\nR\nc\n");
        assert_eq!(d.text(), "a\nL\nR\nc\n");
        assert_eq!(d.output_endings(), LineEndings::Mixed);
        d.apply(toggle(1, Source::Side(0))).expect("picks left");
        assert_eq!(d.output_endings(), LineEndings::Lf);
        // The merge marker style saves no base section.
        let merge_style =
            MergeOptions { marker_style: MarkerStyle::Merge, ..MergeOptions::default() };
        let d = doc_with("a\nb\r\nc\n", "a\nL\nc\n", "a\nR\nc\n", merge_style.clone());
        assert_eq!(d.output_endings(), LineEndings::Lf);
        // Left deletes the line, right's line has no newline: the result breaks no line, so
        // the markers alone hold an ending, base's first one.
        let crlf = doc_with("b\r\n", "", "R", merge_style.clone());
        assert_eq!((crlf.text(), crlf.output_endings()), ("R".to_owned(), LineEndings::CrLf));
        let lf = doc_with("b\n", "", "R", merge_style);
        assert_eq!(lf.output_endings(), LineEndings::Lf);
        // An LF base section in a CRLF file.
        let d = doc("a\r\nb\nc\r\n", "a\r\nL\r\nc\r\n", "a\r\nR\r\nc\r\n");
        assert_eq!(d.output_endings(), LineEndings::Mixed);
        // A conflict at the start: the markers are LF, as left's first line is.
        let d = doc("b\r\n", "L\n", "R\n");
        assert_eq!(d.text(), "L\nR\n");
        assert_eq!(d.output_endings(), LineEndings::Mixed);
    }

    #[test]
    fn saved_endings_are_the_converted_outputs() {
        let d = crlf_base_lf_edit();
        let expected = [
            (OutputEnding::Keep, LineEndings::Mixed),
            (OutputEnding::Base, LineEndings::CrLf),
            (OutputEnding::Lf, LineEndings::Lf),
            (OutputEnding::CrLf, LineEndings::CrLf),
        ];
        for (ending, endings) in expected {
            assert_eq!(d.saved_endings(ending), endings, "{ending:?}");
        }
        let unbroken = doc("a", "a", "a");
        for ending in ENDINGS {
            assert_eq!(unbroken.saved_endings(ending), LineEndings::None, "{ending:?}");
        }
    }

    // Region lines

    /// Every region's [`MergeDocument::region_lines`], as plain line numbers.
    fn region_lines_of(d: &MergeDocument) -> Vec<Range<usize>> {
        d.region_ixs()
            .map(|r| {
                let lines = d.region_lines(r);
                lines.start.get()..lines.end.get()
            })
            .collect()
    }

    /// The lines of each region by looking up its bytes in the text: the oracle for
    /// [`MergeDocument::region_lines`].
    fn region_lines_by_lookup(d: &MergeDocument) -> Vec<Range<usize>> {
        d.region_ixs()
            .map(|r| {
                let bytes = d.region_bytes(r);
                let line_of = |byte: usize| d.line_of_byte(ByteOffset::new(byte)).get();
                let start = line_of(bytes.start);
                if bytes.is_empty() { start..start } else { start..line_of(bytes.end - 1) + 1 }
            })
            .collect()
    }

    #[test]
    fn region_lines_follow_picks_clears_and_undo() {
        let mut d = conflict();
        // `a\n`, then `L\nR\n` unresolved, then `c\n`.
        assert_eq!(region_lines_of(&d), [0..1, 1..3, 3..4]);
        d.apply(pick(1, &[L])).expect("picks");
        assert_eq!(region_lines_of(&d), [0..1, 1..2, 2..3]);
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clears");
        // An empty region has no lines, at the line it would start on.
        assert_eq!(region_lines_of(&d), [0..1, 1..1, 1..2]);
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(region_lines_of(&d), [0..1, 1..2, 2..3]);
        d.apply(Command::Redo).expect("redoes");
        assert_eq!(region_lines_of(&d), [0..1, 1..1, 1..2]);
    }

    #[test]
    fn region_lines_follow_edits() {
        let mut d = conflict();
        // `a\n` becomes `a\nx\n`: region 0 gains a line and the rest move down one.
        d.apply(edit(&[(1..1, "\nx")])).expect("edits");
        assert_eq!(region_lines_of(&d), [0..2, 2..4, 4..5]);
        // Deleting `c`'s newline leaves an unterminated last line, which counts.
        let end = text_of(&d).len();
        d.apply(edit(&[(end - 1..end, "")])).expect("edits");
        assert_eq!(text_of(&d), "a\nx\nL\nR\nc");
        assert_eq!(region_lines_of(&d), [0..2, 2..4, 4..5]);
        // `a\nx\nL\n` becomes `z`: regions 0 and 1 merge into `zR\n`, on line 0.
        d.apply(edit(&[(0..6, "z")])).expect("edits");
        assert_eq!(text_of(&d), "zR\nc");
        assert_eq!(region_lines_of(&d), [0..1, 1..2]);
        d.apply(Command::Undo).expect("undoes");
        assert_eq!(region_lines_of(&d), [0..2, 2..4, 4..5]);
    }

    #[test]
    fn region_lines_match_a_lookup_of_their_bytes_through_every_command() {
        const LINES: usize = 4000;
        const SEED: u64 = 7;
        let t = crate::testing::generate(LINES, SEED);
        let mut d = doc(&t.base, &t.left, &t.right);
        let check = |d: &MergeDocument, step: &str| {
            assert_eq!(region_lines_of(d), region_lines_by_lookup(d), "after {step}");
        };
        check(&d, "loading");
        let conflicts: Vec<usize> = d
            .region_ixs()
            .filter(|&r| d.region(r).kind == Kind::Conflict)
            .map(RegionIx::get)
            .collect();
        assert!(conflicts.len() >= 2, "the seed gives conflicts to pick");
        d.apply(pick(conflicts[0], &[L, R])).expect("picks");
        check(&d, "a pick of two sources");
        d.apply(Command::Clear { region: RegionIx::new(conflicts[1]) }).expect("clears");
        check(&d, "a clear");
        // Typing a newline inside a line, a deletion across several regions, and an
        // insertion at the very end.
        d.apply(edit(&[(5..5, "\n")])).expect("edits");
        check(&d, "a newline mid-line");
        let len = text_of(&d).len();
        d.apply(edit(&[(len / 3..2 * len / 3, "joined\n")])).expect("edits");
        check(&d, "a deletion across regions");
        let len = text_of(&d).len();
        d.apply(edit(&[(len..len, "tail")])).expect("edits");
        check(&d, "an insertion at the end");
        // One batch: the first patch joins the regions of a span into one, so the second
        // patch's region, further on, moves down the region list.
        let len = text_of(&d).len();
        let (cut, typed) = (len / 6..len / 4, 3 * len / 4);
        d.apply(edit(&[(cut, ""), (typed..typed, "\n\n")])).expect("edits");
        check(&d, "a batch that joins regions before another patch");
        d.apply(Command::PickAll { scope: PickScope::Unresolved, sources: vec![R] })
            .expect("picks all");
        check(&d, "a pick of every unresolved conflict");
        for step in ["undo 1", "undo 2", "undo 3", "undo 4", "undo 5"] {
            d.apply(Command::Undo).expect("undoes");
            check(&d, step);
        }
        d.apply(Command::Redo).expect("redoes");
        check(&d, "redo");
        let len = text_of(&d).len();
        d.apply(edit(&[(0..len, "")])).expect("edits");
        check(&d, "deleting everything");
    }
}
