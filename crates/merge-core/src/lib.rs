//! Text loading, line diffing, three-way alignment, region classification, resolution state, undo and output rendering. No UI, no VCS code and no file I/O.

pub mod boundaries;
pub mod diff;
pub mod diff2;
pub mod document;
pub mod edit;
mod histogram;
pub mod index;
pub mod jj;
pub mod journal;
pub mod lengths;
pub mod markers;
pub mod merge3;
mod region_lines;
pub mod suggest;
pub mod text;

#[cfg(any(test, feature = "testing"))]
pub mod testing;
