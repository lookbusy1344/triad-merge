//! Helpers shared by the window tests.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, App, AppContext as _, Entity, TestAppContext};
use merge_app::{MergeWindow, WindowSettings};
use merge_core::document::MergeDocument;
pub use merge_core::testing::{doc, text, three};

pub fn open(cx: &mut TestAppContext, doc: MergeDocument) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let opened = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::open(doc, &WindowSettings::default(), cx).expect("window opens")
    });
    act(cx, opened.0, TestWindowExt::render_frame);
    opened
}

pub fn act(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    f: impl FnOnce(&mut gpui_kit::Window, &mut App),
) {
    cx.update_window(window, |_, window, cx| f(window, cx)).expect("window open");
}

/// The editor's text and the core's output.
pub fn texts(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> (String, String) {
    cx.read(|cx| {
        let view = view.read(cx);
        (view.result_editor().read(cx).text().to_string(), view.document().text())
    })
}

pub fn set_caret(cx: &mut TestAppContext, view: &Entity<MergeWindow>, offset: usize) {
    view.update(cx, |view, cx| {
        view.result_editor().update(cx, |state, cx| state.set_selected_range(offset..offset, cx));
    });
}

pub fn caret(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> usize {
    cx.read(|cx| view.read(cx).result_editor().read(cx).cursor())
}

/// Open a merge window that writes to `output`, recording the exit codes it finishes with
/// in place of exiting the process.
pub fn open_saving(
    cx: &mut TestAppContext,
    doc: MergeDocument,
    output: &std::path::Path,
) -> (AnyWindowHandle, Entity<MergeWindow>, Finished) {
    let settings = WindowSettings {
        destination: Some(merge_session::resolve::Destination::file_at(output.to_owned())),
        ..WindowSettings::default()
    };
    open_with(cx, doc, &settings)
}

/// Open a merge window with `settings`, recording the exit codes it finishes with.
pub fn open_with(
    cx: &mut TestAppContext,
    doc: MergeDocument,
    settings: &WindowSettings,
) -> (AnyWindowHandle, Entity<MergeWindow>, Finished) {
    let (opened, finished) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        let finished = Finished::install(cx);
        let opened = merge_app::open(doc, settings, cx).expect("window opens");
        (opened, finished)
    });
    act(cx, opened.0, TestWindowExt::render_frame);
    (opened.0, opened.1, finished)
}

/// The exit codes a window finished with, in order.
#[derive(Clone, Default)]
pub struct Finished(std::rc::Rc<std::cell::RefCell<Vec<merge_app::ExitCode>>>);

impl Finished {
    /// Record finishes in place of exiting the process.
    pub fn install(cx: &mut App) -> Self {
        let finished = Self::default();
        let record = finished.clone();
        cx.set_global(merge_app::Finish::new(move |code, _| {
            record.0.borrow_mut().push(code);
        }));
        finished
    }

    pub fn codes(&self) -> Vec<merge_app::ExitCode> {
        self.0.borrow().clone()
    }
}

/// Sizes of a tool window, from `docs/STYLE.md`: a 32 px header across the top over a 1 px
/// border, its text and 22 px buttons 12 px in from the sides, and the list under it, inset
/// 4 px, with 24 px rows whose text starts 8 px further in.
pub mod tool_window {
    use gpui_kit::{AnyWindowHandle, ElementId, Pixels, TestAppContext, px};

    use super::{bounds, viewport_width};

    pub const HEADER_HEIGHT: Pixels = px(32.);
    /// Where the header's border ends and what follows it starts.
    pub const UNDER_HEADER: Pixels = px(32. + 1.);
    pub const SIDE: Pixels = px(12.);
    pub const LIST_INSET: Pixels = px(4.);
    /// A text button, as Zed's `ButtonSize::Default`.
    pub const BUTTON_HEIGHT: Pixels = px(22.);
    pub const ROW_HEIGHT: Pixels = px(24.);
    /// Where a tool window's list starts: under the header, its border and the list's inset.
    pub const LIST_TOP: Pixels = px(32. + 1. + 4.);
    /// STYLE.md: chrome text and errors take 12 px on 16 px lines.
    pub const TEXT_LINE_HEIGHT: Pixels = px(16.);

    /// Check the frame of a tool window: `summary` at the header's left, `last_button` at its
    /// right, both centred in the header, and `first_row` across the list at its top.
    pub fn assert_frame(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        summary: impl Into<ElementId>,
        last_button: impl Into<ElementId>,
        first_row: impl Into<ElementId>,
    ) {
        let width = viewport_width(cx, window);
        let middle = HEADER_HEIGHT / 2.;
        let summary = bounds(cx, window, summary);
        assert_eq!(summary.left(), SIDE, "summary at the header's left");
        assert_eq!(summary.center().y, middle, "summary centred in the header");
        let button = bounds(cx, window, last_button);
        assert_eq!(button.right(), width - SIDE, "last button at the header's right");
        assert_eq!(button.size.height, BUTTON_HEIGHT);
        assert_eq!(button.center().y, middle, "button centred in the header");
        let row = bounds(cx, window, first_row);
        assert_eq!((row.left(), row.top()), (LIST_INSET, LIST_TOP), "first row's corner");
        assert_eq!(
            (row.size.width, row.size.height),
            (width - LIST_INSET - LIST_INSET, ROW_HEIGHT)
        );
    }
}

/// The bounds of `id` in `window`'s last frame.
pub fn bounds(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let id = id.into();
    let mut found = gpui_kit::Bounds::default();
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        found = window.find(id).bounds();
    });
    found
}

/// The accessible name of `id` in `window`'s last frame; empty without one.
pub fn label(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> String {
    let id = id.into();
    let mut found = String::new();
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        found = window.find(id).label().map_or_else(String::new, str::to_owned);
    });
    found
}

/// The accessible role of `id` in `window`'s last frame.
pub fn role(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> Option<gpui_kit::Role> {
    let id = id.into();
    let mut found = None;
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        found = window.find(id).role();
    });
    found
}

/// The width of `window`'s content.
pub fn viewport_width(cx: &mut TestAppContext, window: AnyWindowHandle) -> gpui_kit::Pixels {
    let mut width = gpui_kit::Pixels::ZERO;
    act(cx, window, |window, _| width = window.viewport_size().width);
    width
}

/// Whether `id` shows as selected in `window`'s last frame.
pub fn is_selected(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
) -> bool {
    let id = id.into();
    let mut selected = false;
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        selected = window.find(id).selected() == Some(true);
    });
    selected
}
