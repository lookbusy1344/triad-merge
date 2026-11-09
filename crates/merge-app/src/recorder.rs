//! Turns the result editor's edits into core edit batches (spec §4.5).
//!
//! The editor reports each user action to its edit observer: one edit, or several applied as
//! one change from the highest offset down. Applied in that order, every edit's range is in
//! the text before the action, so the batch needs no offset arithmetic.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::base::input::EditObserver;
use gpui_kit::component::input::{InputEdit, Rope};
use merge_core::document::{EditBatch, TextPatch};

/// Batches the editor made since the view last took them, oldest first.
#[derive(Clone, Default)]
pub(crate) struct Recorded {
    batches: Rc<RefCell<Vec<EditBatch>>>,
    /// Set while the view applies the core's patches, which are not user edits.
    muted: Rc<Cell<bool>>,
}

impl Recorded {
    pub(crate) fn take(&self) -> Vec<EditBatch> {
        std::mem::take(&mut *self.batches.borrow_mut())
    }

    /// Whether user edits wait to go to the core, which then lags the editor's text.
    pub(crate) fn pending(&self) -> bool {
        !self.batches.borrow().is_empty()
    }

    /// Run `f` with recording off.
    pub(crate) fn muted<T>(&self, f: impl FnOnce() -> T) -> T {
        let was = self.muted.replace(true);
        let out = f();
        self.muted.set(was);
        out
    }

    fn push(&self, batch: EditBatch) {
        if !self.muted.get() && !batch.0.is_empty() {
            self.batches.borrow_mut().push(batch);
        }
    }

    /// The editor's edit observer: each change it reports becomes one batch.
    pub(crate) fn observer(&self) -> EditObserver {
        let recorded = self.clone();
        Rc::new(move |edits: &[(InputEdit, Rope)]| {
            recorded.push(batch(edits.iter().map(|(edit, text)| (edit, text))));
        })
    }
}

/// One action's edits, each with the text just after it, from the highest offset down.
/// Empty edits are dropped.
fn batch<'a>(edits: impl DoubleEndedIterator<Item = (&'a InputEdit, &'a Rope)>) -> EditBatch {
    EditBatch(
        edits
            .rev()
            .filter(|(e, _)| e.start_byte != e.old_end_byte || e.start_byte != e.new_end_byte)
            .map(|(e, text)| TextPatch {
                range: e.start_byte..e.old_end_byte,
                text: text.slice(e.start_byte..e.new_end_byte).to_string(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use gpui_kit::component::input::Point;

    use super::*;

    fn edit(start: usize, old_end: usize, new_end: usize) -> InputEdit {
        let origin = Point::new(0, 0);
        InputEdit {
            start_byte: start,
            old_end_byte: old_end,
            new_end_byte: new_end,
            start_position: origin,
            old_end_position: origin,
            new_end_position: origin,
        }
    }

    fn patch(range: Range<usize>, text: &str) -> TextPatch {
        TextPatch { range, text: text.to_owned() }
    }

    #[test]
    fn one_edit_takes_its_text_from_the_text_after_it() {
        // "abcd" with "bc" replaced by "XYZ".
        let after = Rope::from("aXYZd");
        let e = edit(1, 3, 4);
        assert_eq!(batch([(&e, &after)].into_iter()), EditBatch(vec![patch(1..3, "XYZ")]));
    }

    #[test]
    fn several_edits_come_out_in_ascending_order() {
        // "a\nb\nc\n" with "x" typed at 4, then 2, then 0, as three cursors do.
        let texts =
            [Rope::from("a\nb\nxc\n"), Rope::from("a\nxb\nxc\n"), Rope::from("xa\nxb\nxc\n")];
        let edits = [edit(4, 4, 5), edit(2, 2, 3), edit(0, 0, 1)];
        assert_eq!(
            batch(edits.iter().zip(&texts)),
            EditBatch(vec![patch(0..0, "x"), patch(2..2, "x"), patch(4..4, "x")])
        );
    }

    #[test]
    fn deletions_have_empty_text() {
        let after = Rope::from("ad");
        let e = edit(1, 3, 1);
        assert_eq!(batch([(&e, &after)].into_iter()), EditBatch(vec![patch(1..3, "")]));
    }

    #[test]
    fn empty_edits_are_dropped() {
        let text = Rope::from("abc");
        let e = edit(2, 2, 2);
        assert_eq!(batch([(&e, &text)].into_iter()), EditBatch(Vec::new()));
    }

    #[test]
    fn muted_and_empty_batches_are_not_recorded() {
        let recorded = Recorded::default();
        recorded.push(EditBatch(Vec::new()));
        recorded.muted(|| recorded.push(EditBatch(vec![patch(0..0, "x")])));
        assert_eq!(recorded.take(), Vec::<EditBatch>::new());
        recorded.push(EditBatch(vec![patch(0..0, "y")]));
        assert_eq!(recorded.take(), vec![EditBatch(vec![patch(0..0, "y")])]);
        assert_eq!(recorded.take(), Vec::<EditBatch>::new(), "take empties the queue");
    }

    #[test]
    fn the_observer_records_one_change_as_one_batch() {
        let recorded = Recorded::default();
        let observer = recorded.observer();
        let texts = [Rope::from("a\nxb\n"), Rope::from("xa\nxb\n")];
        observer(&[(edit(2, 2, 3), texts[0].clone()), (edit(0, 0, 1), texts[1].clone())]);
        observer(&[(edit(1, 2, 1), Rope::from("x\nxb\n"))]);
        assert_eq!(
            recorded.take(),
            vec![
                EditBatch(vec![patch(0..0, "x"), patch(2..2, "x")]),
                EditBatch(vec![patch(1..2, "")]),
            ]
        );
    }

    #[test]
    fn the_observer_records_nothing_while_muted() {
        let recorded = Recorded::default();
        let observer = recorded.observer();
        recorded.muted(|| observer(&[(edit(0, 0, 1), Rope::from("x"))]));
        assert_eq!(recorded.take(), Vec::<EditBatch>::new());
    }

    #[test]
    fn muting_nests() {
        let recorded = Recorded::default();
        recorded.muted(|| {
            recorded.muted(|| {});
            recorded.push(EditBatch(vec![patch(0..0, "x")]));
        });
        assert_eq!(recorded.take(), Vec::<EditBatch>::new());
    }
}
