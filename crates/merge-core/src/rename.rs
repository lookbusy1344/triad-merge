//! Rename detection between deleted and added files, by content, as jj's git backend finds
//! renames (spec §6.1).

use std::cmp::Ordering;

/// A deleted or added file. It holds no bytes: [`renames`] loads them only for a similarity
/// pass, and only for files whose sizes allow a match.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a, K> {
    /// The file's identity, which orders the search: git's blob ID in triad.
    pub id: K,
    /// The path, its components joined by `/`.
    pub location: &'a [u8],
    /// The length of its bytes.
    pub len: usize,
    /// It holds a NUL byte within git's sniff length.
    pub binary: bool,
}

impl<'a, K> Candidate<'a, K> {
    /// The candidate for a file of `bytes`.
    #[must_use]
    pub fn of(id: K, location: &'a [u8], bytes: &[u8]) -> Self {
        Self { id, location, len: bytes.len(), binary: is_binary(bytes) }
    }
}

/// Which slice given to [`renames`] a file to load is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Deleted,
    Added,
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

/// The renames among `deleted` and `added`. `load` reads the bytes of a file, by its group
/// and its index there. Each file loads at most once, and a deleted file's bytes stay only
/// until it pairs or the search ends.
///
/// # Errors
///
/// The first error `load` returns.
pub fn renames<K: Ord, E>(
    deleted: &[Candidate<'_, K>],
    added: &[Candidate<'_, K>],
    load: impl FnMut(Group, usize) -> Result<Vec<u8>, E>,
) -> Result<Vec<Rename>, E> {
    renames_within(deleted, added, LIMIT, load)
}

/// [`renames`] under the limit `limit`.
fn renames_within<K: Ord, E>(
    deleted: &[Candidate<'_, K>],
    added: &[Candidate<'_, K>],
    limit: usize,
    mut load: impl FnMut(Group, usize) -> Result<Vec<u8>, E>,
) -> Result<Vec<Rename>, E> {
    let sources = by_id_and_location(deleted);
    let destinations = by_id_and_location(added);
    let mut taken = vec![false; deleted.len()];
    let mut matched = vec![false; added.len()];
    let mut found = Vec::new();
    for &dest in &destinations {
        let file = &added[dest];
        if file.len == 0 {
            continue;
        }
        let first = sources.partition_point(|&s| deleted[s].id < file.id);
        let same_id = sources[first..]
            .iter()
            .copied()
            .take_while(|&s| deleted[s].id == file.id)
            .filter(|&s| !taken[s]);
        if let Some(source) = preferring_name(same_id, deleted, file) {
            taken[source] = true;
            matched[dest] = true;
            found.push(Rename { deleted: source, added: dest });
        }
    }
    let open_sources = taken.iter().filter(|&&t| !t).count();
    let open_destinations = matched.iter().filter(|&&m| !m).count();
    if exceeds(open_sources, open_destinations, limit) {
        return Ok(found);
    }
    let mut loaded: Vec<Option<Vec<u8>>> = vec![None; deleted.len()];
    for &dest in &destinations {
        let file = &added[dest];
        if matched[dest] || file.len == 0 || file.binary {
            continue;
        }
        let open: Vec<usize> = sources
            .iter()
            .copied()
            .filter(|&s| !taken[s] && !deleted[s].binary && sizes_allow(deleted[s].len, file.len))
            .collect();
        if open.is_empty() {
            continue;
        }
        let new = load(Group::Added, dest)?;
        let mut best: Option<(usize, f32, bool)> = None;
        for source in open {
            let old = match loaded[source].take() {
                Some(bytes) => bytes,
                None => load(Group::Deleted, source)?,
            };
            let score = similarity(&old, &new);
            loaded[source] = Some(old);
            if score < SIMILARITY {
                continue;
            }
            let same_name = file_name(deleted[source].location) == file_name(file.location);
            let better = best.is_none_or(|(_, best_score, best_same_name)| {
                match score.total_cmp(&best_score) {
                    Ordering::Greater => true,
                    Ordering::Equal => same_name && !best_same_name,
                    Ordering::Less => false,
                }
            });
            if better {
                best = Some((source, score, same_name));
            }
        }
        if let Some((source, ..)) = best {
            taken[source] = true;
            loaded[source] = None;
            found.push(Rename { deleted: source, added: dest });
        }
    }
    Ok(found)
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

/// Whether `bytes` hold a NUL byte within git's sniff length, as gix tells binary files.
#[must_use]
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(crate::text::BINARY_SNIFF_BYTES).any(|&byte| byte == 0)
}

/// Whether files of `old` and `new` bytes can reach [`SIMILARITY`]. The bytes kept are at
/// most the smaller length, so [`similarity`] is at most this ratio, in the same arithmetic.
#[expect(
    clippy::cast_precision_loss,
    reason = "the same f32 arithmetic as `similarity`, so the bound holds as it rounds"
)]
fn sizes_allow(old: usize, new: usize) -> bool {
    old.min(new) as f32 / old.max(new) as f32 >= SIMILARITY
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
    use std::cell::RefCell;

    use super::*;

    /// A test file: its ID, path and bytes.
    #[derive(Clone, Copy)]
    struct File<'a> {
        id: u32,
        location: &'a str,
        bytes: &'a str,
    }

    fn file<'a>(id: u32, location: &'a str, bytes: &'a str) -> File<'a> {
        File { id, location, bytes }
    }

    fn candidates<'a>(files: &[File<'a>]) -> Vec<Candidate<'a, u32>> {
        files
            .iter()
            .map(|f| Candidate::of(f.id, f.location.as_bytes(), f.bytes.as_bytes()))
            .collect()
    }

    /// Renames as `(deleted, added)` pairs, and loads as `(group, index)`.
    type Run = (Vec<(usize, usize)>, Vec<(Group, usize)>);

    /// The renames under `limit`, as sorted pairs, and each load in the order made.
    fn run(deleted: &[File<'_>], added: &[File<'_>], limit: usize) -> Run {
        let loads = RefCell::new(Vec::new());
        let load = |group, ix: usize| -> Result<Vec<u8>, ()> {
            loads.borrow_mut().push((group, ix));
            let files = match group {
                Group::Deleted => deleted,
                Group::Added => added,
            };
            Ok(files[ix].bytes.as_bytes().to_vec())
        };
        let found = renames_within(&candidates(deleted), &candidates(added), limit, load)
            .expect("the loads succeed");
        let mut pairs: Vec<_> = found.into_iter().map(|r| (r.deleted, r.added)).collect();
        pairs.sort_unstable();
        (pairs, loads.into_inner())
    }

    fn pairs(deleted: &[File<'_>], added: &[File<'_>]) -> Vec<(usize, usize)> {
        run(deleted, added, LIMIT).0
    }

    fn loads(deleted: &[File<'_>], added: &[File<'_>]) -> Vec<(Group, usize)> {
        run(deleted, added, LIMIT).1
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
    fn sizes_at_half_are_compared() {
        // 4 bytes against 8: the sizes allow the threshold, and all 4 stay.
        assert_eq!(pairs(&[file(1, "a", "1\n2\n")], &[file(2, "b", "1\n2\n3\n4\n")]), [(0, 0)]);
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
    fn an_identity_match_skips_other_ids_on_both_sides_of_it() {
        // In ID order the deleted files run 1, 2, 2, 3: the match is in the middle. Binary,
        // so only the identity pass pairs them.
        let deleted = [
            file(3, "c", "\0c\n"),
            file(2, "z/x", "\0x\n"),
            file(1, "a", "\0a\n"),
            file(2, "y/x", "\0x\n"),
        ];
        assert_eq!(pairs(&deleted, &[file(2, "x", "\0x\n")]), [(3, 0)]);
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
        let found = |deleted: &[File<'_>], limit| run(deleted, &added, limit).0;
        // 2 × 2 is the limit squared: the search runs.
        assert_eq!(found(&deleted, 2), [(0, 0)]);
        // 3 × 2 is past it.
        let more = [deleted[0], deleted[1], file(7, "x", "s\n")];
        assert_eq!(found(&more, 2), []);
        // An exact match still counts and leaves 4 × 1 open, within the limit. Counted with
        // the matched deleted file, 5 × 1 would pass it.
        let exact =
            [deleted[0], deleted[1], file(8, "w", "r\n"), file(6, "v", "t\n"), file(5, "u", "o\n")];
        assert_eq!(found(&exact, 2), [(0, 0), (2, 1)]);
    }

    #[test]
    fn past_the_limit_nothing_is_loaded() {
        let deleted = [file(1, "a", "1\n2\n3\n4\n"), file(9, "z", "q\n"), file(7, "x", "s\n")];
        let added = [file(2, "b", "1\n2\n3\n5\n"), file(8, "y", "r\n")];
        assert_eq!(run(&deleted, &added, 2).1, []);
    }

    #[test]
    fn an_exact_match_loads_nothing() {
        assert_eq!(loads(&[file(1, "a", "x\n")], &[file(1, "b", "x\n")]), []);
    }

    #[test]
    fn sizes_too_far_apart_load_nothing() {
        // 2 bytes against 8 can keep at most a quarter.
        assert_eq!(loads(&[file(1, "a", "a\n")], &[file(2, "b", "a\nb\nc\nd\n")]), []);
        // Binary and empty files load nothing either.
        assert_eq!(loads(&[file(1, "a", "\0\n2\n3\n4\n")], &[file(2, "b", "1\n2\n3\n4\n")]), []);
        assert_eq!(loads(&[file(1, "a", "1\n")], &[file(2, "b", "")]), []);
    }

    #[test]
    fn each_file_loads_once_and_a_paired_file_is_not_loaded_again() {
        // Both added files compare with both deleted ones; `s1` pairs with `d1` first.
        let deleted = [file(1, "s1", "1\n2\n3\n4\n"), file(2, "s2", "5\n6\n7\n8\n")];
        let added = [file(3, "d1", "1\n2\n3\n9\n"), file(4, "d2", "5\n6\n7\n9\n")];
        let (pairs, loads) = run(&deleted, &added, LIMIT);
        assert_eq!(pairs, [(0, 0), (1, 1)]);
        assert_eq!(
            loads,
            [(Group::Added, 0), (Group::Deleted, 0), (Group::Deleted, 1), (Group::Added, 1),]
        );
    }

    #[test]
    fn a_failed_load_is_returned() {
        let deleted = candidates(&[file(1, "a", "1\n2\n3\n4\n")]);
        let added = candidates(&[file(2, "b", "1\n2\n3\n5\n")]);
        let found = renames(&deleted, &added, |group, ix| Err((group, ix)));
        assert_eq!(found, Err((Group::Added, 0)));
    }
}
