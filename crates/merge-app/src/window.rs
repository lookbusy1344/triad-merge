//! The merge window (DESIGN.md §A6.2): the toolbar, the inputs' aligned rows, the editable result
//! with its go-to-line field, and the status bar.
//!
//! The core is the source of truth. The editor makes the user's edits first; the window sends
//! them to the core and applies the core's corrections. Picks, undo and redo come from the
//! core as patches, which the window applies to the editor with recording muted.

use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui_kit::base::input::{self, GutterMark, SpacerNote};
use gpui_kit::base::{Scrollbar, ScrollbarMode};
use gpui_kit::component::input::{
    Editor, EditorState, Escape, InputEvent, InputState, RangeDecoration,
    RangeDecorationCollection, RangeDecorationStyle, RopeExt as _,
};
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Theme, h_flex, v_flex};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, ClickEvent, ClipboardItem, Context,
    DispatchPhase, Div, ElementId, Entity, EntityInputHandler as _, FocusHandle, FontWeight,
    HighlightStyle, Hsla, IndentAdjustment, InteractiveElement as _, IntoElement, LineFragment,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels,
    PromptLevel, Render, Role, ScrollEvent, SharedString, Size, StatefulInteractiveElement as _,
    Styled as _, StyledText, Subscription, Task, TestSupportExt as _, Toggled, Window,
    WindowOptions, canvas, div, point, prelude::FluentBuilder as _, px, uniform_list,
};
use merge_config::settings::{Colours, DEFAULT_COLOURS, Scrollbars};
use merge_core::boundaries::Boundaries;
use merge_core::diff::DiffOptions;
use merge_core::document::{
    Applied, Command, EditBatch, LabelCountError, MergeDocument, MergeOptions, OutputEnding,
    PickScope, Resolution, TextPatch,
};
use merge_core::index::{ByteOffset, Line, RegionIx};
use merge_core::journal::{InputHashes, Journal, RestoreError, input_hashes};
use merge_core::merge3::{Inputs, Kind, LEFT, RIGHT, Source};
use merge_core::suggest::{self, EditSuggestion};
use merge_core::text::Text;
use merge_session::ending::{self, Closing, Next, SaveFailed, Undone, UnresolvedSave};
use merge_session::io::WriteError;
use merge_session::origin::{self, find_merge};
use merge_session::repository::Handoff;
use merge_session::resolve::Record;
use merge_syntax::boundaries::SyntaxBoundaries;
use merge_syntax::language::Language;
use merge_view::blink::AtRest;
use merge_view::chunks::Limits;
use merge_view::heights::Heights;
use merge_view::highlight::{CellSpans, Highlights};
use merge_view::marks::{self, Band, Tone};
use merge_view::nav::{self, Pin, Target};
use merge_view::origin::hover_text;
use merge_view::placeholder;
use merge_view::rows::{self, PaneError, PaneSet, RowIx, RowMap};
use merge_view::ruler::RulerMark;
use merge_view::runs::{self, Matched};
use merge_view::scroll::{Displayed, Geometry, ResultRows};
use merge_view::search::{Case, Hit, InputMatches};
use merge_view::selection::InputSelection;
use merge_view::status::{FileNames, Status};
use merge_view::tabs;
use merge_view::{scroll, status};
use vcs::{IdKind, RepoKind, VcsError};

use crate::blink::{self, Flash, SearchSteps};
use crate::choice::{self, Choice, MenuAction, MenuChoice};
use crate::editor_sync::{self, Caret};
use crate::input_find::{self, Step};
use crate::input_select::{
    self, AUTOSCROLL_MAX_ROWS, AUTOSCROLL_TICK, CellHit, INPUT_PANES_CONTEXT, InputSelect,
};
use crate::measure::{self, Measure, Measurement, Mode, Report, Run};
use crate::origin_column::{
    Annotated, Described, HeadingText, ORIGIN_WIDTH, OriginCell, OriginColumn, Visibility,
};
use crate::pane_scroll::PaneScroll;
use crate::recorder::Recorded;
use crate::recovery::JournalTasks;
use crate::row_wrap::Fit;
use crate::ruler;
use crate::scrollbars;
use crate::settings_banner::{self, render as render_banner};
use crate::sideways;
use crate::strings::prompt;
use crate::syntax::{self, Colouring, InputSyntax};
use crate::tones::{Palette, Tones};
use crate::toolbar::{self, GROUPS, Labels};
use crate::wrap::Wrap;
use crate::{
    Cancel, CarryMovedEdit, ClearChoice, CopyInput, ExitCode, Find, GoToLine, LINE_HEIGHT,
    NextConflict, NextDifference, PreviousConflict, PreviousDifference, Redo, SaveAndExit,
    SaveCopyAs, SelectAllInput, TakeBase, TakeLeft, TakeLeftAllUnresolved, TakeLeftThenRight,
    TakeRight, TakeRightAllUnresolved, TakeRightThenLeft, TakeSortedUnion, TakeWholeLeft,
    TakeWholeRight, ToggleBase, ToggleBasePane, ToggleLeft, ToggleOriginColumn, ToggleRight,
    ToggleScrollSync, ToggleSoftWrap, ToggleSyntax, Typography, Undo, finish, go_to_line,
    ignore_closed_view, strings,
};
use merge_session::recovery::{Recovery, WriteMode, Written, prune};
use merge_session::session::{CannotSave, Refused, SaveJob, SaveTarget, Session};

/// One row of the side panes at the default text size: an editor line, so the rows and the
/// result stay level. [`Typography::line_height`] gives it for other sizes.
pub const ROW_HEIGHT: Pixels = LINE_HEIGHT;
const LINE_NUMBER_WIDTH: Pixels = px(52.);
const PICK_WIDTH: Pixels = px(18.);
/// Space between a pane's left edge and its pick target.
const PICK_INSET: Pixels = px(3.);
/// The conflict mark beside each input pane's line numbers.
const INPUT_MARK_WIDTH: Pixels = px(12.);
/// The line number slot of a filler row whose side deleted base lines there.
const DELETED_ROW_MARK: &str = "−";
/// What a pick target shows when its source is in the region's pick, when an unresolved
/// conflict shows its source undecided, and otherwise.
const PICKED: &str = "●";
const SHOWN: &str = "◐";
const UNPICKED: &str = "○";
const PANE_DIVIDER: Pixels = px(1.);
/// Pane headings, relative to the text size.
const HEADING_TEXT_SCALE: f32 = 0.9;
/// A heading's role chip, relative to the text size.
const CHIP_TEXT_SCALE: f32 = 0.75;
const CHIP_ALPHA: f32 = 0.18;
const CHIP_RADIUS: Pixels = px(3.);
const HEADING_PADDING: Pixels = px(3.);
/// The bar in the pane's tone under each heading.
const ACCENT_HEIGHT: Pixels = px(2.);
/// How long the journal waits after a command before it is written (spec §6.2).
pub const JOURNAL_DEBOUNCE: Duration = Duration::from_millis(500);
/// The index of the answer that goes ahead in every confirmation.
const GO_AHEAD: usize = 0;

/// A close question's button label.
fn closing_label(answer: Closing) -> &'static str {
    match answer {
        Closing::SaveAndExit => prompt::SAVE_AND_EXIT,
        Closing::Discard => prompt::DISCARD,
        Closing::KeepEditing => prompt::KEEP_EDITING,
    }
}

/// The title of the alert after a save that left `undone` undone.
fn undone_title(undone: Undone) -> &'static str {
    match undone {
        Undone::Write => prompt::CANNOT_WRITE,
        Undone::Record(_) => prompt::CANNOT_RECORD,
    }
}

/// A failed save's alert's button label.
fn failed_label(answer: SaveFailed) -> &'static str {
    match answer {
        SaveFailed::TryAgain => prompt::TRY_AGAIN,
        SaveFailed::SaveCopyAs => prompt::SAVE_COPY_AS,
        SaveFailed::KeepEditing => prompt::KEEP_EDITING,
        SaveFailed::Quit => prompt::QUIT,
    }
}

/// Scroll tops closer than this are equal, so a synced pane does not echo back.
pub(crate) const SCROLL_EPSILON: Pixels = px(scroll::TOP_EPSILON);
/// Soft wrap lays out every line before the first frame, so it is offered only for inputs of
/// up to this many lines (spec §5).
pub const SOFT_WRAP_MAX_LINES: usize = 10_000;
/// The opacity of the marks on characters that differ, in the top panes.
const INTRALINE_ALPHA: f32 = 0.35;

/// The key contexts of the bindings in [`crate::init`].
pub(crate) const WINDOW_CONTEXT: &str = "MergeWindow";
pub(crate) const RESULT_CONTEXT: &str = "MergeResult";

pub use crate::go_to_line::{GO_TO_LINE_FIELD_ID, GO_TO_LINE_ID};
pub use crate::input_find::{input_match_count_id, input_match_next_id, input_match_previous_id};

/// Element ids, also used by tests.
pub const RESULT_ID: &str = "result";
/// The overview ruler beside the result.
pub const RULER_ID: &str = "ruler";
pub const ROWS_ID: &str = "rows";
pub const INPUT_LINES_ID: &str = "input-lines";
pub const STATUS_ID: &str = "status";
pub const TOOLBAR_ID: &str = "toolbar";
/// The toolbar's count of unresolved conflicts.
pub const CONFLICT_BADGE_ID: &str = "conflict-badge";
/// The status bar's warning that the saved output mixes line endings.
pub const MIXED_ENDINGS_ID: &str = "mixed-endings";
/// The status bar's note after Save Copy As.
pub const COPY_NOTE_ID: &str = "copy-note";

/// The pick target of `pane` on `row`, the first row of a region that differs.
#[must_use]
pub fn pick_id(pane: usize, row: RowIx) -> ElementId {
    ElementId::Name(format!("pick-{pane}-{row}").into())
}

/// The cell of `pane` on `row`.
#[must_use]
pub fn cell_id(pane: usize, row: RowIx) -> ElementId {
    ElementId::Name(format!("cell-{pane}-{row}").into())
}

/// The text of `pane`'s cell on `row`, which clips and scrolls sideways.
#[must_use]
pub fn cell_text_id(pane: usize, row: RowIx) -> ElementId {
    ElementId::Name(format!("cell-text-{pane}-{row}").into())
}

/// Display row `sub` of `pane`'s cell on `row` while it wraps, after its first.
#[must_use]
pub fn cell_part_id(pane: usize, row: RowIx, sub: usize) -> ElementId {
    ElementId::Name(format!("cell-{pane}-{row}-{sub}").into())
}

/// The text on display row `sub` of `pane`'s cell on `row`, after its first.
#[must_use]
pub fn cell_text_part_id(pane: usize, row: RowIx, sub: usize) -> ElementId {
    ElementId::Name(format!("cell-text-{pane}-{row}-{sub}").into())
}

/// The revision chip of `pane`'s heading, which holds the hover card.
#[must_use]
pub fn revision_chip_id(pane: usize) -> ElementId {
    ElementId::Name(format!("revision-{pane}").into())
}

/// The origin column cell of `pane` on `row`, which holds the hover card.
#[must_use]
pub fn origin_id(pane: usize, row: RowIx) -> ElementId {
    ElementId::Name(format!("origin-{pane}-{row}").into())
}

/// Whether the editors colour their text by its language's syntax (spec §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    Highlighted,
    Plain,
}

/// The `syntax-highlighting` setting: `true` is [`Syntax::Highlighted`].
impl From<bool> for Syntax {
    fn from(on: bool) -> Self {
        if on { Self::Highlighted } else { Self::Plain }
    }
}

impl PickState {
    fn of(doc: &MergeDocument, region: RegionIx, source: Source) -> Self {
        let unresolved = *doc.resolution(region) == Resolution::Unresolved;
        if doc.is_picked(region, source) {
            Self::On
        } else if unresolved && matches!(source, Source::Side(_)) {
            Self::Shown
        } else {
            Self::Off
        }
    }
}

/// Where a merge run's line origins come from (spec §7.3): its output, title and working
/// directory, as [`merge_session::origin::find_merge`] takes them, and the run's repository.
#[derive(Debug, Clone)]
pub struct OriginLookup {
    pub output: PathBuf,
    pub title: Option<String>,
    pub cwd: PathBuf,
    /// The repository the command line opened; an empty slot finds it from the paths.
    pub repo: Handoff,
}

impl OriginLookup {
    /// The sources of the merge of `inputs`, in the run's repository.
    fn find(&self, inputs: &Inputs) -> Result<origin::Found, VcsError> {
        find_merge(self.repo.take(), &self.cwd.join(&self.output), self.title.as_deref(), inputs)
    }
}

