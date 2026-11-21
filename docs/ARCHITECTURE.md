# triad — architecture

How the code is laid out: what each crate does, its main types, and how a run moves through them. [SPEC.md](SPEC.md) sets the behaviour. [DESIGN.md](DESIGN.md) holds the reasons, the decisions and the measurements; this file cites it by section. AGENTS.md § Layout lists the directories.

## 1. Crates and layers

triad is a Cargo workspace of eleven crates. Each crate depends only on crates in earlier layers (`DESIGN.md` §A13.7):

| Layer | Crate | Role |
|---|---|---|
| 1 | `merge-core` | The engine: text decoding, line diff, n-way alignment, resolution, undo, output. No I/O, no VCS, no UI. |
| 1 | `vcs` | The `Repository` trait and the types both backends fill. |
| 2 | `merge-syntax` | Languages, tree-sitter grammars, highlight spans, syntax boundaries for the merge. |
| 2 | `merge-config` | The config file's format, the settings, the key bindings. |
| 2 | `vcs-git` | `Repository` over gix. |
| 2 | `vcs-jj` | `Repository` over jj-lib, plus the jj-native operations. |
| 3 | `merge-view` | The view model: rows, scroll sync, navigation, status, origins. No GPUI. |
| 3 | `merge-session` | The layer under both front ends: semantics, repository discovery, loading, saving, recovery, exit codes. |
| 4 | `merge-cli` | Argument parsing and every form that runs without a window. No GPUI. |
| 4 | `merge-app` | The GPUI shell: windows, panes, toolbar, menus, keys. |
| 5 | `triad-merge` | The binaries `triad-merge` and `triad-merge-gui`. |

```
                        triad-merge
                       /           \
               merge-cli            merge-app
                    \              /    |
                     merge-session   merge-view
                    /   |    |    \      |    \
          merge-config  |  vcs-jj vcs-git |  merge-syntax
                 |      |      \   /      |   /
                 |      |       vcs       |  /
                 +------+----- merge-core +-+
```

Three boundaries carry the design:

- **No GUI below `merge-app`.** `merge-cli`, and with it `--headless`, `inspect` and their tests, never link GPUI (`DESIGN.md` §A2).
- **No backend choice below `merge-session`.** `merge-core` knows no VCS. `merge-view` takes only `vcs`'s plain types for line origins. `merge-session` is the one crate that opens a git or jj repository.
- **The core is the source of truth.** Every change to a merge's result goes through `MergeDocument::apply` and comes back as patches. The editors show the text; they do not own it.

### 1.1 A run, end to end

1. `triad-merge`'s `main` loads the settings path and calls `merge_cli::run` with the process arguments.
2. `merge_cli::args` parses them into an `Invocation`: `Window`, `Batch` or `ShowConfig`.
3. A `Batch` runs to completion in `merge-cli` over `merge-session` and returns `Outcome::Exit(code)`.
4. A `Window` form is settled first: `merge-cli` finds its repository (`RunRepository`), chooses the semantics and, for `resolve`, the target. It returns `Outcome::Window(WindowForm)`. A bad input exits with 3 here, before any window shows.
5. `triad-merge` hands the `WindowForm` to `merge_app::run`. On Windows the console binary starts `triad-merge-gui` with the same arguments and forwards its exit code (`DESIGN.md` §A4.13).
6. `merge_app::run` loads the inputs through `merge-session`, opens the window and returns the `ExitCode` the window finishes with.

Exit codes are spec §6.2's and live in `merge_session::ExitCode`: 0 resolved, 1 conflicts left, 2 cancelled, 3 error.

## 2. `merge-core`

The engine. It turns decoded texts into aligned regions, holds each region's resolution, builds the result text, and undoes and redoes. It reads no files, opens no repository and draws nothing, so it runs the same under the CLI, the GUI, the benchmarks and the property tests.

### 2.1 Text

`text::Text` is a decoded, immutable file: a `String` and its line starts, behind an `Arc`, so a clone costs a pointer. `Text::decode` reads a byte order mark, then UTF-16 without one, then UTF-8, then Windows-1252, and rejects binary input (spec §4.1). Lines end at `\n` and include it; a lone `\r` is an ordinary character. The `Encoding` travels with the text so the output encodes back the same way.

### 2.2 Diff

