//! The recovery journal of a session (spec §6.2): its commands, its file and its writes.
//! [`Recovery`] builds each write's bytes; the front end runs the write and its sync on a
//! background thread, since a sync can take tens of milliseconds, and holds the tasks.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use crate::io::write_private;
use merge_core::document::{Command, Start};
use merge_core::journal::{InputHashes, JOURNAL_EXTENSION, Journal, input_hashes};
use merge_core::merge3::Inputs;

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
const JOURNAL_MAX_AGE_DAYS: u64 = 30;
/// A journal not written for this long is deleted.
const JOURNAL_MAX_AGE: Duration = Duration::from_secs(JOURNAL_MAX_AGE_DAYS * SECONDS_PER_DAY);

/// Delete the journals in `dir` older than [`JOURNAL_MAX_AGE`] at `now`. A journal that cannot
/// be deleted is logged and stays, since it costs only disk space.
pub fn prune(dir: &Path, now: SystemTime) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(%error, dir = %dir.display(), "cannot list the journals");
            return;
        }
    };
    for entry in entries {
        if let Err(error) = entry.and_then(|entry| prune_entry(&entry.path(), now)) {
            tracing::warn!(%error, dir = %dir.display(), "cannot delete an old journal");
        }
    }
}

fn prune_entry(path: &Path, now: SystemTime) -> io::Result<()> {
    if path.extension().is_none_or(|extension| extension != JOURNAL_EXTENSION) {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(path)?;
    // A file written in the future has no age, and `duration_since` fails.
    let expired = now.duration_since(metadata.modified()?).is_ok_and(|age| age > JOURNAL_MAX_AGE);
    if expired {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// The journal at `path`, if it has this format and holds commands.
fn read_saved(path: &Path) -> Option<Journal> {
    let journal = match std::fs::read(path) {
        Ok(bytes) => Journal::parse(&bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(%error, "cannot read the journal");
            return None;
        }
    };
    match journal {
        Ok(journal) if !journal.is_current() => {
            tracing::warn!("the journal has another format, so it is not offered");
            None
        }
        Ok(journal) => (!journal.commands().is_empty()).then_some(journal),
        Err(error) => {
            tracing::warn!(%error, "the journal does not parse");
            None
        }
    }
}

/// The journal file of a session whose inputs are still hashing: hashing them names it.
#[derive(Debug, Clone)]
pub struct Locator {
    dir: PathBuf,
    name: Option<PathBuf>,
}

/// The inputs' hashes, and the journal a crashed session left for them.
#[derive(Debug)]
pub struct FoundJournal {
    pub hashes: InputHashes,
    pub saved: Option<Journal>,
}

impl Locator {
    /// Hash `inputs`, which start from `start`, and read the journal they name. Slow: the
    /// hashes take 100 ms per input at 1M lines.
    #[must_use]
    pub fn find(&self, inputs: &Inputs, start: &Start) -> FoundJournal {
        let hashes = input_hashes(inputs);
        let path = self.path(&hashes, start);
        FoundJournal { hashes, saved: read_saved(&path) }
    }

    /// The directory the journals live in.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, hashes: &InputHashes, start: &Start) -> PathBuf {
        let journal = Journal::with_hashes(hashes.clone(), start.clone());
        self.dir.join(journal.file_name(self.name.as_deref()))
    }
}

/// Lets one generation of a journal's file operations through until it closes, in order with
/// every other generation's. An operation holds `order`, which all generations share, while
/// it runs, and runs only while its own generation is open. [`Gate::close`] does not wait: a
/// running operation ends first, and one that starts later skips.
#[derive(Clone, Debug)]
struct Gate {
    open: Arc<AtomicBool>,
    order: Arc<Mutex<()>>,
}

impl Gate {
    fn new() -> Self {
        Self { open: Arc::new(AtomicBool::new(true)), order: Arc::new(Mutex::new(())) }
    }

    /// The next generation, in order after this one's.
    fn next(&self) -> Self {
        Self { open: Arc::new(AtomicBool::new(true)), order: Arc::clone(&self.order) }
    }

    fn close(&self) {
        // The lock orders this store before every operation that starts after a later
        // generation's, so the flag guards no other data.
        self.open.store(false, Ordering::Relaxed);
    }

    /// Wait for the running operation; then whether this generation is still open.
    fn enter(&self) -> (MutexGuard<'_, ()>, bool) {
        // The lock guards no data, so a panic holding it leaves nothing broken.
        let running = self.order.lock().unwrap_or_else(PoisonError::into_inner);
        (running, self.open.load(Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteMode {
    /// Replace the file with the whole journal.
    Whole,
    /// Add the commands the file lacks to its end.
    Append,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Written {
    Done,
    /// The gate closed before the write could start.
    Skipped,
}

/// One write of the journal file.
#[derive(Debug)]
pub struct Write {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub mode: WriteMode,
    gate: Gate,
}

impl Write {
    /// Write and sync the bytes, unless the gate has closed.
    ///
    /// # Errors
    ///
    /// The I/O error of a write that fails. An append to a missing file fails with
    /// [`io::ErrorKind::NotFound`].
    pub fn run(&self) -> io::Result<Written> {
        let (_running, open) = self.gate.enter();
        if !open {
            return Ok(Written::Skipped);
        }
        match self.mode {
            WriteMode::Whole => {
                if let Some(dir) = self.path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                write_private(&self.path, &self.bytes).map_err(|e| e.source)?;
            }
            WriteMode::Append => {
                let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
                file.write_all(&self.bytes)?;
                file.sync_data()?;
            }
        }
        Ok(Written::Done)
    }
}

/// The removal of the journal file, after any write running.
#[derive(Debug)]
pub struct Removal {
    pub path: PathBuf,
    gate: Gate,
}

impl Removal {
    /// Remove the file, unless the gate has closed. A missing file counts as removed.
    ///
    /// # Errors
    ///
    /// The I/O error of a removal that fails.
    pub fn run(&self) -> io::Result<Written> {
        let (_running, open) = self.gate.enter();
        if !open {
            return Ok(Written::Skipped);
        }
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(Written::Done),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Written::Done),
            Err(error) => Err(error),
        }
    }
}

/// What the journal file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnDisk {
    /// Not known, so the next write replaces it whole: before the first write, after a
    /// restore and after a failed write.
    Unknown,
    /// This many commands, which later writes append to.
    Commands(usize),
    /// A removal runs, and no write starts until it ends.
    Removing,
    /// A crashed session's journal, offered for a restore. No write replaces it until the
    /// answer: a crash while the offer is open still finds it.
    Offered,
}

/// This session's journal and its writes.
pub struct Recovery {
    dir: PathBuf,
    /// The path that names the journal: the output, or what stands in for it (spec §6.2).
    name: Option<PathBuf>,
    log: Log,
    on_disk: OnDisk,
    gate: Gate,
}

/// The journal, once the inputs' hashes have named its file.
enum Log {
    /// The inputs hash off the UI thread. Commands wait here until they finish.
    Hashing {
        commands: Vec<Command>,
    },
    Ready {
        path: PathBuf,
        journal: Journal,
    },
}

impl Recovery {
    /// A journal in `dir`, named for `name`, whose inputs are still hashing.
    #[must_use]
    pub fn new(dir: PathBuf, name: Option<PathBuf>) -> Self {
        Self {
            dir,
            name,
            log: Log::Hashing { commands: Vec::new() },
            on_disk: OnDisk::Unknown,
            gate: Gate::new(),
        }
    }

    /// The journal's file, once the inputs are hashed.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match &self.log {
            Log::Ready { path, .. } => Some(path),
            Log::Hashing { .. } => None,
        }
    }

    pub fn record(&mut self, command: Command) {
        match &mut self.log {
            Log::Hashing { commands } => commands.push(command),
            Log::Ready { journal, .. } => journal.record(command),
        }
    }

    /// Name the file from `hashes` and move the waiting commands into the journal, for a
    /// document that started from `start`. Returns whether the inputs were still hashing.
    pub fn hashed(&mut self, hashes: InputHashes, start: Start) -> bool {
        let Log::Hashing { commands } = &mut self.log else {
            return false;
        };
        let mut journal = Journal::with_hashes(hashes, start);
        for command in std::mem::take(commands) {
            journal.record(command);
        }
        let path = self.dir.join(journal.file_name(self.name.as_deref()));
        self.log = Log::Ready { path, journal };
        true
    }

    fn len(&self) -> usize {
        match &self.log {
            Log::Hashing { commands } => commands.len(),
            Log::Ready { journal, .. } => journal.commands().len(),
        }
    }

    /// The file lacks commands.
    fn behind(&self) -> bool {
        let len = self.len();
        len > 0 && self.on_disk != OnDisk::Commands(len)
    }

    /// The next write and the number of commands the file holds after it: none while the
    /// inputs hash or when the file is up to date.
    pub fn next_write(&self) -> Option<(Write, usize)> {
        let Log::Ready { path, journal } = &self.log else {
            return None;
        };
        if !self.behind() {
            return None;
        }
        let upto = journal.commands().len();
        let (mode, bytes) = match self.on_disk {
            OnDisk::Commands(from) => (WriteMode::Append, journal.lines_from(from)),
            OnDisk::Unknown => (WriteMode::Whole, journal.to_lines()),
            OnDisk::Removing | OnDisk::Offered => return None,
        };
        match bytes {
            Ok(bytes) => {
                Some((Write { path: path.clone(), bytes, mode, gate: self.gate.clone() }, upto))
            }
            Err(error) => {
                tracing::warn!(%error, "cannot serialise the journal");
                None
            }
        }
    }

    /// Take the result of a write in `mode` that leaves the file with `upto` commands.
    /// Returns whether the file still lacks commands a write should carry now.
    pub fn written(&mut self, mode: WriteMode, upto: usize, result: io::Result<Written>) -> bool {
        match result {
            Ok(Written::Done) => {
                self.on_disk = OnDisk::Commands(upto);
                true
            }
            Ok(Written::Skipped) => false,
            Err(error) => {
                self.on_disk = OnDisk::Unknown;
                let path = self.path().map(|path| path.display().to_string());
                if mode == WriteMode::Append {
                    tracing::warn!(%error, ?path, "cannot append to the journal; writing it whole");
                    true
                } else {
                    tracing::warn!(%error, ?path, "cannot write the journal");
                    // Only commands that came during the failed write try again.
                    self.len() > upto
                }
            }
        }
    }

    /// Write the journal now, on this thread, after any operation running, hashing the inputs
    /// with `hashes` if they are still hashing: the session is ending. `start` is the
    /// document's.
    pub fn flush(&mut self, hashes: impl FnOnce() -> InputHashes, start: Start) {
        // The file holds every command this session wrote, and no write is in flight: each
        // write's result is taken before the next starts.
        if self.on_disk == OnDisk::Commands(self.len()) {
            return;
        }
        self.retire();
        if matches!(self.log, Log::Hashing { .. }) {
            self.hashed(hashes(), start);
        }
        let Some((write, upto)) = self.next_write() else {
            return;
        };
        match write.run() {
            Ok(Written::Done) => self.on_disk = OnDisk::Commands(upto),
            Ok(Written::Skipped) => {}
            Err(error) => {
                tracing::warn!(%error, path = %write.path.display(), "cannot write the journal");
            }
        }
    }

    /// End the writes so far and return the removal of the file, which runs after any write
    /// running; none before the inputs are hashed, when this session has written no file. No
    /// write starts until [`Recovery::removed`] takes the removal's result.
    pub fn discard(&mut self) -> Option<Removal> {
        self.retire();
        let path = self.path()?.to_owned();
        self.on_disk = OnDisk::Removing;
        Some(Removal { path, gate: self.gate.clone() })
    }

    /// Take the result of the removal [`Recovery::discard`] gave. Returns whether the file
    /// lacks commands a write should carry now.
    pub fn removed(&mut self, result: io::Result<Written>) -> bool {
        if let Err(error) = result {
            // The next write replaces the file whole, so it holds no stale commands.
            let path = self.path().map(|path| path.display().to_string());
            tracing::warn!(%error, ?path, "cannot remove the journal");
        }
        if self.on_disk == OnDisk::Removing {
            self.on_disk = OnDisk::Unknown;
        }
        self.behind()
    }

    /// Where this session's journal goes once the inputs hash, for a thread of its own.
    #[must_use]
    pub fn locator(&self) -> Locator {
        Locator { dir: self.dir.clone(), name: self.name.clone() }
    }

    /// The journal a crashed session left for these inputs, if it has this format and holds
    /// commands.
    #[must_use]
    pub fn saved(&self) -> Option<Journal> {
        read_saved(self.path()?)
    }

    /// Offer `saved`, the journal a crashed session left, for a restore. Writes wait for the
    /// answer: [`Recovery::restored`], or [`Recovery::discard`] to start over.
    pub fn offer(&mut self, saved: Option<Journal>) -> Option<Journal> {
        let journal = saved?;
        self.on_disk = OnDisk::Offered;
        Some(journal)
    }

    /// Carry on from `journal`, which the session was restored from.
    pub fn restored(&mut self, journal: Journal) {
        // The file may end in a line the crash cut short, so the next write replaces it.
        self.retire();
        // A restore is offered only once the inputs are hashed.
        if let Log::Ready { journal: current, .. } = &mut self.log {
            *current = journal;
        }
    }

    /// End the writes so far without waiting: one running ends before any later one starts,
    /// the rest skip, and the next one writes whole. The front end cancels its tasks first.
    fn retire(&mut self) {
        self.gate.close();
        self.gate = self.gate.next();
        self.on_disk = OnDisk::Unknown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use merge_core::document::{Labels, MergeDocument, MergeOptions};
    use merge_core::index::RegionIx;
    use merge_core::journal::{JOURNAL_VERSION, input_hashes};
    use merge_core::merge3::Inputs;
    use merge_core::testing::three;
    use std::sync::{Barrier, TryLockError};

    const DAY: Duration = Duration::from_secs(SECONDS_PER_DAY);

    /// A fixed clock, well after the epoch.
    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + 20_000 * DAY
    }

    fn file_written_at(path: &Path, modified: SystemTime) {
        let file = std::fs::File::create(path).expect("created");
        file.set_modified(modified).expect("set");
    }

    #[test]
    fn prune_deletes_a_journal_older_than_thirty_days() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("old.jsonl");
        file_written_at(&path, now() - 30 * DAY - Duration::from_secs(1));
        prune(dir.path(), now());
        assert!(!path.exists());
    }

    #[test]
    fn prune_keeps_a_journal_of_exactly_thirty_days() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("edge.jsonl");
        file_written_at(&path, now() - 30 * DAY);
        prune(dir.path(), now());
        assert!(path.exists());
    }

    #[test]
    fn prune_keeps_a_journal_written_in_the_future() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("skewed.jsonl");
        file_written_at(&path, now() + DAY);
        prune(dir.path(), now());
        assert!(path.exists());
    }

    #[test]
    fn prune_keeps_what_is_not_a_journal_and_deletes_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = now() - 31 * DAY;
        let other = dir.path().join("notes.txt");
        file_written_at(&other, old);
        let directory = dir.path().join("directory.jsonl");
        std::fs::create_dir(&directory).expect("created");
        let journal = dir.path().join("old.jsonl");
        file_written_at(&journal, old);
        prune(dir.path(), now());
        assert!(other.exists());
        assert!(directory.exists());
        assert!(!journal.exists());
    }

    #[cfg(unix)]
    #[test]
    fn prune_goes_on_past_what_it_cannot_delete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let old = now() - 31 * DAY;
        let directory = dir.path().join("a-directory.jsonl");
        std::fs::create_dir(&directory).expect("created");
        std::fs::File::open(&directory).expect("opened").set_modified(old).expect("set");
        let journal = dir.path().join("old.jsonl");
        file_written_at(&journal, old);
        prune(dir.path(), now());
        assert!(directory.exists());
        assert!(!journal.exists());
    }

    #[test]
    fn prune_of_a_missing_directory_makes_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("journal");
        prune(&missing, now());
        assert!(!missing.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_whole_write_keeps_the_journal_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("j.jsonl");
        std::fs::write(&path, b"h\n").expect("written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("set");
        write(path.clone(), b"h\nc\n", WriteMode::Whole, &Gate::new()).run().expect("written");
        let mode = std::fs::metadata(&path).expect("exists").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    fn write(path: PathBuf, bytes: &[u8], mode: WriteMode, gate: &Gate) -> Write {
        Write { path, bytes: bytes.to_vec(), mode, gate: gate.clone() }
    }

    #[test]
    fn a_whole_write_makes_the_directory_and_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("journal").join("j.jsonl");
        let done = write(path.clone(), b"h\nc\n", WriteMode::Whole, &Gate::new()).run();
        assert_eq!(done.expect("written"), Written::Done);
        assert_eq!(std::fs::read(&path).expect("readable"), b"h\nc\n");
    }

    #[test]
    fn an_append_adds_to_the_end() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("j.jsonl");
        std::fs::write(&path, b"h\n").expect("written");
        let done = write(path.clone(), b"c\n", WriteMode::Append, &Gate::new()).run();
        assert_eq!(done.expect("appended"), Written::Done);
        assert_eq!(std::fs::read(&path).expect("readable"), b"h\nc\n");
    }

    #[test]
    fn an_append_to_a_missing_file_fails_and_makes_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("j.jsonl");
        let error = write(path.clone(), b"c\n", WriteMode::Append, &Gate::new())
            .run()
            .expect_err("no file to append to");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!path.exists());
    }

    #[test]
    fn a_closed_gate_skips_every_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("j.jsonl");
        std::fs::write(&path, b"h\n").expect("written");
        let gate = Gate::new();
        let whole = write(path.clone(), b"other\n", WriteMode::Whole, &gate);
        let append = write(path.clone(), b"c\n", WriteMode::Append, &gate);
        gate.close();
        assert_eq!(whole.run().expect("no error"), Written::Skipped);
        assert_eq!(append.run().expect("no error"), Written::Skipped);
        assert_eq!(std::fs::read(&path).expect("readable"), b"h\n");
    }

    #[test]
    fn closing_one_gate_leaves_a_new_one_open() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("j.jsonl");
        Gate::new().close();
        let done = write(path.clone(), b"h\n", WriteMode::Whole, &Gate::new()).run();
        assert_eq!(done.expect("written"), Written::Done);
    }

    // `Recovery`: the session's journal and its writes.

    fn inputs() -> Inputs {
        three("b\n", "l\n", "r\n")
    }

    fn hashes() -> InputHashes {
        input_hashes(&inputs())
    }

    /// How [`inputs`] start with the default options.
    fn start() -> Start {
        MergeDocument::new(inputs(), MergeOptions::default(), Labels::none())
            .expect("no labels")
            .start()
    }

    fn clear(region: RegionIx) -> Command {
        Command::Clear { region }
    }

    /// A journal in `dir`, its inputs hashed, holding `commands`.
    fn ready(dir: &Path, commands: usize) -> Recovery {
        let mut recovery = Recovery::new(dir.to_owned(), None);
        recovery.hashed(hashes(), start());
        (0..commands).for_each(|region| recovery.record(clear(RegionIx::new(region))));
        recovery
    }

    fn file_commands(recovery: &Recovery) -> Vec<Command> {
        let path = recovery.path().expect("hashed");
        let bytes = std::fs::read(path).expect("readable");
        Journal::parse(&bytes).expect("parses").commands().to_vec()
    }

    /// Run the next write and report it, as the window does.
    fn write_next(recovery: &mut Recovery) -> bool {
        let (write, upto) = recovery.next_write().expect("a write");
        let result = write.run();
        recovery.written(write.mode, upto, result)
    }

    #[test]
    fn commands_wait_for_the_hash_and_then_write_whole() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = Recovery::new(dir.path().to_owned(), None);
        recovery.record(clear(RegionIx::new(0)));
        recovery.record(clear(RegionIx::new(1)));
        assert!(recovery.next_write().is_none());
        assert!(recovery.path().is_none());
        assert!(recovery.hashed(hashes(), start()));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 2));
        let file = Journal::with_hashes(hashes(), start()).file_name(None);
        assert_eq!(write.path, dir.path().join(file));
        assert_eq!(write.run().expect("written"), Written::Done);
        assert_eq!(file_commands(&recovery), [clear(RegionIx::new(0)), clear(RegionIx::new(1))]);
    }

    #[test]
    fn the_journal_name_names_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let name = PathBuf::from("/repo/a.txt");
        let mut recovery = Recovery::new(dir.path().to_owned(), Some(name.clone()));
        recovery.hashed(hashes(), start());
        let file = Journal::with_hashes(hashes(), start()).file_name(Some(&name));
        assert_eq!(recovery.path(), Some(dir.path().join(file).as_path()));
    }

    #[test]
    fn a_second_hash_changes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        let other = input_hashes(&three("x\n", "y\n", "z\n"));
        assert!(!recovery.hashed(other, start()));
        let file = Journal::with_hashes(hashes(), start()).file_name(None);
        assert_eq!(recovery.path(), Some(dir.path().join(file).as_path()));
        assert_eq!(recovery.next_write().expect("a write").1, 1);
    }

    #[test]
    fn a_journal_up_to_date_has_no_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 0);
        assert!(recovery.next_write().is_none());
        recovery.record(clear(RegionIx::new(0)));
        assert!(write_next(&mut recovery));
        assert!(recovery.next_write().is_none());
    }

    #[test]
    fn after_a_write_the_next_appends_only_the_new_commands() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 2);
        assert!(write_next(&mut recovery));
        recovery.record(clear(RegionIx::new(2)));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Append, 3));
        let line = write.bytes.strip_suffix(b"\n").expect("a whole line");
        let appended: Command = serde_json::from_slice(line).expect("one command");
        assert_eq!(appended, clear(RegionIx::new(2)));
        recovery.written(write.mode, upto, write.run());
        assert_eq!(
            file_commands(&recovery),
            [clear(RegionIx::new(0)), clear(RegionIx::new(1)), clear(RegionIx::new(2))]
        );
    }

    #[test]
    fn a_failed_append_writes_whole_next() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        recovery.record(clear(RegionIx::new(1)));
        let failed = Err(io::Error::from(io::ErrorKind::NotFound));
        assert!(recovery.written(WriteMode::Append, 2, failed));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 2));
    }

    #[test]
    fn a_failed_whole_write_tries_again_only_for_newer_commands() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 2);
        let failed = || Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(!recovery.written(WriteMode::Whole, 2, failed()));
        recovery.record(clear(RegionIx::new(2)));
        assert!(recovery.written(WriteMode::Whole, 2, failed()));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 3));
    }

    #[test]
    fn a_skipped_write_leaves_the_next_whole() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(!recovery.written(WriteMode::Whole, 1, Ok(Written::Skipped)));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 1));
    }

    #[test]
    fn flush_while_hashing_hashes_and_writes_whole() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = Recovery::new(dir.path().to_owned(), None);
        recovery.record(clear(RegionIx::new(0)));
        recovery.flush(hashes, start());
        assert_eq!(file_commands(&recovery), [clear(RegionIx::new(0))]);
        assert!(recovery.next_write().is_none());
    }

    #[test]
    fn flush_rewrites_the_file_whole_and_skips_writes_taken_before() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        recovery.record(clear(RegionIx::new(1)));
        let (stale, _) = recovery.next_write().expect("a write");
        std::fs::write(recovery.path().expect("hashed"), b"cut short").expect("written");
        recovery.flush(hashes, start());
        assert_eq!(file_commands(&recovery), [clear(RegionIx::new(0)), clear(RegionIx::new(1))]);
        assert_eq!(stale.run().expect("no error"), Written::Skipped);
    }

    /// Discard the journal and run its removal, as the window does.
    fn discard_now(recovery: &mut Recovery) -> bool {
        let removal = recovery.discard().expect("a named journal");
        let result = removal.run();
        recovery.removed(result)
    }

    #[test]
    fn discard_removes_the_file_and_the_next_write_is_whole() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        let path = recovery.path().expect("hashed").to_owned();
        assert!(discard_now(&mut recovery), "the file lacks the command");
        assert!(!path.exists());
        recovery.record(clear(RegionIx::new(1)));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 2));
    }

    #[test]
    fn discard_before_the_hash_and_of_a_missing_file_removes_nothing_else() {
        let dir = tempfile::tempdir().expect("tempdir");
        let other = dir.path().join("other.jsonl");
        std::fs::write(&other, b"h\n").expect("written");
        assert!(Recovery::new(dir.path().to_owned(), None).discard().is_none());
        let removal = ready(dir.path(), 1).discard().expect("a named journal");
        assert_eq!(removal.run().expect("no error"), Written::Done);
        assert!(other.exists());
        assert_eq!(std::fs::read_dir(dir.path()).expect("listed").count(), 1);
    }

    #[test]
    fn no_write_starts_until_the_removal_ends() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        let removal = recovery.discard().expect("a named journal");
        recovery.record(clear(RegionIx::new(1)));
        assert!(recovery.next_write().is_none(), "the removal would delete it");
        assert!(recovery.removed(removal.run()));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 2));
        assert!(recovery.written(write.mode, upto, write.run()));
        assert_eq!(file_commands(&recovery), [clear(RegionIx::new(0)), clear(RegionIx::new(1))]);
    }

    #[test]
    fn a_removal_without_new_commands_starts_no_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        let removal = recovery.discard().expect("a named journal");
        assert!(recovery.removed(removal.run()), "the file lacks the command it held");
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 1));
        let mut empty = ready(dir.path(), 0);
        let removal = empty.discard().expect("a named journal");
        assert!(!empty.removed(removal.run()), "no commands to write");
        assert!(empty.next_write().is_none());
    }

    #[test]
    fn a_failed_removal_lets_the_next_write_replace_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        let _removal = recovery.discard().expect("a named journal");
        recovery.record(clear(RegionIx::new(1)));
        let failed = Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(recovery.removed(failed));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 2));
    }

    #[test]
    fn a_removal_taken_before_a_flush_skips_and_keeps_the_flushed_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        let removal = recovery.discard().expect("a named journal");
        recovery.record(clear(RegionIx::new(1)));
        recovery.flush(hashes, start());
        assert_eq!(removal.run().expect("no error"), Written::Skipped);
        assert_eq!(file_commands(&recovery), [clear(RegionIx::new(0)), clear(RegionIx::new(1))]);
    }

    /// Hold the lock that orders the journal's file operations on another thread, as a
    /// running write does, while `during` runs on this one.
    fn while_a_write_runs<T: Send>(gate: &Gate, during: impl FnOnce() -> T + Send) -> T {
        let held = Barrier::new(2);
        let release = Barrier::new(2);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let _running = gate.order.lock().expect("not poisoned");
                held.wait();
                release.wait();
            });
            held.wait();
            let result = during();
            release.wait();
            result
        })
    }

    #[test]
    fn retiring_does_not_wait_for_a_running_write() {
        // A retire that waited would deadlock here, and nextest would end the test.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        let gate = recovery.gate.clone();
        let next = while_a_write_runs(&gate, || {
            let removal = recovery.discard().expect("a named journal");
            recovery.restored(Journal::with_hashes(hashes(), start()));
            removal
        });
        assert_eq!(next.run().expect("no error"), Written::Skipped, "restored retired it");
    }

    #[test]
    fn a_write_of_the_next_journal_waits_for_a_running_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        let running = recovery.gate.clone();
        let (next, removal) = while_a_write_runs(&running, || {
            let removal = recovery.discard().expect("a named journal");
            let next = recovery.gate.clone();
            let blocked = matches!(next.order.try_lock(), Err(TryLockError::WouldBlock));
            (blocked, matches!(removal.gate.order.try_lock(), Err(TryLockError::WouldBlock)))
        });
        assert!(next, "a later write runs after the running one");
        assert!(removal, "the removal runs after the running write");
    }

    #[test]
    fn saved_reads_the_journal_a_crash_left() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut crashed = ready(dir.path(), 2);
        assert!(write_next(&mut crashed));
        let path = crashed.path().expect("hashed").to_owned();
        let mut bytes = std::fs::read(&path).expect("readable");
        bytes.extend_from_slice(b"{\"Clear\":");
        std::fs::write(&path, bytes).expect("written");
        let saved = ready(dir.path(), 0).saved().expect("a journal");
        assert_eq!(saved.commands(), [clear(RegionIx::new(0)), clear(RegionIx::new(1))]);
    }

    #[test]
    fn saved_offers_nothing_unless_a_current_journal_holds_commands() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(Recovery::new(dir.path().to_owned(), None).saved().is_none());
        let recovery = ready(dir.path(), 0);
        assert!(recovery.saved().is_none(), "no file");
        let path = recovery.path().expect("hashed").to_owned();
        let empty = Journal::with_hashes(hashes(), start());
        let header = empty.to_lines().expect("serialises");
        std::fs::write(&path, &header).expect("written");
        assert!(recovery.saved().is_none(), "no commands");
        std::fs::write(&path, b"not json\n").expect("written");
        assert!(recovery.saved().is_none(), "does not parse");
        let mut full = empty;
        full.record(clear(RegionIx::new(0)));
        let current = format!("\"version\":{JOURNAL_VERSION}");
        let old = String::from_utf8(full.to_lines().expect("serialises")).expect("utf-8").replacen(
            &current,
            "\"version\":1",
            1,
        );
        assert_ne!(old.as_bytes(), full.to_lines().expect("serialises"));
        std::fs::write(&path, old).expect("written");
        assert!(recovery.saved().is_none(), "another format");
    }

    /// A crashed session's journal of `commands` commands in `dir`, for [`ready`]'s inputs.
    fn crashed(dir: &Path, commands: usize) -> PathBuf {
        let mut crashed = ready(dir, commands);
        assert!(write_next(&mut crashed));
        crashed.path().expect("hashed").to_owned()
    }

    /// Read the saved journal, as the background does, and offer it.
    fn offer_saved(recovery: &mut Recovery) -> Option<Journal> {
        let saved = recovery.saved();
        recovery.offer(saved)
    }

    #[test]
    fn the_locator_reads_the_journal_the_hashed_inputs_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = crashed(dir.path(), 2);
        let mut recovery = Recovery::new(dir.path().to_owned(), None);
        let found = recovery.locator().find(&inputs(), &start());
        assert_eq!(found.saved.as_ref().map(|j| j.commands().len()), Some(2));
        assert!(recovery.hashed(found.hashes, start()));
        assert_eq!(recovery.path(), Some(path.as_path()));
    }

    #[test]
    fn the_locator_names_the_journal_by_its_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        crashed(dir.path(), 1);
        let recovery = Recovery::new(dir.path().to_owned(), Some(PathBuf::from("/repo/a.txt")));
        assert!(recovery.locator().find(&inputs(), &start()).saved.is_none(), "another name");
    }

    #[test]
    fn flush_of_a_journal_up_to_date_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        let path = recovery.path().expect("hashed").to_owned();
        std::fs::write(&path, b"marker").expect("written");
        recovery.flush(hashes, start());
        assert_eq!(std::fs::read(&path).expect("kept"), b"marker");
    }

    #[test]
    fn an_open_offer_holds_every_write_until_the_answer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = crashed(dir.path(), 2);
        let mut recovery = ready(dir.path(), 0);
        let offered = offer_saved(&mut recovery).expect("a journal to offer");
        assert_eq!(offered.commands().len(), 2);
        recovery.record(clear(RegionIx::new(5)));
        assert!(recovery.next_write().is_none(), "the crashed journal stays");
        assert_eq!(
            Journal::parse(&std::fs::read(&path).expect("kept")).expect("parses").commands().len(),
            2
        );
        assert!(discard_now(&mut recovery), "start over: the new command waits");
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 1));
    }

    #[test]
    fn a_restore_ends_the_hold() {
        let dir = tempfile::tempdir().expect("tempdir");
        crashed(dir.path(), 1);
        let mut recovery = ready(dir.path(), 0);
        let offered = offer_saved(&mut recovery).expect("a journal to offer");
        recovery.restored(offered);
        recovery.record(clear(RegionIx::new(1)));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 2));
    }

    #[test]
    fn without_a_journal_to_offer_nothing_is_held() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 0);
        assert!(offer_saved(&mut recovery).is_none());
        recovery.record(clear(RegionIx::new(0)));
        assert_eq!(recovery.next_write().expect("a write").1, 1);
    }

    #[test]
    fn restored_replaces_the_commands_and_writes_whole_next() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut recovery = ready(dir.path(), 1);
        assert!(write_next(&mut recovery));
        let mut journal = Journal::with_hashes(hashes(), start());
        journal.record(clear(RegionIx::new(5)));
        journal.record(clear(RegionIx::new(6)));
        recovery.restored(journal);
        recovery.record(clear(RegionIx::new(7)));
        let (write, upto) = recovery.next_write().expect("a write");
        assert_eq!((write.mode, upto), (WriteMode::Whole, 3));
        recovery.written(write.mode, upto, write.run());
        assert_eq!(
            file_commands(&recovery),
            [clear(RegionIx::new(5)), clear(RegionIx::new(6)), clear(RegionIx::new(7))]
        );
    }
}
