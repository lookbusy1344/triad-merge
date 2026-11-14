//! Every string the window shows, in one place for later translation (spec §6.5).

use gpui_kit::SharedString;
use merge_core::diff::{CaseRule, Comparison, LineEndingRule, Whitespace};
use merge_core::diff2::Side;
use merge_core::index::RegionIx;
use merge_core::merge3::{HunkLevel, Semantics};
use merge_core::text::{Encoding, LineEnding};
use merge_session::ExitCode;
use merge_session::jj_native::{
    AbsorbHunk, Absorbed, CommitEffect, FileVersion, Goes, Rewrite, SHORT_OPERATION, StackCommit,
    StackPath, Stay, VersionState, stays,
};
use merge_view::nav::Sides;
use merge_view::status::{Divergence, Status};
use vcs::{ChangeInfo, IdKind, RepoKind};

use crate::choice::{Choice, MenuAction};
use crate::open_dialog::{Mode, Slot};
use crate::pick_list::Row;
use crate::toolbar::Tool;

pub(crate) const APP_NAME: &str = "triad";
/// The About window's title and its menu item.
pub(crate) const ABOUT_TITLE: &str = "About triad";
/// The About window's line under the version.
pub(crate) const DESCRIPTION: &str = "Three-way merge and two-way diff";
/// The About window's line under the description.
pub(crate) const VCS_SUPPORT: &str = "Built for Jujutsu, with full Git support";
/// The prefix of every message on stderr, as `merge-cli` writes them.
pub(crate) const PROGRAM: &str = "triad-merge";
/// The prefix of the measurement lines on stderr (DESIGN.md §A6.7).
pub(crate) const MEASURE: &str = "triad-merge measure";
pub(crate) const RESULT_TITLE: &str = "Result";
pub(crate) const TOOLBAR: &str = "Merge tools";
pub(crate) const DIFF_TOOLBAR: &str = "Diff tools";
/// The accessible label of the merge window's aligned input lines.
pub(crate) const INPUT_LINES: &str = "Input lines";
pub(crate) const GO_TO_LINE_FIELD: &str = "Line number";
/// An input pane's count when the result's search matches nothing in it (spec §6.3).
pub(crate) const NO_MATCHES: &str = "No matches";
/// The settings banner's close button.
pub(crate) const CLOSE: &str = "Close";

/// The About window's version line.
pub(crate) fn version() -> SharedString {
    format!("Version {}", env!("CARGO_PKG_VERSION")).into()
}

/// The About window's line naming the source of the build. `script/build-id` sets both
/// variables for release builds; rustc rebuilds the crate when they change.
pub(crate) fn build_of_this_binary() -> Option<SharedString> {
    let profile = if cfg!(debug_assertions) { Profile::Debug } else { Profile::Release };
    build(option_env!("TRIAD_COMMIT"), option_env!("TRIAD_CHANGE"), profile)
}

/// The commit, then the jj change; an empty value counts as none. Without a commit, a release
/// build has no line and a debug build shows the null ids, so the window keeps its release
/// layout.
pub(crate) fn build(
    commit: Option<&str>,
    change: Option<&str>,
    profile: Profile,
) -> Option<SharedString> {
    match (commit.filter(|id| !id.is_empty()), profile) {
        (Some(commit), _) => Some(match change.filter(|id| !id.is_empty()) {
            Some(change) => format!("Commit {commit}, change {change}").into(),
            None => format!("Commit {commit}").into(),
        }),
        (None, Profile::Debug) => {
            Some(format!("Commit {NULL_COMMIT}, change {ROOT_CHANGE}").into())
        }
        (None, Profile::Release) => None,
    }
}

/// git's null object id, cut to the build line's 12 characters.
const NULL_COMMIT: &str = "000000000000";
/// jj's root change id, cut to the build line's 12 characters.
const ROOT_CHANGE: &str = "zzzzzzzzzzzz";

/// The build profile, which decides whether a build without a commit shows placeholder ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Profile {
    Debug,
    Release,
}

/// The settings banner's accessible label: what it is, then its errors.
pub(crate) fn settings_errors_label(errors: &str) -> String {
    format!("The config file has errors: {}", errors.replace('\n', "; "))
}

/// The confirmations and alerts of Save & Exit and Cancel (spec §6.2). The first answer
/// of each goes ahead.
pub(crate) mod prompt {
    use merge_core::merge3::Semantics;

    pub(crate) const SAVE_UNRESOLVED: &str = "Save with unresolved conflicts?";
    pub(crate) const SAVE_WITH_CONFLICTS: &str = "Save with Conflicts";
    pub(crate) const KEEP_EDITING: &str = "Keep Editing";
    pub(crate) const SAVE_AND_EXIT: &str = super::menu::SAVE_AND_EXIT;
    pub(crate) const DISCARD_DETAIL: &str =
        "Discard writes nothing, and the merge stays unresolved in git or jj.";
    pub(crate) const DISCARD: &str = "Discard";
    pub(crate) const CANNOT_WRITE: &str = "Cannot write the output";
    pub(crate) const CANNOT_WRITE_COPY: &str = "Cannot write the copy";
    pub(crate) const CANNOT_RECORD: &str = "The repository did not record the save";
    pub(crate) const NO_OUTPUT: &str = "This window has no output file.";
    pub(crate) const QUIT: &str = "Quit";
    pub(crate) const TRY_AGAIN: &str = "Try Again";
    pub(crate) const SAVE_COPY_AS: &str = "Save Copy As…";
    pub(crate) const RESTORE_SESSION: &str = "Restore the previous session?";
    pub(crate) const RESTORE_DETAIL: &str = "A session on these files ended before its merge was resolved. Its picks and edits can \
         come back.";
    pub(crate) const RESTORE: &str = "Restore";
    pub(crate) const START_OVER: &str = "Start Over";
    pub(crate) const CANNOT_RESTORE: &str = "Cannot restore the session";
    pub(crate) const SAVE_CHANGES: &str = "Save your changes?";
    pub(crate) const SAVE: &str = "Save";
    pub(crate) const CANNOT_SAVE: &str = "Cannot save the file";
    pub(crate) const OK: &str = "OK";
    pub(crate) const CHANGED_DETAIL: &str =
        "Reload it and lose your edits, or keep your version and save over it later.";
    pub(crate) const RELOAD: &str = "Reload";
    pub(crate) const KEEP_MINE: &str = "Keep Mine";

    /// The question before a save into a jj commit that changes the conflicts of
    /// `descendants` of its descendants (spec §7.5.1).
    pub(crate) fn descendants_changed(descendants: usize) -> String {
        match descendants {
            1 => "Saving changes the conflicts of 1 descendant.".to_owned(),
            n => format!("Saving changes the conflicts of {n} descendants."),
        }
    }

    /// The question when a file with unsaved edits changes on disk.
    pub(crate) fn changed_on_disk(name: &str) -> String {
        format!("{name} changed on disk.")
    }

    /// What saving with `unresolved` conflicts left does. Under git, `git mergetool` then
    /// puts back its own file, and the recovery journal holds the work (spec §6.2).
    pub(crate) fn save_unresolved_detail(unresolved: usize, semantics: Semantics) -> String {
        let conflicts = if unresolved == 1 {
            "1 conflict keeps its"
        } else {
            &format!("{unresolved} conflicts keep their")
        };
        let detail = format!("{conflicts} markers, and git or jj sees the merge as unresolved.");
        match semantics {
            Semantics::Git => format!(
                "{detail} git mergetool then puts back its own copy of the file; run it again \
                 to restore this session."
            ),
            Semantics::Jj => detail,
        }
    }
}

