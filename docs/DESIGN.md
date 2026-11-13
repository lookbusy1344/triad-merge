# triad — design notes

The design of what is built: the reasons, the decisions and the current measurements. [SPEC.md](SPEC.md) sets the behaviour; [PLAN.md](PLAN.md) holds the status and the open work. A reference such as §B5.8 names part B, section 5.8. Section numbers are stable: code, scripts and docs cite them, so a section that goes leaves a gap.

Measurements come from an M4 Pro MacBook Pro (2025), macOS 27.2, 120 Hz display, release build with the `dist` profile's settings, triples from `merge_core::testing::generate` with seed `0x5EED`, unless a section says otherwise.

## A. Engine, view, windows and release

### A1 Rules

AGENTS.md holds the working rules. Two design rules sit here:

- **git and jj parity** (spec §1.2). Shared types follow jj's model: n sides, n − 1 bases, an optional label per term. git fills the same types. A fact only one VCS records is an optional field, never a separate code path in `merge-view` or `merge-app`.
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

`Text` holds the decoded content as `Arc<str>` and a line index; a clone shares both. `decode` works on bytes; the caller reads files and reports I/O errors. Lines end at `\n` only; a lone `\r` is a character inside a line, as in git and the result editor. A file is binary when a NUL appears in the first `BINARY_SNIFF_BYTES` (8000) bytes, git's rule; a UTF-16 file with a BOM is not binary. Errors carry byte offsets from the file start, BOM included.

#### A4.3 Diffing (`merge_core::diff`)

Line diffs run imara's indent heuristic, as git's diff does; the merge's diffs do not (§A4.4). Character diffs use Myers, since histogram does badly on small alphabets, and stop above `MAX_INTRALINE_DIFF_CHARS` (10 000). Whitespace modes rewrite only the interning key (`diff::line_key`): `Trailing` drops trailing spaces, tabs, `\r` and the `\n`; `Amount` also collapses runs; `All` removes all whitespace. Whitespace is space, tab and `\r`, as in git's `isspace`.

#### A4.4 Three-way alignment (`merge_core::merge3`)

N-sided throughout (spec §4.4, §7.4): `Inputs` holds n sides and n − 1 bases, and each `Region` one range per input. For a three-way merge, `OneSide(0)` is spec's `LeftOnly`, `OneSide(1)` `RightOnly`, `AllSame` `BothSame`.

Git semantics:

1. Diff base 0 against every other input, placing hunks as `git merge-file` does (spec §4.2): `diff_lines_for_merge`, with git's histogram in `histogram.rs`.
2. Merge hunks that overlap or touch in base 0 coordinates, and track each input's line delta to place its ranges.
3. Classify each cluster with jj-core's `trivial_merge` on the lines' comparison keys: `AllSame` when every side holds the same text, else `OneSide(i)`, else `Conflict`.
4. Refinement (on by default) is `git merge-file --zdiff3`: lines all sides share at the start and end of a conflict split off into `AllSame` regions with empty base ranges. The conflict keeps its whole base ranges, shared middle lines stay in it, and conflicts never join.

git's histogram matches `git diff --histogram --no-indent-heuristic`: anchor choice without prefix stripping, skipping lines commoner than the best anchor, the 64-occurrence limit and its Myers fallback, sliding, and the re-diff of a moved group. Part B gives jj semantics.

#### A4.5 Resolution, result text and output (`merge_core::document`)

`MergeDocument` owns everything a session changes: the inputs, the options, the regions and their resolutions, the result as a `ropey` rope, and the undo history. The core is the single source of truth. Every command, undo included, returns the minimal patches the GUI applies to its editor; the GUI never replaces the whole text. `Applied::regions_changed` tells the view to rebuild its region-indexed state.

Region offsets live in a Fenwick tree of region byte lengths, and a second one of region newline counts (`region_lines`), so an edit updates one entry and a lookup is a prefix search, both O(log n) in the regions. `MergeDocument::new` renders each region as a `Cow<str>` into a `RopeBuilder`; unchanged, one-side and single-pick regions borrow their input.

`History` holds one entry per command: the inverse patches and, when the command merged regions, the regions, resolutions and lengths it replaced. Consecutive single-patch insertions at the caret without `\n` merge into one entry until another command runs, as gpui-base's undo manager groups them. A multi-cursor batch is one entry.

#### A4.6 Edit model (`merge_core::document`)

`Command::Edit(EditBatch)` carries one user action. The rules are spec §4.5's. One rule needs its reason: a zero-width insertion after a byte other than `\n` joins the region holding that byte, and one at a line start goes to an empty region at that offset if there is one; without the first clause, typing `ab` into an empty region would put `b` in the next region. A typed newline takes `typed_ending`; since the editor has already made the edit, `apply` returns only the corrections, which the view applies inside its reentrancy guard.

#### A4.7 Two-way diff (`merge_core::diff2`)

`DiffDocument` holds two buffers, each a rope with its own history and read-only flag, and the regions with a version. `recompute` takes O(1) rope clones for a background diff; `install` takes the result only when its version still matches, which gives spec §4.6's debounce. `copy_region` recomputes first when the regions lag the text. `merge_core::edit` holds what both documents share: `TextPatch`, `EditBatch`, batch checks, inverse patches and line-ending fixes.

#### A4.8 Oracle corpus

`fixtures/oracle/corpus.jsonl` holds about 2,000 triples from `script/gen-oracle`: hand seeds, seeded mutations, and the first 200 lines of triad's own files at pinned commits. It needs git 2.54 or later and records `git merge-file -p --diff-algorithm=histogram` under `--diff3` and `--zdiff3`, with the marker size computed by spec §4.4's rule and passed as `--marker-size`, since git keeps the requested size. The exit code is the conflict count.

The test checks every case in both modes: a clean merge matches byte for byte; a conflicted one matches the conflict count and the text outside the markers.

