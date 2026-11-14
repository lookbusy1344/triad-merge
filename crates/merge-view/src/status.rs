//! The status bar and pane titles (spec §5). Counting visits every region, once per command.

use std::borrow::Cow;
use std::path::Path;

use merge_core::diff::{CaseRule, LineEndingRule, Whitespace};
use merge_core::document::{MergeDocument, OutputEnding};
use merge_core::merge3::{HunkLevel, Kind, Refine, SameChange, Semantics, Source};
use merge_core::text::{Encoding, LineEnding, LineEndings, Text, output_encoding};
use smallvec::SmallVec;

use crate::marks::{self, Tone};
use crate::rows::PaneSet;

const PANES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// Regions not `Unchanged`: whitespace-only changes are not counted (spec §4.2).
    pub differences: usize,
    pub conflicts: usize,
    /// Regions merged word by word, which the user should review (spec §5).
    pub word_merged: usize,
    /// Conflicts still showing their markers.
    pub unresolved: usize,
    /// The encoding the result is written in (spec §4.1).
    pub encoding: Encoding,
    /// The inputs do not all share one encoding.
    pub encoding_mismatch: bool,
    /// The ending newlines typed into the result take.
    pub line_ending: LineEnding,
    /// The saved output would hold both `\n` and `\r\n` endings.
    pub mixed_endings: bool,
    pub whitespace: Whitespace,
    /// Whether line endings take part in comparison (spec §4.2).
    pub line_endings: LineEndingRule,
    /// Whether letter case takes part in comparison (spec §4.2).
    pub case: CaseRule,
    /// Whose merge this is, and its rules (spec §4.3).
    pub semantics: Semantics,
    pub same_change: SameChange,
    pub hunk_level: HunkLevel,
    pub refine: Refine,
    /// Whether deep refinement and the word merge keep off the inside of syntax nodes.
    pub syntax_boundaries: bool,
}

/// A setting under which triad merges unlike the VCS whose semantics it follows (spec §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divergence {
    /// A whitespace mode, or line endings ignored, under jj's semantics: jj compares bytes.
    Whitespace,
    /// Case ignored under either semantics: neither git nor jj folds case.
    Case,
    /// `keep` under git's semantics: git resolves a change both sides made.
    SameChangeKept,
    /// The word level under git's semantics: git merges lines only.
    WordLevel,
    /// Deep refinement under either semantics: neither VCS splits a conflict inside.
    DeepRefine,
    /// Syntax boundaries, with deep refinement or the word level: neither VCS reads syntax.
    SyntaxBoundaries,
}

impl Status {
    /// The settings that make this merge differ from its VCS's, in a fixed order.
    pub fn divergences(&self) -> impl Iterator<Item = Divergence> {
        let jj = self.semantics == Semantics::Jj;
        [
            (
                jj && (self.whitespace != Whitespace::None
                    || self.line_endings == LineEndingRule::Ignore),
                Divergence::Whitespace,
            ),
            (self.case == CaseRule::Ignore, Divergence::Case),
            (!jj && self.same_change == SameChange::Keep, Divergence::SameChangeKept),
            (!jj && self.hunk_level == HunkLevel::Word, Divergence::WordLevel),
            (self.refine == Refine::Deep, Divergence::DeepRefine),
            (
                self.syntax_boundaries
                    && (self.refine == Refine::Deep || self.hunk_level == HunkLevel::Word),
                Divergence::SyntaxBoundaries,
            ),
        ]
        .into_iter()
        .filter_map(|(diverges, divergence)| diverges.then_some(divergence))
    }
}

