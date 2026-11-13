# triad — specification

triad is a native three-way merge and two-way diff tool for single text files. It is built for jj, with full parity for git. It takes base, left and right, and writes a merged output file, with p4merge's merge workflow. It runs as the merge and diff tool of jj and git, and reads repositories through jj-lib and gix to resolve conflicts in place and show where each line came from. macOS 27 on Apple Silicon ships first; Windows 11 and Linux follow the macOS beta.

## 1. Scope

### 1.1 Release 1

- Three-way merge of one text file: base, left, right → output.
- GUI in p4merge style: left and right panes side by side, each showing its changes from base, and the editable result pane below.
- Non-conflicting changes apply automatically. The user resolves each conflict by choosing a source or editing the result.
- Runs as a merge tool from `jj resolve` and `git mergetool`.
- Two-way diff of one text file pair: two editable panes side by side (§4.6). Runs as a diff tool from `jj diff --tool` and `git difftool`. `triad-merge diff PATH` compares a working-copy file with its parent revision through jj-lib or gix, and saves edits straight to the working copy.
- A headless CLI mode for scripts, tests and agents.

### 1.2 jj first, git parity

jj leads design, defaults and examples. git has equal support at every stage:

- Every feature works on both, and ships for both in the same milestone.
- The `vcs` types follow the richer of the two models, which is jj's: a conflict has n sides and n − 1 bases, and each term may carry a label. git fills the same types: two sides, one base, and labels from `HEAD`, `MERGE_HEAD` or the rebase state.
- Facts that only one VCS records, such as jj change IDs, jj's per-term conflict labels and git's staged-versus-`HEAD` distinction, are optional fields. The UI shows them when present and works without them.
- One conformance suite (§8.8) gates each feature on both backends.
- Each backend crate holds its VCS's rules behind the `vcs::Repository` trait: revision syntax, which commits an input may come from (§7.3), the conflict style git's config asks for, and the marker length jj recorded for a file it wrote. Merge rules stay in `merge-core`, jj's among them: `merge_core::jj::marker_length` gives the length jj's materialiser writes, from jj-core. Outside the backends, triad branches on the VCS only for the tool protocols of §7, merge semantics, tool config and display; `resolve` takes jj's length for a jj conflict without a record (§7.4). gix and jj-lib appear outside the backends only to read and write tool config (§7.1). `vcs_jj::config` layers jj's config files, for both jj's merge settings and the snapshot options.

Built after release 1:

- Chunk origin: the commit, author and change that introduced each chunk (jj-lib annotate, gix blame), §7.3.
- Native conflict resolution: read a conflicted file straight from the repository and write the resolution back. For jj this means the working-copy commit's tree, through jj-lib, including conflicts with more than two sides. For git it means the index's conflict stages, through gix. §7.4.
- Syntax highlighting (tree-sitter), §6.4.
- Merge semantics by repository: jj's in any repository with `.jj`; git's in a git repository. triad does not support git merges in a jj repository. Word-level merging and other refinements are opt-in (§4.3).

Later:

- Folder and multi-file merges.
- Fuzzing (§8.9).

### 1.3 Excluded

- Binary and image merging.
- Modify/delete and rename conflicts. git and jj resolve these themselves and never pass them to a merge tool. Add/add conflicts are in scope: the base is empty.
- Intel Macs, 32-bit targets and Linux on ARM.
- Running the `git` or `jj` executables. All VCS access goes through gix and jj-lib.

## 2. Platform and tooling

| Item | Choice |
|---|---|
| Licence | MIT. Every crate sets `license = "MIT"`. |
| Language | Rust edition 2024 on the stable toolchain. `rust-toolchain.toml` names channel `stable`; `rust-version = "1.99"` in the workspace sets the minimum. |
| UI | GPUI through `gpui-kit` 0.8.1 from crates.io, with triad's patched `gpui-base` in `vendor/` |
| Build | `cargo` only. Scripts build each platform's package. No Xcode project and no Xcode GUI. |
| Tests | `cargo nextest run` is the test runner, locally and in CI. `.config/nextest.toml` sets `slow-timeout` with `terminate-after`, so a hung test fails instead of stalling the run. It also defines a `ci` profile with JUnit output and a `quick` profile for the commit gate. nextest cannot run doctests, so `script/checks` and CI also run `cargo test --doc --workspace`. |

### 2.1 Targets

| Platform | Rust target | Minimum OS | GPUI renderer | Package |
|---|---|---|---|---|
| macOS, Apple Silicon | `aarch64-apple-darwin` | macOS 27 | Metal | None: a local build, `cargo install --path crates/triad-merge`, or `script/macos-app` for a local `triad.app` with the Dock icon. Release 1 is for local use; a signed package comes later |
| Linux x64 | `x86_64-unknown-linux-gnu` | glibc 2.35 (Ubuntu 22.04 era); Wayland and X11 | Vulkan | AppImage and `.tar.gz`; Flatpak later |
| Windows x64 | `x86_64-pc-windows-msvc` | Windows 11 | DirectX 11 | Authenticode-signed MSI and winget manifest |
| Windows ARM64 | `aarch64-pc-windows-msvc` | Windows 11 | DirectX 11 | Same as x64 |

macOS is outside the App Store. The App Store sandbox blocks access to the arbitrary paths a merge tool receives.

`images/` holds the mark, the logo and the app icon: a T in three pieces, the left arm graphite, the right arm light grey, the stem green. The arms are the inputs and the stem is the result. These are brand colours, separate from the tones of §5. `script/gen-images.swift` writes every file. `docs/mark-colours.md` lists the scheme in use and the alternatives. The Linux packages carry `triad-icon.svg`, and every window names the desktop entry `triad-merge` as its app ID. `triad.app` carries `triad.icns`. `triad.ico` is for the Windows package.

### 2.2 Cross-platform rules

- Only `merge-app` touches platform APIs, and those calls sit in a `platform` module with one file per OS. All other crates build and pass their tests on every target.
- Key bindings use GPUI's `secondary` modifier: ⌘ on macOS, Ctrl on Linux and Windows. §6.3 shows the macOS keys.
- Menus: the global menu bar on macOS, and gpui-component's title-bar menu on Linux and Windows, after the mark. Both are built from one menu definition. The View menu ticks the views the active window shows: Base Pane, Blame, Syntax Highlighting, Soft Wrap and Sync Result Scrolling. Blame shows only once the window's line origins load (§7.3). The diff window has no base pane and no result, so it never ticks those two. Soft Wrap stays unticked when the files are too long to wrap.
- Prompts: the platform's alert on macOS and Windows, and triad's own on Linux. There, Enter takes the first button. Escape takes Keep Editing, Keep Mine or Cancel, or the only button, and otherwise does nothing, so it never discards a recovery journal.
- A prompt that can lose work puts the answer that keeps it first, so the default button (Enter) never loses it: Save & Exit or Save when a window closes with changes, Keep Mine when a file with edits changes on disk, Restore for a recovery journal, Try Again after a failed save.
- Paths use `std::path` and `camino`. No code assumes `/` separators, case-sensitive file systems or POSIX permissions.
- CI builds, lints and tests on Linux x64, macOS ARM64 and Windows x64 for each push to `main` and each pull request, and weekly. Windows ARM64 joins CI, on native ARM64 runners, before version 1.0. Linux ARM64 is not a target.

### 2.3 No unsafe code

triad's own crates contain no `unsafe` code. Third-party crates may use it.

- The workspace `Cargo.toml` sets `[workspace.lints.rust] unsafe_code = "forbid"`. Every crate sets `[lints] workspace = true`, and CI fails if a crate lacks it.
- `forbid` cannot be overridden with `#[allow]`. A need for FFI or a platform call is met by a third-party crate with a safe API, never by local `unsafe`.
- The spike crate (§11) is exempt. It overrides the lint with `unsafe_code = "warn"`.
- Design choices follow from this rule. Atomic output writes use `std::fs::rename`, which replaces an existing file on all three platforms. The Windows console binary does not hide or detach consoles, which would need raw Win32 calls.

### 2.4 Lints and formatting

The workspace `Cargo.toml` holds all lint settings. Crates inherit them through `[lints] workspace = true`.

```toml
[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
allow_attributes_without_reason = "warn"
```

- CI and `script/checks` run `cargo clippy --workspace --all-targets -- -D warnings`, so every pedantic warning fails the build.
- A pedantic lint is turned off for the whole workspace only in `[workspace.lints.clippy]`, with a comment giving the reason.
- A local `#[allow(clippy::…)]` or `#[expect(clippy::…)]` must carry `reason = "…"`. Prefer `#[expect]`, which fails once the lint no longer fires.
- CI runs `cargo fmt --check`, with a `rustfmt.toml` at the workspace root.

### 2.5 Design reference: Zed

Zed is the main reference for UI and editor design decisions. It is built on GPUI, solves the same problems at the same scale, and is well engineered. Before designing a feature, check how Zed does it. Examples:

- split diff alignment with spacer rows (its block map)
- conflict resolution in the editor
- scroll sync
- display-map layering
- keymaps and actions
- headless UI tests
- git integration

Licence rule: Zed's editor and application crates are GPL-3.0, and GPUI is Apache-2.0. triad is MIT.

- Study Zed's design, documentation, behaviour and architecture, and describe the approach in our own words in the spec or plan.
- Write the code from that description. Never copy or translate GPL code into triad.
- Code from Apache-2.0 or MIT crates (GPUI, gpui-component) may be reused with attribution.
- Every design taken from Zed names its source (a doc page, blog post or source file) in the spec or plan.

Product name: **triad**, written lowercase everywhere: UI, docs, comments, paths and bundle names, at the start of a sentence too. The binary is `triad-merge` (`triad-merge.exe` on Windows). A bare `triad` would clash with an unrelated CLI of that name on crates.io. Packages (winget, AppImage) also use `triad-merge`.

## 3. Architecture

A Cargo workspace. Each crate depends only on the crates above it in this list.

| Crate | Role | Depends on |
|---|---|---|
| `merge-core` | Text loading, line diffing, three-way alignment, region classification, resolution state, undo, output rendering. No UI and no VCS code. | `imara-diff`, `ropey` |
| `vcs` | The `Repository` trait: detect the repo, read a file at a revision, read a conflict, write a resolution, annotate lines. Shared types for origin and conflicts. Release 1 needs only detection and reading a file at a revision, for native diff mode (§6.1). | — |
| `vcs-git` | `Repository` for git, through gix. | `vcs`, `gix` |
| `vcs-jj` | `Repository` for jj, through jj-lib. | `vcs`, `jj-lib` |
| `merge-syntax` | Languages from file names, tree-sitter grammars, highlight spans, and the syntax boundaries syntax-aware merging keeps its cuts out of. No UI. | `merge-core`, `tree-sitter` |
| `merge-view` | View model: aligned rows for each pane, scroll mapping between panes, navigation, highlight spans, commands. Pure Rust, no GPUI. | `merge-core`, `merge-syntax`, `vcs` |
| `merge-config` | The config file's format, the settings, key bindings, and the names of their values. No repository and no file writes. | `merge-core`, `vcs` |
| `merge-session` | The layer under both front ends: the merge session (the document, its recovery journal, its save target and whether a save runs), the merge semantics, repository discovery, conflict loading, line origins, the recovery journal, reading and writing a merge's files, exit codes, and the window forms. The only crate that picks a VCS backend. | `merge-config`, `merge-core`, `merge-syntax`, `vcs-git`, `vcs-jj` |
| `merge-cli` | Argument parsing, headless merge, JSON inspection, `install`. A library with no GUI dependency. | `merge-config`, `merge-session` |
| `merge-app` | GPUI shell: windows, panes, toolbar, menus, keyboard shortcuts. A thin renderer over `merge-view`. | `merge-view`, `merge-config`, `merge-session`, `gpui-component` |
| `triad-merge` | The binaries: `triad-merge`, plus `triad-merge-gui` on Windows (§6.1). Routes headless commands to `merge-cli` and GUI runs to `merge-app`, so `merge-cli` never links GPUI. | `merge-cli`, `merge-app` |

`merge-app` and `merge-view` only see the `vcs` trait. They contain no git- or jj-specific code. A shared conformance test suite runs against both backends (§8.8).

`merge-app` holds no merge logic. Every user action is a `merge-view` command with a unit test. The GUI translates input events into commands and renders the view model.

## 4. Merge engine (`merge-core`)

### 4.1 Input decoding

Each input is checked in this order:

1. BOM: a UTF-8, UTF-16 LE or UTF-16 BE BOM sets the encoding.
2. Binary: with no BOM, the file is binary if its first 8000 bytes contain a NUL byte, which is git's rule. Binary input is an error (exit code 3). The BOM check runs first because UTF-16 text contains NUL bytes.
3. UTF-8: with no BOM, the file must be valid UTF-8. Invalid UTF-8 is an error (exit code 3). Lossy decoding would corrupt the output.

All inputs are decoded to Rust strings before diffing.

