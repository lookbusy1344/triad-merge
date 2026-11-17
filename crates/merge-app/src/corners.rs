//! The corner radius of buttons, list rows, inputs, tabs and menus: Zed's `rounded_sm`, which
//! its `ButtonLike` and `ListItem` take (`docs/STYLE.md` § Controls). gpui-component's themes
//! carry 6 px; triad writes 4 px into both theme files, so every change of mode keeps it.

use std::rc::Rc;

use gpui_kit::App;
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeConfig};

/// Zed's `rounded_sm`: 0.25 rem at its 16 px rem. `ThemeConfig` takes whole pixels.
pub(crate) const RADIUS: usize = 4;

fn with_radius(config: &ThemeConfig) -> Rc<ThemeConfig> {
    Rc::new(ThemeConfig { radius: Some(RADIUS), ..config.clone() })
}

/// Give both themes Zed's radius and redraw in the current mode.
pub(crate) fn install(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    theme.light_theme = with_radius(&theme.light_theme);
    theme.dark_theme = with_radius(&theme.dark_theme);
    let mode = cx.theme().mode;
    Theme::change(mode, None, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::component::ThemeMode;
    use gpui_kit::{TestAppContext, px};

    /// Zed's `rounded_sm`, from `gpui_macros/src/styles.rs`: 4 px.
    const ZED_ROUNDED_SM: f32 = 4.;
    /// gpui-component's large radius, for dialogs and popovers, which Zed's `rounded_lg` matches.
    const ZED_ROUNDED_LG: f32 = 8.;

    fn init(cx: &mut TestAppContext, mode: ThemeMode) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            Theme::change(mode, None, cx);
            install(cx);
        });
    }

    #[gpui_kit::test]
    fn the_light_theme_takes_zeds_radius(cx: &mut TestAppContext) {
        init(cx, ThemeMode::Light);
        assert_eq!(cx.read(|cx| cx.theme().radius), px(ZED_ROUNDED_SM));
    }

    #[gpui_kit::test]
    fn the_dark_theme_takes_zeds_radius(cx: &mut TestAppContext) {
        init(cx, ThemeMode::Dark);
        assert_eq!(cx.read(|cx| cx.theme().radius), px(ZED_ROUNDED_SM));
    }

    #[gpui_kit::test]
    fn the_radius_survives_a_change_of_mode(cx: &mut TestAppContext) {
        init(cx, ThemeMode::Light);
        cx.update(|cx| Theme::change(ThemeMode::Dark, None, cx));
        assert_eq!(cx.read(|cx| cx.theme().radius), px(ZED_ROUNDED_SM));
        cx.update(|cx| Theme::change(ThemeMode::Light, None, cx));
        assert_eq!(cx.read(|cx| cx.theme().radius), px(ZED_ROUNDED_SM));
    }

    #[gpui_kit::test]
    fn the_app_takes_zeds_radius(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::init(cx);
        });
        assert_eq!(cx.read(|cx| cx.theme().radius), px(ZED_ROUNDED_SM));
    }

    #[gpui_kit::test]
    fn the_large_radius_stays(cx: &mut TestAppContext) {
        init(cx, ThemeMode::Light);
        assert_eq!(cx.read(|cx| cx.theme().radius_lg), px(ZED_ROUNDED_LG));
    }
}
