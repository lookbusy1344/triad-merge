//! Headless tests of the editors' syntax languages (`docs/PLAN.md` §F5, task 5).

use crate::common;
use merge_view::rows::RowIx;

use std::path::{Path, PathBuf};

use common::{act, doc, open_with};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::ThemeMode;
use gpui_kit::component::highlighter::{HighlightTheme, LanguageRegistry};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Bounds, Entity, HighlightStyle, Pixels, TestAppContext};
use merge_app::diff_window::{DiffFile, DiffSettings, DiffWindow, Origin};
use merge_app::toolbar::Tool;
use merge_app::window::{CellHighlights, MergeWindow, Syntax, cell_id};
use merge_app::{ToggleSyntax, WindowSettings};
use merge_core::diff2::Side;
use merge_core::text::Text;
use merge_view::status::FileNames;

const PLAIN: &str = "plain";

fn three_way(name: &str) -> FileNames {
    let path = Path::new(name);
    FileNames::three_way(path, path, path)
}

fn result_language(cx: &mut TestAppContext, settings: &WindowSettings) -> String {
    let (_, view, _) = open_with(cx, doc("a\n", "b\n", "a\n"), settings);
    cx.read(|cx| view.read(cx).result_editor().read(cx).language_name().to_string())
}

#[test]
fn the_registry_has_a_grammar_for_each_language() {
    for name in ["rust", "go", "csharp"] {
        let config = LanguageRegistry::singleton().language(name);
        assert!(config.is_some_and(|c| c.has_grammar()), "{name}");
    }
}

#[gpui_kit::test]
fn the_result_takes_the_inputs_language(cx: &mut TestAppContext) {
    let settings =
        WindowSettings { file_names: three_way("BASE_1234.rs"), ..WindowSettings::default() };
    assert_eq!(result_language(cx, &settings), "rust");
}

#[gpui_kit::test]
fn the_output_names_the_language_when_the_inputs_do_not(cx: &mut TestAppContext) {
    let settings = WindowSettings {
        file_names: three_way("BASE_1234"),
        output: Some(PathBuf::from("src/a.go")),
        title: Some("src/a.cs".to_owned()),
        ..WindowSettings::default()
    };
    assert_eq!(result_language(cx, &settings), "go");
}

#[gpui_kit::test]
fn the_title_names_the_language_last(cx: &mut TestAppContext) {
    let settings = WindowSettings {
        file_names: three_way("BASE_1234"),
        output: Some(PathBuf::from("out")),
        title: Some("src/Program.cs".to_owned()),
        ..WindowSettings::default()
    };
    assert_eq!(result_language(cx, &settings), "csharp");
}

#[gpui_kit::test]
fn an_unknown_name_is_plain(cx: &mut TestAppContext) {
    let settings = WindowSettings {
        file_names: three_way("a.txt"),
        output: Some(PathBuf::from("a.txt")),
        ..WindowSettings::default()
    };
    assert_eq!(result_language(cx, &settings), PLAIN);
}

#[gpui_kit::test]
fn without_syntax_the_result_is_plain(cx: &mut TestAppContext) {
    let settings = WindowSettings {
        file_names: three_way("a.rs"),
        syntax: Syntax::Plain,
        ..WindowSettings::default()
    };
    assert_eq!(result_language(cx, &settings), PLAIN);
}

fn file(origin: Origin) -> DiffFile {
    DiffFile { origin, text: Text::decode(b"x\n".to_vec()).expect("decodes"), read_only: false }
}

fn pane_languages(
    cx: &mut TestAppContext,
    left: Origin,
    right: Origin,
    settings: &DiffSettings,
) -> [String; 2] {
    let view = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        let (_, view) = merge_app::diff_window::open(file(left), file(right), settings, cx)
            .expect("window opens");
        view
    });
    cx.read(|cx| {
        let window: &DiffWindow = view.read(cx);
        [Side::Left, Side::Right]
            .map(|side| window.editor(side).read(cx).language_name().to_string())
    })
}

