//! Headless tests of the result pane's placeholder rows (spec §5).

use crate::common;

use common::{act, doc, open, set_caret, texts};
use gpui_kit::base::input::GutterMark;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use merge_app::window::RESULT_ID;
use merge_app::{MergeWindow, TakeLeft, TakeRight, Undo};

/// The result editor's spacer rows and the notes on them.
type Shown = (Vec<(usize, usize)>, Vec<(usize, String)>);

fn shown(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Shown {
    cx.read(|cx| {
        let state = view.read(cx).result_editor().read(cx);
        (
            state.display_map().spacers().to_vec(),
            state
                .spacer_notes()
                .iter()
                .map(|note| (note.line(), note.text().to_string()))
                .collect(),
        )
    })
}

fn deleted_on_left(line: usize) -> Shown {
    (vec![(line, 1)], vec![(line, "2 lines deleted (Left)".to_owned())])
}

/// Left deletes b, c; right changes them. The caret sits in the conflict.
fn opened(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let (window, view) = open(cx, doc("a\nb\nc\nz\n", "a\nz\n", "a\nB\nC\nz\n"));
    act(cx, window, |window, cx| window.click(RESULT_ID, cx));
    set_caret(cx, &view, 2);
    (window, view)
}

#[gpui_kit::test]
fn an_unresolved_conflict_shows_the_deleting_side_above_the_other(cx: &mut TestAppContext) {
    let (_, view) = opened(cx);
    assert_eq!(texts(cx, &view).0, "a\nB\nC\nz\n");
    assert_eq!(shown(cx, &view), deleted_on_left(1));
}

#[gpui_kit::test]
fn the_placeholder_follows_picks_and_undo(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeLeft), cx);
    });
    assert_eq!(texts(cx, &view).0, "a\nz\n");
    assert_eq!(shown(cx, &view), deleted_on_left(1));

    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeRight), cx);
    });
    assert_eq!(texts(cx, &view).0, "a\nB\nC\nz\n");
    assert_eq!(shown(cx, &view), (vec![], vec![]));

    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(Undo), cx);
    });
    assert_eq!(texts(cx, &view).0, "a\nz\n");
    assert_eq!(shown(cx, &view), deleted_on_left(1));
}

/// Each placeholder's gutter mark label and whether it has a fill and a border.
fn looks(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Vec<(Option<String>, bool, bool)> {
    cx.read(|cx| {
        view.read(cx)
            .result_editor()
            .read(cx)
            .spacer_notes()
            .iter()
            .map(|note| {
                (
                    note.gutter_mark().map(|mark| mark.label().to_string()),
                    note.fill().is_some(),
                    note.border().is_some(),
                )
            })
            .collect()
    })
}

#[gpui_kit::test]
fn a_placeholder_looks_like_the_block_it_stands_for(cx: &mut TestAppContext) {
    let (window, view) = opened(cx);
    // Unresolved: framed and marked as a conflict block.
    assert_eq!(looks(cx, &view), [(Some("!".to_owned()), true, true)]);
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(TakeLeft), cx);
    });
    // Picked: left's band and mark, no frame.
    assert_eq!(looks(cx, &view), [(Some("L".to_owned()), true, false)]);
}

#[gpui_kit::test]
fn a_placeholder_takes_its_colours_from_the_settings(cx: &mut TestAppContext) {
    use gpui_kit::hsla;
    use merge_config::settings::{Colour, Colours, DEFAULT_COLOURS, Theme, Themed};
    const DEGREES: f32 = 360.;
    let settings = merge_app::WindowSettings {
        theme: Theme::Light,
        colours: Colours {
            left: Themed::both(Colour::rgb(0x00_00_ff)),
            conflict: Themed::both(Colour::rgb(0xff_00_00)),
            deleted: Themed::both(Colour::rgb(0x00_ff_00)),
            band_opacity: Themed { light: 0.5, dark: 0.25 },
            ..DEFAULT_COLOURS
        },
        ..merge_app::WindowSettings::default()
    };
    let (_, view, _) =
        common::open_with(cx, doc("a\nb\nc\nz\n", "a\nz\n", "a\nB\nC\nz\n"), &settings);
    let red = hsla(0., 1., 0.5, 1.);
    let note = cx.read(|cx| {
        let notes = view.read(cx).result_editor().read(cx).spacer_notes();
        assert_eq!(notes.len(), 1, "one placeholder for left's deletion");
        let note = &notes[0];
        (note.color(), note.fill(), note.border(), note.gutter_mark().map(GutterMark::color))
    });
    assert_eq!(
        note,
        (
            hsla(120. / DEGREES, 1., 0.5, 1.),
            Some(hsla(240. / DEGREES, 1., 0.5, 0.5)),
            Some(red),
            Some(red),
        )
    );
}
