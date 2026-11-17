//! The origin column both windows show (spec §6.4, §7.3): each input's line origins, whether
//! the column is turned on, the ID it names a commit by, and the commit each input shows in
//! its heading.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::input::{LineLabel, LineLabels};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{Hsla, Pixels, SharedString, px};
use merge_core::merge3::Source;
use merge_view::origin::{OriginKey, Origins, commit_runs, hover_text, revision_heading, short_id};
use vcs::{Annotation, IdKind, LineOrigin, VcsError};

/// The origin column's width.
pub(crate) const ORIGIN_WIDTH: Pixels = px(72.);

/// Whether a column shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Hidden,
    Shown,
}

impl Visibility {
    #[must_use]
    pub fn shown(self) -> bool {
        self == Self::Shown
    }

    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Hidden => Self::Shown,
            Self::Shown => Self::Hidden,
        }
    }
}

impl From<bool> for Visibility {
    fn from(shown: bool) -> Self {
        if shown { Self::Shown } else { Self::Hidden }
    }
}

/// What a pane's origin column shows for one cell: the short ID on the first line of a run,
/// and the hover card's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginCell {
    pub label: Option<SharedString>,
    pub hover: SharedString,
}

/// A pane heading as drawn (spec §5): its title, and the chip of the commit its input shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingText {
    /// The title, without a leading ID the chip shows.
    pub title: SharedString,
    pub chip: Option<OriginCell>,
}

impl HeadingText {
    /// `title` alone, with no chip.
    #[must_use]
    pub fn plain(title: &SharedString) -> Self {
        Self { title: title.clone(), chip: None }
    }
}

/// What the VCS records about each input's commit.
pub(crate) type Described = Vec<Result<Option<LineOrigin>, VcsError>>;
/// Each input's annotation.
pub(crate) type Annotated = Vec<Result<Option<Annotation>, VcsError>>;

/// A window's line origins and how its column shows them.
pub(crate) struct OriginColumn {
    /// Shared with the task that labels the result off the UI thread.
    origins: Arc<Origins>,
    column: Visibility,
    kind: IdKind,
    /// The commit each input shows, once loaded.
    revisions: Vec<(Source, LineOrigin)>,
}

impl OriginColumn {
    /// The column of a window over `sides` sides and `bases` bases, none loaded.
    pub(crate) fn new(sides: usize, bases: usize, column: Visibility, kind: IdKind) -> Self {
        Self { origins: Arc::new(Origins::new(sides, bases)), column, kind, revisions: Vec::new() }
    }

    pub(crate) fn origins(&self) -> &Origins {
        &self.origins
    }

    /// The origins, for a task off the UI thread.
    pub(crate) fn shared(&self) -> Arc<Origins> {
        Arc::clone(&self.origins)
    }

    /// The ID the column and the headings name a commit by.
    pub(crate) fn kind(&self) -> IdKind {
        self.kind
    }

    /// Whether the column shows: it is turned on and the origins have loaded. A window
    /// without origins draws no empty column (spec §7.3).
    pub(crate) fn shown(&self) -> bool {
        self.column.shown() && self.origins.loaded()
    }

    /// Turn the column on or off.
    pub(crate) fn toggle(&mut self) {
        self.column = self.column.toggled();
    }

    /// Show `annotation` as `source`'s origins. An input the window lacks is a defect, logged.
    pub(crate) fn set(&mut self, source: Source, annotation: Annotation) {
        if let Err(error) = Arc::make_mut(&mut self.origins).set(source, annotation) {
            tracing::error!(%error, ?source, "an annotation for an input the window lacks");
        }
    }

