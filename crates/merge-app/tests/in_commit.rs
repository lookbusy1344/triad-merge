//! Headless tests of a save into a jj commit from the merge window: the save asks first when
//! it changes the conflicts of a descendant (spec §7.5.1).

mod jj {
    use std::path::Path;

    use crate::common::{self, act};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, TestAppContext};
    use merge_app::{ExitCode, WindowSettings};
    use merge_core::document::MergeOptions;
    use vcs_jj::config::ConfigEnv;
    use vcs_jj::testing::{Committed, Fixture, Held};

    const SAVE: &str = "Save";
    const KEEP_EDITING: &str = "Keep Editing";
    const SAVE_WITH_CONFLICTS: &str = "Save with Conflicts";
    const TWO_CHANGED: &str = "Saving changes the conflicts of 2 descendants.";

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

    /// The merge window on `a.txt` in `commit`, as `resolve -r` opens it.
    fn open_in(
        cx: &mut TestAppContext,
        fixture: &Fixture,
        commit: &Committed,
    ) -> (AnyWindowHandle, common::Finished) {
        let opened = merge_session::resolve::load_at_commit(
            fixture.root().as_std_path(),
            &commit.commit,
            Path::new("a.txt"),
            Some(ConfigEnv::default()),
        )
        .expect("it loads")
        .open(MergeOptions::default())
        .expect("labels fit");
        let settings = WindowSettings {
            output: Some(opened.file.into_std_path_buf()),
            record: opened.record,
            repo: opened.repo,
            ..WindowSettings::default()
        };
        let (window, _, finished) = common::open_with(cx, opened.doc, &settings);
        (window, finished)
    }

    /// The merge window on `a.txt` in `commit`, with its settings as `resolve -r` and the
    /// stack window build them, journalled in `journal`.
    fn open_journalled(
        cx: &mut TestAppContext,
        fixture: &Fixture,
        commit: &Committed,
        journal: &Path,
    ) -> (AnyWindowHandle, gpui_kit::Entity<merge_app::MergeWindow>) {
        let root = fixture.root().as_std_path();
        let opened = merge_session::resolve::load_at_commit(
            root,
            &commit.commit,
            Path::new("a.txt"),
            Some(ConfigEnv::default()),
        )
        .expect("it loads")
        .open(MergeOptions::default())
        .expect("labels fit");
        let base =
            WindowSettings { journal_dir: Some(journal.to_owned()), ..WindowSettings::default() };
        let repo = merge_session::repository::RunRepository::find([root]);
        let (doc, settings) = merge_app::resolve_queue::window_settings(opened, root, repo, base);
        let (window, view, _) = common::open_with(cx, doc, &settings);
        cx.run_until_parked();
        (window, view)
    }

    fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
        act(cx, window, |window, cx| window.press(key, cx));
        cx.run_until_parked();
    }

    /// Take the left side of the one conflict.
    fn pick_left(cx: &mut TestAppContext, window: AnyWindowHandle) {
        press(cx, window, "secondary-alt-down");
        press(cx, window, "secondary-1");
    }

    fn short(id: &str) -> &str {
        const SHORT: usize = 8;
        &id[..SHORT]
    }

    #[gpui_kit::test]
    fn a_save_that_changes_descendants_asks_and_names_them(cx: &mut TestAppContext) {
        let (fixture, merge, child) = stacked();
        let wc = fixture.working_copy_commit();
        let (window, finished) = open_in(cx, &fixture, &merge);
        pick_left(cx, window);
        press(cx, window, "secondary-s");
        let (message, detail) = cx.pending_prompt().expect("the descendants' question");
        assert_eq!(message, TWO_CHANGED);
        assert_eq!(
            detail,
            format!(
                "{}: a.txt -> none\n{}: a.txt -> none",
                short(&child.change_id),
                short(&wc.change_id)
            )
        );
        assert_eq!(fixture.held_in(&fixture.latest(&merge), "a.txt"), Held::Conflict);
        cx.simulate_prompt_answer(SAVE);
        cx.run_until_parked();
        assert_eq!(finished.codes(), [ExitCode::Resolved]);
        assert_eq!(
            fixture.held_in(&fixture.latest(&merge), "a.txt"),
            Held::File(b"left\n".to_vec())
        );
    }

    #[gpui_kit::test]
    fn keep_editing_writes_nothing_and_keeps_the_window(cx: &mut TestAppContext) {
        let (fixture, merge, _) = stacked();
        let operations = fixture.operation_count();
        let (window, finished) = open_in(cx, &fixture, &merge);
        pick_left(cx, window);
        press(cx, window, "secondary-s");
        cx.simulate_prompt_answer(KEEP_EDITING);
        cx.run_until_parked();
        assert!(!cx.has_pending_prompt());
        assert_eq!(finished.codes(), []);
        assert_eq!(fixture.held_in(&fixture.latest(&merge), "a.txt"), Held::Conflict);
        assert_eq!(fixture.operation_count(), operations);
        // The result is unlocked: a second save asks again.
        press(cx, window, "secondary-s");
        let (message, _) = cx.pending_prompt().expect("the question again");
        assert_eq!(message, TWO_CHANGED);
    }

    #[gpui_kit::test]
    fn a_save_that_rebases_descendants_with_the_same_conflicts_does_not_ask(
        cx: &mut TestAppContext,
    ) {
        // `child` merges `merge` with a third branch, so a.txt stays conflicted there with
        // either resolution of `merge`.
        let fixture = Fixture::new();
        let base = fixture.commit_on(&[], "base", &[("a.txt", b"a\n")]);
        let left = fixture.commit_on(&[&base], "left", &[("a.txt", b"left\n")]);
        let right = fixture.commit_on(&[&base], "right", &[("a.txt", b"right\n")]);
        let other = fixture.commit_on(&[&base], "other", &[("a.txt", b"other\n")]);
        let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
        let child = fixture.commit_merged(&[&merge, &other], "child", &[]);
        fixture.check_out(&[&child]);
        fixture.check_out_to_disk();
        let (window, finished) = open_in(cx, &fixture, &merge);
        pick_left(cx, window);
        press(cx, window, "secondary-s");
        assert!(!cx.has_pending_prompt());
        assert_eq!(finished.codes(), [ExitCode::Resolved]);
        assert_eq!(fixture.held_in(&fixture.latest(&child), "a.txt"), Held::Conflict);
    }

    #[gpui_kit::test]
    fn a_save_that_changes_no_descendant_writes_without_asking(cx: &mut TestAppContext) {
        let (fixture, merge, _) = stacked();
        let (window, finished) = open_in(cx, &fixture, &merge);
        press(cx, window, "secondary-s");
        // The markers read back as the conflict the commit holds: nothing rebases.
        let (message, _) = cx.pending_prompt().expect("the unresolved question");
        assert_eq!(message, "Save with unresolved conflicts?");
        cx.simulate_prompt_answer(SAVE_WITH_CONFLICTS);
        cx.run_until_parked();
        assert!(!cx.has_pending_prompt());
        assert_eq!(finished.codes(), [ExitCode::Conflicts]);
    }

    #[gpui_kit::test]
    fn a_merge_in_a_commit_restores_after_a_crash(cx: &mut TestAppContext) {
        let (fixture, merge, _) = stacked();
        let journal = tempfile::tempdir().expect("tempdir");
        let (window, view) = open_journalled(cx, &fixture, &merge, journal.path());
        pick_left(cx, window);
        cx.executor().advance_clock(merge_app::window::JOURNAL_DEBOUNCE);
        cx.run_until_parked();
        let picked = cx.read(|cx| view.read(cx).document().text());
        assert_eq!(picked, "left\n");
        // The first window never finishes: a crash. The run after it loads the same conflict
        // into a new temporary file.
        let (_, after) = open_journalled(cx, &fixture, &merge, journal.path());
        assert!(cx.has_pending_prompt(), "a restore is offered");
        cx.simulate_prompt_answer("Restore");
        cx.run_until_parked();
        assert_eq!(cx.read(|cx| after.read(cx).document().text()), picked);
    }
}