libgit2's file-merge tests (`merge/files.c`, `merge/trees/whitespace.c`, `merge/trees/automerge.c`) give a case list, not data. libgit2 is GPL-2.0 and its linking exception does not cover its tests, so each scenario is a hand seed written from scratch, `seed-libgit2-<scenario>`, with git's output as the expected value. The jj corpus is §B5.2.

#### A4.9 Properties

Proptest suites for spec §8.2, with a fixed seed and checked-in regressions: alignment in `merge3`; output, two-way and edit sequences in `merge-core/tests/properties.rs`. Fuzzing waits for release 1 (spec §8.9): `cargo-fuzz` needs nightly.

#### A4.10 Headless CLI

`--headless`, `inspect --json` and `diff --inspect --json` (schema in `docs/json-schema.md`). Logging goes to stderr and a rotating file in the cache directory, filtered by `TRIAD_LOG`; `TRIAD_LOG_DIR` moves it, which the binary tests use. Each `fixtures/core/<case>/` holds `base`, `left`, `right`, the `expected` bytes, the `exit` code, and optionally `args` and `config`.

#### A4.11 Atomic write

`merge_session::io` writes a temporary file in the target's directory, copies the target's permissions when it exists, syncs, renames over the target, then on Unix syncs the directory. A new file gets the mode `std::fs::write` gives (`0o666` less the umask), not `tempfile`'s `0o600`. A failed directory sync is logged: the file already holds its bytes.

#### A4.12 Benchmarks

`cargo bench -p merge-core --bench merge`, on 10k, 100k and 1M lines:

- `decode`, `align`, `output`: no target; they show where `load` spends its time.
- `load`: decode three inputs and build the `MergeDocument`. Spec §9 load. `load-jj` and `align-jj` are jj mode (§B6).
- `load-word-small`, `load-word-large`: jj mode at the word level (§B6).
- `toggle`, `keystroke`: spec §9 take left/right and keystroke.

`script/check-bench` fails when a mean exceeds 120% of its spec §9 target. The `bench` workflow runs it weekly on `ubuntu-latest` with `TRIAD_BENCH_TOLERANCE_PERCENT=500`: that runner measured `load/1M` at 834–1401 ms over three runs of unchanged code, and the rayon-parallel word and bounded benches lose more on its 4 vCPUs, up to 3.1 times their target. The job catches gross regressions; the reference machines decide the targets. Memory peaks do not vary and keep their limits.

#### A4.13 Windows console binary

The console binary (spec §6.1) starts `triad-merge-gui.exe`, waits, and forwards its exit code: `merge_cli::forward` runs the child with inherited stdio; any other code, a signal or a failed start gives 3. The tests' child is the `forward_stand_in` example, which names its arguments on stderr and exits with `TRIAD_STAND_IN_EXIT`. The Windows CI targets check the real pair.

### A5 `merge-view`

A plain-Rust view model with no GPUI. Every lookup is by index or offset, in O(log n). Nothing builds a list of all rows: at 1M lines that is tens of MB, rebuilt after every merging edit.

- **Rows** (`rows::RowMap`): a Fenwick tree of rows per region over the visible `PaneSet`. Filler rows sit at the end of a region; a region every visible pane leaves empty has no rows. `update` rebuilds only when `Applied::regions_changed`.
- **Scroll mapping** (`scroll`): rows ↔ result lines through `RowMap::first_row` and `MergeDocument::region_bytes`. A position maps to the same region at the same line within it, held to the region's last row or line, and keeps its fraction.
- **Navigation** (`nav`): next, previous, first and last `Difference` or `Unresolved`, with no wrap-around. A difference is a region not `Unchanged`, so whitespace-only changes are skipped (spec §4.2). `nav::held_region` and `diff::held_region` are the one place a stored pin and caret become a region; each returns a region of the document whatever the inputs.
- **Highlights** (`highlight`): intra-line spans on demand, cached by region in an LRU of `INTRALINE_CACHE_REGIONS` (256), emptied when regions change. The diff window keys its cache by `DiffDocument::version`.
- **Status** (`status`): counts, labels, encoding, line ending and whitespace mode; counting visits every region once per command.

### A6 GUI (macOS first)

#### A6.1 Vendored editor

`vendor/gpui-base` is gpui-base 0.8.1 with triad's patch, through `[patch.crates-io]`, with Apache-2.0 notices; `PATCHES.md` lists every change. The main ones: full-line fills and gutter marks, the edit observer (§F3), spacer rows and their click event, hidden lines (§A6.5), line labels (§C5.4), one-pass line layout, a resting caret, and tab stops. The vendored crate sits outside the workspace; `script/checks` runs its unit tests.

Zed paints row highlights and diff hunk backgrounds as quads across the text area and hunk status as a gutter strip. triad does the same and adds a letter, since colour is never the only signal (spec §6.5).

gpui-component's `Input::render` copies the whole text each frame, but only with an accessibility client or `test-support`; it is not measured (§A6.7).

#### A6.2 Window and panes

