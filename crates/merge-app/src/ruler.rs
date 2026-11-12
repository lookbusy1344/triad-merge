//! The overview ruler's strip (`docs/DESIGN.md` §E4): the marks of
//! [`merge_view::ruler`], painted down a narrow strip, with the pane's vertical scrollbar
//! thumb over them. A click on a mark goes to its region; a click elsewhere scrolls there; a
//! drag of the thumb scrolls the pane.

use merge_core::index::RegionIx;
use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    App, Bounds, Hsla, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Styled as _, TestSupportExt as _,
    Window, canvas, div, fill, point, px, size,
};
use merge_view::marks::Tone;
use merge_view::ruler::{RulerMark, Style, drag_offset, line_at, mark_at, painted, thumb};

use crate::scrollbars::Feed;
use crate::tones::Tones;

/// The strip's width.
pub(crate) const RULER_WIDTH_PX: f32 = 12.;
/// The shortest a mark paints, so a one-line region shows in a long file.
pub(crate) const RULER_MIN_MARK_PX: f32 = 2.;
/// The shortest the thumb paints, so a long file leaves it something to grab.
const MIN_THUMB_PX: f32 = 16.;
/// How much of the foreground the thumb takes.
const THUMB_OPACITY: f32 = 0.18;
/// How much of the foreground the thumb takes while dragged.
const DRAGGED_THUMB_OPACITY: f32 = 0.3;
/// A resolved conflict paints the conflict tone over this share of the width.
const RESOLVED_SHARE: f32 = 0.5;

/// What a click on the strip asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Click {
    /// Make the region current and scroll to it.
    Region(RegionIx),
    /// Scroll the line into view.
    Line(usize),
}

/// What the strip shows: the marks, the lines they share, and the pane's scroll, whose thumb
/// is the pane's vertical scrollbar.
pub(crate) struct Strip {
    pub(crate) marks: Rc<[RulerMark]>,
    pub(crate) total: usize,
    pub(crate) feed: Feed,
}

/// A press on the thumb, held until the button goes up.
#[derive(Debug, Clone, Copy)]
struct Grab {
    /// How far down the thumb it took hold.
    at: f32,
    /// Where in the window the button went down.
    from: Pixels,
    moved: bool,
    /// What letting go without a move asks for.
    click: Option<Click>,
}

/// The thumb's drag, kept from frame to frame.
#[derive(Clone, Default)]
pub(crate) struct Dragging(Rc<Cell<Option<Grab>>>);

