//! Every string the window shows, in one place (spec §6.5). Each function names a message of
//! the catalogues in `locales/` and formats it in the run's language through [`crate::i18n`].
//! Product names, file and VCS ids, encodings, line endings and arrows stay in code: they
//! read the same in every language.

use std::borrow::Cow;

use gpui_kit::SharedString;
use merge_core::diff::{
    BlankLineRule, CaseRule, Comparison, DiffOptions, LineEndingRule, Whitespace,
};
use merge_core::diff2::Side;
use merge_core::index::RegionIx;
use merge_core::merge3::{HunkLevel, Semantics, Source};
use merge_core::text::{Encoding, LineEnding};
use merge_session::ExitCode;
use merge_session::dir_diff::{Change, ChangedFile, Content};
use merge_session::jj_native::{
    AbsorbHunk, Absorbed, CommitEffect, FileVersion, Goes, Rewrite, SHORT_OPERATION, StackCommit,
    StackPath, Stay, VersionState, stays,
};
use merge_session::native::{LeftAt, Revision};
use merge_view::nav::Sides;
use merge_view::status::{Divergence, Role, Status};
use vcs::{ChangeInfo, IdKind, RepoKind};

use crate::choice::{Choice, MenuAction};
use crate::i18n::{self, Locale, t};
use crate::open_dialog::{Mode, Slot};
use crate::pick_list::{self, Row, Shown};
use crate::toolbar::Tool;

