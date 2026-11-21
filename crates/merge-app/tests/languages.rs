//! The merge and About windows in Spanish and Japanese (spec §6.5). Spanish labels run longer than
//! English and Japanese ones shorter, so the toolbar's label threshold moves both ways. The language belongs to the process
//! and a run sets it once; nextest runs each test in a process of its own, so each test here
//! sets its language before its first window. Expected text is the catalogues' text.

use std::path::Path;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use merge_app::About;
use merge_app::about::{BUILD_ID, DESCRIPTION_ID, VCS_ID, VERSION_ID};
use merge_app::open_dialog::Slot;
use merge_app::toolbar::{Badge, LABELS_MIN_WIDTH, Tool, labels_min_width};
use merge_app::window::CONFLICT_BADGE_ID;
use merge_app::{MergeWindow, WindowSettings};
use merge_config::settings::UiLanguage;
use merge_view::status::FileNames;

use crate::common::{act, doc, open, open_with};
use crate::tools::{
    DEFAULT_TOOLBAR_TEXT, TO_CONFLICT_TEN, in_result, label_shown, resize, tools_fit,
    twelve_conflicts,
};

/// The language of this test's process.
fn speak(language: UiLanguage) {
    merge_app::set_language(language, None);
}

/// git's temporary names, as `git mergetool` passes them, and an output file.
fn named(output: &Path) -> WindowSettings {
    WindowSettings {
        file_names: FileNames::three_way(
            Path::new("./foo_BASE_1234.c"),
            Path::new("./foo_LOCAL_1234.c"),
            Path::new("./foo_REMOTE_1234.c"),
        ),
        destination: Some(merge_session::resolve::Destination::file_at(output.join("foo.c"))),
        ..WindowSettings::default()
    }
}

