//! Frame measurement in the running app (PLAN.md §A6.7). `TRIAD_MEASURE=<mode>` makes the merge
//! window drive itself for a fixed number of frames, report the frame intervals and the time
//! each action took on stderr, and finish with exit code 2 without writing.

use std::cmp::Reverse;
use std::fmt;
use std::str::FromStr;
use std::time::{Duration, Instant};

use gpui_kit::{Pixels, px};

/// The environment variable that names the mode.
pub const ENV: &str = "TRIAD_MEASURE";
/// Frames each driving mode runs for.
pub const FRAMES: usize = 300;
/// How far a scrolling mode moves its pane each frame.
pub const SCROLL_STEP: Pixels = px(37.);
/// The report line's label for the time from the command to the last chunk on screen
/// landing.
pub const PARSED: &str = "chunks on screen parsed";
/// The frames a report lists as the slowest.
const SLOWEST: usize = 5;
const PERCENTILE: usize = 95;
const PERCENT: usize = 100;
const NS_PER_MS: f64 = 1_000_000.;

/// What the window does on each measured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Nothing: the run ends after the first frame, for start-up time and memory.
    Open,
    /// Scroll the rows; the result follows.
    ScrollRows,
    /// Scroll the result; the rows follow.
    ScrollResult,
    /// Type one character into the result at a difference near the middle.
    Type,
    /// Toggle the left and the right in turn on a difference near the middle.
    Pick,
    /// Nothing, as [`Mode::Open`], but the run ends only once the chunks the top panes show
    /// and their neighbours have parsed, for the cost of the syntax colours on opening.
    Syntax,
}

const MODES: [Mode; 6] =
    [Mode::Open, Mode::ScrollRows, Mode::ScrollResult, Mode::Type, Mode::Pick, Mode::Syntax];

impl Mode {
    /// The value of [`ENV`] that names this mode.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Mode::Open => "open",
            Mode::ScrollRows => "scroll-rows",
            Mode::ScrollResult => "scroll-result",
            Mode::Type => "type",
            Mode::Pick => "pick",
            Mode::Syntax => "syntax",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{ENV}: unknown mode `{0}`; expected open, scroll-rows, scroll-result, type, pick or syntax"
)]
pub struct UnknownMode(pub String);

impl FromStr for Mode {
    type Err = UnknownMode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MODES.into_iter().find(|mode| mode.name() == s).ok_or_else(|| UnknownMode(s.to_owned()))
    }
}

/// A measured session: the mode, how many frames it acts on, and when the command started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measure {
    pub mode: Mode,
    pub frames: usize,
    pub started: Instant,
}

impl Measure {
    /// `mode` for [`FRAMES`] frames, or for one frame in [`Mode::Open`] and [`Mode::Syntax`].
    #[must_use]
    pub fn new(mode: Mode, started: Instant) -> Self {
        let frames = match mode {
            Mode::Open | Mode::Syntax => 1,
            Mode::ScrollRows | Mode::ScrollResult | Mode::Type | Mode::Pick => FRAMES,
        };
        Self { mode, frames, started }
    }

    /// The measure `TRIAD_MEASURE` asks for, if any.
    ///
    /// # Errors
    ///
    /// [`UnknownMode`] when the variable names no mode.
    pub fn from_env(started: Instant) -> Result<Option<Self>, UnknownMode> {
        std::env::var(ENV).ok().map(|value| Ok(Self::new(value.parse()?, started))).transpose()
    }
}

/// The running measurement: one call to [`Run::frame`] per frame drawn.
#[derive(Debug)]
pub struct Run {
    measure: Measure,
    last: Option<Instant>,
    intervals: Vec<Duration>,
    work: Vec<Duration>,
}

impl Run {
    #[must_use]
    pub fn new(measure: Measure) -> Self {
        Self {
            measure,
            last: None,
            intervals: Vec::with_capacity(measure.frames),
            work: Vec::with_capacity(measure.frames),
        }
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.measure.mode
    }