/// Choices for one window that do not change the merge.
#[derive(Debug, Clone)]
pub struct WindowSettings {
    /// The window title; [`strings::APP_NAME`] without one.
    pub title: Option<String>,
    /// The result opens with soft wrap, for inputs of up to [`SOFT_WRAP_MAX_LINES`] lines.
    pub soft_wrap: bool,
    /// Where Save & Exit writes (spec §6.2).
    pub output: Option<PathBuf>,
    /// The VCS conflict each Save & Exit to `output` is recorded in (spec §7.4); none
    /// outside `resolve`.
    pub record: Option<Record>,
    /// Where the recovery journal goes; none without one.
    pub journal_dir: Option<PathBuf>,
    /// The path that names the journal from one run to the next (spec §6.2); `output` without
    /// one.
    pub journal_name: Option<PathBuf>,
    pub typography: Typography,
    /// The size the window opens at; no larger than its display.
    pub window_size: Size<Pixels>,
    /// The rows and the result share a scroll position (spec §5).
    pub scroll_sync: bool,
    /// When the panes show their scrollbars (spec §6.4).
    pub scrollbars: Scrollbars,
    /// A three-way merge opens with the base pane showing.
    pub base_pane: bool,
    /// Whether the window opens with the origin column (spec §6.4).
    pub origin_column: Visibility,
    /// The ID that names a jj revision in the headings and the origin column (spec §6.4).
    pub revision_id: IdKind,
    /// The windows' appearance (spec §6.4).
    pub theme: merge_config::settings::Theme,
    /// The tones' colours (spec §6.4).
    pub colours: Colours,
    /// What Save & Exit does while conflicts are unresolved (spec §6.4).
    pub unresolved_save: UnresolvedSave,
    /// The line endings Save writes (spec §6.4).
    pub line_ending: OutputEnding,
    /// The result colours its text by the language its file names name (spec §6.4).
    pub syntax: Syntax,
    /// The longest result, in lines, its editor colours by syntax (spec §6.4).
    pub syntax_max_lines: usize,
    /// Where the inputs' line origins come from; none for a merge no VCS started.
    pub origins: Option<OriginLookup>,
    /// What the open dialog aligns the merge it starts with.
    pub options: MergeOptions,
    /// What the open dialog compares the diff it starts with (spec §6.4).
    pub diff_options: DiffOptions,
    /// Drive the window and report its frames (DESIGN.md §A6.7).
    pub measure: Option<Measure>,
    /// The inputs' file names, which title the panes of inputs without a label.
    pub file_names: FileNames,
    /// The repository the merge runs in, which the status bar names; none outside one.
    pub repo: Option<RepoKind>,
    /// The run's working directory; none when the process has none.
    pub cwd: Option<PathBuf>,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            title: None,
            soft_wrap: false,
            output: None,
            record: None,
            journal_dir: None,
            journal_name: None,
            typography: Typography::default(),
            window_size: crate::window_size(merge_config::settings::DEFAULT_WINDOW_SIZE),
            scroll_sync: true,
            scrollbars: Scrollbars::Always,
            base_pane: false,
            origin_column: Visibility::Hidden,
            revision_id: IdKind::default(),
            theme: merge_config::settings::Theme::System,
            colours: DEFAULT_COLOURS,
            unresolved_save: UnresolvedSave::Confirm,
            line_ending: OutputEnding::Keep,
            syntax: Syntax::Highlighted,
            syntax_max_lines: merge_config::settings::DEFAULT_SYNTAX_MAX_LINES,
            origins: None,
            options: MergeOptions::default(),
            diff_options: merge_config::settings::Settings::default().two_way(),
            measure: None,
            file_names: FileNames::default(),
            repo: None,
            cwd: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Panes(#[from] PaneError),
    #[error("cannot open the window: {0}")]
    Window(#[from] anyhow::Error),
}

/// Why the session cannot switch to a rebuilt document.
#[derive(Debug, thiserror::Error)]
enum RebuildError {
    #[error(transparent)]
    Restore(#[from] RestoreError),
    #[error(transparent)]
    Labels(#[from] LabelCountError),
    #[error(transparent)]
    Panes(#[from] PaneError),
}

/// Log the refusal of `what`: expected while a save runs, a defect otherwise.
fn log_refused(refused: &Refused, what: &str) {
    match refused {
        Refused::Saving => tracing::debug!(what, "refused while a save runs"),
        Refused::Core(error) => tracing::error!(%error, what, "the core refused"),
    }
}

/// `doc` with the rows of `panes`.
fn with_rows(doc: MergeDocument, panes: PaneSet) -> Result<(MergeDocument, RowMap), RebuildError> {
    let rows = RowMap::new(&doc, panes)?;
    Ok((doc, rows))
}

/// How the result editor colours by syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Highlighter {
    /// gpui-component's highlighter parses the whole file.
    Whole,
    /// Past `syntax-whole-file-max-lines`, a chunk at a time (`editor_syntax`).
    Chunked,
}

pub struct MergeWindow {
    menu_bar: crate::menu_bar::WindowMenu,
    /// The document, its journal and its save: the source of truth.
    session: Session,
    palette: Palette,
    panes: PaneSet,
    rows: RowMap,
    highlights: Highlights,
    /// The inputs' syntax spans, as their parses land.
    syntax: InputSyntax,
    colouring: Colouring,
    result_highlighter: Highlighter,
    /// The inputs' parse has started; it runs once, when the colours first show.
    parsing: bool,
    headings: Vec<Heading>,
    /// The result pane's heading, from the output's file name.
    result_heading: Heading,
    file_names: FileNames,
    /// The tasks that write the session's journal.
    journal: JournalTasks,
    result: Entity<EditorState>,
    regions: RangeDecorationCollection,
    /// The bands `regions` was last given, and in which tones; the editor moves them with
    /// each edit.
    shown_bands: (Vec<Band>, Tones),
    /// The frame around the highlighted region in the result.
    highlight: RangeDecorationCollection,
    /// The current region and its bytes in the result, unless it is unchanged.
    highlighted: Option<(RegionIx, Range<usize>)>,
    /// The highlight's blink after a jump.
    flash: Flash<()>,
    /// The fill a step through the result's search blinks over the match it reached.
    found_flash: RangeDecorationCollection,
    /// The steps the result's search takes.
    result_steps: SearchSteps,
    /// The blink of the match a search step reached.
    found: Flash<Found>,
    recorded: Recorded,
    status: SharedString,
    repo: Option<RepoKind>,
    /// The core's counts behind the status line and the conflict badge.
    counts: Status,
    /// The overview ruler's marks (`docs/DESIGN.md` §E4).
    ruler: Rc<[RulerMark]>,
    /// The result's scroll, for the ruler's thumb.
    ruler_feed: scrollbars::Feed,
    ruler_drag: ruler::Dragging,
    /// When the rows' bar shows (spec §6.4).
    scrollbar_mode: ScrollbarMode,
    /// The number of the conflict holding the caret, for the conflict badge.
    conflict_number: Option<usize>,
    /// The panes' scroll, soft wrap, scroll sync and the region `go_to` brings into view.
    scroll: PaneScroll,
    /// The region of each result placeholder, in order, for scroll sync's result rows.
    placeholders: Vec<RegionIx>,
    typography: Typography,
    /// The config file's bad lines, which the settings banner shows.
    settings_errors: Option<SharedString>,
    /// What the last Save Copy As wrote, until the merge next changes.
    copy_note: Option<SharedString>,
    /// The latest copy being written. Each copy waits for the one before, and the window
    /// exits only once the last has landed.
    copying: Option<Task<()>>,
    /// Save & Exit's write and record, running off the UI thread while the session is
    /// saving.
    save_task: Option<Task<()>>,
    go_to_line: go_to_line::Field,
    /// The input panes' matches of the result search's query.
    input_find: input_find::Found,
    /// The top panes' focus and selection.
    input_select: InputSelect,
    /// The steps that scroll the top panes while a selection drag holds the pointer past
    /// their edge.
    autoscroll: Option<Task<()>>,
    pinned: Option<Pin>,
    measurement: Measurement,
    /// The inputs' line origins and their column.
    origin_column: OriginColumn,
    _subscriptions: Vec<Subscription>,
}

/// The panes a merge shows at first: left and right, or every side of a wider merge.
#[must_use]
pub fn default_panes(doc: &MergeDocument) -> PaneSet {
    let inputs = doc.inputs();
    if inputs.is_three_way() { PaneSet::LeftRight } else { PaneSet::Sides(inputs.sides().len()) }
}

/// Open a merge window.
///
/// # Errors
///
/// [`OpenError`] when the panes do not fit the merge or the platform refuses a window.
pub fn open(
    doc: MergeDocument,
    settings: &WindowSettings,
    cx: &mut App,
) -> Result<(AnyWindowHandle, Entity<MergeWindow>), OpenError> {
    crate::apply_theme(settings.theme, cx);
    let panes = match default_panes(&doc) {
        PaneSet::LeftRight if settings.base_pane => PaneSet::LeftBaseRight,
        panes => panes,
    };
    let rows = RowMap::new(&doc, panes)?;
    let options = WindowOptions {
        window_bounds: Some(crate::window_bounds(settings.window_size, cx)),
        ..crate::menu_bar::window_options()
    };
    let title = settings.title.clone().unwrap_or_else(|| strings::APP_NAME.to_owned());
    Ok(gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title(&title);
        let view = cx.new(|cx| MergeWindow::new(doc, panes, rows, settings, window, cx));
        // Keys reach the window only through a focused element.
        let result = view.read(cx).result.clone();
        result.update(cx, |state, cx| state.focus(window, cx));
        // A measured run starts at the top, so runs compare with the ones in DESIGN.md §A6.7.
        if settings.measure.is_none() {
            view.update(cx, |this, cx| this.open_on_first_target(window, cx));
        }
        // Closing the window is Cancel, which decides whether and how the session ends.
        let this = view.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            ignore_closed_view(this.update(cx, |this, cx| this.cancel(window, cx)));
            false
        });
        view
    })?)
}

/// The language a merge's file names give.
fn language(settings: &WindowSettings) -> Option<Language> {
    Language::of_run(
        settings.file_names.paths(),
        settings.output.as_deref(),
        settings.title.as_deref(),
    )
}

/// The result editor on `doc`'s text, recording its edits in `recorded`, and whether it
/// colours a chunk at a time.
fn result_editor(
    doc: &MergeDocument,
    settings: &WindowSettings,
    wrap: Wrap,
    colouring: Colouring,
    recorded: &Recorded,
    window: &mut Window,
    cx: &mut Context<MergeWindow>,
) -> (Entity<EditorState>, Highlighter) {
    // The editor needs its own copy of the starting text.
    let text = doc.text();
    let language = colouring.editor_language();
    let chunked = colouring.chunked_factory(doc.line_count());
    let highlighter = if chunked.is_some() { Highlighter::Chunked } else { Highlighter::Whole };
    let editor = cx.new(|cx| {
        let mut state = EditorState::new(window, cx)
            .language(language)
            .line_number(true)
            .folding(false)
            .indent_guides(false)
            .soft_wrap(wrap.wraps())
            .gutter_marks(true)
            .searchable(true)
            .tab_size(settings.typography.tab_size())
            .default_value(text);
        state.set_edit_observer(recorded.observer());
        if let Some(factory) = chunked {
            state.set_highlighter_factory(factory, cx);
        }
        state
    });
    (editor, highlighter)
}

impl MergeWindow {
    /// The colours of the window's theme.
    fn tones(&self, cx: &App) -> &Tones {
        self.palette.tones(cx.theme().is_dark())
    }

    fn new(
        doc: MergeDocument,
        panes: PaneSet,
        rows: RowMap,
        settings: &WindowSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let recorded = Recorded::default();
        let wrap = input_wrap(&doc, settings.soft_wrap);
        let palette = Palette::new(&settings.colours);
        let tones = palette.tones(cx.theme().is_dark());
        let bands = marks::bands(&doc);
        let decorations = region_decorations(&bands, tones);
        let (spacers, notes, placeholders) = result_placeholders(&doc, tones);
        let colouring =
            Colouring::new(language(settings), settings.syntax, settings.syntax_max_lines);
        let (result, result_highlighter) =
            result_editor(&doc, settings, wrap, colouring, &recorded, window, cx);
        let (regions, highlight, found_flash) = result.update(cx, |state, cx| {
            state.set_spacers(spacers, cx);
            state.set_spacer_notes(notes, cx);
            (
                state.create_range_decorations_collection(decorations, cx),
                state.create_range_decorations_collection(Vec::new(), cx),
                state.create_range_decorations_collection(Vec::new(), cx),
            )
        });
        let go_to_line = go_to_line::Field::new(window, cx);
        let input_focus = cx.focus_handle();
        let subscriptions = Self::subscribe(&result, go_to_line.input(), &input_focus, window, cx);
        if let Some(lookup) = settings.origins.clone() {
            Self::load_origins(lookup, &doc, window, cx);
        }
        Self::count_widest(&doc, settings.typography.tab_width, cx);
        let syntax = colouring
            .language()
            .map(|language| Self::input_syntax(language, &doc))
            .unwrap_or_default();
        let (sides, bases) = (doc.inputs().sides().len(), doc.inputs().bases().len());
        let counts = status::status(&doc, settings.line_ending);
        let ruler = merge_view::ruler::marks(&doc).collect();
        let scrollbar_mode = scrollbars::mode(settings.scrollbars);
        let ruler_feed = scrollbars::Feed::default();
        result.update(cx, |state, _| {
            scrollbars::install(state, scrollbar_mode, Some(ruler_feed.clone()));
        });
        let recovery = Self::journaling(&doc, settings, window, cx);
        let target = SaveTarget {
            output: settings.output.clone(),
            record: settings.record.clone(),
            line_ending: settings.line_ending,
            unresolved_save: settings.unresolved_save,
        };
        Self {
            menu_bar: crate::menu_bar::WindowMenu::new(cx),
            status: strings::status(settings.repo, &counts).into(),
            repo: settings.repo,
            counts,
            ruler,
            ruler_feed,
            ruler_drag: ruler::Dragging::default(),
            scrollbar_mode,
            conflict_number: None,
            highlights: Highlights::new(panes),
            syntax,
            colouring,
            result_highlighter,
            parsing: colouring.language().is_some(),
            headings: headings(&doc, panes, &settings.file_names),
            result_heading: status::result_heading(settings.output.as_deref()).into(),
            file_names: settings.file_names.clone(),
            journal: JournalTasks::default(),
            session: Session::new(doc, target, recovery),
            panes,
            scroll: PaneScroll::new(rows.row_count(), wrap, settings.scroll_sync),
            rows,
            result,
            regions,
            shown_bands: (bands, *tones),
            highlight,
            highlighted: None,
            flash: Flash::default(),
            found_flash,
            result_steps: SearchSteps::default(),
            found: Flash::default(),
            recorded,
            placeholders,
            typography: settings.typography.clone(),
            palette,
            settings_errors: None,
            copy_note: None,
            copying: None,
            save_task: None,
            go_to_line,
            input_find: input_find::Found::default(),
            input_select: InputSelect::new(input_focus),
            autoscroll: None,
            pinned: None,
            measurement: Measurement::new(settings.measure),
            origin_column: OriginColumn::new(
                sides,
                bases,
                settings.origin_column,
                settings.revision_id,
            ),
            _subscriptions: subscriptions,
        }
    }

