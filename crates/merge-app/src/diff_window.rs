//! The two-way diff window (DESIGN.md §A6.5): the toolbar, two editable panes over a
//! [`DiffDocument`], and the status bar.
//!
//! As in the merge window, the core is the source of truth. Each editor makes the user's
//! edits first; the window sends them to the core and applies the core's corrections. Undo
//! and redo come from the core as patches. Regions are recomputed off the UI thread once
//! edits pause for [`DIFF_DEBOUNCE`], or at once when navigation needs them.
//!
//! Line origins load off the UI thread after the window shows (spec §7.3). Each pane keeps
//! its revision's annotation and the text it describes, and carries it onto the pane's text
//! in the same background step as each recompute.

use std::borrow::Cow;

use merge_core::unusual::escaped;
use std::ops::Range;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::base::ScrollbarMode;
use gpui_kit::base::input::{GutterMark, InputHighlighterFactory, SpacerNote};
use gpui_kit::component::input::{
    Editor, EditorState, InputEvent, RangeDecoration, RangeDecorationCollection,
    RangeDecorationStyle, RopeExt as _,
};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClickEvent, ClipboardItem, Context, Div,
    ElementId, Entity, Focusable as _, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, PromptButton, PromptLevel, Render, Role, SharedString, Size,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    WindowOptions, canvas, div, point, prelude::FluentBuilder as _, px,
};
use merge_config::settings::{Collapsing, Colours, DEFAULT_COLOURS, DiffLayout, Scrollbars};
use merge_core::diff::DiffOptions;
use merge_core::diff2::{DiffDocument, Edited, Kind2, Marking, Region2, Side};
use merge_core::edit::TextPatch;
use merge_core::index::RegionIx;
use merge_core::line_map::LineMap;
use merge_core::merge3::Source;
use merge_core::patch::{DEFAULT_CONTEXT, Scope, Titles};
use merge_core::text::{Encoding, Text, share_fallback, windows_1252_offset};
use merge_session::SessionError;
use merge_session::diff_tool::{self, Saved, SideSave};
use merge_session::origin::carry;
use merge_session::repository::{Handoff, WorkingCopy};
use merge_syntax::language::Language;
use merge_view::diff::{
    self, MOVE_MARK, Pin, differences, first_change, held_region, move_ends, move_target,
    next_change, previous_change, region_bytes, spacers, spacers_by, tone,
};
use merge_view::fold::{self, Fold, FoldRow, LineEdit, Opened};
use merge_view::scroll::{Follow, ScrollSync, TopAnchor};
use vcs::{Annotation, IdKind, RepoKind, VcsError};

use crate::blink::{self, Flash, SearchSteps};
use crate::editor_sync::{self, Caret};
use crate::file_watch::Watch;
use crate::move_pill::{MOVE_PILL_INSET, move_pill};
use crate::origin_column::{
    Annotated, Described, HeadingText, OriginCell, OriginColumn, Visibility,
};
use crate::own_writes::OwnWrites;
use crate::recorder::Recorded;
use crate::ruler;
use crate::scrollbars;
use crate::settings_banner::{self, render as render_banner};
use crate::status_bar;
use crate::strings::{self, prompt};
use crate::syntax::Colouring;
use crate::tones::{Palette, Tones};
use crate::toolbar::{self, DIFF_GROUPS, Labels};
use crate::unified_pane::{UnifiedLook, UnifiedPane};
use crate::window::{
    INTRALINE_ALPHA, OpenError, SCROLL_EPSILON, SOFT_WRAP_MAX_LINES, Syntax, revision_chip_element,
    revision_chip_id,
};
use crate::wrap::Wrap;
use crate::{
    Cancel, CopyAsPatch, CopyDifferenceAsPatch, ExitCode, Find, GoToMoveEnd, NextDifference,
    PreviousDifference, Redo, Save, SaveAndExit, ToggleCollapseUnchanged, ToggleOriginColumn,
    ToggleSoftWrap, ToggleSyntax, ToggleUnified, ToggleWhitespace, Typography, Undo, alert, finish,
    ignore_closed_view,
};

/// How long edits must pause before the regions are recomputed (spec §4.6).
pub const DIFF_DEBOUNCE: Duration = Duration::from_millis(100);
const COPY_BUTTON_WIDTH: Pixels = px(14.);
/// The copy buttons' column between the panes: one button each way.
const GUTTER_WIDTH: Pixels = px(28.);
/// Changed regions show in one colour on both sides (spec §5).
const SIDES: [Side; 2] = [Side::Left, Side::Right];

/// What wrapped panes were levelled for: each pane's text width and wrapped row count.
type WrapFit = [(Pixels, usize); 2];
/// The temporary directory `jj diff --tool` makes, and its sides' directories.
/// The answers to a change on disk under unsaved edits, in button order (spec §6.1). The
/// first keeps the edits, so the default button never loses them.
fn on_disk_change() -> [SharedString; 2] {
    [prompt::keep_mine(), prompt::reload()]
}

/// The key contexts of the bindings in [`crate::init`].
pub(crate) const DIFF_CONTEXT: &str = "DiffWindow";
pub(crate) const PANE_CONTEXT: &str = "DiffPane";

/// The banner's element id.
pub const BANNER_ID: &str = "banner";
/// The element id of the banner after a save jj did not snapshot.
pub const SNAPSHOT_BANNER_ID: &str = "snapshot-banner";
/// The unified pane (spec §5).
pub const UNIFIED_PANE_ID: &str = "unified-pane";
/// The status bar's element id.
pub const STATUS_ID: &str = "diff-status";
/// The status bar's text.
pub const STATUS_TEXT_ID: &str = "diff-status-text";
/// The overview ruler beside the right pane.
pub const DIFF_RULER_ID: &str = "diff-ruler";

/// The element id of `side`'s pane, also used by tests.
#[must_use]
pub fn pane_id(side: Side) -> ElementId {
    ElementId::Name(format!("pane-{side:?}").into())
}

/// The button on changed region `region` that copies it into side `to`.
#[must_use]
pub fn copy_id(region: RegionIx, to: Side) -> ElementId {
    ElementId::Name(format!("copy-{region}-{to:?}").into())
}

/// The pill over `side`'s end of the move that leaves region `from`.
#[must_use]
pub fn move_pill_id(side: Side, from: RegionIx) -> ElementId {
    ElementId::Name(format!("move-{from}-{side:?}").into())
}

/// Where a side's text comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A file on disk, which the side saves to and watches.
    File(PathBuf),
    /// A revision of a file in a repository, named by `title` (spec §6.1). It has no file
    /// to save to or watch.
    Revision { title: String },
}

impl Origin {
    fn path(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Revision { .. } => None,
        }
    }

    fn name(&self) -> Cow<'_, str> {
        match self {
            Self::File(path) => path.to_string_lossy(),
            Self::Revision { title } => Cow::Borrowed(title),
        }
    }
}

/// One side of a diff, decoded.
#[derive(Debug, Clone)]
pub struct DiffFile {
    pub origin: Origin,
    pub text: Text,
    pub read_only: bool,
}

impl DiffFile {
    /// A revision's text, titled `title`. Revisions are always read-only (spec §6.1).
    #[must_use]
    pub fn revision(text: Text, title: impl Into<String>) -> Self {
        Self { origin: Origin::Revision { title: title.into() }, text, read_only: true }
    }
}

/// Where a diff run's line origins come from (spec §7.3). `repo` holds the repository the
/// command line opened; an empty slot finds it from the paths.
#[derive(Debug, Clone)]
pub enum DiffLookup {
    /// `diff PATH`: the working-tree file, and the revision it is compared with.
    Native { path: PathBuf, rev: Option<String>, repo: Handoff },
    /// `diff LEFT RIGHT`, as `git difftool` and `jj diff --tool` run it, in `cwd`.
    Tool { paths: [PathBuf; 2], title: Option<String>, cwd: PathBuf, repo: Handoff },
}

impl DiffLookup {
    /// The jj working copy a save snapshots: `diff PATH`'s, in a jj repository. A tool diff
    /// belongs to the jj or git command that ran it, which takes the files in itself.
    #[must_use]
    pub fn working_copy(&self) -> Option<WorkingCopy> {
        match self {
            Self::Native { repo, .. } => repo.working_copy(),
            Self::Tool { .. } => None,
        }
    }

    /// Each side's annotation, and what the VCS records about the commit it shows.
    fn annotate(&self, inputs: [&Text; 2]) -> Result<(Annotated, Described), VcsError> {
        let found = match self {
            Self::Native { path, rev, repo } => {
                let repo = repo.take();
                merge_session::origin::find_diff(repo, path, rev.as_deref(), inputs)?
            }
            Self::Tool { paths, title, cwd, repo } => {
                let repo = repo.take();
                let paths = paths.each_ref().map(|path| cwd.join(path));
                merge_session::origin::find_diff_tool(repo, &paths, title.as_deref(), cwd, inputs)?
            }
        };
        Ok((found.annotate(&inputs), found.describe()))
    }
}

/// A pane's annotation and the text it describes, the pane's text when the window opened.
#[derive(Clone)]
struct Carried {
    text: Text,
    annotation: Arc<Annotation>,
}

#[derive(Debug, Clone)]
pub struct DiffSettings {
    /// The window title; [`strings::APP_NAME`] without one.
    pub title: Option<String>,
    pub options: DiffOptions,
    /// Watch both files for changes on disk (spec §6.1). Tests call
    /// [`DiffWindow::disk_changed`] instead.
    pub watch: bool,
    pub typography: Typography,
    /// The size the window opens at; no larger than its display.
    pub window_size: Size<Pixels>,
    /// Where the sides' line origins come from; none without a lookup.
    pub origins: Option<DiffLookup>,
    /// Whether the panes open with the origin column (spec §6.4).
    pub origin_column: Visibility,
    /// The ID that names a jj revision in the headings and the origin column (spec §6.4).
    pub revision_id: IdKind,
    /// The windows' appearance (spec §6.4).
    pub theme: merge_config::settings::Theme,
    /// The tones' colours (spec §6.4).
    pub colours: Colours,
    /// The repository the files lie in, which the status bar names; none outside one.
    pub repo: Option<RepoKind>,
    /// The panes colour their text by the language their file names name (spec §6.4).
    pub syntax: Syntax,
    /// The longest side, in lines, the panes colour by syntax (spec §6.4).
    pub syntax_max_lines: usize,
    /// The panes open with soft wrap, for sides of up to [`SOFT_WRAP_MAX_LINES`] lines.
    pub soft_wrap: bool,
    /// Whether the panes open with their whitespace shown (spec §5).
    pub whitespace: Visibility,
    /// Whether the panes open with their unchanged regions collapsed (spec §5).
    pub collapsing: Collapsing,
    /// Whether the window opens side by side or unified (spec §5).
    pub layout: DiffLayout,
    /// When the panes show their scrollbars (spec §6.4).
    pub scrollbars: Scrollbars,
    /// The run's working directory, which relative paths resolve against; none when the
    /// process has none.
    pub cwd: Option<PathBuf>,
    /// The jj working copy each save snapshots: in triad's own runs in a jj repository
    /// (spec §6.1). None when jj or git ran triad as its tool, since the run is theirs.
    pub working_copy: Option<WorkingCopy>,
}

impl Default for DiffSettings {
    fn default() -> Self {
        Self {
            title: None,
            options: DiffOptions::default(),
            watch: false,
            typography: Typography::default(),
            window_size: crate::window_size(merge_config::settings::DEFAULT_WINDOW_SIZE),
            origins: None,
            origin_column: Visibility::Hidden,
            revision_id: IdKind::default(),
            theme: merge_config::settings::Theme::System,
            colours: DEFAULT_COLOURS,
            repo: None,
            syntax: Syntax::Highlighted,
            syntax_max_lines: merge_config::settings::DEFAULT_SYNTAX_MAX_LINES,
            soft_wrap: false,
            whitespace: Visibility::Hidden,
            collapsing: Collapsing::default(),
            layout: DiffLayout::SideBySide,
            scrollbars: Scrollbars::Always,
            cwd: None,
            working_copy: None,
        }
    }
}

