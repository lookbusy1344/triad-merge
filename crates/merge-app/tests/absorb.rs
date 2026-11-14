//! Headless tests of the absorb window (spec §7.5.6): the working copy's hunks, each with
//! the commit it goes into, sent elsewhere or kept by a click, and written by Apply.

mod jj {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::common::tool_window::{EDGE, ROW_GAP, ROW_HEIGHT};
    use crate::common::{act, bounds, is_selected};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, App, Entity, TestAppContext, VisualTestContext};
    use merge_app::absorb_window::{
        self, APPLY_ID, AbsorbStart, AbsorbWindow, KEEP_ID, REFRESH_ID, destination_id, hunk_id,
        lines_id,
    };
    use merge_app::{ExitCode, Typography, WindowSettings};
    use vcs_jj::config::ConfigEnv;
    use vcs_jj::testing::{Committed, Fixture, Held};

    const A_BEFORE: &[u8] = b"1\n2\n3\n4\n5\n";
    const A_AFTER: &[u8] = b"1\n2\nTHREE\n4\n5\n";
    const B_BEFORE: &[u8] = b"x\ny\nz\n";
    const B_AFTER: &[u8] = b"x\nY\nz\n";

    /// `one` writes `a.txt`, `two` on it writes `b.txt`; the working copy sits on `two`,
    /// checked out to disk, with one line of each file changed.
    fn stack() -> (Fixture, Committed, Committed) {
        let fixture = Fixture::new();
        let one = fixture.commit_on(&[], "one", &[("a.txt", A_BEFORE)]);
        let two = fixture.commit_merged(&[&one], "two", &[("b.txt", B_BEFORE)]);
        fixture.check_out(&[&two]);
        fixture.check_out_to_disk();
        std::fs::write(fixture.root().join("a.txt"), A_AFTER).expect("a.txt is written");
        std::fs::write(fixture.root().join("b.txt"), B_AFTER).expect("b.txt is written");
        (fixture, one, two)
    }

    struct Started {
        window: AnyWindowHandle,
        view: Entity<AbsorbWindow>,
        exits: Rc<RefCell<Vec<ExitCode>>>,
    }

    /// The absorb window at `fixture`'s root over every path, recording the codes it exits
    /// with, once its list has landed.
    fn start(cx: &mut TestAppContext, fixture: &Fixture) -> Started {
        start_with(cx, fixture, WindowSettings::default())
    }

    /// [`start`] with `settings`.
    fn start_with(cx: &mut TestAppContext, fixture: &Fixture, settings: WindowSettings) -> Started {
        let exits = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&exits);
        let start = AbsorbStart {
            cwd: fixture.root().as_std_path().to_owned(),
            paths: Vec::new(),
            into: None,
            settings,
            jj_config: Some(ConfigEnv::default()),
        };
        let (window, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            merge_app::init(cx);
            absorb_window::start(start, move |code, _: &mut App| record.borrow_mut().push(code), cx)
                .expect("the window opens")
        });
        cx.run_until_parked();
        Started { window, view, exits }
    }

    fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
        act(cx, window, |window, cx| window.press(key, cx));
        cx.run_until_parked();
    }

    fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: gpui_kit::ElementId) {
        act(cx, window, TestWindowExt::render_frame);
        act(cx, window, |window, cx| window.click(id, cx));
        cx.run_until_parked();
    }

    /// Each hunk's path and the full ID of the commit it goes into now.
    fn targets(
        cx: &mut TestAppContext,
        view: &Entity<AbsorbWindow>,
    ) -> Vec<(String, Option<String>)> {
        cx.read(|cx| {
            let view = view.read(cx);
            view.hunks()
                .iter()
                .zip(view.targets())
                .map(|(hunk, target)| (hunk.path.to_string(), target.map(str::to_owned)))
                .collect()
        })
    }

    fn note(cx: &mut TestAppContext, view: &Entity<AbsorbWindow>) -> Option<String> {
        cx.read(|cx| view.read(cx).note().map(str::to_owned))
    }

    fn selected(cx: &mut TestAppContext, view: &Entity<AbsorbWindow>) -> Option<usize> {
        cx.read(|cx| view.read(cx).selected())
    }

    #[gpui_kit::test]
    fn the_window_lists_each_hunk_with_the_commit_that_wrote_its_lines(cx: &mut TestAppContext) {
        let (fixture, one, two) = stack();
        let started = start(cx, &fixture);
        assert_eq!(
            targets(cx, &started.view),
            [("a.txt".to_owned(), Some(one.commit)), ("b.txt".to_owned(), Some(two.commit))]
        );
        let texts: Vec<_> = cx.read(|cx| {
            started
                .view
                .read(cx)
                .hunks()
                .iter()
                .map(|h| (h.before.clone(), h.after.clone()))
                .collect()
        });
        assert_eq!(
            texts,
            [(b"3\n".to_vec(), b"THREE\n".to_vec()), (b"y\n".to_vec(), b"Y\n".to_vec())]
        );
        assert_eq!(selected(cx, &started.view), None);
    }

    #[gpui_kit::test]
    fn apply_sends_each_hunk_where_the_list_shows(cx: &mut TestAppContext) {
        let (fixture, one, two) = stack();
        let started = start(cx, &fixture);
        click(cx, started.window, APPLY_ID.into());
        assert_eq!(fixture.held_in(&fixture.latest(&one), "a.txt"), Held::File(A_AFTER.to_vec()));
        assert_eq!(fixture.held_in(&fixture.latest(&two), "b.txt"), Held::File(B_AFTER.to_vec()));
        assert_eq!(note(cx, &started.view).as_deref(), Some("Absorbed 2 hunks into 2 commits."));
        // The working copy has nothing left to absorb.
        assert_eq!(targets(cx, &started.view), []);
        assert_eq!(started.exits.borrow().as_slice(), [], "the run goes on");
    }

    #[gpui_kit::test]
    fn a_hunk_sent_to_another_commit_goes_there(cx: &mut TestAppContext) {
        let (fixture, one, two) = stack();
        let started = start(cx, &fixture);
        // The destinations are children first: `two`, then `one`.
        click(cx, started.window, hunk_id(0));
        assert_eq!(selected(cx, &started.view), Some(0));
        click(cx, started.window, destination_id(0));
        assert_eq!(targets(cx, &started.view)[0].1.as_deref(), Some(two.commit.as_str()));
        // The next hunk is selected for the next choice.
        assert_eq!(selected(cx, &started.view), Some(1));
        click(cx, started.window, APPLY_ID.into());
        assert_eq!(fixture.held_in(&fixture.latest(&one), "a.txt"), Held::File(A_BEFORE.to_vec()));
        assert_eq!(fixture.held_in(&fixture.latest(&two), "a.txt"), Held::File(A_AFTER.to_vec()));
        assert_eq!(fixture.held_in(&fixture.latest(&two), "b.txt"), Held::File(B_AFTER.to_vec()));
        assert_eq!(note(cx, &started.view).as_deref(), Some("Absorbed 2 hunks into 1 commit."));
    }

    #[gpui_kit::test]
    fn a_kept_hunk_stays_in_the_working_copy(cx: &mut TestAppContext) {
        let (fixture, one, two) = stack();
        let started = start(cx, &fixture);
        click(cx, started.window, hunk_id(1));
        click(cx, started.window, KEEP_ID.into());
        assert_eq!(targets(cx, &started.view)[1].1, None);
        // The last hunk has no next one: the selection stays.
        assert_eq!(selected(cx, &started.view), Some(1));
        click(cx, started.window, APPLY_ID.into());
        assert_eq!(fixture.held_in(&fixture.latest(&one), "a.txt"), Held::File(A_AFTER.to_vec()));
        assert_eq!(fixture.held_in(&fixture.latest(&two), "b.txt"), Held::File(B_BEFORE.to_vec()));
        assert_eq!(
            fixture.held_in(&fixture.working_copy_commit(), "b.txt"),
            Held::File(B_AFTER.to_vec())
        );
        assert_eq!(note(cx, &started.view).as_deref(), Some("Absorbed 1 hunk into 1 commit."));
        // Absorbing into `one` rebased `two`: the kept hunk now goes into its new commit.
        let two_now = fixture.latest(&two).commit;
        assert_eq!(targets(cx, &started.view), [("b.txt".to_owned(), Some(two_now))]);
    }

    #[gpui_kit::test]
    fn a_choice_without_a_selected_hunk_changes_nothing(cx: &mut TestAppContext) {
        let (fixture, ..) = stack();
        let started = start(cx, &fixture);
        let before = targets(cx, &started.view);
        click(cx, started.window, KEEP_ID.into());
        click(cx, started.window, destination_id(1));
        assert_eq!(targets(cx, &started.view), before);
    }

    #[gpui_kit::test]
    fn apply_after_the_repository_moved_on_writes_nothing_and_says_so(cx: &mut TestAppContext) {
        let (fixture, one, _) = stack();
        let started = start(cx, &fixture);
        let operation = fixture.operation_ids()[0].clone();
        // An edit snapshotted since the listing may renumber the hunks.
        std::fs::write(fixture.root().join("a.txt"), b"ONE\n2\nTHREE\n4\n5\n").expect("written");
        let mut repo = vcs_jj::JjRepository::discover(fixture.root()).expect("the repository");
        vcs::Repository::snapshot(&mut repo).expect("the snapshot");
        click(cx, started.window, APPLY_ID.into());
        assert_eq!(fixture.held_in(&fixture.latest(&one), "a.txt"), Held::File(A_BEFORE.to_vec()));
        let short = &operation[..vcs_jj::SHORT_OPERATION];
        assert_eq!(
            note(cx, &started.view),
            Some(format!(
                "checking the listing's operation: the repository changed since operation \
                 {short}; list the hunks again"
            ))
        );
        // The choices stay until a refresh lists the hunks again.
        assert_eq!(targets(cx, &started.view).len(), 2);
        click(cx, started.window, REFRESH_ID.into());
        let paths: Vec<_> = targets(cx, &started.view).into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, ["a.txt", "a.txt", "b.txt"]);
    }

    #[gpui_kit::test]
    fn closing_the_window_ends_the_run_with_0(cx: &mut TestAppContext) {
        let (fixture, one, _) = stack();
        let started = start(cx, &fixture);
        assert!(VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
        assert_eq!(fixture.held_in(&fixture.latest(&one), "a.txt"), Held::File(A_BEFORE.to_vec()));
    }

    #[gpui_kit::test]
    fn the_window_keeps_the_tool_window_edge_and_gaps(cx: &mut TestAppContext) {
        let (fixture, ..) = stack();
        let started = start(cx, &fixture);
        let refresh = bounds(cx, started.window, REFRESH_ID);
        assert_eq!((refresh.left(), refresh.top()), (EDGE, EDGE));
        let hunk = bounds(cx, started.window, hunk_id(0));
        assert_eq!(hunk.top(), refresh.bottom() + ROW_GAP);
        assert_eq!(hunk.size.height, ROW_HEIGHT);
        let commit = bounds(cx, started.window, destination_id(0));
        assert_eq!(commit.top(), hunk.top());
        assert_eq!(commit.size.height, ROW_HEIGHT);
    }

    #[gpui_kit::test]
    fn a_hunks_lines_take_the_editor_line_height(cx: &mut TestAppContext) {
        let (fixture, ..) = stack();
        let typography = Typography { size: gpui_kit::px(20.), ..Typography::default() };
        let line = typography.line_height();
        let settings = WindowSettings { typography, ..WindowSettings::default() };
        let started = start_with(cx, &fixture, settings);
        // `- 3` and `+ THREE`.
        assert_eq!(bounds(cx, started.window, lines_id(0)).size.height, line + line);
    }

    #[gpui_kit::test]
    fn the_keys_select_a_hunk_and_send_it_to_a_commit(cx: &mut TestAppContext) {
        let (fixture, one, two) = stack();
        let started = start(cx, &fixture);
        press(cx, started.window, "down");
        assert_eq!(selected(cx, &started.view), Some(0));
        assert!(is_selected(cx, started.window, hunk_id(0)));
        // Enter moves to the commits, on the one the hunk goes into: `one`, the second.
        press(cx, started.window, "enter");
        assert!(is_selected(cx, started.window, destination_id(1)));
        press(cx, started.window, "up");
        assert!(is_selected(cx, started.window, destination_id(0)));
        press(cx, started.window, "enter");
        assert_eq!(targets(cx, &started.view)[0].1.as_deref(), Some(two.commit.as_str()));
        assert_eq!(selected(cx, &started.view), Some(1));
        assert!(!is_selected(cx, started.window, destination_id(0)));
        // Back in the hunks: Up selects the first hunk again.
        press(cx, started.window, "up");
        assert_eq!(selected(cx, &started.view), Some(0));
        assert_eq!(targets(cx, &started.view)[1].1.as_deref(), Some(two.commit.as_str()));
        assert_ne!(one.commit, two.commit);
    }

    #[gpui_kit::test]
    fn down_past_the_commits_reaches_the_working_copy(cx: &mut TestAppContext) {
        let (fixture, ..) = stack();
        let started = start(cx, &fixture);
        press(cx, started.window, "down");
        press(cx, started.window, "enter");
        for _ in 0..3 {
            press(cx, started.window, "down");
        }
        assert!(is_selected(cx, started.window, KEEP_ID));
        press(cx, started.window, "enter");
        assert_eq!(targets(cx, &started.view)[0].1, None);
    }

    #[gpui_kit::test]
    fn escape_leaves_the_commits_and_changes_nothing(cx: &mut TestAppContext) {
        let (fixture, ..) = stack();
        let started = start(cx, &fixture);
        let before = targets(cx, &started.view);
        press(cx, started.window, "down");
        press(cx, started.window, "enter");
        press(cx, started.window, "up");
        press(cx, started.window, "escape");
        assert!(!is_selected(cx, started.window, destination_id(0)));
        // The keys move through the hunks again.
        press(cx, started.window, "down");
        assert_eq!(selected(cx, &started.view), Some(1));
        assert_eq!(targets(cx, &started.view), before);
    }
}
