//! The GUI forms of the command line (spec §6.1): load, open a window, and exit with the code
//! the window finishes with.

use std::path::{Path, PathBuf};
use std::time::Instant;

use gpui_kit::{App, px};
use merge_config::keybind::KeyBindings;
use merge_config::settings::Settings as Stored;
use merge_core::diff::DiffOptions;
use merge_core::document::{MergeDocument, MergeOptions};
use merge_core::text::Text;
use merge_session::WindowForm;
use merge_session::invocation::{Absorb, History, Resolve, Stack, Window};
use merge_session::repository::{Handoff, RunRepository};
use merge_session::resolve::Snapshot;
use merge_session::semantics::Caller;
use merge_view::status::FileNames;
use vcs::RepoKind;

use crate::absorb_window::{self, AbsorbStart};
use crate::assets::AppAssets;
use crate::diff_window::{self, DiffFile, DiffLookup, DiffSettings, Origin};
use crate::dir_window::{self, DirStart, Source};
use crate::history_window::{self, HistoryStart};
use crate::measure::{self, Measure};
use crate::resolve_queue::{self, Queue};
use crate::stack_window::{self, StackStart};
use crate::window::{OpenError, OriginLookup, Watching, WindowSettings};
use crate::{ExitCode, Finish, Typography, open_dialog, strings};

/// A diff window opened from the open dialog or the history window watches its files on
/// disk (spec §6.1).
const WATCH_FILES: bool = true;

/// Run a window form of the command line until its window finishes. Inputs load before the
/// window opens, so a bad input exits with 3 and a message, and no window shows. A merge
/// or diff window opens with the config file's bad lines in its banner. With
/// `TRIAD_MEASURE` set, a merge window measures itself from here (DESIGN.md §A6.7).
#[must_use]
pub fn run(form: WindowForm) -> ExitCode {
    let started = Instant::now();
    let WindowForm { window, settings: mut stored, settings_errors: errors, cwd, repo } = form;
    let system = sys_locale::get_locale();
    crate::set_language(stored.ui_language, system.as_deref());
    let bindings = std::mem::take(&mut stored.key_bindings);
    let base = WindowSettings { cwd: cwd.clone(), ..window_settings(&stored) };
    let dir = cwd.as_deref();
    match window {
        Window::Merge(merge) => {
            let measure = Measure::from_env(started).unwrap_or_else(|error| {
                eprintln!("{}: {error}; measuring nothing", strings::PROGRAM);
                None
            });
            let language = merge.language();
            let doc = match merge_session::load(&merge.files, merge.options, merge.labels, language)
            {
                Ok(doc) => doc,
                Err(error) => return report(&error.to_string()),
            };
            if measure.is_some() {
                let loaded = measure::span("load and merge", started.elapsed());
                eprintln!("{}: {loaded}", strings::MEASURE);
            }
            let files = &merge.files;
            let settings = WindowSettings {
                repo: repo.kind(),
                journal_name: Some(
                    Caller::new(&repo, dir, Some(&merge.output))
                        .journal_name(&merge.output, merge.title.as_deref()),
                ),
                origins: Some(origin_lookup(dir, &merge.output, merge.title.as_deref(), repo)),
                title: merge.title,
                destination: Some(merge_session::resolve::Destination::file(merge.output)),
                measure,
                file_names: FileNames::three_way(&files.base, &files.left, &files.right),
                ..base
            };
            launch_merge(doc, settings, errors, bindings)
        }
        Window::Resolve(resolve) => run_resolve(resolve, repo, base, errors, bindings),
        Window::OpenDialog => {
            launch(bindings, move |cx| open_dialog::open(base, WATCH_FILES, cx).map(drop))
        }
        Window::Diff(diff) => {
            let pair = ToolPair {
                paths: [diff.left, diff.right],
                absent: [None, None],
                readonly: [diff.readonly_left, diff.readonly_right],
                title: diff.title,
            };
            let ToolDiff { left, right, origins, kind, title } =
                match tool_diff(pair, repo, working_dir(dir)) {
                    Ok(loaded) => loaded,
                    Err(error) => return report(&error.to_string()),
                };
            let settings = diff_settings(title, diff.diff, base, origins, kind);
            launch_diff(left, right, settings, errors, bindings)
        }
        Window::DiffDirs(diff) => {
            let source = Source::Dirs {
                left: diff.left,
                right: diff.right,
                readonly: [diff.readonly_left, diff.readonly_right],
            };
            let start = DirStart {
                cwd: working_dir(dir),
                source,
                title: diff.title,
                diff: diff.diff,
                settings: base,
                watch: WATCH_FILES,
            };
            launch_dirs(start, bindings)
        }
        Window::DiffChanges { rev, diff, title } => {
            let source = Source::Changes { rev, jj_config: repo.jj_config().cloned() };
            let start = DirStart {
                cwd: working_dir(dir),
                source,
                title,
                diff,
                settings: base,
                watch: WATCH_FILES,
            };
            launch_dirs(start, bindings)
        }
        Window::DiffNative { path, rev, op, diff, title } => {
            let window = NativeDiff { path, rev, op, diff, title };
            run_native_diff(window, repo, base, errors, bindings)
        }
        Window::History(history) => run_history(history, &repo, base, bindings),
        Window::Stack(stack) => run_stack(stack, &repo, base, bindings),
        Window::Absorb(absorb) => run_absorb(absorb, &repo, working_dir(dir), base, bindings),
    }
}