`diff::diff_lines` diffs two `Text`s into `Hunk`s under a `DiffOptions`: the algorithm, whitespace, line-ending, blank-line and case rules, and `ignore-pattern` regexes (`pattern`). The options change only the key a line is interned under, never its bytes. `histogram` is a port of git's histogram diff with git's slider compaction, so conflicts form where `git merge-file` puts them; imara-diff serves `Myers` and `MyersMinimal`. `jj` wraps `jj_core::diff::ContentDiff` and `trivial_merge` for jj's semantics (`DESIGN.md` §B2).

### 2.3 Alignment

`merge3` aligns a merge's inputs. `Inputs` holds n sides and n − 1 bases in jj's order; a three-way merge is left = side 0, base = base 0, right = side 1. Base 0 is the reference: every other input diffs against it, and regions are found in its line coordinates. `merge3::align` returns `Vec<Region>`: one line range per side and per base, and a `Kind`: `Unchanged`, `OneSide(i)`, `AllSame`, `WordMerged` or `Conflict`. The regions cover every input in order, with no gaps.

`Alignment` picks the rules: `Semantics::Git` joins overlapping or touching changes as `merge-file` does; `Semantics::Jj` makes each jj hunk a region. `Refine` splits shared lines off a conflict's ends (`zdiff3`-style) or deeper. `HunkLevel::Word` merges a conflict word by word when every word resolves. A caller with a grammar passes `boundaries::Boundaries` to keep refinement cuts out of syntax nodes (`DESIGN.md` §B5.8); `merge-core` itself knows no grammar. `merge3::moves` finds blocks a side moved.

### 2.4 The document

`document::MergeDocument` is one merge session's state:

- the `Inputs`, labels, options and regions;
- one `Resolution` per region: `Auto`, `Pick(sources)`, `Lines(picked runs)`, `Edited` or `Unresolved`;
- the result text in a `ropey::Rope`, which is the concatenation of every region's text;
- each region's byte length and line count in Fenwick trees (`lengths`, `region_lines`), so offset↔region lookups cost O(log n);
- undo and redo stacks of steps.

`MergeDocument::apply(Command) -> Result<Applied, CommandError>` is the one way to change it. `Command` is `Toggle`, `ToggleLines`, `Clear`, `Pick`, `PickAll`, `Edit(EditBatch)`, `Undo` or `Redo`. `Applied` carries the `TextPatch`es that turn the old result into the new one, sorted and disjoint, and whether regions merged or split. A pick returns the patch the editor must apply; a typed edit, which the editor has already made, returns only the line-ending corrections. `output()` gives `Output::Resolved(text)` or `Output::Conflicted(text with markers)`; `markers` writes and reads git's and jj's marker styles.

Typed edits that cross region boundaries merge regions; `Edited` marks text that lives only in the result. `suggest` offers resolutions the user may take from a region's menu; the core never applies them itself.

### 2.5 Two-way diff

`diff2` is the two-way counterpart: `regions(left, right)` gives `Region2`s of `Kind2`, and `DiffDocument` holds two editable buffers with their regions and undo. `Recompute`, `Marking` and `Patching` are snapshots of both texts (O(1) rope clones) that compute regions, intra-line marks and a patch off the UI thread; each result carries the version it belongs to. `diff2::moves` finds moved blocks; `patch` prints regions as a `git diff`-style unified patch.

### 2.6 Around the edges

- `journal`: the recovery journal's format, JSON Lines of a header (input hashes, options) and one `Command` per line; restore replays them.
- `edit`: `TextPatch` and `EditBatch`, shared by the merge result and the diff buffers.
- `index`: newtypes `RegionIx`, `Line`, `ByteOffset`, so a swapped argument does not compile (`DESIGN.md` §A13.1).
- `line_map`: where old lines land after a reload from disk, to keep the caret.
- `rename`: content-based rename and copy detection, as jj's git backend does it.
- `unusual`: characters that draw as nothing or as a plain space.
- `testing` (feature): a seeded generator of large triples for benchmarks and app measurement.

## 3. `vcs`

The contract between triad and a version control system. It holds the `Repository` trait and the types both backends fill, and depends on nothing but `camino` and `thiserror`.

