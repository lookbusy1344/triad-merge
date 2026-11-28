//! The texts a diff window wrote, or is writing, to one side's file. The file watcher reports
//! the window's own writes too; a change on disk that matches one of these is not news.

use merge_core::diff2::Rope;
use merge_core::text::Encoding;

/// One write's text, as the file holds it once the write lands.
#[derive(Debug, Clone, PartialEq)]
struct Written {
    text: Rope,
    encoding: Encoding,
}

/// A side's writes: those still running, and the last that landed.
#[derive(Debug, Default)]
pub(crate) struct OwnWrites {
    /// Writes started and not landed, by the pane's change count when each was taken.
    running: Vec<(u64, Written)>,
    landed: Option<Written>,
}

impl OwnWrites {
    /// A write of `text` taken at change count `changes` starts. It counts as the window's
    /// own from now, since the watcher can report it before its result arrives.
    pub(crate) fn start(&mut self, changes: u64, text: Rope, encoding: Encoding) {
        self.running.push((changes, Written { text, encoding }));
    }

    /// The write taken at `changes` ended; `ok` says whether it landed.
    pub(crate) fn ended(&mut self, changes: u64, ok: bool) {
        let Some(at) = self.running.iter().position(|(c, _)| *c == changes) else {
            return;
        };
        let (_, written) = self.running.remove(at);
        if ok {
            self.landed = Some(written);
        }
    }

    /// The file was read back into the pane: the last write no longer describes it.
    pub(crate) fn reloaded(&mut self) {
        self.landed = None;
    }

    /// Whether the file holding `text` in `encoding` is one of the window's own writes.
    pub(crate) fn holds(&self, text: &str, encoding: Encoding) -> bool {
        let own = |w: &Written| w.encoding == encoding && w.text == text;
        self.landed.as_ref().is_some_and(own) || self.running.iter().any(|(_, w)| own(w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(text: &str) -> Rope {
        Rope::from(text)
    }

    const UTF8: Encoding = Encoding::Utf8;

    #[test]
    fn a_running_write_is_own_before_it_lands() {
        let mut writes = OwnWrites::default();
        writes.start(1, rope("xa\n"), UTF8);
        assert!(writes.holds("xa\n", UTF8));
        assert!(!writes.holds("a\n", UTF8));
    }

    #[test]
    fn a_landed_write_stays_own_until_a_reload() {
        let mut writes = OwnWrites::default();
        writes.start(1, rope("xa\n"), UTF8);
        writes.ended(1, true);
        assert!(writes.holds("xa\n", UTF8));
        writes.reloaded();
        assert!(!writes.holds("xa\n", UTF8));
    }

    #[test]
    fn a_failed_write_is_not_own() {
        let mut writes = OwnWrites::default();
        writes.start(1, rope("a\n"), UTF8);
        writes.ended(1, true);
        writes.start(2, rope("xa\n"), UTF8);
        writes.ended(2, false);
        assert!(!writes.holds("xa\n", UTF8));
        assert!(writes.holds("a\n", UTF8), "the last landed write still is");
    }

    #[test]
    fn writes_end_by_their_change_count_in_any_order() {
        let mut writes = OwnWrites::default();
        writes.start(1, rope("x\n"), UTF8);
        writes.start(2, rope("xy\n"), UTF8);
        writes.ended(2, true);
        assert!(writes.holds("x\n", UTF8), "write 1 still runs");
        writes.ended(1, false);
        assert!(!writes.holds("x\n", UTF8));
        assert!(writes.holds("xy\n", UTF8));
    }

    #[test]
    fn the_encoding_must_match_too() {
        let mut writes = OwnWrites::default();
        writes.start(1, rope("a\n"), UTF8);
        assert!(!writes.holds("a\n", Encoding::Utf16Le));
    }
}
