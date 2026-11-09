//! Line origins in the merge window (spec §5, §7.3): the commit behind each row of the top
//! panes and each line of the result pane, the gutter label at the start of each run, and the
//! hover text.

use std::ops::Range;

use chrono::{DateTime, FixedOffset};
use merge_core::document::{MergeDocument, Resolution};
use merge_core::index::{ByteOffset, Line};
use merge_core::merge3::{Inputs, Kind, Region, Source};
use smallvec::SmallVec;
use vcs::{Annotation, IdKind, LineOrigin};

/// Inputs a result line can show at once: every input of a three-way merge.
const SHOWN: usize = 3;

/// The input lines a result line shows, in order of preference.
pub type Shown = SmallVec<[(Source, usize); SHOWN]>;

/// How the hover card shows an origin's time: the author's local time and offset.
const HOVER_TIME: &str = "%Y-%m-%d %H:%M %:z";

/// A source the merge does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the merge has no {0:?}")]
pub struct NoInput(pub Source);

/// The annotation of each input, as far as it is known. Lines no run covers have no origin.
#[derive(Debug, Clone, Default)]
pub struct Origins {
    sides: Vec<Option<Annotation>>,
    bases: Vec<Option<Annotation>>,
}

impl Origins {
    /// No origins yet for a merge of `sides` sides and `bases` bases.
    #[must_use]
    pub fn new(sides: usize, bases: usize) -> Self {
        Self { sides: vec![None; sides], bases: vec![None; bases] }
    }

    /// Whether any input has its annotation, so there is blame to show.
    #[must_use]
    pub fn loaded(&self) -> bool {
        self.sides.iter().chain(&self.bases).any(Option::is_some)
    }

    fn slot(&mut self, source: Source) -> Option<&mut Option<Annotation>> {
        match source {
            Source::Side(i) => self.sides.get_mut(i),
            Source::Base(i) => self.bases.get_mut(i),
        }
    }

    /// Give `source` its annotation, replacing any it had.
    ///
    /// # Errors
    ///
    /// [`NoInput`] when the merge has no such input.
    pub fn set(&mut self, source: Source, annotation: Annotation) -> Result<(), NoInput> {
        let slot = self.slot(source).ok_or(NoInput(source))?;
        *slot = Some(annotation);
        Ok(())
    }

    /// The origin of `line` of `source`.
    #[must_use]
    pub fn line(&self, source: Source, line: usize) -> Option<&LineOrigin> {
        let annotation = match source {
            Source::Side(i) => self.sides.get(i),
            Source::Base(i) => self.bases.get(i),
        };
        annotation?.as_ref()?.origin(line)
    }

    /// The origin of a top pane's cell; a filler row has none.
    #[must_use]
    pub fn cell(&self, source: Source, cell: Option<usize>) -> Option<&LineOrigin> {
        self.line(source, cell?)
    }

    /// The gutter label of a top pane's cell: the short ID of `kind` on the first line of each
    /// run of lines from one commit.
    #[must_use]
    pub fn cell_label(&self, source: Source, cell: Option<usize>, kind: IdKind) -> Option<&str> {
        let line = cell?;
        let above = line.checked_sub(1).and_then(|l| self.line(source, l));
        label(self.line(source, line), above, kind)
    }

    /// The origin of result line `line`: that of the first input line it shows that has one.
    #[must_use]
    pub fn result_line(&self, doc: &MergeDocument, line: Line) -> Option<&LineOrigin> {
        shown_from(doc, line).into_iter().find_map(|(source, line)| self.line(source, line))
    }

    /// The gutter label of result line `line`: the short ID of `kind` on the first line of
    /// each run of lines from one commit.
    #[must_use]
    pub fn result_label(&self, doc: &MergeDocument, line: usize, kind: IdKind) -> Option<&str> {
        let above = line.checked_sub(1).and_then(|l| self.result_line(doc, Line::new(l)));
        label(self.result_line(doc, Line::new(line)), above, kind)
    }
}

/// `origin`'s short ID of `kind`, unless the line above comes from the same commit.
fn label<'a>(
    origin: Option<&'a LineOrigin>,
    above: Option<&LineOrigin>,
    kind: IdKind,
) -> Option<&'a str> {
    let origin = origin?;
    let continues = above.is_some_and(|above| above.change.commit == origin.change.commit);
    (!continues).then(|| short_id(origin, kind))
}