- `MergeWindow` holds the session (`merge_session::session::Session`), the panes and the components below, and wires them to GPUI. Logic with its own state sits in components with unit tests: `PaneScroll`, `OriginColumn`, `input_find::Found`, `go_to_line::Field`, `Measurement`, `JournalTasks`. What stays in `window.rs` is action handlers, task spawns, editor updates and rendering.
- Top panes: a `uniform_list` reading rows from `RowMap`, with a hit target per region.
- Result pane: `EditorState` with folding and indent guides off, line numbers on, multi-cursor on. Soft wrap only up to `SOFT_WRAP_MAX_LINES` (10 000) lines.
- Edits: the edit observer (§F3) receives the editor's `InputEdit`s, and the recorder groups one action's edits into an `EditBatch`. The editor applies a multi-cursor action from the highest offset down, so each edit's range is in the text before the action. When several edits wait, they go as one patch, the difference between the core's text and the editor's, since the core's corrections to the first would shift the rest. Core patches go back through `replace_text_in_range` in UTF-16 offsets, converted through the editor's rope, inside a reentrancy guard.
- Key contexts: the editor's own bindings sit in a deeper context and win over the window's, so `merge_app::init` binds each window key twice, for the window and for the result editor. Undo and redo bypass the editor's own undo. Until something has the focus no key reaches the window, so the window focuses the result when it opens. gpui-component's `Button` calls `prevent_default` on mouse down, so a toolbar click keeps the focus.
- The window pins the region that navigation or a pick made current, with the caret it left: an empty region holds no byte, so the caret alone cannot name it.
- Toolbar: `toolbar::Labels::for_width` drops every label below `LABELS_MIN_WIDTH` (1052 px at the default toolbar text, more at a larger `toolbar-font-size`); unlabelled, it fits at `COMPACT_MIN_WIDTH` (788 px).
- Go to line: a field above the result, mapped through `scroll::row_of_line`. Zed's modal takes `row:column`; triad takes a line.
- Open dialog: Merge and Diff tabs after Zed's modals (small text, a tab strip, icon-only pickers), on gpui-base's headless `Tabs`, which carry tab roles. Start validates the slots on the UI thread and hands a `Load` to the background executor, which reads, decodes and merges the inputs or reads the diff's files and finds their repository; the dialog opens the window when it lands. The button shows its loading state meanwhile and takes no clicks.
- gpui-kit's `test-support` pins `cocoa` 0.26.0 and `unicode-properties` 0.1.3 in the lockfile through weak dependency features (rust-lang/cargo#10801).

#### A6.3 Scroll sync for every source

A zero-size canvas between the rows and the result runs the sync in its prepaint, every frame. It compares both scroll tops with the last sync and copies from the pane that moved, so the pane scrolled last leads and a synced pane does not echo back. The canvas sits before the result in prepaint order because the editor applies a deferred scroll offset in its prepaint and clears it in paint. `merge_view::scroll::ScrollSync` holds the rule; `pane_scroll::PaneScroll` holds the rows' handle, sideways scroll, wrap and both syncs.

Zed's split diff gives both editors the same rows through spacer blocks. triad's rows and result differ in line count, so it maps by region.

**Sideways.** The top panes' text sits in a clipping cell per pane and shifts by one shared offset (`sideways::Sideways`). The list keeps vertical wheel moves (`restrict_scroll_to_axis`); a wheel listener on the rows takes the sideways part when it is larger. The offset runs to the widest line a shown pane holds, counted off the UI thread with tabs expanded, times the advance of `m`, less the text width an invisible canvas records each frame. A second `ScrollSync` with identity maps keeps rows and result level sideways.

**Wrapped top panes.** The list holds one item per display row. `row_wrap::RowWrap` gives each aligned row the display rows of its tallest cell, wrapping each cell with GPUI's `LineWrapper` in prepaint at the recorded width. `merge_view::heights::Heights` maps items to display rows and back. Scroll sync, reveal, Go to Line and search steps work in aligned rows and convert through the heights; the result's come from the editor's display map. When heights change, the rows keep their top aligned row.

#### A6.4 Save, cancel and the journal

Spec §6.2 sets the behaviour. A window ends through the `Finish` global: `merge_app::run` exits the process with the code, since GPUI's quit on macOS exits with 0; tests record the code. Prompts are platform prompts, which tests answer with `simulate_prompt_answer`.

The journal (`merge_session::recovery`) holds the format version, SHA-256 of each input's decoded text, the `MergeOptions`, SHA-256 of the starting alignment and the command log, at `<cache>/journal/<key>.json`. Restore aligns again and refuses a journal whose alignment hash differs, since commands name regions and byte offsets. `JOURNAL_DEBOUNCE` (500 ms) restarts with each command; tests drive it with the executor's fake clock. `JournalTasks` holds the waiting write and the running write or removal, and cancels them before a flush, a discard or a restore (§A13.4).

`merge_session::ending` decides Save & Exit, Close and the answers to their alerts without GPUI: each step gives a `Next`, and `Journal` says whether the journal is removed, written or left. `Session::apply` and `Session::restore` refuse while a save runs, and the result is read-only until it ends. `Session::start_save` moves the `Recovery` into the `SaveJob`, which writes the journal and then the output on one background thread; `Session::saved` takes it back. The UI thread neither syncs nor hashes the inputs for a save: with the inputs still hashing, the job hashes them, 100 ms per input at 1M lines.

#### A6.5 Two-way diff window

- Two editor panes over `DiffDocument`, edits through the core as in the merge window, per-side undo, a background recompute after 100 ms (`DIFF_DEBOUNCE_MS`). gpui-component's `Editor` sets the read-only flag on each render, so read-only goes on the element.
- File watching: `notify` watches each file's directory and calls `DiffWindow::disk_changed`; the watcher has no headless test, since its events come on another thread. `OwnWrites` holds each side's writes from their start, so a change the watcher reports before the save's result arrives is still the window's own.
- **Alignment: spacer rows, as Zed does.** Zed's split diff keeps both sides editable and aligns them with spacer rows from its block map. The vendored editor has a `SpacerMap` layer between `FoldMap` and `DisplayMap` (`EditorState::set_spacers`): sorted runs of inserted rows, mapped by binary search, painted hatched, skipped by cursor movement, selection and hit testing. A spacer row is a display row, and line painting places each line at its own display row. `merge_view::diff::spacers` gives each changed region's shorter side the difference in rows. Between a keystroke and the recompute, spacers stay on their line numbers. `EditorState::set_hidden_lines` hides whole lines through `FoldMap`, apart from tree-sitter's folds and with folding off; a hidden line's spacers drop out, and a left press on a spacer row emits `InputEvent::SpacerClick` with the line below it. These carry the fold rows of §G2.
- **Collapsed unchanged regions** (spec §5). `merge_view::fold::folds` gives the lines each side hides from the regions, the context and the opened regions. The window hands each editor its folds' lines with `set_hidden_lines`; `fold_spacers` moves filler rows off hidden lines and adds one spacer row per fold, which a spacer note names. While the regions lag the text, the folds stay as they are, as the spacers do. `Opened` keeps an opened region by its lines on each side. Each edit moves them by its `LineEdit`: the lines that differ once what the old and new text share at either end is trimmed, since a replace-all sends the whole text as one patch. A recompute keeps the opened regions that share lines with an unchanged region. Fold rows are display rows, so scroll sync, the copy buttons, a jump's reveal and a ruler click read rows from the editors' display maps. The editor scrolls a search match out of a fold once it lays out.
- Spacers give both panes the same rows, so they share one scroll top. The left pane, laid out first, takes a move a frame late.
- Copy buttons: a 28 px column between the panes holds → and ← on each changed region's first row. None show while regions lag the text, since a recompute can renumber regions. A copy is an ordinary edit, so undo on that side reverses it.
- A jump or ruler click blinks a 2 px frame round the region (`merge_app::blink`). The vendored editor paints fills over the active line and borders last.
- `jj diff --tool` with the working copy on the right opens the working-copy file and makes the left copy read-only (`merge_session::diff_tool::jj_working_copy`, spec §7.1); otherwise a path under `jj-diff-<random>/` gets a banner saying edits will be lost.