    /// The recovery journal `settings` asks for, with the inputs' hashes computing.
    fn journaling(
        doc: &MergeDocument,
        settings: &WindowSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Recovery> {
        settings.journal_dir.clone().map(|dir| {
            Self::hash_inputs(doc, &dir, window, cx);
            let name = settings.journal_name.clone().or_else(|| settings.output.clone());
            Recovery::new(dir, name)
        })
    }

    /// Find and annotate the inputs off the UI thread; they show their origins when that
    /// finishes. Only a three-way merge has the base, left and right a VCS run names.
    fn load_origins(
        lookup: OriginLookup,
        doc: &MergeDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Texts share their content, so the copy is O(1).
        let inputs = doc.inputs().clone();
        cx.spawn_in(window, async move |this, cx| {
            let annotations = cx
                .background_spawn(async move {
                    let found = lookup.find(&inputs)?;
                    let texts: Vec<&Text> = origin::merge_inputs(&inputs).map(|(_, t)| t).collect();
                    Ok::<_, VcsError>((found.annotate(&texts), found.describe()))
                })
                .await;
            ignore_closed_view(this.update(cx, |this, cx| this.loaded_origins(annotations, cx)));
        })
        .detach();
    }

    /// Count each input's widest line off the UI thread, for the top panes' sideways scroll.
    fn count_widest(doc: &MergeDocument, tab_width: usize, cx: &mut Context<Self>) {
        let inputs = doc.inputs();
        let sides = (0..inputs.sides().len()).map(Source::Side);
        let bases = (0..inputs.bases().len()).map(Source::Base);
        // Texts share their content, so the copies are O(1).
        let texts: Vec<_> =
            sides.chain(bases).map(|source| (source, inputs.text(source).clone())).collect();
        cx.spawn(async move |this, cx| {
            let widest =
                cx.background_spawn(async move { sideways::widest(&texts, tab_width) }).await;
            ignore_closed_view(this.update(cx, |this, cx| {
                this.scroll.sideways_mut().set_widest(widest);
                cx.notify();
            }));
        })
        .detach();
    }

    /// The inputs' syntax under `language`, none parsed yet: each chunk parses when its rows
    /// show (`docs/DESIGN.md` §F4).
    fn input_syntax(language: Language, doc: &MergeDocument) -> InputSyntax {
        let grammar = match syntax::grammar(language) {
            Ok(grammar) => grammar,
            Err(error) => {
                tracing::warn!(%error, "the top panes show plain");
                return InputSyntax::default();
            }
        };
        let inputs = doc.inputs();
        let sides = (0..inputs.sides().len()).map(Source::Side);
        let bases = (0..inputs.bases().len()).map(Source::Base);
        let texts = sides.chain(bases).map(|source| (source, inputs.text(source)));
        InputSyntax::new(grammar, texts, merge_syntax::syntax::CHUNK_LINES, Limits::DEFAULT)
    }

    /// Parse the queued chunks off the UI thread, as many as each input has parses free. A
    /// chunk's rows draw in colour when it lands.
    fn start_parses(&mut self, cx: &mut Context<Self>) {
        while let Some(parse) = self.syntax.next_parse() {
            cx.spawn(async move |this, cx| {
                let (source, chunk) = (parse.source, parse.chunk);
                let spans = cx.background_spawn(async move { parse.run() }).await;
                ignore_closed_view(this.update(cx, |this, cx| {
                    this.syntax.landed(source, chunk, Some(spans));
                    this.chunk_landed(source, chunk);
                    this.start_parses(cx);
                    cx.notify();
                }));
            })
            .detach();
        }
    }

    /// Note that chunk `chunk` of `source` has landed; a run measuring syntax reports when.
    fn chunk_landed(&self, source: Source, chunk: merge_view::chunks::ChunkId) {
        let Some(run) = self.measurement.running().filter(|run| run.mode() == Mode::Syntax) else {
            return;
        };
        let took = run.started().elapsed();
        let what = format!("parsed {source:?} {chunk:?}");
        eprintln!("{}: {}", strings::MEASURE, measure::span(&what, took));
        if self.syntax.pending() == 0 {
            eprintln!("{}: {}", strings::MEASURE, measure::span(measure::PARSED, took));
        }
    }

    fn loaded_origins(
        &mut self,
        found: Result<(Annotated, Described), VcsError>,
        cx: &mut Context<Self>,
    ) {
        let (annotations, described) = match found {
            Ok(found) => found,
            Err(error) => {
                tracing::warn!(%error, "cannot find the inputs' line origins; none show");
                return;
            }
        };
        let sources = || origin::merge_inputs(self.session.document().inputs()).map(|(s, _)| s);
        let sources: Vec<Source> = sources().collect();
        self.origin_column.described(sources.iter().copied(), described);
        self.origin_column.annotated(sources, annotations);
        self.refresh_result_labels(cx);
        cx.notify();
    }

    /// `pane`'s heading as drawn: its title, without a leading ID its chip shows, and the
    /// revision chip; none for a pane the window lacks.
    #[must_use]
    pub fn heading_text(&self, pane: usize) -> Option<HeadingText> {
        let source = *self.panes.sources().get(pane)?;
        Some(self.origin_column.heading(source, &self.headings.get(pane)?.title))
    }

    /// The revision chip of `pane`'s heading: none for a pane whose input shows no commit.
    #[must_use]
    pub fn revision_chip(&self, pane: usize) -> Option<OriginCell> {
        self.heading_text(pane)?.chip
    }

    /// The origin column of `pane` on `row`: none for a filler row, a row or pane the window
    /// lacks, or a line without an origin.
    #[must_use]
    pub fn origin_cell(&self, pane: usize, row: RowIx) -> Option<OriginCell> {
        let source = *self.panes.sources().get(pane)?;
        let cell = *self.rows.get_row(self.session.document(), row)?.cells.get(pane)?;
        self.origin_column.cell(source, cell)
    }

    /// The origin column's cells of one row, pane by pane; none while the column is hidden.
    fn row_origins(
        &self,
        cells: &[Option<usize>],
    ) -> Option<std::vec::IntoIter<Option<OriginCell>>> {
        self.origin_column.shown().then(|| {
            self.panes
                .sources()
                .into_iter()
                .zip(cells)
                .map(|(source, &line)| self.origin_column.cell(source, line))
                .collect::<Vec<_>>()
                .into_iter()
        })
    }

    /// The origin of result line `line`, as its gutter shows it; none for a line without one.
    #[must_use]
    pub fn result_origin(&self, line: usize) -> Option<OriginCell> {
        let doc = self.session.document();
        let origins = self.origin_column.origins();
        let origin = origins.result_line(doc, Line::new(line))?;
        Some(OriginCell {
            label: origins
                .result_label(doc, line, self.origin_column.kind())
                .map(|label| SharedString::from(label.to_owned())),
            hover: hover_text(origin).into(),
        })
    }

    /// Give the result pane's gutter the origins of the current result, while the column
    /// shows.
    fn refresh_result_labels(&self, cx: &mut Context<Self>) {
        let doc = self.session.document();
        let origins = self.origin_column.origins();
        let lines = (0..doc.line_count()).map(|line| origins.result_line(doc, Line::new(line)));
        let labels = self.origin_column.labels(lines, cx.theme().muted_foreground);
        self.result.update(cx, |state, cx| state.set_line_labels(labels, cx));
    }

    /// Whether the panes show the origin column: it is turned on and the origins have
    /// loaded. A window without origins draws no empty column (spec §7.3).
    #[must_use]
    pub fn origin_column(&self) -> bool {
        self.origin_column.shown()
    }

    fn toggle_origin_column(
        &mut self,
        _: &ToggleOriginColumn,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.origin_column.toggle();
        self.refresh_result_labels(cx);
        cx.notify();
    }

    /// The byte ranges of the result's region bands, as the editor holds them after moving them
    /// with each edit.
    #[must_use]
    pub fn result_band_ranges(&self, cx: &App) -> Vec<Range<usize>> {
        self.regions.get_ranges(cx)
    }

    #[must_use]
    pub fn document(&self) -> &MergeDocument {
        self.session.document()
    }

    /// Where Save & Exit writes.
    #[must_use]
    pub fn output(&self) -> Option<&Path> {
        self.session.output()
    }

    #[must_use]
    pub fn result_editor(&self) -> &Entity<EditorState> {
        &self.result
    }

    #[must_use]
    pub fn status_text(&self) -> &SharedString {
        &self.status
    }

    /// The text and highlights top pane `pane` draws on row `row`; none for a cell without
    /// a line.
    pub fn cell_highlights(
        &mut self,
        pane: usize,
        row: RowIx,
        cx: &App,
    ) -> Option<(SharedString, CellHighlights)> {
        let cells = self.rows.get_row(self.session.document(), row)?;
        let line = (*cells.cells.get(pane)?)?;
        let source = *self.panes.sources().get(pane)?;
        let kind = self.session.document().region(cells.region).kind;
        let changed = changed_colour(self.tones(cx), kind, source);
        let text = self.session.document().inputs().text(source);
        let syntax = self.colouring.line(&self.syntax, source, line, &cx.theme().highlight_theme);
        let matched: Vec<_> = self.input_find.in_cell(row, pane).collect();
        let fills = self.match_fills(pane, cx);
        let spans = self.highlights.row(self.session.document(), &self.rows, row).get(pane)?;
        let tab_width = self.typography.tab_width;
        let selected = self.input_select.in_line(self.session.document(), source, line);
        let paint = CellPaint { changed, fills, selected: cx.theme().selection, tab_width };
        Some(line_highlights(text.line(line), spans, &syntax, &matched, selected, paint))
    }

    /// The top panes' selection, in its input's bytes.
    #[must_use]
    pub fn input_selection(&self) -> Option<InputSelection> {
        self.input_select.selection()
    }

    fn press_input(
        &mut self,
        hit: &CellHit,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(self.input_select.focus(), cx);
        let inputs = input_select::Inputs {
            doc: self.session.document(),
            rows: &self.rows,
            tab_width: self.typography.tab_width,
        };
        self.input_select.press(inputs, hit, event);
        tracing::debug!(selection = ?self.input_select.selection(), "top pane press");
        cx.notify();
    }

    fn drag_input(&mut self, hit: &CellHit, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let inputs = input_select::Inputs {
            doc: self.session.document(),
            rows: &self.rows,
            tab_width: self.typography.tab_width,
        };
        if self.input_select.drag(inputs, hit, event) {
            cx.notify();
        }
    }

    /// ⌘C in the top panes: copy their selection; with nothing selected, copy nothing.
    fn copy_input(&mut self, _: &CopyInput, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_select.text(self.session.document());
        tracing::debug!(selection = ?self.input_select.selection(), bytes = text.map(str::len), "top pane copy");
        if let Some(text) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
    }

    /// ⌘A in the top panes: select the whole input of the selection's pane, else the first.
    fn select_all_input(&mut self, _: &SelectAllInput, _: &mut Window, cx: &mut Context<Self>) {
        let Some(&first) = self.panes.sources().first() else {
            return;
        };
        self.input_select.select_all(self.session.document(), first);
        cx.notify();
    }

    /// The panes' titles, in pane order.
    pub fn pane_titles(&self) -> impl Iterator<Item = &SharedString> {
        self.headings.iter().map(|heading| &heading.title)
    }

    /// The result pane's title.
    #[must_use]
    pub fn result_title(&self) -> &SharedString {
        &self.result_heading.title
    }

    /// Whether the result editor colours a chunk at a time, past `syntax-whole-file-max-lines`.
    #[must_use]
    pub fn result_chunked(&self) -> bool {
        self.result_highlighter == Highlighter::Chunked
    }

    /// What a measured run reported, once it has ended.
    #[must_use]
    pub fn measured(&self) -> Option<&Report> {
        self.measurement.report()
    }

    /// Take one step of a measured run: on the first frame, report when it shows; then act
    /// once a frame; once the frame after the last action draws, report and finish with 2,
    /// writing nothing.
    fn drive_measure(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.measurement.running() else {
            return;
        };
        if run.mode() == Mode::Syntax && !run.first() && self.syntax.pending() > 0 {
            // The next parse to land draws again.
            return;
        }
        if run.first() {
            let started = run.started();
            window.on_next_frame(move |_, _| {
                eprintln!(
                    "{}: {}",
                    strings::MEASURE,
                    measure::span("first frame", started.elapsed())
                );
            });
        }
        let mode = run.mode();
        if let Some(report) = self.measurement.frame(Instant::now()) {
            for line in report.to_string().lines() {
                eprintln!("{}: {line}", strings::MEASURE);
            }
            self.journal.cancel();
            self.session.end_journal();
            finish(ExitCode::Cancelled, cx);
            return;
        }
        if matches!(mode, Mode::Open | Mode::Syntax) {
            window.request_animation_frame();
            return;
        }
        let began = Instant::now();
        self.measure_step(mode, window, cx);
        if let Some(run) = self.measurement.running_mut() {
            run.worked(began.elapsed());
        }
        window.request_animation_frame();
    }

    /// One action of a measured run.
    fn measure_step(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        match mode {
            Mode::Open | Mode::Syntax => {}
            Mode::ScrollRows => {
                let top = self.scroll.top() + measure::SCROLL_STEP;
                self.scroll.set_top(f32::from(top));
            }
            Mode::ScrollResult => self.result.update(cx, |state, cx| {
                let offset = state.scroll_offset();
                state.set_scroll_offset(point(offset.x, offset.y - measure::SCROLL_STEP), cx);
            }),
            Mode::Type => {
                if self.measured_steps() == 0 {
                    let region = nav::middle_difference(self.session.document());
                    self.go_to(region, window, cx);
                }
                self.result.update(cx, |state, cx| {
                    state.replace_text_in_range(None, "x", window, cx);
                });
                // Now, so the action's time holds the core's work.
                self.send_edits(window, cx);
            }
            Mode::Pick => {
                let sides = [Source::Side(0), Source::Side(1)];
                let source = sides[self.measured_steps() % sides.len()];
                self.pick(
                    nav::middle_difference(self.session.document()),
                    Some(source),
                    window,
                    cx,
                );
            }
        }
    }

    /// The actions a measured run has taken so far.
    fn measured_steps(&self) -> usize {
        self.measurement.running().map_or(0, Run::actions)
    }

    /// How far the rows are scrolled down.
    #[must_use]
    pub fn rows_scroll_top(&self) -> Pixels {
        self.scroll.top()
    }

    /// The height of the rows' view.
    #[must_use]
    pub fn rows_view_height(&self) -> Pixels {
        self.scroll.view_height()
    }

    /// Scroll the rows and the result each to its own lines of the region `go_to` set aside:
    /// centred when they fit, else their first line at the top (spec §5). Runs in prepaint,
    /// after the rows lay out and before the result does, so the rows' height is known and
    /// the result applies its top this frame. Before the result's first layout it waits a
    /// frame. Returns whether it placed the panes, which then need no sync this frame.
    fn apply_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(region) = self.scroll.revealing(self.session.document().regions().len()) else {
            return false;
        };
        let (line_h, result_view, result_span) = {
            let state = self.result.read(cx);
            let Some(line_h) = state.line_height() else {
                window.refresh();
                return false;
            };
            let result = ResultRows::new(self.session.document(), &self.placeholders);
            let span = result_heights(result, self.scroll.wrap(), state).span(result.span(region));
            (line_h, state.input_bounds().size.height, span)
        };
        let first = self.rows.first_row(region).get();
        let rows_top = scroll::reveal_top(
            self.scroll.heights().span(first..first + self.rows.rows_in(region)),
            f32::from(self.typography.line_height()),
            f32::from(self.rows_view_height()),
        );
        let result_top = scroll::reveal_top(result_span, f32::from(line_h), f32::from(result_view));
        // The caret sits at the start of a line, so the result goes back to the left edge, as
        // the caret's own scroll would take it.
        self.result.update(cx, |state, cx| {
            state.set_scroll_offset(point(Pixels::ZERO, -px(result_top)), cx);
        });
        self.scroll.revealed(rows_top, result_top, &self.panes.sources());
        // The rows drew this frame already.
        window.refresh();
        true
    }

