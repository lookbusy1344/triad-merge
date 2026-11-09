//! Atomic file writes (spec §6.2): the target holds either its old bytes or all the new ones.

use std::borrow::Cow;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tempfile::{Builder, NamedTempFile};

/// Requested mode of a new file, before the umask, as `open(2)` callers such as
/// `std::fs::write` ask for.
#[cfg(unix)]
const NEW_FILE_MODE: u32 = 0o666;

/// Longest chain of links a write follows, as on Linux.
const MAX_LINKS: usize = 40;

/// Who may read a written file.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    /// An existing file's permissions, or those of any new file.
    Shared,
    /// The owner alone.
    Private,
}

#[derive(Debug, thiserror::Error)]
#[error("cannot write {}: {source}", path.display())]
pub struct WriteError {
    pub path: PathBuf,
    #[source]
    pub source: io::Error,
}

/// Write `bytes` to a temporary file beside `path`, then rename it over `path`. An existing
/// file keeps its permissions; a new one gets the permissions of any newly created file. A
/// link at `path` stays: the file it names takes the bytes.
///
/// # Errors
///
/// [`WriteError`] when the temporary file cannot be created, written or renamed, or `path` is
/// a loop of links. `path` is then unchanged and the temporary file is gone.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    write_replacing(path, Access::Shared, |file| file.write_all(bytes))
}

/// Write what `write` writes, buffered, as [`write_atomic`] does.
///
/// # Errors
///
/// As [`write_atomic`], and the error `write` returns.
pub fn write_atomic_with(
    path: &Path,
    write: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> Result<(), WriteError> {
    write_replacing(path, Access::Shared, |file| {
        let mut buffered = io::BufWriter::new(file);
        write(&mut buffered)?;
        buffered.flush()
    })
}

/// Write `bytes` as [`write_atomic`] does, for a file only its owner reads: on Unix the file
/// ends with mode 0600, whatever mode an existing file has.
///
/// # Errors
///
/// As [`write_atomic`].
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), WriteError> {
    write_replacing(path, Access::Private, |file| file.write_all(bytes))
}