#### A6.6 Accessibility and settings

Roles and labels follow spec §6.5. GPUI builds an AccessKit tree from each element's role and label. The test platform has no AccessKit adapter, so tests read what gpui-kit records for `test_support()` elements (`tests/a11y.rs`). AccessKit's macOS adapter reports every toggled state but off as 1, so the targets of an unresolved conflict report off and their labels end ", unresolved". Gaps are in `docs/gpui-kit-issues.md`: no live announcements, bands and gutter marks not exposed, only on-screen rows visible.

A change to the alignment options re-aligns the open merge while every region keeps its automatic resolution (`MergeWindow::set_options`); otherwise the status bar says it applies from the next session.

#### A6.7 Performance

`merge_app::measure`: `TRIAD_MEASURE=open|scroll-rows|scroll-result|type|pick|syntax` makes the window act once a frame for 300 frames, reports frame intervals and each action's time on stderr, and exits with 2 without writing. `script/measure-app` runs every mode at 100k and 1M lines and fails on a missed spec §9 target. A window in the background gets 41.7 ms frames and keystrokes three to four times slower, from macOS; keep it frontmost. `script/ax-tree.swift` prints and presses the accessibility tree; GPUI builds it on the first query, so that query can return an empty window.

`script/measure-app 120`, 2026-11-08:

| Lines | Load and merge | First frame | Scroll p95, rows / result | Keystroke work, mean / max | Pick work, mean / max | Peak RSS |
|---|---|---|---|---|---|---|
| 100k | 46–50 ms | 210–223 ms | 8.4 / 8.4 ms | 0.3 / 0.5 ms | 0.4 / 0.6 ms | 221 MB |
| 1M | 521–548 ms | 727–754 ms | 8.4 / 8.4 ms | 1.9 / 5.2 ms | 4.4 / 6.3 ms | 869 MB |

`script/check-bench`, same day:

| Benchmark | 100k | 1M |
|---|---|---|
| `load` | 24 ms | 434 ms |
| `load-jj` | 41 ms | 498 ms |
| `load-word-small` | 53 ms | 610 ms |
| `load-word-large` | 1.57 s | 15.6 s |
| `load-word-small-rust-bounded` | 47 ms | 511 ms |
| `load-deep-rust-bounded` | 91 ms | 953 ms |
| `toggle`, `keystroke` | 0.7 µs | 0.8 µs |
| Session heap, git / jj | 32 / 56 MB | 362 / 607 MB |

`cargo bench -p merge-cli --bench memory` measures the session's peak heap through a counting allocator (`cap`); `script/check-bench` holds it below spec §9's memory row in git and jj mode. Saving is the peak: `MergeDocument::output` copies the rope into a `String`.

What sets these figures:

- **First frame.** Interval 0 holds the editor's line layout; the vendored editor reads the lines in one pass. Interval 1 (45–53 ms) fills the glyph atlas and builds Metal pipelines, once.
- **Journal hashing** runs once per input on its own thread, so the window shows first.
- **Incremental refresh.** The editor moves its decorations with each edit; `refresh` compares the new bands with the moved ranges (`marks::bands_unchanged`) and rebuilds only when a range or look differs. The keystroke maximum at 1M is the first key into a region, which changes its look. A pick always rebuilds. Bands, placeholders and status counts still scan every region per command.
- **Ruler.** `ruler::painted` gives each whole pixel the style of the last mark on it and joins equal neighbours, so a long file paints no more runs than the strip has pixels. One quad per mark took a third of the main thread at 1M.
- **Alignment** is 368 of load's 449 ms at 1M and grows faster than linear (17 ms at 100k): the first place to look if load nears 1 s.
- **Known whole-text copies**, each a few milliseconds and one file's size at 1M: `Recompute::run` copies both diff texts twice; `MergeDocument::output` copies the rope to save; `Text` keeps an 8-byte line start per line where `u32` would do under 4 GiB.

**Idle memory.** The caret's blink draws every 500 ms, which keeps the Metal driver's reclaimable blocks in use: a bare GPUI window takes 52 MB idle and 185 MB blinking. The vendored caret rests, shown, after 10 s without a key, click or focus. At rest the demo diff of `fixtures/demo/code_a.py` and `code_b.py` takes 98 MB of physical footprint (p4merge: 121 MB): 37 MB of drawables, 33 MB of heap, and the binary. GPUI counts a window losing the front as a blur, which stops the blink. GPUI's per-window path texture and the drawables remain.

### A7 Tool integration and the macOS release

#### A7.1 `vcs` crate and native diff mode

