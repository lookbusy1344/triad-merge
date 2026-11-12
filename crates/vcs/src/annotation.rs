//! The commit that last changed each line of a file (spec §7.3, `docs/DESIGN.md` §C4).

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;
use std::ops::Range;

use crate::ChangeInfo;

/// A moment as the VCS records it: seconds since the Unix epoch, and the recorder's offset
/// from UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    pub seconds: i64,
    pub offset_seconds: i32,
}

/// The commit that last changed a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineOrigin {
    pub change: ChangeInfo,
    pub author: String,
    /// When the author made the change.
    pub time: Timestamp,
}

/// Lines `lines` of a file come from `origins[origin]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginRun {
    pub lines: Range<usize>,
    pub origin: usize,
}

/// The origins of one file's lines. Each commit appears once in `origins`, in the order its
/// first line appears. The runs lie in order without overlapping, and touching runs have
/// different origins. A line no run covers has no origin: a backend covers every line, and an
/// annotation carried onto edited text leaves the changed lines out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Annotation {
    pub origins: Vec<LineOrigin>,
    pub runs: Vec<OriginRun>,
}

impl Annotation {
    /// The annotation of runs of lines, each with the key of its commit, in order and
    /// covering the file. `origin` gives a commit's origin; it runs once per distinct key, in
    /// order of first appearance. Neighbouring runs with one key merge.
    ///
    /// # Errors
    ///
    /// The first error `origin` returns.
    pub fn from_runs<K: Eq + Hash, E>(
        runs: impl IntoIterator<Item = (Range<usize>, K)>,
        mut origin: impl FnMut(&K) -> Result<LineOrigin, E>,
    ) -> Result<Self, E> {
        let mut annotation = Self::default();
        let mut index: HashMap<K, usize> = HashMap::new();
        for (lines, key) in runs {
            let at = match index.entry(key) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    annotation.origins.push(origin(entry.key())?);
                    *entry.insert(annotation.origins.len() - 1)
                }
            };
            match annotation.runs.last_mut() {
                Some(last) if last.origin == at && last.lines.end == lines.start => {
                    last.lines.end = lines.end;
                }
                _ => annotation.runs.push(OriginRun { lines, origin: at }),
            }
        }
        Ok(annotation)
    }

    /// The origin of line `line`, or `None` past the last line.
    #[must_use]
    pub fn origin(&self, line: usize) -> Option<&LineOrigin> {
        let run = self.runs.partition_point(|run| run.lines.end <= line);
        self.runs
            .get(run)
            .filter(|run| run.lines.contains(&line))
            .map(|run| &self.origins[run.origin])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(summary: &str) -> LineOrigin {
        LineOrigin {
            change: ChangeInfo {
                commit: format!("c-{summary}"),
                change_id: None,
                summary: Some(summary.to_owned()),
            },
            author: "triad".to_owned(),
            time: Timestamp { seconds: 0, offset_seconds: 0 },
        }
    }

    fn run(lines: Range<usize>, origin: usize) -> OriginRun {
        OriginRun { lines, origin }
    }

    fn build(runs: &[(Range<usize>, &'static str)]) -> (Annotation, Vec<&'static str>) {
        let mut asked = Vec::new();
        let annotation = Annotation::from_runs(runs.iter().cloned(), |key: &&str| {
            asked.push(*key);
            Ok::<_, ()>(origin(key))
        })
        .unwrap();
        (annotation, asked)
    }

    #[test]
    fn each_commit_is_looked_up_once_in_order_of_first_appearance() {
        let (annotation, asked) = build(&[(0..1, "b"), (1..3, "a"), (3..4, "b")]);
        assert_eq!(asked, ["b", "a"]);
        assert_eq!(annotation.origins, [origin("b"), origin("a")]);
        assert_eq!(annotation.runs, [run(0..1, 0), run(1..3, 1), run(3..4, 0)]);
    }

    #[test]
    fn neighbouring_runs_of_one_commit_merge() {
        let (annotation, _) = build(&[(0..1, "a"), (1..2, "a"), (2..5, "a"), (5..6, "b")]);
        assert_eq!(annotation.runs, [run(0..5, 0), run(5..6, 1)]);
    }

    #[test]
    fn no_runs_give_an_empty_annotation() {
        let (annotation, asked) = build(&[]);
        assert_eq!(annotation, Annotation::default());
        assert_eq!(asked, [""; 0]);
        assert_eq!(annotation.origin(0), None);
    }

    #[test]
    fn a_line_finds_its_runs_origin() {
        let (annotation, _) = build(&[(0..2, "a"), (2..3, "b"), (3..5, "a")]);
        let summary = |line| annotation.origin(line).and_then(|o| o.change.summary.as_deref());
        assert_eq!(
            (0..6).map(summary).collect::<Vec<_>>(),
            [Some("a"), Some("a"), Some("b"), Some("a"), Some("a"), None]
        );
    }

    #[test]
    fn the_first_lookup_error_stops_the_build() {
        let mut asked = Vec::new();
        let result = Annotation::from_runs([(0..1, "a"), (1..2, "b"), (2..3, "c")], |key| {
            asked.push(*key);
            if *key == "b" { Err("no b") } else { Ok(origin(key)) }
        });
        assert_eq!(result, Err("no b"));
        assert_eq!(asked, ["a", "b"]);
    }
}
