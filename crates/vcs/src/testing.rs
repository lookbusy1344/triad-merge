//! Fixture inputs shared by the backends' `testing` modules.

/// The commit, in hex, that a fixture submodule points at. Nothing reads it: a tree holds a
/// submodule's commit ID without the commit.
pub const SUBMODULE_COMMIT: &str = "5ab1e5ab1e5ab1e5ab1e5ab1e5ab1e5ab1e5ab1e";

/// A second submodule commit, in hex, for a submodule that moves.
pub const SUBMODULE_COMMIT_MOVED: &str = "b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0";

/// What a fixture writes at a path in a tree or a conflict stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry<'a> {
    File(&'a [u8]),
    Executable(&'a [u8]),
    /// A symlink to the target.
    Symlink(&'a str),
    /// A submodule at [`SUBMODULE_COMMIT`].
    Submodule,
    /// A submodule at [`SUBMODULE_COMMIT_MOVED`].
    SubmoduleMoved,
}

impl Entry<'_> {
    /// A submodule's commit in hex; `None` for any other entry.
    #[must_use]
    pub fn submodule_commit(self) -> Option<&'static str> {
        match self {
            Self::Submodule => Some(SUBMODULE_COMMIT),
            Self::SubmoduleMoved => Some(SUBMODULE_COMMIT_MOVED),
            Self::File(_) | Self::Executable(_) | Self::Symlink(_) => None,
        }
    }
}

impl<'a> Entry<'a> {
    /// A plain file for each `(path, bytes)`.
    #[must_use]
    pub fn files(files: &[(&'a str, &'a [u8])]) -> Vec<(&'a str, Self)> {
        files.iter().map(|&(path, bytes)| (path, Self::File(bytes))).collect()
    }
}
