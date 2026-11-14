//! The crate's integration tests, in one binary: one link per change instead of one per
//! file. Each file is a module.
#![expect(
    clippy::expect_used,
    reason = "test support: a failed setup fails the test, named by its message"
)]

mod a11y;
mod about;
mod choices;
mod collapse;
mod common;
mod diff;
mod history;
mod in_commit;
mod input_find;
mod input_select;
mod journal;
mod keybind;
mod line_ending;
mod measure;
mod menu_bar;
mod open_dialog;
mod origins;
mod placeholders;
mod resolve_queue;
mod ruler;
mod save;
mod save_copy;
mod scroll;
mod scrollbars;
mod stack;
mod syntax;
mod tools;
mod window;
mod wrap;
