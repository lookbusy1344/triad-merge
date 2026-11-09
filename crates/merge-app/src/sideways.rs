//! The top panes' sideways scroll (spec §5): one offset for every pane, held between the left
//! edge and the end of the widest line a pane shows. The panes' text widths it records also
//! set where their rows wrap.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{Bounds, IntoElement, Pixels, Styled as _, Window, canvas, px};
use merge_core::merge3::Source;
use merge_core::text::Text;
use merge_view::scroll::clamp_left;
use merge_view::tabs;

/// The character whose advance sets a column's width. The panes draw in a monospace font.
const ADVANCE_CHAR: char = 'm';

/// The widest line of each input, in columns, tabs expanded.
pub(crate) type Widest = Vec<(Source, usize)>;

/// Count the widest line of each of `texts`. Runs off the UI thread: it reads every line.
pub(crate) fn widest(texts: &[(Source, Text)], tab_width: usize) -> Widest {
    texts.iter().map(|(source, text)| (*source, tabs::widest(text.lines(), tab_width))).collect()
}

#[derive(Default)]
pub(crate) struct Sideways {
    /// How far the panes' text is scrolled right.
    left: Pixels,
    /// Each input's widest line; `None` until counted.
    widest: Option<Widest>,
    /// Each pane's text width, as the first row on screen laid it out in the last frame.
    views: Rc<RefCell<Vec<Pixels>>>,
    /// The width of one column; `None` until measured.
    advance: Option<Pixels>,
}

impl Sideways {
    /// Panes measured as a frame would: `widest` columns per input, `advance` per column,
    /// and each pane's text width in `views`.
    #[cfg(test)]
    pub(crate) fn measured(widest: Widest, advance: Pixels, views: Vec<Pixels>) -> Self {
        Self {
            left: Pixels::ZERO,
            widest: Some(widest),
            views: Rc::new(RefCell::new(views)),
            advance: Some(advance),
        }
    }

    pub(crate) fn left(&self) -> Pixels {
        self.left
    }

    /// Each pane's text width, as the last frame laid it out.
    pub(crate) fn views(&self) -> std::cell::Ref<'_, Vec<Pixels>> {
        self.views.borrow()
    }

    pub(crate) fn set_widest(&mut self, widest: Widest) {
        self.widest = Some(widest);
    }

    /// `left` held within the range of the panes that show `sources`, in order.
    pub(crate) fn clamp(&self, left: Pixels, sources: &[Source]) -> Pixels {
        let (Some(widest), Some(advance)) = (&self.widest, self.advance) else {
            return Pixels::ZERO;
        };
        let views = self.views.borrow();
        let columns = |source: &Source| {
            widest.iter().find(|(s, _)| s == source).map_or(0, |&(_, columns)| columns)
        };
        sources
            .iter()
            .zip(views.iter())
            .map(|(source, view)| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "past 2^24 columns the end is off by less than a column"
                )]
                let content = f32::from(advance) * columns(source) as f32;
                px(clamp_left(f32::from(left), content, f32::from(*view)))
            })
            .max()
            .unwrap_or(Pixels::ZERO)
    }

    /// Move the panes to `left`, held within their range. Whether they moved.
    pub(crate) fn scroll_to(&mut self, left: Pixels, sources: &[Source]) -> bool {
        let held = self.clamp(left, sources);
        let moved = held != self.left;
        self.left = held;
        moved
    }

    /// Measure a column in the panes' font, once.
    pub(crate) fn measure(&mut self, font: &gpui_kit::Font, size: Pixels, window: &Window) {
        if self.advance.is_some() {
            return;
        }
        let text = window.text_system();
        match text.advance(text.resolve_font(font), size, ADVANCE_CHAR) {
            Ok(advance) => self.advance = Some(advance.width),
            Err(error) => {
                // Without a column width the panes stay at the left edge.
                tracing::warn!(%error, "cannot measure the panes' font; no sideways scroll");
            }
        }
    }

    /// An invisible element over pane `pane`'s text that records its width as it lays out.
    pub(crate) fn probe(&self, pane: usize) -> impl IntoElement + use<> {
        let views = Rc::clone(&self.views);
        canvas(
            move |bounds: Bounds<Pixels>, _, _| {
                let mut views = views.borrow_mut();
                if views.len() <= pane {
                    views.resize(pane + 1, Pixels::ZERO);
                }
                views[pane] = bounds.size.width;
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_full()
    }
}