#[gpui_kit::test]
fn both_diff_panes_take_the_first_named_language(cx: &mut TestAppContext) {
    let languages = pane_languages(
        cx,
        Origin::Revision { title: "a.rs@abc".to_owned() },
        Origin::File(PathBuf::from("src/a.go")),
        &DiffSettings::default(),
    );
    assert_eq!(languages, ["go", "go"]);
}

#[gpui_kit::test]
fn the_diff_title_names_the_language_last(cx: &mut TestAppContext) {
    let languages = pane_languages(
        cx,
        Origin::File(PathBuf::from("/tmp/left")),
        Origin::File(PathBuf::from("/tmp/right")),
        &DiffSettings { title: Some("src/a.rs".to_owned()), ..DiffSettings::default() },
    );
    assert_eq!(languages, ["rust", "rust"]);
}

#[gpui_kit::test]
fn without_syntax_the_diff_panes_are_plain(cx: &mut TestAppContext) {
    let languages = pane_languages(
        cx,
        Origin::File(PathBuf::from("a.cs")),
        Origin::File(PathBuf::from("b.cs")),
        &DiffSettings { syntax: Syntax::Plain, ..DiffSettings::default() },
    );
    assert_eq!(languages, [PLAIN, PLAIN]);
}

/// The left pane of a two-pane window.
const LEFT_PANE: usize = 0;
const FIRST_ROW: usize = 0;

fn rust_merge(cx: &mut TestAppContext, syntax: Syntax) -> gpui_kit::Entity<MergeWindow> {
    let settings =
        WindowSettings { file_names: three_way("a.rs"), syntax, ..WindowSettings::default() };
    // The left side renames `a` to `b`; `fn` is the same on both.
    let (_, view, _) = open_with(cx, doc("fn a() {}\n", "fn b() {}\n", "fn a() {}\n"), &settings);
    view
}

fn left_cell(cx: &mut TestAppContext, view: &gpui_kit::Entity<MergeWindow>) -> CellHighlights {
    cx.update(|cx| {
        view.update(cx, |view, cx| view.cell_highlights(LEFT_PANE, RowIx::new(FIRST_ROW), cx))
            .expect("the left pane has a line")
            .1
    })
}

fn capture(cx: &mut TestAppContext, name: &str) -> HighlightStyle {
    cx.read(|cx| {
        cx.theme().highlight_theme.style.syntax.style(name).expect("the theme styles the capture")
    })
}

/// The style of the run that holds byte `at`.
fn at(highlights: &CellHighlights, at: usize) -> HighlightStyle {
    highlights
        .iter()
        .find(|(range, _)| range.contains(&at))
        .map(|&(_, style)| style)
        .unwrap_or_default()
}

const FN: usize = 0;
const NAME: usize = 3;

#[gpui_kit::test]
fn a_rust_input_draws_its_keyword_in_the_keyword_colour(cx: &mut TestAppContext) {
    let view = rust_merge(cx, Syntax::Highlighted);
    cx.run_until_parked();
    let cell = left_cell(cx, &view);
    assert_eq!(at(&cell, FN), capture(cx, "keyword"));
    assert_eq!(cell.first().map(|(range, _)| range.clone()), Some(0..2));
}

#[gpui_kit::test]
fn the_change_mark_draws_over_the_syntax_colour(cx: &mut TestAppContext) {
    let plain = rust_merge(cx, Syntax::Plain);
    cx.run_until_parked();
    let mark = at(&left_cell(cx, &plain), NAME).background_color;
    assert!(mark.is_some(), "the renamed function is marked");
    let view = rust_merge(cx, Syntax::Highlighted);
    cx.run_until_parked();
    let name = at(&left_cell(cx, &view), NAME);
    assert_eq!(name.background_color, mark);
    let function = capture(cx, "function").color;
    assert!(function.is_some(), "the theme colours function names");
    assert_eq!(name.color, function);
}

#[gpui_kit::test]
fn without_syntax_the_top_panes_carry_no_colour(cx: &mut TestAppContext) {
    let view = rust_merge(cx, Syntax::Plain);
    cx.run_until_parked();
    let cell = left_cell(cx, &view);
    assert!(cell.iter().all(|(_, style)| style.color.is_none()), "{cell:?}");
}