- The output uses base's encoding and BOM. If base is empty, it uses left's.
- If the inputs use different encodings, a warning shows in the status bar and in the `inspect` output.
- Lines end at `\n`, as in git `merge-file` and the result editor. A line includes its terminator: `\n`, or `\r\n` when a `\r` comes before it. A lone `\r` is an ordinary character inside a line. A missing newline at end of file is part of the last line.
- Line endings: each region keeps its source's bytes, so the result may mix endings. The `output-line-ending` setting (§6.4) converts the saved output's endings, conflict markers included; a last line without a newline stays without one, and a lone `\r` stays. The result pane keeps the source bytes. A newline typed or pasted into the result pane takes base's dominant ending: the ending of most of base's lines, with ties going to `\n`. If base has no line breaks, left's dominant ending applies, then `\n`.
- Output is exact at the text level. For any region, the output holds the chosen source's exact characters and line terminators.
- An empty or missing base file is an add/add merge. When left and right differ, the whole file is one conflict, unless refinement (§4.3) splits off lines both sides share.

### 4.2 Diffing

- Lines are interned to integer tokens. The merge diffs base→left and base→right as git `merge-file` 2.54 does, so conflicts form where git's do: git's histogram algorithm, written in `merge-core`; slider compaction without the indent heuristic; and a Myers re-diff of each change group that compaction moved. `imara-diff`'s histogram strips the common prefix and suffix first and anchors elsewhere on repetitive input. Myers and Myers-minimal use `imara-diff`. The two-way diff (§4.6) uses `imara-diff` with the indent heuristic, as git `diff` does.
- Algorithms: Histogram (default), Myers, Myers-minimal. The user picks in settings; the CLI takes `--diff-algorithm`.
- jj semantics (§4.3) use jj's line diff instead, from `jj-core`: one diff of base 0 against every other input. It anchors on base's rarest lines that occur as often in the other input, matched by their longest common subsequence, and trims the common prefix and suffix only where no anchor exists. It gives up anchoring where every line of base occurs more than 100 times. When any input is empty, the whole file is one hunk. The algorithm setting does not apply. jj compares stored bytes; triad compares decoded text, so for UTF-16 inputs, and for inputs that disagree about a byte order mark, triad's regions are those of the decoded text and may differ from jj's.
- Whitespace modes, applied when interning lines for comparison only:
  - `none` (default)
  - `trailing`: ignore trailing whitespace and line-ending differences
  - `amount`: also treat runs of whitespace as one space
  - `all`: ignore all whitespace
- Line endings: `merge-ignore-line-endings` (merges, off by default) and `diff-ignore-line-endings` (the two-way diff, on by default) make `\r\n`, `\n` and a missing final newline compare equal, under any whitespace mode. A lone `\r` stays part of the line. Every whitespace mode other than `none` drops line endings already.
- Whitespace modes and the line-ending rule change which lines count as equal, under both semantics. They never change output bytes. On each line of a region that compares equal under the mode, the output takes the bytes of a side that differs from base. If both sides differ, it takes left's. A `BothSame` region takes left's bytes. The region shows as a dimmed whitespace-only change and is not counted as a difference.
- Changed line pairs get a second, character-level diff for highlighting inside lines. This diff runs lazily for visible rows only.

### 4.3 Alignment and classification

Merge semantics: `git` (default) or `jj`. Under `git`, the engine walks both diffs over base line coordinates and merges hunks that overlap or touch into regions. Under `jj`, each hunk of jj's diff (§4.2) is a region: a hunk where every input agrees is `Unchanged`, and every other hunk is classified as below. Each region has a range in base, left and right, and one of these kinds:

| Kind | Condition | Automatic result |
|---|---|---|
| `Unchanged` | Neither side changed the base lines | base |
| `LeftOnly` | Only left changed | left |
| `RightOnly` | Only right changed | right |
| `BothSame` | Both changed, with identical text | left |
| `WordMerged` | Both changed, with different text, and every word resolves (hunk level `word`) | the words merged |
| `Conflict` | Both changed, with different text | none; needs a decision |

Hunks that touch without overlapping are a conflict, as in git's `merge-file`.

With more than two sides (§7.4), base 0 is the reference: every other side and base is diffed against it, and regions are found in its line coordinates.

Both semantics classify a changed region by jj's trivial resolution (`jj-core` `trivial_merge`), comparing lines under the whitespace mode. Each base cancels one side with the same text. When one text is left, the region resolves to it: `BothSame` (all sides equal) if every side holds it, else the lowest side that holds it. When two texts are left and the same-change setting is `accept`, every changed side made the same change, and the region resolves to the text that sides hold more often than bases. Otherwise it is a conflict. For two sides this gives the table above.

Same change: `accept` (default) or `keep`, jj's `merge.same-change`. `keep` leaves a region whose changed sides made the same change a conflict, in both semantics.

Hunk level: `line` (default) or `word`, jj's `merge.hunk-level`. Under `word`, in both semantics, each conflict is merged again word by word, as jj-lib's `merge_hunk_by_word` does: jj's word diff (`jj-core` `ContentDiff::by_word`) of the conflict's texts in every input, with each differing word hunk resolved by the trivial resolution and the same-change setting. If every word hunk resolves, the region is `WordMerged` and its automatic result is the merged text. If any word hunk does not resolve, the whole region stays a conflict: a conflict is never merged in part. Words compare as bytes, whatever the whitespace mode. The word merge runs before conflict refinement, so a `WordMerged` region is never split.

For every input where git `merge-file` reports no conflicts, automatic output under `git` semantics must equal git's output byte for byte (§8.3). Under `jj` semantics, the regions and automatic output must equal jj-lib's `merge_hunks` for the jj corpus (`docs/DESIGN.md` §B5.2), except for the encodings above.

Conflict refinement: an option, on by default, under both semantics, trims lines that both sides share at the start and end of a conflict into separate `BothSame` regions. A conflict whose sides are all equal, which `keep` leaves, stays whole. These regions have an empty base range; the conflict keeps its whole base range, which its `|||||||` section shows. Lines shared inside a conflict stay in it. This is git's `--zdiff3` behaviour.

