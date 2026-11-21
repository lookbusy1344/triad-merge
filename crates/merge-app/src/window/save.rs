//! The save flow: Save, Save & Exit, Save Copy As, close and their alerts (spec §6.1).

use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, Context, PromptButton, PromptLevel, Window};
use merge_session::SaveError;
use merge_session::ending::{self, After, Closing, Next, Undone};
use merge_session::jj_native::CommitEffect;
use merge_session::session::{CannotSave, SaveJob};

use crate::strings::prompt;
use crate::{ExitCode, Save, SaveAndExit, SaveCopyAs, finish, ignore_closed_view, strings};

use super::{GO_AHEAD, MergeWindow, alert, closing_label, failed_label, undone_title};

impl MergeWindow {
    /// Write and record the output, unresolved conflicts as markers, and go on merging
    /// (spec §6.2). Without an output, ask for a path as Save Copy As does.
    pub(super) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.saving() {
            self.send_edits(window, cx);
        }
        self.step(self.session.save_in_place_requested(), window, cx);
    }

    /// Write the output and finish: with 0 when resolved, or with 1 after the user agrees
    /// to save with conflict markers (spec §6.2).
    pub(super) fn save_and_exit(
        &mut self,
        _: &SaveAndExit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.session.saving() {
            self.send_edits(window, cx);
        }
        self.step(self.session.save_requested(), window, cx);
    }

    /// Carry out the next step of ending the session (spec §6.2).
    pub(super) fn step(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        match next {
            Next::Stay => {}
            Next::ConfirmUnresolved { unresolved } => {
                self.confirm_unresolved(unresolved, window, cx);
            }
            Next::ConfirmClose => Self::confirm_close(window, cx),
            Next::SaveAndExit => self.save_and_exit(&SaveAndExit, window, cx),
            Next::Write(after) => self.check_descendants(after, window, cx),
            Next::Record(code, after) => self.record_again(code, after, window, cx),
            Next::SaveCopyAs => self.save_copy_as(&SaveCopyAs, window, cx),
            Next::Failed { undone, error, after } => {
                let detail = crate::error_text::describe(error.error());
                // The save wrote the journal first; a failed write tries again.
                self.write_journal(cx);
                Self::save_failed(undone, after, detail, window, cx);
            }
            Next::Saved(_) => {
                let unresolved = self.session.document().unresolved();
                self.save_note = Some(strings::saved(unresolved).into());
                cx.notify();
            }
            Next::SavedUnrecorded { error, .. } => {
                let error = error.error();
                tracing::warn!(%error, "the repository did not record the save");
                // The text says the output is written and why the record failed.
                self.save_note = Some(crate::error_text::describe(error).into());
                cx.notify();
            }
            Next::Refused { error } => {
                let detail = crate::error_text::describe(&error);
                // Nothing is written and the merge goes on, so its journal does too.
                self.schedule_journal(cx);
                alert(&prompt::cannot_write(), &detail, window, cx);
            }
            Next::Finish { code, journal } => {
                self.journal.cancel();
                self.session.finish_journal(journal);
                self.exit(code, window, cx);
            }
        }
    }

    /// Ask before writing markers for `unresolved` conflicts.
    pub(super) fn confirm_unresolved(
        &self,
        unresolved: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = window.prompt(
            PromptLevel::Warning,
            &prompt::save_unresolved(),
            Some(&prompt::save_unresolved_detail(
                unresolved,
                self.session.document().options().semantics,
            )),
            &[
                PromptButton::new(prompt::save_with_conflicts()),
                PromptButton::new(prompt::keep_editing()),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let next = ending::unresolved_answered(answer.await == Ok(GO_AHEAD));
            ignore_closed_view(this.update_in(cx, |this, window, cx| this.step(next, window, cx)));
        })
        .detach();
    }

    /// Ask for a path, starting in OUTPUT's directory, and write a copy there.
    pub(super) fn save_copy_as(
        &mut self,
        _: &SaveCopyAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_edits(window, cx);
        let start = self
            .session
            .output()
            .and_then(Path::parent)
            .map_or_else(PathBuf::new, Path::to_path_buf);
        let picked = cx.prompt_for_new_path(&start, None);
        cx.spawn_in(window, async move |this, cx| {
            // A cancelled or failed picker writes nothing.
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.save_copy(&path, window, cx);
            }));
        })
        .detach();
    }

    /// Write the output to `path` as Save & Exit would, unresolved conflicts as markers,
    /// without a confirmation (`docs/DESIGN.md` §E6), off the UI thread. The window stays open;
    /// OUTPUT, the exit code and the journal are unchanged. A failure shows an alert.
    pub fn save_copy(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.send_edits(window, cx);
        let (pending, unresolved) = self.session.copy();
        let path = path.to_owned();
        let before = self.copying.take();
        self.copying = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(before) = before {
                before.await;
            }
            let written = cx.background_spawn(async move { pending.write(&path) }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.copy_written(written, unresolved, window, cx);
            }));
        }));
    }

    pub(super) fn copy_written(
        &mut self,
        written: Result<ExitCode, SaveError>,
        unresolved: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match written {
            Ok(_) => {
                self.save_note = Some(strings::copy_saved(unresolved).into());
                cx.notify();
            }
            Err(error) => alert(
                &prompt::cannot_write_copy(),
                &crate::error_text::describe(&error),
                window,
                cx,
            ),
        }
    }

    /// Finish with `code` once the copies being written have landed.
    pub(super) fn exit(&mut self, code: ExitCode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(copying) = self.copying.take() else {
            finish(code, cx);
            return;
        };
        cx.spawn_in(window, async move |_, cx| {
            copying.await;
            ignore_closed_view(cx.update(|_, cx| finish(code, cx)));
        })
        .detach();
    }

    /// Before a save into a jj commit, preview it off the UI thread, and ask first when it
    /// changes the conflicts of a descendant (spec §7.5.1). Any other save writes at once.
    /// The result stays locked while the preview runs, so the save writes what it showed.
    /// The save then goes on as `after` says.
    pub(super) fn check_descendants(
        &mut self,
        after: After,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.save_task.is_some() {
            return;
        }
        let Some(job) = self.session.preview() else {
            return self.write(after, window, cx);
        };
        self.set_saving(true, cx);
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            let previewed = cx.background_spawn(async move { job.run() }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.save_task = None;
                this.set_saving(false, cx);
                let changed: Vec<CommitEffect> = match previewed {
                    Ok(rewrite) => rewrite
                        .rewritten
                        .into_iter()
                        .filter(|effect| effect.before != effect.after)
                        .collect(),
                    Err(error) => {
                        // The save meets the same refusal, or an encoding's, and reports it.
                        tracing::warn!(%error, "cannot preview the save into the commit");
                        Vec::new()
                    }
                };
                if changed.is_empty() {
                    this.write(after, window, cx);
                } else {
                    Self::confirm_descendants(&changed, after, window, cx);
                }
            }));
        }));
    }

    /// Ask before a save that changes the conflicts of the descendants in `changed`.
    pub(super) fn confirm_descendants(
        changed: &[CommitEffect],
        after: After,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = window.prompt(
            PromptLevel::Info,
            &prompt::descendants_changed(changed.len()),
            Some(&strings::descendant_effects(changed)),
            &[PromptButton::new(prompt::save()), PromptButton::new(prompt::keep_editing())],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let save = answer.await == Ok(GO_AHEAD);
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                if save {
                    this.write(after, window, cx);
                }
            }));
        })
        .detach();
    }

    /// Write the output and record the save off the UI thread, then go on as `after` says.
    pub(super) fn write(&mut self, after: After, window: &mut Window, cx: &mut Context<Self>) {
        // The save writes the journal first; its tasks stop.
        self.journal.cancel();
        match self.session.start_save(after) {
            Ok(job) => self.save_in_background(job, window, cx),
            Err(CannotSave::NoOutput) => Self::fail(&prompt::no_output(), window, cx),
            Err(CannotSave::Saving) => {}
        }
    }

    /// Record again, off the UI thread, a save whose output is written with `code`, without
    /// writing it again, then go on as `after` says.
    pub(super) fn record_again(
        &mut self,
        code: ExitCode,
        after: After,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Ok(job) = self.session.start_record(code, after) {
            self.save_in_background(job, window, cx);
        }
    }

    /// Run `job` on a background thread with the result locked, then go on or end the
    /// session as it says, or say why it failed.
    pub(super) fn save_in_background(
        &mut self,
        job: SaveJob,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_saving(true, cx);
        if let Some(watch) = &mut self.output_watch {
            watch.saving();
        }
        self.save_task = Some(cx.spawn_in(window, async move |this, cx| {
            let saved = cx.background_spawn(async move { job.run() }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.save_task = None;
                this.set_saving(false, cx);
                let next = this.session.saved(saved);
                // The session goes on: OUTPUT may hold the save's write now.
                if !matches!(next, Next::Finish { .. }) {
                    this.stamp_output(window, cx);
                }
                this.step(next, window, cx);
            }));
        }));
    }

    /// Lock the result while a save writes it, and unlock it after.
    pub(super) fn set_saving(&self, saving: bool, cx: &mut Context<Self>) {
        let locked = saving || self.asking_output;
        self.result.update(cx, |state, cx| state.set_readonly(locked, cx));
    }

    /// Lock the result while a save runs or the question about OUTPUT is open.
    pub(super) fn lock_result(&self, cx: &mut Context<Self>) {
        self.set_saving(self.session.saving(), cx);
    }

    /// Say why a save failed, and offer the answers `undone` allows: try again, which goes
    /// on as `after` says, save a copy, keep editing, or quit with 3 (spec §6.2). The window
    /// stays open until the user saves or quits; the journal holds the work meanwhile.
    pub(super) fn save_failed(
        undone: Undone,
        after: After,
        detail: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::error!(detail, "cannot save");
        let answers = undone.answers();
        let labels: Vec<PromptButton> =
            answers.iter().map(|&answer| PromptButton::new(failed_label(answer))).collect();
        let answer =
            window.prompt(PromptLevel::Critical, &undone_title(undone), Some(&detail), &labels, cx);
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let answer = answer.await.ok().and_then(|answer| answers.get(answer).copied());
            let next = ending::failure_answered(answer, undone, after);
            // Quit, or an alert that closed with its window, ends the session; so does a
            // window gone before the answer could reach it.
            let handled = !matches!(next, Next::Finish { .. })
                && handle
                    .update(cx, |_, window, cx| {
                        this.update(cx, |this, cx| this.step(next, window, cx))
                    })
                    .is_ok_and(|updated| updated.is_ok());
            if !handled {
                cx.update(|cx| {
                    eprintln!("{}: {detail}", strings::PROGRAM);
                    finish(ExitCode::Error, cx);
                });
            }
        })
        .detach();
    }

    /// Report an error on stderr and in an alert, then finish with 3.
    pub(super) fn fail(detail: &str, window: &mut Window, cx: &mut Context<Self>) {
        tracing::error!(detail, "cannot save");
        eprintln!("{}: {detail}", strings::PROGRAM);
        let answer = window.prompt(
            PromptLevel::Critical,
            &prompt::cannot_write(),
            Some(detail),
            &[PromptButton::new(prompt::quit())],
            cx,
        );
        cx.spawn(async move |_, cx| {
            // Quit, or the alert closing with the window, finishes alike.
            let _ = answer.await;
            cx.update(|cx| finish(ExitCode::Error, cx));
        })
        .detach();
    }

    /// Close the window (spec §6.2). With nothing changed since the last Save, finish at
    /// once: with that Save's code, or with 2 before any. After a change, ask: Save & Exit,
    /// Discard (finish as before, writing nothing) or Keep Editing.
    pub(super) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.saving() {
            self.send_edits(window, cx);
        }
        self.step(self.session.close_requested(), window, cx);
    }

    /// Ask whether to save, discard or keep editing.
    pub(super) fn confirm_close(window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            &prompt::save_changes(),
            Some(&prompt::discard_detail()),
            &Closing::ALL.map(|answer| PromptButton::new(closing_label(answer))),
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = answer.await.ok().and_then(|answer| Closing::ALL.get(answer).copied());
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                let next = this.session.close_answered(choice);
                this.step(next, window, cx);
            }));
        })
        .detach();
    }
}