The types follow jj's model (spec §1.2): a `Conflict` has n `sides` and n − 1 `bases`, each a `ConflictTerm` with an optional label (`Term`) and `TermContent`. git fills the same types with two sides and one base. A fact only one VCS records is an optional field, never a second code path in the front ends.

`Repository` (object-safe, `Send`) groups its methods by job:

- **Identity:** `kind() -> RepoKind`, `root()`.
- **Reading trees:** `file_at(path, &RevSpec)`, `entry_at`, `tree(rev) -> Box<dyn Tree>`, `changed_paths(rev)`, `default_parent(path)`.
- **Conflicts:** `conflicts()` lists conflicted paths; `read_conflict(path)` gives the terms; `recorded(path)`; `conflict_style()` reads the user's marker style.
- **Line origins** (spec §7.3): `annotate(path, commit) -> Annotation`, `describe(commit) -> LineOrigin`, `merge_candidates()`, `copy_candidates()`, `working_copy_id()`, `index_base()`, `commits(revset)`, `merge_base`. Each backend says which commits an input may come from; `merge-session` matches them by content.
- **Writing:** `snapshot()` takes the working copy in; `record_save(path, resolved)` records a save.

`RevSpec` names a revision as the user typed it; `CommitId` wraps a hex ID. `VcsError` covers a missing repository, a path outside it, non-UTF-8 paths, a missing working copy, and `Backend { context, source }` for anything the library returns.

There is no backend registry. `merge-session` picks the backend by the nearest `.jj` or `.git` (`DESIGN.md` §A13.6).

A jj-native feature, one git cannot have, does not join the trait. It lives in `vcs-jj`'s own API and reaches the front ends through `merge-session` (§4.2).

## 4. The backends: `vcs-git` and `vcs-jj`

Both reach their VCS only through its library, never its executable. gix is exact-pinned to the version jj-lib uses, so the build holds one copy (`DESIGN.md` §A3); `script/check-workspace` enforces it.

### 4.1 `vcs-git`

`GitRepository` wraps a `gix::Repository`. `discover` searches up from a path; `open` takes a root.

- **Conflicts** come from the index: `read_conflict` reads stages 1–3 of a path, and labels them from the operation git stopped in (merge, rebase, cherry-pick, revert), which `merge_candidates` also reports.
- **Origins** use gix's `blame`.
- **Saving:** `snapshot` does nothing, since git takes the working tree in only at `git add`. `record_save` stages a resolved file through git's clean filters, as `git add` would.
- `tool_config` writes the `mergetool` and `difftool` entries of spec §7.1 into a git config file.
- `testing` (feature) builds repositories in temporary directories through gix, conflicted ones through `gix-merge`; the product never merges trees.

### 4.2 `vcs-jj`

`JjRepository` holds a jj-lib `ReadonlyRepo` at the workspace's single head operation, the workspace name and root, and the jj config its snapshots read. More than one head operation is an error: triad refuses concurrent operations and never reconciles them.

- **Conflicts** are the working-copy commit's merged tree value: its adds are the sides and its removes the bases, in jj's order, so an n-sided conflict reaches the core whole.
- **Saving:** `snapshot` and `record_save` take a working-copy snapshot as jj-cli does, first importing a moved git `HEAD` in a colocated workspace. `record_save` is an ordinary operation that `jj undo` reverts.
- `config` layers jj's config as jj-cli 0.47 does (defaults, system, user or `JJ_CONFIG`, repository and workspace secure config, `JJ_*` overrides) and resolves `--when` scopes. Snapshots honour the user's ignore and track rules; `merge_settings` reads `merge.same-change` and `merge.hunk-level` (`DESIGN.md` §A13.6, §B3).
- `tool_config` writes the merge and diff tool entries through jj-lib's config layer.

The jj-native API (`DESIGN.md` part I) sits on `JjRepository` outside the trait:

- `native`: `stack(revset)` lists conflicted commits and their paths; `conflict_at`, `parent_conflicts_at` read a conflict in any commit; `write_conflict` writes a resolution into a commit in one transaction and rebases its descendants. `CommitEffect` reports what a write would do to each descendant, for the stack window's preview.
- `history`: a file's versions in the working-copy commit across the operation log, and `working_copy_at(op)`.
- `absorb`: the working copy's hunks against its parents, each with the ancestor it goes into, and the moves the user chose.