/// Each descendant's change ID and its conflicted paths before and after a rewrite, one line
/// each, as `resolve -r --headless` reports them: `abc12345: a.txt, b.txt -> none`.
pub(crate) fn descendant_effects<'a>(
    effects: impl IntoIterator<Item = &'a CommitEffect>,
) -> String {
    effects
        .into_iter()
        .map(|effect| {
            let change = effect.change.id(IdKind::Change);
            format!("{change}: {} -> {}", path_list(&effect.before), path_list(&effect.after))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `paths` joined by commas, or [`NO_PATHS`].
fn path_list(paths: &[impl AsRef<str>]) -> String {
    if paths.is_empty() {
        NO_PATHS.to_owned()
    } else {
        paths.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(", ")
    }
}

/// A descendant's conflicted paths when it has none.
const NO_PATHS: &str = "none";

/// The stack window's title (spec §7.5.1).
pub(crate) const STACK_TITLE: &str = "Conflicted commits";
pub(crate) const STACK_EMPTY: &str = "No conflicted commits.";
pub(crate) const HISTORY_EMPTY: &str = "No versions.";
/// The stack and history windows' button that lists again.
pub(crate) const REFRESH: &str = "Refresh";
pub(crate) const STACK_PREVIEW: &str = "Preview Save";
pub(crate) const PREVIEW_RUNNING: &str = "Working out what the save would do…";

/// A change's IDs, as the lists print them: change, then commit.
fn change_ids(change: &ChangeInfo) -> SharedString {
    format!("{} {}", change.id(IdKind::Change), change.id(IdKind::Commit)).into()
}

/// A listed commit's heading, as `stack --list` prints it: its change, commit and summary.
pub(crate) fn stack_commit(commit: &StackCommit) -> Row {
    Row {
        lead: Some(change_ids(&commit.change)),
        text: commit.change.summary.clone().unwrap_or_default().into(),
        trail: None,
    }
}

/// A listed path, marked when a parent holds a conflict there too.
pub(crate) fn stack_path(path: &StackPath) -> Row {
    Row {
        lead: None,
        text: path.path.to_string().into(),
        trail: path.inherited.then_some(SharedString::new_static("(inherited)")),
    }
}

/// What the stack window says after the merge window on `path` in `change` finished with
/// `code`; nothing after a cancel.
pub(crate) fn stack_note(code: ExitCode, path: &str, change: &str) -> Option<String> {
    match code {
        ExitCode::Resolved => Some(format!("Resolved {path} in {change}.")),
        ExitCode::Conflicts => Some(format!("Saved {path} in {change} with conflicts.")),
        ExitCode::Cancelled => None,
        ExitCode::Error => Some(format!("The merge of {path} in {change} ended with an error.")),
    }
}

/// What saving the open merge window would do: whether the path resolves, and each
/// descendant's conflicts before and after.
pub(crate) fn stack_preview(rewrite: &Rewrite) -> String {
    let change = rewrite.target.id(IdKind::Change);
    let outcome = if rewrite.resolved {
        format!("Saving resolves the path in {change}.")
    } else {
        format!("Saving keeps conflicts in {change}.")
    };
    if rewrite.rewritten.is_empty() {
        format!("{outcome}\nNo descendant is rebased.")
    } else {
        format!("{outcome}\n{}", descendant_effects(&rewrite.rewritten))
    }
}

/// The history window's title (spec §7.5.2).
pub(crate) fn history_title(path: &str) -> String {
    format!("History of {path}")
}

/// A listed version, as `history --list` prints it: the operation where it appeared, the
/// time, the description, and a mark when the path holds no file there.
pub(crate) fn history_version(version: &FileVersion) -> Row {
    let short: String = version.operation.chars().take(SHORT_OPERATION).collect();
    let mark = match version.state {
        VersionState::File => None,
        VersionState::Conflict => Some("(conflict)"),
        VersionState::Absent => Some("(absent)"),
        VersionState::Other => Some("(other)"),
    };
    Row {
        lead: Some(format!("{short} {}", version.time).into()),
        text: version.description.clone().into(),
        trail: mark.map(SharedString::new_static),
    }
}

/// The absorb window's title (spec §7.5.3).
pub(crate) const ABSORB_TITLE: &str = "Absorb";
pub(crate) const ABSORB_EMPTY: &str = "Nothing to absorb.";
pub(crate) const ABSORB_APPLY: &str = "Apply";
/// The choice that keeps a hunk in the working copy.
const ABSORB_KEEP: &str = "Working copy";

/// A listed hunk, as `absorb --list` prints it, with the commit chosen for it now: the
/// listing's reason when it stays as listed, `kept` when the choice keeps a hunk the listing
/// moved.
pub(crate) fn absorb_hunk(hunk: &AbsorbHunk, target: Option<&ChangeInfo>) -> Row {
    let goes = match (target, &hunk.goes) {
        (Some(t), _) => format!("-> {} {}", t.id(IdKind::Change), t.id(IdKind::Commit)),
        (None, Goes::Stays(stay)) => stays(stay),
        (None, Goes::Into(_)) => stays(&Stay::Kept),
    };
    let (left, right) = (&hunk.left, &hunk.right);
    Row {
        lead: Some(hunk.number.to_string().into()),
        text: hunk.path.to_string().into(),
        trail: Some(
            format!(
                "left {}..{} right {}..{} {goes}",
                left.start, left.end, right.start, right.end
            )
            .into(),
        ),
    }
}

/// A hunk's lines, each with the side it is on: those it replaces, each after `- `, on the
/// left, then those it holds, after `+ `, on the right.
pub(crate) fn absorb_lines(hunk: &AbsorbHunk) -> Vec<(Side, String)> {
    let marked = |bytes: &[u8], side: Side, mark: &'static str| {
        String::from_utf8_lossy(bytes)
            .lines()
            .map(|line| (side, format!("{mark}{line}")))
            .collect::<Vec<_>>()
    };
    [marked(&hunk.before, Side::Left, "- "), marked(&hunk.after, Side::Right, "+ ")].concat()
}

/// A commit a hunk may go into, with the number of hunks going there when any.
pub(crate) fn absorb_destination(change: &ChangeInfo, hunks: usize) -> Row {
    Row {
        lead: Some(change_ids(change)),
        text: change.summary.clone().unwrap_or_default().into(),
        trail: with_hunks(hunks),
    }
}

/// The working copy as a choice, with the number of hunks kept when any.
pub(crate) fn absorb_keep(hunks: usize) -> Row {
    Row { lead: None, text: ABSORB_KEEP.into(), trail: with_hunks(hunks) }
}

fn with_hunks(hunks: usize) -> Option<SharedString> {
    (hunks > 0).then(|| format!("· {}", count(hunks, "hunk", "hunks")).into())
}

/// What Apply did: the hunks moved and the commits they went into, then each commit whose
/// conflicts changed.
pub(crate) fn absorb_note(absorbed: &Absorbed) -> String {
    let moved: Vec<&str> =
        absorbed.hunks.iter().filter_map(AbsorbHunk::target).map(|t| t.commit.as_str()).collect();
    let mut commits = moved.clone();
    commits.sort_unstable();
    commits.dedup();
    let summary = format!(
        "Absorbed {} into {}.",
        count(moved.len(), "hunk", "hunks"),
        count(commits.len(), "commit", "commits")
    );
    let mut changed =
        absorbed.rewrite.rewritten.iter().filter(|effect| effect.before != effect.after).peekable();
    let note = if changed.peek().is_none() {
        summary
    } else {
        format!("{summary}\n{}", descendant_effects(changed))
    };
    if absorbed.rewrite.stale { format!("{note}\n{STALE_WORKING_COPY}") } else { note }
}

/// A written rewrite whose working copy could not be checked out.
const STALE_WORKING_COPY: &str = "The working copy is stale: run jj workspace update-stale.";

/// A path absorb cannot split, and why.
pub(crate) fn absorb_skipped(path: &str, reason: &str) -> String {
    format!("Skipped {path}: {reason}")
}

/// The open dialog's window title on `mode`'s tab.
pub(crate) fn open_title(mode: Mode) -> &'static str {
    match mode {
        Mode::Merge => "Open a merge",
        Mode::Diff => "Open a diff",
    }
}

/// The name of `mode`'s tab in the open dialog.
pub(crate) fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Merge => "Merge",
        Mode::Diff => "Diff",
    }
}

