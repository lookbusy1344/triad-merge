//! View model: aligned rows for each pane, scroll mapping between panes, navigation, highlight spans, status and line origins. Pure Rust, no GPUI.

pub mod blink;
pub mod chunks;
pub mod diff;
pub mod fold;
pub mod heights;
pub mod highlight;
pub mod marks;
pub mod nav;
pub mod origin;
pub mod placeholder;
pub mod rows;
pub mod ruler;
pub mod runs;
pub mod scroll;
pub mod search;
pub mod selection;
pub mod status;
pub mod tabs;