/// The runs of `lines` that come from one commit, each with the origin of its first line, in
/// order. Lines without an origin belong to no run.
pub fn commit_runs<'a>(
    lines: impl IntoIterator<Item = Option<&'a LineOrigin>>,
) -> Vec<(Range<usize>, &'a LineOrigin)> {
    let mut runs: Vec<(Range<usize>, &LineOrigin)> = Vec::new();
    for (line, origin) in lines.into_iter().enumerate() {
        let Some(origin) = origin else {
            continue;
        };
        match runs.last_mut() {
            Some((lines, last))
                if lines.end == line && last.change.commit == origin.change.commit =>
            {
                lines.end = line + 1;
            }
            _ => runs.push((line..line + 1, origin)),
        }
    }
    runs
}

/// The input lines result line `line` shows, each with its line number in that input. An
/// automatic unchanged line shows base and every side, in that order, unless a side's bytes
/// differ under a whitespace mode, which then shows that side alone (spec §4.2). A line of a
/// pick or an unresolved conflict shows its block's source. Edited and word-merged text, and a
/// line past the end, show none: no one input line holds them.
#[must_use]
pub fn shown_from(doc: &MergeDocument, line: Line) -> Shown {
    if line.get() >= doc.line_count() {
        return Shown::new();
    }
    let byte = doc.byte_of_line(line);
    let index = doc.region_at_byte(byte);
    let region = doc.region(index);
    match doc.resolution(index) {
        Resolution::Edited => Shown::new(),
        Resolution::Auto => {
            let start = doc.line_of_byte(doc.region_start(index));
            automatic(doc.inputs(), region, line.since(start))
        }
        Resolution::Pick(_) | Resolution::Unresolved => doc
            .blocks(index)
            .into_iter()
            .find(|(_, bytes)| bytes.contains(&byte.get()))
            .map(|(source, bytes)| {
                let offset = line.since(doc.line_of_byte(ByteOffset::new(bytes.start)));
                (source, lines(region, source).start + offset)
            })
            .into_iter()
            .collect(),
    }
}

/// The input lines line `n` of an automatic region shows, as `auto_text` renders it.
fn automatic(inputs: &Inputs, region: &Region, n: usize) -> Shown {
    let side = |i: usize| (Source::Side(i), region.sides[i].start + n);
    let sides = 0..inputs.sides().len();
    match region.kind {
        Kind::Unchanged => {
            let base_line = region.bases[0].start + n;
            let base = inputs.bases()[0].line(base_line);
            match sides.clone().find(|&i| inputs.sides()[i].line(side(i).1) != base) {
                Some(changed) => std::iter::once(side(changed)).collect(),
                None => {
                    std::iter::once((Source::Base(0), base_line)).chain(sides.map(side)).collect()
                }
            }
        }
        Kind::OneSide(i) => std::iter::once(side(i)).collect(),
        Kind::AllSame => sides.map(side).collect(),
        Kind::WordMerged | Kind::Conflict => Shown::new(),
    }
}

fn lines(region: &Region, source: Source) -> Range<usize> {
    match source {
        Source::Side(i) => region.sides[i].clone(),
        Source::Base(i) => region.bases[i].clone(),
    }
}

/// The ID a gutter shows for `origin` (spec §6.4): of `kind` under jj; git's commit.
#[must_use]
pub fn short_id(origin: &LineOrigin, kind: IdKind) -> &str {
    origin.change.id(kind)
}

/// A pane heading's title and revision chip, for a pane whose input shows the commit `origin`
/// (spec §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionHeading<'t, 'o> {
    /// The title without a leading ID of the commit, which the chip shows.
    pub title: &'t str,
    /// The chip's short ID, of the kind the settings pick.
    pub id: &'o str,
}