#[must_use]
/// The counts and modes of `doc`, saved with `ending`.
pub fn status(doc: &MergeDocument, ending: OutputEnding) -> Status {
    let inputs = doc.inputs();
    let (base, left) = (&inputs.bases()[0], &inputs.sides()[0]);
    let mut encodings = inputs.bases().iter().chain(inputs.sides()).map(Text::encoding);
    let first = encodings.next();
    let count = |f: &dyn Fn(usize) -> bool| (0..doc.regions().len()).filter(|&r| f(r)).count();
    Status {
        differences: count(&|r| doc.regions()[r].kind != Kind::Unchanged),
        conflicts: count(&|r| doc.regions()[r].kind == Kind::Conflict),
        word_merged: count(&|r| doc.regions()[r].kind == Kind::WordMerged),
        unresolved: doc.unresolved(),
        encoding: output_encoding(base, left),
        encoding_mismatch: encodings.any(|e| Some(e) != first),
        line_ending: doc.typed_ending(),
        mixed_endings: doc.saved_endings(ending) == LineEndings::Mixed,
        whitespace: doc.options().diff.whitespace,
        line_endings: doc.options().diff.line_endings,
        case: doc.options().diff.case,
        semantics: doc.options().semantics,
        same_change: doc.options().same_change,
        hunk_level: doc.options().hunk_level,
        refine: doc.options().refine,
        syntax_boundaries: doc.options().syntax_boundaries,
    }
}

/// The inputs' file names, which title the panes of inputs without a label.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileNames {
    sides: Vec<String>,
    bases: Vec<String>,
}

impl FileNames {
    /// The names of a three-way merge's files: each path's last component, or the whole
    /// path when it has none.
    #[must_use]
    pub fn three_way(base: &Path, left: &Path, right: &Path) -> Self {
        Self { sides: vec![file_name(left), file_name(right)], bases: vec![file_name(base)] }
    }

    /// The names in the order a language is looked up from: the sides, then the bases.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.sides.iter().chain(&self.bases).map(Path::new)
    }

    fn of(&self, source: Source) -> Option<&str> {
        let name = match source {
            Source::Side(i) => self.sides.get(i),
            Source::Base(i) => self.bases.get(i),
        };
        name.map(String::as_str).filter(|name| !name.is_empty())
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
        .into_owned()
}

/// A pane's heading (spec §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneHeading<'a> {
    pub title: Cow<'a, str>,
    /// The pane's default title, when `title` is something else: a file name or label
    /// does not say which input it is.
    pub role: Option<Cow<'static, str>>,
    /// The tone of the heading's accent: the tone of the pane's bands.
    pub tone: Tone,
}

/// Each pane's heading. The title is its input's label, or its file name, or Left, Base and
/// Right in a three-way merge, or Side n and Base n with more sides.
#[must_use]
pub fn pane_headings<'a>(
    doc: &'a MergeDocument,
    panes: PaneSet,
    names: &'a FileNames,
) -> SmallVec<[PaneHeading<'a>; PANES]> {
    let labels = doc.labels();
    panes
        .sources()
        .into_iter()
        .map(|source| {
            let label = match source {
                Source::Side(i) => labels.side(i),
                Source::Base(i) => labels.base(i),
            };
            let role = role_name(doc, source);
            let (title, role) = match label.or_else(|| names.of(source)) {
                Some(title) if !title.eq_ignore_ascii_case(&role) => {
                    (Cow::Borrowed(title), Some(role))
                }
                Some(title) => (Cow::Borrowed(title), None),
                None => (role, None),
            };
            PaneHeading { title, role, tone: marks::pane_tone(source) }
        })
        .collect()
}

/// The result pane's role.
pub const RESULT_ROLE: &str = "Result";

/// The result pane's heading: the output's file name, or the whole path when it has none,
/// with the role in a chip; without an output, the role alone.
#[must_use]
pub fn result_heading(output: Option<&Path>) -> PaneHeading<'static> {
    let (title, role) = match output.map(file_name) {
        Some(name) if !name.eq_ignore_ascii_case(RESULT_ROLE) => {
            (Cow::Owned(name), Some(Cow::Borrowed(RESULT_ROLE)))
        }
        Some(name) => (Cow::Owned(name), None),
        None => (Cow::Borrowed(RESULT_ROLE), None),
    };
    PaneHeading { title, role, tone: Tone::Edited }
}