/// Each heading's role chip, in pane order, then the result's.
fn roles(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Vec<Option<String>> {
    cx.read(|cx| {
        let view = view.read(cx);
        let panes = (0..view.pane_titles().count()).map(|pane| view.heading_role(pane));
        panes.chain([view.result_role()]).map(|role| role.map(ToString::to_string)).collect()
    })
}

/// Each pane's title, then the result's.
fn titles(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> Vec<String> {
    cx.read(|cx| {
        let view = view.read(cx);
        view.pane_titles().chain([view.result_title()]).map(ToString::to_string).collect()
    })
}

fn status(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> String {
    cx.read(|cx| view.read(cx).status_text().to_string())
}

fn badge(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<String> {
    let mut badge = None;
    act(cx, window, |window, _| {
        badge = window.find(CONFLICT_BADGE_ID).label().map(str::to_owned);
    });
    badge
}

/// A window of twelve conflicts at its tenth, with the narrowest width that shows labels.
fn at_conflict_ten(cx: &mut TestAppContext) -> (AnyWindowHandle, gpui_kit::Pixels) {
    let (window, _) = in_result(cx, twelve_conflicts());
    for _ in 0..TO_CONFLICT_TEN {
        act(cx, window, |window, cx| window.click(Tool::NextConflict.id(), cx));
    }
    let mut threshold = gpui_kit::Pixels::ZERO;
    act(cx, window, |window, _| {
        threshold = labels_min_width(DEFAULT_TOOLBAR_TEXT, Badge::Counts, window);
    });
    (window, threshold)
}

/// The labels show from `threshold` and not a pixel below it, and the tools fit there.
fn assert_labels_fit_from(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    threshold: gpui_kit::Pixels,
) {
    resize(cx, window, threshold - gpui_kit::px(1.));
    assert!(!label_shown(cx, window, Tool::Left), "a pixel below {threshold:?}");
    resize(cx, window, threshold);
    assert!(label_shown(cx, window, Tool::Left), "at {threshold:?}");
    assert!(tools_fit(cx, window), "at {threshold:?}");
}

const DIFFERENT: [&str; 3] = ["a\nb\nc\n", "a\nB\nc\n", "a\nb\nc\n"];

#[gpui_kit::test]
fn spanish_headings_name_each_role(cx: &mut TestAppContext) {
    speak(UiLanguage::Es);
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_, view, _) = open_with(cx, doc("a\n", "b\n", "c\n"), &named(dir.path()));
    let expected =
        [Some("IZQUIERDA"), Some("DERECHA"), Some("RESULTADO")].map(|r| r.map(str::to_owned));
    assert_eq!(roles(cx, &view), expected);
    let (_, view) = open(cx, doc("a\n", "b\n", "c\n"));
    assert_eq!(titles(cx, &view), ["Izquierda", "Derecha", "Resultado"]);
    assert_eq!(roles(cx, &view), [None, None, None]);
}

#[gpui_kit::test]
fn japanese_headings_name_each_role(cx: &mut TestAppContext) {
    speak(UiLanguage::Ja);
    let dir = tempfile::tempdir().expect("temporary directory");
    let (_, view, _) = open_with(cx, doc("a\n", "b\n", "c\n"), &named(dir.path()));
    let expected = [Some("左"), Some("右"), Some("結果")].map(|r| r.map(str::to_owned));
    assert_eq!(roles(cx, &view), expected);
    let (_, view) = open(cx, doc("a\n", "b\n", "c\n"));
    assert_eq!(titles(cx, &view), ["左", "右", "結果"]);
}

#[gpui_kit::test]
fn the_spanish_status_bar_counts_in_spanish(cx: &mut TestAppContext) {
    speak(UiLanguage::Es);
    let [base, left, right] = DIFFERENT;
    let (_, view) = open(cx, doc(base, left, right));
    assert_eq!(
        status(cx, &view),
        "sin repo · fusión git · 1 diferencia · 0 conflictos · 0 sin resolver · UTF-8 · LF · \
         espacios significativos"
    );
}

#[gpui_kit::test]
fn the_japanese_status_bar_counts_in_japanese(cx: &mut TestAppContext) {
    speak(UiLanguage::Ja);
    let [base, left, right] = DIFFERENT;
    let (_, view) = open(cx, doc(base, left, right));
    assert_eq!(
        status(cx, &view),
        "リポジトリなし · git マージ · 1 件の差分 · 0 件の競合 · 0 件未解決 · UTF-8 · LF · 空白を区別"
    );
}

#[gpui_kit::test]
fn the_spanish_toolbar_needs_more_width_for_its_longer_labels(cx: &mut TestAppContext) {
    speak(UiLanguage::Es);
    let (window, threshold) = at_conflict_ten(cx);
    assert_eq!(badge(cx, window).as_deref(), Some("Conflicto 10 de 12 · 12 sin resolver"));
    // Izquierda, Derecha, Conflicto and the badge run ten letters past English in all.
    assert!(threshold > LABELS_MIN_WIDTH, "{threshold:?}");
    assert_labels_fit_from(cx, window, threshold);
}

#[gpui_kit::test]
fn the_japanese_toolbar_shows_its_shorter_labels_in_less_width(cx: &mut TestAppContext) {
    speak(UiLanguage::Ja);
    let (window, threshold) = at_conflict_ten(cx);
    assert_eq!(badge(cx, window).as_deref(), Some("競合 10/12 · 12 件未解決"));
    // Each label is one or two ideographs, against four to ten letters in English.
    assert!(threshold < LABELS_MIN_WIDTH, "{threshold:?}");
    assert_labels_fit_from(cx, window, threshold);
}

/// The About window's lines: version, build, description and VCS support. Test builds get no
/// `TRIAD_COMMIT`, so only a debug build has a build line. GPUI's test window keeps no title
/// to read, so the title goes unchecked here.
fn about(cx: &mut TestAppContext) -> [Option<String>; 4] {
    let (merge, _) = open(cx, doc("a\n", "b\n", "c\n"));
    cx.update(|cx| cx.dispatch_action(&About));
    cx.run_until_parked();
    let about = cx
        .update(|cx| cx.windows().into_iter().find(|w| w.window_id() != merge.window_id()))
        .expect("the About window is open");
    act(cx, about, TestWindowExt::render_frame);
    let mut found = [None, None, None, None];
    act(cx, about, |window, _| {
        let text = |id| window.try_find(id).and_then(|e| e.label().map(str::to_owned));
        let lines = [text(VERSION_ID), text(BUILD_ID), text(DESCRIPTION_ID), text(VCS_ID)];
        found = lines;
    });
    found
}

/// `build` on a debug build, where the About window names the placeholder IDs.
fn debug_only(build: &str) -> Option<String> {
    cfg!(debug_assertions).then(|| build.to_owned())
}

#[gpui_kit::test]
fn the_spanish_about_window_reads_in_spanish(cx: &mut TestAppContext) {
    speak(UiLanguage::Es);
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        about(cx),
        [
            Some(format!("Versión {version}")),
            debug_only("Commit 000000000000, cambio zzzzzzzzzzzz"),
            Some("Fusión de tres vías y comparación de dos vías".to_owned()),
            Some("Hecho para Jujutsu, con soporte completo de Git".to_owned()),
        ]
    );
}

#[gpui_kit::test]
fn the_japanese_about_window_reads_in_japanese(cx: &mut TestAppContext) {
    speak(UiLanguage::Ja);
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        about(cx),
        [
            Some(format!("バージョン {version}")),
            debug_only("コミット 000000000000、変更 zzzzzzzzzzzz"),
            Some("3 方向マージと 2 方向比較".to_owned()),
            Some("Jujutsu 向けに設計、Git も完全サポート".to_owned()),
        ]
    );
}

/// gpui-component takes the run's language by its tag, and reads triad's catalogue for it.
#[test]
fn gpui_component_speaks_traditional_chinese_with_the_windows() {
    speak(UiLanguage::ZhTw);
    assert_eq!(&*gpui_kit::component::locale(), "zh-TW");
}

/// The result's search panel in replace mode, rendered: the labels of its Replace and Replace
/// All buttons.
fn replace_buttons(cx: &mut TestAppContext) -> [Option<String>; 2] {
    let (window, _) = open(cx, doc("a\n", "b\n", "c\n"));
    act(cx, window, |window, cx| window.click(merge_app::window::RESULT_ID, cx));
    act(cx, window, |window, cx| window.press("secondary-f", cx));
    cx.run_until_parked();
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(gpui_kit::base::input::Replace), cx);
    });
    act(cx, window, TestWindowExt::render_frame);
    let mut labels = [None, None];
    act(cx, window, |window, _| {
        labels =
            ["replace-one", "replace-all"].map(|id| window.find(id).label().map(str::to_owned));
    });
    labels
}