- `vcs` holds the `Repository` trait and its types; paths are `camino::Utf8Path` relative to the root. `RevSpec` is `Index`, a `Commit(CommitId)`, or a revision expression in the backend's syntax. A `CommitId` is looked up by ID (gix's object header, jj's index), so a prefix, `HEAD` or `@` in one is `UnknownRevision`. `Term::title` gives a pane title: the label; else the change ID (jj) or commit (git), then the summary. `VcsError::Backend` boxes gix and jj-lib errors, so `vcs` names neither.
- `vcs-git` (gix 0.90). `default_parent` is the index, labelled `index`, when the file's entry differs from `HEAD`'s, and before the first commit; otherwise `HEAD`, labelled with the short branch name.
- `vcs-jj` (jj-lib 0.47.1) loads the workspace with jj-lib's default settings at its single head operation. Several heads is `JjError::ConcurrentOperations`: reconciling them writes an operation. `file_at` evaluates a revset with jj-lib's built-in functions and no user aliases; several commits read from the merge of their trees, as `jj diff` does. A conflicted path is `Conflicted`; an ambiguous prefix, a divergent change or a conflicted bookmark is `AmbiguousRevision`. `default_parent` is `@-`, with the change ID and commit at jj's display length; a merge working copy gives an empty `Term`.
- The conformance suite (`merge-session/tests/conformance.rs`) runs every check on git, jj and internal jj repositories built through gix and jj-lib. `vcs-jj`'s `testing` feature builds workspaces with fixed settings and seed, so IDs repeat.

#### A7.2 `triad-merge install`

Writes spec §7.1's entries. git: gix's config `File`, written only when it changed. jj: jj-lib's `ConfigLayer` through `toml_edit`, keeping comments and other tables. `vcs_git::tool_config` and `vcs_jj::tool_config` hold each VCS's keys and file; `merge_session::install` holds triad's arguments. `Env` carries `HOME`, `XDG_CONFIG_HOME`, `GIT_CONFIG_GLOBAL` and `JJ_CONFIG`, so tests never touch the real environment. jj's merge tools get `$base`, `$left`, `$right`, `$output`, `$marker_length` and `$path`, and no side labels. `install` writes the absolute path of the running binary.

#### A7.3 End-to-end checks

`triad-merge/tests/tool_integration.rs`. Neither gix nor jj-lib runs merge tools, so the harness takes each VCS's half from its source:

- git: the installed `cmd` runs under `sh -c` with `BASE`, `LOCAL`, `REMOTE` and `MERGED`, as `git mergetool` and `git difftool` run it, with paths holding spaces and quotes.
- jj: `vcs_jj::testing::Fixture::resolve` follows `jj resolve` (`cli/src/merge_tools/external.rs`) through jj-lib: input files, pre-filled output, `choose_materialized_conflict_marker_len`, `$name` interpolation, exit-code rules, and `update_from_content` on the output.

