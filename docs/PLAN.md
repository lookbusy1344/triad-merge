# triad — plan

Inputs: [SPEC.md](SPEC.md). Rules for changes: `AGENTS.md`.

This is the one plan. §1–§5 hold the status, the next steps and the open work. §1 states where the project stands; the history is in jj. Parts A–G hold the design notes of each phase: the design, the reasons, the measurements and the decisions. A reference such as §B5.8 names part B, section 5.8. Where a later part changes an earlier one, the later part holds: part E's config file replaces the TOML settings of §A6.6, for example.

Every task follows the ground rules of §A1: tests first, expected values from the spec, hand calculation or an oracle, one task per commit, the spec updated in the same change, jj first with git parity.

## 1. Status

As of 2026-11-11.

| Milestone | State | Part |
|---|---|---|
| M1 `merge-core` and headless CLI | Done | §A4 |
| M2 `merge-view` | Done | §A5 |
| M3 GUI release 1, macOS | Done, except the spoken VoiceOver steps of the release checklist | §A6 |
| M4 tool integration, local macOS release | Done; release checklist passes on git and jj (§A7.3). `jj diff --tool` saves the right side to the working copy when it shows it (§A6.5); release checklist step 6 needs a rerun on jj | §A7 |
| Beta, macOS | Open: testers build `triad.app` and install it in `/Applications` (§A7.4) | §A7.4 |
| M4b Linux and Windows | Linux test machine and packages build (§E7.1); final checks need hardware, Windows a certificate | §A8, §E7 |
| Merge semantics: jj's merge, word level, refinements, syntax-aware boundaries | Done | B |
| M5 chunk origin | Done, with each input's commit in its pane heading (§C5.7) | C |
| M6 native resolution | Done: git stages a resolved save; jj snapshots before the read and after each save (§D5.5). A save that resolves some of a file's conflicts leaves jj a conflict of the rest, which `jj edit` and `jj new` carry and a later `resolve` loads, in a working-copy commit or an ancestor whose descendants rebase; git reopens it from the file's markers | D |
| M7 syntax highlighting | Done: Rust, Go, C#, Python, Swift, HTML, JavaScript, TypeScript, TSX | F |
| p4merge parity | Done: config file, overview ruler with the scrollbar thumb, scrollbars, output line ending, Save Copy As and key bindings | E |
| Review and diff features | Bulk takes (§G3) and line endings in comparison (§G4) done; the rest open | G |
| Code quality | Done, except the recovery gate (§A13.4) and the optional row-rendering move (§A13.3) | §A13 |

Every spec §9 target is met (§A6.7); README and spec §9 carry the figures.

The top panes select and copy their input's text (spec §5): `merge_view::selection` holds the selection as byte offsets in the input, and `merge-app`'s `input_select` maps pointer positions to them through the drawn text's layout.

Dependencies: jj-lib and jj-core 0.47.1 on gix 0.90 (§A7.1). gpui-kit 0.8.1 with the vendored `gpui-base` patch (`vendor/gpui-base/PATCHES.md`); the tree-sitter pins follow it. gpui-kit 0.9 is not adopted (`docs/gpui-kit-issues.md`).

The Dock icon needs `triad.app` (`script/macos-app`): `setApplicationIconImage` is unsafe in `objc2-app-kit`, so a bare binary cannot set it under the `unsafe_code` rule.

CI: `triad.yml` tests on Linux, macOS and Windows; `bench.yml` and `audit.yml` (`cargo audit`, `cargo deny check advisories`) run weekly. Linux and macOS pass on GitHub's runners and the Linux jobs under `act`; Windows waits on the next push.

The release profile builds with fat LTO, one codegen unit, `panic = "abort"` and stripped symbols: `triad-merge` is 35.4 MB on macOS ARM64, and a release build takes 3 min (root `Cargo.toml`).

The workspace has 11 crates. `script/checks` runs format, workspace rules, build, clippy, the full test suite and `cargo deny`; `script/checks --quick`, run before each commit, leaves out the jj, scrolling and highlighter tests and runs only the tests of the changed crates and their dependents.

## 2. Next steps

In order. Each is one or more commits.

1. **Beta reports.** Testers' reports come first. Each bug starts with a test that reproduces it; README § Report a problem says what testers send.
2. **Close M3.** The spoken VoiceOver steps of the release checklist, with the owner at the machine.
3. **Recovery gate** (§A13.4). Measure first; drop the item if the stall stays under one frame.
4. **Review and diff features** (part G), in its order: collapse unchanged regions (§G2), the rest of the comparison filters (§G4), moved code in the two-way diff (§G5); then §G6–§G14.
5. **More grammars,** one commit each (§F9, decision 1): C, C++, Java and Kotlin; then JSON, YAML, TOML, CSS and Markdown; then Bash, Ruby, PHP and SQL.
6. **M4b** (§A8, §E7) once the owner has the machines and the certificate of §E7.3.

## 3. Outstanding work

| Work | Blocked on | Where |
|---|---|---|
| Release checklist: spoken VoiceOver steps | The owner | §A6.7 |
| Per-command scans in `MergeWindow::refresh`: bands, placeholders, status counts; a pick at 1M costs 4.4 / 6.3 ms mean / max | A measurement that calls for them | §A6.7 |
| `DiffScroll`: the diff window's eight scroll fields in one component, as `PaneScroll` | A diff scroll bug | §A13.3 |
| Row rendering out of `window.rs`, optional: GPUI code, no unit tests to gain | — | §A13.3 |
| Recovery gate on the UI thread | A measurement | §A13.4 |
| Linux and Windows release | Hardware, certificate, tool installs | §A8, §E7 |
| Windows icon resource | A Windows build machine | §E7.2 |
| A histogram diff that gives git's hunks faster: 1.5 s against jj's 70 ms on the 100k-line `deep` triple | — | §B6 |
| More grammars | — | §F9 |
| Windows CI passing | A push | `.github/workflows/triad.yml` |
| Windows ARM64 in CI (`windows-11-arm`), before version 1.0 | Windows x64 passing in CI | `.github/workflows/triad.yml`, §E7 |
| A sideways scrollbar for the top panes: a `ScrollbarHandle` over `Sideways` | — | §E4.4 |
| Review and diff features, §G2, §G4–§G14 | Owner's choices on §G15's open points | G |
| Razor | A grammar that covers its code blocks | §F1 |
| Fuzzing | After release 1; `cargo-fuzz` needs nightly | §A4.9, spec §8.9 |
| Whole-text copies: `Recompute::run`, `MergeDocument::output`, 8-byte line starts | A measurement that calls for them | §A6.7 |
| Accessibility gaps: no live announcements, region bands and gutter marks not exposed | gpui-kit | `docs/gpui-kit-issues.md` |
| gpui-kit 0.9: build, rebase the patch, measure | — | `docs/gpui-kit-issues.md` |

## 4. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| gpui-component API churn on upgrade | Rework in `merge-app` | Exact pin; upgrade as a separate task; `merge-app` stays thin |
| The vendored editor patch drifts from new gpui-kit releases | Rework on upgrade | Keep the patch small and listed in `PATCHES.md` |
| Editor slow paths: very long lines, many cursors | Missed spec §9 targets | Benchmarks and `script/measure-app`; intra-line diffs capped |
| Editor and core text drift apart | Wrong output bytes | The core is the source of truth; tests compare the editor's rope with the core's after each command |
| jj-lib API changes between releases | `vcs-jj` breaks on bump | Exact pin; the conformance suite gates each bump |
| gix and jj-lib disagree on gix version | Two gix copies | `vcs-git` pinned to jj-lib's gix; `script/check-workspace` fails on two |
| Windows GUI and console split surprises git or jj | Lost exit codes or stderr | §A4.13 tests in CI; manual check in M4b |
| Linux desktop rendering | Poor Linux experience | Real-desktop check in M4b; record in `gpui-kit-issues.md` |
| Build time and disk use grow with the dependency tree | Slow iteration | §A13.5; the quick pre-commit profile |

## 5. Decisions

Each part keeps its decisions with their reasons and dates: §A11, §B7, §C6, §D6 and §F9, and inline where they arose: §A7.1 (gix 0.88 and read-only release 1, closed 2026-11-05), §A7.4 (local install, ad hoc signing), §E1 (parity scope), §E3.3 (config location). None is pending.

# Design notes
## A. Engine, view, windows and release (M1–M4b)

### A1 Ground rules

- **Spec first.** If a task finds the spec wrong or incomplete, update `SPEC.md` in the same change.
- **Expected values.** They come from the spec, from hand calculation, or from the git oracle corpus (§A4.8). Never from the code's own output.
- **Test runs.** While iterating, run targeted tests: `cargo nextest run --workspace -E 'package(<crate>) & test(<filter>)'`. Run the full suite once before each commit. Log output to a file and read its tail.
- **Lints.** `unsafe_code = "forbid"` and clippy pedantic with `-D warnings` apply from the first commit of every production crate (spec §2.3, §2.4).
- **Commits.** One task or sub-task per commit, as a conventional commit, through jj. Commits happen only when the owner asks. Nothing is pushed; the owner pushes. CI (`triad.yml`) runs on each push to `main`, each pull request and weekly, on Linux, macOS and Windows; docs-only changes skip it. `bench.yml` and `audit.yml` run by hand and weekly. Locally, `act` runs the Linux jobs.
- **git and jj parity** (spec §1.2). Every VCS task is done when it passes the conformance suite on git, jj and internal jj repos. Shared types follow jj's model: n sides, n − 1 bases, an optional label per term. git fills the same types. Facts that only one VCS records are optional fields, never a separate code path in `merge-view` or `merge-app`.
- **Zed first.** Before designing any UI, editor or VCS-integration task, check how Zed solves it, and record what was found and the source in the task. Spec §2.5 sets the licence rule: study the design, never copy GPL code. Tasks with a known Zed counterpart:
  - §A5.2 and §A6.3: scroll sync, from Zed's split diff.
  - §A6.1 and §A6.5: display-map layers and spacer rows, from Zed's block map.
  - §A6.2: action and keymap contexts.
  - §A6.2: showing unresolved conflicts as markers in the editor, from Zed's inline conflict resolution.
  - §A7.1: repository discovery; jj wins when `.jj` and `.git` share a directory.
  - M5: blame display.

### A2 Workspace layout

```
triad/
  Cargo.toml                workspace, lints, shared dependency versions
  rust-toolchain.toml       the stable channel
  deny.toml                 cargo-deny: licences, advisories, bans
  AGENTS.md                 commands, conventions, test layout
  .config/nextest.toml
  crates/
    merge-core/             text, diff, three-way alignment, resolution, edits, undo, output
    merge-syntax/           languages, grammars, highlight spans, syntax-aware merge boundaries
    merge-view/             view model: rows, navigation, scroll mapping, commands
    merge-session/          settings, semantics, discovery, conflicts, origins, file reads and writes
    merge-cli/              argument parsing, headless modes, inspect, install (library)
    merge-app/              GPUI shell (library)
    triad-merge/            binaries: triad-merge, triad-merge-gui (Windows)
    vcs/                    Repository trait and shared types
    vcs-git/                gix backend
    vcs-jj/                 jj-lib backend
  vendor/gpui-base/         patched copy, from M3 (§A6.1)
  fixtures/
    core/                   hand-written triples with expected outputs
    oracle/                 generated by script/gen-oracle from git merge-file
  script/
    gen-oracle              dev-time only; uses git merge-file
    package <target>        Linux and Windows, M4b
    sign <target>           Linux and Windows, M4b
  docs/
```

The `triad-merge` crate holds both binaries, depending on `merge-cli` and `merge-app` (spec §3). Keeping `merge-cli` a GUI-free library means `--headless` and `inspect` tests never link GPUI.

### A3 Shared dependency choices

| Need | Crate | Version policy |
|---|---|---|
| Line diff | `imara-diff` | 0.2.x |
| Result text model in core | `ropey` | 1.6.x. gpui-base uses ropey 2 beta internally; the two never cross. |
| GUI | `gpui-kit` (re-exports `gpui-component` and `gpui-base`) | `=0.8.1`, exact pin; bump on purpose |
| CLI | `clap` (derive) | 4.x |
| Errors | `thiserror` in libraries, plain `main` in binaries | latest |
| Serialisation (journal, `--json`) | `serde`, `serde_json` | 1.x |
| Paths | `camino` | 1.x |
| Atomic write | `tempfile` | 3.x |
| Config dirs | `directories` | latest |
| File watching | `notify` | 8.x, the version gpui-component already uses |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | latest |
| Property tests | `proptest` | 1.x, fixed seed |
| Benchmarks | `criterion` | latest |
| Git | `gix` | exact pin, the version jj-lib uses: 0.90 with jj-lib 0.47.1 |
| jj | `jj-lib` | exact pin, `git` feature only; bump with gix |
| Async bridge | `pollster`, `futures` | already in the tree through jj-lib; `vcs-jj` runs jj-lib's async APIs to completion |
| Test corpus | `serde`, `serde_json` | dev-dependencies of merge-core, to read the oracle corpus |
| Conflicted git fixtures | `gix-merge`, through gix's `merge` feature | M6; dev-dependency of `vcs-git` only. The product never merges trees. |

### A4 Phase M1 — `merge-core` and headless CLI

M1 needs no GUI. It produces a correct, tested engine and the `triad-merge --headless`, `inspect` and `diff --inspect` commands.

#### A4.1 Workspace bootstrap

- Move the spike's seeded fixture generator to `merge-core::testing`, behind a `testing` feature. Then delete `spike/`, `fixtures/spike/` and `script/bundle-spike`, and record commit `65d590d`. The spike's seven alignment tests are ported from that commit in §A4.4, where the code they test is written.
- Replace the workspace manifest with the full layout from §A2. `merge-app` takes its GPUI dependency from the start, so CI builds GPUI on every target from M1. `vcs-git` and `vcs-jj` stay empty until M4: every jj-lib release so far needs gix 0.88, which carries GHSA-q54r-jf2p-rfr2.
- Add `deny.toml`. Allowed licences: MIT, Apache-2.0 (with or without LLVM-exception), BSD-2/3, 0BSD, CC0-1.0, ISC, Unicode-3.0, Zlib, MPL-2.0. GPL and AGPL are denied. Unmaintained-crate advisories fail only for direct dependencies.
- Add `script/check-workspace`: every crate sets `[lints] workspace = true`, and the build holds one gix version.
- Add `script/checks` (the AGENTS.md checklist), `script/pre-commit`, `script/pre-push` and `script/agent-pre-push-hook`, after the pattern in HashRust. Claude Code runs the push hook through `.claude/settings.json`.
- Add `AGENTS.md` and a short README per crate (spec §10).
- Add a CI workflow at `.github/workflows/triad.yml`:
  - Matrix: `macos-latest` (arm64), `ubuntu-latest`, `windows-latest`. `windows-11-arm` joins before version 1.0 (§3).
  - Linux installs `pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev libvulkan-dev libfontconfig-dev libfreetype-dev libasound2-dev libzstd-dev`.
  - A `checks` job on Linux: `cargo fmt --check`, `script/check-workspace`, `cargo deny check`.
  - A `test` job on each target: `cargo clippy --workspace --all-targets -- -D warnings`; `cargo nextest run --workspace --profile ci`.
  - Test the workflow locally with `act` for the Linux jobs. act needs OrbStack running.
- Done when the Linux job passes under `act`, and CI passes on the three targets after the owner pushes. Linux and macOS pass; Windows waits on the next push (§3).

#### A4.2 Text loading (`merge-core::text`)

Types:

```rust
pub enum Encoding { Utf8, Utf8Bom, Utf16Le, Utf16Be }
pub enum LineEnding { Lf, CrLf }
pub struct Text {                              // immutable once loaded
    encoding: Encoding,
    content: Arc<str>,                         // decoded
    line_starts: Vec<usize>,                   // byte offsets; lines end at '\n' and include it
}
pub enum DecodeError {                         // offsets count bytes from the file start, BOM included
    Binary,
    InvalidUtf8 { offset: usize },
    InvalidUtf16 { offset: usize },            // unpaired surrogate
    OddUtf16Length,
}
impl Text {
    pub fn decode(bytes: &[u8]) -> Result<Text, DecodeError>;
    pub fn encode(&self, text: &str) -> Vec<u8>;  // writes in self's encoding and BOM
    pub fn line(&self, n: usize) -> &str;
    pub fn line_count(&self) -> usize;
    pub fn dominant_ending(&self) -> Option<LineEnding>;  // None when the text has no '\n'
}
pub fn output_encoding(base: &Text, left: &Text) -> Encoding;   // base's, or left's if base is empty
pub fn typed_ending(base: &Text, left: &Text) -> LineEnding;    // spec §4.1
```

Lines end at `\n` only. A lone `\r` is a character inside a line, as in git and the result editor. `decode` works on bytes; reading files and reporting I/O errors belongs to the caller (§A4.10).

Tests, written first:

- BOM detection for each encoding. A UTF-16 file with NUL bytes is not treated as binary.
- Binary: `BINARY_SNIFF_BYTES` is 8000, git's rule. A NUL at index 7999 makes the file binary; a NUL at index 8000 does not.
- Invalid UTF-8 reports the byte offset of the first bad sequence. An unpaired UTF-16 surrogate reports its byte offset.
- Line splitting: `\n`, `\r\n`, a lone `\r` inside a line (`"a\rb\n"` is one line), mixed endings, no final newline, empty file (0 lines), a file of one `\n` (1 line).
- Dominant ending: more CRLF than LF lines gives `CrLf`; a tie gives `Lf`; no `\n` gives `None`.
- `typed_ending`: from base; from left when base has no `\n`; `Lf` when neither has one.
- Round trip: decode then encode gives the input bytes, for every encoding.
- `output_encoding`: base's encoding and BOM; left's when base is empty.

#### A4.3 Diffing (`merge-core::diff`)

```rust
pub enum Algorithm { Histogram, Myers, MyersMinimal }
pub enum Whitespace { None, Trailing, Amount, All }
pub struct DiffOptions { algorithm: Algorithm, whitespace: Whitespace }
pub struct Hunk { before: Range<usize>, after: Range<usize> }   // line ranges
pub fn diff_lines(before: &Text, after: &Text, opts: DiffOptions) -> Vec<Hunk>;
pub fn diff_chars(before: &str, after: &str) -> Vec<Hunk>;      // intra-line, byte ranges
pub fn intraline_hunks(before: &str, after: &str) -> Option<Vec<Hunk>>;  // None above the limit
pub fn line_key(line: &str, mode: Whitespace) -> Cow<'_, str>;
```

Line diffs run imara's indent heuristic, as git's diff does. The merge's diffs do not (§A4.4). Character diffs always use Myers, because histogram does badly on small alphabets.

Whitespace modes rewrite the interning key only. A line's key under `Trailing` drops trailing spaces, tabs, `\r` and the `\n`; under `Amount` it also collapses runs; under `All` it removes all whitespace. Whitespace is space, tab and `\r`, as in git's `isspace`.

Tests:

- Hand-built pairs that every algorithm must agree on. Hunk ranges are worked out on paper.
- One pair where histogram and minimal Myers differ, and one where the indent heuristic moves an insertion, with expected hunks from `git diff --no-index -U0`.
- Properties (proptest, fixed seed): hunks are ordered, disjoint and in bounds; lines outside hunks pair with equal keys in every mode; applying the hunks rebuilds `after` exactly; the same holds for character hunks.
- Each whitespace mode, with inputs that differ only in that mode's whitespace (no hunks), and one real change (one hunk).
- `diff_chars` on identical strings, a pure insertion, a replacement in the middle, and multi-byte characters. Ranges must land on UTF-8 boundaries.
- `diff_chars` is not called above `MAX_INTRALINE_DIFF_CHARS` (10 000); the caller enforces this, and a test covers the caller.

#### A4.4 Three-way alignment (`merge-core::merge3`)

N-sided from the start (spec §4.4, §7.4):

```rust
pub enum Source { Side(usize), Base(usize) }
pub struct Inputs { sides: Vec<Text>, bases: Vec<Text> }   // Inputs::new: sides.len() == bases.len() + 1 >= 2
pub enum Kind { Unchanged, OneSide(usize), AllSame, Conflict }
pub struct Region {
    pub kind: Kind,
    pub bases: Vec<Range<usize>>,    // one range per base
    pub sides: Vec<Range<usize>>,    // one range per side
}
pub fn align(inputs: &Inputs, opts: DiffOptions, refine: bool) -> Vec<Region>;
```

For a three-way merge, `OneSide(0)` is the spec's `LeftOnly`, `OneSide(1)` is `RightOnly`, and `AllSame` is `BothSame`. The algorithm is the spike's cluster sweep, generalised:

1. Diff base 0 against every other side and base, placing hunks as git `merge-file` does (spec §4.2): `diff_lines_for_merge`, with git's histogram in `histogram.rs`.
2. Merge hunks that overlap or touch in base 0 coordinates.
3. Track each input's line delta to place its ranges.
4. Classify each cluster: `AllSame` when every side holds the same text, else jj's trivial resolution (`OneSide(i)`), else `Conflict`. Lines compare by their whitespace-mode keys.
5. Refinement (on by default) splits off the lines that all sides share at the start and end of a conflict into `AllSame` regions with empty base ranges. The conflict keeps its whole base ranges. Lines shared inside a conflict stay in it. Conflicts are never joined. This is `git merge-file --zdiff3`, checked by hand: shared leading and trailing lines move out of the markers, a shared middle line stays in, and two conflicts separated by one line stay two conflicts, even when the separating lines hold no letters or digits.

Tests:

- The spike's seven alignment cases, ported with the same hand-worked expectations.
- Insertions at the same base position on both sides, the same and different. Deletion against modification. A whole-file conflict with an empty base (add/add).
- Refinement: a conflict with two shared leading lines becomes `AllSame` (2 lines) then `Conflict`, and the same for trailing lines. A shared middle line stays inside the conflict. Two conflicts separated by `}` stay two regions.
- Whitespace mode: a side that changed only whitespace under `Trailing` produces `Unchanged` for comparison. The output-byte rule from spec §4.2 is tested in §A4.5 below.
- One N-sided case: three sides and two bases, with a conflict on one line. Worked out by hand. Also a trivial resolution where each other side cancels a base.
- Properties: regions cover every input in order; swapping left and right swaps `OneSide(0)` and `OneSide(1)`; a side equal to base gives no conflict.
- Oracle (§A4.8): in both modes, the conflict count and the automatic text between conflicts equal git's, except the known divergences listed in the test.
- git's histogram, with expected hunks from `git diff --no-index --histogram --no-indent-heuristic -U0`: anchor choice without prefix stripping, skipping lines commoner than the best anchor, resuming the scan past a run, the 64-occurrence limit and its Myers fallback, sliding down, lining up with a change on the other side, and the re-diff of a moved group.

#### A4.5 Resolution, result text and output (`merge-core::document`)

`MergeDocument` owns everything a merge session changes:

```rust
pub enum Resolution { Auto, Pick(Vec<Source>), Edited, Unresolved }  // Edited text lives in `result`
pub struct MergeDocument {
    inputs: Inputs,
    options: MergeOptions,             // algorithm, whitespace, refine, marker length
    regions: Vec<Region>,
    resolutions: Vec<Resolution>,
    result: Rope,                      // ropey; the source of truth for result text
    region_lens: RegionLengths,        // Fenwick tree of region byte lengths
    typed_ending: LineEnding,
    history: History,                  // undo/redo
}
pub struct TextPatch { range: Range<usize>, text: String }  // result byte offsets, pre-edit
pub struct EditBatch(Vec<TextPatch>);  // sorted, non-overlapping; one user action
pub enum Command { Toggle { region, source }, Clear { region }, Edit(EditBatch), Undo, Redo }  // Edit lands in §A4.6
pub struct Applied { patches: Vec<TextPatch>, regions_changed: bool }  // patches sorted, offsets in the text before them
impl MergeDocument {
    pub fn apply(&mut self, cmd: Command) -> Applied;     // patches the view applies to its editor
    pub fn output(&self) -> (String, bool /* fully resolved */);
    pub fn region_at_byte(&self, offset: usize) -> usize; // O(log n)
    pub fn region_bytes(&self, region: usize) -> Range<usize>; // O(log n)
    pub fn line_of_byte(&self, offset: usize) -> usize;   // O(log n), via ropey
}
```

The core is the single source of truth. Every change, including undo, returns the minimal patches the GUI applies to its editor. The GUI never replaces the whole text. Region offsets live in a Fenwick tree of region lengths, so an edit updates one entry and a lookup is a prefix search: both O(log n). This is the incremental mapping that spike follow-up 4 requires. `regions_changed` tells the view that regions merged or split, so it rebuilds its region-indexed state (§A5).

Rules, each from spec §4.4:

- `Toggle` appends or removes a source.
- An empty `Pick` removes the region's text.
- `Pick` on an `Edited` region replaces the edit; undo restores it.
- Unresolved regions render diff3 markers. The marker length is the larger of the requested length and one more than the longest marker-like run at line start in any input.
- Whitespace-only regions take the changed side's bytes. If both sides changed, they take left's.

`History` holds one entry per command. Each entry stores the inverse patches and, when the command merged regions, the regions, resolutions and lengths it replaced. Consecutive single-patch insertions at the caret with no `\n` merge into one entry until another command, undo and redo included, runs between them, the same rule as the editor's own undo manager (`gpui-base` `undo_manager.rs`). A multi-cursor batch is one entry.

Tests:

- Each `Pick` order for two sides, plus base. Toggle on, toggle off, clear.
- The patches returned by each command, worked out by hand: offsets and text.
- Output for: all resolved; one unresolved, with markers; marker growth when an input line starts with `<<<<<<<`.
- Marker line endings by the spec §4.4 rule: CRLF file, LF file, mixed sides, conflict at the first line, add/add, and a section without a final newline. Expected bytes from the `seed-crlf-markers-*` oracle cases.
- Undo/redo sequences of picks, and a new command dropping the redo history. The sequences with edits (pick → edit → pick → undo × 3 → redo × 3, and undo grouping: typing `abc` is one step; `ab`, newline, `c` is three; a paste is one; a three-cursor batch is one) need `Command::Edit` and land in §A4.6.
- Oracle (§A4.8): the output, the resolved flag and the marker length equal git's for every case in both modes, except the known divergences.
- More than two sides: jj snapshot markers, with and without labels.
- The whitespace output rule from spec §4.2.

#### A4.6 Edit model (`merge-core::document`, continued)

`Command::Edit(EditBatch)` comes from the editor's `InputEdit`s for one user action (spike §2). Rules from spec §4.5:

- An edit inside one region makes it `Edited`.
- An edit across region boundaries merges the touched regions into one `Edited` region. Its ranges are the union of theirs. Its kind is `Conflict` if any merged region was a conflict, otherwise the first region's kind.
- A zero-width insertion after a byte other than `\n` joins the region holding that byte. At a line start it goes to an empty region at that offset if there is one (the first, if several), otherwise to the region after the boundary, or to the last region at end of file. Without the first clause, typing `ab` into an empty region would put `b` in the next region.
- Deleting a whole region's text leaves an `Edited` region with empty text. The region stays, so it can be typed into again.
- A newline typed or pasted into the result takes `typed_ending` (spec §4.1). The editor has already made the user's edit, so `apply` returns only the corrections: in a CRLF file a patch that turns each inserted `\n` into `\r\n`, in an LF file one that turns `\r\n` into `\n`. The view applies them inside the same reentrancy guard.
- Undo of a merging edit restores the regions it merged.

Tests:

- Each rule, with byte offsets worked out by hand.
- Typing one character into an unresolved region's markers: the region becomes `Edited`, and its text is the old marker text with that character.
- Typing into a region picked as empty, between two non-empty regions: the text goes to the empty region.
- A delete across three regions, then undo: three regions with their old kinds, resolutions and lengths.
- Paste over a whole region. A paste containing `\n` into a CRLF file yields `\r\n` and returns the correcting patch.
- A batch of three insertions in three regions makes all three `Edited` and undoes in one step.
- `region_at_byte`, `region_bytes` and `line_of_byte` after a sequence of edits, against expected values counted by hand. Lone `\r` doesn't start a line.

#### A4.7 Two-way diff (`merge-core::diff2`)

```rust
pub enum Kind2 { Unchanged, Changed }
pub struct Region2 { kind: Kind2, left: Range<usize>, right: Range<usize> }
pub struct DiffDocument { left: Buffer, right: Buffer, regions: Vec<Region2>, version: u64 }
pub struct Buffer { text: Rope, history: History, read_only: bool }
pub struct Edited { side: Side, version: u64, patches: Vec<TextPatch> }  // as `Applied` in §A4.5
impl DiffDocument {
    pub fn edit(&mut self, side: Side, batch: EditBatch) -> Result<Edited, DiffEditError>;  // ReadOnly, NoRegion, Edit(BatchError)
    pub fn copy_region(&mut self, region: usize, to: Side) -> Result<Edited, DiffEditError>;
    pub fn undo(&mut self, side: Side) -> Edited;  // and redo
    pub fn recompute(&self) -> Recompute;  // snapshot to diff off-thread
    pub fn install(&mut self, version: u64, regions: Vec<Region2>) -> bool;  // false for a stale result
}
pub struct Recompute { version: u64, left: Rope, right: Rope, opts: DiffOptions }  // O(1) rope clones
impl Recompute { pub fn run(self) -> (u64, Vec<Region2>); }
```

The view runs `Recompute::run` on a background task. It installs the result only if `version` still matches, which handles the debounce of spec §4.6. `copy_region` recomputes first when the regions lag the text, so a copy during the debounce uses the current lines. Each buffer has its own history with the §A4.5 undo grouping, and typed newlines take that buffer's dominant ending. `merge-core::edit` holds what both documents share: `TextPatch`, `EditBatch`, batch checks, inverse patches and the line-ending fixes.

Tests:

- The spec §8.1 two-way list.
- Copying left to right, and right to left, then undo.
- A stale recompute result is rejected.
- Editing a read-only side returns `Err(ReadOnly)` and leaves the text unchanged.

#### A4.8 Oracle corpus

- `script/gen-oracle` generates about 2,000 triples:
  - from small hand-written seeds;
  - from proptest-style mutations with a fixed seed;
  - from real files taken from this repo's history.
