//! `absorb`: each hunk of the working-copy commit against its parents, with the ancestor it
//! goes into, moved there on request (`DESIGN.md` §I5).

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use camino::Utf8PathBuf;
use futures::StreamExt as _;
use jj_lib::absorb::{AbsorbSource, absorb_hunks};
use jj_lib::annotate::FileAnnotator;
use jj_lib::backend::{CommitId, CopyId, TreeValue};
use jj_lib::commit::Commit;
use jj_lib::diff::{ContentDiff, DiffHunkKind};
use jj_lib::files::try_merge;
use jj_lib::matchers::{EverythingMatcher, Matcher, PrefixMatcher};
use jj_lib::merge::Merge;
use jj_lib::merged_tree_builder::MergedTreeBuilder;
use jj_lib::object_id::ObjectId as _;
use jj_lib::repo::{MutableRepo, Repo};
use jj_lib::repo_path::RepoPathBuf;
use pollster::FutureExt as _;
use vcs::{ChangeInfo, VcsError};

use crate::native::{Mutated, Rewrite};
use crate::{JjError, JjRepository, backend};

/// The commits a hunk may go into without `--into`: the mutable ancestors of the working
/// copy's parents.
pub const DEFAULT_INTO: &str = "mutable()";

/// One hunk of the working-copy commit against its parents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsorbHunk {
    /// The hunk's number, from 1, in path order then line order.
    pub number: usize,
    pub path: Utf8PathBuf,
    /// The lines it replaces in the parents' file, from 0, end excluded.
    pub left: Range<usize>,
    /// The lines it holds in the working copy's file.
    pub right: Range<usize>,
    /// The commit it goes into, or why it stays in the working copy.
    pub goes: Goes,
    /// The lines it replaces, as the parents' file holds them.
    pub before: Vec<u8>,
    /// The lines it holds, as the working copy's file holds them.
    pub after: Vec<u8>,
}

impl AbsorbHunk {
    /// The commit it goes into; none when it stays.
    #[must_use]
    pub fn target(&self) -> Option<&ChangeInfo> {
        match &self.goes {
            Goes::Into(change) => Some(change),
            Goes::Stays(_) => None,
        }
    }
}

/// Where a hunk goes: `C` names a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Goes<C = ChangeInfo> {
    Into(C),
    Stays(Stay<C>),
}

/// Why a hunk stays in the working copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stay<C = ChangeInfo> {
    /// No one commit in `--into` last changed every line it replaces and holds the file: its
    /// lines come from several commits, from none, or from a commit outside `--into`; or it
    /// is an insertion where two commits' lines meet.
    NoCommit,
    /// The commit that wrote its lines would take it with a conflict, as `jj absorb` writes
    /// it (`DESIGN.md` §I5).
    WouldConflict(C),
    /// `--keep` chose it.
    Kept,
}

impl<C> Goes<C> {
    /// The same choice with each commit named by `name`.
    fn named<D, E>(self, mut name: impl FnMut(C) -> Result<D, E>) -> Result<Goes<D>, E> {
        Ok(match self {
            Self::Into(commit) => Goes::Into(name(commit)?),
            Self::Stays(Stay::NoCommit) => Goes::Stays(Stay::NoCommit),
            Self::Stays(Stay::WouldConflict(commit)) => {
                Goes::Stays(Stay::WouldConflict(name(commit)?))
            }
            Self::Stays(Stay::Kept) => Goes::Stays(Stay::Kept),
        })
    }
}

/// A commit a hunk may go into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// The commit's full ID.
    pub id: vcs::CommitId,
    pub change: ChangeInfo,
}

/// A choice that overrides where a hunk goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkChoice {
    /// Send hunk `hunk` into the one commit `rev` names.
    Move { hunk: usize, rev: String },
    /// Keep hunk `hunk` in the working copy.
    Keep(usize),
}