jj makes markers four longer than the longest marker-like line, counting `+ - % \`; `$marker_length` carries its length to triad (spec §4.4). `docs/release-checklist.md` covers what needs the window, for git and jj alike.

`git mergetool` restores the file from its backup after exit 1, discarding triad's output. A save that leaves conflicts therefore writes the journal at once and keeps it; the next run offers the restore, with the caret on the first unresolved conflict. The merge window opens on the first unresolved conflict, else the first difference.

#### A7.4 Local install on macOS

Release 1 is for local use: no Developer ID signing, notarisation or Homebrew cask (owner, 2026-10-27). `script/macos-app` builds `triad.app` and signs it ad hoc; only the bundle gives the Dock its icon, since `setApplicationIconImage` is unsafe in `objc2-app-kit`. README § Build and install gives the commands.

### A11 Decisions

1. Two-way alignment uses spacer rows through a `SpacerMap` patch (§A6.5) (owner, 2026-10-25).
2. `gpui-base` is vendored with triad's patches (§A6.1) (owner, 2026-10-25).
3. The licence is MIT (owner, 2026-10-25). libgit2's tests are GPL-2.0-only and Zed's crates GPL-3.0-or-later, so they give case lists and designs only.
4. A repository with `.jj` is a jj repository and merges with jj's semantics (§B3) (owner, 2026-10-27).
5. gix moves with jj-lib, exact-pinned together, so the build holds one gix (§A3).

### A13 Code structure

#### A13.1 Index newtypes

`RegionIx`, `Line` and `ByteOffset` (`merge_core::index`), `RowIx` (`merge_view::rows`) and `CommitId` (`vcs`) keep a swapped argument from compiling. Each offers only the arithmetic its callers need; `RegionIx` serialises as its number. `Source::Side` and `Source::Base` keep `usize`. `region_bytes` and `blocks` stay `Range<usize>`: `Range<T>` is not iterable on stable. Each newtype has a compiling doctest and a `compile_fail` twin that swaps one argument; stable rustdoc ignores the error code, so the compiling twin shows the failure comes from the swap. The commit gate skips doctests.

#### A13.2 Fallible lookups at the view boundary

A caret, scroll or row position held across a command could fall out of range after an undo, a realignment or a shrinking edit. Each stored position is dropped or recomputed before use: a realignment, undo, redo or edit drops the pin; picks and navigation send waiting edits before reading the caret; the diff window hides copy buttons and refuses `copy_region` while regions lag, and navigation recomputes first. `MergeDocument::region_at` and `RowMap::get_row` return `Option`; `nav::held_region` and `diff::held_region` always return a region. Clippy's `indexing_slicing` is not enabled: its 64 findings index by `Side`, fixed arrays, ranges the same call computes, or checked sources.

#### A13.3 Window size

`window.rs` and `diff_window.rs` are large because the windows have many features; their logic with state sits in components (§A6.2). The two windows share `OriginColumn`, `editor_sync`, `blink`, `ruler`, `settings_banner` and `Wrap`. Their remaining shared method names are accessors and wiring of 2–9 lines. The diff window keeps eight loose scroll fields; a `DiffScroll` like `PaneScroll` waits for a diff scroll bug to start from (PLAN.md §3).

#### A13.4 Recovery gate

A journal write costs one or two syncs, and on macOS `sync_all` and `sync_data` are `F_FULLFSYNC`, about 4 ms each on an Apple SSD. A whole write (temporary file, sync, rename, directory sync) takes 9.0 ms median and 14 ms max from 1 KB to 1 MB; an append takes 4.0 ms median and 5.2 ms max. A whole write is longer than a 120 Hz frame (8.3 ms), so the UI thread never waits for one.

`Gate` orders the journal's file operations. Every generation of a journal shares one lock, which a write or a removal holds while it runs; each generation has its own open flag. `Recovery::retire` clears the flag without waiting and starts the next generation. A running operation ends before any of the next generation starts, and one of the old generation that starts later skips. A write that has renamed its file cannot be undone, so order comes from the lock, not the flag.

`Locator::find` hashes the inputs and reads the journal they name on the background executor. `Recovery::offer` takes that journal for the restore offer and holds every write until the answer (`OnDisk::Offered`), so a crash while the offer is open keeps that journal. `Recovery::discard` returns a `Removal`, which the window runs on the background executor in the slot of the running write. No write starts until `Recovery::removed` takes its result; then commands made since start a new file. Restore replays the journal on the background executor; Start Over and a failed restore take this path. Save & Exit writes the journal in its save job (§A6.4). As the window finishes, `Session::finish_journal` writes or removes on the UI thread and waits for a running operation: the window is closing. A flush of a file that holds every command writes nothing, so a save that leaves conflicts does not write the journal twice.

Tests hold the lock on a second thread behind a barrier, with no sleeps. A retire that waited would deadlock, and nextest ends it at 30 s.

#### A13.5 Pre-commit gate time

Limit: 35 s on a warm build (AGENTS.md). The gate takes 12 s after an edit to `merge-app` and 16 s after one to `merge-core`.

An edit under `vendor/` has a 60 s limit. It takes 45 s: the 107k-line `gpui-base` compiles three times, for clippy (7 s), for the workspace tests with `merge-app` and `triad-merge` (20 s), and for its own unit tests in `target/vendor`, with test features and debug info (8 s). The next run takes 10 s.

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
- `merge-config` holds the config file's format, settings, key bindings and the names of their values (`named::Named`, over strum derives). It reaches no repository; `script/check-workspace` holds it to `merge-core` and `vcs`.
- `merge-session` is the one crate that picks a backend: semantics, discovery, `resolve`, origins, loading, saving, recovery, exit codes and `WindowForm`.
- `merge-cli` parses arguments and runs batch forms (`Invocation` is `Window`, `Batch` or `ShowConfig`); `merge-app` is the GPUI shell and matches no headless form.

**One repository rule.** `native::nearest_repo` finds the nearest `.jj` directory or `.git` entry, jj on a tie, for the semantics and for discovery, which opens only that backend at that root. A `.git` git cannot open is `NotARepository`, never a climb to an outer repository (spec §6.1).

**One repository per run.** `merge_session::repository::RunRepository` holds the directories a run's repository may lie in, in order, and opens the first on first use; every step takes it: semantics, marker style, `resolve`, origins and saves. `find` opens a git candidate as it finds it and passes over one git cannot open, so kind, root and origins come from one repository. A `Handoff` found again keeps the run's jj config.

**One save.** `EncodedOutput::new(document, ending)` encodes; `write(path)` writes (`io::WriteError`); `save(path, record, run)` writes, then records the save in the run's repository (`SaveError::Write` wrote nothing; `SaveError::Record` wrote the file and keeps the `VcsError`). Save & Exit runs it on a background thread.

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
- jj merges bytes. A UTF-8 BOM is part of its first line, and UTF-16 splits at each `0x0A` byte, so jj and triad diverge on UTF-16 and on inputs that disagree about a BOM.
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
- `ignore-whitespace` applies in both modes: jj mode passes `ContentDiff::for_tokenizer` a `CompareBytes` keyed by `diff::line_key`. Spec §4.2's output rule applies, so any mode other than `none` diverges from jj and the status bar marks it.
- `same_change = keep` in git mode and `hunk_level = word` in git mode diverge from git and are marked.
- `refine-conflicts` changes no output, so it stays on in both modes.

### B5 Design

#### B5.1 The jj engine

`merge-core` depends on `jj-core` (Apache-2.0, exact-pinned with jj-lib), so the diff is jj's by construction. `files.rs`'s hunk resolution lives in `jj-lib`, which `merge-core` must not depend on, so `merge-core` re-implements those 80 lines over `Merge` and `trivial_merge`, with an Apache-2.0 notice. jj-core adds about 44 crates to a clean `merge-core` build (5.5 s against 3.6 s). Byte ranges map to line ranges by binary search in `Text`'s line index.

jj-core's `regex` and `aho-corasick` add `PartialEq<Span>` impls for `Range<usize>`, so `assert_eq!(ranges, [])` cannot infer its type in any crate linking `merge-core`; such tests compare against a typed empty constant.

#### B5.2 jj oracle corpus

`fixtures/oracle/jj-corpus.jsonl`, from `script/gen-jj-oracle` (an example in `vcs-jj`, the only crate that can call `jj_lib::files::merge_hunks`). It runs the git corpus's triples and jj seeds under each `same_change` × `hunk_level`, and records each mode's conflict count and jj's hunks in order, each `{"resolved": text}` or `{"conflict": {"sides", "bases"}}`, so tests check where each conflict lies. Hand seeds cover repetitive lines, touching changes, identical changes, word-resolvable conflicts, multi-byte UTF-8 in words, the 100-occurrence limit, and five- and seven-term merges. The corpus stores UTF-8 strings, so UTF-16 and BOM cases are unit tests. The oracle shares only jj-core with triad, so it tests triad's hunk resolution, its mapping to regions and its rendering, not the diff.

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

`merge-core` stays free of tree-sitter: `merge_core::boundaries` holds `Cuts` and the `Boundaries` trait; `merge_syntax::boundaries::SyntaxBoundaries` implements it, parsing each region's window, never the whole file. Each thread keeps one parser.

### B6 Benchmarks

| Measure | 100k lines | 1M lines |
|---|---|---|
| `load-jj` | 41 ms | 503 ms |
| `align-jj` | 34 ms | 437 ms |
| Session peak heap, jj / git | 56 / 32 MB | 607 / 362 MB |
| `load-word-small` (a one-line conflict every 20 lines) | 52 ms | 601 ms |
| `load-word-large` (one conflict over the whole file) | 1.58 s | 15.4 s |
| `load-word-small-rust`, off / bounded | 43 / 47 ms | 467 / 512 ms |
| `load-deep-rust`, off / bounded | 45 / 91 ms | 482 / 949 ms |

- `load-word-large` is one call to jj-core's `ContentDiff::by_word`, about 15 µs per line, as jj's own word merge makes. Spec §9 exempts it (§B7, decision 5); `script/check-bench` holds it below 2 s at 100k and 20 s at 1M.
- `cargo bench -p merge-syntax --bench boundaries` writes valid Rust: four-line functions with a conflict in every fifth. The deep case parses each conflict's window in both sides, about 9 µs a conflict.
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

`merge_view::origin::Origins` holds one optional `Annotation` per input. A result line takes the origin of the input its region's pick takes it from; an unchanged line prefers base; edited text has none. A run's label goes on its first line. `carry` is a byte-exact line diff, so a line changed only in whitespace loses its origin.

#### C5.4 Window

Each input annotates in a background task; a failure is logged and leaves that input without origins. The diff window carries each pane's annotation onto its current text in each recompute's background step (`Recompute::run_with`), so edits, undo and reloads update origins (§C6, decision 3). The editors' gutters show labels through the vendored `EditorState::set_line_labels`. Hover cards are GPUI tooltips, which headless tests do not open; the text each shows is tested.

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
- `JjRepository::snapshot` follows jj-cli: it imports a moved git `HEAD` first (`import_git_head`), refuses a stale working copy (`VcsError::StaleWorkspace`, naming `jj workspace update-stale`), reloads an updated one, snapshots with the user's options, commits a rewrite of `@` and rebases descendants, and marks new files intent-to-add in git's index. An internal workspace skips the import and the index. git's other refs are left to jj's next command.
- Snapshot options come from the user's jj config: `snapshot.auto-track` with the layers' `[fileset-aliases]`, `snapshot.max-new-file-size` (0 is no limit), and git's ignore files. `fsmonitor` is always `none`: triad's jj-lib has no watchman.

`cargo bench -p merge-cli --bench resolve`, 2026-11-09:

| | 10k files | 100k files |
|---|---|---|
| jj: load a conflict | 12 ms | 105 ms |
| jj: list conflicts | 11 ms | 99 ms |
| jj: record a save | 52 ms | 168 ms |
| git: load / stage | | 6 / 30 ms |

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

#### E3.8 Appearance and save keys

Spec §6.4 lists the keys. The default colours are `tones.rs`'s hues as `#rrggbb`: HSL saturation 0.7 and lightness 0.45 in the light theme, 0.65 in the dark; the current region's 88° at 0.85/0.4 light and 0.75/0.55 dark. `merge-app::tones` builds a `Palette` from the settings when a window opens. `conflict-style` applies under git semantics, defaulting to git's `merge.conflictStyle` read through gix; `zdiff3` reads as `diff3`, since refinement is `refine-conflicts`'s job.

