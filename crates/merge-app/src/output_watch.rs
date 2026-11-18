//! Whether another program changed OUTPUT while the merge window is open (spec §6.3). The
//! window stamps OUTPUT when it opens and after a save that did not end the session, and
//! checks each change the watcher reports against that stamp. Stamps and checks read the
//! file off the UI thread; each carries the generation it began in, and a result from an
//! older generation is dropped.

use std::io;
use std::path::{Path, PathBuf};

use merge_session::stamp::Stamp;

use crate::file_watch::Watch;

pub(crate) struct OutputWatch {
    path: PathBuf,
    /// What OUTPUT held at the latest stamp; none until it lands, or when it cannot be read.
    stamp: Option<Stamp>,
    /// Counts the stamps begun and the saves started.
    generation: u64,
    /// OUTPUT differs from the stamp.
    changed: bool,
    _watch: Option<Watch>,
}

/// A read of OUTPUT to run off the UI thread, and the generation it belongs to.
pub(crate) struct Read {
    pub(crate) path: PathBuf,
    pub(crate) generation: u64,
}

impl Read {
    pub(crate) fn run(&self) -> io::Result<Stamp> {
        Stamp::read(&self.path)
    }
}

impl OutputWatch {
    pub(crate) fn new(path: &Path, watch: Option<Watch>) -> Self {
        Self { path: path.to_owned(), stamp: None, generation: 0, changed: false, _watch: watch }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// OUTPUT differs from what it held at the latest stamp.
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }

    /// Begin a new stamp. Checks already running are dropped, and the change goes until a
    /// check finds one again.
    pub(crate) fn restamp(&mut self) -> Read {
        self.generation += 1;
        self.stamp = None;
        self.changed = false;
        self.read()
    }

    /// A save starts: it writes OUTPUT itself, so checks already running are dropped.
    pub(crate) fn saving(&mut self) {
        self.generation += 1;
    }

    /// Begin a check of a change the watcher reported; none before the stamp lands.
    pub(crate) fn check(&self) -> Option<Read> {
        self.stamp.map(|_| self.read())
    }

    /// The stamp begun in `generation` read `read`.
    pub(crate) fn stamped(&mut self, generation: u64, read: io::Result<Stamp>) {
        if generation != self.generation {
            return;
        }
        self.stamp = read
            .inspect_err(|error| {
                tracing::warn!(%error, "cannot read the output; changes to it go unseen");
            })
            .ok();
    }

    /// The check begun in `generation` read `read`. Returns whether the change flag moved.
    pub(crate) fn checked(&mut self, generation: u64, read: io::Result<Stamp>) -> bool {
        if generation != self.generation {
            return false;
        }
        let now = match read {
            Ok(now) => now,
            Err(error) => {
                tracing::info!(%error, "cannot read the changed output; nothing changes");
                return false;
            }
        };
        let changed = self.stamp.is_some_and(|stamp| stamp != now);
        let moved = changed != self.changed;
        self.changed = changed;
        moved
    }

    fn read(&self) -> Read {
        Read { path: self.path.clone(), generation: self.generation }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A watch stamped with `bytes`.
    fn stamped(bytes: &[u8]) -> OutputWatch {
        let mut watch = OutputWatch::new(Path::new("out"), None);
        let read = watch.restamp();
        watch.stamped(read.generation, Ok(Stamp::of(bytes)));
        watch
    }

    fn check(watch: &mut OutputWatch, read: io::Result<Stamp>) -> bool {
        let generation = watch.check().expect("stamped").generation;
        watch.checked(generation, read)
    }

    #[test]
    fn nothing_is_checked_before_the_stamp_lands() {
        let mut watch = OutputWatch::new(Path::new("out"), None);
        assert!(watch.check().is_none());
        let _ = watch.restamp();
        assert!(watch.check().is_none(), "the stamp is still being read");
    }

    #[test]
    fn other_bytes_are_a_change_and_the_old_ones_undo_it() {
        let mut watch = stamped(b"theirs\n");
        assert!(check(&mut watch, Ok(Stamp::of(b"other\n"))));
        assert!(watch.changed());
        assert!(!check(&mut watch, Ok(Stamp::of(b"again\n"))), "still changed");
        assert!(check(&mut watch, Ok(Stamp::of(b"theirs\n"))));
        assert!(!watch.changed());
    }

    #[test]
    fn a_file_that_appears_or_goes_is_a_change() {
        let mut watch = OutputWatch::new(Path::new("out"), None);
        let read = watch.restamp();
        watch.stamped(read.generation, Ok(Stamp::Absent));
        assert!(check(&mut watch, Ok(Stamp::of(b""))));
        let mut watch = stamped(b"theirs\n");
        assert!(check(&mut watch, Ok(Stamp::Absent)));
    }

    #[test]
    fn a_check_that_cannot_read_leaves_the_flag() {
        let mut watch = stamped(b"theirs\n");
        assert!(check(&mut watch, Ok(Stamp::of(b"other\n"))));
        assert!(!check(&mut watch, Err(io::Error::other("busy"))));
        assert!(watch.changed());
    }

    #[test]
    fn a_stamp_that_cannot_read_sees_no_change() {
        let mut watch = OutputWatch::new(Path::new("out"), None);
        let read = watch.restamp();
        watch.stamped(read.generation, Err(io::Error::other("busy")));
        assert!(watch.check().is_none());
    }

    #[test]
    fn a_check_begun_before_a_save_started_is_dropped() {
        let mut watch = stamped(b"theirs\n");
        let read = watch.check().expect("stamped");
        watch.saving();
        assert!(!watch.checked(read.generation, Ok(Stamp::of(b"merged\n"))));
        assert!(!watch.changed());
    }

    #[test]
    fn a_restamp_clears_the_change_and_drops_older_reads() {
        let mut watch = stamped(b"theirs\n");
        let early = watch.check().expect("stamped");
        assert!(check(&mut watch, Ok(Stamp::of(b"merged\n"))));
        let first = watch.restamp();
        assert!(!watch.changed());
        let second = watch.restamp();
        watch.stamped(first.generation, Ok(Stamp::of(b"stale\n")));
        assert!(watch.check().is_none(), "the older stamp is dropped");
        watch.stamped(second.generation, Ok(Stamp::of(b"merged\n")));
        assert!(
            !watch.checked(early.generation, Ok(Stamp::of(b"x\n"))),
            "the older check is dropped"
        );
        assert!(!check(&mut watch, Ok(Stamp::of(b"merged\n"))));
        assert!(check(&mut watch, Ok(Stamp::of(b"other\n"))));
    }
}