/// The answers when the window closes with unsaved edits, in button order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Closing {
    Save,
    Discard,
    KeepEditing,
}

const CLOSING: [Closing; 3] = [Closing::Save, Closing::Discard, Closing::KeepEditing];

struct Pane {
    origin: Origin,
    encoding: Encoding,
    editor: Entity<EditorState>,
    recorded: Recorded,
    bands: RangeDecorationCollection,
    /// The characters that differ from the paired line in the other pane (spec §5).
    marks: RangeDecorationCollection,
    /// The frame a jump blinks around its region.
    flash: RangeDecorationCollection,
    /// The fill a search step blinks over the match it reached.
    found: RangeDecorationCollection,
    /// The steps the pane's search takes.
    steps: SearchSteps,
    read_only: bool,
    /// Edited since it was opened or last saved.
    modified: bool,
    /// Counts the edits and reloads, so a save that lands knows whether the text it wrote is
    /// still the pane's.
    changes: u64,
    /// The texts the window wrote or is writing to the file. A change on disk that reads back
    /// as one is the window's own save.
    writes: OwnWrites,
    /// A prompt about a change on disk is open.
    asking: bool,
    /// The read of the file running after a change on disk, and what follows it.
    reading: Option<(OnRead, Task<()>)>,
    title: SharedString,
    /// The annotation the pane's origins are carried from, once it has loaded.
    annotated: Option<Carried>,
}

/// Whether the window exits once a save has written every side.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AfterSave {
    Stay,
    Exit,
}

/// One side's text on its way to its file, off the UI thread.
struct SideWrite {
    side: Side,
    /// The pane's change count when the text was taken.
    changes: u64,
}

/// What follows a read of a pane's file after a change on disk.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OnRead {
    /// Reload a pane without edits; ask first in a pane with edits.
    Ask,
    /// Reload: the user said so.
    Reload,
}

/// A file read from disk after a change, with the map from the pane's text as it stood when
/// the read began; no map when the texts match.
struct DiskRead {
    text: Result<Text, SessionError>,
    map: Option<LineMap>,
}

/// The view top a reload keeps (spec §6.1). Both panes take it once they lay out with the
/// new text, and again once the recompute after the reload re-levels them.
#[derive(Clone, Copy)]
struct HeldTop {
    side: Side,
    anchor: TopAnchor,
    /// The panes take it before they next lay out.
    due: bool,
    /// The recompute has landed: this is the last time the panes take it.
    last: bool,
}

/// How far a pane scrolls right and down when it next lays out; an axis left unset keeps
/// the pane's own. A pane reads back its old offset until it lays out, so one call gives it
/// both axes: two calls, one per axis, would undo each other.
#[derive(Clone, Copy, Default, PartialEq)]
struct ScrollTarget {
    left: Option<Pixels>,
    top: Option<Pixels>,
}

impl ScrollTarget {
    fn left(left: Pixels) -> Self {
        Self { left: Some(left), top: None }
    }

    fn top(top: Pixels) -> Self {
        Self { left: None, top: Some(top) }
    }

    fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// Take each axis `later` sets.
    fn set(&mut self, later: Self) {
        self.left = later.left.or(self.left);
        self.top = later.top.or(self.top);
    }

    /// The editor's scroll offset for this target over the pane's `offset` now.
    fn over(self, offset: gpui_kit::Point<Pixels>) -> gpui_kit::Point<Pixels> {
        point(self.left.map_or(offset.x, |left| -left), self.top.map_or(offset.y, |top| -top))
    }
}

pub struct DiffWindow {
    menu_bar: crate::menu_bar::WindowMenu,
    /// Keeps the window at `window_size` across a frame triad draws.
    fit: crate::decorations::Fit,
    doc: DiffDocument,
    /// Whether the panes colour a chunk at a time, past `syntax-whole-file-max-lines`.
    chunked: bool,
    palette: Palette,
    panes: [Pane; 2],
    comparison: String,
    repo: Option<RepoKind>,
    status: SharedString,
    recompute: Option<Task<()>>,
    /// The latest save. Each save waits for the one before, so writes to a file land in the
    /// order asked.
    saving: Option<Task<()>>,
    /// Whether the last save to land failed to write a side.
    save_failed: bool,
    /// Whether the recompute task has taken its snapshot and is diffing it.
    computing: bool,
    /// Navigations and patch copies asked for while the regions lag the text, in order. Each
    /// runs once the recompute installs; an edit drops them.
    waiting: Vec<Waiting>,
    /// Whether the window shows the two panes or the unified pane (spec §5).
    layout: DiffLayout,
    /// The unified pane while it shows; made again each time it opens.
    unified: Option<UnifiedPane>,
    /// When the panes' scrollbars show, for a unified pane made later.
    scrollbar_mode: ScrollbarMode,
    /// The region navigation last made current, on the side it started from, and the caret
    /// it left there. While that caret stays put, the region is current: an empty region
    /// holds no line, so the caret alone cannot name it.
    pinned: Option<Pin>,
    /// The blink of the region a jump reached.
    flash: Flash<RegionIx>,
    /// The blink of the match a search step reached: its pane and bytes.
    found: Flash<(Side, usize, usize)>,
    /// Why edits here do not last, when they do not.
    banner: Option<SharedString>,
    /// The jj working copy each save snapshots; none outside triad's own jj runs.
    working_copy: Option<WorkingCopy>,
    /// Why jj did not snapshot the last save, until a save it snapshots.
    snapshot_failed: Option<SharedString>,
    /// The config file's bad lines, which the settings banner shows.
    settings_errors: Option<SharedString>,
    /// Each pane's scroll top after the last sync, left then right.
    synced: [Pixels; 2],
    /// The scroll offset each pane takes when it next lays out, left then right.
    targets: [ScrollTarget; 2],
    /// Where the panes stood sideways at the last sync, left as the rows and right as the
    /// result.
    sideways_sync: ScrollSync,
    /// The right pane's scroll, for the ruler's thumb.
    ruler_feed: scrollbars::Feed,
    ruler_drag: ruler::Dragging,
    /// Whether the panes wrap long lines.
    wrap: Wrap,
    /// Whether both panes show their whitespace (spec §5).
    whitespace: Visibility,
    /// What the wrapped panes' spacers were levelled for; `None` until they are, and after
    /// the regions change.
    wrap_fit: Option<WrapFit>,
    /// The region `go_to` moved to, which both panes scroll into view once laid out.
    reveal: Option<RegionIx>,
    /// The view top a reload keeps, until the recompute after it re-levels the panes.
    held_top: Option<HeldTop>,
    /// The collapsed unchanged regions.
    collapse: Collapse,
    /// How far the left pane's first row sits below the top of the copy buttons' column.
    gutter_offset: Pixels,
    typography: Typography,
    /// Each pane's origins, carried onto its text at the last installed recompute, and
    /// their column.
    origin_column: OriginColumn,
    colouring: Colouring,
    _watch: Option<Watch>,
    _subscriptions: Vec<Subscription>,
}

/// The diff window's collapsed unchanged regions (spec §5).
struct Collapse {
    on: bool,
    /// The lines a collapsed region keeps at each end that faces a change.
    context: usize,
    /// The regions the user opened.
    opened: Opened,
    /// The folds the panes show.
    folds: Vec<Fold>,
    /// Each pane's current search match when last seen, left then right.
    searched: [Option<Range<usize>>; 2],
}

impl Collapse {
    fn new(collapsing: Collapsing) -> Self {
        Self {
            on: collapsing.on,
            context: collapsing.context_lines,
            opened: Opened::default(),
            folds: Vec::new(),
            searched: [None, None],
        }
    }

    /// Move the opened regions past `patches` to `side`, and open the folds they touch.
    /// `line_start` gives the line of a byte of the text the patches apply to and the byte
    /// where that line starts; `slice` gives a run of its bytes.
    fn note_edits(
        &mut self,
        side: Side,
        patches: &[TextPatch],
        line_start: impl Fn(usize) -> (usize, usize),
        slice: impl Fn(Range<usize>) -> String,
    ) {
        if !self.on {
            return;
        }
        // The last patch first, so the lines of the ones before it stay put.
        for patch in patches.iter().rev() {
            let (line, head) = line_start(patch.range.start);
            // A copy of the patched lines, from the start of the first, to find the lines
            // that differ: a replace-all patches the whole text at once.
            let old = slice(head..patch.range.end);
            let new = [&old[..patch.range.start - head], patch.text.as_str()].concat();
            let edit = LineEdit::new(&old, &new, line);
            for touched in fold::folds_over(&self.folds, side, edit.lines.clone()) {
                self.opened.open(touched);
            }
            self.opened.shift(side, &edit);
        }
    }
}

/// A step to the next or previous difference, from `side`'s current region.
#[derive(Clone, Copy)]
struct Navigation {
    step: fn(&DiffDocument, RegionIx) -> Option<RegionIx>,
    side: Side,
}

/// What a patch copy holds (spec §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchTarget {
    /// Every difference.
    File,
    /// The difference holding the caret of the pane it started from.
    Difference,
}

/// A command that needs the regions of the current text.
#[derive(Clone, Copy)]
enum Waiting {
    Step(Navigation),
    CopyPatch(PatchTarget, Side),
}

/// Open a diff window.
///
/// # Errors
///
/// [`OpenError::Window`] when the platform refuses a window.
pub fn open(
    left: DiffFile,
    right: DiffFile,
    settings: &DiffSettings,
    cx: &mut App,
) -> Result<(AnyWindowHandle, Entity<DiffWindow>), OpenError> {
    crate::apply_theme(settings.theme, cx);
    let options = WindowOptions {
        window_bounds: Some(crate::window_bounds(settings.window_size, cx)),
        ..crate::menu_bar::window_options()
    };
    let title = settings.title.as_deref().map_or(Cow::Borrowed(strings::APP_NAME), escaped);
    Ok(gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title(&title);
        let view = cx.new(|cx| DiffWindow::new([left, right], settings, window, cx));
        // Keys reach the window only through a focused element.
        let left = view.read(cx).panes[0].editor.clone();
        left.update(cx, |state, cx| state.focus(window, cx));
        view.update(cx, |this, cx| {
            if this.collapse.on {
                this.set_spacers(cx);
            }
            this.open_on_first_difference(window, cx);
            if this.layout == DiffLayout::Unified {
                this.show_unified(window, cx);
            }
        });
        let this = view.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            ignore_closed_view(this.update(cx, |this, cx| this.close(window, cx)));
            false
        });
        view
    })?)
}

impl DiffWindow {
    /// Follow `side`'s editor: its edits, spacer and gutter clicks, and search steps.
    fn subscribe_pane(
        editor: &Entity<EditorState>,
        side: Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> [Subscription; 2] {
        [
            cx.subscribe_in(editor, window, move |this, _, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => this.send_edits(side, window, cx),
                    InputEvent::SpacerClick { line } => this.open_fold_above(side, *line, cx),
                    InputEvent::GutterMarkClick { line } => {
                        this.go_to_other_end(side, *line, window, cx);
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Focus | InputEvent::Blur => {}
                }
            }),
            cx.observe(editor, move |this, _, cx| this.follow_search_step(side, cx)),
        ]
    }

