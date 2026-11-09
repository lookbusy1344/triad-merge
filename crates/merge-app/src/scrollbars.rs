//! The panes' scrollbars (spec §5, §6.4): when they show, and which bars each editor paints.
//! The result and the diff window's right pane take their vertical bar in the overview ruler,
//! which reads their scroll from a [`Feed`].

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::input::EditorScrollbarContext;
use gpui_kit::base::{Scrollbar, ScrollbarAxis, ScrollbarHandle as _, ScrollbarMode};
use gpui_kit::component::input::EditorState;
use gpui_kit::{IntoElement as _, div};
use merge_config::settings::Scrollbars;
use merge_view::ruler::Scrolled;

/// How gpui shows a bar the `scrollbars` setting names.
pub(crate) fn mode(shown: Scrollbars) -> ScrollbarMode {
    match shown {
        Scrollbars::Always => ScrollbarMode::Always,
        Scrollbars::Hover => ScrollbarMode::Hover,
        Scrollbars::WhenScrolling => ScrollbarMode::Scrolling,
    }
}

/// The bars an editor paints of those it `offers`: all of them, or with a ruler beside it,
/// all but the vertical, which the ruler is.
fn painted(offers: ScrollbarAxis, ruler: bool) -> Option<ScrollbarAxis> {
    match (offers, ruler) {
        (axis, false) => Some(axis),
        (ScrollbarAxis::Both | ScrollbarAxis::Horizontal, true) => Some(ScrollbarAxis::Horizontal),
        (ScrollbarAxis::Vertical, true) => None,
    }
}

/// An editor's vertical scroll as its last frame laid it out, for the ruler's thumb.
#[derive(Clone, Default)]
pub(crate) struct Feed(Rc<Cell<Option<Scrolled>>>);

impl Feed {
    /// The scroll; none before the editor's first frame.
    pub(crate) fn scrolled(&self) -> Option<Scrolled> {
        self.0.get()
    }

    fn record(&self, context: &EditorScrollbarContext) {
        self.0.set(Some(Scrolled {
            offset: -f32::from(context.offset().y),
            viewport: f32::from(context.viewport_bounds().size.height),
            content: f32::from(context.content_size().height),
        }));
    }
}

/// Paint `state`'s bars in `mode`. With a `ruler`, the ruler is the vertical bar: the editor
/// paints only its sideways one and records its scroll in the ruler's feed.
pub(crate) fn install(state: &mut EditorState, mode: ScrollbarMode, ruler: Option<Feed>) {
    state.set_scrollbar_renderer(Some(Rc::new(move |context, _, _| {
        if let Some(feed) = &ruler {
            feed.record(&context);
        }
        match painted(context.axis(), ruler.is_some()) {
            Some(axis) => Scrollbar::new(&context).axis(axis).mode(mode).into_any_element(),
            None => div().into_any_element(),
        }
    })));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_setting_names_its_mode() {
        assert_eq!(mode(Scrollbars::Always), ScrollbarMode::Always);
        assert_eq!(mode(Scrollbars::Hover), ScrollbarMode::Hover);
        assert_eq!(mode(Scrollbars::WhenScrolling), ScrollbarMode::Scrolling);
    }

    #[test]
    fn an_editor_without_a_ruler_paints_every_bar_it_offers() {
        assert_eq!(painted(ScrollbarAxis::Both, false), Some(ScrollbarAxis::Both));
        assert_eq!(painted(ScrollbarAxis::Vertical, false), Some(ScrollbarAxis::Vertical));
    }

    #[test]
    fn an_editor_beside_a_ruler_paints_only_its_sideways_bar() {
        assert_eq!(painted(ScrollbarAxis::Both, true), Some(ScrollbarAxis::Horizontal));
        // Wrapped lines offer no sideways bar.
        assert_eq!(painted(ScrollbarAxis::Vertical, true), None);
    }
}
