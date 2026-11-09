//! triad's logo and mark, from `images/`, in the tones that read on the theme.

use gpui_kit::{Img, ObjectFit, Pixels, Styled as _, StyledImage as _, img, px};

const LOGO: &str = "images/triad-logo.svg";
const LOGO_DARK: &str = "images/triad-logo-dark.svg";
const MARK: &str = "images/triad-mark.svg";
const MARK_DARK: &str = "images/triad-mark-dark.svg";

/// The asset path and bytes of each file the app draws.
pub(crate) const FILES: [(&str, &[u8]); 4] = [
    (LOGO, include_bytes!("../../../images/triad-logo.svg")),
    (LOGO_DARK, include_bytes!("../../../images/triad-logo-dark.svg")),
    (MARK, include_bytes!("../../../images/triad-mark.svg")),
    (MARK_DARK, include_bytes!("../../../images/triad-mark-dark.svg")),
];

/// The logo's size in the About window: its view box is 734.68 by 186 units.
pub(crate) const ABOUT_LOGO_WIDTH: Pixels = px(221.);
pub(crate) const ABOUT_LOGO_HEIGHT: Pixels = px(56.);
/// The mark's side in a title bar or toolbar.
const MARK_SIDE: Pixels = px(18.);

pub(crate) const fn logo_path(dark: bool) -> &'static str {
    if dark { LOGO_DARK } else { LOGO }
}

pub(crate) const fn mark_path(dark: bool) -> &'static str {
    if dark { MARK_DARK } else { MARK }
}

/// The mark and the wordmark, filling the element that holds them.
pub(crate) fn logo(dark: bool) -> Img {
    img(logo_path(dark)).size_full().object_fit(ObjectFit::Contain)
}

/// The mark alone, at title bar and toolbar size.
pub(crate) fn mark(dark: bool) -> Img {
    img(mark_path(dark)).size(MARK_SIDE).flex_none()
}