    /// Copy the scroll position from the pane that moved since the last sync to the other,
    /// mapped by region (spec §5). Runs in prepaint on every frame, so it sees every source
    /// of scrolling: wheel, scrollbar and caret.
    fn sync_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.scroll.syncing() {
            return;
        }
        let state = self.result.read(cx);
        let Some(line_h) = state.line_height() else {
            return;
        };
        let geometry = Geometry {
            row_h: f32::from(self.typography.line_height()),
            rows_view: f32::from(self.rows_view_height()),
            line_h: f32::from(line_h),
            result_view: f32::from(state.input_bounds().size.height),
        };
        let result_offset = state.scroll_offset();
        let (rows, result) =
            (&self.rows, ResultRows::new(self.session.document(), &self.placeholders));
        let sources = self.panes.sources();
        let wrap = self.scroll.wrap();
        // Built only when a pane moved: a wrapped result reads every line's display rows.
        let mapped = |map: fn(&RowMap, ResultRows, Displayed, Geometry, f32) -> f32| {
            move |row_heights: &Heights, top| {
                let displayed =
                    Displayed { rows: row_heights, result: &result_heights(result, wrap, state) };
                map(rows, result, displayed, geometry, top)
            }
        };
        let synced = self.scroll.sync(
            f32::from(-result_offset.y),
            f32::from(-result_offset.x),
            mapped(scroll::rows_to_result),
            mapped(scroll::result_to_rows),
            &sources,
        );
        if synced.result_left.is_some() || synced.result_top.is_some() {
            let offset = point(
                synced.result_left.map_or(result_offset.x, |left| -px(left)),
                synced.result_top.map_or(result_offset.y, |top| -px(top)),
            );
            self.result.update(cx, |state, cx| state.set_scroll_offset(offset, cx));
        }
        if synced.rows_moved {
            // The rows drew this frame already.
            window.refresh();
        }
    }

    /// Measure the top panes' columns, wrap their rows to the widths they laid out at while
    /// soft wrap is on, and hold their sideways scroll within those widths. Runs in prepaint,
    /// after the rows lay out.
    fn fit_rows(&mut self, window: &mut Window, cx: &App) {
        let family =
            self.typography.font.clone().unwrap_or_else(|| cx.theme().mono_font_family.clone());
        let font = gpui_kit::font(family);
        self.scroll.sideways_mut().measure(&font, self.typography.size, window);
        let old = self.scroll.heights().clone();
        let changed = if self.scroll.wrap().wraps() {
            self.wrap_rows(font, cx)
        } else {
            self.scroll.row_wrap_mut().unwrap(self.rows.row_count())
        };
        if changed {
            let result_top = f32::from(-self.result.read(cx).scroll_offset().y);
            self.scroll.keep_place(&old, f32::from(self.typography.line_height()), result_top);
        }
        let left = self.scroll.sideways().left();
        if self.scroll.scroll_sideways_to(left, &self.panes.sources()) || changed {
            window.refresh();
        }
    }

    /// Wrap the top panes' rows to the widths their text laid out at in the last frame.
    /// Whether their heights changed.
    fn wrap_rows(&mut self, font: gpui_kit::Font, cx: &App) -> bool {
        let panes = self.panes.sources().len();
        let Some(widths) = self.scroll.sideways().views().get(..panes).map(<[Pixels]>::to_vec)
        else {
            // Not laid out yet.
            return false;
        };
        let fit = Fit { widths, panes: self.panes, rows: self.rows.row_count() };
        let (doc, rows, sources) = (self.session.document(), &self.rows, self.panes.sources());
        let tab_width = self.typography.tab_width;
        let cells = |row: usize| {
            let cells = rows.row(doc, RowIx::new(row)).cells;
            sources
                .iter()
                .zip(cells)
                .map(|(&source, line)| {
                    line.map(|n| {
                        let text = doc.inputs().text(source).line(n);
                        tabs::expand(text.trim_end_matches(['\n', '\r']), tab_width).into_text()
                    })
                })
                .collect::<Vec<_>>()
        };
        let mut wrapper = cx.text_system().line_wrapper(font, self.typography.size);
        self.scroll.row_wrap_mut().fit(fit, cells, |text, width| {
            if width <= Pixels::ZERO {
                return Vec::new();
            }
            wrapper
                .wrap_line(&[LineFragment::text(text)], width, IndentAdjustment::NoIndent)
                .map(|boundary| boundary.ix)
                .collect()
        })
    }

    /// How far the top panes' text is scrolled right.
    #[must_use]
    pub fn rows_scroll_left(&self) -> Pixels {
        self.scroll.sideways().left()
    }

    /// The top panes' display rows: how many each aligned row takes.
    #[must_use]
    pub fn row_heights(&self) -> &Heights {
        self.scroll.heights()
    }

    /// Scroll the top panes' text sideways by a wheel's horizontal move, unless the move is
    /// mostly vertical: the list scrolls by that.
    fn wheel_sideways(&mut self, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.x.abs() <= delta.y.abs() {
            return;
        }
        let left = self.scroll.sideways().left() - delta.x;
        if self.scroll.scroll_sideways_to(left, &self.panes.sources()) {
            cx.notify();
        }
    }

    /// Turn soft wrap in the result and the top panes on or off (spec §5, §6.4). Inputs past
    /// [`SOFT_WRAP_MAX_LINES`] never wrap.
    fn toggle_soft_wrap(
        &mut self,
        _: &ToggleSoftWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wrap = self.scroll.toggle_wrap();
        if wrap == Wrap::TooLong {
            tracing::info!(max_lines = SOFT_WRAP_MAX_LINES, "the inputs are too long to wrap");
            return;
        }
        self.result.update(cx, |state, cx| state.set_soft_wrap(wrap.wraps(), window, cx));
        // The top panes wrap at the next prepaint.
        cx.notify();
    }

    /// Turn scroll sync off or on (spec §5). Turned on, it brings the top panes to the
    /// result, down and sideways.
    fn toggle_scroll_sync(
        &mut self,
        _: &ToggleScrollSync,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.scroll.toggle_sync();
        window.refresh();
    }

    /// Whether the go-to-line field shows.
    #[must_use]
    pub fn go_to_line_open(&self) -> bool {
        self.go_to_line.is_open()
    }

    /// The result editor's edits, caret and search, and the go-to-line field's Enter.
    fn subscribe(
        result: &Entity<EditorState>,
        go_to_line: &Entity<InputState>,
        input_focus: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        vec![
            // The top panes' selection ends when they lose the focus.
            cx.on_focus_out(input_focus, window, |this, _, _, cx| {
                tracing::debug!("top panes lost the focus; their selection ends");
                this.input_select.clear();
                cx.notify();
            }),
            cx.subscribe_in(result, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.send_edits(window, cx);
                }
            }),
            // The caret moves without a `Change` event, so the badge follows every update.
            cx.observe(result, |this, _, cx| {
                this.follow_caret(cx);
                this.follow_search(cx);
            }),
            cx.subscribe_in(go_to_line, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.confirm_go_to_line(window, cx);
                }
            }),
            cx.observe_window_activation(window, |this, _, cx| {
                // Another window may have changed the menu bar meanwhile.
                this.menu_bar.forget();
                cx.notify();
            }),
        ]
    }

    /// The View menu items this window ticks.
    fn view_ticks(&self) -> crate::ViewTicks {
        crate::ViewTicks {
            base_pane: self.panes == PaneSet::LeftBaseRight,
            origin_column: self.origin_column.shown(),
            origins_loaded: self.origin_column.origins().loaded(),
            syntax: self.colouring.shown(),
            soft_wrap: self.scroll.wrap().wraps(),
            scroll_sync: self.scroll.syncing(),
        }
    }

    /// Whether the highlight is blinking after a jump.
    #[must_use]
    pub fn flashing(&self) -> bool {
        self.flash.running()
    }

    /// Whether the highlight shows now: there is one, and the blink has not hidden it.
    #[must_use]
    pub fn highlight_shown(&self) -> bool {
        self.highlighted.is_some() && self.flash.shows(AtRest::Shown)
    }

    /// The highlighted region and its bytes in the result: the current region, unless it
    /// is unchanged.
    #[must_use]
    pub fn highlighted(&self) -> Option<(RegionIx, Range<usize>)> {
        self.highlighted.clone()
    }

    #[must_use]
    pub fn panes(&self) -> PaneSet {
        self.panes
    }

    /// The region picks act on: the pinned one, or the one holding the caret.
    #[must_use]
    pub fn current_region(&self, cx: &App) -> RegionIx {
        let caret = ByteOffset::new(self.result.read(cx).cursor());
        nav::held_region(self.session.document(), self.pinned, caret)
    }

    fn is_highlighted(&self, region: RegionIx) -> bool {
        self.highlighted.as_ref().is_some_and(|&(r, _)| r == region)
    }

    /// Highlight the region holding the caret and number its conflict, and redraw when
    /// either changes. While edits wait for the core, its regions lag the caret;
    /// [`Self::refresh`] runs once they land.
    fn follow_caret(&mut self, cx: &mut Context<Self>) {
        if self.recorded.pending() {
            return;
        }
        let region = self.current_region(cx);
        let number = nav::conflict_number(self.session.document(), region);
        let highlighted = (self.session.document().region(region).kind != Kind::Unchanged)
            .then(|| (region, self.session.document().region_bytes(region)));
        if highlighted != self.highlighted {
            self.highlighted = highlighted;
            self.paint_highlight(cx);
        }
        if number != self.conflict_number {
            self.conflict_number = number;
            cx.notify();
        }
    }

    /// Frame the highlighted region in the result, unless the blink hides it.
    fn paint_highlight(&mut self, cx: &mut Context<Self>) {
        let frame = self
            .highlighted
            .as_ref()
            .filter(|_| self.highlight_shown())
            .map(|(_, bytes)| blink::region_frame(bytes.clone(), self.tones(cx).current()));
        self.highlight.set(frame.into_iter().collect(), cx);
        cx.notify();
    }

    /// Blink the highlight: hidden and shown in turn, starting hidden and ending shown.
    fn start_flash(&mut self, cx: &mut Context<Self>) {
        Flash::start(self, (), |this| &mut this.flash, Self::paint_highlight, cx);
    }

    /// Blink the match a search step reached: shown and hidden in turn, starting shown and
    /// ending hidden. An input pane's current match takes the frame's colour while it shows;
    /// in the result, a fill in that colour shows under the search's own.
    fn start_found(&mut self, found: Found, cx: &mut Context<Self>) {
        Flash::start(self, found, |this| &mut this.found, Self::paint_found, cx);
    }

    fn paint_found(&mut self, cx: &mut Context<Self>) {
        let fill = match self.found.lit() {
            Some(Found::Result(start, end)) => {
                Some(blink::match_flash(start..end, self.tones(cx).current()))
            }
            Some(Found::Input(_)) | None => None,
        };
        self.found_flash.set(fill.into_iter().collect(), cx);
        cx.notify();
    }

    /// The fills of top pane `pane`'s matches: its current match takes the frame's colour
    /// while its blink shows.
    fn match_fills(&self, pane: usize, cx: &App) -> MatchFills {
        let fills = MatchFills::of(cx.theme().selection);
        if self.found.lit() == Some(Found::Input(pane)) {
            MatchFills { current: self.tones(cx).current(), ..fills }
        } else {
            fills
        }
    }

    /// The byte ranges the result fills now after a search step.
    #[must_use]
    pub fn find_flashes(&self, cx: &App) -> Vec<Range<usize>> {
        self.found_flash.get_ranges(cx)
    }

    /// Send the user's edits to the core, one batch per action. Each keystroke normally
    /// arrives alone. When several wait, the later ones are in the editor's offsets, which
    /// the core's corrections to the earlier ones would shift, so they go as one patch.
    fn send_edits(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let doc = self.session.document();
        let Some(batch) = editor_sync::take_batch(&self.result, &self.recorded, || doc.text(), cx)
        else {
            return;
        };
        self.pinned = None;
        match self.apply(Command::Edit(batch), cx) {
            Ok(applied) => {
                self.apply_patches(&applied.patches, Caret::Follow, window, cx);
                self.follow(&applied, cx);
                self.refresh(cx);
            }
            Err(error) => {
                log_refused(&error, "an edit; reloading the result");
                self.reload_result(window, cx);
            }
        }
    }

    /// Run a command that is not an edit, after any edits still waiting.
    fn run(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.send_edits(window, cx);
        match self.apply(command, cx) {
            Ok(applied) => {
                self.apply_patches(&applied.patches, Caret::AtChange, window, cx);
                self.follow(&applied, cx);
                self.refresh(cx);
            }
            Err(error) => log_refused(&error, "a command"),
        }
    }

    fn apply_patches(
        &self,
        patches: &[TextPatch],
        caret: Caret,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        editor_sync::apply_patches(&self.result, &self.recorded, patches, caret, window, cx);
    }

    /// Apply `command` to the session, which journals it, and write the journal once
    /// commands pause.
    fn apply(&mut self, command: Command, cx: &mut Context<Self>) -> Result<Applied, Refused> {
        let applied = self.session.apply(command)?;
        self.schedule_journal(cx);
        Ok(applied)
    }

    /// Write the journal once commands pause for [`JOURNAL_DEBOUNCE`].
    fn schedule_journal(&mut self, cx: &mut Context<Self>) {
        if self.session.recovery().is_none() {
            return;
        }
        // Replacing the task drops the one waiting, which cancels it.
        self.journal.waiting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(JOURNAL_DEBOUNCE).await;
            ignore_closed_view(this.update(cx, Self::write_journal));
        }));
    }

    /// Start the next write on a background thread. While one runs, it starts the next when
    /// it ends.
    fn write_journal(&mut self, cx: &mut Context<Self>) {
        self.journal.waiting = None;
        if self.journal.running.is_some() {
            return;
        }
        let Some((write, upto)) = self.session.recovery().and_then(Recovery::next_write) else {
            return;
        };
        self.journal.running = Some(cx.spawn(async move |this, cx| {
            let mode = write.mode;
            let result = cx.background_spawn(async move { write.run() }).await;
            ignore_closed_view(this.update(cx, |this, cx| this.written(mode, upto, result, cx)));
        }));
    }

    /// Take the result of a write that leaves the file with `upto` commands.
    fn written(
        &mut self,
        mode: WriteMode,
        upto: usize,
        result: io::Result<Written>,
        cx: &mut Context<Self>,
    ) {
        if let Some(recovery) = self.session.recovery_mut()
            && self.journal.written(recovery, mode, upto, result)
        {
            self.write_journal(cx);
        }
    }

    /// Delete the journal's file on a background thread; then commands made since start a
    /// new one.
    fn discard_journal(&mut self, cx: &mut Context<Self>) {
        self.journal.cancel();
        let Some(removal) = self.session.recovery_mut().and_then(Recovery::discard) else {
            self.write_journal(cx);
            return;
        };
        self.journal.running = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { removal.run() }).await;
            ignore_closed_view(this.update(cx, |this, cx| this.removed(result, cx)));
        }));
    }

    /// Take the result of the journal's removal.
    fn removed(&mut self, result: io::Result<Written>, cx: &mut Context<Self>) {
        if let Some(recovery) = self.session.recovery_mut()
            && self.journal.removed(recovery, result)
        {
            self.write_journal(cx);
        }
    }

    /// Off the UI thread, delete old journals and hash the inputs; then name the journal and
    /// offer any restore.
    fn hash_inputs(doc: &MergeDocument, dir: &Path, window: &mut Window, cx: &mut Context<Self>) {
        // Texts share their content, so the copy is O(1).
        let inputs = doc.inputs().clone();
        let dir = dir.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let hashes = cx
                .background_spawn(async move {
                    prune(&dir, SystemTime::now());
                    input_hashes(&inputs)
                })
                .await;
            ignore_closed_view(
                this.update_in(cx, |this, window, cx| this.hashed(hashes, window, cx)),
            );
        })
        .detach();
    }

    fn hashed(&mut self, hashes: InputHashes, window: &mut Window, cx: &mut Context<Self>) {
        let start = self.session.document().start();
        let Some(recovery) = self.session.recovery_mut() else {
            return;
        };
        // A session that ended has hashed them itself.
        if !recovery.hashed(hashes, start) {
            return;
        }
        if !self.offer_restore(window, cx) {
            self.write_journal(cx);
        }
    }

    /// Offer the journal a crashed session left for these inputs. Returns whether it asked.
    fn offer_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(journal) = self.session.recovery().and_then(Recovery::saved) else {
            return false;
        };
        let answer = window.prompt(
            PromptLevel::Info,
            prompt::RESTORE_SESSION,
            Some(prompt::RESTORE_DETAIL),
            &[prompt::RESTORE, prompt::START_OVER],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let restore = answer.await == Ok(GO_AHEAD);
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                if restore {
                    this.restore(journal, window, cx);
                } else {
                    this.discard_journal(cx);
                }
            }));
        })
        .detach();
        true
    }

    /// Replace the session with the journal's replay.
    fn restore(&mut self, journal: Journal, window: &mut Window, cx: &mut Context<Self>) {
        // Texts share their content and line index, so the copy is O(1).
        let inputs = self.session.document().inputs().clone();
        let boundaries = self.colouring.detected().map(SyntaxBoundaries::new);
        let restored = journal
            .restore_bounded(
                inputs,
                self.session.document().labels().clone(),
                boundaries.as_ref().map(|b| b as &dyn Boundaries),
            )
            .map_err(RebuildError::from)
            .and_then(|doc| with_rows(doc, self.panes));
        let (doc, rows) = match restored {
            Ok(restored) => restored,
            Err(error) => {
                tracing::warn!(%error, "cannot restore the journal");
                let detail = error.to_string();
                let answer = window.prompt(
                    PromptLevel::Warning,
                    prompt::CANNOT_RESTORE,
                    Some(&detail),
                    &[prompt::OK],
                    cx,
                );
                cx.spawn(async move |_, _| {
                    // The alert's one button only closes it.
                    let _ = answer.await;
                })
                .detach();
                // It would fail the same way on the next open.
                self.discard_journal(cx);
                return;
            }
        };
        if let Err(refused) = self.session.accepts() {
            // The save ends the session with the merge as it stood, or fails and goes on.
            tracing::info!(%refused, "the restore came during a save and is dropped");
            return;
        }
        self.journal.cancel();
        if let Err(refused) = self.session.restore(doc, journal) {
            tracing::warn!(%refused, "the session refused the restore");
            return;
        }
        self.show_restored(rows, window, cx);
        self.open_on_first_target(window, cx);
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

    /// Show the session's restored document, laid out in `rows`.
    fn show_restored(&mut self, rows: RowMap, window: &mut Window, cx: &mut Context<Self>) {
        self.rows = rows;
        self.scroll.row_wrap_mut().rows_changed(self.rows.row_count());
        self.highlights = Highlights::new(self.panes);
        self.pinned = None;
        self.reload_result(window, cx);
        self.search_inputs_again(cx);
    }

    fn follow(&mut self, applied: &Applied, cx: &mut Context<Self>) {
        self.rows.update(self.session.document(), applied);
        self.highlights.update(applied);
        if applied.regions_changed {
            self.scroll.row_wrap_mut().rows_changed(self.rows.row_count());
            self.search_inputs_again(cx);
        }
    }
    fn reload_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.session.document().text();
        self.recorded.muted(|| {
            self.result.update(cx, |state, cx| {
                let caret = state.cursor().min(text.len());
                state.set_value(text, window, cx);
                state.set_selected_range(caret..caret, cx);
            });
        });
        self.refresh(cx);
    }

    /// Redraw the region bands, marks and status from the core.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.copy_note = None;
        let tones = *self.tones(cx);
        let bands = marks::bands(self.session.document());
        let (shown, shown_tones) = &self.shown_bands;
        let moved = tones == *shown_tones
            && marks::bands_unchanged(shown, &self.regions.get_ranges(cx), &bands);
        if !moved {
            self.regions.set(region_decorations(&bands, &tones), cx);
        }
        self.shown_bands = (bands, tones);
        let (spacers, notes, placeholders) = result_placeholders(self.session.document(), &tones);
        self.placeholders = placeholders;
        self.result.update(cx, |state, cx| {
            state.set_spacers(spacers, cx);
            state.set_spacer_notes(notes, cx);
        });
        self.refresh_result_labels(cx);
        self.counts = status::status(self.session.document(), self.session.line_ending());
        self.ruler = merge_view::ruler::marks(self.session.document()).collect();
        self.follow_caret(cx);
        self.status = strings::status(self.repo, &self.counts).into();
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.pinned = None;
        self.run(Command::Undo, window, cx);
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.pinned = None;
        self.run(Command::Redo, window, cx);
    }

    /// Toggle `source` in `region`'s pick, or clear the pick with `None`. The region stays
    /// current.
    fn pick(
        &mut self,
        region: RegionIx,
        source: Option<Source>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match source {
            Some(source) => Command::Toggle { region, source },
            None => Command::Clear { region },
        };
        self.run(command, window, cx);
        self.go_to(region, window, cx);
    }

    fn pick_current(
        &mut self,
        source: Option<Source>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_edits(window, cx);
        let region = self.current_region(cx);
        self.pick(region, source, window, cx);
    }

    /// Set the current region's pick to `choice`. The region stays current.
    fn take(&mut self, choice: Choice, window: &mut Window, cx: &mut Context<Self>) {
        self.send_edits(window, cx);
        let region = self.current_region(cx);
        let sources = choice.sources().to_vec();
        self.run(Command::Pick { region, sources }, window, cx);
        self.go_to(region, window, cx);
    }

    /// Pick `source` in every region `scope` names, as one undo step (spec §4.4). The caret
    /// stays where it is.
    fn take_all(
        &mut self,
        scope: PickScope,
        source: Source,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_edits(window, cx);
        let sources = vec![source];
        self.run(Command::PickAll { scope, sources }, window, cx);
    }

    /// Take the current region's suggestion `wanted` as an edit, when the region offers it
    /// (spec §4.4). The regions it changes become `Edited` in one undo step, as if the user had
    /// typed the text.
    fn take_suggestion(
        &mut self,
        wanted: EditSuggestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_edits(window, cx);
        let region = self.current_region(cx);
        if let Some(batch) = suggest::edit(self.session.document(), region, wanted) {
            self.edit_regions(batch, region, window, cx);
        }
    }

    /// Apply `batch` to the result as one edit, then return to `region`.
    fn edit_regions(
        &mut self,
        batch: EditBatch,
        region: RegionIx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(refused) = self.session.accepts() {
            log_refused(&refused, "a suggested edit");
            return;
        }
        // The editor takes the text first, as when the user types; the core then records it.
        self.apply_patches(&batch.0, Caret::AtChange, window, cx);
        self.pinned = None;
        match self.apply(Command::Edit(batch), cx) {
            Ok(applied) => {
                self.apply_patches(&applied.patches, Caret::Follow, window, cx);
                self.follow(&applied, cx);
                self.refresh(cx);
            }
            Err(error) => {
                log_refused(&error, "a suggested edit; reloading the result");
                self.reload_result(window, cx);
            }
        }
        self.go_to(region, window, cx);
    }

    /// Bind the Merge menu's commands on `root`.
    fn on_merge_actions(root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|this, _: &ToggleLeft, window, cx| {
            this.pick_current(Some(Source::Side(0)), window, cx);
        }))
        .on_action(cx.listener(|this, _: &ToggleBase, window, cx| {
            this.pick_current(Some(Source::Base(0)), window, cx);
        }))
        .on_action(cx.listener(|this, _: &ToggleRight, window, cx| {
            this.pick_current(Some(Source::Side(1)), window, cx);
        }))
        .on_action(
            cx.listener(|this, _: &ClearChoice, window, cx| this.pick_current(None, window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &TakeBase, window, cx| this.take(Choice::Base, window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &TakeLeft, window, cx| this.take(Choice::Left, window, cx)),
        )
        .on_action(
            cx.listener(|this, _: &TakeRight, window, cx| this.take(Choice::Right, window, cx)),
        )
        .on_action(cx.listener(|this, _: &TakeLeftThenRight, window, cx| {
            this.take(Choice::LeftThenRight, window, cx);
        }))
        .on_action(cx.listener(|this, _: &TakeRightThenLeft, window, cx| {
            this.take(Choice::RightThenLeft, window, cx);
        }))
        .on_action(cx.listener(|this, _: &TakeLeftAllUnresolved, window, cx| {
            this.take_all(PickScope::Unresolved, Source::Side(LEFT), window, cx);
        }))
        .on_action(cx.listener(|this, _: &TakeRightAllUnresolved, window, cx| {
            this.take_all(PickScope::Unresolved, Source::Side(RIGHT), window, cx);
        }))
        .on_action(cx.listener(|this, _: &TakeWholeLeft, window, cx| {
            this.take_all(PickScope::All, Source::Side(LEFT), window, cx);
        }))
        .on_action(cx.listener(|this, _: &TakeWholeRight, window, cx| {
            this.take_all(PickScope::All, Source::Side(RIGHT), window, cx);
        }))
        .on_action(cx.listener(|this, _: &TakeSortedUnion, window, cx| {
            this.take_suggestion(EditSuggestion::SortedUnion, window, cx);
        }))
        .on_action(cx.listener(|this, _: &CarryMovedEdit, window, cx| {
            this.take_suggestion(EditSuggestion::CarryEdit, window, cx);
        }))
    }

    /// The right-click menu's choices for the current region.
    fn menu_choices(&self, cx: &App) -> Vec<MenuChoice> {
        let region = self.current_region(cx);
        let line_of = |to: RegionIx| {
            self.session.document().line_of_byte(self.session.document().region_start(to)).get() + 1
        };
        let suggested: Vec<MenuAction> = suggest::suggestions(
            self.session.document().inputs(),
            self.session.document().regions(),
            region,
        )
        .iter()
        .map(|suggestion| MenuAction::suggested(suggestion, line_of))
        .collect();
        choice::menu_choices(Choice::of(self.session.document().resolution(region)), &suggested)
    }

    /// Put the caret at the start of `region` and bring its rows into view.
    fn go_to(&mut self, region: RegionIx, window: &mut Window, cx: &mut Context<Self>) {
        let region = region.min(RegionIx::new(self.session.document().regions().len() - 1));
        let caret = self.session.document().region_start(region);
        self.result.update(cx, |state, cx| {
            let position = state.text().offset_to_position(caret.get());
            state.set_cursor_position(position, window, cx);
        });
        self.pinned = Some(Pin { region, caret });
        self.scroll.reveal(region);
        cx.notify();
    }

    /// Put the caret on the first unresolved conflict, else the first difference, so the
    /// choice keys act at once (spec §6.3). With neither it stays at the start.
    fn open_on_first_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let first = nav::first(self.session.document(), Target::Unresolved)
            .or_else(|| nav::first(self.session.document(), Target::Difference));
        if let Some(region) = first {
            self.go_to(region, window, cx);
        }
    }

    fn navigate(
        &mut self,
        step: fn(&MergeDocument, RegionIx, Target) -> Option<RegionIx>,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_edits(window, cx);
        if let Some(region) = step(self.session.document(), self.current_region(cx), target) {
            self.go_to(region, window, cx);
            self.start_flash(cx);
        }
    }

    /// Write the output and finish: with 0 when resolved, or with 1 after the user agrees
    /// to save with conflict markers (spec §6.2).
    fn save_and_exit(&mut self, _: &SaveAndExit, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.saving() {
            self.send_edits(window, cx);
        }
        self.step(self.session.save_requested(), window, cx);
    }

    /// Carry out the next step of ending the session (spec §6.2).
    fn step(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        match next {
            Next::Stay => {}
            Next::ConfirmUnresolved { unresolved } => {
                self.confirm_unresolved(unresolved, window, cx);
            }
            Next::ConfirmClose => Self::confirm_close(window, cx),
            Next::SaveAndExit => self.save_and_exit(&SaveAndExit, window, cx),
            Next::Write => self.write_and_finish(window, cx),
            Next::Record(code) => self.record_again(code, window, cx),
            Next::SaveCopyAs => self.save_copy_as(&SaveCopyAs, window, cx),
            Next::Failed { undone, detail } => {
                // The save wrote the journal first; a failed write tries again.
                self.write_journal(cx);
                Self::save_failed(undone, detail, window, cx);
            }
            Next::Finish { code, journal } => {
                self.journal.cancel();
                self.session.finish_journal(journal);
                self.exit(code, window, cx);
            }
        }
    }

    /// Ask before writing markers for `unresolved` conflicts.
    fn confirm_unresolved(&self, unresolved: usize, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            prompt::SAVE_UNRESOLVED,
            Some(&prompt::save_unresolved_detail(
                unresolved,
                self.session.document().options().semantics,
            )),
            &[prompt::SAVE_WITH_CONFLICTS, prompt::KEEP_EDITING],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let next = ending::unresolved_answered(answer.await == Ok(GO_AHEAD));
            ignore_closed_view(this.update_in(cx, |this, window, cx| this.step(next, window, cx)));
        })
        .detach();
    }

    /// Ask for a path, starting in OUTPUT's directory, and write a copy there.
    fn save_copy_as(&mut self, _: &SaveCopyAs, window: &mut Window, cx: &mut Context<Self>) {
        self.send_edits(window, cx);
        let start = self
            .session
            .output()
            .and_then(Path::parent)
            .map_or_else(PathBuf::new, Path::to_path_buf);
        let picked = cx.prompt_for_new_path(&start, None);
        cx.spawn_in(window, async move |this, cx| {
            // A cancelled or failed picker writes nothing.
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.save_copy(&path, window, cx);
            }));
        })
        .detach();
    }

    /// Write the output to `path` as Save & Exit would, unresolved conflicts as markers,
    /// without a confirmation (`docs/DESIGN.md` §E6), off the UI thread. The window stays open;
    /// OUTPUT, the exit code and the journal are unchanged. A failure shows an alert.
    pub fn save_copy(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.send_edits(window, cx);
        let (encoded, unresolved) = self.session.copy();
        let path = path.to_owned();
        let before = self.copying.take();
        self.copying = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(before) = before {
                before.await;
            }
            let written = cx.background_spawn(async move { encoded.write(&path) }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.copy_written(written, unresolved, window, cx);
            }));
        }));
    }

    fn copy_written(
        &mut self,
        written: Result<ExitCode, WriteError>,
        unresolved: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match written {
            Ok(_) => {
                self.copy_note = Some(strings::copy_saved(unresolved).into());
                cx.notify();
            }
            Err(error) => {
                let detail = error.to_string();
                tracing::error!(detail, "cannot write the copy");
                let answer = window.prompt(
                    PromptLevel::Critical,
                    prompt::CANNOT_WRITE_COPY,
                    Some(&detail),
                    &[prompt::OK],
                    cx,
                );
                cx.spawn(async move |_, _| {
                    // The alert's one button only closes it; the window stays as it was.
                    let _ = answer.await;
                })
                .detach();
            }
        }
    }

    /// Finish with `code` once the copies being written have landed.
    fn exit(&mut self, code: ExitCode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(copying) = self.copying.take() else {
            finish(code, cx);
            return;
        };
        cx.spawn_in(window, async move |_, cx| {
            copying.await;
            ignore_closed_view(cx.update(|_, cx| finish(code, cx)));
        })
        .detach();
    }

    /// Write the output and record the save off the UI thread, then finish.
    fn write_and_finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The save writes the journal first; its tasks stop.
        self.journal.cancel();
        match self.session.start_save() {
            Ok(job) => self.save_in_background(job, window, cx),
            Err(CannotSave::NoOutput) => Self::fail(prompt::NO_OUTPUT, window, cx),
            Err(CannotSave::Saving) => {}
        }
    }

    /// Record again, off the UI thread, a save whose output is written with `code`, without
    /// writing it again.
    fn record_again(&mut self, code: ExitCode, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(job) = self.session.start_record(code) {
            self.save_in_background(job, window, cx);
        }
    }

    /// Run `job` on a background thread with the result locked, then end the session or
    /// say why it failed.
    fn save_in_background(&mut self, job: SaveJob, window: &mut Window, cx: &mut Context<Self>) {
        self.set_saving(true, cx);
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            let saved = cx.background_spawn(async move { job.run() }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.save_task = None;
                this.set_saving(false, cx);
                let next = this.session.saved(saved);
                this.step(next, window, cx);
            }));
        }));
    }

    /// Lock the result while a save writes it, and unlock it after.
    fn set_saving(&self, saving: bool, cx: &mut Context<Self>) {
        self.result.update(cx, |state, cx| state.set_readonly(saving, cx));
    }

    /// Say why a save failed, and offer the answers `undone` allows: try again, save a copy,
    /// keep editing, or quit with 3 (spec §6.2). The window stays open until the user saves
    /// or quits; the journal holds the work meanwhile.
    fn save_failed(undone: Undone, detail: String, window: &mut Window, cx: &mut Context<Self>) {
        tracing::error!(detail, "cannot save");
        let answers = undone.answers();
        let labels: Vec<&str> = answers.iter().map(|&answer| failed_label(answer)).collect();
        let answer =
            window.prompt(PromptLevel::Critical, undone_title(undone), Some(&detail), &labels, cx);
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let answer = answer.await.ok().and_then(|answer| answers.get(answer).copied());
            let next = ending::failure_answered(answer, undone);
            // Quit, or an alert that closed with its window, ends the session; so does a
            // window gone before the answer could reach it.
            let handled = !matches!(next, Next::Finish { .. })
                && handle
                    .update(cx, |_, window, cx| {
                        this.update(cx, |this, cx| this.step(next, window, cx))
                    })
                    .is_ok_and(|updated| updated.is_ok());
            if !handled {
                cx.update(|cx| {
                    eprintln!("{}: {detail}", strings::PROGRAM);
                    finish(ExitCode::Error, cx);
                });
            }
        })
        .detach();
    }

    /// Report an error on stderr and in an alert, then finish with 3.
    fn fail(detail: &str, window: &mut Window, cx: &mut Context<Self>) {
        tracing::error!(detail, "cannot save");
        eprintln!("{}: {detail}", strings::PROGRAM);
        let answer = window.prompt(
            PromptLevel::Critical,
            prompt::CANNOT_WRITE,
            Some(detail),
            &[prompt::QUIT],
            cx,
        );
        cx.spawn(async move |_, cx| {
            // Quit, or the alert closing with the window, finishes alike.
            let _ = answer.await;
            cx.update(|cx| finish(ExitCode::Error, cx));
        })
        .detach();
    }

    /// Close the window (spec §6.2). With nothing changed, finish with 2 at once. After a
    /// change, ask: Save & Exit, Discard (finish with 2, writing nothing) or Keep Editing.
    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.saving() {
            self.send_edits(window, cx);
        }
        self.step(self.session.close_requested(), window, cx);
    }

    /// Ask whether to save, discard or keep editing.
    fn confirm_close(window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            prompt::SAVE_CHANGES,
            Some(prompt::DISCARD_DETAIL),
            &Closing::ALL.map(closing_label),
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = answer.await.ok().and_then(|answer| Closing::ALL.get(answer).copied());
            let next = ending::close_answered(choice);
            ignore_closed_view(this.update_in(cx, |this, window, cx| this.step(next, window, cx)));
        })
        .detach();
    }

    /// Open the result's search, from anywhere in the window.
    fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        self.result.update(cx, |state, cx| {
            state.focus(window, cx);
            state.open_search(false, cx);
        });
    }

    /// Follow the result's search into the input panes: search them when its query or case
    /// changes, and drop their matches when it closes or empties.
    fn follow_search(&mut self, cx: &mut Context<Self>) {
        let session = self.result.read(cx).search_session();
        if let Some(bytes) = self.result_steps.stepped(session) {
            self.start_found(Found::Result(bytes.start, bytes.end), cx);
        }
        let session = self.result.read(cx).search_session();
        let wanted = (session.is_active() && !session.query.is_empty()).then(|| {
            let case = if session.case_insensitive { Case::Insensitive } else { Case::Sensitive };
            (session.query.clone(), case)
        });
        match self.input_find.follow(wanted) {
            input_find::Follow::Unchanged => {}
            input_find::Follow::Search => self.search_inputs(Keep::No, cx),
            input_find::Follow::Cleared => cx.notify(),
        }
    }

    /// Search the input panes again for the same query after the rows or panes changed,
    /// keeping each pane's current match.
    fn search_inputs_again(&mut self, cx: &mut Context<Self>) {
        if self.input_find.query().is_some() {
            self.search_inputs(Keep::Yes, cx);
        }
    }

    /// Search the visible inputs off the UI thread (`docs/DESIGN.md` §A14 measures why). With
    /// `keep`, each pane's current match stays if the search still finds it; otherwise a
    /// pane's first match from the top row is current.
    fn search_inputs(&mut self, keep: Keep, cx: &mut Context<Self>) {
        // The search runs on another thread; the query is short.
        let Some((query, case)) = self.input_find.query().cloned() else {
            return;
        };
        let search = self.input_find.begin();
        let keep = match keep {
            Keep::Yes => self.input_find.starts(),
            Keep::No => Vec::new(),
        };
        let inputs = self.session.document().inputs();
        let sources = self.panes.sources();
        // `Text` clones share their buffer.
        let texts: Vec<(Source, Text)> =
            sources.iter().map(|&source| (source, inputs.text(source).clone())).collect();
        // A copy of the line ranges, one per region, so the rows can be found off the UI
        // thread.
        let regions = self.session.document().regions().to_vec();
        let rows = self.rows.clone();
        let from = self.top_row();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    InputMatches::find_in(texts.iter().map(|(s, t)| (*s, t)), &query, case)
                        .map(|matches| matches.ordered(&regions, &rows))
                })
                .await;
            ignore_closed_view(this.update(cx, |this, cx| {
                let hits = found
                    .inspect_err(|error| tracing::warn!(%error, "the input search found nothing"))
                    .unwrap_or_default();
                this.input_find.land(search, hits, &sources, &keep, from);
                cx.notify();
            }));
        })
        .detach();
    }

    /// Step pane `pane` to its next or previous match and scroll that row to the middle.
    fn step_input_match(&mut self, pane: usize, step: Step, cx: &mut Context<Self>) {
        if let Some(row) = self.input_find.step(pane, step).map(|hit| hit.row) {
            self.scroll.centre(row);
            self.start_found(Found::Input(pane), cx);
        }
        cx.notify();
    }

    /// Each input pane's search controls, while the result's search runs.
    fn input_controls(&self, cx: &mut Context<Self>) -> Vec<Option<AnyElement>> {
        (0..self.headings.len())
            .map(|pane| {
                let title = &self.headings[pane].title;
                let previous = cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.step_input_match(pane, Step::Previous, cx);
                });
                let next = cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.step_input_match(pane, Step::Next, cx);
                });
                input_find::controls(&self.input_find, pane, title, previous, next)
                    .map(IntoElement::into_any_element)
            })
            .collect()
    }

    /// The first row the input panes show.
    fn top_row(&self) -> RowIx {
        self.scroll.top_row(self.typography.line_height())
    }

    /// Input pane `pane`'s match count, `n of m` or `No matches`; `None` while the result's
    /// search is closed or empty.
    #[must_use]
    pub fn input_match_count(&self, pane: usize) -> Option<String> {
        self.input_find.count(pane)
    }

    /// Input pane `pane`'s current match.
    #[must_use]
    pub fn input_match_current(&self, pane: usize) -> Option<&Hit> {
        self.input_find.current(pane)
    }

    /// Open the go-to-line field, empty.
    fn open_go_to_line(&mut self, _: &GoToLine, window: &mut Window, cx: &mut Context<Self>) {
        self.go_to_line.open(window, cx);
        cx.notify();
    }

    fn close_go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.go_to_line.close();
        self.result.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Move to the line the field names, or leave the field open when it names none.
    fn confirm_go_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send_edits(window, cx);
        let Some(line) = self.go_to_line.line(self.session.document().line_count(), cx) else {
            return;
        };
        self.close_go_to_line(window, cx);
        let at = self.session.document().byte_of_line(Line::new(line)).get();
        self.pinned = None;
        self.result.update(cx, |state, cx| {
            let position = state.text().offset_to_position(at);
            state.set_cursor_position(position, window, cx);
        });
        if let Some(row) = scroll::row_of_line(&self.rows, self.session.document(), Line::new(line))
        {
            self.scroll.centre(row);
        }
    }

    /// The result editor, with its overview ruler at the right.
    fn render_result(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let this = cx.entity().downgrade();
        h_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id(RESULT_ID)
                    .test_support()
                    .key_context(RESULT_CONTEXT)
                    .flex_1()
                    .h_full()
                    .child(
                        // The element sets the state's read-only flag each frame.
                        Editor::new(&self.result)
                            .aria_label(strings::RESULT_TITLE)
                            .readonly(self.session.saving())
                            .context_menu(move |menu, _, cx| {
                                let items = this
                                    .upgrade()
                                    .map(|view| view.read(cx).menu_choices(cx))
                                    .unwrap_or_default();
                                choice_menu(menu, &items)
                            })
                            .when_some(self.typography.font.clone(), gpui_kit::Styled::font_family)
                            .text_size(self.typography.size)
                            .line_height(gpui_kit::relative(self.typography.spacing))
                            .bordered(false)
                            .h(gpui_kit::relative(1.)),
                    ),
            )
            .child(self.render_ruler(cx))
    }

    /// The overview ruler beside the result, with the result's vertical scrollbar thumb.
    fn render_ruler(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let shown = ruler::Strip {
            marks: Rc::clone(&self.ruler),
            total: merge_view::ruler::total(self.session.document()),
            feed: self.ruler_feed.clone(),
        };
        let (this, result) = (cx.entity().downgrade(), self.result.downgrade());
        ruler::strip(
            RULER_ID,
            shown,
            self.ruler_drag.clone(),
            *self.tones(cx),
            cx.theme().foreground,
            move |click, window, cx| {
                ignore_closed_view(this.update(cx, |this, cx| this.ruler_click(click, window, cx)));
            },
            move |top, _, cx| {
                ignore_closed_view(result.update(cx, |state, cx| {
                    let offset = state.scroll_offset();
                    state.set_scroll_offset(point(offset.x, px(-top)), cx);
                }));
            },
        )
    }

    /// Go to the region a ruler mark stands for, or scroll the result to the line clicked.
    fn ruler_click(&mut self, click: ruler::Click, window: &mut Window, cx: &mut Context<Self>) {
        self.send_edits(window, cx);
        match click {
            ruler::Click::Region(region) => {
                self.go_to(region, window, cx);
                self.start_flash(cx);
            }
            ruler::Click::Line(line) => self.result.update(cx, |state, cx| {
                let Some(line_h) = state.line_height() else {
                    return;
                };
                let offset = state.scroll_offset();
                let view = f32::from(state.input_bounds().size.height);
                let top = merge_view::ruler::centred_top(line, f32::from(line_h), view);
                state.set_scroll_offset(point(offset.x, px(-top)), cx);
            }),
        }
    }

    /// The toolbar, with labels when `window` is wide enough for them.
    fn render_toolbar(&self, window: &Window, cx: &App) -> impl IntoElement + use<> {
        let (tones, dark) = (self.tones(cx), cx.theme().is_dark());
        let badge = strings::conflict_badge(&self.counts, self.conflict_number).map(|text| {
            let text = SharedString::from(text);
            div()
                .id(CONFLICT_BADGE_ID)
                .test_support()
                .role(Role::Status)
                .aria_label(text.clone())
                .px_2()
                .rounded_md()
                .bg(tones.conflict_badge(self.counts.unresolved == 0))
                .text_size(self.typography.toolbar_size)
                .child(text)
        });
        toolbar::bar(
            strings::TOOLBAR,
            &GROUPS,
            WINDOW_CONTEXT,
            dark,
            Labels::for_width(
                window.viewport_size().width,
                toolbar::labels_min_width(self.typography.toolbar_size, window),
            ),
            self.typography.toolbar_size,
        )
        .child(div().flex_1())
        .children(badge)
    }

    /// The input panes' headings, and the result's.
    /// The panes' headings, each input heading with its search controls from `controls`.
    fn render_headings(
        &self,
        theme: &Theme,
        controls: Vec<Option<AnyElement>>,
    ) -> (impl IntoElement + use<>, impl IntoElement + use<>) {
        let heading = |pane, h: &Heading, text, controls| {
            let accent = self.palette.tones(theme.is_dark()).mark(h.tone);
            let look = HeadingLook { text_size: self.typography.size, divided: pane > 0, accent };
            pane_heading(theme, look, pane, h.role.clone(), text, controls)
        };
        let inputs = h_flex().w_full().children(
            self.headings.iter().zip(controls).enumerate().map(|(pane, (h, controls))| {
                let text = self.heading_text(pane).unwrap_or_else(|| HeadingText::plain(&h.title));
                heading(pane, h, text, controls)
            }),
        );
        let r = &self.result_heading;
        let result = h_flex().w_full().child(heading(0, r, HeadingText::plain(&r.title), None));
        (inputs, result)
    }

    /// Turn the syntax colours off or on (spec §6.4). The inputs parse the first time they
    /// show; the result editor colours only within `syntax-whole-file-max-lines`.
    fn toggle_syntax(&mut self, _: &ToggleSyntax, _: &mut Window, cx: &mut Context<Self>) {
        self.colouring.toggle();
        let language = self.colouring.editor_language();
        self.result.update(cx, |state, cx| state.set_highlighter(language, cx));
        if let Some(language) = self.colouring.language()
            && !self.parsing
        {
            self.parsing = true;
            self.syntax = Self::input_syntax(language, self.session.document());
        }
        cx.notify();
    }

    /// Show or hide the base pane of a three-way merge (spec §5).
    fn toggle_base_pane(&mut self, _: &ToggleBasePane, _: &mut Window, cx: &mut Context<Self>) {
        let panes = match self.panes {
            PaneSet::LeftRight => PaneSet::LeftBaseRight,
            PaneSet::LeftBaseRight => PaneSet::LeftRight,
            PaneSet::Sides(_) => return,
        };
        let rows = match RowMap::new(self.session.document(), panes) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::error!(%error, "cannot show these panes");
                return;
            }
        };
        self.panes = panes;
        self.input_select.keep_shown(&panes.sources());
        self.rows = rows;
        self.scroll.row_wrap_mut().rows_changed(self.rows.row_count());
        self.highlights = Highlights::new(panes);
        self.headings = headings(self.session.document(), panes, &self.file_names);
        self.search_inputs_again(cx);
        cx.notify();
    }

    /// The search matches each of `panes` top panes shows on `row`.
    fn row_matches(&self, row: RowIx, panes: usize) -> Vec<Vec<(Range<usize>, Matched)>> {
        (0..panes).map(|pane| self.input_find.in_cell(row, pane).collect()).collect()
    }

    /// How each of `panes` top panes' cells paint, before a cell's own change mark.
    fn cell_paints(&self, panes: usize, cx: &App) -> Vec<CellPaint> {
        (0..panes)
            .map(|pane| CellPaint {
                changed: None,
                fills: self.match_fills(pane, cx),
                selected: cx.theme().selection,
                tab_width: self.typography.tab_width,
            })
            .collect()
    }

    /// The inputs' aligned rows. The list cannot take a role, so a table around it holds them.
    fn render_rows(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        v_flex()
            .id(INPUT_LINES_ID)
            .test_support()
            .role(Role::Table)
            .aria_label(strings::INPUT_LINES)
            .aria_row_count(self.rows.row_count())
            .aria_column_count(self.headings.len())
            .flex_1()
            .min_h_0()
            .relative()
            .track_focus(self.input_select.focus())
            .key_context(INPUT_PANES_CONTEXT)
            .on_action(cx.listener(Self::copy_input))
            .on_action(cx.listener(Self::select_all_input))
            .on_scroll(cx.listener(Self::wheel_sideways))
            .child({
                // One item per display row: an aligned row takes several while it wraps.
                let mut list = uniform_list(
                    ROWS_ID,
                    self.scroll.heights().rows(),
                    cx.processor(|this, range: Range<usize>, _window, cx| {
                        let rows = this.rows_of(range.clone());
                        this.want_syntax(rows);
                        let first = range.start;
                        let shown = range
                            .filter_map(|ix| {
                                let (row, sub) = this.scroll.heights().at(ix)?;
                                let display = DisplayRow { ix, row: RowIx::new(row), sub };
                                Some(this.render_row(display, ix == first, cx))
                            })
                            .collect::<Vec<_>>();
                        this.start_parses(cx);
                        shown
                    }),
                )
                .track_scroll(self.scroll.list())
                .flex_1();
                // A sideways wheel scrolls the text, not the list.
                list.style().restrict_scroll_to_axis = Some(true);
                list
            })
            .child(Scrollbar::vertical(self.scroll.list()).mode(self.scrollbar_mode))
            .when(self.input_select.dragging(), |d| d.child(Self::drag_listener(cx)))
    }

    /// While a selection drag holds the button: the pointer's moves and its release anywhere
    /// in the window, so a drag past the panes' edge scrolls them (spec §5).
    fn drag_listener(cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let moved = this.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble {
                        ignore_closed_view(moved.update(cx, |this, cx| {
                            this.drag_past_edge(bounds, event, window, cx);
                        }));
                    }
                });
                let released = this.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        ignore_closed_view(released.update(cx, |this, cx| {
                            this.input_select.release();
                            cx.notify();
                        }));
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    /// A drag's move anywhere: past the panes' edge, start the steps that scroll them.
    fn drag_past_edge(
        &mut self,
        panes: gpui_kit::Bounds<Pixels>,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.input_select.release();
            cx.notify();
            return;
        }
        let most = self.typography.line_height() * AUTOSCROLL_MAX_ROWS;
        let step =
            input_select::autoscroll_step(event.position.y, panes.top(), panes.bottom(), most);
        self.input_select.set_edge(step);
        if self.input_select.edge().is_some() && self.autoscroll.is_none() {
            self.autoscroll = Some(cx.spawn_in(window, async move |this, cx| {
                loop {
                    cx.background_executor().timer(AUTOSCROLL_TICK).await;
                    match this.update(cx, Self::autoscroll_step) {
                        Ok(true) => {}
                        Ok(false) | Err(_) => break,
                    }
                }
                ignore_closed_view(this.update(cx, |this, _| this.autoscroll = None));
            }));
        }
    }

    /// One autoscroll step: scroll the panes toward the pointer and move the selection's end
    /// to the row at that edge. Whether the steps go on.
    fn autoscroll_step(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(step) = self.input_select.edge() else {
            return false;
        };
        let row_h = self.typography.line_height();
        let view = self.scroll.view_height();
        #[expect(clippy::cast_precision_loss, reason = "row counts stay far below 2^24")]
        let rows = row_h * self.scroll.heights().rows() as f32;
        let top = (self.scroll.top() + step).max(Pixels::ZERO).min((rows - view).max(Pixels::ZERO));
        self.scroll.set_top(f32::from(top));
        let down = step > Pixels::ZERO;
        let row = if down { self.bottom_row(top, view, row_h) } else { self.scroll.top_row(row_h) };
        let inputs = input_select::Inputs {
            doc: self.session.document(),
            rows: &self.rows,
            tab_width: self.typography.tab_width,
        };
        self.input_select.drag_to_row(inputs, row, down);
        cx.notify();
        true
    }

    /// The last aligned row whose display rows reach into a view `view` high from `top`.
    fn bottom_row(&self, top: Pixels, view: Pixels, row_h: Pixels) -> RowIx {
        let heights = self.scroll.heights();
        let last = heights.rows().saturating_sub(1);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a non-negative whole number of rows"
        )]
        let display = (((top + view) / row_h).ceil() as usize).saturating_sub(1).min(last);
        RowIx::new(heights.at(display).map_or(0, |(row, _)| row))
    }

    /// The aligned rows display rows `display` show.
    fn rows_of(&self, display: Range<usize>) -> Range<usize> {
        let heights = self.scroll.heights();
        let row = |ix: usize| heights.at(ix).map_or(heights.items(), |(row, _)| row);
        match display.end.checked_sub(1) {
            Some(last) if display.start < display.end => row(display.start)..row(last) + 1,
            _ => 0..0,
        }
    }

    /// Rows `rows` show; their lines' chunks parse while the colours are on.
    fn want_syntax(&mut self, rows: Range<usize>) {
        if self.colouring.language().is_none() {
            return;
        }
        for index in rows {
            let cells = self.rows.row(self.session.document(), RowIx::new(index)).cells;
            for (source, line) in self.panes.sources().into_iter().zip(cells) {
                if let Some(line) = line {
                    self.syntax.want(source, line);
                }
            }
        }
    }

    /// Display row `display`: an aligned row, or one of the rows it wraps to. The first row on
    /// screen, `probed`, measures each pane's text width.
    #[expect(
        clippy::too_many_lines,
        reason = "one cell per pane from many borrowed parts; the highlight cache's borrow \
                  keeps them in one function"
    )]
    fn render_row(
        &mut self,
        display: DisplayRow,
        probed: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let DisplayRow { ix, row: index, sub } = display;
        let head = sub == 0;
        let row = self.rows.row(self.session.document(), index);
        let region = row.region;
        let current = self.is_highlighted(region) && self.highlight_shown();
        let first = self.rows.first_row(region) == index;
        let pickable =
            head && first && self.session.document().region(region).kind != Kind::Unchanged;
        // Before the spans, which borrow the highlight cache.
        let mut origins = self.row_origins(&row.cells);
        let matches = self.row_matches(index, row.cells.len());
        let paints = self.cell_paints(row.cells.len(), cx);
        let spans = self.highlights.row(self.session.document(), &self.rows, index);
        let theme = cx.theme();
        let tones = self.palette.tones(theme.is_dark());
        let strip = tones.current();
        let kind = self.session.document().region(region).kind;
        let input_mark = marks::input_mark(kind)
            .filter(|_| head)
            .map(|mark| (mark.label, tones.mark(mark.tone)));
        let (filler, muted, border) = (theme.muted, theme.muted_foreground, theme.border);
        let doc = self.session.document();
        let inputs = doc.inputs();
        // The theme is borrowed from `cx`, which the pick targets take mutably.
        let highlight_theme = Arc::clone(&theme.highlight_theme);
        let colouring = self.colouring;
        let left = self.scroll.sideways().left();
        let cells = self
            .panes
            .sources()
            .into_iter()
            .zip(&row.cells)
            .zip(spans)
            .enumerate()
            .map(|(pane, ((source, line), spans))| {
                let cell = line.map(|n| (n, inputs.text(source).line(n)));
                let deleted = rows::deleted_lines(self.session.document().region(region), source);
                let tone = marks::input_tone(kind, source);
                let band = tone.map(|tone| tones.band(tone));
                let changed = changed_colour(tones, kind, source);
                let syntax = line.map_or_else(Vec::new, |n| {
                    colouring.line(&self.syntax, source, n, &highlight_theme)
                });
                let title = &self.headings[pane].title;
                let target = pickable.then(|| {
                    let label = pick_label(self.session.document(), title, region);
                    let target = PickTarget {
                        state: PickState::of(self.session.document(), region, source),
                        shown: tones.mark(Tone::Conflict),
                        pane,
                        row: index,
                        region,
                        source,
                    };
                    pick_target(target, label, cx)
                });
                let deleted_colour = deleted.filter(|_| head).map(|_| tones.deleted());
                let note = deleted.filter(|_| head && first).map(strings::deleted_label);
                let origin = origins.as_mut().map(|o| o.next().flatten());
                let text = cell.and_then(|(n, text)| {
                    let paint = paints[pane].with_changed(changed);
                    let selected = self.input_select.in_line(doc, source, n);
                    let (text, highlights) =
                        line_highlights(text, spans, &syntax, &matches[pane], selected, paint);
                    let segment = self.scroll.row_wrap().segment(index.get(), pane, sub, &text)?;
                    Some((segment.start, segment_text(text, highlights, segment)))
                });
                let segment_start = text.as_ref().map_or(0, |(start, _)| *start);
                let text = text.map(|(_, (text, highlights))| {
                    StyledText::new(text).with_highlights(highlights)
                });
                let hit = CellHit {
                    source,
                    row: index,
                    segment_start,
                    layout: text.as_ref().map(|text| text.layout().clone()),
                };
                let moved = hit.clone();
                let (cell_id, text_id) = display.ids(pane);
                div()
                    .id(cell_id)
                    .test_support()
                    .when(head, |d| {
                        let label = strings::cell_label(
                            title,
                            cell.map(|(n, text)| (n, text.trim_end_matches(['\n', '\r']))),
                            deleted,
                        );
                        d.role(Role::Cell).aria_column_index(pane).aria_label(label)
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.go_to(region, window, cx);
                        // Moving the caret focuses the result; the press focused the panes.
                        window.focus(this.input_select.focus(), cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.press_input(&hit, event, window, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                        this.drag_input(&moved, event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, _| this.input_select.release()),
                    )
                    .flex_1()
                    .h_full()
                    .flex()
                    .overflow_hidden()
                    .when(pane > 0, |d| d.border_l(PANE_DIVIDER).border_color(border))
                    .when_some(band, gpui_kit::Styled::bg)
                    .when(cell.is_none() && deleted.is_none(), |d| d.bg(filler))
                    .child(pick_slot(target, current.then_some(strip)))
                    .child(input_mark_cell(input_mark))
                    .when_some(origin, |d, origin| {
                        d.child(display.origin_slot(pane, origin, muted))
                    })
                    .child(line_number_cell(line.filter(|_| head), deleted_colour, muted))
                    .child(cell_text(
                        text_id,
                        text,
                        deleted_colour.zip(note).map(|(colour, note)| deletion_note(colour, note)),
                        left,
                        probed.then(|| self.scroll.sideways().probe(pane)),
                    ))
            })
            .collect::<Vec<_>>();
        h_flex()
            .id(ix)
            .when(head, |d| d.role(Role::Row).aria_row_index(index.get()))
            .h(self.typography.line_height())
            .w_full()
            .children(cells)
    }
}

/// One of the list's display rows: display row `sub` of aligned row `row`.
#[derive(Clone, Copy)]
struct DisplayRow {
    /// Its place in the list.
    ix: usize,
    row: RowIx,
    sub: usize,
}

impl DisplayRow {
    /// The ids of `pane`'s cell and of its text on this display row.
    fn ids(self, pane: usize) -> (ElementId, ElementId) {
        let Self { row, sub, .. } = self;
        if sub == 0 {
            (cell_id(pane, row), cell_text_id(pane, row))
        } else {
            (cell_part_id(pane, row, sub), cell_text_part_id(pane, row, sub))
        }
    }

    /// `pane`'s origin column on this display row: the origin on the row's first, empty on
    /// the rows it wraps to.
    fn origin_slot(self, pane: usize, origin: Option<OriginCell>, muted: Hsla) -> AnyElement {
        if self.sub == 0 {
            origin_column_cell(pane, self.row, origin, muted).into_any_element()
        } else {
            div().w(ORIGIN_WIDTH).flex_none().into_any_element()
        }
    }
}

/// The display rows of each of the result's rows, as its editor lays them out under `wrap`.
fn result_heights(result: ResultRows, wrap: Wrap, state: &EditorState) -> Heights {
    match wrap {
        Wrap::On => result.heights(|line| {
            state.display_map().buffer_line_to_display_row_range(line).map_or(0, |rows| rows.len())
        }),
        Wrap::Off | Wrap::TooLong => Heights::Uniform(result.count()),
    }
}

/// A top pane's line number slot: the line's number, [`DELETED_ROW_MARK`] in `deleted`'s colour
/// on a filler row of a side that deleted base lines, or nothing on another filler row.
fn line_number_cell(
    line: Option<usize>,
    deleted: Option<gpui_kit::Hsla>,
    muted: gpui_kit::Hsla,
) -> impl IntoElement {
    div()
        .w(LINE_NUMBER_WIDTH)
        .flex_none()
        .text_color(muted)
        .when_some(line, |d, n| d.child(SharedString::from((n + 1).to_string())))
        .when_some(deleted, |d, colour| d.text_color(colour).child(DELETED_ROW_MARK))
}

/// A top pane's conflict mark slot: the mark's letter in its colour, or nothing.
fn input_mark_cell(mark: Option<(&'static str, gpui_kit::Hsla)>) -> impl IntoElement {
    div().w(INPUT_MARK_WIDTH).flex_none().when_some(mark, |d, (label, colour)| {
        d.text_color(colour).child(SharedString::new_static(label))
    })
}

/// The note on the first filler row of a side that deleted base lines.
fn deletion_note(colour: gpui_kit::Hsla, note: String) -> impl IntoElement {
    div().whitespace_nowrap().italic().text_color(colour).child(note)
}

fn origin_column_cell(
    pane: usize,
    row: RowIx,
    origin: Option<OriginCell>,
    muted: gpui_kit::Hsla,
) -> impl IntoElement {
    div()
        .id(origin_id(pane, row))
        .w(ORIGIN_WIDTH)
        .flex_none()
        .overflow_hidden()
        .text_color(muted)
        .when_some(origin, |d, OriginCell { label, hover }| {
            d.children(label)
                .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
        })
}

/// Whether a pick target's source is in its region's pick.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickState {
    On,
    Off,
    /// An unresolved conflict shows the source in the result, undecided.
    Shown,
}

/// What a pick target stands for.
#[derive(Clone, Copy)]
struct PickTarget {
    state: PickState,
    /// The target's colour while an unresolved conflict shows its source.
    shown: Hsla,
    pane: usize,
    row: RowIx,
    region: RegionIx,
    source: Source,
}

/// The fixed-width slot at a pane's left edge: a strip, coloured on the highlighted
/// region's rows, then the pick target, if any.
fn pick_slot(target: Option<impl IntoElement>, strip: Option<gpui_kit::Hsla>) -> impl IntoElement {
    h_flex()
        .w(PICK_INSET + PICK_WIDTH)
        .h_full()
        .flex_none()
        .child(div().w(PICK_INSET).h_full().when_some(strip, gpui_kit::Styled::bg))
        .children(target)
}

/// The accessible label of the pick target for `region` in the pane titled `title`.
fn pick_label(doc: &MergeDocument, title: &str, region: RegionIx) -> String {
    let unresolved = *doc.resolution(region) == Resolution::Unresolved;
    strings::pick_label(title, region, doc.regions().len(), unresolved)
}

/// The pick target on a region's first row: filled, and toggled on, when its source is in
/// the pick; half filled in the conflict tone, and toggled off, when an unresolved conflict
/// shows it.
fn pick_target(
    target: PickTarget,
    label: String,
    cx: &mut Context<MergeWindow>,
) -> impl IntoElement + use<> {
    let PickTarget { state, shown, pane, row, region, source } = target;
    div()
        .id(pick_id(pane, row))
        .test_support()
        .role(Role::Button)
        .aria_toggled(match state {
            PickState::On => Toggled::True,
            PickState::Off | PickState::Shown => Toggled::False,
        })
        .aria_label(label)
        .w(PICK_WIDTH)
        .flex_none()
        .cursor_pointer()
        .when(state == PickState::Shown, |d| d.text_color(shown))
        .child(match state {
            PickState::On => PICKED,
            PickState::Off => UNPICKED,
            PickState::Shown => SHOWN,
        })
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.pick(region, Some(source), window, cx);
        }))
}

