# triad — design notes

The design of what is built: the reasons, the decisions and the current measurements. [SPEC.md](SPEC.md) sets the behaviour; [PLAN.md](PLAN.md) holds the status and the open work. A reference such as §B5.8 names part B, section 5.8. Section numbers are stable: code, scripts and docs cite them, so a section that goes leaves a gap.

Measurements come from an M4 Pro MacBook Pro (2025), macOS 27.2, 120 Hz display, release build with the `dist` profile's settings, triples from `merge_core::testing::generate` with seed `0x5EED`, unless a section says otherwise.

## A. Engine, view, windows and release

### A1 Rules

AGENTS.md holds the working rules. Three design rules sit here:

- **git and jj parity** (spec §1.2). Shared types follow jj's model: n sides, n − 1 bases, an optional label per term. git fills the same types. A fact only one VCS records is an optional field, never a separate code path in `merge-view` or `merge-app`. A jj-native feature, one git cannot have, sits in `vcs-jj`'s own API outside the trait and reaches the front ends through `merge-session` (part I).
- **jj's results only.** A jj-native feature writes only what jj's own command could write: trees, parents, rebased descendants, conflict format and labels, bookmarks, git `HEAD` and refs, the working copy on disk. triad departs from jj only by doing less: it refuses earlier, skips an operation that changes nothing, or keeps a change in the working copy where jj writes a conflict. Each departure is named in the spec and the user guide, shown to the user where it happens, and marked in the oracle (§I5).
- **Zed first.** Before designing a UI, editor or VCS-integration feature, check how Zed solves it and record what was found and the source. Spec §2.5 sets the licence rule: study the design, never copy GPL code.

### A2 Workspace layout

AGENTS.md § Layout lists the crates; §A13.7 gives their layers. The `triad-merge` crate holds both binaries and depends on `merge-cli` and `merge-app` (spec §3). `merge-cli` is a GUI-free library, so the `--headless` and `inspect` tests never link GPUI.

### A3 Shared dependency choices

| Need | Crate | Version policy |
|---|---|---|
| Line diff | `imara-diff` | 0.2.x |
| Result text model in core | `ropey` | 1.6.x. gpui-base uses ropey 2 beta internally; the two never cross. |
| GUI | `gpui-kit` (re-exports `gpui-component` and `gpui-base`) | `=0.8.1`, exact pin; bump on purpose |
| CLI | `clap` (derive) | 4.x |
| Errors | `thiserror` in libraries | latest |
| Serialisation (journal, `--json`) | `serde`, `serde_json` | 1.x |
| Paths | `camino` | 1.x |
| Atomic write | `tempfile` | 3.x |
| Config dirs | `directories` | latest |
| File watching | `notify` | 8.x, the version gpui-component uses |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | latest |
| Property tests | `proptest` | 1.x, fixed seed |
| Benchmarks | `criterion` | latest |
| Git | `gix` | exact pin, the version jj-lib uses: 0.90 with jj-lib 0.47.1 |
| jj | `jj-lib`, `jj-core` | exact pin; bump with gix |
| Async bridge | `pollster`, `futures` | in the tree through jj-lib; `vcs-jj` runs jj-lib's async APIs to completion |
| Conflicted git fixtures | `gix-merge`, through gix's `merge` feature | dev-dependency of `vcs-git` only. The product never merges trees. |

`script/check-workspace` fails on two copies of gix.

Rejected (owner, 2026-10-27):

- `similar`: overlaps `imara-diff` without histogram or the indent heuristic.
- `difftastic`: a binary with no stable library API, whose syntax-node output does not map to line regions.
- `difflib`: nothing over `imara-diff`.
- `gix-merge` as engine or oracle: it returns merged bytes and a conflict flag, with no regions or n-sided model, and its histogram is `imara-diff`'s, not git's.
- `threeway_merge` as a second oracle: it wraps git's own xdiff under LGPL-2.1, so it is neither independent of git nor allowed by `deny.toml`.

### A4 `merge-core` and the headless CLI

#### A4.2 Text loading (`merge_core::text`)

`Text` holds the decoded content as `Arc<str>` and a line index; a clone shares both. `decode` works on bytes; the caller reads files and reports I/O errors. Lines end at `\n` only; a lone `\r` is a character inside a line, as in git and the result editor. A file with a NUL in the first `BINARY_SNIFF_BYTES` (8000) bytes, git's binary rule, is UTF-16 without a BOM when `utf16_order` finds a byte order there: no `00 00` unit and more line feed units (`0A 00` LE, `00 0A` BE) in one order than in the other; the whole file must then decode in that order, or it is binary. The test works on units, never on which bytes are NUL, since text such as `一` (U+4E00, `00 4E` in LE) puts NULs at both parities. `Encoding` names each UTF with and without a BOM (`Utf16Le`, `Utf16LeBom`), so a save writes back what it read. Errors carry byte offsets from the file start, BOM included.

Text with no BOM that is not UTF-8 decodes as `Encoding::Windows1252`: `WINDOWS_1252_HIGH` holds WHATWG's 32 characters for 0x80–0x9F, the unassigned bytes as C1 controls, and every other byte is the character of its value. The table is triad's own; a test checks all 255 non-NUL bytes against `encoding_rs`, already in the tree through gix-filter, which is a dev-dependency only. Every byte maps to one character and back, so `Text::bytes` re-encodes a decoded text without failing, and line origins compare it with the repository's bytes. `Encoding::encode` returns `EncodeError` for a character the encoding cannot hold, with its line; only Windows-1252 refuses one. `Encoding::holds` checks a text in pieces without encoding it: `save_text` checks the diff window's rope first, then writes, so a refusal writes nothing. `EncodedOutput::new` refuses the same way. It runs in the save's job, which returns `SaveError::Encode`; `ending::saved` turns that into `Next::Refused`, so the merge window shows an alert and keeps its journal going. `MergeDocument`'s markers leave out a label the output encoding cannot hold; `labels()` keeps it for the pane titles.

#### A4.3 Diffing (`merge_core::diff`)