/// What `absorb` found, and on `--apply` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Absorbed {
    /// The full ID of the operation the hunks were read at, after the snapshot. `--apply`
    /// with choices names it, so the hunk numbers mean what the listing showed.
    pub operation: String,
    pub hunks: Vec<AbsorbHunk>,
    /// The commits a hunk may go into: the mutable ancestors of the working copy's parents
    /// that `--into` names, children first.
    pub destinations: Vec<Destination>,
    /// Paths it cannot split into hunks, with the reason: a conflict, or a symlink or
    /// submodule on either side.
    pub skipped: Skipped,
    pub rewrite: Rewrite,
}

/// Paths `absorb` cannot split, each with the reason.
type Skipped = Vec<(Utf8PathBuf, String)>;

/// A hunk's byte range in the parents' file and in the working copy's.
type RangePair = (Range<usize>, Range<usize>);

/// A hunk with what it takes to write it.
struct Planned {
    path: RepoPathBuf,
    /// The index into `files`.
    file: usize,
    left: Range<usize>,
    right: Range<usize>,
    goes: Goes<CommitId>,
}

/// One changed file: the parents' text and the working copy's.
struct ChangedFile {
    path: RepoPathBuf,
    left: Vec<u8>,
    right: Vec<u8>,
    deleted: bool,
    executable: bool,
    copy_id: CopyId,
}

