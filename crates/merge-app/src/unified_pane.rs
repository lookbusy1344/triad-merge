//! The diff window's unified pane (spec §5, two-way mode): one read-only editor over the
//! unified text of `merge_view::unified`, each line numbered on both sides, with the left
//! tone over deleted lines and the right tone over inserted ones.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::ScrollbarMode;
use gpui_kit::base::input::LineNumbers;
use gpui_kit::component::input::{
    EditorState, RangeDecoration, RangeDecorationCollection, RangeDecorationStyle, RopeExt as _,
};
use gpui_kit::{App, AppContext as _, Context, Entity, Window};
use merge_core::diff2::{DiffDocument, Side};
use merge_view::diff::tone;
use merge_view::unified::{LineKind, Unified};

use crate::tones::Tones;
use crate::{Typography, scrollbars};

/// How the pane's editor looks when it is made; the window's View toggles change it after.
pub(crate) struct UnifiedLook<'a> {
    pub soft_wrap: bool,
    pub whitespace: bool,
    pub scrollbars: ScrollbarMode,
    pub typography: &'a Typography,
    pub tones: &'a Tones,
    /// The editor's language name, the panes'.
    pub language: &'static str,
}

pub(crate) struct UnifiedPane {
    editor: Entity<EditorState>,
    bands: RangeDecorationCollection,
    view: Rc<Unified>,
    /// The document version the pane shows.
    version: u64,
}

impl UnifiedPane {
    /// A pane over `doc`'s current regions, which must be current.
    pub(crate) fn new<V: 'static>(
        doc: &DiffDocument,
        look: &UnifiedLook<'_>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Self {
        let (view, text) = build(doc);
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(look.language)
                .line_number(true)
                .folding(false)
                .indent_guides(false)
                .soft_wrap(look.soft_wrap)
                .show_whitespaces(look.whitespace)
                .searchable(true)
                .replaceable(false)
                .tab_size(look.typography.tab_size())
                .default_value(text);
            scrollbars::install(&mut state, look.scrollbars, None);
            state
        });
        let bands = editor.update(cx, |state, cx| {
            let bands =
                state.create_range_decorations_collection(decorations(&view, look.tones), cx);
            state.set_line_numbers(Some(numbers(&view)), cx);
            bands
        });
        Self { editor, bands, view, version: doc.version() }
    }

    /// Show `doc`'s current text and regions, unless the pane shows them already. The caret
    /// stays on its line number, cut to the new text.
    pub(crate) fn update<V: 'static>(
        &mut self,
        doc: &DiffDocument,
        tones: &Tones,
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        if doc.version() == self.version {
            return;
        }
        let (view, text) = build(doc);
        self.editor.update(cx, |state, cx| {
            let line = state.text().offset_to_position(state.cursor()).line;
            state.set_value(text, window, cx);
            let at = state.text().line_start_offset(line as usize);
            state.set_selected_range(at..at, cx);
            state.set_line_numbers(Some(numbers(&view)), cx);
        });
        self.bands.set(decorations(&view, tones), cx);
        self.view = view;
        self.version = doc.version();
    }

    pub(crate) fn editor(&self) -> &Entity<EditorState> {
        &self.editor
    }

    pub(crate) fn view(&self) -> &Unified {
        &self.view
    }

    /// The bands as the editor holds them.
    pub(crate) fn band_ranges(&self, cx: &App) -> Vec<Range<usize>> {
        self.bands.get_ranges(cx)
    }

    /// The unified line holding the caret.
    pub(crate) fn caret_line(&self, cx: &App) -> usize {
        let state = self.editor.read(cx);
        state.text().offset_to_position(state.cursor()).line as usize
    }

    /// Put the caret at the start of unified line `line`, which scrolls it into view.
    pub(crate) fn go_to_line(&self, line: usize, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |state, cx| {
            let at = state.text().line_start_offset(line);
            let position = state.text().offset_to_position(at);
            state.set_cursor_position(position, window, cx);
        });
    }
}

fn build(doc: &DiffDocument) -> (Rc<Unified>, String) {
    let (view, text) = Unified::build(doc.text(Side::Left), doc.text(Side::Right), doc.regions());
    (Rc::new(view), text)
}

fn numbers(view: &Rc<Unified>) -> LineNumbers {
    let view = Rc::clone(view);
    LineNumbers::new(view.label_width(), move |line| view.label(line).into())
}

/// A full-line band over each run of deleted lines in the left tone, and of inserted lines in
/// the right tone.
fn decorations(view: &Unified, tones: &Tones) -> Vec<RangeDecoration> {
    [(LineKind::Deleted, Side::Left), (LineKind::Inserted, Side::Right)]
        .into_iter()
        .flat_map(|(kind, side)| {
            let colour = tones.band(tone(side));
            view.bytes_of(kind).map(move |bytes| {
                RangeDecoration::new(bytes)
                    .with_style(RangeDecorationStyle::FullLine)
                    .with_color(colour)
            })
        })
        .collect()
}
