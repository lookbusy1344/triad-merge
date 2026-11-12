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
use std::ops::Range;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use async_channel::Receiver;
use gpui_kit::base::ScrollbarMode;
use gpui_kit::base::input::InputHighlighterFactory;
use gpui_kit::component::input::{
    Editor, EditorState, InputEvent, RangeDecoration, RangeDecorationCollection,
    RangeDecorationStyle, RopeExt as _,
};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClickEvent, Context, Div, ElementId, Entity,
    Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, PromptLevel,
    Render, Role, SharedString, Size, StatefulInteractiveElement as _, Styled as _, Subscription,
    Task, TestSupportExt as _, Window, WindowOptions, canvas, div, point,
    prelude::FluentBuilder as _, px,
};
use merge_config::settings::{Colours, DEFAULT_COLOURS, Scrollbars};
use merge_core::diff::DiffOptions;
use merge_core::diff2::{DiffDocument, Edited, Kind2, Region2, Rope, Side};
use merge_core::index::RegionIx;
use merge_core::merge3::Source;
use merge_core::text::{Encoding, Text};
use merge_session::SessionError;
use merge_session::origin::carry;
use merge_session::repository::Handoff;
use merge_syntax::language::Language;
use merge_view::diff::{
    self, Pin, changed_bytes, differences, first_change, held_region, next_change, previous_change,
    region_bytes, region_rows_by, spacers, spacers_by,
};
use merge_view::scroll::{Follow, ScrollSync};
use vcs::{Annotation, IdKind, RepoKind, VcsError};

use crate::blink::{self, Flash, SearchSteps};
use crate::editor_sync::{self, Caret};
use crate::origin_column::{
    Annotated, Described, HeadingText, OriginCell, OriginColumn, Visibility,
};
use crate::recorder::Recorded;
use crate::ruler;
use crate::scrollbars;
use crate::settings_banner::{self, render as render_banner};
use crate::strings::{self, prompt};
use crate::syntax::Colouring;
use crate::tones::{Palette, Tones};
use crate::toolbar::{self, DIFF_GROUPS, Labels};
use crate::window::{
    OpenError, SCROLL_EPSILON, SOFT_WRAP_MAX_LINES, Syntax, revision_chip_element, revision_chip_id,
};
use crate::wrap::Wrap;
use crate::{
    Cancel, ExitCode, Find, NextDifference, PreviousDifference, Redo, Save, ToggleOriginColumn,
    ToggleSoftWrap, ToggleSyntax, Typography, Undo, finish, ignore_closed_view,
};

/// How long edits must pause before the regions are recomputed (spec §4.6).
pub const DIFF_DEBOUNCE: Duration = Duration::from_millis(100);
/// How long the watcher waits after a change on disk for the events that follow it.
const DISK_SETTLE: Duration = Duration::from_millis(100);
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
const ON_DISK_CHANGE: [&str; 2] = [prompt::KEEP_MINE, prompt::RELOAD];

/// The key contexts of the bindings in [`crate::init`].
pub(crate) const DIFF_CONTEXT: &str = "DiffWindow";
pub(crate) const PANE_CONTEXT: &str = "DiffPane";

/// The banner's element id.
pub const BANNER_ID: &str = "banner";
/// The status bar's element id.
pub const STATUS_ID: &str = "diff-status";
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
    /// When the panes show their scrollbars (spec §6.4).
    pub scrollbars: Scrollbars,
    /// The run's working directory, which relative paths resolve against; none when the
    /// process has none.
    pub cwd: Option<PathBuf>,
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
            scrollbars: Scrollbars::Always,
            cwd: None,
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
    /// The text and encoding the window last wrote to the file, until a reload. A change on
    /// disk that reads back as it is the window's own save.
    saved: Option<(Rope, Encoding)>,
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
    path: PathBuf,
    encoding: Encoding,
    text: Rope,
}

impl SideWrite {
    fn run(&self) -> Result<(), SessionError> {
        merge_session::save_text(&self.path, self.encoding, self.text.chunks())
    }
}

/// What follows a read of a pane's file after a change on disk.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OnRead {
    /// Reload a pane without edits; ask first in a pane with edits.
    Ask,
    /// Reload: the user said so.
    Reload,
}