Every write follows jj's own order: snapshot, rewrite in one transaction, rebase descendants, check out a moved working-copy commit. `testing` (feature) builds jj repositories through jj-lib, in the default (colocated) layout unless a test names another. `examples/` generate the jj oracle corpora from jj-lib and from jj's CLI (`DESIGN.md` §B5.2, §I5).

## 5. `merge-syntax`

Languages and their tree-sitter grammars, for two consumers: syntax colours in the top panes, and syntax boundaries in the merge. tree-sitter stops here; no crate below it links a grammar, and it has no UI.

### 5.1 Languages

`language::Language` enumerates the supported languages and maps each to its grammar. A merge tool's inputs often carry temporary names such as `BASE_1234.rs`, so the caller lists every name it knows, best first: the inputs, the output, then the `--title` path. The first whole name or extension triad knows wins; failing that, a `#!` line; failing that, plain text (`DESIGN.md` §F5).

### 5.2 Highlighting

`syntax::Grammar` wraps a tree-sitter language with its highlight and injection queries. A tree-sitter tree costs about 90 bytes per byte of text, so a 100k-line input does not fit the memory target whole. `syntax::highlight` parses the text in chunks of about `CHUNK_LINES` lines, each cut before a top-level line, and keeps only the spans: `LineSpans`, one list of `Span`s per line. A span keeps every capture over it, outermost first, so the theme can fall back to an outer colour. Injections parse one level deep: Rust macro bodies as Rust, HTML scripts as JavaScript.

`chunk_starts`, `cut` and `highlight_lines` serve `merge-view::chunks`, which parses the chunks on screen on demand (§7).

### 5.3 Boundaries

`boundaries::SyntaxBoundaries` implements `merge_core::boundaries::Boundaries`. For each region it parses a window from the top-level line at or before the region to the next top-level line after it, never the whole file. Refinement and the word merge then avoid cuts that would split a syntax node (`DESIGN.md` §B5.8). A window over `MAX_WINDOW_LINES`, or a parse error across the region, leaves every cut allowed, so a bad parse never blocks a merge.

## 6. `merge-config`

The config file and what it holds: its format, the settings, the key bindings, and the names of their values. It reads files but writes none except the template, opens no repository and parses no command line. `script/check-workspace` holds it to `merge-core` and `vcs`.

- **`config`** parses the format (`DESIGN.md` §E3.1–3.2): `key = value` lines, `#` comments, and `config-file` includes read in place. It returns `Parsed`: the `Entry`s in order and the `ConfigError`s. It does not know the keys. File access goes through a `Files` trait (`Disk` in the product), so tests run on in-memory files.
- **`settings`** knows the keys. `Settings` is one struct of every setting: merge options, diff options, colours (`Themed<Colour>` per light and dark), fonts, wrap, layout, scrollbars, the revision ID kind, the UI language, window size and the key bindings. `load(path) -> Loaded` reads a file into `Settings` plus a list of `SettingsError`s; a bad line is reported and skipped, never fatal, and `merge-app` shows the list in its settings banner. `render` prints the settings back in the file's form for `--show-config`; `template` writes a commented default file for Settings….
- **`keybind`** parses `keybind` lines into `KeyBindings`: `Chord`s of `Modifier`s and a key, per `Platform`, bound to `KeyAction`s. It knows no GUI; `merge-app::keys` turns the bindings into GPUI's (`DESIGN.md` §E9).
- **`named`** gives each enum a setting or flag names its kebab-case name and help text through strum derives. Parsers in the config file, the key bindings and `merge-cli`'s clap setup are built from `Named::names` and `Named::help`, so no list of names can miss a variant.

A command-line flag overrides the config file for one run. Under jj's semantics, `merge.same-change` and `merge.hunk-level` start from the user's jj config, and a triad setting or flag overrides each (`merge-session::jj_config`, `DESIGN.md` §B3).

## 7. `merge-view`

The view model: everything the windows compute that is not drawing. It turns a `MergeDocument` or a `DiffDocument` into rows, positions, marks, labels and counts. Pure Rust with no GPUI, so its logic runs under plain unit tests.

### 7.1 The merge window

