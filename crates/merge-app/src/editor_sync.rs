//! The bridge between an editor and the core text it shows: the user's edits go to the core
//! as one batch, and the core's patches come back to the editor unrecorded. The merge
//! window's result and each pane of the diff window use it.

use gpui_kit::base::input::RopeExt as _;
use gpui_kit::component::input::EditorState;
use gpui_kit::{App, Entity, EntityInputHandler as _, Window};
use merge_core::document::{EditBatch, TextPatch};

use crate::patches::{difference, end_of_last, map_offset};
use crate::recorder::Recorded;

/// Where the caret goes after the core's patches reach the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Caret {
    /// Stay put, moving with the text around it: for corrections to the user's edit.
    Follow,
    /// The end of the last change: for undo, redo and picks.
    AtChange,
}

/// The user's edits to `editor` since the last call, as one batch against the core's text.
/// One recorded batch goes as it is. Several become the one patch between `core()` and the
/// editor's text, since only the first is in the core's offsets. None without edits.
pub(crate) fn take_batch(
    editor: &Entity<EditorState>,
    recorded: &Recorded,
    core: impl FnOnce() -> String,
    cx: &App,
) -> Option<EditBatch> {
    let mut batches = recorded.take();
    if batches.len() > 1 {
        let editor = editor.read(cx).text().to_string();
        Some(EditBatch(difference(&core(), &editor).into_iter().collect()))
    } else {
        batches.pop()
    }
}

/// Apply the core's `patches` to `editor` without recording them, and place the caret.
pub(crate) fn apply_patches(
    editor: &Entity<EditorState>,
    recorded: &Recorded,
    patches: &[TextPatch],
    caret: Caret,
    window: &mut Window,
    cx: &mut App,
) {
    if patches.is_empty() {
        return;
    }
    recorded.muted(|| {
        editor.update(cx, |state, cx| {
            let before = state.cursor();
            // From the end back, so earlier offsets stay valid.
            for patch in patches.iter().rev() {
                let text = state.text();
                let range = text.offset_to_offset_utf16(patch.range.start)
                    ..text.offset_to_offset_utf16(patch.range.end);
                state.replace_text_in_range(Some(range), &patch.text, window, cx);
            }
            let at = match caret {
                Caret::Follow => map_offset(before, patches),
                Caret::AtChange => end_of_last(patches).unwrap_or(before),
            };
            state.set_selected_range(at..at, cx);
        });
    });
}
