//! Selecting and copying text in the input panes.

use gpui_kit::{ClipboardItem, Context, MouseDownEvent, MouseMoveEvent, Window};
use merge_view::selection::InputSelection;

use crate::input_select::{self, CellHit};
use crate::{CopyInput, SelectAllInput};

use super::MergeWindow;

impl MergeWindow {
    /// The top panes' selection, in its input's bytes.
    #[must_use]
    pub fn input_selection(&self) -> Option<InputSelection> {
        self.input_select.selection()
    }

    pub(super) fn press_input(
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
            drawing: self.drawing(),
        };
        self.input_select.press(inputs, hit, event);
        tracing::debug!(selection = ?self.input_select.selection(), "top pane press");
        cx.notify();
    }

    pub(super) fn drag_input(
        &mut self,
        hit: &CellHit,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let inputs = input_select::Inputs {
            doc: self.session.document(),
            rows: &self.rows,
            drawing: self.drawing(),
        };
        if self.input_select.drag(inputs, hit, event) {
            cx.notify();
        }
    }

    /// ⌘C in the top panes: copy their selection; with nothing selected, copy nothing.
    pub(super) fn copy_input(&mut self, _: &CopyInput, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_select.text(self.session.document());
        tracing::debug!(selection = ?self.input_select.selection(), bytes = text.map(str::len), "top pane copy");
        if let Some(text) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
    }

    /// ⌘A in the top panes: select the whole input of the selection's pane, else the first.
    pub(super) fn select_all_input(
        &mut self,
        _: &SelectAllInput,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(&first) = self.panes.sources().first() else {
            return;
        };
        self.input_select.select_all(self.session.document(), first);
        cx.notify();
    }
}