/// The open dialog's start button on `mode`'s tab, named for what it does.
pub(crate) fn start(mode: Mode) -> &'static str {
    match mode {
        Mode::Merge => "Merge",
        Mode::Diff => "Compare",
    }
}

/// A slot's name in the open dialog.
pub(crate) fn slot_name(slot: Slot) -> &'static str {
    match slot {
        Slot::Base => "Base",
        Slot::Left => "Left",
        Slot::Right => "Right",
        Slot::Output => "Output",
    }
}

/// The accessible label of a slot's Choose button.
pub(crate) fn choose_label(slot: Slot) -> String {
    format!("Choose the {} file", slot_name(slot).to_lowercase())
}

/// Why the open dialog cannot start: `slot` is empty.
pub(crate) fn missing_slot(slot: Slot) -> String {
    format!("Choose a file for {}.", slot_name(slot))
}

/// Menu and menu item names, in the platform's title case.
pub(crate) mod menu {
    pub(crate) const SETTINGS: &str = "Settings…";
    pub(crate) const SAVE: &str = "Save";
    pub(crate) const SAVE_AND_EXIT: &str = "Save & Exit";
    pub(crate) const SAVE_COPY_AS: &str = "Save Copy As…";
    pub(crate) const CANCEL: &str = "Cancel";
    pub(crate) const EDIT: &str = "Edit";
    pub(crate) const MERGE: &str = "Merge";
    pub(crate) const GO: &str = "Go";
    pub(crate) const VIEW: &str = "View";
    pub(crate) const UNDO: &str = "Undo";
    pub(crate) const REDO: &str = "Redo";
    pub(crate) const FIND: &str = "Find…";
    pub(crate) const GO_TO_LINE: &str = "Go to Line…";
    pub(crate) const TOGGLE_LEFT: &str = "Toggle Left";
    pub(crate) const TOGGLE_BASE: &str = "Toggle Base";
    pub(crate) const TOGGLE_RIGHT: &str = "Toggle Right";
    pub(crate) const CLEAR_CHOICE: &str = "Clear Choice";
    pub(crate) const TAKE_BASE: &str = "Take Base";
    pub(crate) const TAKE_LEFT: &str = "Take Left";
    pub(crate) const TAKE_RIGHT: &str = "Take Right";
    pub(crate) const TAKE_LEFT_THEN_RIGHT: &str = "Take Left then Right";
    pub(crate) const TAKE_RIGHT_THEN_LEFT: &str = "Take Right then Left";
    pub(crate) const TAKE_SORTED_UNION: &str = "Take Sorted Union";
    pub(crate) const TAKE_LEFT_ALL_UNRESOLVED: &str = "Take Left for All Unresolved";
    pub(crate) const TAKE_RIGHT_ALL_UNRESOLVED: &str = "Take Right for All Unresolved";
    pub(crate) const TAKE_WHOLE_LEFT: &str = "Take Whole Left File";
    pub(crate) const TAKE_WHOLE_RIGHT: &str = "Take Whole Right File";
    pub(crate) const CARRY_MOVED_EDIT: &str = "Carry Edit to Moved Block";
    pub(crate) const NEXT_DIFFERENCE: &str = "Next Difference";
    pub(crate) const PREVIOUS_DIFFERENCE: &str = "Previous Difference";
    pub(crate) const NEXT_CONFLICT: &str = "Next Conflict";
    pub(crate) const PREVIOUS_CONFLICT: &str = "Previous Conflict";
    pub(crate) const BASE_PANE: &str = "Base Pane";
    pub(crate) const BLAME: &str = "Blame";
    pub(crate) const SYNTAX: &str = "Syntax Highlighting";
    pub(crate) const SOFT_WRAP: &str = "Soft Wrap";
    pub(crate) const COLLAPSE_UNCHANGED: &str = "Collapse Unchanged";
    pub(crate) const SCROLL_SYNC: &str = "Sync Result Scrolling";
}

/// A toolbar button's text.
pub(crate) fn tool_label(tool: Tool) -> Option<&'static str> {
    match tool {
        Tool::PreviousDifference => Some("Difference"),
        Tool::PreviousConflict => Some("Conflict"),
        Tool::Left => Some("Left"),
        Tool::Base => Some("Base"),
        Tool::Right => Some("Right"),
        Tool::Clear => Some("Clear"),
        Tool::LeftThenRight => Some("Both"),
        Tool::NextDifference
        | Tool::NextConflict
        | Tool::RightThenLeft
        | Tool::Undo
        | Tool::Redo
        | Tool::BasePane
        | Tool::Find
        | Tool::GoToLine
        | Tool::Syntax
        | Tool::SoftWrap => None,
    }
}

/// The accessible label of the Both button's menu trigger.
pub(crate) const BOTH_MENU: &str = "Take both sides in either order";

/// The orders in the Both button's menu.
pub(crate) const LEFT_THEN_RIGHT: &str = "Left then right";
pub(crate) const RIGHT_THEN_LEFT: &str = "Right then left";

/// A toolbar button's accessible label and tooltip.
pub(crate) fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::PreviousDifference => "Previous difference",
        Tool::NextDifference => "Next difference",
        Tool::PreviousConflict => "Previous conflict",
        Tool::NextConflict => "Next conflict",
        Tool::Left => "Toggle left in the current region",
        Tool::Base => "Toggle base in the current region",
        Tool::Right => "Toggle right in the current region",
        Tool::Clear => "Clear the current region's choice",
        Tool::LeftThenRight => "Take left then right in the current region",
        Tool::RightThenLeft => "Take right then left in the current region",
        Tool::Undo => "Undo",
        Tool::Redo => "Redo",
        Tool::BasePane => "Show or hide the base pane",
        Tool::Find => "Find",
        Tool::GoToLine => "Go to a line in the result",
        Tool::Syntax => "Turn syntax colours on or off",
        Tool::SoftWrap => "Turn soft wrap on or off",
    }
}

/// A pick target's accessible label: the pane's title, where its region sits, and whether
/// the region is an unresolved conflict. The label carries that state because macOS reads a
/// mixed toggle as on.
pub(crate) fn pick_label(
    title: &str,
    region: RegionIx,
    regions: usize,
    unresolved: bool,
) -> String {
    let n = region.next();
    if unresolved {
        format!("{title} in region {n} of {regions}, unresolved")
    } else {
        format!("{title} in region {n} of {regions}")
    }
}

