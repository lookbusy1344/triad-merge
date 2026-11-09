//! Fixture inputs shared by the backends' `testing` modules.

/// The commit, in hex, that every fixture submodule points at. Nothing reads it: a tree
/// holds a submodule's commit ID without the commit.
pub const SUBMODULE_COMMIT: &str = "5ab1e5ab1e5ab1e5ab1e5ab1e5ab1e5ab1e5ab1e";

/// What a fixture writes at a path in a tree or a conflict stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry<'a> {
    File(&'a [u8]),
    Executable(&'a [u8]),
    /// A symlink to the target.
    Symlink(&'a str),
    /// A submodule at [`SUBMODULE_COMMIT`].
    Submodule,
}

impl<'a> Entry<'a> {
    /// A plain file for each `(path, bytes)`.
    #[must_use]
    pub fn files(files: &[(&'a str, &'a [u8])]) -> Vec<(&'a str, Self)> {
        files.iter().map(|&(path, bytes)| (path, Self::File(bytes))).collect()
    }
}
