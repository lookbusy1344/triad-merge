//! triad's configuration: the config file's format, the settings it holds, the key bindings,
//! and the names of their values. No repository, no file writes and no argument parser:
//! `merge-session` and the front ends build on it.

pub mod config;
pub mod keybind;
pub mod named;
pub mod settings;
