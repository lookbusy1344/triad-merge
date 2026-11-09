// Modified for Triad: an idle cursor rests, shown, after IDLE_TIMEOUT. See PATCHES.md.
use gpui::{Context, Pixels, Task, px};
use instant::Duration;

pub(crate) const INTERVAL: Duration = Duration::from_millis(500);
const PAUSE_DELAY: Duration = Duration::from_millis(300);
/// Triad: how long a cursor blinks without input before it rests, shown.
/// Every frame keeps the GPU driver's memory in use, about 120 MB on macOS;
/// a window that draws nothing lets the driver reclaim it.
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
/// The toggles a cursor makes before it rests.
const BLINKS_BEFORE_REST: u32 = (IDLE_TIMEOUT.as_millis() / INTERVAL.as_millis()) as u32;

pub(super) const CURSOR_WIDTH: Pixels = px(1.);

/// To manage the Input cursor blinking.
///
/// It will start blinking with a interval of 500ms.
/// Every loop will notify the view to update the `visible`, and Input will observe this update to touch repaint.
///
/// The input painter will check if this in visible state, then it will draw the cursor.
pub(crate) struct BlinkCursor {
    visible: bool,
    paused: bool,
    epoch: usize,
    /// Toggles left before the cursor rests.
    blinks_left: u32,

    _task: Task<()>,
}

impl BlinkCursor {
    pub(crate) fn new() -> Self {
        Self {
            visible: false,
            paused: false,
            epoch: 0,
            blinks_left: BLINKS_BEFORE_REST,
            _task: Task::ready(()),
        }
    }

    /// Start the blinking from a visible cursor.
    ///
    /// A single focus can start the cursor more than once: `InputState::focus`
    /// starts it and then the focus observer starts it again, and window
    /// activation starts it too. Resuming the running cycle would toggle the
    /// cursor that was just shown off again, so every start restarts the cycle.
    pub(crate) fn start(&mut self, cx: &mut Context<Self>) {
        self.paused = false;
        self.visible = false;
        self.blinks_left = BLINKS_BEFORE_REST;
        let epoch = self.next_epoch();
        self.blink(epoch, cx);
    }