    fn new(
        mut files: [DiffFile; 2],
        settings: &DiffSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let [left, right] = &mut files;
        share_fallback(&mut [&mut left.text, &mut right.text]);
        let [left, right] = &files;
        let mut doc = DiffDocument::new(&left.text, &right.text, settings.options.clone());
        Self::load_marks(doc.marking(), window, cx);
        if let Some(lookup) = settings.origins.clone() {
            // Texts share their content, so the copies are O(1).
            Self::load_origins(lookup, [left.text.clone(), right.text.clone()], window, cx);
        }
        let palette = Palette::new(&settings.colours);
        let tones = *palette.tones(cx.theme().is_dark());
        let names = [left.origin.path(), right.origin.path()].into_iter().flatten();
        let language = Language::of_run(names, None, settings.title.as_deref())
            .or_else(|| Language::of_scripts([left, right].map(|file| file.text.content())));
        let colouring = Colouring::new(language, settings.syntax, settings.syntax_max_lines);
        // Sides that open past the limit never wrap, though edits may shorten them.
        let wrap =
            Wrap::new(settings.soft_wrap, [left, right].iter().map(|file| file.text.line_count()));
        let ruler_feed = scrollbars::Feed::default();
        let look = Look {
            soft_wrap: wrap.wraps(),
            whitespace: settings.whitespace.shown(),
            scrollbars: scrollbars::mode(settings.scrollbars),
            ruler_feed: &ruler_feed,
            typography: &settings.typography,
            tones: &tones,
            language: colouring.editor_language(),
            chunked: colouring.chunked_factory(left.text.line_count().max(right.text.line_count())),
        };
        let mut subscriptions = Vec::with_capacity(SIDES.len());
        let [left, right] = files;
        let panes = [(Side::Left, left), (Side::Right, right)].map(|(side, file)| {
            doc.set_read_only(side, file.read_only);
            let pane = new_pane(&doc, side, file, &look, window, cx);
            subscriptions.extend(Self::subscribe_pane(&pane.editor, side, window, cx));
            pane
        });
        subscriptions.push(cx.observe_window_activation(window, |this, _, cx| {
            // Another window may have changed the menu bar meanwhile.
            this.menu_bar.forget();
            cx.notify();
        }));
        let comparison = strings::diff_comparison(&settings.options);
        // Without a working directory, a relative path stays relative.
        let cwd = settings.cwd.as_deref().unwrap_or(Path::new(""));
        let editable = panes.iter().filter(|p| !p.read_only);
        let banner = jj_banner(editable.filter_map(|p| p.origin.path()), cwd);
        let watch = settings
            .watch
            .then(|| watch(&panes.each_ref().map(|p| p.origin.path()), cwd, window, cx));
        Self {
            menu_bar: crate::menu_bar::WindowMenu::new(cx),
            fit: crate::decorations::Fit::default(),
            colouring,
            chunked: look.chunked.is_some(),
            status: strings::diff_status(settings.repo, differences(&doc), &comparison).into(),
            repo: settings.repo,
            doc,
            panes,
            comparison,
            recompute: None,
            saving: None,
            save_failed: false,
            computing: false,
            waiting: Vec::new(),
            layout: settings.layout,
            unified: None,
            scrollbar_mode: look.scrollbars,
            pinned: None,
            flash: Flash::default(),
            found: Flash::default(),
            banner,
            working_copy: settings.working_copy.clone(),
            snapshot_failed: None,
            settings_errors: None,
            synced: [Pixels::ZERO; 2],
            targets: [ScrollTarget::default(); 2],
            sideways_sync: ScrollSync::default(),
            ruler_feed,
            ruler_drag: ruler::Dragging::default(),
            wrap,
            whitespace: settings.whitespace,
            wrap_fit: None,
            reveal: None,
            held_top: None,
            collapse: Collapse::new(settings.collapsing),
            gutter_offset: Pixels::ZERO,
            typography: settings.typography.clone(),
            palette,
            origin_column: OriginColumn::new(
                SIDES.len(),
                0,
                settings.origin_column,
                settings.revision_id,
            ),
            _watch: watch,
            _subscriptions: subscriptions,
        }
    }

    #[must_use]
    pub fn document(&self) -> &DiffDocument {
        &self.doc
    }

    /// Whether the panes colour a chunk at a time, past `syntax-whole-file-max-lines`.
    #[must_use]
    pub fn chunked(&self) -> bool {
        self.chunked
    }

    #[must_use]
    pub fn editor(&self, side: Side) -> &Entity<EditorState> {
        &self.pane(side).editor
    }

    /// The unified pane's editor while it shows.
    #[must_use]
    pub fn unified_editor(&self) -> Option<&Entity<EditorState>> {
        self.unified.as_ref().map(UnifiedPane::editor)
    }

    /// The unified pane's lines while it shows.
    #[must_use]
    pub fn unified_view(&self) -> Option<&merge_view::unified::Unified> {
        self.unified.as_ref().map(UnifiedPane::view)
    }

    /// The byte ranges of the unified pane's bands, as its editor holds them; none while it
    /// is hidden.
    #[must_use]
    pub fn unified_band_ranges(&self, cx: &App) -> Vec<Range<usize>> {
        self.unified.as_ref().map_or_default(|pane| pane.band_ranges(cx))
    }

    /// The byte ranges of `side`'s marks on the characters that differ, as the editor holds
    /// them.
    #[must_use]
    pub fn mark_ranges(&self, side: Side, cx: &App) -> Vec<Range<usize>> {
        self.pane(side).marks.get_ranges(cx)
    }

    /// The byte ranges of `side`'s bands, as the editor holds them.
    #[must_use]
    pub fn band_ranges(&self, side: Side, cx: &App) -> Vec<Range<usize>> {
        self.pane(side).bands.get_ranges(cx)
    }

    /// Whether a diff of a snapshot of the text runs off the UI thread.
    #[must_use]
    pub fn computing(&self) -> bool {
        self.computing
    }

    /// The banner over the panes, if any.
    #[must_use]
    pub fn banner(&self) -> Option<&SharedString> {
        self.banner.as_ref()
    }

    /// Why jj did not snapshot the last save, if it did not.
    #[must_use]
    pub fn snapshot_failed(&self) -> Option<&SharedString> {
        self.snapshot_failed.as_ref()
    }

    #[must_use]
    pub fn status_text(&self) -> &SharedString {
        &self.status
    }

    /// The pane's title: its path or revision, with a marker while it has unsaved edits.
    #[must_use]
    pub fn title(&self, side: Side) -> &SharedString {
        &self.pane(side).title
    }

    /// The origin of `side`'s line `line`: none for a line without one, or before the
    /// origins have loaded.
    #[must_use]
    pub fn origin_cell(&self, side: Side, line: usize) -> Option<OriginCell> {
        self.origin_column.cell(Source::Side(index(side)), Some(line))
    }