impl JjRepository {
    /// Split the working-copy commit's changes to `paths`, all without any, into hunks
    /// against its parents, each with the commit it goes into: the one commit in `into`,
    /// default [`DEFAULT_INTO`], among the parents' ancestors that last changed every line
    /// the hunk replaces, or for an insertion the commit holding the lines on both sides.
    /// `choices` override. With `apply`, each commit takes its hunks, the descendants
    /// rebase, and the working copy keeps the rest, in one operation; else nothing is
    /// written. `listed_at` names the operation a listing read: the hunks are read only
    /// when the repository is still at it after the snapshot.
    ///
    /// # Errors
    ///
    /// [`VcsError::Backend`] holding [`JjError::NoHunk`] for a choice of a hunk that does
    /// not exist, [`JjError::NotADestination`] for a move to a commit outside `into`,
    /// [`JjError::UnknownOperation`] for a `listed_at` jj cannot resolve and
    /// [`JjError::RepositoryMoved`] when the repository has moved past it;
    /// [`VcsError::Immutable`]; and the errors of a write ([`JjRepository::write_conflict`]).
    pub fn absorb(
        &mut self,
        paths: &[Utf8PathBuf],
        into: Option<&str>,
        choices: &[HunkChoice],
        apply: bool,
        listed_at: Option<&str>,
    ) -> Result<Absorbed, VcsError> {
        let repo_paths = paths
            .iter()
            .map(|path| {
                RepoPathBuf::from_relative_path(path)
                    .map_err(|_| VcsError::OutsideRepository(path.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let into = into.unwrap_or(DEFAULT_INTO);
        let revsets = self.revsets()?;
        let mut found = None;
        let rewrite = self.rewrite(&revsets, !apply, |this, repo| {
            if let Some(op) = listed_at {
                this.check_at_operation(op)?;
            }
            let operation = this.repo.op_id().hex();
            let source = this.working_copy_in(repo)?;
            let parents: Vec<String> = source.parent_ids().iter().map(CommitId::hex).collect();
            let expr = format!("({into}) & ::({})", parents.join(" | "));
            let destinations = this.evaluate(repo, &expr, &revsets)?;
            let domain = this.resolve_expression(repo, &expr, &revsets)?.into_inner().0;
            let parent_tree = source
                .parent_tree(repo.index())
                .block_on()
                .map_err(backend("merging the parents"))?;
            let matcher: Box<dyn Matcher> = if repo_paths.is_empty() {
                Box::new(EverythingMatcher)
            } else {
                Box::new(PrefixMatcher::new(&repo_paths))
            };
            let (files, skipped) =
                this.changed_files(&parent_tree, &source.tree(), matcher.as_ref())?;
            let mut planned = this.plan(repo, &source, &files, &domain)?;
            for choice in choices {
                let (number, target) = this.chosen(repo, choice, &destinations, &revsets)?;
                let count = planned.len();
                let hunk =
                    number.checked_sub(1).and_then(|index| planned.get_mut(index)).ok_or_else(
                        || backend("choosing a hunk")(JjError::NoHunk { hunk: number, count }),
                    )?;
                hunk.goes = target;
            }
            let targets: BTreeSet<&CommitId> =
                planned.iter().filter_map(|hunk| destination(&hunk.goes)).collect();
            let mut roots = vec![source.clone()];
            for id in targets {
                roots.push(repo.store().get_commit(id).map_err(backend("reading a commit"))?);
            }
            // A listing writes nothing to the store: the trees are built only to apply.
            if apply {
                write_hunks(repo, &source, &parent_tree, &files, &planned)?;
            }
            let hunks = planned
                .iter()
                .enumerate()
                .map(|(index, hunk)| {
                    let file = &files[hunk.file];
                    let goes = hunk.goes.clone().named(|id| {
                        let commit =
                            repo.store().get_commit(&id).map_err(backend("reading a commit"))?;
                        this.change_info(&commit)
                    })?;
                    Ok(AbsorbHunk {
                        number: index + 1,
                        path: Utf8PathBuf::from(hunk.path.as_internal_file_string()),
                        left: lines(&file.left, &hunk.left),
                        right: lines(&file.right, &hunk.right),
                        goes,
                        before: file.left[hunk.left.clone()].to_vec(),
                        after: file.right[hunk.right.clone()].to_vec(),
                    })
                })
                .collect::<Result<Vec<_>, VcsError>>()?;
            let destinations = destinations
                .iter()
                .map(|commit| {
                    Ok(Destination {
                        id: vcs::CommitId::new(commit.id().hex()),
                        change: this.change_info(commit)?,
                    })
                })
                .collect::<Result<Vec<_>, VcsError>>()?;
            found = Some((operation, hunks, destinations, skipped));
            Ok(Mutated {
                description: "absorb in triad".to_owned(),
                target: source,
                path: None,
                roots,
            })
        })?;
        let (operation, hunks, destinations, skipped) = found.unwrap_or_default();
        Ok(Absorbed { operation, hunks, destinations, skipped, rewrite })
    }

    /// The hunk `choice` names and where it now goes.
    fn chosen(
        &self,
        repo: &MutableRepo,
        choice: &HunkChoice,
        destinations: &[Commit],
        revsets: &crate::Revsets,
    ) -> Result<(usize, Goes<CommitId>), VcsError> {
        match choice {
            HunkChoice::Keep(number) => Ok((*number, Goes::Stays(Stay::Kept))),
            HunkChoice::Move { hunk, rev } => {
                let commit = match <[Commit; 1]>::try_from(self.evaluate(repo, rev, revsets)?) {
                    Ok([commit]) => commit,
                    Err(found) if found.is_empty() => {
                        return Err(VcsError::UnknownRevision(rev.clone()));
                    }
                    Err(_) => return Err(VcsError::AmbiguousRevision(rev.clone())),
                };
                if !destinations.iter().any(|d| d.id() == commit.id()) {
                    return Err(backend("choosing a hunk's commit")(JjError::NotADestination(
                        rev.clone(),
                    )));
                }
                Ok((*hunk, Goes::Into(commit.id().clone())))
            }
        }
    }

    /// Each file's hunks, each with the commit its annotation over `domain` gives, kept
    /// only where jj's merge takes the hunk into that commit's file without a conflict.
    fn plan(
        &self,
        repo: &MutableRepo,
        source: &Commit,
        files: &[ChangedFile],
        domain: &std::sync::Arc<jj_lib::revset::ResolvedRevsetExpression>,
    ) -> Result<Vec<Planned>, VcsError> {
        let mut planned = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let mut annotator =
                FileAnnotator::with_file_content(source, &file.path, file.left.clone());
            annotator.compute(repo, domain).block_on().map_err(backend("annotating a file"))?;
            let annotation = annotator.to_annotation();
            // A line no commit in the domain wrote is the source's own when a commit outside
            // the domain sits between them: it has no commit to go into either.
            let ranges: Vec<(CommitId, Range<usize>)> = annotation
                .compact_line_ranges()
                .filter_map(|(commit, range)| Some((commit.ok()?.id().clone(), range)))
                .filter(|(commit, _)| commit != source.id())
                .collect();
            // Each target's text of the file, read once; none when it holds no plain file.
            let mut texts: BTreeMap<CommitId, Option<Vec<u8>>> = BTreeMap::new();
            // The hunks of the file each target takes so far, in line order: a hunk goes in
            // only when jj's merge takes it together with them.
            let mut taken: BTreeMap<CommitId, Vec<RangePair>> = BTreeMap::new();
            let diff = ContentDiff::by_line([&file.left, &file.right]);
            for hunk in diff.hunk_ranges().filter(|hunk| hunk.kind == DiffHunkKind::Different) {
                let (left, right) = (hunk.ranges[0].clone(), hunk.ranges[1].clone());
                let goes = match target(&ranges, &left) {
                    Some(commit) => {
                        let text = match texts.entry(commit.clone()) {
                            Entry::Occupied(found) => found.into_mut(),
                            Entry::Vacant(slot) => {
                                slot.insert(self.text_in(repo, &commit, &file.path)?)
                            }
                        };
                        match text.as_deref() {
                            Some(text) => {
                                let ranges = taken.entry(commit.clone()).or_default();
                                ranges.push((left.clone(), right.clone()));
                                if merges_cleanly(repo, text, file, ranges) {
                                    Goes::Into(commit)
                                } else {
                                    ranges.pop();
                                    Goes::Stays(Stay::WouldConflict(commit))
                                }
                            }
                            None => Goes::Stays(Stay::NoCommit),
                        }
                    }
                    None => Goes::Stays(Stay::NoCommit),
                };
                planned.push(Planned { path: file.path.clone(), file: index, goes, left, right });
            }
        }
        Ok(planned)
    }

    /// The file at `path` in commit `id`; none when the commit holds no plain file there.
    fn text_in(
        &self,
        repo: &MutableRepo,
        id: &CommitId,
        path: &RepoPathBuf,
    ) -> Result<Option<Vec<u8>>, VcsError> {
        let commit = repo.store().get_commit(id).map_err(backend("reading a commit"))?;
        let value = commit
            .tree()
            .path_value(path)
            .block_on()
            .map_err(backend("finding a path in a tree"))?;
        match value.as_resolved() {
            Some(Some(TreeValue::File { id, .. })) => self.read_file(path, id).map(Some),
            _ => Ok(None),
        }
    }

    /// The working-copy commit in `repo`.
    fn working_copy_in(&self, repo: &MutableRepo) -> Result<Commit, VcsError> {
        let id = repo
            .view()
            .get_wc_commit_id(&self.workspace)
            .ok_or_else(|| VcsError::NoWorkingCopy(self.root.clone()))?;
        repo.store().get_commit(id).map_err(backend("reading the working-copy commit"))
    }

    /// The files `matcher` takes that changed from `parent` to `source` and were files on
    /// both sides, or deleted; and the paths skipped, with the reason. A file new in the
    /// working copy has no commit to go into, and stays.
    fn changed_files(
        &self,
        parent: &jj_lib::merged_tree::MergedTree,
        source: &jj_lib::merged_tree::MergedTree,
        matcher: &dyn Matcher,
    ) -> Result<(Vec<ChangedFile>, Skipped), VcsError> {
        let mut files = Vec::new();
        let mut skipped = Vec::new();
        let mut stream = parent.diff(source, matcher);
        while let Some(entry) = stream.next().block_on() {
            let values = entry.values.map_err(backend("comparing the trees"))?;
            let shown = || Utf8PathBuf::from(entry.path.as_internal_file_string());
            let (left_id, executable, copy_id) = match values.before.as_resolved() {
                Some(None) => continue,
                Some(Some(TreeValue::File { id, executable, copy_id })) => {
                    (id.clone(), *executable, copy_id.clone())
                }
                Some(Some(_)) => {
                    skipped.push((shown(), "not a file in the parents".to_owned()));
                    continue;
                }
                None => {
                    skipped.push((shown(), "conflicted in the parents".to_owned()));
                    continue;
                }
            };
            let (right, deleted) = match values.after.as_resolved() {
                Some(None) => (Vec::new(), true),
                Some(Some(TreeValue::File { id, .. })) => (self.read_file(&entry.path, id)?, false),
                Some(Some(_)) => {
                    skipped.push((shown(), "not a file in the working copy".to_owned()));
                    continue;
                }
                None => {
                    skipped.push((shown(), "conflicted in the working copy".to_owned()));
                    continue;
                }
            };
            let left = self.read_file(&entry.path, &left_id)?;
            files.push(ChangedFile { path: entry.path, left, right, deleted, executable, copy_id });
        }
        Ok((files, skipped))
    }
}

/// jj's merge takes the hunks `ranges`, each replacing its left range of `file`'s parents'
/// text with its right range of the working-copy text, into `target`, the file in the
/// hunks' commit, without a conflict, as `absorb_hunks` merges them: the commit's text, less
/// the parents', plus the hunks.
fn merges_cleanly(
    repo: &MutableRepo,
    target: &[u8],
    file: &ChangedFile,
    ranges: &[RangePair],
) -> bool {
    let applied = combine(&file.left, &file.right, ranges);
    let inputs = Merge::from_vec(vec![target, file.left.as_slice(), applied.as_slice()]);
    try_merge(&inputs, repo.store().merge_options()).is_some()
}

/// The commit `goes` sends a hunk into; none when it stays.
fn destination(goes: &Goes<CommitId>) -> Option<&CommitId> {
    match goes {
        Goes::Into(commit) => Some(commit),
        Goes::Stays(_) => None,
    }
}

/// Move each planned hunk into its target, rebase the descendants, and leave the rest in
/// `source`, as jj's `absorb` writes.
fn write_hunks(
    repo: &mut MutableRepo,
    source: &Commit,
    parent_tree: &jj_lib::merged_tree::MergedTree,
    files: &[ChangedFile],
    planned: &[Planned],
) -> Result<(), VcsError> {
    let trees = target_trees(repo, files, planned, parent_tree)?;
    if trees.is_empty() {
        return Ok(());
    }
    let absorb_source = AbsorbSource::from_tree(source.clone(), source.tree(), parent_tree.clone())
        .block_on()
        .map_err(backend("reading the working copy's parents"))?;
    absorb_hunks(repo, &absorb_source, trees.into_iter().collect())
        .block_on()
        .map_err(backend("absorbing the hunks"))?;
    Ok(())
}

/// For each commit a hunk goes into, the parents' tree with that commit's hunks applied.
fn target_trees(
    repo: &MutableRepo,
    files: &[ChangedFile],
    planned: &[Planned],
    parent_tree: &jj_lib::merged_tree::MergedTree,
) -> Result<BTreeMap<CommitId, MergedTreeBuilder>, VcsError> {
    let mut selected: BTreeMap<(CommitId, usize), Vec<RangePair>> = BTreeMap::new();
    for hunk in planned {
        if let Some(target) = destination(&hunk.goes) {
            selected
                .entry((target.clone(), hunk.file))
                .or_default()
                .push((hunk.left.clone(), hunk.right.clone()));
        }
    }
    let mut trees: BTreeMap<CommitId, MergedTreeBuilder> = BTreeMap::new();
    for ((commit, index), ranges) in selected {
        let file = &files[index];
        let text = combine(&file.left, &file.right, &ranges);
        let value = if text.is_empty() && file.deleted {
            None
        } else {
            let id = repo
                .store()
                .write_file(&file.path, &mut text.as_slice())
                .block_on()
                .map_err(backend("writing a hunk's file"))?;
            Some(TreeValue::File { id, executable: file.executable, copy_id: file.copy_id.clone() })
        };
        trees
            .entry(commit)
            .or_insert_with(|| MergedTreeBuilder::new(parent_tree.clone()))
            .set_or_remove(file.path.clone(), value);
    }
    Ok(trees)
}

/// The commit a hunk replacing `left` goes into, from the parents' file's annotation
/// `ranges`, compacted: the one range that holds `left`; for an insertion, the range it
/// sits in or ends, unless another range starts where it sits.
fn target(ranges: &[(CommitId, Range<usize>)], left: &Range<usize>) -> Option<CommitId> {
    let at = ranges.iter().position(|(_, range)| range.end >= left.end)?;
    let (commit, range) = &ranges[at];
    let between = left.is_empty()
        && range.end == left.start
        && ranges.get(at + 1).is_some_and(|(_, next)| next.start == left.start);
    (range.start <= left.start && !between).then(|| commit.clone())
}

/// `left` with each `(left, right)` byte range pair replaced by `right`'s bytes.
fn combine(left: &[u8], right: &[u8], ranges: &[RangePair]) -> Vec<u8> {
    let mut text = Vec::with_capacity(right.len());
    let mut at = 0;
    for (from, to) in ranges {
        text.extend_from_slice(&left[at..from.start]);
        text.extend_from_slice(&right[to.clone()]);
        at = from.end;
    }
    text.extend_from_slice(&left[at..]);
    text
}

/// The line range, from 0, of the byte range `bytes` of `text`, which starts and ends at
/// line starts.
#[expect(
    clippy::naive_bytecount,
    reason = "once per hunk in the report, over one file; not worth a dependency"
)]
fn lines(text: &[u8], bytes: &Range<usize>) -> Range<usize> {
    let line = |end: usize| text[..end].iter().filter(|&&byte| byte == b'\n').count();
    line(bytes.start)..line(bytes.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> CommitId {
        CommitId::new(vec![byte])
    }

    #[test]
    fn a_hunk_inside_one_range_goes_to_its_commit_and_one_across_two_stays() {
        let ranges = [(id(1), 0..10), (id(2), 10..20)];
        assert_eq!(target(&ranges, &(2..4)), Some(id(1)));
        assert_eq!(target(&ranges, &(12..20)), Some(id(2)));
        assert_eq!(target(&ranges, &(8..12)), None);
    }

    #[test]
    fn an_insertion_goes_to_the_range_around_it_and_between_two_stays() {
        let ranges = [(id(1), 0..10), (id(2), 10..20)];
        assert_eq!(target(&ranges, &(4..4)), Some(id(1)));
        assert_eq!(target(&ranges, &(10..10)), None);
        assert_eq!(target(&ranges, &(20..20)), Some(id(2)));
        assert_eq!(target(&ranges, &(0..0)), Some(id(1)));
    }

    #[test]
    fn combine_replaces_each_selected_range() {
        assert_eq!(combine(b"abcdef", b"aXcdYf", &[(1..2, 1..2), (4..5, 4..5)]), b"aXcdYf");
        assert_eq!(combine(b"abcdef", b"aXcdYf", &[(4..5, 4..5)]), b"abcdYf");
        assert_eq!(combine(b"ab", b"aZZb", &[(1..1, 1..3)]), b"aZZb");
    }

    #[test]
    fn lines_count_newlines_before_each_end() {
        assert_eq!(lines(b"a\nb\nc\n", &(2..4)), 1..2);
        assert_eq!(lines(b"a\nb\nc\n", &(6..6)), 3..3);
    }
}