/// The toolbar's conflict count: how many conflicts are unresolved out of all of them,
/// led by the current conflict's `number` when the caret is in one, and the `sides` that
/// conflict keeps in a merge of more than two. `None` for a merge without conflicts.
pub(crate) fn conflict_badge(
    status: &Status,
    number: Option<usize>,
    sides: Option<Sides>,
) -> Option<String> {
    let Status { conflicts, unresolved, .. } = *status;
    let sides =
        sides.map_or_else(String::new, |Sides { kept, of }| format!(" · {kept} of {of} sides"));
    match (conflicts, unresolved, number) {
        (0, _, _) => None,
        (all, 0, Some(n)) => Some(format!("Conflict {n} of {all}{sides} · all resolved")),
        (all, left, Some(n)) => Some(format!("Conflict {n} of {all}{sides} · {left} unresolved")),
        (1, 0, None) => Some("Conflict resolved".to_owned()),
        (all, 0, None) => Some(format!("All {all} resolved")),
        (all, left, None) => Some(format!("{left} of {all} unresolved")),
    }
}

/// A side-pane cell's accessible label: its pane, its line number and its text, or that the
/// pane has no line in this row, with the base lines it deleted there, if any.
pub(crate) fn cell_label(
    title: &str,
    line: Option<(usize, &str)>,
    deleted: Option<usize>,
) -> String {
    match (line, deleted) {
        (Some((n, text)), _) => format!("{title}, line {}: {text}", n + 1),
        (None, None) => format!("{title}, no line"),
        (None, Some(lines)) => format!("{title}, no line: {}", deleted_label(lines)),
    }
}

/// The note on a result placeholder: the base lines the `roles` deleted, or with no roles,
/// that the region picks nothing and drops its longest input's lines.
pub(crate) fn placeholder_label<'a>(
    lines: usize,
    roles: impl IntoIterator<Item = &'a str>,
) -> String {
    let roles = roles.into_iter().collect::<Vec<_>>().join(", ");
    let lines = count(lines, "line", "lines");
    if roles.is_empty() {
        format!("Nothing picked ({lines} dropped)")
    } else {
        format!("{lines} deleted ({roles})")
    }
}

/// The note on the first filler row of a side that deleted `lines` base lines.
pub(crate) fn deleted_label(lines: usize) -> String {
    format!("{} deleted", count(lines, "line", "lines"))
}

/// The previous-match button of the input pane titled `title`.
pub(crate) fn previous_match(title: &str) -> String {
    format!("Previous match in {title}")
}

/// The next-match button of the input pane titled `title`.
pub(crate) fn next_match(title: &str) -> String {
    format!("Next match in {title}")
}

/// An input pane's match count: match `n` of `total`.
pub(crate) fn match_count(n: usize, total: usize) -> String {
    format!("{n} of {total}")
}

/// The go-to-line field's prompt, for a result of `lines` lines.
pub(crate) fn go_to_line_prompt(lines: usize) -> String {
    format!("Go to line (1–{}):", lines.max(1))
}

fn encoding(encoding: Encoding) -> &'static str {
    match encoding {
        Encoding::Utf8 => "UTF-8",
        Encoding::Utf8Bom => "UTF-8 with BOM",
        Encoding::Utf16Le => "UTF-16 LE",
        Encoding::Utf16Be => "UTF-16 BE",
    }
}

fn line_ending(ending: LineEnding) -> &'static str {
    match ending {
        LineEnding::Lf => "LF",
        LineEnding::CrLf => "CRLF",
    }
}

/// The status bar's item for how lines compare. Every whitespace mode drops line endings.
pub(crate) fn comparison(mode: Comparison) -> &'static str {
    match (mode.whitespace, mode.line_endings) {
        (Whitespace::None, LineEndingRule::Compare) => "whitespace significant",
        (Whitespace::None, LineEndingRule::Ignore) => "line endings ignored",
        (Whitespace::Trailing, _) => "trailing whitespace ignored",
        (Whitespace::Amount, _) => "whitespace amount ignored",
        (Whitespace::All, _) => "all whitespace ignored",
    }
}

/// The diff window's items for how lines compare: [`comparison`]'s, then `case ignored`.
pub(crate) fn diff_comparison(mode: Comparison) -> String {
    match mode.case {
        CaseRule::Compare => comparison(mode).to_owned(),
        CaseRule::Ignore => format!("{} · case ignored", comparison(mode)),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The diff window's banner over copies from `jj diff --tool`.
pub(crate) const JJ_DIFF_BANNER: &str =
    "These are temporary copies from jj diff --tool. Edits here are lost when the window closes.";

/// The marker after a diff pane's title while it has unsaved edits.
pub(crate) const MODIFIED: &str = " ●";
/// The copy buttons between the diff panes: left into right, and right into left.
pub(crate) const COPY_TO_RIGHT: &str = "→";
pub(crate) const COPY_TO_LEFT: &str = "←";
/// The copy buttons' accessible labels.
pub(crate) const COPY_TO_RIGHT_LABEL: &str = "Copy this difference to the right";
pub(crate) const COPY_TO_LEFT_LABEL: &str = "Copy this difference to the left";

/// A diff pane's accessible label: its side and its file.
pub(crate) fn diff_pane_label(left: bool, name: &str) -> String {
    let side = if left { "Left" } else { "Right" };
    format!("{side}: {name}")
}

/// The status bars' first item: the repository the files lie in (spec §5).
fn repo(kind: Option<RepoKind>) -> &'static str {
    match kind {
        Some(RepoKind::Git) => "git repo",
        Some(RepoKind::Jj) => "jj repo",
        None => "no repo",
    }
}

/// The text of a fold row, which stands in for `hidden` unchanged lines.
pub(crate) fn unchanged_lines(hidden: usize) -> String {
    count(hidden, "unchanged line", "unchanged lines")
}

/// The diff window's status bar text.
pub(crate) fn diff_status(kind: Option<RepoKind>, differences: usize, comparison: &str) -> String {
    format!("{} · {} · {comparison}", repo(kind), count(differences, "difference", "differences"))
}

/// The status bar's item after the repository: whose merge, and whether it merges words.
fn mode(status: &Status) -> String {
    let vcs = match status.semantics {
        Semantics::Git => "git merge",
        Semantics::Jj => "jj merge",
    };
    match status.hunk_level {
        HunkLevel::Line => vcs.to_owned(),
        HunkLevel::Word => format!("{vcs} by word"),
    }
}

fn divergence(divergence: Divergence, semantics: Semantics) -> &'static str {
    match (divergence, semantics) {
        (Divergence::Whitespace, _) => "whitespace compared unlike jj",
        (Divergence::Case, Semantics::Git) => "case ignored, unlike git",
        (Divergence::Case, Semantics::Jj) => "case ignored, unlike jj",
        (Divergence::SameChangeKept, _) => "same changes kept, unlike git",
        (Divergence::WordLevel, _) => "words merged, unlike git",
        (Divergence::DeepRefine, Semantics::Git) => "conflicts split inside, unlike git",
        (Divergence::DeepRefine, Semantics::Jj) => "conflicts split inside, unlike jj",
        (Divergence::SyntaxBoundaries, Semantics::Git) => "syntax boundaries kept, unlike git",
        (Divergence::SyntaxBoundaries, Semantics::Jj) => "syntax boundaries kept, unlike jj",
    }
}

/// The status bar's warning, in the conflict tone, that the saved output mixes line endings.
pub(crate) const MIXED_ENDINGS: &str = "mixed endings";

/// The status bar's note after Save Copy As: how many conflicts the copy holds.
pub(crate) fn copy_saved(unresolved: usize) -> String {
    match unresolved {
        0 => "Copy saved, resolved".to_owned(),
        1 => "Copy saved with 1 conflict".to_owned(),
        n => format!("Copy saved with {n} conflicts"),
    }
}