/// The strip, `id`ed for tests. A left click calls `on_click` with what it asks for; a drag
/// of the thumb calls `on_scroll` with the pane's new scroll offset.
pub(crate) fn strip(
    id: &'static str,
    shown: Strip,
    dragging: Dragging,
    tones: Tones,
    foreground: Hsla,
    on_click: impl Fn(Click, &mut Window, &mut App) + 'static,
    on_scroll: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let Strip { marks, total, feed } = shown;
    let on_click = Rc::new(on_click);
    let laid_out: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
    let press = {
        let (marks, feed, at, dragging, on_click) = (
            Rc::clone(&marks),
            feed.clone(),
            Rc::clone(&laid_out),
            dragging.clone(),
            Rc::clone(&on_click),
        );
        move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
            let Some(bounds) = at.get() else {
                return;
            };
            let height = f32::from(bounds.size.height);
            let y = f32::from(event.position.y - bounds.origin.y);
            let mark = mark_at(&marks, y, total, height, RULER_MIN_MARK_PX).map(Click::Region);
            let Some(scrolled) = feed.scrolled() else {
                on_click(mark.unwrap_or(Click::Line(line_at(y, total, height))), window, cx);
                return;
            };
            let held = thumb(scrolled, height, MIN_THUMB_PX);
            if held.contains(&y) {
                let at = y - held.start;
                let from = event.position.y;
                dragging.0.set(Some(Grab { at, from, moved: false, click: mark }));
                // The thumb takes its dragged tint.
                window.refresh();
            } else if let Some(mark) = mark {
                on_click(mark, window, cx);
            } else {
                on_click(Click::Line(line_at(y, total, height)), window, cx);
                // The thumb now centres on the press; a drag goes on from there.
                let at = (held.end - held.start) / 2.;
                let from = event.position.y;
                dragging.0.set(Some(Grab { at, from, moved: true, click: None }));
            }
        }
    };
    div()
        .id(id)
        .test_support()
        .flex_none()
        .w(px(RULER_WIDTH_PX))
        .h_full()
        .on_mouse_down(MouseButton::Left, press)
        .child(
            canvas(
                {
                    let laid_out = Rc::clone(&laid_out);
                    move |bounds, _, _| laid_out.set(Some(bounds))
                },
                move |bounds, (), window, _| {
                    let height = f32::from(bounds.size.height);
                    let scrolled = feed.scrolled();
                    let held = scrolled.map(|scrolled| thumb(scrolled, height, MIN_THUMB_PX));
                    let dragged = dragging.0.get().is_some();
                    paint(window, bounds, &marks, total, held, dragged, &tones, foreground);
                    window.on_mouse_event({
                        let dragging = dragging.clone();
                        move |event: &MouseMoveEvent, _, window, cx| {
                            let Some(mut grab) = dragging.0.get() else {
                                return;
                            };
                            if event.pressed_button != Some(MouseButton::Left) {
                                // The button went up where the strip did not see it.
                                dragging.0.set(None);
                                return;
                            }
                            let Some(scrolled) = scrolled else {
                                return;
                            };
                            grab.moved |= event.position.y != grab.from;
                            dragging.0.set(Some(grab));
                            if grab.moved {
                                let y = f32::from(event.position.y - bounds.origin.y);
                                let top = drag_offset(y - grab.at, scrolled, height, MIN_THUMB_PX);
                                on_scroll(top, window, cx);
                            }
                        }
                    });
                    window.on_mouse_event({
                        let (dragging, on_click) = (dragging.clone(), Rc::clone(&on_click));
                        move |event: &MouseUpEvent, _, window, cx| {
                            if event.button != MouseButton::Left {
                                return;
                            }
                            let Some(grab) = dragging.0.take() else {
                                return;
                            };
                            if let (false, Some(click)) = (grab.moved, grab.click) {
                                on_click(click, window, cx);
                            }
                            window.refresh();
                        }
                    });
                },
            )
            .size_full(),
        )
}

#[expect(clippy::too_many_arguments, reason = "the strip's parts, borrowed for one paint")]
fn paint(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    marks: &[RulerMark],
    total: usize,
    held: Option<std::ops::Range<f32>>,
    dragged: bool,
    tones: &Tones,
    foreground: Hsla,
) {
    let height = f32::from(bounds.size.height);
    let width = bounds.size.width;
    let band = |from: f32, to: f32, left: Pixels, wide: Pixels| {
        Bounds::new(
            point(bounds.origin.x + left, bounds.origin.y + px(from)),
            size(wide, px(to - from)),
        )
    };
    for run in painted(marks, total, height, RULER_MIN_MARK_PX) {
        let (from, to) = (run.px.start, run.px.end);
        match run.style {
            Style::Plain(tone) => {
                window.paint_quad(fill(band(from, to, px(0.), width), tones.mark(tone)));
            }
            Style::Unresolved => {
                window.paint_quad(fill(band(from, to, px(0.), width), tones.mark(Tone::Conflict)));
            }
            Style::Resolved(tone) => {
                let split = width * RESOLVED_SHARE;
                window.paint_quad(fill(band(from, to, px(0.), split), tones.mark(Tone::Conflict)));
                window.paint_quad(fill(band(from, to, split, width - split), tones.mark(tone)));
            }
        }
    }
    if let Some(held) = held {
        let mut tint = foreground;
        tint.a *= if dragged { DRAGGED_THUMB_OPACITY } else { THUMB_OPACITY };
        window.paint_quad(fill(band(held.start, held.end, px(0.), width), tint));
    }
}
