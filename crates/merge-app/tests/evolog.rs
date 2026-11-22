//! Headless tests of the evolog window (spec §7.5.4): a change's versions, newest first, each
//! opening the directory window on the change's edits since that version, and the exit code.

mod jj {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::common::tool_window::assert_frame;
    use crate::common::{act, label};
    use gpui_kit::base::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, App, Entity, TestAppContext, VisualTestContext};
    use merge_app::dir_window::{DirWindow, SUMMARY_ID as DIR_SUMMARY};
    use merge_app::evolog_window::{
        self, EvologStart, EvologWindow, NOTE_ID, REFRESH_ID, SUMMARY_ID, version_id,
    };
    use merge_app::{ExitCode, WindowSettings};
    use merge_core::diff::DiffOptions;
    use merge_session::dir_diff::{Change, Content};
    use vcs_jj::config::ConfigEnv;
    use vcs_jj::testing::{Committed, Fixture};

    /// The characters of an ID jj shows in a small repository: its shortest unique prefix,
    /// at least 8.
    const SHORT: usize = 8;

    /// `parent` holding `a.txt`, `child` on it adding `b.txt` as `x\n`; `parent` rewritten,
    /// which rebases `child`; then `child` edited to hold `b.txt` as `y\n`. Gives the child's
    /// versions, oldest first.
    fn edited_after_a_rebase(fixture: &Fixture) -> [Committed; 3] {
        let parent = fixture.commit_on(&[], "parent", &[("a.txt", b"1\n")]);
        let child = fixture.commit_on(&[&parent], "child", &[("a.txt", b"1\n"), ("b.txt", b"x\n")]);
        fixture.amend(&parent, &[("a.txt", b"0\n")]);
        let rebased = fixture.latest(&child);
        let edited = fixture.amend(&rebased, &[("a.txt", b"0\n"), ("b.txt", b"y\n")]);
        [child, rebased, edited]
    }

    struct Started {
        window: AnyWindowHandle,
        view: Entity<EvologWindow>,
        exits: Rc<RefCell<Vec<ExitCode>>>,
    }

    /// The evolog window on `change`, recording the codes it exits with, once its list has
    /// landed.
    fn start(cx: &mut TestAppContext, fixture: &Fixture, change: &str) -> Started {
        let exits = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&exits);
        let start = EvologStart {
            cwd: fixture.root().as_std_path().to_owned(),
            rev: Some(change.to_owned()),
            limit: None,
            listed: None,
            diff: DiffOptions::default(),
            settings: WindowSettings::default(),
            discovery: merge_session::repository::Discovery::new(ConfigEnv::default()),
            watch: false,
        };
        let (window, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            merge_app::init(cx);
            evolog_window::start(start, move |code, _: &mut App| record.borrow_mut().push(code), cx)
                .expect("the window opens")
        });
        cx.run_until_parked();
        Started { window, view, exits }
    }

    fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: gpui_kit::ElementId) {
        act(cx, window, TestWindowExt::render_frame);
        act(cx, window, |window, cx| window.click(id, cx));
        cx.run_until_parked();
    }

    /// Each open directory window.
    fn dir_windows(cx: &mut TestAppContext) -> Vec<(AnyWindowHandle, Entity<DirWindow>)> {
        cx.update(|cx| {
            cx.windows()
                .into_iter()
                .filter_map(|w| {
                    let root = w.downcast::<Root>()?.read(cx).ok()?;
                    let view = root.view().clone().downcast::<DirWindow>().ok()?;
                    Some((w, view))
                })
                .collect()
        })
    }

    fn commits(cx: &mut TestAppContext, view: &Entity<EvologWindow>) -> Vec<String> {
        cx.read(|cx| {
            view.read(cx).versions().iter().map(|v| v.commit.as_str().to_owned()).collect()
        })
    }

    fn note(cx: &mut TestAppContext, view: &Entity<EvologWindow>) -> Option<String> {
        cx.read(|cx| view.read(cx).note().map(str::to_owned))
    }

    #[gpui_kit::test]
    fn the_window_lists_the_change_s_versions_newest_first(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, rebased, edited] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        assert_eq!(commits(cx, &started.view), [edited.commit, rebased.commit, child.commit]);
        assert_eq!(label(cx, started.window, SUMMARY_ID), "3 versions");
        assert_eq!(dir_windows(cx).len(), 0);
        assert_eq!(note(cx, &started.view), None);
    }

    #[gpui_kit::test]
    fn a_version_opens_the_change_s_own_edits_since_it(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, ..] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        // The oldest version: the rebase's edit to `a.txt` is taken out.
        click(cx, started.window, version_id(2));
        let [(window, dir)] = &dir_windows(cx)[..] else { panic!("one directory window") };
        let files: Vec<_> = cx.read(|cx| {
            dir.read(cx)
                .files()
                .iter()
                .map(|f| (f.display_path(), f.change.clone(), f.content))
                .collect()
        });
        assert_eq!(files, [("b.txt".to_owned(), Change::Modified, Content::Text)]);
        let since = &child.commit[..SHORT];
        assert_eq!(label(cx, *window, DIR_SUMMARY), format!("1 file since {since}"));
        assert_eq!(note(cx, &started.view), None);
    }

    #[gpui_kit::test]
    fn the_current_version_opens_no_window_and_says_so(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, ..] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        click(cx, started.window, version_id(0));
        assert_eq!(dir_windows(cx).len(), 0);
        let shown = note(cx, &started.view);
        assert_eq!(shown.as_deref(), Some("No difference from the current version."));
        assert_eq!(label(cx, started.window, NOTE_ID), "No difference from the current version.");
    }

    #[gpui_kit::test]
    fn a_second_version_brings_the_open_directory_window_forward(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, ..] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        click(cx, started.window, version_id(2));
        click(cx, started.window, version_id(1));
        assert_eq!(dir_windows(cx).len(), 1);
    }

    #[gpui_kit::test]
    fn closing_the_directory_window_lets_another_version_open(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, ..] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        click(cx, started.window, version_id(2));
        let [(dir, _)] = dir_windows(cx)[..] else { panic!("one directory window") };
        act(cx, dir, |window, cx| window.press("escape", cx));
        cx.run_until_parked();
        assert_eq!(dir_windows(cx).len(), 0);
        assert_eq!(started.exits.borrow().as_slice(), [], "the run goes on");
        click(cx, started.window, version_id(1));
        assert_eq!(dir_windows(cx).len(), 1);
    }

    #[gpui_kit::test]
    fn the_window_stays_open_while_its_directory_window_is(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, ..] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        click(cx, started.window, version_id(2));
        assert!(!VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), []);
    }

    #[gpui_kit::test]
    fn closing_the_window_ends_the_run_with_0(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, ..] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        assert!(VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
    }

    #[gpui_kit::test]
    fn a_revision_naming_no_commit_shows_the_error(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, "none()");
        assert_eq!(commits(cx, &started.view), Vec::<String>::new());
        let shown = note(cx, &started.view).expect("an error");
        assert!(shown.contains("none()"), "{shown}");
    }

    #[gpui_kit::test]
    fn the_window_has_the_tool_window_frame(cx: &mut TestAppContext) {
        let fixture = Fixture::new();
        let [child, ..] = edited_after_a_rebase(&fixture);
        let started = start(cx, &fixture, &child.change_id);
        assert_frame(cx, started.window, SUMMARY_ID, REFRESH_ID, version_id(0));
    }
}
