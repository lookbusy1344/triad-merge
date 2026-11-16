//! The files that differ between two directories, for `diff LEFT_DIR RIGHT_DIR` (spec §6.1).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use merge_core::text::{DecodeError, Text};

#[derive(Debug, thiserror::Error)]
pub enum DirDiffError {
    #[error("{} is not a directory", .0.display())]
    NotADirectory(PathBuf),
    #[error("diff takes two files or two directories: {} is a directory, {} is not", .dir.display(), .other.display())]
    Mixed { dir: PathBuf, other: PathBuf },
    #[error("cannot read {}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What `diff LEFT RIGHT` compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pair {
    Files,
    Directories,
}

/// Whether `left` and `right` are two directories or two files. A path that cannot be read
/// counts as a file, so reading it reports why.
///
/// # Errors
///
/// [`DirDiffError::Mixed`] when one is a directory and the other is not.
pub fn pair(left: &Path, right: &Path) -> Result<Pair, DirDiffError> {
    match (left.is_dir(), right.is_dir()) {
        (true, true) => Ok(Pair::Directories),
        (false, false) => Ok(Pair::Files),
        (true, false) => Err(DirDiffError::Mixed { dir: left.into(), other: right.into() }),
        (false, true) => Err(DirDiffError::Mixed { dir: right.into(), other: left.into() }),
    }
}

/// How a file differs between the two directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Only in the right directory.
    Added,
    /// Only in the left directory.
    Deleted,
    /// In both, with other bytes, or another link target.
    Modified,
}

/// What a listed file holds, which says whether the diff window opens it. Ordered by
/// precedence: a file whose sides differ takes the greater side's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Content {
    /// Every side present decodes (spec §4.1): the diff window opens it.
    Text,
    /// A side is neither binary nor UTF-8 or UTF-16.
    Undecodable,
    /// A side holds a NUL byte within git's sniff length.
    Binary,
    /// A side is no regular file: a symlink triad does not follow, a socket, a device, or a
    /// conflict in a jj revision.
    Special,
}

/// One file that differs: its path below both directories, how it changed and what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub change: Change,
    pub content: Content,
}

impl ChangedFile {
    /// The path with `/` between its components on every platform, as git and jj print one.
    /// A component that is not UTF-8 shows with U+FFFD in place of its bad bytes.
    #[must_use]
    pub fn display_path(&self) -> String {
        let components: Vec<_> =
            self.path.components().map(|c| c.as_os_str().to_string_lossy()).collect();
        components.join("/")
    }

    /// Whether the diff window opens this file.
    #[must_use]
    pub fn opens(&self) -> bool {
        self.content == Content::Text
    }
}

/// The files that differ between `left` and `right`, ordered by path, component by
/// component. Directories only hold files: an empty one lists nothing, and a file on one side
/// with a directory of the same name on the other is a delete and an add. A file with the
/// same bytes on both sides is left out. A symlink to a regular file at an absolute path
/// counts as that file, since git's dir diff links each working-tree file so; any other
/// symlink compares by its target and is [`Content::Special`].
///
/// # Errors
///
/// [`DirDiffError`] when either is not a directory, or an entry cannot be read.
pub fn changed_files(left: &Path, right: &Path) -> Result<Vec<ChangedFile>, DirDiffError> {
    let (left, right) = (walk(left)?, walk(right)?);
    let paths: BTreeSet<&PathBuf> = left.keys().chain(right.keys()).collect();
    paths
        .into_iter()
        .filter_map(|path| {
            compare(left.get(path), right.get(path))
                .map(|found| {
                    found.map(|(change, content)| ChangedFile {
                        path: path.clone(),
                        change,
                        content,
                    })
                })
                .transpose()
        })
        .collect()
}

/// A non-directory entry below a root.
enum Entry {
    /// A regular file, or a symlink to one at an absolute path: the path that reads it.
    File(PathBuf),
    /// Anything else: a symlink's target, or none for a socket, FIFO or device.
    Special(Option<PathBuf>),
}

fn read_error(path: &Path) -> impl FnOnce(std::io::Error) -> DirDiffError + '_ {
    move |source| DirDiffError::Read { path: path.to_path_buf(), source }
}

