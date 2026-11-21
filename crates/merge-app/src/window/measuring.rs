//! The measurement harness `TRIAD_MEASURE` drives (DESIGN.md §A6.7).

use std::time::Instant;

use gpui_kit::{Context, EntityInputHandler as _, Window, point};
use merge_core::merge3::Source;
use merge_view::nav;

use crate::measure::{self, Mode, Report, Run};
use crate::{ExitCode, finish, strings};

use super::MergeWindow;

impl MergeWindow {
    /// What a measured run reported, once it has ended.
    #[must_use]
    pub fn measured(&self) -> Option<&Report> {
        self.measurement.report()
    }

    /// Take one step of a measured run: on the first frame, report when it shows; then act
    /// once a frame; once the frame after the last action draws, report and finish with 2,
    /// writing nothing.
    pub(super) fn drive_measure(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = self.measurement.running() else {
            return;
        };
        if run.mode() == Mode::Syntax && !run.first() && self.syntax.pending() > 0 {
            // The next parse to land draws again.
            return;
        }
        if run.first() {
            let started = run.started();
            window.on_next_frame(move |_, _| {
                eprintln!(
                    "{}: {}",
                    strings::MEASURE,
                    measure::span("first frame", started.elapsed())
                );
            });
        }
        let mode = run.mode();
        if let Some(report) = self.measurement.frame(Instant::now()) {
            for line in report.to_string().lines() {
                eprintln!("{}: {line}", strings::MEASURE);
            }
            self.journal.cancel();
            self.session.end_journal();
            finish(ExitCode::Cancelled, cx);
            return;
        }
        if matches!(mode, Mode::Open | Mode::Syntax) {
            window.request_animation_frame();
            return;
        }
        let began = Instant::now();
        self.measure_step(mode, window, cx);
        if let Some(run) = self.measurement.running_mut() {
            run.worked(began.elapsed());
        }
        window.request_animation_frame();
    }

    /// One action of a measured run.
    pub(super) fn measure_step(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        match mode {
            Mode::Open | Mode::Syntax => {}
            Mode::ScrollRows => {
                let top = self.scroll.top() + measure::SCROLL_STEP;
                self.scroll.set_top(f32::from(top));
            }
            Mode::ScrollResult => self.result.update(cx, |state, cx| {
                let offset = state.scroll_offset();
                state.set_scroll_offset(point(offset.x, offset.y - measure::SCROLL_STEP), cx);
            }),
            Mode::Type => {
                if self.measured_steps() == 0 {
                    let region = nav::middle_difference(self.session.document());
                    self.go_to(region, window, cx);
                }
                self.result.update(cx, |state, cx| {
                    state.replace_text_in_range(None, "x", window, cx);
                });
                // Now, so the action's time holds the core's work.
                self.send_edits(window, cx);
            }
            Mode::Pick => {
                let sides = [Source::Side(0), Source::Side(1)];
                let source = sides[self.measured_steps() % sides.len()];
                self.pick(
                    nav::middle_difference(self.session.document()),
                    Some(source),
                    window,
                    cx,
                );
            }
        }
    }

    /// The actions a measured run has taken so far.
    pub(super) fn measured_steps(&self) -> usize {
        self.measurement.running().map_or(0, Run::actions)
    }
}