fn write_replacing(
    path: &Path,
    access: Access,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> Result<(), WriteError> {
    let fail = |source| WriteError { path: path.to_owned(), source };
    let target = link_target(path).map_err(fail)?;
    let mut temp = temp_file_beside(&target, access).map_err(fail)?;
    write(temp.as_file_mut()).map_err(fail)?;
    temp.as_file().sync_all().map_err(fail)?;
    if access == Access::Shared {
        match fs::metadata(&target) {
            Ok(existing) => {
                fs::set_permissions(temp.path(), existing.permissions()).map_err(fail)?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(fail(e)),
        }
    }
    // On failure the error owns the temporary file, which deletes it when dropped.
    temp.persist(&target).map_err(|e| fail(e.error))?;
    sync_dir_of(&target);
    Ok(())
}

/// Sync the directory holding `path`, so the rename that put the file there survives a
/// crash. The file holds its bytes already; a failure only leaves the rename unsynced, so it
/// is logged and the write stands.
#[cfg(unix)]
fn sync_dir_of(path: &Path) {
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    if let Err(error) = fs::File::open(dir).and_then(|dir| dir.sync_all()) {
        tracing::warn!(%error, dir = %dir.display(), "cannot sync the directory after a write");
    }
}

/// `std` opens no directory handle to sync on Windows.
#[cfg(not(unix))]
fn sync_dir_of(_: &Path) {}

/// The file `path` names: `path`, or the end of its chain of links. That file may not exist.
fn link_target(path: &Path) -> io::Result<Cow<'_, Path>> {
    let mut at = Cow::Borrowed(path);
    for _ in 0..MAX_LINKS {
        match fs::symlink_metadata(&at) {
            Ok(metadata) if metadata.is_symlink() => {
                let link = fs::read_link(&at)?;
                // An absolute link replaces the directory; a relative one starts from it.
                at = Cow::Owned(at.parent().unwrap_or(Path::new("")).join(link));
            }
            Ok(_) => return Ok(at),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(at),
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("too many levels of links"))
}

/// A temporary file in `path`'s directory, so the rename stays on one file system. A bare file
/// name has the empty parent, which resolves against the working directory.
fn temp_file_beside(path: &Path, access: Access) -> io::Result<NamedTempFile> {
    let dir = path.parent().unwrap_or(Path::new(""));
    match access {
        Access::Shared => new_file_builder().tempfile_in(dir),
        // `tempfile` makes its files owner-only.
        Access::Private => Builder::new().tempfile_in(dir),
    }
}

/// A builder whose files get the mode of a newly created file, not `tempfile`'s owner-only one.
#[cfg(unix)]
fn new_file_builder() -> Builder<'static, 'static> {
    use std::os::unix::fs::PermissionsExt;
    let mut builder = Builder::new();
    builder.permissions(fs::Permissions::from_mode(NEW_FILE_MODE));
    builder
}

#[cfg(not(unix))]
fn new_file_builder() -> Builder<'static, 'static> {
    Builder::new()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        const PERMISSION_BITS: u32 = 0o777;
        fs::metadata(path).expect("exists").permissions().mode() & PERMISSION_BITS
    }

    #[cfg(unix)]
    #[test]
    fn a_new_private_file_is_owner_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("private");
        write_private(&path, b"new\n").expect("written");
        assert_eq!(mode(&path), 0o600);
        assert_eq!(fs::read(&path).expect("readable"), b"new\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_private_write_narrows_a_readable_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("private");
        fs::write(&path, b"old\n").expect("written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("set");
        write_private(&path, b"new\n").expect("written");
        assert_eq!(mode(&path), 0o600);
        assert_eq!(fs::read(&path).expect("readable"), b"new\n");
    }

    #[test]
    fn a_private_write_that_fails_names_the_path_and_leaves_no_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing").join("private");
        let error = write_private(&path, b"new\n").expect_err("no such directory");
        assert_eq!(error.path, path);
        assert_eq!(error.source.kind(), io::ErrorKind::NotFound);
        assert!(!path.exists());
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("dir lists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn writes_a_new_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.txt");
        write_atomic(&path, b"a\nb\n").expect("write");
        assert_eq!(fs::read(&path).expect("read"), b"a\nb\n");
        assert_eq!(entries(dir.path()), ["out.txt"]);
    }

    #[test]
    fn replaces_an_existing_file_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.txt");
        fs::write(&path, b"old contents that are longer\n").expect("seed");
        write_atomic(&path, b"new\n").expect("write");
        assert_eq!(fs::read(&path).expect("read"), b"new\n");
        assert_eq!(entries(dir.path()), ["out.txt"]);
    }

    #[test]
    fn writes_empty_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out.txt");
        fs::write(&path, b"x").expect("seed");
        write_atomic(&path, b"").expect("write");
        assert_eq!(fs::read(&path).expect("read"), b"");
    }

    #[test]
    fn a_missing_directory_fails_with_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing").join("out.txt");
        let error = write_atomic(&path, b"x").expect_err("no directory");
        assert_eq!(
            (error.path.as_path(), error.source.kind()),
            (path.as_path(), io::ErrorKind::NotFound)
        );
        assert_eq!(entries(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn a_failed_rename_leaves_the_target_and_no_temporary_file() {
        // A directory cannot be replaced by a file.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("out");
        fs::create_dir(&path).expect("target directory");
        fs::write(path.join("kept"), b"k").expect("seed");
        let error = write_atomic(&path, b"x").expect_err("target is a directory");
        assert_eq!(error.path, path);
        assert_eq!(entries(dir.path()), ["out"]);
        assert_eq!(entries(&path), ["kept"]);
    }

    #[cfg(unix)]
    mod unix {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        use super::*;

        const PERMISSION_BITS: u32 = 0o7777;
        const EXECUTABLE: u32 = 0o755;
        const PRIVATE: u32 = 0o600;
        const SHARED: u32 = 0o664;
        const READ_ONLY_DIR: u32 = 0o555;

        fn mode(path: &Path) -> u32 {
            fs::metadata(path).expect("metadata").permissions().mode() & PERMISSION_BITS
        }

        #[test]
        fn an_existing_file_keeps_its_permissions() {
            let dir = tempfile::tempdir().expect("tempdir");
            for (name, wanted) in [("exe", EXECUTABLE), ("private", PRIVATE), ("shared", SHARED)] {
                let path = dir.path().join(name);
                fs::write(&path, b"old").expect("seed");
                fs::set_permissions(&path, fs::Permissions::from_mode(wanted)).expect("chmod");
                write_atomic(&path, b"new").expect("write");
                assert_eq!(mode(&path), wanted, "{name}");
            }
        }

        #[test]
        fn a_new_file_gets_the_permissions_of_any_new_file() {
            let dir = tempfile::tempdir().expect("tempdir");
            let reference = dir.path().join("reference");
            fs::write(&reference, b"r").expect("reference");
            let path = dir.path().join("out.txt");
            write_atomic(&path, b"x").expect("write");
            assert_eq!(mode(&path), mode(&reference));
        }

        #[test]
        fn a_read_only_directory_fails_and_leaves_the_target() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("out.txt");
            fs::write(&path, b"old").expect("seed");
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(READ_ONLY_DIR))
                .expect("chmod dir");
            let result = write_atomic(&path, b"new");
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(EXECUTABLE))
                .expect("restore dir");
            let error = result.expect_err("directory is read-only");
            assert_eq!(
                (error.path.as_path(), error.source.kind()),
                (path.as_path(), io::ErrorKind::PermissionDenied)
            );
            assert_eq!(fs::read(&path).expect("read"), b"old");
            assert_eq!(entries(dir.path()), ["out.txt"]);
        }

        fn link_to(target: &str, link: &Path) {
            std::os::unix::fs::symlink(target, link).expect("link");
        }

        fn link_text(link: &Path) -> String {
            fs::read_link(link).expect("still a link").to_str().expect("utf-8").to_owned()
        }

        #[test]
        fn a_link_stays_and_the_file_it_names_changes() {
            let dir = tempfile::tempdir().expect("tempdir");
            let (file, link) = (dir.path().join("file"), dir.path().join("link"));
            fs::write(&file, b"old").expect("seed");
            link_to("file", &link);
            write_atomic(&link, b"new").expect("write");
            assert_eq!(link_text(&link), "file");
            assert_eq!(fs::read(&file).expect("read"), b"new");
            assert_eq!(entries(dir.path()), ["file", "link"]);
        }

        #[test]
        fn a_relative_link_resolves_against_the_links_directory() {
            let dir = tempfile::tempdir().expect("tempdir");
            let (real, links) = (dir.path().join("real"), dir.path().join("links"));
            fs::create_dir_all(&real).expect("real");
            fs::create_dir_all(&links).expect("links");
            let file = real.join("file");
            fs::write(&file, b"old").expect("seed");
            let link = links.join("link");
            link_to("../real/file", &link);
            write_atomic(&link, b"new").expect("write");
            assert_eq!(link_text(&link), "../real/file");
            assert_eq!(fs::read(&file).expect("read"), b"new");
            assert_eq!(entries(&real), ["file"]);
            assert_eq!(entries(&links), ["link"]);
        }

        #[test]
        fn a_chain_of_links_reaches_the_file() {
            let dir = tempfile::tempdir().expect("tempdir");
            let file = dir.path().join("file");
            fs::write(&file, b"old").expect("seed");
            link_to("file", &dir.path().join("second"));
            let first = dir.path().join("first");
            link_to("second", &first);
            write_atomic(&first, b"new").expect("write");
            assert_eq!(link_text(&first), "second");
            assert_eq!(link_text(&dir.path().join("second")), "file");
            assert_eq!(fs::read(&file).expect("read"), b"new");
        }

        #[test]
        fn a_dangling_link_creates_the_file_it_names() {
            let dir = tempfile::tempdir().expect("tempdir");
            let link = dir.path().join("link");
            link_to("file", &link);
            write_atomic(&link, b"new").expect("write");
            assert_eq!(link_text(&link), "file");
            assert_eq!(fs::read(dir.path().join("file")).expect("read"), b"new");
        }

        #[test]
        fn a_linked_file_keeps_its_permissions() {
            let dir = tempfile::tempdir().expect("tempdir");
            let (file, link) = (dir.path().join("file"), dir.path().join("link"));
            fs::write(&file, b"old").expect("seed");
            fs::set_permissions(&file, fs::Permissions::from_mode(PRIVATE)).expect("chmod");
            link_to("file", &link);
            write_atomic(&link, b"new").expect("write");
            assert_eq!(mode(&file), PRIVATE);
        }

        #[test]
        fn the_temporary_file_goes_beside_the_linked_file() {
            // The link's directory is read-only; only the file's directory is written.
            let dir = tempfile::tempdir().expect("tempdir");
            let (real, links) = (dir.path().join("real"), dir.path().join("links"));
            fs::create_dir_all(&real).expect("real");
            fs::create_dir_all(&links).expect("links");
            let link = links.join("link");
            link_to("../real/file", &link);
            fs::set_permissions(&links, fs::Permissions::from_mode(READ_ONLY_DIR))
                .expect("chmod dir");
            let result = write_atomic(&link, b"new");
            fs::set_permissions(&links, fs::Permissions::from_mode(EXECUTABLE))
                .expect("restore dir");
            result.expect("the file's directory is writable");
            assert_eq!(fs::read(real.join("file")).expect("read"), b"new");
        }

        #[test]
        fn a_loop_of_links_fails_and_leaves_them() {
            let dir = tempfile::tempdir().expect("tempdir");
            let (one, two) = (dir.path().join("one"), dir.path().join("two"));
            link_to("two", &one);
            link_to("one", &two);
            let error = write_atomic(&one, b"new").expect_err("no file at the end");
            assert_eq!(error.path, one);
            assert_eq!(link_text(&one), "two");
            assert_eq!(link_text(&two), "one");
            assert_eq!(entries(dir.path()), ["one", "two"]);
        }
    }
}
