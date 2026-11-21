//! The recovery journal: writing it, hashing the inputs and offering a restore (spec §6.2).

use std::io;
use std::path::Path;
use std::time::SystemTime;

use gpui_kit::{AppContext as _, Context, PromptButton, PromptLevel, Window};
use merge_core::boundaries::Boundaries;
use merge_core::document::MergeDocument;
use merge_core::journal::{Journal, RestoreError};
use merge_session::recovery::{FoundJournal, Locator, Recovery, WriteMode, Written, prune};
use merge_syntax::boundaries::SyntaxBoundaries;
use merge_view::highlight::Highlights;
use merge_view::moves;
use merge_view::rows::RowMap;

use crate::ignore_closed_view;
use crate::strings::prompt;

use super::{GO_AHEAD, JOURNAL_DEBOUNCE, MergeWindow, RebuildError, WindowSettings, with_rows};

impl MergeWindow {
    /// The recovery journal `settings` asks for, with the inputs' hashes computing.
    pub(super) fn journaling(
        doc: &MergeDocument,
        settings: &WindowSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Recovery> {
        settings.journal_dir.clone().map(|dir| {
            let name =
                settings.journal_name.clone().or_else(|| settings.output().map(Path::to_path_buf));
            let recovery = Recovery::new(dir, name);
            Self::hash_inputs(doc, recovery.locator(), window, cx);
            recovery
        })
    }

    /// Write the journal once commands pause for [`JOURNAL_DEBOUNCE`].
    pub(super) fn schedule_journal(&mut self, cx: &mut Context<Self>) {
        if self.session.recovery().is_none() {
            return;
        }
        // Replacing the task drops the one waiting, which cancels it.
        self.journal.waiting = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(JOURNAL_DEBOUNCE).await;
            ignore_closed_view(this.update(cx, Self::write_journal));
        }));
    }

    /// Start the next write on a background thread. While one runs, it starts the next when
    /// it ends.
    pub(super) fn write_journal(&mut self, cx: &mut Context<Self>) {
        self.journal.waiting = None;
        if self.journal.running.is_some() {
            return;
        }
        let Some((write, upto)) = self.session.recovery().and_then(Recovery::next_write) else {
            return;
        };
        self.journal.running = Some(cx.spawn(async move |this, cx| {
            let mode = write.mode;
            let result = cx.background_spawn(async move { write.run() }).await;
            ignore_closed_view(this.update(cx, |this, cx| this.written(mode, upto, result, cx)));
        }));
    }

    /// Take the result of a write that leaves the file with `upto` commands.
    pub(super) fn written(
        &mut self,
        mode: WriteMode,
        upto: usize,
        result: io::Result<Written>,
        cx: &mut Context<Self>,
    ) {
        if let Some(recovery) = self.session.recovery_mut()
            && self.journal.written(recovery, mode, upto, result)
        {
            self.write_journal(cx);
        }
    }

    /// Delete the journal's file on a background thread; then commands made since start a
    /// new one.
    pub(super) fn discard_journal(&mut self, cx: &mut Context<Self>) {
        self.journal.cancel();
        let Some(removal) = self.session.recovery_mut().and_then(Recovery::discard) else {
            self.write_journal(cx);
            return;
        };
        self.journal.running = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { removal.run() }).await;
            ignore_closed_view(this.update(cx, |this, cx| this.removed(result, cx)));
        }));
    }

    /// Take the result of the journal's removal.
    pub(super) fn removed(&mut self, result: io::Result<Written>, cx: &mut Context<Self>) {
        if let Some(recovery) = self.session.recovery_mut()
            && self.journal.removed(recovery, result)
        {
            self.write_journal(cx);
        }
    }

    /// Off the UI thread, delete old journals, hash the inputs and read the journal they
    /// name; then name this session's journal and offer any restore.
    pub(super) fn hash_inputs(
        doc: &MergeDocument,
        locator: Locator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Texts share their content, so the copy is O(1).
        let inputs = doc.inputs().clone();
        let start = doc.start();
        cx.spawn_in(window, async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    prune(locator.dir(), SystemTime::now());
                    locator.find(&inputs, &start)
                })
                .await;
            ignore_closed_view(
                this.update_in(cx, |this, window, cx| this.hashed(found, window, cx)),
            );
        })
        .detach();
    }

    pub(super) fn hashed(
        &mut self,
        found: FoundJournal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = self.session.document().start();
        let Some(recovery) = self.session.recovery_mut() else {
            return;
        };
        // A session that ended has hashed them itself.
        if !recovery.hashed(found.hashes, start) {
            return;
        }
        if !self.offer_restore(found.saved, window, cx) {
            self.write_journal(cx);
        }
    }

    /// Offer `saved`, the journal a crashed session left for these inputs. Returns whether it
    /// asked.
    pub(super) fn offer_restore(
        &mut self,
        saved: Option<Journal>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(journal) = self.session.recovery_mut().and_then(|r| r.offer(saved)) else {
            return false;
        };
        let answer = window.prompt(
            PromptLevel::Info,
            &prompt::restore_session(),
            Some(&prompt::restore_detail()),
            &[PromptButton::new(prompt::restore()), PromptButton::new(prompt::start_over())],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let restore = answer.await == Ok(GO_AHEAD);
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                if restore {
                    this.restore(journal, window, cx);
                } else {
                    this.discard_journal(cx);
                }
            }));
        })
        .detach();
        true
    }

    /// Replay `journal` off the UI thread, then switch the session to it.
    pub(super) fn restore(
        &mut self,
        journal: Journal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Texts share their content and line index, so the copy is O(1).
        let inputs = self.session.document().inputs().clone();
        let labels = self.session.document().labels().clone();
        let boundaries = self.colouring.detected().map(SyntaxBoundaries::new);
        // The writes stay held while the replay runs, so a crash keeps the journal.
        cx.spawn_in(window, async move |this, cx| {
            let (journal, replayed) = cx
                .background_spawn(async move {
                    let boundaries = boundaries.as_ref().map(|b| b as &dyn Boundaries);
                    let replayed = journal.restore_bounded(inputs, labels, boundaries);
                    (journal, replayed)
                })
                .await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.replayed(journal, replayed, window, cx);
            }));
        })
        .detach();
    }

    /// Switch the session to `replayed`, the replay of `journal`, or say why it cannot.
    pub(super) fn replayed(
        &mut self,
        journal: Journal,
        replayed: Result<MergeDocument, RestoreError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let restored =
            replayed.map_err(RebuildError::from).and_then(|doc| with_rows(doc, self.panes));
        let (doc, rows) = match restored {
            Ok(restored) => restored,
            Err(error) => {
                tracing::warn!(%error, "cannot restore the journal");
                let detail = error.describe();
                let answer = window.prompt(
                    PromptLevel::Warning,
                    &prompt::cannot_restore(),
                    Some(&detail),
                    &[prompt::ok_button()],
                    cx,
                );
                cx.spawn(async move |_, _| {
                    // The alert's one button only closes it.
                    let _ = answer.await;
                })
                .detach();
                // It would fail the same way on the next open.
                self.discard_journal(cx);
                return;
            }
        };
        if let Err(refused) = self.session.accepts() {
            // The save ends the session with the merge as it stood, or fails and goes on.
            tracing::info!(%refused, "the restore came during a save and is dropped");
            return;
        }
        self.journal.cancel();
        if let Err(refused) = self.session.restore(doc, journal) {
            tracing::warn!(%refused, "the session refused the restore");
            return;
        }
        self.show_restored(rows, window, cx);
        self.open_on_first_target(window, cx);
    }

    /// Show the session's restored document, laid out in `rows`.
    pub(super) fn show_restored(
        &mut self,
        rows: RowMap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rows = rows;
        self.move_ends = moves::move_ends(self.session.document(), &self.rows);
        self.scroll.rows_changed(self.rows.row_count());
        self.highlights = Highlights::new(self.panes);
        self.pinned = None;
        self.reload_result(window, cx);
        self.search_inputs_again(cx);
    }
}