/// The status bar's text (spec §5).
pub(crate) fn status(kind: Option<RepoKind>, status: &Status) -> String {
    let mismatch = if status.encoding_mismatch { " (inputs differ)" } else { "" };
    let word_merged = if status.word_merged > 0 {
        format!(" · {} word-merged", status.word_merged)
    } else {
        String::new()
    };
    let divergences = status.divergences().fold(String::new(), |mut all, d| {
        all.push_str(" · ");
        all.push_str(divergence(d, status.semantics));
        all
    });
    format!(
        "{} · {}{divergences} · {} · {}{word_merged} · {} unresolved · {}{mismatch} · {} · {}",
        repo(kind),
        mode(status),
        count(status.differences, "difference", "differences"),
        count(status.conflicts, "conflict", "conflicts"),
        status.unresolved,
        encoding(status.encoding),
        line_ending(status.line_ending),
        comparison(Comparison {
            whitespace: status.whitespace,
            line_endings: status.line_endings,
            case: status.case,
        }),
    )
}

/// A choice's item in the result pane's right-click menu.
pub(crate) fn choice_label(choice: Choice) -> &'static str {
    match choice {
        Choice::Nothing => "None",
        Choice::Base => "Base",
        Choice::Left => "Left",
        Choice::Right => "Right",
        Choice::LeftThenRight => "Left + Right",
        Choice::RightThenLeft => "Right + Left",
    }
}

/// An item of the result pane's right-click menu.
pub(crate) fn menu_label(action: MenuAction) -> &'static str {
    match action {
        MenuAction::Take(choice) => choice_label(choice),
        MenuAction::SortedUnion => "Sorted Union",
        MenuAction::CarryEdit { .. } => "Carry Edit to Moved Block",
    }
}

/// A suggestion's item in the right-click menu, above the choices (spec §4.4).
pub(crate) fn suggested_label(action: MenuAction) -> SharedString {
    match action {
        MenuAction::CarryEdit { from, line } => {
            format!("Suggested: Carry {}'s Edit to Line {line}", choice_label(from))
        }
        MenuAction::Take(_) | MenuAction::SortedUnion => {
            format!("Suggested: {}", menu_label(action))
        }
    }
    .into()
}

/// The editing items after the choices in the right-click menu.
pub(crate) mod edit_menu {
    pub(crate) const CUT: &str = "Cut";
    pub(crate) const COPY: &str = "Copy";
    pub(crate) const PASTE: &str = "Paste";
    pub(crate) const SELECT_ALL: &str = "Select All";
}

#[cfg(test)]
mod tests {
    use super::*;
    use merge_core::merge3::{Refine, SameChange};

    fn change(id: &str, summary: Option<&str>) -> ChangeInfo {
        ChangeInfo {
            commit: format!("c-{id}"),
            change_id: Some(id.to_owned()),
            summary: summary.map(str::to_owned),
        }
    }

    fn hunk(number: usize, goes: Goes) -> AbsorbHunk {
        AbsorbHunk {
            number,
            path: "a.txt".into(),
            left: 2..3,
            right: 2..4,
            goes,
            before: b"c\n".to_vec(),
            after: b"C\nD\n".to_vec(),
        }
    }

    #[test]
    fn a_hunk_reads_as_absorb_lists_it_with_its_lines() {
        let kept = hunk(1, Goes::Stays(Stay::WouldConflict(change("mmmm", None))));
        assert_eq!(
            absorb_hunk(&kept, None).label(),
            "1 a.txt left 2..3 right 2..4 stays: would conflict in mmmm"
        );
        assert_eq!(
            absorb_hunk(&kept, None),
            Row {
                lead: Some("1".into()),
                text: "a.txt".into(),
                trail: Some("left 2..3 right 2..4 stays: would conflict in mmmm".into())
            }
        );
        let target = change("kkkk", None);
        assert_eq!(
            absorb_hunk(&kept, Some(&target)).label(),
            "1 a.txt left 2..3 right 2..4 -> kkkk c-kkkk"
        );
        // A hunk the listing moved, kept in the working copy by a choice in the window.
        let moved = hunk(2, Goes::Into(target.clone()));
        assert_eq!(absorb_hunk(&moved, None).label(), "2 a.txt left 2..3 right 2..4 stays: kept");
        let line = |side, text: &str| (side, text.to_owned());
        assert_eq!(
            absorb_lines(&kept),
            [line(Side::Left, "- c"), line(Side::Right, "+ C"), line(Side::Right, "+ D")]
        );
    }

    #[test]
    fn a_choice_counts_the_hunks_going_there() {
        let target = change("kkkk", Some("fix the parser"));
        assert_eq!(absorb_destination(&target, 0).label(), "kkkk c-kkkk fix the parser");
        assert_eq!(absorb_destination(&target, 1).label(), "kkkk c-kkkk fix the parser · 1 hunk");
        assert_eq!(absorb_destination(&change("mmmm", None), 2).label(), "mmmm c-mmmm · 2 hunks");
        assert_eq!(absorb_keep(0).label(), "Working copy");
        assert_eq!(absorb_keep(3).label(), "Working copy · 3 hunks");
        assert_eq!(
            absorb_destination(&target, 1),
            Row {
                lead: Some("kkkk c-kkkk".into()),
                text: "fix the parser".into(),
                trail: Some("· 1 hunk".into())
            }
        );
        assert_eq!(absorb_skipped("b.bin", "not a file"), "Skipped b.bin: not a file");
    }

    #[test]
    fn the_apply_note_counts_moved_hunks_and_distinct_commits() {
        let absorbed = |hunks| Absorbed {
            operation: String::new(),
            hunks,
            destinations: Vec::new(),
            skipped: Vec::new(),
            rewrite: Rewrite {
                target: change("kkkk", None),
                written: true,
                resolved: true,
                sides: None,
                rewritten: Vec::new(),
                latest: None,
                stale: false,
            },
        };
        let note = |hunks| absorb_note(&absorbed(hunks));
        assert_eq!(
            note(vec![
                hunk(1, Goes::Into(change("kkkk", None))),
                hunk(2, Goes::Stays(Stay::NoCommit)),
                hunk(3, Goes::Into(change("kkkk", None)))
            ]),
            "Absorbed 2 hunks into 1 commit."
        );
        assert_eq!(
            note(vec![hunk(1, Goes::Into(change("kkkk", None)))]),
            "Absorbed 1 hunk into 1 commit."
        );
        assert_eq!(
            note(vec![
                hunk(1, Goes::Into(change("kkkk", None))),
                hunk(2, Goes::Into(change("mmmm", None)))
            ]),
            "Absorbed 2 hunks into 2 commits."
        );
        assert_eq!(
            note(vec![hunk(1, Goes::Stays(Stay::NoCommit))]),
            "Absorbed 0 hunks into 0 commits."
        );
    }

    #[test]
    fn the_apply_note_lists_each_commit_whose_conflicts_changed() {
        let effect = |id: &str, before: &[&str], after: &[&str]| CommitEffect {
            change: change(id, None),
            before: before.iter().map(Into::into).collect(),
            after: after.iter().map(Into::into).collect(),
        };
        let absorbed = Absorbed {
            operation: String::new(),
            hunks: vec![hunk(1, Goes::Into(change("kkkk", None)))],
            destinations: Vec::new(),
            skipped: Vec::new(),
            rewrite: Rewrite {
                target: change("wwww", None),
                written: true,
                resolved: true,
                sides: None,
                rewritten: vec![
                    effect("kkkk", &[], &["a.txt"]),
                    effect("mmmm", &["b.txt"], &["b.txt"]),
                ],
                latest: None,
                stale: false,
            },
        };
        assert_eq!(absorb_note(&absorbed), "Absorbed 1 hunk into 1 commit.\nkkkk: none -> a.txt");
    }

