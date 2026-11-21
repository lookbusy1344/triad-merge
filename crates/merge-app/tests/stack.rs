//! Headless tests of the stack window (spec §7.5.1): the conflicted jj commits, each path
//! opening in the merge window, a refresh after each save, the preview, and the exit code.

mod jj {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::common::tool_window::{LIST_INSET, LIST_TOP, ROW_HEIGHT, assert_frame};
    use crate::common::{act, bounds, is_selected, label, viewport_width};
    use gpui_kit::base::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, App, Entity, Role, TestAppContext, VisualTestContext};
    use merge_app::stack_window::{
        self, EMPTY_ID, PREVIEW_ID, Preview, REFRESH_ID, SUMMARY_ID, StackStart, StackWindow,
        commit_id, path_id,
    };
    use merge_app::window::{inherited_pane_pill_id, inherited_pill_id};
    use merge_app::{ExitCode, MergeWindow, WindowSettings};
    use merge_core::document::MergeOptions;
    use merge_core::index::RegionIx;
    use vcs_jj::config::ConfigEnv;
    use vcs_jj::testing::{Committed, Fixture, Held};

    const SAVE: &str = "Save";
    const SHORT: usize = 8;
    /// `merge`'s place in the list: the oldest of three commits, listed last.
    const MERGE: usize = 2;

    fn short(id: &str) -> &str {
        &id[..SHORT]
    }

    /// `merge` of `left` and `right` conflicts in `a.txt`; `child` adds `b.txt`; the working
    /// copy sits on `child`, on disk, and inherits the conflict with it.
    fn stacked() -> (Fixture, Committed, Committed) {
        let fixture = Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
        let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
        let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
        let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
        let child = fixture.commit_merged(&[&merge], "child", &[("b.txt", b"b\n")]);
        fixture.check_out(&[&child]);
        fixture.check_out_to_disk();
        (fixture, merge, child)
    }

    struct Started {
        window: AnyWindowHandle,
        view: Entity<StackWindow>,
        exits: Rc<RefCell<Vec<ExitCode>>>,
    }

    /// The stack window at `fixture`'s root on the default revset, recording the codes it
    /// exits with, once its list has landed.
    fn start(cx: &mut TestAppContext, fixture: &Fixture) -> Started {
        start_on(cx, fixture, None)
    }

    /// [`start`] on `revset`.
    fn start_on(cx: &mut TestAppContext, fixture: &Fixture, revset: Option<&str>) -> Started {
        let exits = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&exits);
        let start = StackStart {
            cwd: fixture.root().as_std_path().to_owned(),
            revset: revset.map(str::to_owned),
            options: MergeOptions::default(),
            settings: WindowSettings::default(),
            discovery: merge_session::repository::Discovery::new(ConfigEnv::default()),
        };
        let (window, view) = cx.update(|cx| {
            gpui_kit::init(cx);
            merge_app::init(cx);
            stack_window::start(start, move |code, _: &mut App| record.borrow_mut().push(code), cx)
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

    /// Each open merge window.
    fn merge_windows(cx: &mut TestAppContext) -> Vec<AnyWindowHandle> {
        cx.update(|cx| {
            cx.windows()
                .into_iter()
                .filter(|w| {
                    w.downcast::<Root>()
                        .and_then(|root| root.read(cx).ok())
                        .is_some_and(|root| root.view().clone().downcast::<MergeWindow>().is_ok())
                })
                .collect()
        })
    }

    /// The listed commits' change IDs, shortened, each with its paths and their inherited
    /// marks.
    fn listed(
        cx: &mut TestAppContext,
        view: &Entity<StackWindow>,
    ) -> Vec<(String, Vec<(String, bool)>)> {
        cx.read(|cx| {
            view.read(cx)
                .commits()
                .iter()
                .map(|c| {
                    let change = c.change.change_id.clone().expect("jj names the change");
                    let paths =
                        c.paths.iter().map(|p| (p.path.to_string(), p.inherited())).collect();
                    (short(&change).to_owned(), paths)
                })
                .collect()
        })
    }

    #[gpui_kit::test]
    fn the_window_lists_the_conflicted_commits_newest_first(cx: &mut TestAppContext) {
        let (fixture, merge, child) = stacked();
        let wc = fixture.working_copy_commit();
        let started = start(cx, &fixture);
        let a = |inherited| vec![("a.txt".to_owned(), inherited)];
        assert_eq!(
            listed(cx, &started.view),
            [
                (short(&wc.change_id).to_owned(), a(true)),
                (short(&child.change_id).to_owned(), a(true)),
                (short(&merge.change_id).to_owned(), a(false)),
            ]
        );
        assert_eq!(merge_windows(cx), []);
    }

    #[gpui_kit::test]
    fn a_path_opens_its_conflict_and_a_save_lists_the_commits_again(cx: &mut TestAppContext) {
        let (fixture, merge, _) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(MERGE, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        press(cx, merging, "secondary-alt-down");
        press(cx, merging, "secondary-1");
        press(cx, merging, "secondary-q");
        cx.simulate_prompt_answer(SAVE);
        cx.run_until_parked();
        assert_eq!(merge_windows(cx), []);
        assert_eq!(
            fixture.held_in(&fixture.latest(&merge), "a.txt"),
            Held::File(b"left\n".to_vec())
        );
        // The descendants inherited the conflict and lose it with it.
        assert_eq!(listed(cx, &started.view), []);
        let note = cx.read(|cx| started.view.read(cx).note().map(str::to_owned));
        assert_eq!(note, Some(format!("Resolved a.txt in {}.", short(&merge.change_id))));
        assert_eq!(started.exits.borrow().as_slice(), [], "the run goes on");
    }

    fn note(cx: &mut TestAppContext, started: &Started) -> Option<String> {
        cx.read(|cx| started.view.read(cx).note().map(str::to_owned))
    }

    /// What `read` gives of the one open merge window.
    fn merging<T>(cx: &mut TestAppContext, read: impl FnOnce(&MergeWindow) -> T) -> T {
        let windows = merge_windows(cx);
        assert_eq!(windows.len(), 1, "one merge window");
        let root = windows.first().and_then(AnyWindowHandle::downcast::<Root>).expect("a root");
        cx.read(|cx| {
            let view = root.read(cx).expect("the root").view().clone();
            let view = view.downcast::<MergeWindow>().expect("a merge window");
            read(view.read(cx))
        })
    }

    /// The banner of the one open merge window.
    fn banner(cx: &mut TestAppContext) -> Option<String> {
        merging(cx, |window| window.notice().map(ToString::to_string))
    }

    /// The conflict regions the one open merge window draws as inherited.
    fn inherited(cx: &mut TestAppContext) -> Vec<RegionIx> {
        merging(cx, |window| {
            let doc = window.document();
            doc.region_ixs().filter(|&r| window.inherited().contains(r)).collect()
        })
    }

    /// The inherited pill with id `id` in the one open merge window: its role and label.
    fn pill(
        cx: &mut TestAppContext,
        id: &gpui_kit::ElementId,
    ) -> Option<(Option<Role>, Option<String>)> {
        let windows = merge_windows(cx);
        assert_eq!(windows.len(), 1, "one merge window");
        let mut found = None;
        for window in windows {
            act(cx, window, |window, cx| {
                window.render_frame(cx);
                found =
                    window.try_find(id.clone()).map(|e| (e.role(), e.label().map(str::to_owned)));
            });
        }
        found
    }

    #[gpui_kit::test]
    fn an_inherited_path_draws_its_parent_s_conflict_as_inherited(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(1, 0));
        // a.txt holds one region: the conflict of left and right over a.
        let conflict = RegionIx::new(0);
        assert_eq!(inherited(cx), [conflict]);
        let shown = Some((Some(Role::Label), Some("inherited".to_owned())));
        assert_eq!(pill(cx, &inherited_pill_id(conflict)), shown);
        // The left and right panes, 0 and 1 with the base pane hidden, carry it too.
        assert_eq!(pill(cx, &inherited_pane_pill_id(0, conflict)), shown);
        assert_eq!(pill(cx, &inherited_pane_pill_id(1, conflict)), shown);
    }

    #[gpui_kit::test]
    fn a_picked_inherited_conflict_loses_its_pill(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(1, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        press(cx, merging, "secondary-alt-down");
        press(cx, merging, "secondary-1");
        assert_eq!(pill(cx, &inherited_pill_id(RegionIx::new(0))), None);
        assert_eq!(pill(cx, &inherited_pane_pill_id(0, RegionIx::new(0))), None);
    }

    #[gpui_kit::test]
    fn a_conflict_that_arises_in_its_commit_draws_as_its_own(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(MERGE, 0));
        assert_eq!(inherited(cx), []);
        assert_eq!(pill(cx, &inherited_pill_id(RegionIx::new(0))), None);
        assert_eq!(pill(cx, &inherited_pane_pill_id(0, RegionIx::new(0))), None);
    }

    #[gpui_kit::test]
    fn an_inherited_path_opens_with_a_banner_naming_where_its_conflict_arises(
        cx: &mut TestAppContext,
    ) {
        let (fixture, merge, child) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(1, 0));
        let (change, origin) = (short(&child.change_id), short(&merge.change_id));
        assert_eq!(
            banner(cx),
            Some(format!(
                "a.txt in {change} inherits its conflict from {origin}. Resolving it in \
                 {origin} first clears it here; a resolution saved here first will likely \
                 conflict again."
            ))
        );
        assert_eq!(note(cx, &started), None, "the stack window says nothing");
    }

    #[gpui_kit::test]
    fn a_path_whose_conflict_arises_in_its_own_commit_opens_without_a_banner(
        cx: &mut TestAppContext,
    ) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(MERGE, 0));
        assert_eq!(banner(cx), None);
    }

    #[gpui_kit::test]
    fn a_conflict_from_outside_the_list_opens_without_a_banner(cx: &mut TestAppContext) {
        let (fixture, _, child) = stacked();
        let started = start_on(cx, &fixture, Some(&format!("{}::", child.commit)));
        assert_eq!(listed(cx, &started.view).len(), 2);
        click(cx, started.window, path_id(1, 0));
        assert_eq!(banner(cx), None);
    }

    #[gpui_kit::test]
    fn a_second_path_while_a_merge_window_is_open_opens_nothing(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(0, 0));
        let open = merge_windows(cx);
        assert_eq!(open.len(), 1);
        click(cx, started.window, path_id(1, 0));
        assert_eq!(merge_windows(cx), open);
    }

    #[gpui_kit::test]
    fn a_cancelled_merge_window_closes_and_the_list_stays(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        let before = listed(cx, &started.view);
        click(cx, started.window, path_id(0, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        // Nothing changed, so Cancel ends the merge without asking.
        press(cx, merging, "secondary-w");
        cx.run_until_parked();
        assert_eq!(merge_windows(cx), []);
        assert_eq!(listed(cx, &started.view), before);
        assert_eq!(cx.read(|cx| started.view.read(cx).note().map(str::to_owned)), None);
    }

    #[gpui_kit::test]
    fn preview_shows_each_descendants_conflicts_and_writes_nothing(cx: &mut TestAppContext) {
        let (fixture, merge, child) = stacked();
        let wc = fixture.working_copy_commit();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(MERGE, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        press(cx, merging, "secondary-alt-down");
        press(cx, merging, "secondary-1");
        let operations = fixture.operation_count();
        click(cx, started.window, PREVIEW_ID.into());
        let preview = cx.read(|cx| started.view.read(cx).preview().cloned());
        let Some(Preview::Done(rewrite)) = preview else {
            panic!("a finished preview, got {preview:?}")
        };
        assert!(rewrite.resolved);
        assert!(!rewrite.written);
        let effects: Vec<_> = rewrite
            .rewritten
            .iter()
            .map(|d| (d.change.change_id.clone(), d.before.len(), d.after.len()))
            .collect();
        assert_eq!(
            effects,
            [
                (Some(short(&child.change_id).to_owned()), 1, 0),
                (Some(short(&wc.change_id).to_owned()), 1, 0)
            ]
        );
        assert_eq!(fixture.held_in(&merge, "a.txt"), Held::Conflict);
        assert_eq!(fixture.operation_count(), operations);
    }

    #[gpui_kit::test]
    fn preview_needs_an_open_merge_window(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, PREVIEW_ID.into());
        assert_eq!(cx.read(|cx| started.view.read(cx).preview().cloned()), None);
    }

    #[gpui_kit::test]
    fn closing_the_window_ends_the_run_with_1_while_conflicts_are_left(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        assert!(VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Conflicts]);
    }

    /// jj's shortest conflict markers.
    const JJ_MARKER_LENGTH: usize = 7;

    #[gpui_kit::test]
    fn closing_while_a_refresh_runs_waits_for_its_list_and_exits_by_it(cx: &mut TestAppContext) {
        let (fixture, merge, _) = stacked();
        let started = start(cx, &fixture);
        // Resolved outside the window: the refresh lists no commit.
        vcs_jj::JjRepository::discover(fixture.root())
            .expect("the repository")
            .with_config(ConfigEnv::default())
            .write_conflict(
                &vcs::CommitId::new(merge.commit.clone()),
                "a.txt".as_ref(),
                b"m\n",
                JJ_MARKER_LENGTH,
                false,
            )
            .expect("the resolution is written");
        act(cx, started.window, TestWindowExt::render_frame);
        act(cx, started.window, |window, cx| window.click(REFRESH_ID, cx));
        assert!(!VisualTestContext::from_window(started.window, cx).simulate_close());
        assert_eq!(started.exits.borrow().as_slice(), []);
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
    }

    #[gpui_kit::test]
    fn closing_the_window_after_every_conflict_resolves_ends_with_0(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(MERGE, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        press(cx, merging, "secondary-alt-down");
        press(cx, merging, "secondary-1");
        press(cx, merging, "secondary-q");
        cx.simulate_prompt_answer(SAVE);
        cx.run_until_parked();
        assert!(VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Resolved]);
    }

    #[gpui_kit::test]
    fn the_window_stays_open_while_its_merge_window_is(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(0, 0));
        assert!(!VisualTestContext::from_window(started.window, cx).simulate_close());
        cx.run_until_parked();
        assert_eq!(started.exits.borrow().as_slice(), []);
        assert_eq!(merge_windows(cx).len(), 1);
    }

    #[gpui_kit::test]
    fn the_window_has_the_tool_window_frame(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        assert_frame(cx, started.window, SUMMARY_ID, PREVIEW_ID, commit_id(0));
        let refresh = bounds(cx, started.window, REFRESH_ID);
        assert!(refresh.right() < bounds(cx, started.window, PREVIEW_ID).left());
    }

    #[gpui_kit::test]
    fn the_summary_counts_the_commits(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        let count = listed(cx, &started.view).len();
        assert_eq!(label(cx, started.window, SUMMARY_ID), format!("{count} commits"));
    }

    #[gpui_kit::test]
    fn escape_closes_the_window_and_ends_the_run_with_1_while_conflicts_are_left(
        cx: &mut TestAppContext,
    ) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        press(cx, started.window, "escape");
        assert_eq!(started.exits.borrow().as_slice(), [ExitCode::Conflicts]);
        assert!(!cx.update(|cx| cx.windows().contains(&started.window)), "the window closed");
    }

    #[gpui_kit::test]
    fn headings_and_paths_are_rows_across_the_list(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        let width = viewport_width(cx, started.window) - LIST_INSET - LIST_INSET;
        let heading = bounds(cx, started.window, commit_id(0));
        let path = bounds(cx, started.window, path_id(0, 0));
        assert_eq!((heading.size.width, heading.size.height), (width, ROW_HEIGHT));
        assert_eq!((path.size.width, path.size.height), (width, ROW_HEIGHT));
        assert_eq!(path.left(), LIST_INSET);
        assert_eq!(path.top(), heading.bottom());
    }

    #[gpui_kit::test]
    fn down_selects_the_first_path_and_enter_opens_it(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        assert!(!is_selected(cx, started.window, path_id(0, 0)));
        press(cx, started.window, "down");
        assert!(is_selected(cx, started.window, path_id(0, 0)));
        assert_eq!(merge_windows(cx), []);
        press(cx, started.window, "enter");
        assert_eq!(merge_windows(cx).len(), 1);
    }

    #[gpui_kit::test]
    fn the_selection_moves_across_commits_and_stops_at_either_end(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        // Three commits of one path each.
        for _ in 0..4 {
            press(cx, started.window, "down");
        }
        assert!(is_selected(cx, started.window, path_id(2, 0)));
        assert!(!is_selected(cx, started.window, path_id(0, 0)));
        for _ in 0..4 {
            press(cx, started.window, "up");
        }
        assert!(is_selected(cx, started.window, path_id(0, 0)));
        assert!(!is_selected(cx, started.window, path_id(2, 0)));
    }

    #[gpui_kit::test]
    fn up_with_nothing_selected_selects_the_last_path(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        press(cx, started.window, "up");
        assert!(is_selected(cx, started.window, path_id(2, 0)));
    }

    #[gpui_kit::test]
    fn enter_with_nothing_selected_opens_nothing(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        press(cx, started.window, "enter");
        assert_eq!(merge_windows(cx), []);
    }

    #[gpui_kit::test]
    fn an_empty_list_says_so_where_the_list_starts(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start_on(cx, &fixture, Some("root()"));
        assert_eq!(listed(cx, &started.view), []);
        assert_eq!(bounds(cx, started.window, EMPTY_ID).top(), LIST_TOP);
    }
}
