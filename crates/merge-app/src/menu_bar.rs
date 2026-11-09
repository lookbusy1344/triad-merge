//! The in-window menu bar of Linux and Windows (spec §2.2). macOS shows the same menus in
//! the global bar.

use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::{GlobalState, TitleBar, h_flex};
use gpui_kit::{
    App, Entity, IntoElement, Menu, ParentElement as _, Styled as _, Window, WindowOptions,
};

use crate::{Cancel, ViewTicks, strings, toolbar};

pub(crate) const IN_WINDOW: bool = !cfg!(target_os = "macos");

/// The window options a window with a menu bar starts from.
pub(crate) fn window_options() -> WindowOptions {
    let options = if IN_WINDOW { TitleBar::window_options() } else { WindowOptions::default() };
    WindowOptions { app_id: Some(strings::PROGRAM.to_owned()), ..options }
}

/// The window options of a window without a menu bar.
pub(crate) fn plain_options() -> WindowOptions {
    WindowOptions { app_id: Some(strings::PROGRAM.to_owned()), ..WindowOptions::default() }
}

/// Give the in-window bars their menus. A bar reads them when it is built.
pub(crate) fn publish(menus: Vec<Menu>, cx: &mut App) {
    GlobalState::global_mut(cx).set_app_menus(menus.into_iter().map(Menu::owned).collect());
}

/// A window's menus: its own bar, on the platforms that draw one, and the View menu ticks it
/// last showed.
#[derive(Debug)]
pub(crate) struct WindowMenu {
    bar: Option<Entity<AppMenuBar>>,
    shown: Option<ViewTicks>,
}

impl WindowMenu {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self { bar: IN_WINDOW.then(|| AppMenuBar::new(cx)), shown: None }
    }

    /// The window's own bar; none on macOS.
    pub(crate) fn bar(&self) -> Option<&Entity<AppMenuBar>> {
        self.bar.as_ref()
    }

    /// Put `ticks` in the menu bars while the window is active and they changed: the global
    /// bar, and the window's own bar where there is one.
    pub(crate) fn show(&mut self, ticks: ViewTicks, window: &Window, cx: &mut App) {
        if !window.is_window_active() || self.shown == Some(ticks) {
            return;
        }
        self.shown = Some(ticks);
        cx.set_menus(crate::menus(ticks));
        publish(crate::menus(ticks), cx);
        if let Some(bar) = &self.bar {
            bar.update(cx, AppMenuBar::reload);
        }
    }

    /// Forget what was shown, so the next [`WindowMenu::show`] shows it again: another window
    /// may have changed the bars since.
    pub(crate) fn forget(&mut self) {
        self.shown = None;
    }
}

/// The title bar that holds the mark and `bar`. Its close button cancels, as the key does, so
/// that unsaved work is asked about.
pub(crate) fn render(
    bar: Option<&Entity<AppMenuBar>>,
    dark: bool,
) -> Option<impl IntoElement + use<>> {
    bar.cloned().map(|bar| {
        TitleBar::new()
            .on_close_window(|_, window, cx| window.dispatch_action(Box::new(Cancel), cx))
            .child(h_flex().gap_2().child(toolbar::mark(dark)).child(bar))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The name of `packaging/linux/triad-merge.desktop`, which a desktop matches to the window.
    const DESKTOP_ENTRY: &str = "triad-merge";

    #[test]
    fn every_window_names_the_desktop_entry() {
        assert_eq!(window_options().app_id.as_deref(), Some(DESKTOP_ENTRY));
        assert_eq!(plain_options().app_id.as_deref(), Some(DESKTOP_ENTRY));
    }
}