    #[test]
    fn the_apply_note_says_how_to_update_a_stale_working_copy() {
        let absorbed = Absorbed {
            operation: String::new(),
            hunks: vec![hunk(1, Goes::Into(change("kkkk", None)))],
            destinations: Vec::new(),
            skipped: Vec::new(),
            rewrite: Rewrite {
                target: change("wwww", None),
                written: true,
                resolved: true,
                sides: None,
                rewritten: Vec::new(),
                latest: None,
                stale: true,
            },
        };
        assert_eq!(
            absorb_note(&absorbed),
            "Absorbed 1 hunk into 1 commit.\n\
             The working copy is stale: run jj workspace update-stale."
        );
    }

    fn version(state: VersionState) -> FileVersion {
        FileVersion {
            operation: "0123456789abcdef0123".to_owned(),
            time: "2026-11-13T10:00:00+01:00".to_owned(),
            description: "snapshot working copy".to_owned(),
            commit: vcs::CommitId::new("cccc".to_owned()),
            state,
        }
    }

    #[test]
    fn a_version_reads_as_history_lists_it() {
        let line = |state| history_version(&version(state)).label();
        assert_eq!(
            line(VersionState::File),
            "0123456789ab 2026-11-13T10:00:00+01:00 snapshot working copy"
        );
        assert_eq!(
            line(VersionState::Absent),
            "0123456789ab 2026-11-13T10:00:00+01:00 snapshot working copy (absent)"
        );
        assert_eq!(
            line(VersionState::Conflict),
            "0123456789ab 2026-11-13T10:00:00+01:00 snapshot working copy (conflict)"
        );
        assert_eq!(
            line(VersionState::Other),
            "0123456789ab 2026-11-13T10:00:00+01:00 snapshot working copy (other)"
        );
        assert_eq!(
            history_version(&version(VersionState::Absent)),
            Row {
                lead: Some("0123456789ab 2026-11-13T10:00:00+01:00".into()),
                text: "snapshot working copy".into(),
                trail: Some("(absent)".into())
            }
        );
        assert_eq!(history_title("src/a.txt"), "History of src/a.txt");
    }

    #[test]
    fn the_build_line_names_the_commit_and_the_jj_change() {
        assert_eq!(
            build(Some("1a2b3c4d5e6f"), Some("kxqpzmvwrlty"), Profile::Release).as_deref(),
            Some("Commit 1a2b3c4d5e6f, change kxqpzmvwrlty")
        );
    }

    #[test]
    fn a_git_build_names_only_the_commit() {
        assert_eq!(
            build(Some("1a2b3c4d5e6f-dirty"), None, Profile::Release).as_deref(),
            Some("Commit 1a2b3c4d5e6f-dirty")
        );
        assert_eq!(
            build(Some("1a2b3c4d5e6f"), Some(""), Profile::Release).as_deref(),
            Some("Commit 1a2b3c4d5e6f")
        );
    }

    #[test]
    fn a_build_without_a_commit_has_no_build_line() {
        assert_eq!(build(None, None, Profile::Release), None);
        assert_eq!(build(Some(""), None, Profile::Release), None);
        assert_eq!(build(None, Some("kxqpzmvwrlty"), Profile::Release), None);
    }

    #[test]
    fn a_debug_build_without_a_commit_shows_the_null_ids() {
        let placeholder = Some("Commit 000000000000, change zzzzzzzzzzzz");
        assert_eq!(build(None, None, Profile::Debug).as_deref(), placeholder);
        assert_eq!(build(Some(""), Some(""), Profile::Debug).as_deref(), placeholder);
        assert_eq!(build(None, Some("kxqpzmvwrlty"), Profile::Debug).as_deref(), placeholder);
    }

    #[test]
    fn a_debug_build_given_a_commit_shows_it() {
        assert_eq!(
            build(Some("1a2b3c4d5e6f"), Some("kxqpzmvwrlty"), Profile::Debug).as_deref(),
            Some("Commit 1a2b3c4d5e6f, change kxqpzmvwrlty")
        );
        assert_eq!(
            build(Some("1a2b3c4d5e6f"), None, Profile::Debug).as_deref(),
            Some("Commit 1a2b3c4d5e6f")
        );
    }

    #[test]
    fn the_repository_leads_the_merge_status() {
        let jj = Status { semantics: Semantics::Jj, ..with(1, 0, 0) };
        assert_eq!(
            status(Some(RepoKind::Jj), &jj),
            "jj repo · jj merge · 1 difference · 0 conflicts · 0 unresolved · UTF-8 · LF · \
             whitespace significant"
        );
        assert!(
            status(Some(RepoKind::Git), &with(1, 0, 0)).starts_with("git repo · git merge · "),
            "{}",
            status(Some(RepoKind::Git), &with(1, 0, 0))
        );
        assert!(
            status(None, &with(1, 0, 0)).starts_with("no repo · git merge · "),
            "{}",
            status(None, &with(1, 0, 0))
        );
    }

    #[test]
    fn the_repository_and_the_semantics_show_apart() {
        assert!(
            status(Some(RepoKind::Git), &Status { semantics: Semantics::Jj, ..with(1, 0, 0) })
                .starts_with("git repo · jj merge · "),
            "an explicit jj merge in a git repository shows both"
        );
    }

    #[test]
    fn the_repository_leads_the_diff_status() {
        assert_eq!(
            diff_status(Some(RepoKind::Jj), 1, "whitespace significant"),
            "jj repo · 1 difference · whitespace significant"
        );
        assert_eq!(
            diff_status(Some(RepoKind::Git), 2, "whitespace significant"),
            "git repo · 2 differences · whitespace significant"
        );
        assert_eq!(
            diff_status(None, 0, "all whitespace ignored"),
            "no repo · 0 differences · all whitespace ignored"
        );
    }

    #[test]
    fn ignored_line_endings_show_unless_a_whitespace_mode_drops_them_anyway() {
        let by = |whitespace, line_endings| {
            comparison(Comparison { whitespace, line_endings, case: CaseRule::Compare })
        };
        assert_eq!(by(Whitespace::None, LineEndingRule::Compare), "whitespace significant");
        assert_eq!(by(Whitespace::None, LineEndingRule::Ignore), "line endings ignored");
        assert_eq!(by(Whitespace::Trailing, LineEndingRule::Ignore), "trailing whitespace ignored");
    }

    #[test]
    fn the_diff_status_adds_an_item_when_case_is_ignored() {
        let ignoring = Comparison { case: CaseRule::Ignore, ..Comparison::default() };
        assert_eq!(diff_comparison(ignoring), "whitespace significant · case ignored");
        let spaces = Comparison { whitespace: Whitespace::All, ..ignoring };
        assert_eq!(diff_comparison(spaces), "all whitespace ignored · case ignored");
        assert_eq!(diff_comparison(Comparison::default()), "whitespace significant");
    }

    #[test]
    fn ignored_case_shows_as_a_divergence_under_either_semantics() {
        let git = Status { case: CaseRule::Ignore, ..with(1, 0, 0) };
        assert!(
            status(None, &git)
                .starts_with("no repo · git merge · case ignored, unlike git · 1 difference · "),
            "{}",
            status(None, &git)
        );
        let jj = Status { semantics: Semantics::Jj, ..git };
        assert!(
            status(None, &jj)
                .starts_with("no repo · jj merge · case ignored, unlike jj · 1 difference · "),
            "{}",
            status(None, &jj)
        );
    }