#[gpui_kit::test]
fn the_japanese_search_panel_replaces_in_japanese(cx: &mut TestAppContext) {
    speak(UiLanguage::Ja);
    assert_eq!(replace_buttons(cx), [Some("置換".to_owned()), Some("すべて置換".to_owned())]);
}

#[gpui_kit::test]
fn the_polish_search_panel_replaces_in_polish(cx: &mut TestAppContext) {
    speak(UiLanguage::Pl);
    assert_eq!(
        replace_buttons(cx),
        [Some("Zamień".to_owned()), Some("Zamień wszystko".to_owned())]
    );
}

/// The open dialog in the run's language, rendered.
fn open_dialog(cx: &mut TestAppContext) -> AnyWindowHandle {
    let (window, _) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::open_dialog::open(
            WindowSettings::default(),
            merge_session::repository::Discovery::default(),
            false,
            cx,
        )
        .expect("the dialog opens")
    });
    act(cx, window, TestWindowExt::render_frame);
    window
}

/// Each slot's name, at the labels' size, fits its column and ends before its field starts.
fn assert_slot_names_fit(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, _| {
        let style = window.text_style();
        let size = gpui_kit::rems(LABEL_REMS).to_pixels(window.rem_size());
        for slot in [Slot::Base, Slot::Left, Slot::Right, Slot::Output] {
            let label = window.find(slot.label_id());
            let name = label.label().expect("the label names its slot").to_owned();
            let run = style.to_run(name.len());
            let text = window.text_system().shape_line(name.clone().into(), size, &[run], None);
            let (label, field) = (label.bounds(), window.find(slot.id()).bounds());
            assert!(label.size.width >= text.width, "{name}: {label:?} against {:?}", text.width);
            assert!(label.right() <= field.left(), "{name}: {label:?} against {field:?}");
        }
    });
}

/// The labels' text size, `text_xs`.
const LABEL_REMS: f32 = 0.75;

#[gpui_kit::test]
fn the_russian_open_dialog_widens_its_labels_for_its_longest_name(cx: &mut TestAppContext) {
    speak(UiLanguage::Ru);
    let window = open_dialog(cx);
    act(cx, window, |window, _| {
        let output = window.find(Slot::Output.label_id());
        assert_eq!(output.label(), Some("Результат"));
    });
    assert_slot_names_fit(cx, window);
}

#[gpui_kit::test]
fn the_spanish_open_dialog_fits_izquierda(cx: &mut TestAppContext) {
    speak(UiLanguage::Es);
    let window = open_dialog(cx);
    assert_slot_names_fit(cx, window);
}
