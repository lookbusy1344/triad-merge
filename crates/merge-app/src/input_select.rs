//! Selection in the top panes (spec §5, Selection in the top panes): presses and drags over a
//! pane's rows select its input's text, and ⌘C copies it. [`merge_view::selection`] holds
//! the model; this module turns pointer positions into its byte offsets.

use std::ops::Range;
use std::time::Duration;

use gpui_kit::{
    FocusHandle, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, TextLayout, point, px,
};
use merge_core::document::MergeDocument;
use merge_core::merge3::Source;
use merge_view::rows::{RowIx, RowMap};
use merge_view::selection::{self, InputSelection};
use merge_view::tabs;

/// The key context of the top panes, where ⌘C and ⌘A act on their selection.
pub const INPUT_PANES_CONTEXT: &str = "InputPanes";

/// Presses that select a word, and presses from which on a press selects a line.
const WORD_PRESSES: usize = 2;
const LINE_PRESSES: usize = 3;

/// The top panes' focus and selection.
pub(crate) struct InputSelect {
    focus: FocusHandle,
    selection: Option<InputSelection>,
    /// A press in a top pane started a drag that the left button still holds.
    dragging: bool,
    /// The scroll of each autoscroll step while the drag holds the pointer past the panes'
    /// top or bottom edge.
    edge: Option<Pixels>,
}

/// What a press or drag over one display row of one pane needs to find its byte: the input,
/// the aligned row, where the row's part of the line starts in the drawn text, and the
/// layout of that part.
#[derive(Clone)]
pub(crate) struct CellHit {
    pub source: Source,
    pub row: RowIx,
    /// The byte of the tab-expanded line at which the display row starts.
    pub segment_start: usize,
    /// The drawn text's layout; `None` on a filler row.
    pub layout: Option<TextLayout>,
}

/// What a lookup of a pointer's byte needs from the window.
#[derive(Clone, Copy)]
pub(crate) struct Inputs<'a> {
    pub doc: &'a MergeDocument,
    pub rows: &'a RowMap,
    pub tab_width: usize,
}

impl InputSelect {
    pub(crate) fn new(focus: FocusHandle) -> Self {
        Self { focus, selection: None, dragging: false, edge: None }
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(crate) fn selection(&self) -> Option<InputSelection> {
        self.selection
    }

    /// End the selection, as when the panes lose the focus.
    pub(crate) fn clear(&mut self) {
        self.selection = None;
        self.release();
    }

    /// Whether a drag from a top pane holds the left button.
    pub(crate) fn dragging(&self) -> bool {
        self.dragging
    }

    /// The scroll of the next autoscroll step: some while a drag holds the pointer past an
    /// edge.
    pub(crate) fn edge(&self) -> Option<Pixels> {
        self.edge.filter(|_| self.dragging)
    }

    /// Hold the pointer past an edge with `step`, or between the edges with `None`.
    pub(crate) fn set_edge(&mut self, step: Option<Pixels>) {
        self.edge = step.filter(|_| self.dragging);
    }

    /// Move the end of a drag to row `row`: the start of its line, or with `end` the end of
    /// its text.
    pub(crate) fn drag_to_row(&mut self, inputs: Inputs<'_>, row: RowIx, end: bool) {
        let Some(held) = self.selection.filter(|_| self.dragging) else {
            return;
        };
        let source = held.source();
        let line = inputs.rows.line_at_row(inputs.doc, row, source);
        let column = if end { usize::MAX } else { 0 };
        let text = inputs.doc.inputs().text(source);
        self.selection = Some(held.with_head(selection::offset(text, line, column)));
    }

    /// End the selection when no pane in `shown` shows its input.
    pub(crate) fn keep_shown(&mut self, shown: &[Source]) {
        if self.selection.is_some_and(|s| !shown.contains(&s.source())) {
            self.clear();
        }
    }

    /// A left press over `hit`: a new selection, its end moved with Shift in the same pane,
    /// a word on a double press, a line on a triple one.
    pub(crate) fn press(&mut self, inputs: Inputs<'_>, hit: &CellHit, event: &MouseDownEvent) {
        let source = hit.source;
        let text = inputs.doc.inputs().text(source);
        let line = inputs.rows.line_at_row(inputs.doc, hit.row, source);
        let shifted = event.modifiers.shift;
        self.dragging = event.click_count < WORD_PRESSES;
        self.selection = Some(match (event.click_count, line) {
            (count, Ok(n)) if count >= LINE_PRESSES => {
                let bytes = selection::line_bytes(text, n);
                InputSelection::new(source, bytes.start, bytes.end)
            }
            (WORD_PRESSES, Ok(n)) => {
                let column = column(inputs, hit, n, event.position, Pick::Under);
                let word = selection::word_at(text.line(n), column);
                let start = selection::line_bytes(text, n).start;
                InputSelection::new(source, start + word.start, start + word.end)
            }
            _ => {
                let at = offset(inputs, hit, line, event.position);
                match self.selection {
                    Some(held) if shifted && held.source() == source => held.with_head(at),
                    _ => InputSelection::caret(source, at),
                }
            }
        });
    }

    /// A pointer move over `hit`: the end of a drag in the same pane follows it. A move
    /// without the left button ends the drag. Whether the selection changed.
    pub(crate) fn drag(
        &mut self,
        inputs: Inputs<'_>,
        hit: &CellHit,
        event: &MouseMoveEvent,
    ) -> bool {
        if event.pressed_button != Some(MouseButton::Left) {
            self.release();
            return false;
        }
        let Some(held) = self.selection.filter(|s| self.dragging && s.source() == hit.source)
        else {
            return false;
        };
        let line = inputs.rows.line_at_row(inputs.doc, hit.row, hit.source);
        let moved = held.with_head(offset(inputs, hit, line, event.position));
        self.selection = Some(moved);
        moved != held
    }

    pub(crate) fn release(&mut self) {
        self.dragging = false;
        self.edge = None;
    }

    /// Select the whole input of the selection's pane, else `first`'s.
    pub(crate) fn select_all(&mut self, doc: &MergeDocument, first: Source) {
        let source = self.selection.map_or(first, InputSelection::source);
        let len = doc.inputs().text(source).content().len();
        self.selection = Some(InputSelection::new(source, 0, len));
    }

    /// The selected text; `None` when nothing is selected.
    pub(crate) fn text<'a>(&self, doc: &'a MergeDocument) -> Option<&'a str> {
        let held = self.selection?;
        Some(held.text(doc.inputs().text(held.source()).content())).filter(|t| !t.is_empty())
    }

    /// The selected part of `source`'s line `line`, in bytes of the line.
    pub(crate) fn in_line(
        &self,
        doc: &MergeDocument,
        source: Source,
        line: usize,
    ) -> Option<Range<usize>> {
        let held = self.selection?;
        let text = doc.inputs().text(source);
        let start = selection::line_bytes(text, line).start;
        let len = text.line(line).trim_end_matches(['\n', '\r']).len();
        held.in_line(source, start..start + len)
    }
}

