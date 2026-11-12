//! The crate's integration tests, in one binary: one link per change instead of one per
//! file. Each file is a module.
#![expect(
    clippy::expect_used,
    reason = "test support: a failed setup fails the test, named by its message"
)]

mod conformance;
mod diff_tool;
mod discovery;
mod install;
mod native;
mod origin;
mod resolve;
mod write_relative;