/// How a cell paints its marks and matches, and how wide its tabs run.
#[derive(Clone, Copy)]
struct CellPaint {
    /// The background of the characters that differ.
    changed: Option<Hsla>,
    fills: MatchFills,
    /// The background of the selected characters.
    selected: Hsla,
    tab_width: usize,
}

impl CellPaint {
    /// This paint with `changed` behind the characters that differ.
    const fn with_changed(self, changed: Option<Hsla>) -> Self {
        Self { changed, ..self }
    }
}

/// The editors' search paints the other matches in the selection colour at this saturation.
const OTHER_MATCH_SATURATION: f32 = 0.1;

/// The backgrounds of search matches in the top panes, as the editors' search paints them.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MatchFills {
    other: Hsla,
    current: Hsla,
}

impl MatchFills {
    fn of(selection: Hsla) -> Self {
        Self { other: Hsla { s: OTHER_MATCH_SATURATION, ..selection }, current: selection }
    }
}

/// A line without its terminator, in its syntax colours, with the characters that differ
/// marked over them and the search matches over both.
/// A top pane's text on one row, `id`: clipped, and scrolled `left` to the right, or the
/// deletion note on a filler row, which stays put. `probe` measures its width, which every
/// row shares: the panes wrap at it.
fn cell_text(
    id: ElementId,
    line: Option<StyledText>,
    note: Option<impl IntoElement>,
    left: Pixels,
    probe: Option<impl IntoElement>,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .cursor_text()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .when_some(line, |d, line| {
            d.child(div().relative().left(-left).whitespace_nowrap().child(line))
        })
        .children(note)
        .children(probe)
}

