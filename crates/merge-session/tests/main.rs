//! The crate's integration tests, in one binary: one link per change instead of one per
//! file. Each file is a module.

mod conformance;
mod diff_tool;
mod discovery;
mod install;
mod native;
mod origin;
mod resolve;
mod write_relative;
