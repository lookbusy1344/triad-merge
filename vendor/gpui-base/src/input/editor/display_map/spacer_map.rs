// Added for Triad: spacer rows between buffer lines. See PATCHES.md.
//! SpacerMap: spacer projection layer (fold display rows → display rows).
//!
//! A spacer is a visual row that belongs to no buffer line, as two-way diff
//! alignment needs. Spacers are asked for above buffer lines; a line at or past
//! the end puts them after the last row. The map keeps sorted runs, each with a
//! running count of the spacer rows before it, so both directions are a binary
//! search, as in `FoldMap`.

/// Spacer rows that sit above one inner row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    /// The inner (fold display) row the spacers sit above; the inner row count
    /// for spacers after the last row.
    inner: usize,
    rows: usize,
    /// Spacer rows in the runs before this one.
    before: usize,
}

impl Run {
    /// The first display row of this run's spacers.
    fn start(&self) -> usize {
        self.inner + self.before
    }
}

/// What a display row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    /// An inner row.
    Real(usize),
    /// A spacer above inner row `below`, which equals the inner row count after
    /// the last row.
    Spacer { below: usize },
}

#[derive(Debug, Default)]
pub(super) struct SpacerMap {
    /// Requested spacers: (buffer line, rows), sorted by line, no zero rows.
    requested: Vec<(usize, usize)>,
    runs: Vec<Run>,
    total: usize,
    inner_count: usize,
}

impl SpacerMap {
    /// Ask for `rows` spacer rows above each buffer line. Lines may repeat and
    /// come in any order; their rows add up.
    pub(super) fn set(&mut self, mut spacers: Vec<(usize, usize)>) {
        spacers.retain(|&(_, rows)| rows > 0);
        spacers.sort_unstable_by_key(|&(line, _)| line);
        spacers.dedup_by(|later, kept| {
            let same = later.0 == kept.0;
            if same {
                kept.1 += later.1;
            }
            same
        });
        self.requested = spacers;
    }

    pub(super) fn requested(&self) -> &[(usize, usize)] {
        &self.requested
    }

    /// Place the requested spacers. `anchor` gives the inner row of a buffer
    /// line's first visible row, or `None` when the line is hidden; its spacers
    /// are then dropped. Lines at or past `buffer_lines` go after the last row.
    pub(super) fn rebuild(
        &mut self,
        inner_count: usize,
        buffer_lines: usize,
        anchor: impl Fn(usize) -> Option<usize>,
    ) {
        self.inner_count = inner_count;
        self.runs.clear();
        let mut before = 0;
        for &(line, rows) in &self.requested {
            let inner = if line >= buffer_lines {
                Some(inner_count)
            } else {
                anchor(line)
            };
            let Some(inner) = inner else { continue };
            match self.runs.last_mut() {
                // Lines past the end all land after the last row.
                Some(last) if last.inner == inner => last.rows += rows,
                _ => self.runs.push(Run {
                    inner,
                    rows,
                    before,
                }),
            }
            before += rows;
        }
        self.total = before;
    }

    /// Spacer rows in total.
    pub(super) fn total(&self) -> usize {
        self.total
    }

    /// Display rows: inner rows and spacers.
    pub(super) fn display_count(&self) -> usize {
        self.inner_count + self.total
    }

    /// The display row of an inner row.
    pub(super) fn to_display(&self, inner: usize) -> usize {
        // Runs at or before `inner` put their rows ahead of it.
        let ix = self.runs.partition_point(|run| run.inner <= inner);
        match ix.checked_sub(1).map(|i| self.runs[i]) {
            Some(run) => inner + run.before + run.rows,
            None => inner,
        }
    }

    /// What display row `row` is.
    pub(super) fn row(&self, row: usize) -> Row {
        // The last run starting at or before `row`.
        let ix = self.runs.partition_point(|run| run.start() <= row);
        let Some(run) = ix.checked_sub(1).map(|i| self.runs[i]) else {
            return Row::Real(row);
        };
        if row < run.start() + run.rows {
            Row::Spacer { below: run.inner }
        } else {
            Row::Real(row - run.before - run.rows)
        }
    }

