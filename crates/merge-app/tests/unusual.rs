//! Headless tests of unusual characters in the panes (spec §5): an invisible character draws
//! as its code point on a grey box, and an odd space on a grey background.

use crate::common;

use common::{doc, open};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{HighlightStyle, SharedString, TestAppContext};
use merge_view::rows::RowIx;

const LEFT_PANE: usize = 0;
/// The grey behind an unusual character: the muted text colour at this opacity (spec §5).
const GREY_OPACITY: f32 = 0.25;
const FIRST_ROW: usize = 0;

/// What the left pane draws on the first row of a merge whose inputs are all `line`.
fn left_cell(
    cx: &mut TestAppContext,
    line: &str,
) -> (SharedString, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let (_, view) = open(cx, doc(line, line, line));
    view.update(cx, |view, cx| view.cell_highlights(LEFT_PANE, RowIx::new(FIRST_ROW), cx))
        .expect("the cell has a line")
}

#[gpui_kit::test]
fn an_invisible_character_draws_as_its_code_point_on_a_grey_box(cx: &mut TestAppContext) {
    let (text, highlights) = left_cell(cx, "a\u{202E}b\n");
    let colour = cx.update(|cx| cx.theme().muted_foreground);
    let fill = colour.opacity(GREY_OPACITY);
    let badge = HighlightStyle {
        color: Some(colour),
        background_color: Some(fill),
        ..HighlightStyle::default()
    };
    assert_eq!(text.as_ref(), "aU+202Eb");
    assert_eq!(highlights, [(1..7, badge)]);
}

#[gpui_kit::test]
fn an_odd_space_draws_on_a_grey_background(cx: &mut TestAppContext) {
    let (text, highlights) = left_cell(cx, "a\u{00A0}b\n");
    let fill = cx.update(|cx| cx.theme().muted_foreground.opacity(GREY_OPACITY));
    let shade = HighlightStyle { background_color: Some(fill), ..HighlightStyle::default() };
    assert_eq!(text.as_ref(), "a\u{00A0}b");
    assert_eq!(highlights, [(1..3, shade)]);
}