/// Every non-directory entry below `root`, by its path relative to `root`.
fn walk(root: &Path) -> Result<BTreeMap<PathBuf, Entry>, DirDiffError> {
    if !fs::metadata(root).map_err(read_error(root))?.is_dir() {
        return Err(DirDiffError::NotADirectory(root.to_path_buf()));
    }
    let mut found = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(dir) = pending.pop() {
        let at = root.join(&dir);
        for item in fs::read_dir(&at).map_err(read_error(&at))? {
            let item = item.map_err(read_error(&at))?;
            let relative = dir.join(item.file_name());
            let path = item.path();
            let kind = item.file_type().map_err(read_error(&path))?;
            if kind.is_dir() {
                pending.push(relative);
            } else if kind.is_file() {
                found.insert(relative, Entry::File(path));
            } else if kind.is_symlink() {
                found.insert(relative, link(path)?);
            } else {
                found.insert(relative, Entry::Special(None));
            }
        }
    }
    Ok(found)
}

/// A symlink's entry: the file it names at an absolute path, else its target.
fn link(path: PathBuf) -> Result<Entry, DirDiffError> {
    let target = fs::read_link(&path).map_err(read_error(&path))?;
    let to_file = target.is_absolute() && fs::metadata(&path).is_ok_and(|meta| meta.is_file());
    Ok(if to_file { Entry::File(path) } else { Entry::Special(Some(target)) })
}

/// How a path differs between its two entries; `None` when it does not.
fn compare(
    left: Option<&Entry>,
    right: Option<&Entry>,
) -> Result<Option<(Change, Content)>, DirDiffError> {
    Ok(match (left, right) {
        (Some(entry), None) => Some((Change::Deleted, content_of(entry)?)),
        (None, Some(entry)) => Some((Change::Added, content_of(entry)?)),
        (Some(Entry::File(left)), Some(Entry::File(right))) => {
            let left_bytes = fs::read(left).map_err(read_error(left))?;
            let right_bytes = fs::read(right).map_err(read_error(right))?;
            (left_bytes != right_bytes)
                .then(|| (Change::Modified, decoded(left_bytes).max(decoded(right_bytes))))
        }
        (Some(Entry::Special(Some(left))), Some(Entry::Special(Some(right)))) if left == right => {
            None
        }
        (Some(_), Some(_)) => Some((Change::Modified, Content::Special)),
        (None, None) => None,
    })
}

/// What `entry` holds.
fn content_of(entry: &Entry) -> Result<Content, DirDiffError> {
    match entry {
        Entry::Special(_) => Ok(Content::Special),
        Entry::File(path) => fs::read(path).map(decoded).map_err(read_error(path)),
    }
}