    /// The spacer rows above inner row `inner`.
    pub(super) fn rows_above(&self, inner: usize) -> usize {
        self.runs
            .binary_search_by_key(&inner, |run| run.inner)
            .map_or(0, |ix| self.runs[ix].rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No folds and no wrapping: line n is inner row n.
    fn built(lines: usize, spacers: &[(usize, usize)]) -> SpacerMap {
        let mut map = SpacerMap::default();
        map.set(spacers.to_vec());
        map.rebuild(lines, lines, Some);
        map
    }

    /// Each display row, in order.
    fn rows(map: &SpacerMap) -> Vec<Row> {
        (0..map.display_count()).map(|r| map.row(r)).collect()
    }

    #[test]
    fn no_spacers_is_the_identity() {
        let map = built(3, &[]);
        assert_eq!(map.display_count(), 3);
        assert_eq!(rows(&map), [Row::Real(0), Row::Real(1), Row::Real(2)]);
        assert_eq!(map.to_display(2), 2);
    }

    #[test]
    fn spacers_sit_above_their_line() {
        // 0, two spacers, 1, 2, one spacer, 3.
        let map = built(4, &[(1, 2), (3, 1)]);
        assert_eq!(map.total(), 3);
        assert_eq!(
            rows(&map),
            [
                Row::Real(0),
                Row::Spacer { below: 1 },
                Row::Spacer { below: 1 },
                Row::Real(1),
                Row::Real(2),
                Row::Spacer { below: 3 },
                Row::Real(3),
            ]
        );
        assert_eq!(
            (0..4).map(|i| map.to_display(i)).collect::<Vec<_>>(),
            [0, 3, 4, 6]
        );
        assert_eq!(map.rows_above(1), 2);
        assert_eq!(map.rows_above(2), 0);
        assert_eq!(map.rows_above(3), 1);
    }

    #[test]
    fn spacers_above_line_0_come_first() {
        let map = built(2, &[(0, 1)]);
        assert_eq!(
            rows(&map),
            [Row::Spacer { below: 0 }, Row::Real(0), Row::Real(1)]
        );
        assert_eq!(map.to_display(0), 1);
    }

    #[test]
    fn spacers_past_the_end_follow_the_last_row() {
        let map = built(2, &[(2, 1), (9, 1)]);
        assert_eq!(
            rows(&map),
            [
                Row::Real(0),
                Row::Real(1),
                Row::Spacer { below: 2 },
                Row::Spacer { below: 2 },
            ]
        );
        assert_eq!(map.rows_above(2), 2);
    }

    #[test]
    fn repeated_lines_add_up_and_order_does_not_matter() {
        let map = built(3, &[(2, 1), (1, 1), (2, 2), (1, 0)]);
        assert_eq!(map.requested(), [(1, 1), (2, 3)]);
        assert_eq!(map.rows_above(2), 3);
        // 0, one spacer, 1, three spacers, 2.
        assert_eq!(map.to_display(2), 6);
    }

    #[test]
    fn a_hidden_line_drops_its_spacers() {
        let mut map = SpacerMap::default();
        map.set(vec![(1, 2), (2, 1)]);
        // Line 1 is folded away: lines 0 and 2 are inner rows 0 and 1.
        map.rebuild(2, 3, |line| match line {
            0 => Some(0),
            2 => Some(1),
            _ => None,
        });
        assert_eq!(map.total(), 1);
        assert_eq!(
            rows(&map),
            [Row::Real(0), Row::Spacer { below: 1 }, Row::Real(1)]
        );
    }

    #[test]
    fn wrapped_lines_anchor_at_their_first_row() {
        let mut map = SpacerMap::default();
        map.set(vec![(1, 1)]);
        // Line 0 wraps over inner rows 0 and 1; line 1 starts at inner row 2.
        map.rebuild(3, 2, |line| Some(line * 2));
        assert_eq!(map.to_display(1), 1, "the second row of line 0");
        assert_eq!(map.row(2), Row::Spacer { below: 2 });
        assert_eq!(map.to_display(2), 3);
    }

    #[test]
    fn a_rebuild_follows_new_anchors_and_set_replaces() {
        let mut map = built(3, &[(1, 1)]);
        map.rebuild(4, 4, Some);
        assert_eq!(map.display_count(), 5);
        map.set(vec![]);
        map.rebuild(4, 4, Some);
        assert_eq!(map.display_count(), 4);
        assert_eq!(map.row(3), Row::Real(3));
    }
}
