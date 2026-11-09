//! The blink after a jump to a region (spec §5). For [`STEPS`] steps the target is hidden and
//! shown in turn, starting opposite to how it shows at rest; then it shows as at rest again.
//! The merge window's frame shows at rest, so its blink starts hidden; the diff window's
//! frame does not, so its blink starts shown. The window runs the timers.

/// The steps of a blink.
pub const STEPS: u32 = 5;

/// How a blink's target shows when no blink runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtRest {
    Shown,
    Hidden,
}

/// A blink of a target `T`, such as the region it frames, and its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blink<T> {
    running: Option<(T, u32)>,
}

impl<T> Default for Blink<T> {
    fn default() -> Self {
        Self { running: None }
    }
}

impl<T: Copy> Blink<T> {
    /// Start a blink of `target` at step 0, in place of any running one.
    pub fn start(&mut self, target: T) {
        self.running = Some((target, 0));
    }

    /// Move to `step` as the timers fire. `None`, or a step past the last, ends the blink.
    pub fn step(&mut self, step: Option<u32>) {
        self.running = match (self.running, step) {
            (Some((target, _)), Some(step)) if step < STEPS => Some((target, step)),
            _ => None,
        };
    }

    /// The target of the running blink.
    #[must_use]
    pub fn target(&self) -> Option<T> {
        self.running.map(|(target, _)| target)
    }

    #[must_use]
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Whether the target shows now, for a target that shows `at_rest`.
    #[must_use]
    pub fn shows(&self, at_rest: AtRest) -> bool {
        let rest = at_rest == AtRest::Shown;
        self.running.map_or(rest, |(_, step)| (step % 2 == 0) != rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the target shows at each step and after the blink ends.
    fn shown_through(at_rest: AtRest) -> (Vec<bool>, bool) {
        let mut blink = Blink::default();
        blink.start(());
        let steps = (0..STEPS)
            .map(|step| {
                blink.step(Some(step));
                blink.shows(at_rest)
            })
            .collect();
        blink.step(None);
        (steps, blink.shows(at_rest))
    }

    #[test]
    fn a_target_shown_at_rest_starts_hidden_and_ends_shown() {
        assert_eq!(shown_through(AtRest::Shown), (vec![false, true, false, true, false], true));
    }

    #[test]
    fn a_target_hidden_at_rest_starts_shown_and_ends_hidden() {
        assert_eq!(shown_through(AtRest::Hidden), (vec![true, false, true, false, true], false));
    }

    #[test]
    fn without_a_blink_the_target_shows_as_at_rest() {
        let blink = Blink::<u8>::default();
        assert!(!blink.running());
        assert_eq!(blink.target(), None);
        assert!(blink.shows(AtRest::Shown));
        assert!(!blink.shows(AtRest::Hidden));
    }

    #[test]
    fn the_target_holds_through_the_steps() {
        let mut blink = Blink::default();
        blink.start(7);
        blink.step(Some(3));
        assert_eq!(blink.target(), Some(7));
        assert!(blink.running());
    }

    #[test]
    fn a_new_blink_restarts_at_step_0_on_its_target() {
        let mut blink = Blink::default();
        blink.start(1);
        blink.step(Some(3));
        blink.start(2);
        assert_eq!(blink.target(), Some(2));
        assert!(!blink.shows(AtRest::Shown), "step 0 hides a target shown at rest");
    }

    #[test]
    fn a_step_past_the_last_or_none_ends_the_blink() {
        let mut blink = Blink::default();
        blink.start(1);
        blink.step(Some(STEPS));
        assert!(!blink.running());
        assert_eq!(blink.target(), None);
        blink.start(1);
        blink.step(None);
        assert!(!blink.running());
    }

    #[test]
    fn a_step_without_a_running_blink_starts_nothing() {
        let mut blink = Blink::<u8>::default();
        blink.step(Some(1));
        assert!(!blink.running());
    }
}
