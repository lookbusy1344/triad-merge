//! Helpers shared by the window tests.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, App, AppContext as _, Entity, TestAppContext};
use merge_app::{MergeWindow, WindowSettings};
use merge_core::document::MergeDocument;
pub use merge_core::testing::{doc, text};

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
    let settings = WindowSettings { output: Some(output.to_owned()), ..WindowSettings::default() };
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

/// Sizes of a tool window, from `docs/STYLE.md`: the edge, the gap between rows, a small
/// control and a list row.
pub mod tool_window {
    use gpui_kit::{Pixels, px};

    pub const EDGE: Pixels = px(12.);
    pub const ROW_GAP: Pixels = px(8.);
    pub const CONTROL_HEIGHT: Pixels = px(24.);
    pub const ROW_HEIGHT: Pixels = px(24.);
    /// Where a tool window's list starts: under the edge, its row of buttons and a gap.
    pub const LIST_TOP: Pixels = px(12. + 24. + 8.);
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