Line diffs run imara's indent heuristic, as git's diff does; the merge's diffs do not (§A4.4). Character diffs use Myers, since histogram does badly on small alphabets, and stop above `MAX_INTRALINE_DIFF_CHARS` (10 000). Whitespace modes rewrite only the interning key (`diff::line_key`): `Trailing` drops trailing spaces, tabs, `\r` and the `\n`; `Amount` also collapses runs; `All` removes all whitespace. Whitespace is space, tab and `\r`, as in git's `isspace`. `CaseRule::Ignore` folds the key last, with `unicase`'s full Unicode case folding, already in the tree through GPUI; a key of ASCII without capitals stays borrowed. `ignore-pattern` cuts first, on the line without its terminator (`merge_core::pattern::Patterns`, compiled once and shared by `Arc`). `Comparison<'p>` borrows the patterns, so it stays `Copy`; `DiffOptions` and `MergeOptions` own them and are `Clone` only, and functions take them by reference. A comparison without filters borrows the static `pattern::NONE`, and a line no pattern matches stays borrowed. An exact comparison (`Comparison::is_exact`: the default rules, no patterns) returns the line itself before any rule runs: the chain of rules costs `align` 7% at 1M lines. `BlankLineRule` is no key rule: `diff2::regions` runs the diff as usual and marks a hunk `Kind2::Ignored` when `diff::is_blank_line` holds for every line on both sides, as git's `xdl_mark_ignorable` marks change groups. Blank lines that keyed equal would merge into `Unchanged` regions with unequal line counts on the two sides; a region kind of its own keeps `Unchanged` regions equal in length, which folds and scroll sync rely on, and gives the view filler rows for free. The field sits in `DiffOptions` beside the other filters; only `diff2` reads it, and `Settings::two_way` alone sets it, so merges never see it. An ignored region inside a long unchanged stretch splits that stretch's fold in two.

#### A4.4 Three-way alignment (`merge_core::merge3`)

N-sided throughout (spec §4.4, §7.4): `Inputs` holds n sides and n − 1 bases, and each `Region` one range per input. For a three-way merge, `OneSide(0)` is spec's `LeftOnly`, `OneSide(1)` `RightOnly`, `AllSame` `BothSame`.

Git semantics:

1. Diff base 0 against every other input, placing hunks as `git merge-file` does (spec §4.2): `diff_lines_for_merge`, with git's histogram in `histogram.rs`.
2. Merge hunks that overlap or touch in base 0 coordinates, and track each input's line delta to place its ranges.
3. Classify each cluster with jj-core's `trivial_merge` on the lines' comparison keys: `AllSame` when every side holds the same text, else `OneSide(i)`, else `Conflict`.
4. Refinement (on by default) is `git merge-file --zdiff3`: lines all sides share at the start and end of a conflict split off into `AllSame` regions with empty base ranges. The conflict keeps its whole base ranges, shared middle lines stay in it, and conflicts never join.

git's histogram matches `git diff --histogram --no-indent-heuristic`: anchor choice without prefix stripping, skipping lines commoner than the best anchor, the 64-occurrence limit and its Myers fallback, sliding, and the re-diff of a moved group. Part B gives jj semantics.

Moved blocks (`merge3::side_moves`, `PLAN.md` §G16) come from the aligned regions, three-way only. For each side, every region that is not `Unchanged` gives one change of base against that side: its base and side ranges, less the lines at either end that compare equal, since a git region joins hunks of both sides that touch. `diff2::moves` matches those changes as in the two-way diff (§A4.7), so the two windows agree on what a move is. A move names input lines, never regions: `MergeDocument` finds the moves once at load, and edits that join the result's regions leave them.

Duplicates (`merge3::duplicates`) are blocks both sides insert at different places, each in a `OneSide` region, so a clean merge takes the block twice. Each side's changes in those regions, trimmed as above, are matched against the other side's by `diff2::moves`, once in each direction, so a block inside the other side's larger insert is found too. A conflict is left out: the user resolves it. `MergeDocument` finds them at load with the moves. `suggest::duplicate_copy` finds the copy a region holds while both stand, and `EditSuggestion::DropDuplicate` deletes its result lines, which may be part of a larger insert, so the rest of the region stays. On an M4 Pro both searches together cost about 1 ms of `load-jj`'s 41 ms at 100k lines and 16 ms of its 511 ms at 1M, 2–3%; under git semantics, 0.4 ms of 27 ms and less than the noise of 473 ms; `held_duplicates` maps both copies to result lines while each copy's region takes its side's text, automatically or picked alone. The result pane shows no move: its bands and marks say which input each line came from.

#### A4.5 Resolution, result text and output (`merge_core::document`)

`MergeDocument` owns everything a session changes: the inputs, the options, the regions and their resolutions, the result as a `ropey` rope, and the undo history. The core is the single source of truth. Every command, undo included, returns the minimal patches the GUI applies to its editor; the GUI never replaces the whole text. `Applied::regions_changed` tells the view to rebuild its region-indexed state.

Region offsets live in a Fenwick tree of region byte lengths, and a second one of region newline counts (`region_lines`), so an edit updates one entry and a lookup is a prefix search, both O(log n) in the regions. `MergeDocument::new` renders each region as a `Cow<str>` into a `RopeBuilder`; unchanged, one-side and single-pick regions borrow their input.

`History` holds one entry per command: the inverse patches and, when the command merged regions, the regions, resolutions and lengths it replaced. Consecutive single-patch insertions at the caret without `\n` merge into one entry until another command runs, as gpui-base's undo manager groups them. A multi-cursor batch is one entry.

#### A4.6 Edit model (`merge_core::document`)

`Command::Edit(EditBatch)` carries one user action. The rules are spec §4.5's. One rule needs its reason: a zero-width insertion after a byte other than `\n` joins the region holding that byte, and one at a line start goes to an empty region at that offset if there is one; without the first clause, typing `ab` into an empty region would put `b` in the next region. A typed newline takes `typed_ending`; since the editor has already made the edit, `apply` returns only the corrections, which the view applies inside its reentrancy guard.

#### A4.7 Two-way diff (`merge_core::diff2`)

`DiffDocument` holds two buffers, each a rope with its own history and read-only flag, and the regions with a version. `recompute` takes O(1) rope clones for a background diff; `install` takes the result only when its version still matches, which gives spec §4.6's debounce. `copy_region` recomputes first when the regions lag the text.

The moves (spec §4.6) run with the regions in each recompute and install with them as one `Diffed`, so they never name regions of another version. `diff2::moves` keys every right line of a `Changed` region once, by `line_key`, into a map from key to its places. Each block checks only the places of its line with the fewest, shifted back by that line's offset and kept inside the region; a block that matches twice stops at the second. A block of lines that each repeat many times among the inserted lines still checks every place of the rarest. Anchoring on the first line instead is quadratic where many blocks start with `}` or a blank line: 4.3 s at 100k lines. The benchmark `diff-load-braced` (`testing::generate_braced_changes`) builds that case: every block starts with `}`. On an M4 Pro the search costs 1.2 ms at 100k lines and 19 ms at 1M, of a load of 29 ms and 494 ms under Myers; on the generated pair, 0.1 ms and 1.2 ms of 9 ms and 181 ms. Histogram takes 2.1 s to diff the braced pair at 100k lines, Myers 29 ms: imara's histogram slows on a line repeated 20,000 times on each side, as git's does. Places that overlap another block's, found by a sweep in order of start with the largest end so far, drop out. `suggest::carried_edit` keeps its own search: it compares bytes, inside one-side regions of a merge, and needs no map for one conflict.

The intra-line marks (spec §5) also run in each recompute and install with the regions in `Diffed`. `diff2::marks` pairs each changed region's lines outside the move ends in order, left with right, and gives each side the byte ranges `diff::intraline` finds, terminators left out. `diff::intraline` drops a pair under the `Similarity` floor: kept characters other than whitespace against those of the longer line, in integer thousandths, so `DiffOptions` stays `Eq`. Whitespace stays out of the count because indentation alone made unrelated lines look half alike. At 0.5 the floor clears pairs of unrelated code, which keep 6–50% of their characters, and keeps renames and edited arguments, which keep 58–95%. The floor lives in `DiffOptions` so it reaches both windows, `MergeDocument::options` and the journal without new plumbing; no comparison reads it. They cost about 1.3 µs per pair on an M4 Pro: 130 ms when all 100k lines differ, 1.3 s at 1M, against a diff of 20 ms and 480 ms. That breaks spec §9's load target, so `DiffDocument::new` leaves the marks empty and the window computes them off the UI thread from `DiffDocument::marking`; `install_marks` takes them only for the current version. `copy_region` while the regions lag runs a recompute, marks included, on the UI thread.

`merge_core::edit` holds what both documents share: `TextPatch`, `EditBatch`, batch checks, inverse patches and line-ending fixes.

`merge_core::patch::unified` writes a two-way diff's regions as a unified diff, in `git diff`'s format: hunk headers counted from 1, with a count of 1 left out and an empty range naming the line before it; `\ No newline at end of file` after a line without a terminator; and git's default function name after the header, the nearest left line above the hunk that starts with an ASCII letter, `_` or `$`, cut to 80 bytes and trimmed of trailing whitespace. git cuts at byte 80; triad cuts at the character boundary at or before it. Two changes share a hunk when at most twice the context lies between them. Context comes from unchanged regions alone and prints from the right side, as `git diff -w` prints it. A change the comparison ignores starts no hunk; inside one it prints as a change, and context stops at it. `Scope::Region` gives one changed region with its context. The diff window copies a patch from `DiffDocument::patching`, a snapshot like `marking`'s: O(1) rope clones and the current regions, `None` while they lag the text, so a region index always names a region of the text the patch shows. The patch builds off the UI thread; a copy asked while the regions lag waits with the navigations for the recompute.

#### A4.8 Oracle corpus

`fixtures/oracle/corpus.jsonl` holds about 2,000 triples from `script/gen-oracle`: hand seeds, seeded mutations, and the first 200 lines of triad's own files at pinned commits. It needs git 2.54 or later and records `git merge-file -p --diff-algorithm=histogram` under `--diff3` and `--zdiff3`, with the marker size computed by spec §4.4's rule and passed as `--marker-size`, since git keeps the requested size. The exit code is the conflict count.

The test checks every case in both modes: a clean merge matches byte for byte; a conflicted one matches the conflict count and the text outside the markers.

`fixtures/oracle/patch-corpus.jsonl`, from `script/gen-patch-oracle`, holds `git diff --no-index --histogram --indent-heuristic` of two pairs of each triple, base against left and left against right, at context 0, 1 and 3, without the user's or the system's git config: 11,730 patches, hunks only. The two-way diff picks other hunks than git's histogram on 58 of the 3,910 pairs (spec §4.4); the test holds that count. On every other pair `unified` gives git's patch byte for byte at all three sizes.

libgit2's file-merge tests (`merge/files.c`, `merge/trees/whitespace.c`, `merge/trees/automerge.c`) give a case list, not data. libgit2 is GPL-2.0 and its linking exception does not cover its tests, so each scenario is a hand seed written from scratch, `seed-libgit2-<scenario>`, with git's output as the expected value. The jj corpus is §B5.2.

#### A4.9 Properties

Proptest suites for spec §8.2, with a fixed seed and checked-in regressions: alignment in `merge3`; output, two-way and edit sequences in `merge-core/tests/properties.rs`. Most use files of up to 12 lines from a few line shapes, so lines repeat and diffs tie. One suite runs the output and marker properties on triples of up to 3,000 lines: a base of common shapes and two sides that each edit it in up to 60 places. Only an edit inserts the marker-like line and the 8 KB line, so one input can hold them while the others do not. Fuzzing waits for release 1 (spec §8.9): `cargo-fuzz` needs nightly.

#### A4.10 Headless CLI

`--headless`, `inspect --json` and `diff --inspect --json` (schema in `docs/json-schema.md`). Every report and error passes through `merge_core::unusual::escaped` (§A6.8). Logging goes to stderr and a rotating file in the cache directory, filtered by `TRIAD_LOG`; `TRIAD_LOG_DIR` moves it, which the binary tests use. Each `fixtures/core/<case>/` holds `base`, `left`, `right`, the `expected` bytes, the `exit` code, and optionally `args` and `config`.

#### A4.11 Atomic write

`merge_session::io` creates a temporary file in the target's directory, gives it the target's permissions when the target exists, before any byte goes in, so the new contents are never more readable than the old, then writes, syncs, renames over the target, then on Unix syncs the directory. A new file gets the mode `std::fs::write` gives (`0o666` less the umask), not `tempfile`'s `0o600`. A failed directory sync is logged: the file already holds its bytes.

#### A4.12 Benchmarks

`cargo bench -p merge-core --bench merge`, on 10k, 100k and 1M lines:

- `decode`, `align`, `output`: no target; they show where `load` spends its time.
- `load`: decode three inputs and build the `MergeDocument`. Spec §9 load. `load-jj` and `align-jj` are jj mode (§B6).
- `load-word-small`, `load-word-large`: jj mode at the word level (§B6).
- `toggle`, `keystroke`: spec §9 take left/right and keystroke.
- `diff-load`, `diff-load-braced`: build a `DiffDocument`, regions and moves, held to spec §9 load. `-moves` alone is the move search (§A4.7).

`script/check-bench` fails when a mean exceeds 120% of its spec §9 target. The `bench` workflow runs it weekly on `ubuntu-latest` with `TRIAD_BENCH_TOLERANCE_PERCENT=500`: that runner measured `load/1M` at 834–1401 ms over three runs of unchanged code, and the rayon-parallel word and bounded benches lose more on its 4 vCPUs, up to 3.1 times their target. The job catches gross regressions; the reference machines decide the targets. Memory peaks do not vary and keep their limits.

#### A4.13 Windows console binary

The console binary (spec §6.1) starts `triad-merge-gui.exe`, waits, and forwards its exit code: `merge_cli::forward` runs the child with inherited stdio; any other code, a signal or a failed start gives 3. The tests' child is the `forward_stand_in` example, which names its arguments on stderr and exits with `TRIAD_STAND_IN_EXIT`. The Windows CI targets check the real pair.

### A5 `merge-view`

A plain-Rust view model with no GPUI. Every lookup is by index or offset, in O(log n). Nothing builds a list of all rows: at 1M lines that is tens of MB, rebuilt after every merging edit.

- **Rows** (`rows::RowMap`): a Fenwick tree of rows per region over the visible `PaneSet`. Filler rows sit at the end of a region; a region every visible pane leaves empty has no rows. `update` rebuilds only when `Applied::regions_changed`.
- **Folds** (`merge_fold`): an unchanged region whose result holds the automatic text hides its aligned rows and result lines past `context-lines` at each end facing another region; both counts match, so the panes hide alike. `MergeOpened` keeps an opened region by its first side's lines, which never change, so it survives the renumbering of edits and picks. `Heights::fold` gives a fold's items one display row, its first item's, in O(folds) memory without wrap.
- **Scroll mapping** (`scroll`): rows ↔ result lines through `RowMap::first_row` and `MergeDocument::region_bytes`. A position maps to the same region at the same line within it, held to the region's last row or line, and keeps its fraction.
- **Navigation** (`nav`): next, previous, first and last `Difference` or `Unresolved`, with no wrap-around. A difference is a region not `Unchanged`, so whitespace-only changes are skipped (spec §4.2). `nav::held_region` and `diff::held_region` are the one place a stored pin and caret become a region; each returns a region of the document whatever the inputs.
- **Highlights** (`highlight`): intra-line spans on demand, cached by region in an LRU of `INTRALINE_CACHE_REGIONS` (256), emptied when regions change. The diff window takes its marks from the core instead (§A4.7).
- **Status** (`status`): counts, labels, encoding, line ending and whitespace mode; counting visits every region once per command.

### A6 GUI (macOS first)

#### A6.1 Vendored editor

`vendor/gpui-base` is gpui-base 0.8.1 with triad's patch, through `[patch.crates-io]`, with Apache-2.0 notices; `PATCHES.md` lists every change. The main ones: full-line fills and gutter marks, the edit observer (§F3), spacer rows and their click event, hidden lines (§A6.5), line labels (§C5.4), line numbers by a function, one-pass line layout, a resting caret, and tab stops. The vendored crate sits outside the workspace; `script/checks` runs its unit tests.

Zed paints row highlights and diff hunk backgrounds as quads across the text area and hunk status as a gutter strip. triad does the same and adds a letter, since colour is never the only signal (spec §6.5).

gpui-component's `Input::render` copies the whole text each frame, but only with an accessibility client or `test-support`; it is not measured (§A6.7).

#### A6.2 Window and panes

- `MergeWindow` holds the session (`merge_session::session::Session`), the panes and the components below, and wires them to GPUI. Logic with its own state sits in components with unit tests: `PaneScroll`, `OriginColumn`, `input_find::Found`, `go_to_line::Field`, `Measurement`, `JournalTasks`. What stays in `window.rs` is action handlers, task spawns, editor updates and rendering.
- Top panes: a `uniform_list` reading rows from `RowMap`, with a hit target per region.
- Result pane: `EditorState` with folding and indent guides off, line numbers on, multi-cursor on. Soft wrap only up to `SOFT_WRAP_MAX_LINES` (10 000) lines.
- Edits: the edit observer (§F3) receives the editor's `InputEdit`s, and the recorder groups one action's edits into an `EditBatch`. The editor applies a multi-cursor action from the highest offset down, so each edit's range is in the text before the action. When several edits wait, they go as one patch, the difference between the core's text and the editor's, since the core's corrections to the first would shift the rest. `patches::untouched` gives the bytes at each end that no waiting edit touched, and `batched_difference` compares both ropes in place from there through `patches::ByteText`; only the patch's new text is copied. Core patches go back through `replace_text_in_range` in UTF-16 offsets, converted through the editor's rope, inside a reentrancy guard.
- Key contexts: the editor's own bindings sit in a deeper context and win over the window's, so `merge_app::init` binds each window key twice, for the window and for the result editor. Undo and redo bypass the editor's own undo. Until something has the focus no key reaches the window, so the window focuses the result when it opens. gpui-component's `Button` calls `prevent_default` on mouse down, so a toolbar click keeps the focus.
- The window pins the region that navigation or a pick made current, with the caret it left: an empty region holds no byte, so the caret alone cannot name it.
- Toolbar: `toolbar::Labels::for_width` drops every label below `LABELS_MIN_WIDTH` (1080 px at the default toolbar text, more at a larger `toolbar-font-size`); unlabelled, it fits at `COMPACT_MIN_WIDTH` (816 px). `toolbar::Badge` picks the reference badge the threshold makes room for: `Sides` in a merge of more than two sides, whose badge names the sides a conflict keeps.
- **Collapsed unchanged regions** (spec §5). `MergeWindow::show_folds` takes the folds `merge_fold::folds` gives (§A5) after every command, base pane toggle and collapse change. The result hides each fold's lines with `set_hidden_lines` and shows one spacer row with its note at the line after them; a placeholder above hidden lines moves to that line, and the rows of one line go in region order. `PaneScroll` composes the wrap heights with the folds, so the top panes' list draws one fold row per fold; `result_heights` folds the result's rows the same way, so scroll sync, reveals and ruler clicks count display rows. A change of folds keeps the top panes on their aligned row and makes the result follow (`ScrollSync::bring_result`). `want_syntax` skips hidden rows, so a fold never queues their parse. The result's current search match opens its fold once, as in the diff window.
- Moved blocks (`PLAN.md` §G16): `merge_view::moves::move_ends` places each side's moves in the row table, keyed by pane and row, since moves name input lines and the window has up to three panes. Each move has an origin and a destination in its side's pane and, while the base pane shows, a third end on its base lines there. The window keeps the ends and finds them again whenever the rows change: a command that joins regions, a restored session, the base pane shown or hidden. A cell on a side's end's rows takes `MOVE_TONE` for its band (`MoveEnd::tinted`); base's lines keep their region's band, so base shows only the pills; the end's first row holds the pill, `move_pill::move_pill`, which the diff window also uses, in a run placed absolutely against the cell's right edge. A base row can start one end of each side's move, so a row holds a run of pills, and a pill's id names the other end's input. Its press stops there, so the cell starts no selection and makes no other region current.
- Go to line: a card over the top of the result, after Zed's modal, mapped through `scroll::row_of_line`. It floats in the result's box, so opening it moves no text. Zed's modal takes `row:column`; triad takes a line.
- Open dialog: Merge and Diff tabs after Zed's modals (small text, a tab strip, icon-only pickers), on gpui-base's headless `Tabs`, which carry tab roles. Start validates the slots on the UI thread and hands a `Load` to the background executor, which reads, decodes and merges the inputs or reads the diff's files and finds their repository; the dialog opens the window when it lands. The button shows its loading state meanwhile and takes no clicks. The dialog keeps one line for its error between the slots and the start button (`docs/STYLE.md` § Errors).
- gpui-kit's `test-support` pins `cocoa` 0.26.0 and `unicode-properties` 0.1.3 in the lockfile through weak dependency features (rust-lang/cargo#10801).

#### A6.3 Scroll sync for every source

A zero-size canvas between the rows and the result runs the sync in its prepaint, every frame. It compares both scroll tops with the last sync and copies from the pane that moved, so the pane scrolled last leads and a synced pane does not echo back. The canvas sits before the result in prepaint order because the editor applies a deferred scroll offset in its prepaint and clears it in paint. `merge_view::scroll::ScrollSync` holds the rule; `pane_scroll::PaneScroll` holds the rows' handle, sideways scroll, wrap and both syncs.

The map from one pane's top to the other's (`rows_to_result`, `result_to_rows`, `Heights::keep_top`) runs in f64 and rounds to f32 pixels once, at its end. A line height such as 15 px has no exact f32 reciprocal, so an f32 chain lands a ulp off and the tests' exact tops would hold only for some window heights.

Zed's split diff gives both editors the same rows through spacer blocks. triad's rows and result differ in line count, so it maps by region.

**The diff window.** Both panes are editors, and a canvas between them runs the same sync after the left pane's prepaint. An editor applies a scroll offset only as it lays out and reads back its old one till then, so a second `set_scroll_offset` in one frame undoes the first's other axis. Each sync, reveal, reload and ruler move therefore records a `ScrollTarget` per pane, an optional left and top, and each pane takes its target in one call: the right after the sync, the left before it next lays out.

**Sideways.** The top panes' text sits in a clipping cell per pane and shifts by one shared offset (`sideways::Sideways`). The list keeps vertical wheel moves (`restrict_scroll_to_axis`); a wheel listener on the rows takes the sideways part when it is larger. The offset runs to the widest line a shown pane holds, counted off the UI thread with tabs expanded, times the advance of `m`, less the text width an invisible canvas records each frame. A second `ScrollSync` with identity maps keeps rows and result level sideways.

**Wrapped top panes.** The list holds one item per display row. `row_wrap::RowWrap` gives each aligned row the display rows of its tallest cell, wrapping each cell with GPUI's `LineWrapper` in prepaint at the recorded width. `merge_view::heights::Heights` maps items to display rows and back. Scroll sync, reveal, Go to Line and search steps work in aligned rows and convert through the heights; the result's come from the editor's display map. When heights change, the rows keep their top aligned row.

#### A6.4 Save, cancel and the journal

Spec §6.2 sets the behaviour. A window ends through the `Finish` global: `merge_app::run` exits the process with the code, since GPUI's quit on macOS exits with 0; tests record the code. Prompts are platform prompts, which tests answer with `simulate_prompt_answer`.

The journal (`merge_session::recovery`) holds the format version, SHA-256 of each input's decoded text, the `MergeOptions`, SHA-256 of the starting alignment and the command log, at `<cache>/journal/<key>.json`. Restore aligns again and refuses a journal whose alignment hash differs, since commands name regions and byte offsets. `JOURNAL_DEBOUNCE` (500 ms) restarts with each command; tests drive it with the executor's fake clock. `JournalTasks` holds the waiting write and the running write or removal, and cancels them before a flush, a discard or a restore (§A13.4).

`merge_session::ending` decides Save, Save & Exit, Close and the answers to their alerts without GPUI: each step gives a `Next`, and `Journal` says whether the journal is removed, written or left. A save carries an `After` (`Exit` or `Stay`) from the key through `SaveJob`, `Saved` and a failure's Try Again, so one write path serves both. A `Stay` save ends in `Next::Saved`, or `Next::SavedUnrecorded` when the repository refused the record, which the window shows as a status note, not an alert. `Session` counts applied commands and restores; Close after a Save compares the count with the one the Save took, and before any Save asks `MergeDocument::changed`. Close and Discard after a Save end with its code, since OUTPUT holds what it wrote and git and jj read the code. `Session::apply` and `Session::restore` refuse while a save runs, and the result is read-only until it ends. `Session::start_save` moves the `Recovery` into the `SaveJob`, with a `PendingOutput`: `MergeDocument::snapshot`, the document without its undo history, 0.9 ms at 1M lines. The job writes the journal, then builds, encodes and writes the output on one background thread; building costs 16 ms at 1M lines, 47 ms with line endings made. `Session::saved` takes the `Recovery` back. Save Copy As and the preview of a save into a commit take a `PendingOutput` the same way. The UI thread neither syncs nor hashes the inputs for a save: with the inputs still hashing, the job hashes them, 100 ms per input at 1M lines.

#### A6.5 Two-way diff window

- Two editor panes over `DiffDocument`, edits through the core as in the merge window, per-side undo, a background recompute after 100 ms (`DIFF_DEBOUNCE_MS`). One diff runs at a time: an edit while one runs leaves it, and a diff that lands for an older text starts the next, after the debounce unless a navigation waits. Close and Discard finish only once the save running has landed; a save that fails keeps the window open. gpui-component's `Editor` sets the read-only flag on each render, so read-only goes on the element.
- File watching: `file_watch::watch` runs `notify` on each file's directory and reports each changed file once per 100 ms settle, by its index; the diff window passes both sides to `DiffWindow::disk_changed`, and the merge window passes OUTPUT to `MergeWindow::output_disk_changed`. The watcher has no headless test, since its events come on another thread. `OwnWrites` holds each side's writes from their start, so a change the watcher reports before the save's result arrives is still the window's own.
- Reload keeps place: the read off the UI thread also builds a `merge_core::line_map::LineMap`, an exact line diff of the pane's text as the read began against the file's, skipped when they match. `LineMap::offset` moves the caret; `merge_view::scroll::TopAnchor` holds the view's top as a line and a pixel distance below that line's first display row, so spacer rows above the line keep their share of the view. `apply_held_top` sets both panes' top before the left pane lays out, as a jump's reveal does, then once more after the recompute that follows the reload, when the spacers change. An edit or a jump drops it.
- OUTPUT watch (spec §6.2): `merge_session::stamp::Stamp` is a file's length and 64-bit SipHash, or its absence. `OutputWatch` stamps OUTPUT as the window opens and after a save that leaves the session open, and checks each reported change against the stamp, off the UI thread. A generation counter, raised by each stamp and each save start, drops a read that began before either. The banner shows while the check differs from the stamp. `OutputWatch::ask` is true for the first change of the session only; while its question is open, `asking_output` makes the result read-only on render and `run` and `edit_regions` drop commands.
- **Alignment: spacer rows, as Zed does.** Zed's split diff keeps both sides editable and aligns them with spacer rows from its block map. The vendored editor has a `SpacerMap` layer between `FoldMap` and `DisplayMap` (`EditorState::set_spacers`): sorted runs of inserted rows, mapped by binary search, painted hatched, skipped by cursor movement, selection and hit testing. A spacer row is a display row, and line painting places each line at its own display row. `merge_view::diff::spacers` gives each changed region's shorter side the difference in rows. Between a keystroke and the recompute, spacers stay on their line numbers. `EditorState::set_hidden_lines` hides whole lines through `FoldMap`, apart from tree-sitter's folds and with folding off; a hidden line's spacers drop out, and a left press on a spacer row emits `InputEvent::SpacerClick` with the line below it. These carry the fold rows of §G2.
- **Collapsed unchanged regions** (spec §5). `merge_view::fold::folds` gives the lines each side hides from the regions, the context and the opened regions. The window hands each editor its folds' lines with `set_hidden_lines`; `fold_spacers` moves filler rows off hidden lines and adds one spacer row per fold, which a spacer note names. While the regions lag the text, the folds stay as they are, as the spacers do. `Opened` keeps an opened region by its lines on each side. Each edit moves them by its `LineEdit`: the lines that differ once what the old and new text share at either end is trimmed, since a replace-all sends the whole text as one patch. A recompute keeps the opened regions that share lines with an unchanged region. Fold rows are display rows, so scroll sync, the copy buttons, a jump's reveal and a ruler click read rows from the editors' display maps. The editor scrolls a search match out of a fold once it lays out.
- Spacers give both panes the same rows, so they share one scroll top. The left pane, laid out first, takes a move a frame late.
- Moved blocks: `merge_view::diff::pieces` cuts each move's end out of its changed region and gives it `MOVE_TONE`, `Tone::Moved`, which takes the `both` colour setting with its band at half the band opacity (`tones::MOVED_BAND_SHARE`), so moved text reads fainter than a side's change and paints no fill over another. `diff::bands` and the ruler's `diff_marks` both read the pieces. Each pane reserves the gutter mark column at all times, so a move found by a recompute does not shift the text; the band carries `MOVE_MARK`. The vendored editor emits `InputEvent::GutterMarkClick { line }` on a press in that column, and `move_target` maps the line to the other end's region and the byte of the block's first line there, none while the regions lag the text. `move_ends` gives each end with the other end's lines; the window floats a pill for each end whose first row lies wholly in view, anchored top-right to the pane, in an `anchored` layer so it paints over the editor text. Its colours are fixed in both themes, `MOVE_PILL_FILL` and `MOVE_PILL_TEXT`, not the move tone, so the pill stands apart from the teal band beneath it. A press on the pill takes the path of a press on the `M`. The right end can sit below its region's start, among new lines, so the jump puts the caret on the block's first line and pins the region with that caret.
- Copy buttons: a 28 px column between the panes holds → and ← on each changed region's first row. None show while regions lag the text, since a recompute can renumber regions. A copy is an ordinary edit, so undo on that side reverses it.
- Intra-line marks: each pane fills the core's marks for its side in its tone at `INTRALINE_ALPHA`, the top panes' opacity, in a range decoration made after the bands and before the search fills, so a search fill lies over them. The editor shifts them through edits; the next installed recompute replaces them.
- A jump or ruler click blinks a 2 px frame round the region (`merge_app::blink`). The vendored editor paints fills over the active line and borders last.
- `jj diff --tool` with the working copy on the right opens the working-copy file and makes the left copy read-only (`merge_session::diff_tool::read_pair`, spec §7.1); otherwise a path under `jj-diff-<random>/` gets a banner saying edits will be lost.
- A save writes each modified side through `merge_session::diff_tool::save`, which snapshots the jj working copy when a write succeeded; the window only takes the results.

#### A6.6 Accessibility and settings

Roles and labels follow spec §6.5. GPUI builds an AccessKit tree from each element's role and label. The test platform has no AccessKit adapter, so tests read what gpui-kit records for `test_support()` elements (`tests/a11y.rs`). AccessKit's macOS adapter reports every toggled state but off as 1, so the targets of an unresolved conflict report off and their labels end ", unresolved". Gaps are in `docs/gpui-kit-issues.md`: no live announcements, bands and gutter marks not exposed, only on-screen rows visible. `PLAN.md` part J closes them through `a11y_synthetic_children`, which hands triad the raw AccessKit node, and `on_a11y_action`.

A change to the alignment options re-aligns the open merge while every region keeps its automatic resolution (`MergeWindow::set_options`); otherwise the status bar says it applies from the next session.

#### A6.7 Performance

`merge_app::measure`: `TRIAD_MEASURE=open|scroll-rows|scroll-result|type|pick|syntax` makes the window act once a frame for 300 frames, reports frame intervals and each action's time on stderr, and exits with 2 without writing. `script/measure-app` runs every mode at 100k and 1M lines and fails on a missed spec §9 target. A window in the background gets 41.7 ms frames and keystrokes three to four times slower, from macOS; keep it frontmost. `script/ax-tree.swift` prints and presses the accessibility tree; GPUI builds it on the first query, so that query can return an empty window.

`script/measure-app 120`, 2026-11-24:

| Lines | Load and merge | First frame | Scroll p95, rows / result | Keystroke work, mean / max | Pick work, mean / max | Peak RSS |
|---|---|---|---|---|---|---|
| 100k | 48–52 ms | 196–257 ms | 8.4 / 8.4 ms | 0.3 / 1.1 ms | 0.5 / 1.0 ms | 211 MB |
| 1M | 537–547 ms | 743–763 ms | 8.4 / 8.4 ms | 2.3 / 5.5 ms | 4.9 / 6.7 ms | 873 MB |

`script/check-bench`, 2026-11-24:

| Benchmark | 100k | 1M |
|---|---|---|
| `load` | 26 ms | 456 ms |
| `load-jj` | 45 ms | 535 ms |
| `load-word-small` | 56 ms | 668 ms |
| `load-word-large` | 1.59 s | 16.0 s |
| `load-word-small-rust-bounded` | 50 ms | 542 ms |
| `load-deep-rust-bounded` | 48 ms | 503 ms |
| `toggle`, `keystroke` | 0.8 / 0.8 µs | 0.9 / 0.9 µs |
| Session heap, git / jj | 32 / 56 MB | 363 / 607 MB |

`cargo bench -p merge-cli --bench memory` measures the session's peak heap through a counting allocator (`cap`); `script/check-bench` holds it below spec §9's memory row in git and jj mode. Saving is the peak: `MergeDocument::output` copies the rope into a `String`.

What sets these figures:

- **First frame.** Interval 0 holds the editor's line layout; the vendored editor reads the lines in one pass. Interval 1 (44–53 ms) fills the glyph atlas and builds Metal pipelines, once.
- **Journal hashing** runs once per input on its own thread, so the window shows first.
- **Incremental refresh.** The editor moves its decorations with each edit; `refresh` compares the new bands with the moved ranges (`marks::bands_unchanged`) and rebuilds only when a range or look differs. The keystroke maximum at 1M is the first key into a region, which changes its look. A pick always rebuilds. Bands, placeholders and status counts still scan every region per command.
- **Ruler.** `ruler::painted` gives each whole pixel the style of the last mark on it and joins equal neighbours, so a long file paints no more runs than the strip has pixels. One quad per mark took a third of the main thread at 1M.
- **Alignment** is 381 of load's 456 ms at 1M and grows faster than linear (17 ms at 100k): the first place to look if load nears 1 s.
- **Known whole-text copies**, each a few milliseconds and one file's size at 1M: `Recompute::run` copies both diff texts twice; `MergeDocument::output` copies the rope to save; `Text` keeps an 8-byte line start per line where `u32` would do under 4 GiB.

**Idle memory.** The caret's blink draws every 500 ms, which keeps the Metal driver's reclaimable blocks in use: a bare GPUI window takes 52 MB idle and 185 MB blinking. The vendored caret rests, shown, after 10 s without a key, click or focus. At rest the demo diff of `fixtures/demo/code_a.py` and `code_b.py` takes 98 MB of physical footprint (p4merge: 121 MB): 37 MB of drawables, 33 MB of heap, and the binary. GPUI counts a window losing the front as a blur, which stops the blink. GPUI's per-window path texture and the drawables remain.

**Against VS Code** (README § Performance). `script/compare-vscode` runs the 100k and 1M triples, VS Code's `--merge` given `git merge-file`'s output as its result, a fresh `--user-data-dir` with `--disable-extensions`. It launches the tool, samples the CPU time of its process tree every 100 ms through `ps`, and takes as ready the last sample with more than 20 ms of work before 1.5 s with under 30 ms. Memory is the sum of `footprint`'s `phys_footprint` over the tree at that point; it counts GPU surfaces, so triad's figure exceeds its peak RSS. The first VS Code launch on the fresh profile builds its caches and does not count.

#### A6.8 Unusual characters

`merge_core::unusual` holds spec §5's lists in two sorted range tables and classifies a character by binary search; `escaped` writes the classified characters of terminal output as JSON escapes, so `report` in `merge-cli` and the app's error line need no JSON-aware path.

- **Top panes.** `merge_view::tabs::expand` writes each invisible character as its `CodePoint`, as it writes a tab as spaces, and keeps the map between line and drawn bytes, so wrapping, hit testing, selection and the sideways extent follow without change. `runs::cell` carries the marks as a fifth layer; the window paints them in `UnusualPaint`'s colours.
- **Editors.** The vendored editor takes a `CharMarker` (`EditorState::char_marks`). A badge character is shaped as spaces of its byte length, so bytes and glyph indices stay aligned and no bidirectional control reaches the shaper; its glyphs then become one, as wide as its label in `0`s plus 2 px each side, the way tab stops widen tabs. The wrapper measures with the same steps. A `\r` that ends a line is left to the line ending. The fills paint with the line backgrounds and the labels after the text.
- **Colours.** `gpui_base::input::char_mark_colours` gives both the editors and the top panes the muted text colour and that colour at 25 % opacity, which shows on white, on dark and on the change bands.
- **Not marked.** Lookalike letters need a confusables table and a rule for text in other scripts; spec §5 leaves them out.

#### A6.9 Directory diff

- `merge_session::dir_diff::changed_files` walks each directory into a `BTreeMap` keyed by relative path, so the listing comes out in `Path`'s component order, then joins the two maps. A walk skips every entry named `.git` or `.jj` (`dir_diff::VCS_DIRS`) and reads each other entry's own type: a directory is walked, a regular file kept, and a symlink read with `read_link`. A link at `PATH` whose target is `TREE/PATH`, for an absolute `TREE` holding `.git`, and resolves to a regular file counts as a file: git's dir diff links its right directory into the work tree so. Any other link keeps its target and compares by it, so a link committed to a repository, to a file outside it, never opens and is never written through. A link to a directory is never walked, so a cycle cannot form.
- Files in both directories compare by bytes. Each file is read once and decoded with `Text::decode` to name its content: the comparison settings play no part, so a listed file can show no difference in the diff window under them.
- `dir_diff::pair` tells two directories from two files before either is read.
- `merge_app::dir_window` follows the history window: a list window that opens one diff window at a time, takes over the app's `Finish` so the diff window's close comes back to it, and lists again; a refresh asked while a listing runs marks it `ListJob::Stale`, which drops that listing when it lands and starts another. A file loads off the UI thread through `run::tool_diff` over `diff_tool::read_pair`, the loader of `diff LEFT RIGHT`, so each file gets the same jj working-copy check and line-origin lookup; a side without the file is an empty `Origin::Revision`. The CLI turns `Window::Diff` into `Window::DiffDirs` when `pair` finds two directories, and opens no repository: each file finds its own as it loads.
- `diff` with no file lists through `Repository::changed_paths`, which gives the paths that may differ between a revision and the working copy. git takes gix's status with `head_tree` set to the revision's tree: tree against index, then index against disk, untracked files, submodules and rename tracking off. gix's status pairs a staged rename by default and reports its new path alone. gix drops an entry whose stat alone went stale. jj diffs the revision's tree, the merge of its commits' trees, against the working-copy commit's with `MergedTree::diff`. `native::changed_files` snapshots, resolves the revision to its commits' full IDs with `Repository::commits` and reads through them from then on: `RevSpec::Commit` for one, jj's revset union for several, `RevSpec::Empty` for none, which is git's `HEAD` before the first commit. `Repository::tree` resolves that revision once into a `vcs::Tree` (a gix tree, the index or nothing; a jj `MergedTree`), and the listing reads every candidate, and every deleted file the rename pass loads, from it. `Repository::entry_at` takes a tree per call. At 2000 paths in a dev build, reading through one tree costs 33 ms; `entry_at` per path costs 41 ms against one parent and 930 ms against a merge of two, where each call merges the parents' trees again. A tree entry names a symlink, submodule or directory as such. The listing reads each candidate there and on disk and keeps those whose bytes differ, so both backends list the same set. A path absent on disk, or a directory there, is special when the working copy's own tree holds a submodule at it: git's index, jj's working-copy commit, so a submodule moved, added or removed lists as modified, added or deleted. The candidates are a superset: an executable-bit change or a staged change undone on disk drops out at the byte compare.
- `merge_core::rename::renames` copies the rename pass of gix's `rewrites::Tracker` as jj's git backend sets it (`get_copy_records`: 50 %, limit 1000, empty files untracked, histogram, no text conversion): an identity pass, then a similarity pass, each walking added files in order of ID then location and preferring a source of the same name. `rename::copies` runs the same search as gix's copy pass under `CopySource::FromSetOfModifiedFiles`, after renames, on the added files left: its sources are modified files, by their new blob, and a source stays one after it pairs, so the limit counts every modified file. Both take candidates generic over their ID; `dir_diff::with_rewrites` gives each git's blob ID (`vcs_git::blob_id`), so ties break as jj's do. A modified path carries the blob of its right side when that side is a regular file: gix counts a modification by its new entry, so a link that becomes a file is a source and a file that becomes a link is not. An executable-bit change alone is no source: the listing compares bytes and drops it, where jj's tree diff keeps it as a modification. The score is gix's f32 arithmetic, so a file at the threshold falls the same way. A candidate holds its ID, length and binary flag, not its bytes: `renames` loads a file through a callback only for the similarity pass, only when its size and the other file's can reach 50 % (the score is at most the smaller length over the larger, in the same f32 arithmetic), and each file at most once per search; a deleted file's bytes go once it pairs, a modified file's stay to the end of the copy search. Past the limit nothing loads. The identity pass finds each added file's equal IDs by binary search in the sorted deleted files. The listing matches `jj diff` on `fixtures/oracle/jj-cli-renames.json` (§I5), renames and copies: 224 cases, 55 of them copies. A listing reads each one-sided file once to hash it and name its content with `Text::check`, then drops the bytes, and hashes each modified file's right side as it compares; `with_rewrites` reads them again, from disk or from the pinned revision, for the similarity pass alone.
- `dir_window::Source` is `Dirs` or `Changes`. A `Changes` listing builds a fresh `RunRepository` off the UI thread each time, so each refresh snapshots; it keeps the listing's `native::Against`, the repository's root, the revision as given and its pinned commits, for the files it opens and their line origins. A file opens through `native::load_listed` and `run::native_sides`, the loaders of `diff PATH`, with `DiffLookup::Native` for its line origins.

#### A6.10 Visible whitespace

One set of symbols serves every pane: `merge_view::tabs` holds them for the top panes, `gpui_base::input` for the editors, and a test holds the two sets equal. Zed draws a space as `•` at half the text size; triad draws `·` at the text size, which looks the same and lets the top panes write it into their text.

- **Top panes.** `tabs::expand` takes a `Drawing`: the tab width and whether whitespace shows. It reads the line with its ending. Shown, a space becomes `·`, a tab `→` and the spaces to its stop, each `\r` of the ending `␍`, and a line without `\n` gains `∅`. Each symbol is a replacement in the byte map, so hit testing, selection and search ranges map as for tabs and badges; a byte in the ending's symbols maps to the end of the text. The symbols are a `Mark::Whitespace` layer of `runs::cell`, drawn in the muted text colour. The row wrap's `Fit` keys on the setting, and a toggle counts the widest line again, since the ending's symbols add columns.
- **Editors.** gpui-base paints `→` and `•` over spaces and tabs. The patch (§A6.1) finds them in each visual line's own text, since the shaped text holds a badge character as spaces, adds `␍` for a carriage return that ends a line and `∅` after a last line without a newline, draws the space at the text size, and colours every symbol in the muted text colour of the badges' labels in place of the theme's `editor_invisible`, so the editors and the top panes match. `char_marks::whitespace_symbols` places them: a space's symbol in the middle of its space, a tab's and a carriage return's at their start, `∅` after the text or after `␍`, whichever ends later. The symbols paint over the text and change no layout, so a toggle only repaints.

#### A6.11 Unified view

`merge_view::unified::Unified` builds the pane's text from both ropes and the regions in one pass and keeps a run per region side: its first line and byte, its kind, both sides' first line and its region. Runs number O(regions), so a line's numbers, kind and region are a binary search, and the text goes to the editor. `merge-app`'s `UnifiedPane` holds a read-only editor over that text. The vendored editor's `LineNumbers` (§A6.1) take a function from line to label, so the gutter draws both columns through `Unified::label`; full-line bands from `Unified::bytes_of` give the tones. The pane exists only while it shows and is made again from the document each time; `update` skips a document version it already shows and keeps the caret's line number. The two panes stay alive under it, so their edits, undo and carets survive a toggle. Navigation and `current` read the unified caret while the pane shows; the View toggles act on every live editor.

### A7 Tool integration and the macOS release

#### A7.1 `vcs` crate and native diff mode

- `vcs` holds the `Repository` trait and its types; paths are `camino::Utf8Path` relative to the root. `RevSpec` is `Index`, a `Commit(CommitId)`, or a revision expression in the backend's syntax. A `CommitId` is looked up by ID (gix's object header, jj's index), so a prefix, `HEAD` or `@` in one is `UnknownRevision`. `Term::title` gives a pane title: the label; else the change ID (jj) or commit (git), then the summary. `VcsError::Backend` boxes gix and jj-lib errors, so `vcs` names neither.
- `vcs-git` (gix 0.90). `default_parent` is the index, labelled `index`, when the file's entry differs from `HEAD`'s, and before the first commit; otherwise `HEAD`, labelled with the short branch name.
- `vcs-jj` (jj-lib 0.47.1) loads the workspace with jj-lib's default settings at its single head operation. Several heads is `JjError::ConcurrentOperations`: reconciling them writes an operation. `file_at` and `commits` evaluate a revset with the user's `revset-aliases` over jj's defaults (§I1), read from the user's jj config at each call; several commits read from the merge of their trees, as `jj diff` does. A conflicted path is `Conflicted`; an ambiguous prefix, a divergent change or a conflicted bookmark is `AmbiguousRevision`. `default_parent` is `@-`, with the change ID and commit at jj's display length; a merge working copy gives an empty `Term`.
- `changed_paths` lists candidates, not an exact set: git's status reports an entry whose mode alone changed, and jj's tree diff one whose executable bit alone changed. jj's skips a file whose id is the same on both sides; git's sorts its paths into a `BTreeSet<Utf8PathBuf>` for component order.
- The conformance suite (`merge-session/tests/conformance.rs`) runs every check on git, jj and internal jj repositories built through gix and jj-lib, and six checks on jj's simple store (`Fixture::simple`, `Workspace::init_simple`), which holds no git: discovery, the parent's `Term`, annotation, a conflict read, a resolved save and the changed paths. `vcs-jj`'s `testing` feature builds workspaces with fixed settings and seed, so IDs repeat.

#### A7.2 `triad-merge install`

Writes spec §7.1's entries. git: gix's config `File`, written only when it changed. jj: jj-lib's `ConfigLayer` through `toml_edit`, keeping comments and other tables. `vcs_git::tool_config` and `vcs_jj::tool_config` hold each VCS's keys and file; `merge_session::install` holds triad's arguments. `Env` carries `HOME`, `XDG_CONFIG_HOME`, `GIT_CONFIG_GLOBAL` and `JJ_CONFIG`, so tests never touch the real environment. jj's merge tools get `$base`, `$left`, `$right`, `$output`, `$marker_length` and `$path`, and no side labels. `install` writes the absolute path of the running binary.

#### A7.3 End-to-end checks

`triad-merge/tests/tool_integration.rs`. Neither gix nor jj-lib runs merge tools, so the harness takes each VCS's half from its source:

- git: the installed `cmd` runs under `sh -c` with `BASE`, `LOCAL`, `REMOTE` and `MERGED`, as `git mergetool` and `git difftool` run it, with paths holding spaces and quotes.
- jj: `vcs_jj::testing::Fixture::resolve` follows `jj resolve` (`cli/src/merge_tools/external.rs`) through jj-lib: input files, pre-filled output, `choose_materialized_conflict_marker_len`, `$name` interpolation, exit-code rules, and `update_from_content` on the output.

jj makes markers four longer than the longest marker-like line, counting `+ - % \`; `$marker_length` carries its length to triad (spec §4.4). `docs/release-checklist.md` covers what needs the window, for git and jj alike.

`git mergetool` restores the file from its backup after exit 1, discarding triad's output. A save that leaves conflicts therefore writes the journal at once and keeps it; the next run offers the restore, with the caret on the first unresolved conflict. The merge window opens on the first unresolved conflict, else the first difference.

#### A7.4 Local install on macOS

Release 1 is for local use: no Developer ID signing, notarisation or Homebrew cask (owner, 2026-10-27). `script/macos-app` builds `triad.app` and signs it ad hoc; only the bundle gives the Dock its icon, since `setApplicationIconImage` is unsafe in `objc2-app-kit`. README § Install gives the commands.

### A11 Decisions

1. Two-way alignment uses spacer rows through a `SpacerMap` patch (§A6.5) (owner, 2026-10-25).
2. `gpui-base` is vendored with triad's patches (§A6.1) (owner, 2026-10-25).
3. The licence is MIT (owner, 2026-10-25). libgit2's tests are GPL-2.0-only and Zed's crates GPL-3.0-or-later, so they give case lists and designs only.
4. A repository with `.jj` is a jj repository and merges with jj's semantics (§B3) (owner, 2026-10-27).
5. gix moves with jj-lib, exact-pinned together, so the build holds one gix (§A3).
6. triad ships as release binaries and builds from source in its repository, never through crates.io (owner, 2026-11-18). crates.io takes no path or git dependencies and ignores `[patch.crates-io]`. A crates.io `triad-merge` would need all ten internal crates published under public names, plus the patched `gpui-base` (§A6.1) as a forked crate: twelve crates to version in lockstep, each a public API. The workspace keeps its crates for their layering (§A2), and the `triad-merge` crate on crates.io stays a placeholder that holds the name. A library crate that stands alone, such as `merge-core`, may be published later under a `triad-` name.
7. `jj-local-util` is vendored with a test-only `skip-sync` feature, which leaves out jj's flush to disk (§A13.5) (owner, 2026-11-23).

### A13 Code structure

#### A13.1 Index newtypes

`RegionIx`, `Line` and `ByteOffset` (`merge_core::index`), `RowIx` (`merge_view::rows`) and `CommitId` (`vcs`) keep a swapped argument from compiling. Each offers only the arithmetic its callers need; `RegionIx` serialises as its number. `Source::Side` and `Source::Base` keep `usize`. `region_bytes` and `blocks` stay `Range<usize>`: `Range<T>` is not iterable on stable. Each newtype has a compiling doctest and a `compile_fail` twin that swaps one argument; stable rustdoc ignores the error code, so the compiling twin shows the failure comes from the swap. The commit gate skips doctests.

#### A13.2 Fallible lookups at the view boundary

A caret, scroll or row position held across a command could fall out of range after an undo, a realignment or a shrinking edit. Each stored position is dropped or recomputed before use: a realignment, undo, redo or edit drops the pin; picks and navigation send waiting edits before reading the caret; the diff window hides copy buttons and refuses `copy_region` while regions lag, and navigation recomputes first. `MergeDocument::region_at` and `RowMap::get_row` return `Option`; `nav::held_region` and `diff::held_region` always return a region. Clippy's `indexing_slicing` is not enabled: its 64 findings index by `Side`, fixed arrays, ranges the same call computes, or checked sources.

#### A13.3 Window size

`window.rs` and `diff_window.rs` are large because the windows have many features; their logic with state sits in components (§A6.2). The two windows share `OriginColumn`, `editor_sync`, `blink`, `ruler`, `settings_banner` and `Wrap`. Their remaining shared method names are accessors and wiring of 2–9 lines. The diff window keeps seven loose scroll fields; a `DiffScroll` like `PaneScroll` waits for a diff scroll bug to start from (PLAN.md §3).

#### A13.4 Recovery gate

A journal write costs one or two syncs, and on macOS `sync_all` and `sync_data` are `F_FULLFSYNC`, about 4 ms each on an Apple SSD. A whole write (temporary file, sync, rename, directory sync) takes 9.0 ms median and 14 ms max from 1 KB to 1 MB; an append takes 4.0 ms median and 5.2 ms max. A whole write is longer than a 120 Hz frame (8.3 ms), so the UI thread never waits for one.

`Gate` orders the journal's file operations. Every generation of a journal shares one lock, which a write or a removal holds while it runs; each generation has its own open flag. `Recovery::retire` clears the flag without waiting and starts the next generation. A running operation ends before any of the next generation starts, and one of the old generation that starts later skips. A write that has renamed its file cannot be undone, so order comes from the lock, not the flag.

`Locator::find` hashes the inputs and reads the journal they name on the background executor. `Recovery::offer` takes that journal for the restore offer and holds every write until the answer (`OnDisk::Offered`), so a crash while the offer is open keeps that journal. `Recovery::discard` returns a `Removal`, which the window runs on the background executor in the slot of the running write. No write starts until `Recovery::removed` takes its result; then commands made since start a new file. Restore replays the journal on the background executor; Start Over and a failed restore take this path. Save & Exit writes the journal in its save job (§A6.4). As the window finishes, `Session::finish_journal` writes or removes on the UI thread and waits for a running operation: the window is closing. A flush of a file that holds every command writes nothing, so a save that leaves conflicts does not write the journal twice.

Tests hold the lock on a second thread behind a barrier, with no sleeps. A retire that waited would deadlock, and nextest ends it at 30 s.

#### A13.5 Pre-commit gate time

Limit: 35 s on a warm build (AGENTS.md). The gate takes 12 s after an edit to `merge-app` and 16 s after one to `merge-core`.

An edit under `vendor/` has a 60 s limit. It takes 45 s: the 107k-line `gpui-base` compiles three times, for clippy (7 s), for the workspace tests with `merge-app` and `triad-merge` (20 s), and for its own unit tests in `target/vendor`, with test features and debug info (8 s). The next run takes 10 s.

The full suite, which `script/pre-push` runs, takes 17 s on a warm build; the quick profile, which `script/pre-commit` runs, takes 9 s. `script/checks` stops a full run after 60 s and a quick one after 45 s: each run's time plus nextest's 30 s per-test cap, so nextest names a hung test first. Every test build happens before the limit starts, `gpui-base`'s included.

On Windows the gate has no time limit. Each test process takes 57 ms to start, even alone, and the quick profile takes 102 s on an 8-core machine. The gate scripts run there under Git Bash. `script/platform.sh` covers what differs: jj prints paths with `\`, cargo prints `C:\…` manifest paths, native `jq` and `python` write CRLF, and native programs such as rustfmt take `C:/…` paths. `script/test-pre-commit` checks the path handling. A script written there has no executable bit; `script/check-workspace` fails on macOS and Linux when a file under `script/` has a `#!` line and no executable bit.