/// One function per message without arguments, named for it.
macro_rules! texts {
    ($($(#[$doc:meta])* $name:ident = $id:literal;)+) => {
        $(
            $(#[$doc])*
            pub(crate) fn $name() -> SharedString {
                t!($id)
            }
        )+
    };
}

pub(crate) const APP_NAME: &str = "triad";
/// The prefix of every message on stderr, as `merge-cli` writes them.
pub(crate) const PROGRAM: &str = "triad-merge";
/// The prefix of the measurement lines on stderr (DESIGN.md §A6.7).
pub(crate) const MEASURE: &str = "triad-merge measure";
/// Between the items of a status bar, a badge or a list row.
const ITEMS: &str = " · ";

texts! {
    /// The About window's title and its menu item.
    about_title = "about-title";
    /// The About window's line under the version.
    description = "about-description";
    /// The About window's line under the description.
    vcs_support = "about-vcs-support";
    result_title = "result-title";
    toolbar = "merge-toolbar";
    diff_toolbar = "diff-toolbar";
    /// The accessible label of the merge window's aligned input lines.
    input_lines = "input-lines";
    go_to_line_field = "go-to-line-field";
    /// An input pane's count when the result's search matches nothing in it (spec §6.3).
    no_matches = "no-matches";
    /// The settings banner's close button.
    close = "close";
}

/// The About window's version line.
pub(crate) fn version() -> SharedString {
    t!("about-version", version = env!("CARGO_PKG_VERSION")).into()
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
    let line = |commit: &str, change: Option<&str>| match change {
        Some(change) => t!("about-build-commit-change", commit = commit, change = change).into(),
        None => t!("about-build-commit", commit = commit).into(),
    };
    match (commit.filter(|id| !id.is_empty()), profile) {
        (Some(commit), _) => Some(line(commit, change.filter(|id| !id.is_empty()))),
        (None, Profile::Debug) => Some(line(NULL_COMMIT, Some(ROOT_CHANGE))),
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

/// The merge window's banner after another program changed OUTPUT, named `name` (spec §6.3).
pub(crate) fn output_changed_banner(name: &str) -> String {
    t!("output-changed-banner", name = name)
}

/// The settings banner's accessible label: what it is, then its errors.
pub(crate) fn settings_errors_label(errors: &str) -> String {
    t!("settings-errors", errors = errors.replace('\n', "; ").as_str())
}

/// The confirmations and alerts of Save & Exit and Cancel (spec §6.2). The first answer
/// of each goes ahead.
pub(crate) mod prompt {
    use gpui_kit::{PromptButton, SharedString};
    use merge_core::merge3::Semantics;

    use crate::i18n::t;

    texts! {
        save_unresolved = "prompt-save-unresolved";
        save_with_conflicts = "prompt-save-with-conflicts";
        keep_editing = "prompt-keep-editing";
        save_and_exit = "menu-save-and-exit";
        discard_detail = "prompt-discard-detail";
        discard = "prompt-discard";
        cannot_write = "prompt-cannot-write";
        cannot_write_copy = "prompt-cannot-write-copy";
        cannot_record = "prompt-cannot-record";
        no_output = "prompt-no-output";
        quit = "prompt-quit";
        try_again = "prompt-try-again";
        save_copy_as = "menu-save-copy-as";
        restore_session = "prompt-restore-session";
        restore_detail = "prompt-restore-detail";
        restore = "prompt-restore";
        start_over = "prompt-start-over";
        cannot_restore = "prompt-cannot-restore";
        save_changes = "prompt-save-changes";
        save = "menu-save";
        cannot_save = "prompt-cannot-save";
        ok = "prompt-ok";
        changed_detail = "prompt-changed-detail";
        reload = "prompt-reload";
        keep_mine = "prompt-keep-mine";
        output_changed_detail = "prompt-output-changed-detail";
    }

    /// The button that closes an alert. It takes the platform's role of OK.
    pub(crate) fn ok_button() -> PromptButton {
        PromptButton::ok(ok())
    }

    /// The question before a save into a jj commit that changes the conflicts of
    /// `descendants` of its descendants (spec §7.5.1).
    pub(crate) fn descendants_changed(descendants: usize) -> String {
        t!("prompt-descendants-changed", count = descendants)
    }

    /// The question when a file with unsaved edits changes on disk.
    pub(crate) fn changed_on_disk(name: &str) -> String {
        t!("prompt-changed-on-disk", name = name)
    }

    /// What saving with `unresolved` conflicts left does. Under git, `git mergetool` then
    /// puts back its own file, and the recovery journal holds the work (spec §6.2).
    pub(crate) fn save_unresolved_detail(unresolved: usize, semantics: Semantics) -> String {
        match semantics {
            Semantics::Git => t!("prompt-unresolved-detail-git", count = unresolved),
            Semantics::Jj => t!("prompt-unresolved-detail", count = unresolved),
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

/// `paths` joined by commas, or the word for none.
fn path_list(paths: &[impl AsRef<str>]) -> String {
    if paths.is_empty() {
        t!("no-paths").to_string()
    } else {
        paths.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(", ")
    }
}

texts! {
    /// The stack window's title (spec §7.5.1).
    stack_title = "stack-title";
    stack_empty = "stack-empty";
    history_empty = "history-empty";
    /// The stack and history windows' button that lists again.
    refresh = "refresh";
    /// The stack window's button that previews a save.
    stack_preview_button = "stack-preview";
    preview_running = "preview-running";
}

/// A change's IDs, as the lists print them: change, then commit.
fn change_ids(change: &ChangeInfo) -> SharedString {
    format!("{} {}", change.id(IdKind::Change), change.id(IdKind::Commit)).into()
}

/// A listed commit's heading, as `stack --list` prints it: its change, commit and summary.
pub(crate) fn stack_commit(commit: &StackCommit) -> Row {
    let ids = change_ids(&commit.change);
    let text =
        commit.change.summary.clone().map_or_else(|| t!("no-description"), SharedString::from);
    let label = commit.working_copy.then(|| format!("{} {ids} {text}", t!("working-copy")).into());
    Row {
        mark: commit.working_copy.then_some(SharedString::new_static(WORKING_COPY_MARK)),
        lead: Some(ids),
        text,
        label,
        placeholder: commit.change.summary.is_none(),
        hover: Some(stack_commit_tooltip(commit).into()),
        ..Row::default()
    }
}

/// jj's mark for the working-copy commit, as `jj log` draws it.
const WORKING_COPY_MARK: &str = "@";

/// A listed commit's tooltip: its full change and commit IDs, the time `jj log` shows, and
/// the whole description.
pub(crate) fn stack_commit_tooltip(commit: &StackCommit) -> String {
    let description = commit.description.trim_end();
    let ids = t!(
        "commit-tooltip",
        change = &commit.change_id,
        commit = commit.id.as_str(),
        date = merge_view::origin::local_time(commit.time).as_str(),
    );
    if description.is_empty() {
        format!("{ids}\n\n{}", t!("no-description"))
    } else {
        format!("{ids}\n\n{description}")
    }
}

/// A listed path, conflicted, marked when a parent holds a conflict there too.
pub(crate) fn stack_path(path: &StackPath) -> Row {
    Row {
        status: Some(pick_list::Status::Conflict),
        text: path.path.to_string().into(),
        shown: Shown::Path,
        trail: path.inherited().then(|| t!("inherited")),
        quiet: path.untouched,
        ..Row::default()
    }
}

/// The pill over an inherited conflict in the result (spec §5).
pub(crate) fn inherited_pill() -> SharedString {
    t!("inherited-pill")
}

/// What the stack window says while the merge window is open on `path` in `change`, whose
/// conflict arises in the listed commit `origin`.
pub(crate) fn stack_inherited(path: &str, change: &str, origin: &str) -> String {
    t!("stack-inherited", path = path, change = change, origin = origin)
}

/// The stack window's summary: the conflicted commits listed.
pub(crate) fn stack_summary(commits: usize) -> String {
    t!("count-commits", count = commits)
}

/// What the stack window says after the merge window on `path` in `change` finished with
/// `code`; nothing after a cancel.
pub(crate) fn stack_note(code: ExitCode, path: &str, change: &str) -> Option<String> {
    match code {
        ExitCode::Resolved => Some(t!("stack-resolved", path = path, change = change)),
        ExitCode::Conflicts => Some(t!("stack-saved-conflicts", path = path, change = change)),
        ExitCode::Cancelled => None,
        ExitCode::Error => Some(t!("stack-error", path = path, change = change)),
    }
}

/// What saving the open merge window would do: whether the path resolves, and each
/// descendant's conflicts before and after.
pub(crate) fn stack_preview(rewrite: &Rewrite) -> String {
    let change = rewrite.target.id(IdKind::Change);
    let outcome = if rewrite.resolved {
        t!("preview-resolves", change = change)
    } else {
        t!("preview-keeps", change = change)
    };
    if rewrite.rewritten.is_empty() {
        format!("{outcome}\n{}", t!("preview-no-rebase"))
    } else {
        format!("{outcome}\n{}", descendant_effects(&rewrite.rewritten))
    }
}

/// A status icon's tooltip: the state in one word.
pub(crate) fn status_tooltip(status: pick_list::Status) -> SharedString {
    match status {
        pick_list::Status::Added => t!("status-added"),
        pick_list::Status::Modified => t!("status-modified"),
        pick_list::Status::Deleted => t!("status-deleted"),
        pick_list::Status::Renamed => t!("status-moved"),
        pick_list::Status::Copied => t!("status-copied"),
        pick_list::Status::Conflict => t!("status-conflicted"),
    }
}

texts! {
    /// The directory window's title without `--title` (spec §6.1).
    dirs_title = "dirs-title";
    dirs_empty = "dirs-empty";
}

/// A listed file: how it changed as an icon, its path, and in the trail a rename's old path
/// and, for a file that does not open, why. Its accessible name says the change in words.
pub(crate) fn dir_file(file: &ChangedFile) -> Row {
    let from = file.from().unwrap_or_default();
    let (status, change) = match &file.change {
        Change::Added => (pick_list::Status::Added, t!("file-added").to_string()),
        Change::Deleted => (pick_list::Status::Deleted, t!("file-deleted").to_string()),
        Change::Modified => (pick_list::Status::Modified, t!("file-modified").to_string()),
        Change::Renamed { .. } => (pick_list::Status::Renamed, t!("file-renamed", from = &from)),
        Change::Copied { .. } => (pick_list::Status::Copied, t!("file-copied", from = &from)),
    };
    // A copy shows the added icon, so its trail names the copy.
    let shown_from = match &file.change {
        Change::Renamed { .. } => Some(t!("file-from", from = &from)),
        Change::Copied { .. } => Some(change.clone()),
        Change::Added | Change::Deleted | Change::Modified => None,
    };
    let why = match file.content {
        Content::Text => None,
        Content::Undecodable => Some(t!("file-undecodable")),
        Content::Binary => Some(t!("file-binary")),
        Content::Special => Some(t!("file-special")),
    };
    let path = file.display_path();
    let label = match &why {
        Some(why) => t!("file-label-why", path = path.as_str(), change = &change, why = why),
        None => t!("file-label", path = path.as_str(), change = &change),
    };
    let trail = [shown_from.as_deref(), why.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    let shown = if file.change == Change::Deleted { Shown::Deleted } else { Shown::Path };
    Row {
        status: Some(status),
        text: path.into(),
        shown,
        trail: (!trail.is_empty()).then(|| trail.into()),
        label: Some(label.into()),
        ..Row::default()
    }
}

/// The directory window's summary: the files listed and, for a repository's changes, the
/// revision they are compared with.
pub(crate) fn dirs_summary(files: usize, against: Option<&str>) -> String {
    let files = t!("count-files", count = files);
    match against {
        Some(rev) => t!("dirs-against", files = &files, rev = rev),
        None => files,
    }
}

/// The history window's title (spec §7.5.2).
pub(crate) fn history_title(path: &str) -> String {
    t!("history-title", path = path)
}

/// A listed version, as `history --list` prints it: the operation where it appeared, the
/// time, the description, and a mark when the path holds no file there.
pub(crate) fn history_version(version: &FileVersion) -> Row {
    let short: String = version.operation.chars().take(SHORT_OPERATION).collect();
    let mark = match version.state {
        VersionState::File => None,
        VersionState::Conflict => Some(t!("version-conflict")),
        VersionState::Absent => Some(t!("version-absent")),
        VersionState::Other => Some(t!("version-other")),
    };
    Row {
        lead: Some(format!("{short} {}", version.time).into()),
        text: version.description.clone().into(),
        trail: mark,
        ..Row::default()
    }
}

/// The history window's summary: the versions listed.
pub(crate) fn history_summary(versions: usize) -> String {
    t!("count-versions", count = versions)
}

texts! {
    /// The absorb window's title (spec §7.5.3).
    absorb_title = "absorb-title";
    absorb_empty = "absorb-empty";
    absorb_apply = "absorb-apply";
}

/// A listed hunk, as `absorb --list` prints it, with the commit chosen for it now: the
/// listing's reason when it stays as listed, `kept` when the choice keeps a hunk the listing
/// moved. The trail is the command line's text, so it reads the same in every language.
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
        shown: Shown::Path,
        trail: Some(
            format!(
                "left {}..{} right {}..{} {goes}",
                left.start, left.end, right.start, right.end
            )
            .into(),
        ),
        ..Row::default()
    }
}

/// The absorb window's summary: the hunks listed.
pub(crate) fn absorb_summary(hunks: usize) -> String {
    t!("count-hunks", count = hunks)
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
        ..Row::default()
    }
}

/// The working copy as a choice, with the number of hunks kept when any.
pub(crate) fn absorb_keep(hunks: usize) -> Row {
    Row { text: t!("absorb-keep"), trail: with_hunks(hunks), ..Row::default() }
}

fn with_hunks(hunks: usize) -> Option<SharedString> {
    (hunks > 0).then(|| format!("· {}", t!("count-hunks", count = hunks)).into())
}

/// What Apply did: the hunks moved and the commits they went into, then each commit whose
/// conflicts changed.
pub(crate) fn absorb_note(absorbed: &Absorbed) -> String {
    let moved: Vec<&str> =
        absorbed.hunks.iter().filter_map(AbsorbHunk::target).map(|t| t.commit.as_str()).collect();
    let mut commits = moved.clone();
    commits.sort_unstable();
    commits.dedup();
    let summary = t!(
        "absorb-note",
        hunks = &t!("count-hunks", count = moved.len()),
        commits = &t!("count-commits", count = commits.len()),
    );
    let mut changed =
        absorbed.rewrite.rewritten.iter().filter(|effect| effect.before != effect.after).peekable();
    let note = if changed.peek().is_none() {
        summary
    } else {
        format!("{summary}\n{}", descendant_effects(changed))
    };
    if absorbed.rewrite.stale { format!("{note}\n{}", t!("absorb-stale")) } else { note }
}

/// A path absorb cannot split, and why.
pub(crate) fn absorb_skipped(path: &str, reason: &str) -> String {
    t!("absorb-skipped", path = path, reason = reason)
}

/// The open dialog's window title on `mode`'s tab.
pub(crate) fn open_title(mode: Mode) -> SharedString {
    match mode {
        Mode::Merge => t!("open-title-merge"),
        Mode::Diff => t!("open-title-diff"),
    }
}

/// The name of `mode`'s tab in the open dialog.
pub(crate) fn mode_name(mode: Mode) -> SharedString {
    match mode {
        Mode::Merge => t!("mode-merge"),
        Mode::Diff => t!("mode-diff"),
    }
}

/// The open dialog's start button on `mode`'s tab, named for what it does.
pub(crate) fn start(mode: Mode) -> SharedString {
    match mode {
        Mode::Merge => t!("start-merge"),
        Mode::Diff => t!("start-diff"),
    }
}

/// The message for a key of gpui-component's own catalogue, for the text of its that triad's
/// windows show: the search panel's Replace buttons and a text field's right-click menu.
pub(crate) fn component_message(key: &str) -> Option<&'static str> {
    let id = match key {
        "Input.Replace" => "component-replace",
        "Input.Replace All" => "component-replace-all",
        "Input.Clear" => "component-clear",
        "Input.Cut" => "edit-cut",
        "Input.Copy" => "edit-copy",
        "Input.Paste" => "edit-paste",
        "Input.Select All" => "edit-select-all",
        _ => return None,
    };
    Some(id)
}

/// The keys of gpui-component's catalogue that [`component_message`] names.
pub(crate) const COMPONENT_KEYS: [&str; 7] = [
    "Input.Replace",
    "Input.Replace All",
    "Input.Clear",
    "Input.Cut",
    "Input.Copy",
    "Input.Paste",
    "Input.Select All",
];

/// A slot's name in the open dialog.
pub(crate) fn slot_name(slot: Slot) -> SharedString {
    match slot {
        Slot::Base => t!("slot-base"),
        Slot::Left => t!("slot-left"),
        Slot::Right => t!("slot-right"),
        Slot::Output => t!("slot-output"),
    }
}

/// The accessible label of a slot's Choose button.
pub(crate) fn choose_label(slot: Slot) -> SharedString {
    match slot {
        Slot::Base => t!("choose-base"),
        Slot::Left => t!("choose-left"),
        Slot::Right => t!("choose-right"),
        Slot::Output => t!("choose-output"),
    }
}

/// Why the open dialog cannot start: `slot` is empty.
pub(crate) fn missing_slot(slot: Slot) -> String {
    t!("missing-slot", slot = &slot_name(slot))
}

/// Menu and menu item names, in the platform's title case.
pub(crate) mod menu {
    use gpui_kit::SharedString;

    use crate::i18n::t;

    texts! {
        settings = "menu-settings";
        save = "menu-save";
        save_and_exit = "menu-save-and-exit";
        save_copy_as = "menu-save-copy-as";
        cancel = "menu-cancel";
        edit = "menu-edit";
        merge = "menu-merge";
        go = "menu-go";
        view = "menu-view";
        undo = "menu-undo";
        redo = "menu-redo";
        find = "menu-find";
        go_to_line = "menu-go-to-line";
        copy_as_patch = "menu-copy-as-patch";
        unified = "menu-unified";
        copy_difference_as_patch = "menu-copy-difference-as-patch";
        toggle_left = "menu-toggle-left";
        toggle_base = "menu-toggle-base";
        toggle_right = "menu-toggle-right";
        clear_choice = "menu-clear-choice";
        pick_selected_lines = "menu-pick-selected-lines";
        take_base = "menu-take-base";
        take_left = "menu-take-left";
        take_right = "menu-take-right";
        take_left_then_right = "menu-take-left-then-right";
        take_right_then_left = "menu-take-right-then-left";
        take_sorted_union = "menu-take-sorted-union";
        take_left_all_unresolved = "menu-take-left-all-unresolved";
        take_right_all_unresolved = "menu-take-right-all-unresolved";
        take_whole_left = "menu-take-whole-left";
        take_whole_right = "menu-take-whole-right";
        carry_moved_edit = "menu-carry-moved-edit";
        drop_duplicate = "menu-drop-duplicate";
        next_difference = "menu-next-difference";
        previous_difference = "menu-previous-difference";
        next_conflict = "menu-next-conflict";
        previous_conflict = "menu-previous-conflict";
        go_to_move_end = "menu-go-to-move-end";
        base_pane = "menu-base-pane";
        blame = "menu-blame";
        syntax = "menu-syntax";
        soft_wrap = "menu-soft-wrap";
        whitespace = "menu-whitespace";
        collapse_unchanged = "menu-collapse-unchanged";
        scroll_sync = "menu-scroll-sync";
    }
}

/// A toolbar button's text.
pub(crate) fn tool_label(tool: Tool) -> Option<SharedString> {
    tool_label_in(i18n::current(), tool).map(i18n::shared)
}

/// A toolbar button's text in `locale`.
pub(crate) fn tool_label_in(locale: Locale, tool: Tool) -> Option<Cow<'static, str>> {
    let id = match tool {
        Tool::PreviousDifference => "tool-label-difference",
        Tool::PreviousConflict => "tool-label-conflict",
        Tool::Left => "tool-label-left",
        Tool::Right => "tool-label-right",
        Tool::Base
        | Tool::LeftThenRight
        | Tool::Clear
        | Tool::NextDifference
        | Tool::NextConflict
        | Tool::RightThenLeft
        | Tool::Undo
        | Tool::Redo
        | Tool::BasePane
        | Tool::Find
        | Tool::GoToLine
        | Tool::Syntax
        | Tool::SoftWrap
        | Tool::Whitespace => return None,
    };
    Some(i18n::format_in(locale, id, &[]))
}

/// The More button's label in `locale`.
pub(crate) fn more_in(locale: Locale) -> Cow<'static, str> {
    i18n::format_in(locale, "more", &[])
}

texts! {
    /// The More button's label, and the accessible label and tooltip of it and its menu.
    more = "more";
    more_menu = "more-menu";
    /// The rows of the More menu.
    base = "more-base";
    left_then_right = "more-left-then-right";
    right_then_left = "more-right-then-left";
    none = "more-none";
}

/// A toolbar button's accessible label and tooltip.
pub(crate) fn tool_name(tool: Tool) -> SharedString {
    match tool {
        Tool::PreviousDifference => t!("tool-previous-difference"),
        Tool::NextDifference => t!("tool-next-difference"),
        Tool::PreviousConflict => t!("tool-previous-conflict"),
        Tool::NextConflict => t!("tool-next-conflict"),
        Tool::Left => t!("tool-left"),
        Tool::Base => t!("tool-base"),
        Tool::Right => t!("tool-right"),
        Tool::LeftThenRight => t!("tool-left-then-right"),
        Tool::RightThenLeft => t!("tool-right-then-left"),
        Tool::Clear => t!("tool-clear"),
        Tool::Undo => t!("tool-undo"),
        Tool::Redo => t!("tool-redo"),
        Tool::BasePane => t!("tool-base-pane"),
        Tool::Find => t!("tool-find"),
        Tool::GoToLine => t!("tool-go-to-line"),
        Tool::Syntax => t!("tool-syntax"),
        Tool::SoftWrap => t!("tool-soft-wrap"),
        Tool::Whitespace => t!("tool-whitespace"),
    }
}

/// Whether a pick target's region waits on the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pending {
    /// Resolved, or never a conflict.
    No,
    /// An unresolved conflict.
    Unresolved,
    /// An unresolved conflict a parent commit holds too (spec §7.5.1).
    Inherited,
}

/// A pick target's accessible label: the pane's title, where its region sits, and whether
/// the region is an unresolved conflict, and an inherited one. The label carries that state
/// because macOS reads a mixed toggle as on.
pub(crate) fn pick_label(
    title: &str,
    region: RegionIx,
    regions: usize,
    pending: Pending,
) -> String {
    let region = region.next().get();
    match pending {
        Pending::No => t!("pick-label", title = title, region = region, regions = regions),
        Pending::Unresolved => {
            t!("pick-label-unresolved", title = title, region = region, regions = regions)
        }
        Pending::Inherited => {
            t!("pick-label-inherited", title = title, region = region, regions = regions)
        }
    }
}

/// A line number's accessible label when it picks its line: the line's number and the
/// pane's title. Its toggle state says whether the line is picked.
pub(crate) fn line_pick_label(title: &str, line: usize) -> String {
    t!("line-pick-label", line = line, title = title)
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
    match (conflicts, unresolved, number) {
        (0, _, _) => None,
        (all, left, Some(number)) => {
            Some(current_conflict_badge(i18n::current(), number, all, sides, left))
        }
        (1, 0, None) => Some(t!("badge-resolved-one").to_string()),
        (all, 0, None) => Some(t!("badge-resolved-all", count = all)),
        (all, left, None) => Some(t!("badge-unresolved-of", unresolved = left, count = all)),
    }
}

/// The conflict badge in `locale` while the caret is in conflict `number` of `all`, which
/// keeps `sides`, with `unresolved` conflicts left.
pub(crate) fn current_conflict_badge(
    locale: Locale,
    number: usize,
    all: usize,
    sides: Option<Sides>,
    unresolved: usize,
) -> String {
    let current = i18n::format_in(
        locale,
        "badge-conflict",
        &[("number", number.into()), ("count", all.into())],
    );
    let sides = sides.map(|Sides { kept, of }| {
        i18n::format_in(locale, "badge-sides", &[("kept", kept.into()), ("sides", of.into())])
    });
    let left = if unresolved == 0 {
        i18n::format_in(locale, "badge-all-resolved", &[])
    } else {
        i18n::format_in(locale, "unresolved", &[("count", unresolved.into())])
    };
    [Some(current), sides, Some(left)].into_iter().flatten().collect::<Vec<_>>().join(ITEMS)
}

/// A side-pane cell's accessible label: its pane, its line number and its text, or that the
/// pane has no line in this row, with the base lines it deleted there, if any.
pub(crate) fn cell_label(
    title: &str,
    line: Option<(usize, &str)>,
    deleted: Option<usize>,
) -> String {
    match (line, deleted) {
        (Some((n, text)), _) => t!("cell-line", title = title, line = n + 1, text = text),
        (None, None) => t!("cell-no-line", title = title),
        (None, Some(lines)) => {
            t!("cell-no-line-deleted", title = title, deleted = &deleted_label(lines))
        }
    }
}

/// A pane's role: the chip beside a file name, or the title of a pane without one.
/// A diff side's title at `left`'s revision, marked when the file does not exist there.
pub(crate) fn revision_title(left: &LeftAt) -> String {
    let name = match &left.revision {
        Revision::Named(name) => Cow::Borrowed(name.as_str()),
        Revision::Operation(short) => Cow::Owned(t!("at-operation", operation = short)),
    };
    if left.exists { name.into_owned() } else { no_file(&name) }
}

/// The title `name` marked as a side where the file does not exist.
pub(crate) fn no_file(name: &str) -> String {
    t!("no-file", name = name)
}

pub(crate) fn role(role: Role) -> Cow<'static, str> {
    let locale = i18n::current();
    match role {
        Role::Left => i18n::format_in(locale, "role-left", &[]),
        Role::Right => i18n::format_in(locale, "role-right", &[]),
        Role::Base => i18n::format_in(locale, "role-base", &[]),
        Role::Side(n) => i18n::format_in(locale, "role-side", &[("number", n.into())]),
        Role::BaseOf(n) => i18n::format_in(locale, "role-base-of", &[("number", n.into())]),
        Role::Result => i18n::format_in(locale, "role-result", &[]),
    }
}

/// The note on a result placeholder: the base lines the `roles` deleted, or with no roles,
/// that the region picks nothing and drops its longest input's lines.
pub(crate) fn placeholder_label<'a>(
    lines: usize,
    roles: impl IntoIterator<Item = &'a str>,
) -> String {
    let roles = roles.into_iter().collect::<Vec<_>>().join(", ");
    if roles.is_empty() {
        t!("placeholder-nothing", count = lines)
    } else {
        t!("placeholder-deleted", count = lines, roles = &roles)
    }
}