/// What a regular file's bytes hold, by spec §4.1's decoding.
pub(crate) fn decoded(bytes: Vec<u8>) -> Content {
    match Text::decode(bytes) {
        Ok(_) => Content::Text,
        Err(DecodeError::Binary) => Content::Binary,
        Err(_) => Content::Undecodable,
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use super::*;

    struct Dirs {
        root: tempfile::TempDir,
        left: PathBuf,
        right: PathBuf,
    }

    impl Dirs {
        fn new(left: &[(&str, &[u8])], right: &[(&str, &[u8])]) -> Self {
            let root = tempfile::tempdir().unwrap();
            let (l, r) = (root.path().join("left"), root.path().join("right"));
            for (dir, files) in [(&l, left), (&r, right)] {
                std::fs::create_dir(dir).unwrap();
                for (name, bytes) in files {
                    let path = dir.join(name);
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(path, bytes).unwrap();
                }
            }
            Self { root, left: l, right: r }
        }

        fn changed(&self) -> Vec<(String, Change, Content)> {
            changed_files(&self.left, &self.right)
                .unwrap()
                .into_iter()
                .map(|file| (file.display_path(), file.change, file.content))
                .collect()
        }
    }

    fn entry(path: &str, change: Change, content: Content) -> (String, Change, Content) {
        (path.to_owned(), change, content)
    }

    #[test]
    fn lists_added_deleted_and_modified_files_by_path_and_leaves_out_equal_ones() {
        let dirs = Dirs::new(
            &[("b.txt", b"same\n"), ("src/a.rs", b"one\n"), ("src/gone.rs", b"x\n")],
            &[("b.txt", b"same\n"), ("src/a.rs", b"two\n"), ("new.md", b"n\n")],
        );
        assert_eq!(
            dirs.changed(),
            [
                entry("new.md", Change::Added, Content::Text),
                entry("src/a.rs", Change::Modified, Content::Text),
                entry("src/gone.rs", Change::Deleted, Content::Text),
            ]
        );
    }

    #[test]
    fn paths_sort_by_component_so_a_directory_comes_before_a_longer_sibling_name() {
        // Byte order puts "a-b" (0x2D) before "a/c" (0x2F); component order compares "a"
        // with "a-b" first.
        let dirs = Dirs::new(&[], &[("a-b", b"1\n"), ("a/c", b"2\n")]);
        assert_eq!(
            dirs.changed(),
            [
                entry("a/c", Change::Added, Content::Text),
                entry("a-b", Change::Added, Content::Text)
            ]
        );
    }

    #[test]
    fn identical_directories_list_nothing() {
        let dirs = Dirs::new(&[("x/y.txt", b"1\n")], &[("x/y.txt", b"1\n")]);
        assert_eq!(dirs.changed(), []);
    }

    #[test]
    fn an_empty_directory_on_one_side_lists_nothing() {
        let dirs = Dirs::new(&[], &[]);
        std::fs::create_dir_all(dirs.right.join("empty/deeper")).unwrap();
        assert_eq!(dirs.changed(), []);
    }

    #[test]
    fn a_file_on_one_side_and_a_directory_on_the_other_is_a_delete_and_an_add() {
        let dirs = Dirs::new(&[("a", b"file\n")], &[("a/b", b"inside\n")]);
        assert_eq!(
            dirs.changed(),
            [
                entry("a", Change::Deleted, Content::Text),
                entry("a/b", Change::Added, Content::Text)
            ]
        );
    }

    #[test]
    fn a_rename_is_a_delete_and_an_add() {
        let dirs = Dirs::new(&[("old.rs", b"body\n")], &[("new.rs", b"body\n")]);
        assert_eq!(
            dirs.changed(),
            [
                entry("new.rs", Change::Added, Content::Text),
                entry("old.rs", Change::Deleted, Content::Text),
            ]
        );
    }

    #[test]
    fn content_names_binary_and_undecodable_sides() {
        let dirs = Dirs::new(
            &[("both.bin", b"\x00a"), ("m.txt", b"text\n"), ("u.txt", b"ok\n")],
            &[
                ("both.bin", b"\x00b"),
                ("m.txt", b"\x00bin"),
                ("u.txt", b"\xFF\xFE\xFF"),
                ("n", b"\xC3"),
            ],
        );
        assert_eq!(
            dirs.changed(),
            [
                entry("both.bin", Change::Modified, Content::Binary),
                entry("m.txt", Change::Modified, Content::Binary),
                entry("n", Change::Added, Content::Undecodable),
                entry("u.txt", Change::Modified, Content::Undecodable),
            ]
        );
    }

    #[test]
    fn only_a_text_file_opens() {
        let file = |content| ChangedFile { path: "a".into(), change: Change::Added, content };
        assert!(file(Content::Text).opens());
        assert!(!file(Content::Binary).opens());
        assert!(!file(Content::Undecodable).opens());
        assert!(!file(Content::Special).opens());
    }

    #[test]
    fn the_display_path_joins_components_with_a_slash() {
        let file = ChangedFile {
            path: ["src", "deep", "a.rs"].iter().collect(),
            change: Change::Modified,
            content: Content::Text,
        };
        assert_eq!(file.display_path(), "src/deep/a.rs");
    }

    #[test]
    fn pair_tells_two_directories_from_two_files_and_refuses_a_mix() {
        let dirs = Dirs::new(&[("f", b"x")], &[("g", b"y")]);
        let (file, other) = (dirs.left.join("f"), dirs.right.join("g"));
        let missing = dirs.left.join("missing");
        assert_eq!(pair(&dirs.left, &dirs.right).unwrap(), Pair::Directories);
        assert_eq!(pair(&file, &other).unwrap(), Pair::Files);
        assert_eq!(pair(&missing, &other).unwrap(), Pair::Files);
        for (left, right) in [(&dirs.left, &file), (&file, &dirs.left)] {
            let error = pair(left, right).unwrap_err();
            assert!(
                matches!(&error, DirDiffError::Mixed { dir, other }
                    if *dir == dirs.left && *other == file),
                "{error:?}"
            );
        }
        let error = pair(&missing, &dirs.right).unwrap_err();
        assert!(
            matches!(&error, DirDiffError::Mixed { dir, other }
                if *dir == dirs.right && *other == missing),
            "{error:?}"
        );
    }

    #[test]
    fn a_side_that_is_a_file_is_refused() {
        let dirs = Dirs::new(&[("f", b"x")], &[]);
        let file = dirs.left.join("f");
        let error = changed_files(&file, &dirs.right).unwrap_err();
        assert!(matches!(&error, DirDiffError::NotADirectory(path) if *path == file), "{error:?}");
        let error = changed_files(&dirs.right, &file).unwrap_err();
        assert!(matches!(&error, DirDiffError::NotADirectory(path) if *path == file), "{error:?}");
    }

    #[test]
    fn a_missing_side_is_a_read_error_naming_it() {
        let dirs = Dirs::new(&[], &[]);
        let missing = dirs.left.join("missing");
        let error = changed_files(&missing, &dirs.right).unwrap_err();
        assert!(
            matches!(&error, DirDiffError::Read { path, source }
                if *path == missing && source.kind() == ErrorKind::NotFound),
            "{error:?}"
        );
    }

    #[cfg(unix)]
    mod links {
        use std::os::unix::fs::symlink;

        use super::*;

        #[test]
        fn a_link_to_an_absolute_file_counts_as_that_file() {
            // git's dir diff links each working-tree file into the right directory.
            let dirs = Dirs::new(&[("same", b"1\n"), ("edit", b"1\n")], &[]);
            let tree = dirs.root.path().join("tree");
            std::fs::create_dir(&tree).unwrap();
            std::fs::write(tree.join("same"), b"1\n").unwrap();
            std::fs::write(tree.join("edit"), b"2\n").unwrap();
            symlink(tree.join("same"), dirs.right.join("same")).unwrap();
            symlink(tree.join("edit"), dirs.right.join("edit")).unwrap();
            assert_eq!(dirs.changed(), [entry("edit", Change::Modified, Content::Text)]);
        }

        #[test]
        fn a_relative_link_compares_by_target() {
            let dirs =
                Dirs::new(&[("t1", b"x\n"), ("t2", b"x\n")], &[("t1", b"x\n"), ("t2", b"x\n")]);
            symlink("t1", dirs.left.join("kept")).unwrap();
            symlink("t1", dirs.right.join("kept")).unwrap();
            // Both targets hold the same bytes; the target differs.
            symlink("t1", dirs.left.join("moved")).unwrap();
            symlink("t2", dirs.right.join("moved")).unwrap();
            symlink("t1", dirs.right.join("added")).unwrap();
            assert_eq!(
                dirs.changed(),
                [
                    entry("added", Change::Added, Content::Special),
                    entry("moved", Change::Modified, Content::Special),
                ]
            );
        }

        #[test]
        fn a_link_on_one_side_and_a_file_on_the_other_is_special() {
            let dirs = Dirs::new(&[("a", b"t1")], &[("t1", b"t1")]);
            symlink("t1", dirs.right.join("a")).unwrap();
            assert_eq!(
                dirs.changed(),
                [
                    entry("a", Change::Modified, Content::Special),
                    entry("t1", Change::Added, Content::Text),
                ]
            );
        }

        #[test]
        fn a_link_to_a_directory_is_special_and_not_followed() {
            let dirs = Dirs::new(&[], &[("d/f", b"1\n")]);
            symlink(dirs.right.join("d"), dirs.right.join("loop")).unwrap();
            assert_eq!(
                dirs.changed(),
                [
                    entry("d/f", Change::Added, Content::Text),
                    entry("loop", Change::Added, Content::Special),
                ]
            );
        }

        #[test]
        fn a_dangling_absolute_link_is_special() {
            let dirs = Dirs::new(&[], &[]);
            symlink(dirs.left.join("nowhere"), dirs.right.join("dangling")).unwrap();
            assert_eq!(dirs.changed(), [entry("dangling", Change::Added, Content::Special)]);
        }
    }
}