#[gpui_kit::test]
fn the_top_panes_draw_plain_until_the_parse_lands(cx: &mut TestAppContext) {
    let view = rust_merge(cx, Syntax::Highlighted);
    let before = left_cell(cx, &view);
    assert!(before.iter().all(|(_, style)| style.color.is_none()), "{before:?}");
    cx.run_until_parked();
    assert_eq!(at(&left_cell(cx, &view), FN), capture(cx, "keyword"));
}

fn keyword_colour(theme: &HighlightTheme) -> Option<gpui_kit::Hsla> {
    theme.style.syntax.style("keyword").and_then(|style| style.color)
}

fn themed_merge(
    cx: &mut TestAppContext,
    theme: merge_config::settings::Theme,
) -> gpui_kit::Entity<MergeWindow> {
    let settings =
        WindowSettings { file_names: three_way("a.rs"), theme, ..WindowSettings::default() };
    let (_, view, _) = open_with(cx, doc("fn a() {}\n", "fn b() {}\n", "fn a() {}\n"), &settings);
    cx.run_until_parked();
    view
}

#[test]
fn the_themes_colour_keywords_apart() {
    let light = keyword_colour(&HighlightTheme::default_light());
    assert!(light.is_some());
    assert_ne!(light, keyword_colour(&HighlightTheme::default_dark()));
}

#[gpui_kit::test]
fn the_syntax_colours_follow_the_theme_setting(cx: &mut TestAppContext) {
    let view = themed_merge(cx, merge_config::settings::Theme::Dark);
    assert_eq!(
        at(&left_cell(cx, &view), FN).color,
        keyword_colour(&HighlightTheme::default_dark())
    );
}

#[gpui_kit::test]
fn the_syntax_colours_follow_an_appearance_change(cx: &mut TestAppContext) {
    let view = themed_merge(cx, merge_config::settings::Theme::Light);
    assert_eq!(
        at(&left_cell(cx, &view), FN).color,
        keyword_colour(&HighlightTheme::default_light())
    );
    cx.update(|cx| gpui_kit::component::Theme::change(ThemeMode::Dark, None, cx));
    assert_eq!(
        at(&left_cell(cx, &view), FN).color,
        keyword_colour(&HighlightTheme::default_dark())
    );
}

fn capped(
    cx: &mut TestAppContext,
    result_lines: &str,
    max: usize,
) -> gpui_kit::Entity<MergeWindow> {
    let settings = WindowSettings {
        file_names: three_way("a.rs"),
        syntax_max_lines: max,
        ..WindowSettings::default()
    };
    let (_, view, _) = open_with(cx, doc(result_lines, result_lines, result_lines), &settings);
    cx.run_until_parked();
    view
}

fn language_of(cx: &mut TestAppContext, view: &gpui_kit::Entity<MergeWindow>) -> String {
    cx.read(|cx| view.read(cx).result_editor().read(cx).language_name().to_string())
}

const TWO_LINES: &str = "fn a() {}\nfn b() {}\n";

#[gpui_kit::test]
fn a_result_at_the_line_limit_is_highlighted_whole(cx: &mut TestAppContext) {
    let view = capped(cx, TWO_LINES, 2);
    assert_eq!(language_of(cx, &view), "rust");
    assert!(!cx.read(|cx| view.read(cx).result_chunked()));
}

#[gpui_kit::test]
fn a_result_past_the_line_limit_is_highlighted_a_chunk_at_a_time(cx: &mut TestAppContext) {
    let view = capped(cx, TWO_LINES, 1);
    assert_eq!(language_of(cx, &view), "rust");
    assert!(cx.read(|cx| view.read(cx).result_chunked()));
    assert_eq!(at(&left_cell(cx, &view), FN), capture(cx, "keyword"));
}