    /// Find and annotate both sides off the UI thread, then carry the annotations onto the
    /// panes' text.
    fn load_origins(
        lookup: DiffLookup,
        texts: [Text; 2],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let (texts, annotated) = cx
                .background_spawn(async move {
                    let annotated = lookup.annotate(texts.each_ref());
                    (texts, annotated)
                })
                .await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.loaded_origins(texts, annotated, window, cx);
            }));
        })
        .detach();
    }

    fn loaded_origins(
        &mut self,
        texts: [Text; 2],
        annotated: Result<(Annotated, Described), VcsError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (annotated, described) = match annotated {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(%error, "cannot find the sides' line origins; none show");
                return;
            }
        };
        self.origin_column.described(SIDES.map(|side| Source::Side(index(side))), described);
        for ((side, text), annotation) in SIDES.into_iter().zip(texts).zip(annotated) {
            match annotation {
                Ok(Some(annotation)) => {
                    self.pane_mut(side).annotated =
                        Some(Carried { text, annotation: Arc::new(annotation) });
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, ?side, "cannot annotate a side; it shows no origins");
                }
            }
        }
        self.schedule_recompute(window, cx);
    }

    /// `side`'s heading as drawn: its title, with its modified marker and without a leading
    /// ID its chip shows, and the revision chip.
    #[must_use]
    pub fn heading_text(&self, side: Side) -> HeadingText {
        self.origin_column.heading(Source::Side(index(side)), &self.pane(side).title)
    }

    /// The revision chip of `side`'s heading: none for a side that shows no commit.
    #[must_use]
    pub fn revision_chip(&self, side: Side) -> Option<OriginCell> {
        self.heading_text(side).chip
    }

    /// What the recompute carries each pane's annotation from. Cloning shares the texts and
    /// the annotations.
    fn carry_job(&self) -> [Option<Carried>; 2] {
        self.panes.each_ref().map(|pane| pane.annotated.clone())
    }

    /// Show the origins `carried` gives, for the text a recompute just installed.
    fn install_origins(&mut self, carried: [Option<Annotation>; 2]) {
        for (side, annotation) in SIDES.into_iter().zip(carried) {
            let Some(annotation) = annotation else {
                continue;
            };
            self.origin_column.set(Source::Side(index(side)), annotation);
        }
    }

    fn pane(&self, side: Side) -> &Pane {
        &self.panes[index(side)]
    }

    fn pane_mut(&mut self, side: Side) -> &mut Pane {
        &mut self.panes[index(side)]
    }

    /// Send the edits `side`'s editor made to the core.
    fn send_edits(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane(side);
        let text = self.doc.text(side);
        let Some(batch) = editor_sync::take_batch(&pane.editor, &pane.recorded, &text, cx) else {
            return;
        };
        self.collapse.note_edits(
            side,
            &batch.0,
            |byte| {
                let line = text.byte_to_line(byte);
                (line, text.line_to_byte(line))
            },
            |bytes| text.byte_slice(bytes).to_string(),
        );
        let before = self.doc.version();
        match self.doc.edit(side, batch) {
            Ok(edited) => self.follow(before, &edited, Caret::Follow, window, cx),
            Err(error) => {
                tracing::error!(%error, ?side, "the core refused an edit; reloading the pane");
                let caret = self.editor(side).read(cx).cursor();
                self.reload(side, caret, window, cx);
            }
        }
    }

    /// Bring `edited`'s side up to date with the core, and recompute once edits pause. A
    /// change took the document past version `before`.
    fn follow(
        &mut self,
        before: u64,
        edited: &Edited,
        caret: Caret,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = edited.side;
        let text = self.panes[index(side)].editor.read(cx).text();
        self.collapse.note_edits(
            side,
            &edited.patches,
            |byte| {
                let line = text.offset_to_point(byte).row;
                (line, text.line_start_offset(line))
            },
            |bytes| text.slice(bytes).to_string(),
        );
        let pane = self.pane(side);
        editor_sync::apply_patches(
            &pane.editor,
            &pane.recorded,
            &edited.patches,
            caret,
            window,
            cx,
        );
        if edited.version != before {
            self.pinned = None;
            self.held_top = None;
            self.waiting.clear();
            self.pane_mut(side).changes += 1;
            self.set_modified(side, true);
            self.schedule_recompute(window, cx);
        }
        self.refresh_bands(side, cx);
    }
    /// Give `side`'s editor the core's text, with the caret at `caret`, cut to the text.
    fn reload(&mut self, side: Side, caret: usize, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.doc.text(side).to_string();
        let pane = self.pane(side);
        pane.recorded.muted(|| {
            pane.editor.update(cx, |state, cx| {
                let caret = caret.min(text.len());
                state.set_value(text, window, cx);
                state.set_selected_range(caret..caret, cx);
            });
        });
    }

    /// Recompute the regions [`DIFF_DEBOUNCE`] after the last edit, or at once while a
    /// navigation or a patch copy waits for them.
    fn schedule_recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let delay = self.waiting.is_empty().then_some(DIFF_DEBOUNCE);
        self.start_recompute(delay, window, cx);
    }

    /// Recompute the regions off the UI thread after `delay`. Replacing a task still waiting
    /// out its delay cancels it. One diff runs at a time: while one runs, this does nothing,
    /// and a diff that lands for an older text starts the next.
    fn start_recompute(
        &mut self,
        delay: Option<Duration>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.computing {
            return;
        }
        self.recompute = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(delay) = delay {
                cx.background_executor().timer(delay).await;
            }
            let Ok((job, origins)) = this.update(cx, |this, _| {
                this.computing = true;
                (this.doc.recompute(), this.carry_job())
            }) else {
                return;
            };
            let (diffed, carried) = cx
                .background_spawn(async move { job.run_with(|texts| carried(&origins, texts)) })
                .await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.computing = false;
                if this.doc.install(diffed) {
                    this.install_origins(carried);
                    this.refresh(cx);
                    if this.layout == DiffLayout::Unified {
                        this.show_unified(window, cx);
                    }
                    for waiting in std::mem::take(&mut this.waiting) {
                        match waiting {
                            Waiting::Step(navigation) => this.step(navigation, window, cx),
                            Waiting::CopyPatch(target, side) => {
                                this.copy_patch(target, side, window, cx);
                            }
                        }
                    }
                } else {
                    this.schedule_recompute(window, cx);
                }
            }));
        }));
    }

    /// Give each pane's gutter its origins, while the column shows: a label per run of its
    /// annotation, whose hover card builds its text when it shows.
    fn refresh_labels(&self, cx: &mut Context<Self>) {
        let colour = cx.theme().muted_foreground;
        for side in SIDES {
            let runs = self.origin_column.origins().runs(Source::Side(index(side)));
            let labels = self.origin_column.run_labels(runs, colour);
            self.pane(side).editor.update(cx, |state, cx| state.set_line_labels(labels, cx));
        }
    }

    fn toggle_origin_column(
        &mut self,
        _: &ToggleOriginColumn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.origin_column.toggle();
        self.refresh_labels(cx);
        cx.notify();
    }

    /// Turn soft wrap in both panes on or off (spec §5, §6.4). Sides that opened past
    /// [`SOFT_WRAP_MAX_LINES`] never wrap.
    fn toggle_soft_wrap(
        &mut self,
        _: &ToggleSoftWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wrap = self.wrap.toggled();
        if self.wrap == Wrap::TooLong {
            tracing::info!(max_lines = SOFT_WRAP_MAX_LINES, "the sides are too long to wrap");
            return;
        }
        let wrap = self.wrap.wraps();
        for editor in self.editors() {
            editor.update(cx, |state, cx| state.set_soft_wrap(wrap, window, cx));
        }
        // Wrapped panes level once laid out (`level_wrapped`); unwrapped ones level by lines.
        self.set_spacers(cx);
    }

    /// Show or hide the whitespace in both panes (spec §5).
    fn toggle_whitespace(
        &mut self,
        _: &ToggleWhitespace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.whitespace = self.whitespace.toggled();
        let shown = self.whitespace.shown();
        for editor in self.editors() {
            editor.update(cx, |state, cx| state.set_show_whitespaces(shown, window, cx));
        }
        cx.notify();
    }

    /// Collapse every unchanged region, or show them all (spec §5).
    fn toggle_collapse_unchanged(
        &mut self,
        _: &ToggleCollapseUnchanged,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.collapse.on = !self.collapse.on;
        self.collapse.opened.clear();
        self.set_spacers(cx);
        cx.notify();
    }

    /// Open the fold whose row sits above `line` of `side`, if any.
    fn open_fold_above(&mut self, side: Side, line: usize, cx: &mut Context<Self>) {
        let Some(above) = fold::fold_above(&self.collapse.folds, side, line) else {
            return;
        };
        self.collapse.opened.open(above);
        self.set_spacers(cx);
        cx.notify();
    }

    /// Open the folds that hide `line` of `side`. The editor scrolls its caret into view
    /// when it next lays out.
    fn open_folds_at(&mut self, side: Side, line: usize, cx: &mut Context<Self>) {
        let over: Vec<_> = fold::folds_over(&self.collapse.folds, side, line..line + 1).collect();
        if over.is_empty() {
            return;
        }
        for fold in over {
            self.collapse.opened.open(fold);
        }
        self.set_spacers(cx);
        cx.notify();
    }

    /// Turn both panes' syntax colours off or on (spec §6.4).
    fn toggle_syntax(&mut self, _: &ToggleSyntax, _: &mut Window, cx: &mut Context<Self>) {
        self.colouring.toggle();
        let language = self.colouring.editor_language();
        for editor in self.editors() {
            editor.update(cx, |state, cx| state.set_highlighter(language, cx));
        }
    }

    /// Both panes' editors, then the unified pane's while it shows.
    fn editors(&self) -> impl Iterator<Item = Entity<EditorState>> + use<> {
        let unified = self.unified.as_ref().map(|pane| pane.editor().clone());
        SIDES.map(|side| self.editor(side).clone()).into_iter().chain(unified)
    }

    /// Show the unified pane or the two panes (spec §5).
    fn toggle_unified(&mut self, _: &ToggleUnified, window: &mut Window, cx: &mut Context<Self>) {
        match self.layout {
            DiffLayout::SideBySide => {
                self.layout = DiffLayout::Unified;
                self.show_unified(window, cx);
            }
            DiffLayout::Unified => {
                self.layout = DiffLayout::SideBySide;
                self.unified = None;
                self.editor(Side::Left).update(cx, |state, cx| state.focus(window, cx));
            }
        }
        cx.notify();
    }

    /// Bring the unified pane up to the current text and regions, or make it with the focus
    /// in it. While the regions lag the text, the recompute that catches up shows it.
    fn show_unified(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.doc.regions_current() {
            self.start_recompute(None, window, cx);
            return;
        }
        let tones = *self.palette.tones(cx.theme().is_dark());
        if let Some(pane) = &mut self.unified {
            pane.update(&self.doc, &tones, window, cx);
            return;
        }
        let look = UnifiedLook {
            soft_wrap: self.wrap.wraps(),
            whitespace: self.whitespace.shown(),
            scrollbars: self.scrollbar_mode,
            typography: &self.typography,
            tones: &tones,
            language: self.colouring.editor_language(),
        };
        let pane = UnifiedPane::new(&self.doc, &look, window, cx);
        pane.editor().update(cx, |state, cx| state.focus(window, cx));
        self.unified = Some(pane);
        cx.notify();
    }

    /// The View menu items this window ticks. It has no base pane and no result.
    fn view_ticks(&self) -> crate::ViewTicks {
        crate::ViewTicks {
            origin_column: self.origin_column.shown(),
            origins_loaded: self.origin_column.origins().loaded(),
            syntax: self.colouring.shown(),
            soft_wrap: self.wrap.wraps(),
            whitespace: self.whitespace.shown(),
            collapse_unchanged: self.collapse.on,
            unified: self.layout == DiffLayout::Unified,
            ..crate::ViewTicks::default()
        }
    }

    /// Redraw both panes' bands, spacers, origins and the status from the core.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_labels(cx);
        for side in SIDES {
            self.refresh_bands(side, cx);
            self.refresh_marks(side, cx);
        }
        self.collapse.opened.renew(&self.doc);
        self.set_spacers(cx);
        if self.doc.regions_current()
            && let Some(held) = &mut self.held_top
        {
            held.due = true;
            held.last = true;
        }
        self.status =
            strings::diff_status(self.repo, differences(&self.doc), &self.comparison).into();
        cx.notify();
    }

    /// Give each pane the spacer rows that level it with the other (spec §5): by lines, or
    /// under soft wrap by the display rows each pane's lines wrap to. With unchanged regions
    /// collapsed, each pane hides its folds' lines and shows a fold row for each. While the
    /// regions lag the text, the folds stay as they are, as the spacers do.
    fn set_spacers(&mut self, cx: &mut Context<Self>) {
        self.wrap_fit = None;
        if self.doc.regions_current() {
            self.collapse.folds = if self.collapse.on {
                fold::folds(&self.doc, self.collapse.context, &self.collapse.opened)
            } else {
                Vec::new()
            };
            for side in SIDES {
                let hidden = self.collapse.folds.iter().map(|fold| fold.lines(side)).collect();
                self.editor(side).update(cx, |state, cx| state.set_hidden_lines(hidden, cx));
            }
        }
        let wrapped = self.wrap.wraps();
        let colour = cx.theme().muted_foreground;
        for side in SIDES {
            let rows: Vec<_> = if wrapped {
                spacers_by(&self.doc, side, self.line_rows(cx)).collect()
            } else {
                spacers(&self.doc, side).collect()
            };
            let (rows, fold_rows) = fold::fold_spacers(rows, &self.collapse.folds, side);
            let notes = fold_notes(&fold_rows, colour);
            self.editor(side).update(cx, |state, cx| {
                state.set_spacers(rows, cx);
                state.set_spacer_notes(notes, cx);
            });
        }
        if wrapped {
            self.wrap_fit = Some(self.wrap_fit(cx));
        }
    }

    /// The display rows a run of one side's lines takes in its pane: one per line, or under
    /// soft wrap the rows each line wraps to.
    fn line_rows<'a>(&self, cx: &'a App) -> impl Fn(Side, Range<usize>) -> usize + use<'a> {
        let maps = SIDES.map(|side| self.editor(side).read(cx).display_map());
        let wrapped = self.wrap.wraps();
        move |side, lines: Range<usize>| {
            if wrapped {
                let map = maps[index(side)];
                lines.map(|line| map.visible_wrap_row_count_for_buffer_line(line)).sum()
            } else {
                lines.len()
            }
        }
    }

    fn wrap_fit(&self, cx: &App) -> WrapFit {
        SIDES.map(|side| {
            let state = self.editor(side).read(cx);
            (state.input_bounds().size.width, state.display_map().wrap_row_count())
        })
    }

    /// Level wrapped panes again when either wrapped differently since the spacers were set:
    /// after a resize, or once soft wrap first lays out. Runs in prepaint, after both panes
    /// wrapped; the new spacers show from the next frame.
    fn level_wrapped(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.wrap.wraps() || self.wrap_fit == Some(self.wrap_fit(cx)) {
            return;
        }
        self.set_spacers(cx);
        window.refresh();
    }

    fn refresh_bands(&self, side: Side, cx: &mut Context<Self>) {
        let bands = bands(&self.doc, side, self.palette.tones(cx.theme().is_dark()));
        let pane = self.pane(side);
        pane.bands.clear(cx);
        pane.bands.append(bands, cx);
    }

    /// Fill the characters the core marks on `side`. While the regions lag the text, the
    /// editor carries the fills through the edits instead.
    fn refresh_marks(&self, side: Side, cx: &mut Context<Self>) {
        if !self.doc.regions_current() {
            return;
        }
        let colour =
            self.palette.tones(cx.theme().is_dark()).mark(tone(side)).opacity(INTRALINE_ALPHA);
        let marks = self.doc.marks().side(side).iter().map(|bytes| {
            RangeDecoration::new(bytes.clone())
                .with_style(RangeDecorationStyle::Fill)
                .with_color(colour)
        });
        self.pane(side).marks.set(marks.collect(), cx);
    }

    /// Mark the characters that differ off the UI thread: a large file's marks cost more than
    /// its diff (`DESIGN.md` §A4.7).
    fn load_marks(marking: Option<Marking>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(marking) = marking else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let (version, marks) = cx.background_spawn(async move { marking.run() }).await;
            ignore_closed_view(this.update(cx, |this, cx| {
                if this.doc.install_marks(version, marks) {
                    for side in SIDES {
                        this.refresh_marks(side, cx);
                    }
                }
            }));
        })
        .detach();
    }

    fn set_modified(&mut self, side: Side, modified: bool) {
        let pane = self.pane_mut(side);
        pane.modified = modified;
        pane.title = title(&pane.origin, modified);
    }

    /// The side whose editor has the focus: undo and redo act on it.
    fn focused(&self, window: &Window, cx: &App) -> Option<Side> {
        SIDES.into_iter().find(|&side| {
            self.pane(side).editor.read(cx).focus_handle(cx).contains_focused(window, cx)
        })
    }

    /// Open the search of the focused pane, else the left pane's.
    fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focused(window, cx).unwrap_or(Side::Left);
        let editor = match &self.unified {
            Some(pane) => pane.editor(),
            None => &self.pane(side).editor,
        };
        editor.update(cx, |state, cx| {
            state.focus(window, cx);
            state.open_search(false, cx);
        });
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(side) = self.focused(window, cx) else {
            return;
        };
        self.send_edits(side, window, cx);
        let before = self.doc.version();
        let edited = self.doc.undo(side);
        self.follow(before, &edited, Caret::AtChange, window, cx);
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(side) = self.focused(window, cx) else {
            return;
        };
        self.send_edits(side, window, cx);
        let before = self.doc.version();
        let edited = self.doc.redo(side);
        self.follow(before, &edited, Caret::AtChange, window, cx);
    }

    /// The region holding the unified pane's caret while it shows, else `side`'s, or the
    /// pinned one.
    fn current(&self, side: Side, cx: &App) -> RegionIx {
        if let Some(pane) = &self.unified
            && let Some(region) = pane.view().region_at(pane.caret_line(cx))
        {
            return region;
        }
        let caret = self.pane(side).editor.read(cx).cursor();
        held_region(&self.doc, self.pinned, side, caret)
    }

    /// Move both carets to the start of the region `step` finds from the focused side's
    /// current one. While the regions lag the text, the step waits for a recompute, started
    /// at once unless one is already diffing.
    fn navigate(
        &mut self,
        step: fn(&DiffDocument, RegionIx) -> Option<RegionIx>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let navigation = Navigation { step, side: self.focused(window, cx).unwrap_or(Side::Left) };
        if self.doc.regions_current() {
            self.step(navigation, window, cx);
            return;
        }
        self.wait_for_regions(Waiting::Step(navigation), window, cx);
    }

    /// Run `waiting` once the regions catch up with the text, recomputing them at once unless a
    /// recompute is already diffing.
    fn wait_for_regions(&mut self, waiting: Waiting, window: &mut Window, cx: &mut Context<Self>) {
        self.waiting.push(waiting);
        if !self.computing {
            self.start_recompute(None, window, cx);
        }
    }

    /// Copy `target` as a patch from the focused pane, once the regions are current.
    fn ask_copy_patch(&mut self, target: PatchTarget, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focused(window, cx).unwrap_or(Side::Left);
        if self.doc.regions_current() {
            self.copy_patch(target, side, window, cx);
        } else {
            self.wait_for_regions(Waiting::CopyPatch(target, side), window, cx);
        }
    }

    /// Write `target`'s patch to the clipboard, built off the UI thread, its headers the panes'
    /// titles without the modified marker. A patch with no change leaves the clipboard alone,
    /// as does a difference copy from a caret outside any difference.
    fn copy_patch(
        &mut self,
        target: PatchTarget,
        side: Side,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = match target {
            PatchTarget::File => Scope::File,
            PatchTarget::Difference => Scope::Region(self.current(side, cx)),
        };
        let Some(patching) = self.doc.patching(scope) else {
            return;
        };
        let [left, right] = SIDES.map(|side| self.pane(side).origin.name().into_owned());
        cx.spawn_in(window, async move |_, cx| {
            let patch = cx
                .background_spawn(async move {
                    patching.run(DEFAULT_CONTEXT, Titles { left: &left, right: &right })
                })
                .await;
            if !patch.is_empty() {
                // Only a closed app fails the update, and then no clipboard is left to fill.
                let _ = cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(patch)));
            }
        })
        .detach();
    }

    /// Take `navigation`'s step over the installed regions and blink the region it reaches.
    /// A step that finds nothing stays on the current region, and blinks it if it is a
    /// difference, so a lone difference still shows.
    fn step(&mut self, navigation: Navigation, window: &mut Window, cx: &mut Context<Self>) {
        let Navigation { step, side } = navigation;
        let from = self.current(side, cx);
        let to = step(&self.doc, from).or_else(|| {
            self.doc
                .regions()
                .get(from.get())
                .is_some_and(|region| region.kind == Kind2::Changed)
                .then_some(from)
        });
        match (to, &self.unified) {
            (Some(region), Some(pane)) => {
                if let Some(line) = pane.view().first_line(region) {
                    pane.go_to_line(line, window, cx);
                }
            }
            (Some(region), None) => {
                self.go_to(region, side, window, cx);
                self.start_flash(region, cx);
            }
            (None, _) => {}
        }
    }

    /// Put both carets on the first difference, with the focus left where `open` set it.
    fn open_on_first_difference(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(region) = first_change(&self.doc) {
            self.go_to(region, Side::Left, window, cx);
        }
    }

    /// Move both carets to the start of `region` and make it current, with the focus in `side`.
    fn go_to(&mut self, region: RegionIx, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        let at = region_bytes(&self.doc, side, region).start;
        self.go_to_at(region, side, at, window, cx);
    }
    /// Goes to `region` as `go_to` does, with `side`'s caret at byte `at` inside it.
    fn go_to_at(
        &mut self,
        region: RegionIx,
        side: Side,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let other = region_bytes(&self.doc, side.other(), region).start;
        // Moving a caret focuses its editor, so `side` goes last and keeps the focus.
        for (to, at) in [(side.other(), other), (side, at)] {
            self.pane(to).editor.update(cx, |state, cx| {
                let position = state.text().offset_to_position(at);
                state.set_cursor_position(position, window, cx);
            });
        }
        self.pinned = Some(Pin { side, region, caret: at });
        self.reveal = Some(region);
        self.held_top = None;
        cx.notify();
    }

    /// Go to the other end of the move whose end on `side` holds `line`, as a jump does, with
    /// the focus in the other pane. A line outside every move's end does nothing.
    fn go_to_other_end(
        &mut self,
        side: Side,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = move_target(&self.doc, side, line) {
            self.go_to_at(target.region, side.other(), target.at, window, cx);
            self.start_flash(target.region, cx);
        }
    }

    /// Go to the other end of the move the focused pane's caret sits in.
    fn go_to_move_end(&mut self, _: &GoToMoveEnd, window: &mut Window, cx: &mut Context<Self>) {
        let Some(side) = self.focused(window, cx) else {
            return;
        };
        let state = self.pane(side).editor.read(cx);
        let line = state.text().offset_to_point(state.cursor()).row;
        self.go_to_other_end(side, line, window, cx);
    }

    /// Blink a frame around `region` in each pane where it has lines (the editor drops an
    /// empty range): shown and hidden in turn, starting shown and ending hidden.
    fn start_flash(&mut self, region: RegionIx, cx: &mut Context<Self>) {
        Flash::start(self, region, |this| &mut this.flash, Self::paint_flash, cx);
    }

    /// Frame the blinking region in both panes, on the blink's shown steps.
    fn paint_flash(&mut self, cx: &mut Context<Self>) {
        let shown = self.flash.lit().filter(|region| region.get() < self.doc.regions().len());
        let border = self.palette.tones(cx.theme().is_dark()).current();
        for side in SIDES {
            let frame = shown
                .map(|region| blink::region_frame(region_bytes(&self.doc, side, region), border));
            self.pane(side).flash.set(frame.into_iter().collect(), cx);
        }
        cx.notify();
    }

    /// Blink the match `side`'s search stepped to, if it stepped.
    fn follow_search_step(&mut self, side: Side, cx: &mut Context<Self>) {
        let pane = &mut self.panes[index(side)];
        let session = pane.editor.read(cx).search_session();
        let reached = pane.steps.stepped(session);
        let current = session
            .is_active()
            .then(|| session.matcher.current())
            .flatten()
            .and_then(|ix| session.matcher.matched_ranges().get(ix).cloned());
        // The current match opens its fold once, so a collapse after it stays collapsed.
        if current != self.collapse.searched[index(side)] {
            self.collapse.searched[index(side)].clone_from(&current);
            if let Some(bytes) = current {
                let line = self.pane(side).editor.read(cx).text().offset_to_point(bytes.start).row;
                self.open_folds_at(side, line, cx);
            }
        }
        if let Some(bytes) = reached {
            let target = (side, bytes.start, bytes.end);
            Flash::start(self, target, |this| &mut this.found, Self::paint_found, cx);
        }
    }

    /// Fill the blinking match, on the blink's shown steps.
    fn paint_found(&mut self, cx: &mut Context<Self>) {
        let lit = self.found.lit();
        let colour = self.palette.tones(cx.theme().is_dark()).current();
        for side in SIDES {
            let fill = lit
                .filter(|&(at, ..)| at == side)
                .map(|(_, start, end)| blink::match_flash(start..end, colour));
            self.pane(side).found.set(fill.into_iter().collect(), cx);
        }
        cx.notify();
    }

    /// The byte ranges `side`'s pane fills now after a search step.
    #[must_use]
    pub fn find_flashes(&self, side: Side, cx: &App) -> Vec<Range<usize>> {
        self.pane(side).found.get_ranges(cx)
    }

    /// Whether a frame is blinking after a jump.
    #[must_use]
    pub fn flashing(&self) -> bool {
        self.flash.running()
    }

    /// Whether `side`'s file holding `text`, in the pane's encoding, would be the window's
    /// own save: one written, or one being written.
    #[must_use]
    pub fn is_own_write(&self, side: Side, text: &str) -> bool {
        let pane = self.pane(side);
        pane.writes.holds(text, pane.encoding)
    }

    /// The byte ranges `side`'s pane frames now.
    #[must_use]
    pub fn flash_frames(&self, side: Side, cx: &App) -> Vec<Range<usize>> {
        self.pane(side).flash.get_ranges(cx)
    }

    /// Write each side with unsaved edits to its own path, as it stands now, off the UI
    /// thread and once the save before has landed, then snapshot the jj working copy when
    /// the run owns one. On a failure, an alert says why and the side keeps its marker; a
    /// failed snapshot leaves the file written and a banner says why. `after` says whether
    /// the window exits once every side is saved.
    fn save(&mut self, after: AfterSave, window: &mut Window, cx: &mut Context<Self>) {
        for side in SIDES {
            self.send_edits(side, window, cx);
        }
        let (writes, saves): (Vec<_>, Vec<_>) = self.side_writes().into_iter().unzip();
        for (write, save) in writes.iter().zip(&saves) {
            // A rope clone shares its chunks.
            let text = save.text.clone();
            self.pane_mut(write.side).writes.start(write.changes, text, save.encoding);
        }
        let working_copy = self.working_copy.clone();
        let before = self.saving.take();
        self.saving = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(before) = before {
                before.await;
            }
            let Saved { written, snapshot } = cx
                .background_spawn(async move { diff_tool::save(&saves, working_copy.as_ref()) })
                .await;
            let written = writes.into_iter().zip(written).collect();
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.snapshotted(snapshot);
                this.saved(written, after, window, cx);
            }));
        }));
    }

    /// What a save writes: each side with unsaved edits. A revision is read-only, so it is
    /// never modified.
    fn side_writes(&self) -> Vec<(SideWrite, SideSave)> {
        SIDES
            .into_iter()
            .filter_map(|side| {
                let pane = self.pane(side);
                let (true, Some(path)) = (pane.modified, pane.origin.path()) else {
                    return None;
                };
                let save = SideSave {
                    path: path.to_path_buf(),
                    encoding: pane.encoding,
                    text: self.doc.snapshot(side),
                };
                Some((SideWrite { side, changes: pane.changes }, save))
            })
            .collect()
    }

    /// Take the outcome of a save's snapshot: none when it took none. jj snapshots the file
    /// on its next command, so a failure only says why.
    fn snapshotted(&mut self, snapshot: Option<Result<(), VcsError>>) {
        match snapshot {
            None => {}
            Some(Ok(())) => self.snapshot_failed = None,
            Some(Err(error)) => {
                tracing::warn!(%error, "jj did not snapshot the save; its next command will");
                let detail = crate::error_text::describe(&error);
                self.snapshot_failed = Some(strings::snapshot_failed(&detail));
            }
        }
    }

    /// Take the results of a save's writes. A side edited while its write ran keeps its
    /// marker.
    fn saved(
        &mut self,
        written: Vec<(SideWrite, Result<(), SessionError>)>,
        after: AfterSave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut all_saved = true;
        for (write, result) in written {
            match result {
                Ok(()) => {
                    let pane = self.pane_mut(write.side);
                    let unchanged = pane.changes == write.changes;
                    pane.writes.ended(write.changes, true);
                    if unchanged {
                        self.set_modified(write.side, false);
                    }
                }
                Err(error) => {
                    all_saved = false;
                    self.pane_mut(write.side).writes.ended(write.changes, false);
                    let detail = crate::error_text::describe(&error);
                    let answer = window.prompt(
                        PromptLevel::Critical,
                        &prompt::cannot_save(),
                        Some(&detail),
                        &[prompt::ok_button()],
                        cx,
                    );
                    cx.spawn(async move |_, _| {
                        // The alert's one button only closes it.
                        let _ = answer.await;
                    })
                    .detach();
                }
            }
        }
        self.save_failed = !all_saved;
        cx.notify();
        if all_saved && after == AfterSave::Exit {
            finish(ExitCode::Resolved, cx);
        }
    }

    /// Finish with 0 once the saves running have landed: a save asked for before the close
    /// still reaches its file. A save that fails keeps the window open, and its alert says
    /// why.
    fn exit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(saving) = self.saving.take() else {
            finish(ExitCode::Resolved, cx);
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            saving.await;
            ignore_closed_view(this.update(cx, |this, cx| {
                if !this.save_failed {
                    finish(ExitCode::Resolved, cx);
                }
            }));
        })
        .detach();
    }

    /// Close the window, asking first when a side has unsaved edits. The window exits with 0.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for side in SIDES {
            self.send_edits(side, window, cx);
        }
        if !self.panes.iter().any(|p| p.modified) {
            self.exit(window, cx);
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            &prompt::save_changes(),
            None,
            &[
                PromptButton::new(prompt::save()),
                PromptButton::new(prompt::discard()),
                PromptButton::new(prompt::keep_editing()),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = answer.await else {
                return;
            };
            ignore_closed_view(this.update_in(cx, |this, window, cx| match CLOSING.get(answer) {
                Some(Closing::Save) => this.save(AfterSave::Exit, window, cx),
                Some(Closing::Discard) => this.exit(window, cx),
                Some(Closing::KeepEditing) | None => {}
            }));
        })
        .detach();
    }

    /// `side`'s file changed on disk (spec §6.1). A pane without edits reloads it; a pane
    /// with edits asks first. A file that cannot be read, or reads as the pane's own text,
    /// changes nothing. The file is read off the UI thread.
    pub fn disk_changed(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane(side);
        if pane.asking {
            return;
        }
        // A reload the user asked for stays one.
        let then = match pane.reading {
            Some((OnRead::Reload, _)) => OnRead::Reload,
            _ => OnRead::Ask,
        };
        self.read_disk(side, then, window, cx);
    }

    /// Read `side`'s file on a background thread, then act on it. A new read replaces the
    /// one running, whose result then goes unused. A revision has no file to read.
    fn read_disk(&mut self, side: Side, then: OnRead, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.pane(side).origin.path().map(Path::to_path_buf) else {
            return;
        };
        let before = self.doc.snapshot(side);
        let task = cx.spawn_in(window, async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let text = merge_session::read_text(&path);
                    let map = text
                        .as_ref()
                        .ok()
                        .filter(|text| before != text.content())
                        .map(|text| LineMap::from_rope(&before, text));
                    DiskRead { text, map }
                })
                .await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.disk_read(side, read, then, window, cx);
            }));
        });
        self.pane_mut(side).reading = Some((then, task));
    }

    fn disk_read(
        &mut self,
        side: Side,
        read: DiskRead,
        then: OnRead,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pane_mut(side).reading = None;
        let Some(text) = self.changed(side, read.text) else {
            return;
        };
        let pane = self.pane(side);
        if then == OnRead::Reload || !pane.modified {
            self.reload_from(side, &text, read.map.as_ref(), window, cx);
            return;
        }
        let question = prompt::changed_on_disk(&pane.origin.name());
        let answer = window.prompt(
            PromptLevel::Warning,
            &question,
            Some(&prompt::changed_detail()),
            &on_disk_change().map(PromptButton::new),
            cx,
        );
        self.pane_mut(side).asking = true;
        cx.spawn_in(window, async move |this, cx| {
            let reload = answer.await.ok().and_then(|answer| on_disk_change().get(answer).cloned())
                == Some(prompt::reload());
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.pane_mut(side).asking = false;
                // The file may have changed again while the prompt was open.
                if reload {
                    this.read_disk(side, OnRead::Reload, window, cx);
                }
            }));
        })
        .detach();
    }

    /// The text `read` from `side`'s file, unless the read failed or the text matches the
    /// pane.
    fn changed(&self, side: Side, read: Result<Text, SessionError>) -> Option<Text> {
        let text = match read {
            Ok(text) => text,
            Err(error) => {
                tracing::info!(%error, "cannot read a changed file; the pane keeps its text");
                return None;
            }
        };
        // A pane that reads one character per byte keeps the two sides' one fallback
        // (spec §4.1).
        let fallback = SIDES.iter().any(|&side| self.pane(side).encoding.one_byte_per_char());
        let text = if fallback { text.to_windows_1252().unwrap_or(text) } else { text };
        let pane = self.pane(side);
        let same = text.encoding() == pane.encoding && self.doc.text(side) == text.content();
        let own_save = pane.writes.holds(text.content(), text.encoding());
        (!same && !own_save).then_some(text)
    }

    /// Replace `side` with `text`, read from disk. The caret and the view's top move with
    /// their lines, through `map` from the pane's text to `text`.
    fn reload_from(
        &mut self,
        side: Side,
        text: &Text,
        map: Option<&LineMap>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let caret = self.editor(side).read(cx).cursor();
        let top = self.top_anchor(side, cx);
        let before = self.doc.snapshot(side);
        self.doc.reload(side, text);
        self.pinned = None;
        self.waiting.clear();
        let pane = self.pane_mut(side);
        pane.encoding = text.encoding();
        pane.changes += 1;
        pane.writes.reloaded();
        let caret =
            map.map_or(caret, |map| map.offset(before.slice(..), self.doc.text(side), caret));
        self.reload(side, caret, window, cx);
        if text.encoding() == Encoding::Windows1252 {
            self.read_bytewise(side.other(), window, cx);
        }
        self.held_top = top.map(|anchor| HeldTop {
            side,
            anchor: map.map_or(anchor, |map| anchor.on(map.line(anchor.line))),
            due: true,
            last: false,
        });
        self.set_modified(side, false);
        self.schedule_recompute(window, cx);
        self.refresh_bands(side, cx);
        tracing::info!(path = %self.pane(side).origin.name(), "reloaded");
    }

    /// Read `side`'s pane, if UTF-8 without a byte order mark, one character per byte beside
    /// the other side's Windows-1252 text (spec §4.1). Its edits and the caret's place in the
    /// bytes stay; its undo history goes, as a reload's does.
    fn read_bytewise(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane(side).encoding != Encoding::Utf8 {
            return;
        }
        let utf8 = self.doc.text(side).to_string();
        let caret = windows_1252_offset(&utf8, self.editor(side).read(cx).cursor().min(utf8.len()));
        let Some(text) = Text::from_utf8(utf8).to_windows_1252() else {
            return;
        };
        self.doc.reload(side, &text);
        let pane = self.pane_mut(side);
        pane.encoding = text.encoding();
        pane.changes += 1;
        self.reload(side, caret, window, cx);
        self.refresh_bands(side, cx);
    }

    /// Show `errors`, the config file's bad lines, in the settings banner; no errors close it.
    pub fn set_settings_errors(&mut self, errors: &[String], cx: &mut Context<Self>) {
        self.settings_errors = settings_banner::text(errors);
        cx.notify();
    }

    /// The settings banner's text, if it shows.
    #[must_use]
    pub fn settings_banner(&self) -> Option<&SharedString> {
        self.settings_errors.as_ref()
    }

    /// Give both panes the scroll top of the one that moved since the last sync (spec §5).
    /// Spacer rows make the panes the same height, so the same top shows the same rows. Runs
    /// in prepaint on every frame, so it sees every source of scrolling: wheel, scrollbar and
    /// caret.
    fn sync_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.level_wrapped(window, cx);
        self.sync_sideways(cx);
        let top = |side: Side| -self.pane(side).editor.read(cx).scroll_offset().y;
        let tops = [top(Side::Left), top(Side::Right)];
        let moved =
            |side: Side| (tops[index(side)] - self.synced[index(side)]).abs() > SCROLL_EPSILON;
        let Some(from) = SIDES.into_iter().find(|&side| moved(side)) else {
            return;
        };
        let target = tops[index(from)];
        self.synced = [target; 2];
        // The copy buttons follow the rows.
        cx.notify();
        self.scroll_pane(from.other(), ScrollTarget::top(target));
    }

    /// Give both panes the left offset of the one that moved since the last sync. A pane
    /// whose lines end short of that offset stops at its end, and does not pull the other
    /// back.
    fn sync_sideways(&mut self, cx: &mut Context<Self>) {
        let left = |side: Side| -self.panes[index(side)].editor.read(cx).scroll_offset().x;
        let follow = self.sideways_sync.step(
            f32::from(left(Side::Left)),
            f32::from(left(Side::Right)),
            |left| left,
            |left| left,
        );
        let (side, target) = match follow {
            Some(Follow::Result(target)) => (Side::Right, target),
            Some(Follow::Rows(target)) => (Side::Left, target),
            None => return,
        };
        self.scroll_pane(side, ScrollTarget::left(px(target)));
    }

    /// Set part of the offset `side`'s pane takes when it next lays out: the left before
    /// it lays out, the right after the sync (`render_panes`).
    fn scroll_pane(&mut self, side: Side, target: ScrollTarget) {
        self.targets[index(side)].set(target);
    }

    /// Give `side`'s pane its target, both axes in one call.
    fn apply_target(&mut self, side: Side, cx: &mut Context<Self>) {
        let target = std::mem::take(&mut self.targets[index(side)]);
        if target.is_empty() {
            return;
        }
        self.pane(side).editor.update(cx, |state, cx| {
            let offset = target.over(state.scroll_offset());
            state.set_scroll_offset(offset, cx);
        });
    }

    /// Scroll both panes to the region `go_to` set aside: centred when it fits, else its
    /// first row at the top. Runs before the left pane lays out, with the height of its last
    /// layout; before the first layout it waits a frame.
    fn apply_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A recompute since `go_to` may have dropped the region.
        let regions = self.doc.regions().len();
        let Some(region) = self.reveal.take().filter(|r| r.get() < regions) else {
            return;
        };
        let state = self.pane(Side::Left).editor.read(cx);
        let Some(line_h) = state.line_height() else {
            self.reveal = Some(region);
            window.refresh();
            return;
        };
        let view = state.input_bounds().size.height;
        let top = merge_view::scroll::reveal_top(
            self.region_display_rows(region, cx),
            f32::from(line_h),
            f32::from(view),
        );
        // The carets sit at the start of a line, so the panes go back to the left edge, as
        // the carets' own scroll would take them.
        for side in SIDES {
            self.scroll_pane(side, ScrollTarget { left: Some(Pixels::ZERO), top: Some(px(top)) });
        }
        self.sideways_sync.place(0.0, 0.0);
    }

    /// Where `side`'s view starts, by line; `None` before its first layout.
    fn top_anchor(&self, side: Side, cx: &App) -> Option<TopAnchor> {
        let state = self.editor(side).read(cx);
        let row_h = f32::from(state.line_height()?);
        let map = state.display_map();
        Some(TopAnchor::at(
            f32::from(self.shared_top(cx)),
            row_h,
            |row| map.display_row_to_buffer_line(row),
            |line| map.buffer_line_to_display_row(line),
        ))
    }

    /// Scroll both panes to the view top a reload keeps, when it is due. Runs before the left
    /// pane lays out, as [`Self::apply_reveal`] does.
    fn apply_held_top(&mut self, cx: &mut Context<Self>) {
        let Some(held) = self.held_top.filter(|held| held.due) else {
            return;
        };
        let state = self.editor(held.side).read(cx);
        let Some(row_h) = state.line_height() else {
            return;
        };
        let map = state.display_map();
        let top = held.anchor.top(f32::from(row_h), |line| map.buffer_line_to_display_row(line));
        self.held_top = (!held.last).then_some(HeldTop { due: false, ..held });
        for side in SIDES {
            self.scroll_pane(side, ScrollTarget::top(px(top)));
        }
    }

    /// The scroll top both panes share: the right's when it moved since the last sync, as
    /// the left takes it only in the next frame.
    fn shared_top(&self, cx: &App) -> Pixels {
        let top = |side: Side| -self.pane(side).editor.read(cx).scroll_offset().y;
        let right = top(Side::Right);
        if (right - self.synced[index(Side::Right)]).abs() > SCROLL_EPSILON {
            right
        } else {
            top(Side::Left)
        }
    }

    /// Replace region `region`'s lines in `to` with the other side's (spec §4.6).
    fn copy(&mut self, region: RegionIx, to: Side, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.doc.version();
        match self.doc.copy_region(region, to) {
            Ok(edited) => self.follow(before, &edited, Caret::AtChange, window, cx),
            Err(error) => tracing::error!(%error, %region, ?to, "the core refused a copy"),
        }
    }

    /// Note where the left pane's rows start against the gutter at `bounds`, which lays out
    /// after the left pane.
    fn place_gutter(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let rows_top = self.pane(Side::Left).editor.read(cx).input_bounds().top();
        let offset = rows_top - bounds.top();
        if (offset - self.gutter_offset).abs() > SCROLL_EPSILON {
            self.gutter_offset = offset;
            cx.notify();
        }
    }

    /// The display rows `region` spans in either pane: from the row where it starts, as many
    /// as its taller side takes. Rows above it count fold rows and leave out folded lines.
    fn region_display_rows(&self, region: RegionIx, cx: &App) -> Range<usize> {
        let region = &self.doc.regions()[region.get()];
        let rows = self.line_rows(cx);
        let start = self.region_row(region, cx);
        start
            ..start
                + rows(Side::Left, region.left.clone()).max(rows(Side::Right, region.right.clone()))
    }

    /// The display row where `region` starts. The side with more lines has no spacers of
    /// its own in the region, so its first line is the region's first row on both sides.
    fn region_row(&self, region: &Region2, cx: &App) -> usize {
        let side = if region.left.len() >= region.right.len() { Side::Left } else { Side::Right };
        let map = self.pane(side).editor.read(cx).display_map();
        let last = map.buffer_line_count().saturating_sub(1);
        map.buffer_line_to_display_row(region.lines(side).start.min(last))
    }

    /// The column between the panes, with copy buttons on each changed region's first row.
    /// Regions that lag the text have none: their numbers may not survive the recompute.
    fn render_gutter(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (border, hover) = (theme.border, theme.secondary_hover);
        let first_row = self.gutter_offset - self.shared_top(cx);
        let line_height = self.typography.line_height();
        let height = window.viewport_size().height;
        let writable = SIDES.map(|side| !self.pane(side).read_only);
        let rows: Vec<_> = if self.doc.regions_current() {
            self.doc
                .regions()
                .iter()
                .enumerate()
                .filter(|(_, r)| r.kind == Kind2::Changed)
                .map(|(region, r)| {
                    (RegionIx::new(region), first_row + line_height * self.region_row(r, cx))
                })
                .filter(|&(_, top)| top + line_height > Pixels::ZERO && top < height)
                .collect()
        } else {
            Vec::new()
        };
        let button = |region: RegionIx, to: Side, cx: &mut Context<Self>| {
            let (glyph, label) = match to {
                Side::Right => (strings::COPY_TO_RIGHT, strings::copy_to_right_label()),
                Side::Left => (strings::COPY_TO_LEFT, strings::copy_to_left_label()),
            };
            div().w(COPY_BUTTON_WIDTH).h_full().flex_none().when(writable[index(to)], |d| {
                d.child(
                    div()
                        .id(copy_id(region, to))
                        .test_support()
                        .role(Role::Button)
                        .aria_label(label)
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .child(glyph)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.copy(region, to, window, cx);
                        })),
                )
            })
        };
        let buttons: Vec<_> = rows
            .into_iter()
            .map(|(region, top)| {
                h_flex()
                    .absolute()
                    .top(top)
                    .left_0()
                    .h(line_height)
                    .child(button(region, Side::Right, cx))
                    .child(button(region, Side::Left, cx))
            })
            .collect();
        let this = cx.entity().downgrade();
        div()
            .relative()
            .w(GUTTER_WIDTH)
            .h_full()
            .flex_none()
            .overflow_hidden()
            .border_x_1()
            .border_color(border)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        ignore_closed_view(
                            this.update(cx, |this, cx| this.place_gutter(bounds, cx)),
                        );
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(buttons)
    }

    /// The overview ruler beside the right pane, with the pane's vertical scrollbar thumb.
    fn render_ruler(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let shown = ruler::Strip {
            marks: merge_view::ruler::diff_marks(&self.doc, Side::Right).collect(),
            total: merge_view::ruler::diff_total(&self.doc, Side::Right),
            feed: self.ruler_feed.clone(),
        };
        let (this, scrolled) = (cx.entity().downgrade(), cx.entity().downgrade());
        ruler::strip(
            DIFF_RULER_ID,
            shown,
            self.ruler_drag.clone(),
            *self.palette.tones(cx.theme().is_dark()),
            cx.theme().foreground,
            move |click, window, cx| {
                ignore_closed_view(this.update(cx, |this, cx| this.ruler_click(click, window, cx)));
            },
            move |top, _, cx| {
                ignore_closed_view(scrolled.update(cx, |this, cx| {
                    this.scroll_pane(Side::Right, ScrollTarget::top(px(top)));
                    cx.notify();
                }));
            },
        )
    }

    /// Go to the region a ruler mark stands for, or scroll the right pane to the line clicked.
    fn ruler_click(&mut self, click: ruler::Click, window: &mut Window, cx: &mut Context<Self>) {
        match click {
            ruler::Click::Region(region) => {
                self.go_to(region, Side::Right, window, cx);
                self.start_flash(region, cx);
            }
            ruler::Click::Line(line) => {
                let state = self.pane(Side::Right).editor.read(cx);
                let Some(line_h) = state.line_height() else {
                    return;
                };
                let view = f32::from(state.input_bounds().size.height);
                // Spacers and folds part display rows from lines.
                let row = state.display_map().buffer_line_to_display_row(line);
                let top = merge_view::ruler::centred_top(row, f32::from(line_h), view);
                self.scroll_pane(Side::Right, ScrollTarget::top(px(top)));
                cx.notify();
            }
        }
    }

    /// The pills over each end of a move whose first line shows in `side`'s pane (spec §5):
    /// on that line's row, against the pane's right edge. Each names the other end's line and
    /// goes there on a click. While the regions lag the text there are none.
    fn render_move_pills(&self, side: Side, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        let state = self.pane(side).editor.read(cx);
        let Some(line_height) = state.line_height() else {
            return Vec::new();
        };
        let bounds = state.input_bounds();
        let scroll = state.scroll_offset().y;
        let map = state.display_map();
        let shown: Vec<_> = move_ends(&self.doc, side)
            .into_iter()
            .filter_map(|end| {
                let row = map.buffer_line_to_display_row(end.lines.start);
                let top = bounds.top() + scroll + line_height * row;
                (top >= bounds.top() && top + line_height <= bounds.bottom()).then_some((end, top))
            })
            .collect();
        let text_size = self.typography.size * crate::window::CHIP_TEXT_SCALE;
        shown
            .into_iter()
            .map(|(end, top)| {
                let (line, other_line) = (end.lines.start, end.other_lines.start);
                let pill = move_pill(
                    move_pill_id(side, end.from),
                    strings::move_pill(side, other_line),
                    strings::move_pill_label(side, other_line),
                    line_height,
                    text_size,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.go_to_other_end(side, line, window, cx);
                }));
                gpui_kit::anchored()
                    .anchor(gpui_kit::Anchor::TopRight)
                    .position(point(bounds.right() - MOVE_PILL_INSET, top))
                    .child(pill)
                    .into_any_element()
            })
            .collect()
    }

    fn render_pane(&self, side: Side, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let pills = self.render_move_pills(side, cx);
        let theme = cx.theme();
        let pane = self.pane(side);
        let HeadingText { title, chip } = self.heading_text(side);
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                h_flex()
                    .gap_2()
                    .px_2()
                    .bg(theme.secondary)
                    .text_color(theme.muted_foreground)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_start()
                            .child(title),
                    )
                    .children(chip.map(|chip| {
                        revision_chip_element(
                            theme,
                            self.typography.size,
                            revision_chip_id(index(side)),
                            chip,
                        )
                    })),
            )
            .child(
                div()
                    .id(pane_id(side))
                    .test_support()
                    .key_context(PANE_CONTEXT)
                    .flex_1()
                    .min_h_0()
                    .child(
                        // The element sets the state's read-only flag each frame.
                        Editor::new(&pane.editor)
                            .aria_label(strings::diff_pane_label(
                                side == Side::Left,
                                &pane.origin.name(),
                            ))
                            .readonly(pane.read_only)
                            .font_family(self.typography.editor_font(cx.theme()))
                            .text_size(self.typography.size)
                            .line_height(gpui_kit::relative(self.typography.spacing))
                            .bordered(false)
                            .h(gpui_kit::relative(1.)),
                    )
                    .children(pills),
            )
    }
}