pub struct DiffWindow {
    menu_bar: crate::menu_bar::WindowMenu,
    doc: DiffDocument,
    /// Whether the panes colour a chunk at a time, past `syntax-whole-file-max-lines`.
    chunked: bool,
    palette: Palette,
    panes: [Pane; 2],
    whitespace: &'static str,
    repo: Option<RepoKind>,
    status: SharedString,
    recompute: Option<Task<()>>,
    /// The latest save. Each save waits for the one before, so writes to a file land in the
    /// order asked.
    saving: Option<Task<()>>,
    /// Whether the recompute task has taken its snapshot and is diffing it.
    computing: bool,
    /// Navigations asked for while the regions lag the text, in order. Each runs once the
    /// recompute installs; an edit drops them.
    waiting: Vec<Navigation>,
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
    /// The config file's bad lines, which the settings banner shows.
    settings_errors: Option<SharedString>,
    /// Each pane's scroll top after the last sync, left then right.
    synced: [Pixels; 2],
    /// A scroll top for the left pane, set after it drew, to apply before it draws next.
    left_top: Option<Pixels>,
    /// The same for the left pane's left offset.
    left_left: Option<Pixels>,
    /// Where the panes stood sideways at the last sync, left as the rows and right as the
    /// result.
    sideways_sync: ScrollSync,
    /// The right pane's scroll, for the ruler's thumb.
    ruler_feed: scrollbars::Feed,
    ruler_drag: ruler::Dragging,
    /// Whether the panes wrap long lines.
    wrap: Wrap,
    /// What the wrapped panes' spacers were levelled for; `None` until they are, and after
    /// the regions change.
    wrap_fit: Option<WrapFit>,
    /// The region `go_to` moved to, which both panes scroll into view once laid out.
    reveal: Option<RegionIx>,
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

/// A step to the next or previous difference, from `side`'s current region.
#[derive(Clone, Copy)]
struct Navigation {
    step: fn(&DiffDocument, RegionIx) -> Option<RegionIx>,
    side: Side,
}

/// The task that starts the watcher on both files' directories and passes its events on.
/// The watcher lives as long as the task.
struct Watch {
    _events: Task<()>,
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
    let title = settings.title.clone().unwrap_or_else(|| strings::APP_NAME.to_owned());
    Ok(gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title(&title);
        let view = cx.new(|cx| DiffWindow::new([left, right], settings, window, cx));
        // Keys reach the window only through a focused element.
        let left = view.read(cx).panes[0].editor.clone();
        left.update(cx, |state, cx| state.focus(window, cx));
        view.update(cx, |this, cx| this.open_on_first_difference(window, cx));
        let this = view.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            ignore_closed_view(this.update(cx, |this, cx| this.close(window, cx)));
            false
        });
        view
    })?)
}

