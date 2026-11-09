//! The About window (spec §6.3): the logo, the version, the commit of a release build, what
//! triad does and the version control systems it serves.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, FocusHandle, Global, Hsla,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Pixels, Render, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    WindowBounds, WindowOptions, div, px, size,
};

use crate::{logo, strings};

const WINDOW_WIDTH: Pixels = px(360.);
/// 16 padding, the 56 logo, 16 below it, the version's 26 line (16 px text at the theme's
/// line height of 1.618), 8 gap, the description's two 16 lines, 16 padding.
const WINDOW_HEIGHT: Pixels = px(170.);
/// The build line, one line of 12 px text, and the gap above it.
const BUILD_LINE_HEIGHT: Pixels = px(24.);
/// The line height of the 12 px lines: 4 px of leading keeps the description's two lines one
/// paragraph.
const SMALL_LINE_HEIGHT: Pixels = px(16.);
const ESCAPE: &str = "escape";

/// The elements tests look up.
pub const LOGO_ID: &str = "about-logo";
pub const VERSION_ID: &str = "about-version";
pub const BUILD_ID: &str = "about-build";
pub const DESCRIPTION_ID: &str = "about-description";
pub const VCS_ID: &str = "about-vcs";

/// The window, while one was opened. Its handle fails once the window has closed.
struct Opened(AnyWindowHandle);

impl Global for Opened {}

/// Bring the window forward, or open it. A refusal from the platform goes to the log: the
/// merge goes on without the window.
pub(crate) fn show(cx: &mut App) {
    let opened = cx.try_global::<Opened>().map(|opened| opened.0);
    let raised = opened
        .is_some_and(|window| window.update(cx, |_, window, _| window.activate_window()).is_ok());
    if raised {
        return;
    }
    match open(cx) {
        Ok(window) => cx.set_global(Opened(window)),
        Err(error) => tracing::error!(%error, "the About window did not open"),
    }
}

fn open(cx: &mut App) -> anyhow::Result<AnyWindowHandle> {
    let build = strings::build_of_this_binary();
    let height = if build.is_some() { WINDOW_HEIGHT + BUILD_LINE_HEIGHT } else { WINDOW_HEIGHT };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(WINDOW_WIDTH, height),
            cx,
        ))),
        is_resizable: false,
        is_minimizable: false,
        ..crate::menu_bar::plain_options()
    };
    let (window, _) = gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title(strings::ABOUT_TITLE);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        cx.new(|_| AboutWindow { focus, build })
    })?;
    Ok(window)
}

struct AboutWindow {
    focus: FocusHandle,
    build: Option<SharedString>,
}

fn close_on_escape(event: &KeyDownEvent, window: &mut Window, _: &mut App) {
    if event.keystroke.key == ESCAPE {
        window.remove_window();
    }
}

impl Render for AboutWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let version = strings::version();
        let theme = cx.theme();
        v_flex()
            .track_focus(&self.focus)
            .on_key_down(close_on_escape)
            .size_full()
            .p_4()
            .gap_2()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id(LOGO_ID)
                    .test_support()
                    .role(Role::Image)
                    .aria_label(strings::APP_NAME)
                    .flex_none()
                    .mb_2()
                    .w(logo::ABOUT_LOGO_WIDTH)
                    .h(logo::ABOUT_LOGO_HEIGHT)
                    .child(logo::logo(theme.is_dark())),
            )
            .child(
                div()
                    .id(VERSION_ID)
                    .test_support()
                    .role(Role::Label)
                    .aria_label(version.clone())
                    .child(version),
            )
            .children(
                self.build.clone().map(|build| small_line(BUILD_ID, build, theme.muted_foreground)),
            )
            .child(
                v_flex()
                    .items_center()
                    .child(small_line(
                        DESCRIPTION_ID,
                        strings::DESCRIPTION.into(),
                        theme.foreground,
                    ))
                    .child(small_line(VCS_ID, strings::VCS_SUPPORT.into(), theme.foreground)),
            )
    }
}

/// A label in STYLE.md's 12 px dialog text.
fn small_line(id: &'static str, text: SharedString, colour: Hsla) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .role(Role::Label)
        .aria_label(text.clone())
        .text_xs()
        .line_height(SMALL_LINE_HEIGHT)
        .text_color(colour)
        .child(text)
}