/// Watch both sides' files; a change reaches [`DiffWindow::disk_changed`].
fn watch(
    paths: &[Option<&Path>],
    cwd: &Path,
    window: &mut Window,
    cx: &mut Context<DiffWindow>,
) -> Watch {
    let changed = |this: &mut DiffWindow, at: usize, window: &mut Window, cx: &mut Context<_>| {
        if let Some(&side) = SIDES.get(at) {
            this.disk_changed(side, window, cx);
        }
    };
    crate::file_watch::watch(paths, cwd, changed, window, cx)
}

/// The banner for editable files `jj diff --tool` copied, whose edits it throws away
/// (spec §7.1).
fn jj_banner<'a>(paths: impl IntoIterator<Item = &'a Path>, cwd: &Path) -> Option<SharedString> {
    paths.into_iter().any(|path| in_jj_diff_copy(path, cwd)).then(strings::jj_diff_banner)
}

/// Whether `path`, taken from `cwd` when relative, lies in a side of the temporary directory
/// `jj diff --tool` makes: `jj-diff-<random>/left`, `/right` or `/output` (spec §7.1).
fn in_jj_diff_copy(path: &Path, cwd: &Path) -> bool {
    merge_session::origin::jj_diff_copy(path, cwd).is_some()
}

fn index(side: Side) -> usize {
    match side {
        Side::Left => 0,
        Side::Right => 1,
    }
}