/// The heading of a pane titled `title` whose input shows the commit `origin`. The chip
/// holds the short ID of `kind`. A title that starts with one of the commit's IDs as a word,
/// as the title of a jj or git term without a label does, loses it and the space after it.
#[must_use]
pub fn revision_heading<'t, 'o>(
    origin: &'o LineOrigin,
    title: &'t str,
    kind: IdKind,
) -> RevisionHeading<'t, 'o> {
    let change = &origin.change;
    let after_id = |id: &str| {
        let rest = title.strip_prefix(id)?;
        if rest.is_empty() { Some(rest) } else { rest.strip_prefix(' ') }
    };
    let title = change
        .change_id
        .as_deref()
        .and_then(after_id)
        .or_else(|| after_id(&change.commit))
        .unwrap_or(title);
    RevisionHeading { title, id: short_id(origin, kind) }
}

/// The hover card's text for `origin`: the change ID and commit, the author and the author's
/// local time, then the summary. A time chrono cannot represent shows as seconds since the
/// epoch.
#[must_use]
pub fn hover_text(origin: &LineOrigin) -> String {
    let change = &origin.change;
    let ids = match &change.change_id {
        Some(id) => format!("{id} {}", change.commit),
        None => change.commit.clone(),
    };
    let time = FixedOffset::east_opt(origin.time.offset_seconds)
        .zip(DateTime::from_timestamp(origin.time.seconds, 0))
        .map_or_else(
            || format!("{} s", origin.time.seconds),
            |(offset, utc)| utc.with_timezone(&offset).format(HOVER_TIME).to_string(),
        );
    let author = format!("{}, {time}", origin.author);
    match change.summary.as_deref().filter(|s| !s.is_empty()) {
        Some(summary) => format!("{ids}\n{author}\n{summary}"),
        None => format!("{ids}\n{author}"),
    }
}

#[cfg(test)]
mod tests {
    use crate::rows::RowIx;
    use merge_core::diff::{DiffOptions, Whitespace};
    use merge_core::document::{Command, EditBatch, Labels, MergeOptions, TextPatch};
    use merge_core::index::RegionIx;
    use merge_core::merge3::HunkLevel;
    use merge_core::text::Text;
    use vcs::{ChangeInfo, OriginRun, Timestamp};

    use super::*;
    use crate::rows::{PaneSet, RowMap};

    const L: Source = Source::Side(0);
    const R: Source = Source::Side(1);
    const B: Source = Source::Base(0);

    fn text(s: &str) -> Text {
        Text::decode(s.as_bytes()).expect("utf-8")
    }

    fn doc_with(base: &str, left: &str, right: &str, options: MergeOptions) -> MergeDocument {
        MergeDocument::new(
            Inputs::three_way(text(base), text(left), text(right)),
            options,
            Labels::none(),
        )
        .expect("no labels")
    }

    fn doc(base: &str, left: &str, right: &str) -> MergeDocument {
        doc_with(base, left, right, MergeOptions::default())
    }

    fn git(commit: &str) -> LineOrigin {
        LineOrigin {
            change: ChangeInfo {
                commit: commit.to_owned(),
                change_id: None,
                summary: Some(format!("summary of {commit}")),
            },
            author: "Ada".to_owned(),
            time: Timestamp { seconds: 0, offset_seconds: 0 },
        }
    }

    /// Runs of lines, each naming a commit; each commit is one origin.
    fn annotation(runs: &[(Range<usize>, &str)]) -> Annotation {
        let mut commits: Vec<&str> = Vec::new();
        let runs = runs
            .iter()
            .map(|(lines, commit)| {
                let origin = commits.iter().position(|c| c == commit).unwrap_or_else(|| {
                    commits.push(commit);
                    commits.len() - 1
                });
                OriginRun { lines: lines.clone(), origin }
            })
            .collect();
        Annotation { origins: commits.into_iter().map(git).collect(), runs }
    }

    fn origins(annotated: &[(Source, Annotation)]) -> Origins {
        let mut origins = Origins::new(2, 1);
        for (source, annotation) in annotated {
            origins.set(*source, annotation.clone()).expect("the input exists");
        }
        origins
    }

    fn commit(origin: Option<&LineOrigin>) -> Option<&str> {
        origin.map(|o| o.change.commit.as_str())
    }

    fn shown(doc: &MergeDocument) -> Vec<Vec<(Source, usize)>> {
        (0..=doc.line_count()).map(|line| shown_from(doc, Line::new(line)).to_vec()).collect()
    }

