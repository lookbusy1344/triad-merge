//! Which revision each input's line origins come from (spec §7.3,
//! `docs/DESIGN.md` §C3).
//!
//! An input takes its origins from a commit whose file holds exactly the input's bytes, or, for
//! the working copy of `diff PATH`, from its parent revision by a line diff. Every other input
//! has none: renamed files, inputs the user edited, a `jj resolve -r` of a revision triad cannot
//! see. Only UTF-8 inputs qualify, since only their lines split where the VCS's do.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use camino::{Utf8Path, Utf8PathBuf};
use std::convert::Infallible;

use merge_core::diff::{DiffOptions, diff_lines};
use merge_core::merge3::{Inputs, Source as Input};
use merge_core::text::{Encoding, Text};
use vcs::{Annotation, CommitId, LineOrigin, RepoKind, Repository, RevSpec, VcsError};

use crate::repository::RunRepository;

/// How an input's lines relate to its revision's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// The file at the revision holds the input's bytes, so each line takes the origin of the
    /// same line there.
    Exact,
    /// The input is the working copy, or the index, on top of the revision. Lines the revision
    /// holds keep their origins, carried by [`carry`]; changed lines have
    /// none.
    Changed,
}

/// The revision an input's origins come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The file's path from the repository root.
    pub path: Utf8PathBuf,
    /// The full commit ID, so the annotation reads the commit checked here even when a ref
    /// moves in between.
    pub commit: CommitId,
    pub fit: Fit,
}

/// `jj diff --tool` copies each side to a temporary directory whose name starts with this,
/// in a subdirectory named for the side (jj-cli 0.47 `merge_tools/external.rs`).
pub const JJ_DIFF_PREFIX: &str = "jj-diff-";
/// The side `jj diff --tool` copies the revision it shows changes in to.
pub(crate) const JJ_DIFF_RIGHT: &str = "right";
const JJ_DIFF_SIDES: [&str; 3] = ["left", JJ_DIFF_RIGHT, "output"];
/// A two-way diff's inputs: left and right.
const SIDES: usize = 2;

/// Sources found in a repository, which holds them for annotating. Each list holds one entry
/// per input, in the order the lookup took the inputs: a merge's in [`merge_inputs`] order,
/// a diff's left then right.
pub struct Found {
    repo: Option<Box<dyn Repository>>,
    pub sources: Vec<Option<Source>>,
    /// The commit each input shows, which its pane heading names: the commit of an exact
    /// source; the revision `diff PATH` reads its left side from, whether or not the file is
    /// there; and jj's working-copy commit for the working copy. git's working tree and index
    /// are no commit.
    pub revisions: Vec<Option<CommitId>>,
}

impl Found {
    fn none(inputs: usize) -> Self {
        Self { repo: None, sources: vec![None; inputs], revisions: vec![None; inputs] }
    }

    /// Sources found in `repo`, each input showing the commit of its exact source.
    fn exact(repo: Box<dyn Repository>, sources: Vec<Option<Source>>) -> Self {
        let revisions = sources
            .iter()
            .map(|source| {
                source
                    .as_ref()
                    .filter(|source| source.fit == Fit::Exact)
                    .map(|source| source.commit.clone())
            })
            .collect();
        Self { repo: Some(repo), sources, revisions }
    }

    /// What the VCS records about each input's commit, in the inputs' order: none for an
    /// input that shows no commit. Each input fails on its own.
    ///
    /// # Errors
    ///
    /// Per input, [`VcsError`] when the commit cannot be read.
    #[must_use]
    pub fn describe(&self) -> Vec<Result<Option<LineOrigin>, VcsError>> {
        self.revisions
            .iter()
            .map(|revision| match (&self.repo, revision) {
                (Some(repo), Some(commit)) => repo.describe(commit).map(Some),
                _ => Ok(None),
            })
            .collect()
    }

    /// Each input's annotation, `inputs` in the order the lookup took them: none without a
    /// source, or when the file does not exist at its commit. A `Changed` input takes its
    /// commit's annotation carried onto its text. Each input fails on its own.
    ///
    /// # Errors
    ///
    /// Per input, [`VcsError`] when the history cannot be read.
    #[must_use]
    pub fn annotate(&self, inputs: &[&Text]) -> Vec<Result<Option<Annotation>, VcsError>> {
        self.sources
            .iter()
            .zip(inputs)
            .map(|(source, input)| match (&self.repo, source) {
                (Some(repo), Some(source)) => annotation(repo.as_ref(), source, input),
                _ => Ok(None),
            })
            .collect()
    }
}