    /// Show each input's annotation, in the order of `sources`. An input that cannot be
    /// annotated shows no origins, and says why in the log.
    pub(crate) fn annotated(
        &mut self,
        sources: impl IntoIterator<Item = Source>,
        annotations: Annotated,
    ) {
        for (source, annotation) in sources.into_iter().zip(annotations) {
            match annotation {
                Ok(Some(annotation)) => self.set(source, annotation),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, ?source, "cannot annotate an input; it shows no origins");
                }
            }
        }
    }

    /// Take the commit each input shows, in the order of `sources`, for the headings.
    pub(crate) fn described(
        &mut self,
        sources: impl IntoIterator<Item = Source>,
        described: Described,
    ) {
        self.revisions = revisions(sources, described).collect();
    }

    /// The cell of `source`'s line `line`; none for a filler row or a line without an
    /// origin.
    pub(crate) fn cell(&self, source: Source, line: Option<usize>) -> Option<OriginCell> {
        let origin = self.origins.cell(source, line)?;
        Some(OriginCell {
            label: self.origins.cell_label(source, line, self.kind).map(SharedString::new),
            hover: hover_text(origin).into(),
        })
    }

    /// The heading of `source`'s pane, titled `title`.
    pub(crate) fn heading(&self, source: Source, title: &SharedString) -> HeadingText {
        let revision = self.revisions.iter().find(|(s, _)| *s == source).map(|(_, origin)| origin);
        heading_text(revision, title, self.kind)
    }

    /// An editor gutter's labels for `lines` while the column shows.
    pub(crate) fn labels<'a>(
        &self,
        lines: impl IntoIterator<Item = Option<&'a LineOrigin>>,
        colour: Hsla,
    ) -> Option<LineLabels> {
        self.shown().then(|| origin_labels(lines, colour, self.kind))
    }

    /// An editor gutter's labels for `runs`, from [`Origins::result_runs`], while the column
    /// shows. A run whose origin is gone gets no label. A short ID fits inside its
    /// `SharedString`, so a label allocates nothing; the hover card's text builds when it
    /// shows.
    pub(crate) fn run_labels(
        &self,
        runs: &[(Range<usize>, OriginKey)],
        colour: Hsla,
    ) -> Option<LineLabels> {
        self.shown().then(|| {
            let labels = runs
                .iter()
                .filter_map(|(lines, key)| {
                    let label = SharedString::new(short_id(self.origins.origin(*key)?, self.kind));
                    let (origins, key) = (Arc::clone(&self.origins), *key);
                    let hover = move |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
                        let text = origins.origin(key).map_or_default(hover_text);
                        Tooltip::new(text).build(window, cx)
                    };
                    Some(LineLabel::new(lines.clone(), label).with_tooltip(hover))
                })
                .collect();
            LineLabels::new(ORIGIN_WIDTH, colour, labels)
        })
    }
}

/// An editor gutter's labels for `lines`, one per run of lines from one commit: the short ID,
/// and the hover card over the run.
fn origin_labels<'a>(
    lines: impl IntoIterator<Item = Option<&'a LineOrigin>>,
    colour: Hsla,
    kind: IdKind,
) -> LineLabels {
    let labels = commit_runs(lines)
        .into_iter()
        .map(|(lines, origin)| {
            let hover = SharedString::from(hover_text(origin));
            LineLabel::new(lines, SharedString::new(short_id(origin, kind)))
                .with_tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
        })
        .collect();
    LineLabels::new(ORIGIN_WIDTH, colour, labels)
}

/// The commits `described` names, each with its input from `inputs`; an input whose commit
/// cannot be read shows none.
fn revisions(
    inputs: impl IntoIterator<Item = Source>,
    described: Described,
) -> impl Iterator<Item = (Source, LineOrigin)> {
    inputs.into_iter().zip(described).filter_map(|(input, described)| {
        described
            .inspect_err(|error| {
                tracing::warn!(%error, ?input, "cannot read an input's commit; its heading names none");
            })
            .ok()
            .flatten()
            .map(|origin| (input, origin))
    })
}