impl DiffWindow {
    fn new(
        files: [DiffFile; 2],
        settings: &DiffSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let [left, right] = &files;
        let mut doc = DiffDocument::new(&left.text, &right.text, settings.options);
        if let Some(lookup) = settings.origins.clone() {
            // Texts share their content, so the copies are O(1).
            Self::load_origins(lookup, [left.text.clone(), right.text.clone()], window, cx);
        }
        let palette = Palette::new(&settings.colours);
        let tones = *palette.tones(cx.theme().is_dark());
        let names = [left.origin.path(), right.origin.path()].into_iter().flatten();
        let colouring = Colouring::new(
            Language::of_run(names, None, settings.title.as_deref()),
            settings.syntax,
            settings.syntax_max_lines,
        );
        // Sides that open past the limit never wrap, though edits may shorten them.
        let wrap =
            Wrap::new(settings.soft_wrap, [left, right].iter().map(|file| file.text.line_count()));
        let ruler_feed = scrollbars::Feed::default();
        let look = Look {
            soft_wrap: wrap.wraps(),
            scrollbars: scrollbars::mode(settings.scrollbars),
            ruler_feed: &ruler_feed,
            typography: &settings.typography,
            tones: &tones,
            language: colouring.editor_language(),
            chunked: colouring.chunked_factory(left.text.line_count().max(right.text.line_count())),
        };
        let chunked = look.chunked.is_some();
        let mut subscriptions = Vec::with_capacity(SIDES.len());
        let [left, right] = files;
        let panes = [(Side::Left, left), (Side::Right, right)].map(|(side, file)| {
            doc.set_read_only(side, file.read_only);
            let pane = new_pane(&doc, side, file, &look, window, cx);
            subscriptions.push(cx.subscribe_in(
                &pane.editor,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.send_edits(side, window, cx);
                    }
                },
            ));
            subscriptions.push(cx.observe(&pane.editor, move |this, _, cx| {
                this.follow_search_step(side, cx);
            }));
            pane
        });
        subscriptions.push(cx.observe_window_activation(window, |this, _, cx| {
            // Another window may have changed the menu bar meanwhile.
            this.menu_bar.forget();
            cx.notify();
        }));
        let whitespace = strings::comparison(settings.options.comparison());
        // Without a working directory, a relative path stays relative.
        let cwd = settings.cwd.as_deref().unwrap_or(Path::new(""));
        let editable = panes.iter().filter(|p| !p.read_only);
        let banner = jj_banner(editable.filter_map(|p| p.origin.path()), cwd);
        let watch = settings
            .watch
            .then(|| watch(panes.each_ref().map(|p| p.origin.path()), cwd, window, cx));
        Self {
            menu_bar: crate::menu_bar::WindowMenu::new(cx),
            colouring,
            chunked,
            status: strings::diff_status(settings.repo, differences(&doc), whitespace).into(),
            repo: settings.repo,
            doc,
            panes,
            whitespace,
            recompute: None,
            saving: None,
            computing: false,
            waiting: Vec::new(),
            pinned: None,
            flash: Flash::default(),
            found: Flash::default(),
            banner,
            settings_errors: None,
            synced: [Pixels::ZERO; 2],
            left_top: None,
            left_left: None,
            sideways_sync: ScrollSync::default(),
            ruler_feed,
            ruler_drag: ruler::Dragging::default(),
            wrap,
            wrap_fit: None,
            reveal: None,
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

    /// The banner over the panes, if any.
    #[must_use]
    pub fn banner(&self) -> Option<&SharedString> {
        self.banner.as_ref()
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
        let core = || self.doc.text(side).to_string();
        let Some(batch) = editor_sync::take_batch(&pane.editor, &pane.recorded, core, cx) else {
            return;
        };
        let before = self.doc.version();
        match self.doc.edit(side, batch) {
            Ok(edited) => self.follow(before, &edited, Caret::Follow, window, cx),
            Err(error) => {
                tracing::error!(%error, ?side, "the core refused an edit; reloading the pane");
                self.reload(side, window, cx);
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
            self.waiting.clear();
            self.pane_mut(side).changes += 1;
            self.set_modified(side, true);
            self.schedule_recompute(window, cx);
        }
        self.refresh_bands(side, cx);
    }
    fn reload(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.doc.text(side).to_string();
        let pane = self.pane(side);
        pane.recorded.muted(|| {
            pane.editor.update(cx, |state, cx| {
                let caret = state.cursor().min(text.len());
                state.set_value(text, window, cx);
                state.set_selected_range(caret..caret, cx);
            });
        });
    }

    /// Recompute the regions [`DIFF_DEBOUNCE`] after the last edit, or at once while a
    /// navigation waits for them.
    fn schedule_recompute(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let delay = self.waiting.is_empty().then_some(DIFF_DEBOUNCE);
        self.start_recompute(delay, window, cx);
    }

    /// Recompute the regions off the UI thread after `delay`. Replacing the task drops the
    /// one before, which cancels it or discards its result.
    fn start_recompute(
        &mut self,
        delay: Option<Duration>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.computing = false;
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
            let (version, regions, carried) = cx
                .background_spawn(async move { job.run_with(|texts| carried(&origins, texts)) })
                .await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.computing = false;
                if this.doc.install(version, regions) {
                    this.install_origins(carried);
                    this.refresh(cx);
                    for navigation in std::mem::take(&mut this.waiting) {
                        this.step(navigation, window, cx);
                    }
                }
            }));
        }));
    }

    /// Give each pane's gutter its origins, while the column shows.
    fn refresh_labels(&self, cx: &mut Context<Self>) {
        let colour = cx.theme().muted_foreground;
        let origins = self.origin_column.origins();
        for side in SIDES {
            let source = Source::Side(index(side));
            let lines = (0..self.doc.text(side).len_lines()).map(|line| origins.line(source, line));
            let labels = self.origin_column.labels(lines, colour);
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
        for side in SIDES {
            self.editor(side).update(cx, |state, cx| state.set_soft_wrap(wrap, window, cx));
        }
        // Wrapped panes level once laid out (`level_wrapped`); unwrapped ones level by lines.
        self.set_spacers(cx);
    }

    /// Turn both panes' syntax colours off or on (spec §6.4).
    fn toggle_syntax(&mut self, _: &ToggleSyntax, _: &mut Window, cx: &mut Context<Self>) {
        self.colouring.toggle();
        let language = self.colouring.editor_language();
        for side in SIDES {
            self.editor(side).update(cx, |state, cx| state.set_highlighter(language, cx));
        }
    }

    /// The View menu items this window ticks. It has no base pane and no result.
    fn view_ticks(&self) -> crate::ViewTicks {
        crate::ViewTicks {
            origin_column: self.origin_column.shown(),
            origins_loaded: self.origin_column.origins().loaded(),
            syntax: self.colouring.shown(),
            soft_wrap: self.wrap.wraps(),
            ..crate::ViewTicks::default()
        }
    }

    /// Redraw both panes' bands, spacers, origins and the status from the core.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_labels(cx);
        for side in SIDES {
            self.refresh_bands(side, cx);
        }
        self.set_spacers(cx);
        self.status =
            strings::diff_status(self.repo, differences(&self.doc), self.whitespace).into();
        cx.notify();
    }

    /// Give each pane the spacer rows that level it with the other (spec §5): by lines, or
    /// under soft wrap by the display rows each pane's lines wrap to.
    fn set_spacers(&mut self, cx: &mut Context<Self>) {
        self.wrap_fit = None;
        let wrapped = self.wrap.wraps();
        for side in SIDES {
            let rows: Vec<_> = if wrapped {
                spacers_by(&self.doc, side, self.line_rows(cx)).collect()
            } else {
                spacers(&self.doc, side).collect()
            };
            self.editor(side).update(cx, |state, cx| state.set_spacers(rows, cx));
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
        self.pane(side).editor.update(cx, |state, cx| {
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

    /// The region holding `side`'s caret, or the pinned one.
    fn current(&self, side: Side, cx: &App) -> RegionIx {
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
        self.waiting.push(navigation);
        if !self.computing {
            self.start_recompute(None, window, cx);
        }
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
        if let Some(region) = to {
            self.go_to(region, side, window, cx);
            self.start_flash(region, cx);
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
        // Moving a caret focuses its editor, so `side` goes last and keeps the focus.
        for to in [side.other(), side] {
            let at = region_bytes(&self.doc, to, region).start;
            self.pane(to).editor.update(cx, |state, cx| {
                let position = state.text().offset_to_position(at);
                state.set_cursor_position(position, window, cx);
            });
        }
        self.pinned =
            Some(Pin { side, region, caret: region_bytes(&self.doc, side, region).start });
        self.reveal = Some(region);
        cx.notify();
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
        let reached = pane.steps.stepped(pane.editor.read(cx).search_session());
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

    /// The byte ranges `side`'s pane frames now.
    #[must_use]
    pub fn flash_frames(&self, side: Side, cx: &App) -> Vec<Range<usize>> {
        self.pane(side).flash.get_ranges(cx)
    }

    /// Write each side with unsaved edits to its own path, as it stands now, off the UI
    /// thread and once the save before has landed. On a failure, an alert says why and the
    /// side keeps its marker. `after` says whether the window exits once every side is saved.
    fn save(&mut self, after: AfterSave, window: &mut Window, cx: &mut Context<Self>) {
        for side in SIDES {
            self.send_edits(side, window, cx);
        }
        let writes = self.side_writes();
        let before = self.saving.take();
        self.saving = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(before) = before {
                before.await;
            }
            let written = cx
                .background_spawn(async move {
                    writes
                        .into_iter()
                        .map(|write| {
                            let result = write.run();
                            (write, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.saved(written, after, window, cx);
            }));
        }));
    }

    /// What a save writes: each side with unsaved edits. A revision is read-only, so it is
    /// never modified.
    fn side_writes(&self) -> Vec<SideWrite> {
        SIDES
            .into_iter()
            .filter_map(|side| {
                let pane = self.pane(side);
                let (true, Some(path)) = (pane.modified, pane.origin.path()) else {
                    return None;
                };
                Some(SideWrite {
                    side,
                    changes: pane.changes,
                    path: path.to_path_buf(),
                    encoding: pane.encoding,
                    text: self.doc.snapshot(side),
                })
            })
            .collect()
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
                    pane.saved = Some((write.text, write.encoding));
                    if unchanged {
                        self.set_modified(write.side, false);
                    }
                }
                Err(error) => {
                    all_saved = false;
                    let detail = error.to_string();
                    let answer = window.prompt(
                        PromptLevel::Critical,
                        prompt::CANNOT_SAVE,
                        Some(&detail),
                        &[prompt::OK],
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
        cx.notify();
        if all_saved && after == AfterSave::Exit {
            finish(ExitCode::Resolved, cx);
        }
    }

    /// Close the window, asking first when a side has unsaved edits. The window exits with 0.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for side in SIDES {
            self.send_edits(side, window, cx);
        }
        if !self.panes.iter().any(|p| p.modified) {
            finish(ExitCode::Resolved, cx);
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            prompt::SAVE_CHANGES,
            None,
            &[prompt::SAVE, prompt::DISCARD, prompt::KEEP_EDITING],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = answer.await else {
                return;
            };
            ignore_closed_view(this.update_in(cx, |this, window, cx| match CLOSING.get(answer) {
                Some(Closing::Save) => this.save(AfterSave::Exit, window, cx),
                Some(Closing::Discard) => finish(ExitCode::Resolved, cx),
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
        let task = cx.spawn_in(window, async move |this, cx| {
            let read = cx.background_spawn(async move { merge_session::read_text(&path) }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.disk_read(side, read, then, window, cx);
            }));
        });
        self.pane_mut(side).reading = Some((then, task));
    }

    fn disk_read(
        &mut self,
        side: Side,
        read: Result<Text, SessionError>,
        then: OnRead,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pane_mut(side).reading = None;
        let Some(text) = self.changed(side, read) else {
            return;
        };
        let pane = self.pane(side);
        if then == OnRead::Reload || !pane.modified {
            self.reload_from(side, &text, window, cx);
            return;
        }
        let question = prompt::changed_on_disk(&pane.origin.name());
        let answer = window.prompt(
            PromptLevel::Warning,
            &question,
            Some(prompt::CHANGED_DETAIL),
            &ON_DISK_CHANGE,
            cx,
        );
        self.pane_mut(side).asking = true;
        cx.spawn_in(window, async move |this, cx| {
            let reload = answer.await.ok().and_then(|answer| ON_DISK_CHANGE.get(answer))
                == Some(&prompt::RELOAD);
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
        let pane = self.pane(side);
        let same = text.encoding() == pane.encoding && self.doc.text(side) == text.content();
        let own_save = pane.saved.as_ref().is_some_and(|(saved, encoding)| {
            *encoding == text.encoding() && *saved == text.content()
        });
        (!same && !own_save).then_some(text)
    }

    fn reload_from(
        &mut self,
        side: Side,
        text: &Text,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.doc.reload(side, text);
        self.pinned = None;
        self.waiting.clear();
        let pane = self.pane_mut(side);
        pane.encoding = text.encoding();
        pane.changes += 1;
        pane.saved = None;
        self.reload(side, window, cx);
        self.set_modified(side, false);
        self.schedule_recompute(window, cx);
        self.refresh_bands(side, cx);
        tracing::info!(path = %self.pane(side).origin.name(), "reloaded");
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
        self.sync_sideways(window, cx);
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
        match from.other() {
            Side::Right => self.scroll_to(Side::Right, target, cx),
            Side::Left => {
                // The left pane laid out this frame already, and its paint drops an offset
                // set now: apply it before the left lays out in the next frame.
                self.left_top = Some(target);
                window.refresh();
            }
        }
    }

    /// Give both panes the left offset of the one that moved since the last sync. A pane
    /// whose lines end short of that offset stops at its end, and does not pull the other
    /// back.
    fn sync_sideways(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let left = |side: Side| -self.panes[index(side)].editor.read(cx).scroll_offset().x;
        let follow = self.sideways_sync.step(
            f32::from(left(Side::Left)),
            f32::from(left(Side::Right)),
            |left| left,
            |left| left,
        );
        match follow {
            Some(Follow::Result(target)) => self.scroll_left_to(Side::Right, px(target), cx),
            Some(Follow::Rows(target)) => {
                // As for a top: the left pane takes it before it lays out next.
                self.left_left = Some(px(target));
                window.refresh();
            }
            None => {}
        }
    }

    /// Apply a scroll top and left offset that `sync_scroll` set aside for the left pane.
    fn sync_left(&mut self, cx: &mut Context<Self>) {
        if let Some(top) = self.left_top.take() {
            self.scroll_to(Side::Left, top, cx);
        }
        if let Some(left) = self.left_left.take() {
            self.scroll_left_to(Side::Left, left, cx);
        }
    }

    fn scroll_left_to(&self, side: Side, left: Pixels, cx: &mut Context<Self>) {
        self.pane(side).editor.update(cx, |state, cx| {
            let y = state.scroll_offset().y;
            state.set_scroll_offset(point(-left, y), cx);
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
            region_rows_by(&self.doc, region, self.line_rows(cx)),
            f32::from(line_h),
            f32::from(view),
        );
        // The carets sit at the start of a line, so the panes go back to the left edge, as
        // the carets' own scroll would take them.
        for side in SIDES {
            self.pane(side).editor.update(cx, |state, cx| {
                state.set_scroll_offset(point(Pixels::ZERO, -px(top)), cx);
            });
        }
        self.sideways_sync.place(0.0, 0.0);
    }

    fn scroll_to(&self, side: Side, top: Pixels, cx: &mut Context<Self>) {
        self.pane(side).editor.update(cx, |state, cx| {
            let x = state.scroll_offset().x;
            state.set_scroll_offset(point(x, -top), cx);
        });
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
                Side::Right => (strings::COPY_TO_RIGHT, strings::COPY_TO_RIGHT_LABEL),
                Side::Left => (strings::COPY_TO_LEFT, strings::COPY_TO_LEFT_LABEL),
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
                ignore_closed_view(
                    scrolled.update(cx, |this, cx| this.scroll_to(Side::Right, px(top), cx)),
                );
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
                let top = merge_view::ruler::centred_top(line, f32::from(line_h), view);
                self.scroll_to(Side::Right, px(top), cx);
            }
        }
    }

    fn render_pane(&self, side: Side, cx: &App) -> impl IntoElement + use<> {
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
                            .when_some(self.typography.font.clone(), gpui_kit::Styled::font_family)
                            .text_size(self.typography.size)
                            .line_height(gpui_kit::relative(self.typography.spacing))
                            .bordered(false)
                            .h(gpui_kit::relative(1.)),
                    ),
            )
    }
}

/// Where `path`, as given on the command line, sits for the watcher: its directory made absolute
/// and free of symlinks, as the platform's events name it. The file itself may not exist:
/// a rename may be replacing it. `None` without a file name or an existing directory.
fn watch_target(path: &Path, cwd: &Path) -> Option<PathBuf> {
    let absolute = cwd.join(path);
    let name = absolute.file_name()?;
    let dir = absolute
        .parent()?
        .canonicalize()
        .inspect_err(|error| tracing::warn!(%error, path = %path.display(), "cannot watch a file"))
        .ok()?;
    Some(dir.join(name))
}

/// Watch both files' directories: editors save by writing a new file and renaming it over
/// the old one, which a watch on the file itself would lose. Starting a platform watch can
/// take seconds (`FSEvents` on macOS), so it starts off the UI thread.
fn watch(
    paths: [Option<&Path>; 2],
    cwd: &Path,
    window: &mut Window,
    cx: &mut Context<DiffWindow>,
) -> Watch {
    let (sender, receiver) = async_channel::unbounded::<Side>();
    let targets = paths.map(|path| path.and_then(|path| watch_target(path, cwd)));
    let dirs: Vec<PathBuf> = targets
        .iter()
        .flatten()
        .filter_map(|target| target.parent().map(Path::to_path_buf))
        .collect();
    let handler = move |event: notify::Result<notify::Event>| match event {
        Ok(event) if !event.kind.is_access() => {
            for side in SIDES {
                let target = targets[index(side)].as_deref();
                if event.paths.iter().any(|p| Some(p.as_path()) == target) {
                    // The window may have closed; then nobody listens.
                    let _ = sender.try_send(side);
                }
            }
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "the file watcher failed"),
    };
    let starting = cx.background_spawn(async move {
        let mut watcher = notify::recommended_watcher(handler)
            .inspect_err(|error| tracing::warn!(%error, "cannot watch the files"))
            .ok()?;
        for dir in &dirs {
            if let Err(error) =
                notify::Watcher::watch(&mut watcher, dir, notify::RecursiveMode::NonRecursive)
            {
                tracing::warn!(%error, dir = %dir.display(), "cannot watch a directory");
            }
        }
        Some(watcher)
    });
    let events = cx.spawn_in(window, async move |this, cx| {
        let Some(_watcher) = starting.await else {
            return;
        };
        while let Ok(first) = receiver.recv().await {
            // A save gives several events; read each file once they settle.
            cx.background_executor().timer(DISK_SETTLE).await;
            let gone = this
                .update_in(cx, |this, window, cx| {
                    for side in changed_sides(first, &receiver) {
                        this.disk_changed(side, window, cx);
                    }
                })
                .is_err();
            if gone {
                break;
            }
        }
    });
    Watch { _events: events }
}

/// The side of `first` and of every event waiting in `receiver`, each once, left first.
fn changed_sides(first: Side, receiver: &Receiver<Side>) -> impl Iterator<Item = Side> + use<> {
    let mut changed = [false; SIDES.len()];
    changed[index(first)] = true;
    while let Ok(side) = receiver.try_recv() {
        changed[index(side)] = true;
    }
    SIDES.into_iter().filter(move |&side| changed[index(side)])
}

/// The banner for editable files `jj diff --tool` copied, whose edits it throws away
/// (spec §7.1).
fn jj_banner<'a>(paths: impl IntoIterator<Item = &'a Path>, cwd: &Path) -> Option<SharedString> {
    paths.into_iter().any(|path| in_jj_diff_copy(path, cwd)).then(|| strings::JJ_DIFF_BANNER.into())
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
    let name = origin.name();
    if modified { format!("{name}{}", strings::MODIFIED).into() } else { name.into_owned().into() }
}

/// How a new pane's editor looks.
struct Look<'a> {
    soft_wrap: bool,
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
            .folding(false)
            .indent_guides(false)
            .soft_wrap(look.soft_wrap)
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
    let (bands, flash, found) = editor.update(cx, |state, cx| {
        (
            state.create_range_decorations_collection(decorations, cx),
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
        flash,
        found,
        steps: SearchSteps::default(),
        read_only: file.read_only,
        modified: false,
        asking: false,
        changes: 0,
        saved: None,
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

/// A full-line band over each of `side`'s changed regions.
fn bands(doc: &DiffDocument, side: Side, tones: &Tones) -> Vec<RangeDecoration> {
    changed_bytes(doc, side)
        .map(|bytes| {
            RangeDecoration::new(bytes)
                .with_style(RangeDecorationStyle::FullLine)
                .with_color(tones.band(diff::tone(side)))
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
    }
}

impl Render for DiffWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ticks = self.view_ticks();
        self.menu_bar.show(ticks, window, cx);
        let theme = cx.theme();
        let (mono, secondary, muted, warning, warning_foreground) = (
            theme.mono_font_family.clone(),
            theme.secondary,
            theme.muted_foreground,
            theme.warning,
            theme.warning_foreground,
        );
        self.on_view_actions(v_flex(), cx)
            .size_full()
            .key_context(DIFF_CONTEXT)
            .on_action(cx.listener(Self::find))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(|this, _: &NextDifference, window, cx| {
                this.navigate(next_change, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousDifference, window, cx| {
                this.navigate(previous_change, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &Save, window, cx| this.save(AfterSave::Stay, window, cx)),
            )
            .on_action(cx.listener(|this, _: &Cancel, window, cx| this.close(window, cx)))
            .font_family(self.typography.font.clone().unwrap_or(mono))
            .text_size(self.typography.size)
            .children(crate::menu_bar::render(self.menu_bar.bar(), cx.theme().is_dark()))
            .child(toolbar::bar(
                strings::DIFF_TOOLBAR,
                &DIFF_GROUPS,
                DIFF_CONTEXT,
                cx.theme().is_dark(),
                Labels::Shown,
                self.typography.toolbar_size,
            ))
            .children(render_banner(self.settings_errors.as_ref(), |v| &mut v.settings_errors, cx))
            .when_some(self.banner.clone(), |d, banner| {
                d.child(
                    div()
                        .id(BANNER_ID)
                        .test_support()
                        .role(Role::Alert)
                        .aria_label(banner.clone())
                        .px_2()
                        .bg(warning)
                        .text_color(warning_foreground)
                        .child(banner),
                )
            })
            .child(
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
                                    this.sync_left(cx);
                                    this.apply_reveal(window, cx);
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
                                ignore_closed_view(
                                    this.update(cx, |this, cx| this.sync_scroll(window, cx)),
                                );
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .size_0()
                    })
                    .child(self.render_gutter(window, cx))
                    .child(self.render_pane(Side::Right, cx))
                    .child(self.render_ruler(cx)),
            )
            .child(
                div()
                    .id(STATUS_ID)
                    .test_support()
                    .role(Role::Status)
                    .aria_label(self.status.clone())
                    .px_2()
                    .bg(secondary)
                    .text_color(muted)
                    .child(self.status.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_change_on_disk_offers_keep_mine_first() {
        assert_eq!(super::ON_DISK_CHANGE, ["Keep Mine", "Reload"]);
    }

    use super::*;

    fn changed(events: &[Side]) -> Vec<Side> {
        let (sender, receiver) = async_channel::unbounded();
        let (&first, rest) = events.split_first().expect("an event");
        for &side in rest {
            sender.try_send(side).expect("open channel");
        }
        let sides: Vec<Side> = changed_sides(first, &receiver).collect();
        assert!(receiver.is_empty(), "every waiting event is taken");
        sides
    }

    #[test]
    fn repeated_events_for_one_side_read_it_once() {
        assert_eq!(changed(&[Side::Left, Side::Left, Side::Left]), [Side::Left]);
    }

    #[test]
    fn events_for_both_sides_read_each_once_left_first() {
        assert_eq!(changed(&[Side::Right, Side::Left, Side::Right]), [Side::Left, Side::Right]);
    }

    #[test]
    fn one_event_reads_its_side_alone() {
        assert_eq!(changed(&[Side::Right]), [Side::Right]);
    }

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

    fn canonical(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().canonicalize().expect("the directory exists")
    }

    #[test]
    fn a_bare_name_sits_in_the_working_directory() {
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(watch_target(Path::new("x"), cwd.path()), Some(canonical(&cwd).join("x")));
    }

    #[test]
    fn a_relative_directory_resolves_against_the_working_directory() {
        let cwd = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(cwd.path().join("sub")).expect("mkdir");
        assert_eq!(
            watch_target(Path::new("./sub/../sub/x"), cwd.path()),
            Some(canonical(&cwd).join("sub").join("x"))
        );
    }

    #[test]
    fn an_absolute_path_ignores_the_working_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            watch_target(&dir.path().join("x"), cwd.path()),
            Some(canonical(&dir).join("x"))
        );
    }

    #[test]
    fn the_file_need_not_exist_for_a_rename_can_replace_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!dir.path().join("gone").exists());
        assert_eq!(
            watch_target(&dir.path().join("gone"), dir.path()),
            Some(canonical(&dir).join("gone"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_directory_resolves_to_its_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("real")).expect("mkdir");
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link"))
            .expect("symlink");
        assert_eq!(
            watch_target(&dir.path().join("link").join("x"), dir.path()),
            Some(canonical(&dir).join("real").join("x"))
        );
    }

    #[test]
    fn a_missing_directory_has_no_target() {
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(watch_target(Path::new("no/such/x"), cwd.path()), None);
    }

    #[test]
    fn a_path_without_a_file_name_has_no_target() {
        let cwd = tempfile::tempdir().expect("tempdir");
        assert_eq!(watch_target(Path::new(".."), cwd.path()), None);
    }
}