/// The part `segment` of a cell's `text`, with the parts of `highlights` that fall in it, in
/// the part's own bytes. The whole text when the part covers it.
fn segment_text(
    text: SharedString,
    highlights: CellHighlights,
    segment: Range<usize>,
) -> (SharedString, CellHighlights) {
    if segment == (0..text.len()) {
        return (text, highlights);
    }
    let clipped = highlights
        .into_iter()
        .filter_map(|(range, style)| {
            let start = range.start.max(segment.start);
            let end = range.end.min(segment.end);
            (start < end).then(|| (start - segment.start..end - segment.start, style))
        })
        .collect();
    (SharedString::from(text[segment].to_owned()), clipped)
}

/// The text a cell draws and its highlights.
fn line_highlights(
    line: &str,
    spans: &CellSpans,
    syntax: &[(Range<usize>, HighlightStyle)],
    matched: &[(Range<usize>, Matched)],
    selected: Option<Range<usize>>,
    paint: CellPaint,
) -> (SharedString, CellHighlights) {
    let (text, runs) = runs::cell(line, spans, syntax, matched, selected, paint.tab_width);
    (
        SharedString::from(text.into_owned()),
        runs.into_iter()
            .map(|run| {
                let style = match_style(
                    run_style(run.style, run.changed, paint.changed),
                    run.matched,
                    paint.fills,
                );
                let style = if run.selected {
                    HighlightStyle { background_color: Some(paint.selected), ..style }
                } else {
                    style
                };
                (run.range, style)
            })
            .collect(),
    )
}