/// A merge's inputs in jj's order of terms, every base then every side, as [`find_merge`]
/// takes them.
pub fn merge_inputs(inputs: &Inputs) -> impl Iterator<Item = (Input, &Text)> {
    let bases = inputs.bases().iter().enumerate().map(|(n, text)| (Input::Base(n), text));
    let sides = inputs.sides().iter().enumerate().map(|(n, text)| (Input::Side(n), text));
    bases.chain(sides)
}

/// The sources of a merge writing `output`, an absolute path, with the window title `title`,
/// in the run's repository, which the sources keep for annotating
/// ([`RunRepository::of_merge`]).
/// The sources follow [`merge_inputs`]' order, for a merge of any width.
/// - jj: `title` names the file, as `jj resolve` passes `$path`. Each input takes the first
///   of `@`, its parents and, with two parents, their merge base whose file holds its bytes.
/// - git: the output names the file, as `git mergetool` passes `$MERGED`. Left is `HEAD`;
///   right and base follow from the operation stopped on the conflict.
///
/// No repository, or a file outside it, gives no sources.
///
/// # Errors
///
/// [`VcsError`] when the repository cannot be opened or read.
pub fn find_merge(
    repo: RunRepository,
    output: &Path,
    title: Option<&str>,
    inputs: &Inputs,
) -> Result<Found, VcsError> {
    let Some(repo) = repo.into_open()? else {
        return Ok(Found::none(inputs.bases().len() + inputs.sides().len()));
    };
    let sources = merge_in(repo.as_ref(), output, title, inputs)?;
    Ok(Found::exact(repo, sources))
}

fn merge_in(
    repo: &dyn Repository,
    output: &Path,
    title: Option<&str>,
    inputs: &Inputs,
) -> Result<Vec<Option<Source>>, VcsError> {
    // `jj resolve` names the file in the title; `git mergetool` writes to it.
    let path = match repo.kind() {
        RepoKind::Jj => title.map(Utf8PathBuf::from),
        RepoKind::Git => relative(repo, output),
    };
    let Some(path) = path else {
        return Ok(merge_inputs(inputs).map(|_| None).collect());
    };
    let candidates = repo.merge_candidates()?;
    merge_inputs(inputs)
        .map(|(input, text)| {
            let from = match input {
                Input::Base(n) => candidates.base(n).collect::<Vec<_>>(),
                Input::Side(n) => candidates.side(n).collect(),
            };
            first_exact(repo, &path, text, from)
        })
        .collect()
}

/// The sources of `diff PATH` against `rev`, or against the default parent without one, in
/// the run's repository, the one holding `path` ([`RunRepository::of_file`]), which the
/// sources keep for annotating. Left is the revision's file, right the working copy annotated
/// against the default parent. The index, which has no history of its own, is annotated
/// against `HEAD`.
///
/// # Errors
///
/// [`VcsError`] when the repository cannot be opened or read.
pub fn find_diff(
    repo: RunRepository,
    path: &Path,
    rev: Option<&str>,
    inputs: [&Text; 2],
) -> Result<Found, VcsError> {
    let Some(repo) = repo.into_open()? else {
        return Ok(Found::none(inputs.len()));
    };
    diff_in(repo, path, rev, inputs)
}

/// The sources of `diff PATH` in `repo`, and the commit each side shows; none for a file
/// outside the repository.
fn diff_in(
    owned: Box<dyn Repository>,
    path: &Path,
    rev: Option<&str>,
    [left, right]: [&Text; 2],
) -> Result<Found, VcsError> {
    let repo = owned.as_ref();
    let Some(path) = relative(repo, path) else {
        return Ok(Found::none(SIDES));
    };
    let parent = repo.default_parent(&path)?.rev;
    let history = history(repo, &parent)?;
    let parent = match &parent {
        RevSpec::Expr(expr) => Some(expr.as_str()),
        RevSpec::Commit(id) => Some(id.as_str()),
        RevSpec::Index => None,
    };
    let read_from = rev.or(parent);
    let left_revision = read_from.map(|rev| single_commit(repo, rev)).transpose()?.flatten();
    let left_source = match read_from {
        Some(_) => first_exact(repo, &path, left, &left_revision)?,
        None => changed(&path, left, history.clone()),
    };
    let right_revision = repo.working_copy_id()?;
    let right_source = changed(&path, right, history);
    Ok(Found {
        sources: vec![left_source, right_source],
        revisions: vec![left_revision, right_revision],
        repo: Some(owned),
    })
}

