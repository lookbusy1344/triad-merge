//! Index newtypes. Regions, lines and byte offsets are all counted in `usize`, and a value of
//! one kind in place of another compiles as bare `usize`. Each type here holds one kind.
//!
//! Each type's doctests pin one swap: a block that compiles, and its twin that passes the wrong
//! kind and must not. Stable rustdoc ignores the error code on `compile_fail`, so the compiling
//! twin is what shows that the other block fails on the swap.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The index of a merge's or diff's region.
///
/// ```
/// # use merge_core::{document::MergeDocument, index::RegionIx};
/// fn bytes(doc: &MergeDocument, region: RegionIx) -> std::ops::Range<usize> {
///     doc.region_bytes(region)
/// }
/// ```
///
/// ```compile_fail,E0308
/// # use merge_core::{document::MergeDocument, index::Line};
/// fn bytes(doc: &MergeDocument, line: Line) -> std::ops::Range<usize> {
///     doc.region_bytes(line)
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RegionIx(usize);

impl RegionIx {
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// The region after this one.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The region before this one, unless this is the first.
    #[must_use]
    pub const fn previous(self) -> Option<Self> {
        match self.0.checked_sub(1) {
            Some(index) => Some(Self(index)),
            None => None,
        }
    }
}

impl fmt::Display for RegionIx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A byte offset into the result.
///
/// ```
/// # use merge_core::{document::MergeDocument, index::{ByteOffset, Line}};
/// fn line(doc: &MergeDocument, offset: ByteOffset) -> Line {
///     doc.line_of_byte(offset)
/// }
/// ```
///
/// ```compile_fail,E0308
/// # use merge_core::{document::MergeDocument, index::Line};
/// fn line(doc: &MergeDocument, line: Line) -> Line {
///     doc.line_of_byte(line)
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(usize);

impl ByteOffset {
    #[must_use]
    pub const fn new(offset: usize) -> Self {
        Self(offset)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for ByteOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The index of a line of the result, from 0.
///
/// ```
/// # use merge_core::{document::MergeDocument, index::{ByteOffset, Line}};
/// fn start(doc: &MergeDocument, line: Line) -> ByteOffset {
///     doc.byte_of_line(line)
/// }
/// ```
///
/// ```compile_fail,E0308
/// # use merge_core::{document::MergeDocument, index::ByteOffset};
/// fn start(doc: &MergeDocument, offset: ByteOffset) -> ByteOffset {
///     doc.byte_of_line(offset)
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Line(usize);

impl Line {
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// The line `lines` below this one.
    #[must_use]
    pub const fn plus(self, lines: usize) -> Self {
        Self(self.0 + lines)
    }

    /// How many lines this one is below `first`.
    ///
    /// # Panics
    ///
    /// If this line is above `first`.
    #[must_use]
    pub const fn since(self, first: Self) -> usize {
        self.0 - first.0
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbours_step_by_one_and_the_first_has_no_previous() {
        assert_eq!(RegionIx::new(3).next(), RegionIx::new(4));
        assert_eq!(RegionIx::new(3).previous(), Some(RegionIx::new(2)));
        assert_eq!(RegionIx::new(0).previous(), None);
    }

    #[test]
    fn a_region_index_serialises_as_its_number_so_journals_keep_their_form() {
        assert_eq!(serde_json::to_string(&RegionIx::new(7)).unwrap(), "7");
        assert_eq!(serde_json::from_str::<RegionIx>("7").unwrap(), RegionIx::new(7));
    }

    #[test]
    fn line_distances_count_lines_between() {
        assert_eq!(Line::new(5).plus(2), Line::new(7));
        assert_eq!(Line::new(7).since(Line::new(5)), 2);
        assert_eq!(Line::new(5).since(Line::new(5)), 0);
    }

    #[test]
    fn a_region_index_prints_as_its_number() {
        assert_eq!(RegionIx::new(7).to_string(), "7");
    }
}