    /// Stop the blinking and clear the blink state, so the next [`Self::start`]
    /// begins from a visible cursor instead of resuming a stale pause.
    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        self.epoch = 0;
        self.paused = false;
        self.visible = false;
        self._task = Task::ready(());
        cx.notify();
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch += 1;
        self.epoch
    }

    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        // A task from an earlier blink lifecycle must not mutate the current state.
        if epoch != self.epoch {
            return;
        }

        if self.paused {
            self.visible = true;
            return;
        }

        // Rest shown, with no timer. `pause` and `start` wake the cursor.
        if self.blinks_left == 0 {
            if !self.visible {
                self.visible = true;
                cx.notify();
            }
            self._task = Task::ready(());
            return;
        }
        self.blinks_left -= 1;

        self.visible = !self.visible;
        cx.notify();

        // Schedule the next blink
        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(INTERVAL).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.blink(epoch, cx));
            }
        });
    }

    pub(crate) fn visible(&self) -> bool {
        // Keep showing the cursor if paused
        self.paused || self.visible
    }

    /// Show the cursor immediately and restart the idle delay before blinking resumes.
    ///
    /// This is a no-op on a cursor that is not blinking. `epoch` is zero only
    /// before the first [`Self::start`] and after [`Self::stop`], so a zero
    /// epoch means the input is not focused, and there is no cursor on screen
    /// to keep visible. Pausing it anyway would start a blink loop that no blur
    /// is left to stop, and every blink repaints the view that the input is in.
    pub(crate) fn pause(&mut self, cx: &mut Context<Self>) {
        if self.epoch == 0 {
            return;
        }

        self.paused = true;
        self.visible = true;
        cx.notify();

        // Every pause replaces the pending timer, keeping repeated input visible.
        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PAUSE_DELAY).await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if epoch != this.epoch {
                        return;
                    }

                    this.paused = false;
                    this.blinks_left = BLINKS_BEFORE_REST;
                    this.blink(epoch, cx);
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn repeated_pauses_keep_cursor_visible_until_idle(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));
        // Only a focused input blinks, and only a blinking cursor pauses.
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        for _ in 0..5 {
            cursor.update(cx, |cursor, cx| cursor.pause(cx));
            cx.run_until_parked();
            cx.executor().advance_clock(Duration::from_millis(200));
            cx.run_until_parked();
            assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));
        }
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));
        cx.executor().advance_clock(INTERVAL);
        cx.run_until_parked();
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));
    }

    #[gpui::test]
    fn starting_a_blinking_cursor_keeps_it_visible(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());

        // Clicking into an input starts the cursor from `focus` and again from
        // the focus observer; the second start must not hide the new cursor.
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));

        // The restart drops the first cycle's timer, so the cursor stays
        // visible for a full interval before it blinks off.
        cx.executor()
            .advance_clock(INTERVAL - Duration::from_millis(1));
        cx.run_until_parked();
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));
        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));
    }

    #[gpui::test]
    fn pausing_a_cursor_that_is_not_blinking_does_not_start_it(cx: &mut TestAppContext) {
        // Never focused, so `start` was never called: the epoch is still zero.
        let cursor = cx.new(|_| BlinkCursor::new());

        // What a programmatic `set_value` on an unfocused input does.
        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cx.run_until_parked();
        cx.executor().advance_clock(PAUSE_DELAY);
        cx.run_until_parked();
        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));

        let notifies = Rc::new(Cell::new(0usize));
        let counter = notifies.clone();
        let _observer =
            cx.update(|cx| cx.observe(&cursor, move |_, _| counter.set(counter.get() + 1)));
        cx.run_until_parked();

        cx.executor().advance_clock(INTERVAL * 6);
        cx.run_until_parked();

        assert_eq!(
            notifies.get(),
            0,
            "a cursor that was never started is blinking, and every blink repaints the view"
        );
    }

    #[gpui::test]
    fn blurring_a_paused_cursor_leaves_the_next_focus_blinking(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();

        // Typing pauses the blink, then the input is blurred before the pause
        // elapses: tabbing away right after a keystroke does exactly this.
        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cx.run_until_parked();
        cursor.update(cx, |cursor, cx| cursor.stop(cx));
        cx.run_until_parked();
        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));

        // Focusing again shows the cursor and blinks it, rather than leaving a
        // stale pause to swallow the start.
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));
        cx.executor().advance_clock(INTERVAL);
        cx.run_until_parked();
        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));
    }

    #[gpui::test]
    fn stopping_a_paused_cursor_ends_the_blink_loop(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cx.run_until_parked();
        cursor.update(cx, |cursor, cx| cursor.stop(cx));
        cx.run_until_parked();

        let notifications = Rc::new(Cell::new(0usize));
        let count = notifications.clone();
        let _observer = cx.update(|cx| cx.observe(&cursor, move |_, _| count.set(count.get() + 1)));
        cx.run_until_parked();

        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();

        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));
        assert_eq!(notifications.get(), 0, "a stopped cursor kept blinking");

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));
    }

    #[gpui::test]
    fn stopping_a_blinking_cursor_ends_the_blink_loop(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        cursor.update(cx, |cursor, cx| cursor.stop(cx));
        cx.run_until_parked();

        let notifications = Rc::new(Cell::new(0usize));
        let count = notifications.clone();
        let _observer = cx.update(|cx| cx.observe(&cursor, move |_, _| count.set(count.get() + 1)));
        cx.run_until_parked();

        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();

        assert!(!cursor.read_with(cx, |cursor, _| cursor.visible()));
        assert_eq!(notifications.get(), 0, "a stopped cursor kept blinking");
    }

    // Triad: an idle cursor rests.

    /// Counts the cursor's notifications, each of which repaints the view.
    fn count_notifications(
        cursor: &gpui::Entity<BlinkCursor>,
        cx: &mut TestAppContext,
    ) -> (Rc<Cell<usize>>, gpui::Subscription) {
        let notifications = Rc::new(Cell::new(0usize));
        let count = notifications.clone();
        let observer = cx.update(|cx| cx.observe(cursor, move |_, _| count.set(count.get() + 1)));
        cx.run_until_parked();
        (notifications, observer)
    }

    fn visible(cursor: &gpui::Entity<BlinkCursor>, cx: &mut TestAppContext) -> bool {
        cursor.read_with(cx, |cursor, _| cursor.visible())
    }

    fn advance(cx: &mut TestAppContext, by: Duration) {
        cx.executor().advance_clock(by);
        cx.run_until_parked();
    }

    #[gpui::test]
    fn triad_a_cursor_rests_visible_after_the_idle_timeout(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        let (notifications, _observer) = count_notifications(&cursor, cx);

        // Toggles at 0.5 s, 1 s, ... 9.5 s: 19 after the start's own. The 20th
        // toggle, at 9.5 s, hides the cursor.
        advance(cx, IDLE_TIMEOUT - Duration::from_millis(1));
        assert!(!visible(&cursor, cx));
        assert_eq!(notifications.get(), 19);

        // At 10 s the cursor shows and stays.
        advance(cx, Duration::from_millis(1));
        assert!(visible(&cursor, cx));
        assert_eq!(notifications.get(), 20);

        advance(cx, INTERVAL * 20);
        assert!(visible(&cursor, cx));
        assert_eq!(notifications.get(), 20, "a resting cursor kept blinking");
    }

    #[gpui::test]
    fn triad_typing_wakes_a_resting_cursor_for_another_timeout(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        advance(cx, IDLE_TIMEOUT);

        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cx.run_until_parked();
        let (notifications, _observer) = count_notifications(&cursor, cx);

        // The pause ends with a toggle that hides the cursor, then it blinks.
        advance(cx, PAUSE_DELAY);
        assert!(!visible(&cursor, cx));
        advance(cx, INTERVAL);
        assert!(visible(&cursor, cx));
        assert_eq!(notifications.get(), 2);

        // 20 toggles, from the pause's end to 9.5 s after it. The first hid the
        // cursor, so the 20th shows it.
        advance(cx, IDLE_TIMEOUT - INTERVAL);
        assert!(visible(&cursor, cx));
        assert_eq!(notifications.get(), 20);

        // A full timeout after the pause ends, the cursor rests. It is shown
        // already, so the rest repaints nothing, and no toggle follows.
        advance(cx, INTERVAL * 20);
        assert!(visible(&cursor, cx));
        assert_eq!(notifications.get(), 20, "the cursor did not rest again");
    }

    #[gpui::test]
    fn triad_starting_a_resting_cursor_blinks_it_again(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        advance(cx, IDLE_TIMEOUT + INTERVAL * 4);

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        assert!(visible(&cursor, cx));
        advance(cx, INTERVAL);
        assert!(!visible(&cursor, cx));
        advance(cx, INTERVAL);
        assert!(visible(&cursor, cx));
    }

    #[gpui::test]
    fn triad_a_cursor_stopped_while_resting_stays_hidden(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        advance(cx, IDLE_TIMEOUT);
        cursor.update(cx, |cursor, cx| cursor.stop(cx));
        cx.run_until_parked();
        let (notifications, _observer) = count_notifications(&cursor, cx);

        advance(cx, INTERVAL * 20);
        assert!(!visible(&cursor, cx));
        assert_eq!(notifications.get(), 0);
    }
}