/// The name of `source`'s role: Left, Base and Right in a three-way merge, or Side n and
/// Base n with more sides.
#[must_use]
pub fn role_name(doc: &MergeDocument, source: Source) -> Cow<'static, str> {
    let three_way = doc.inputs().is_three_way();
    match (source, three_way) {
        (Source::Side(0), true) => Cow::Borrowed("Left"),
        (Source::Side(_), true) => Cow::Borrowed("Right"),
        (Source::Base(_), true) => Cow::Borrowed("Base"),
        (Source::Side(i), false) => Cow::Owned(format!("Side {}", i + 1)),
        (Source::Base(i), false) => Cow::Owned(format!("Base {}", i + 1)),
    }
}

/// Each pane's title, as [`pane_headings`] gives it.
#[must_use]
pub fn pane_titles<'a>(
    doc: &'a MergeDocument,
    panes: PaneSet,
    names: &'a FileNames,
) -> SmallVec<[Cow<'a, str>; PANES]> {
    pane_headings(doc, panes, names).into_iter().map(|heading| heading.title).collect()
}

#[cfg(test)]
mod tests {
    use merge_core::diff::DiffOptions;
    use merge_core::document::{Command, EditBatch, Labels, MergeOptions, TextPatch};
    use merge_core::index::RegionIx;
    use merge_core::merge3::Inputs;
    use merge_core::text::Text;

    use super::*;

    /// The status of `doc` saved with each line's own ending.
    fn status(doc: &MergeDocument) -> Status {
        super::status(doc, OutputEnding::Keep)
    }

    #[test]
    fn mixed_endings_count_only_where_the_save_keeps_them() {
        let d = with(
            b"a\r\nb\r\n",
            b"a\r\nB\n",
            b"a\r\nb\r\n",
            MergeOptions::default(),
            Labels::none(),
        );
        assert!(status(&d).mixed_endings);
        assert!(!super::status(&d, OutputEnding::Lf).mixed_endings);
        let lf = with(b"a\n", b"A\n", b"a\n", MergeOptions::default(), Labels::none());
        assert!(!status(&lf).mixed_endings);
    }

    fn text(s: &[u8]) -> Text {
        Text::decode(s).expect("decodes")
    }

    fn with(
        base: &[u8],
        left: &[u8],
        right: &[u8],
        options: MergeOptions,
        labels: Labels,
    ) -> MergeDocument {
        MergeDocument::new(Inputs::three_way(text(base), text(left), text(right)), options, labels)
            .expect("labels fit")
    }

    fn three(base: &str, left: &str, right: &str) -> MergeDocument {
        with(
            base.as_bytes(),
            left.as_bytes(),
            right.as_bytes(),
            MergeOptions::default(),
            Labels::none(),
        )
    }

    /// Regions: 0 a, 1 B (left only), 2 c, 3 D (right only), 4 e, 5 f (conflict), 6 g.
    /// Result: "a\nB\nc\nD\ne\n" then the conflict.
    fn mixed() -> MergeDocument {
        three("a\nb\nc\nd\ne\nf\ng\n", "a\nB\nc\nd\ne\nF\ng\n", "a\nb\nc\nD\ne\nX\ng\n")
    }

    fn counts(doc: &MergeDocument) -> (usize, usize, usize) {
        let s = status(doc);
        (s.differences, s.conflicts, s.unresolved)
    }

    fn mode_of(options: MergeOptions) -> (Semantics, HunkLevel, Vec<Divergence>) {
        let s = status(&with(b"a\n", b"a\n", b"a\n", options, Labels::none()));
        (s.semantics, s.hunk_level, s.divergences().collect())
    }

    #[test]
    fn the_default_merges_are_git_and_jj_without_divergence() {
        assert_eq!(mode_of(MergeOptions::default()), (Semantics::Git, HunkLevel::Line, vec![]));
        let jj = MergeOptions { semantics: Semantics::Jj, ..MergeOptions::default() };
        assert_eq!(mode_of(jj), (Semantics::Jj, HunkLevel::Line, vec![]));
    }

