//! Watching OUTPUT on disk and asking about a change there (spec §6.1).

use std::path::Path;

use gpui_kit::{AppContext as _, Context, PromptButton, PromptLevel, SharedString, Window};

use crate::file_watch;
use crate::output_watch::OutputWatch;
use crate::strings::prompt;
use crate::{ignore_closed_view, strings};

use super::{MergeWindow, OUTPUT_QUIT, Watching, WindowSettings};

impl MergeWindow {
    /// The watch on `settings`' output, with a watcher when the settings ask for one.
    pub(super) fn watch_output(
        settings: &WindowSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<OutputWatch> {
        let path = settings.output()?;
        let watch = (settings.watch_output == Watching::On).then(|| {
            // Without a working directory, a relative path stays relative.
            let cwd = settings.cwd.as_deref().unwrap_or(Path::new(""));
            let changed = |this: &mut Self, _: usize, window: &mut Window, cx: &mut Context<_>| {
                this.output_disk_changed(window, cx);
            };
            file_watch::watch(&[Some(path)], cwd, changed, window, cx)
        });
        Some(OutputWatch::new(path, watch))
    }

    /// Read what OUTPUT holds off the UI thread, as the stamp later changes are measured
    /// against (spec §6.3).
    pub(super) fn stamp_output(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(read) = self.output_watch.as_mut().map(OutputWatch::restamp) else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let generation = read.generation;
            let stamp = cx.background_spawn(async move { read.run() }).await;
            ignore_closed_view(this.update(cx, |this, cx| {
                if let Some(watch) = &mut this.output_watch {
                    watch.stamped(generation, stamp);
                }
                cx.notify();
            }));
        })
        .detach();
    }

    /// OUTPUT changed on disk (spec §6.3): read it off the UI thread, and show the banner when
    /// it no longer holds what it held at the stamp. A save's own write changes nothing.
    pub fn output_disk_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.saving() {
            return;
        }
        let Some(read) = self.output_watch.as_ref().and_then(OutputWatch::check) else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let generation = read.generation;
            let now = cx.background_spawn(async move { read.run() }).await;
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                if this.output_watch.as_mut().is_some_and(|w| w.checked(generation, now)) {
                    this.ask_output_changed(window, cx);
                    cx.notify();
                }
            }));
        })
        .detach();
    }

    /// The banner's text while OUTPUT differs from its stamp.
    pub(super) fn output_banner(&self) -> Option<SharedString> {
        let watch = self.output_watch.as_ref().filter(|watch| watch.changed())?;
        let path = watch.path();
        let name = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
        Some(strings::output_changed_banner(&name).into())
    }

    /// Ask once, as soon as another program changes OUTPUT, whether to go on with the merge
    /// (spec §6.2). The merge takes no picks or edits until the answer. Keep Editing goes on;
    /// Quit closes the window as Close does.
    pub(super) fn ask_output_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(watch) = self.output_watch.as_mut() else {
            return;
        };
        if !watch.ask() {
            return;
        }
        let path = watch.path().display().to_string();
        self.asking_output = true;
        self.lock_result(cx);
        let answer = window.prompt(
            PromptLevel::Warning,
            &prompt::changed_on_disk(&path),
            Some(&prompt::output_changed_detail()),
            &[PromptButton::new(prompt::keep_editing()), PromptButton::new(prompt::quit())],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let quit = answer.await == Ok(OUTPUT_QUIT);
            ignore_closed_view(this.update_in(cx, |this, window, cx| {
                this.asking_output = false;
                this.lock_result(cx);
                if quit {
                    this.cancel(window, cx);
                }
            }));
        })
        .detach();
    }
}