#[gpui_kit::test]
fn a_diff_side_past_the_line_limit_highlights_both_panes_a_chunk_at_a_time(
    cx: &mut TestAppContext,
) {
    let two = |path: &str| DiffFile {
        origin: Origin::File(PathBuf::from(path)),
        text: Text::decode(TWO_LINES.as_bytes().to_vec()).expect("decodes"),
        read_only: false,
    };
    let settings = DiffSettings { syntax_max_lines: 1, ..DiffSettings::default() };
    let view = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::diff_window::open(
            file(Origin::File(PathBuf::from("a.rs"))),
            two("b.rs"),
            &settings,
            cx,
        )
        .expect("window opens")
        .1
    });
    let languages = cx.read(|cx| {
        [Side::Left, Side::Right]
            .map(|side| view.read(cx).editor(side).read(cx).language_name().to_string())
    });
    assert_eq!(languages, ["rust", "rust"]);
    assert!(cx.read(|cx| view.read(cx).chunked()));
    let settings = DiffSettings { syntax_max_lines: 2, ..DiffSettings::default() };
    let view = cx.update(|cx| {
        merge_app::diff_window::open(two("a.rs"), two("b.rs"), &settings, cx)
            .expect("window opens")
            .1
    });
    let languages = cx.read(|cx| {
        [Side::Left, Side::Right]
            .map(|side| view.read(cx).editor(side).read(cx).language_name().to_string())
    });
    assert_eq!(languages, ["rust", "rust"]);
    assert!(!cx.read(|cx| view.read(cx).chunked()), "two lines are at the limit");
}

#[gpui_kit::test]
fn a_csharp_input_draws_its_keyword_in_the_keyword_colour(cx: &mut TestAppContext) {
    let settings = WindowSettings { file_names: three_way("a.cs"), ..WindowSettings::default() };
    let (_, view, _) =
        open_with(cx, doc("class A {}\n", "class B {}\n", "class A {}\n"), &settings);
    cx.run_until_parked();
    let cell = left_cell(cx, &view);
    assert_eq!(at(&cell, 0), capture(cx, "keyword"));
    assert_eq!(cell.first().map(|(range, _)| range.clone()), Some(0..5));
}

/// Each side renames `a`: a conflict on the one row.
fn conflict(cx: &mut TestAppContext, syntax: Syntax) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let settings =
        WindowSettings { file_names: three_way("a.rs"), syntax, ..WindowSettings::default() };
    let (window, view, _) =
        open_with(cx, doc("fn a() {}\n", "fn b() {}\n", "fn c() {}\n"), &settings);
    cx.run_until_parked();
    (window, view)
}

/// The background each byte of the left cell draws over its band.
fn backgrounds(cell: &CellHighlights, length: usize) -> Vec<Option<gpui_kit::Hsla>> {
    (0..length).map(|byte| at(cell, byte).background_color).collect()
}

const LINE_LENGTH: usize = "fn b() {}".len();

#[gpui_kit::test]
fn in_a_conflict_the_syntax_leaves_the_band_and_the_mark_as_they_are(cx: &mut TestAppContext) {
    let (_, plain) = conflict(cx, Syntax::Plain);
    let unstyled = left_cell(cx, &plain);
    let (_, view) = conflict(cx, Syntax::Highlighted);
    let styled = left_cell(cx, &view);
    // The renamed name is marked; nothing else draws a background over the band.
    let mark = at(&unstyled, NAME).background_color;
    assert!(mark.is_some(), "{unstyled:?}");
    assert_eq!(backgrounds(&unstyled, LINE_LENGTH)[FN], None);
    assert_eq!(backgrounds(&styled, LINE_LENGTH), backgrounds(&unstyled, LINE_LENGTH));
    assert_eq!(at(&styled, FN), capture(cx, "keyword"));
}