    #[must_use]
    pub fn started(&self) -> Instant {
        self.measure.started
    }

    /// Whether no frame has been drawn yet.
    #[must_use]
    pub fn first(&self) -> bool {
        self.last.is_none()
    }

    /// Note a frame drawn at `now`. `None` asks for one more action; the report comes once
    /// every frame has acted and the frame after the last action has drawn.
    pub fn frame(&mut self, now: Instant) -> Option<Report> {
        if let Some(last) = self.last {
            self.intervals.push(now - last);
        }
        self.last = Some(now);
        (self.intervals.len() == self.measure.frames).then(|| Report {
            mode: self.measure.mode,
            frames: Summary::of(&self.intervals),
            work: Summary::of(&self.work),
        })
    }

    /// The actions noted so far.
    #[must_use]
    pub fn actions(&self) -> usize {
        self.work.len()
    }

    /// Note how long this frame's action took.
    pub fn worked(&mut self, took: Duration) {
        self.work.push(took);
    }
}

/// A window's measurement: none, a run, or the report of a run that ended.
#[derive(Debug)]
pub enum Measurement {
    Off,
    Running(Run),
    Done(Report),
}

impl Measurement {
    #[must_use]
    pub fn new(measure: Option<Measure>) -> Self {
        measure.map_or(Self::Off, |measure| Self::Running(Run::new(measure)))
    }

    #[must_use]
    pub fn running(&self) -> Option<&Run> {
        match self {
            Self::Running(run) => Some(run),
            Self::Off | Self::Done(_) => None,
        }
    }

    pub fn running_mut(&mut self) -> Option<&mut Run> {
        match self {
            Self::Running(run) => Some(run),
            Self::Off | Self::Done(_) => None,
        }
    }

    /// The report of a run that ended.
    #[must_use]
    pub fn report(&self) -> Option<&Report> {
        match self {
            Self::Done(report) => Some(report),
            Self::Off | Self::Running(_) => None,
        }
    }

    /// Note a frame drawn at `now`, as [`Run::frame`]. Returns the report on the frame that
    /// ends the run, and keeps it.
    pub fn frame(&mut self, now: Instant) -> Option<&Report> {
        let report = self.running_mut()?.frame(now)?;
        *self = Self::Done(report);
        self.report()
    }
}

/// What a run measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub mode: Mode,
    /// The intervals between frames.
    pub frames: Summary,
    /// The time each action took.
    pub work: Summary,
}

/// The mean, the 95th percentile by nearest rank, the maximum, and the slowest samples with
/// their indices, slowest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub count: usize,
    pub mean: Duration,
    pub p95: Duration,
    pub max: Duration,
    pub slowest: Vec<(usize, Duration)>,
}

impl Summary {
    #[must_use]
    pub fn of(samples: &[Duration]) -> Self {
        let Some(&max) = samples.iter().max() else {
            return Self::default();
        };
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        // Nearest rank: the smallest sample with at least 95% of the samples at or below it.
        let rank = (samples.len() * PERCENTILE).div_ceil(PERCENT);
        let mut slowest: Vec<_> = samples.iter().copied().enumerate().collect();
        // Stable, so equal samples keep their order.
        slowest.sort_by_key(|&(_, took)| Reverse(took));
        slowest.truncate(SLOWEST);
        let count = u32::try_from(samples.len()).unwrap_or(u32::MAX);
        Self {
            count: samples.len(),
            mean: samples.iter().sum::<Duration>() / count,
            p95: sorted[rank - 1],
            max,
            slowest,
        }
    }
}

/// Milliseconds with one decimal.
struct Ms(Duration);