    #[test]
    fn an_input_line_takes_its_runs_origin() {
        let origins = origins(&[(L, annotation(&[(0..2, "c1"), (2..3, "c2")]))]);
        let lines: Vec<_> = (0..4).map(|n| commit(origins.line(L, n))).collect();
        assert_eq!(lines, [Some("c1"), Some("c1"), Some("c2"), None]);
        assert_eq!(commit(origins.line(R, 0)), None, "right has no annotation");
        assert_eq!(commit(origins.line(B, 0)), None, "base has no annotation");
    }

    #[test]
    fn a_source_the_merge_lacks_is_refused() {
        let mut origins = Origins::new(2, 1);
        assert_eq!(
            origins.set(Source::Side(2), Annotation::default()),
            Err(NoInput(Source::Side(2)))
        );
        assert_eq!(
            origins.set(Source::Base(1), Annotation::default()),
            Err(NoInput(Source::Base(1)))
        );
        assert_eq!(origins.set(R, Annotation::default()), Ok(()));
    }

    #[test]
    fn origins_are_loaded_once_any_input_has_an_annotation() {
        let mut origins = Origins::new(2, 1);
        assert!(!origins.loaded(), "none before any annotation");
        assert_eq!(
            origins.set(Source::Side(2), Annotation::default()),
            Err(NoInput(Source::Side(2)))
        );
        assert!(!origins.loaded(), "a refused annotation loads nothing");
        assert_eq!(origins.set(B, Annotation::default()), Ok(()));
        assert!(origins.loaded(), "the base alone is enough");
    }

    #[test]
    fn a_later_annotation_replaces_an_earlier_one() {
        let mut origins = origins(&[(L, annotation(&[(0..1, "old")]))]);
        origins.set(L, annotation(&[(0..1, "new")])).expect("the input exists");
        assert_eq!(commit(origins.line(L, 0)), Some("new"));
    }

    #[test]
    fn side_pane_rows_take_their_lines_origins_and_filler_rows_none() {
        // Rows: a | X, Y (left only, right fills) | b.
        let doc = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        let map = RowMap::new(&doc, PaneSet::LeftRight).expect("panes fit");
        let origins = origins(&[
            (L, annotation(&[(0..1, "c1"), (1..3, "c2"), (3..4, "c1")])),
            (R, annotation(&[(0..2, "r1")])),
        ]);
        let pane = |pane: usize, source: Source| -> Vec<(Option<&str>, Option<&str>)> {
            (0..map.row_count())
                .map(|row| {
                    let cell = map.row(&doc, RowIx::new(row)).cells[pane];
                    (
                        commit(origins.cell(source, cell)),
                        origins.cell_label(source, cell, IdKind::Change),
                    )
                })
                .collect()
        };
        assert_eq!(
            pane(0, L),
            [
                (Some("c1"), Some("c1")),
                (Some("c2"), Some("c2")),
                (Some("c2"), None),
                (Some("c1"), Some("c1")),
            ]
        );
        assert_eq!(
            pane(1, R),
            [(Some("r1"), Some("r1")), (None, None), (None, None), (Some("r1"), None),]
        );
    }

    #[test]
    fn a_line_no_run_covers_has_no_origin_and_the_next_covered_line_is_labelled() {
        // A working copy's annotation from its parent: line 1 changed, so no run covers it.
        let origins = origins(&[(L, annotation(&[(0..1, "c1"), (2..3, "c1")]))]);
        let labels: Vec<_> =
            (0..3).map(|n| origins.cell_label(L, Some(n), IdKind::Change)).collect();
        assert_eq!(labels, [Some("c1"), None, Some("c1")]);
    }

    #[test]
    fn automatic_lines_show_base_then_each_side_or_the_side_that_changed() {
        let doc = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        assert_eq!(
            shown(&doc),
            [
                vec![(B, 0), (L, 0), (R, 0)],
                vec![(L, 1)],
                vec![(L, 2)],
                vec![(B, 1), (L, 3), (R, 1)],
                vec![],
            ]
        );
    }

    #[test]
    fn the_same_change_on_both_sides_shows_both() {
        let doc = doc("a\n", "b\n", "b\n");
        assert_eq!(doc.regions()[0].kind, Kind::AllSame);
        assert_eq!(shown(&doc), [vec![(L, 0), (R, 0)], vec![]]);
    }