/// How a pointer picks a character boundary.
#[derive(Clone, Copy)]
enum Pick {
    /// The boundary nearest the pointer, for the ends of a selection.
    Nearest,
    /// The start of the character under the pointer, for the word a double press takes.
    Under,
}

/// The byte of the input `position` over `hit` names.
fn offset(
    inputs: Inputs<'_>,
    hit: &CellHit,
    line: Result<usize, usize>,
    position: Point<Pixels>,
) -> usize {
    let text = inputs.doc.inputs().text(hit.source);
    let column = line.map_or(0, |n| column(inputs, hit, n, position, Pick::Nearest));
    selection::offset(text, line, column)
}

/// The byte of line `line` that `position` over `hit` names. A row without a layout gives
/// the line's start.
fn column(
    inputs: Inputs<'_>,
    hit: &CellHit,
    line: usize,
    position: Point<Pixels>,
    pick: Pick,
) -> usize {
    let Some(layout) = &hit.layout else {
        return 0;
    };
    let text = inputs.doc.inputs().text(hit.source).line(line);
    let expanded = tabs::expand(text.trim_end_matches(['\n', '\r']), inputs.tab_width);
    expanded.line_offset(hit.segment_start + drawn_index(layout, position, pick))
}

/// The byte of `layout`'s one line of text at `position`'s x. Left of the text is 0; right
/// of it, the text's length.
fn drawn_index(layout: &TextLayout, position: Point<Pixels>, pick: Pick) -> usize {
    let Some(line) = layout.line_layout_for_index(0) else {
        return 0;
    };
    // One line, so only x counts: a pointer below the glyphs' height stays on it.
    let within = point(position.x - layout.bounds().origin.x, px(0.));
    let height = layout.line_height();
    let found = match pick {
        Pick::Nearest => line.closest_index_for_position(within, height),
        Pick::Under => line.index_for_position(within, height),
    };
    found.unwrap_or_else(|index| index)
}

/// How often a drag held past the top panes' edge scrolls them (spec §5).
pub(crate) const AUTOSCROLL_TICK: Duration = Duration::from_millis(16);
/// A step scrolls this share of the pointer's distance past the edge.
const AUTOSCROLL_GAIN: f32 = 0.5;
/// The least a step scrolls.
const AUTOSCROLL_MIN_STEP: Pixels = px(2.);
/// The most a step scrolls, in rows.
pub(crate) const AUTOSCROLL_MAX_ROWS: f32 = 2.;

/// The scroll of one tick for a pointer at `y` with the panes between `top` and `bottom`:
/// toward the pointer, by [`AUTOSCROLL_GAIN`] of its distance past the edge, from
/// [`AUTOSCROLL_MIN_STEP`] up to `most`; negative scrolls up. `None` between the edges.
pub(crate) fn autoscroll_step(
    y: Pixels,
    top: Pixels,
    bottom: Pixels,
    most: Pixels,
) -> Option<Pixels> {
    let past = if y < top {
        y - top
    } else if y > bottom {
        y - bottom
    } else {
        return None;
    };
    let step = (past.abs() * AUTOSCROLL_GAIN).max(AUTOSCROLL_MIN_STEP).min(most);
    Some(if past < Pixels::ZERO { -step } else { step })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOP: Pixels = px(100.);
    const BOTTOM: Pixels = px(500.);
    const MOST: Pixels = px(40.);

    fn step(y: f32) -> Option<Pixels> {
        autoscroll_step(px(y), TOP, BOTTOM, MOST)
    }

    #[test]
    fn between_the_edges_nothing_scrolls() {
        assert_eq!(step(100.), None);
        assert_eq!(step(300.), None);
        assert_eq!(step(500.), None);
    }

    #[test]
    fn below_the_bottom_a_step_is_half_the_distance_down() {
        assert_eq!(step(530.), Some(px(15.)));
        assert_eq!(step(520.), Some(px(10.)));
    }

    #[test]
    fn above_the_top_a_step_is_half_the_distance_up() {
        assert_eq!(step(70.), Some(px(-15.)));
    }

    #[test]
    fn a_step_takes_at_least_the_least_and_at_most_the_most() {
        assert_eq!(step(501.), Some(px(2.)));
        assert_eq!(step(99.), Some(px(-2.)));
        assert_eq!(step(700.), Some(px(40.)));
        assert_eq!(step(-300.), Some(px(-40.)));
    }
}