/// A run's style with the fill of the search match it lies in.
fn match_style(
    style: HighlightStyle,
    matched: Option<Matched>,
    fills: MatchFills,
) -> HighlightStyle {
    let fill = match matched {
        Some(Matched::Current) => fills.current,
        Some(Matched::Other) => fills.other,
        None => return style,
    };
    HighlightStyle { background_color: Some(fill), ..style }
}

/// A cell's highlights in bytes of the text it draws, sorted and disjoint.
pub type CellHighlights = Vec<(Range<usize>, HighlightStyle)>;

/// The background of the characters that differ in `source`'s cells of a `kind` region.
fn changed_colour(tones: &Tones, kind: Kind, source: Source) -> Option<Hsla> {
    marks::input_tone(kind, source).map(|tone| tones.mark(tone).opacity(INTRALINE_ALPHA))
}

/// A run's style: its syntax style, with the change mark's background when it differs.
fn run_style(
    style: Option<HighlightStyle>,
    differs: bool,
    changed: Option<gpui_kit::Hsla>,
) -> HighlightStyle {
    let style = style.unwrap_or_default();
    match changed.filter(|_| differs) {
        Some(mark) => HighlightStyle { background_color: Some(mark), ..style },
        None => style,
    }
}

/// How a pane heading draws: its text size, whether a divider sits on its left, and its tone.
#[derive(Clone, Copy)]
pub(crate) struct HeadingLook {
    pub text_size: Pixels,
    pub divided: bool,
    pub accent: Hsla,
}