    #[test]
    fn an_unchanged_line_that_differs_in_whitespace_shows_the_side_it_comes_from() {
        let options = MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::Trailing, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        let doc = doc_with("x\ny\n", "x\ny\n", "x \ny\n", options);
        assert_eq!(doc.text(), "x \ny\n");
        assert_eq!(shown(&doc), [vec![(R, 0)], vec![(B, 1), (L, 1), (R, 1)], vec![]]);
    }

    #[test]
    fn an_unresolved_conflict_shows_each_side_in_turn() {
        let doc = doc("a\nb\nc\n", "a\nL\nc\n", "a\nR1\nR2\nc\n");
        assert_eq!(doc.resolutions()[1], Resolution::Unresolved);
        assert_eq!(doc.text(), "a\nL\nR1\nR2\nc\n");
        assert_eq!(
            shown(&doc)[1..5],
            [vec![(L, 1)], vec![(R, 1)], vec![(R, 2)], vec![(B, 2), (L, 2), (R, 3)]]
        );
    }

    #[test]
    fn a_pick_shows_its_sources_in_its_order() {
        let mut doc = doc("a\nb\nc\n", "a\nL\nc\n", "a\nR1\nR2\nc\n");
        doc.apply(Command::Pick { region: RegionIx::new(1), sources: vec![R, B, L] })
            .expect("the pick applies");
        assert_eq!(doc.text(), "a\nR1\nR2\nb\nL\nc\n");
        assert_eq!(shown(&doc)[1..5], [vec![(R, 1)], vec![(R, 2)], vec![(B, 1)], vec![(L, 1)]]);
    }

    #[test]
    fn a_source_without_a_final_newline_keeps_its_lines() {
        let mut doc = doc("a\nb", "a\nL", "a\nR");
        doc.apply(Command::Pick { region: RegionIx::new(1), sources: vec![L, R] })
            .expect("the pick applies");
        assert_eq!(doc.text(), "a\nL\nR");
        assert_eq!(shown(&doc), [vec![(B, 0), (L, 0), (R, 0)], vec![(L, 1)], vec![(R, 1)], vec![]]);
    }

    #[test]
    fn edited_and_word_merged_text_shows_no_input() {
        let mut conflict = doc("a\nb\nc\n", "a\nL\nc\n", "a\nR1\nR2\nc\n");
        conflict
            .apply(Command::Edit(EditBatch(vec![TextPatch { range: 2..3, text: "M".to_owned() }])))
            .expect("the edit applies");
        assert_eq!(conflict.text(), "a\nM\nR1\nR2\nc\n");
        assert_eq!(conflict.resolutions()[1], Resolution::Edited);
        assert_eq!(shown(&conflict)[1..4], [vec![], vec![], vec![]]);

        // An edit inside text one side gave: the left side no longer holds it.
        let mut one_side = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        one_side
            .apply(Command::Edit(EditBatch(vec![TextPatch { range: 2..3, text: "Z".to_owned() }])))
            .expect("the edit applies");
        assert_eq!(one_side.text(), "a\nZ\nY\nb\n");
        assert_eq!(one_side.resolutions()[1], Resolution::Edited);
        assert_eq!(shown(&one_side)[1..3], [vec![], vec![]]);

        let options = MergeOptions { hunk_level: HunkLevel::Word, ..MergeOptions::default() };
        let doc = doc_with("a b c\n", "A b c\n", "a b C\n", options);
        assert_eq!(doc.regions()[0].kind, Kind::WordMerged);
        assert_eq!(shown(&doc), [vec![], vec![]]);
    }

    #[test]
    fn a_result_line_takes_the_first_shown_input_with_an_origin() {
        let doc = doc("a\nb\n", "a\nX\nY\nb\n", "a\nb\n");
        let origins = origins(&[
            (L, annotation(&[(0..1, "l1"), (1..3, "l2"), (3..4, "l1")])),
            (R, annotation(&[(0..2, "r1")])),
        ]);
        let lines: Vec<_> =
            (0..5).map(|n| commit(origins.result_line(&doc, Line::new(n)))).collect();
        assert_eq!(lines, [Some("l1"), Some("l2"), Some("l2"), Some("l1"), None]);

        let with_base = {
            let mut o = origins.clone();
            o.set(B, annotation(&[(0..2, "b1")])).expect("base exists");
            o
        };
        let lines: Vec<_> =
            (0..4).map(|n| commit(with_base.result_line(&doc, Line::new(n)))).collect();
        assert_eq!(lines, [Some("b1"), Some("l2"), Some("l2"), Some("b1")]);
    }

