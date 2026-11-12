//! The crate's integration tests, in one binary: one link per change instead of one per
//! file. Each file is a module.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: a failed setup or check fails the test, named by its message"
)]

mod hostile_input;
mod jj_oracle;
mod oracle;
mod properties;