/// The note on the first filler row of a side that deleted `lines` base lines.
pub(crate) fn deleted_label(lines: usize) -> String {
    t!("lines-deleted", count = lines)
}

/// The previous-match button of the input pane titled `title`.
pub(crate) fn previous_match(title: &str) -> String {
    t!("previous-match", title = title)
}

/// The next-match button of the input pane titled `title`.
pub(crate) fn next_match(title: &str) -> String {
    t!("next-match", title = title)
}

/// An input pane's match count: match `n` of `total`.
pub(crate) fn match_count(n: usize, total: usize) -> String {
    t!("match-count", number = n, count = total)
}

/// The go-to-line field's prompt, for a result of `lines` lines.
pub(crate) fn go_to_line_prompt(lines: usize) -> String {
    t!("go-to-line-prompt", last = lines.max(1))
}

/// Go to Line's help while its field names no line: the caret's line, from 1, of `lines`.
pub(crate) fn go_to_line_current(line: usize, lines: usize) -> String {
    t!("go-to-line-current", line = line, last = lines.max(1))
}

/// Go to Line's help while its field names a line: the line Enter goes to, from 1.
pub(crate) fn go_to_line_target(line: usize) -> String {
    t!("go-to-line-target", line = line)
}

pub(crate) fn encoding(encoding: Encoding) -> SharedString {
    match encoding {
        Encoding::Utf8 => "UTF-8".into(),
        Encoding::Utf8Bom => t!("encoding-utf8-bom"),
        Encoding::Utf16Le => "UTF-16 LE".into(),
        Encoding::Utf16Be => "UTF-16 BE".into(),
        Encoding::Windows1252 => "Windows-1252".into(),
    }
}