/// Open the directory window on `start`.
fn launch_dirs(start: DirStart, bindings: KeyBindings) -> ExitCode {
    launch(bindings, move |cx| {
        dir_window::start(start, |code, _| exit(code), cx).map(drop)?;
        Ok(())
    })
}

/// The history window on `history`'s file, whose diff windows watch their files.
fn run_history(
    history: History,
    repo: &RunRepository,
    base: WindowSettings,
    bindings: KeyBindings,
) -> ExitCode {
    let start = HistoryStart {
        cwd: working_dir(base.cwd.as_deref()),
        path: history.path,
        limit: history.limit,
        jj_config: repo.jj_config().cloned(),
        settings: base,
        watch: WATCH_FILES,
    };
    launch(bindings, move |cx| {
        history_window::start(start, |code, _| exit(code), cx).map(drop)?;
        Ok(())
    })
}

/// The absorb window on `absorb`'s paths and `--into`, from the working directory `cwd`,
/// whose hunks' lines take `settings`' font and colours.
fn run_absorb(
    absorb: Absorb,
    repo: &RunRepository,
    cwd: PathBuf,
    settings: WindowSettings,
    bindings: KeyBindings,
) -> ExitCode {
    let start = AbsorbStart {
        cwd,
        paths: absorb.paths,
        into: absorb.into,
        settings,
        jj_config: repo.jj_config().cloned(),
    };
    launch(bindings, move |cx| {
        absorb_window::start(start, |code, _| exit(code), cx).map(drop)?;
        Ok(())
    })
}

/// The stack window on `stack`'s revset, whose merge windows read with the run's jj config.
fn run_stack(
    stack: Stack,
    repo: &RunRepository,
    base: WindowSettings,
    bindings: KeyBindings,
) -> ExitCode {
    let start = StackStart {
        cwd: working_dir(base.cwd.as_deref()),
        revset: stack.revset,
        options: stack.options,
        jj_config: repo.jj_config().cloned(),
        settings: base,
    };
    launch(bindings, move |cx| {
        stack_window::start(start, |code, _| exit(code), cx).map(drop)?;
        Ok(())
    })
}

/// The working-copy file that `right`, the right side of `jj diff --tool`, copies, so its
/// edits save there (spec §7.1). Only a path in jj's temporary directory opens `repo`.
fn jj_working_copy(repo: &mut RunRepository, right: &Path, cwd: &Path) -> Option<PathBuf> {
    merge_session::origin::jj_diff_copy(right, cwd)?;
    let found = match repo.open() {
        Ok(Some(open)) => merge_session::diff_tool::jj_working_copy(open, right, cwd),
        Ok(None) => Ok(None),
        Err(error) => Err(error.into()),
    };
    found.unwrap_or_else(|error| {
        // The window still opens on jj's copy, and its banner says edits there are lost.
        tracing::warn!(%error, "cannot tell whether the right side is the working copy");
        None
    })
}

