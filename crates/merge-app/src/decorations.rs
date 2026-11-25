//! The parts of a window's frame triad draws itself where the compositor draws none: Linux under
//! client-side decorations, as GNOME on Wayland. gpui-component's `window_border` then pads
//! every window by a shadow and a border, and a window without a toolbar takes a title bar
//! here. The window grows by what they take, so the content keeps the size the window opened
//! at. Where the system draws the frame, both parts draw nothing and change no size.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, TITLE_BAR_HEIGHT, TitleBar, v_flex};
use gpui_kit::{
    AnyElement, App, Bounds, Decorations, Div, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Role, SharedString, Size, Stateful,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, canvas, div, size,
};

/// The title bar of a window without a toolbar.
pub const TITLE_BAR_ID: &str = "window-title-bar";

/// Whether triad draws the window's frame: client-side decorations.
fn app_drawn(window: &Window) -> bool {
    matches!(window.window_decorations(), Decorations::Client { .. })
}

/// The window's new size when the chrome, what the frame and title bar take, changed from
/// `last` to `chrome`: the window grows or shrinks by the change, so the content keeps its
/// size. None while the chrome stays, so a size the user or the platform picks stays.
fn refit(last: Size<Pixels>, chrome: Size<Pixels>, viewport: Size<Pixels>) -> Option<Size<Pixels>> {
    (chrome != last).then(|| {
        size(
            viewport.width + chrome.width - last.width,
            viewport.height + chrome.height - last.height,
        )
    })
}

/// Keeps a window's content at the size the window opened at, across the chrome triad draws:
/// at open, and each time the chrome changes, as when a Wayland compositor grants server-side
/// decorations after the first frame. The window's view holds it.
#[derive(Debug, Default)]
pub(crate) struct Fit {
    /// The chrome last fitted; none at open, when the window's size is all content.
    chrome: Rc<Cell<Size<Pixels>>>,
}

impl Fit {
    /// An invisible element over the window's root, which takes all of the window but the
    /// frame, and `title` of it the title bar.
    pub(crate) fn probe(&self, title: Pixels) -> impl IntoElement + use<> {
        let last = Rc::clone(&self.chrome);
        canvas(
            move |bounds: Bounds<Pixels>, window: &mut Window, _| {
                let viewport = window.viewport_size();
                let chrome = size(
                    viewport.width - bounds.size.width,
                    viewport.height - bounds.size.height + title,
                );
                if let Some(fits) = refit(last.get(), chrome, viewport) {
                    window.resize(fits);
                    last.set(chrome);
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_full()
    }
}

/// The title bar of a window without a toolbar, where triad draws the frame: `title` and the
/// window's controls, whose close button runs `close`. None where the system draws the frame
/// and its own title bar.
pub(crate) fn title_bar(
    window: &Window,
    title: SharedString,
    close: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Option<AnyElement> {
    app_drawn(window).then(|| {
        TitleBar::new()
            .on_close_window(move |_, window, cx| close(window, cx))
            .child(
                div()
                    .id(TITLE_BAR_ID)
                    .test_support()
                    .role(Role::Label)
                    .aria_label(title.clone())
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().foreground)
                    .child(title),
            )
            .into_any_element()
    })
}

/// The root of a window without a toolbar: it fills the window, holds the title bar where
/// triad draws the frame, and fits the window so that the rest keeps the size the window
/// opened at.
pub(crate) fn plain_root(
    id: &'static str,
    fit: &Fit,
    title: SharedString,
    close: impl Fn(&mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    let bar = title_bar(window, title, close, cx);
    let height = if bar.is_some() { TITLE_BAR_HEIGHT } else { Pixels::ZERO };
    v_flex().id(id).size_full().child(fit.probe(height)).children(bar)
}

#[cfg(test)]
mod tests {
    use gpui_kit::px;

    use super::*;

    /// gpui-component's frame under client-side decorations: a 20 px shadow and a 1 px border
    /// on each side.
    const FRAME: f32 = 2. * 21.;
    /// The title bar a window without a toolbar takes there.
    const TITLE: f32 = 34.;
    const OPENED: Size<Pixels> = Size { width: px(360.), height: px(170.) };
    const NONE: Size<Pixels> = Size { width: px(0.), height: px(0.) };

    fn framed(title: f32) -> Size<Pixels> {
        size(px(FRAME), px(FRAME + title))
    }

    #[test]
    fn the_first_frame_triad_draws_grows_the_window_by_the_frame_and_title_bar() {
        assert_eq!(refit(NONE, framed(TITLE), OPENED), Some(size(px(402.), px(246.))));
    }

    #[test]
    fn a_window_the_system_frames_keeps_its_size() {
        assert_eq!(refit(NONE, NONE, OPENED), None);
    }

    /// The user's resize, or the platform's clamp to the screen, changes the window and its
    /// content alike: the chrome stays, and so does the size.
    #[test]
    fn a_size_picked_after_the_fit_stays() {
        assert_eq!(refit(framed(TITLE), framed(TITLE), size(px(300.), px(150.))), None);
    }

    /// A Wayland compositor may grant server-side decorations after the first frame: the
    /// window gives back what the frame and title bar took.
    #[test]
    fn chrome_that_goes_gives_its_room_back() {
        assert_eq!(refit(framed(TITLE), NONE, size(px(402.), px(246.))), Some(OPENED));
    }
}