/// The heading of a pane titled `title` whose input shows the commit `revision`, its chip
/// naming the ID of `kind`.
fn heading_text(revision: Option<&LineOrigin>, title: &SharedString, kind: IdKind) -> HeadingText {
    let Some(origin) = revision else {
        return HeadingText::plain(title);
    };
    let heading = revision_heading(origin, title, kind);
    let shown = if heading.title.len() == title.len() {
        title.clone()
    } else {
        SharedString::new(heading.title)
    };
    HeadingText {
        title: shown,
        chip: Some(OriginCell {
            label: Some(SharedString::new(heading.id)),
            hover: hover_text(origin).into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use vcs::{ChangeInfo, OriginRun, Timestamp};

    use super::*;

    const LEFT: Source = Source::Side(0);
    const RIGHT: Source = Source::Side(1);

    fn origin(change: &str) -> LineOrigin {
        LineOrigin {
            change: ChangeInfo {
                commit: "c0ffee".to_owned(),
                change_id: Some(change.to_owned()),
                summary: Some("fix".to_owned()),
            },
            author: "A".to_owned(),
            time: Timestamp { seconds: 0, offset_seconds: 0 },
        }
    }

    /// Two lines from one change.
    fn annotation() -> Annotation {
        Annotation {
            origins: vec![origin("kxyz")],
            runs: vec![OriginRun { lines: 0..2, origin: 0 }],
        }
    }

    fn column(shown: Visibility) -> OriginColumn {
        OriginColumn::new(2, 1, shown, IdKind::Change)
    }

    #[test]
    fn the_column_shows_once_turned_on_and_loaded() {
        let mut c = column(Visibility::Shown);
        assert!(!c.shown(), "nothing loaded");
        c.annotated([LEFT], vec![Ok(Some(annotation()))]);
        assert!(c.shown());
        c.toggle();
        assert!(!c.shown(), "turned off");
        let mut hidden = column(Visibility::Hidden);
        hidden.set(LEFT, annotation());
        assert!(!hidden.shown(), "loaded but off");
    }

    #[test]
    fn a_cell_labels_the_first_line_of_a_run_and_every_line_hovers() {
        let mut c = column(Visibility::Shown);
        assert_eq!(c.cell(LEFT, Some(0)), None, "nothing loaded");
        c.annotated([LEFT, RIGHT], vec![Ok(Some(annotation())), Ok(None)]);
        let first = c.cell(LEFT, Some(0)).expect("an origin");
        assert_eq!(first.label.as_deref(), Some("kxyz"));
        let second = c.cell(LEFT, Some(1)).expect("an origin");
        assert_eq!((second.label, second.hover), (None, first.hover));
        assert_eq!(c.cell(LEFT, None), None, "a filler row");
        assert_eq!(c.cell(RIGHT, Some(0)), None, "an input without an annotation");
    }

    #[test]
    fn a_heading_takes_its_inputs_commit_and_others_stay_plain() {
        let mut c = column(Visibility::Hidden);
        let title = SharedString::from("a.rs");
        c.described([LEFT, RIGHT], vec![Ok(Some(origin("kxyz"))), Ok(None)]);
        let left = c.heading(LEFT, &title);
        assert_eq!(left.chip.and_then(|chip| chip.label).as_deref(), Some("kxyz"));
        assert_eq!(c.heading(RIGHT, &title), HeadingText::plain(&title));
    }

    /// Lines 0 and 2 from change `kone`, line 1 from `ktwo`.
    fn split_annotation(one: &str, two: &str) -> Annotation {
        Annotation {
            origins: vec![origin(one), origin(two)],
            runs: vec![
                OriginRun { lines: 0..1, origin: 0 },
                OriginRun { lines: 1..2, origin: 1 },
                OriginRun { lines: 2..3, origin: 0 },
            ],
        }
    }

    const SPLIT_RUNS: [(Range<usize>, OriginKey); 3] = [
        (0..1, OriginKey { source: LEFT, index: 0 }),
        (1..2, OriginKey { source: LEFT, index: 1 }),
        (2..3, OriginKey { source: LEFT, index: 0 }),
    ];

    fn run_label_texts(c: &OriginColumn) -> Vec<(Range<usize>, String)> {
        let labels = c.run_labels(&SPLIT_RUNS, Hsla::default()).expect("the column shows");
        labels.labels().iter().map(|l| (l.lines().clone(), l.label().to_string())).collect()
    }

    #[test]
    fn a_new_annotation_relabels_its_runs_and_a_missing_origin_has_none() {
        let mut c = column(Visibility::Shown);
        c.set(LEFT, split_annotation("kone", "ktwo"));
        c.set(LEFT, split_annotation("knew", "kalt"));
        assert_eq!(
            run_label_texts(&c),
            [(0..1, "knew".to_owned()), (1..2, "kalt".to_owned()), (2..3, "knew".to_owned())]
        );
        let gone = [(0..1, OriginKey { source: LEFT, index: 2 }), (1..2, SPLIT_RUNS[1].1)];
        let labels = c.run_labels(&gone, Hsla::default()).expect("the column shows");
        assert_eq!(labels.labels().len(), 1);
        assert_eq!(labels.labels()[0].lines(), &(1..2));
    }

    #[test]
    fn labels_follow_the_column() {
        let mut c = column(Visibility::Shown);
        c.set(LEFT, annotation());
        let lines = [Some(&origin("kxyz"))];
        assert!(c.labels(lines, Hsla::default()).is_some());
        c.toggle();
        assert!(c.labels(lines, Hsla::default()).is_none());
    }
}