/// The bounds of the first rows' cells in every pane.
fn cell_bounds(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<Option<Bounds<Pixels>>> {
    let mut bounds = Vec::new();
    act(cx, window, |window, cx| {
        window.render_frame(cx);
        bounds = (0..PANES)
            .flat_map(|pane| (0..ROWS).map(move |row| (pane, row)))
            .map(|(pane, row)| {
                window.try_find(cell_id(pane, RowIx::new(row))).map(|cell| cell.bounds())
            })
            .collect();
    });
    bounds
}

/// Left and right; the base pane starts hidden.
const PANES: usize = 2;
const ROWS: usize = 3;

#[gpui_kit::test]
fn the_rows_keep_their_bounds_when_the_parse_lands(cx: &mut TestAppContext) {
    let settings = WindowSettings { file_names: three_way("a.rs"), ..WindowSettings::default() };
    let (window, _, _) = open_with(
        cx,
        doc("fn a() {}\n// x\nlet\n", "fn b() {}\n// x\nlet\n", "fn a() {}\n// y\nlet\n"),
        &settings,
    );
    let before = cell_bounds(cx, window);
    assert!(before.iter().all(Option::is_some), "{before:?}");
    cx.run_until_parked();
    assert_eq!(cell_bounds(cx, window), before);
}

fn toggle_by_button(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| window.click(Tool::Syntax.id(), cx));
    cx.run_until_parked();
}

fn toggle_by_menu(cx: &mut TestAppContext, window: AnyWindowHandle) {
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(ToggleSyntax), cx);
    });
    cx.run_until_parked();
}

fn rust_window(
    cx: &mut TestAppContext,
    syntax: Syntax,
    max: usize,
) -> (AnyWindowHandle, Entity<MergeWindow>) {
    let settings = WindowSettings {
        file_names: three_way("a.rs"),
        syntax,
        syntax_max_lines: max,
        ..WindowSettings::default()
    };
    let (window, view, _) =
        open_with(cx, doc("fn a() {}\n", "fn b() {}\n", "fn a() {}\n"), &settings);
    act(cx, window, TestWindowExt::render_frame);
    cx.run_until_parked();
    (window, view)
}

/// The result editor's language and the colour of the left cell's `fn`.
fn shown(cx: &mut TestAppContext, view: &Entity<MergeWindow>) -> (String, Option<gpui_kit::Hsla>) {
    (language_of(cx, view), at(&left_cell(cx, view), FN).color)
}

#[gpui_kit::test]
fn the_syntax_button_turns_the_colours_off_and_on(cx: &mut TestAppContext) {
    let (window, view) = rust_window(cx, Syntax::Highlighted, usize::MAX);
    let keyword = capture(cx, "keyword").color;
    assert_eq!(shown(cx, &view), ("rust".to_owned(), keyword));
    toggle_by_button(cx, window);
    assert_eq!(shown(cx, &view), (PLAIN.to_owned(), None));
    toggle_by_button(cx, window);
    assert_eq!(shown(cx, &view), ("rust".to_owned(), keyword));
}

#[gpui_kit::test]
fn the_menu_item_turns_on_a_window_opened_plain(cx: &mut TestAppContext) {
    let (window, view) = rust_window(cx, Syntax::Plain, usize::MAX);
    assert_eq!(shown(cx, &view), (PLAIN.to_owned(), None));
    toggle_by_menu(cx, window);
    assert_eq!(shown(cx, &view), ("rust".to_owned(), capture(cx, "keyword").color));
    toggle_by_menu(cx, window);
    assert_eq!(shown(cx, &view), (PLAIN.to_owned(), None));
}

#[gpui_kit::test]
fn turned_on_past_the_line_limit_every_pane_colours(cx: &mut TestAppContext) {
    // One result line over a limit of none.
    let (window, view) = rust_window(cx, Syntax::Plain, 0);
    assert!(cx.read(|cx| view.read(cx).result_chunked()));
    assert_eq!(shown(cx, &view), (PLAIN.to_owned(), None));
    toggle_by_button(cx, window);
    assert_eq!(shown(cx, &view), ("rust".to_owned(), capture(cx, "keyword").color));
}

#[gpui_kit::test]
fn a_file_without_a_language_stays_plain_when_turned_on(cx: &mut TestAppContext) {
    let settings = WindowSettings {
        file_names: three_way("a.txt"),
        syntax: Syntax::Plain,
        ..WindowSettings::default()
    };
    let (window, view, _) =
        open_with(cx, doc("fn a() {}\n", "fn b() {}\n", "fn a() {}\n"), &settings);
    toggle_by_button(cx, window);
    assert_eq!(shown(cx, &view), (PLAIN.to_owned(), None));
}