/// A pane's heading: a role chip, when it has one, then its title, then the chip of the
/// commit its input shows, over a bar in its tone. It lines up with its pane: the same width
/// and the same divider on its left.
fn pane_heading(
    theme: &Theme,
    look: HeadingLook,
    pane: usize,
    role: Option<SharedString>,
    text: HeadingText,
    controls: Option<AnyElement>,
) -> impl IntoElement + use<> {
    let HeadingLook { text_size, divided, accent } = look;
    let HeadingText { title, chip } = text;
    v_flex()
        .flex_1()
        .min_w_0()
        .bg(theme.secondary)
        .font_family(theme.font_family.clone())
        .when(divided, |d| d.border_l(PANE_DIVIDER).border_color(theme.border))
        .child(
            h_flex()
                .gap_2()
                .px_2()
                .py(HEADING_PADDING)
                .text_size(text_size * HEADING_TEXT_SCALE)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .children(role.map(|role| {
                    div()
                        .flex_none()
                        .px_1()
                        .rounded(CHIP_RADIUS)
                        .bg(accent.opacity(CHIP_ALPHA))
                        .text_color(accent)
                        .text_size(text_size * CHIP_TEXT_SCALE)
                        .child(role)
                }))
                // The end of a file name says the most, so a long one loses its start.
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .child(title),
                )
                .children(chip.map(|chip| {
                    revision_chip_element(theme, text_size, revision_chip_id(pane), chip)
                }))
                .children(controls),
        )
        .child(div().h(ACCENT_HEIGHT).w_full().bg(accent))
}

/// The chip naming the commit a pane's input shows, with its hover card. It keeps its width
/// while the title loses its start.
pub(crate) fn revision_chip_element(
    theme: &Theme,
    text_size: Pixels,
    id: ElementId,
    chip: OriginCell,
) -> impl IntoElement + use<> {
    let OriginCell { label, hover } = chip;
    div()
        .id(id)
        .flex_none()
        .px_1()
        .rounded(CHIP_RADIUS)
        .border_1()
        .border_color(theme.border)
        .font_family(theme.mono_font_family.clone())
        .font_weight(FontWeight::NORMAL)
        .text_color(theme.muted_foreground)
        .text_size(text_size * CHIP_TEXT_SCALE)
        .children(label)
        .tooltip(move |window, cx| Tooltip::new(hover.clone()).build(window, cx))
}

/// A pane's heading, as the window draws it.
struct Heading {
    title: SharedString,
    /// The role chip's text, in capitals.
    role: Option<SharedString>,
    tone: Tone,
}

impl From<status::PaneHeading<'_>> for Heading {
    fn from(heading: status::PaneHeading<'_>) -> Self {
        Self {
            title: heading.title.into_owned().into(),
            role: heading.role.map(|role| role.to_uppercase().into()),
            tone: heading.tone,
        }
    }
}

/// Whether the result wraps at first: if `wanted`, unless an input is too long to wrap.
fn input_wrap(doc: &MergeDocument, wanted: bool) -> Wrap {
    let inputs = doc.inputs();
    let lines = inputs.sides().iter().chain(inputs.bases());
    Wrap::new(wanted, lines.map(Text::line_count))
}

fn headings(doc: &MergeDocument, panes: PaneSet, names: &FileNames) -> Vec<Heading> {
    status::pane_headings(doc, panes, names).into_iter().map(Heading::from).collect()
}

/// The result pane's right-click menu: the current region's suggestions, then the choices,
/// with the one it holds checked, then the editing items the editor's own menu has.
fn choice_menu(menu: NativeMenu, items: &[MenuChoice]) -> NativeMenu {
    use strings::edit_menu;
    items
        .iter()
        .enumerate()
        .fold(menu, |menu, (n, item)| {
            let after_suggestions = n > 0 && items[n - 1].suggested && !item.suggested;
            let menu = if after_suggestions { menu.separator() } else { menu };
            let label = if item.suggested {
                strings::suggested_label(item.action)
            } else {
                strings::menu_label(item.action).into()
            };
            menu.menu_with_check(label, item.checked, item.action.action())
        })
        .separator()
        .menu(edit_menu::CUT, Box::new(input::Cut))
        .menu(edit_menu::COPY, Box::new(input::Copy))
        .menu(edit_menu::PASTE, Box::new(input::Paste))
        .separator()
        .menu(edit_menu::SELECT_ALL, Box::new(input::SelectAll))
}

/// One spacer row and its note for each placeholder of the result (`placeholder::placeholders`),
/// filled, framed and marked as the block it stands for (`marks::placeholder_mark`).
fn result_placeholders(
    doc: &MergeDocument,
    tones: &Tones,
) -> (Vec<(usize, usize)>, Vec<SpacerNote>, Vec<RegionIx>) {
    let colour = tones.deleted();
    let found = placeholder::placeholders(doc);
    let regions = found.iter().map(|p| p.region).collect();
    let (spacers, notes) = found
        .iter()
        .map(|p| {
            let roles = p.sources.iter().map(|&s| status::role_name(doc, s)).collect::<Vec<_>>();
            let label = strings::placeholder_label(p.lines, roles.iter().map(AsRef::as_ref));
            let look = marks::placeholder_mark(doc, p.region, &p.sources);
            let note = SpacerNote::new(p.line.get(), label, colour)
                .with_fill(tones.band(look.tone))
                .with_gutter_mark(GutterMark::new(look.mark.label, tones.mark(look.mark.tone)));
            let note = match look.border {
                Some(tone) => note.with_border(tones.mark(tone)),
                None => note,
            };
            ((p.line.get(), placeholder::PLACEHOLDER_ROWS), note)
        })
        .unzip();
    (spacers, notes, regions)
}

/// A full-line band and a gutter mark for each band of the result (`marks::bands`).
fn region_decorations(bands: &[Band], tones: &Tones) -> Vec<RangeDecoration> {
    bands
        .iter()
        .map(|band| {
            let decoration = RangeDecoration::new(band.bytes.clone())
                .with_style(RangeDecorationStyle::FullLine)
                .with_color(tones.band(band.tone))
                .with_gutter_mark(GutterMark::new(band.mark.label, tones.mark(band.mark.tone)));
            match band.border {
                Some(tone) => decoration.with_border(tones.mark(tone)),
                None => decoration,
            }
        })
        .collect()
}

impl MergeWindow {
    /// The status bar: the counts, the copy note and the mixed-endings warning.
    fn render_status(&self, mixed_tone: Hsla, secondary: Hsla, muted: Hsla) -> impl IntoElement {
        div()
            .id(STATUS_ID)
            .test_support()
            .role(Role::Status)
            .aria_label(self.status.clone())
            .flex()
            .gap_2()
            .px_2()
            .bg(secondary)
            .text_color(muted)
            .child(self.status.clone())
            .children(self.copy_note.clone().map(|note| {
                div()
                    .id(COPY_NOTE_ID)
                    .test_support()
                    .role(Role::Status)
                    .aria_label(note.clone())
                    .child(note)
            }))
            .when(self.counts.mixed_endings, |d| {
                d.child(
                    div()
                        .id(MIXED_ENDINGS_ID)
                        .test_support()
                        .role(Role::Status)
                        .aria_label(strings::MIXED_ENDINGS)
                        .text_color(mixed_tone)
                        .child(strings::MIXED_ENDINGS),
                )
            })
    }

    /// Whether the window draws its own menu bar (spec §2.2).
    #[must_use]
    pub fn has_menu_bar(&self) -> bool {
        self.menu_bar.bar().is_some()
    }
}

impl Render for MergeWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ticks = self.view_ticks();
        self.menu_bar.show(ticks, window, cx);
        self.drive_measure(window, cx);
        let mixed_tone = self.tones(cx).mark(Tone::Conflict);
        let controls = self.input_controls(cx);
        let theme = cx.theme();
        let (mono, border, secondary, muted) =
            (theme.mono_font_family.clone(), theme.border, theme.secondary, theme.muted_foreground);
        let (input_headings, result_heading) = self.render_headings(theme, controls);
        Self::on_merge_actions(v_flex(), cx)
            .size_full()
            .key_context(WINDOW_CONTEXT)
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(|this, _: &NextDifference, window, cx| {
                this.navigate(nav::next, Target::Difference, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousDifference, window, cx| {
                this.navigate(nav::previous, Target::Difference, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextConflict, window, cx| {
                this.navigate(nav::next, Target::Unresolved, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousConflict, window, cx| {
                this.navigate(nav::previous, Target::Unresolved, window, cx);
            }))
            .on_action(cx.listener(Self::toggle_base_pane))
            .on_action(cx.listener(Self::toggle_syntax))
            .on_action(cx.listener(Self::toggle_soft_wrap))
            .on_action(cx.listener(Self::toggle_scroll_sync))
            // Without origins there is nothing to blame, so the Blame key does nothing.
            .when(self.origin_column.origins().loaded(), |root| {
                root.on_action(cx.listener(Self::toggle_origin_column))
            })
            .on_action(cx.listener(Self::find))
            .on_action(cx.listener(Self::open_go_to_line))
            .on_action(cx.listener(Self::save_and_exit))
            .on_action(cx.listener(Self::save_copy_as))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.cancel(window, cx)))
            .font_family(self.typography.font.clone().unwrap_or(mono))
            .text_size(self.typography.size)
            .children(crate::menu_bar::render(self.menu_bar.bar(), cx.theme().is_dark()))
            .child(self.render_toolbar(window, cx))
            .children(render_banner(self.settings_errors.as_ref(), |v| &mut v.settings_errors, cx))
            .child(input_headings)
            .child(self.render_rows(cx))
            .child({
                // Between the panes: after the rows' prepaint, and before the result's, which
                // applies a scroll offset set here and drops one set after it.
                let this = cx.entity().downgrade();
                canvas(
                    move |_, window, cx| {
                        ignore_closed_view(this.update(cx, |this, cx| {
                            this.fit_rows(window, cx);
                            if !this.apply_reveal(window, cx) {
                                this.sync_scroll(window, cx);
                            }
                        }));
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .size_0()
            })
            .child(div().h(PANE_DIVIDER).w_full().bg(border))
            .child(result_heading)
            .when(self.go_to_line.is_open(), |d| {
                let close =
                    cx.listener(|this, _: &Escape, window, cx| this.close_go_to_line(window, cx));
                d.child(self.go_to_line.render(self.session.document().line_count(), close))
            })
            .child(self.render_result(cx))
            .child(self.render_status(mixed_tone, secondary, muted))
    }
}

/// Whether a search keeps each pane's current match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keep {
    Yes,
    No,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_offers_save_and_exit_first_then_discard_then_keep_editing() {
        assert_eq!(Closing::ALL.map(closing_label), ["Save & Exit", "Discard", "Keep Editing"]);
    }

    #[test]
    fn a_segment_keeps_the_highlights_inside_it_in_its_own_bytes() {
        let style = |colour| HighlightStyle { color: Some(colour), ..HighlightStyle::default() };
        let (red, blue) = (style(gpui_kit::red()), style(gpui_kit::blue()));
        // "abcdefgh": red over "bcd", blue over "fg".
        let highlights = vec![(1..4, red), (5..7, blue)];
        let (text, clipped) = segment_text("abcdefgh".into(), highlights.clone(), 3..6);
        assert_eq!(text.as_ref(), "def");
        assert_eq!(clipped, vec![(0..1, red), (2..3, blue)]);
        let (text, clipped) = segment_text("abcdefgh".into(), highlights.clone(), 4..5);
        assert_eq!((text.as_ref(), clipped), ("e", vec![]));
        let (text, clipped) = segment_text("abcdefgh".into(), highlights.clone(), 0..8);
        assert_eq!((text.as_ref(), clipped), ("abcdefgh", highlights));
    }

    #[test]
    fn a_mark_keeps_the_syntax_colour_and_sets_the_background() {
        let red = gpui_kit::red();
        let blue = gpui_kit::blue();
        let keyword = HighlightStyle { color: Some(red), ..HighlightStyle::default() };
        assert_eq!(
            run_style(Some(keyword), true, Some(blue)),
            HighlightStyle {
                color: Some(red),
                background_color: Some(blue),
                ..HighlightStyle::default()
            }
        );
        assert_eq!(run_style(Some(keyword), false, Some(blue)), keyword);
        assert_eq!(run_style(Some(keyword), true, None), keyword);
        assert_eq!(
            run_style(None, true, Some(blue)),
            HighlightStyle { background_color: Some(blue), ..HighlightStyle::default() }
        );
    }

    #[test]
    fn a_match_fills_over_a_mark_and_keeps_the_syntax_colour() {
        let (red, blue, green, grey) =
            (gpui_kit::red(), gpui_kit::blue(), gpui_kit::green(), gpui_kit::black());
        let fills = MatchFills { other: grey, current: green };
        let marked = HighlightStyle {
            color: Some(red),
            background_color: Some(blue),
            ..HighlightStyle::default()
        };
        let filled = |background| HighlightStyle {
            color: Some(red),
            background_color: Some(background),
            ..HighlightStyle::default()
        };
        assert_eq!(match_style(marked, Some(Matched::Current), fills), filled(green));
        assert_eq!(match_style(marked, Some(Matched::Other), fills), filled(grey));
        assert_eq!(match_style(marked, None, fills), marked);
    }

    #[test]
    fn other_matches_fill_in_a_greyed_selection_colour() {
        let selection = gpui_kit::hsla(0.6, 0.8, 0.5, 0.4);
        assert_eq!(
            MatchFills::of(selection),
            MatchFills { other: gpui_kit::hsla(0.6, 0.1, 0.5, 0.4), current: selection }
        );
    }
}

/// What a search step reached, for its blink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    /// The current match of a top pane.
    Input(usize),
    /// A match in the result, by its bytes.
    Result(usize, usize),
}