    #[test]
    fn the_result_gutter_labels_each_runs_first_line() {
        let mut doc = doc("a\nb\nc\n", "a\nL\nc\n", "a\nR1\nR2\nc\n");
        doc.apply(Command::Pick { region: RegionIx::new(1), sources: vec![R, L] })
            .expect("the pick applies");
        // Result: a (l1) | R1, R2 (r2) | L (l2) | c (l1).
        let origins = origins(&[
            (L, annotation(&[(0..1, "l1"), (1..2, "l2"), (2..3, "l1")])),
            (R, annotation(&[(0..1, "r1"), (1..3, "r2"), (3..4, "r1")])),
        ]);
        let labels: Vec<_> =
            (0..6).map(|n| origins.result_label(&doc, n, IdKind::Change)).collect();
        assert_eq!(labels, [Some("l1"), Some("r2"), None, Some("l2"), Some("l1"), None]);
    }

    #[test]
    fn the_gutter_shows_jjs_change_id_and_gits_commit() {
        let mut jj = git("1a2b3c4d");
        jj.change.change_id = Some("kxqvmnop".to_owned());
        assert_eq!(short_id(&jj, IdKind::Change), "kxqvmnop");
        assert_eq!(short_id(&git("1a2b3c4"), IdKind::Change), "1a2b3c4");
    }

    #[test]
    fn the_commit_kind_shows_jjs_commit() {
        let mut jj = git("1a2b3c4d");
        jj.change.change_id = Some("kxqvmnop".to_owned());
        assert_eq!(short_id(&jj, IdKind::Commit), "1a2b3c4d");
        assert_eq!(short_id(&git("1a2b3c4"), IdKind::Commit), "1a2b3c4");
    }

    /// `annotation` with a jj change ID, `change of COMMIT`, on each origin.
    fn jj_annotation(runs: &[(Range<usize>, &str)]) -> Annotation {
        let mut jj = annotation(runs);
        for origin in &mut jj.origins {
            origin.change.change_id = Some(format!("change of {}", origin.change.commit));
        }
        jj
    }

    #[test]
    fn every_gutter_label_takes_the_kind() {
        let doc = doc("a\nb\n", "a\nb\n", "a\nb\n");
        let origins = origins(&[(B, jj_annotation(&[(0..1, "c1"), (1..2, "c2")]))]);
        let cells = |kind| (0..2).map(|n| origins.cell_label(B, Some(n), kind)).collect::<Vec<_>>();
        let results =
            |kind| (0..2).map(|n| origins.result_label(&doc, n, kind)).collect::<Vec<_>>();
        assert_eq!(cells(IdKind::Change), [Some("change of c1"), Some("change of c2")]);
        assert_eq!(cells(IdKind::Commit), [Some("c1"), Some("c2")]);
        assert_eq!(results(IdKind::Change), [Some("change of c1"), Some("change of c2")]);
        assert_eq!(results(IdKind::Commit), [Some("c1"), Some("c2")]);
    }

    fn jj(commit: &str, change_id: &str) -> LineOrigin {
        let mut origin = git(commit);
        origin.change.change_id = Some(change_id.to_owned());
        origin
    }