- **No flush to disk in tests.** jj-lib persists every object, operation, view and index file through `jj-local-util`, which flushes each with `sync_data`: on macOS an `F_FULLFSYNC`, which this disk does about 700 times a second and serialises across processes, against 20,000 a second for `fsync`. With the flush, the tests that build jj repositories take 116 s of the full suite's 123 s, and more test threads do not shorten them. `vendor/jj-local-util` adds a `skip-sync` feature that leaves the flush out; `vcs-jj`'s `testing` feature turns it on. Only dev-dependencies turn `testing` on, so a release build (`cargo build --profile dist -p triad-merge`) keeps the flush; `script/check-workspace` fails when a non-dev dependency turns `skip-sync` on. A debug binary built with `--all-targets`, as `script/checks` builds it, skips the flush too.
- **Object files.** With debug info, rustc on macOS keeps every build's `*.rcgu.o` beside its output under `target/debug/build`, and rustc lists the directories it links from: 1.26 million objects in one directory cost 10 s a listing. `[profile.dev] debug = false` stops them; panics keep their locations, and `CARGO_PROFILE_DEV_DEBUG=line-tables-only` turns debug info on. A `target/` built with debug info needs `find target/debug/build -name '*.rcgu.o' -delete` once.
- **Optimised dependencies.** `[profile.dev.package."*"] opt-level = 1`: merge-app's tests take 75 s of CPU against 130 s at 0; the highlighter comparison 1.3 s against 4.6 s. A change to the setting rebuilds every dependency, 84 s cold.
- **One integration-test binary per crate**, `tests/main.rs`.
- **No serial tests in the quick profile.** A test with `threads-required = "num-test-threads"` holds every thread; the six highlighter comparisons cost 10 s of 20.
- **jj tests in `jj` modules**, which the quick profile drops by path.
- **Scoped checks.** `script/checks --quick` tests the changed crates and their dependents, and runs `cargo deny` and the vendored tests only when their inputs changed.

