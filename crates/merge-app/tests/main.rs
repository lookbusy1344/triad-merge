//! The crate's integration tests, in one binary: one link per change instead of one per
//! file. Each file is a module.
#![expect(
    clippy::expect_used,
    reason = "test support: a failed setup fails the test, named by its message"
)]

mod a11y;
mod about;
mod absorb;
mod choices;
mod chrome;
mod collapse;
mod common;
mod diff;
mod dirs;
mod evolog;
mod history;
mod in_commit;
mod input_find;
mod input_select;
mod journal;
mod keybind;
mod languages;
mod line_ending;
mod line_picks;
mod measure;
mod menu_bar;
mod moves;
mod open_dialog;
mod origins;
mod output_change;
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
mod unusual;
mod whitespace;
mod window;
mod wrap;