fn title(origin: &Origin, modified: bool) -> SharedString {
    let name = escaped(&origin.name()).into_owned();
    if modified { format!("{name}{}", strings::MODIFIED).into() } else { name.into() }
}

/// How a new pane's editor looks.
struct Look<'a> {
    soft_wrap: bool,
    whitespace: bool,
    /// When the panes' bars show.
    scrollbars: ScrollbarMode,
    /// The right pane's scroll, which its ruler is the vertical bar of.
    ruler_feed: &'a scrollbars::Feed,
    typography: &'a Typography,
    tones: &'a Tones,
    /// The editor's language name, the same for both panes.
    language: &'static str,
    /// The chunked highlighter past `syntax-whole-file-max-lines`, for both panes.
    chunked: Option<InputHighlighterFactory>,
}

/// The notes of `rows`: blank notes on the spacer rows above each fold row, then the fold
/// row's count of hidden lines.
fn fold_notes(rows: &[FoldRow], colour: Hsla) -> Vec<SpacerNote> {
    rows.iter()
        .flat_map(|row| {
            let blank = std::iter::repeat_n(SpacerNote::new(row.line, "", colour), row.before);
            let note = SpacerNote::new(row.line, strings::unchanged_lines(row.hidden), colour);
            blank.chain(std::iter::once(note))
        })
        .collect()
}

