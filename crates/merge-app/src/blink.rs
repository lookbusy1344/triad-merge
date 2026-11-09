//! The blink after a step: a jump to a region or a difference, or a step through search
//! matches (spec §5, §6.3). [`Flash`] runs the blink of every window; its state is
//! [`merge_view::blink::Blink`]. [`SearchSteps`] tells a step through an editor's matches from
//! a new search.

use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::input::SearchSession;
use gpui_kit::component::input::{RangeDecoration, RangeDecorationStyle};
use gpui_kit::{Context, Hsla, Pixels, Task, px, transparent_black};
use merge_view::blink::{AtRest, Blink, STEPS};

use crate::ignore_closed_view;

/// How long each step of a blink lasts: 500 ms for the blink.
pub(crate) const STEP: Duration = Duration::from_millis(100);
/// The width of a jump's frame around the current region.
pub(crate) const BORDER: Pixels = px(2.);

/// A blink of a target `T` and the timers that step it.
pub(crate) struct Flash<T> {
    blink: Blink<T>,
    /// One timer per step after the first; replacing them cancels a running blink.
    timers: Vec<Task<()>>,
}

impl<T> Default for Flash<T> {
    fn default() -> Self {
        Self { blink: Blink::default(), timers: Vec::new() }
    }
}

impl<T: Copy + 'static> Flash<T> {
    /// Blink `target` in `view`, in place of any running blink. `flash` finds this flash in
    /// the view, and `repaint` paints the view's targets: now, at step 0, and as each timer
    /// fires, step `n` `n * STEP` in. The last timer ends the blink.
    pub(crate) fn start<V: 'static>(
        view: &mut V,
        target: T,
        flash: fn(&mut V) -> &mut Self,
        repaint: fn(&mut V, &mut Context<V>),
        cx: &mut Context<V>,
    ) {
        let timers = (1..=STEPS)
            .map(|step| {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(STEP * step).await;
                    ignore_closed_view(this.update(cx, |this, cx| {
                        flash(this).blink.step((step < STEPS).then_some(step));
                        repaint(this, cx);
                    }));
                })
            })
            .collect();
        let this = flash(view);
        this.blink.start(target);
        this.timers = timers;
        repaint(view, cx);
    }

    #[must_use]
    pub(crate) fn running(&self) -> bool {
        self.blink.running()
    }

    /// Whether the target shows now, for a target that shows `at_rest`.
    #[must_use]
    pub(crate) fn shows(&self, at_rest: AtRest) -> bool {
        self.blink.shows(at_rest)
    }

    /// The target of the running blink while it shows, for a target hidden at rest.
    #[must_use]
    pub(crate) fn lit(&self) -> Option<T> {
        self.blink.target().filter(|_| self.blink.shows(AtRest::Hidden))
    }
}

/// The frame a jump blinks around a region's lines.
pub(crate) fn region_frame(bytes: Range<usize>, colour: Hsla) -> RangeDecoration {
    RangeDecoration::new(bytes)
        .with_style(RangeDecorationStyle::FullLine)
        .with_color(transparent_black())
        .with_border(colour)
        .with_border_width(BORDER)
}

/// The fill a search step blinks over the match it reached in an editor, as an input pane
/// fills its current match in that colour. The editor paints it under its translucent
/// search fill, so it shows through.
pub(crate) fn match_flash(bytes: Range<usize>, colour: Hsla) -> RangeDecoration {
    RangeDecoration::new(bytes).with_style(RangeDecorationStyle::Fill).with_color(colour)
}

/// An editor's search matches, shared with its matcher until it finds them again.
type Matches = Rc<Vec<Range<usize>>>;

/// Tells the steps an editor's search takes from the rest of its changes. A step moves the
/// current match over the same matches; typing a query or editing the text finds the matches
/// again, which is no step.
#[derive(Default)]
pub(crate) struct SearchSteps {
    /// The matches last seen, shared with the matcher until it finds them again, and the
    /// current one among them.
    seen: Option<(Matches, Option<usize>)>,
}

impl SearchSteps {
    /// The match `session` stepped to since the last call, if it stepped.
    pub(crate) fn stepped(&mut self, session: &SearchSession) -> Option<Range<usize>> {
        if !session.is_active() {
            self.seen = None;
            return None;
        }
        let matches = session.matcher.matched_ranges();
        let current = session.matcher.current();
        let stepped = self
            .seen
            .as_ref()
            .is_some_and(|(seen, was)| Rc::ptr_eq(seen, &matches) && *was != current);
        let reached = current.filter(|_| stepped).and_then(|ix| matches.get(ix).cloned());
        self.seen = Some((matches, current));
        reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::hsla;

    #[test]
    fn a_match_blinks_as_a_fill_of_its_bytes_in_the_given_colour() {
        let lime = hsla(0.25, 0.85, 0.4, 1.);
        let flash = match_flash(2..5, lime);
        assert_eq!(flash.range(), &(2..5));
        assert_eq!(flash.style(), RangeDecorationStyle::Fill);
        assert_eq!(flash.color(), Some(lime));
    }
}