#[gpui_kit::test]
fn the_diff_windows_button_and_menu_item_turn_both_panes_off_and_on(cx: &mut TestAppContext) {
    let rust = |path: &str| DiffFile {
        origin: Origin::File(PathBuf::from(path)),
        text: Text::decode(TWO_LINES.as_bytes().to_vec()).expect("decodes"),
        read_only: false,
    };
    let settings = DiffSettings { syntax: Syntax::Plain, ..DiffSettings::default() };
    let (window, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        merge_app::init(cx);
        merge_app::diff_window::open(rust("a.rs"), rust("b.rs"), &settings, cx)
            .expect("window opens")
    });
    act(cx, window, TestWindowExt::render_frame);
    let languages = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            [Side::Left, Side::Right]
                .map(|side| view.read(cx).editor(side).read(cx).language_name().to_string())
        })
    };
    assert_eq!(languages(cx), [PLAIN, PLAIN]);
    toggle_by_button(cx, window);
    assert_eq!(languages(cx), ["rust", "rust"]);
    toggle_by_menu(cx, window);
    assert_eq!(languages(cx), [PLAIN, PLAIN]);
}

/// Lines of a long Rust merge: four chunks of `CHUNK_LINES`, none cut early, as no line
/// follows a blank one.
const LONG_LINES: usize = 7000;
/// The line, and row, of the long merge's one conflict: in the last chunk.
const FAR_LINE: usize = 6500;
/// A first conflict near the top, where the window opens.
const NEAR_LINE: usize = 1;

fn long_rust(edit: Option<&str>) -> String {
    (0..LONG_LINES)
        .map(|n| match edit {
            Some(name) if n == NEAR_LINE || n == FAR_LINE => format!("fn {name}() {{}}\n"),
            _ => format!("fn f{n}() {{}}\n"),
        })
        .collect()
}

#[gpui_kit::test]
fn a_row_or_pane_past_the_end_has_no_highlights(cx: &mut TestAppContext) {
    let (_, view, _) = open_with(cx, doc("a\n", "a\n", "a\n"), &WindowSettings::default());
    let shown = |pane, row| {
        cx.update(|cx| {
            view.update(cx, |view, cx| view.cell_highlights(pane, RowIx::new(row), cx).is_some())
        })
    };
    assert!(shown(LEFT_PANE, 0));
    assert!(!shown(LEFT_PANE, 1));
    assert!(!shown(9, 0));
}

fn row_keyword(cx: &mut TestAppContext, view: &Entity<MergeWindow>, row: usize) -> HighlightStyle {
    let cell = cx.update(|cx| {
        view.update(cx, |view, cx| view.cell_highlights(LEFT_PANE, RowIx::new(row), cx))
            .expect("the left pane has the line")
            .1
    });
    at(&cell, FN)
}

#[gpui_kit::test]
fn only_the_chunks_on_screen_parse_until_the_rows_move(cx: &mut TestAppContext) {
    let settings = WindowSettings {
        file_names: three_way("a.rs"),
        syntax: Syntax::Highlighted,
        ..WindowSettings::default()
    };
    let doc = doc(&long_rust(None), &long_rust(Some("left")), &long_rust(Some("right")));
    let (window, view, _) = open_with(cx, doc, &settings);
    act(cx, window, TestWindowExt::render_frame);
    cx.run_until_parked();
    let keyword = capture(cx, "keyword");
    assert_eq!(row_keyword(cx, &view, FIRST_ROW), keyword);
    assert_eq!(
        row_keyword(cx, &view, FAR_LINE),
        HighlightStyle::default(),
        "the last chunk is far from the screen"
    );
    act(cx, window, |window, cx| {
        window.click(merge_app::toolbar::Tool::NextConflict.id(), cx);
    });
    act(cx, window, TestWindowExt::render_frame);
    cx.run_until_parked();
    assert_eq!(row_keyword(cx, &view, FAR_LINE), keyword);
}
