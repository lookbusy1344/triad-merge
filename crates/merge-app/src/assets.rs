//! The app's assets: GPUI Kit's component icons, the toolbar's icons and the logo.

use std::borrow::Cow;

use gpui_kit::assets::{Assets, icon_assets};
use gpui_kit::{AssetSource, SharedString};

use crate::logo;

icon_assets!(
    ToolIcons,
    [
        ChevronUp,
        ChevronDown,
        ChevronsUp,
        ChevronsDown,
        SquareArrowLeft,
        SquareDot,
        SquareArrowRight,
        Eraser,
        ArrowLeftRight,
        ArrowRightLeft,
        Undo2,
        Redo2,
        Columns3,
        Search,
        Hash,
        Highlighter,
        TextWrap,
    ]
);

/// GPUI Kit's component icons, the toolbar's icons, which the component set lacks, and the logo.
pub(crate) struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = logo::FILES.iter().find(|(file, _)| *file == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        match ToolIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(ToolIcons.list(path)?);
        paths.extend(
            logo::FILES
                .iter()
                .filter(|(file, _)| file.starts_with(path))
                .map(|(file, _)| SharedString::from(*file)),
        );
        paths.sort_unstable();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use crate::toolbar::{GROUPS, tools};

    use super::*;

    const SVG_START: &[u8] = b"<svg";
    /// GPUI Kit's component icons: the lines of `gpui-kit-assets` 0.8.1's `default-icons.txt`.
    const COMPONENT_ICONS: usize = 106;
    /// Toolbar icons outside the component set: all 17 but chevron-up, chevron-down, undo-2,
    /// redo-2 and search.
    const EXTRA_TOOL_ICONS: usize = 12;

    #[test]
    fn every_tool_icon_loads() {
        for tool in tools(&GROUPS) {
            let path = tool.icon().path();
            let bytes = AppAssets
                .load(&path)
                .unwrap()
                .unwrap_or_else(|| panic!("{tool:?}: no asset at {path}"));
            assert!(bytes.starts_with(SVG_START), "{tool:?}: {path} is not an SVG");
        }
    }

    #[test]
    fn component_icons_still_load() {
        let bytes = AppAssets.load("icons/check.svg").unwrap().unwrap();
        assert!(bytes.starts_with(SVG_START));
    }

    #[test]
    fn an_unknown_path_is_an_error() {
        let error = AppAssets.load("icons/no-such-icon.svg").unwrap_err();
        assert_eq!(error.to_string(), r#"could not find asset at path "icons/no-such-icon.svg""#);
    }

    #[test]
    fn the_logo_files_load_as_the_files_of_images() {
        for (dark, path, file) in [
            (false, logo::logo_path(false), "triad-logo.svg"),
            (true, logo::logo_path(true), "triad-logo-dark.svg"),
            (false, logo::mark_path(false), "triad-mark.svg"),
            (true, logo::mark_path(true), "triad-mark-dark.svg"),
        ] {
            let bytes = AppAssets.load(path).unwrap().unwrap();
            let on_disk =
                std::fs::read(format!("{}/../../images/{file}", env!("CARGO_MANIFEST_DIR")))
                    .unwrap();
            assert_eq!(bytes.as_ref(), on_disk.as_slice(), "dark {dark}: {path}");
        }
    }

    #[test]
    fn the_list_names_the_logo_files_and_leaves_the_icons_alone() {
        let paths = AppAssets.list("images/").unwrap();
        assert_eq!(
            paths,
            [
                "images/triad-logo-dark.svg",
                "images/triad-logo.svg",
                "images/triad-mark-dark.svg",
                "images/triad-mark.svg",
            ]
        );
    }

    #[test]
    fn the_list_holds_both_sets_once_each_in_order() {
        let paths = AppAssets.list("icons/").unwrap();
        assert_eq!(paths.len(), COMPONENT_ICONS + EXTRA_TOOL_ICONS);
        assert!(paths.is_sorted());
        assert!(paths.iter().any(|path| path == "icons/hash.svg"));
        assert!(paths.iter().any(|path| path == "icons/check.svg"));
    }
}
