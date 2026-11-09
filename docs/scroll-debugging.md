# Debugging trackpad scrolling

Where to look when scrolling in a pane feels slow, sticky or stops short, and how to reproduce it in a test.

## The two scroll paths

The result pane and the diff panes are `gpui-base` editors with scroll bounce, on by default on macOS. A wheel event over one goes through `InputBaseState::on_scroll_wheel` (`vendor/gpui-base/src/input/base/state.rs`) and `Overscroll::packet` (`vendor/gpui-base/src/overscroll.rs`). The editor claims the event in the capture phase through `scroll_bounce::claim_scroll`.

The merge window's top panes are a plain GPUI `uniform_list` without scroll bounce. GPUI hit-tests a wheel event at the pointer, so the list handles only wheels over itself.

A symptom in the editors alone points at the editor's wheel path. A symptom in every pane points at scroll sync, `sync_scroll` in `crates/merge-app/src/window.rs` and `diff_window.rs`.

## What macOS sends

GPUI's macOS backend (`gpui-pre-macos/src/events.rs`) maps trackpad phases:

| NSEvent phase | `TouchPhase` |
|---|---|
| `MayBegin`, `Began` | `Started` |
| `Ended` | `Ended` |
| anything else, momentum included | `Moved` |

Deltas are fractional `ScrollDelta::Pixels`. `TestWindowExt::scroll` sends whole-pixel `Moved` packets only, and misses every failure below.

## Overscroll state

`Overscroll::packet` asks a closure to scroll the text by `dy` and to return how much it used. The unused part stretches the edge. While an edge is stretched, `Physics::pull` takes every packet until the motion reverses past the edge. `Physics::begin` keeps a returning edge where it is, so a gesture that starts before the edge settles grabs it.

State that lives for one gesture: `Physics::dragging`, the stretch `position`, and `Overscroll::spent`. A symptom that lasts one gesture and clears when the fingers lift points here.

## Failure modes and their tests

| Symptom | Cause | Tests |
|---|---|---|
| A gesture sticks after a few packets and scrolls about half a line over the whole trackpad | The closure reports a used amount a rounding error short of `dy`, which starts a stretch mid-text. Each closure returns `dy` unless the offset clamped. | `one_trackpad_gesture_scrolls_the_result_by_every_packet` (merge-app), `test_scroll_bounce_keeps_a_gesture_scrolling_through_rounding` (editor), `a_gesture_keeps_scrolling_through_rounding` (`ScrollBounce`) |
| The start of every scroll feels slow | A finger lifted mid-text while moving throws the edge, and the next gesture pulls it. The edge is let go with speed only when stretched. | `lifting_a_moving_finger_inside_the_text_leaves_the_edge_at_rest` (overscroll), `test_scroll_bounce_starts_the_next_gesture_scrolling` (editor), `the_gesture_after_a_lift_scrolls_the_result_from_its_first_packet` (merge-app) |
| The wheel cannot reach the end of a pane | Scroll sync copies a follower's clamped top back to the pane that moved. `merge_view::scroll::ScrollSync` records the pane the last sync moved, and the next sync takes its top as synced. | `wheel_over_the_result_scrolls_it_to_its_end`, `wheel_over_the_rows_scrolls_them_past_the_end_of_the_result` (merge-app), `a_follower_held_at_its_end_does_not_pull_the_leader_back` (merge-view) |

### Signatures

What each failure looks like in a probe, to tell a recurrence from a new bug:

- **Rounding.** A gesture of 1.37 px packets over the result: the text moves for 5 packets, then sticks for the other 1495. Printing `rest`, `used` and `unused` per packet in `Overscroll::packet` shows `unused 1.19e-7` on the packet where it sticks: `after - before` of two f32 tops came out short of `dy`. Lifting the fingers clears it.
- **Throw on lift.** Swipe, lift while moving, step the clock and print the stretch each frame. A stretch that grows from 0 (to about −2.85 px) after a lift mid-text is the throw. In the next gesture the top stays put while the stretch grows.
- **Clamp fed back.** A wheel sweep to the end of the result stops short: 2608 px of 2834.5 px in the fixture that found it. In a merge that deletes the second half of the file, the wheel cannot scroll the rows through the deleted lines. The scrollbar still reaches the end.

### Ruled out

- Wheel events going to the wrong pane. GPUI hit-tests at the pointer, and the editor claims the event in the capture phase.
- Frame cost of re-rendering the rows on each sync. That would slow every frame, not one gesture, and the scrollbar path pays it too.

### Ranges

The panes' ranges differ: the result scrolls half a viewport past its last line (`scroll_beyond_last_line`), the viewports differ in height, and a region can hold more lines in one pane than in another. A follower given a top past its end clamps in its next prepaint.

The scrollbar sets an absolute offset on each mouse move, so it hides any bug that undoes or swallows a wheel delta. A working scrollbar rules out little.

## Method

1. Ask when it happens: during a drag, at the start of a gesture, or during momentum. Each points at a different branch of `Overscroll::packet`. Ask whether lifting the fingers clears it: per-gesture state if so.
2. Reproduce in a headless test with events shaped like the platform's: touch phases, fractional deltas, the clock advanced between packets (`cx.executor().advance_clock`), a frame drawn after each packet. Send them with `window.dispatch_event` at the pane's centre, as `trackpad` in `crates/merge-app/tests/scroll.rs` does.
3. Assert each packet's movement, not only the end state. A sweep that checks each step shows where the response changes. Take the expected ends from line counts and viewport heights.
4. Once a test reproduces it, print the physics state per packet (`rest`, `used`, `unused`) to find the first wrong value. Remove the prints before committing.
5. Fix in the vendored crate, mark the change with a `triad:` comment, list it in `vendor/gpui-base/PATCHES.md`, and describe the problem in `docs/gpui-kit-issues.md`.

## Not covered by tests

- Momentum packets after a lift, and a gesture that starts during momentum.
- A slow drag that stops before the fingers lift; it throws nothing.
- Mouse wheels and other devices that send precise deltas without phases.