    fn with(differences: usize, conflicts: usize, unresolved: usize) -> Status {
        Status {
            differences,
            conflicts,
            unresolved,
            encoding: Encoding::Utf8,
            encoding_mismatch: false,
            line_ending: LineEnding::Lf,
            mixed_endings: false,
            whitespace: Whitespace::None,
            line_endings: LineEndingRule::Compare,
            case: CaseRule::Compare,
            word_merged: 0,
            semantics: Semantics::Git,
            same_change: SameChange::Accept,
            hunk_level: HunkLevel::Line,
            refine: Refine::Ends,
            syntax_boundaries: false,
        }
    }

    #[test]
    fn syntax_boundaries_name_the_vcs_they_diverge_from() {
        let bounded = |semantics| Status {
            semantics,
            hunk_level: HunkLevel::Word,
            syntax_boundaries: true,
            ..with(1, 1, 1)
        };
        assert!(
            status(None, &bounded(Semantics::Jj))
                .starts_with("no repo · jj merge by word · syntax boundaries kept, unlike jj · "),
            "{}",
            status(None, &bounded(Semantics::Jj))
        );
        assert!(
            status(None, &bounded(Semantics::Git)).starts_with(
                "no repo · git merge by word · words merged, unlike git · \
                 syntax boundaries kept, unlike git · "
            ),
            "{}",
            status(None, &bounded(Semantics::Git))
        );
    }

    #[test]
    fn deep_refinement_names_the_vcs_it_diverges_from() {
        let deep = |semantics| Status { semantics, refine: Refine::Deep, ..with(1, 1, 1) };
        assert_eq!(
            status(None, &deep(Semantics::Git)),
            "no repo · git merge · conflicts split inside, unlike git · 1 difference · 1 conflict · \
             1 unresolved · UTF-8 · LF · whitespace significant"
        );
        assert!(
            status(None, &deep(Semantics::Jj))
                .starts_with("no repo · jj merge · conflicts split inside, unlike jj · "),
            "{}",
            status(None, &deep(Semantics::Jj))
        );
    }

    #[test]
    fn word_merged_regions_show_only_when_there_are_some() {
        let one = Status { word_merged: 1, ..with(2, 1, 1) };
        assert_eq!(
            status(None, &one),
            "no repo · git merge · 2 differences · 1 conflict · 1 word-merged · 1 unresolved · UTF-8 · LF · \
             whitespace significant"
        );
        let three = Status { word_merged: 3, ..with(3, 0, 0) };
        assert!(status(None, &three).contains(" · 3 word-merged · "), "{}", status(None, &three));
    }

    fn conflict_badge_at(number: Option<usize>, status: &Status) -> Option<String> {
        conflict_badge(status, number, None)
    }

    fn effect(change: &str, before: &[&str], after: &[&str]) -> CommitEffect {
        let paths = |paths: &[&str]| paths.iter().map(|&p| p.into()).collect();
        CommitEffect {
            change: vcs::ChangeInfo {
                commit: "c0ffee12".to_owned(),
                change_id: Some(change.to_owned()),
                summary: None,
            },
            before: paths(before),
            after: paths(after),
        }
    }

    #[test]
    fn each_descendant_shows_its_conflicts_before_and_after() {
        let effects = [
            effect("kmzzmxvq", &["a.txt", "b.txt"], &[]),
            effect("vruxwmqv", &["a.txt"], &["b.txt"]),
        ];
        assert_eq!(
            descendant_effects(&effects),
            "kmzzmxvq: a.txt, b.txt -> none\nvruxwmqv: a.txt -> b.txt"
        );
    }

    #[test]
    fn a_descendant_without_a_change_id_shows_its_commit() {
        let mut effect = effect("unused", &[], &["a.txt"]);
        effect.change.change_id = None;
        assert_eq!(descendant_effects([&effect]), "c0ffee12: none -> a.txt");
    }

    fn rewrite(resolved: bool, rewritten: Vec<CommitEffect>) -> Rewrite {
        Rewrite {
            target: vcs::ChangeInfo {
                commit: "1a2b3c4d".to_owned(),
                change_id: Some("qpvuntsm".to_owned()),
                summary: None,
            },
            resolved,
            sides: None,
            rewritten,
            written: false,
            latest: None,
            stale: false,
        }
    }

    #[test]
    fn the_preview_says_whether_the_path_resolves_and_lists_each_descendant() {
        let effects = vec![effect("kmzzmxvq", &["a.txt"], &[])];
        assert_eq!(
            stack_preview(&rewrite(true, effects)),
            "Saving resolves the path in qpvuntsm.\nkmzzmxvq: a.txt -> none"
        );
        assert_eq!(
            stack_preview(&rewrite(false, Vec::new())),
            "Saving keeps conflicts in qpvuntsm.\nNo descendant is rebased."
        );
    }

    #[test]
    fn a_listed_commit_reads_change_commit_and_summary() {
        let mut commit = StackCommit {
            id: vcs::CommitId::new("1a2b3c4d5e6f".to_owned()),
            change: vcs::ChangeInfo {
                commit: "1a2b3c4d".to_owned(),
                change_id: Some("qpvuntsm".to_owned()),
                summary: Some("merge".to_owned()),
            },
            paths: Vec::new(),
        };
        assert_eq!(stack_commit(&commit).label(), "qpvuntsm 1a2b3c4d merge");
        assert_eq!(stack_commit(&commit).text, "merge");
        commit.change.summary = None;
        assert_eq!(stack_commit(&commit).label(), "qpvuntsm 1a2b3c4d");
    }

    #[test]
    fn an_inherited_path_is_marked() {
        let path = |inherited| StackPath { path: "src/a.rs".into(), inherited };
        assert_eq!(stack_path(&path(false)).label(), "src/a.rs");
        assert_eq!(stack_path(&path(true)).label(), "src/a.rs (inherited)");
        assert_eq!(stack_path(&path(true)).text, "src/a.rs");
    }

    #[test]
    fn the_note_after_a_merge_window_says_how_it_ended() {
        let note = |code| stack_note(code, "a.txt", "qpvuntsm");
        assert_eq!(note(ExitCode::Resolved).as_deref(), Some("Resolved a.txt in qpvuntsm."));
        assert_eq!(
            note(ExitCode::Conflicts).as_deref(),
            Some("Saved a.txt in qpvuntsm with conflicts.")
        );
        assert_eq!(note(ExitCode::Cancelled), None);
        assert_eq!(
            note(ExitCode::Error).as_deref(),
            Some("The merge of a.txt in qpvuntsm ended with an error.")
        );
    }

    #[test]
    fn the_descendants_question_counts_them() {
        assert_eq!(prompt::descendants_changed(1), "Saving changes the conflicts of 1 descendant.");
        assert_eq!(
            prompt::descendants_changed(3),
            "Saving changes the conflicts of 3 descendants."
        );
    }

    #[test]
    fn a_conflict_of_more_than_two_sides_shows_the_sides_jj_keeps() {
        let sides = Some(Sides { kept: 2, of: 3 });
        assert_eq!(
            conflict_badge(&with(25, 20, 5), Some(3), sides).as_deref(),
            Some("Conflict 3 of 20 · 2 of 3 sides · 5 unresolved")
        );
        assert_eq!(
            conflict_badge(&with(25, 20, 0), Some(3), sides).as_deref(),
            Some("Conflict 3 of 20 · 2 of 3 sides · all resolved")
        );
    }