fn new_pane(
    doc: &DiffDocument,
    side: Side,
    file: DiffFile,
    look: &Look,
    window: &mut Window,
    cx: &mut Context<DiffWindow>,
) -> Pane {
    let recorded = Recorded::default();
    let factory = recorded.clone();
    let decorations = bands(doc, side, look.tones);
    // The editor needs its own copy of the starting text.
    let text = doc.text(side).to_string();
    let editor = cx.new(|cx| {
        let mut state = EditorState::new(window, cx)
            .language(look.language)
            .line_number(true)
            .gutter_marks(true)
            .char_marks(crate::window::char_mark)
            .folding(false)
            .indent_guides(false)
            .soft_wrap(look.soft_wrap)
            .show_whitespaces(look.whitespace)
            .searchable(true)
            // Replace edits the right side only (spec §5, Two-way mode).
            .replaceable(side == Side::Right)
            .tab_size(look.typography.tab_size())
            .default_value(text);
        state.set_edit_observer(factory.observer());
        let ruler = (side == Side::Right).then(|| look.ruler_feed.clone());
        scrollbars::install(&mut state, look.scrollbars, ruler);
        if let Some(chunked) = &look.chunked {
            state.set_highlighter_factory(Rc::clone(chunked), cx);
        }
        state.set_spacers(spacers(doc, side).collect(), cx);
        state
    });
    // Collections paint in the order they are made: a search fill lies over the marks.
    let (bands, marks, flash, found) = editor.update(cx, |state, cx| {
        (
            state.create_range_decorations_collection(decorations, cx),
            state.create_range_decorations_collection(Vec::new(), cx),
            state.create_range_decorations_collection(Vec::new(), cx),
            state.create_range_decorations_collection(Vec::new(), cx),
        )
    });
    Pane {
        title: title(&file.origin, false),
        origin: file.origin,
        encoding: file.text.encoding(),
        editor,
        recorded,
        bands,
        marks,
        flash,
        found,
        steps: SearchSteps::default(),
        read_only: file.read_only,
        modified: false,
        asking: false,
        changes: 0,
        writes: OwnWrites::default(),
        reading: None,
        annotated: None,
    }
}