/// The files `diff LEFT RIGHT` compares, as `git difftool` and `jj diff --tool` pass them.
pub(crate) struct ToolPair {
    pub paths: [PathBuf; 2],
    /// A side whose file is absent opens empty and read-only, titled so.
    pub absent: [Option<String>; 2],
    pub readonly: [bool; 2],
    /// `--title`: the window's title, and the file's path for the line origins.
    pub title: Option<String>,
}

/// A tool diff's sides, loaded, with its line origins' lookup, its repository's kind and
/// its window's title.
pub(crate) struct ToolDiff {
    pub left: DiffFile,
    pub right: DiffFile,
    pub origins: DiffLookup,
    pub kind: Option<RepoKind>,
    pub title: Option<String>,
}

/// Load `pair` from the working directory `cwd`, in the run's repository `repo`. A right
/// side that copies jj's working copy opens the working-copy file, and the left copy turns
/// read-only, since its edits would be lost (spec §7.1).
///
/// # Errors
///
/// When a side cannot be read or decoded.
pub(crate) fn tool_diff(
    pair: ToolPair,
    mut repo: RunRepository,
    cwd: PathBuf,
) -> Result<ToolDiff, merge_session::SessionError> {
    let ToolPair { paths: [left, right], absent: [left_absent, right_absent], readonly, title } =
        pair;
    let [readonly_left, readonly_right] = readonly;
    let working_copy = (!readonly_right && right_absent.is_none())
        .then(|| jj_working_copy(&mut repo, &right, &cwd))
        .flatten();
    let (right_file, readonly_left) = match working_copy {
        Some(file) => (file, true),
        None => (right.clone(), readonly_left),
    };
    let left_side = tool_side(left.clone(), left_absent, readonly_left)?;
    let right_side = tool_side(right_file, right_absent, readonly_right)?;
    let kind = repo.kind();
    let origins = DiffLookup::Tool {
        paths: [left, right],
        title: title.clone(),
        cwd,
        repo: Handoff::new(repo),
    };
    Ok(ToolDiff { left: left_side, right: right_side, origins, kind, title })
}

/// The side at `path`, or an empty read-only side titled `absent`.
fn tool_side(
    path: PathBuf,
    absent: Option<String>,
    read_only: bool,
) -> Result<DiffFile, merge_session::SessionError> {
    let Some(title) = absent else {
        let text = merge_session::read_text(&path)?;
        return Ok(DiffFile { origin: Origin::File(path), text, read_only });
    };
    Text::decode(Vec::new())
        .map(|text| DiffFile::revision(text, title))
        .map_err(|source| merge_session::SessionError::Decode { path, source })
}

/// The parts of a `diff PATH` window form.
struct NativeDiff {
    path: PathBuf,
    rev: Option<String>,
    op: Option<String>,
    diff: DiffOptions,
    title: Option<String>,
}

/// The diff window of `diff PATH`: the revision against the working-tree file, both read
/// from the run's repository `repo`, which the window keeps for the line origins.
fn run_native_diff(
    window: NativeDiff,
    mut repo: RunRepository,
    base: WindowSettings,
    errors: Vec<String>,
    bindings: KeyBindings,
) -> ExitCode {
    let NativeDiff { path, rev, op, diff, title } = window;
    let dir = base.cwd.as_deref();
    let kind = repo.kind();
    let loaded = match &op {
        Some(op) => merge_session::native::load_at_operation(&mut repo, &path, op)
            .map(|(native, commit)| (native, Some(commit.as_str().to_owned()))),
        None => merge_session::native::load(&mut repo, &path, rev.as_deref()).map(|n| (n, rev)),
    };
    let (native, rev) = match loaded {
        Ok(loaded) => loaded,
        Err(error) => return report(&error.to_string()),
    };
    let origins =
        DiffLookup::Native { path: working_dir(dir).join(&path), rev, repo: Handoff::new(repo) };
    let (left, right) = match native_sides(native) {
        Ok(sides) => sides,
        Err(error) => return report(&error.to_string()),
    };
    let settings = diff_settings(title, diff, base, origins, kind);
    launch_diff(left, right, settings, errors, bindings)
}