/// The sources of `diff LEFT RIGHT` as `git difftool` or `jj diff --tool` runs it, in `cwd`
/// with the window title `title`, `paths` joined to `cwd`. Both tools pass temporary copies, so each side takes its
/// source by the byte check alone.
///
/// The file is the path below a `jj-diff-<random>` side, else a side that lies in the working
/// tree, else `title`. The repository is the run's
/// ([`RunRepository::of_diff_tool`]), which the sources keep for annotating.
/// - git: a side in the working tree is annotated against the default parent, by a line diff;
///   a copy takes `HEAD` when it holds its bytes, else, when the index does, `HEAD` by a line
///   diff.
/// - jj: each side takes the first of `@-` and `@` whose file holds its bytes.
///
/// # Errors
///
/// [`VcsError`] when the repository cannot be opened or read.
pub fn find_diff_tool(
    repo: RunRepository,
    paths: &[PathBuf; 2],
    title: Option<&str>,
    cwd: &Path,
    inputs: [&Text; 2],
) -> Result<Found, VcsError> {
    let Some(repo) = repo.into_open()? else {
        return Ok(Found::none(inputs.len()));
    };
    let sources = diff_tool_in(repo.as_ref(), paths, title, cwd, inputs)?;
    Ok(Found::exact(repo, sources.into()))
}

fn diff_tool_in(
    repo: &dyn Repository,
    paths: &[PathBuf; 2],
    title: Option<&str>,
    cwd: &Path,
    [left, right]: [&Text; 2],
) -> Result<[Option<Source>; 2], VcsError> {
    let in_tree = paths.each_ref().map(|path| relative(repo, path));
    let copied = paths
        .iter()
        .find_map(|path| jj_diff_copy(path, cwd))
        .and_then(|path| Utf8PathBuf::from_path_buf(path).ok());
    let path = copied
        .or_else(|| in_tree[1].clone())
        .or_else(|| in_tree[0].clone())
        .or_else(|| title.map(Utf8PathBuf::from));
    let Some(path) = path else {
        return Ok([None, None]);
    };
    let [left_in_tree, right_in_tree] = in_tree.map(|found| found.as_ref() == Some(&path));
    match repo.kind() {
        RepoKind::Jj => {
            let candidates = repo.copy_candidates()?;
            Ok([
                first_exact(repo, &path, left, &candidates)?,
                first_exact(repo, &path, right, &candidates)?,
            ])
        }
        RepoKind::Git => Ok([
            git_tool_side(repo, &path, left, left_in_tree)?,
            git_tool_side(repo, &path, right, right_in_tree)?,
        ]),
    }
}

/// `source`'s annotation, as [`Found::annotate`] gives it.
fn annotation(
    repo: &dyn Repository,
    source: &Source,
    input: &Text,
) -> Result<Option<Annotation>, VcsError> {
    let Some(annotation) = repo.annotate(&source.path, &source.commit)? else {
        return Ok(None);
    };
    match source.fit {
        Fit::Exact => Ok(Some(annotation)),
        Fit::Changed => {
            let rev = RevSpec::Commit(source.commit.clone());
            let Some(bytes) = repo.file_at(&source.path, &rev)? else {
                return Ok(None);
            };
            match Text::decode(bytes) {
                Ok(from) => Ok(Some(carry(&annotation, &from, input))),
                // A binary revision shares no lines with a text input: nothing to carry.
                Err(error) => {
                    tracing::debug!(%error, path = %source.path, "the revision is not text");
                    Ok(None)
                }
            }
        }
    }
}

/// One side of `git difftool`: the working-tree file itself, or a copy.
fn git_tool_side(
    repo: &dyn Repository,
    path: &Utf8Path,
    input: &Text,
    in_tree: bool,
) -> Result<Option<Source>, VcsError> {
    if in_tree {
        let parent = repo.default_parent(path)?.rev;
        return Ok(changed(path, input, history(repo, &parent)?));
    }
    if let Some(source) = first_exact(repo, path, input, &repo.copy_candidates()?)? {
        return Ok(Some(source));
    }
    let staged = match repo.file_at(path, &RevSpec::Index) {
        Err(VcsError::OutsideRepository(_)) => return Ok(None),
        staged => staged?,
    };
    let is_staged =
        is_utf8(input) && staged.is_some_and(|bytes| bytes == *input.encode(input.content()));
    if is_staged { Ok(changed(path, input, repo.index_base()?)) } else { Ok(None) }
}