#### A13.6 VCS boundary

The `vcs` trait follows jj's model, and each backend holds its VCS's rules: `merge_candidates`, `copy_candidates`, `working_copy_id` and `index_base` for origin lookup (spec §7.3), `conflict_style`, tool config, snapshots and saves. `merge-session` holds no revision syntax; it branches on `RepoKind` only for tool protocols, semantics, config and display. jj's marker rule is a merge rule, so it lives in `merge_core::jj::marker_length` over jj-core. There is no backend registry: a third VCS would reshape the interface.

jj config: `vcs_jj::config` loads jj-cli's layers (jj-lib's defaults, jj-cli's snapshot defaults, system files, user files or `JJ_CONFIG`, the repository's and workspace's secure config, `JJ_*` overrides) and resolves `--when` scopes. The merge settings start from an empty stack, since jj-lib's defaults set both keys and an unset key must leave triad's settings their precedence. Reading writes nothing: secure config is found by ID or legacy path, never migrated. No host name and no environment beyond `JJ_CONFIG` pass, so most `--when.hostnames` and `--when.environments` scopes do not match (spec §6.1).

#### A13.7 Crate layering

Each crate depends only on crates in earlier layers:

1. `merge-core`, `vcs`
2. `merge-syntax` (on `merge-core`); `merge-config` (on `merge-core`, `vcs`); `vcs-git`, `vcs-jj` (on `vcs`)
3. `merge-view` (on `merge-core`, `merge-syntax`, `vcs`); `merge-session` (on the above and both backends)
4. `merge-cli`, `merge-app` (on `merge-session`, `merge-config`; `merge-app` also on `merge-view`)
5. `triad-merge`

- `merge-core` does no file I/O and knows no VCS.
- `merge-syntax` holds languages, grammars, highlight spans and syntax boundaries; tree-sitter stops there.
- `merge-config` holds the config file's format, settings, key bindings and the names of their values (`named::Named`, over strum derives). It reaches no repository; `script/check-workspace` holds it to `merge-core`. `merge_session::origin::id_kind` turns its `RevisionId` into `vcs::IdKind`.
- `merge-session` is the one crate that picks a backend: semantics, discovery, `resolve`, origins, loading, saving, recovery, exit codes and `WindowForm`. The front ends take the jj-native report types from `merge_session::jj_native`; `script/check-workspace` holds `vcs-jj` and `vcs-git` to their dev-dependencies.
- `merge-cli` parses arguments and runs batch forms (`Invocation` is `Window`, `Batch` or `ShowConfig`); `merge-app` is the GPUI shell and matches no headless form.

**One repository rule.** `native::nearest_repo` finds the nearest `.jj` directory or `.git` entry, jj on a tie, for the semantics and for discovery, which opens only that backend at that root. A `.git` git cannot open is `NotARepository`, never a climb to an outer repository (spec §6.1).

**One repository per run.** `merge_session::repository::RunRepository` holds the directories a run's repository may lie in, in order, and opens the first on first use; every step takes it: semantics, marker style, `resolve`, origins and saves. `find` opens a git candidate as it finds it and passes over one git cannot open, so kind, root and origins come from one repository. A `Handoff` found again keeps the run's jj config. The jj config comes from one `Discovery` per run, built from the run's `Env`; the front ends never name jj's `ConfigEnv`.

**One save.** `EncodedOutput::new(document, ending)` encodes; `write(path)` writes (`io::WriteError`); `save(path, record, run)` writes, then records the save in the run's repository (`SaveError::Encode` and `SaveError::Write` wrote nothing; `SaveError::Record` wrote the file and keeps the `VcsError`). The merge window builds, encodes and saves on a background thread, from a `PendingOutput`.

### A14 Search and replace

- **Editors.** The result and both diff panes use gpui-component's search panel; the diff window's left pane is `replaceable(false)`.
- **Input panes** are rows, not editors, so the result's search session drives them: the window observes the result editor and searches every input pane (`input_find.rs`) over `merge_view::search`. The matcher is Aho-Corasick on one literal with ASCII case folding, as in the editors, so both count alike.
- **Off the UI thread.** On three 100k-line inputs of 4.7 MB, case-insensitive: `value_9999` (33 matches) takes 9.8 ms to find; `e` (1.2M matches) 17 ms to find and 60 ms to order. The search runs on the background executor over O(1) `Text` clones; a result for an older query drops.
- **Focus.** The input panes hold no focus: a click moves the result caret and keeps the focus there, so the result's query serves every pane.
- **Blink.** `merge_app::blink::Flash` runs every blink. An editor's search panel steps inside gpui-component, so `blink::SearchSteps` reads steps off the session: the current match moved while the matcher's `Rc` of ranges stayed.
- `script/measure-app` has no mode with a query open.

## B. Merge semantics

triad merges as the repository's VCS does: `git merge-file` in a git repository, jj's file merge in a jj one. triad's own refinements sit on top in both modes and change no output unless turned on.

### B1 Goal

- **Default output equals the VCS's**, byte for byte, with conflicts in the same places. In jj mode this holds when every input is UTF-8 and either all or none start with a byte order mark (§B2).
- **One model.** Both modes produce the same `Region` list (spec §4.3).
- **Refinements are opt-in and visible.** A refinement that changes the output is off by default, marks what it resolved, and shows in the status bar.

### B2 How jj merges (jj-core 0.47)

From `jj-core/src/diff.rs`, `jj-core/src/merge.rs` and `jj-lib/src/files.rs`:

- `jj-core` holds the diff (`ContentDiff`, `CompareBytes`, the tokenizers) and `Merge` with `trivial_merge`. `jj-lib`'s `files.rs` holds the file merge on top, about 80 lines.
- `ContentDiff::by_line` diffs base 0 against every other input in one pass. A hunk is `Matching` only where all inputs agree.
- jj's line diff anchors first: among base's least frequent lines, those as frequent in the other input, matched by LCS, recursing between anchors; only without an anchor does it trim prefix and suffix. It gives up anchoring where every base line occurs over 100 times. No slider compaction, no indent heuristic, so it differs from git's histogram on repetitive input.
- jj merges bytes. A UTF-8 BOM is part of its first line, and UTF-16 splits at each `0x0A` byte, so jj and triad diverge on UTF-16, with or without a BOM, and on inputs that disagree about a BOM. Windows-1252 is one byte per character, so a line-level merge of its text gives jj's bytes.
- Lines compare byte-exact; the merge has no whitespace modes.
- An empty base or side makes the whole file one `Different` hunk.
- `merge.same-change` (default `accept`) resolves a hunk where every side made the same change; `keep` leaves it conflicted.
- `merge.hunk-level = word` merges each conflicted line hunk word by word and keeps the result only when the whole hunk resolves. A word is a run of ASCII letters, digits, `_` and bytes `0x80`–`0xff`.

### B3 Choosing the mode

A repository with `.jj` is a jj repository and merges with jj semantics. git merges there are unsupported: mutating git commands race jj's import and export of refs.

