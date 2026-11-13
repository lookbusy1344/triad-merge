//! Headless tests of the stack window (spec §7.5.1): the conflicted jj commits, each path
//! opening in the merge window, a refresh after each save, the preview, and the exit code.

mod jj {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::common::act;
    use gpui_kit::base::Root;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, App, Entity, TestAppContext, VisualTestContext};
    use merge_app::stack_window::{self, PREVIEW_ID, Preview, StackStart, StackWindow, path_id};
    use merge_app::{ExitCode, MergeWindow, WindowSettings};
    use merge_core::document::MergeOptions;
    use vcs_jj::config::ConfigEnv;
    use vcs_jj::testing::{Committed, Fixture, Held};

    const SAVE: &str = "Save";
    const SHORT: usize = 8;

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
        let exits = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&exits);
        let start = StackStart {
            cwd: fixture.root().as_std_path().to_owned(),
            revset: None,
            options: MergeOptions::default(),
            settings: WindowSettings::default(),
            jj_config: Some(ConfigEnv::default()),
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
                    let paths = c.paths.iter().map(|p| (p.path.to_string(), p.inherited)).collect();
                    (short(&change).to_owned(), paths)
                })
                .collect()
        })
    }

    #[gpui_kit::test]
    fn the_window_lists_the_conflicted_commits_parents_first(cx: &mut TestAppContext) {
        let (fixture, merge, child) = stacked();
        let wc = fixture.working_copy_commit();
        let started = start(cx, &fixture);
        let a = |inherited| vec![("a.txt".to_owned(), inherited)];
        assert_eq!(
            listed(cx, &started.view),
            [
                (short(&merge.change_id).to_owned(), a(false)),
                (short(&child.change_id).to_owned(), a(true)),
                (short(&wc.change_id).to_owned(), a(true)),
            ]
        );
        assert_eq!(merge_windows(cx), []);
    }

    #[gpui_kit::test]
    fn a_path_opens_its_conflict_and_a_save_lists_the_commits_again(cx: &mut TestAppContext) {
        let (fixture, merge, _) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(0, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        press(cx, merging, "secondary-alt-down");
        press(cx, merging, "secondary-1");
        press(cx, merging, "secondary-s");
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
        click(cx, started.window, path_id(0, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        press(cx, merging, "secondary-alt-down");
        press(cx, merging, "secondary-1");
        let operations = fixture.operation_count();
        click(cx, started.window, PREVIEW_ID.into());
        let preview = cx.read(|cx| started.view.read(cx).preview().cloned());
        let Some(Preview::Done(Some(rewrite))) = preview else {
            panic!("a finished preview, got {preview:?}")
        };
        assert!(rewrite.resolved);
        assert!(!rewrite.written);
        let effects: Vec<_> = rewrite
            .descendants
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

    #[gpui_kit::test]
    fn closing_the_window_after_every_conflict_resolves_ends_with_0(cx: &mut TestAppContext) {
        let (fixture, ..) = stacked();
        let started = start(cx, &fixture);
        click(cx, started.window, path_id(0, 0));
        let [merging] = merge_windows(cx)[..] else { panic!("one merge window") };
        press(cx, merging, "secondary-alt-down");
        press(cx, merging, "secondary-1");
        press(cx, merging, "secondary-s");
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
}