### E4 Overview ruler

A strip at the right edge of the result pane and the diff window's right pane marks every region not `Unchanged` (spec §5). `merge_view::ruler` places marks by result lines, since the strip stands beside the result; `painted` merges them per pixel (§A6.7). Under soft wrap the marks are approximate. The strip is the pane's vertical scrollbar: the thumb is the view's share of the content, at least 16 px. The drag state lives in the window, since the strip is rebuilt each frame, and mouse handlers are window-wide so a drag goes on past it.

#### E4.4 Scrollbars

`scrollbars` (`always`, `hover`, `when-scrolling`) maps to gpui-base's `ScrollbarMode`. Each editor gets its bars through `EditorState::set_scrollbar_renderer`; the result and the diff's right pane paint only the sideways bar and feed their vertical scroll to the ruler in the same frame. gpui-base times a bar's fade with `Instant::now`, which tests cannot advance, so tests check visibility through clicks. The top panes' sideways scroll is `Sideways`, not a scroll handle, so they have no sideways bar.

### E5 Output line ending

`output-line-ending` (`keep`, `base`, `lf`, `crlf`; spec §4.1) converts at save, so the result pane keeps source bytes and picks stay exact. `keep` is the default so git's byte-for-byte output holds. `MergeDocument` counts the result's `\n` and `\r\n` endings as each patch lands, so the status bar's `mixed endings` check costs only the unresolved conflicts' markers and bases. `saved_endings` gives the endings under a setting without building the output; `output_with` builds and converts it.

### E6 Save Copy As

⇧⌘S writes the output as Save & Exit would, through the same encoder, and leaves OUTPUT, the exit code and the journal alone. p4merge's Save As retargets the window; triad writes a copy because git and jj read OUTPUT, so the window must never lose it.

### E7 Linux and Windows

#### E7.1 Linux

`script/linux` runs an x64 Ubuntu 22.04 container under Rosetta in OrbStack: glibc 2.35, Mesa's software renderer, sway without a display for Wayland, and Xwayland for X11. Sway stands in for GNOME; timings there say nothing about spec §9.

- `script/package` builds the AppImage and `.tar.gz` there, so the binary needs glibc 2.35 at most. Both bundle `libxkbcommon`, `libxkbcommon-x11` and `libxcb-xkb` by rpath; `libxcb` stays the system's. CI's `package` job tests the release build and smoke-tests the AppImage with `APPIMAGE_EXTRACT_AND_RUN=1`. appimagetool 1.9.1 and the type-2 runtime are pinned by digest.
- The `linux-emulated` nextest profile leaves out `forward::a_missing_gui_binary_is_an_error`: Rosetta's `posix_spawn` reports success for a missing file.
- `script/linux/smoke` drives the real window on Wayland and X11. X11 runs on GL: Mesa 23's software Vulkan shows nothing in a 32-bit window.
- `merge_app::menu_bar` draws gpui-component's `TitleBar` and `AppMenuBar` on Linux and Windows. `merge_app::prompt_view` replaces GPUI's fallback prompt on Linux; `run` installs it, since headless tests answer the test platform's prompts.