const fn line_ending(ending: LineEnding) -> &'static str {
    match ending {
        LineEnding::Lf => "LF",
        LineEnding::CrLf => "CRLF",
    }
}

/// The status bar's item for how lines compare. Every whitespace mode drops line endings.
pub(crate) fn comparison(mode: Comparison<'_>) -> SharedString {
    match (mode.whitespace, mode.line_endings) {
        (Whitespace::None, LineEndingRule::Compare) => t!("compare-whitespace"),
        (Whitespace::None, LineEndingRule::Ignore) => t!("compare-line-endings"),
        (Whitespace::Trailing, _) => t!("compare-trailing"),
        (Whitespace::Amount, _) => t!("compare-amount"),
        (Whitespace::All, _) => t!("compare-all"),
    }
}

/// The diff window's items for how lines compare: [`comparison`]'s, then `case ignored`,
/// `patterns ignored` and `blank lines ignored` where they apply.
pub(crate) fn diff_comparison(options: &DiffOptions) -> String {
    let mode = options.comparison();
    [
        Some(comparison(mode)),
        (mode.case == CaseRule::Ignore).then(|| t!("case-ignored")),
        (!mode.patterns.is_empty()).then(|| t!("patterns-ignored")),
        (options.blank_lines == BlankLineRule::Ignore).then(|| t!("blank-lines-ignored")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(ITEMS)
}

texts! {
    /// The diff window's banner over copies from `jj diff --tool`.
    jj_diff_banner = "jj-diff-banner";
    /// The copy buttons' accessible labels.
    copy_to_right_label = "copy-to-right";
    copy_to_left_label = "copy-to-left";
}

/// The marker after a diff pane's title while it has unsaved edits.
pub(crate) const MODIFIED: &str = " ●";
/// The copy buttons between the diff panes: left into right, and right into left.
pub(crate) const COPY_TO_RIGHT: &str = "→";
pub(crate) const COPY_TO_LEFT: &str = "←";

/// The initial a move pill gives `side`'s pane.
fn side_initial(side: Side) -> SharedString {
    match side {
        Side::Left => t!("initial-left"),
        Side::Right => t!("initial-right"),
    }
}

/// The pill over one end of a moved block in `side`'s pane: the other pane's initial and the
/// 1-based line its end starts on, with an arrow toward that pane.
pub(crate) fn move_pill(side: Side, other_line: usize) -> String {
    let line = other_line + 1;
    let initial = side_initial(side.other());
    match side {
        Side::Left => format!("{initial}{line} →"),
        Side::Right => format!("← {initial}{line}"),
    }
}

/// The pill over one end of a moved block in a merge window's top pane: an arrow toward the
/// other end's row, up or down, then the other end's input initial and 1-based line.
pub(crate) fn merge_move_pill(other: Source, other_line: usize, up: bool) -> String {
    let arrow = if up { "↑" } else { "↓" };
    let initial = match other {
        Source::Base(_) => t!("initial-base"),
        Source::Side(0) => t!("initial-left"),
        Source::Side(_) => t!("initial-right"),
    };
    format!("{arrow} {initial}{}", other_line + 1)
}

/// The accessible label of the pill [`merge_move_pill`] writes.
pub(crate) fn merge_move_pill_label(other: Source, other_line: usize) -> String {
    let line = other_line + 1;
    match other {
        Source::Base(_) => t!("moved-to-base", line = line),
        Source::Side(0) => t!("moved-to-left", line = line),
        Source::Side(_) => t!("moved-to-right", line = line),
    }
}

/// The accessible label of the pill [`move_pill`] writes.
pub(crate) fn move_pill_label(side: Side, other_line: usize) -> String {
    let line = other_line + 1;
    match side.other() {
        Side::Left => t!("moved-to-left", line = line),
        Side::Right => t!("moved-to-right", line = line),
    }
}

/// A diff pane's accessible label: its side and its file.
pub(crate) fn diff_pane_label(left: bool, name: &str) -> String {
    if left { t!("diff-pane-left", name = name) } else { t!("diff-pane-right", name = name) }
}

/// The unified pane's accessible name: both sides' files.
pub(crate) fn unified_pane_label(left: &str, right: &str) -> String {
    t!("diff-pane-unified", left = left, right = right)
}

/// The status bars' first item: the repository the files lie in (spec §5).
fn repo(kind: Option<RepoKind>) -> SharedString {
    match kind {
        Some(RepoKind::Git) => t!("repo-git"),
        Some(RepoKind::Jj) => t!("repo-jj"),
        None => t!("repo-none"),
    }
}

/// The text of a fold row, which stands in for `hidden` unchanged lines.
pub(crate) fn unchanged_lines(hidden: usize) -> String {
    t!("count-unchanged-lines", count = hidden)
}

/// The diff window's status bar text.
pub(crate) fn diff_status(kind: Option<RepoKind>, differences: usize, comparison: &str) -> String {
    [repo(kind).to_string(), t!("count-differences", count = differences), comparison.to_owned()]
        .join(ITEMS)
}

/// The status bar's item after the repository: whose merge, and whether it merges words.
fn mode(status: &Status) -> SharedString {
    match (status.semantics, status.hunk_level) {
        (Semantics::Git, HunkLevel::Line) => t!("mode-git"),
        (Semantics::Jj, HunkLevel::Line) => t!("mode-jj"),
        (Semantics::Git, HunkLevel::Word) => t!("mode-git-word"),
        (Semantics::Jj, HunkLevel::Word) => t!("mode-jj-word"),
    }
}

fn divergence(divergence: Divergence, semantics: Semantics) -> SharedString {
    match (divergence, semantics) {
        (Divergence::Whitespace, _) => t!("diverge-whitespace-jj"),
        (Divergence::Case, Semantics::Git) => t!("diverge-case-git"),
        (Divergence::Case, Semantics::Jj) => t!("diverge-case-jj"),
        (Divergence::Patterns, Semantics::Git) => t!("diverge-patterns-git"),
        (Divergence::Patterns, Semantics::Jj) => t!("diverge-patterns-jj"),
        (Divergence::SameChangeKept, _) => t!("diverge-same-change-git"),
        (Divergence::WordLevel, _) => t!("diverge-word-git"),
        (Divergence::DeepRefine, Semantics::Git) => t!("diverge-deep-git"),
        (Divergence::DeepRefine, Semantics::Jj) => t!("diverge-deep-jj"),
        (Divergence::SyntaxBoundaries, Semantics::Git) => t!("diverge-syntax-git"),
        (Divergence::SyntaxBoundaries, Semantics::Jj) => t!("diverge-syntax-jj"),
    }
}

texts! {
    /// The status bar's warning, in the conflict tone, that the saved output mixes line
    /// endings.
    mixed_endings = "mixed-endings";
}

/// The status bar's note after Save Copy As: how many conflicts the copy holds.
pub(crate) fn copy_saved(unresolved: usize) -> String {
    t!("copy-saved", count = unresolved)
}

/// The status bar's note after Save: how many conflicts the output holds.
pub(crate) fn saved(unresolved: usize) -> String {
    t!("saved", count = unresolved)
}

/// The diff window's banner after a save jj did not snapshot, and why.
pub(crate) fn snapshot_failed(error: &str) -> SharedString {
    t!("snapshot-failed-banner", error = error).into()
}

/// The status bar's text (spec §5).
pub(crate) fn status(kind: Option<RepoKind>, status: &Status) -> String {
    let encoding = encoding(status.encoding);
    let encoding = if status.encoding_mismatch {
        t!("inputs-differ", encoding = &encoding)
    } else {
        encoding.to_string()
    };
    let lead = [repo(kind), mode(status)]
        .into_iter()
        .chain(status.divergences().map(|d| divergence(d, status.semantics)))
        .map(SharedString::into);
    let counts = [
        Some(t!("count-differences", count = status.differences)),
        Some(t!("count-conflicts", count = status.conflicts)),
        (status.word_merged > 0).then(|| t!("word-merged", count = status.word_merged)),
        (status.duplicated > 0).then(|| t!("count-duplicated-blocks", count = status.duplicated)),
        Some(t!("unresolved", count = status.unresolved)),
    ]
    .into_iter()
    .flatten();
    let comparison = comparison(Comparison {
        whitespace: status.whitespace,
        line_endings: status.line_endings,
        ..Comparison::default()
    });
    lead.chain(counts)
        .chain([encoding, line_ending(status.line_ending).to_owned(), comparison.into()])
        .collect::<Vec<String>>()
        .join(ITEMS)
}

/// A choice's item in the result pane's right-click menu.
pub(crate) fn choice_label(choice: Choice) -> SharedString {
    match choice {
        Choice::Nothing => t!("choice-none"),
        Choice::Base => t!("choice-base"),
        Choice::Left => t!("choice-left"),
        Choice::Right => t!("choice-right"),
        Choice::LeftThenRight => t!("choice-left-then-right"),
        Choice::RightThenLeft => t!("choice-right-then-left"),
    }
}

/// An item of the result pane's right-click menu.
pub(crate) fn menu_label(action: MenuAction) -> SharedString {
    match action {
        MenuAction::Take(choice) => choice_label(choice),
        MenuAction::SortedUnion => t!("action-sorted-union"),
        MenuAction::CarryEdit { .. } => t!("menu-carry-moved-edit"),
        MenuAction::DropDuplicate { .. } => t!("menu-drop-duplicate"),
    }
}

/// A suggestion's item in the right-click menu, above the choices (spec §4.4).
pub(crate) fn suggested_label(action: MenuAction) -> SharedString {
    match action {
        MenuAction::CarryEdit { from, line } => {
            t!("suggested-carry", from = &choice_label(from), line = line)
        }
        MenuAction::DropDuplicate { line } => t!("suggested-drop", line = line),
        MenuAction::Take(_) | MenuAction::SortedUnion => {
            t!("suggested", action = &menu_label(action))
        }
    }
    .into()
}

/// The editing items after the choices in the right-click menu.
pub(crate) mod edit_menu {
    use gpui_kit::SharedString;

    use crate::i18n::t;

    texts! {
        cut = "edit-cut";
        copy = "edit-copy";
        paste = "edit-paste";
        select_all = "edit-select-all";
    }
}

#[cfg(test)]
mod tests {
    use merge_session::jj_native::Origin;

    use super::*;

    /// The message ids this file names outside its tests: in `t!`, `texts!`, `format_in` (after
    /// its `locale`) and the match arms that pick an id.
    fn named_ids() -> std::collections::BTreeSet<&'static str> {
        const FILE: &str = include_str!("strings.rs");
        const OPENERS: [&str; 4] = ["t!(", "locale,", " = ", "=> "];
        const ERRORS: &str = include_str!("error_text.rs");
        let source = &FILE[..FILE.find("#[cfg(test)]").expect("a test module")];
        let mut ids = std::collections::BTreeSet::new();
        for opener in OPENERS {
            for (start, _) in source.match_indices(opener) {
                let Some(rest) = source[start + opener.len()..].trim_start().strip_prefix('"')
                else {
                    continue;
                };
                let id = &rest[..rest.find('"').expect("a closing quote")];
                let is_id = !id.is_empty()
                    && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                // A `texts!` entry is `name = "id";` on its own line; a constant is not one.
                let line = &source[source[..start].rfind('\n').map_or(0, |end| end + 1)..start];
                let in_texts = opener != " = "
                    || (rest[id.len()..].starts_with("\";")
                        && line.trim_start().chars().all(|c| c.is_ascii_lowercase() || c == '_'));
                if is_id && in_texts {
                    ids.insert(id);
                }
            }
        }
        let errors = ERRORS.find("#[cfg(test)]").map_or(ERRORS, |end| &ERRORS[..end]);
        for (start, _) in errors.match_indices("\"err-") {
            let rest = &errors[start + 1..];
            ids.insert(&rest[..rest.find('"').expect("a closing quote")]);
        }
        ids
    }

    #[test]
    fn every_message_this_file_names_is_in_the_english_catalogue_and_none_is_unused() {
        const ENGLISH: &str = include_str!("../locales/en.ftl");
        let english: std::collections::BTreeSet<&str> = ENGLISH
            .lines()
            .filter(|line| line.starts_with(|c: char| c.is_ascii_lowercase()))
            .filter_map(|line| line.split_once(" =").map(|(id, _)| id))
            .collect();
        assert!(english.len() > 200, "{}", english.len());
        let named = named_ids();
        assert_eq!(
            named.difference(&english).collect::<Vec<_>>(),
            Vec::<&&str>::new(),
            "unknown ids"
        );
        assert_eq!(
            english.difference(&named).collect::<Vec<_>>(),
            Vec::<&&str>::new(),
            "unused ids"
        );
    }

    #[test]
    fn a_merge_move_pill_points_up_or_down_at_the_other_ends_line() {
        assert_eq!(merge_move_pill(Source::Base(0), 0, true), "↑ B1");
        assert_eq!(merge_move_pill(Source::Side(0), 39, false), "↓ L40");
        assert_eq!(merge_move_pill(Source::Side(1), 4, true), "↑ R5");
        assert_eq!(
            merge_move_pill_label(Source::Base(0), 11),
            "Moved block: go to its other end, base line 12"
        );
        assert_eq!(
            merge_move_pill_label(Source::Side(0), 39),
            "Moved block: go to its other end, left line 40"
        );
        assert_eq!(
            merge_move_pill_label(Source::Side(1), 0),
            "Moved block: go to its other end, right line 1"
        );
    }

    #[test]
    fn a_move_pill_names_the_other_panes_line_and_points_at_it() {
        assert_eq!(move_pill(Side::Left, 114), "R115 →");
        assert_eq!(move_pill(Side::Right, 36), "← L37");
        assert_eq!(move_pill(Side::Left, 0), "R1 →");
        assert_eq!(
            move_pill_label(Side::Left, 114),
            "Moved block: go to its other end, right line 115"
        );
        assert_eq!(
            move_pill_label(Side::Right, 36),
            "Moved block: go to its other end, left line 37"
        );
    }
    use merge_core::merge3::{Refine, SameChange};
    use merge_core::pattern::Patterns;

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
                shown: Shown::Path,
                trail: Some("left 2..3 right 2..4 stays: would conflict in mmmm".into()),
                ..Row::default()
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
                trail: Some("· 1 hunk".into()),
                ..Row::default()
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
                trail: Some("(absent)".into()),
                ..Row::default()
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
            comparison(Comparison { whitespace, line_endings, ..Comparison::default() })
        };
        assert_eq!(by(Whitespace::None, LineEndingRule::Compare), "whitespace significant");
        assert_eq!(by(Whitespace::None, LineEndingRule::Ignore), "line endings ignored");
        assert_eq!(by(Whitespace::Trailing, LineEndingRule::Ignore), "trailing whitespace ignored");
    }

    #[test]
    fn the_diff_status_adds_an_item_when_case_is_ignored() {
        let ignoring = DiffOptions { case: CaseRule::Ignore, ..DiffOptions::default() };
        assert_eq!(diff_comparison(&ignoring), "whitespace significant · case ignored");
        let spaces = DiffOptions { whitespace: Whitespace::All, ..ignoring };
        assert_eq!(diff_comparison(&spaces), "all whitespace ignored · case ignored");
        assert_eq!(diff_comparison(&DiffOptions::default()), "whitespace significant");
    }

    #[test]
    fn the_diff_status_adds_an_item_when_patterns_are_in_force() {
        let digits = Patterns::new([r"\d"]).expect("valid");
        let cutting = DiffOptions { patterns: digits, ..DiffOptions::default() };
        assert_eq!(diff_comparison(&cutting), "whitespace significant · patterns ignored");
        let both = DiffOptions { case: CaseRule::Ignore, ..cutting };
        assert_eq!(
            diff_comparison(&both),
            "whitespace significant · case ignored · patterns ignored"
        );
    }

    #[test]
    fn the_diff_status_adds_an_item_when_blank_lines_are_ignored() {
        let blank = DiffOptions { blank_lines: BlankLineRule::Ignore, ..DiffOptions::default() };
        assert_eq!(diff_comparison(&blank), "whitespace significant · blank lines ignored");
        let every = DiffOptions {
            case: CaseRule::Ignore,
            patterns: Patterns::new([r"\d"]).expect("valid"),
            whitespace: Whitespace::Trailing,
            ..blank
        };
        assert_eq!(
            diff_comparison(&every),
            "trailing whitespace ignored · case ignored · patterns ignored · blank lines ignored"
        );
    }

    #[test]
    fn ignore_patterns_show_as_a_divergence_under_either_semantics() {
        let git = Status { patterns: 1, ..with(1, 0, 0) };
        assert!(
            status(None, &git).starts_with(
                "no repo · git merge · patterns ignored, unlike git · 1 difference · "
            ),
            "{}",
            status(None, &git)
        );
        let jj = Status { semantics: Semantics::Jj, ..git };
        assert!(
            status(None, &jj)
                .starts_with("no repo · jj merge · patterns ignored, unlike jj · 1 difference · "),
            "{}",
            status(None, &jj)
        );
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
            patterns: 0,
            word_merged: 0,
            duplicated: 0,
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

    #[test]
    fn duplicated_blocks_show_only_when_there_are_some() {
        let one = Status { duplicated: 1, ..with(3, 0, 0) };
        assert_eq!(
            status(None, &one),
            "no repo · git merge · 3 differences · 0 conflicts · 1 duplicated block · 0 unresolved · \
             UTF-8 · LF · whitespace significant"
        );
        let two = Status { duplicated: 2, word_merged: 1, ..with(3, 1, 0) };
        assert!(
            status(None, &two).contains(" · 1 word-merged · 2 duplicated blocks · 0 unresolved"),
            "{}",
            status(None, &two)
        );
        assert!(!status(None, &with(3, 0, 0)).contains("duplicated"));
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

    /// A listed commit described `merge` with a body, made 2001-02-03T04:05:06Z at UTC+01:00.
    fn listed_commit() -> StackCommit {
        StackCommit {
            id: vcs::CommitId::new("1a2b3c4d5e6f".to_owned()),
            change_id: "qpvuntsmwlqt".to_owned(),
            change: vcs::ChangeInfo {
                commit: "1a2b3c4d".to_owned(),
                change_id: Some("qpvuntsm".to_owned()),
                summary: Some("merge".to_owned()),
            },
            description: "merge\n\nThe body.\n".to_owned(),
            time: vcs::Timestamp { seconds: 981_173_106, offset_seconds: 3600 },
            working_copy: false,
            paths: Vec::new(),
        }
    }

    #[test]
    fn the_working_copy_commit_carries_jj_s_at_as_its_mark() {
        let mut commit = listed_commit();
        assert_eq!(stack_commit(&commit).mark, None);
        commit.working_copy = true;
        let row = stack_commit(&commit);
        assert_eq!(row.mark.as_deref(), Some("@"));
        assert_eq!(row.lead.as_deref(), Some("qpvuntsm 1a2b3c4d"));
        assert_eq!(row.label(), "working copy qpvuntsm 1a2b3c4d merge");
    }

    #[test]
    fn a_listed_commit_s_tooltip_gives_its_full_ids_time_and_description() {
        let commit = listed_commit();
        let expected = "Change ID: qpvuntsmwlqt\nCommit ID: 1a2b3c4d5e6f\n\
                        Date: 2001-02-03 05:05 +01:00\n\nmerge\n\nThe body.";
        assert_eq!(stack_commit_tooltip(&commit), expected);
        assert_eq!(stack_commit(&commit).hover.as_deref(), Some(expected));
    }

    #[test]
    fn a_listed_commit_without_a_description_says_so_as_jj_does() {
        let mut commit = listed_commit();
        commit.description = String::new();
        assert_eq!(
            stack_commit_tooltip(&commit),
            "Change ID: qpvuntsmwlqt\nCommit ID: 1a2b3c4d5e6f\n\
             Date: 2001-02-03 05:05 +01:00\n\n(no description set)"
        );
    }

    #[test]
    fn a_listed_commit_reads_change_commit_and_summary() {
        let mut commit = listed_commit();
        assert_eq!(stack_commit(&commit).label(), "qpvuntsm 1a2b3c4d merge");
        assert_eq!(stack_commit(&commit).text, "merge");
        assert!(!stack_commit(&commit).placeholder);
        // jj log's text for a commit without a description, muted in its place.
        commit.change.summary = None;
        assert_eq!(stack_commit(&commit).label(), "qpvuntsm 1a2b3c4d (no description set)");
        assert_eq!(stack_commit(&commit).text, "(no description set)");
        assert!(stack_commit(&commit).placeholder);
    }

    #[test]
    fn a_pick_target_says_whether_its_conflict_waits_and_is_inherited() {
        let region = RegionIx::new(1);
        assert_eq!(pick_label("Left", region, 3, Pending::No), "Left in region 2 of 3");
        assert_eq!(
            pick_label("Left", region, 3, Pending::Unresolved),
            "Left in region 2 of 3, unresolved"
        );
        assert_eq!(
            pick_label("Left", region, 3, Pending::Inherited),
            "Left in region 2 of 3, unresolved, inherited"
        );
    }

    #[test]
    fn an_inherited_path_names_where_its_conflict_arises() {
        assert_eq!(
            stack_inherited("a.txt", "royxmykx", "mzvwutvl"),
            "a.txt in royxmykx inherits its conflict from mzvwutvl. Resolving it in mzvwutvl \
             first clears it here; a resolution saved here first will likely conflict again."
        );
    }

    #[test]
    fn an_inherited_path_is_marked() {
        let path = |origin| StackPath { path: "src/a.rs".into(), origin, untouched: false };
        let listed = || Origin::Listed(vcs::CommitId::new("1a2b".to_owned()));
        assert_eq!(stack_path(&path(Origin::Here)).label(), "src/a.rs");
        assert_eq!(stack_path(&path(listed())).label(), "src/a.rs (inherited)");
        assert_eq!(stack_path(&path(Origin::Unlisted)).label(), "src/a.rs (inherited)");
        assert_eq!(stack_path(&path(listed())).text, "src/a.rs");
        assert_eq!(stack_path(&path(Origin::Here)).status, Some(pick_list::Status::Conflict));
        assert_eq!(stack_path(&path(Origin::Here)).shown, Shown::Path);
        assert!(!stack_path(&path(Origin::Here)).quiet);
        assert!(!stack_path(&path(listed())).quiet);
    }

    #[test]
    fn a_path_whose_conflict_is_its_parent_s_alone_is_quiet() {
        let path = StackPath {
            path: "src/a.rs".into(),
            origin: Origin::Listed(vcs::CommitId::new("1a2b".to_owned())),
            untouched: true,
        };
        assert!(stack_path(&path).quiet);
        assert_eq!(stack_path(&path).label(), "src/a.rs (inherited)");
    }

    fn changed(path: &str, change: Change, content: Content) -> ChangedFile {
        ChangedFile { path: path.into(), change, content }
    }

    #[test]
    fn a_listed_file_shows_its_change_as_an_icon_and_says_it_in_its_label() {
        let modified = dir_file(&changed("src/a.rs", Change::Modified, Content::Text));
        assert_eq!(
            modified,
            Row {
                status: Some(pick_list::Status::Modified),
                text: "src/a.rs".into(),
                shown: Shown::Path,
                label: Some("src/a.rs modified".into()),
                ..Row::default()
            }
        );
        let deleted = dir_file(&changed("gone.txt", Change::Deleted, Content::Text));
        assert_eq!(
            (deleted.status, deleted.shown, deleted.label()),
            (Some(pick_list::Status::Deleted), Shown::Deleted, "gone.txt deleted".into())
        );
        let binary = dir_file(&changed("img.png", Change::Added, Content::Binary));
        assert_eq!(
            (binary.status, binary.trail.clone(), binary.label()),
            (Some(pick_list::Status::Added), Some("binary".into()), "img.png added, binary".into())
        );
    }

    #[test]
    fn a_renamed_file_names_its_old_path_in_the_trail() {
        let renamed = Change::Renamed { from: "invoice/util.py".into() };
        let row = dir_file(&changed("invoice/helpers.py", renamed.clone(), Content::Text));
        assert_eq!(row.status, Some(pick_list::Status::Renamed));
        assert_eq!(row.trail, Some("from invoice/util.py".into()));
        assert_eq!(row.label(), "invoice/helpers.py renamed from invoice/util.py");
        let row = dir_file(&changed("b.bin", renamed, Content::Undecodable));
        assert_eq!(row.trail, Some("from invoice/util.py, undecodable".into()));
        assert_eq!(row.label(), "b.bin renamed from invoice/util.py, undecodable");
    }

    #[test]
    fn a_copied_file_shows_the_added_icon_and_names_its_source_in_the_trail() {
        let copied = Change::Copied { from: "invoice/totals.py".into() };
        let row = dir_file(&changed("invoice/tax.py", copied.clone(), Content::Text));
        assert_eq!(row.status, Some(pick_list::Status::Copied));
        assert_eq!(row.shown, Shown::Path);
        assert_eq!(row.trail, Some("copied from invoice/totals.py".into()));
        assert_eq!(row.label(), "invoice/tax.py copied from invoice/totals.py");
        let row = dir_file(&changed("b.bin", copied, Content::Binary));
        assert_eq!(row.trail, Some("copied from invoice/totals.py, binary".into()));
        assert_eq!(row.label(), "b.bin copied from invoice/totals.py, binary");
    }

    #[test]
    fn each_status_icon_has_a_tooltip_that_says_what_it_means() {
        use pick_list::Status;
        let tips = [
            Status::Added,
            Status::Modified,
            Status::Deleted,
            Status::Renamed,
            Status::Copied,
            Status::Conflict,
        ]
        .map(status_tooltip);
        assert_eq!(tips, ["Added", "Modified", "Deleted", "Moved", "Copied", "Conflicted",]);
    }

    #[test]
    fn the_summaries_count_what_each_window_lists() {
        assert_eq!(dirs_summary(1, None), "1 file");
        assert_eq!(dirs_summary(10, Some("@-")), "10 files against @-");
        assert_eq!(dirs_summary(0, None), "0 files");
        assert_eq!(history_summary(9), "9 versions");
        assert_eq!(stack_summary(1), "1 commit");
        assert_eq!(absorb_summary(2), "2 hunks");
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
        assert_eq!(
            suggested_label(MenuAction::DropDuplicate { line: 8 }).as_ref(),
            "Suggested: Drop Duplicate of Line 8"
        );
    }

    #[test]
    fn a_revision_title_names_the_revision_and_marks_a_missing_file() {
        let at = |revision, exists| revision_title(&LeftAt { revision, exists });
        let named = |name: &str| Revision::Named(name.to_owned());
        let operation = || Revision::Operation("0123456789ab".to_owned());
        assert_eq!(at(named("main"), true), "main");
        assert_eq!(at(named("HEAD~1"), false), "HEAD~1 (no file)");
        assert_eq!(at(operation(), true), "@ at operation 0123456789ab");
        assert_eq!(at(operation(), false), "@ at operation 0123456789ab (no file)");
        assert_eq!(no_file("gone.txt"), "gone.txt (no file)");
    }

    #[test]
    fn each_role_has_its_english_name() {
        let roles =
            [Role::Left, Role::Right, Role::Base, Role::Side(3), Role::BaseOf(2), Role::Result];
        assert_eq!(
            roles.map(role),
            ["Left", "Right", "Base", "Side 3", "Base 2", "Result"].map(Cow::Borrowed)
        );
        assert_eq!(roles.map(role), roles.map(Role::english), "the view model's names agree");
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