- **`rows`**: `RowMap` aligns the top panes. Each region takes as many rows as its longest visible pane; shorter panes get filler rows at the region's end. `PaneSet` says which inputs show; `RowIx` indexes rows. `RowMap` keeps a Fenwick tree of row counts, so a region's rows cost O(log n).
- **`heights`** maps items to display rows when lines wrap or regions fold. Scroll sync and jumps work in item positions; panes scroll by display rows.
- **`scroll`**: `ScrollSync` keeps the top panes and the result in step. Each pane has an anchor, a content position at the middle of its view that moves to the edge near the top or end. Anchors line up by region and by share of the way through it: a point 40 % through a region's rows meets 40 % through its result lines and placeholder rows.
- **`nav`**: next, previous, first and last region of a `Target`: `Difference` or `Unresolved`. Navigation stops at the ends; it never wraps. `held_region` resolves the caret and a stored `Pin` to a valid region after an undo or a realignment (`DESIGN.md` §A13.2).
- **`marks`**, **`ruler`**, **`placeholder`**, **`merge_fold`**: each region's gutter label and tone, the overview ruler's marks, the hatched row that stands for an empty region in the result, and folded unchanged regions.
- **`status`**: `Status` counts conflicts, unresolved regions and differences for the status bar; `pane_headings` and `result_heading` title the panes.
- **`origin`**: line origins per row and per result line, the gutter label at the start of each run, and the hover text.
- **`inherited`**, **`moves`**, **`line_pick`**, **`selection`**, **`search`**: conflicts inherited from a parent commit, moved-block ends, which lines a press picks, selection in a read-only pane, and literal search over every visible input with Aho-Corasick.

### 7.2 The diff window

`diff` gives the two-way window its bands, pieces, spacer rows, moved-block ends and navigation over a `DiffDocument`. `fold` collapses unchanged regions, keeping `context` lines facing each change. `unified` builds the read-only unified view's text with both sides' line numbers.

### 7.3 Drawing support

- **`chunks`**: `ChunkedSpans` schedules syntax parses for the top panes. A row on screen asks for its line; that queues its chunk and neighbours. `next_parse` hands out the queued chunk nearest the focus while fewer than `Limits::in_flight` parses run; at most `Limits::cached` chunks keep their spans. The window runs the parses on GPUI's background executor.
- **`highlight`**: intra-line character highlights per region, in a small LRU cache that survives edits that keep the regions.
- **`runs`**: a cell's text as styled runs, syntax colours with highlights and search matches over them. The style type is generic, so no GPUI type crosses in.
- **`tabs`**: a line as it draws outside an editor: tabs to stops, invisible characters as badges, visible whitespace as symbols.
- **`blink`**: the blink state after a jump. The window owns the timers.

## 8. `merge-session`

The layer under both front ends. It owns everything a merge does outside the engine and outside the UI: which semantics apply, which repository a run works in, how inputs load, how output saves, how the journal keeps work across a crash, and what each exit means. No GUI and no argument parser.

### 8.1 Forms and the repository

`invocation::Window` lists the forms that open a window: `OpenDialog`, `Merge`, `Diff`, `DiffDirs`, `DiffChanges`, `DiffNative`, `Resolve`, `Stack`, `History`, `Absorb`. `merge-cli` fills them; `merge-app` matches on them. `WindowForm` bundles one with its settings, settings errors, working directory and repository.

`repository::RunRepository` is the one repository of a run. It holds the directories the repository may lie in, in order, and opens the first match on first use; semantics, marker style, `resolve`, origins and saves all read the same open repository. `native::nearest_repo` finds the nearest `.jj` directory or `.git` entry, jj on a tie; a `.git` git cannot open is `NotARepository`, never a climb to an outer repository. A `Handoff` carries the repository to a second window, such as from the stack window to a merge window. `repository::Discovery` is how a run finds its repositories: a front end builds it once from the run's `Env` (`Discovery::from_env`), and every repository of the run, the windows' later ones included, comes from its constructors, so jj's snapshots, writes and revsets read the user's jj config. `RunRepository::discovery` hands it to the next window.

`diff_tool` holds the two-way diff's file rules: `read_pair` reads the sides of `diff LEFT RIGHT` and opens the working-copy file for jj's right-side copy (spec §7.1); `save` writes a diff window's sides and snapshots the jj working copy.

### 8.2 Choosing how to merge