    #[test]
    fn whitespace_modes_diverge_from_jj_only() {
        for whitespace in [Whitespace::Trailing, Whitespace::Amount, Whitespace::All] {
            let diff = DiffOptions { whitespace, ..DiffOptions::default() };
            let jj = MergeOptions { semantics: Semantics::Jj, diff, ..MergeOptions::default() };
            assert_eq!(mode_of(jj).2, [Divergence::Whitespace], "{whitespace:?}");
            let git = MergeOptions { diff, ..MergeOptions::default() };
            assert_eq!(mode_of(git).2, [], "git ignores whitespace by choice: {whitespace:?}");
        }
    }

    #[test]
    fn ignoring_line_endings_diverges_from_jj_only() {
        let diff = DiffOptions { line_endings: LineEndingRule::Ignore, ..DiffOptions::default() };
        let jj = MergeOptions { semantics: Semantics::Jj, diff, ..MergeOptions::default() };
        assert_eq!(mode_of(jj).2, [Divergence::Whitespace]);
        let git = MergeOptions { diff, ..MergeOptions::default() };
        assert_eq!(mode_of(git).2, [], "git merge takes -Xignore-cr-at-eol");
    }

    #[test]
    fn ignoring_case_diverges_from_both() {
        let diff = DiffOptions { case: CaseRule::Ignore, ..DiffOptions::default() };
        let git = MergeOptions { diff, ..MergeOptions::default() };
        assert_eq!(mode_of(git).2, [Divergence::Case], "git merge-file has no case option");
        let jj = MergeOptions { semantics: Semantics::Jj, diff, ..MergeOptions::default() };
        assert_eq!(mode_of(jj).2, [Divergence::Case]);
        let spaces = DiffOptions { whitespace: Whitespace::All, ..diff };
        let jj = MergeOptions { diff: spaces, ..jj };
        assert_eq!(mode_of(jj).2, [Divergence::Whitespace, Divergence::Case]);
    }

    #[test]
    fn keeping_the_same_change_and_word_merging_diverge_from_git_only() {
        let keep = MergeOptions { same_change: SameChange::Keep, ..MergeOptions::default() };
        assert_eq!(mode_of(keep).2, [Divergence::SameChangeKept]);
        let word = MergeOptions { hunk_level: HunkLevel::Word, ..MergeOptions::default() };
        assert_eq!(mode_of(word), (Semantics::Git, HunkLevel::Word, vec![Divergence::WordLevel]));
        let both = MergeOptions {
            same_change: SameChange::Keep,
            hunk_level: HunkLevel::Word,
            ..MergeOptions::default()
        };
        assert_eq!(mode_of(both).2, [Divergence::SameChangeKept, Divergence::WordLevel]);
        let jj = MergeOptions {
            semantics: Semantics::Jj,
            same_change: SameChange::Keep,
            hunk_level: HunkLevel::Word,
            ..MergeOptions::default()
        };
        assert_eq!(mode_of(jj), (Semantics::Jj, HunkLevel::Word, vec![]), "jj has both");
    }

    #[test]
    fn deep_refinement_diverges_from_both() {
        for semantics in [Semantics::Git, Semantics::Jj] {
            let deep = MergeOptions { semantics, refine: Refine::Deep, ..MergeOptions::default() };
            assert_eq!(mode_of(deep).2, [Divergence::DeepRefine], "{semantics:?}");
            let off = MergeOptions { semantics, refine: Refine::Off, ..MergeOptions::default() };
            assert_eq!(mode_of(off).2, [], "{semantics:?}: diff3 style");
        }
    }

    #[test]
    fn syntax_boundaries_diverge_only_with_a_refinement_they_act_on() {
        let on = MergeOptions { syntax_boundaries: true, ..MergeOptions::default() };
        assert_eq!(mode_of(on).2, [], "nothing for them to act on");
        let deep = MergeOptions { refine: Refine::Deep, ..on };
        assert_eq!(mode_of(deep).2, [Divergence::DeepRefine, Divergence::SyntaxBoundaries]);
        let word = MergeOptions { semantics: Semantics::Jj, hunk_level: HunkLevel::Word, ..on };
        assert_eq!(mode_of(word).2, [Divergence::SyntaxBoundaries], "jj word merges");
        let off = MergeOptions { syntax_boundaries: false, ..word };
        assert_eq!(mode_of(off).2, []);
    }