impl fmt::Display for Ms {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[expect(clippy::cast_precision_loss, reason = "a frame is far below 2^53 ns")]
        let ms = self.0.as_nanos() as f64 / NS_PER_MS;
        write!(f, "{ms:.1} ms")
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} samples, mean {}, p95 {}, max {}; slowest",
            self.count,
            Ms(self.mean),
            Ms(self.p95),
            Ms(self.max)
        )?;
        for (index, took) in &self.slowest {
            write!(f, " #{index} {}", Ms(*took))?;
        }
        Ok(())
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: frames {}\n{}: work {}",
            self.mode.name(),
            self.frames,
            self.mode.name(),
            self.work
        )
    }
}

/// One measured span, for the stderr report.
#[must_use]
pub fn span(what: &str, took: Duration) -> String {
    format!("{what} {}", Ms(took))
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn every_mode_parses_from_its_name() {
        let names = ["open", "scroll-rows", "scroll-result", "type", "pick", "syntax"];
        for (name, mode) in names.into_iter().zip(MODES) {
            assert_eq!(name.parse::<Mode>(), Ok(mode));
            assert_eq!(mode.name(), name);
        }
    }

    #[test]
    fn an_unknown_mode_is_refused_by_name() {
        assert_eq!("scroll".parse::<Mode>(), Err(UnknownMode("scroll".into())));
        assert_eq!("".parse::<Mode>(), Err(UnknownMode(String::new())));
        assert_eq!("Open".parse::<Mode>(), Err(UnknownMode("Open".into())));
    }

    #[test]
    fn open_runs_one_frame_and_the_others_run_the_full_count() {
        let now = Instant::now();
        assert_eq!(Measure::new(Mode::Open, now).frames, 1);
        assert_eq!(Measure::new(Mode::Syntax, now).frames, 1);
        for mode in [Mode::ScrollRows, Mode::ScrollResult, Mode::Type, Mode::Pick] {
            assert_eq!(Measure::new(mode, now).frames, FRAMES);
        }
    }

    #[test]
    fn the_summary_of_ten_samples() {
        // Sorted: 1 2 3 4 5 6 7 8 9 30. Sum 75, mean 7.5. Rank ceil(9.5) = 10: 30.
        let samples = [3, 1, 30, 2, 4, 5, 6, 9, 8, 7].map(ms);
        let summary = Summary::of(&samples);
        assert_eq!(summary.count, 10);
        assert_eq!(summary.mean, Duration::from_micros(7_500));
        assert_eq!(summary.p95, ms(30));
        assert_eq!(summary.max, ms(30));
        assert_eq!(summary.slowest, [(2, ms(30)), (7, ms(9)), (8, ms(8)), (9, ms(7)), (6, ms(6))]);
    }

    #[test]
    fn the_95th_percentile_of_forty_samples_is_the_38th() {
        // 1..=40 ms. Rank 40 × 0.95 = 38.
        let samples: Vec<_> = (1..=40).map(ms).collect();
        let summary = Summary::of(&samples);
        assert_eq!(summary.p95, ms(38));
        assert_eq!(summary.mean, Duration::from_micros(20_500));
    }

    #[test]
    fn equal_samples_keep_their_order_among_the_slowest() {
        let summary = Summary::of(&[ms(5), ms(5), ms(9), ms(5), ms(5), ms(5), ms(5)]);
        assert_eq!(summary.slowest, [(2, ms(9)), (0, ms(5)), (1, ms(5)), (3, ms(5)), (4, ms(5))]);
    }

    #[test]
    fn one_sample_is_its_own_summary() {
        let summary = Summary::of(&[ms(4)]);
        assert_eq!(
            summary,
            Summary { count: 1, mean: ms(4), p95: ms(4), max: ms(4), slowest: vec![(0, ms(4))] }
        );
    }

    #[test]
    fn no_samples_summarise_as_zero() {
        assert_eq!(Summary::of(&[]), Summary::default());
    }

    #[test]
    fn a_run_reports_after_the_frame_that_follows_the_last_action() {
        let start = Instant::now();
        let mut run = Run::new(Measure { mode: Mode::Type, frames: 3, started: start });
        assert!(run.first());
        // Frames at 0, 8, 16 and 40 ms: three intervals, three actions.
        assert_eq!(run.frame(start), None);
        assert!(!run.first());
        run.worked(ms(1));
        assert_eq!(run.frame(start + ms(8)), None);
        run.worked(ms(2));
        assert_eq!(run.frame(start + ms(16)), None);
        run.worked(ms(6));
        let report = run.frame(start + ms(40)).expect("three intervals");
        assert_eq!(report.mode, Mode::Type);
        assert_eq!(report.frames.count, 3);
        assert_eq!(report.frames.mean, Duration::from_nanos(13_333_333));
        assert_eq!(report.frames.max, ms(24));
        assert_eq!(report.frames.slowest, [(2, ms(24)), (0, ms(8)), (1, ms(8))]);
        assert_eq!(report.work.count, 3);
        assert_eq!(report.work.mean, ms(3));
        assert_eq!(report.work.max, ms(6));
    }

    #[test]
    fn an_open_run_reports_after_its_second_frame() {
        let start = Instant::now();
        let mut run = Run::new(Measure::new(Mode::Open, start));
        assert_eq!(run.frame(start), None);
        let report = run.frame(start + ms(9)).expect("one interval");
        assert_eq!(report.frames.max, ms(9));
        assert_eq!(report.work, Summary::default());
    }

    #[test]
    fn the_report_reads_in_milliseconds() {
        let report = Report {
            mode: Mode::ScrollRows,
            frames: Summary::of(&[ms(8), Duration::from_micros(8_340), ms(194)]),
            work: Summary::of(&[Duration::from_micros(240)]),
        };
        assert_eq!(
            report.to_string(),
            "scroll-rows: frames 3 samples, mean 70.1 ms, p95 194.0 ms, max 194.0 ms; \
             slowest #2 194.0 ms #1 8.3 ms #0 8.0 ms\n\
             scroll-rows: work 1 samples, mean 0.2 ms, p95 0.2 ms, max 0.2 ms; slowest #0 0.2 ms"
        );
        assert_eq!(span("first frame", ms(687)), "first frame 687.0 ms");
    }

    #[test]
    fn a_window_without_a_measure_never_measures() {
        let mut measurement = Measurement::new(None);
        assert!(measurement.running().is_none());
        assert!(measurement.frame(Instant::now()).is_none());
        assert!(measurement.report().is_none());
    }

    #[test]
    fn an_open_run_reports_on_its_second_frame_and_then_holds_the_report() {
        let start = Instant::now();
        let mut measurement = Measurement::new(Some(Measure::new(Mode::Open, start)));
        assert_eq!(measurement.running().map(Run::mode), Some(Mode::Open));
        assert!(measurement.frame(start).is_none());
        assert!(measurement.report().is_none());
        let report = measurement.frame(start + ms(16)).expect("the run ends");
        assert_eq!((report.mode, report.frames.count), (Mode::Open, 1));
        assert_eq!(report.frames.max, ms(16));
        assert!(measurement.running().is_none());
        assert_eq!(measurement.report().map(|r| r.frames.max), Some(ms(16)));
    }

    #[test]
    fn an_ended_run_reports_once_and_keeps_its_report() {
        let start = Instant::now();
        let mut measurement = Measurement::new(Some(Measure::new(Mode::Open, start)));
        measurement.frame(start);
        measurement.frame(start + ms(16));
        assert!(measurement.frame(start + ms(40)).is_none());
        assert_eq!(measurement.report().map(|r| r.frames.max), Some(ms(16)));
    }

    #[test]
    fn a_running_measurement_notes_its_actions() {
        let start = Instant::now();
        let mut measurement = Measurement::new(Some(Measure::new(Mode::Pick, start)));
        measurement.frame(start);
        measurement.running_mut().expect("running").worked(ms(3));
        measurement.running_mut().expect("running").worked(ms(5));
        assert_eq!(measurement.running().map(Run::actions), Some(2));
    }
}