| Source | Mode |
|---|---|
| `--merge-semantics git` or `jj` | as given; `git` in a jj repository exits with 3, writes nothing, and names `jj resolve` |
| `install --git` / `--jj` | writes the flag into the tool config, so a tool run never probes |
| native resolution (spec §7.4) | the backend's VCS |
| `--merge-semantics auto` (default) | the nearest ancestor with `.jj` or `.git`, probing the working directory, then the output path; neither gives git |
| settings file `merge-semantics` | as the flag; the flag wins |

`jj resolve` writes all four files to a temporary `jj-resolve-<random>/` directory, and runs the tool in the caller's working directory, which may lie outside any repository (`jj -R`). So `auto` probes the working directory first, and a path in `jj-resolve-<random>/` also marks jj as the caller.

In jj mode, `merge.hunk-level` and `merge.same-change` come from the user's jj config (§A13.6); a triad flag or setting overrides each. Under `jj resolve`, jj has already merged with them, so they change triad's output only on manual runs and in native resolution.

### B4 Options

| Field | Values | git default | jj default |
|---|---|---|---|
| `semantics` | `git`, `jj` | `git` | `jj` |
| `same_change` | `accept`, `keep` | `accept` | from jj config |
| `hunk_level` | `line`, `word` | `line` | from jj config |

- `diff-algorithm` applies in git mode only; jj mode has one diff.
- `ignore-whitespace` applies in both modes: jj mode passes `ContentDiff::for_tokenizer` a `CompareBytes` keyed by `diff::line_key`. Spec §4.2's output rule applies, so any mode other than `none` diverges from jj and the status bar marks it. `ignore-case` and `ignore-pattern` take the same path and diverge from both git and jj: neither folds case or cuts text.
- `same_change = keep` in git mode and `hunk_level = word` in git mode diverge from git and are marked.
- `refine-conflicts` changes no merge result, so it stays on in both modes. Under jj semantics it changes no saved file either: `MergeDocument::output` writes markers over each conflict and the changed regions next to it, the pieces of one jj hunk, so jj reads the base back as it stores it (spec §4.4).

### B5 Design

#### B5.1 The jj engine

`merge-core` depends on `jj-core` (Apache-2.0, exact-pinned with jj-lib), so the diff is jj's by construction. `files.rs`'s hunk resolution lives in `jj-lib`, which `merge-core` must not depend on, so `merge-core` re-implements those 80 lines over `Merge` and `trivial_merge`, with an Apache-2.0 notice. jj-core adds about 44 crates to a clean `merge-core` build (5.5 s against 3.6 s). Byte ranges map to line ranges by binary search in `Text`'s line index.

jj-core's `regex` and `aho-corasick` add `PartialEq<Span>` impls for `Range<usize>`, so `assert_eq!(ranges, [])` cannot infer its type in any crate linking `merge-core`; such tests compare against a typed empty constant.

#### B5.2 jj oracle corpus

`fixtures/oracle/jj-corpus.jsonl`, from `script/gen-jj-oracle` (an example in `vcs-jj`, the only crate that can call `jj_lib::files::merge_hunks`). Its first line, `{"jj_core": VERSION}`, names the jj-core release from `Cargo.lock`, and a test fails when the lock pins another, so a jj upgrade writes the corpus again (`docs/jj-upgrade.md`). It runs the git corpus's triples and jj seeds under each `same_change` × `hunk_level`, and records each mode's conflict count and jj's hunks in order, each `{"resolved": text}` or `{"conflict": {"sides", "bases"}}`, so tests check where each conflict lies. Hand seeds cover repetitive lines, touching changes, identical changes, word-resolvable conflicts, multi-byte UTF-8 in words, the 100-occurrence limit, and five- and seven-term merges. The corpus stores UTF-8 strings, so UTF-16 and BOM cases are unit tests. The oracle shares only jj-core with triad, so it tests triad's hunk resolution, its mapping to regions and its rendering, not the diff.

1,970 cases. jj's line-mode conflict count differs from git's in 58 git cases; where both merge cleanly, the outputs match. Word mode resolves more than line mode in 261.

#### B5.3 jj mode in `merge-core`

`align` dispatches on `semantics`: each `Matching` hunk becomes `Unchanged`, each `Different` hunk a region classified by the shared `classify` (`trivial_merge` on comparison keys). With more than two sides this adds jj's same-change rule: sides X, X, Y over bases Y, b resolve to X. `same_change = keep` makes an all-sides-equal region a `Conflict`, and refinement leaves it whole. jj mode matches every case of the jj corpus in all four modes, hunk for hunk.

#### B5.4 Word-level merge

`Kind::WordMerged` holds `Resolution::Auto`; its text comes from `merge3::word_merge` whenever it renders, so regions carry no text and undo returns to it as to any automatic text. `align_merged` word-merges conflicts in parallel on rayon and keeps each text for `MergeDocument::new`. The tokenizer follows jj's, so jj mode matches byte for byte; git mode uses the same. Word-merged regions have their own tone and gutter letter, and the status bar counts them.

#### B5.5 Mode selection