    #[test]
    fn the_sides_show_only_while_the_caret_is_in_a_conflict() {
        let sides = Some(Sides { kept: 3, of: 4 });
        assert_eq!(
            conflict_badge(&with(25, 20, 5), None, sides).as_deref(),
            Some("5 of 20 unresolved")
        );
    }

    #[test]
    fn the_conflict_badge_leads_with_the_current_conflict() {
        assert_eq!(
            conflict_badge_at(Some(3), &with(25, 20, 5)).as_deref(),
            Some("Conflict 3 of 20 · 5 unresolved")
        );
        assert_eq!(
            conflict_badge_at(Some(20), &with(25, 20, 0)).as_deref(),
            Some("Conflict 20 of 20 · all resolved")
        );
        assert_eq!(
            conflict_badge_at(Some(1), &with(1, 1, 1)).as_deref(),
            Some("Conflict 1 of 1 · 1 unresolved")
        );
    }

    #[test]
    fn the_conflict_badge_counts_unresolved_out_of_all() {
        assert_eq!(
            conflict_badge_at(None, &with(12, 10, 5)).as_deref(),
            Some("5 of 10 unresolved")
        );
        assert_eq!(
            conflict_badge_at(None, &with(12, 10, 10)).as_deref(),
            Some("10 of 10 unresolved")
        );
        assert_eq!(conflict_badge_at(None, &with(1, 1, 1)).as_deref(), Some("1 of 1 unresolved"));
    }

    #[test]
    fn the_conflict_badge_says_when_every_conflict_is_resolved() {
        assert_eq!(conflict_badge_at(None, &with(12, 10, 0)).as_deref(), Some("All 10 resolved"));
        assert_eq!(conflict_badge_at(None, &with(1, 1, 0)).as_deref(), Some("Conflict resolved"));
    }

    #[test]
    fn a_merge_without_conflicts_has_no_badge() {
        assert_eq!(conflict_badge_at(None, &with(3, 0, 0)), None);
        assert_eq!(conflict_badge_at(None, &with(0, 0, 0)), None);
    }

    #[test]
    fn the_save_detail_counts_the_conflicts_left() {
        assert_eq!(
            prompt::save_unresolved_detail(1, Semantics::Jj),
            "1 conflict keeps its markers, and git or jj sees the merge as unresolved."
        );
        assert_eq!(
            prompt::save_unresolved_detail(3, Semantics::Jj),
            "3 conflicts keep their markers, and git or jj sees the merge as unresolved."
        );
    }

    #[test]
    fn under_git_the_save_detail_says_git_mergetool_puts_back_its_file() {
        assert_eq!(
            prompt::save_unresolved_detail(2, Semantics::Git),
            "2 conflicts keep their markers, and git or jj sees the merge as unresolved. \
             git mergetool then puts back its own copy of the file; run it again to restore \
             this session."
        );
    }

    #[test]
    fn the_go_to_line_prompt_names_the_last_line() {
        assert_eq!(go_to_line_prompt(42), "Go to line (1–42):");
        assert_eq!(go_to_line_prompt(0), "Go to line (1–1):");
    }

    #[test]
    fn counts_take_singular_and_plural_nouns() {
        assert_eq!(
            status(None, &with(1, 1, 0)),
            "no repo · git merge · 1 difference · 1 conflict · 0 unresolved · UTF-8 · LF · \
             whitespace significant"
        );
        assert_eq!(
            status(None, &with(0, 2, 2)),
            "no repo · git merge · 0 differences · 2 conflicts · 2 unresolved · UTF-8 · LF · \
             whitespace significant"
        );
    }

    #[test]
    fn an_encoding_mismatch_and_other_modes_show() {
        let s = Status {
            encoding: Encoding::Utf16Le,
            encoding_mismatch: true,
            line_ending: LineEnding::CrLf,
            whitespace: Whitespace::Trailing,
            ..with(3, 0, 0)
        };
        assert_eq!(
            status(None, &s),
            "no repo · git merge · 3 differences · 0 conflicts · 0 unresolved · UTF-16 LE (inputs differ) · \
             CRLF · trailing whitespace ignored"
        );
    }

    #[test]
    fn the_mode_leads_and_each_divergence_follows_it() {
        let jj = Status { semantics: Semantics::Jj, ..with(1, 0, 0) };
        assert!(
            status(None, &jj).starts_with("no repo · jj merge · 1 difference · "),
            "{}",
            status(None, &jj)
        );
        let jj_spaces = Status { whitespace: Whitespace::All, ..jj };
        assert_eq!(
            status(None, &jj_spaces),
            "no repo · jj merge · whitespace compared unlike jj · 1 difference · 0 conflicts · \
             0 unresolved · UTF-8 · LF · all whitespace ignored"
        );
        let jj_endings = Status { line_endings: LineEndingRule::Ignore, ..jj };
        assert_eq!(
            status(None, &jj_endings),
            "no repo · jj merge · whitespace compared unlike jj · 1 difference · 0 conflicts · \
             0 unresolved · UTF-8 · LF · line endings ignored"
        );
        let git_both =
            Status { same_change: SameChange::Keep, hunk_level: HunkLevel::Word, ..with(1, 0, 0) };
        assert!(
            status(None, &git_both).starts_with(
                "no repo · git merge by word · same changes kept, unlike git · words merged, unlike git · \
                 1 difference · "
            ),
            "{}",
            status(None, &git_both)
        );
        let jj_word = Status { hunk_level: HunkLevel::Word, same_change: SameChange::Keep, ..jj };
        assert!(
            status(None, &jj_word).starts_with("no repo · jj merge by word · 1 difference · "),
            "{}",
            status(None, &jj_word)
        );
    }

    #[test]
    fn the_menu_names_each_choice_by_its_sources() {
        let labels = Choice::ALL.map(|choice| menu_label(MenuAction::Take(choice)));
        assert_eq!(labels, ["None", "Base", "Left", "Right", "Left + Right", "Right + Left"]);
    }

    #[test]
    fn a_suggestion_names_its_choice() {
        assert_eq!(
            suggested_label(MenuAction::Take(Choice::LeftThenRight)).as_ref(),
            "Suggested: Left + Right"
        );
        assert_eq!(suggested_label(MenuAction::SortedUnion).as_ref(), "Suggested: Sorted Union");
        assert_eq!(
            suggested_label(MenuAction::CarryEdit { from: Choice::Right, line: 4 }).as_ref(),
            "Suggested: Carry Right's Edit to Line 4"
        );
    }

    #[test]
    fn a_deletion_names_its_line_count() {
        assert_eq!(deleted_label(1), "1 line deleted");
        assert_eq!(deleted_label(5), "5 lines deleted");
    }

    #[test]
    fn a_cell_label_names_its_line_or_its_deletion() {
        assert_eq!(cell_label("Left", Some((0, "fn a()")), None), "Left, line 1: fn a()");
        assert_eq!(cell_label("Left", None, None), "Left, no line");
        assert_eq!(cell_label("Left", None, Some(2)), "Left, no line: 2 lines deleted");
    }

    #[test]
    fn a_placeholder_names_who_deleted_or_that_nothing_was_picked() {
        assert_eq!(placeholder_label(3, ["Left"]), "3 lines deleted (Left)");
        assert_eq!(placeholder_label(1, ["Right"]), "1 line deleted (Right)");
        assert_eq!(placeholder_label(2, ["Left", "Right"]), "2 lines deleted (Left, Right)");
        assert_eq!(placeholder_label(4, []), "Nothing picked (4 lines dropped)");
        assert_eq!(placeholder_label(1, []), "Nothing picked (1 line dropped)");
    }
}
