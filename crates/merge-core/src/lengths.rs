//! Lengths of a run of regions in a Fenwick tree: updating one length and finding the region
//! at an offset both cost O(log n). The merge result keeps its regions' byte lengths here, and
//! the view its regions' row counts.

use std::ops::Range;

/// The length of each region, in order.
#[derive(Debug, Clone)]
pub struct RegionLengths {
    lens: Vec<usize>,
    /// 1-based Fenwick tree: `tree[k]` sums the `k.isolate_lowest_one()` lengths ending at `k`.
    tree: Vec<usize>,
}

impl RegionLengths {
    #[must_use]
    pub fn new(lens: Vec<usize>) -> Self {
        let tree = Self::build(&lens);
        Self { lens, tree }
    }

    fn build(lens: &[usize]) -> Vec<usize> {
        let mut tree = vec![0; lens.len() + 1];
        for (i, &len) in lens.iter().enumerate() {
            let k = i + 1;
            tree[k] += len;
            let parent = k + k.isolate_lowest_one();
            if parent < tree.len() {
                tree[parent] += tree[k];
            }
        }
        tree
    }

    /// Replace the lengths of the regions in `range` with `lens`, rebuilding the tree: O(n).
    pub fn splice(&mut self, range: Range<usize>, lens: impl IntoIterator<Item = usize>) {
        self.lens.splice(range, lens);
        self.tree = Self::build(&self.lens);
    }

    /// The number of regions.
    #[must_use]
    pub fn count(&self) -> usize {
        self.lens.len()
    }

    #[must_use]
    pub fn len(&self, region: usize) -> usize {
        self.lens[region]
    }

    pub fn set(&mut self, region: usize, len: usize) {
        let old = std::mem::replace(&mut self.lens[region], len);
        let mut k = region + 1;
        while k < self.tree.len() {
            self.tree[k] = self.tree[k] - old + len;
            k += k.isolate_lowest_one();
        }
    }

    /// The offset where `region` starts.
    #[must_use]
    pub fn start(&self, region: usize) -> usize {
        let mut k = region;
        let mut sum = 0;
        while k > 0 {
            sum += self.tree[k];
            k -= k.isolate_lowest_one();
        }
        sum
    }

    #[must_use]
    pub fn total(&self) -> usize {
        self.start(self.lens.len())
    }

    /// The first region that starts at or after `offset`, or the region count when none does.
    #[must_use]
    pub fn first_starting_from(&self, offset: usize) -> usize {
        offset.checked_sub(1).map_or(0, |before| self.leading(before) + 1)
    }

    /// The region whose bytes include `offset`, or `None` at or past the end. Empty regions
    /// hold no bytes and are never returned.
    #[must_use]
    pub fn containing(&self, offset: usize) -> Option<usize> {
        let count = self.leading(offset);
        (count < self.lens.len()).then_some(count)
    }

    /// The most leading regions whose lengths sum to no more than `total`.
    fn leading(&self, total: usize) -> usize {
        let mut count = 0;
        let mut rest = total;
        let mut step = (self.tree.len() - 1).checked_ilog2().map_or(0, |bits| 1 << bits);
        while step > 0 {
            let next = count + step;
            if next < self.tree.len() && self.tree[next] <= rest {
                count = next;
                rest -= self.tree[next];
            }
            step >>= 1;
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_and_total_sum_the_lengths_before() {
        let lens = RegionLengths::new(vec![2, 0, 3, 1]);
        let starts: Vec<usize> = (0..4).map(|r| lens.start(r)).collect();
        assert_eq!(starts, [0, 2, 2, 5]);
        assert_eq!(lens.total(), 6);
        assert_eq!(lens.len(2), 3);
    }

    #[test]
    fn containing_skips_empty_regions_and_stops_at_the_end() {
        let lens = RegionLengths::new(vec![2, 0, 3, 1]);
        let found: Vec<Option<usize>> = (0..=7).map(|o| lens.containing(o)).collect();
        assert_eq!(found, [Some(0), Some(0), Some(2), Some(2), Some(2), Some(3), None, None]);
    }

    #[test]
    fn set_updates_later_starts_and_lookups() {
        let mut lens = RegionLengths::new(vec![2, 0, 3, 1]);
        lens.set(1, 4);
        assert_eq!((lens.start(2), lens.start(3), lens.total()), (6, 9, 10));
        assert_eq!(
            (lens.containing(2), lens.containing(5), lens.containing(6)),
            (Some(1), Some(1), Some(2))
        );
        lens.set(0, 0);
        assert_eq!((lens.start(1), lens.containing(0)), (0, Some(1)));
        assert_eq!(lens.len(1), 4);
    }

    #[test]
    fn first_starting_from_finds_the_first_region_at_or_past_an_offset() {
        // Starts 0, 2, 2, 5; the text ends at 6.
        let lens = RegionLengths::new(vec![2, 0, 3, 1]);
        let found: Vec<usize> = (0..=6).map(|o| lens.first_starting_from(o)).collect();
        assert_eq!(found, [0, 1, 1, 3, 3, 3, 4]);
        assert_eq!(RegionLengths::new(vec![0, 0]).first_starting_from(0), 0);
        assert_eq!(RegionLengths::new(vec![]).first_starting_from(0), 0);
    }

    #[test]
    fn splice_replaces_a_run_of_lengths() {
        let mut lens = RegionLengths::new(vec![2, 0, 3, 1]);
        lens.splice(1..3, [5]);
        let starts: Vec<usize> = (0..3).map(|r| lens.start(r)).collect();
        assert_eq!((starts, lens.total()), (vec![0, 2, 7], 8));
        assert_eq!((lens.len(1), lens.containing(7)), (5, Some(2)));
        lens.splice(1..2, [1, 0, 2]);
        let starts: Vec<usize> = (0..5).map(|r| lens.start(r)).collect();
        assert_eq!((starts, lens.total()), (vec![0, 2, 3, 3, 5], 6));
        assert_eq!(lens.containing(3), Some(3));
    }

    #[test]
    fn no_regions_or_only_empty_ones_contain_nothing() {
        assert_eq!(RegionLengths::new(vec![]).containing(0), None);
        assert_eq!(RegionLengths::new(vec![]).total(), 0);
        assert_eq!(RegionLengths::new(vec![0, 0]).containing(0), None);
    }

    #[test]
    fn many_regions_agree_with_a_linear_scan() {
        // Lengths 0..=4 repeating, so every tree level and empty regions are exercised.
        const REGIONS: usize = 37;
        const CYCLE: usize = 5;
        let lens: Vec<usize> = (0..REGIONS).map(|r| r % CYCLE).collect();
        let tree = RegionLengths::new(lens.clone());
        let mut start = 0;
        for (r, &len) in lens.iter().enumerate() {
            assert_eq!(tree.start(r), start, "start of {r}");
            for offset in start..start + len {
                assert_eq!(tree.containing(offset), Some(r), "offset {offset}");
            }
            start += len;
        }
        assert_eq!(tree.containing(start), None);
    }
}
