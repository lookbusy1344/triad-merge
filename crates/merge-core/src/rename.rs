//! Rename detection between deleted and added files, by content, as jj's git backend finds
//! renames (spec §6.1).

/// A deleted or added file.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a, K> {
    /// The file's identity, which orders the search: git's blob ID in triad.
    pub id: K,
    /// The path, its components joined by `/`.
    pub location: &'a [u8],
    pub bytes: &'a [u8],
}

/// The deleted file `deleted` renamed to the added file `added`, as indices into the slices
/// given to [`renames`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rename {
    pub deleted: usize,
    pub added: usize,
}

/// The share of a deleted file's bytes an added file must keep to count as its rename.
pub const SIMILARITY: f32 = 0.5;

/// Above this many deleted files times this many added files, squared, the search keeps to
/// exact matches.
pub const LIMIT: usize = 1000;

/// The renames among `deleted` and `added`.
#[must_use]
pub fn renames<K: Ord>(deleted: &[Candidate<'_, K>], added: &[Candidate<'_, K>]) -> Vec<Rename> {
    renames_within(deleted, added, LIMIT)
}

/// [`renames`] under the limit `limit`.
fn renames_within<K: Ord>(
    deleted: &[Candidate<'_, K>],
    added: &[Candidate<'_, K>],
    limit: usize,
) -> Vec<Rename> {
    let sources = by_id_and_location(deleted);
    let destinations = by_id_and_location(added);
    let mut taken = vec![false; deleted.len()];
    let mut found = Vec::new();
    let mut matched = vec![false; added.len()];
    for &dest in &destinations {
        if added[dest].bytes.is_empty() {
            continue;
        }
        let same_id =
            sources.iter().copied().filter(|&s| !taken[s] && deleted[s].id == added[dest].id);
        if let Some(source) = preferring_name(same_id, deleted, &added[dest]) {
            taken[source] = true;
            matched[dest] = true;
            found.push(Rename { deleted: source, added: dest });
        }
    }
    let open_sources = taken.iter().filter(|&&t| !t).count();
    let open_destinations = matched.iter().filter(|&&m| !m).count();
    if exceeds(open_sources, open_destinations, limit) {
        return found;
    }
    for &dest in &destinations {
        let file = &added[dest];
        if matched[dest] || file.bytes.is_empty() || is_binary(file.bytes) {
            continue;
        }
        let mut best: Option<(usize, f32, bool)> = None;
        for &source in sources.iter().filter(|&&s| !taken[s] && !is_binary(deleted[s].bytes)) {
            let score = similarity(deleted[source].bytes, file.bytes);
            if score < SIMILARITY {
                continue;
            }
            let same_name = file_name(deleted[source].location) == file_name(file.location);
            let better = best.is_none_or(|(_, best_score, best_same_name)| {
                match score.total_cmp(&best_score) {
                    std::cmp::Ordering::Greater => true,
                    std::cmp::Ordering::Equal => same_name && !best_same_name,
                    std::cmp::Ordering::Less => false,
                }
            });
            if better {
                best = Some((source, score, same_name));
            }
        }
        if let Some((source, ..)) = best {
            taken[source] = true;
            found.push(Rename { deleted: source, added: dest });
        }
    }
    found
}

/// The indices of `files` ordered by identity, then by location.
fn by_id_and_location<K: Ord>(files: &[Candidate<'_, K>]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|&a, &b| {
        files[a].id.cmp(&files[b].id).then_with(|| files[a].location.cmp(files[b].location))
    });
    order
}

/// The first of `sources` whose file name is `dest`'s, else the first.
fn preferring_name<K>(
    mut sources: impl Iterator<Item = usize>,
    deleted: &[Candidate<'_, K>],
    dest: &Candidate<'_, K>,
) -> Option<usize> {
    let first = sources.next()?;
    let name = file_name(dest.location);
    if file_name(deleted[first].location) == name {
        return Some(first);
    }
    Some(sources.find(|&s| file_name(deleted[s].location) == name).unwrap_or(first))
}

fn exceeds(sources: usize, destinations: usize, limit: usize) -> bool {
    let limit = u128::try_from(limit).unwrap_or(u128::MAX);
    let product = u128::try_from(sources).unwrap_or(u128::MAX)
        * u128::try_from(destinations).unwrap_or(u128::MAX);
    product > limit.saturating_mul(limit)
}

/// The last component of `location`.
fn file_name(location: &[u8]) -> &[u8] {
    location.rsplit(|&byte| byte == b'/').next().unwrap_or(location)
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(crate::text::BINARY_SNIFF_BYTES).any(|&byte| byte == 0)
}

/// The share of `old`'s bytes that `new` keeps, over the larger of the two lengths: a
/// histogram line diff, each line with its terminator.
#[expect(
    clippy::cast_precision_loss,
    reason = "the score follows gix's f32 arithmetic, so a score at the threshold falls as jj's does"
)]
fn similarity(old: &[u8], new: &[u8]) -> f32 {
    let input = imara_diff::InternedInput::new(
        imara_diff::sources::byte_lines(old),
        imara_diff::sources::byte_lines(new),
    );
    let diff = imara_diff::Diff::compute(imara_diff::Algorithm::Histogram, &input);
    let removed: usize = diff
        .hunks()
        .flat_map(|hunk| hunk.before)
        .map(|token| input.interner[input.before[token as usize]].len())
        .sum();
    (old.len() - removed) as f32 / old.len().max(new.len()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file<'a>(id: u32, location: &'a str, bytes: &'a str) -> Candidate<'a, u32> {
        Candidate { id, location: location.as_bytes(), bytes: bytes.as_bytes() }
    }

    fn pairs(deleted: &[Candidate<'_, u32>], added: &[Candidate<'_, u32>]) -> Vec<(usize, usize)> {
        let mut found: Vec<_> =
            renames(deleted, added).into_iter().map(|r| (r.deleted, r.added)).collect();
        found.sort_unstable();
        found
    }

    #[test]
    fn identical_bytes_are_a_rename() {
        assert_eq!(pairs(&[file(1, "a.txt", "x\n")], &[file(1, "b/c.txt", "x\n")]), [(0, 0)]);
    }

    #[test]
    fn a_file_keeping_three_quarters_of_its_bytes_is_a_rename() {
        // "4\n" goes: 6 of 8 bytes stay.
        let deleted = [file(1, "a", "1\n2\n3\n4\n")];
        assert_eq!(pairs(&deleted, &[file(2, "b", "1\n2\n3\n5\n")]), [(0, 0)]);
    }

    #[test]
    fn half_the_bytes_kept_is_a_rename_and_less_is_not() {
        // "cd\n" goes: 3 of 6 bytes stay, exactly the threshold.
        assert_eq!(pairs(&[file(1, "a", "ab\ncd\n")], &[file(2, "b", "ab\nxy\n")]), [(0, 0)]);
        // "2\n3\n4\n" goes: 2 of 8 bytes stay.
        assert_eq!(pairs(&[file(1, "a", "1\n2\n3\n4\n")], &[file(2, "b", "1\n5\n6\n7\n")]), []);
        // "c\n" and "dd\n" go: 2 of 7 bytes stay.
        assert_eq!(pairs(&[file(1, "a", "ab\nc\ndd\n")], &[file(2, "b", "ab\n")]), []);
    }

    #[test]
    fn the_larger_file_sets_the_scale() {
        // Nothing of "a\n" goes, but the new file is four times its size: 2 / 8.
        assert_eq!(pairs(&[file(1, "a", "a\n")], &[file(2, "b", "a\nb\nc\nd\n")]), []);
    }

    #[test]
    fn the_most_similar_deleted_file_wins() {
        let deleted = [file(1, "s1", "1\n2\n8\n9\n"), file(2, "s2", "1\n2\n3\n9\n")];
        assert_eq!(pairs(&deleted, &[file(3, "d", "1\n2\n3\n4\n")]), [(1, 0)]);
    }

    #[test]
    fn a_tie_goes_to_the_deleted_file_of_the_same_name() {
        // Both keep 6 of 8 bytes; `x.rs` comes later in order.
        let deleted = [file(1, "b.rs", "1\n2\n3\n9\n"), file(2, "a/x.rs", "1\n2\n3\n8\n")];
        assert_eq!(pairs(&deleted, &[file(3, "dir/x.rs", "1\n2\n3\n4\n")]), [(1, 0)]);
        // The same for identical bytes.
        let deleted = [file(1, "a/y.rs", "x\n"), file(1, "b/x.rs", "x\n")];
        assert_eq!(pairs(&deleted, &[file(1, "c/x.rs", "x\n")]), [(1, 0)]);
    }

    #[test]
    fn a_tie_without_a_name_goes_to_the_first_by_identity_then_location() {
        let deleted = [file(2, "a", "1\n2\n3\n9\n"), file(1, "z", "1\n2\n3\n8\n")];
        assert_eq!(pairs(&deleted, &[file(3, "d", "1\n2\n3\n4\n")]), [(1, 0)]);
        let deleted = [file(1, "z", "x\n"), file(1, "m", "x\n")];
        assert_eq!(pairs(&deleted, &[file(1, "d", "x\n")]), [(1, 0)]);
    }

    #[test]
    fn a_deleted_file_is_renamed_once_by_the_first_added_file_in_order() {
        let added = [file(1, "b", "x\n"), file(1, "a", "x\n")];
        assert_eq!(pairs(&[file(1, "s", "x\n")], &added), [(0, 1)]);
        // Added files go in order of identity: `b`, id 2, before `a`, id 3.
        let added = [file(3, "a", "1\n2\n3\n5\n"), file(2, "b", "1\n2\n3\n6\n")];
        assert_eq!(pairs(&[file(1, "s", "1\n2\n3\n4\n")], &added), [(0, 1)]);
    }

    #[test]
    fn exact_matches_come_before_similar_ones() {
        // `a` sorts first and is similar; `b` is identical, so it takes the deleted file.
        let added = [file(1, "a", "1\n2\n3\n5\n"), file(2, "b", "1\n2\n3\n4\n")];
        assert_eq!(pairs(&[file(2, "s", "1\n2\n3\n4\n")], &added), [(0, 1)]);
    }

    #[test]
    fn an_empty_added_file_is_never_a_rename() {
        assert_eq!(pairs(&[file(1, "a", "")], &[file(1, "b", "")]), []);
        assert_eq!(pairs(&[file(1, "a", "x\n")], &[file(2, "b", "")]), []);
    }

    #[test]
    fn binary_files_match_only_when_identical() {
        assert_eq!(pairs(&[file(1, "a", "\0x\n")], &[file(1, "b", "\0x\n")]), [(0, 0)]);
        let deleted = [file(1, "a", "\0\n2\n3\n4\n")];
        assert_eq!(pairs(&deleted, &[file(2, "b", "\0\n2\n3\n5\n")]), []);
        assert_eq!(pairs(&[file(1, "a", "1\n2\n3\n4\n")], &[file(2, "b", "\0\n2\n3\n4\n")]), []);
        // A binary deleted file, though the text keeps 6 of its 8 bytes.
        assert_eq!(pairs(&[file(1, "a", "\0\n2\n3\n4\n")], &[file(2, "b", "1\n2\n3\n4\n")]), []);
    }

    #[test]
    fn past_the_limit_only_exact_matches_count() {
        let deleted = [file(1, "a", "1\n2\n3\n4\n"), file(9, "z", "q\n")];
        let added = [file(2, "b", "1\n2\n3\n5\n"), file(8, "y", "r\n")];
        let found = |deleted: &[Candidate<'_, u32>], limit| {
            renames_within(deleted, &added, limit)
                .into_iter()
                .map(|r| (r.deleted, r.added))
                .collect::<Vec<_>>()
        };
        // 2 × 2 is the limit squared: the search runs.
        assert_eq!(found(&deleted, 2), [(0, 0)]);
        // 3 × 2 is past it.
        let more = [deleted[0], deleted[1], file(7, "x", "s\n")];
        assert_eq!(found(&more, 2), []);
        // An exact match still counts and leaves 4 × 1 open, within the limit. Counted with
        // the matched deleted file, 5 × 1 would pass it.
        let exact =
            [deleted[0], deleted[1], file(8, "w", "r\n"), file(6, "v", "t\n"), file(5, "u", "o\n")];
        assert_eq!(found(&exact, 2), [(2, 1), (0, 0)]);
    }
}