/// A `diff PATH` window's sides: `native`'s revision on the left, read-only, and the
/// working-tree file on the right, which saves.
///
/// # Errors
///
/// When the working-tree file cannot be read.
pub(crate) fn native_sides(
    native: merge_session::native::NativeDiff,
) -> Result<(DiffFile, DiffFile), merge_session::SessionError> {
    let text = merge_session::read_text(&native.right)?;
    let right = DiffFile { origin: Origin::File(native.right), text, read_only: false };
    let title = strings::revision_title(&native.left_at);
    Ok((DiffFile::revision(native.left, title), right))
}

/// The merge window on the conflict `resolve` names, read from the repository (spec §7.4).
fn run_resolve(
    mut resolve: Resolve,
    mut repo: RunRepository,
    base: WindowSettings,
    errors: Vec<String>,
    bindings: KeyBindings,
) -> ExitCode {
    let Some(target) = resolve.target.take() else {
        return report("resolve has no file to open");
    };
    let cwd = working_dir(base.cwd.as_deref());
    if resolve.path.is_none() {
        let queue = Queue::new(cwd, resolve.options, base, |code, _| exit(code));
        let queue = match repo.jj_config() {
            Some(config) => queue.with_jj_config(config.clone()),
            None => queue,
        };
        return launch(bindings, move |cx| {
            resolve_queue::start(queue, cx);
            Ok(())
        });
    }
    let loaded = match resolve.in_commit() {
        Some(rev) => {
            merge_session::resolve::load_at_commit(&cwd, rev, &target, repo.jj_config().cloned())
        }
        None => merge_session::resolve::load(&mut repo, &cwd, &target, Snapshot::Take),
    };
    let opened = match loaded
        .map_err(|e| e.to_string())
        .and_then(|loaded| loaded.open(resolve.options).map_err(|e| e.to_string()))
    {
        Ok(opened) => opened,
        Err(error) => return report(&error),
    };
    let (doc, settings) = resolve_queue::window_settings(opened, &cwd, repo, base);
    launch_merge(doc, settings, errors, bindings)
}

/// Open the merge window on `doc`, with the config file's bad lines, `errors`, in its banner.
fn launch_merge(
    doc: MergeDocument,
    settings: WindowSettings,
    errors: Vec<String>,
    bindings: KeyBindings,
) -> ExitCode {
    launch(bindings, move |cx| {
        let (_, view) = crate::window::open(doc, &settings, cx)?;
        view.update(cx, |view, cx| view.set_settings_errors(&errors, cx));
        Ok(())
    })
}

/// Open the diff window on `left` and `right`, with the config file's bad lines, `errors`,
/// in its banner.
fn launch_diff(
    left: DiffFile,
    right: DiffFile,
    settings: DiffSettings,
    errors: Vec<String>,
    bindings: KeyBindings,
) -> ExitCode {
    launch(bindings, move |cx| {
        let (_, view) = diff_window::open(left, right, &settings, cx)?;
        view.update(cx, |view, cx| view.set_settings_errors(&errors, cx));
        Ok(())
    })
}

/// A diff window's settings: the run's title, options and origins, and the settings file's
/// text, scrolling and origin column.
pub(crate) fn diff_settings(
    title: Option<String>,
    options: DiffOptions,
    base: WindowSettings,
    origins: DiffLookup,
    repo: Option<RepoKind>,
) -> DiffSettings {
    DiffSettings {
        working_copy: origins.working_copy(),
        title,
        options,
        watch: true,
        typography: base.typography,
        window_size: base.window_size,
        origins: Some(origins),
        origin_column: base.origin_column,
        revision_id: base.revision_id,
        theme: base.theme,
        colours: base.colours,
        repo,
        syntax: base.syntax,
        syntax_max_lines: base.syntax_max_lines,
        soft_wrap: base.soft_wrap,
        whitespace: base.whitespace,
        collapsing: base.collapsing,
        layout: base.diff_layout,
        scrollbars: base.scrollbars,
        cwd: base.cwd,
    }
}

