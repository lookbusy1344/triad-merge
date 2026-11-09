# merge-core

Text loading, line diffing, three-way alignment, region classification, resolution state, undo and output rendering. No UI and no VCS code.

Public API:

- `text`: `Text::decode` and `Text::encode`, line access, `dominant_ending`, `output_encoding` and `typed_ending` (spec §4.1).
- `diff`: `diff_lines` with three algorithms, four whitespace modes and a line-ending rule, `diff_chars`, `intraline_hunks` and `line_key` (spec §4.2).
- `merge3`: `Inputs`, `align` and the classified `Region`s of a merge with n sides and n − 1 bases (spec §4.3). `Alignment` picks git's or jj's semantics (`Semantics`), the same-change rule (`SameChange`), word-level merging (`HunkLevel`), the diff options and refinement (`Refine`); `MergeOptions::alignment` builds it. `align_merged` also keeps each `WordMerged` region's text; `word_merge` computes one from the inputs on demand.
- `jj`: `LineDiff`, jj's line diff of a merge's inputs as `Hunk`s in line coordinates, under a whitespace mode (`docs/PLAN.md` §B2).
- `suggest`: the resolutions triad offers for a conflict and never applies itself (spec §4.4).
- `document`: `MergeDocument`, a merge session: regions, resolutions, the result text, commands and undo (spec §4.4, §4.5).
- `diff2`: `DiffDocument`, the two-way diff's regions and editable buffers (spec §4.6).
- `edit`: text patches and edit batches, shared by both documents.
- `journal`: the recovery journal, `input_hashes` and restore (spec §6.2).
- `lengths`: region lengths in a Fenwick tree.

The `testing` feature exposes `testing::generate`, a seeded generator for large triples.
