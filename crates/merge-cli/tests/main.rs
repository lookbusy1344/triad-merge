//! The crate's integration tests, in one binary: one link per change instead of one per
//! file. Each file is a module.
#![expect(
    clippy::expect_used,
    reason = "test support: a failed setup fails the test, named by its message"
)]

mod install;
mod markers;
mod resolve;