Deep refinement (`refine-conflicts = "deep"`) goes further in a three-way conflict, under both semantics. After the ends are trimmed, it diffs left against right inside the conflict (git's histogram, under the whitespace mode). Each block of lines both sides share splits off into a `BothSame` region with an empty base range, when one base position fits the block on both sides: each side's diff from base allows a cut there before and after the block. Between matching lines a cut has one place; at or inside a change it may take any place in the change's base range. When several positions fit, the lowest is taken, so a base line whose place is open goes to the later piece. The pieces between the blocks stay conflicts, since their base ranges rest on that choice. A conflict with more than two sides, or whose sides are all equal, stays whole. Neither git nor jj splits a conflict inside, so the status bar marks deep refinement (§5). Conflicts are never joined: git joins nearby conflicts (its `zealous` and `zealous-alnum` levels) only in the two-way marker style, which triad doesn't write.

Syntax boundaries (`syntax-boundaries = true`, off by default) keep both refinements off the inside of syntax nodes, under both semantics, in a file whose language triad knows (§6.4). The language comes from the file names: the inputs', then the output's, then `--title`; under `resolve`, the conflicted path. A region parses with the lines around it, from the top-level line at or before it to the next one, at most 20 000 lines. Deep refinement drops a shared block whose edge, in either side, falls inside a node that does not hold the whole conflict without its surrounding whitespace. The word merge leaves a region a conflict when a word hunk resolved from a side starts or ends inside a token of that side, such as a string's content or a comment. The setting only rejects: every output is one the merge without the refinement gives. A window too long, a parse with an error or missing node across the region, or an unknown language leaves the merge as without the setting. Neither VCS reads syntax, so the status bar marks the setting when deep refinement or the word level is on (§5).

### 4.4 Resolution model

Each region holds a `Resolution`:

- `Auto`: the automatic result from §4.3, for non-conflict regions only.
- `Pick(Vec<Source>)`: an ordered list of sources, each at most once. An empty list means the region contributes nothing. `[Left, Right]` is "left then right". A source whose text ends without a newline gets the typed ending (§4.1) when another non-empty source follows it, so two sources never share a line.

`Source` names one input of a conflict with any number of sides: `Side(i)` for a side and `Base(i)` for a base. A conflict with n sides has n − 1 bases. A three-way merge has `Side(0)` = left, `Side(1)` = right and `Base(0)` = base. The UI words left, right and base map onto these. Only the GUI layout differs for more than two sides (§7.4).
- `Edited`: text the user typed in the result pane. The text lives in the result buffer; the resolution only records that the region holds it.
- `Unresolved`: the starting state of every conflict.

Rules:

- Clicking a source toggles it. Adding a source appends it to the order. This matches p4merge, where selection order sets output order. Toggling off a conflict's last source leaves it unresolved, so a conflict never resolves to nothing by accident; Clear Choice and None pick nothing on purpose. Other regions toggle down to an empty pick.
- An `Auto` region whose text is one side's counts as picking that side: `LeftOnly` and `BothSame` pick left, `RightOnly` picks right. Its pick target shows filled, and toggling starts from that pick, so toggling left on a `LeftOnly` region removes its text and toggling right gives left then right. An `Unchanged` or `WordMerged` region picks nothing, so its first toggle picks that source alone.
- A take command sets the pick outright: none, base, left, right, left then right, or right then left. None drops the region's text; base restores base's text. Taking a pick the region already holds changes nothing.
- A bulk take sets one pick in many regions as one undo step. Take Left for All Unresolved and Take Right for All Unresolved pick that side in every unresolved conflict; picked and edited conflicts stay. Take Whole Left File and Take Whole Right File pick that side in every region, so the result is that input's text byte for byte, as git's `checkout --ours` and `--theirs`. Regions that already hold the pick stay as they are; a bulk take that changes no region adds no undo step.
- Picking a source for an `Edited` region replaces the edit. Undo restores it.
- The result pane's text is the concatenation of every region's text. An unresolved region shows every side in order, left then right, joined as in a pick of those sides.
- The output text is the result pane's text, with conflict markers in place of each unresolved region.
- In the output, an unresolved region renders as git diff3-style conflict markers (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`) holding left, base and right. jj reads this style when configured as in §7.1. Under git semantics, the `merge` conflict style (§6.4) drops the `|||||||` marker and the base section. Each marker except `=======` carries its input's label when one is given. With more than two sides the region renders in jj's snapshot style: `<<<<<<< conflict`, then `+++++++` before each side and `-------` before each base, labelled or numbered (`side #1`, `base #1`), then `>>>>>>> conflict ends`.
- Markers are 7 characters long by default; `--marker-length N` sets the length, from 1 to 65536. If any input line starts with a run of one marker character (`<`, `|`, `=`, `>`; in snapshot style `<`, `>`, `+`, `-`, `%`) at least that long, the markers become one longer than the longest such run, so no input line can be mistaken for a marker. git keeps the requested length. jj's own materialiser makes markers four longer than the longest marker-like line, counting `+`, `-`, `%` and `\` runs in every style, and parses only markers at least that long; it passes that length as `$marker_length`, which the §7.1 entries hand to triad as `--marker-length`.
- Marker lines end in `\r\n` when base's first line ends in `\r\n` and neither side's reference line ends in a bare `\n`. A side's reference line is the line just before the conflict, or its first line when the conflict starts the file. A line without a newline settles nothing. Otherwise marker lines end in `\n`. This matches git `merge-file`.
- Inside the markers, a section whose last line has no newline gets one, in the markers' ending, so the next marker starts on its own line. This also matches git.
- triad always builds the merge from base, left and right. It ignores whatever the output file contains at start-up, such as markers that git or jj wrote there.
- Suggestions (`merge_core::suggest`) are resolutions triad offers for a conflict and never applies itself. The user takes one from the region's menu (§5), and undo reverts it like any take command. A conflict where every side inserts lines and base has none suggests taking every side in order: left then right in a three-way merge. A conflict where every side keeps every base line and adds lines, each input's lines are sorted and every line ends in a newline suggests the sorted union: the sides' lines ordered by their text without the line ending, each as often as the side that holds it most, with its own bytes. It is offered only when it differs from every side in order. Take Sorted Union replaces the region's text with it, and the region becomes `Edited`. In a three-way merge, a conflict where one side deletes base's lines and the other side changes them suggests carrying the edit when the deleting side inserts those lines, unchanged, at exactly one place in another region that resolves to its text: a move. A block of blank lines is never matched. Carry Edit to Moved Block puts the editing side's text in place of the moved lines there and the deleting side's nothing in the conflict, as one edit: both regions become `Edited`, and one undo reverts both.
- Undo and redo cover resolution changes and edits in one linear history. `merge-core` owns this history. The result pane editor's own undo is disabled, and ⌘Z / ⇧⌘Z go to the core.

### 4.5 Edits in the result pane

The result is one editable buffer. Region boundaries are anchors that move with edits.

- An edit inside one region turns that region into `Edited`.
- An edit across a boundary merges the affected regions into one `Edited` region. Its base, left and right ranges become the union of theirs. Its kind is `Conflict` if any merged region was a conflict, otherwise the kind of the first.
- A zero-width insertion after a byte other than `\n` joins the region that holds that byte, so typing mid-line stays in the region the line started in. At a line start it goes to an empty region at that offset if there is one (the first, if several), otherwise to the region after the boundary, or to the last region at end of file.
- An unresolved conflict shows its sides in the editable text, left then right, with no markers. Typing in it makes the region `Edited`, starting from that text, and the output then holds the edited text without markers.
- Line breaks in typed or pasted text take the typed ending (§4.1): `\n` becomes `\r\n` in a CRLF file, and `\r\n` becomes `\n` in an LF file. A lone `\r` stays.
- A multi-cursor edit is one batch of changes and one undo step.
- Undo restores the region structure as well as the text, so undoing a merging edit splits the regions again.

This model is the hardest part of the core and gets the most tests (§8).

### 4.6 Two-way diff

Two-way diff reuses the decoding (§4.1) and diffing (§4.2) code: the same algorithms, whitespace modes and intra-line diff. One diff runs, left→right. The result is a list of regions, each with a range in left and right:

| Kind | Condition |
|---|---|
| `Unchanged` | Lines equal |
| `Changed` | Lines differ; either range may be empty, for a pure insert or delete |

Identical inputs give one `Unchanged` region.

Both sides are editable buffers:

- After an edit, the diff runs again. It runs on a background task, debounced by `DIFF_DEBOUNCE_MS` (100 ms). Lines outside the changed area keep their regions, so the view doesn't jump.
- **Copy to other side**: replaces the matching range in the other buffer with the current region's text, in either direction. This is an ordinary edit, so undo reverses it.
- Each buffer has its own undo history, owned by `merge-core` as in §4.4, with the undo grouping of §4.5. A newline typed or pasted into a buffer takes that file's dominant ending, or `\n` if it has no line breaks.
- A side is read-only if its file is read-only on disk, or if `--readonly-left` / `--readonly-right` is given. Revision snapshots in native mode (§6.1) are always read-only.

## 5. View model (`merge-view`)

- **Panes**: left and right on top, result below. A base pane between left and right is off by default and toggles with a toolbar button and a shortcut, since p4merge shows base.
- **Aligned rows**: the top panes show the same number of rows. A side with fewer lines in a region gets filler rows, drawn as hatched gaps, so matching regions stay level. A side with no lines in a changed region where base has lines deleted them: its filler rows take the side's colour and a red `−` in the line number column, and the first reads `N lines deleted` in red italics, counting base's lines. Its cells' accessible labels end `: N lines deleted`. The result pane shows such a deletion as a placeholder: one hatched row that belongs to no line, reading `N lines deleted (Left)` in red italics, at the deleting side's place among the region's blocks. The row looks like the block it stands for: in an unresolved conflict, the deleting side's colour with an orange `!` and an orange border; after a pick or an automatic deletion, the source's colour and mark (`L`, `R`, `=` for a same change, `+` for several sources); for an empty pick, the cleared mark `-`. It shows in an unresolved conflict, a one-side or same-change deletion, and a pick that takes the deleting side. Neighbouring deleting sides share one row: `N lines deleted (Left, Right)`. A region picked as nothing shows `Nothing picked (N lines dropped)`, counting its longest input. Edited, unchanged and word-merged regions show none. A placeholder is not part of the text: the caret passes over it and save never writes it.
- **Scroll sync**: the top panes share one scroll position. The result pane follows by region. Each pane has an anchor: a point at the middle of its view, which moves to the top edge over the first half view of scrolling and to the bottom edge over the last, so the panes' tops meet and their ends meet. Scrolling one pane puts the other's anchor at the same share of the region its own anchor is in: a share of a region's rows meets the same share of its result rows, which are its lines and its placeholder rows. A region with rows in one pane only holds the other pane still while the anchor crosses it. The top panes always share one scroll position, since their aligned rows face each other. Scroll sync ties the result to them. The `scroll-sync` setting (§6.4) sets it when a merge window opens. View → Sync Result Scrolling (⌘K) turns it on and off for the window. Turned on, it brings the top panes to the result, down and sideways.
- **Navigation**: next and previous difference, next and previous unresolved conflict, first and last. Each region in the top panes has a hit target on its first row, filled while the pane's source is in the region's pick (§4.4); clicking it toggles that source. On an unresolved conflict each side's target shows half filled in the conflict tone, since the result pane shows every side; clicking it picks that side alone. Clicking anywhere else in a row makes its region current. A jump scrolls its region into view in each pane by that pane's own lines of it: centred when they fit, else with their first line at the top. The result often holds more lines of a conflict than the top panes hold rows, so the two panes can differ in placement; scroll sync goes on from those positions. The current region, unless unchanged, is highlighted in an opaque lime, clear of every tone (§5): a bar at the left edge of each top pane beside its rows, and a 2 px frame around it in the result. The highlight follows the caret however it moves. After a jump with Previous or Next conflict or difference, the frame and bars blink for 0.5 s: hidden and shown in turn every 100 ms, starting hidden and ending shown. A second jump restarts the blink, and a jump that finds no target does not blink.
- **Overview ruler**: a strip 12 px wide at the right edge of the result pane, and of the diff window's right pane, marks every region that is not `Unchanged`, at its lines' share of the whole text. A mark is at least 2 px tall, so a region of one line, or of none, shows. Marks take their region's tone (§5); an unresolved conflict is the conflict tone across the strip, and a resolved one the conflict tone on the left half and its resolution's tone on the right. The strip is the pane's vertical scrollbar: a translucent thumb over the marks is the view's share of the pane's content, at least 16 px tall, placed by the share of the scroll range scrolled, and it shows whatever the `scrollbars` setting. A drag of the thumb scrolls the pane, and holds at either end of its range. A click on a mark makes its region current and scrolls to it, as Next and Previous do, with their blink; this holds for a mark under the thumb when the button goes up without a move. A press elsewhere scrolls that line to the middle of the pane and leaves the caret, and a drag from there moves the thumb. A click on the thumb off the marks does nothing. Marks are visual only; navigation reaches every region from the keyboard.
- **Scrollbars**: the top panes share one vertical bar at their right edge. Each editor, the result and both diff panes, has a sideways bar at its foot while its lines scroll sideways. The diff window's left pane also has a vertical bar. The result and the diff window's right pane take their vertical bar in the overview ruler. The `scrollbars` setting (§6.4) sets when the bars show: always, while the pointer is over the pane, or while the pane scrolls. A hidden bar takes no clicks. A bar overlays the pane's right or bottom edge; the editors keep a 10 px margin there, so it covers no text.
- **Conflict badge**: the right end of the merge window's toolbar counts the unresolved conflicts out of all of them: `5 of 10 unresolved`, on the conflict tone. Once none is unresolved it reads `All 10 resolved` (`Conflict resolved` for one), on green. While the current region (§4.4) is a conflict, the badge leads with its number, counted in file order: `Conflict 3 of 10 · 5 unresolved`, or `Conflict 3 of 10 · all resolved`. It follows the caret however it moves. The total counts `Conflict` regions, so a pick leaves it fixed. A merge without conflicts, and the diff window, show no badge.
- **Current region**: the region the result pane cursor sits in. Navigating moves the cursor to the start of the target region. Clicking a region in a top pane makes it current. Choice commands (§6.3) act on the current region.
- **Long lines**: lines scroll sideways. The top panes share one left offset, which runs from the left edge to the end of the widest line any of them shows. The two diff panes take the same left offset, and so do the top panes and the result with scroll sync on: a pane whose lines end sooner stops at its end and does not pull the other back. A sideways wheel or trackpad move over the top panes scrolls them; a move more vertical than sideways scrolls the rows. A jump (§5, Navigation) takes every pane back to the left edge. Soft wrap wraps the result, the top panes and both diff panes instead. In the top panes an aligned row takes as many display rows as its tallest cell wraps to, so the panes stay level; its continuation rows carry no gutter. In the diff window each region takes as many display rows as its taller side wraps to, unchanged regions included, since panes of different widths wrap one line differently; the other side gets filler rows below it, so the panes stay level. Wrapped panes do not scroll sideways. Scroll sync and jumps count each pane's display rows: a share of a row or a result line is the same share of the display rows it wraps to. The `soft-wrap` setting (§6.4) sets it when a window opens, off by default; View → Soft Wrap and the toolbar's Soft Wrap button turn it on and off. It applies only to files of up to `SOFT_WRAP_MAX_LINES` (10 000) lines in every input, and both controls do nothing above that. The editor wraps every line before its first frame, which costs 2.7 s at 100k lines. Character-level diffs run only on lines under 10 000 characters (`MAX_INTRALINE_DIFF_CHARS`). Longer changed lines are highlighted as whole lines.
- **Intra-line highlights**: in a changed region, the n-th line of each side pairs with the n-th line of base (the first base with more than two sides). A side's cell marks the characters that differ from base. The base cell marks the union of its differences from every side, with overlapping and touching ranges joined. Lines with no partner, lines equal to base and `Unchanged` regions, whitespace-only ones included, get no marks. A line over `MAX_INTRALINE_DIFF_CHARS` is marked whole. Marks depend only on the inputs and the regions, so edits that keep the regions keep them.
- **Status**: the repository first, `git repo`, `jj repo` or `no repo`, found as the semantics find it (§6.1): the working directory's, else that of the output's directory. A diff window takes the directory of `PATH` (`diff PATH`) or of the left file in place of the output's. Then the mode, `git merge` or `jj merge`, with `by word` at the word hunk level (§4.3). After it, one item for each setting under which triad merges unlike that VCS: a whitespace mode other than `none`, or ignored line endings, under jj semantics ("whitespace compared unlike jj"), `same-change = keep` under git semantics ("same changes kept, unlike git") the word level under git semantics ("words merged, unlike git"), deep refinement under either ("conflicts split inside, unlike git" or "unlike jj") and syntax boundaries under either, with deep refinement or the word level ("syntax boundaries kept, unlike git" or "unlike jj"). Then counts of differences, conflicts, word-merged regions (shown only when there are some, so the user reviews them) and unresolved conflicts, plus the encoding, line ending and whitespace mode in use; under `none` with line endings ignored, `line endings ignored` in place of the whitespace mode. `mixed endings`, in the conflict tone, follows when the saved output would hold both `\n` and `\r\n` endings: under `output-line-ending = keep` (§6.4) only. A difference is a region whose kind is not `Unchanged`, so an edit to an unchanged region adds none. The encoding is the output encoding (§4.1), with a warning when the inputs differ. Pane titles are the inputs' labels; without one, the input's file name; without either, Left, Base and Right in a three-way merge, or Side n and Base n with more sides. File names title the panes only: conflict markers carry labels alone (§4.4). The result pane's title is the output's file name; without an output, Result. Each pane heading sits over a bar in its pane's tone (the result's is the edited tone). A heading whose title does not name its role (Left, Base, Right, Side n, Base n, Result, compared without regard to case) also shows the role in a chip before the title. A long title loses its start. After the title, a revision chip names the commit the pane's input shows (§7.3): under jj its change ID, or its commit with `revision-id = commit` (§6.4); under git its commit. Its hover card is the origin column's for that commit. The chip loads with the line origins, and is left out when the input shows no commit. A title that starts with one of the commit's IDs as a word, as the title of a conflict term without a label does (§7.4), shows without it: the chip names the commit, so the ID never shows twice. Conflict markers keep the whole title. The diff window's headings take the same chip.
- **Colours**: left blue, right pink, base yellow, conflict orange, edited purple, text from both sides teal, by default; the `color-*` settings change them (§6.4). Light and dark themes follow the system, or the `theme` setting.
  - Each top pane colours only its own input's changes. A side's rows take its colour where that side changed the region and stay plain where it matches base. The base pane takes the base colour in every changed region. A conflict shows blue on the left and pink on the right.
  - The result pane colours each region by the source of its text, so left text stays blue and right text stays pink. A region picked from several sources, or an unresolved conflict, shows one block per source in that source's colour.
- **Gutters**: line numbers in every pane. The top panes mark each conflict row with `!` in the conflict colour. The result gutter shows each block's source (L, R, B) or the region's state (edited, unresolved, word-merged) with the same colours. A word-merged region shows `W` on a green band until a pick or an edit replaces it; undoing that returns to the merged text. Each block of an unresolved conflict carries an orange `!` and an orange border, so it reads apart from a decided left then right.
- **Line origins** (§7.3): each line of an input with origins names the commit that last changed it. A top pane's row takes its line's origin; a filler row has none. A result line takes the origin of the first input line it shows that has one: an automatic unchanged line shows base, then each side, or only the side whose bytes it takes under a whitespace mode (§4.2); a one-side or same-change line shows its sides; a line of a pick or an unresolved conflict shows its block's source. Edited and word-merged text has none. In the diff window and for a working-copy input, the revision's origins are carried onto the current text by a byte-exact line diff, run with each recompute of regions: an edited line has none until undo restores it, and a reload from disk keeps the origins of the lines the file still holds. Origins load off the UI thread after the window shows; a run outside a repository, or one whose annotation fails, shows none and logs why. View → Blame, and the `show-origin-column` setting (§6.4), show the origin gutter column. The column and the Blame item show only once the origins load: a window without origins has neither, and the Blame key does nothing there. It shows the short ID on the first line of each run of lines from one commit: under jj the change ID, or the commit with `revision-id = commit` (§6.4); under git the commit. In the result pane and the diff panes the column sits in the editor's gutter, left of the region marks: a run scrolled partly out of view shows its ID on its first visible line, and its hover card covers the whole run. The diff window takes the same setting and toggle. The hover card shows the change ID and commit, the author and the author's local time (`2001-02-03 05:05 +01:00`), then the summary.
- **Right-click menu**: right-clicking in the result moves the caret there, unless it is inside the selection, and opens a menu. The current region's suggestions (§4.4) come first, each as `Suggested: ` and the take command it makes, then a separator. Then None, Base, Left, Right, Left + Right, Right + Left, each a take command on the current region, with a check by the one the region holds. A carried edit appears as `Suggested: Carry Left's Edit to Line N` or `Right's`, where N is the result line the moved block starts on. The sorted union and the carried edit appear only as suggestions; the Merge menu's Take Sorted Union and Carry Edit to Moved Block do nothing in a region without one. Cut, Copy, Paste and Select All follow.
- **Selection in the top panes**: a press in a top pane's row focuses the top panes and starts a selection there, in that pane's input; dragging over the pane's rows moves its end. Shift-press moves the end of the selection already in that pane. A double press selects the word under the pointer: letters, digits and `_`, else the one other character there. A triple press selects the line with its terminator. The ends are byte offsets in the input's text, so a selection runs through filler rows without taking anything from them: a point on a filler row is the start of the input's next line. A point inside a tab's spaces goes to the nearer side of the tab; a point left of the text, or in the gutter, is the line's start; right of it, the line's end. The selected characters fill in the selection colour, over the search fills. ⌘C copies the selected text, exactly as the input holds it, terminators and tabs included; with nothing selected it copies nothing. ⌘A selects the whole input of the pane holding the selection, else the left pane. While a drag holds the pointer above or below the top panes, they scroll toward it every 16 ms, by half the pointer's distance past the edge, at least 2 px and at most two rows a step; the end of the selection follows, to the start of the top row's line going up and the end of the bottom row's line going down. Moving back over the panes, or releasing the button anywhere, stops the scrolling. Over a top pane's text the pointer is a text cursor. A press without a drag also makes its region current. The selection ends when the top panes lose the focus, or when its pane hides. The top panes take no edits.
- **Two-way mode**: left and right panes only, with no result pane and no base pane. Both panes are editors (§4.6). Each pane has the editor's search, with a case toggle; replace edits the right pane only. Changed regions show in blue on the left and pink on the right, as in merge mode. The panes always share one scroll position, down and sideways, so their filler rows face each other; the `scroll-sync` setting and Sync Result Scrolling do not apply. Aligned rows, filler rows, navigation (next and previous difference), intra-line highlighting and the status bar work as in merge mode. After a jump with Previous or Next difference, or a click on a ruler mark, a 2 px lime frame blinks around the region in each pane where it has lines, for 0.5 s: shown and hidden in turn every 100 ms, starting shown and ending hidden. A second jump restarts the blink. A step that finds no target stays on the current region and blinks it if it is a difference, so a lone difference shows. Only the focused pane shades the caret's line. Copy-to-other-side buttons replace the choice buttons. Each pane title shows a modified marker while it has unsaved edits. The window opens with the focus in the left pane and both carets on the first difference, scrolled into view. With no difference, the carets stay at the start. A toolbar above the panes has previous and next difference, undo and redo, which act on the focused pane, Find, which opens the focused pane's search, else the left pane's, and the syntax toggle (§6.4).

## 6. Application (`merge-app`)

### 6.1 Invocation

```
triad-merge BASE LEFT RIGHT -o OUTPUT [--diff-algorithm A] [--ignore-whitespace W] [--marker-length N] [--title T]
                                     [--label-left L] [--label-right R] [--label-base B]
                                     [--merge-semantics auto|git|jj] [--same-change accept|keep] [--hunk-level line|word]
triad-merge BASE LEFT RIGHT OUTPUT                       # positional form, as git passes it
triad-merge --headless BASE LEFT RIGHT -o OUTPUT         # auto-merge only, no window
triad-merge --headless BASE LEFT RIGHT -o OUTPUT --commands FILE   # auto-merge, then FILE's commands
triad-merge inspect BASE LEFT RIGHT [--diff-algorithm A] [--ignore-whitespace W] [--merge-semantics S] [--same-change C] [--hunk-level H] [--json]
                                                         # print regions and kinds; no output file
triad-merge diff LEFT RIGHT [--diff-algorithm A] [--ignore-whitespace W] [--title T]
                [--readonly-left] [--readonly-right]    # two-way diff window
triad-merge diff PATH [--rev REV]                        # working copy against its parent revision
triad-merge diff --inspect LEFT RIGHT [--diff-algorithm A] [--ignore-whitespace W] [--json]   # print two-way regions; no window
triad-merge install --git | --jj [--config FILE]          # register as merge and diff tool (§7.1)
triad-merge show-config [--default] [--docs]            # print the settings in effect (§6.4)
```

Merge semantics (§4.3) follow the repository:

- `--merge-semantics auto` (default) merges with jj's semantics when the output lies in jj's resolve directory, a directory named `jj-resolve-<random>`, or when the caller's repository is a jj repository; otherwise with git's. The caller's repository is the nearest directory, from the working directory up, holding a `.jj` directory or a `.git` directory or file. Without one, the output's directory is searched the same way; `inspect`, which has no output, searches the working directory only. A directory holding both `.jj` and `.git` is a jj repository. A `.git` that git cannot open names no repository: the search goes on to the next directory, not further up. The search opens git's repository to check it and never jj's, and an unreadable repository cannot fail the merge. Every step of a run takes this one repository: the semantics, the marker style, the journal name, the status bar and the line origins.
- `--merge-semantics git` in a jj repository or jj's resolve directory exits with 3 before writing anything. The message names the repository or directory and says to resolve with `jj resolve`. `--merge-semantics jj` works anywhere.
- `--same-change accept|keep` sets the same-change rule. Without the flag or the `same-change` setting (§6.4), jj semantics read `merge.same-change` from jj's config, and git semantics take `accept`. triad reads jj's config files in jj's order, later files winning: under Unix `/etc/jj/config.toml` and `/etc/jj/conf.d/*.toml`; the user files (each path in `JJ_CONFIG`, which replaces the system and user files, or else `~/.jjconfig.toml`, then `jj/config.toml` and `jj/conf.d/*.toml` in the user config directory); the repository's file (`jj/repos/<id>/config.toml` in the user config directory, `<id>` from `.jj/repo/config-id`, or else `.jj/repo/config.toml`); and the workspace's file, found the same way through `.jj/workspace-config-id` and `.jj/workspace-config.toml`. A secondary workspace's `.jj/repo` file names the repository. Conditional `[[--scope]]` tables apply as in jj: `--when.repositories` and `--when.workspaces` match the workspace, and `--when.platforms` the platform. The read has no command, host name or environment beyond `JJ_CONFIG`, so `--when.commands` and `--when.hostnames` never match, and `--when.environments` matches only on `JJ_CONFIG`. jj's `--config` arguments are not read. triad only reads these files: it does not migrate a legacy repository or workspace file, or write the metadata jj keeps beside a config ID. A value jj does not accept, a config id other than 20 hex digits, or a file that is not TOML, exits with 3 and names the file.
- `--hunk-level line|word` sets the hunk level (§4.3), with the same precedence as `--same-change`: the flag, the `hunk-level` setting, then under jj semantics jj's `merge.hunk-level`, read from the same files; else `line`. A bad value of either key exits with 3 and names the file and key.

Two-way diff:

- **Save** (⌘S) writes each edited side back to its own path. It uses the atomic write and permission rules from §6.2 and keeps the window open. It writes each side as it stands at the keypress, off the UI thread; saves land in the order asked. A side edited while its write runs keeps its unsaved marker. Close with Save exits once every side is written, and stays open if a write fails.
- Closing the window (⌘W, ⌘Q or the close button) with unsaved edits asks whether to save (the first answer), discard or keep editing. A save that fails shows an alert and leaves the window open.
- Undo and redo act on the pane with the focus. Each side has its own history.
- Next and previous difference (⌘↓, ⌘↑) go from the region holding the focused pane's caret, and put both panes' carets at the start of the target region. While the regions lag the text, the step waits for the recompute off the UI thread, which then starts without the debounce. Steps asked for while waiting run in order once it lands; an edit or a reload drops them. A region reached this way stays current while the caret stays put, so an empty region can be left in either direction.
- Regions are recomputed off the UI thread once edits pause for 100 ms (`DIFF_DEBOUNCE`). The status bar shows the repository (§5), the number of differences and the whitespace mode, or `line endings ignored` under `none` with `diff-ignore-line-endings` (§4.2).
- If a file changes on disk while it's open, the pane reloads it, unless the pane has unsaved edits. Then the pane asks whether to keep its own version (Keep Mine, the first answer) or reload, once for any number of changes while it asks; Reload reads the file as it is then. A reload clears that side's undo history and takes the file's encoding. A file that cannot be read, or reads as the pane's own text in the same encoding, or as a text the window is saving there or last saved there, until the pane next reloads, changes nothing; a save counts from the moment it starts, since its write can reach the watcher before its result. The window watches each file's directory, since editors save by renaming a new file over the old one. It waits 100 ms (`DISK_SETTLE`) after a change for the events that follow it, then reads each changed file once, off the UI thread.
- Exit codes: the window exits with 0. `diff --inspect` follows `diff(1)`: 0 if the inputs are equal under the whitespace mode and line-ending rule, 1 if they differ. Errors exit with 3, as in merge mode.

Native mode (`triad-merge diff PATH`):

- Finds the repository by the rule of `--merge-semantics auto` (§6.4): the nearest directory above the file holding a `.jj` directory or a `.git` directory or file, jj when it holds both. Only that directory's backend opens, at that directory: a `.git` that git cannot open names no repository, and the search does not go further up.
- Shows the parent version on the left, read-only, and the working-copy file on the right, editable. Saving writes straight to the working copy. The left pane is titled by the VCS's label for the revision (git's branch, or `index`), else by the revision (`@-`, `HEAD`, or `--rev` as typed); its revision chip names the commit (§5). A file that does not exist at the revision shows as empty, and the left title ends in "(no file)".
- Parents: for jj, `@-`; for git, the index if the file has staged changes, otherwise `HEAD`. When `@-` is more than one commit (the working copy is a merge), the left side is the file in the merge of their trees, as jj diffs it, and the pane is titled `@-`. A file conflicted there is an error.
- `--rev` picks another revision, in the VCS's own syntax. For jj it is a revset, evaluated with jj-lib's built-in functions; user revset aliases such as `trunk()` are not loaded. A revset naming several commits reads from the merge of their trees.
- jj is read at its head operation, without a snapshot, so `--rev @` shows the working-copy commit as of the last jj command. triad never writes to a jj repository here: when concurrent operations have left more than one head operation, it refuses with an error and leaves jj to reconcile them on its next command.
- jj snapshots the edit on its next command, as it does for any editor.

Started with no arguments, the app shows a compact open dialog, headed by the mark beside two tabs: Merge and Diff. The Merge tab has four file slots: base, left, right and output. The Diff tab has two: left and right, the Merge tab's own left and right, so a path typed on one tab shows on the other. Each slot takes a typed path or one chosen with the platform's file picker; the output picker starts in base's directory. On the Merge tab, Merge, or Enter in a slot, opens the merge window and closes the dialog. On the Diff tab, Compare, or Enter in a slot, opens the diff window as `diff LEFT RIGHT` does, with its repository, line origins and file watch, and closes the dialog. The inputs load and merge off the UI thread; while they do, the button shows a spinner and a second Merge, Compare or Enter does nothing. An empty slot or an input that cannot be read or decoded leaves the dialog open, with the slots kept and the reason shown. A switch of tab clears the reason. Nothing is written until Save & Exit.

`--commands FILE` applies merge commands to the automatic merge before `--headless` writes it, so scripts and tests can pick, edit, undo and redo without a window. FILE is JSON Lines, one command per line, in the encoding `docs/json-schema.md` defines; lines of only whitespace are skipped. Commands apply in order, each to the merge the ones before it left, as the recovery journal replays them. A region is an index into the regions `inspect` prints for the same inputs and options, as the regions stand after the commands before it. The exit code follows §6.2: 0 when the result is resolved, 1 when it is written with conflict markers. A line that does not parse or apply exits with 3 before anything is written, naming the file, the line and the reason. `--commands` without `--headless` is an argument error.

`inspect` merges with the options a merge of the same inputs takes, the settings file's `refine-conflicts` and `syntax-boundaries` included, and with syntax boundaries in the language the inputs' names give, so its regions are the ones `--commands` numbers. It exits with 0 without conflicts and 1 with them. `docs/json-schema.md` defines the `--json` output of `inspect` and `diff --inspect`. Argument errors exit with 3, as other errors do; `--help` and `--version` print to stdout and exit with 0. A subcommand name is recognised only as the first argument, so a file named `inspect` or `diff` can be merged.

The process stays in the foreground until its window closes, because git and jj wait for it to exit. Each invocation is a separate process with one window. Concurrent merges run as separate instances.

Platform details:

- macOS: git and jj run the binary directly, at the absolute path `install` writes. A launch through LaunchServices (`open`) would return at once.
- Windows: a GUI-subsystem binary has no console, so stderr and the headless modes would be lost. The package ships two binaries from one crate: `triad-merge.exe` (console subsystem) and `triad-merge-gui.exe` (GUI subsystem). The console binary runs `--headless` and `inspect` itself. For GUI runs it starts `triad-merge-gui.exe` with `std::process::Command`, waits for it and forwards its exit code. git and jj start it from their own console, so no new console window appears. Merge tool config points at `triad-merge.exe`.
- Linux: a single binary.

### 6.2 Save and exit

- **Save & Exit** (⌘S) writes OUTPUT and quits. The write, and in `resolve` its record in the repository (§7.4), run off the UI thread; the recovery journal is written first. While they run, the result is read-only, picks, edits, undo and redo change nothing, and Save & Exit and Close do nothing: the save ends the session with the merge as it stood at the keypress, or fails and leaves the window open for editing again.
  - All conflicts resolved: exit code 0.
  - Conflicts left unresolved: a confirmation sheet appears. On confirm, the output gets conflict markers for the unresolved regions and the exit code is 1. Under git semantics the sheet adds that `git mergetool` then puts back its own copy of the file, and that running it again restores the session. With `confirm-unresolved-save = false` (§6.4), the output is written so at once, with no sheet.
- **Save Copy As…** (⇧⌘S) asks for a path in a save dialog that starts in OUTPUT's directory, and writes the output there as Save & Exit would, unresolved conflicts as markers and line endings by `output-line-ending` (§6.4), with the atomic write. It asks no confirmation. The window stays open, and OUTPUT, the exit code and the recovery journal are unchanged, so git and jj see no difference. The copy is written off the UI thread; copies land in the order asked. Once it is written, the status bar says how many conflicts it holds, until the merge next changes. A write failure shows an alert and leaves the window as it was. Save & Exit and Cancel finish only once every copy being written has landed.
- **Close** (⌘W, ⌘Q, Cancel in the menu, closing the window) with nothing changed writes nothing and exits with code 2 at once. After a change it asks "Save your changes?" with three answers:
  - Save & Exit, the first answer and the default, does what ⌘S does, the confirmation for unresolved conflicts included: exit code 0 when resolved, 1 with conflict markers. Keep Editing in that confirmation leaves the window open.
  - Discard writes nothing and exits with code 2.
  - Keep Editing leaves the window open.
- Errors (bad arguments, unreadable input, binary input, write failure) exit with code 3 and print a message on stderr. The inputs load before the window opens, so an input error shows no window. A failed Save & Exit shows an alert with the reason and four answers: Try Again writes again, with no second confirmation for conflicts; Save Copy As… opens its save dialog (below); Keep Editing closes the alert; Quit exits with 3 and prints the reason on stderr. The window stays open until a save succeeds or the user quits, and the recovery journal keeps the work meanwhile. An alert that closes with its window counts as Quit. In `resolve`, a save the repository does not record (§7.4), such as a git index another command holds, is written but not recorded: its alert says so and offers Try Again, Keep Editing and Quit. Try Again records the save again without writing the file; Quit exits with 3. `resolve --headless` exits with 3 and says the file is written.
- Closing the open dialog is Cancel: exit code 2.

| Code | Meaning | git (`trustExitCode`) | jj (`merge-conflict-exit-codes = [1]`) |
|---|---|---|---|
| 0 | Output written, fully resolved | resolved | resolved |
| 1 | Output written with conflict markers | not resolved; `git mergetool` restores the file from its backup, with git's own markers, and discards triad's output; the recovery journal keeps the work, and the next run offers it back | reads the markers; those regions stay conflicted |
| 2 | Cancelled, nothing written | not resolved | tool failed; conflict unchanged |
| 3 | Error | not resolved | tool failed; conflict unchanged |

Cancel and conflicted output need different codes. jj reads the output file for any code in `merge-conflict-exit-codes`, so a cancel that also exited with 1 would be read as an edit.

Writing:

- The output is written atomically: to a temporary file in the same directory (`tempfile`), then renamed over OUTPUT with `std::fs::rename`. On Unix the directory is synced after the rename. An existing OUTPUT keeps its permissions, including the executable bit. A new OUTPUT gets the permissions of any new file. A link at OUTPUT stays: the file it names takes the bytes, through the same rename, in that file's directory. The rename puts a new file in place, so only the permission bits carry over: a hard link to OUTPUT keeps the old bytes, and an owner other than the writing user, extended attributes and ACLs are lost.
- After every resolution change and edit, the app writes a recovery journal to the platform cache directory, keyed by a hash of the three inputs and the output path. `jj resolve` passes an output in a new temporary directory each run, so there the repository's canonical root joined to `$path` stands in for the output path. The write waits until commands pause for 500 ms. The journal is JSON Lines: a header line, then one line per command. Writes and their syncs run off the UI thread, one at a time. The inputs hash, and the journal they name is read, off the UI thread too, so the window shows first and the restore offer follows. Restore replays the journal off the UI thread. A write appends the commands since the last one; the first write, the first after a restore and the one after a failed write replace the whole file atomically. A last line without its newline is a write cut short, and restore drops it. Reopening the same inputs after a crash offers to restore the session. While the offer is open, no write replaces the journal, so a second crash still finds it; Start Over then journals the commands made meanwhile, and Restore replaces them with the restored session. A restore aligns with the options the journal recorded, semantics, same-change setting, hunk level, refinement, syntax boundaries and line-ending rule included; Start Over deletes it. The header carries a format version, now 10; a journal of another version is not offered, and the session's first write replaces it. The header also holds a hash of the alignment the commands were made on: each region's kind, line ranges and text length as the document opened. Commands name regions and byte offsets, so a restore whose alignment hashes otherwise, as after a triad or grammar upgrade changes the regions, is refused as a journal that cannot be replayed. A journal that cannot be replayed raises an alert and is deleted, and the session stays as opened. On Unix a journal has mode 0600. A window deletes journals not written for more than 30 days as it opens, off the UI thread. A resolved Save & Exit and Cancel delete the journal. A save that leaves conflicts writes it at once and keeps it, since `git mergetool` discards that output (table above). Save & Exit writes it first, off the UI thread, on the thread that then writes OUTPUT; a write failure keeps it. A restore answered while a save runs is dropped: the save writes the merge as it stood.

### 6.3 Keyboard shortcuts

macOS keys are shown. On Linux and Windows, ⌘ is Ctrl and ⌥ is Alt. The `keybind` setting changes them.

| Action | Key |
|---|---|
| Next / previous difference | ⌘↓ / ⌘↑; also ⌘] / ⌘[ in the diff window |
| Next / previous conflict | ⌘] / ⌘[, or ⌥⌘↓ / ⌥⌘↑ |
| Toggle left / base / right for current region | ⌘1 / ⌘2 / ⌘3 |
| Clear choice (empty) | ⌘0 |
| Take left then right / right then left | ⌘4 / ⌘5 |
| Undo / redo | ⌘Z / ⇧⌘Z |
| Toggle base pane / blame | ⌘B / ⌘I |
| Toggle syntax highlighting / soft wrap / scroll sync | ⌘Y / ⌘R / ⌘K |
| Find / go to line | ⌘F / ⌘L |
| Save & exit / cancel | ⌘S / ⌘W |
| Save copy as | ⇧⌘S |
| Add a cursor below / above in the result | ⌥⇧↓ / ⌥⇧↑ |
| Copy / select all in a top pane | ⌘C / ⌘A |

In the result and the diff panes these keys run the command, not the editor's own binding: ⌘↑/↓ do not move to the start and end, ⌘[/] do not indent, ⌥⌘↑/↓ do not add cursors.

`keybind = CHORD=ACTION` binds a chord to a command: `keybind = cmd+j=next-conflict`.

- A chord is modifiers and a key joined by `+`. The modifiers are `ctrl`, `alt` and `shift`, with `cmd` on macOS and `super` on Linux and Windows. The key is one printing character, in lower case, or a GPUI key name: `enter`, `tab`, `space`, `backspace`, `delete`, `insert`, `escape`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1` to `f35`.
- An action is the kebab-case name of a menu command: `undo`, `redo`, `next-difference`, `previous-difference`, `next-conflict`, `previous-conflict`, `toggle-left`, `toggle-base`, `toggle-right`, `clear-choice`, `take-base`, `take-left`, `take-right`, `take-left-then-right`, `take-right-then-left`, `take-left-all-unresolved`, `take-right-all-unresolved`, `take-whole-left`, `take-whole-right`, `take-sorted-union`, `carry-moved-edit`, `toggle-base-pane`, `toggle-origin-column`, `toggle-syntax`, `toggle-soft-wrap`, `toggle-scroll-sync`, `find`, `go-to-line`, `save-and-exit`, `save-copy-as`, `save`, `cancel`, `open-settings`, `about`.
- Each line adds a binding. A later line for a chord replaces an earlier one in each window that has both commands, so `cmd+s=take-left` leaves the diff window's `cmd+s=save`, and the defaults `cmd+]=next-difference` then `cmd+]=next-conflict` give ⌘] to conflicts in the merge window and to differences in the diff window. A binding replaced in every window is dropped.
- `CHORD=unbind` makes the chord do nothing, in every window and in the editors. `clear` does the same for every chord bound so far. An empty value gives the default bindings.
- The last `=` parts a line, so `cmd+==take-left` binds the `=` key.
- An unknown action, a malformed chord or a modifier the platform lacks is a bad line (§6.4); the other bindings apply.
- The menus show a command's newest chord. `show-config --default` lists the defaults.

The whole flow works from the keyboard. The window opens with the focus in the result and the caret on the first unresolved conflict, scrolled into view; without one, on the first difference; without either, at the start. A restored session (§6.2) puts the caret there too. A measured run (`TRIAD_MEASURE`) opens at the start, so runs compare. These keys work inside the result editor too, over the editor's own bindings for them. The current region is the one holding the result caret; after navigation or a pick it stays the region moved to, even when it is empty.

- **Find** opens the result editor's search and replace, with a case toggle. A replacement is an edit like any other, undone through the merge's own undo. The input panes take no replacement. A step to the next or previous match blinks it for 0.5 s, shown and hidden in turn every 100 ms, starting shown and ending hidden: in lime, under the search's translucent fill in an editor (the result or a diff pane) and in place of the current match's fill in an input pane. A second step restarts the blink. Typing a query, an edit, or a step that stays on an editor's only match blinks nothing.
- **Search in the input panes.** The result's search query and case setting also search every input pane that shows; a hidden base pane is not searched. A query is literal text, matched as the editors' search matches it: case-insensitive by default, folding ASCII letters only, matches not overlapping. A match lies within one line, so a query holding a line break finds nothing. While the result's search is open with a query, each input pane's heading carries its own controls: previous match, a count (`n of m` or `No matches`), and next match. Each pane keeps its own current match, at first the first match on or below the top row. A step moves that pane's current match, wrapping at either end, and scrolls its row to the middle; the caret and the focus stay where they are. Each match fills its characters as the editors' search fills its matches: the current one in the selection colour, the others in that colour greyed. A fill lies over the intra-line marks. A base pane shown or hidden, or a change to the rows, searches again and keeps each pane's current match when it is still found. Closing the result's search, or clearing its query, removes the controls and the fills.
- **Go to line** opens a field above the result. It takes a result line number from 1. A number past the end goes to the last line; anything else leaves the field open. Enter moves the result caret to the start of the line and the top panes to the line's row. Escape closes the field. Either way the focus returns to the result.

Each action has a menu item. The toolbar holds the actions people click. Each toolbar control shows a Lucide icon; its tooltip and accessible name name the action. A label, in text 60% of a small button's size, goes only on the controls that pick:

- Previous and next difference form one control, `‹ Difference ›`: two icon buttons with the label between them. Previous and next conflict form `‹ Conflict ›`.
- Left, Base, Right and Clear each show their label.
- Both is a split button. It takes left then right; its menu, named "Take both sides in either order", offers Left then right and Right then left, each with its key, in the toolbar's label text. The menu opens with no row selected. ↓ and ↑ select the next and previous row, wrapping, and start at the first and last. Enter runs the selected row; with none selected it does nothing. A row, Escape or a click outside closes the menu and gives the focus back.
- Undo, Redo, Base pane, Find, Go to line, Syntax and Soft Wrap show icons alone.

A merge window narrower than 1052 px shows no labels, so the toolbar and the longest conflict badge fit down to 788 px. The diff window's toolbar has no width rule: it holds `‹ Difference ›`, Undo, Redo, Find, Syntax and Soft Wrap. A toolbar click, or a pick from the Both menu, leaves the focus where it was. The menus:

| Menu | Items |
|---|---|
| triad | About triad, Settings… (§6.4), Save & Exit and Cancel (§6.2) |
| Edit | Undo, Redo, Find…, Go to Line… |
| Merge | Toggle Left, Toggle Base, Toggle Right, Clear Choice, Take Base, Take Left, Take Right, Take Left then Right, Take Right then Left, Take Sorted Union, Carry Edit to Moved Block, Take Left for All Unresolved, Take Right for All Unresolved, Take Whole Left File, Take Whole Right File |
| Go | Next Difference, Previous Difference, Next Conflict, Previous Conflict |
| View | Base Pane, Blame (once origins load), Syntax Highlighting, Soft Wrap |

The app's name in the interface is lower case, `triad`, as in the logo: the first menu, the default window title, the desktop entry and the bundle.

On macOS the toolbars of the merge and diff windows start with the mark. Linux and Windows show it in the title bar. A click on the mark opens About triad.

**About triad** opens one window, or brings it forward: the logo, the version, and two lines of 12 px text on what triad does: "Three-way merge and two-way diff" and "Built for Jujutsu, with full Git support". A release build from `script/macos-app` or `script/package` adds a line of muted 12 px text under the version with its commit, and its jj change when built in a jj repository: `Commit 1a2b3c4d5e6f, change kxqpzmvwrlty`, each 12 characters. jj names the head of the non-empty revisions in `::@`, where a merge counts as non-empty, since a clean merge is empty; git names `HEAD`, a merge commit included, with `-dirty` when the working tree has changes. `script/build-id` sets `TRIAD_COMMIT` and `TRIAD_CHANGE` for the compiler; without `TRIAD_COMMIT`, a release build shows no line and a debug build shows the null ids, `Commit 000000000000, change zzzzzzzzzzzz`: git's null object id and jj's root change id. Escape and the window's own close button close it. It leaves the merge window as it was.

### 6.4 Settings

Merge semantics, same-change rule, hunk level, diff algorithm, whitespace mode, line-ending rules, conflict refinement, syntax boundaries, tab width, font and size, scroll sync, scrollbars, base pane default, origin column default, theme, line height, tone colours and band opacity, conflict style, the unresolved-save confirmation. Every setting is in the config file. Five of them, the merge options, are also command-line options that hold over the file for one run.

The default text size is 10 pt, with lines 1.5 times the text size (`line-height`). The rows of the side panes are as tall as the editor lines.

triad reads the config file and its includes once, when it starts. A change to them applies from the next run. The file's bad lines are reported on stderr, and the merge and diff windows show them in a banner above the panes, with the alert role. The banner closes with its close button.

**Settings…** (⌘, on macOS, Ctrl+, elsewhere), in the triad menu, opens the config file in the default text editor: `open -t` on macOS, `xdg-open` on Linux, Notepad on Windows. A missing file is first written, with its directory, from a template: a header line, then each key's description and the key at its default, commented out. The template reads back as the defaults. If the editor cannot start or the opener exits with an error, the message goes to stderr and the file manager reveals the file. A file that cannot be written goes to stderr alone.

Settings are a text file, `config.triad`:

- macOS and Linux: `$XDG_CONFIG_HOME/triad/config.triad` when `XDG_CONFIG_HOME` is absolute, else `~/.config/triad/config.triad`.
- Windows: `%APPDATA%\triad\config.triad`.

`TRIAD_CONFIG` names another file. Empty variables count as unset.

The file holds one setting per line, as `key = value`:

```
# triad config
ignore-whitespace = trailing
font = Iosevka Term
font-size = 12
config-file = ?local.conf
```

- Spaces around `=` and at the line's ends are trimmed. Keys are lowercase and case-sensitive. The value runs from the first `=` to the end of the line.
- A line whose first non-blank character is `#` is a comment. A `#` after a value is part of the value.
- A value in double quotes loses the quotes. Quotes are needed only for leading or trailing spaces, and for an include path that starts with a literal `?`.
- An empty value sets the key back to its default. A key given twice takes the later value.
- `config-file = PATH` reads another file at that point, as if its lines stood there, so a later line overrides it. A relative path is relative to the including file. A leading `?` makes the file optional: a missing optional file is skipped, a missing required one is an error. A file that includes itself, directly or through others, is an error.
- Strings are bare words; booleans are `true` and `false`. Names are case-sensitive.
- UTF-8, with `\n` or `\r\n` line endings; a byte-order mark at the start is skipped.