/// Each pane's annotation carried onto its current text, `texts`, left then right.
fn carried(origins: &[Option<Carried>; 2], texts: [&Text; 2]) -> [Option<Annotation>; 2] {
    std::array::from_fn(|n| {
        origins[n].as_ref().map(|from| carry(&from.annotation, &from.text, texts[n]))
    })
}

/// A full-line band over each of `side`'s changed regions, with each end of a move in the
/// move tone and the move mark.
fn bands(doc: &DiffDocument, side: Side, tones: &Tones) -> Vec<RangeDecoration> {
    diff::bands(doc, side)
        .into_iter()
        .map(|band| {
            let decoration = RangeDecoration::new(band.bytes)
                .with_style(RangeDecorationStyle::FullLine)
                .with_color(tones.band(band.tone));
            match band.other_end {
                Some(_) => decoration.with_gutter_mark(
                    GutterMark::new(MOVE_MARK.label, tones.mark(MOVE_MARK.tone)).clickable(),
                ),
                None => decoration,
            }
        })
        .collect()
}

impl DiffWindow {
    /// Bind the View menu's commands on `root`. Without origins there is nothing to blame,
    /// so the Blame key does nothing.
    fn on_view_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.when(self.origin_column.origins().loaded(), |root| {
            root.on_action(cx.listener(Self::toggle_origin_column))
        })
        .on_action(cx.listener(Self::toggle_syntax))
        .on_action(cx.listener(Self::toggle_soft_wrap))
        .on_action(cx.listener(Self::toggle_whitespace))
        .on_action(cx.listener(Self::toggle_collapse_unchanged))
        .on_action(cx.listener(Self::toggle_unified))
    }

    /// Bind the Go menu's commands on `root`.
    fn on_go_actions(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|this, _: &NextDifference, window, cx| {
            this.navigate(next_change, window, cx);
        }))
        .on_action(cx.listener(|this, _: &PreviousDifference, window, cx| {
            this.navigate(previous_change, window, cx);
        }))
        .on_action(cx.listener(Self::go_to_move_end))
    }
}

impl DiffWindow {
    /// The two panes, the gutter between them and the ruler.
    fn render_panes(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child({
                // Before the left pane lays out.
                let this = cx.entity().downgrade();
                canvas(
                    move |_, window, cx| {
                        ignore_closed_view(this.update(cx, |this, cx| {
                            this.apply_held_top(cx);
                            this.apply_reveal(window, cx);
                            this.apply_target(Side::Left, cx);
                        }));
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_0()
            })
            .child(self.render_pane(Side::Left, cx))
            .child({
                // Between the panes: after the left's prepaint, and before the
                // right's, which applies a scroll offset set here.
                let this = cx.entity().downgrade();
                canvas(
                    move |_, window, cx| {
                        ignore_closed_view(this.update(cx, |this, cx| {
                            this.sync_scroll(window, cx);
                            this.apply_target(Side::Right, cx);
                            // The left laid out this frame already, and its paint
                            // drops an offset set now: it takes its target next frame.
                            if !this.targets[index(Side::Left)].is_empty() {
                                window.refresh();
                            }
                        }));
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_0()
            })
            .child(self.render_gutter(window, cx))
            .child(self.render_pane(Side::Right, cx))
            .child(self.render_ruler(cx))
    }

    /// The unified pane across the window (spec §5): read-only, its keys the panes' keys.
    fn render_unified(
        &self,
        pane: &UnifiedPane,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let name = |side| self.pane(side).origin.name().into_owned();
        div()
            .id(UNIFIED_PANE_ID)
            .test_support()
            .key_context(PANE_CONTEXT)
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                Editor::new(pane.editor())
                    .aria_label(strings::unified_pane_label(&name(Side::Left), &name(Side::Right)))
                    .readonly(true)
                    .font_family(self.typography.editor_font(cx.theme()))
                    .text_size(self.typography.size)
                    .line_height(gpui_kit::relative(self.typography.spacing))
                    .bordered(false)
                    .h(gpui_kit::relative(1.)),
            )
    }
}

impl Render for DiffWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ticks = self.view_ticks();
        self.menu_bar.show(ticks, window, cx);
        let theme = cx.theme();
        let ui_font = theme.font_family.clone();
        let status = status_bar::bar(STATUS_ID, self.status.clone(), theme)
            .child(status_bar::item(STATUS_TEXT_ID, self.status.clone()));
        Self::on_go_actions(self.on_view_actions(v_flex(), cx), cx)
            .size_full()
            .child(self.fit.probe(Pixels::ZERO))
            .key_context(DIFF_CONTEXT)
            .on_action(cx.listener(Self::find))
            .on_action(cx.listener(|this, _: &CopyAsPatch, window, cx| {
                this.ask_copy_patch(PatchTarget::File, window, cx);
            }))
            .on_action(cx.listener(|this, _: &CopyDifferenceAsPatch, window, cx| {
                this.ask_copy_patch(PatchTarget::Difference, window, cx);
            }))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(
                cx.listener(|this, _: &Save, window, cx| this.save(AfterSave::Stay, window, cx)),
            )
            .on_action(cx.listener(|this, _: &SaveAndExit, window, cx| {
                this.save(AfterSave::Exit, window, cx);
            }))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.close(window, cx)))
            .font_family(ui_font)
            .text_size(self.typography.size)
            .child(crate::menu_bar::render(
                self.menu_bar.bar(),
                toolbar::bar(
                    strings::diff_toolbar(),
                    &DIFF_GROUPS,
                    DIFF_CONTEXT,
                    cx.theme(),
                    Labels::Shown,
                    self.typography.toolbar_size,
                    &toolbar::views_on(ticks).collect::<Vec<_>>(),
                ),
                cx.theme().is_dark(),
                window.is_fullscreen(),
            ))
            .children(render_banner(self.settings_errors.as_ref(), |v| &mut v.settings_errors, cx))
            .children(self.banner.clone().map(|text| alert::warning_banner(BANNER_ID, text, cx)))
            .children(
                self.snapshot_failed
                    .clone()
                    .map(|text| alert::warning_banner(SNAPSHOT_BANNER_ID, text, cx)),
            )
            .child(match &self.unified {
                Some(pane) => self.render_unified(pane, cx).into_any_element(),
                None => self.render_panes(window, cx).into_any_element(),
            })
            .child(status)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_change_on_disk_offers_keep_mine_first() {
        assert_eq!(super::on_disk_change(), ["Keep Mine", "Reload"]);
    }

    use super::*;

    fn copy(path: &str, cwd: &str) -> bool {
        in_jj_diff_copy(Path::new(path), Path::new(cwd))
    }

    #[test]
    fn each_side_of_jjs_directory_is_a_copy() {
        assert!(copy("/tmp/jj-diff-a1b2/left/src/a.rs", "/"));
        assert!(copy("/tmp/jj-diff-a1b2/right/a.rs", "/"));
        assert!(copy("/tmp/jj-diff-a1b2/output/a.rs", "/"));
    }

    #[test]
    fn other_paths_are_not() {
        assert!(!copy("/tmp/jj-diff-a1b2/left_state/a.rs", "/"), "jj's state");
        assert!(!copy("/tmp/jj-diff-a1b2/a.rs", "/"), "no side");
        assert!(!copy("/home/u/project/left/a.rs", "/"), "no jj directory");
        assert!(!copy("/tmp/jj-diff-/left/a.rs", "/"), "no random part");
        assert!(!copy("/tmp/my-jj-diff-a1/left/a.rs", "/"), "a prefix elsewhere");
    }

    #[test]
    fn relative_paths_start_from_the_working_directory() {
        assert!(copy("left/a.rs", "/tmp/jj-diff-a1b2"));
        assert!(copy("../jj-diff-a1b2/right/a.rs", "/tmp/elsewhere"));
        assert!(!copy("a.rs", "/tmp/jj-diff-a1b2"));
        // `..` leaves jj's directory before the side.
        assert!(!copy("/tmp/jj-diff-a1b2/../left/a.rs", "/"));
    }
}
