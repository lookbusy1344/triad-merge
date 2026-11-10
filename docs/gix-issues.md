# gix issues

Known problems in `gix` 0.90 (`gix-index` 0.58) that affect triad, and triad's answer to each.

## The index writer keeps only three extensions

`gix_index::State::write_to` writes the tree cache, the sparse-index marker and the end-of-index entry. It writes no resolve-undo (`REUC`), untracked cache (`UNTR`) or file system monitor (`FSMN`) extension, and offers no hook to add one. A split index is read whole and written whole, without its `link`.

A resolved `resolve` save in git (spec §7.4) rewrites the index through gix, so:

- The resolved path gets no resolve-undo record. `git checkout -m PATH` cannot recreate its conflict, as it can after `git add`.
- Resolve-undo records of other paths, from earlier `git add`s, are dropped.
- git rebuilds the untracked cache and the file system monitor data on its next run.

triad leaves it. Writing `REUC` itself would mean splicing an extension into gix's output and recomputing the end-of-index entry and the checksum.