    #[test]
    fn word_merged_regions_are_counted_and_are_not_conflicts() {
        // Region 1 merges by word; region 3 changes words on both sides that clash.
        let options = MergeOptions {
            hunk_level: merge_core::merge3::HunkLevel::Word,
            ..MergeOptions::default()
        };
        let d = with(
            b"a\nx = 1;\nb\nk = 1;\n",
            b"a\ny = 1;\nb\nk = 2;\n",
            b"a\nx = 2;\nb\nk = 3;\n",
            options,
            Labels::none(),
        );
        let s = status(&d);
        assert_eq!((s.differences, s.conflicts, s.word_merged, s.unresolved), (2, 1, 1, 1));
        assert_eq!(status(&mixed()).word_merged, 0);
    }

    #[test]
    fn counts_follow_picks_edits_and_merging_edits() {
        let mut d = mixed();
        assert_eq!(counts(&d), (3, 1, 1));
        d.apply(Command::Toggle { region: RegionIx::new(5), source: Source::Side(1) })
            .expect("pick right");
        assert_eq!(counts(&d), (3, 1, 0));
        d.apply(Command::Undo).expect("undo");
        // Typing into the unchanged region 2 keeps its kind.
        d.apply(Command::Edit(EditBatch(vec![TextPatch { range: 4..4, text: "x".to_owned() }])))
            .expect("type");
        assert_eq!(counts(&d), (3, 1, 1));
        // Deleting "\nxc\nD" joins B, c and D into one left-only region.
        d.apply(Command::Edit(EditBatch(vec![TextPatch { range: 3..8, text: String::new() }])))
            .expect("merging edit");
        assert_eq!(d.regions().len(), 5);
        assert_eq!(counts(&d), (2, 1, 1));
    }

    #[test]
    fn picks_clears_and_edits_change_the_merge_until_undone() {
        let mut d = mixed();
        assert!(!d.changed(), "a new merge");
        d.apply(Command::Toggle { region: RegionIx::new(5), source: Source::Side(0) })
            .expect("pick left");
        assert!(d.changed(), "a pick");
        d.apply(Command::Undo).expect("undo");
        assert!(!d.changed(), "the pick undone");
        d.apply(Command::Clear { region: RegionIx::new(1) }).expect("clear");
        assert!(d.changed(), "a cleared automatic region");
        d.apply(Command::Undo).expect("undo");
        d.apply(Command::Edit(EditBatch(vec![TextPatch { range: 0..0, text: "x".to_owned() }])))
            .expect("type");
        assert!(d.changed(), "an edit to an unchanged region");
    }

    #[test]
    fn identical_inputs_count_nothing() {
        assert_eq!(counts(&three("a\n", "a\n", "a\n")), (0, 0, 0));
    }

    #[test]
    fn the_encoding_is_bases_and_a_mix_is_flagged() {
        let utf16le = |s: &str| -> Vec<u8> {
            [0xFF, 0xFE].into_iter().chain(s.encode_utf16().flat_map(u16::to_le_bytes)).collect()
        };
        let mixed = with(b"a\n", &utf16le("a\n"), b"a\n", MergeOptions::default(), Labels::none());
        assert_eq!(
            (status(&mixed).encoding, status(&mixed).encoding_mismatch),
            (Encoding::Utf8, true)
        );
        let empty_base =
            with(b"", &utf16le("a\n"), &utf16le("a\n"), MergeOptions::default(), Labels::none());
        assert_eq!(
            (status(&empty_base).encoding, status(&empty_base).encoding_mismatch),
            (Encoding::Utf16Le, true)
        );
        let same = three("a\n", "a\n", "a\n");
        assert!(!status(&same).encoding_mismatch);
    }