/// The kind of the repository a run writing `file` lies in ([`RunRepository::of_merge`]):
/// the working directory's, else that of `file`'s directory.
pub(crate) fn repo_kind(cwd: Option<&Path>, file: &Path) -> Option<RepoKind> {
    RunRepository::of_merge(cwd, Some(file)).kind()
}

/// Where a merge run's line origins come from: its output and title, from the working
/// directory `cwd`, in the run's repository `repo`.
fn origin_lookup(
    cwd: Option<&Path>,
    output: &Path,
    title: Option<&str>,
    repo: RunRepository,
) -> OriginLookup {
    OriginLookup {
        output: output.to_owned(),
        title: title.map(str::to_owned),
        cwd: working_dir(cwd),
        repo: Handoff::new(repo),
    }
}

/// The working directory origins and conflicts are looked up from. Without one it is empty,
/// which finds no repository, so the lookup falls back to the files' own directories.
pub(crate) fn working_dir(cwd: Option<&Path>) -> PathBuf {
    cwd.map_or_default(Path::to_path_buf)
}

/// The window settings the settings file gives, with the platform's journal directory.
fn window_settings(stored: &Stored) -> WindowSettings {
    WindowSettings {
        journal_dir: merge_session::journal_dir(),
        typography: Typography {
            font: stored.font.clone().map(Into::into),
            size: px(stored.font_size),
            toolbar_size: px(stored.toolbar_font_size),
            tab_width: stored.tab_width,
            spacing: stored.line_height,
        },
        window_size: crate::window_size(stored.window_size),
        theme: stored.theme,
        colours: stored.colours,
        unresolved_save: stored.confirm_unresolved_save.into(),
        line_ending: stored.line_ending,
        syntax: stored.syntax.into(),
        syntax_max_lines: stored.syntax_max_lines,
        soft_wrap: stored.soft_wrap,
        whitespace: stored.show_whitespace.into(),
        collapsing: crate::diff_window::Collapsing {
            on: stored.collapse_unchanged,
            context_lines: stored.context_lines,
        },
        diff_layout: stored.diff_layout,
        scroll_sync: stored.scroll_sync,
        scrollbars: stored.scrollbars,
        base_pane: stored.base_pane,
        origin_column: stored.origin_column.into(),
        revision_id: stored.revision_id.into(),
        diff_options: stored.two_way(),
        options: MergeOptions {
            diff: stored.diff.clone(),
            refine: stored.refine,
            syntax_boundaries: stored.syntax_boundaries,
            marker_style: stored.conflict_style.map_or_else(Default::default, Into::into),
            ..MergeOptions::default()
        },
        watch_output: Watching::On,
        notice: None,
        ..WindowSettings::default()
    }
}

fn report(message: &str) -> ExitCode {
    eprintln!("{}: {}", strings::PROGRAM, merge_core::unusual::escaped(message));
    ExitCode::Error
}

/// Start the platform, open a window and exit the process when the window finishes.
fn launch(
    bindings: KeyBindings,
    open: impl FnOnce(&mut App) -> Result<(), OpenError> + 'static,
) -> ExitCode {
    gpui_kit::application().with_assets(AppAssets).run(move |cx| {
        gpui_kit::init(cx);
        crate::init_with(&bindings, cx);
        crate::prompt_view::install(cx);
        // The platform's own quit exits with 0 on macOS, so the process exits here instead.
        cx.set_global(Finish::new(|code, _| exit(code)));
        if let Err(error) = open(cx) {
            exit(report(&error.to_string()));
        }
        cx.activate(true);
    });
    // Where the platform's loop returns, it ended without a decision.
    ExitCode::Error
}

fn exit(code: ExitCode) -> ! {
    std::process::exit(i32::from(code as u8))
}

#[cfg(test)]
mod tests {
    use merge_core::diff::{
        Algorithm, BlankLineRule, CaseRule, DiffOptions, LineEndingRule, Whitespace,
    };
    use merge_core::merge3::Refine;
    use merge_core::pattern::Patterns;