Every key is optional:

| Key | Values | Default |
|---|---|---|
| `merge-semantics` | `auto`, `git`, `jj` (§6.1) | `auto` |
| `same-change` | `accept`, `keep` | jj's `merge.same-change` under jj semantics, else `accept` |
| `hunk-level` | `line`, `word` | jj's `merge.hunk-level` under jj semantics, else `line` |
| `diff-algorithm` | `histogram`, `myers`, `myers-minimal` | `histogram` |
| `ignore-whitespace` | `none`, `trailing`, `amount`, `all` | `none` |
| `merge-ignore-line-endings` | `true`, `false`: merges compare `\r\n`, `\n` and a missing final newline as equal (§4.2) | `false` |
| `diff-ignore-line-endings` | `true`, `false`: the same for the diff window and `diff --inspect` | `true` |
| `refine-conflicts` | `true`, `false`, `deep` | `true` |
| `syntax-boundaries` | `true`, `false`: keep deep refinement's splits and word merges off the inside of syntax nodes (§4.3) | `false` |
| `tab-width` | 1–16 | 4 |
| `font` | a font family | the theme's monospace font |
| `font-size` | 6–72, in points | 10 |
| `toolbar-font-size` | 6–12, in points: the toolbar's labels, its conflict badge and its menus, in both windows. The merge window's toolbar drops its labels below a width that grows with this text (§6.3) | 8.4 |
| `window-size` | `WIDTHxHEIGHT` in points, each 400–16384: the size the merge and diff windows open at, centred on the main display. A window opens no larger than the display's visible area | `1400x900` |
| `scroll-sync` | `true`, `false`: the merge window's result scrolls with the top panes (§5). The diff panes always scroll together | `true` |
| `scrollbars` | `always`, `hover` (while the pointer is over the pane), `when-scrolling` (while the pane scrolls, fading once it stops): when the panes' scrollbars show (§5). The overview ruler's thumb always shows | `always` |
| `show-base-pane` | `true`, `false` | `false` |
| `show-origin-column` | `true`, `false`: the line origin column (View → Blame) shows when a window's origins load (§5, §7.3) | `false` |
| `revision-id` | `change`, `commit`: the ID that names a jj revision in the origin column and the pane headings (§5). git names commits under either | `change` |
| `theme` | `system`, `light`, `dark`: `system` follows the system's appearance when triad starts | `system` |
| `line-height` | 1–3, a multiple of the text size | 1.5 |
| `color-left` | a colour | `light:#2273c3,dark:#67a6e4` |
| `color-right` | a colour | `light:#c32273,dark:#e467a6` |
| `color-base` | a colour | `light:#c3b622,dark:#e4da67` |
| `color-conflict` | a colour | `light:#c36522,dark:#e49b67` |
| `color-result-edit` | a colour | `light:#8022c3,dark:#b067e4` |
| `color-same-change` | a colour | `light:#22c3c3,dark:#67e4e4` |
| `color-word-merged` | a colour | `light:#22c322,dark:#67e467` |
| `color-current-region` | a colour: the current region's bars and frame, and the blink on the current search match | `light:#6cbd0f,dark:#92e236` |
| `color-deleted` | a colour: deletion notes and marks | `light:#c32222,dark:#e46767` |
| `color-resolved` | a colour: the conflict badge once no conflict is left | `light:#22c358,dark:#67e491` |
| `band-opacity` | 0–1, for both themes or as a pair | `light:0.18,dark:0.28` |
| `conflict-style` | `diff3`, `merge` | git's `merge.conflictStyle` under git semantics, else `diff3` |
| `output-line-ending` | `keep`, `base`, `lf`, `crlf`: the saved merge's line endings. `keep` leaves each line's; `base` gives every line base's dominant ending (§4.1); `lf` and `crlf` give every line that ending. Headless runs and `inspect --json` follow it; the two-way diff saves each side as it is | `keep` |
| `confirm-unresolved-save` | `true`, `false`: Save & Exit asks first while conflicts are unresolved (§6.2) | `true` |
| `keybind` | `CHORD=ACTION`, `CHORD=unbind` or `clear`, on as many lines as needed (§6.3) | the keys of §6.3 |
| `syntax-highlighting` | `true`, `false`: when a window opens, every pane colours its text by the language of the first file name that has one: the inputs, the output, the title (`.rs`, `.go`, `.cs`, `.py`, `.pyi`, `.swift`, `.html`, `.htm`, `.js`, `.mjs`, `.cjs`, `.jsx`, `.ts`, `.mts`, `.cts`, `.tsx`). The toolbar's Syntax button and View → Syntax Highlighting turn the colours off and on while the window is open, in the merge and diff windows alike; the inputs parse the first time the colours show. The base, left and right panes draw plain until their parse, off the UI thread, finishes; intra-line marks draw over the syntax colours | `true` |
| `syntax-whole-file-max-lines` | a whole number: the longest file, in lines, the result and diff editors parse whole for syntax colours; a longer one parses the chunks on screen, as the base, left and right panes do | `20000` |
| `soft-wrap` | `true`, `false`: the result, the top panes and the diff editors wrap long lines when a window opens; `false` scrolls them sideways (§5). Files over 10 000 lines never wrap | `false` |

