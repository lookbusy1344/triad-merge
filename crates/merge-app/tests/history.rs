//! Headless tests of the history window (spec §7.5.4): a file's versions across jj's
//! operation log, each opening in the diff window against the working-copy file, a refresh
//! after the diff window closes, and the exit code.

mod jj {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::common::act;
    use gpui_kit::base::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, App, Entity, TestAppContext, VisualTestContext};
    use merge_app::diff_window::{DiffWindow, copy_id};
    use merge_app::history_window::{self, HistoryStart, HistoryWindow, version_id};
    use merge_app::{ExitCode, WindowSettings};
    use merge_core::diff2::Side;
    use merge_core::index::RegionIx;
    use merge_session::jj_native::VersionState;
    use vcs_jj::config::ConfigEnv;
    use vcs_jj::testing::Fixture;

    const SHORT: usize = vcs_jj::SHORT_OPERATION;

    /// A working copy on disk whose `a.txt` was snapshotted as `1\n`, then as `2\n`. Gives
    /// the two snapshots' operations, oldest first.
    fn edited() -> (Fixture, [String; 2]) {
        let fixture = Fixture::new();
        fixture.check_out_to_disk();
        let ops = [&b"1\n"[..], b"2\n"].map(|bytes| {
            std::fs::write(fixture.root().join("a.txt"), bytes).expect("a.txt is written");
            let mut repo = vcs_jj::JjRepository::discover(fixture.root()).expect("the repository");
            vcs::Repository::snapshot(&mut repo).expect("the snapshot");
            fixture.operation_ids()[0].clone()
        });
        (fixture, ops)
    }

    struct Started {
        window: AnyWindowHandle,
        view: Entity<HistoryWindow>,
        exits: Rc<RefCell<Vec<ExitCode>>>,
    }

    /// The history window on `a.txt` at `fixture`'s root, recording the codes it exits with,
    /// once its list has landed.
    fn start(cx: &mut TestAppContext, fixture: &Fixture, limit: Option<usize>) -> Started {
        let exits = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&exits);
        let start = HistoryStart {
            cwd: fixture.root().as_std_path().to_owned(),
            path: "a.txt".into(),
            limit,
            settings: WindowSettings::default(),
            jj_config: Some(ConfigEnv::default()),
            watch: false,
        };
        let (window, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            merge_app::init(cx);
            history_window::start(
                start,
                move |code, _: &mut App| record.borrow_mut().push(code),
                cx,
            )
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

    fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
        act(cx, window, |window, cx| window.press(key, cx));
        cx.run_until_parked();
    }

    /// Each open diff window.
    fn diff_windows(cx: &mut TestAppContext) -> Vec<(AnyWindowHandle, Entity<DiffWindow>)> {
        cx.update(|cx| {
            cx.windows()
                .into_iter()
                .filter_map(|w| {
                    let root = w.downcast::<Root>()?.read(cx).ok()?;
                    let view = root.view().clone().downcast::<DiffWindow>().ok()?;
                    Some((w, view))
                })
                .collect()
        })
    }

    /// `side`'s text and title in `view`.
    fn side(cx: &mut TestAppContext, view: &Entity<DiffWindow>, side: Side) -> (String, String) {
        cx.read(|cx| {
            let view = view.read(cx);
            (view.document().text(side).to_string(), view.title(side).to_string())
        })
    }

    /// The listed versions' operations and states.
    fn listed(
        cx: &mut TestAppContext,
        view: &Entity<HistoryWindow>,
    ) -> Vec<(String, VersionState)> {
        cx.read(|cx| {
            view.read(cx).versions().iter().map(|v| (v.operation.clone(), v.state)).collect()
        })
    }

    fn note(cx: &mut TestAppContext, view: &Entity<HistoryWindow>) -> Option<String> {
        cx.read(|cx| view.read(cx).note().map(str::to_owned))
    }

    #[gpui_kit::test]
    fn the_window_lists_each_version_newest_first(cx: &mut TestAppContext) {
        let (fixture, [first, second]) = edited();
        let started = start(cx, &fixture, None);
        let versions = listed(cx, &started.view);
        let states: Vec<_> = versions.iter().map(|(_, state)| *state).collect();
        assert_eq!(states, [VersionState::File, VersionState::File, VersionState::Absent]);
        let ops: Vec<_> = versions.iter().take(2).map(|(op, _)| op.clone()).collect();
        assert_eq!(ops, [second, first]);
        assert_eq!(diff_windows(cx).len(), 0);
        assert_eq!(note(cx, &started.view), None);
    }

    #[gpui_kit::test]
    fn a_limit_lists_the_newest_versions(cx: &mut TestAppContext) {
        let (fixture, [_, second]) = edited();
        let started = start(cx, &fixture, Some(1));
        assert_eq!(listed(cx, &started.view), [(second, VersionState::File)]);
    }

    #[gpui_kit::test]
    fn a_version_opens_in_the_diff_window_against_the_working_copy(cx: &mut TestAppContext) {
        let (fixture, [first, _]) = edited();
        let started = start(cx, &fixture, None);
        click(cx, started.window, version_id(1));
        let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
        let title = format!("@ at operation {}", &first[..SHORT]);
        assert_eq!(side(cx, view, Side::Left), ("1\n".to_owned(), title));
        assert_eq!(side(cx, view, Side::Right).0, "2\n");
    }

    #[gpui_kit::test]
    fn an_absent_version_opens_with_an_empty_left_side(cx: &mut TestAppContext) {
        let (fixture, _) = edited();
        let started = start(cx, &fixture, None);
        click(cx, started.window, version_id(2));
        let [(_, view)] = &diff_windows(cx)[..] else { panic!("one diff window") };
        let (text, title) = side(cx, view, Side::Left);
        assert_eq!(text, "");
        assert!(title.ends_with(" (no file)"), "{title}");
    }

    #[gpui_kit::test]
    fn a_version_copied_back_and_saved_restores_it_and_the_list_grows(cx: &mut TestAppContext) {
        let (fixture, ops) = edited();
        let started = start(cx, &fixture, None);
        click(cx, started.window, version_id(1));
        let [(diff, _)] = diff_windows(cx)[..] else { panic!("one diff window") };
        click(cx, diff, copy_id(RegionIx::new(0), Side::Right));
        press(cx, diff, "secondary-s");
        press(cx, diff, "secondary-w");
        assert_eq!(diff_windows(cx).len(), 0);
        let on_disk = std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads");
        assert_eq!(on_disk, b"1\n");
        // The refresh snapshots the restored file: a new version at the top.
        let versions = listed(cx, &started.view);
        assert_eq!(versions.len(), 4, "{versions:?}");
        assert!(!ops.contains(&versions[0].0), "a new operation: {versions:?}");
        assert_eq!(versions[0].1, VersionState::File);
        assert_eq!(started.exits.borrow().as_slice(), [], "the run goes on");
    }

    #[gpui_kit::test]
    fn a_second_version_while_a_diff_window_is_open_opens_nothing(cx: &mut TestAppContext) {
        let (fixture, _) = edited();
        let started = start(cx, &fixture, None);
        click(cx, started.window, version_id(1));
        let open: Vec<_> = diff_windows(cx).into_iter().map(|(w, _)| w).collect();
        assert_eq!(open.len(), 1);
        click(cx, started.window, version_id(0));
        let now: Vec<_> = diff_windows(cx).into_iter().map(|(w, _)| w).collect();
        assert_eq!(now, open);
    }

    #[gpui_kit::test]
    fn a_working_copy_file_gone_from_disk_says_why_and_opens_nothing(cx: &mut TestAppContext) {
        let (fixture, _) = edited();
        let started = start(cx, &fixture, None);
        let file = fixture.root().join("a.txt");
        std::fs::remove_file(&file).expect("a.txt is removed");
        click(cx, started.window, version_id(1));
        assert_eq!(diff_windows(cx).len(), 0);
        let said = note(cx, &started.view).expect("a note");
        let root = dunce::canonicalize(fixture.root()).expect("the root");
        let missing = root.join("a.txt");
        assert!(said.starts_with(&format!("cannot find {}: ", missing.display())), "{said}");
        // The window is free to open another version.
        std::fs::write(&file, b"3\n").expect("a.txt is written");
        click(cx, started.window, version_id(1));
        assert_eq!(diff_windows(cx).len(), 1);
        assert_eq!(note(cx, &started.view), None);
    }

    #[gpui_kit::test]
    fn closing_the_window_ends_the_run_with_0(cx: &mut TestAppContext) {
        let (fixture, _) = edited();
        let started = start(cx, &fixture, None);
        assert!(VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
    }

    #[gpui_kit::test]
    fn the_window_stays_open_while_its_diff_window_is(cx: &mut TestAppContext) {
        let (fixture, _) = edited();
        let started = start(cx, &fixture, None);
        click(cx, started.window, version_id(1));
        assert!(!VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), []);
        assert_eq!(diff_windows(cx).len(), 1);
    }
}