    fn heading<'t>(origin: &LineOrigin, title: &'t str, kind: IdKind) -> (&'t str, String) {
        let h = revision_heading(origin, title, kind);
        (h.title, h.id.to_owned())
    }

    #[test]
    fn a_heading_names_its_commit_beside_a_label_or_file_name() {
        let origin = jj("1a2b3c4d", "kxqvmnop");
        assert_eq!(heading(&origin, "main", IdKind::Change), ("main", "kxqvmnop".to_owned()));
        assert_eq!(heading(&origin, "a.txt", IdKind::Commit), ("a.txt", "1a2b3c4d".to_owned()));
        assert_eq!(
            heading(&git("1a2b3c4"), "a_LOCAL_1234.txt", IdKind::Change),
            ("a_LOCAL_1234.txt", "1a2b3c4".to_owned())
        );
    }

    #[test]
    fn a_title_that_starts_with_the_change_id_loses_it_to_the_chip() {
        let origin = jj("1a2b3c4d", "kxqvmnop");
        assert_eq!(
            heading(&origin, "kxqvmnop fix parsing", IdKind::Change),
            ("fix parsing", "kxqvmnop".to_owned())
        );
        assert_eq!(
            heading(&origin, "kxqvmnop fix parsing", IdKind::Commit),
            ("fix parsing", "1a2b3c4d".to_owned()),
            "the chip names the commit; the title names neither ID"
        );
        assert_eq!(
            heading(&origin, "kxqvmnop", IdKind::Change),
            ("", "kxqvmnop".to_owned()),
            "a title of the ID alone is left empty"
        );
    }

    #[test]
    fn a_git_title_that_starts_with_the_commit_loses_it_to_the_chip() {
        assert_eq!(
            heading(&git("1a2b3c4"), "1a2b3c4 fix parsing", IdKind::Change),
            ("fix parsing", "1a2b3c4".to_owned())
        );
    }

    #[test]
    fn a_title_that_only_begins_with_the_ids_characters_keeps_them() {
        let origin = git("1a2b3c4");
        assert_eq!(
            heading(&origin, "1a2b3c4d.txt", IdKind::Commit),
            ("1a2b3c4d.txt", "1a2b3c4".to_owned())
        );
        assert_eq!(
            heading(&origin, "x 1a2b3c4", IdKind::Commit),
            ("x 1a2b3c4", "1a2b3c4".to_owned())
        );
    }

    #[test]
    fn the_hover_names_the_change_author_time_and_summary() {
        let origin = LineOrigin {
            change: ChangeInfo {
                commit: "1a2b3c4d".to_owned(),
                change_id: Some("kxqvmnop".to_owned()),
                summary: Some("fix parsing".to_owned()),
            },
            author: "Ada".to_owned(),
            // 2001-02-03T04:05:06Z, recorded at UTC+01:00.
            time: Timestamp { seconds: 981_173_106, offset_seconds: 3600 },
        };
        assert_eq!(
            hover_text(&origin),
            "kxqvmnop 1a2b3c4d\nAda, 2001-02-03 05:05 +01:00\nfix parsing"
        );
    }

    #[test]
    fn the_hover_of_a_git_commit_without_a_summary_west_of_utc() {
        let origin = LineOrigin {
            change: ChangeInfo { commit: "1a2b3c4".to_owned(), change_id: None, summary: None },
            author: "Ada".to_owned(),
            // The epoch, recorded at UTC−01:30: the evening before, local time.
            time: Timestamp { seconds: 0, offset_seconds: -5400 },
        };
        assert_eq!(hover_text(&origin), "1a2b3c4\nAda, 1969-12-31 22:30 -01:30");
    }

    #[test]
    fn the_hover_of_a_time_out_of_range_shows_the_raw_seconds() {
        let mut origin = git("1a2b3c4");
        origin.time = Timestamp { seconds: i64::MAX, offset_seconds: 0 };
        assert_eq!(
            hover_text(&origin),
            format!("1a2b3c4\nAda, {} s\nsummary of 1a2b3c4", i64::MAX)
        );
    }

    #[test]
    fn lines_from_one_commit_in_a_row_make_one_run() {
        let (one, two) = (git("c1"), git("c2"));
        let lines = [Some(&one), Some(&one), None, Some(&two), Some(&one), Some(&one)];
        assert_eq!(commit_runs(lines), [(0..2, &one), (3..4, &two), (4..6, &one)]);
    }

    #[test]
    fn lines_without_origins_make_no_runs() {
        assert_eq!(commit_runs([]), []);
        assert_eq!(commit_runs([None, None]), []);
        let one = git("c1");
        assert_eq!(commit_runs([None, Some(&one)]), [(1..2, &one)]);
        // A line without an origin ends the run, even between lines of one commit.
        assert_eq!(commit_runs([Some(&one), None, Some(&one)]), [(0..1, &one), (2..3, &one)]);
    }

    #[test]
    fn origins_of_one_commit_are_one_run_whoever_holds_them() {
        // Two copies of one commit's origin, as two inputs give them.
        let (one, again) = (git("c1"), git("c1"));
        assert_eq!(commit_runs([Some(&one), Some(&again)]), [(0..2, &one)]);
    }
}