Tab stops sit every `tab-width` spaces in every pane, and the Tab key types that many spaces.

A colour is `#rrggbb`, hex digits in either case, and opaque. A colour or `band-opacity` value is one value for both themes, or a pair `light:A,dark:B` in either order that names each theme once; spaces around `,` and `:` are trimmed. A tone's colour is its gutter marks, labels and changed words; the band behind its rows is that colour at `band-opacity`. The cleared tone stays grey. The default colours are the tones' hues at HSL saturation 0.7 and lightness 0.45 in the light theme, 0.65 in the dark; the current region's lime is 88° at saturation 0.85 and lightness 0.4 (light), 0.75 and 0.55 (dark). The gutter letters stay whatever the colours (§6.5). `show-config` prints a pair as one value when both themes share it.

`conflict-style` sets the markers of a two-sided conflict in the output under git semantics, in the window, headless and in `resolve` (§4.4). `diff3` writes the base section; `merge` drops it and its `|||||||` marker, as git's `merge.conflictStyle = merge` does. Without the key, triad reads git's `merge.conflictStyle` from the repository the semantics found (§6.1), with the user's and system's git config under git's rules: `merge` gives `merge`; `diff3`, `zdiff3`, no value and no git repository give `diff3`. Refinement is `refine-conflicts`'s job, so `zdiff3` reads as `diff3`. git compares the value case-sensitively. A value git does not know, and a git config that cannot be read, are logged and give `diff3`. jj semantics ignore the key and write `diff3`, the style `install --jj` configures (§7.1). The open dialog takes the key alone. The marker parser reads both styles (§7.4).

