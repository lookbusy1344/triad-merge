//! What a file holds, by length and hash, so the merge window can tell when another program
//! changes OUTPUT (spec §6.3).

use std::hash::{DefaultHasher, Hasher as _};
use std::io;
use std::path::Path;

/// A file's bytes by length and hash, or its absence. Two stamps differ when the bytes do;
/// a collision of the 64-bit hash at the same length goes unseen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stamp {
    Absent,
    Held { len: usize, hash: u64 },
}

impl Stamp {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(bytes);
        Self::Held { len: bytes.len(), hash: hasher.finish() }
    }

    /// The stamp of the file at `path` now. A missing file is [`Stamp::Absent`].
    ///
    /// # Errors
    ///
    /// The read's error when the file exists and cannot be read.
    pub fn read(path: &Path) -> io::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Self::of(&bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::Absent),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_bytes_give_the_same_stamp() {
        assert_eq!(Stamp::of(b"a\nb\n"), Stamp::of(b"a\nb\n"));
    }

    #[test]
    fn other_bytes_of_the_same_length_give_another_stamp() {
        assert_ne!(Stamp::of(b"a\nb\n"), Stamp::of(b"a\nc\n"));
    }

    #[test]
    fn an_empty_file_is_not_an_absent_one() {
        assert!(matches!(Stamp::of(b""), Stamp::Held { len: 0, .. }));
        assert_ne!(Stamp::of(b""), Stamp::Absent);
    }

    #[test]
    fn a_missing_file_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Stamp::read(&dir.path().join("gone")).unwrap(), Stamp::Absent);
    }

    #[test]
    fn a_file_reads_as_the_stamp_of_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out");
        std::fs::write(&path, "x\n").unwrap();
        assert_eq!(Stamp::read(&path).unwrap(), Stamp::of(b"x\n"));
    }

    #[test]
    fn a_directory_in_the_files_place_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            Stamp::read(dir.path()).is_err_and(|error| error.kind() != io::ErrorKind::NotFound)
        );
    }
}