Detection looks for `.jj` and `.git` by name (§A13.7's rule), so it cannot fail on an unreadable repository and costs no load. Semantics, same-change and hunk level are chosen once per run.

#### B5.6 Status bar

It leads with `git merge` or `jj merge`, `by word` at the word level, then one item per divergence from the VCS.

#### B5.7 Refinements

Each is opt-in, a suggestion in the region's menu, never an automatic resolution:

1. **Insert/insert:** `Suggested: Left + Right` when both sides insert at one place and delete nothing (`merge_core::suggest`).
2. **Sorted union:** when both sides add lines to a block sorted in every input. Applied as an edit, so undo and the journal need nothing new.
3. **Deep refine:** `refine-conflicts = deep` splits a conflict at interior shared lines that map consistently to base. It only splits; no piece resolves, since a piece's base range rests on an open choice of cut.
4. **Moved blocks:** `Carry Edit to Moved Block`, three-way only. A move is a conflict where one side deletes base's lines and the other edits them, and the deleting side inserts the same lines once elsewhere in a region resolved to its text. Carrying is one edit of both regions.
5. **Syntax-aware boundaries** (§B5.8).

`mergiraf` (GPL-3.0) is out under spec §2.5.

#### B5.8 Syntax-aware boundaries

The setting only rejects: a deep-refine split or a word merge that cuts through a syntax node is dropped, and the region stays as it would without it. Every output is one the run without the setting could give. It applies in both modes; in jj mode a rejected word merge diverges from jj, so the status bar marks it.

A region's text, less surrounding whitespace, is its core; a cut is a byte offset of one input.

- **Deep refine.** A cut is clean when no node lies across it inside the core: every node holding bytes on both sides also holds the whole core. A shared block is kept only when both its edges are clean in every side. A shared `}` closing a different block on each side fails on the side whose block it closes.
- **Word merge.** A cut is clean when it lies inside no token (a leaf: a string's content, a comment, `->`). The node rule would reject nearly every word merge, since every word hunk lies inside some statement. The merge's text stays only when every word hunk resolved from a side starts and ends at clean cuts of each side holding its text.
- The region parses with the lines around it, from the top-level line at or before it to the first top-level line after it (§F4). A window over 20 000 lines, an input without a grammar, or an error node across the core leaves every cut clean.

`merge-core` stays free of tree-sitter: `merge_core::boundaries` holds `Cuts` and the `Boundaries` trait; `merge_syntax::boundaries::SyntaxBoundaries` implements it, parsing each region's window, never the whole file. `align_bounded` refines each conflict on rayon, so the parses spread over the cores; each thread keeps one parser.

### B6 Benchmarks

| Measure | 100k lines | 1M lines |
|---|---|---|
| `load-jj` | 42 ms | 505 ms |
| `align-jj` | 34 ms | 426 ms |
| Session peak heap, jj / git | 56 / 32 MB | 607 / 363 MB |
| `load-word-small` (a one-line conflict every 20 lines) | 57 ms | 657 ms |
| `load-word-large` (one conflict over the whole file) | 1.61 s | 16.1 s |
| `load-word-small-rust`, off / bounded | 45 / 50 ms | 498 / 541 ms |
| `load-deep-rust`, off / bounded | 43 / 49 ms | 458 / 510 ms |

- `load-word-large` is one call to jj-core's `ContentDiff::by_word`, about 16 µs per line, as jj's own word merge makes. Spec §9 exempts it (§B7, decision 5); `script/check-bench` holds it below 2 s at 100k and 20 s at 1M.
- `cargo bench -p merge-syntax --bench boundaries` writes valid Rust: four-line functions with a conflict in every fifth. The deep case parses each conflict's window in both sides, about 9 µs a conflict on one core.
- **git's histogram on regular input.** The headless merge of the 100k-line `deep` triple takes 1.5 s under git semantics and 70 ms under jj's. Its unchanged runs are all about one length, each region's anchor is the first, so each split takes one function off the front and the recursion goes about 5 000 levels deep. git's `xhistogram.c` does the same: `git merge-file` takes 1.27 s. The hunks must match git's (spec §4.2).

### B7 Decisions

1. Depend on `jj-core` and re-implement `files.rs`'s hunk resolution in `merge-core` (spike, 2026-10-28; §B5.1).
2. A repository with `.jj` merges with jj semantics; `--merge-semantics git` there is an error (owner, 2026-10-27; §B3).
3. Word-merged regions in git mode are automatic, as in jj, when `hunk_level = word` is on, with their own tone and a status-bar mark (owner, 2026-10-28).
4. `ignore-whitespace` in jj mode is allowed, with spec §4.2's output rule and a status-bar mark (owner, 2026-10-28).
5. A word-level merge of one conflict as large as the file follows jj's cost and has no spec §9 target. A port of jj's word diff gains too little; a size cap breaks parity (owner, 2026-10-28).
6. Syntax-aware boundaries share M7's tree-sitter and grammars, so the build holds one copy of each (owner, 2026-10-29).
7. Syntax-aware boundaries only reject a cut; a rejected cut does not move to the nearest clean line. One setting, `syntax-boundaries`, covers both refinements in both modes (owner, 2026-11-04).

## C. Chunk origin

### C1 Goal

- Each line names the commit that last changed it, where the VCS records one: commit, jj change ID, author, author's date and summary, in a hover card and an optional gutter column.
- Lines the VCS has not recorded, and inputs it cannot place (§C3), show no origin.
- The window never waits: annotation runs off the UI thread. Its cost grows with history.
- git and jj fill the same types; jj adds change IDs.

### C2 How the backends annotate

- **gix:** `Repository::blame_file`, behind gix's `blame` feature (`gix-blame`, MIT or Apache-2.0), in `vcs-git` only. It walks every parent; rename tracking stays off.
- **jj-lib:** `FileAnnotator::from_commit(commit, path)`, then `compute(repo, RevsetExpression::all())`, so every line resolves; `vcs-jj` takes an `Err` origin as the origin. `with_file_content` annotates text no commit holds. jj-lib tracks no renames.

### C3 Which inputs can be annotated

An input gets origins from a revision only when the file there holds exactly the input's bytes, and only when it is UTF-8. This one check covers renames, a `jj resolve -r` triad cannot see, a stopped rebase, and inputs edited before the run. Spec §7.3 holds the table of which revision each input comes from; the row follows the repository's kind, not the semantics. A working-copy side annotates its parent and carries the result onto its text by a line diff (`Fit::Changed`): `with_file_content` on jj, a line diff on git.

### C4 Types

`vcs::Annotation` holds one `LineOrigin` per commit and `OriginRun`s covering lines, so a 100k-line file with few commits stays small; `origin(line)` is a binary search, and runs may leave lines uncovered. `Repository::annotate(path, &CommitId)` gives `None` when the file does not exist there. `commits(rev)` and `merge_base(one, two)` pin each input to one commit; git's merge state names no merge base.

### C5 Design

#### C5.2 Which revision each input comes from

`merge_session::origin` gives one `Option<Source>` per input: path, `CommitId` and `Fit` (`Exact` or `Changed`). git's right side and base follow the stopped operation (a revert's right side is `REVERT_HEAD^`, its base `REVERT_HEAD`; a cherry-pick's and a rebase's base is the commit's parent). jj tries `@`, its parents, and their merge base, which gives a two-sided conflict at `@` its base. A file that does not exist is found through its nearest existing directory.

#### C5.3 View model

`merge_view::origin::Origins` holds one optional `Arc<Annotation>` per input, so a copy for a background task or a hover card costs a reference count per input. A result line takes the origin of the input its region's pick takes it from; an unchanged line prefers base; edited text has none. A run's label goes on its first line. `carry` is a byte-exact line diff, so a line changed only in whitespace loses its origin.

#### C5.4 Window

Each input annotates in a background task; a failure is logged and leaves that input without origins. The diff window carries each pane's annotation onto its current text in each recompute's background step (`Recompute::run_with`), so edits, undo and reloads update origins (§C6, decision 3). The editors' gutters show labels through the vendored `EditorState::set_line_labels`. The result's labels visit every line, 231 ms at 1M lines, so `Origins::result_runs` runs on the background executor over a `MergeDocument::snapshot` and a copy of the `Origins`, and hands back each run's lines and `OriginKey`. A command starts it once commands pause for 100 ms (`ORIGIN_DEBOUNCE`); the load and the toggle start it at once; a newer start cancels it. Until it lands, the labels stay on their lines. The UI thread turns the runs into labels, 65k runs at 1M lines. A diff pane's labels are its annotation's runs (`Origins::runs`), one per run, with no visit of each line. A short ID fits inline in its `SharedString`, a `SmolStr`, so a label allocates nothing; each hover card holds its annotation and builds its text when it shows. Hover cards are GPUI tooltips, which headless tests do not open; the text each shows is tested.

#### C5.5 Cost

`cargo bench -p merge-cli --bench annotate`, 2026-10-29: one 100-line file whose line 0 comes from the first commit, so every annotation walks the whole history.

| Commits | git (gix-blame) | jj (jj-lib) |
|---|---|---|
| 1,000 | 213 ms | 78 ms |
| 10,000 | 1.93 s | 890 ms |

Linear in the commits walked: about 0.2 ms each on git and 0.09 ms on jj. Spec §9 sets no target.

#### C5.6 Tool-mode diff sources

`git difftool` and `jj diff --tool` pass temporary copies, so `diff LEFT RIGHT` finds sources by §C3's byte check alone, in the working directory's repository, with the path from the title or the right side's path. git tries `HEAD`, the index's parent `HEAD` by a line diff, and the working tree; jj tries `@-` and `@`. git's difftool passes `--title "$MERGED"`, which names the file when both sides are copies.

#### C5.7 Revision chips

Each pane heading names its input's commit after the title (spec §5). `Found::describe` reads each through `Repository::describe`. `revision-id` (`change`, `commit`) picks jj's ID; git shows commits. A title that starts with the commit's ID (`Term::title` without a label) shows without it beside the chip; markers keep the whole label.

### C6 Decisions

1. The result pane shows origins through the region's pick (owner, 2026-10-29).
2. No rename tracking in git: jj-lib has none, and §C3's byte check drops inputs a rename moved (owner, 2026-10-29).
3. The diff window's panes take origins by a line diff of the revision's text against the pane's text, with each recompute. A line map shifted by each edit needs its own undo, redo and reload handling; hiding origins after the first edit loses the pane users edit; a fresh `annotate` per pause costs history (owner, 2026-10-29).

## D. Native resolution

### D1 Goal

- `triad-merge resolve [PATH]` opens a conflicted file from the repository with no merge tool between; with no path, it lists the conflicted files and opens them in turn.
- git's conflict comes from index stages 1–3; jj's from the working-copy commit's tree as n sides and n − 1 bases.
- Each term carries what the VCS knows: git's stopped operation (spec §7.3), jj's `ConflictLabels`.
- A jj conflict of more than two sides opens one top pane per side.
- The merge mode follows the backend.

### D2 Writing to the repository

- **git:** a resolved save stages the path as `git add` does: the file through `convert_to_git`, a blob, a stat from the file, the mode from the file or, without `core.fileMode`, from ours, theirs, then base. The stages go, the entry goes in, the tree extension is dropped, and the index is written under its lock. A save with markers leaves the stages.
- **jj:** two snapshots, as `jj resolve` makes (§D5.5).

### D3 What can be resolved

| Terms | Result |
|---|---|
| Files, each text in one encoding (spec §4.1) | Opens. An absent term is an empty input. |
| A binary file among the terms | Refused: not text. |
| A symlink, submodule or directory among the terms | Refused: no text merge resolves it. |
| Every side absent (delete/delete, a file renamed two ways) | Refused: the resolution is a deletion. |

The executable bit never conflicts alone: both VCSs merge it apart from the bytes, and a merge of one bit always resolves. A content conflict whose terms differ in the bit carries each term's own. `read_conflict` reports what the VCS holds; `merge_session::resolve` decides what it refuses. `conflicts()` lists every conflicted path, so `resolve` can say why it skips one.

### D4 Types

`vcs::Conflict` holds the path and `ConflictTerm`s for n sides and n − 1 bases in jj's order; git gives stage 2, stage 3 and stage 1. A term's `TermContent` is `File { bytes, executable }`, `Absent`, or `Other(EntryKind)`.

### D5 Design

#### D5.2 Reading a conflict

The conformance cases come from libgit2's tree-merge tests (`trivial.c`, `modeconflict.c`, `renames.c`, `recursive.c`), rebuilt through gix and jj-lib (§A4.8's licence rule); git conflicts come from `gix-merge`'s tree merge in `vcs-git`'s dev-dependencies. rename/modify conflicts at the new name with all three stages; rename/rename follows git's `ort` layout, the base alone at the old name and each side at its name. A criss-cross takes the virtual merge base at stage 1, which names no commit; jj gives several bases. jj's labels index the path's value term by term; jj gives no `ChangeInfo`.

#### D5.3 `resolve` opens the window

The repository is the working directory's, else PATH's. Inputs are the terms, sides then bases; each label is `Term::title`. The window's title is PATH from the root, and its output the working-tree file. Semantics follow the repository. The `resolve` queue (`resolve_queue::Finder`) lists conflicts, loads the next file and skips refusals on a background thread; a save closes only the merge window it opened. The queue's windows do not follow the settings file.

#### D5.4 Saving to the working copy

- **Resolved in the working tree.** git keeps stages until `git add`, so a file resolved by hand still lists. When every side is a file the VCS wrote markers, so a working-tree text file without a conflict-opening line holds a resolution and is refused. A non-text file is refused too; a missing file passes, as does a conflict with an absent side, which git writes without markers.
- **Edited beside the markers** (git). A file with markers may hold edits the stages lack. `resolve` compares the sides the file's markers give with the sides triad's git-semantics merge of the stages gives, ignoring labels, marker length and zdiff3 trimming. A match loads from the stages, with labels and bases; a difference opens from the markers (§D5.7), keeping the edits at the cost of an empty base in git's `merge` style. A byte comparison would fail on labels, attributes, filters and `-X` options.
- **Marker length.** jj parses only markers at least as long as the length it materialised the file with (`TreeState::file_states`). The run's length is the larger of the requested length and jj's recorded one, or jj's choice for the terms without a record.

#### D5.5 Writing the repository

- **Before.** `resolve` snapshots the working copy before reading the conflict, in its own operation, so the user's unrecorded edits land there and the conflict is read with them. A path the snapshot leaves clean exits as a clean path does.
- **After.** Each save snapshots again in a second operation, `save <path> in triad`, so `jj undo` reverts the resolution alone. A save whose markers read back as the conflict jj holds writes no operation. A run cancelled after the first snapshot leaves that operation, as `jj status` does.
- `JjRepository::snapshot` follows jj-cli: it imports a moved git `HEAD` first (`import_git_head`), refuses a stale working copy (`VcsError::StaleWorkspace`, naming `jj workspace update-stale`), reloads an updated one, snapshots with the user's options, commits a rewrite of `@` and rebases descendants, and marks new files intent-to-add in git's index. After the snapshot it imports git's refs as jj-cli's `import_export_git_refs` does: `GitImportOptions` from `GitSettings` and each remote's `auto-track-bookmarks` parsed with `revset::parse_string_expression`; on a change, a rebase that skips `::(immutable_heads() | root())` as it stood before the import, a new commit on an `@` the import made immutable, `reset_head`, export and checkout; without one, an export, committed only when it changes something. An internal workspace skips the imports and the index.
- In a colocated workspace, `take_snapshot` takes jj's `git_import_export.lock` in the repository directory before it loads the head operation, and `rewrite` takes it before `reset_head`, the export, the commit and the checkout, as jj-cli does. jj-cli waits for the same lock before it loads, so a jj command and triad do not both build on one head and leave two. An internal workspace has no such lock in jj either: a jj command that commits between triad's load and triad's commit leaves two heads, which jj merges on its next command and triad refuses until then.
- Snapshot options come from the user's jj config: `snapshot.auto-track` with the layers' `[fileset-aliases]`, `snapshot.max-new-file-size` (0 is no limit), and git's ignore files. `fsmonitor` is always `none`: triad's jj-lib has no watchman.

`cargo bench -p merge-cli --bench resolve`, 2026-11-18:

| | 10k files | 100k files |
|---|---|---|
| jj: load a conflict | 12 ms | 101 ms |
| jj: list conflicts | 11 ms | 96 ms |
| jj: record a save | 53 ms | 168 ms |
| git: load / stage | 1.0 / 3.5 ms | 6 / 33 ms |

`jj util snapshot` on the same shape takes 108 ms unchanged and 176 ms after one edit: the cost is jj's scan, which `jj resolve` pays too. Saves, lists and loads run off the UI thread. `resolve::load` takes `Snapshot::Taken` to read at a snapshot the caller just took, so the queue makes two scans per file.

#### D5.6 Line origins in `resolve`

A `resolve` window looks origins up as a merge does, from the working-tree file. jj gives no commit per term, so `@`'s parents and their merge base stand in; a conflict whose sides come from other commits, as after a rebase, shows none.

#### D5.7 Conflict markers

`resolve PATH` also opens a file whose conflict lives only in its markers: committed markers, stages cleared by `git add`, files from other tools. A file with a VCS conflict always opens from the VCS. `merge_core::markers::parse` is triad's own parser. jj-lib's `parse_conflict` rejects git's two-way style, needs the arity up front and drops labels.

| Style | Writer | Sections |
|---|---|---|
| git `merge` | git's default | `<<<<<<<` left `=======` right `>>>>>>>`; no base |
| git `diff3`, `zdiff3`; jj `git` | git, triad, jj | `<<<<<<<` left `\|\|\|\|\|\|\|` base `=======` right `>>>>>>>` |
| jj snapshot | jj, triad with more than two sides | `<<<<<<<`, `+++++++` before each side and `-------` before each base, `>>>>>>>` |
| jj diff | jj's default | `<<<<<<<`, `%%%%%%%` (with `\\\\\\\` notes) before a diff from a base to a side, `+++++++` before a plain side, `>>>>>>>` |

- A marker is a run of one marker character, then whitespace or the line's end. The marker length is the longest run of `<`, at least 7, at which the markers parse; a longer run that does not parse falls back to the next length.
- Every conflict has the same number of sides. A start without an end, sections out of order, a bad diff line or mixed arity is an error naming the line.
- Each input's text is the text outside the conflicts plus its section in each conflict. A conflict without a base section opens as add/add.
- A section's last line keeps its newline unless the end marker's line has none, as jj reads it.
- Labels come from the first conflict's marker lines; jj's numbered defaults count as none.
- A cross-check against jj-lib parses jj's `materialize_merge_result` output in each style, two to four sides, LF and CRLF.

### D6 Decisions

1. Saving to the working copy comes before repository writes; git then needs `git add`, jj snapshots after the save (owner, 2026-10-29).
2. Conflicts of non-files are refused (owner, 2026-10-29). Choosing an executable bit or a symlink target is not a text merge.
3. On jj, `resolve` also snapshots before reading, in its own operation, so `jj undo` reverts the resolution alone (owner, 2026-11-01).

## E. p4merge parity

### E1 Scope

The gaps between triad and p4merge that matter for text merges: the config file (§E3), the overview ruler (§E4), the output line ending (§E5), Save Copy As (§E6), Linux and Windows (§E7), syntax highlighting (part F), appearance and save keys (§E3.8) and key bindings (§E9). Out of scope: image and binary diff, encodings other than UTF-8 and UTF-16, a settings dialog (the config file replaces it).

### E3 Config file

A plain `key = value` file in Ghostty's style. The file is the settings UI: Settings… opens it in the user's editor. triad reads it once at startup; a run is short, and a watcher costs more than it gives.

#### E3.1 Format

Spec §6.4 gives the format. One choice needs its reason: `config-file = PATH` reads the other file at that point, so a later line overrides it. This is git's rule for `include.path`. Ghostty reads includes at the end, so a file's own lines cannot override them; triad takes git's rule because a later line winning is the rule for every other key.

#### E3.2 Errors

Each bad line is reported as `PATH:LINE: message`, and its key keeps the value before it; the other lines apply. The windows show the errors in a banner with the alert role (`settings_banner`); headless runs print them on stderr.

#### E3.3 Location

`$XDG_CONFIG_HOME/triad/config.triad` (`~/.config/…` without it) on macOS and Linux, `%APPDATA%\triad\config.triad` on Windows; `TRIAD_CONFIG` names another file. macOS uses the XDG path, as Ghostty, git and jj do, because users keep their dotfiles there. The `.triad` extension lets editors recognise the file. One location per platform: Ghostty's second macOS location gives two files to keep apart for no gain.

#### E3.5 Commands

- Settings… opens the file with `open -t` on macOS, `xdg-open` on Linux, Notepad on Windows; Launch Services has no handler for `.triad`, so `open_with_system` cannot. A missing file is first written with every key commented out at its default. If no editor opens, stderr names the file and the file manager reveals it.
- The command line takes only the five merge options; every other setting is in the file only, so `--help` stays short and a caller cannot change how the window looks. Tests set `TRIAD_CONFIG`.
- One table in `merge_config::settings` holds each key's description and drives `show-config --docs`, the file Settings… writes, and spec §6.4's table.

#### E3.6 Languages

The windows' text lives in Project Fluent catalogues, one per language: `crates/merge-app/locales/<tag>.ftl`. `include_str!` builds them into the binary, so a run reads no files and a missing catalogue fails the build. `strings.rs` names the messages, and `error_text.rs` those of error details; `i18n.rs` formats it with `fluent-bundle`'s concurrent bundle, parsed on first use. Fluent gives each language its CLDR plural rules: Ukrainian has three forms, Polish three that differ from Ukrainian's at 21 and 12–14, Japanese, Korean and Chinese one, French and Portuguese count 0 as singular, and Russian the same three as Ukrainian. Isolation marks are off: no catalogue is right to left, and they would show as stray characters.

- A run fixes its language once, before its first window, from `ui-language`; `auto` reads `sys-locale` and matches the language subtag. Chinese also reads the script, then the region: `zh-Hant`, `zh-TW`, `zh-HK` and `zh-MO` take the traditional catalogue, `zh-Hans-HK` and the rest the simplified. Tests never set it, so they run in English.
- A message missing from a catalogue, or one that fails to format, falls back to English and logs a warning; a message missing from English shows its id and logs an error. Tests keep both from happening: every catalogue parses, holds English's messages with English's variables, and formats each message at the counts 0, 1, 2, 5, 21 and 22; every id `strings.rs` or `error_text.rs` names is in English, and every English message is named.
- A message with no arguments borrows from its catalogue: the bundle is `'static`, so the text reaches GPUI as a static `SharedString` without a copy.
- The crates below the app name no language for text the windows show. `merge-view`'s pane headings carry a `Role`, which the caller names: `strings::role` in the app, `Role::english` in the view's tests. `merge-session`'s `diff` sides carry a `LeftAt`, the revision and whether the file exists there, which `strings::revision_title` names.
- `error_text::describe` names an error a window shows, and its sources, in the run's language. It downcasts each `std::error::Error` to a type triad defines and gives each variant a message; `merge-session` hands the window the typed `SaveError` and `EncodeError` of a failed save, never a string. In English each message reads as the error's `Display`, which a test checks for each kind of message. A system error reads by its kind; an `io::Error` that wraps another error reads as that error. Text from jj-lib, gix, GPUI and the system keeps its English, as does an error that marks a bug in triad, such as a label count, so a bug report reads the same everywhere.
- The macOS bundle lists every language in `CFBundleLocalizations` and sets `CFBundleAllowMixedLocalizations`, so AppKit's own text, such as the file dialogs and Enter Full Screen, follows the system's language. It follows the system, not `ui-language`. A test checks the list against `Locale::ALL`.
- gpui-component labels its own text through `rust-i18n`, whose catalogue lacks most of triad's languages. `i18n::set_locale` sets gpui-component's language to the run's tag and lends it triad's catalogues through `_rust_i18n_extend`, which rust-i18n 4 generates in the crate that holds a catalogue. gpui-component asks `ComponentText` first for `triad.<key>`, then its own catalogue. `strings::component_message` maps the keys triad's windows show to messages: the search panel's Replace and Replace All, a field's Clear, and Cut, Copy, Paste and Select All in a field's right-click menu. The hook is hidden from the docs; gpui-kit's exact pin keeps it, and a release that drops it fails the build. Tests open the search panel in Japanese and Polish and read the buttons.
- Prompt buttons are built as `PromptButton::ok` and `PromptButton::cancel` by kind. GPUI's conversion from `&str` decides the kind from the English words OK and Cancel.
- The toolbar shows its labels from `LABELS_MIN_WIDTH`, measured with the English labels. The threshold adds the width of the labels and the reference badge in the run's language, takes away their English width, and rounds up to a whole pixel, as a window's width is: Spanish moves it out to 1131 px, Japanese in to 894 px.
- The open dialog's slot names take a column as wide as the longest in the run's language, 52 px at least: Spanish's Izquierda and Russian's Результат need 65 px.
- The language belongs to the process, and `merge_app::set_language` fixes it once. nextest runs each test in a process of its own, so `tests/languages.rs` opens merge and About windows in Spanish and Japanese, the two that sit either side of English's toolbar width, and checks the headings, status bar, badge, label threshold, the About window's lines and, in Spanish and Russian, the open dialog's slot names. GPUI's test window keeps no title to read.
- English stays where text is read by scripts or matches the command line: CLI output, `inspect` JSON, config errors, and the absorb trail, which prints as `absorb --list` does.

Adding a language: copy `en.ftl` to `<tag>.ftl` and translate it, add the tag to `Locale` in `i18n.rs` and to `UiLanguage` in `merge_config::settings`, add the language subtag to `Locale::of_system`, and add Apple's name for it to `CFBundleLocalizations` in `script/macos-app`. The catalogue tests then check it.

#### E3.8 Appearance and save keys

Spec §6.4 lists the keys. The default colours are `tones.rs`'s hues as `#rrggbb`: HSL saturation 0.7 and lightness 0.45 in the light theme, 0.65 in the dark; the current region's 88° at 0.85/0.4 light and 0.75/0.55 dark. `merge-app::tones` builds a `Palette` from the settings when a window opens. `conflict-style` applies under git semantics, defaulting to git's `merge.conflictStyle` read through gix; `zdiff3` reads as `diff3`, since refinement is `refine-conflicts`'s job.

### E4 Overview ruler

A strip at the right edge of the result pane and the diff window's right pane marks every region not `Unchanged` (spec §5). `merge_view::ruler` places marks by result lines, since the strip stands beside the result; `painted` merges them per pixel (§A6.7). Both copies of each held duplicate come after the regions' marks as `Style::Duplicate`, so they paint over them; the strip draws one as a conflict-tone fill with the copy's tone inset 2 px. Under soft wrap the marks are approximate. The strip is the pane's vertical scrollbar: the thumb is the view's share of the content, at least 16 px. The drag state lives in the window, since the strip is rebuilt each frame, and mouse handlers are window-wide so a drag goes on past it.

#### E4.4 Scrollbars

`scrollbars` (`always`, `hover`, `when-scrolling`) maps to gpui-base's `ScrollbarMode`. Each editor gets its bars through `EditorState::set_scrollbar_renderer`; the result and the diff's right pane paint only the sideways bar and feed their vertical scroll to the ruler in the same frame. gpui-base times a bar's fade with `Instant::now`, which tests cannot advance, so tests check visibility through clicks. The top panes' sideways scroll is `Sideways`, not a scroll handle, so they have no sideways bar.

### E5 Output line ending

`output-line-ending` (`keep`, `base`, `lf`, `crlf`; spec §4.1) converts at save, so the result pane keeps source bytes and picks stay exact. `keep` is the default so git's byte-for-byte output holds. `MergeDocument` counts the result's `\n` and `\r\n` endings as each patch lands, so the status bar's `mixed endings` check costs only the unresolved conflicts' markers and bases. `saved_endings` gives the endings under a setting without building the output; `output_with` builds and converts it.

### E6 Save Copy As

⇧⌘S writes the output as Save & Exit would, through the same encoder, and leaves OUTPUT, the exit code and the journal alone. p4merge's Save As retargets the window; triad writes a copy because git and jj read OUTPUT, so the window must never lose it.

### E7 Linux and Windows

#### E7.1 Linux

`script/linux` runs an Ubuntu 22.04 container in OrbStack, x64 under Rosetta by default, ARM64 natively with `TRIAD_LINUX_ARCH=aarch64`; each architecture has its own image and volumes. It holds glibc 2.35, Mesa's software renderer, sway without a display for Wayland, and Xwayland for X11. Sway stands in for GNOME; timings there say nothing about spec §9.

- `script/package` builds the AppImage and `.tar.gz` there, natively for the container's architecture, so the binary needs glibc 2.35 at most. `release.yml` builds ARM64 on GitHub's `ubuntu-24.04-arm` runner. Both bundle `libxkbcommon`, `libxkbcommon-x11` and `libxcb-xkb` by rpath; `libxcb` stays the system's. CI's `package` job tests the release build and smoke-tests the AppImage with `APPIMAGE_EXTRACT_AND_RUN=1`. appimagetool 1.9.1 and the type-2 runtime are pinned by digest for each architecture.
- The `linux-emulated` nextest profile leaves out `forward::a_missing_gui_binary_is_an_error`: Rosetta's `posix_spawn` reports success for a missing file.
- `script/linux/smoke` drives the real window on Wayland and X11. X11 runs on GL: Mesa 23's software Vulkan shows nothing in a 32-bit window.
- A desktop check runs in a VM: Ubuntu 24.04 ARM64 under VMware Fusion on Apple silicon, GNOME on Wayland at scale 1, Mesa's lavapipe for Vulkan, since Fusion's virtual GPU has no Vulkan driver. GNOME offers no server-side decorations, so gpui-component draws the frame there (`decorations`, `STYLE.md` § Window sizes). Timings there say nothing about spec §9.
- `merge_app::menu_bar` draws gpui-component's `TitleBar` over the merge and diff windows: on macOS it holds the toolbar, on Linux and Windows the mark and `AppMenuBar`. `merge_app::prompt_view` replaces GPUI's fallback prompt on Linux; `run` installs it, since headless tests answer the test platform's prompts.

#### E7.2 Windows

The console and GUI binaries forward as §A4.13 gives. `script/windows-app.ps1` builds both with the build line `script/build-id` names; `script/platform.sh` runs the gate scripts under Git Bash (§A13.5).

### E9 Key bindings

`keybind = CHORD=ACTION`, in Ghostty's form; spec §6.3 gives the rules. `merge_config::keybind` parses and holds the bindings; `merge_app::keys` turns them into GPUI's.

- **Per-window replacement.** A later line replaces a chord only in the windows that have both commands: `cmd+s=copy-as-patch` gives the diff window Copy as Patch and leaves the merge window's Save.
- **`cmd` and `super`.** Each is a bad line on the other's platforms. A shared config keeps its chords in an optional include per platform.
- **Unbound chords** bind to GPUI's `NoAction` with no context, which also stops the editors' own bindings.
- **Menu chord.** macOS shows an action's first binding, so `keys::bind` registers the newest first.
- **⌘Q is Save & Exit.** No default chord closes a window with unsaved work without asking: ⌘Q saves, ⌘W asks. A tool window has nothing to save, so Save & Exit binds there too and closes it, as Cancel does.
- **`show-config`** prints `clear` first, so the output reads back as the same bindings, and after a `clear` lists each dropped default as `CHORD=unbind`.

## F. Syntax highlighting

### F1 Goal

- Every pane colours its text by syntax; bands draw under the colours, intra-line marks over them (spec §5).
- Grammars: Rust, Go, C#, Python, Swift, Java, Kotlin, HTML, JavaScript, TypeScript, TSX, JSON, YAML, TOML, CSS, C, C++, Markdown, Bash, Zig.
- The window never waits for a parse.

Not covered: Razor, which has no grammar that covers its code blocks; styles inside JavaScript templates tagged `sql` or `graphql`, whose grammars triad does not build.

### F2 What gpui-component gives

- Grammars as cargo features through `gpui-kit`, each building its C sources. JSON comes with the base `tree-sitter` feature. `LanguageRegistry::register` takes grammars from other crates.
- C# and Swift come without a highlight query, and Kotlin with one that names nodes its grammar lacks and does not compile; triad registers the three again with their grammar crates' queries. TSX comes with only the lines TypeScript adds to JavaScript's query; triad registers it with gpui-component's TypeScript query and injections. C++ comes with only the lines `tree-sitter-cpp` adds to C's query; triad registers it with C's query followed by C++'s. `merge_syntax::language::Language::highlight_query` holds the C#, Swift, Kotlin and C++ queries; `merge-app` registers them with the editors' registry and builds TSX's from the registry's TypeScript entry. `.h` takes C++, whose grammar parses C headers too, as in Zed.
- The editors' query puts locals before highlights, so an unstyled local capture (`@local.reference` on a JavaScript identifier) draws plain. The top panes take the same query.
- The editor's tree-sitter highlighter is crate-private: it parses within 2 ms on the UI thread, else in the background after 150 ms, and always in the background over 256 KiB. `SyntaxHighlighter` is public and `Send`.
- `HighlightTheme::default_light()` and `default_dark()` resolve capture names to colours.

### F3 The edit observer

An editor holds one highlighter, and gpui-component's tree-sitter highlighter is crate-private, so triad can neither wrap it nor record edits through the highlighter slot. The vendored `EditorState::set_edit_observer` gives the edit recorder every edit the editor reports to its highlighter, in the same order and at the same moment, and leaves the slot to the highlighter.

### F4 The top panes

The top panes are rows triad draws, and their texts never change.

- Each input is cut into chunks of about 2 000 lines (`CHUNK_LINES`, `merge_syntax::syntax::chunk_starts`). A chunk ends before the first top-level line at or after its 2 000th: a line at column 0, after a blank line, not starting with `}`, `)`, `]` or `</`; without one, at its 2 000th line. A chunk's spans equal those of the whole parse unless a construct crosses a forced cut. A long HTML document is one element and a long JSON document one value, so the first closing tags or brackets after each cut draw plain.
- A chunk parses alone, off the UI thread, when a row of it shows. A row on screen queues its chunk and one on each side; the nearest parses first; each input runs at most two parses at once; a queued chunk more than two from the screen drops when the rows move. Each input keeps 16 chunks' spans, dropping the farthest. A failed chunk draws plain and does not parse again.
- Each line keeps capture spans: byte ranges with up to three captures, outermost first. Overlaps follow gpui-component: of two captures of one node in a row the first wins, and a capture an earlier match gave the same bytes counts once, since a pattern with several captures under one node matches once per combination of them; a capture paints over those starting before it; an injection parses one level deep when its language is built in (Rust macros, HTML scripts and styles, JavaScript `html`, `css`, `json` and `yaml` templates, Markdown's inline text, HTML blocks and fences).
- A Markdown fence names its language in its info string. The name resolves as gpui-component's registry resolves it, aliases such as `rs` and `py` included. Each language a fence may name builds its grammar the first time a parse meets it, off the UI thread, and reserves 1 024 capture indexes: Markdown's grammar builds in 5 ms, against 214 ms with all 17 others built at once (release build).
- A combined injection, Markdown's inline text, parses the ranges of all its matches as one text. As in the editors, inline Markdown leaves out the ranges with no markup byte and keeps the bytes between the rest.

#### F4.1 The editors past `syntax-whole-file-max-lines`

Below `syntax-whole-file-max-lines` (20 000) gpui-component's highlighter parses the editor whole. Above it, triad's (`editor_syntax.rs`) cuts the rope by §F4's rule in one pass, off the UI thread, and parses chunks as the editor draws them. An edit merges the chunks it touches into one with a new id; a late parse of an old chunk drops; a chunk past twice `CHUNK_LINES` cuts again; an edit whose rows do not match the chunks cuts afresh. The editor's first update carries an edit, so a rebuilt highlighter takes the whole text as a fresh cut.

### F5 Language detection

`merge_syntax::language::Language::of_run` holds spec §4.3's order of names: the inputs', then the output's, then the title. git and jj pass temporary inputs (`BASE_1234.rs`), so the fallbacks matter. A name matches by a whole file name first, in its own case (`.bashrc`, `PKGBUILD`), then by its extension in any case. git names a dotfile's temporary inputs `.bashrc_BASE_1234`, which match nothing, so the output's name gives the language. The first match wins. Without a name, `Language::of_shebang` reads the first line of each side, then each base: a script without an extension, such as a git hook, names its interpreter there. A name wins over the first line, since an extension is the stronger sign. `merge_session::language` holds the order once, for the merge's syntax boundaries and the merge window's colours alike; the diff window applies it to its two texts.

### F6 Measurements

#### F6.2 Contrast at the defaults

gpui-component's light comment colour `#007fff` is 3.83:1 on white and 2.8–3.4 on the light bands. triad sets its own light colours (`syntax_colours.rs`): comments `#0066cc` (5.57:1 on white, 4.11 on the worst band) and attributes `#7d6629` (5.52 / 4.07). The dark theme keeps gpui-component's colours, which give the variable captures no colour. A test holds every capture of both themes at 3:1 on every default band.

#### F6.3 Measured cost

A full parse costs about 90 bytes per byte of text: one 3.2 MB file of real Rust takes about 290 MB, and full parses of three 100k-line inputs reached 1 GB. Colouring costs about 31 µs per generated line and 10 µs per line of real Rust, parse, query and paint together. Hence chunks on screen only (§F4) and whole-file editors only up to 20 000 lines (§F4.1).

`script/measure-app 120 rs`, 2026-11-04. The `rs` run names the output `out.rs`, so all four texts parse as Rust; the generated lines parse with error nodes.

| | 100k lines | 1M lines |
|---|---|---|
| First frame | 217–257 ms | 762–780 ms |
| Scrolling the result, frame p95 | 8.4 ms | 8.5 ms |
| Keystroke / pick, max | 0.6 / 0.5 ms | 5.3 / 5.2 ms |
| Peak resident memory | 208–263 MB | 911–934 MB |

The editors parse inline Markdown as one text from the first range with markup to the last, so a block line between two, such as a link reference definition, takes inline colours from text a chunk of the top panes may not hold. There the two can differ; whole, they match.

The rows on screen colour 110–185 ms after the first frame at any length; a chunk scrolled to parses in about 60 ms. Spans take about 12 bytes per capture run: the top panes stay within about 50 MB of plain text at 1M lines, the chunked result within 70 MB.

### F9 Decisions

1. Razor is deferred. Further grammars come one per commit (PLAN.md §2).
2. The edit recorder moves to a gpui-base edit observer (§F3).
3. The top panes keep chunked capture spans; the editors highlight whole files up to `syntax-whole-file-max-lines` (owner, 2026-11-03).
4. triad sets its own light colours for captures under 3:1 on a band (owner, 2026-11-03).
5. The top panes parse only the chunks on screen and their neighbours, and keep 16 per input (owner, 2026-11-04).
6. Past `syntax-whole-file-max-lines` the editors parse a chunk at a time with triad's highlighter (owner, 2026-11-04).

## I. jj-native features

Features git cannot have (spec §1.2, §7.5). Each rests on something jj records and git does not:

| Command | jj fact it rests on |
|---|---|
| `stack`, `resolve -r REV PATH` | Conflicts in commits; descendants rebase on a rewrite |
| `history PATH`, `diff PATH --op OP` | The operation log's working-copy snapshots |
| `absorb` | Annotation; descendants rebase on a rewrite |
| `evolog`, `diff --since`, earlier resolutions | The predecessors each operation records for the commits it rewrites |

Each ships a command line with `--json`, `--headless` or `--inspect` for scripts, tests and agents. A command that fits a window triad has opens it; `stack`, `history` and `absorb` have windows of their own (§I2, §I3, §I4). In a git repository each refuses with `needs a jj repository` and exits with 3.

### I1 Rewrites

- `vcs-jj` holds them in its own API on `JjRepository` (`native.rs`), outside the `vcs::Repository` trait: git has no implementation to give. `merge-session::jj_native` opens the repository and refuses git with `ResolveError::NeedsJj`.
- `JjRepository::rewrite` is the one path every write takes. It snapshots the working copy, loads the workspace with the user's config, and reads the head repository through that workspace: the checkout compares trees by their store's address, so a repository from a second load panics in jj-core. The closure rewrites in one transaction and names its target and the commits it rewrote. `rebase_descendants` then moves the rest. A transaction with no changes commits nothing, so a save that reads back as the held conflict writes no operation.
- The report (`Rewrite::rewritten`) pairs each rewritten commit other than the target, and each descendant, with what the operation made of it, so each shows its conflicted paths before and after. A commit still visible is itself; otherwise its successor is the visible commit of its change whose predecessors in the operation lead back to it, so each copy of a divergent change maps to its own rewrite. No such commit means the operation abandoned it. jj-lib records predecessors in the operation and exposes them only on a written one: the rewrite writes its transaction, publishes it on a write and leaves it unpublished on a dry run. A dry run so adds an operation, its view and index to the stores, unreferenced, as it adds commits and trees. For `resolve -r` the list is the strict descendants; for `absorb` it starts with the commits the hunks went into, so a destination left conflicted shows.
- A moved `@` is checked out with `Workspace::check_out`, against the tree the snapshot recorded. The checkout follows the published operation, so a checkout that fails leaves the rewrite in place: `Rewrite::stale` reports it, the reports name `jj workspace update-stale`, and the error goes to the log. In a colocated repository `git::reset_head` moves git's `HEAD` to the new `@-` before the operation commits, and the refs export, as jj does after a command.
- Immutability follows jj's CLI: jj-lib has no `immutable()`. `DEFAULT_ALIASES` carries jj's default `revset-aliases`, less `builtin_log()`, which only `jj log` reads: `trunk()`, `builtin_immutable_heads()`, `immutable_heads()`, `immutable()`, `mutable()`, `visible()` and `hidden()`. The user's `revset-aliases` table replaces any of them. `mine()` matches the user's `user.email`, as in jj: jj's documented `immutable_heads()` that freezes other people's commits names it. A command reads the aliases and the email once, as `Revsets`, and every revset it evaluates takes them. jj-cli is no dependency, so its defaults come through an oracle (§I5). The target is checked before the snapshot, so a refusal writes no operation, and every rewritten commit again inside the transaction.

### I2 Conflicts in commits

- `stack` snapshots the working copy, as a jj command does, then evaluates the revset with those aliases. The list keeps the order of jj's revset stream, children first, as `jj log` and the history window show. A path is inherited when any parent's tree holds a conflict at it. `native::link_origins` then gives each inherited path its `Origin`: it walks the list oldest first, so a parent's origins are set before its children's, and takes the first parent in the list that holds the path, through that parent's own origin. A chain that leaves the list gives `Origin::Unlisted`: the conflict arises in a commit the revset leaves out, such as an immutable one, and the child is the place to resolve it. A path is `untouched` when one parent's tree holds the same `MergedTreeValue` at it as the commit's: the commit left the conflict as it came. The stack window hands the merge window a `WindowSettings::notice` only for `Origin::Listed`; the merge window shows it as a warning banner above its panes, as it shows a changed OUTPUT, since the stack window sits behind it.
- `load_at_commit` reads each parent's conflict at the path (`JjRepository::parent_conflicts_at`) into `Loaded::parents`; a parent's conflict that does not decode is logged and left out, since it only marks regions. The merge window takes them as `WindowSettings::parents` and runs `merge_view::inherited::Inherited::find` once at open: it aligns each parent of the merge's arity with the document's options and keys each conflict region by its bases' and sides' text. A conflict region of the document with a key the parents hold is inherited. Regions are fixed for the window's life, so the set never goes stale. Keys are text, so lines the commit added above a conflict leave it inherited, and an edit inside it makes it the commit's own. `Tone::Inherited` replaces `Tone::Conflict` for an inherited region's `!` marks, borders and ruler mark (`ruler::Style::Inherited`). Its blocks keep their side tones; `Band::inherited` makes `Tones::inherited_band` blend each 60% toward the inherited colour in RGB, so the side stays readable and the block reads apart from the commit's own conflicts. The top panes blend the same way. The `inherited` pill sits at the right of the first row of each unresolved inherited region: in each side's top pane in the run of that row's move pills (`row_pills`), and in the result placed from the editor's display rows as the diff window places its move pills. Both draw through `MergeWindow::inherited_pill` and `move_pill::label_pill`.
- The stack window (`merge-app::stack_window`) lists through `jj_native::stacked`, which also gives the repository root the paths are relative to. It takes over the app's `Finish`, as the resolve queue does, so a merge window's end comes back to it: the window closes, the note says how it ended, and the list loads again. `merge-cli` lists once before the window opens, so an empty list, a git repository or a bad revset exits without one; the listing travels in `WindowForm::listing`, and the window shows it first and lists again only on a refresh or after a merge window. Listing, loading a path and the preview run off the UI thread; each opens the repository afresh, so a save between them is always seen. Preview takes `MergeWindow::preview_job`, the same dry run the save's confirmation runs.
- A listing snapshots, so it writes. Each of the three windows refuses a close while a listing, a load or Apply runs off the UI thread: it sets `closing`, and the job's handler ends the run when the job lands, opening no window and logging an error that no one will see. A process that ends mid-job could leave the working copy stale between jj's operation and its checkout.
- The directory, stack, history and absorb windows share `merge-app::tool_window`, after the panels of Zed's git and project panels: a 32 px header with a muted summary at the left and outlined text buttons from `merge-app::button` at the right, the one button that writes its default action, a muted status line under it, the lists, and a footer for a result such as Preview Save's. The root holds the `ToolWindow` key context: Cancel binds there as well as in the merge and diff windows (`keys::Contexts`), and Cancel or a list's `Leave` (Escape) runs the window's own `close_requested`, the check its close button runs, before `remove_window`.
- The tool windows build their lists from `merge-app::pick_list`, after Zed's `ListItem`, `ListHeader` (`crates/ui/src/components/list/`) and git panel entries (`crates/git_ui/src/git_panel.rs`): gpui-component's `ListItem` restyled to 24 px rows inset 4 px in the list, an optional status icon, a muted lead and trail beside text that ends in an ellipsis, and a semibold heading. A path shows its file name, then its directory muted and cut first; a deleted one is struck through. Status icons are Zed's squares in the theme's status colours: added `success`, modified `warning`, renamed `info`, deleted and conflicted `danger`. Each list is a focusable `ListBox` with its own key context, so ↑, ↓, Enter and Escape act on the focused list only; a selection stays inside the list (`pick_list::next`, `previous`) and scrolls into view through a `ScrollHandle`, which counts the list's direct children, so a heading and the rows under it are siblings. A row's accessible name is its parts joined, the line `--list` prints, or a label that says the icon in words. The absorb window has two lists, hunks and commits; Enter and Escape move the focus between them.
- A tree holds every path at the tree's number of sides; a path that conflicts in fewer carries pairs of terms that cancel. `native::simplified` drops them as jj does for a conflict of files, keyed by file ID so an executable bit alone keeps no term, and keeps the labels of the terms that stay. `conflict_in`, and so every read of a conflict, `@`'s included, goes through it: the sides are jj's, and markers triad writes have the arity `update_from_content` parses, which simplifies the same way. Without it a path of two sides in a tree of three opens with a phantom third side, and its markers save as a resolved file with the markers in it.
- `resolve -r` loads through `resolve::load_at_commit`: the commit's conflict, decoded as `load` decodes `@`'s, with jj's marker length from `merge_core::jj::marker_length`. Its `Destination::Commit` carries an `InCommit`: the commit and the marker length. A save encodes the result and hands the bytes to `InCommit::write`; no file holds them. A refusal is `SaveError::Commit`, a failed write that Try Again repeats. Both front ends save through `Destination`, so the window and `--headless` write the same way.
- The write parses the result back with jj-lib's `update_from_content` over the path's file IDs, which returns the terms it was given, so a conflict keeps each term's bit; a result of another arity is refused (`JjError::SidesChanged`). A resolved file takes the executable bit the terms agree on (`resolve_file_executable`).
- The merge window checks a save into a commit before it writes: `Session::preview` hands a `PreviewJob`, the document's snapshot and the record's `InCommit`, to a background thread, which encodes the output as the save does and runs `InCommit::write` as a dry run over those bytes. The result stays locked meanwhile, so the save writes what the preview read. The window asks only when a descendant's `before` and `after` differ; a refused dry run logs a warning and goes on to the save, which reports the refusal.
- `InCommit` keeps the commit triad read and the commit the path must still match: the commit read, then the commit each save wrote (`Rewrite::latest`). Every copy of it shares the second, behind a mutex, so a save run on a copy moves it for all. A write takes the matched commit while it is visible, else its change's one visible commit (`native::current`): a commit of a divergent change is written in place, and the other copy stays. A hidden commit of a divergent change is `AmbiguousRevision`. The write compares the path's simplified value there with the one in the matched commit, and the executable bit a resolved file would take from every term, cancelled ones included (`held_conflict`); a difference refuses the write (`JjError::ConflictChanged`). The comparison is of the path alone, so a snapshot that rewrites the commit with other files takes the save. Saved markers parse against the terms of the commit read, since triad's markers follow them; before the first save, against the change's own terms, as `jj resolve -r` reads them. A save that brings back a conflict an earlier save resolved writes into the resolved tree widened to the conflict's sides, with the labels of the commit read.
- The merge window's conflict badge counts the sides of the current conflict with `merge3::simplified_sides`: jj-core's `Merge::simplify` over the region's comparison keys, so it follows the whitespace mode as `classify` does. `nav::conflict_sides` gives it only for a merge of more than two sides.
- `Rewrite::sides` is the path's number of sides after the rewrite, simplified, and `Rewrite::resolved` is about the path: a commit may hold other conflicts.

### I3 A file's history in the operation log

- `JjRepository::history` walks the operation log from the head with `walk_ancestors`, lazily, and reads each operation's view for the workspace's working-copy commit. It reads the path's value from that commit's tree only when the commit differs from the previous operation's, so a run of operations on one commit costs one tree lookup. It loads no repository per operation: the view and the store suffice.
- Versions compare by `MergedTreeValue`: file IDs, not bytes. A run of equal values keeps the oldest operation, since the walk is newest first and each equal value renames the run. `--limit N` stops at the first value of version N + 1, so version N still carries the operation where it appeared.
- Cost: on this repository's log of 931 operations, `history docs/PLAN.md` takes 43.1 ms in a release build, 16.0 ms with `--limit 5`; the 16 ms is start-up and the snapshot. The walk costs about 29 µs per operation, so a log of 100 000 operations takes about 3 s without a limit.
- The history window (`merge-app::history_window`) follows the stack window's design (§I2): it lists through `jj_native::file_history`, which also gives the repository root, takes over the app's `Finish` so a diff window's close comes back to it, and lists again then. A version opens through the same `native::load_at_operation` and `run::native_sides` as `diff --op`. Listing and loading run off the UI thread, each on a fresh repository; a listing snapshots, so a save from the diff window shows as a new version. `merge-cli` lists the versions before the window opens, so a git repository or a path outside the repository exits without one; the window shows that listing first.
- `diff --op` resolves the operation with `resolve_op_with_repo`, takes the working-copy commit from its view, and hands the commit's full ID to the `diff PATH` path as the revision. A commit ID resolves in jj whether or not the commit is still visible, so the line origins and revision chip work unchanged.

### I4 Absorb by hunk

- `JjRepository::absorb` follows jj-lib's `split_hunks_to_trees`, which gives no per-hunk control, then hands the result to jj-lib's `absorb_hunks`. It diffs the working copy's tree against its parents' merged tree, annotates each changed file's parent text with `FileAnnotator` over `(INTO) & mutable() & ::parents`, and splits the file with `ContentDiff::by_line`. Hunk ranges are byte ranges; the report counts newlines for line numbers.
- The rule for a hunk is jj's for one that is not a pure deletion: the first compacted annotation range that ends at or past the hunk must start at or before it, and an insertion at the seam of two ranges stays. A pure deletion takes the same rule, so a deletion over two commits' lines stays where jj would split it: each piece deletes a line next to the lines the other commit wrote, jj's merge conflicts on adjacent changes, and `jj absorb` 0.47.1 leaves both commits conflicted.
- A line no commit in `(INTO) & mutable() & ::parents` wrote is annotated to the working-copy commit when a commit outside the domain sits between them. The plan drops those ranges with the domain's misses, so such a line has no commit to go into. An immutable commit `--into` names is outside the domain: its lines stay, and a listing never names a commit the write would refuse.
- The annotation rule sees only lines. A later commit changes the target's file next to a hunk without leaving a line the annotation can attribute: an insertion by a commit outside the domain at the end of the target's lines, or a deletion. jj's merge of the hunk into the target then conflicts, and `jj absorb` 0.47.1 writes the conflict. So each hunk the annotation places is also merged on trial, as `absorb_hunks` merges it: jj-lib's `files::try_merge` with the store's merge options over the target's file, the parents' file and the parents' file with the hunk applied. A hunk whose trial conflicts stays. Each target's file is read once per file. A `--move` choice skips the trial: the user asked for that commit.
- Each `AbsorbHunk` says where it goes: `Goes::Into` a commit, or `Goes::Stays` with a `Stay` reason: `NoCommit` when the annotation names no one commit, `WouldConflict` with the commit whose trial merge conflicts (`merges_cleanly`: the commit's file, the parents' file, and the parents' file with the hunk and every earlier hunk of the file accepted for that commit, as `absorb_hunks` merges them; diff alignment can make two hunks conflict together that merge alone), `Kept` for `--keep`. `merge_session::jj_native::stays` gives the reason's text to the listing and the window. Each `AbsorbHunk` carries its bytes before and after, and `Absorbed::destinations` the commits `--into` allows with their full IDs, children first: what the absorb window shows and sends.
- The absorb window (`merge-app::absorb_window`) holds one choice per hunk, an index into the destinations or none to keep it. Apply sends every hunk's choice explicitly, a full commit ID for a move, with `--listed-at` the listing's operation, so the write uses the numbers the window showed or refuses. It runs off the UI thread; a success lists again, a refusal keeps the choices. Choosing is by click: a hunk, then a commit. `merge-cli` lists once before the window opens, so a run with no hunk, a git repository or a bad `--into` exits without one; the window shows that listing first.
- Choices rewrite the plan before any tree is built. Each target commit gets the parents' tree with its hunks spliced into each file; `absorb_hunks` merges that over the parents' tree into the target, rewrites descendants in topological order and abandons an emptied working copy.
- Listing and `--apply` run the same code inside `JjRepository::rewrite`: a listing is a dry run, which snapshots and drops the transaction. A listing builds no trees and writes no file to the store; only `--apply` does. The listing reports the head operation after its snapshot. `--listed-at OP` is checked inside the rewrite, after the snapshot, against the head: hunk numbers come from a diff of the working copy, so any operation since the listing, an edit snapshotted or a jj command, may renumber them. The command line asks for it whenever `--apply` carries choices; `--apply` alone recomputes the plan and needs none. An abandoned working copy leaves its change with no visible commit; the rewrite then counts the target as resolved.

### I6 Versions of a change

- `JjRepository::evolog` (`evolog.rs`) snapshots, resolves one commit with the user's aliases, then runs jj-lib's `evolution::walk_predecessors` from it, as `jj evolog` does: newest first, a version with several predecessors, such as a squash, followed by each of them. A hidden commit resolves by its commit ID, so a walk can start from an earlier version. `--limit` cuts the stream before any further commit is read. The walk stops at the first operation that stores no predecessors; jj-lib emits the commits it still holds with no operation.
- `JjRepository::interdiff` resolves both commits after one snapshot and keeps only their IDs, short forms and whether the parents differ. `interdiff_paths` and `interdiff_sides` each rebuild the trees from the IDs: jj-lib's `rewrite::rebase_to_dest_parent` merges the old tree's change onto the new commit's parents' tree, and returns the old tree as it is when the parents are the same. A side holding a conflict of files is written with jj's markers in the user's `ui.conflict-marker-style`, with the rebase's labels, as `jj diff` shows it; any other conflict is `InterdiffSide::Other`.
- `merge_session::native::interdiff` lists the paths whose bytes differ and pairs renames and copies with `dir_diff::with_rewrites`, as `diff --rev` lists the working copy's changes, so both listings pair files alike.
- The windows list before they open. `merge-cli` reads the evolog, the interdiff listing or the one file of `diff PATH --since`, so a git repository or a bad revision exits 3 with no window, and the window shows the listing at once. The evolog window lists again only on Refresh.
- The evolog window opens a version by listing its interdiff off the UI thread; an empty listing opens nothing and says so, and a listing with files opens the directory window with that listing as its first. The directory window's `Source::Interdiff` keeps the root and the `Interdiff` it listed, and loads a file's two sides from them, so a file opens the versions the list shows even after a later rewrite of the change. A renamed file reads its old path on the left. Both sides are read-only and carry no line origins: no file on disk holds either.
- The directory window takes over the app's `Finish` as it does for `diff --rev`, so the evolog window learns of its close through the directory window's exit callback alone and needs no `Finish` of its own.

### I7 Earlier resolutions

- `JjRepository::earlier_resolutions` walks the evolog from the commit read, without a snapshot: the load took one. It reads the path's `MergedTreeValue` in every version, one tree lookup each, and keeps the walk in memory with each version's predecessor IDs. A version holding a file whose predecessor holds a conflict resolves it; the run goes back through the first conflicted predecessor to its oldest. The predecessor links, not the walk's order, decide the pairs, so a squash's second line of predecessors pairs only within itself. Only the pairs' conflicts and files are read.
- The oldest version of the run holds the conflict as the rebase or merge made it, before any partial edit. A resolution spread over several snapshots so pairs with the first conflict, and its regions key as a later rebase brings them back.
- `merge_view::earlier::Earlier::find` merges each pair's conflict with the document's options and no syntax boundaries, as `Inherited::find` aligns a parent, and keys its regions the same way. The result pane shows an unresolved conflict as its sides' lines, and `MergeDocument::region_lines` gives each region's lines there, so no markers are written: a line diff of that text against the resolved file, with exact comparison, finds the neighbour lines on each side of the region. A neighbour inside a hunk gives the region no resolution. Keys of another arity have another length, so a pair of another shape matches nothing without a check of its own. A region's offers keep the pairs' order, newest first, each text once.
- `merge_session::jj_native::earlier_resolutions` decodes each pair's conflict as a parent's (`resolve::parent_inputs`) and its file as text; one that does not decode is logged and dropped, since it only loses an offer.
- `Destination::earlier_source` names where a loaded conflict looks: the commit `resolve -r` read, or for a working-copy conflict the working-copy commit at search time, read from the view without a snapshot, so a search never races a save's snapshot. A git conflict has none.
- A take is one `Command::Edit` whose patches replace each region's bytes in the result with the resolution's text, so the region turns edited and one undo takes back the lot. `Earlier::pending` leaves out regions already resolved.
- jj rebases a change's diff, so a conflict a rebase makes after a resolution holds the resolution as a side and its key differs from every earlier conflict's. Matches come from a change returned to a conflicted version: `jj restore --from`, or a squash or split that brings an earlier tree back.

### I5 Oracles from jj's CLI

`vcs-jj` re-implements three pieces of jj-cli: the end of a transaction (§I1), the default revset aliases (§I1) and absorb's split into hunks (§I4). `merge_core::rename` re-implements the rename pass of jj's git backend (§A6.9). jj-cli is no dependency, so its behaviour is checked against corpora that jj's own CLI writes.

- `script/gen-jj-cli-oracle` (the example `gen_jj_cli_oracle` in `vcs-jj`) runs the `jj` on `PATH` with a config of its own and refuses unless its version is the jj-lib version `Cargo.lock` pins. It writes `fixtures/oracle/jj-cli-aliases.json`, jj's default `revset-aliases` from `jj config list --include-defaults`; `fixtures/oracle/jj-cli-absorb.json`; `fixtures/oracle/jj-cli-resolve.json`; and `fixtures/oracle/jj-cli-renames.json`. Each records the jj version.
- Absorb cases are seeded random stacks: `base` on the root and one to three commits on it, over one to three files, with replaced, inserted and deleted lines, a line `}` shared across commits, files without a final newline, a file added in a commit, and a working copy that edits, deletes or adds files. Merge cases, from a second seed, put two branches on `base`, each editing its own file, then either a merge commit on both with the working copy on it, or the working copy on both. `--into` cases, from a third seed, are linear stacks absorbed into one or two commits named back from the working copy (`@--`, `@- | @---`). A case records each commit's parents, the working copy's parents, the `--into` given, each commit's tree after `jj absorb` and the commits it left conflicted. The corpus keeps, out of a pool of 200 of each kind with a hunk to absorb, 48 linear cases jj leaves resolved and 16 where it writes a conflict, and 16 and 4 of each other kind.
- `vcs-jj`'s `oracle` tests: both corpora carry the version `Cargo.lock` pins, so a jj-lib upgrade fails until the corpora are written again; `DEFAULT_ALIASES` equals jj's set, whitespace aside; and triad's `absorb --apply`, on the same history built through jj-lib with the same `--into`, leaves every commit and the working copy as jj does. Where jj writes a conflict, triad must leave no commit conflicted and the working copy as it was (§I4).
- Cost: an absorb case takes about 180 ms of a debug build's CPU. The cases of one test share one repository, each history on the root, and the 104 run in 52 tests of two; the nine resolve cases, each in its own repository, run in nine tests. Under the full suite's load on an Apple M4 Pro an absorb test takes 3.8–6.3 s and a resolve test 2.8–4.5 s, inside nextest's 30 s.
- Resolve cases are written by hand, one per path through `JjRepository::rewrite`: a conflict in a merge commit with a descendant, bookmarks on both and `@` on the descendant; an edit on disk not yet snapshotted; the target as `@-` and as `@`; markers that keep the conflict and markers equal to it; a tagged, so immutable, target with and without an edit on disk; an internal repository. jj writes each with `jj resolve -r` and a merge tool that copies a fixed result, with `merge-tool-edits-conflict-markers`. A case records every visible commit's parents and tree, the files on disk, git's `HEAD` and branches and tags, and the operations the write added. Conflicts compare in jj's `snapshot` marker style, and every marker line without its label, since labels name change IDs. The test builds the same repository through jj-lib, writes with `JjRepository::write_conflict`, and compares the whole record.
- triad departs from `jj resolve -r` in two places, each marked in the corpus and applied to jj's record before the comparison: a refusal of an immutable target comes before the snapshot and writes no operation (§I1), and a result that reads back as the held conflict writes nothing (spec §7.5.1). jj writes an operation in both. `absorb` departs twice: a hunk whose merge into its commit would conflict stays, reported as `stays: would conflict in CHANGE`, and the absorb corpus holds jj's conflicted results apart and checks that triad keeps those hunks; and an `--into` that names an immutable commit leaves that commit's hunks in the working copy, where jj refuses the whole absorb.
- Rename cases are seeded pairs of trees: `left` committed, `right` in the working copy on it, and the entries `jj diff`'s template names between them, each a status (`modified`, `added`, `removed`, `renamed`, `copied`), a source and a target. 160 random cases draw two to six files from four names in three directories, of one to eight short lines, half from 12 shared lines and half unique, so many pairs score near 50 %; a file repeats another's bytes, is empty or is binary now and then; each file is kept, edited, deleted or moved, with or without edits, and up to two files are added, half from another file's bytes. 16 tie cases, from a second seed, delete two files of the same bytes and add one copy with a line replaced, under the name of one of them, so the scores tie and the name decides. 48 copy cases, from a third seed, modify most files, delete or move a few, and add one to three files from a modified file's new or old bytes or a kept file's, edited or not, so copies from modified files stand beside files that are no source. The corpus holds 248 renames and 55 copies. `merge-session`'s `rename_oracle` test writes each case's trees as two directories and compares `dir_diff::changed_files` with jj's entries. Each of these changes fails it: no name preference among files of the same bytes (1 case), none on a tied score (7), added files walked by path in place of blob ID (3), a 60 % threshold (22), a copy source that pairs once (7), copies scored against a modified file's old bytes (8), and renamed files kept as copy candidates (4).
- Six changes to `rewrite` each fail the resolve test: no reset of git's `HEAD`, no export of refs, no checkout of a moved `@`, no snapshot first, no immutability check before the snapshot, and a rewrite for a result equal to the held conflict.
- Three changes to the plan each fail the absorb tests: no seam rule for an insertion (4 cases), no trial merge (14), and a range that ends at a hunk's end not holding it (27). Ignoring `--into` fails 12 of the 52 absorb tests, and reading the working copy's first parent in place of the merge of its parents fails 7.