The command line takes five settings: `--merge-semantics`, `--same-change`, `--hunk-level`, `--diff-algorithm` and `--ignore-whitespace`, on the forms that take them, as `--KEY VALUE` or `--KEY=VALUE`. They change the merge, so a caller can set them for one run; each holds over the file. The other settings are in the config file only: `--font-size=12` is an unknown argument and exits with 3. A run that needs other settings points `TRIAD_CONFIG` at its own file. A missing file gives the defaults. Each bad line is reported on stderr as `PATH:LINE: message`: a line with no `=` or no key, an unknown key, a value of the wrong kind or out of range, a missing required include, an include cycle. A bad line leaves its key at the value before it, and the other lines apply, so a merge started by git or jj still opens. A file that cannot be read gives the defaults.

`show-config` prints the settings in effect in the file's form, one `key = value` line per key in the table's order, after includes and bad lines. `keybind` prints `keybind = clear`, then one line per binding, then one line per chord that does nothing. A key with no value, such as `font` without one, prints as `key =`. A value the file would trim or unquote prints in quotes. `--default` prints the defaults. `--docs` puts each key's description above it as a `#` comment, with a blank line between keys. The output reads back as the settings it shows. It exits with 0, or with 3 when the file has bad lines, which go to stderr.

### 6.5 Accessibility and language

- Every control has an accessible role and label, set through gpui-component's `role` and `aria_label`:
  - toolbar buttons, the copy buttons and the open dialog's buttons are buttons named for their action;
  - the open dialog's Merge and Diff tabs are tabs in a tab list, named for their mode, selected while they show;
  - a pick target is a toggle button named for its pane's title and region ("Left in region 2 of 3"), on while its source is in the pick and off otherwise; in an unresolved conflict the name ends ", unresolved" ("Left in region 2 of 3, unresolved"), since macOS reads a mixed toggle as on;
  - the merge window's input lines are a table of rows and cells, each cell named for its pane, line number and text, or "no line";
  - the result and the diff panes are multi-line text fields named "Result", "Left: PATH" and "Right: PATH";
  - the status bars and the conflict badge have the status role and their text as label;
  - the jj banner and the open dialog's error are alerts.
- Status changes are not announced, and region bands and gutter marks are not exposed: GPUI has no live regions, and range decorations are visual only (`docs/gpui-kit-issues.md`).
- Region kinds use a gutter letter as well as a colour, so colour is never the only signal.
- English only for release 1. All UI strings go through one module so translation can come later.
- No telemetry and no network access.

## 7. VCS integration

### 7.1 Release 1: merge and diff tool

Tool mode needs no library access. Native diff mode (§6.1) reads one file at one revision through `vcs`. Merge tool configuration:

```toml
[ui]
merge-editor = "triad"

[merge-tools.triad]
program = "triad-merge"
merge-args = ["$base", "$left", "$right", "-o", "$output", "--title", "$path", "--marker-length", "$marker_length", "--merge-semantics", "jj"]
merge-tool-edits-conflict-markers = true
merge-conflict-exit-codes = [1]
conflict-marker-style = "git"
```

- `merge-tool-edits-conflict-markers = true` makes jj parse markers left in the output and keep those regions conflicted.
- `merge-conflict-exit-codes = [1]` makes jj read the output after an exit-1 save (§6.2).
- `conflict-marker-style = "git"` makes jj expect the diff3 markers that triad writes.

```gitconfig
[merge]
    tool = triad
[mergetool "triad"]
    cmd = triad-merge "$BASE" "$LOCAL" "$REMOTE" "$MERGED" --merge-semantics git
    trustExitCode = true
```

- `--merge-semantics jj` and `--merge-semantics git` name the caller's semantics, so `jj resolve -R repo` run from outside the repository still merges as jj does, and `git mergetool` in a jj repository is refused (§6.1).
- jj passes no side labels, so the panes show the names of the files jj passes and the window shows `$path`. git passes temporary file names such as `foo_LOCAL_1234.c`, which the panes show as titles.

Diff tool configuration:

```toml
[merge-tools.triad]
diff-args = ["diff", "$left", "$right"]
diff-invocation-mode = "file-by-file"
diff-do-chdir = false
```