`semantics::choose` picks `Semantics::Git` or `Semantics::Jj`. A repository with `.jj` merges with jj's semantics, and a git merge there is an error (`DESIGN.md` §B3). `jj_config` maps jj's `merge.same-change` and `merge.hunk-level` onto the engine's options. `choose_marker_style` reads the repository's conflict style. `apply_semantics` runs both and sets the result in a form's `MergeOptions`, the one step each front-end form takes. `language` names the grammar for syntax boundaries.

### 8.3 Loading

- `load` reads a merge's files into `Inputs` and builds the `MergeDocument`.
- `resolve::load` reads a conflict for `resolve`: from the repository through `read_conflict`, or else from the file's conflict markers (`merge_core::markers`). `load_at_commit` reads one in another jj commit for the stack window.
- `native` loads the inputs of `diff PATH` (the working-copy file and its parent revision, or a jj operation's version) and lists changed files for `diff [--rev]`.
- `dir_diff` lists the files that differ between two directories.
- `origin` finds which commit each input's line origins come from: one whose file holds the input's exact bytes, or for the working copy its parent by a line diff (`DESIGN.md` §C3). `carry` moves an annotation onto an edited text.

### 8.4 Saving and ending

`session::Session` holds one merge window's state below the UI: the `MergeDocument`, its `Recovery`, the `SaveTarget`, whether a save runs, and the last save's outcome. A save becomes a `SaveJob` with no borrow of the session, which the window runs on a background thread; `Session::saved` takes back the result.

`ending` decides each step of Save, Save & Exit and Close as pure functions: they take what the window knows and return a `Next` that says what to ask, write or finish. The window does the asking. This keeps the decision table under unit tests and the GPUI code thin.

`resolve::Destination` says where a result goes: a file, with the VCS conflict a save records when there is one, or a jj commit's conflict (`InCommit`). `EncodedOutput` encodes the result; `save` writes a file atomically (`io::write_atomic`: a temporary file beside the target, synced and renamed over it) and then records it, or hands the bytes to the commit. `SaveError` says which part failed and whether anything was written.

### 8.5 Recovery

`recovery::Recovery` builds the journal's writes; the front end runs each write and its sync on a background thread, since a sync on macOS costs about 4 ms. `Locator` finds an earlier journal for the same inputs; a private `Gate` orders writes and removals across journal generations with one lock (`DESIGN.md` §A13.4). `replay` applies a file of commands for `--headless --commands`, in the journal's encoding.

### 8.6 Other services

`install` writes tool entries through each backend's `tool_config`. `jj_native` opens the jj repository for `stack`, `history` and `absorb` and refuses a git one. `stamp` fingerprints OUTPUT so the window can tell another program changed it. `env` captures the environment variables that locate git's, jj's and triad's config.

## 9. `merge-cli`

The command line and every form that runs without a window. A library with no GUI dependency, so its tests run without GPUI.

- **`args`** parses spec §6.1's command line with clap's derive API into an `Invocation`:
  - `Window(merge_session::invocation::Window)`;
  - `Batch`: `Headless`, `Resolve` (headless), `Inspect`, `DiffInspect`, `ChangesInspect`, `Install`, or `Jj(JjBatch)` with `Stack`, `History` and `Absorb`;
  - `ShowConfig`.

  `parse_with` takes the loaded `Settings`, so a flag's default is the config file's value. Flag names and values come from `merge-config::named`.
- **`lib`**: `run(args, settings_path, stdout, stderr) -> Outcome` is the entry point. It loads the settings, parses, and either runs a batch form to an `ExitCode` or settles a window form (repository, semantics, `resolve` target) and returns `Outcome::Window(WindowForm)`. stdout and stderr are parameters, so the tests capture them. `headless` writes the automatic merge, with replayed commands, without a window.
- **`inspect`** prints a merge's regions, or a two-way diff, as text or as the JSON of `docs/json-schema.md`, or a unified patch.
- **`jj_native`** formats the reports of the jj-native batch forms as text or JSON.
- **`install`** names this program and reports what `merge_session::install` wrote.
- **`logging`** sets up `tracing` to stderr and a daily file in the platform cache directory, both filtered by `TRIAD_LOG`.
- **`forward`** is the Windows console binary's half of a GUI launch: start `triad-merge-gui`, wait, pass its exit code on.

Errors are `CliError`, wrapping `merge-session`'s; `run` prints one line and exits with 3.

## 10. `merge-app`

The GPUI shell: windows, panes, editors, toolbar, menus and keys. It draws what `merge-view` computes and sends what the user does to `merge-core` through `merge-session`. The editor is gpui-component's, from the vendored, patched `gpui-base` (`DESIGN.md` §A6.1).

### 10.1 Entry

`run::run(WindowForm) -> ExitCode` sets the UI language, builds the window settings from `Settings`, loads the inputs, opens the window for the form and runs the GPUI app until it finishes. `resolve_queue` opens each conflicted file in turn for `resolve` without a path.

### 10.2 The merge window

`window::MergeWindow` (`DESIGN.md` §A6.2) keeps each job's methods in a child module of `window`: `output` (watching OUTPUT), `origins` (line origins), `inputs` (input selection), `measuring` (the measurement harness), `scrolling` (scroll sync, wrapping, folds), `journal` (the recovery journal), `save` (the save flow) and `search` (find). `window.rs` holds the type, its construction and its rendering. It holds the `Session`, the `RowMap`, the result editor (`Entity<EditorState>`), region bands, highlights, syntax state, scroll, status counts, ruler marks and the journal tasks. Its loop:

1. The user types in the result editor. The editor applies the edit at once.
2. `recorder` turns the editor's report into an `EditBatch`; `editor_sync` sends it as `Command::Edit` through the session to `MergeDocument::apply`.
3. A pick, undo or redo goes to `apply` directly.
4. `apply` returns patches. `editor_sync` applies them to the editor with recording muted; `patches` places the caret.
5. When regions changed, the window rebuilds its `RowMap`, bands and marks from `merge-view`.
6. The command goes to the journal, written on the background executor.

The top panes are rows, not editors: `pane_scroll`, `row_wrap` and `sideways` scroll and wrap them; `input_select` and `input_find` give them selection and search driven by the result's search session.

### 10.3 Other windows

- `diff_window`: the two-way diff, two editable panes over a `DiffDocument`, with region recomputation debounced off the UI thread; `unified_pane` for the unified layout.
- `dir_window`, `stack_window`, `history_window`, `absorb_window`: tool windows that list things and open a merge or diff window per item. All share `tool_window` (frame) and `pick_list` (rows).
- `open_dialog`, `about`, `prompt_view` (Linux alerts), `menu_bar` (Linux and Windows in-window menus).

### 10.4 Shared parts

- Chrome: `toolbar`, `menu_button`, `button`, `tooltip`, `status_bar`, `alert`, `settings_banner`, `elevation`, `corners`, after Zed's components and `docs/STYLE.md`.
- Panes: `ruler`, `scrollbars`, `wrap`, `origin_column`, `move_pill`, `blink`, `go_to_line`, `tones` (region colours from the settings).
- Syntax: `syntax` and `editor_syntax` colour the top panes and editors, parsing off the UI thread; `syntax_colours` adjusts the light theme for contrast.
- Files: `file_watch` and `output_watch` notice changes another program makes; `own_writes` tells the window's own writes apart.
- Text: `strings` holds every string a window shows, each a Fluent message from `locales/` formatted by `i18n`; `error_text` names each error in the run's language.
- `keys` turns `merge-config`'s `KeyBindings` into GPUI's.
- `measure`: `TRIAD_MEASURE=<mode>` makes the merge window drive itself and report frame times (`DESIGN.md` §A6.7).

### 10.5 Threads

The UI thread never waits on disk, a repository or a parse. Saves, journal writes, line-origin lookups, syntax parses, search and diff recomputation run on GPUI's background executor over cheap clones (`Text` and `Rope` share their buffers). A result carries the generation it began in; one from an older generation drops.

## 11. `triad-merge`

The binaries. It holds no logic.

- `main.rs` (`triad-merge`): initialise logging, find the settings path, call `merge_cli::run`. On `Outcome::Window` it calls `merge_app::run`, except on Windows, where it starts `triad-merge-gui` beside it and forwards the exit code, so the console process keeps git's or jj's console.
- `gui.rs` (`triad-merge-gui`): the Windows GUI-subsystem entry point. It takes the same command line and runs window forms itself.

The crate depends on `merge-cli` and `merge-app`, so it is the only place the GUI and the CLI meet.