    #[test]
    fn the_line_ending_and_whitespace_mode_are_shown() {
        let options = MergeOptions {
            diff: DiffOptions { whitespace: Whitespace::Amount, ..DiffOptions::default() },
            ..MergeOptions::default()
        };
        let d = with(b"a\r\n", b"a\r\n", b"a\r\n", options, Labels::none());
        assert_eq!(
            (status(&d).line_ending, status(&d).whitespace),
            (LineEnding::CrLf, Whitespace::Amount)
        );
        assert_eq!(status(&three("a\n", "a\n", "a\n")).whitespace, Whitespace::None);
    }

    fn git_names() -> FileNames {
        FileNames::three_way(
            Path::new("/tmp/foo_BASE_1234.c"),
            Path::new("./foo_LOCAL_1234.c"),
            Path::new("foo_REMOTE_1234.c"),
        )
    }

    #[test]
    fn pane_titles_use_file_names_without_labels() {
        let unlabelled = three("a\n", "a\n", "a\n");
        assert_eq!(
            pane_titles(&unlabelled, PaneSet::LeftBaseRight, &git_names()).to_vec(),
            ["foo_LOCAL_1234.c", "foo_BASE_1234.c", "foo_REMOTE_1234.c"]
        );
        assert_eq!(
            pane_titles(&unlabelled, PaneSet::LeftRight, &git_names()).to_vec(),
            ["foo_LOCAL_1234.c", "foo_REMOTE_1234.c"]
        );
    }

    #[test]
    fn a_label_wins_over_a_file_name_and_an_empty_one_does_not() {
        let d = with(
            b"a\n",
            b"a\n",
            b"a\n",
            MergeOptions::default(),
            Labels::three_way("ours", "", "theirs"),
        );
        assert_eq!(
            pane_titles(&d, PaneSet::LeftBaseRight, &git_names()).to_vec(),
            ["ours", "foo_BASE_1234.c", "theirs"]
        );
    }

    #[test]
    fn paths_list_the_sides_then_the_bases() {
        assert_eq!(
            git_names().paths().collect::<Vec<_>>(),
            [
                Path::new("foo_LOCAL_1234.c"),
                Path::new("foo_REMOTE_1234.c"),
                Path::new("foo_BASE_1234.c"),
            ]
        );
        assert_eq!(FileNames::default().paths().count(), 0);
    }

    #[test]
    fn a_path_without_a_file_name_shows_whole() {
        let names = FileNames::three_way(Path::new("/"), Path::new(".."), Path::new(""));
        assert_eq!(
            pane_titles(&three("a\n", "a\n", "a\n"), PaneSet::LeftBaseRight, &names).to_vec(),
            ["..", "/", "Right"]
        );
    }

    #[test]
    fn pane_titles_use_labels_then_defaults() {
        let unlabelled = three("a\n", "a\n", "a\n");
        assert_eq!(
            pane_titles(&unlabelled, PaneSet::LeftBaseRight, &FileNames::default()).to_vec(),
            ["Left", "Base", "Right"]
        );
        let labelled = with(
            b"a\n",
            b"a\n",
            b"a\n",
            MergeOptions::default(),
            Labels::three_way("ours", "anc", "theirs"),
        );
        assert_eq!(
            pane_titles(&labelled, PaneSet::LeftBaseRight, &FileNames::default()).to_vec(),
            ["ours", "anc", "theirs"]
        );
        let empty = with(
            b"a\n",
            b"a\n",
            b"a\n",
            MergeOptions::default(),
            Labels::three_way("", "", "theirs"),
        );
        assert_eq!(
            pane_titles(&empty, PaneSet::LeftBaseRight, &FileNames::default()).to_vec(),
            ["Left", "Base", "theirs"]
        );
    }