    use super::*;
    use crate::origin_column::Visibility;

    #[test]
    fn the_repository_comes_from_the_file_when_the_working_directory_has_none() {
        let t = tempfile::tempdir().unwrap();
        let plain = t.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let git = vcs_git::testing::Fixture::new();
        assert_eq!(
            repo_kind(Some(&plain), &git.root().join("out.txt").into_std_path_buf()),
            Some(RepoKind::Git)
        );
        assert_eq!(repo_kind(Some(&plain), &plain.join("out.txt")), None);
        assert_eq!(repo_kind(None, &plain.join("out.txt")), None);
    }

    #[test]
    fn a_run_in_jj_names_its_journal_by_the_repositorys_file() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("repo");
        std::fs::create_dir_all(root.join(".jj")).unwrap();
        let output = t.path().join("jj-resolve-AAAA/output");
        let repo = RunRepository::of_merge(Some(&root), Some(&output));
        // Repository roots are canonical, as discovery gives them.
        assert_eq!(
            Caller::new(&repo, Some(&root), Some(&output)).journal_name(&output, Some("src/a.txt")),
            dunce::canonicalize(&root).unwrap().join("src/a.txt")
        );
    }

    #[test]
    fn the_working_directorys_repository_wins_over_the_files() {
        let t = tempfile::tempdir().unwrap();
        let (jj, git) = (t.path().join("jj"), t.path().join("git"));
        std::fs::create_dir_all(jj.join(".jj")).unwrap();
        std::fs::create_dir_all(git.join(".git")).unwrap();
        assert_eq!(repo_kind(Some(&jj), &git.join("out.txt")), Some(RepoKind::Jj));
        assert_eq!(repo_kind(None, &jj.join("out.txt")), Some(RepoKind::Jj));
    }

    #[test]
    fn the_diff_window_takes_the_files_colours_theme_syntax_revision_id_wrap_whitespace_folds_and_scrollbars()
     {
        let base = WindowSettings {
            theme: merge_config::settings::Theme::Dark,
            colours: merge_config::settings::Colours {
                right: merge_config::settings::Themed::both(merge_config::settings::Colour::rgb(
                    0x65_43_21,
                )),
                ..merge_config::settings::DEFAULT_COLOURS
            },
            syntax: crate::window::Syntax::Plain,
            revision_id: vcs::IdKind::Commit,
            soft_wrap: true,
            whitespace: Visibility::Shown,
            collapsing: crate::diff_window::Collapsing { on: true, context_lines: 0 },
            diff_layout: merge_config::settings::DiffLayout::Unified,
            scrollbars: merge_config::settings::Scrollbars::WhenScrolling,
            window_size: gpui_kit::size(px(1024.), px(768.)),
            ..WindowSettings::default()
        };
        let colours = base.colours;
        let lookup = DiffLookup::Native {
            path: PathBuf::from("f"),
            rev: None,
            repo: Handoff::new(RunRepository::none()),
        };
        let diff = diff_settings(None, DiffOptions::default(), base, lookup, None);
        assert_eq!(diff.colours, colours);
        assert_eq!(diff.theme, merge_config::settings::Theme::Dark);
        assert_eq!(diff.syntax, crate::window::Syntax::Plain);
        assert_eq!(diff.revision_id, vcs::IdKind::Commit);
        assert!(diff.soft_wrap);
        assert_eq!(diff.whitespace, Visibility::Shown);
        assert_eq!(diff.collapsing, crate::diff_window::Collapsing { on: true, context_lines: 0 });
        assert_eq!(diff.layout, merge_config::settings::DiffLayout::Unified);
        assert_eq!(diff.scrollbars, merge_config::settings::Scrollbars::WhenScrolling);
        assert_eq!(diff.window_size, gpui_kit::size(px(1024.), px(768.)));
    }

    #[test]
    fn every_window_setting_comes_from_the_file() {
        let stored = Stored {
            semantics: merge_config::settings::Choice::Jj,
            same_change: Some(merge_core::merge3::SameChange::Keep),
            hunk_level: Some(merge_core::merge3::HunkLevel::Word),
            diff: DiffOptions {
                algorithm: Algorithm::Myers,
                whitespace: Whitespace::All,
                line_endings: LineEndingRule::Ignore,
                case: CaseRule::Ignore,
                patterns: Patterns::new(["x"]).expect("valid"),
                blank_lines: BlankLineRule::Compare,
            },
            diff_line_endings: LineEndingRule::Compare,
            diff_blank_lines: BlankLineRule::Ignore,
            refine: Refine::Off,
            syntax_boundaries: true,
            font: Some("Menlo".to_owned()),
            font_size: 14.,
            toolbar_font_size: 11.5,
            window_size: merge_config::settings::WindowSize { width: 1600, height: 1000 },
            tab_width: 8,
            scroll_sync: false,
            scrollbars: merge_config::settings::Scrollbars::Hover,
            base_pane: true,
            origin_column: true,
            revision_id: merge_config::settings::RevisionId::Commit,
            theme: merge_config::settings::Theme::Dark,
            ui_language: merge_config::settings::UiLanguage::Ja,
            line_height: 2.5,
            colours: merge_config::settings::Colours {
                left: merge_config::settings::Themed::both(merge_config::settings::Colour::rgb(
                    0x12_34_56,
                )),
                ..merge_config::settings::DEFAULT_COLOURS
            },
            conflict_style: Some(merge_config::settings::ConflictStyle::Merge),
            line_ending: merge_core::document::OutputEnding::CrLf,
            confirm_unresolved_save: false,
            syntax: false,
            syntax_max_lines: 7,
            soft_wrap: true,
            show_whitespace: true,
            collapse_unchanged: true,
            context_lines: 9,
            diff_layout: merge_config::settings::DiffLayout::Unified,
            key_bindings: merge_config::keybind::KeyBindings::default(),
        };
        let settings = window_settings(&stored);
        assert_eq!(
            settings.typography,
            Typography {
                font: Some("Menlo".into()),
                size: px(14.),
                toolbar_size: px(11.5),
                tab_width: 8,
                spacing: 2.5,
            }
        );
        assert_eq!(settings.theme, merge_config::settings::Theme::Dark);
        assert_eq!(settings.colours, stored.colours);
        assert_eq!(settings.unresolved_save, merge_session::ending::UnresolvedSave::Write);
        assert!(!settings.scroll_sync);
        assert_eq!(settings.scrollbars, merge_config::settings::Scrollbars::Hover);
        assert_eq!(settings.line_ending, merge_core::document::OutputEnding::CrLf);
        assert_eq!(settings.syntax, crate::window::Syntax::Plain);
        assert_eq!(settings.syntax_max_lines, 7);
        assert!(settings.soft_wrap);
        assert_eq!(settings.whitespace, Visibility::Shown);
        assert_eq!(
            settings.collapsing,
            crate::diff_window::Collapsing { on: true, context_lines: 9 }
        );
        assert_eq!(settings.diff_layout, merge_config::settings::DiffLayout::Unified);
        assert!(settings.base_pane);
        assert_eq!(settings.origin_column, Visibility::Shown);
        assert_eq!(settings.revision_id, vcs::IdKind::Commit);
        assert_eq!(settings.options.diff, stored.diff);
        assert_eq!(
            settings.diff_options,
            DiffOptions {
                algorithm: Algorithm::Myers,
                whitespace: Whitespace::All,
                line_endings: LineEndingRule::Compare,
                case: CaseRule::Ignore,
                patterns: Patterns::new(["x"]).expect("valid"),
                blank_lines: BlankLineRule::Ignore,
            }
        );
        assert_eq!(settings.options.refine, Refine::Off);
        assert!(settings.options.syntax_boundaries);
        assert_eq!(settings.options.marker_style, merge_core::document::MarkerStyle::Merge);
        assert_eq!(settings.options.marker_length, MergeOptions::default().marker_length);
        assert_eq!(settings.window_size, gpui_kit::size(px(1600.), px(1000.)));
        assert_eq!(settings.journal_dir, merge_session::journal_dir());
    }
}