/// The commit whose history the lines of `parent` take: the commit it names, or for the
/// index, the commit the index stages on.
fn history(repo: &dyn Repository, parent: &RevSpec) -> Result<Option<CommitId>, VcsError> {
    match parent {
        RevSpec::Expr(expr) => single_commit(repo, expr),
        RevSpec::Commit(id) => Ok(Some(id.clone())),
        RevSpec::Index => repo.index_base(),
    }
}

/// The repository path of a file in a side of the temporary directory `jj diff --tool` makes:
/// what follows `jj-diff-<random>/left`, `/right` or `/output` in `path`, taken from `cwd`
/// when relative (spec §7.1). `None` for any other path.
#[must_use]
pub fn jj_diff_copy(path: &Path, cwd: &Path) -> Option<PathBuf> {
    jj_diff_side(path, cwd).map(|(_, path)| path)
}

/// The side, `left`, `right` or `output`, and the repository path of a file in the temporary
/// directory `jj diff --tool` makes, as [`jj_diff_copy`] reads it.
pub(crate) fn jj_diff_side(path: &Path, cwd: &Path) -> Option<(&'static str, PathBuf)> {
    // Lexical: `..` drops the name before it, as the path names it, not as links resolve.
    let full = cwd.join(path);
    let mut names: Vec<&OsStr> = Vec::new();
    for component in full.components() {
        match component {
            Component::Normal(name) => names.push(name),
            Component::ParentDir => {
                names.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::CurDir => {}
        }
    }
    names.windows(2).enumerate().find_map(|(at, pair)| {
        let [dir, side] = pair else { return None };
        let random = dir
            .to_str()
            .and_then(|d| d.strip_prefix(JJ_DIFF_PREFIX))
            .is_some_and(|rest| !rest.is_empty());
        let side = JJ_DIFF_SIDES.into_iter().find(|s| side == s).filter(|_| random)?;
        Some((side, names[at + 2..].iter().collect()))
    })
}

/// `path` from `repo`'s root, or none when it lies outside. A missing file resolves through
/// its nearest existing directory.
fn relative(repo: &dyn Repository, path: &Path) -> Option<Utf8PathBuf> {
    // Discovery canonicalises the root, so the path takes the same form to compare.
    let canonical = crate::resolve::absolute(path).ok()?;
    canonical.strip_prefix(repo.root()).ok().map(Utf8Path::to_owned)
}

/// The first of `commits` whose `path` holds `input`'s bytes.
fn first_exact(
    repo: &dyn Repository,
    path: &Utf8Path,
    input: &Text,
    commits: impl IntoIterator<Item = impl std::borrow::Borrow<CommitId>>,
) -> Result<Option<Source>, VcsError> {
    if !is_utf8(input) {
        return Ok(None);
    }
    let bytes = input.encode(input.content());
    for commit in commits {
        let commit = commit.borrow();
        match repo.file_at(path, &RevSpec::Commit(commit.clone())) {
            Ok(Some(held)) if held == *bytes => {
                return Ok(Some(Source {
                    path: path.to_owned(),
                    commit: commit.clone(),
                    fit: Fit::Exact,
                }));
            }
            // A conflicted file holds no one side's bytes.
            Ok(_) | Err(VcsError::Conflicted { .. }) => {}
            Err(VcsError::OutsideRepository(_)) => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

/// `input` annotated against `commit` by a line diff; none without a commit.
fn changed(path: &Utf8Path, input: &Text, commit: Option<CommitId>) -> Option<Source> {
    if !is_utf8(input) {
        return None;
    }
    commit.map(|commit| Source { path: path.to_owned(), commit, fit: Fit::Changed })
}

/// The one commit `rev` names; none when it names none or several.
fn single_commit(repo: &dyn Repository, rev: &str) -> Result<Option<CommitId>, VcsError> {
    match repo.commits(rev) {
        Ok(commits) => Ok(<[CommitId; 1]>::try_from(commits).ok().map(|[commit]| commit)),
        Err(VcsError::UnknownRevision(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn is_utf8(input: &Text) -> bool {
    matches!(input.encoding(), Encoding::Utf8 | Encoding::Utf8Bom)
}

/// `annotation`, of the text `from`, carried onto `to` by a byte-exact line diff: each line
/// `to` shares with `from` takes its origin; inserted and changed lines have none.
#[must_use]
pub fn carry(annotation: &Annotation, from: &Text, to: &Text) -> Annotation {
    let hunks = diff_lines(from, to, &DiffOptions::default());
    // The stretches both texts share: `from`'s lines, and where they start in `to`.
    let ends = hunks
        .iter()
        .map(|hunk| (hunk.before.start, hunk.before.end, hunk.after.end))
        .chain(std::iter::once((from.line_count(), 0, 0)));
    let mut shared = Vec::with_capacity(hunks.len() + 1);
    let (mut start, mut at) = (0, 0);
    for (end, next_start, next_at) in ends {
        if start < end {
            shared.push((start..end, at));
        }
        (start, at) = (next_start, next_at);
    }
    let runs = shared.into_iter().flat_map(|(lines, at)| {
        let first = annotation.runs.partition_point(|run| run.lines.end <= lines.start);
        annotation.runs[first..].iter().take_while(move |run| run.lines.start < lines.end).map(
            move |run| {
                let start = run.lines.start.max(lines.start);
                let end = run.lines.end.min(lines.end);
                let shift = |line: usize| line - lines.start + at;
                (shift(start)..shift(end), run.origin)
            },
        )
    });
    let Ok(carried) = Annotation::from_runs(runs, |&origin| {
        Ok::<_, Infallible>(annotation.origins[origin].clone())
    });
    carried
}

#[cfg(test)]
mod carry_tests {
    use merge_core::testing::text;
    use std::ops::Range;

    use vcs::{ChangeInfo, OriginRun, Timestamp};

    use super::*;

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

    /// Each run's lines and its commit.
    fn carried(annotation: &Annotation, from: &str, to: &str) -> Vec<(Range<usize>, String)> {
        let carried = carry(annotation, &text(from), &text(to));
        let commits: Vec<&str> = carried.origins.iter().map(|o| o.change.commit.as_str()).collect();
        let distinct = commits.len();
        let mut seen = commits.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), distinct, "each commit appears once");
        carried.runs.iter().map(|run| (run.lines.clone(), commits[run.origin].to_owned())).collect()
    }

    fn runs(expected: &[(Range<usize>, &str)]) -> Vec<(Range<usize>, String)> {
        expected.iter().map(|(lines, commit)| (lines.clone(), (*commit).to_owned())).collect()
    }

    #[test]
    fn carrying_onto_the_same_text_keeps_every_origin() {
        let from = annotation(&[(0..1, "c1"), (1..3, "c2")]);
        assert_eq!(carried(&from, "a\nb\nc\n", "a\nb\nc\n"), runs(&[(0..1, "c1"), (1..3, "c2")]));
    }

    #[test]
    fn an_inserted_line_has_no_origin_and_the_lines_after_it_move_down() {
        let from = annotation(&[(0..1, "c1"), (1..3, "c2")]);
        assert_eq!(
            carried(&from, "a\nb\nc\n", "a\nX\nb\nc\n"),
            runs(&[(0..1, "c1"), (2..4, "c2")])
        );
    }

    #[test]
    fn a_changed_line_loses_its_origin_even_for_whitespace() {
        let from = annotation(&[(0..1, "c1"), (1..3, "c2")]);
        assert_eq!(carried(&from, "a\nb\nc\n", "a\nb \nc\n"), runs(&[(0..1, "c1"), (2..3, "c2")]));
    }

    #[test]
    fn a_changed_line_splits_the_run_it_lay_in() {
        let from = annotation(&[(0..3, "c1")]);
        assert_eq!(carried(&from, "a\nb\nc\n", "a\nB\nc\n"), runs(&[(0..1, "c1"), (2..3, "c1")]));
    }

    #[test]
    fn a_deleted_line_joins_the_runs_around_it_when_they_share_a_commit() {
        let from = annotation(&[(0..1, "c1"), (1..2, "c2"), (2..3, "c1")]);
        assert_eq!(carried(&from, "a\nx\nb\n", "a\nb\n"), runs(&[(0..2, "c1")]));
    }

    #[test]
    fn a_commit_whose_lines_are_all_gone_is_dropped() {
        let from = annotation(&[(0..1, "c1"), (1..2, "c2")]);
        assert_eq!(carried(&from, "a\nb\n", "b\n"), runs(&[(0..1, "c2")]));
    }

    #[test]
    fn a_line_without_an_origin_stays_without_one() {
        let from = annotation(&[(0..1, "c1"), (2..3, "c1")]);
        assert_eq!(
            carried(&from, "a\nb\nc\n", "z\na\nb\nc\n"),
            runs(&[(1..2, "c1"), (3..4, "c1")])
        );
    }

    #[test]
    fn carrying_from_or_onto_an_empty_text_gives_no_origins() {
        let from = annotation(&[(0..2, "c1")]);
        assert_eq!(carried(&from, "a\nb\n", ""), runs(&[]));
        assert_eq!(carried(&Annotation::default(), "", "a\nb\n"), runs(&[]));
    }
}