#### E7.2 Windows

The console and GUI binaries forward as §A4.13 gives. `images/triad.ico` goes in both binaries as icon resource 1, which GPUI's window class loads, through a build script with a resource crate.

### E9 Key bindings

`keybind = CHORD=ACTION`, in Ghostty's form; spec §6.3 gives the rules. `merge_config::keybind` parses and holds the bindings; `merge_app::keys` turns them into GPUI's.

- **Per-window replacement.** `cmd+s` is `save-and-exit` in the merge window and `save` in the diff window; a later line replaces a chord only in the windows that have both commands.
- **`cmd` and `super`.** Each is a bad line on the other's platforms. A shared config keeps its chords in an optional include per platform.
- **Unbound chords** bind to GPUI's `NoAction` with no context, which also stops the editors' own bindings.
- **Menu chord.** macOS shows an action's first binding, so `keys::bind` registers the newest first; the defaults list `cmd+q` before `cmd+w` so Cancel keeps ⌘W.
- **`show-config`** prints `clear` first, so the output reads back as the same bindings, and after a `clear` lists each dropped default as `CHORD=unbind`.

## F. Syntax highlighting

### F1 Goal

- Every pane colours its text by syntax; bands draw under the colours, intra-line marks over them (spec §5).
- Grammars: Rust, Go, C#, Python, Swift, HTML, JavaScript, TypeScript, TSX.
- The window never waits for a parse.

Not covered: Razor, which has no grammar that covers its code blocks; styles inside HTML and JavaScript templates tagged `css`, `json`, `sql`, `yaml` or `graphql`, whose grammars triad does not build.

### F2 What gpui-component gives

- Grammars as cargo features through `gpui-kit`, each building its C sources. `LanguageRegistry::register` takes grammars from other crates.
- C# and Swift come without a highlight query; triad registers them again with their grammar crates' queries. TSX comes with only the lines TypeScript adds to JavaScript's query; triad registers it with gpui-component's TypeScript query and injections.
- The editors' query puts locals before highlights, so an unstyled local capture (`@local.reference` on a JavaScript identifier) draws plain. The top panes take the same query.
- The editor's tree-sitter highlighter is crate-private: it parses within 2 ms on the UI thread, else in the background after 150 ms, and always in the background over 256 KiB. `SyntaxHighlighter` is public and `Send`.
- `HighlightTheme::default_light()` and `default_dark()` resolve capture names to colours.

### F3 The edit observer

An editor holds one highlighter, and gpui-component's tree-sitter highlighter is crate-private, so triad can neither wrap it nor record edits through the highlighter slot. The vendored `EditorState::set_edit_observer` gives the edit recorder every edit the editor reports to its highlighter, in the same order and at the same moment, and leaves the slot to the highlighter.

### F4 The top panes

The top panes are rows triad draws, and their texts never change.

- Each input is cut into chunks of about 2 000 lines (`CHUNK_LINES`, `merge_syntax::syntax::chunk_starts`). A chunk ends before the first top-level line at or after its 2 000th: a line at column 0, after a blank line, not starting with `}`, `)`, `]` or `</`; without one, at its 2 000th line. A chunk's spans equal those of the whole parse unless a construct crosses a forced cut. A long HTML document is one element, so the first closing tags after each cut draw plain.
- A chunk parses alone, off the UI thread, when a row of it shows. A row on screen queues its chunk and one on each side; the nearest parses first; each input runs at most two parses at once; a queued chunk more than two from the screen drops when the rows move. Each input keeps 16 chunks' spans, dropping the farthest. A failed chunk draws plain and does not parse again.
- Each line keeps capture spans: byte ranges with up to three captures, outermost first. Overlaps follow gpui-component: of two captures of one node the first wins; a capture paints over those starting before it; an injection parses one level deep when its language is built in (Rust macros, HTML scripts, JavaScript `html` templates).

#### F4.1 The editors past `syntax-whole-file-max-lines`

Below `syntax-whole-file-max-lines` (20 000) gpui-component's highlighter parses the editor whole. Above it, triad's (`editor_syntax.rs`) cuts the rope by §F4's rule in one pass, off the UI thread, and parses chunks as the editor draws them. An edit merges the chunks it touches into one with a new id; a late parse of an old chunk drops; a chunk past twice `CHUNK_LINES` cuts again; an edit whose rows do not match the chunks cuts afresh. The editor's first update carries an edit, so a rebuilt highlighter takes the whole text as a fresh cut.

### F5 Language detection

`merge_syntax::language::Language::of_run` holds spec §4.3's order: the inputs' names, then the output's, then the title. git and jj pass temporary inputs (`BASE_1234.rs`), so the fallbacks matter. The first match wins; there is no content sniffing.

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

The rows on screen colour 110–185 ms after the first frame at any length; a chunk scrolled to parses in about 60 ms. Spans take about 12 bytes per capture run: the top panes stay within about 50 MB of plain text at 1M lines, the chunked result within 70 MB.

### F9 Decisions

1. Razor is deferred. Further grammars come one per commit (PLAN.md §2).
2. The edit recorder moves to a gpui-base edit observer (§F3).
3. The top panes keep chunked capture spans; the editors highlight whole files up to `syntax-whole-file-max-lines` (owner, 2026-11-03).
4. triad sets its own light colours for captures under 3:1 on a band (owner, 2026-11-03).
5. The top panes parse only the chunks on screen and their neighbours, and keep 16 per input (owner, 2026-11-04).
6. Past `syntax-whole-file-max-lines` the editors parse a chunk at a time with triad's highlighter (owner, 2026-11-04).