    fn roles<'a>(headings: &'a [PaneHeading<'_>]) -> Vec<Option<&'a str>> {
        headings.iter().map(|h| h.role.as_deref()).collect()
    }

    fn tones(headings: &[PaneHeading<'_>]) -> Vec<Tone> {
        headings.iter().map(|h| h.tone).collect()
    }

    #[test]
    fn file_name_titles_carry_their_role() {
        let (d, names) = (three("a\n", "a\n", "a\n"), git_names());
        let headings = pane_headings(&d, PaneSet::LeftBaseRight, &names);
        assert_eq!(roles(&headings), [Some("Left"), Some("Base"), Some("Right")]);
        assert_eq!(tones(&headings), [Tone::Left, Tone::Base, Tone::Right]);
        let two = pane_headings(&d, PaneSet::LeftRight, &names);
        assert_eq!(roles(&two), [Some("Left"), Some("Right")]);
        assert_eq!(tones(&two), [Tone::Left, Tone::Right]);
    }

    #[test]
    fn a_title_that_names_its_role_has_no_role() {
        let (d, names) = (three("a\n", "a\n", "a\n"), FileNames::default());
        let headings = pane_headings(&d, PaneSet::LeftBaseRight, &names);
        assert_eq!(roles(&headings), [None, None, None]);
        let labelled = with(
            b"a\n",
            b"a\n",
            b"a\n",
            MergeOptions::default(),
            Labels::three_way("LEFT", "", "theirs"),
        );
        let headings = pane_headings(&labelled, PaneSet::LeftBaseRight, &names);
        assert_eq!(roles(&headings), [None, None, Some("Right")]);
    }

    #[test]
    fn the_result_heading_names_the_output_file() {
        let heading = result_heading(Some(Path::new("/repo/src/foo.c")));
        assert_eq!(heading.title, "foo.c");
        assert_eq!(heading.role.as_deref(), Some("Result"));
        assert_eq!(heading.tone, Tone::Edited);
    }

    #[test]
    fn the_result_heading_without_an_output_is_its_role() {
        let heading = result_heading(None);
        assert_eq!(heading.title, "Result");
        assert_eq!(heading.role, None);
        assert_eq!(heading.tone, Tone::Edited);
    }

    #[test]
    fn a_result_file_named_for_its_role_has_no_role() {
        let heading = result_heading(Some(Path::new("out/RESULT")));
        assert_eq!(heading.title, "RESULT");
        assert_eq!(heading.role, None);
    }

    #[test]
    fn a_result_path_without_a_file_name_shows_whole() {
        let heading = result_heading(Some(Path::new("..")));
        assert_eq!(heading.title, "..");
        assert_eq!(heading.role.as_deref(), Some("Result"));
    }

    #[test]
    fn more_sides_take_the_left_tone_then_the_right() {
        let inputs = Inputs::new(
            vec![text(b"a\n"), text(b"a\n"), text(b"a\n")],
            vec![text(b"a\n"), text(b"a\n")],
        )
        .expect("three sides, two bases");
        let d =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
        let names = git_names();
        let headings = pane_headings(&d, PaneSet::Sides(3), &names);
        assert_eq!(roles(&headings), [Some("Side 1"), Some("Side 2"), None]);
        assert_eq!(tones(&headings), [Tone::Left, Tone::Right, Tone::Right]);
    }

    #[test]
    fn more_sides_are_numbered() {
        let inputs = Inputs::new(
            vec![text(b"a\n"), text(b"a\n"), text(b"a\n")],
            vec![text(b"a\n"), text(b"a\n")],
        )
        .expect("three sides, two bases");
        let d =
            MergeDocument::new(inputs, MergeOptions::default(), Labels::none()).expect("no labels");
        assert_eq!(
            pane_titles(&d, PaneSet::Sides(3), &FileNames::default()).to_vec(),
            ["Side 1", "Side 2", "Side 3"]
        );
        // Names for three inputs leave the third side to its number.
        assert_eq!(
            pane_titles(&d, PaneSet::Sides(3), &git_names()).to_vec(),
            ["foo_LOCAL_1234.c", "foo_REMOTE_1234.c", "Side 3"]
        );
    }
}