`file-by-file` makes jj run the tool once for each changed file, because triad takes single files, not directories. `diff-do-chdir = false` runs the tool in the user's working directory with absolute paths; jj's default runs it in its temporary directory, where no repository is found. jj runs `program` with `diff-args`, so the arguments start with the `diff` subcommand, as git's `cmd` does; `triad-merge LEFT RIGHT` alone is a merge missing its inputs.

```gitconfig
[diff]
    tool = triad
[difftool "triad"]
    cmd = triad-merge diff "$LOCAL" "$REMOTE" --title "$MERGED" --readonly-left
```

`$MERGED` is the file's path in the repository. It titles the window, and it names the file when both sides are temporary copies, as in `git difftool --cached` and diffs of two commits, so their line origins can be found (§7.3). `$LOCAL` is a temporary copy git deletes after the run, so `--readonly-left` keeps edits out of it. `git difftool -R` swaps the sides, which leaves the working-tree file on the read-only left.

Where edits land:

- `jj diff --tool` passes temporary copies of both sides, and jj discards them when the tool exits. A path lies in jj's temporary directory when, made absolute from the working directory and with `..` applied by name, it has a directory `jj-diff-<random>` followed by `left`, `right` or `output`. jj names the directory with that prefix and may pass paths relative to it.
- When the right side shows the working copy, triad opens the working-copy file in its place, so saved edits persist, and the left copy is read-only, as with `git difftool`. jj passes no revisions, so bytes decide: the right path lies under `right`, the repository is jj, and the copy holds the bytes of both the regular file at its path in the working tree and that file in `@`. jj snapshots the working copy before it runs the tool, so `jj diff --tool triad` and `jj diff --tool triad --from REV` qualify. A right side of another revision whose file holds the same bytes also qualifies; the right pane's title names the working-copy file. `--readonly-right` turns the check off.
- Otherwise the window shows a banner when an editable side lies in jj's temporary directory, saying its edits are lost. To edit any jj working-copy file, use native mode, `triad-merge diff PATH`.
- `git difftool` passes the real working-tree path for the working-tree side, `$REMOTE`, so saved edits persist. The left side, a temporary copy, is read-only.

`triad-merge install --jj | --git` writes these entries through jj-lib's config API and gix's config API. The `install` command writes the binary's absolute path, quoted for the platform. On Windows that means backslash paths, a space-safe quoted `cmd` string for git, and `triad-merge.exe` (§6.1).

- The path is the running binary's canonical path. Run from `triad-merge-gui.exe`, it names `triad-merge.exe` beside it. Run from an AppImage, it names the AppImage, whose runtime sets `APPIMAGE`: the binary inside lies in a mount that ends with the run.
- git runs `cmd` through `sh` on every platform, Git for Windows included, so the path is single-quoted, with `'` written as `'\''`. Backslashes and spaces need nothing more inside single quotes.
- `--git` writes to the file `git config --global` uses: `GIT_CONFIG_GLOBAL`; else `~/.gitconfig`, unless only `$XDG_CONFIG_HOME/git/config` (default `~/.config/git/config`) exists. `--config FILE` names another file.
- `--jj` writes to the user config file `jj config set --user` uses: the first path in `JJ_CONFIG`; else `~/.jjconfig.toml` when it exists; else `jj/config.toml` in the user config directory (`$XDG_CONFIG_HOME`, absolute, or `~/.config` on Unix and macOS; `%APPDATA%` on Windows). A `JJ_CONFIG` that names a directory is refused.
- jj runs `program` directly, without a shell, so the path needs only TOML string quoting.
- `install` is the only command that writes git's or jj's config, and only to the one file named above. Merging and diffing read config and never write it.
- `merge.tool` and `diff.tool` (git) and `ui.merge-editor` (jj) become `triad`; jj has no default diff tool to set, so `jj diff --tool triad` names it. Other tools' sections, other settings and comments stay. A second install with the same path leaves the file byte for byte as it was. The file is written atomically, and created with its directory when absent.
- A file the VCS cannot parse is refused and left as it is; the command exits with 3. On success it prints the file it wrote, or that the file held the entries already, and exits with 0.

### 7.2 Git library choice

The git backend uses gix, not libgit2 (`git2`).

- jj-lib's git backend is gix, and it has no libgit2 dependency. With gix, the binary carries one git implementation instead of two, and `vcs-git` pins the gix version jj-lib uses so Cargo builds only one copy.
- gix is pure Rust. There is no C build, no OpenSSL, and no `libgit2-sys` version to track.
- gix covers what triad needs: repo discovery, config read/write, index read/write including conflict stages, object reads, blame (`gix-blame`) and blob merge.
- libgit2 is more mature, and its index-conflict and blame APIs are well proven. It stays a fallback: if gix lacks an operation, `vcs-git` implements that one operation with `git2` behind the same trait.

### 7.3 Chunk origin

- Git: `vcs-git` runs gix blame on `ours` and `theirs` (from the merge heads, or the commits under rebase or cherry-pick). It maps each region's lines to commit, author, date and summary.
- jj: `vcs-jj` runs jj-lib's file annotation on each side of the conflict. It maps lines to change ID, commit, author and description.
- Both backends return the same `LineOrigin` type. The UI shows origin in a hover card and an optional gutter column, and names each input's commit in its pane heading (§5).
- The commit an input shows is the commit of its source when the file there holds the input's bytes; the revision `diff PATH` reads its left side from, whether or not the file exists there or the input is UTF-8; and, under jj, `@` for the working copy of `diff PATH`. git's working tree and index show none, nor does a revision naming several commits. `Repository::describe` gives the heading's ID and hover card.
- `Repository::commits(rev)` gives the full IDs of the commits a revision expression names; `Repository::merge_base(one, two)` gives the nearest common ancestor of two commits named by their full IDs, or none when there is none or several (a criss-cross).
- `Repository::describe(commit)` gives the `LineOrigin` of a commit named by its full ID. An ID the repository does not hold is unknown, and so is a prefix or a revision expression, such as `HEAD` or `@`. The same holds for every method that takes a `CommitId`, and for `RevSpec::Commit`: the backend looks the commit up and parses no expression.
- An input takes origins from a commit whose file holds exactly its bytes, and only when it is UTF-8, with or without a byte order mark. The working copy of `diff PATH` takes them from its parent by a line diff instead: lines the parent holds keep its origins, changed lines have none. Every other input has none. `merge_session::origin` picks the file and checks the bytes; the backend gives the commits to try (`Repository::merge_candidates`, `copy_candidates`, `index_base`, `working_copy_id`):

  | Run | File | Commits tried, first match wins |
  |---|---|---|
  | `diff PATH` | PATH | left: `--rev`, or the default parent; the index, which has no history, by a line diff against `HEAD`. Right: the default parent (`HEAD` for the index), by a line diff |
  | git merge (the repository of the working directory, else of the output, is git) | the output, `$MERGED` | left: `HEAD`. The first of `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD` and `REBASE_HEAD` that exists, `X`, gives right and base: a merge `X` and the merge base of `HEAD` and `X`; a cherry-pick or rebase `X` and `X^`; a revert `X^` and `X` |
  | `diff LEFT RIGHT` from `git difftool` or `jj diff --tool` (the repository of the working directory, else of the right side) | the path below jj's `jj-diff-<random>/left`, `/right` or `/output`, else a side in the working tree, else `--title` | git: a side in the working tree, the default parent by a line diff; a copy, `HEAD`, else, when the index holds its bytes, `HEAD` by a line diff. jj: each side, `@-` when it is one commit, then `@` |
  | jj merge (that repository is jj) | `--title`, which `jj resolve` passes as `$path` | each base and side: `@`, each parent of `@` in the commit's order, and with several parents their one common merge base: each further parent's merge base with the base so far, none when any step has several |
  | `resolve` (spec §7.4), of any number of sides | the working-tree file, and its path from the root as the title | as a git or jj merge: git's terms follow the stopped operation and jj's lie at `@`'s parents and their merge base |

  A revision naming several commits, a conflicted file and a missing one match nothing. A working-tree file that does not exist resolves through its nearest existing directory. No repository, a file outside it, or a jj merge without a title gives no origins.