- It needs git 2.54 or later and checks the version. It runs `git merge-file -p --diff-algorithm=histogram` with `--diff3` (refinement off) and `--zdiff3` (refinement on). It records output and exit code under `fixtures/oracle/`. The exit code is the conflict count.
- A test in `merge-core` checks every case in both modes. When git reports no conflict, the automatic output must match byte for byte. When git reports conflicts, the conflict count must equal git's exit code, and the output outside the markers must match.
- `script/gen-oracle` computes each case's marker size by the spec §4.4 rule and passes it to git as `--marker-size`, because git keeps the requested size. The size is recorded in the case.
- The corpus is one file, `fixtures/oracle/corpus.jsonl`: one JSON object per case with `id`, `base`, `left`, `right`, `marker_size`, and `diff3` and `zdiff3` results (`exit`, `output`). Generation is deterministic: hand seeds, a fixed RNG seed, and triad files at pinned commits. History cases take the first 200 lines of each version. Only triad's own files are used.
- Corpus checks that hold before the merge exists: every source is present with unique ids; git's exit code equals the count of each marker line; a side equal to base gives the other side cleanly; identical sides merge cleanly.
- libgit2's file-merge tests (`tests/libgit2/merge/files.c`, `merge/trees/whitespace.c`, `merge/trees/automerge.c`) supply a case list, not data. libgit2 is GPL-2.0; its linking exception covers linking the library, not copying its tests. Each scenario becomes a hand seed written from scratch, named `seed-libgit2-<scenario>`, with git's output as the expected value. Scenarios: clean merge of edits on separate lines; overlapping edits; empty base or side; one side deletes all lines; whitespace changes at line end and within a line under `Trailing` and `Amount`; inputs without a final newline gain none; CRLF inputs get CRLF markers in both modes; zdiff3 moves common leading and trailing lines out of the conflict. Binary input and favour-a-side options are out of scope for the oracle: binary is tested in §A4.2, and triad has no favour option.
- There is no second oracle of git's merge. `threeway_merge` wraps git's own xdiff under LGPL-2.1, so it is neither independent of git nor allowed by `deny.toml`. The jj corpus, `fixtures/oracle/jj-corpus.jsonl`, records jj's merge of the same triples and of jj seeds; `script/gen-jj-oracle` writes it (§B5.2).

#### A4.9 Properties

- Proptest suites for spec §8.2, with a fixed seed and checked-in regression files. Alignment properties sit in `merge3`; output, two-way and edit-sequence properties in `merge-core/tests/properties.rs`.
- Fuzzing moves to after release 1 (spec §8.9).

#### A4.10 Headless CLI (`merge-cli`, `triad-merge`)

- `clap` definitions for every form in spec §6.1, including the positional git form, `--title` and the `--label-*` flags.
- An `ExitCode` enum with values 0–3 (spec §6.2) and one conversion point.
- `--headless` writes the automatic merge atomically (§A4.11) and exits with 0 or 1.
- `inspect --json` and `diff --inspect --json` print a stable schema: regions, kinds, ranges, conflict count, encodings and an encoding-mismatch warning. The schema is documented in `docs/json-schema.md` and versioned.
- Logging: `tracing` to stderr and a rotating file in the platform cache directory, filtered by `TRIAD_LOG`. `TRIAD_LOG_DIR` overrides the directory; the binary tests use it to keep the user's cache clean.
- Integration tests run the built binary on `fixtures/core` and check output bytes, exit codes and stderr text. Each `fixtures/core/<case>/` holds `base`, `left`, `right`, the `expected` output bytes, the `exit` code, and optionally `args`, one extra argument per line.
- Until the window exists (M4), the window forms exit with 3 and say so on stderr.

#### A4.11 Atomic write (`merge-core::io`)

Writes to `tempfile::NamedTempFile` in the target's directory, copies the target's permissions when it exists, then persists with a rename. A new file gets the mode any new file gets (`0o666` less the umask), not `tempfile`'s `0o600`. Tests: the permissions are kept, including the executable bit on Unix; a new file's mode matches one made by `std::fs::write`; a failed write leaves the target untouched and no temporary file (simulated with a read-only directory, and with a directory as the target); a bare file name writes into the working directory; a missing directory gives `WriteError` with `NotFound`. §A4.10 maps `WriteError` to exit code 3 and tests it there.

#### A4.12 Benchmarks

`criterion` benches in `merge-core/benches/merge.rs`, on 10k, 100k and 1M lines from the seeded generator in `merge-core::testing`:

- `decode`, `align` and `output`: no target of their own; they show where `load` spends its time.
- `load`: decode three inputs and build the `MergeDocument`. Spec §9 load and merge.
- `load-jj` and `align-jj`: the same in jj mode (§B6). `load-jj` has `load`'s target.
- `load-word-small` and `load-word-large`: jj mode at the word level, on `testing::generate_word_conflicts` with one word-resolvable conflict every 20 lines, or one conflict over the whole file. `load-word-small` has `load`'s target; `load-word-large`, exempt from spec §9, has a regression ceiling at about its measured cost (§B6).
- `toggle`: take left or right on one region. Spec §9 take left/right.
- `keystroke`: one typed character at the caret, timed over runs of 1 000 keystrokes into a fresh document. Spec §9 keystroke.

`script/check-bench` reads criterion's results and fails when a mean is above 120% of its spec §9 target. The `bench` workflow runs it by hand and weekly on `ubuntu-latest` with `TRIAD_BENCH_TOLERANCE_PERCENT=500`: load/1M measured 834, 1223 and 1401 ms there over three runs of unchanged code, against 450 ms on a reference machine. The word-level and bounded benches merge on rayon's pool and lose more on the runner's 4 vCPUs. On 2026-11-05 `load-word-small/1M` took 2973 ms there against 608 ms, and `load-word-small-rust-bounded/1M` 3117 ms against 515 ms, 3.1 times its target. Five times the target keeps that case inside the runner's spread. That runner is not a reference machine, so the job catches gross regressions, and the reference machines decide the targets. Memory peaks do not vary and keep their limits.

Release build on an M4 Pro: `load` takes 25 ms at 100k lines and 451 ms at 1M lines, `toggle` and `keystroke` about 1 µs.

#### A4.13 Windows console binary

The console binary (spec §6.1) starts `triad-merge-gui.exe`, waits, and forwards its exit code. M1 builds the forwarding in `triad-merge`. `merge_cli::forward` runs the child with inherited stdio and maps its status to `ExitCode`; any other code, a signal or a failed start gives 3 with a message. `triad-merge-gui` runs the same command line and window forms through `merge_app::run`. The tests' child is the `forward_stand_in` example of `triad-merge`: it names its arguments on stderr and exits with `TRIAD_STAND_IN_EXIT`, or 3. Tests: on every OS, `forward` passes 0–3 through and turns other codes and a missing binary into 3; on the Windows CI targets, `triad-merge.exe` forwards a window form's arguments and exit code, and the child's stderr reaches the caller.

**M1 done when:** §A4.1–4.13 pass on every CI target, and the oracle corpus matches in both modes.

### A5 Phase M2 — `merge-view`

A plain-Rust view model with no GPUI dependency. Every lookup is by index or offset, in O(log n). Nothing builds a list of all rows: at 1M lines that is tens of MB, rebuilt after every merging edit.

#### A5.1 Rows

```rust
pub struct Row { region: usize, cells: SmallVec<[Option<usize>; 3]> }  // one cell per visible pane
pub enum PaneSet { LeftRight, LeftBaseRight, Sides(usize) }            // the first N sides, for jj
pub struct RowMap { /* merge_core::lengths::RegionLengths of rows per region */ }
impl RowMap {
    pub fn new(doc: &MergeDocument, panes: PaneSet) -> Result<Self, PaneError>;  // a pane without an input
    pub fn row_count(&self) -> usize;
    pub fn row(&self, doc: &MergeDocument, index: usize) -> Row;     // O(log n)
    pub fn first_row(&self, region: usize) -> usize;                  // O(log n)
    pub fn rows_in(&self, region: usize) -> usize;                    // O(1)
    pub fn update(&mut self, doc: &MergeDocument, applied: &Applied); // after regions merge or split
}
```

Tests: rows for each region kind; filler placement; base pane on and off; three sides; `row` and `first_row` after a merging edit and after its undo. Filler rows sit at the end of a region. A region every visible pane leaves empty, such as a deletion on both sides with the base pane off, has no rows. `update` rebuilds the tree only when `Applied::regions_changed`: O(regions).

#### A5.2 Scroll mapping

The spike's mapping, moved out of the view and made pure. It reads `RowMap::first_row` and `MergeDocument::region_bytes`, so it holds no per-region state of its own.

```rust
pub fn rows_to_result(rows: &RowMap, doc: &MergeDocument, top: f32, row_h: f32, line_h: f32) -> f32;
pub fn result_to_rows(rows: &RowMap, doc: &MergeDocument, top: f32, row_h: f32, line_h: f32) -> f32;
```

Tests: the spike's two conflict cases; regions whose result is shorter than their rows (a deletion); the last region; empty documents. A position maps to the same region at the same line within it, held to the region's last row or line, and keeps its fraction. Positions before the start or past the end hold to the first or last row or line. `MergeDocument` gains `line_count` and `byte_of_line`, and `RowMap` gains `region_of_row`.

#### A5.3 Navigation and current region

Next and previous difference; next and previous unresolved conflict; first and last; current region from a result byte offset. Tests cover the ends (no wrap-around: navigation stops there), a document with no differences, and the current region after an edit merges regions. `merge-view::nav` holds `next`, `previous`, `first` and `last` for a `Target` (`Difference` or `Unresolved`), and `current_region`. A difference is a region whose kind is not `Unchanged`, so whitespace-only changes are skipped (spec §4.2). A step scans the regions between its start and its target.

#### A5.4 Highlight spans

Intra-line spans for a row, computed on demand and cached by region in an LRU (`INTRALINE_CACHE_REGIONS` = 256). Tests: spans for a replaced word; no spans above the length limit; the cache empties after regions merge.

`merge-view::highlight::Highlights` follows the pairing rule of spec §5. Top-pane spans depend only on the inputs and the regions, so an edit that keeps the regions keeps the cache, and `update` empties it when `Applied::regions_changed`. Two-way mode, where edits change the texts, needs its own cache keyed by `DiffDocument::version`; that comes with §A6.5.

#### A5.5 Status

Counts and labels (spec §5), the encoding and its mismatch warning, the line ending and the whitespace mode. Tests: counts after picks, edits and a merging edit. `merge-view::status` gives `status` and `pane_titles`. Counting visits every region once per command. `MergeDocument` gains `labels()` and `options()`.

**M2 done when:** all of the above pass, and the spike's view logic (commit `65d590d`) has an equivalent in `merge-view`.

### A6 Phase M3 — GUI release 1 (macOS)

#### A6.1 Editor patch for full-line fills