- `Repository::annotate(path, commit)` gives the `Annotation` of a file at a commit named by its full ID, or none when the file does not exist there. It holds each commit once, as a `LineOrigin` (short commit ID, jj's change ID, summary, author name, author time), and runs of lines in order that cover the file, each naming one commit. git follows every parent, without rename tracking. jj annotates over every commit; a file conflicted at the commit is an error, as for `file_at`.

### 7.4 Native resolution

`triad-merge resolve [PATH]` finds the repository at the current directory, picking jj if both `.jj` and `.git` exist. Outside any repository, it finds the one holding PATH. With no path, it lists the conflicted files and opens them in turn. It takes the diff and semantics flags of a merge and `--marker-length`; the semantics are chosen as for a merge whose output is the working-tree file. The window's title is the path relative to the repository root. Each term's title (§7.3) labels its pane and its markers.

A file the VCS holds no conflict for, or one outside any repository, opens from its conflict markers when it has them: git's `merge`, `diff3` and `zdiff3` styles, and jj's snapshot, diff and git styles (`DESIGN.md` §D5.7). A conflict without a base section opens with an empty base there. Labels come from the first conflict's marker lines. Without markers, the file is not conflicted and `resolve` exits with 3.

- Git: read stages 1, 2 and 3 for the path from the index through gix. Stage 1 is the base; stages 2 and 3 are the sides, ours then theirs. Ours is `HEAD`, labelled with its branch; the base and theirs follow §7.3's rules for the stopped operation, and are unnamed without one. A missing stage is an absent term: the file was added, or deleted, on another term. Write the result to the working tree. If fully resolved, replace the conflict stages with one stage-0 entry, which is what `git add` does. Saving with markers leaves the stages in place.
- jj: read the file's `Merge` from the working-copy commit through jj-lib, together with the commit's conflict labels. Its adds are the sides and its removes the bases, in jj's order. Each term takes the tree's conflict label at its position. Write the result to the working copy; jj's snapshot then records it on that commit. Saving with markers writes a conflict jj understands. The pane titles use the labels when the commit has them.
- In a jj repository, `resolve` snapshots the working copy before it reads a conflict, as a jj command does, in an operation of its own (`snapshot working copy`) when the copy changed. The conflict it reads then holds the user's unrecorded edits; a file resolved by hand is no longer conflicted, and `resolve` exits with 3. A read that follows the list of conflicts, or the save before it, at once reads at that step's snapshot without scanning the working copy again. The snapshot follows the user's jj config: `snapshot.auto-track`, `snapshot.max-new-file-size` and the ignore files. After each save, `resolve` snapshots again, in a second operation, `save <path> in triad`, so `jj undo` reverts the save alone. A save whose markers read back as the conflict jj holds leaves the tree as it was and writes no operation. Markers it saves in a jj repository are at least as long as jj's recorded marker length for the file, so jj reads them back.
- A git `HEAD` that moved since jj last read it, as after `git checkout`, is imported first, as jj imports it: an operation `import git head` puts a new working-copy commit on git's `HEAD` and records the files git left there without touching them. The snapshot then takes in any edits on that commit. Git's other refs are not imported; they wait for jj's next command. A snapshot or save that rewrites `@` exports jj's bookmarks and tags to git in the same operation, as jj does after each command, so a bookmark on `@` moves in git too. A ref jj cannot export is logged and left as it is. An internal jj repository has no git working tree and skips the import and the export.
- `resolve` refuses, before it writes anything, a stale jj working copy: the operation it was last updated at is not an ancestor of the head. The message names `jj workspace update-stale`, which jj offers too.
- In a git repository, a fully resolved save replaces the path's conflict stages with one stage-0 entry, as `git add` does: git's clean filters and line-ending rules apply, the entry takes the file's size and times, and its executable bit comes from the file, or from ours, theirs and then the base when `core.fileMode` is false. A path with no stages is left alone. A save with markers leaves the stages. The index is read and written under git's `index.lock`, so a git command running beside triad loses no change. When the lock is taken or the index cannot be written, the save is an error: the file is written and the index stays as it was. gix writes no resolve-undo record, so `git checkout -m` cannot recreate the conflict afterwards, and the write drops the records of other paths (`docs/gix-issues.md`).
- `resolve` refuses a conflict whose working-tree file is text with no line that opens a conflict (§7.4's marker rule: at least seven `<`, then whitespace or the line's end), when every side is a file. The VCS wrote markers there, so the file holds a resolution made by hand. git keeps the conflict in its index until `git add`, and the conflict read from it lacks that resolution, so a save would destroy it. A working-tree file that is not text, such as a hand resolution saved in Latin-1, is refused for the same reason, naming the decoding error. An unreadable file is an error. A missing working-tree file, and a conflict with an absent side, where git writes the surviving side without markers, load from the repository.
- git keeps no record of the working-tree file, so a file with markers may hold edits beside them, or conflicts resolved by hand, that the stages lack. When the VCS has no record of the file and every side is a file, `resolve` compares the file's markers with the conflict: it parses both the file and triad's git-semantics merge of the terms, and compares the sides each rebuilds. Labels, marker length and style, and lines zdiff3 moves out of a conflict do not count; nor do bases, since the `merge` style writes none. When the sides match, the conflict loads from the repository. When they differ, it opens from the file's markers, as a file the VCS holds no conflict for does, so the edits stay; a conflict without a base section then has an empty base. Markers that do not parse are refused, naming the line. In jj the snapshot before the read covers the same case.
- `resolve` refuses a binary conflict, a conflict that holds a symlink, submodule or directory, a conflict with every side absent, and a file whose markers do not parse, naming the path and the reason. git's `ort` strategy leaves the last kind at the old name of a file renamed two ways; it resolves by deleting the file, which `resolve` leaves to the VCS. With a path, a refusal exits with 3. Without one, `resolve` skips the file, prints the reason on stderr, and counts the file as unresolved. Neither git nor jj leaves a conflict in the executable bit alone; each term of a content conflict carries its own bit.

The core model must support more than two sides from the start. Git conflicts and two-sided jj conflicts use the three-pane layout. jj conflicts with more than two sides get one top pane per side.

## 8. Testing

Development is test-first throughout. Expected values come from hand calculation or from git's documented behaviour, never from the code's own output.

### 8.1 `merge-core` unit tests

- Classification for every region kind, with small hand-written inputs.
- Boundaries: empty files, one empty side, identical inputs, no newline at end of file on each side, CRLF and LF mixes, a change at the first or last line, adjacent and overlapping hunks, insertions at the same base position.
- Whitespace modes: each mode, both the equality decision and the bytes that reach the output.
- Resolution: every `Pick` order, toggling, empty list, edit then pick then undo.
- Edits: inside a region, across one boundary, across several, deleting a whole region, typing into an unresolved region.
- Decoding errors: binary, invalid UTF-8, a UTF-16 BOM with an odd byte length.

Two-way diff:
- identical files, one empty file, both empty
- pure insert and pure delete at the start, middle and end
- whitespace modes
- no newline at end of file on either side

### 8.2 Properties (proptest, fixed seed, recorded regressions)

- Picking `Left` for every region gives left exactly. The same holds for `Right` and `Base`.
- Swapping left and right swaps `LeftOnly` with `RightOnly`, keeps the other kinds, and gives the same automatic output for regions that aren't conflicts.
- If left equals base, the automatic output equals right with no conflicts. The mirrored case holds too.
- Region ranges cover base, left and right fully, without gaps or overlaps.
- Two-way: concatenating each region's left text gives left exactly, and the same for right, in both directions. Swapped inputs need not give mirrored regions: where two diffs are equally short, the tie-break depends on direction, as in `git diff`.
- After any sequence of picks, edits, multi-cursor batches, undo and redo, the regions' bytes cover the result text in order, their line ranges cover every input, and undoing everything restores the starting text, regions and resolutions.

### 8.3 Oracle tests

A corpus of base/left/right triples with expected output and conflict status. A one-off script generates it with `git merge-file --diff-algorithm=histogram` (git 2.54 or later; 2.54 moved some histogram hunks), using `--diff3` for refinement off and `--zdiff3` for refinement on. The files are checked in. git's exit code is the conflict count. The product never calls git. The tests compare automatic output and conflict regions with the corpus. A case that differs is listed in the test with its cause. One case is listed: git's Myers and `imara-diff`'s break a tie between equal-cost diffs differently. Each case also holds git's default two-way marker style, and for each style the output with every conflict resolved to the left (`--ours`) and to the right (`--theirs`). Parsing git's markers (§7.4) must give those two texts as the sides, the base sections as the base, and git's labels and marker size; the corpus's base label `base` is jj's default and titles no pane. A side that ends without a newline inside the markers is the one loss: git adds the newline (§4.4) and keeps no record of it. A clean output parses as no conflict.

### 8.4 View model tests

Row alignment, filler rows, scroll mapping between panes, navigation order, and command → state changes. All plain Rust.

### 8.5 GUI tests

Headless `gpui` tests (`TestAppContext`) dispatch actions and assert on view-model state and result text: open files, go to the next conflict, take right, save. Assertions don't depend on pixels.

### 8.6 CLI tests

Integration tests run the built binary in `--headless` and `inspect --json` modes against fixtures. They check output bytes, exit codes and stderr.

### 8.7 Performance tests

Benchmarks (`criterion`) on generated 10k-, 100k- and 1M-line inputs, with a fixed seed. `script/check-bench` fails if a benchmark exceeds §9 targets by more than 20%. The `bench` workflow runs by hand and weekly, and allows five times the target, since shared runners are noisy.

### 8.8 VCS backend tests

The backend tests create real repositories in temporary directories. They write them through gix and jj-lib only, never through the CLIs. One conformance suite, written against the `vcs` trait, runs on git, jj and internal jj repos:

- find conflicted files
- read a conflict and compare it with the expected base/left/right
- write a resolution and check the repo state afterwards: index stages for git, the working-copy tree for jj
- save with markers and check that the file is still conflicted
- save with one of three conflicts resolved: jj holds the other two, in a commit that `jj edit` and `jj new` carry and a child resolves; git keeps the stages and the next `resolve` opens the two from the file's markers. The same for one of two conflicts with three sides
- resolve in an ancestor with descendants, in two saves: each save rebases the descendants, which keep their own edits; after the first a descendant holds jj's term-by-term rebase of the conflict, after the second the resolved file
- annotate known commits and compare the expected origins line by line

Release 1 covers repository discovery (including a git repo nested in a jj workspace and the reverse), reading a file at a revision (a missing path, a directory, a binary file, a renamed file, an unknown revision) and the default parent's `Term`. M5 adds annotation: lines changed over several commits, an earlier revision, a merge, the origin's fields, a missing path, an unknown revision and an empty file; the commits a revision names, a merge base, and a criss-cross, which has none; and describing a commit, a missing commit and a revision expression given as an ID. M6 adds reading a conflict: modify/modify, add/add, modify/delete both ways, binary terms, a clean or missing path, jj's three sides and two bases in order, and each term's label; and the conflicted paths: none in a clean working copy, one among clean files, several in path order (component by component), a file deleted on one side and changed on the other, a file added on both sides, and a jj conflict of three sides. The suite lives in `merge-session`'s tests, because it reaches the backends through `native::discover`.

A backend is done when it passes the whole suite.

### 8.9 Fuzzing

After release 1. The proptest suites of §8.2 cover the merge engine and the edit model, and unit tests cover each decoding error. `cargo-fuzz` needs a nightly toolchain, which the build does not use. When fuzzing is added, the first target is input decoding: arbitrary bytes give a `Text` or a decode error, never a panic. Its harness crate in `fuzz/` will need an exemption from §2.3, since `fuzz_target!` expands to an unsafe `#[no_mangle]` entry point.

## 9. Performance targets

Measured with a release build. The reference machines are an M4 Pro MacBook Pro, an x64 Linux machine with 8 cores and a Windows ARM64 laptop (Snapdragon X). Every target must meet the targets on its reference machine.

| Operation | 100k lines per file | 1M lines per file |
|---|---|---|
| Load and merge | < 100 ms | < 1 s |
| Command to window shown with content | < 300 ms | < 1.5 s |
| Scrolling | display refresh rate, no dropped frames | same |
| Take left/right on one region | < 16 ms | < 16 ms |
| Keystroke in result pane | < 8 ms | < 8 ms |
| Memory | < 300 MB | < 2 GB |

Panes render only visible rows. Character-level diffs and the rows' layout run on demand.

A word-level merge (`hunk-level = word`, §4.3) of a conflict that spans much of a large file has no target. It costs what jj's word merge of the same hunk costs, since both run `jj-core`'s word diff: about 15 µs per conflicting line on an M4 Pro, 1.6 s for one conflict over 100k lines. Many small conflicts meet the load target. `script/check-bench` holds the large case below its measured cost, so a regression still fails.

`resolve`'s repository steps have no target. jj's snapshot scans the working copy and costs what jj's own does: on an M4 Pro with 100k files, `jj util snapshot` takes 108 ms unchanged and 176 ms after one edit; `resolve` takes 105 ms to load a conflict and 168 ms to record a save. git stages a save in 30 ms with 100k index entries (`cargo bench -p merge-cli --bench resolve`).

Measured (DESIGN.md §A6.7) on 2026-11-08 on an M4 Pro, 120 Hz, with `TRIAD_MEASURE`, `/usr/bin/time -l` and the benchmarks:

| Operation | 100k lines | 1M lines |
|---|---|---|
| Load and merge | 46–50 ms | 521–548 ms |
| Command to first frame | 210–223 ms | 727–754 ms |
| Scrolling, frame interval p95 | 8.4 ms | 8.4 ms |
| Pick, mean / max | 0.4 / 0.6 ms | 4.4 / 6.3 ms |
| Keystroke, mean / max | 0.3 / 0.5 ms | 1.9 / 5.2 ms |
| Peak resident memory | 221 MB | 869 MB |

These figures are for plain text. With Rust highlighting (`syntax-highlighting = true`, §6.4), 100k lines peak at 223–228 MB with a first frame of 225–273 ms; the other figures hold. The top panes, and the result and diff editors past `syntax-whole-file-max-lines`, parse the chunks on screen as they show: the rows on screen take their colours under 200 ms after the first frame, and 1M lines peak at 911–934 MB with every pane coloured ([DESIGN.md](DESIGN.md) §F4, §F4.1, §F6.3).

An idle window draws nothing. The caret blinks for 10 s after the last key, click or focus, then stays shown; a window that is not frontmost shows no caret. While a window draws, the macOS Metal driver holds about 115 MB, which it reclaims once drawing stops. The diff of `fixtures/demo/code_a.py` and `code_b.py` in a 1400 × 887 pt window takes 212 MB while the caret blinks and 98 MB at rest ([DESIGN.md](DESIGN.md) §A6.7).

## 10. Agent-friendly practices

- triad is its own repository and Cargo workspace, with the repository root as the workspace root. All commands and paths in this spec are relative to it.
- `cargo build` and `cargo nextest run` are the only build and test commands. `script/package <target>` builds the package for one Linux or Windows target, and `script/sign <target>` signs it (M4b). macOS has no package in release 1.
- `triad-merge inspect --json` exposes the merge model, so agents and tests can check behaviour without a GUI.
- Each crate has a short README stating its role and public API. `AGENTS.md` lists commands, conventions and test layout.
- Logs use `tracing`, with `TRIAD_LOG` setting the filter (default `warn`), and go to stderr and a daily file in the platform cache directory, of which the last seven are kept. `TRIAD_LOG_DIR` puts the files elsewhere. A bad filter or an unusable directory gives one warning, and logging goes on without it.
- Fixtures live under `fixtures/` as plain text triples with expected outputs, named for the case they cover.
- `cargo deny` checks licences and advisories in CI. It rejects GPL and AGPL dependencies. This matters because Zed's editor crates are GPL, while GPUI and gpui-component are Apache-2.0.
- Dependency resolution takes only crate versions published at least three days ago (`.cargo/config.toml`), so a malicious release has time to be yanked first.

## 11. UI toolkit

The UI runs on GPUI, through `gpui-component` and its editor crate `gpui-base`, pinned exactly. The core and the view model do not depend on it.

- The top panes are one GPUI `uniform_list`, which builds only the rows on screen and holds the refresh rate at 1M rows.
- The result pane and the diff panes are `gpui-base` code editors. `vendor/gpui-base` carries triad's patch: full-line region fills, gutter marks, line labels, spacer rows and an edit observer (`vendor/gpui-base/PATCHES.md`).
- ⌘Z goes to the core's undo history, never the editor's (§4.4). Every edit reaches the core as a byte range (§4.5).
- Soft wrap lays out every line before the first frame, so it is limited to `SOFT_WRAP_MAX_LINES` (§5).
- Headless `TestAppContext` tests run under nextest without a display (§8.5).
- A fallback, if GPUI cannot carry a later need: GPUI directly, with custom panes; then another cross-platform Rust UI over the same `merge-view` crate.

`docs/gpui-kit-issues.md` lists the toolkit's known problems and triad's answer to each.

## 12. Milestones

| # | Milestone | Done when |
|---|---|---|
| M1 | `merge-core` (merge and two-way diff) and `merge-cli --headless` / `inspect` / `diff --inspect` | §8.1–8.3, §8.6 and §8.9 pass, and the oracle corpus matches |
| M2 | `merge-view` | §8.4 passes |
| M3 | GUI release 1 | §5–6 complete, §8.5 passes, §9 targets met |
| M4 | Merge and diff tool integration, macOS release | Works end to end from `git mergetool`, `git difftool`, `jj resolve` and `jj diff --tool` on macOS. Native `triad-merge diff PATH` works in git, jj and internal jj repos. §8.8 passes for repo detection and reading a file at a revision. `install` subcommand done. Installed from a local build. |
| M4b | Linux and Windows release | Release 1 behaviour on Linux x64, Windows x64 and Windows ARM64. Packages from §2.1 signed. The four git and jj tool integrations work end to end on each target. §9 targets met on each reference machine. |
| M5 | Chunk origin | §7.3 for git and jj. §8.8 annotation tests pass on both backends. |
| M6 | Native resolution | §7.4 for git and jj, including jj conflicts with more than two sides. §8.8 passes on both backends. |
| M7 | Syntax highlighting | tree-sitter for the main languages |

Version 1.0 needs M4 and M4b: macOS, Linux x64, and Windows x64 and ARM64, with the same features.

## 13. Decisions

1. Base pane: hidden by default; ⌘B shows it.
2. Line endings: regions keep their source bytes; typed newlines take base's dominant ending (§4.1).
3. Invalid UTF-8: refused with exit code 3. No Latin-1 fallback.
4. Unresolved conflicts: the result pane shows each side as a coloured block, left then right; the saved file gets diff3 markers (§4.4, §4.5).