- Vendor `gpui-base` 0.8.1 under `vendor/gpui-base`, carry the patch there, and point at it with `[patch.crates-io]`. The patch carries Apache-2.0 attribution. `cargo deny` covers it.
- The patch adds a `RangeDecorationStyle::FullLine` fill and gutter marks for decorated ranges.
- [gpui-kit-issues.md](gpui-kit-issues.md) records the problems found: fills, the per-frame whole-text copy in `Input::render`, the soft-wrap start-up cost. Nothing is reported to other projects (AGENTS.md).
- Tests: a headless test asserts the fill covers the full content width (using the layout-inspection pattern from gpui-base's own decoration tests).
- Zed: its editor paints row highlights and diff hunk backgrounds as quads across the text area, under the text, and diff hunk status as a coloured strip in the gutter. triad follows that design and adds a letter, because colour must not be the only signal (spec §6.5). Source: Zed's editor behaviour, studied in use; no code read or copied.
- As built:
  - `RangeDecorationStyle::FullLine`, plus `GutterMark` on any decoration. `EditorState::gutter_marks(true)` reserves a 15 px column between the line numbers and the fold icons: a 12 px label, then a 3 px strip. The label sits on the first visible row of the range.
  - The vendored crate sits outside the workspace. `script/checks` runs its 1438 unit tests, 7 of them triad's: fills over empty, mid-line and wrapped rows; horizontal and vertical scroll; gutter width; strip and label geometry; marks off; empty ranges.
  - The whole-text copy is in `gpui-component`'s `Input::render`, not in `gpui-base`. It runs only with an accessibility client or `test-support`. Left for now; §A6.7 decides from measurement whether to vendor `gpui-component` as well.

#### A6.2 Window and panes

- `merge-app::MergeWindow` owns the `MergeDocument` and the view model.
- Top panes: a `uniform_list`, as in the spike, reading rows from `RowMap`, with a hit target per region for choosing a source.
- Result pane: `EditorState` with folding and indent guides off, and line numbers on. Soft wrap is off unless enabled in settings, and only for inputs of up to `SOFT_WRAP_MAX_LINES` (10 000) lines. Multi-cursor stays on.
- Unresolved conflicts show as diff3 markers in the text, with a region band and a gutter letter.
- An edit observer (`EditorState::set_edit_observer`, a gpui-base patch) receives the editor's `InputEdit`s, groups the edits of one action into an `EditBatch`, and sends `Command::Edit`. The highlighter slot stays free for syntax highlighting (§F3). The editor's own undo is bypassed through the `MergeResult > Input` binding.
- Patches from the core are applied with `replace_text_in_range`, which takes UTF-16 offsets. The view converts byte offsets through the editor's rope, O(log n) per patch. A reentrancy guard keeps the view from feeding these patches back to the core as user edits.
- Status bar, toolbar, menus and shortcuts as in spec §5–6, including find (⌘F) and go to line (⌘L) through the editor's search and the view's line mapping.
- Pane titles come from `--label-*`, then from file names; the window title from `--title`.
- Started with no arguments: an open dialog with a Merge tab of four file slots and a Diff tab of two (spec §6.1).
- Part 1: `MergeWindow` with the row list, the result editor, edit routing, undo and redo through the core, region bands and gutter marks (`merge-view::marks`), and the status bar.
  - The editor applies a multi-cursor action from the highest offset down, so each edit's range is in the text before the action. The recorder turns the edits into one `EditBatch` without offset arithmetic.
  - The window sends edits when the editor reports a change. Each keystroke arrives alone in the running app. When several wait, they go as one patch, the difference between the core's text and the editor's, because the core's corrections to the first would shift the rest.
  - Undo and redo send waiting edits first. One binding in the window's key context wins over the editor's own, because it is registered after `gpui_kit::init`.
- Part 2: picks by key and by hit target, navigation, the current region, and the base pane toggle.
  - The editor's own bindings sit in a deeper key context and win over the window's, so `merge_app::init` binds each window key twice: for the window, and for the result editor inside it. ⌥⇧↑/↓ add cursors, since ⌥⌘↑/↓ navigate (spec §6.3).
  - The window pins the region that navigation or a pick made current, with the caret it left. An empty region holds no byte, so the caret alone cannot name it.
  - gpui-kit's `test-support` adds the `gpui-fast` crates to the lockfile, through weak dependency features (rust-lang/cargo#10801). They pin `cocoa` 0.26.0 and `unicode-properties` 0.1.3, so the lockfile holds those.
- Part 3: find, go to line, the toolbar and the menus.
  - ⌘F opens the editor's own search. A replacement reaches the core as an edit through the recorder, like typing. The window binds ⌘F for when the focus is outside the result, and the `Find` action serves the toolbar and the menu.
  - ⌘L opens a field above the result. `merge-view::scroll::row_of_line` maps the line to its row, which `result_to_rows` also uses.
  - The window focuses the result when it opens. Until something has the focus, no key reaches the window's bindings.
  - gpui-component's `Button` calls `prevent_default` on mouse down, so a toolbar click does not take the focus.
  - The toolbar labels only the controls that pick (spec §6.3). `toolbar::Labels::for_width` drops every label below `toolbar::labels_min_width`: `LABELS_MIN_WIDTH`, 1052 px, at the default toolbar text, plus the growth of the labels' and the reference badge's text at a larger `toolbar-font-size`. A headless test measures the bar with the longest badge, `Conflict 10 of 12 · 12 unresolved`: labelled, it needs more than 1000 px; without labels, it fits at `COMPACT_MIN_WIDTH`, 788 px. Each tool button adds 28 px: 24 px wide and a 4 px gap. The Both split button is `merge_app::split_menu`, on gpui-base's `Popover`; a row closes the menu, which gives the focus back, then sends its action from there.
  - The app menu stays empty until §A6.4 adds Save & Exit and Cancel.
  - Zed: go to line is a modal over the editor, opened with ctrl-g. It takes `row` or `row:column`, shows the current line and the line count, and Escape restores the scroll position. triad's field takes a line only, sits above the result and shows the line count. Source: Zed's behaviour, studied in use; no code read or copied.
- Part 4: the open dialog (`merge-app::open_dialog`).
  - `merge_cli::load` reads, decodes and aligns the inputs for both `--headless` and the dialog. `merge-app` depends on `merge-cli`, which links no GUI.
  - `WindowSettings::output` carries the output path to the merge window for §A6.4.
  - gpui-kit's `open_window` wraps each view in gpui-base's `Root`, so tests find a window's view through `Root::view`.
  - Zed has no counterpart: it opens files through the platform picker, as the Choose buttons do.
  - The Diff tab shares the left and right fields with the Merge tab. It opens the diff window as `run` opens `diff LEFT RIGHT`: `RunRepository::of_diff_tool`, a `DiffLookup::Tool` for the origins and `run::diff_settings`. `open_dialog::open` takes `watch`; `run` sets it and tests leave it off, as the diff tests do.
  - Layout after Zed's modals (`crates/ui/src/components/modal.rs`, `ToggleButtonGroup`): small text, muted labels, a tab strip on the muted background with the chosen tab raised, small fields, icon-only ghost pickers and a small primary button. The tabs are gpui-base's headless `Tabs` and `Tab`, so they carry the tab-list and tab roles and the selected state. The mark replaces the wordmark logo. Source: Zed's styling, studied; no code copied.
- Part 5: pane titles from file names. `merge_view::status::FileNames` holds each input's last path component; `pane_titles` takes a label, then the file name, then Left, Base and Right. `merge_app::run` and the open dialog fill `Settings::file_names`. Conflict markers keep labels only.
- Tests (headless):
  - typing in a CRLF file inserts `\r\n`;
  - a three-cursor edit undoes in one ⌘Z;
  - a patch after a multi-byte character lands at the right UTF-16 offset;
  - ⌘L to a line inside a merged region;
  - the open dialog fills the slots and starts a merge;
  - its Diff tab fills left and right and starts a diff.

#### A6.3 Scroll sync for every source

A small custom element wraps the three panes. In `prepaint`, which runs on every frame the window draws, it compares the panes' offsets with the previous frame and applies the mapping from §A5.2. This catches wheel, scrollbar and caret scrolling. Tests: headless scroll by wheel on each pane; a caret move off screen in the result pane moves the rows.

Sideways scroll (spec §5). The top panes' text sits in a clipping cell per pane and shifts left by one shared offset, `sideways::Sideways`. The uniform list keeps vertical wheel moves (`restrict_scroll_to_axis`, so a sideways move does not scroll it down); a wheel listener on the rows takes the sideways part when it is larger than the vertical. The offset runs to the widest line a shown pane holds: each input's widest line, in columns with tabs expanded (`merge_view::tabs::widest`), is counted off the UI thread when the window opens, times the advance of `m` in the panes' font, less the pane's text width, which an invisible canvas in the first row on screen records each frame. A second `ScrollSync` with identity maps keeps the rows and the result level sideways; its follower rule lets a pane stop at its own end without pulling the leader back. The diff window does the same for its two editors, deferring the left pane's offset to its next layout as it does for the top. `wrap::Wrap` holds soft wrap: on, off, or too long (`SOFT_WRAP_MAX_LINES`), which no toggle changes.

Wrapped top panes (spec §5). The uniform list holds one item per display row. `row_wrap::RowWrap` gives each aligned row the display rows of its tallest cell: in prepaint, after the rows lay out, it wraps each cell's tab-expanded text with GPUI's `LineWrapper` at the text width the probe recorded, and keeps the break offsets per cell. Every row's text area has that width: the deletion note sits inside it, not beside it, so the width does not change with the row at the top. It wraps again when the widths, the pane set or the row map change; a change of rows with the same count keeps the old heights for one frame, and a break that no longer fits its text leaves the cell whole. A display row draws its segment of the cell's text and highlights (`segment_text`); continuation rows keep the band, filler and strip but no pick target, mark, origin, line number or accessible role. `merge_view::heights::Heights` maps items to display rows and back, a share of an item being the same share of its rows. Scroll sync, reveal, Go to Line and the input search's steps work in aligned rows and result rows and convert through the rows' heights and the result's, which come from the editor's display map (`buffer_line_to_display_row_range`, placeholders one row each) and are built only when a pane moves. When the heights change the rows keep their top aligned row and tell scroll sync, so the result does not follow. Zed's split diff solves the same problem for two editors side by side by giving each display map the other's wrap snapshot and padding with spacer blocks; the result here sits below the panes, so it maps by anchor instead.

- As built:
  - A zero-size `canvas` between the rows and the result runs `MergeWindow::sync_scroll` in its prepaint. It compares both scroll tops with the last sync and copies from the pane that moved, so the pane scrolled last leads and a synced pane does not echo back.
  - The canvas sits before the result in prepaint order because the editor applies a deferred scroll offset in its own prepaint and clears it in paint: an offset set after its prepaint is lost. The rows have drawn by then, so a change to them asks for one more frame.
  - Tests: wheel on the rows and on the result, inside and past a conflict; the pane scrolled last leads; no echo over later frames; a caret moved off screen moves the rows.
  - Switching sync off is a setting, so it comes with §A6.6.
  - Zed: its split diff gives both editors the same rows through spacer blocks, so one scroll position fits both sides. triad's rows and result differ in line count, so it maps by region. Source: Zed's behaviour, studied in use; no code read or copied.

#### A6.4 Save, cancel and exit codes

Spec §6.2: the confirmation sheet; exit codes 0–3; atomic write; the recovery journal, written on each command and debounced by 500 ms (`JOURNAL_DEBOUNCE_MS`). The journal is serde JSON holding:

- the inputs' hashes;
- the `MergeOptions` (algorithm, whitespace, refinement, marker length);
- the command log.

Restore checks the hashes, re-aligns with the recorded options, then replays the log.

Part 1: Save & Exit, Cancel, the exit codes and the binary.
- `merge_cli::save` writes the output for `--headless` and the window: output encoding, atomic write, exit 0 or 1. `merge_view::status::changed` tells Cancel whether to ask.
- ⌘S, ⌘W and ⌘Q, the app menu, and the window's close button. Confirmations and the write-failure alert are platform prompts, which tests answer with `simulate_prompt_answer`.
- A window ends through the `Finish` global. `merge_app::run` sets it to exit the process with the code, since GPUI's quit on macOS (`NSApplication terminate`) exits with 0. Tests set it to record the code.
- `triad-merge` runs window forms through `merge_app::run` on macOS and Linux. The diff forms report that the diff window is not built yet (§A6.5). On Windows, `triad-merge-gui` runs them through `merge_app::run`.
- Part 2: the recovery journal.
  - `merge_core::journal::Journal` holds the format version, SHA-256 of each input's decoded text (`sha2`, already in the tree), the `MergeOptions`, SHA-256 of the starting alignment (`MergeDocument::start`, version 10) and the command log. Restore aligns again and refuses a journal whose alignment hash differs, since its commands name regions and byte offsets of that alignment. The core's command and option types derive serde. The file is `<cache>/journal/<key>.json`, where the key hashes the input hashes.
  - The window records each command that applies. `JOURNAL_DEBOUNCE` (500 ms) restarts with each command; tests drive it with the executor's fake clock.
  - `Settings::journal_dir` turns the journal on; `merge_app::run` sets it from `merge_cli::journal_dir`. Tests that are not about the journal leave it off.

Tests:

- headless save with all resolved: file content, exit 0;
- save with unresolved conflicts: markers, exit 1;
- cancel: no write, exit 2;
- journal restore after a simulated crash;
- restore when the current settings differ from the journal's options gives the journal's regions.

#### A6.5 Two-way diff window

- Two `EditorState` panes, both editable unless read-only.
- Edits go to `DiffDocument`. A recompute runs on a background task after 100 ms (`DIFF_DEBOUNCE_MS`).
- File watching uses `notify`, with the reload and conflict prompts from spec §6.1.
- `jj diff --tool` with the working copy on the right opens the working-copy file on the right and makes the left copy read-only (`merge_session::diff_tool::jj_working_copy`, spec §7.1). `run.rs` checks before the window opens, and opens the repository only for a path in jj's temporary directory; the origin lookup reuses it. Otherwise, when an editable path sits in jj's temporary directory, a banner says edits will be lost.
- Copy-to-other-side buttons sit in the central gutter.
- **Alignment: spacer rows inside the editors, as Zed does.**
  - Zed's split diff (default since v0.224) keeps both sides editable. It aligns them with spacers: visual rows that belong to no buffer line, drawn with a checkerboard, inserted by its editor's block map.
  - gpui-component's editor has no such layer. Its display map has two layers: soft wrap, then folding.
  - Plan: add a `SpacerMap` layer to the vendored `gpui-base` (§A6.1), between `FoldMap` and `DisplayMap`.
    - It is the inverse of `FoldMap`: sorted runs of inserted rows, each with a running count, mapped by binary search.
    - Its API: `set_spacers(Vec<(buffer_line, rows)>)`.
    - Painting draws spacer rows as hatched bands.
    - Cursor movement, selection and hit testing skip them.
  - This is larger than the full-line fill patch: about 400–600 lines plus tests, mostly in `display_map/` and `base/element.rs`.
  - Spacers are recomputed from the two-way regions after each debounced recompute, and set on both editors.
  - Fallback if the patch proves too costly: region-based scroll sync with connecting bands in the central gutter, as Meld does. The decision is made after a two-day time box at the start of this task.
- Part 1: `merge-app::diff_window`.
  - Two editor panes over `DiffDocument`, edits through the core as in the merge window, per-side undo and redo, the 100 ms background recompute, change bands (`merge_view::diff`), the status bar, Save, the close prompt and read-only sides. `merge_app::run` opens it for `diff LEFT RIGHT`.
  - gpui-component's `Editor` element sets the state's read-only flag on each render, so read-only goes on the element.
- Part 2: file watching. `notify` watches each file's directory and sends the side to `DiffWindow::disk_changed`, which tests call directly; `DiffDocument::reload` replaces one side. The watcher itself has no headless test: its events arrive on another thread.
- Part 3: next and previous difference (`merge_view::diff::{current_region, next_change, previous_change}`, `DiffDocument::regions_current`).
  - A jump, or a click on a ruler mark, blinks a 2 px frame around the region in both panes (spec §5). Each pane holds a second decoration collection for the frame, and the editor drops it on a side where the region is empty. `merge_app::blink` holds the step timers both windows share; the merge window's highlight starts hidden and stays, the diff window's frame starts shown and goes. A step that finds nothing blinks the current difference. The vendored editor paints full-line fills over the active line and borders last, since the theme's opaque active line hid both on the caret's row.
- Part 4: the jj banner, from `jj-diff-<random>/{left,right,output}` in either path.
- Part 5: alignment. The vendored editor has a `SpacerMap` layer (`EditorState::set_spacers`, see `vendor/gpui-base/PATCHES.md`). `merge_view::diff::spacers` gives each changed region's shorter side the difference in rows, above the line after the region. The window sets them on opening and after each recompute; between a keystroke and the recompute they stay on their line numbers.
- Part 6: scroll sync. Spacer rows give both panes the same rows, so they share one scroll top. A canvas between the panes copies the top of the one that moved, as in the merge window. An offset for the left pane, which has laid out by then, waits for a canvas before it in the next frame, since its paint drops an offset set after its prepaint.
- Part 7: copy buttons. A 28 px column between the panes holds → and ← on each changed region's first row, the first row of its longer side. A button that would copy into a read-only side is left out. While regions lag the text there are none, since a recompute can renumber the regions. A copy is `DiffDocument::copy_region`, an ordinary edit, so undo on that side reverses it. The column re-renders on every scroll and measures where the left pane's rows start against its own top.
- Alignment needs the spacer-layer time box below. Painting stacks each buffer line's rows from the first visible line's top, and cursor, selection, decoration, gutter and hit-test code all place rows at `display_row × line_height`, so a spacer row must be a display row in `DisplayMap` and line painting must place each line at its own display row.
- Tests (headless):
  - spacer rows sit at the right display rows after a recompute;
  - the caret moving down over a spacer lands on the next buffer line;
  - a click on a spacer puts the caret on the nearest line;
  - both panes stay row-aligned while typing, checked against the region list;
  - the jj banner shows for a path under jj's temporary directory and not elsewhere.

#### A6.6 Accessibility and settings

- Roles and labels on every control.
- Settings (spec §6.4); §E3 replaces the TOML file with the config file. A change to the algorithm, whitespace mode or refinement re-aligns the open merge only while every region still has its automatic resolution. Otherwise it applies from the next session, and the status bar says so.
- Scroll sync on or off (spec §5), read by `MergeWindow::sync_scroll`.
- Tests:
  - load, defaults and invalid values;
  - a whitespace change before any pick re-aligns;
  - a whitespace change after a pick leaves regions and resolutions unchanged and sets the status message.
- Part 1: `merge_cli::settings` reads the file (spec §6.4) with `toml`, already in the tree. `args::parse_with` puts the flags over it, and `run` reports a bad file on stderr and goes on with the defaults. The binary tests point `TRIAD_CONFIG` (then `TRIAD_SETTINGS`) at a missing file, so the user's settings never reach them.
- Part 2: `Outcome::Window` carries the settings to `merge_app::run`. Both windows take `Typography` (font and size; rows and lines follow `Typography::line_height`) and `scroll_sync`. The merge window opens with the base pane when `show-base-pane` is set, and the open dialog aligns with the file's diff options.
- Part 3: tab width. `Typography::tab_width` sets each editor's `TabSize`, so the Tab key types that many spaces. `gpui-base` moves the glyphs after each tab to the next stop; triad's patch keeps a tab a rounding error short of a stop from ending as a sliver (`vendor/gpui-base/PATCHES.md`). The merge window's side rows expand tabs to spaces with `merge_view::tabs`.
- Part 4: alignment changes during a session. `MergeWindow::set_options` re-aligns while `merge_view::status::changed` is false: a new `MergeDocument` from the same inputs, new rows, a fresh journal in place of the old one. Otherwise it keeps the document and adds `strings::NEXT_SESSION` to the status bar until the options match the session's again. Since §E3.4 the config file is read once at start-up; tests call `set_options` directly.
- Part 5: roles and labels, as spec §6.5 lists them, on the merge window, the diff window and the open dialog. GPUI builds an AccessKit tree from each element's role and label. The test platform has no AccessKit adapter, so tests read the role, label and toggled state that gpui-kit records for `test_support()` elements (`tests/a11y.rs`, and the `named` tests in `tests/diff.rs` and `tests/open_dialog.rs`).
  - Gaps, recorded in `docs/gpui-kit-issues.md`: no live announcements of status changes; region bands and gutter marks are not exposed; a screen reader sees only the input rows on screen.
  - Not yet checked with VoiceOver on a running app. The M3 manual checklist (§A6.7) gets a VoiceOver pass: every control reachable and named, a pick toggled from VoiceOver, the result's text read.

#### A6.7 Performance gate

- `SPIKE_AUTO`-style frame measurement moves into `merge-app` behind a `TRIAD_MEASURE` environment variable.
- Find the cause of the 195 ms stall near the start of 1M-line runs (spike follow-up 5). Fix it, or show that it falls outside spec §9's scrolling target.
- A manual checklist run on the macOS reference machine confirms spec §9 for 100k and 1M lines before M3 closes.
- Performance review before release 1. Measure, then decide what more work is needed:
  - Peak memory of load, merge and save at 100k and 1M lines, against spec §9. Add the measurement to the `load` bench.
  - Profile load, scroll, a keystroke and a pick in the running app, on the reference machines.
  - Known whole-text copies, left until a measurement calls for them. Each costs a few milliseconds and one file's size of memory at 1M lines:
    - Fixed: `Text::decode` decodes UTF-8 in the buffer read from the file, and a `Text` clone shares its content and line index.
    - Fixed: `MergeDocument::new` renders each region to a `Cow<str>` into a `RopeBuilder`. Unchanged, one-side and single-pick regions borrow their input. Load's heap peak at 1M lines is 270 MB (414 MB with the joined copy); at 100k, 26 MB (40 MB).
    - `Recompute::run` copies both diff texts twice for every recompute.
    - `MergeDocument::output` copies the rope into a `String` to save it; saving is now the session's heap peak, 362 MB at 1M lines.
    - `Text` keeps an 8-byte line start for every line; `u32` offsets would do for files under 4 GiB.
  - Record the results here and in spec §9, with the fixes made.
- Part 1: measurement, the stall, start-up and memory. Measured on an M4 Pro MacBook Pro with a 120 Hz display, release build, journal on, triples from `merge_core::testing::generate` with seed `0x5EED`.
  - `merge_app::measure`: `TRIAD_MEASURE=open|scroll-rows|scroll-result|type|pick` makes the merge window act once a frame for 300 frames (`open`: one), then report frame intervals and each action's time on stderr, and finish with 2 without writing. `run` reports load and merge, and the first frame, from its own start. `examples/generate` in `merge-core` writes a generated triple to disk.
  - The stall was the first frame's own layout. Interval 0 runs from the first `render` to the second, so it holds the editor's line layout: `TextWrapper` looked each of the 1M lines up in the rope, twice. The vendored editor now reads the lines in one pass and skips the second (`vendor/gpui-base/PATCHES.md`). Interval 0 at 1M fell from 212 ms (scrolling) and 416 ms (typing, picking) to 69–74 ms. Interval 1 stays at 45–53 ms at both sizes: the main thread is idle for about half of it, and the rest fills the glyph atlas and builds Metal pipelines. Both come once, as the window appears, before any input; the scrolling target holds after them.
  - The journal hashed every input twice on the main thread, about 700 ms at 1M. `Journal::new` now hashes each input once, on its own thread, and `Journal::key` reads those hashes.
  - `cargo bench -p merge-cli --bench memory` measures the peak heap of `merge_cli::load` and `save` through a counting allocator (`cap`); `script/check-bench` holds the session's peak below spec §9's memory row, in git and in jj mode.

  | Lines | Load and merge | First frame | Scroll p95 | Keystroke work, mean / max | Pick work, mean / max | Peak RSS | Core heap peak |
  |---|---|---|---|---|---|---|---|
  | 100k | 30 ms | 200 ms | 8.4–9.1 ms | 0.2 / 0.4 ms | 0.2 / 0.6 ms | 198 MB | 40 MB |
  | 1M | 510 ms | 860 ms | 8.4 ms | 2.4 / 3.3 ms | 2.3 / 3.6 ms | 838 MB | 415 MB |

  Before the fixes, the first frame took 261–301 ms at 100k and 1.54–1.56 s at 1M. Every spec §9 target is met on this machine.
  - Decisions from the measurements. The known whole-text copies stay: load, memory and keystrokes are within target. `MergeWindow::refresh` rebuilds every region decoration on each command, 56% of the main thread while typing at 1M; it stays within the keystroke target, so incremental decorations wait. Alignment is 368 of load's 449 ms at 1M and grows faster than linear (17 ms at 100k); it is the first place to look if load nears 1 s. The VoiceOver whole-text copy (§A6.1) was not measured: it runs only with an accessibility client.
- Part 2: tools for the manual checks, in `docs/release-checklist.md`.
  - `script/measure-app` runs every `TRIAD_MEASURE` mode at 100k and 1M lines and fails on a missed spec §9 target. On the M4 Pro all targets are met when the window stays frontmost. A window left in the background gets frame intervals of 41.7 ms and keystroke work three to four times slower, from macOS, not from triad: the same binary meets the targets on the next run.
  - `script/ax-tree.swift` prints the accessibility tree that VoiceOver reads and presses named elements. On the M4 Pro every role and label in spec §6.5 is present in the merge window, the diff window and the open dialog. Pressing a pick toggle sets its value and resolves the conflict. The status bars are `AXApplicationStatus`; the jj banner and the dialog's error are `AXApplicationAlert`. GPUI builds the tree on the first query from an assistive client, so the first query can return an empty window.
  - Checklist run of `script/measure-app 60` on the M4 Pro MacBook Pro (2025), window frontmost. Every target met:

    | Lines | Load and merge | First frame | Scroll p95, rows / result | Keystroke max | Pick max | Peak RSS |
    |---|---|---|---|---|---|---|
    | 100k | 28–40 ms | 186–212 ms | 8.4 / 8.4 ms | 0.4 ms | 0.4 ms | 190 MB |
    | 1M | 494–501 ms | 825–840 ms | 8.4 / 8.4 ms | 3.3 ms | 3.3 ms | 799 MB |

  - Run of `script/measure-app 120` on the M4 Pro MacBook Pro (2025), macOS 27.2, window frontmost, 2026-11-08, after the ruler fix of §E4. Load and first frame span the six modes; peak RSS is the largest:

    | Lines | Load and merge | First frame | Scroll p95, rows / result | Keystroke work, mean / max | Pick work, mean / max | Peak RSS |
    |---|---|---|---|---|---|---|
    | 100k | 45–48 ms | 211–238 ms | 8.4 / 8.4 ms | 0.5 / 1.0 ms | 0.5 / 0.8 ms | 218 MB |
    | 1M | 511–517 ms | 718–730 ms | 8.4 / 8.4 ms | 7.3 / 8.7 ms | 8.0 / 9.2 ms | 870 MB |

    The keystroke at 1M lines misses its 8 ms target; three runs of the mode alone gave maxima of 8.2–8.7 ms. Keystroke and pick work at 1M grew from 2.4 and 2.3 ms mean since the run above. A profile puts the work in `MergeWindow::refresh`, which rebuilds per-region state on each command: the editor's decoration normalisation, which clips every range against the rope, about 37%; `ruler::marks`, two rope line lookups per region, about 30%; `marks::bands` 13%; `status` through `MergeDocument::output_endings` 9%; `placeholders` 9%. Each pass is linear in the regions. The incremental refresh below fixes it.
  - Incremental refresh, 2026-11-08. `MergeDocument` keeps each region's newline count in a second Fenwick tree (`region_lines`), so `ruler::marks` reads a region's lines in O(log n) of the regions without a lookup in the text. The vendored editor moves its decorations with each edit; `refresh` compares the new bands with the moved ranges (`marks::bands_unchanged`) and rebuilds the collection only when a range or a look differs. A key typed into a region whose look stays the same rebuilds nothing; the first key into a region changes its look to edited and rebuilds. `script/measure-app 120`, same machine, every target met:

    | Lines | Load and merge | First frame | Scroll p95, rows / result | Keystroke work, mean / max | Pick work, mean / max | Peak RSS |
    |---|---|---|---|---|---|---|
    | 100k | 46–50 ms | 210–223 ms | 8.4 / 8.4 ms | 0.3 / 0.5 ms | 0.4 / 0.6 ms | 221 MB |
    | 1M | 521–548 ms | 727–754 ms | 8.4 / 8.4 ms | 1.9 / 5.2 ms | 4.4 / 6.3 ms | 869 MB |

    The 100k `open` mode, the first launch after the relink, took 350 ms to its first frame; three runs of the mode alone took 214–218 ms. The keystroke maximum at 1M is the first key, which rebuilds. A pick changes a look every time and rebuilds; bands, placeholders and the status counts still scan every region on each command.
  - The benchmarks on the same machine and day, `script/check-bench`: every gate met.

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

    `load-deep-rust-bounded` is within 10% of the load target at both sizes.
  - Still open: the spoken VoiceOver steps in the checklist.
- Part 3: idle memory, 2026-11-07. The demo diff of `fixtures/demo/code_a.py` and `code_b.py` took 213 MB of physical footprint (`footprint`), against 121 MB for p4merge. The files and the editors hold about 15 MB of it; a diff of two one-line files took 198 MB. The rest was GPU memory: the Metal driver's 4 MB blocks, 116 MB in all, two 18.5 MB drawables and the window-sized path texture. The driver marks its blocks reclaimable once the window stops drawing, but the caret's blink drew every 500 ms. A bare GPUI window took 52 MB idle and 185 MB redrawing every 500 ms; Zed blinks forever and pays the same.
  - The vendored editor's caret rests, shown, after 10 s without a key, click or focus (`vendor/gpui-base/PATCHES.md`). At rest the demo diff takes 98 MB: 37 MB of drawables, 33 MB of heap and the binary's code and data.
  - GPUI already counts a window that loses the front as a blur, which stops the blink; a test pins it.
  - Left in GPUI: the path texture, made for every window whether or not it draws paths, and the drawables, which grow with the window.

**M3 done when:** spec §5–6 work on macOS, the headless GUI tests pass, and the §A9 targets are met.

### A7 Phase M4 — merge and diff tool integration, macOS release

#### A7.1 `vcs` crate and native diff mode

```rust
pub enum RepoKind { Git, Jj }
pub struct Term { label: Option<String>, change: Option<ChangeInfo> }  // what the VCS knows about one side or base
pub struct ChangeInfo { commit: String, change_id: Option<String>, summary: Option<String> }  // change_id: jj only
pub trait Repository {
    fn kind(&self) -> RepoKind;
    fn file_at(&self, path: &Utf8Path, rev: &RevSpec) -> Result<Option<Vec<u8>>, VcsError>;
    fn default_parent(&self, path: &Utf8Path) -> Result<Parent, VcsError>;
    // M5/M6 add: conflicts(), read_conflict() -> n sides, n - 1 bases and a Term each,
    // write_resolution(), annotate()
}
pub struct Parent { rev: RevSpec, term: Term }   // git: index or HEAD; jj: @-
pub fn discover(start: &Utf8Path) -> Result<Box<dyn Repository>, VcsError>;  // jj wins when .jj and .git share a directory
```

- Add gix and jj-lib here, on the gix version jj-lib uses (see the decision below). `script/check-workspace` confirms one gix version.
- `vcs-git`: `gix::discover`. Reads `HEAD` and the index. `default_parent` is the index entry if the file has staged changes, otherwise `HEAD`. The `Term` label is `index` or the branch name.
- `vcs-jj`: loads the workspace and repo at the head operation, finds the working-copy commit, and reads `@-`'s tree. The `Term` carries `@-`'s change ID and description. jj-lib's async tree APIs run through `pollster::block_on`.
- The left pane title in native mode comes from the `Term`, on both backends.
- Tests (spec §8.8): repositories built in temp directories through gix and jj-lib only. One conformance suite runs on git, jj and internal jj repos. It covers discovery, `file_at` on a missing path, a binary file, a renamed file, and the parent's `Term`.
- Part 1: the shared types in `vcs`. Paths are `camino::Utf8Path`, relative to the repository root. `RevSpec` is `Index` or a revision expression in the backend's syntax. `Term::title` gives the pane title: the label; else the change ID (jj) or commit (git), then the summary; none when the term is empty. `VcsError::Backend` boxes gix and jj-lib errors with a context, so `vcs` names neither. `discover` needs both backends and moves to a crate above them, which `merge-cli` already is.
- Part 2: `vcs-git` on gix 0.90, with the `revision` and `sha1` features only. `GitRepository::discover` searches up from a directory; gix's not-found becomes `NotARepository`. `file_at` reads a blob from a revision's tree or from the index; a missing path or a directory is `None`, and an unresolved revision is `UnknownRevision`. `default_parent` is the index, labelled `index`, when the file's index entry differs from `HEAD`'s, counting an added or removed entry, and before the first commit; otherwise `HEAD`, labelled with the short branch name (none when detached) and carrying the short commit ID and summary. Tests build repositories with gix in temporary directories (`tests/git.rs`).
- Part 3: `diff PATH` inputs, without the window. `merge_cli::native::load` canonicalises PATH, finds its repository with `native::discover` (git only until `vcs-jj`), and reads the file at `--rev` or at the default parent. A file absent there gives an empty left side. The left title is the `Term`'s title, or the revision as typed. `Repository::root` gives the working copy's root; a bare repository is `NoWorkingCopy`. `vcs-git`'s `testing` feature shares the gix-built fixture with `merge-cli`'s tests.
- Part 4: `diff PATH` opens the diff window. `diff_window::Origin` is a file or a revision; `DiffFile::revision` makes a read-only side titled from `left_title`, which is never saved, watched or reloaded. `Invocation::DiffNative` carries `--diff-algorithm`, `--ignore-whitespace` and `--title` as `diff LEFT RIGHT` does. Load errors exit with 3. In a jj repository, git sees jj's `@-` as a detached `HEAD`, so the left title is its commit and summary until `vcs-jj` takes over.
- Part 5: `vcs-jj` on jj-lib 0.46, and the workspace on gix 0.88. `JjRepository::discover` searches up for `.jj`, loads the workspace with jj-lib's default settings, and reads the repository at its single head operation. More than one head is `JjError::ConcurrentOperations` inside `VcsError::Backend`: reconciling them writes an operation, and `vcs-jj` never writes. `file_at` evaluates the expression as a revset with jj-lib's built-in functions and no user aliases; several commits read from the merge of their trees, so `@-` on a merge working copy matches `jj diff`. A conflicted path is `VcsError::Conflicted`; an ambiguous prefix, a divergent change or a conflicted bookmark is `VcsError::AmbiguousRevision`; a symlink, submodule or directory is `None`. `default_parent` is `@-`, its `Term` the change ID and commit at jj's display length (at least 8, longer when a prefix is ambiguous) and the first line of the description; a merge working copy gives an empty `Term`. `native::discover` canonicalises the start, opens both backends and keeps the nearest root, jj on a tie. The conformance suite (`merge-cli/tests/conformance.rs`) runs on git, jj and internal jj repositories built through gix and jj-lib. `vcs-jj`'s `testing` feature makes workspaces in either layout with fixed settings and seed, so IDs repeat. 2026-10-27: no jj-lib release newer than 0.46 exists; 0.46 still needs gix ^0.88.
- Decision (2026-10-26): jj-lib 0.46 now, with the whole workspace on gix 0.88, so the build holds one gix. jj-lib 0.46 requires gix 0.88, which GHSA-q54r-jf2p-rfr2 affects (`>= 0.88.0, <= 0.89.0`; fixed in 0.90.0). The flaw skips the trust filter on `core.protectHFS`, `core.protectNTFS` and `gitoxide.core.protectWindows`, which guard checkout, index creation, merge and tree editing. Release 1 only reads from repositories, so triad never runs those paths.
  - Move `vcs-git` from gix 0.90 to 0.88 when `vcs-jj` adds jj-lib. Pin both with `=` so neither drifts alone.
  - Record the exception in `deny.toml` and `.cargo/audit.toml` once RustSec lists the advisory, citing this decision. On 2026-10-27 neither `cargo deny` nor `cargo audit` reports it, and an ignore for an unknown ID only draws a warning. Dependabot will raise it on the first push of the gix 0.88 lockfile; dismiss it citing this decision.
  - Before any code that writes to a repository (M5/M6 `write_resolution`, or any checkout or index write), move to a jj-lib release on gix 0.90 or newer and drop the exception. Check for one at the start of each M4–M6 session.
  - `vcs-git` stays read-only until then; a test or review that finds a gix write path in release 1 reopens this decision.
  - Closed 2026-11-05: jj-lib and jj-core 0.47.1 require gix ^0.90, and the workspace moves to gix 0.90. The exception is gone; §D5.5 may write. The port follows jj-lib 0.47's API: `DefaultWorkspaceLoaderFactory` loads a workspace, a revset's clock is a `jiff::Zoned`, annotation lines carry the `Commit`, and a tree's conflicts arrive as a stream.

#### A7.2 `triad-merge install`

Writes the spec §7.1 entries through `gix::config` and jj-lib's config layer. Before writing the jj entries, check jj's merge-tool variables for the pinned jj version. If jj passes side labels to tools, add them to `merge-args` and update spec §7.1. Tests: install into an empty config, into an existing config with another tool (kept), and twice (idempotent); Windows path quoting is checked by unit test on every OS.

- Checked 2026-10-27: jj 0.46's merge tools get `$base`, `$left`, `$right`, `$output`, `$marker_length` and `$path`, and no side labels (`cli/src/merge_tools/external.rs`). Spec §7.1's `merge-args` stand.
- Part 1: `install --git [--config FILE]`, in `merge_cli::install`. gix's config `File` sets the five keys and serialises the file, which `write_atomic` writes only when it changed. `Env` carries `HOME`, `XDG_CONFIG_HOME`, `GIT_CONFIG_GLOBAL` and `JJ_CONFIG`, so path selection is tested without touching the real environment. Tests read the result back through gix (`tests/install.rs`).
- Part 2: `install --jj`. jj-lib's `ConfigLayer` parses the file, sets the eight keys and serialises it through `toml_edit`, keeping comments and other tables. Spec §7.1 gave `diff-args = ["$left", "$right"]`, which runs a merge with two files; it is now `["diff", "$left", "$right"]`. `ui.merge-editor = "triad"` matches git's `merge.tool`. A Windows path round-trips through TOML on every OS (`tests/install.rs`).

#### A7.3 End-to-end checks

A test harness builds real conflicts in temp repos with gix and jj-lib, then runs the merge tool through each VCS's merge-tool code path as a library where possible. jj-lib exposes no merge-tool runner, so `jj resolve` and `jj diff --tool` are checked by a manual checklist in `docs/release-checklist.md`, as are `git mergetool` and `git difftool`. The checklist has the same steps for git and jj.

- Part 1: `triad-merge/tests/tool_integration.rs`. Neither gix nor jj-lib runs merge tools, so the harness takes each VCS's half from its source:
  - git: the installed `cmd` runs under `sh -c` with `BASE`, `LOCAL`, `REMOTE` and `MERGED` set, as `git mergetool` and `git difftool` run it, with paths that hold spaces and quotes. A stub program records its arguments, and triad's parser reads them back as the merge or diff.
  - jj: `vcs_jj::testing::Fixture::resolve` follows `jj resolve` in jj-cli 0.46 (`cli/src/merge_tools/external.rs`) through jj-lib: a conflict from two commits over a base, jj's input files and pre-filled git-style output, `choose_materialized_conflict_marker_len`, jj's `$name` interpolation, its exit-code rules, and `update_from_content` on the output. The real binary runs with the installed `merge-args` and `--headless` in place of the window. An unresolved conflict stays the same conflict in jj, clean regions beside it agree with jj's, and an error exit is an abort. `diff-args` and `merge-args` parse as the diff and the titled merge.
  - Found: jj makes markers four longer than the longest marker-like line, counting `+ - % \` in every style; triad's own rule adds one and counts `< | = >`. The tests show `$marker_length` carries jj's length to triad; spec §4.4 now says so.
- Part 2: `docs/release-checklist.md`, "Merge and diff tool integration": one table of ten steps, each with the git and the jj result, run against scratch repositories and scratch configs (`GIT_CONFIG_GLOBAL`, `JJ_CONFIG`). It covers install, cancel, save with conflicts, save resolved, the diff tool, native `diff PATH` in git and jj repositories, and a path with a space. Record each run here.
- Run, 2026-11-05: macOS 27.2 on the M4 Pro MacBook Pro, git 2.56.0, jj 0.47.1, the binary from `cargo install`. Windows were driven by keystrokes through System Events and read through the accessibility tree (`script/ax-tree.swift`); headings were checked from screenshots.

  | # | git | jj |
  |---|---|---|
  | 1–2 | Passed | Passed |
  | 3 | Passed | Passed |
  | 4 | Failed: triad writes diff3 markers and exits 1, and `git mergetool` then restores `a.txt` from its backup (`mv "$BACKUP" "$MERGED"` in `git-mergetool`). triad's output is lost | Passed |
  | 5 | Passed | Passed |
  | 6 | Passed | Passed |
  | 7 | Passed: `main`, then `index` once staged | Passed: `@-` with the change ID in the chip |
  | 8 | — | Passed |
  | 9 | Passed | Passed |

  - The checklist expected wrong results in four steps, now corrected: git step 4 (git's own markers return), step 3 (the base pane is hidden until ⌘B), step 5 (⌘1 acts on the current region, and the merge window opens at the top, so the step goes to the conflict first) and jj steps 7–8 (the ID sits in the revision chip, the description in its hover card). Spec §6.2's exit-code table now says what git does with exit 1.
  - Found: under git, Save with Conflicts discards every region the user resolved, and Save & Exit deletes the recovery journal, so nothing is left. The confirmation says only that git "sees the merge as unresolved".
  - Found: the merge window opens with the caret on the first region, not the first conflict, so ⌘1–⌘5 do nothing until the user navigates. The diff window opens on the first difference.
  - Fixed, 2026-11-05. A save that leaves conflicts writes the journal at once and keeps it; the next `git mergetool` offers the restore, which puts the caret on the first unresolved conflict. Under git semantics the confirmation says so. The merge window opens on the first unresolved conflict, else the first difference; a measured run still opens at the start, so its numbers compare with the tables in §A6.7. Five tests that assumed the window opened at the top now put the caret or the scroll there first, with their assertions unchanged; the syntax test's fixture gains a first conflict near the top, so its far chunk stays off screen. Git step 4 rerun by hand: git puts back its file, the next run offers the restore, and the restored session carries the edit.
  - Found: on an unresolved conflict each side's pick toggle reports value 1 to accessibility clients, the same as a picked side, so VoiceOver speaks both as selected.
  - Fixed, 2026-11-06. AccessKit's macOS adapter reports every toggled state but off as 1 (`docs/gpui-kit-issues.md`). The targets of an unresolved conflict report off, and their labels end ", unresolved".

#### A7.4 Local install on macOS

Release 1 is for local use: no Developer ID signing, notarization or Homebrew cask (owner's decision, 2026-10-27). `script/macos-app` builds `triad.app` and signs it ad hoc; the bundle gives the Dock its icon. README § Build and install gives the commands.

`install` writes the absolute path of the binary it runs from, so the entries survive `cargo clean`. Rerun both after moving the binary. A binary from `cargo install --path crates/triad-merge --locked` works the same, without the Dock icon.

**M4 done when:** the release checklist passes on macOS for git and jj, with the locally installed binary.

### A8 Phase M4b — Linux and Windows

- Linux: AppImage and `.tar.gz`. Check rendering on real GNOME Wayland and X11 desktops (the spike only checked the build and headless tests).
- Windows: the two binaries from spec §6.1 (`triad-merge.exe` console, `triad-merge-gui.exe` GUI) with the forwarding from §A4.13, an MSI via `cargo-wix`, Authenticode signing, and a winget manifest. Check on x64 and ARM64 hardware.
- Run the release checklist on each target, and measure spec §9 on each reference machine.

### A9 Phases M5–M7

Each starts with its own short plan, written when the previous phase closes. M5 and M6 deliver git and jj together and pass the conformance suite on both.

- **M5, chunk origin:** `annotate()` on both backends, filling one `LineOrigin` type. jj adds change IDs; git leaves them empty. Hover cards and a gutter column. Part C holds the plan.
- **M6, native resolution:** part D holds the plan. `read_conflict()` and `write_resolution()`, with git index stages and jj trees. Pane titles from jj's per-term conflict labels (`ConflictLabels` in jj-lib 0.46) and from git's `HEAD`, `MERGE_HEAD` or rebase state. N-sided layout. `triad-merge resolve`. The conformance suite for `conflicts()` and `read_conflict()` takes its case list from libgit2's tree-merge tests (`tests/libgit2/merge/trees/`: `trivial.c`, `modeconflict.c`, `renames.c`, `recursive.c`), rewritten from scratch as repositories built through gix and jj-lib (git conflicts come from a tree merge by `gix-merge`, through gix's `merge` feature, in `vcs-git`'s dev-dependencies), with expected values from git and jj (licence as in §A4.8). Cases: add/add, modify/delete both ways, mode conflicts (each term's executable bit; a symlink or submodule against a file), rename/modify and rename/rename, a binary conflict, and criss-cross merges with more than one merge base.
- **Merge semantics:** jj's merge in jj repositories, word-level merging and opt-in refinements. Part B holds the plan. M6 picks the mode from the backend it resolves through.
- **M7, syntax highlighting:** part F holds the plan. tree-sitter through gpui-component's highlighter, with the edit recorder from §A6.2 moved to a gpui-base edit observer. Syntax-aware merge boundaries (§B5.7, item 5) follow M7 and share its tree-sitter and grammars.

### A11 Decisions

Decided by the owner, 2026-10-25:

1. Two-way alignment (§A6.5): spacer rows in editable panes through a `SpacerMap` patch. Meld-style bands are the fallback after the two-day time box.
2. The `triad-merge` binary crate (§A2), now in spec §3.
3. Vendoring `gpui-base` with the full-line-fill, gutter-mark and spacer patches (§A6.1).
4. The licence is MIT. Spec §13 records the former open questions.

Decided by the owner, 2026-10-27:

5. The licence stays MIT. libgit2's tests are GPL-2.0-only and Zed's crates GPL-3.0-or-later, so no single GPL licence admits both. libgit2's merge tests give a case list only (§A4.8, §A9 M6).
6. Rejected crates. `similar`: overlaps `imara-diff` without histogram or the indent heuristic. `difftastic`: a binary with no stable library API, whose syntax-node output does not map to line regions; revisit after M7 for syntax-aware intra-line highlighting. `difflib`: nothing over `imara-diff`. `gix-merge` as engine or oracle: it returns merged bytes and a conflict flag, with no regions or n-sided model, and its histogram is `imara-diff`'s, not git's. It is used only to build M6 test fixtures.

Decided by the owner, 2026-10-27:

7. A repository with `.jj` is a jj repository and merges with jj's semantics. triad does not support git merges in a jj repository (§B3).

The decisions pending for merge semantics are in §B7.

### A13 Code-quality work

A review against the Rust development skill on 2026-10-30 found these. The restriction lints and `MergeDocument::output`'s `Output` enum are done.

#### A13.1 Index newtypes

Regions, rows, result lines, byte offsets and input numbers are all bare `usize`. `merge-view::scroll` turns rows into regions into lines within a few lines; `MergeWindow::pinned` is an `Option<(usize, usize)>` of a region and a caret. A swapped argument compiles.

- Newtypes in `merge-core`: `RegionIx`, `Line`, `ByteOffset`; in `merge-view`: `RowIx`. Each derives `Copy, Eq, Ord, Hash, Debug` and offers only the arithmetic its callers need. `Source::Side` and `Source::Base` keep their `usize`; sides are few and always named.
- `vcs`: `CommitId` for the hex IDs `Repository::commits` and `merge_base` return, in place of `String`.
- `MergeWindow::pinned` becomes a named struct.
- Tests: no behaviour changes, so the existing suite is the check. Add `compile_fail` doctests (with error codes) that pin one swap per newtype, such as a `Line` passed where a `RowIx` is expected, and add `cargo test --doc --workspace` to `script/checks` and CI with them (AGENTS.md).

Each commit carries one type through every crate, since a `merge-core` signature change breaks its callers and each commit must build. `rows::Row` is a struct, so the row newtype is `RowIx`.

Done:

- **`RegionIx`** (`merge_core::index`), serialised as its number, so journals keep their form. `MergeDocument::region`, `resolution` and `region_ixs` read by it. `RegionLengths` and `MergeDocument`'s private helpers stay on `usize`.
- **`RowIx`** (`merge_view::rows`). `RowMap`, `Highlights`, the scroll mapping and the merge window's row API take it; `row_count` and `rows_in` stay counts.
- **`ByteOffset`, `Line`** (`merge_core::index`) at the document boundary: `region_at_byte`, `region_start`, `line_of_byte`, `byte_of_line`, and the view functions built on them. `region_bytes` and `blocks` stay `Range<usize>`, since `Range<T>` is not iterable on stable and callers slice text with it. `line_count` stays a count.
- **`CommitId`** (`vcs`) for `Repository::commits`, `merge_base` and `origin::Source::commit`. Its hex also serves as a revision expression, through `as_str`. `Change::commit` stays a `String`: the backend shortens it.
- **`Pin`**: `MergeWindow::pinned` is a named struct, as in the diff window.
- **Doctests**: each newtype has a block that compiles and a `compile_fail` twin that swaps one argument. Stable rustdoc ignores the error code on `compile_fail`: a block tagged `E0599` that fails with E0308 passes. The compiling twin therefore shows that the failure comes from the swap. `script/checks` without `--quick` and CI run them; the commit gate does not, to stay under 30 s.

#### A13.2 Fallible lookups at the view boundary

`MergeDocument::region_at_byte` asserts, `blocks` and `region_bytes` index directly, and `RowMap::region_of_row` panics. Each documents its panic, as slice indexing does. Their callers in `merge-view` (`nav::current_region`, `scroll`, `origin`, `highlight`, `marks`) and `merge-app` (`window.rs`, `diff_window.rs`) pass caret, scroll and row positions held across commands. A position kept past an undo, a realignment or an edit that shrinks the text would be out of range and abort the GUI. The recovery journal would keep the work; the session would still be lost.

Not reproduced, 2026-10-31. Each stored position is dropped or recomputed before use:

- The merge window's regions are fixed for a document. A realignment replaces the document and drops the pin; undo, redo and edits drop it too. Picks, takes and navigation send waiting edits before they read the caret, so the caret never runs past the core's text.
- The diff window's regions lag its text until the recompute. Band ranges clamp their lines to the text, copy buttons hide and `copy_region` refuses while regions lag, and navigation recomputes first. An edit drops the pin.
- Scroll mapping holds rows and lines to their counts.

Tests: `realign::a_region_pinned_past_the_new_regions_is_dropped_by_a_realignment`, `diff::regions_left_past_the_end_by_a_deletion_are_harmless_until_the_recompute` and `diff::navigation_from_a_pin_past_the_end_after_an_undo`. Each fails when its guard is removed. The work found that diff navigation moved the focus to the right pane, so typing and undo went to the wrong file; fixed. What remains below is defensive and does not block a beta.

Done:

- `MergeDocument::region_at` and `RowMap::get_row` return `Option`. `region_at_byte`, `region_bytes`, `region_of_row` and `row` keep their panics for indices the same call builds.
- `nav::held_region` and `diff::held_region` are the one place a stored pin and caret become a region. Each always returns a region of the document: a pin past the last region falls back to the caret, and a caret past the text counts as the end. `nav::Pin` and `diff::Pin` moved to `merge-view` so unit tests can hold stale values the windows now drop first.
- `conflict_number` gives `None` past the last region. `cell_highlights` gives `None` for a row or pane past the end, as `origin_cell` does.
- `go_to` keeps its clamp: the ruler's marks can lag a realignment until the next paint.

Clippy's `indexing_slicing` over `merge-view` and `merge-app` gave 64 findings, reviewed once and not enabled. None but the sites above takes a position stored across a command. The rest index by `Side`, a fixed array, a range the same call computes, or a `Source` that `RowMap::new` checked. `chunks` keeps `ChunkId`s across commands and looks each one up, and `focus` is only compared.

#### A13.3 Split `MergeWindow`

`merge-app/src/window.rs` is 2,846 lines; `MergeWindow` holds 38 fields. Its state includes loose flags such as `scroll_sync`. `diff_window.rs` (1,702 lines) follows the same shape.

- Move each concern with its own state into a struct of its own, without GPUI where possible: go-to-line, measurement (`TRIAD_MEASURE`), recovery, scroll sync, origins. `MergeWindow` keeps the document, the panes and the wiring.
- Replace flag pairs that encode one state with an enum, such as `measuring` and `measured` if they cannot both be `Some`.
- Moved logic without GPUI gets unit tests in its module; logic that stays in GPUI keeps its `merge-app/tests` coverage. Each extraction is one commit with the suite passing.
- Then the same for `DiffWindow`, reusing the extracted parts where the two windows share them.

Done:

- **Recovery** (`merge_session::recovery` since §A13.8). `Recovery` owns the journal's name, commands, file, writes and restore offer; its logic has unit tests there. `MergeWindow` keeps the task spawning and the prompts.
- **Go-to-line** (`go_to_line.rs`). `Field` owns the text field, whether it shows, and its rendering.
- **Measurement** (`measure.rs`). `Measurement` is `Off`, `Running(Run)` or `Done(Report)`. `nav::middle_difference` in `merge-view` picks the region the type and pick modes act on.
- **Origins** (`origin_column.rs`, §A13.13 item 6), shared by both windows.
- **Input search** (`input_find.rs`, §A13.13 item 7) and **scroll** (`pane_scroll.rs`, item 8).
- **Scroll sync** (`merge_view::scroll::ScrollSync`). `step` takes both tops and gives the move that keeps the panes together; its logic has unit tests there. `PaneScroll` holds no syncs while sync is off; View → Scroll Sync sets them to `ScrollSync::resync`, whose first step lets the result lead. `DiffWindow` keeps its own sync, always on: both panes share one top, and the left pane takes a move a frame late.

Review of both windows, 2026-11-09. The split is maintainability work; no feature depends on it, and key bindings (§E9) do not wait for it.

`MergeWindow` (`window.rs`: 3,463 lines, 42 fields, 165 methods) holds the session, the panes, the rows and the components below, and wires them to GPUI.

- The logic with its own state sits in components with unit tests: `Session` (save, journal, document), `PaneScroll` (scroll, wrap, sync, reveal), `OriginColumn`, `input_find::Found`, `go_to_line::Field`, `Measurement`, `JournalTasks`.
- Search and the result's decorations keep their wiring on the window; their logic is in `input_find`, `blink`, `marks`, `status`, `ruler` and `nav` (§A13.13 item 9).
- Row rendering is about 500 lines of GPUI elements. Moving it to a module shortens the file and tests nothing new.
- What stays is the window's job: action handlers, task spawns, editor updates through `cx`, rendering. Its size follows from the number of features, not from tangled state.

`DiffWindow` (`diff_window.rs`: 2,088 lines, 30 fields, 103 methods) has the same shape, smaller.

- It shares `OriginColumn`, `editor_sync`, `blink`, `ruler`, `settings_banner` and `Wrap` with the merge window.
- Its scroll state is eight loose fields: `synced`, `left_top`, `left_left`, `sideways_sync`, `wrap`, `wrap_fit`, `reveal`, `gutter_offset`. This is the shape `PaneScroll` replaced in the merge window. Its sync differs (two panes, always on, one shared top), so `PaneScroll` does not fit it; a `DiffScroll` of its own would.
- `recompute`, `computing` and `waiting` describe one recompute cycle. They may hold a state no enum would allow; nothing shows that they do.

Reservations about further splitting now:

- The beta (§A7.4) puts these files in testers' hands. A refactor moves the code a bug report points at, and a scroll refactor risks the behaviour testers see most. Every extraction so far kept the suite green, but the GPUI tests cover behaviour, not every frame's ordering in prepaint.
- What remains is mostly wiring. Splitting it moves code between files and adds indirection without unit tests to gain.
- `DiffScroll` is the one split with logic to test. It is worth doing when a diff scroll bug comes in, starting with that bug's test.

Both windows are fit for the beta as they stand.

#### A13.4 Recovery gate blocks the UI thread

`Recovery::retire` (`window.rs`) calls `Gate::close`, which waits for a running journal write and its sync. `recovery.rs` states a sync can take tens of milliseconds. If `retire` runs on the UI thread, the window stalls for that long.

- First confirm that `retire` runs on the UI thread, and measure the stall with a write in progress. Skip this item if the stall is below one frame.
- Hand the close to the background executor, and start the next whole write only after the close completes. Writes then land in order: a running write finishes before the next replaces the file. A flag the UI thread sets without waiting cannot give this, because a write that has already renamed its file over the journal cannot be undone.
- Tests: a write blocked on a test barrier holds the gate; `close` returns while it is blocked; the next whole write lands after the blocked one and wins. No sleeps; the barrier orders the threads.
- Keep the current guarantee that no write starts after `close`; the existing `a_closed_gate_skips_every_write` test stays.

#### A13.5 Pre-commit gate time

Limit: 35 s on a warm build (`AGENTS.md`). The gate takes 25 s after an edit to `merge-app` and 28 s after an edit to `merge-core`, which runs the tests of every crate but `vcs`, `vcs-git` and `vcs-jj`.

- **Object files.** With debug info, rustc on macOS keeps each build's `*.rcgu.o` files in `target/debug/deps`, and nothing removes them. Every rustc run lists that directory: 1.26 million objects cost 10 s a listing, in each of clippy's and the build's units. `[profile.dev] debug = false` stops them. Panics keep their locations; `CARGO_PROFILE_DEV_DEBUG=line-tables-only` turns debug info on. An older `target/` needs `find target/debug/deps -name '*.rcgu.o' -delete` once.
- **One integration-test binary per crate.** `tests/main.rs` declares each file as a module.
- **No serial tests in the quick profile.** A test with `threads-required = "num-test-threads"` waits for every running test to end and holds every thread while it runs. The six highlighter comparisons cost 10 s of the quick run's 20 s; without them it takes 10 s. pre-push and CI run them.
- **jj tests in `jj` modules.** The quick profile drops a test that builds a jj repository by its module path. A test outside a `jj` module runs in every gate.
- **Scoped checks.** `script/checks --quick` takes the changed paths: tests of the changed crates and their dependents, `cargo deny` and the vendored tests only when their inputs changed, no `cargo build`.

#### A13.6 VCS boundary

Done 2026-11-05. jj is the primary VCS, git the second, and any other a distant third, so the `vcs` trait follows jj's model and each backend holds its VCS's rules. A review found `merge-cli` deciding them itself: it built jj revsets (`@`, `@-`) and git's stopped-operation candidates (a copy of `vcs-git`'s `PENDING`), opened a concrete `GitRepository` for `merge.conflictStyle`, and called jj-lib for the marker length.

- `Repository` gained `merge_candidates`, `copy_candidates`, `working_copy_id` and `index_base` for origin lookup (spec §7.3), `conflict_style` and `marker_length`. `vcs-git` derives both its conflict terms and its merge candidates from one `stopped`.
- `merge_cli::origin` picks the file and checks bytes; the candidates are `CommitId`s, so it holds no revision syntax. It still branches on `RepoKind` for the tool protocols: `jj resolve` names the file in the title and `git mergetool` writes to it; `git difftool` may pass the working-tree file and the index, `jj diff --tool` passes copies.
- jj's merge candidates take `@`'s parents in the commit's order, not the revset's.
- Tests: the backend suites check each method against fixture commits and jj-core's marker rule (four past the longest marker-like run, at least seven). Breaking each method fails both them and the `merge-cli` origin tests.

Not moved: no backend registry or factory discovery, since a third VCS would reshape the interface; `RepoKind` stays for semantics, tool protocols, config and display. Tool config moved to the backends in §A13.8.

One jj config layering, done 2026-11-05. `vcs_jj::config::merge_settings` reads `merge.same-change` and `merge.hunk-level` from the system and user files, the repository's and workspace's files and the `JJ_*` overrides that the snapshot settings use, and resolves `--when` scopes. `merge_cli::jj_config` maps `Env` onto `ConfigEnv` and jj's values onto the engine's.

- The merge read stays read-only. jj-lib's `SecureConfig`, which the snapshot uses, writes metadata beside a config ID and migrates a legacy file; `merge_settings` finds the files by ID or legacy path and only reads them (`reading_writes_nothing`).
- It starts from an empty stack: jj-lib's defaults set both keys, and a key no file sets must stay unset so the settings file and flags (spec §6.4) keep their precedence.
- `ConfigError::Load` now names the file, and `BadId`, `Read` and `Value` name the file and key, for the snapshot path too. `ConfigEnv::jj_system_config_dir` is `/etc/jj`, as `Env` holds it.
- `merge-cli` passes no host name and no environment beyond `JJ_CONFIG`, so `--when.hostnames` and most `--when.environments` scopes do not match a merge's settings (spec §6.1).

#### A13.7 Crate layering

Done 2026-11-05. A review of the crate layers found the DAG sound but the middle muddled: `merge-cli` was the application layer under a CLI name, syntax-aware merging sat in the view model, two rules found a repository, and `vcs` declared an unused `merge-core`. The workspace is now, each crate depending on crates above it:

1. `merge-core`, `vcs`
2. `merge-syntax` (on `merge-core`); `vcs-git`, `vcs-jj` (on `vcs`)
3. `merge-view` (on `merge-core`, `merge-syntax`, `vcs`); `merge-session` (on `merge-core`, `merge-syntax`, `vcs-git`, `vcs-jj`)
4. `merge-cli` (on `merge-session`); `merge-app` (on `merge-view`, `merge-session`)
5. `triad-merge` (on `merge-cli`, `merge-app`)

- **`merge-syntax`** holds `language`, `syntax` and `boundaries`, with tree-sitter and the grammars. The headless merge reaches them without the view model. A `testing` feature gives `LineSpans::with_span_on` to `merge-view`'s tests.
- **`merge-session`** is the layer under both front ends: settings, key bindings, semantics, `jj_config`, `env::Env`, repository discovery (`native`), `resolve`, `origin` with `carry`, `replay`, `io`, `load`, `save`, `read_text`, `journal_dir`, `ExitCode` and `WindowForm`. It is the one crate that picks a backend. `merge-app` depends on it and not on `merge-cli`.
  - `WindowForm` carries `invocation::Window`, the five forms that open a window; `Invocation::into_window` gives it. `merge-app` no longer matches the headless forms.
  - `SessionError` holds the session's failures; `CliError::Session` wraps it.
  - `clap` stays a dependency: `ValueEnum` names the settings' values. Colours, themes and typography stay in the settings types, since one file holds every key (spec §6.4).
  - The log target of `save`'s event is `merge_session`.
- **`merge-core`** does no file I/O: `io` moved to `merge-session`.
- **One repository rule.** `native::nearest_repo` (the nearest `.jj` directory or `.git` entry, jj on a tie) serves both the semantics and `native::discover`, which opens only that backend at that root: `JjRepository::discover` or the new `GitRepository::open`, which does not search up. A `.git` git cannot open is `NotARepository`, not a climb to an outer repository (spec §6.1).
- **`Repository::conflict_style`** returns `Result<Option<vcs::ConflictStyle>, VcsError>`. `vcs-git` parses git's value case-sensitively; an unknown value is `VcsError::BadConfig`, logged by the caller, and gives diff3 (spec §6.4).
- **`merge_view::blink::Blink`** holds the jump blink's state for both windows, with the target's look at rest (`AtRest`): the merge window's frame shows at rest, the diff window's does not. `merge_view::runs::cell` builds a cell's text and runs.

Not done, with reasons:

- Opening the repository once per session. Done since, in §A13.10 item 6.
- A shared origin column type. The state is two `Copy` fields, `Visibility` and `IdKind`, whose types both windows already share.
- Moving the measure mode out of `MergeWindow`. Its state machine is in `measure.rs`; the 90 lines left drive it each frame through the window's scroll handle, `pick`, `go_to` and `send_edits`, which would otherwise widen the window's API.

#### A13.8 Layering review, 2026-11-07

A review of the crate boundaries found four leaks. The `clap` dependency of `merge-session` and the jj-only `jj_config` stay, as §A13.6 and §A13.7 decided.

1. **`merge-syntax` exposed tree-sitter and gpui-component.** Done. `Grammar::new` loads its language into a parser once and fails with `GrammarError::Version`, so `highlight`, `highlight_lines` and `highlight_source` cannot fail and return `LineSpans`. `merge-app` no longer depends on `tree-sitter`. gpui-component's registry names live in `merge_app::syntax::registry_name`. No test reaches `GrammarError::Version`: building a grammar of another ABI version needs FFI.
2. **Tool config in `merge-cli`.** Done. `vcs_git::tool_config` and `vcs_jj::tool_config` hold each VCS's tool keys and the user config file it writes to: `set_tool` edits a config's text, `global_config_path` and `user_config_path` name the file. `merge_session::install` holds triad's arguments, picks the backend, and reads and writes the file, so `merge-session` stays the one crate that picks a backend. `merge_cli::install` names the program (the console binary on Windows, the AppImage on Linux) and reports. `merge-cli` depends on gix only in its tests.
3. **Recovery in `merge-app`.** Done. `merge_session::recovery` holds the journal's commands, file, writes, pruning and restore, with their unit tests. `merge_app::recovery::Journaling` holds the two write tasks and cancels them before the journal's gate closes. `Gate` is private to the module.
4. **`MergeWindow` and `DiffWindow` repeat each other.** Done, at its true size. The windows share 34 method names, but most bodies differ: the settings banner, blinks, ruler marks and patch arithmetic already sit in shared modules, and the methods left are wiring to each window's state. Two were copies. `editor_sync` holds the editor-core bridge both windows use: `take_batch` turns recorded edits into one core batch, and `apply_patches` sends the core's patches back unrecorded and places the caret; `Caret` lives there. `ruler::editor_view` gives the lines an editor shows. The `merge-app` suite covers both through the two windows: breaking either function fails tests of both.

#### A13.9 Second layering pass, 2026-11-07

1. **`merge-cli` called the backends.** Done. §A13.8's tool-config move had `merge_cli::install` call `vcs-git` and `vcs-jj`; `merge_session::install` now does.
2. **GPUI's key form in `merge-session`.** Done. `Chord` gives its modifiers and key; `merge_app::keys::keystroke` writes GPUI's form. The key names stay in `keybind`: spec §6.3 defines them as the config file's vocabulary.
3. **The language rule three times.** Done. `merge_syntax::language::Language::of_run` holds spec §4.3's order: the inputs, then the output, then the title. `Merge::language`, the open dialog, the merge window's colours and the diff window's colours call it. The copies had already drifted: the open dialog built its merge from the inputs' names alone while the window coloured by the output's too, so extensionless inputs with an `out.rs` output were coloured as Rust but merged without Rust's syntax boundaries. `open_dialog::the_output_names_the_language_the_merge_keeps_syntax_boundaries_in` reproduces it.
4. **`merge-app` read the process's working directory.** Done. `merge-cli` reads it once and passes it in `WindowForm::cwd`; the merge window's `Settings` and `DiffSettings` carry it to repository discovery, the journal name, origin lookups, the resolve queue, the open dialog, the file watcher and the jj-copy banner. A window opened in a test takes any directory: `diff::a_relative_jj_copy_resolves_against_the_runs_directory`.

#### A13.10 Architecture review, 2026-11-07

A review of the abstractions reopened three decisions of §A13.6–§A13.8 at the owner's request: the repository opened more than once per run, `clap` in `merge-session`, and the jj marker rule in the `vcs` trait.

1. **Typed commits in the `vcs` trait.** Done. `RevSpec::Commit(CommitId)` reads a file at one commit; `annotate` and `merge_base` take `CommitId`s. The backends look a commit up by its ID (`commit_by_id`): gix's object header, jj's index and store. No expression is parsed, so a prefix, `HEAD` or `@` in a `CommitId` is `UnknownRevision`, and a jj commit reads its own tree. `merge_session::origin` passes the `CommitId`s it holds instead of their hex as expressions. `commits` stays the one method that resolves an expression. Two jj tests of a revision naming several commits went with the `&str` parameters: the types now rule the case out.
2. **jj's marker rule in `merge-core`.** Done. `Repository::marker_length` computed a merge rule from the inputs alone and read nothing from the repository; git's `None` stood for "not jj". `merge_core::jj::marker_length(&Inputs)` calls jj-core's `choose_materialized_conflict_marker_len`, beside jj's diff and trivial merge. `resolve` takes it for a jj conflict without jj's record (a tool protocol, spec §2). The backend tests of the rule moved to `merge_core::jj` and `merge_session` `resolve`.
3. **Suggestions taken in the core.** Done. `merge_core::suggest::edit(doc, region, EditSuggestion)` gives the `EditBatch` that takes a sorted union or a carried edit: the region's bytes, or the conflict's deletion and the carried text, sorted, with no empty patch. `MergeWindow::take_suggestion` sends it to the editor and the core. Its tests apply the batch and check the output, the resolutions and the single undo step.
4. **The diff window saves through `merge-session`.** Done. `merge_session::save_text(path, encoding, chunks)` writes a text's pieces after the encoding's byte order mark through `io::write_atomic_with`, a buffered form of the atomic write. `DiffWindow::save` passes the rope's chunks, so no side is copied into one `String` first. `Encoding::bom` and `Encoding::encode_chunk` split `encode` into its two parts.
5. **No argument parser in `merge-session`.** Done. `clap::ValueEnum` named the values of the settings, key bindings and flags. `merge_session::named::Named` does it now, over strum's `VariantArray`, `IntoStaticStr` and `EnumMessage` derives in kebab case, so no list of names can miss a variant. strum 0.28 was in the tree through gpui. `merge_cli::args::named` builds clap's `PossibleValuesParser` from it, with each value's doc comment as its help: the help text and the case-sensitive matching are unchanged. The session's enums stay apart from the core's, as the config file's vocabulary.
6. **The repository opens once per run.** `merge_session::repository::RunRepository` holds the repositories a run's directories lie in, in order, and opens the first on the first read. Its constructors name each run's directories: `of_merge` (the working directory, else the output's), `of_diff_tool`, `of_file` and `of_resolve` (the working directory, else the file's). Every step takes the run's repository: the semantics (`choose_semantics`, through `Caller::new`), the marker style, `resolve`'s conflict list and load, `diff PATH`'s load and the line origins (`origin::find_merge`, `find_diff`, `find_diff_tool`). No step finds its own. `Caller::find` is `Caller::new` over `of_merge`, so one rule finds a run's repository, and its root is canonical, as discovery gives it; a jj journal name now starts from the canonical root. `WindowForm::repo` carries the repository from `merge-cli` to `merge-app`, where it also gives the status bar's repository kind and the journal name. `Handoff`, a shared slot, takes it into the background task of the window's origin lookup: the first lookup takes it, a later one finds the repository again from the run's directories, by the same rule. The window settings that hold a lookup are `Clone` without `PartialEq`. `Repository` is `Send`. The resolve queue opens one repository per file, for its conflict list, its load and its origins, after the save before it.
7. **One repository from finding to opening.** A review of item 6 found two ways the steps of a run could read different repositories. First, `RunRepository` reported its first candidate's kind and root before opening, and `open` then passed over a `.git` git could not open. A broken `.git` in the working directory gave git semantics and journal name, with the origins of the output's jj repository. `find` now opens a git candidate as it finds it, which is cheap, and passes over one git cannot open. jj's opens on the first read: its backend never reports `NotARepository` for a `.jj` it was given. `kind` and `found` then name what opens: `a_git_entry_git_cannot_open_is_not_found_before_any_step_opens`. Second, when its slot was empty, `Handoff` found the repository again by a closure in `merge-app` that restated the rule, and did it differently. `diff PATH` did not resolve the file's links, and a `resolve` window used `of_merge`, which finds no repository for a file deleted with its directory. `Handoff` now keeps the run's directories and finds the repository again with `RunRepository::find`: `a_handoff_gives_its_open_repository_once_then_finds_it_by_the_same_rule`. It has no `Default`. The tests that stood in empty `.git` directories for repositories now make real ones.

#### A13.11 Code review, 2026-11-09

A review of the layering found the crate graph sound and the save path above the session layer.

1. **One save in the session layer.** Done. The window and `resolve --headless` each wrote the output, recorded the save in the repository and turned both errors into one. `EncodedOutput::save` does it once (§A13.12 item 3). `SaveError::Write` wrote nothing; `SaveError::Record` wrote the output, names its exit code and path, and keeps the `VcsError`. The window's alert after a record failure says the file is written, and its Try Again calls `Record::resave` alone. Tests lock git's index (`.git/index.lock`) to make the record fail.
2. **The three-way shape named once.** Done. `Inputs::is_three_way` replaces four copies of `THREE_WAY_SIDES`.
3. **Save & Exit off the UI thread.** Done. The window encodes the output, writes the journal, then runs `EncodedOutput::save` or, for Try Again after a record failure, `Record::resave` in a background task held in `MergeWindow::saving`. Save & Exit and Close do nothing while it runs. jj's snapshot after a save, 168 ms at 100k files, no longer freezes the window. The window's tests park the executor after a save.
4. **A `resolve` save closes its own window.** Done. The queue closed every window after a save, the About window included. `Queue::window` holds the merge window it opened, and a save closes that one.
5. **The atomic write syncs its directory.** Done. `io::write_replacing` syncs the file, renames it over the target, then on Unix syncs the target's directory, so the rename survives a crash. A failed directory sync is logged: the file already holds its bytes. No test observes a sync; the existing write tests cover the path.
6. **The `resolve` queue lists and loads off the UI thread.** Done. `resolve_queue::Finder` holds the run's directory, options, jj config and the files opened; `Finder::find` lists the conflicts, loads the next file and skips refusals on a background thread, each step a jj snapshot of 99–105 ms at 100k files. The UI thread reports the refusals and opens the window.
7. **One scan fewer per file.** Done. `resolve::load` takes `Snapshot::Take` or `Snapshot::Taken`: `Taken` reads at the snapshot the caller took on the same `RunRepository` just before. `Record::saved_in` records through the run's repository, which then reads at the save's operation, and refuses another repository's before any write. The queue loads at its list's snapshot: two scans per file, from three. `resolve --headless` without a path loads each file at the list's or the last save's snapshot and records through the run: N + 1 scans for N files, from 2N + 1. At 100k files a scan costs about 100 ms (§D5.5). The window's Save & Exit still records through a repository it opens, since the run's went to the origin lookup.

#### A13.12 Structure review, 2026-11-09

A second review on 2026-11-09 read the code, not the plan. The crate graph holds; the work is in the GUI crate's size, the session crate's breadth and three loose ends.

1. **`RunRepository` has no equality.** Done. Its `PartialEq` compared the directories and the repository found, and ignored the open handle and the jj config, so two runs that read different config compared equal. It existed so `WindowForm` and `merge_cli::Outcome` could derive `PartialEq` for tests. Both now derive `Debug` alone; `Outcome::exit_code` gives a finished run's code, and the open-dialog test compares `WindowForm`'s fields. A `compile_fail` doctest on `RunRepository` keeps the equality out.
2. **A `Handoff` found again keeps the jj config.** Done. A take after the first found the repository again from the run's directories but dropped `RunRepository::with_jj_config`, so a jj repository opened by a later origin lookup read the process's jj config, not the run's (`resolve`'s `ConfigEnv`). `Handoff` keeps the config beside the directories: `a_handoff_found_again_reads_the_runs_jj_config`.
3. **One save API.** Done. Six entry points wrote or recorded a merge: `save`, `save_and_record`, `save_and_record_in`, `record_save`, `EncodedOutput::write` and `EncodedOutput::save`, under two error types. Now `EncodedOutput::new(document, ending)` encodes the result; `write(path)` writes it and fails with `io::WriteError`; `save(path, record, run)` writes it, then records the save through `run`, the run's repository, or without one in the conflict's repository opened afresh, and fails with `SaveError`. `Record::resave` records a written save again. `resolve --headless` saves through the run; the window saves without one, since its run's repository went to the origin lookup. `a_save_through_the_runs_repository_stages_it_there_and_refuses_another` covers the `run` path.
4. **Line origins for a merge of any width.** Done. `Repository::merge_candidates` returned three lists, base, left and right, and the merge window looked origins up only for a three-way merge, so a jj conflict of three or more sides opened without origins or revision chips. `vcs::MergeCandidates` holds each base's and side's own commits and the commits every term shares; `base(n)` and `side(n)` give a term's, its own first. git names one base and two sides; jj shares `@`, its parents and their one common merge base, now for any number of parents: each further parent's merge base with the base so far. `origin::Found` holds a `Vec` per input, and `origin::find_merge` takes the merge's `Inputs` in `merge_inputs` order: every base, then every side. Tests: `every_term_of_a_wider_merge_may_hold_the_working_copy_a_parent_or_their_merge_base` and `parents_without_one_common_merge_base_offer_none` (`vcs-jj`), `a_three_sided_jj_conflict_finds_every_term` (`merge-session`), `a_three_sided_jj_merges_headings_name_each_sides_commit` (`merge-app`).
5. **`merge-config` holds the configuration.** Done. `merge-session` held two concerns: the configuration (the config file's format, settings, key bindings and value names, about 3,700 lines with tests) and the session (semantics, repositories, loading, origins, saving, recovery). §A13.7 put both there so one crate served both front ends. `merge-config` now holds `config`, `settings`, `keybind` and `named`, and the value enums `Choice`, `SameChangeArg` and `HunkLevelArg`, which moved from `semantics` to `settings`. It depends on `merge-core` and `vcs` alone; `script/check-workspace` fails the build when it gains another workspace crate. `merge-session` depends on it and re-exports nothing: the front ends name `merge_config::settings` themselves. `merge-session` dropped `serde` and `strum`. The layers of §A13.7 gain `merge-config` at level 2, on `merge-core` and `vcs`.
6. **How a session ends, without GPUI.** Done. `MergeWindow` decided Save & Exit, Close, a save's outcome and the answers to their alerts inline, between prompts and tasks, so the rules of spec §6.2 had only window tests. `merge_session::ending` holds them: `save_requested`, `unresolved_answered`, `close_requested`, `close_answered`, `saved` and `failure_answered` each give a `Next` step, and `Journal` says whether the journal is removed, written or left. `UnresolvedSave`, `Closing`, `Undone` and `SaveFailed` moved there; the window keeps their labels. `MergeWindow::step` carries a step out: it asks, writes, records or finishes. The module's unit tests cover each rule; breaking one fails them and the window's save and journal tests. `window.rs` is 3,740 lines with 56 fields; the rest of §A13.3's split stays optional.
7. **The two windows' shared methods.** Not done, with the measurement. `MergeWindow` and `DiffWindow` share 37 method names. Compared body to body, the near-identical ones are accessors and wiring of 2–9 lines (`status_text`, `settings_banner`, `blame_shown`, `toggle_origin_column`, `next_difference`); together about 20 lines. The ruler's `ruler_click`, `view_ticks` and `render_ruler` already call `ruler::strip`, `ruler::editor_view` and `merge_view::ruler::centred_top`, and differ in each window's state: one result editor, or the diff's right pane with synced scroll. The rest share a name, not a body (`send_edits`, `undo`, `loaded_origins` below 0.65 similarity). A shared editor pane would need one abstraction over `MergeDocument` and the two-way diff's document, whose commands differ, to save those 20 lines. §A13.8 item 4 and §A13.7's origin-column decision stand.

#### A13.13 Session review, 2026-11-09

A third review found two defects and the session state held by the window.

1. **No commands while Save & Exit writes.** Done. A save writes the merge as it stood at the keypress, and a resolved save then deletes the journal and exits. `MergeWindow::apply` refuses every command while `saving` holds a task, and the result editor is read-only until the save ends, so a pick, an edit, undo or redo made during the write no longer vanishes. A failed save unlocks the result. Tests: `a_pick_while_save_and_exit_writes_changes_nothing`, `an_undo_while_save_and_exit_writes_changes_nothing`, `typing_while_save_and_exit_writes_leaves_the_result_as_saved`, `after_a_failed_save_the_result_takes_edits_again`.
2. **`inspect` merges as a merge does.** Done. `inspect` built its options from the diff flags alone and aligned without syntax boundaries, so with `refine-conflicts` or `syntax-boundaries` set in the settings file its regions differed from the ones `--commands` numbers (spec §6). `Invocation::Inspect` carries the merge options a merge takes from the settings and flags, and `inspect` loads through `merge_session::load` with the language of the inputs' names. Tests: `inspect_refines_as_the_settings_file_says`, `inspect_keeps_syntax_boundaries_as_the_settings_file_says`.
3. **The session in the session layer.** Done. `merge_session::session::Session` owns the merge window's `MergeDocument`, its `Recovery`, its `SaveTarget` (output, record, line endings, unresolved-save rule) and whether a save runs. `apply` refuses every command while a save runs and journals each command that applies; `start_save` writes the journal, locks the session and returns a `SaveJob` that runs on a background thread; `saved` unlocks it and gives the next `ending::Next`; `save_requested`, `close_requested`, `copy` and `finish_journal` apply spec §6.2 to the session's state. `MergeDocument::unresolved` and `MergeDocument::changed` give the counts the rules read, so the session needs no `merge-view`. `MergeWindow` holds the session, the save task and `JournalTasks`, the debounce and write tasks, which it cancels before the session flushes, discards or restores the journal. The CLI's headless merges keep `EncodedOutput`: they journal nothing and save once. Tests: `session::tests` in `merge-session`, ten rules without GPUI.
4. **The command line's forms by kind.** Done. `Invocation` is `Window(Window)`, `Batch(Batch)` or `ShowConfig`. `Batch` holds the forms that run without a window: `Headless`, `Resolve` (`resolve --headless`), `Inspect`, `DiffInspect` and `Install`. `Resolve` carries no `headless` flag; the parser picks the kind. `merge_cli::run` matches the three kinds once: `window_form` settles a window's repository and semantics, `batch_form` runs a batch form, and `resolve_prepare` lists `resolve`'s targets and chooses their semantics for both. No `unreachable!` remains in the dispatch.
5. **One name per type, one owner per crate.** Done. The window's settings are `merge_app::WindowSettings`, apart from `merge_config::settings::Settings`. `merge_core::diff2` re-exports `Rope` and `RopeSlice`, which its API returns, and `merge-app` names them there; only `merge-core` depends on `ropey`.
6. **One origin column for both windows.** Done. `MergeWindow` and `DiffWindow` each held `Origins`, the column's `Visibility`, the `IdKind` and the inputs' commits, with the same toggle and shown rule. `origin_column::OriginColumn` holds them: `shown`, `toggle`, `set`, `annotated`, `described`, `cell`, `heading` and `labels`. `Visibility`, `OriginCell`, `HeadingText` and `ORIGIN_WIDTH` moved there from `window`. Unit tests cover the shown rule, cells, headings and labels without GPUI.
7. **Search and scroll state in components.** Done. `input_find::Found` holds the query the input panes are searched for; `Found::follow` returns `Follow::Unchanged`, `Search` or `Cleared`, with unit tests. Item 8 takes the scroll state further.
8. **The merge window's scroll in one component.** Done. `pane_scroll::PaneScroll` holds the rows' list handle, `Sideways`, `RowWrap`, soft wrap, the down and sideways syncs (one `Option`, so they turn on and off together) and the region `go_to` reveals. `sync` steps both syncs, moves the rows and returns the result's moves as `Synced`; `revealed` places both panes and the syncs; `keep_place` holds the rows on their aligned row when wrapping changes their display rows. The window reads the editor's geometry, passes the region mappings in, and applies the result's moves. Fifteen unit tests cover following both ways, sideways range and wrap, sync off and resync, reveal and keep-place, with no window.
9. **Search and the result's decorations stay on the window.** Their logic has modules already: `input_find::Found` and `blink::SearchSteps` for search; `marks::bands`, `marks::bands_unchanged`, `status`, `ruler::marks`, `nav::held_region` and the placeholders for the result. What `follow_search`, `search_inputs`, `refresh`, `follow_caret` and `paint_highlight` keep is wiring: task spawns, `Flash::start` and the editor's decoration collections, all through `cx`. A struct around those fields would move the wiring without testing more. `MergeWindow` holds 42 fields in 3,463 lines.

### A14 Search and replace

Spec §5 (two-way mode) and §6.3 (Find, search in the input panes).

- **Editors.** The merge result and both diff panes use gpui-component's search panel: query, replace, case toggle. The diff window's left pane is `replaceable(false)`. Its toolbar Find opens the focused pane's search, else the left pane's.
- **Input panes.** They are a list of rows, not editors, so the result's search session drives them: the window observes the result editor and, when the query or case changes, searches every input pane (`merge-app/src/input_find.rs`) over `merge_view::search`. Each pane keeps its own hits and current match; its heading holds 18 px ghost step buttons and the count. The matcher is Aho-Corasick on one literal with ASCII case folding, as in the editors' search, so both count alike. Hits are ordered by row, then pane, then byte; `merge_view::runs` paints them as a third layer over syntax and intra-line marks, in the editors' fills.
- **Off the UI thread.** On three 100k-line inputs of 4.7 MB each, release build, case-insensitive: `value_9999` (33 matches) takes 9.8 ms to find and 0.03 ms to order; `e` (1.2M matches) takes 17 ms to find and 60 ms to order. So the search runs on the background executor over O(1) `Text` clones and a copy of the regions, and a result for an older query is dropped (`input_find::Found`).
- **Focus.** The input panes hold no focus of their own: a click on a row moves the result caret and leaves the focus in the result. One query, the result's, therefore serves every pane, and a step scrolls its pane without moving the caret. A focus handle on the rows would change what every key does after a click.
- **Blink.** `merge_app::blink::Flash` runs every blink: a region after a jump, a difference, and a match after a search step. It pairs `merge_view::blink::Blink` with the step timers and repaints through a function the window gives it. An editor's search panel steps inside gpui-component, so `blink::SearchSteps` reads steps off the session: the current match moved while the matcher's `Rc` of ranges stayed. Typing a query or editing finds the matches again into a new `Rc`, which is no step. A step that stays on the only match changes nothing it can see, so it does not blink.
- **Not measured** in the running app: `script/measure-app` has no mode with a query open. With no search a cell looks up an empty hit list; with one, a binary search per row and pane.

## B. Merge semantics

triad merges as the repository's VCS does. In a git repository it follows `git merge-file`, as today. In a jj repository it follows jj's file merge. Refinements of triad's own sit on top in both modes. None of them changes the automatic output unless the user turns it on.

### B1 Goal

- **Default output equals the VCS's.** For any input the caller's VCS resolves, triad's automatic output matches it byte for byte. Where the VCS leaves conflicts, triad shows them in the same places. In jj mode this holds when every input is UTF-8 and either all inputs or none start with a byte order mark (§B2, bytes).
- **One model.** Both modes produce the same `Region` list (spec §4.3). `merge-view`, `merge-app`, the journal and `inspect` see no difference except the recorded options.
- **Refinements are opt-in and visible.** A refinement that changes the output is off by default and marks the regions it resolved, so the user reviews them.

### B2 How jj merges (jj-core 0.46.0)

These facts come from the source, `jj-core/src/diff.rs`, `jj-core/src/merge.rs` and `jj-lib/src/files.rs`. Each task that relies on one cites it again.

- **Where the code lives.** `jj-core` holds the diff (`ContentDiff`, the `CompareBytes` trait, the tokenizers) and `Merge` with `trivial_merge`. `jj-lib`'s `files.rs` holds the file merge on top: `merge_hunks`, `resolve_diff_hunks`, `merge_hunk_by_word` and `collect_hunks`, about 80 lines.

- **One diff over every input.** `ContentDiff::by_line` diffs base 0 against every other input in one pass. A hunk is `Matching` only where all inputs agree; every other hunk is `Different` and holds a range in each input.
- **jj's own line diff.** It anchors first: among base's lines with the lowest occurrence count, it takes those that occur as often in the other input, matches them by the longest common subsequence, and recurses between anchors. Only where no anchor exists does it trim the common prefix and suffix. It gives up anchoring in a region where every line of base occurs more than 100 times. It has no slider compaction and no indent heuristic. It differs from git's histogram on repetitive input.
- **Bytes, not text.** jj merges the stored bytes and decodes nothing. A line ends at `\n` and includes it; a missing final newline leaves the last line without one. This is triad's rule (spec §4.1) on UTF-8 input. A UTF-8 byte order mark is part of jj's first line, and UTF-16 splits at the `0x0A` byte of each `\n`, so jj and triad diverge on UTF-16 and on inputs that disagree about a byte order mark.
- **Byte-exact lines.** The merge compares lines with `CompareBytesExactly`. It has no whitespace modes. `jj-core` has `CompareBytesIgnoreAllWhitespace` and `CompareBytesIgnoreWhitespaceAmount` for `jj diff`; they treat the terminator as whitespace, and neither matches triad's `trailing`.
- **Empty input.** When base or any side is empty, the diff tokenizes nothing, and the whole file is one `Different` hunk.
- **Matching hunks take base's text.** Under a comparator that ignores whitespace, a `Matching` hunk's sides may differ from base; jj keeps base's bytes.
- **Trivial resolution.** `trivial_merge` cancels each base against a side with the same text. triad's spec §4.3 rule is a copy of it.
- **`merge.same-change`** (default `accept`). `accept` resolves a hunk where every side made the same change, as git does. `keep` leaves it conflicted.
- **`merge.hunk-level`** (default `line`). `word` merges each conflicted line hunk again word by word, and keeps the result only when the whole hunk resolves. A word is a run of ASCII letters, digits, `_` and bytes `0x80`–`0xff`. `ContentDiff::by_word` diffs the words first, then refines each changed region one non-word byte at a time.
- **Reference base.** With more than two sides, base 0 is the reference, as in triad.

### B3 Choosing the mode

A repository with `.jj` is a jj repository and merges with jj semantics. triad does not support git merges in a jj repository: mutating git commands in a jj repository race jj's import and export of refs, so jj is the only supported caller there.

| Source | Mode |
|---|---|
| `--merge-semantics git` or `--merge-semantics jj` | as given, but `git` in a jj repository is an error (below) |
| `triad-merge install --git` / `--jj` | writes the flag into the tool config (spec §7.1) |
| native resolution (spec §7.4) | the backend's VCS; the jj backend in any repository with `.jj` |
| `--merge-semantics auto` (default) | the nearest ancestor with `.jj` or `.git`, probing the working directory, then the output path: `.jj` gives jj; `.git` alone gives git; neither gives git |
| settings file `merge-semantics` | as the flag; the flag wins |

- **The flag is the main mechanism.** Both install configs carry it, so a tool run never depends on probing. `auto` serves manual runs.
- **git in a jj repository.** When the probe finds `.jj` and the mode is `git`, from the flag or the settings file, triad writes nothing and exits with 3. The message names the repository and says to resolve with `jj resolve`. This catches `git mergetool` after a `git merge` or `git rebase` in a jj repository.
- **Temporary paths.** `jj resolve` writes all four files, output included, to a temporary `jj-resolve-<random>/` directory as `base_<name>`, `left_<name>`, `right_<name>` and `output_<name>` (jj-cli 0.46 `merge_tools/external.rs`, followed by `vcs_jj::testing::Fixture::resolve`). The output path therefore never lies in the repository, which is why `auto` probes the working directory first. A path in a `jj-resolve-<random>/` directory also marks jj as the caller, as `jj-diff-<random>/` does for the diff banner (spec §7.1), so `auto` picks jj there even when the working directory lies outside any repository. The §B5.1 spike confirms the working directory jj gives a merge tool.

In jj mode, `merge.hunk-level` and `merge.same-change` come from the user's jj config and the repository's `.jj/repo/config.toml`. A triad flag or setting overrides each.

Under `jj resolve`, jj has already merged with these two settings before it wrote the conflict. The conflicts triad receives are ones jj's merge left, so word mode resolves nothing more there. The settings change triad's output on manual runs and in native resolution (spec §7.4), where triad merges from the stored inputs.

### B4 Options

`MergeOptions` (spec §4.3) gains three fields. The journal records them: `semantics` and `same_change` with `JOURNAL_VERSION` 3 (§B5.3), `hunk_level` with version 4 (§B5.4).

| Field | Values | git default | jj default |
|---|---|---|---|
| `semantics` | `git`, `jj` | `git` | `jj` |
| `same_change` | `accept`, `keep` | `accept` | from jj config |
| `hunk_level` | `line`, `word` | `line` | from jj config |

Existing options keep their meaning:

- `same_change = keep` is allowed in git mode, as `hunk_level = word` is (decision 3). git has no such setting, so the status bar marks it there.
- `diff-algorithm` (`histogram`, `myers`, `myers-minimal`) applies in git mode only. jj mode has one diff; the status bar says so.
- `ignore-whitespace` applies in both modes with the same meaning. jj mode passes `ContentDiff::for_tokenizer` a `CompareBytes` that compares and hashes lines by `diff::line_key`, so `trailing`, `amount` and `all` ignore what they ignore in git mode. The output rule of spec §4.2 applies to jj regions too: a line equal under the mode takes the bytes of a side that differs from base, where jj would keep base's. Any mode other than `none` diverges from jj, so in jj mode the status bar marks it.
- `refine-conflicts` (zdiff3 trimming) splits shared lines off a conflict without changing the output, so it stays on by default in both modes.

### B5 Phases

#### B5.1 Decide the jj engine (spike, one day)

Two ways to get jj's line diff into `merge-core`:

1. **Depend on `jj-core`.** Apache-2.0, already in the build through `jj-lib`, exact-pinned with it. The diff is jj's by construction. The hunk resolution of `files.rs` lives in `jj-lib`, which `merge-core` must not depend on (it holds no VCS code), so `merge-core` re-implements those 80 lines over `jj-core`'s `Merge` and `trivial_merge`, with an Apache-2.0 notice. `merge-core` gains `jj-core`'s dependencies: `bstr`, `regex`, `globset`, `futures`, `chrono`, `blake2`, `tempfile`, `async-trait`, `clru` and others, all already in the tree through `vcs-jj`, but new to builds of `merge-core` alone, such as its benchmarks and the CLI library's tests.
2. **Port the diff.** About 600 lines of `diff.rs` plus the 80 lines of `files.rs`, with an Apache-2.0 notice. `merge-core` stays lean. Every jj upgrade needs a diff review.

The spike:

- Wraps `ContentDiff::by_line` over three `Text`s and maps its `hunk_ranges` from bytes to line ranges. Tests from hand calculation: a missing final newline on one side, `\r\n` lines, a lone `\r`, an empty input, repetitive lines.
- Measures load and merge at 100k and 1M lines against spec §9 (< 100 ms, < 1 s) and records peak memory.
- Measures a clean build of `merge-core` with and without `jj-core`.
- Confirms the working directory jj 0.46 runs a merge tool in (§B3).
- Records the result in this plan, as decision 1 of §B7.

Recommendation: option 1, unless the spike misses spec §9.

Result, 2026-10-28, on an M4 Pro, release build, seeded generator (`merge` bench):

| Measure | 100k lines | 1M lines |
|---|---|---|
| `jj-diff`: `LineDiff::new` and every hunk in line ranges | 34 ms | 423 ms |
| `align` (git mode, for comparison) | 16 ms | 373 ms |
| `load` (git mode) | 23 ms | 442 ms |
| Peak resident memory, inputs and jj diff | 83 MB | 963 MB |
| Peak resident memory, inputs and `align` | 53 MB | 502 MB |

- Load in jj mode replaces `align` with the jj diff, so it comes to about 41 ms and 490 ms, under spec §9's 100 ms and 1 s. The jj diff's peak is about twice `align`'s; at 1M lines a session stays under spec §9's 2 GB.
- A clean debug build of `merge-core` compiles 81 crates in 5.5 s, against 37 crates in 3.6 s without `jj-core`.
- `jj-core`'s `regex` and `aho-corasick` add `PartialEq<Span>` impls for `Range<usize>`. `assert_eq!(ranges, [])` then cannot infer its element type in any crate that links `merge-core`; such tests compare against a typed empty constant.
- Byte ranges map to line ranges with one binary search per boundary in `Text`'s line index. Every boundary of a line diff lies on a line boundary.
- Working directory: jj 0.46 runs a merge tool in the caller's working directory. Run from a subdirectory, the tool starts there; run with `jj -R repo` from outside, the tool starts outside any repository, and only the `jj-resolve-<random>/` output path identifies jj (§B3). The output path was `$TMPDIR/jj-resolve-<random>/output_<name>`.

Decision 1 follows: option 1.

#### B5.2 jj oracle corpus

Tests first: the corpus checks of §A4.8, run against the jj corpus.

- `script/gen-jj-oracle` runs the git corpus's triples through `jj_lib::files::merge_hunks` under each combination of `same_change` and `hunk_level`. It writes `fixtures/oracle/jj-corpus.jsonl`: one case per line with `id`, `sides`, `bases`, and under `merges` one result per mode (`accept-line`, `accept-word`, `keep-line`, `keep-word`). A result holds the conflict count and jj's hunks in order: each is `{"resolved": text}` or `{"conflict": {"sides": […], "bases": […]}}`. The conflicts' terms let §B5.3 check where each conflict lies, not only the text between them.
- Hand seeds for the cases where jj and git differ: repetitive lines (braces, blank lines), a change that touches a neighbour's change, identical changes on both sides, word-resolvable conflicts, multi-byte UTF-8 inside words, lines on either side of the 100-occurrence limit, and n-way merges with five and seven terms.
- Corpus checks: every source present with unique ids and all four modes; resolved and conflict hunks alternate, and each conflict holds every input; a side equal to base gives the other side cleanly; identical changed sides resolve under `accept` and conflict, with equal sides, under `keep`; word mode's conflicts are a subset of line mode's; the hand seeds match hand calculation.
- Result, 2026-10-28: 1,970 cases, 1,955 from the git corpus and 15 seeds. jj's line-mode conflict count differs from git's `--diff3` exit code in 58 of the git cases. Where both merge cleanly, the outputs match. Word mode resolves more conflicts than line mode in 261 cases.
- `jj-lib` reads the triples as UTF-8 bytes, which the corpus stores as JSON strings, so the corpus has no UTF-16 or byte-order-mark cases. Unit tests in §B5.3 pin triad's behaviour there: it merges the decoded text, and spec §4.2 records the divergence.
- The oracle runs `jj-lib`'s `files.rs`; option 1 shares only `jj-core` with it. The corpus then tests triad's copy of the hunk resolution, its mapping from hunks to regions and its output rendering, but not the diff. With option 2 it tests the port as well.
- The generator is Rust, since only `jj-lib` can produce the oracle: an example in `vcs-jj`, which already depends on `jj-lib`, run by `script/gen-jj-oracle`. Dev-time only, like `script/gen-oracle`.

#### B5.3 `semantics` in `merge-core`

Tests first:

- Every jj corpus case: the automatic output matches; the conflict count matches; text outside conflicts matches.
- Region invariants (spec §4.3) hold in jj mode: regions cover every input in order, without gaps or overlaps. Add jj mode to the §A4.9 properties.
- A seed where git and jj place a conflict differently gives each mode its own placement.
- n-way inputs (spec §7.4) in jj mode match `merge_hunks` with five and seven terms (three and four sides).

Work:

- `align` dispatches on `semantics`. jj mode turns each `Matching` hunk into an `Unchanged` region and each `Different` hunk into a region classified by the shared `classify`.
- `classify` calls `jj-core`'s `trivial_merge` on the lines' comparison keys, in both modes. For three-way merges it equals the git-mode rule. With more sides it adds jj's same-change rule, which git mode lacked: sides X, X, Y over bases Y, b resolve to X.
- `same_change = keep` classifies an all-sides-equal region as `Conflict`. Its pick options stay as for any conflict. Refinement leaves such a conflict whole: trimming would move all of its sides' lines out.
- `refine-conflicts` and `ignore-whitespace` run on jj regions as on git ones. `ignore-whitespace` goes through the `CompareBytes` of §B4.
- UTF-16 and mismatched byte order marks: jj mode merges the decoded text. A test per case pins the result, from hand calculation.
- `MergeOptions` and the journal header gain `semantics` and `same_change`; `JOURNAL_VERSION` becomes 3. A version-2 journal is not restored. Both fields default when absent, so a version-2 header parses and restore refuses it by its version.

Spec: §4.2, §4.3, §6.2 (journal version). `json-schema.md` changes with `inspect`'s flags in §B5.5. Other documents: `crates/merge-core/README.md` (the options); §A4.8 and `AGENTS.md` (the jj corpus and `script/gen-jj-oracle`, from §B5.2).

#### B5.4 Word-level merge

Tests first:

- jj corpus cases with `hunk_level = word` match.
- A conflict that resolves word by word becomes a resolved region whose text is jj's merged text, in both modes.
- A conflict that resolves only in part stays a whole conflict (jj's all-or-nothing rule).
- Undo of a pick on a word-merged region returns to the word-merged text.
- The journal replays a session with word-merged regions.
- `inspect --json` reports the new kind and its text.

Work:

- A new `Kind::WordMerged`. Its automatic resolution is `Resolution::Merged`, which holds the merged text. Picks and edits work as on other regions; `ClearChoice` returns to `Merged`.
- The journal records the new kind and resolution, so `JOURNAL_VERSION` becomes 4. A version-3 journal is not restored.
- The word tokenizer follows jj's rule (§B2), so jj mode matches byte for byte. Git mode uses the same tokenizer; git has no word-level merge to match.
- The view gives word-merged regions their own tone and gutter letter (spec §5: colour is never the only signal), and the status bar counts them, so the user reviews them.
- `--hunk-level line|word` and the settings file's `hunk-level`, and jj's `merge.hunk-level` under jj semantics, through `jj_config` with the same precedence as `same-change` (§B5.5).

Spec: §4.3, §4.4, §5, §6.4.

#### B5.5 Mode selection and jj config

Tests first:

- `auto` picks jj in a jj repository, internal or not, git in a git repository, git outside any repository. Use the conformance fixtures of spec §8.8.
- `auto` takes the nearest ancestor: a git repository nested in a jj workspace gives git, and the reverse gives jj.
- The working directory wins over the output path when they lie in different repositories.
- An output path in a `jj-resolve-<random>/` directory picks jj, from any working directory.
- `--merge-semantics git` in a jj repository, internal or not, exits with 3, writes no output, and names the repository and `jj resolve` in the message. The same from the settings file.
- The flag overrides the settings file; the settings file overrides `auto`.
- `install --git` writes `--merge-semantics git`; `install --jj` writes `--merge-semantics jj`. Existing install tests get the new arguments.
- `--same-change keep` in git mode classifies an all-sides-equal region as a conflict; `--hunk-level word` in git mode word-merges conflicts.
- jj config: `merge.hunk-level` and `merge.same-change` read from a user config file and from `.jj/repo/config.toml`, the repository's value winning. A missing file gives jj's defaults. A bad value is an error naming the file and key.
- A triad flag overrides the jj config value.

Work:

- `--merge-semantics`, `--same-change` and `--hunk-level` in `merge-cli`; `merge-semantics`, `same_change` and `hunk_level` in the settings file (spec §6.4).
- Repository detection reuses `native::discover`, which keeps the nearest root and picks jj on a tie. No VCS executables.
- `vcs-jj` loads the two keys with `jj_lib::config::StackedConfig`. jj-lib does not locate the user config; jj-cli does. Follow jj's documented order: `$JJ_CONFIG`, then the platform config directory's `jj/config.toml` and `jj/conf.d/*.toml`. Record any source triad does not read (for example `--config` arguments to `jj`) in the spec.

Spec: §6.1, §6.4, §7.1. Other documents: `README.md` (the new flags, and `--diff-algorithm` applying in git mode only), `crates/merge-cli/README.md` (config reading).

#### B5.6 Status bar and settings UI

Tests first: the status bar shows `git merge` or `jj merge`, `word` when on, and a mark when `ignore-whitespace` or `same_change` diverges from the VCS's behaviour: `ignore-whitespace` other than `none` in jj mode, `same_change = keep` in git mode. Settings changes re-align as spec §6.4 says today.

Spec: §5, §6.4.

#### B5.7 Further refinements (each opt-in, each its own task)

In order of value against effort. None changes the default output.

1. **Insert/insert suggestion.** When both sides insert at the same place and delete nothing, offer Take Left then Right as a one-click suggestion.
2. **Sorted-block union.** When both sides add lines to a block whose lines are sorted in every input (imports, dependency tables), offer the sorted union.
3. **Deep refine.** `refine-conflicts = "deep"`: diff left against right inside a conflict and split at interior shared lines that map consistently to base. Smaller conflicts, at the cost of git parity; fixtures from hand calculation.
4. **Moved blocks.** Match blocks deleted in one place and inserted in another on one side, show the moves, and offer to carry the other side's edit to the new place.
5. **Syntax-aware boundaries.** `tree-sitter` (MIT) to keep word-merge and deep-refine boundaries off the inside of syntax nodes (§B5.8). Deferred until after M7 (§B7, decision 6). `mergiraf` (GPL-3.0) is out under spec §2.5, and `difftastic` is already rejected (§A11, decision 6).

Each suggestion appears as a choice in the region's menu, never as an automatic resolution. Each one updates spec §4.4 when it lands.

#### B5.8 Syntax-aware boundaries

§B5.7, item 5. Both refinements only reject: a deep-refine split or a word merge that cuts through a syntax node is dropped, and the region stays as it would be without it. No new text appears: with the setting on, every output is one the run without the refinement could give. It applies in git and jj mode alike; in jj mode a rejected word merge diverges from jj, so the status bar marks the setting there (§B7, decision 7).

**The rule.** A region's text, without the whitespace around it, is its core. A cut is a byte offset of one input.

- **Deep refine.** A cut is clean when no node lies across it inside the core: every node that holds bytes on both sides of it also holds the whole core. `split_shared_blocks` keeps a shared block only when both its edges are clean in every side. A shared `}` that closes a different block on each side, the common bad split, fails on the side whose block it closes.
- **Word merge.** A cut is clean when it lies inside no token, a leaf of the tree: a string's content, a comment, `->`. Every word hunk lies inside some statement of the region, so the node rule would reject nearly every word merge. `word_merge` keeps its text only when every word hunk resolved from a side starts and ends at clean cuts of each side that holds its text. Else the region stays a conflict.
- The region parses with the lines around it: from the top-level line at or before its first line to the first top-level line after it, by the rule of §F4. A window longer than 20 000 lines, an input without a grammar, or an error or missing node across the core leaves every cut clean, so the setting does nothing there.

**Where it lives.**

- `merge-core` stays free of tree-sitter. `merge_core::boundaries` holds `Cuts`, the offsets inside nodes, and the `Boundaries` trait, which gives a region's `node_cuts` and `token_cuts`. The alignment takes an optional `&dyn Boundaries`; none keeps today's behaviour.
- `merge_view::boundaries::SyntaxBoundaries` implements it, parsing each region's window, never the whole file, so the cost follows the conflicts' size (§B6). The grammars move from `merge-app`'s gpui-kit registry to the grammar crates in `merge-view`, at the versions gpui-component links, so the build holds one copy of each and `merge-cli`'s headless modes get them too.
- Setting `syntax-boundaries`, `false` by default, and the flag `--syntax-boundaries`; the journal records it, with `JOURNAL_VERSION` 7. The status bar marks it like the other refinements (§B5.6).

**Tasks,** one commit each:

1. Move language detection and the grammars into `merge-view`, with no GPUI; `merge-app` takes them from there. The highlighter is unchanged. `Language::grammar` gives each grammar from its crate, at the version gpui-component links; a test shows the registry holds the same one.
2. `Cuts` and `Boundaries` in `merge-core`, and `SyntaxBoundaries` in `merge-view`: the rule above, on hand-written Rust, Go, Python and HTML snippets, with expected cuts from hand calculation.
3. Deep refine filtered by `Boundaries`. Tests use a fake `Boundaries` over hand-marked offsets. `merge3::align_bounded`.
4. Word merge filtered by it, with the same fake. `merge3::word_merge_bounded`; a side parses only when a hunk resolves from it.
5. Setting, flag, journal version 7, status bar mark, spec §4.3, §5 and §6.4. Fixtures in `fixtures/core`: a shared `}` split rejected; a word merge inside a string literal rejected; a merge in a file of unknown language unchanged. `MergeDocument::new_bounded` and `Journal::restore_bounded`; the language comes from the file names, as for colours.
6. Benchmark: `load-word-small` and a deep-refine case with the setting on, against spec §9. `merge-view`'s `boundaries` bench (§B6).

### B6 Benchmarks

`cargo bench -p merge-core --bench merge` gains jj mode and word mode. `script/check-bench` gates both against spec §9. Word mode runs only inside conflicts, so its cost depends on the conflicts' size, not the file's; the benchmark uses a file with many small conflicts and one with a large one.

jj mode, 2026-10-28, on an M4 Pro, release build, seeded generator:

| Measure | 100k lines | 1M lines |
|---|---|---|
| `load-jj` | 41 ms | 503 ms |
| `align-jj` | 34 ms | 437 ms |
| Peak heap of a session, `session-jj` (`memory` bench) | 56 MB | 607 MB |

git mode on the same run: `load` 24 ms and 447 ms, sessions 32 MB and 362 MB. `script/check-bench` gates `load-jj` against spec §9's load target and `session-jj` against its memory target.

Word mode, jj semantics at the word level, on `testing::generate_word_conflicts`. `small` puts a one-line conflict every 20 lines, `large` makes the whole file one conflict. Every conflict resolves word by word.

| Measure | 10k lines | 100k lines | 1M lines |
|---|---|---|---|
| `load-word-small` | 5 ms | 52 ms | 601 ms |
| `load-word-large` | 158 ms | 1.58 s | 15.4 s |

`align_merged` word-merges each conflict once, the conflicts in parallel on rayon's pool, and keeps each text for `MergeDocument::new` to render. A later render, after undo or Clear Choice, merges that one region again. `script/check-bench` gates `load-word-small` against spec §9's load target.

`load-word-large` misses the target from 100k lines. One conflict is one call to `jj-core`'s `ContentDiff::by_word`, about 15 µs per line, and a profile puts nearly every sample there: its histogram, hashing and allocation. jj's own word merge of the same hunk makes the same call. Spec §9 exempts the case, decided by the owner, 2026-10-28 (§B7, decision 5). `script/check-bench` holds it below 2 s at 100k lines and 20 s at 1M, about its measured cost.

Syntax boundaries, 2026-11-04, on an M4 Pro, release build: `cargo bench -p merge-view --bench boundaries`. The generated triples are not valid Rust, so the bench writes its own: four-line functions, each followed by a blank line, and a conflict in every fifth function, about one every 20 lines. `word-small` renames a function on one side and changes its operator on the other, a word merge that cuts no token. `deep` wraps a call in a different block on each side, so the blocks' closing `}` lines match and deep refinement splits there unless the setting keeps the conflict whole. Both run under jj's semantics.

| Measure | 100k lines | 1M lines |
|---|---|---|
| `load-word-small-rust`, setting off | 43 ms | 467 ms |
| `load-word-small-rust-bounded` | 47 ms | 512 ms |
| `load-deep-rust`, setting off | 45 ms | 482 ms |
| `load-deep-rust-bounded` | 91 ms | 949 ms |

The deep case parses each conflict's window in both sides: two parses of one function, about 9 µs a conflict, one conflict after another. Each thread keeps one parser; building one per parse costs 4% more. The word case parses a side only when a hunk resolves from it. `script/check-bench` gates both `bounded` runs against spec §9's load target.

git's semantics are slow on this input whatever the refinement: the headless merge of the 100k-line `deep` triple takes 1.5 s under git's semantics and 70 ms under jj's. The time goes to `histogram::diff` over the whole file. Its unchanged runs between conflicts are all about the same length, and each region's anchor is the first such run, so each split takes about one function off the front and the recursion goes about 5 000 levels deep over the file. git's `xhistogram.c` picks anchors the same way: `git merge-file --diff-algorithm=histogram` takes 1.27 s on the same files. The hunks must match git's (spec §4.2). A faster histogram that gives the same hunks is open work.

### B7 Decisions

1. Decided by the spike, 2026-10-28: depend on `jj-core` and re-implement `files.rs`'s hunk resolution in `merge-core` (§B5.1). The spike met spec §9.
2. Decided by the owner, 2026-10-27: a repository with `.jj` is a jj repository and merges with jj semantics. triad does not support git merges there; `--merge-semantics git` in a jj repository is an error (§B3).
3. Decided by the owner, 2026-10-28: word-merged regions in git mode are automatic, as in jj, when the user turns `hunk_level = word` on. They carry the distinct tone of §B5.4, and the status bar marks the word level as a divergence from git (§B5.6).
4. Decided by the owner, 2026-10-28: `ignore-whitespace` in jj mode is allowed, built as in §B4 with spec §4.2's output rule, and the status bar marks any mode other than `none` (§B5.6).
5. Decided by the owner, 2026-10-28: a word-level merge of one conflict as large as the file follows jj's cost and has no spec §9 target (§B6). A port of jj's word diff gains too little, and a size cap on word merging breaks parity with jj.
6. Decided by the owner, 2026-10-29: syntax-aware boundaries (§B5.7, item 5) wait until M7 lands (§A9). M7 brings tree-sitter and its grammars through gpui-component's highlighter and fixes their versions; `merge-core` then shares them, so the build holds one tree-sitter and one set of grammars. Added now, `merge-core` would pick both ahead of M7 and gain a C build per grammar. In jj mode the word merge matches jj byte for byte, so snapping is one more opt-in in either case.

7. Decided by the owner, 2026-11-04: syntax-aware boundaries only reject a deep-refine split or a word merge that cuts a syntax node; a rejected cut does not move to the nearest clean line. One setting, `syntax-boundaries`, covers both refinements, in git and jj mode, and the status bar marks it as a divergence from the VCS (§B5.8).

### B8 Implementation notes

1. §B5.1 spike and decision 1.
2. §B5.2 jj corpus.
3. §B5.3 jj mode in `merge-core`. jj mode matches every line-level case of the jj corpus hunk for hunk.
4. §B5.5 mode selection and jj config, built without `--hunk-level` and `merge.hunk-level`, which move to §B5.4 with the word-level merge they select. Detection looks for `.jj` and `.git` by name instead of calling `native::discover`: it cannot fail on an unreadable repository and costs no repository load. The jj config reader sits in `merge-cli` (`jj_config`), beside `install`'s config paths, which it shares, so `vcs-jj` is unchanged. It reads system config and secure repo and workspace config (`config-id`) as jj 0.46 does, and only reads.
5. §B5.4 word-level merge, with journal version 4. jj mode matches every case of the jj corpus in all four modes, hunk for hunk. `Resolution::Merged` is not needed: a `WordMerged` region holds `Resolution::Auto`, and its text is computed from the inputs by `merge3::word_merge` whenever it is rendered, so regions carry no text and undo returns to it as to any automatic text. The journal records `hunk_level` in its options and replays word-merged regions by aligning again. Three jj seeds pin the word merge (`jj-word-same-word`, `jj-word-two-lines`, `jj-word-spaces`). `--hunk-level`, the `hunk-level` setting and jj's `merge.hunk-level` land here (§B5.5).
6. §B5.6 status bar and settings. The status bar leads with `git merge` or `jj merge`, `by word` at the word level, then one item per divergence. The word level under git semantics also counts as a divergence: git has no word-level merge. The semantics, same-change rule and hunk level are chosen once per run (§B5.5), so a change to them in the settings file applies from the next session; the other alignment settings re-align as spec §6.4 says.
7. §B6 benchmarks, alongside 3 and 5. One conflict as large as the file is exempt from spec §9 (§B7, decision 5).
8. §B5.7 refinements, one task each. Insert/insert: `merge_core::suggest` names the suggestion, and the result pane's menu lists it first as `Suggested: Left + Right`. Sorted-block union: `Suggested: Sorted Union` and the Merge menu's Take Sorted Union replace the region's text with the union as an edit, so undo and the journal need nothing new. Deep refine: `Refine::Deep` (spec §4.3), with journal version 5. It only splits conflicts; no piece resolves, since a piece's base range rests on an open choice of cut. Syntax-aware boundaries deferred until after M7, 2026-10-29 (§B7, decision 6), then planned in §B5.8, 2026-11-04 (decision 7). Moved blocks: `Suggestion::CarryEdit` and the Merge menu's Carry Edit to Moved Block, in three-way merges only. A move is a conflict where one side deletes base's lines and the other edits them, and the deleting side inserts the same lines once elsewhere, in a region that resolves to its text. The suggestion's label names the result line the block moved to; carrying is one edit of both regions.

## C. Chunk origin (M5)

### C1 Goal

- **Each line names the commit that last changed it**, where the VCS records one: the commit, jj's change ID, the author, the author's date and the summary. A hover card shows them; an optional gutter column shows the short ID at the first line of each run.
- **Lines the VCS has not recorded show no origin.** These are lines that exist only in the working copy, the index or a temporary file, and inputs the VCS cannot place (§C3).
- **The window never waits for origins.** Annotation runs off the UI thread after the window shows. Its cost grows with history, not with file size alone.
- **git and jj give the same types and the same view.** jj adds change IDs; git leaves them empty.

### C2 How the backends annotate

- **gix.** `Repository::blame_file(path, commit, options)`, behind gix's `blame` feature, which adds the `gix-blame` crate (MIT or Apache-2.0) and gix's `blob-diff`. It starts at a commit and walks every parent. Each `BlameEntry` holds a run of lines in the blamed file and the commit that introduced it. Rename tracking (`rewrites`) is off unless asked for. The diff algorithm comes from `diff.algorithm` unless given.
- **jj-lib.** `annotate::FileAnnotator::from_commit(commit, path)`, then `compute(repo, domain)`. Each line maps to `Ok(origin)`, or to `Err(origin)` when its true origin lies outside `domain`; with `RevsetExpression::all()` every line resolves. `FileAnnotator::with_file_content` annotates text that no commit holds, and gives its changed lines the starting commit. jj-lib tracks no renames.
- Both calls block and are synchronous to the caller (jj-lib's are `async`, run with `pollster` as elsewhere in `vcs-jj`).

### C3 Which inputs can be annotated

An input gets origins from a revision only when the file at that revision holds exactly the input's bytes. Otherwise it gets none. This one check covers renames, a `jj resolve -r` on a revision triad cannot see, a rebase stopped part way, and inputs the user edited before the run. Origins are also limited to UTF-8 inputs, whose lines split where the VCS's lines split (spec §4.1).

| Run | Path in the repository | Revision per input |
|---|---|---|
| `diff PATH` (spec §6.1) | PATH | left: `--rev`, or the default parent; right: the working copy, annotated against the default parent, with changed lines unattributed |
| `git mergetool` | `$MERGED`, the output path, in the working tree | left: `HEAD`; right: the first of `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD` and `REBASE_HEAD` that exists; base: their merge base |
| `jj resolve` | `$path`, which the install config passes as `--title` (spec §7.1), in the repository holding the working directory | each side and base: the first of `@` and its parents whose file holds the input's bytes |
| `resolve` (M6, spec §7.4) | its own | the terms of the conflict triad reads |

The right side of `diff PATH` and the working copy in jj use `with_file_content` on jj and an annotation of the parent plus a line diff on git, which gives the same result: lines the parent holds keep its origins.

### C4 Types

`vcs` gains:

```rust
pub struct Timestamp { pub seconds: i64, pub offset_seconds: i32 }

pub struct LineOrigin {
    pub change: ChangeInfo,   // commit, jj change ID, summary
    pub author: String,
    pub time: Timestamp,      // the author's
}

/// The origins of one file's lines. Each commit appears once in `origins`.
pub struct Annotation {
    pub origins: Vec<LineOrigin>,
    pub runs: Vec<OriginRun>, // in order, covering every line
}

pub struct OriginRun { pub lines: Range<usize>, pub origin: usize }
```

`Annotation::origin(line)` finds a line's run by binary search. One `LineOrigin` per commit, not per line, keeps a 100k-line file with few commits small.

`Repository::annotate(path, commit: &CommitId) -> Result<Option<Annotation>, VcsError>`: the annotation of `path` at `commit`, or `None` when the file does not exist there. Errors as for `file_at` (§A13.10).

`Repository::commits(rev) -> Result<Vec<String>, VcsError>`: the full IDs of the commits `rev` names. `Repository::merge_base(one, two) -> Result<Option<String>, VcsError>`: their nearest common ancestor; `None` when there is none or several. §C5.2 needs both: it pins each input to one full commit ID, and git's merge state names no merge base.

### C5 Phases

#### C5.1 `annotate` on both backends

Tests first, in the conformance suite (spec §8.8), on git, jj and internal jj repositories:

- Two commits, the second changing one line and adding one: the lines' summaries, in order, from hand calculation; lines from one commit share one origin.
- A line changed in three commits names the last one.
- A merge commit: lines from each parent name the commit on that parent that wrote them.
- Author and time match the fixture's fixed signature.
- A missing path gives `None`; an unknown revision gives `UnknownRevision`; a file conflicted at the revision gives `Conflicted` (jj).

Work: gix's `blame` feature in `vcs-git`; `FileAnnotator` in `vcs-jj`; `LineOrigin`, `Annotation` and the trait method in `vcs`.

#### C5.2 Which revision each input comes from

Tests first, one per row of §C3, in `merge-cli` against gix- and jj-lib-built repositories: a git merge state (`MERGE_HEAD` written by the fixture), a cherry-pick and a rebase state, a jj conflict at `@`, an input that matches no revision, a non-UTF-8 input.

Work: `merge_cli::origin` turns the run's paths and the repository into one `Option<(path, rev)>` per input, with the byte check. Spec §7.3 takes §C3's table.

#### C5.3 View model

Tests first: `merge-view::origin` maps each side pane's rows to runs, filler rows to none; the gutter column shows a short ID at each run's first row; hover text from hand-written origins. The result pane's lines take the origin of the input their region's pick takes them from; edited text has none.

#### C5.4 Window

Tests first, headless: origins arrive after the window shows and fill the hover card and the column; a run with no repository shows none and no error; the View menu's Origin Column toggle and the settings file's `show-origin-column` (default `false`, spec §6.4).

Tests first, diff window: an edit drops the origins of the lines it changes and moves the rest; undo brings them back; a reload from disk keeps the origins of the lines the file still holds.

Work: a background task per input; a failure is logged with `tracing` and leaves that input without origins. A `Changed` input takes its revision's annotation carried onto its text with `merge_view::origin::carry`. The diff window carries each pane's annotation from its revision's text onto the pane's current text each time it recomputes regions, in the same background step (decision 3).

#### C5.5 Cost

Measure `annotate` on generated histories of 1,000 and 10,000 commits touching one file, on both backends, and record the result here. Spec §9 gains a target only if the owner sets one.

Measured, 2026-10-29, on an Apple M4 Pro, release build, `cargo bench -p merge-cli --bench annotate`. One 100-line file; each commit after the first changes one line in turn, and line 0 only ever comes from the first commit, so every annotation walks the whole history. Mean time per annotation:

| Commits | git (gix-blame) | jj (jj-lib) |
|---|---|---|
| 1,000 | 213 ms | 78 ms |
| 10,000 | 1.93 s | 890 ms |

The cost is linear in the commits walked: about 0.2 ms each on git and 0.09 ms on jj. It runs off the UI thread, so a long history delays the origins, not the window. Spec §9 has no target for it.

#### C5.6 Tool-mode diff sources

Tests first, in `merge-cli` against gix- and jj-lib-built repositories: `git difftool` of the working tree, of staged changes and of two commits; `jj diff --tool` of `@` against `@-`; a side that matches nothing.

Work: `git difftool` and `jj diff --tool` pass temporary copies, so `diff LEFT RIGHT` finds sources by the byte check of §C3 alone, with the repository of the working directory and the path from the window title or the right side's path. git tries `HEAD`, then the index's parent `HEAD` by a line diff, and the working tree; jj tries `@-` and `@`. Spec §7.3 gains the row.

#### C5.7 Revision chips in the pane headings

Each pane heading names the commit its input shows, after the title (spec §5). The commit is that of an exact source; the left side of `diff PATH` takes the revision it is read from, so a missing file or a non-UTF-8 revision still names it; jj's working copy takes `@`. git's working tree and index name none. `merge_cli::origin::Found` holds them as `revisions`, and `Found::describe` reads each through `Repository::describe(&CommitId)`, which both backends implement and the conformance suite checks. Both windows load them with the line origins, off the UI thread.

The `revision-id` setting (`change`, `commit`; default `change`) picks the ID jj shows, in the chip and the origin column alike. git shows commits under either. `diff PATH` titles its left pane by the revision's label, else the revision (`@-`), so both panes read as title then chip. A title that starts with one of the commit's IDs, as a git or jj conflict term without a label has (`Term::title`: `<id> <summary>`), shows without it beside the chip (`merge_view::origin::revision_heading`); the markers keep the whole label.

### C6 Decisions

1. Decided by the owner, 2026-10-29: the result pane shows origins, through the region's pick (§C5.3).
2. Decided by the owner, 2026-10-29: no rename tracking in git. jj-lib has none, and the byte check of §C3 already drops inputs a rename moved.
3. Decided by the owner, 2026-10-29: the diff window's panes take origins by a line diff of the revision's text against the pane's current text, run with each recompute of regions. A line map shifted by each edit needs its own handling of undo, redo and reloads; origins hidden after the first edit lose the working copy, the pane users edit; a fresh `annotate` per pause costs history, not file size.

### C7 Implementation notes

1. §C5.1. gix's `blame` feature is on in `vcs-git` only; it adds `gix-blame` 0.18 to the tree. jj's `Err` origins, which name the commit where a search stopped outside its domain, cannot occur over every commit; `vcs-jj` treats them as the origin. A jj revision naming several commits is `AmbiguousRevision`. The conformance fixtures gain `fork_and_merge` (git: `Fixture::merge` and `Fixture::commit_on`). §C5.5's measurement is still open.
2. §C5.2. `merge_cli::origin::{merge, diff}` give one `Option<Source>` per input: the path, a full commit ID and a `Fit`, `Exact` for the byte check or `Changed` for a line diff. Spec §7.3 holds the table, which refines §C3's: git's right side and base follow the stopped operation (a revert's right side is `REVERT_HEAD^` and its base `REVERT_HEAD`; a cherry-pick's and a rebase's base is the commit's parent), jj also tries the merge base of `@`'s two parents, which gives a two-sided conflict at `@` its base, and `diff PATH` against git's index annotates both sides against `HEAD`. The row follows the repository's kind, not the semantics. `vcs` gains `commits` and `merge_base`, in the conformance suite.
3. §C5.3. `merge_view::origin::Origins` holds one optional `Annotation` per input; lines no run covers have no origin, which fits a `Changed` annotation. `shown_from` gives the input lines a result line shows, in order of preference; an unchanged line shows base first, so a base without origins falls back to a side. A run's label goes on its first line: the line above comes from another commit or has none. Decision 1 is taken as §C5.3 assumed. The two-way window maps a pane's line straight to its annotation while the pane is unedited; how edits shift that map is left to §C5.4.
4. `merge_view::origin::carry`, for §C5.4 and decision 3. a byte-exact line diff, so a line changed only in whitespace loses its origin. `Annotation` runs may leave lines uncovered.
5. §C5.6. `merge_cli::origin::diff_tool`. `jj_diff_copy`, which the diff window's banner also uses, gives the repository path below jj's side directory. git's difftool command passes `--title "$MERGED"`, which names the file when both sides are copies, as in `git difftool --cached` and diffs of two commits. The jj fixture gains `edit`, as `jj edit`.
6. §C5.4. Loading: `merge_cli::origin::find_merge`, `find_diff` and `find_diff_tool` keep the repository they open, and `Found::annotate` annotates each source, one result per input, carrying a `Changed` one onto the input's text. A revision that is not text gives no annotation. The settings file gains `show-origin-column`. Merge window: a `git mergetool` or `jj resolve` run passes its output, title and working directory as `OriginLookup`; the window loads the origins on the background executor and shows them in the top panes' origin column, with a hover card on each cell, toggled by the View menu's Origin Column. Diff window: `diff PATH` passes `DiffLookup::Native`, `diff LEFT RIGHT` passes `DiffLookup::Tool`; each pane keeps its annotation and the text it describes, and `Recompute::run_with` carries it onto the pane's text in the recompute's background step, so edits, undo, reloads and navigation's immediate recompute all update the origins. Editor gutters: `vendor/gpui-base` gains line labels (`EditorState::set_line_labels`, see its `PATCHES.md`), and the result pane and the diff panes label each run that `merge_view::origin::commit_runs` finds, with a hover card. §C5.4 is done. The hover cards themselves are GPUI tooltips, which the headless tests do not open; the labels' text and lines, and the text each hover shows, are tested.
7. §C5.5. the table in §C5.5. M5 is complete.

## D. Native resolution (M6)

### D1 Goal

- **`triad-merge resolve [PATH]`** opens a conflicted file straight from the repository, with no merge tool between. With no path, it lists the conflicted files and opens them in turn.
- **git:** the conflict comes from the index's stages 1, 2 and 3. **jj:** it comes from the working-copy commit's tree, as a `Merge` of n sides and n − 1 bases.
- **Each term carries what the VCS knows:** git's `HEAD`, `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD` or `REBASE_HEAD` (the rules of spec §7.3), jj's conflict labels (`ConflictLabels`, jj-lib 0.46).
- **A jj conflict of more than two sides opens with one top pane per side.** The engine and the window already take n sides (`Inputs`, `PaneSet::Sides`).
- **The merge mode follows the backend** (part B): git's in a git repository, jj's in a jj one.

### D2 Writing to the repository

Release 1 runs on gix 0.88, which jj-lib 0.46 requires. GHSA-q54r-jf2p-rfr2 affects gix 0.88 to 0.89: its checkout, index creation, merge and tree editing skip the trust filter on `core.protectHFS`, `core.protectNTFS` and `gitoxide.core.protectWindows`. §A7.1 bars any repository write until a jj-lib release on gix 0.90 or newer exists. jj-lib 0.47.1 is that release; triad moves to it on 2026-11-05.

M6 therefore splits:

- **Reading** (§D5.1–§D5.3) needs no write and goes ahead now.
- **Saving to the working copy** (§D5.4) is a plain file write, as any editor makes. git keeps the conflict stages until `git add`. jj snapshots the file on its next command and parses jj's markers back into a conflict, so a save with markers stays a conflict and a save without them resolves it.
- **Writing the repository** (§D5.5): git's stage-0 entry in the index, as `git add` does, and jj's new tree on the working-copy commit. It waits for jj-lib on gix 0.90 or newer.

### D3 What can be resolved

`read_conflict` reads each term's entry. triad merges text, so:

| Terms | Result |
|---|---|
| Files, each text in one encoding (spec §4.1) | The conflict opens. An absent term (added on one side, deleted on another) is an empty input. |
| A binary file among the terms | Refused: the file is not text. |
| A symlink, submodule or directory among the terms | Refused: no text merge resolves it. |
| Every side absent: a delete/delete, or the old name of a file renamed two ways | Refused: the resolution is a deletion, which the VCS records. |

The executable bit never conflicts alone. git and jj merge it apart from the bytes, and a merge of one bit always resolves: two sides that set it make the same change, which jj counts once. A conflict whose terms differ in the bit is a conflict in the bytes, and each term carries its own bit.

`read_conflict` reports each term's kind and bytes as the VCS holds them; `merge_cli::resolve` decides what it refuses, and its error names the path and the reason.

`conflicts()` lists every conflicted path, whatever its kind, so `resolve` can say why it skips one. Without a PATH, `resolve` skips a refused file, prints the reason on stderr, and counts it as unresolved; with a PATH, a refusal exits with 3.

### D4 Types

`vcs` gains:

```rust
/// One side or base of a conflict.
pub struct ConflictTerm {
    pub term: Term,
    pub content: TermContent,
}

pub enum TermContent {
    File { bytes: Vec<u8>, executable: bool },
    Absent,
    /// A symlink, submodule or directory: no text to merge.
    Other(EntryKind),
}

/// A conflicted file: n sides and n − 1 bases, in jj's order; git gives two sides (stage 2,
/// stage 3) and one base (stage 1).
pub struct Conflict {
    pub path: Utf8PathBuf,
    pub sides: Vec<ConflictTerm>,
    pub bases: Vec<ConflictTerm>,
}
```

`Repository` gains:

- `conflicts() -> Result<Vec<Utf8PathBuf>, VcsError>`: the conflicted paths in the working copy, sorted. git: paths with index stages above 0. jj: the working-copy commit's conflicted paths.
- `read_conflict(path) -> Result<Option<Conflict>, VcsError>`: `None` when `path` is not conflicted.

### D5 Phases

#### D5.1 Conflicts on both backends

Tests first, in the conformance suite (spec §8.8), on git, jj and internal jj repositories:

- No conflicts gives an empty list.
- One conflicted file among clean files gives that path.
- Several, in nested directories, come sorted.
- A jj conflict of three sides is listed once.
- git: a path with stages 1 and 2 only (modify/delete) is listed.

Work: `conflicts()`; the types of §D4 come with §D5.2, which first reads them. The git fixture gains conflict stages in the index: test code may write the index (the advisory is about checkout and trust, which a fixture in a temporary directory never meets). The jj fixture already builds conflicts at `@`.

#### D5.2 Reading a conflict

Tests first, in the conformance suite, with the case list of §A9 M6 from libgit2's tree-merge tests, rebuilt through gix and jj-lib (licence as in §A4.8):

- modify/modify: base, left and right bytes, from hand calculation.
- add/add: an absent base.
- modify/delete, both ways: one absent side.
- the executable bit: a content conflict where the terms differ in it gives each term's bit. jj: sides that differ only in the bit merge cleanly (§D3).
- a symlink or a submodule on one side: `TermContent::Other`. jj: a directory against files is a conflict at the directory's path with a `Directory` term. git's index holds no directory at a conflict stage.
- a binary file: its bytes unchanged.
- rename/modify and rename/rename: git's stages from `gix-merge`'s tree merge (gix's `merge` feature, in `vcs-git`'s dev-dependencies only); jj tracks no renames.
- a criss-cross: git's stage 1 is the virtual merge base `gix-merge` builds; jj gives several bases.
- jj: three sides and two bases, in jj's order.
- A path that is not conflicted gives `None`.

Then the terms: git labels from the stopped operation (spec §7.3's table), jj labels from `ConflictLabels`, and each term's `ChangeInfo` where the VCS has one.

#### D5.3 `resolve` opens the window

The command line:

```
triad-merge resolve [PATH] [--diff-algorithm A] [--ignore-whitespace W] [--merge-semantics S]
                    [--same-change C] [--hunk-level H] [--marker-length N]
```

- The repository is the working directory's, found as `native::discover` finds it. PATH is relative to the working directory and must lie inside the repository. Outside any repository, the repository is PATH's, so a conflicted file opens from the VCS wherever `resolve` runs.
- The inputs are the conflict's terms, sides then bases in the VCS's order. An absent term is an empty text. Each term's label is `Term::title`, or empty without one; the labels title the panes and the markers. A pane without a label falls back to Left, Base and Right, or Side n and Base n.
- The window's title is PATH relative to the repository root. Its output is the working-tree file, so Save & Exit writes there (§D5.4 adds its tests).
- The semantics are chosen as for a merge whose output is the working-tree file: `auto` follows the repository, so git's in git and jj's in jj. Flags and settings override, as for a merge.
- With no PATH, `resolve` prints the conflicted paths, relative to the repository root, one per line, on stdout, and opens the first. With none, it prints `no conflicted files` and exits with 0. §D5.4 opens the rest in turn.
- Exit 3 with a message for: no repository at the working directory; PATH outside the repository; PATH not conflicted (§D5.7 then reads markers); a term that does not decode as text, naming the term (`base`, `side 2`, or its title); a term that is a symlink, submodule or directory, naming the term and the kind.

Tests first:

- `merge_cli::resolve::load`, on git, jj and internal jj repositories: a modify/modify conflict gives base, left and right texts and the terms' titles as labels; add/add gives an empty base; modify/delete an empty side; a three-sided jj conflict gives three sides and two bases in jj's order; a path given from a subdirectory resolves against it.
- Its errors: outside a repository, a path outside the repository, a clean path, a binary term, a symlink term, each with its variant and message.
- `merge_cli::run` with `resolve` and no path in a repository with conflicts: the list on stdout and a window form for the first; with none, the message and exit 0.
- Argument parsing: the flags, and `resolve` with two paths rejected.
- The binary (not Windows, where the window binary loads): `resolve` on a clean path exits with 3 before any window.

Work: `Invocation::Resolve`, `merge_cli::resolve`, and `merge_app::run` opening the merge window from it.

#### D5.4 Saving to the working copy

§D5.5's first snapshot, done 2026-11-09, replaces the unrecorded-edits check below (decision 3), and its stale-working-copy check replaces the stale-record refusal. jj's working-copy state (`TreeState::file_states`: each file's mtime, size and materialised marker length as of the last snapshot) still gives the marker length. Before §D5.5 these checks stood in for the snapshot:

- **Unrecorded edits.** `resolve` reads the conflict from the working-copy commit, which holds the file as of jj's last snapshot. A file whose mtime or size differs from jj's record holds edits the conflict lacks, and a save would destroy them. `resolve` refuses it with exit 3: `PATH has changes jj has not recorded; run a jj command, such as jj status, and try again`. A file jj has no record of, or one deleted from the working tree, passes. A record whose value in the working copy's tree differs from the working-copy commit's is stale: `@` moved without updating the files, as from another workspace. `resolve` refuses it with exit 3: `PATH on disk is not the working-copy commit's; run jj workspace update-stale and try again`.
- **Resolved in the working tree** (both VCSs, added 2026-11-06). git's stages keep the conflict until `git add`, so a file resolved by hand, or by an earlier resolved save, still lists as conflicted, and its stages lack the resolution. When every side is a file the VCS wrote markers, so a working-tree file that is text without a conflict-opening line holds a resolution. `resolve` refuses it as a refusal (spec §7.4): `PATH has no conflict markers left in the working tree, so it looks resolved; mark it resolved, or put its markers back to merge it again`. A file that is not text is refused too, since it may hold a resolution in another encoding; an unreadable file is an error. A missing file passes, as does a conflict with an absent side: git writes a modify/delete conflict's surviving side without markers. Under jj the unrecorded-edits check fires first; a recorded conflict always has markers on disk.
- **Edited beside the markers** (git, added 2026-11-06). A file with markers may hold edits, or conflicts resolved by hand, that git's stages lack. When the VCS has no record of the file, `resolve` parses the file's markers and triad's git-semantics merge of the stages, and compares the sides each rebuilds. A match loads from the stages, with their labels and bases; a difference opens from the file's markers (spec §7.4), keeping the edits at the cost of an empty base where git wrote the `merge` style. A byte comparison would fail on git's labels, attributes, filters and `-X` options; the side comparison ignores labels, markers and zdiff3's trimming. Markers that do not parse are a refusal. A hunk placement that differs from git's, or a line-ending filter, opens from the markers: harmless, since the file holds the same conflict.
- **Marker length.** jj parses only markers at least as long as the length it materialised the file with. The run's marker length is the larger of the requested length and jj's recorded length, or jj's choice for the terms (`choose_materialized_conflict_marker_len`) without a record.

`--headless` with `resolve` writes the automatic merge, with `--commands` applied, to the working-tree file and exits as a headless merge does: 0 resolved, 1 with markers.

With no PATH, the window opens each conflicted file in turn. Save & Exit or a save with markers moves to the next file not yet opened, re-listed after each save; Cancel ends the run with 2. The run exits with 0 when every file resolved, else 1.

Tests first:

- `resolve --headless`: the working-tree file gets the merge in the inputs' encoding (spec §4.1); exit 0 clean, 1 with markers; git's stages are unchanged after either.
- jj: a save with markers is read back by jj-lib's `update_from_content` with jj's recorded marker length into the same sides and bases; inputs with marker-like lines make jj's length longer than triad's own rule.
- jj: a file edited after the last snapshot is refused with the message; a file as jj left it passes.
- git: a file without markers is refused, and `resolve` without a path skips it and leaves it byte for byte; a file with markers, a missing or binary file, and a modify/delete conflict load from the stages.
- The window loop, in `merge-app`'s window tests: after Save & Exit on the first file the second opens; Cancel finishes with 2; the last save finishes with the aggregate code.

Work: `vcs::Repository::working_copy_state(path)` (jj: mtime, size, marker length from the tree state; git: none), `resolve::load` checking it, `--headless` for `resolve`, and the window loop in `merge_app::run`.

#### D5.5 Writing the repository

Done 2026-11-09 (§D7 note 5). jj's writes are two snapshots of the working copy, as `jj resolve` makes (decisions 1 and 3):

- **Before.** `resolve` snapshots the working copy before it reads the conflict, in its own operation. The user's unrecorded edits, in the target file and elsewhere, land there. This snapshot replaces §D5.4's unrecorded-edits refusal: the conflict is read from the commit the snapshot leaves. The stale-working-copy refusal stays. A snapshot that leaves the path clean, because the user resolved it by hand, exits as a clean path does.
- **After.** Each save snapshots the working copy again, in a second operation. `jj undo` then reverts the resolution alone and leaves the user's edits in place.
- **Settings.** Both snapshots take the user's jj config, loaded through jj-lib as the CLI loads it: `snapshot.auto-track`, `snapshot.max-new-file-size`, `fsmonitor`, and the ignore files. A file the user's jj would not track stays untracked.
- A run cancelled after the first snapshot leaves that operation, as `jj status` does.
- A save whose markers read back as the conflict jj holds leaves the tree as it was and writes no operation.
- A moved git `HEAD` is imported first, in an operation `import git head`, as jj-cli's `import_git_head` does: a new working-copy commit on `HEAD`, and the working copy's record reset to it without touching the files. jj-cli also imports git's other refs after its snapshot; `resolve` leaves them to jj's next command, which imports them the same way.
- jj-cli offers to update a stale working copy; `resolve` refuses it before any operation, and names `jj workspace update-stale`.
- Cost, measured 2026-11-09 with `cargo bench -p merge-cli --bench resolve` on an M4 Pro, release: with 100k files, loading a conflict takes 105 ms, listing the conflicts 99 ms and recording a save 168 ms; with 10k files, 12, 11 and 52 ms. `jj util snapshot` on the same shape takes 108 ms unchanged and 176 ms after one edit, so the cost is jj's scan. git loads in 6 ms and stages in 30 ms at 100k entries. Save & Exit writes and records off the UI thread, and so do the queue's list and load (§A13.11). A load right after the list or a save skips its scan (§A13.11 item 7). `jj resolve` pays the same scan before it starts a tool. A watchman-backed fsmonitor would cut the scan; triad's jj-lib has none.

Tests first, in the conformance suite: a full resolution replaces git's stages with one stage-0 entry and leaves other entries as they were; jj's working-copy commit gets a tree with the file resolved and other paths unchanged; a save with markers leaves git's stages and jj's conflict in place.

jj only:

- An unrecorded edit to the target file and one to another file are in the first operation's tree; the second operation's tree differs from it only at the target path.
- Undoing the second operation restores the conflict and keeps both edits on disk.
- A new file matched by `.gitignore`, and one above `snapshot.max-new-file-size`, stay untracked; with `snapshot.auto-track = "none()"`, a new file stays untracked.
- A file the user resolved by hand before the run exits as clean, after the first snapshot.
- A stale working copy is refused before any snapshot, and the operation log is unchanged.
- Cancel leaves one new operation.

#### D5.6 Line origins in `resolve`

A `resolve` window looks its origins up as a merge does, from the working-tree file and its path from the root. The rules give each term's commit: git names its terms by the stopped operation, as `merge_cli::origin` does, and jj gives no commit per term, so `@`'s parents and their merge base stand in. Spec §7.3 gains the row. Tests as in §C5.2.

#### D5.7 Conflict markers

`resolve PATH` also opens a file whose conflict lives only in its markers: a file the VCS holds no conflict for, or one outside any repository. Markers committed by mistake, stages cleared by `git add`, and files from other tools all land here. A file whose VCS conflict exists always opens from the VCS (§D5.3), which loses nothing.

`merge_core::markers::parse` reads the markers. It is triad's own parser, in `merge-core`, with no VCS dependency. jj-lib's `parse_conflict` was the alternative; it rejects git's two-way style, needs the arity up front, and drops the labels.

Styles:

| Style | Writer | Sections |
|---|---|---|
| git `merge` | git's default | `<<<<<<<` left `=======` right `>>>>>>>`; no base |
| git `diff3`, `zdiff3`; jj `git` | git, triad (spec §4.4), jj | `<<<<<<<` left `\|\|\|\|\|\|\|` base `=======` right `>>>>>>>` |
| jj snapshot | jj, triad with more than two sides | `<<<<<<<`, then `+++++++` before each side and `-------` before each base, `>>>>>>>` |
| jj diff | jj's default | `<<<<<<<`, then `%%%%%%%` (with `\\\\\\\` note lines) before a diff from a base to a side, and `+++++++` before a plain side, `>>>>>>>` |

Rules:

- A marker is a run of one marker character, then whitespace or the line's end. The marker length is the longest run of `<`, at least 7, at which the markers parse; every marker has exactly that length. git, jj and triad write all markers of a file at one length, and jj and triad make them longer than any marker-like line inside, so a shorter marker-like line is content. A longer run that does not parse, such as a content line in a file git wrote without growing its markers, falls back to the next length.
- Every conflict must have the same number of sides. A start marker without an end, sections out of order, a diff line that does not start with ` `, `-` or `+`, or conflicts of different arity are errors naming the line.
- The inputs: each side's text is the text outside the conflicts with that side's section in each conflict; each base's text likewise. A conflict without a base section (git `merge`) adds nothing to the base, so it opens as an add/add conflict: the base is unknown.
- A section's last line keeps its newline unless the end marker's line has none, as jj reads it.
- The labels come from the first conflict's marker lines: the text after the marker and one space. jj's numbered defaults (`side #1`, `base #1`, `conflict 1 of 1`, `diff from …`) count as no label.
- The semantics follow the repository the file lies in, else git's.
- The output is the file itself.

Tests first, expected values by hand:

- Each style above, two-sided, with labels and without; jj's styles with three sides.
- A CRLF file; a file whose last conflict ends without a newline; a section that is empty; a content line of 7 `<` inside markers of 11.
- Text before, between and after conflicts reaches every input.
- Errors: no markers; an unclosed conflict; a `=======` before `|||||||`; mixed arity; a bad diff line.
- Cross-check against jj-lib in `merge-cli`'s tests: jj's `materialize_merge_result` of seeded merges in each jj style, with and without labels, two to four sides, LF and CRLF, parsed by triad, gives the texts jj's own `parse_conflict` gives, jj's marker length, and jj's labels.
- `resolve PATH`: a clean file with git diff3 markers in a git repository opens from the markers; outside a repository likewise; a clean file without markers exits with 3, `not conflicted`.

### D6 Decisions

1. Decided by the owner, 2026-10-29: §D5.4 saves to the working copy before the repository writes of §D5.5. git then needs `git add`, as after any editor. jj takes the file in at its next command's snapshot. triad taking the snapshot itself writes an operation and a commit through jj-lib, so it belongs to §D5.5, where it is jj's repository write: snapshot the working copy after the save.
2. Decided by the owner, 2026-10-29: mode-only conflicts and conflicts of non-files are refused (§D3). Neither VCS leaves a mode-only conflict (§D3), so the rule covers non-files only. Choosing an executable bit or a symlink target is not a text merge.
3. Decided by the owner, 2026-11-01: on jj, §D5.5 also snapshots the working copy before reading the conflict, in its own operation, as `jj resolve` does. A snapshot only after the save would put the user's unrecorded edits to other files and the resolution in one operation, and `jj undo` would revert both. The snapshots take the user's jj config, so triad tracks only the files the user's jj would.

### D7 Implementation notes

1. §D5.1. git lists the paths the index holds above stage 0; jj lists the working-copy commit's conflicted paths, in its tree order. `vcs_git::testing::Fixture::stage_conflicts` writes conflict stages, and the conformance suite's `stop_on_conflicts` stops a merge on both backends.
2. §D5.2. Part 1: the types of §D4 and `read_conflict` on both backends, for modify/modify, add/add, modify/delete both ways, binary bytes, a clean or missing path, and jj's three sides. Part 2: the terms. git's follow the stopped operation by spec §7.3's rules: ours is `HEAD` with its branch, and the base and theirs come from the first of `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD` and `REBASE_HEAD`; without one, only `HEAD` is known. jj's take the tree's `ConflictLabels`, which index the path's value term by term; jj gives no `ChangeInfo`. Part 3: `vcs::testing::Entry` gives the fixtures executable files, symlinks and submodules. Each term reads its own executable bit, and a symlink or submodule term reads as `TermContent::Other`, on git, jj and internal jj repositories; jj reads a directory term too. Part 4: `vcs-git`'s `tests/merge.rs` stops merges with `gix-merge` and reads them back. rename/modify conflicts at the new name with all three stages. rename/rename(1to2) follows git's `ort` layout: the base alone at the old name, and each side alone at the name it chose. A criss-cross takes the virtual merge base at stage 1, and its base term names no commit.
3. §D5.3, then §D5.7, then §D5.4. Decided by the owner, 2026-11-01: §D5.3 and §D5.7 go ahead of §D5.2 part 4. §D5.3: `merge_cli::resolve` loads a conflict's terms as inputs with their titles as labels, `merge_cli::resolve_target` lists the conflicted files, and `merge_app::run` opens the merge window on the working-tree file. §D5.7: `merge_core::markers::parse`, and `merge_cli::resolve::load` falling back to it for a clean file or one outside any repository. §D5.4: `Repository::recorded` reads jj's record of each working-tree file; `resolve` refuses unrecorded jj edits, saves at jj's marker length, runs `--headless`, and without a path opens each conflicted file in turn (`merge_app::resolve_queue`). The queue's windows do not follow the settings file. Then, from §D5.2 part 4: `resolve` refuses a conflict with every side absent, and the run without a PATH skips refused files. Then, after review: `resolve` outside any repository finds PATH's, and refuses a jj file whose record is of another version than `@`'s.
4. §D5.6. `merge_app::resolve_queue::window_settings` gives each `resolve` window the merge's origin lookup, and `merge_cli::origin` finds a working-tree file that does not exist through its nearest existing directory. A jj conflict whose sides come from commits other than `@`'s parents, as after a rebase, shows no origins.
5. §D5.5, when jj-lib allows. Part 1, on jj-lib 0.46 with no repository write: `vcs_jj::config` loads the user's jj config in jj-cli 0.46's layers (jj-lib's defaults, jj-cli's snapshot defaults, the host and user names, the system files, the user files or `JJ_CONFIG`'s list, the repository's and the workspace's secure config files, the `JJ_*` overrides), resolves `--when` scopes with no command, and loads the workspace with it. `SnapshotConfig` gives the snapshot options: `snapshot.auto-track` as a matcher with the layers' `[fileset-aliases]`, `snapshot.max-new-file-size` with 0 as no limit, and git's ignore files (`core.excludesFile`, else `<XDG config>/git/ignore`, then `info/exclude`). `fsmonitor.backend` is always `none`: triad's jj-lib has no watchman, and a scan finds the same changes. Part 2, done 2026-11-09 on jj-lib 0.47.1: `JjRepository::snapshot` follows jj-cli's working-copy snapshot: it imports a moved git `HEAD` first (`import_git_head`), refuses a stale working copy (`VcsError::StaleWorkspace`), reloads an updated one, snapshots with the config's options, and commits a rewrite of `@` with the new tree, rebasing descendants and marking new files intent-to-add in git's index; an internal workspace skips the import and the index. The first snapshot is marked as a snapshot operation; the save's is not, so `jj undo` reverts it. Neither writes an operation when the tree is unchanged. `GitRepository::record_save` stages a resolved path: the file through `convert_to_git`, a blob, a stat from the file, the mode from the file or, without `core.fileMode`, from ours, theirs, then the base; the stages go and the entry goes in, the tree extension is dropped, and the index is written under its lock. `resolve::load` snapshots before it reads; `resolve::Record::saved` reopens the repository and records a save, from `resolve --headless` and from the window's Save & Exit, where a failure offers Try Again. `merge_session::jj_config::snapshot_config` gives the run's `Env` to jj's config; `RunRepository::with_jj_config` and `Queue::with_jj_config` carry it.

## E. p4merge parity

This plan closes the gaps between triad and p4merge that matter for text merges. Each phase follows the ground rules of the implementation plan: tests first, expected values from this plan or the spec, one commit per task, and spec changes in the same commit as the code.

### E1 Scope

In scope:

| # | Gap | Phase |
|---|---|---|
| 1 | Options change only through a TOML file and CLI flags; most keys apply only at the next start | §E3 Config file |
| 2 | No overview of where differences and conflicts sit in the file | §E4 Overview ruler |
| 3 | The output may mix line endings, and no option sets one ending | §E5 Output line ending |
| 4 | Merge mode cannot write the result anywhere but OUTPUT | §E6 Save Copy As |
| 5 | Linux and Windows are not released | §E7 Cross-platform release (M4b) |
| 6 | No syntax highlighting | Part F |
| 7 | Colours, theme, line height, conflict style and the unresolved-save prompt are fixed | §E3.8 Appearance and save keys |
| 8 | Key bindings are fixed | §E9 Key bindings |

Out of scope:

- Image and binary diff (spec §1.3).
- Encodings other than UTF-8 and UTF-16 (spec decision 3).
- Directory diffs. git and jj call the tool once per file: `jj diff --tool` with `diff-invocation-mode = "file-by-file"`, which `install --jj` writes, and `git difftool` without `-d`.
- A settings dialog. The config file replaces it (§E3).

Native resolution (M6) is part D.

### E3 Config file

The settings move to a plain `key = value` text file in the style of Ghostty's config. The file is the settings UI: a menu item opens it in the user's editor. triad reads it once at startup, so a change applies from the next run; a merge or diff run is short, and a file watcher costs more than it gives.

#### E3.1 Format

```
# triad config
ignore-whitespace = trailing
font = JetBrains Mono
font-size = 12
refine-conflicts = deep
config-file = ?local.conf
```

- One setting per line: a key, `=`, and a value. Spaces around `=` and at the line's ends are trimmed. Keys are lowercase and case-sensitive.
- A line whose first non-blank character is `#` is a comment. A `#` after a value is part of the value.
- A value in double quotes loses the quotes. Quotes are needed only for leading or trailing spaces, and for an include path that starts with a literal `?`.
- An empty value (`font-size =`) sets the key back to its default.
- A key given twice takes the later value.
- `config-file = PATH` reads another file at that point, as if its lines stood there, so a later line overrides it. This is git's rule for `include.path`. Ghostty reads includes at the end of the file instead, so the file's own lines can never override them; triad takes git's rule because a later line winning is the rule for every other key. A relative path is relative to the including file. A leading `?` makes the file optional: a missing optional file is skipped, a missing required one is an error. A file that includes itself, directly or through others, is an error.
- Keys and values are those of spec §6.4, with bare words for strings: `refine-conflicts = deep`, `font = Iosevka Term`. Booleans are `true` and `false`.
- Encoding UTF-8, line endings `\n` or `\r\n`.

#### E3.2 Errors

Each bad line is reported as `PATH:LINE: message`: an unknown key, a value out of range or of the wrong kind, a line with no `=`, a missing required include, an include cycle. The other lines still apply, and a bad line leaves its key at the value before it. A file that cannot be read gives the defaults.

The merge and diff windows show the errors in a banner above the panes, with the alert role (spec §6.5), and log them with `tracing`. The banner closes with its close button. Headless runs and `inspect` print them on stderr, as now.

#### E3.3 Location

| Platform | Path |
|---|---|
| macOS, Linux | `$XDG_CONFIG_HOME/triad/config.triad`, or `~/.config/triad/config.triad` without `XDG_CONFIG_HOME` |
| Windows | `%APPDATA%\triad\config.triad` |

The `.triad` extension lets editors and the system's file handlers recognise the file; Ghostty renamed its file to `config.ghostty` for the same reason. One location per platform: Ghostty's second macOS location under `Application Support`, loaded after the XDG one, gives two files to keep apart for no gain.

`TRIAD_CONFIG` names another file and replaces `TRIAD_SETTINGS`. triad has had no release, so there is no migration from `settings.toml`.

macOS uses the XDG path, as Ghostty, git and jj do, because users keep their dotfiles there. The `directories` crate stays for the cache directory (spec §6.2).

#### E3.4 Startup

triad reads the file and its includes once, when it starts, before it parses the command line. Every key applies from the next run. Command-line flags hold over the file for the run (spec §6.4).

#### E3.5 Commands

- **Settings…** (⌘, on macOS, Ctrl+, elsewhere), in the triad menu: opens the file in the default text editor: `open -t` on macOS, `xdg-open` on Linux, Notepad on Windows. Launch Services finds no handler for `.triad`, so `open_with_system` cannot open it. A missing file is first written with every key commented out at its default, each with a one-line description. If no editor opens it, stderr names the file and the system file manager reveals it (`reveal_path`).
- `triad-merge show-config [--default] [--docs]`: prints the settings in effect, in file format, after includes and errors; `--default` prints the defaults; `--docs` adds each key's description as a comment above it. Exit 0, or 3 when the file has errors (printed on stderr).

- The command line takes only the five merge options (`--merge-semantics`, `--same-change`, `--hunk-level`, `--diff-algorithm`, `--ignore-whitespace`); each holds over the file for the run. Every other setting is in the file only, so `--help` stays short and a caller cannot change how the window looks. A run with other settings sets `TRIAD_CONFIG`, as the tests and the `fixtures/core` cases with a `config` file do.

The descriptions live in one table in `merge-config::settings`, which drives `--docs`, the file written by Settings… and the spec's §6.4 table.

#### E3.6 Code

- `merge-cli::config`: the parser. `parse(text, origin) -> (Vec<Entry>, Vec<ConfigError>)`, where an entry is a key, a value and its line. Includes resolve through a small trait for reading files, so tests use an in-memory map. `ConfigError` is a `thiserror` enum with a variant per case in §E3.2, each with the path and line.
- `merge-cli::settings`: builds `Settings` from entries, key by key, collecting errors. It drops serde for the settings file. `toml` stays for jj's config (spec §6.1).
- `merge-app::settings_banner`: the banner both window kinds show the startup errors in.

#### E3.7 Tests

Parser, with exact entries or errors:

- Empty file, comments only, blank lines with spaces: no entries.
- `key=value`, `key = value`, `  key   =   value  `: one entry, value `value`.
- `font = "  Mono  "`: value `  Mono  `. `font = a # b`: value `a # b`.
- `font-size =`: resets to 10 after `font-size = 12`.
- `font-size = 12` then `font-size = 14`: 14.
- `nonsense`: no `=`, error at line 1. `scrol-sync = false`: unknown key error naming `scrol-sync`, and `scroll-sync` stays `true`.
- `font-size = 5` and `font-size = 73`: out-of-range errors (spec range 6–72); `font-size = 6` and `= 72` read.
- `tab-width = 0`, `= 17` refused; `1` and `16` read.
- `refine-conflicts = deep`, `refine-conflicts = true`, `refine-conflicts = false` read; `refine-conflicts = shallow` refused.
- A bad line among good ones: the good ones apply.
- Includes: a relative include reads from the including file's directory; its keys apply at the include's position, so a key after the include overrides it and one before is overridden. `?missing` is skipped; `missing` is an error. A includes B includes A: a cycle error naming both files. A file including itself: a cycle error.
- A file with `\r\n` line endings reads as one with `\n`. A UTF-8 BOM at the start is skipped.

Keys: `Font-Size = 12` is an unknown-key error. `config-file = "?odd"` includes the required file `?odd`.

Flags: one test walks the table: the five merge options parse as flags and every other key is clap's unknown argument. `--font-size=12` on a run exits with 3.

Location: `path_from` with `TRIAD_CONFIG` set, with `XDG_CONFIG_HOME` set, and with neither, on each platform's rules.

`show-config`: `--default` output equals a hand-written expected file; parsing that output gives `Settings::default()`. `--default --docs` output parses to the defaults too. A file with an error exits with 3 and names the line.

Banner: a window given bad lines shows them with the alert role, and its close button closes it. A window form carries the file's bad lines in order; a clean file gives none.

#### E3.8 Appearance and save keys

These keys join spec §6.4's table. Each has a flag of the same name (§E3.5).

| Key | Values | Default |
|---|---|---|
| `theme` | `system`, `light`, `dark` | `system` |
| `line-height` | 1.0–3.0, a multiple of the text size | 1.5 |
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
| `confirm-unresolved-save` | `true`, `false` | `true` |

Colours:

- A colour is `#rrggbb`, hex digits in either case. It is opaque.
- A value gives one colour for both themes, or a pair `light:C,dark:C` in either order. Spaces around `,` and `:` are trimmed. A pair names each theme once. `band-opacity` takes the same forms with numbers.
- The colour is the tone's gutter marks, labels and changed words. The band behind the tone's rows is the colour at `band-opacity`.
- The defaults are today's `tones.rs` hues as `#rrggbb`: HSL saturation 0.7 and lightness 0.45 in the light theme, 0.65 in the dark, and the current region's 88° at saturation 0.85 and lightness 0.4 (light), 0.75 and 0.55 (dark). The cleared tone stays grey and has no key.
- Colour is never the only signal (spec §6.5): the gutter letters stay whatever the colours.

`theme = light` or `dark` holds the windows to that theme. `system` follows the system's appearance, as the windows do today.

`conflict-style` sets the markers in the output under git semantics: `diff3` writes the base section, `merge` drops it and the `|||||||` marker, as git's `merge.conflictStyle = merge` does. git's `zdiff3` reads as `diff3`; refinement is `refine-conflicts`'s job. jj semantics ignore the key and write the style `install --jj` configures (spec §7.1). Headless runs follow it too. The marker parser that reopens a saved merge reads both styles; a file in `merge` style takes its base from the VCS.

`confirm-unresolved-save = false` drops the prompt before Save and Exit with unresolved conflicts: the file is written and the run exits with 1, as after **Save with Conflicts**.

Code:

- `merge-cli::settings` gains a `Colour` type (`r`, `g`, `b` as `u8`), a `Themed<T>` pair with `light` and `dark`, and the keys above.
- `merge-app::tones` takes its colours from a `Palette` built from the settings, in place of the constants. A window builds its palette when it opens.
- `merge-core` output gains a marker style; `vcs-git` reads `merge.conflictStyle` through gix.

Tests:

- `color-left = #2273C3` reads as (0x22, 0x73, 0xc3) for both themes. `light:#000000,dark:#ffffff` and `dark:#ffffff, light:#000000` read the same.
- Refused, each naming the line: `#2273c`, `#2273c3f`, `2273c3`, `#gg0000`, `light:#000000`, `light:#000000,light:#ffffff`, `dim:#000000,dark:#ffffff`.
- `band-opacity = 0` and `= 1` read; `-0.1` and `1.1` are refused. `line-height = 1` and `= 3` read; `0.9` and `3.1` are refused.
- `theme = sepia` and `conflict-style = zdiff3` are refused.
- The default palette's light left mark is `#2273c3`, from the HSL hand calculation above, and its band that colour at 0.18.
- A palette with `color-conflict = #ff0000` gives the conflict band (1, 0, 0) at 0.18 in the light theme.
- Output with `merge` style for a one-conflict merge equals a hand-written file without the base section; under jj semantics the key leaves the output in `diff3` style.
- With git's `merge.conflictStyle = merge` in the fixture repository and no key, the output has no base section; with `conflict-style = diff3` it has one.
- Save and Exit with one unresolved conflict and `confirm-unresolved-save = false` writes the file and exits 1 with no prompt.
- `theme = dark` gives the dark palette under a light system appearance.

### E4 Overview ruler

A strip at the right edge of the result pane, and of the diff window's right pane, shows every region that is not `Unchanged`, over the whole file.

See spec §5. `merge_view::ruler` places marks by result lines, not by aligned rows: the strip stands beside the result, which scrolls by its own lines. `painted` gives each whole pixel the style of the last mark on it, since later marks paint over earlier ones, and joins pixels of one style that meet, so a long file paints no more runs than the strip has pixels. Joining only neighbouring marks of one style painted one quad per mark when styles alternated: at 1M lines GPUI's bounds tree took a third of the main thread and the typing frame p95 was 19.7 ms; with whole pixels it is 10.9 ms (2026-11-08). `merge-app::ruler` paints the strip, maps clicks and drags the thumb. Marks and a click's scroll take one line as one row, so under soft wrap they are approximate; the thumb follows the editor's scroll range and reaches both ends. There is no "edit merges two regions" case beyond the core's: an edit across regions leaves one `Edited` region, and the test checks its one mark.

#### E4.1 Behaviour

- Each region is a mark at its share of the file's height: the region's first row over the total rows, and its row count over the total, with a minimum height of `RULER_MIN_MARK_PX` (2 px) so one-line regions show.
- Colours follow spec §5: left blue, right pink, both teal, edited purple, word-merged green. An unresolved conflict is orange and full width; a resolved conflict is orange at half width, beside its source colour.
- The strip is the pane's vertical scrollbar (§E4.4). Its thumb is the view's share of the content, at least 16 px tall, placed by the share of the scroll range scrolled (`merge_view::ruler::thumb`); a drag maps the thumb's top back to an offset (`drag_offset`).
- A press on the thumb holds it; letting go without a move over a mark goes to the region. A press on the track centres that line, then drags on from there.
- Clicking a mark makes its region current and scrolls to it, as Next and Previous do, with the blink of spec §5. Clicking elsewhere on the strip scrolls that place into view.
- The strip is `RULER_WIDTH_PX` (12 px) wide.
- Marks are visual only, as region bands are (spec §6.5). The navigation commands reach every region from the keyboard.

#### E4.2 Code

- `merge-view::ruler`: `marks(rows, regions, states) -> impl Iterator<Item = RulerMark>`, where a mark holds a region index, a start and end as fractions of the file, and a style. Merging marks that round to the same pixel happens at paint time, in `merge-app`, where the height is known.
- `merge-app`: a GPUI element that paints the marks and the thumb, maps clicks to regions and drags to scroll offsets. The drag state (`ruler::Dragging`) lives in the window, since the strip is rebuilt each frame; mouse moves and ups are window-wide handlers registered in paint, so a drag goes on past the strip.

#### E4.3 Tests

In `merge-view`, with hand-built documents:

- A 100-row file with one conflict at rows 40–49: one mark from 0.40 to 0.50, style unresolved.
- After picking left: the same range, style resolved with left.
- An empty file and an all-unchanged file: no marks.
- A region of zero rows on one side (a pure insertion seen from the result): a mark of zero length at its position; the paint step gives it the minimum height.
- Regions at the first and last row: marks start at 0.0 and end at 1.0.
- An edit that merges two regions: one mark covering both.

In `merge-app`: a click at a mark's pixel makes its region current; a click between marks scrolls without changing the current region. A drag of the thumb past either end reaches the wheel's end and the top; a press on the track then a drag reaches the end; a still click on a mark under the thumb goes to its region; a still click on the thumb off the marks leaves the scroll.

#### E4.4 Scrollbars

The `scrollbars` setting (`always`, `hover`, `when-scrolling`; default `always`) maps to gpui-base's `ScrollbarMode`. It is a config key only. The top panes' `uniform_list` carries a vertical `Scrollbar`. Each editor gets its bars through `EditorState::set_scrollbar_renderer` (`merge-app::scrollbars::install`): the diff window's left pane paints both axes; the result and the diff window's right pane paint only the sideways bar, and the renderer records their vertical scroll in a `Feed` the ruler reads in the same frame, after the editor's prepaint. The editors keep a 10 px right margin and the thumb sits 4–10 px in from the edge, so no padding is reserved.

gpui-base times a bar's fade with `Instant::now`, which tests cannot advance. The tests check visibility through clicks: a shown bar takes a click on its track, a hidden one lets it through. A freshly opened editor shows its bar for its idle time whatever the mode, so the mode tests use the top panes' bar.

The top panes have no sideways bar: their sideways scroll is `Sideways`, not a scroll handle, and needs a `ScrollbarHandle` over it.

### E5 Output line ending

#### E5.1 Behaviour

A setting, `output-line-ending`:

| Value | Output |
|---|---|
| `keep` (default) | Each region keeps its source's bytes, as spec §4.1 says now |
| `base` | Every line ends in base's dominant ending (spec §4.1) |
| `lf` | Every line ends in `\n` |
| `crlf` | Every line ends in `\r\n` |

- The conversion runs at save, on the output text, conflict markers included. A missing newline at end of file stays missing. A lone `\r` stays.
- The result pane keeps the source bytes, so picks and edits stay exact while the window is open.
- The status bar shows `mixed endings` in the conflict tone when the output would hold both `\n` and `\r\n` under `keep`, so the user sees it before saving.
- `--headless` and `inspect --json` honour the setting. `inspect --json` gains a `line_endings` field: `lf`, `crlf`, `mixed` or `none`, for the output under the setting.
- The two-way diff saves each side as it is.

`keep` stays the default so git's byte-for-byte output (spec §4.3, §8.3) holds unless the user asks otherwise.

#### E5.2 Tests

`merge-core`, with expected bytes written by hand:

- Base CRLF, left LF with one changed line, right unchanged: `keep` gives CRLF lines and one LF line; `base` gives all CRLF; `lf` all LF.
- An unresolved conflict under `crlf`: marker lines and section lines all end in `\r\n`.
- Last line without a newline under each value: still without one.
- A lone `\r` inside a line under `lf` and `crlf`: unchanged.
- An empty output: empty under every value.
- `base` with base empty takes left's dominant ending; with neither having line breaks, `\n` (spec §4.1).
- Mixed detection: `\n` only gives `lf`; `\r\n` only gives `crlf`; both give `mixed`; no line breaks gives `none`.

CLI: a headless merge with `output-line-ending = lf` in its config file writes LF bytes (`fixtures/core/line-ending-lf`); `output-line-ending = bogus` is a reported bad line, and the merge keeps each line's ending.

#### E5.3 Notes

See spec §4.1, §5, §6.4. `MergeDocument` counts the result's `\n` and `\r\n` endings and updates the counts as each patch lands, so the status bar's check costs the unresolved conflicts' markers and base sections, not a scan of the text. `saved_endings` gives the endings under a setting without building the output; `output_with` builds and converts it at save.

### E6 Save Copy As

#### E6.1 Behaviour

- **Save Copy As…** (⇧⌘S), in the triad menu of the merge window: a save dialog, starting in OUTPUT's directory, writes the output as Save & Exit would, unresolved conflicts as markers, with the atomic write of spec §6.2.
- The window stays open. OUTPUT, the exit code and the recovery journal are unchanged, so git and jj see no difference.
- A write failure shows an alert and leaves the window as it was.
- With unresolved conflicts, the copy is written without the confirmation sheet; the status bar says how many conflicts the copy holds.

p4merge's Save As retargets the window. triad writes a copy instead because git and jj read OUTPUT, so the window must never lose it.

#### E6.2 Tests

- The copy's bytes equal what Save & Exit would write for the same state, with and without unresolved conflicts, under each `output-line-ending` value.
- After a copy, Save & Exit still writes OUTPUT and exits with 0 or 1 as before.
- A copy to a read-only directory gives the write error and leaves `changed` and the journal as they were.

The save dialog itself is GPUI's; tests call the write path with a chosen path.

#### E6.3 Notes

See spec §6.2, §6.3. `MergeWindow::save_copy` writes through `merge_cli::save`, as Save & Exit does, and touches neither OUTPUT nor the journal. The status bar's note, `Copy saved with N conflicts` or `Copy saved, resolved`, clears on the next change to the merge. GPUI's test platform answers the save dialog, so the tests also cover ⇧⌘S, the dialog's start directory and a cancelled dialog.

### E7 Cross-platform release (M4b)

§A8 holds the work. This section orders it and lists what it needs.

#### E7.1 Linux

1. Done: CI's `package` job runs the headless and tool-integration tests on the release build.
2. Done: `script/package` builds the AppImage and the `.tar.gz` in the Linux test machine, so the binary needs glibc 2.35 at most. CI's `package` job smoke tests the AppImage and uploads both. Each bundles `libxkbcommon`, `libxkbcommon-x11` and `libxcb-xkb` by rpath; `libxcb` stays the system's. The AppImage needs no FUSE in the job: `APPIMAGE_EXTRACT_AND_RUN=1`.
3. Check rendering, input and the accessibility tree on GNOME Wayland and X11. Record GPUI problems in `docs/gpui-kit-issues.md`.
4. Check the config path (§E3.3) and the input file watcher on both.
5. Run the release checklist and measure spec §9 on the Linux reference machine.

The Linux test machine (`script/linux`) covers these steps without hardware. It is an x64 Ubuntu 22.04 container under Rosetta in OrbStack: glibc 2.35, Mesa's software renderer, sway without a display for Wayland, and Xwayland for X11.

- Results, 2026-11-05: 1866 tests pass under the `linux-emulated` nextest profile. `script/linux/smoke` passes on Wayland and X11: the real window opens, takes keys, saves, exits with 0 and writes the expected bytes, and the diff window reloads a file replaced by rename. By hand on Wayland: AT-SPI shows the whole tree of the merge window, `~/.config/triad/config.triad` applies, and Settings… reaches `xdg-open`.
- The profile leaves out `forward::a_missing_gui_binary_is_an_error`. Rosetta's `posix_spawn` reports success for a missing file; native arm64 raises `ENOENT`. CI runs the test natively.
- X11 runs on GL in the smoke test. Mesa 23's software Vulkan shows nothing in a 32-bit window; Mesa 25 draws it through Vulkan.
- `merge_app::menu_bar` draws gpui-component's `TitleBar` and `AppMenuBar` on Linux and Windows, from the definition macOS shows in its global bar. The X11 smoke test resolves a conflict and saves through the menus. Sway decorates windows itself, so the title bar's close button, which cancels as the key does, is unchecked; a GNOME Wayland session checks it.
- `merge_app::prompt_view` replaces GPUI's fallback prompt on Linux: it wraps its detail, follows the theme and takes Enter and Escape. `run` installs it, not `init`, because headless tests answer the test platform's prompts.
- Both packages run `--version` on clean Ubuntu 22.04 and 24.04 images; the AppImage also runs `inspect` there and `install --git` names the AppImage. appimagetool 1.9.1 and the type-2 runtime 20251108 are pinned by digest in the image.
- The packages carry `LICENSE-gpui-base`. They have no MIT licence file for triad and no notices for the other dependencies; both are for the release.
- Sway stands in for GNOME. Timings under Rosetta and software rendering say nothing about spec §9.

#### E7.2 Windows

1. The console and GUI binaries and their forwarding exist (§A4.13). Check them from `git mergetool`, `git difftool`, `jj resolve` and `jj diff --tool` in a real console.
2. MSI through `cargo-wix`, Authenticode signing, winget manifest. Embed `images/triad.ico` in both binaries as icon resource 1, which GPUI's window class loads; a build script with a resource crate does it, and needs a Windows build to check.
3. Check on x64 and ARM64 hardware: rendering, IME input, file watching across editors that save by rename.
4. Run the release checklist and measure spec §9.

#### E7.3 Needs from the owner

- A Linux desktop machine, or a VM with GPU access, running GNOME on Wayland and X11, for the final check and the spec §9 measurements. `script/linux` covers the rest.
- Windows x64 and ARM64 machines.
- An Authenticode signing certificate.
- Permission to install `cargo-wix`.

### E9 Key bindings

`keybind` binds a key chord to an action, in Ghostty's form:

```
keybind = cmd+j=next-conflict
keybind = cmd+shift+j=previous-conflict
keybind = cmd+k=unbind
```

- A chord is modifiers and a key joined by `+`: `cmd`, `ctrl`, `alt`, `shift`, and on Linux and Windows `super`. Key names are GPUI's.
- An action is the kebab-case name of a menu command: `take-left`, `next-conflict`, `save-and-exit`. `unbind` removes the chord's binding.
- `keybind` is the one key a later line does not replace: each line adds a binding, and a later line for the same chord wins. `keybind = clear` drops the defaults and every earlier `keybind` line.
- An unknown action, a malformed chord or a modifier the platform lacks is an error on its line (§E3.2); the other bindings apply.
- The menus show the bound chord.
- `show-config --default` lists every default binding as a `keybind` line.

Tests: a binding replaces a default chord and the action runs on it; `unbind` leaves the chord doing nothing; `clear` then one line leaves one binding; `cmd+j=nonsense` and `cmd+=next-conflict` are errors naming the line; every action in the menus has a name (one test walks them).

Done, 2026-11-05. `merge-cli::keybind` parses the lines and holds the bindings; `merge-app::keys` turns them into GPUI's. Decisions the text above leaves open:

- **Per-window replacement.** The defaults bind `cmd+s` to `save-and-exit` in the merge window and `save` in the diff window. A later line replaces a chord's binding only in the windows that have both commands.
- **`cmd` and `super`.** `cmd` is macOS only and `super` is Linux and Windows only, so each is a bad line on the other platform. A config shared across platforms keeps its chords in an optional include per platform.
- **Unbound chords.** `unbind` and `clear` bind the chord to GPUI's `NoAction` with no context, which also stops the editors' own bindings: ⌘↑ moves the caret in the editor otherwise.
- **Menu chord.** macOS shows an action's first binding, so `keys::bind` registers the newest first. The default table lists `cmd+q` before `cmd+w` so the Cancel item keeps ⌘W.
- **`show-config`.** `keybind` prints `clear` first, so the output reads back as the same bindings. After a `clear` it also lists each dropped default chord as `CHORD=unbind`.
- **Case.** A chord is lower case, as keys are (§E3.1). `cmd+J` is a bad line; `cmd+shift+j` is the form.

## F. Syntax highlighting (M7)

### F1 Goal

- **Every pane colours its text by syntax**: the base, left and right rows, the result editor, and both panes of the diff window.
- **Region tones stay readable.** Bands draw under the syntax colours; intra-line marks draw over them (spec §5).
- **Grammars: Rust, Go, C#, Python, Swift, HTML, JavaScript, TypeScript, TSX.** Others follow one per commit.
- **The window never waits for a parse.** A file triad cannot parse in time shows plain until the parse finishes.
- **`syntax-highlighting = false`** turns it off.

Out of scope: Razor (`.cshtml`, `.razor`), which has no grammar in gpui-component that covers its code blocks; the styles inside HTML and the templates JavaScript tags `css`, `json`, `sql`, `yaml` or `graphql`, whose grammars triad does not build, so they draw plain; syntax-aware merge boundaries (§B5.7, item 5), which follow M7.

### F2 What gpui-component gives

- **Grammars** come as cargo features: `tree-sitter-rust`, `tree-sitter-go`, `tree-sitter-csharp`, through `gpui-kit`'s features of the same names. Each feature builds its grammar's C sources.
- **`LanguageRegistry`** maps a language name (`rust`, `go`, `csharp`) to its grammar and queries. `register` takes a grammar from another crate, so a later language need not come from gpui-component.
- **C# and Swift come without a highlight query.** gpui-component registers their grammars with empty ones. triad registers them again with `tree-sitter-c-sharp`'s and `tree-sitter-swift`'s own queries ([gpui-kit-issues.md](gpui-kit-issues.md)).
- **TSX comes with part of a query.** gpui-component registers `tsx` with only the lines `tree-sitter-typescript` adds to JavaScript's query. triad registers it again with gpui-component's TypeScript query and injections; the TSX grammar is TypeScript's with JSX.
- **The editors' query puts the locals patterns before the highlights.** A node's unstyled local capture (`@local.reference` on a JavaScript identifier) wins over its highlight, so such identifiers draw plain. The top panes take the same query. Injected layers take the highlights alone.
- **The editor's highlighter.** `Input` installs gpui-component's tree-sitter highlighter factory unless the editor already has one (`ensure_highlighter_factory`). That highlighter parses within 2 ms on the UI thread and otherwise re-parses in the background after 150 ms; files over 256 KiB always parse in the background. The factory and its highlighter are crate-private.
- **`SyntaxHighlighter`** is public: `new(language)`, `update(edit, text, timeout)` and `styles(range, resolver)`. It parses a rope and gives the highlight spans of a byte range.
- **`HighlightTheme::default_light()` and `default_dark()`** resolve capture names (`keyword`, `string`, `comment`, …) to colours.

### F3 The conflict with the edit recorder

The result editor and the diff panes record their edits through a highlighter: `recorder::EditRecorder` is installed with `set_highlighter_factory` and turns each `InputEdit` into an `EditBatch` (§A6.2). An editor holds one highlighter, so the recorder shuts out gpui-component's. The tree-sitter highlighter is crate-private, and its background parse uses crate-private parts of `SyntaxHighlighter`, so triad cannot wrap or rebuild it.

Decision: patch `vendor/gpui-base` with an edit observer. `EditorState::set_edit_observer` takes a callback that receives every edit the editor reports to its highlighter, single or batched, in the same order and at the same moment. The recorder moves onto it and stops being a highlighter, and the highlighter slot is free for gpui-component's factory. The patch goes in `PATCHES.md` with its tests, as the earlier patches do.

### F4 The top panes

The base, left and right panes are rows that triad draws (`window.rs`), not editors, so no editor highlighter reaches them. Their texts never change while the window is open.

- Each input is cut into chunks of 2 000 lines (`CHUNK_LINES`, `merge_view::syntax::chunk_starts`), and a chunk parses alone, off the UI thread, when one of its rows shows (`merge_view::chunks`). A row on screen queues its chunk and the chunk on each side (`PREFETCH`); the chunk nearest the screen parses first. Each input runs at most two parses at once, so the three panes parse side by side. A queued chunk more than two from the screen drops from the queue when the rows move. Each input keeps the spans of 16 chunks; the chunk farthest from the screen goes first, and parses again when it shows. A chunk whose parse fails draws plain and does not parse again. The rows draw plain and redraw as each chunk lands.
- The cut: A chunk ends before the first top-level line at or after its 2 000th: a line at column 0, after a blank line, that does not start with `}`, `)`, `]` or `</`. Without one in the next 2 000 lines, the chunk ends at its 2 000th line, and a construct across that cut may colour wrongly. An HTML document is one element, so a long one is cut this way every 2 000 lines: the first closing tags after a cut, whose opening tags sit in the chunk before, draw plain. Each chunk's tree is dropped once its spans are taken. A chunk's spans are the same whether it parses alone or as part of the whole text.
- Each line keeps its capture spans: byte ranges of the line, each with the stack of up to three captures over it, outermost first (`Span`). A row folds the theme's styles of its spans' captures, outermost first, as gpui-component's highlighter does.
- The spans follow gpui-component's rules for overlapping captures: of two captures of one node, the first wins; a capture paints over those that start before it; an injection parses in its ranges, one level deep, when its language is built in: Rust's macro bodies as Rust, HTML's scripts as JavaScript, JavaScript's `html` templates as HTML. An injected layer's own injections do not run, and a combined injection draws plain.
- The row's `StyledText` takes the syntax colours as text colour and the intra-line marks as background over them. The band stays the cell's background.
- Above `syntax-whole-file-max-lines` the result and diff editors parse a chunk at a time (§F4.1); below it, gpui-component's highlighter parses them whole.

#### F4.1 The editors past `syntax-whole-file-max-lines`

A file longer than `syntax-whole-file-max-lines` gives the result editor, or both diff panes, triad's own highlighter (`editor_syntax.rs`) through `set_highlighter_factory`. The factory serves the language the file names give, so turning the colours off and on swaps the editor between it and plain.

- The highlighter cuts the editor's rope by the rule of §F4 in one pass over its bytes, off the UI thread. The editor draws plain until the cut lands. A cut of a text that has changed since drops, and the text cuts again.
- `styles` asks for the chunks of the lines it draws, and parses run off the UI thread as for the top panes; the editor redraws as each lands.
- An edit replaces its rows: the chunks it touches become one, with a new id, and the chunks after it move. A parse of the old chunk that lands later drops. A chunk alone keeps its other lines' spans until the new parse lands; the edited lines draw plain meanwhile. Chunks merged by an edit draw plain until they parse. A chunk that grows past twice `CHUNK_LINES` cuts again.
- An edit whose rows do not match the chunks' line count cuts the whole text afresh.
- The editor's first update carries an edit, not the whole text, and a rebuilt highlighter starts again; both take the whole text as a fresh cut.

### F5 Language detection

- From the file name's extension: `.rs` → `rust`, `.go` → `go`, `.cs` → `csharp`, `.py` and `.pyi` → `python`, `.swift` → `swift`, `.html` and `.htm` → `html`, `.js`, `.mjs`, `.cjs` and `.jsx` → `javascript`, `.ts`, `.mts` and `.cts` → `typescript`, `.tsx` → `tsx`. A `Language` table in `merge-view`, tested without GPUI.
- git and jj pass temporary inputs (`BASE_1234.rs`, `/tmp/jj-resolve-…/a.rs`). The output's name comes next, then the `--title` path.
- An unknown extension is plain text. The first match wins; content sniffing is out.

### F6 Spike

One throwaway branch answers these before the tasks start; its findings go in this file.

1. The three features build, and `cargo deny check` passes with the grammar crates and `tree-sitter` (MIT).
2. The edit-observer patch: the recorder on the observer gives the same batches as today, and `crates/merge-app/tests` pass unchanged.
3. Result-pane bands and the current-region frame draw under the syntax colours, and gutter marks are unchanged.
4. `SyntaxHighlighter` across threads (§F4).
5. Cost on `target/measure/100000` renamed to `.rs`: first frame, scroll and typing under `script/measure-app`, against spec §9.

#### F6.1 Findings

Spike of 2026-11-03, on macOS, release build.

1. `gpui-kit`'s features `tree-sitter-rust`, `tree-sitter-go` and `tree-sitter-csharp` build on macOS and add 71 lines to `Cargo.lock`. `cargo deny check` passes. The one open Dependabot alert is an npm package outside triad.
2. and 3. The edit observer and the drawing order are tasks 1, 2 and 5; their tests answer them.
4. `SyntaxHighlighter` is `Send`. A background task can build and parse it and hand it to the UI thread whole.
5. `fixtures/demo/base.txt` repeated to 100 940 lines (3.2 MB) parses in 390 ms with `update(None, text, None)`, off the UI thread. `styles` over a 4 KB screen takes 0.35 ms and gives about 980 spans, so three top panes cost about 1 ms a frame without a cache. The window measurement with `script/measure-app` waits for task 8, once the panes are highlighted.

#### F6.2 Contrast at the defaults

WCAG contrast of each capture colour the Rust, Go and C# queries emit, drawn over each band of `DEFAULT_COLOURS` at the default `band-opacity` on gpui-component's default background. The colours are gpui-component's `HighlightTheme::default_light()` and `default_dark()`.

| Theme | Default text, no band / worst band | Lowest capture, no band | Lowest capture, worst band |
|---|---|---|---|
| Light | 19.8 / 14.6 | comment 3.83 | comment 2.83 (Edited); attribute 3.06 (Edited) |
| Dark | 19.0 / 9.5 | comment 7.30 | comment 3.64 (Base); number, constant, boolean 3.98 (Base) |

- The light theme's comment colour, `#007fff`, is the weak point: 3.83:1 on white before any band. Every light band brings it to 2.8–3.4.
- In the light theme, a band at opacity 0.14 keeps comments at 3.03:1 on the Edited band; 0.18 gives 2.83:1.
- The dark theme gives no colour to the variable captures, which draw in the default text colour.
- triad sets its own light colours (`syntax_colours.rs`): comments `#0066cc` (5.57:1 on white, 4.11 on the worst band) and attributes `#7d6629` (5.52 / 4.07). The dark theme keeps gpui-component's colours. A test holds every capture of both themes at 3:1 on every default band.

#### F6.3 Measured cost

`script/measure-app 120 rs` against `script/measure-app 120`, release build, on macOS at 120 Hz, 2026-11-03. The `rs` run names the output `out.rs`, so all four texts parse as Rust: the three inputs for the top panes and the result editor's own. The generated lines are Rust items behind a line number, which tree-sitter parses with error nodes.

| 100k lines (6 MB per file) | Plain | Rust |
|---|---|---|
| First frame | 213–249 ms | 230–358 ms |
| Scrolling, frame p95 | 8.4 ms | 8.3–8.6 ms |
| Keystroke / pick, max | 0.8 / 0.7 ms | 0.6 / 0.5 ms |
| Peak resident memory | 197–212 MB | 814–1033 MB |

- The result editor's highlighter alone, with the top-pane parses off, takes the peak to 444 MB.
- `fixtures/demo` repeated to 98 000 lines (3.2 MB of real Rust per file): 205 MB plain, 1 368 MB with Rust, frame p95 10.0 ms. One full parse costs about 290 MB, some 90 bytes per byte of text.
- At 1M lines the peak reaches 1.9 GB, under the 2 GB target only because the parses have not finished when the run ends.
- Full parses miss the §F9 memory target at 100k lines; the first frame also misses it in one run of five.

Decision (owner, 2026-11-03): the top panes keep per-line capture spans from a chunked parse (`merge_view::syntax`), and the result and diff editors highlight files up to `syntax-whole-file-max-lines` (default 20 000). Measured again with that design, same machine and settings:

| | 100k lines | 1M lines |
|---|---|---|
| First frame | 225–273 ms | 805–819 ms |
| Scrolling, frame p95 | 8.4–9.0 ms | 8.3–8.4 ms |
| Keystroke / pick, max | 0.9 / 0.5 ms | 5.3 / 5.5 ms |
| Peak resident memory | 197–228 MB | 866–878 MB |

- `fixtures/demo` repeated to 98 000 lines: 204 MB plain, 230 MB with Rust, frame p95 8.7 ms.
- At 1M lines the three parses run past the end of each mode, so the peak is not the finished cost. The spans take about 12 bytes per capture run.

`TRIAD_MEASURE=syntax` waits until every input's parse has landed, so its peak is the finished cost. Release build, macOS, 2026-11-04, output `out.rs`:

| | 100k lines | 1M lines | `fixtures/demo` to 98 000 lines |
|---|---|---|---|
| First frame | 296 ms | 739 ms | 233 ms |
| First input coloured | 3.5 s | 30.8 s | 1.5 s |
| All three coloured | 9.9 s | 91.5 s | 3.0 s |
| Peak resident memory | 248 MB | 1 103 MB | 213 MB |

- Memory meets the §F9 targets. Time to colour does not scale: one input costs about 31 µs per generated line and 10 µs per line of real Rust, for parse, query and paint together. The spike's 390 ms per 100k lines counts the parse alone.
- The inputs parse one after another, so the third pane waits for the first two.

Decision (owner, 2026-11-04): the top panes parse the chunks on screen, as §F4 describes. Measured again, same machine and settings:

| | 100k lines | 1M lines | `fixtures/demo` to 98 000 lines |
|---|---|---|---|
| First frame | 232–264 ms | 762–843 ms | 201–215 ms |
| Chunks on screen coloured | 388 ms | 893–969 ms | 289 ms |
| Scrolling, frame p95 | 8.4 ms | 8.4 ms | 8.4 ms |
| Keystroke / pick, max | | 5.5 / 5.5 ms | |
| Peak resident memory | 231–234 MB | 877–892 MB | 213–217 MB |

- The rows on screen take their colours 110–185 ms after the first frame, at any length. A chunk scrolled to parses in about 60 ms (2 000 lines at 31 µs).
- Memory stays within about 50 MB of plain text at 1M lines: 16 chunks per input hold about 32 000 lines' spans.

With the result editor chunked past `syntax-whole-file-max-lines` (§F4.1), same machine and settings, 2026-11-04:

| | 100k lines | 1M lines |
|---|---|---|
| First frame | 217–257 ms | 762–780 ms |
| Scrolling the result, frame p95 | 8.4 ms | 8.5 ms |
| Keystroke / pick, max | 0.6 / 0.5 ms | 5.3 / 5.2 ms |
| Peak resident memory | 208–263 MB | 911–934 MB |

- gpui-component's highlighter on the 100k-line result alone took the peak to 444 MB (above); the chunked one keeps it within 70 MB of plain text.
- The cut of a 1M-line rope by line lookups took 396 ms on the UI thread; the one-pass cut runs off it.

### F7 Tasks

One commit each, in order.

1. gpui-base: the edit observer, with tests in the vendored crate.
2. The recorder on the edit observer; the editors keep `PLAIN` and behave as today.
3. `merge-view::language`: extension → language, and the fallback through output and title.
4. The `syntax-highlighting` setting, `true` by default, in `merge-cli::settings` and spec §6.4.
5. Result editor and diff panes: set the pane's language; enable `tree-sitter-rust`, `tree-sitter-go` and `tree-sitter-csharp`.
6. Top panes: a parse per input off the UI thread, and syntax spans merged with the intra-line marks.
7. Theme: syntax colours follow `theme`; check contrast of each capture colour against every band at the default `band-opacity`. The colours follow `theme` and live appearance changes (`tests/syntax.rs`); the contrast figures are in §F6.2.
8. `script/measure-app` with highlighting on, against spec §9; record the figures in spec §9.

### F8 Tests

- Language: `a.rs` → Rust; `BASE_1234.rs` → Rust; `BASE_1234` with output `src/a.go` → Go; `.txt` and no extension → plain; `A.RS` → Rust (case-insensitive).
- Recorder on the observer: typing, paste, multi-cursor edits and undo give the batches the current tests expect, unchanged.
- A Rust input's `fn` keyword cell in the left pane carries the theme's keyword colour; the same cell inside a conflict draws the same backgrounds over its band as without syntax: only the intra-line mark.
- With `syntax-highlighting = false`, every span has the default style.
- A Rust, Go and C# sample's spans give the same style at every byte as the editor's highlighter, at the default chunk size and, for Go and C#, at chunks that cut the sample.
- The top panes render plain before the parse lands and highlighted after, without a relayout (row heights unchanged).
- Each chunk parsed alone gives the lines of the whole parse. A 7 000-line input colours its first rows and leaves its last chunk plain until the rows move there.
- The chunk cache: the chunk on screen parses first, then the nearer neighbour, then the earlier of two; no more parses run than the limit; a move drops queued chunks more than two away and keeps those parsing; the cache drops the chunk farthest from the screen, never the screen's and its neighbours; a failed chunk does not parse again.
- A result or diff side of `syntax-whole-file-max-lines` lines takes gpui-component's highlighter; one line more takes the chunked one, in the same language.
- The chunked editor highlighter: over two chunks of the demo it draws what gpui-component's highlighter draws; an edit keeps the other lines' old colours until its chunk parses, then matches a fresh parse; an opened comment recolours the lines after it; a paste of 10 000 lines cuts again; rows that do not match cut afresh; a cut of an older text drops.
- The cut of a rope gives the starts of the cut of a `Text` of the same lines; the one-pass cut matches the rule for every pattern of twelve lines at chunk sizes 1 to 4.
- Every capture colour of both themes holds 3:1 over every default band (`syntax_colours.rs`).

### F9 Decisions

1. Razor is deferred (owner, 2026-11-03). Rust, Go and C# came first; Python, Swift and HTML followed (owner, 2026-11-03), then JavaScript and TypeScript (owner, 2026-11-04).
2. The edit recorder moves to a gpui-base edit observer (§F3) in place of wrapping gpui-component's highlighter, which is crate-private.
3. The top panes keep chunked capture spans; the editors highlight files up to `syntax-whole-file-max-lines` (owner, 2026-11-03; §F6.3).
4. triad sets its own light-theme colours for captures under 3:1 on a band (owner, 2026-11-03; §F6.2).
5. The top panes parse only the chunks on screen and their neighbours, and keep 16 per input (owner, 2026-11-04; §F4, §F6.3).
6. The result and diff editors past `syntax-whole-file-max-lines` parse a chunk at a time with triad's highlighter; below it they keep gpui-component's (owner, 2026-11-04; §F4.1).

## G. Review and diff features

### G1 Goal

Features that other merge and diff tools have and triad lacks, from a comparison on 2026-11-08 against Beyond Compare, Araxis Merge, Kaleidoscope, Meld, KDiff3, IntelliJ and VS Code. None blocks the beta.

Order: G2–G5 first, then G6–G10, then G11–G14. Each section is one or more commits, and updates the spec when it lands. Every comparison rule keeps the rule of spec §4.2: it changes which lines count as equal, never the output bytes.

Out of scope (spec §1.3): image and binary comparison, encodings other than UTF-8 and UTF-16, printing, HTML reports.

### G2 Collapse unchanged regions

Every tool in the comparison has this. It is the main way to move through a large file with few changes.

- An `Unchanged` region shows `context-lines` lines (default 3) at each end that faces a change, and hides the rest behind one fold row: `N unchanged lines`. The first and last regions keep context only on the side that faces a change. A region folds only when it would hide two or more lines.
- A click on the fold row, or a key, expands it. View → Collapse Unchanged folds and expands all. Settings: `collapse-unchanged` (`false`), `context-lines` (0–100).
- Merge window: the top panes' fold row stands in for a run of aligned rows, so the panes stay level. The result editor folds the same lines through gpui-base's `FoldMap`; folding is off there today (§A6.2). Scroll sync counts a fold row as one row in each pane.
- Diff window: both editors fold. The spacer layer sits after `FoldMap` (§A6.5), so its row counts must take folded rows.
- A jump, a search step or a go-to-line into a fold expands it. An edit expands the fold it touches. Line numbers keep the file's numbers.
- Open: whether `FoldMap` can draw a custom placeholder row, or the vendored patch grows (§A6.1).
- Tests: fold boundaries at `2 × context` and `2 × context + 2` lines; first and last regions; a file with no changes; `context-lines = 0`; a jump into a fold; an edit inside a fold; scroll sync across a fold.

### G3 Bulk resolution

Done 2026-11-08: `Command::PickAll { scope, sources }`, with `PickScope::Unresolved` and `PickScope::All`; journal format 9. Left and right are sides 0 and 1, as for the other takes, in a merge of any number of sides.

- Take Left for All Unresolved, Take Right for All Unresolved: the take command on every unresolved conflict. Picked and edited regions stay. With more than two sides, one command per side.
- Take Whole Left File, Take Whole Right File: every region picks that side, so the result is that input's text byte for byte. This is git's `checkout --ours` and `--theirs` for one file, for lock files and generated files.
- Each is one undo step and one journal command. `docs/json-schema.md` and `--commands` gain the commands; the journal format version goes to 9.
- Key-bindable actions: `take-left-all-unresolved`, `take-right-all-unresolved`, `take-whole-left`, `take-whole-right`. Merge menu items; no default keys.
- Tests: three conflicts, one picked right, one edited, one unresolved: only the unresolved one changes; one undo restores all three; whole-file takes give the input's exact bytes, endings and missing final newline included; a merge with no conflicts; a three-sided jj conflict.

### G4 Comparison filters

Each filter applies when lines are interned (`diff::line_key`), under both semantics, alongside the whitespace mode.

- `ignore-case`: lines compare after lower-casing. Open: Unicode lower case per character, or ASCII only as in search.
- Line endings: done 2026-11-08. `merge-ignore-line-endings` (merges, `false`) and `diff-ignore-line-endings` (two-way diff, `true`, owner's choice) make `\r\n`, `\n` and a missing final newline compare equal, through `diff::LineEndingRule` in `diff::Comparison`. Under jj semantics the status bar marks it as a whitespace divergence; git's merge has `-Xignore-cr-at-eol`, so git semantics show none. Journal format 8 records the rule.
- `ignore-pattern = REGEX`, on as many lines as needed, as `keybind`: every match is cut out of the line before comparison (Beyond Compare's "unimportant text"). A pattern that does not compile is a bad config line (spec §6.4); one that matches the empty string is a bad line too. `regex` is already in the tree through `jj-core`.
- `ignore-blank-lines`: a change made only of blank lines is not a difference. Two-way diff first. In a merge it changes which hunks touch, and so which regions conflict; design it after the two-way case.
- Output bytes follow spec §4.2: a line equal under the filters takes the bytes of a side that differs from base, else left's.
- Status bar: one item per filter in force, `unlike git` or `unlike jj` (spec §5). The git oracle runs without filters.
- Tests: per filter, the equality decision and the output bytes; filters combined with each whitespace mode; an invalid and an empty-matching pattern; a pattern that empties a line; CRLF against LF with `merge-ignore-line-endings` and `ignore-whitespace = none`.

### G5 Moved code in the two-way diff

- After the diff, each deleted block is matched against the inserted blocks. A block that occurs, under the whitespace mode and filters, at exactly one inserted place, with at least `MOVE_MIN_LINES` non-blank lines, is a move. Blank blocks never match. The matcher is the one behind Carry Edit (§B5.8 item 4).
- Both ends show in a move tone with an `M` gutter mark; a click on the mark jumps to the other end. Next Difference visits both ends.
- `diff --inspect --json` gains the other end's region index (`docs/json-schema.md`).
- Later: near matches, a moved block that was also edited, shown with an intra-line diff.
- Tests: a block moved up and down; a block with two candidate places (no move); a blank block; a block below `MOVE_MIN_LINES`; a move under `ignore-whitespace = all`.

### G6 Directory diff

`jj diff --tool` runs `file-by-file` today, one window per file in turn.

- `triad-merge diff LEFT_DIR RIGHT_DIR`: one window with a list of changed files (added, deleted, modified; binary files listed and not opened). Selecting a file shows its diff panes; edits save per file.
- jj: `diff-invocation-mode = "dir"`; git: `difftool --dir-diff`. `install` writes the dir-mode entries.
- Native mode: `triad-merge diff` with no path lists the working copy's changes against its parent. `vcs` gains `Repository::changed_paths(from, to)`, in the conformance suite on jj, internal jj and git.
- Open: renames show as delete and add, or by jj's and git's copy tracking.
- Folder merges stay under Later (spec §1.1).

### G7 Alignment points

- The user links a line in one pane to a line in another (right-click → Align With…, or a key on two carets). The diff runs on the pieces between links; no region crosses a link. Links show as gutter marks and last for the session; the journal records them.
- In a merge, links between base and a side constrain that side's diff. That changes which regions conflict, so the status bar shows `aligned by hand, unlike git` or `unlike jj`.
- A link that crosses an existing one is refused.
- Tests: a link splits a region at the hand-computed place; crossing links; a link on lines already matched changes nothing; undo of a link.

### G8 Line picks inside a conflict

- Each line of a side in a conflict gets a hit target in the top pane. Picked lines go into the result in source order, then line order.
- Open: a new resolution, `Lines(Vec<(Source, Range)>)`, that toggles line by line and needs journal format 9; or picks that build `Edited` text, which needs no model change but cannot be toggled back. Proposed: the new resolution.
- Tests: every order of picks across two sides; toggling a line off; a line without a newline before another pick (spec §4.4 rule); undo.

### G9 Unified view in the diff window

- View → Unified: one read-only pane, deleted and inserted lines interleaved, with both line-number columns. Editing stays in side-by-side. Setting `diff-layout = side-by-side | unified`.

### G10 Visible whitespace

- View → Show Whitespace, setting `show-whitespace`: tabs as `→`, spaces as `·`, `\r` as `␍`, and a mark for a missing final newline, in every pane. Check whether gpui-base's editor draws these or needs a patch (§A6.1).

### G11 Patch export

- `diff --inspect --unified[=N]` prints a unified diff with the pane titles as `---` and `+++` headers. The diff window's Edit → Copy as Patch copies the current region or the whole file.
- Oracle: equal to `git diff --no-index` for the corpus pairs, histogram with the indent heuristic.

### G12 Swap sides

- View → Swap Sides in the diff window: the panes, their read-only flags, titles and undo histories change places. The merge window has none: left and right come from the VCS.

### G13 Merge window reload

- The merge window watches its inputs as the diff window does (spec §6.1). A change asks Keep Mine (first) or Reload; Reload rebuilds the merge and drops the resolutions. Low value: git and jj pass copies that do not change.

### G14 Standard input

- `triad-merge diff - FILE` reads one side from stdin, read-only and titled `stdin`. The open dialog's Diff tab takes `Clipboard` in a slot.

### G15 Decisions

None yet. Open: G2's default, G4's case folding and blank lines in a merge, G6's renames, G8's resolution model.
