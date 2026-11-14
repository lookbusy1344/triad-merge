# triad — plan

Where the project stands and what comes next. [SPEC.md](SPEC.md) sets the behaviour, [DESIGN.md](DESIGN.md) holds the design of what is built (parts A–F), [PLAN-diff-editor.md](PLAN-diff-editor.md) holds part H while it is open; part I, the jj-native features, is in `DESIGN.md`. Part G is below. A reference such as §B5.8 names part B, section 5.8, in whichever file holds that part. jj holds the history.

## 1. Status

As of 2026-11-14.

| Milestone | State | Part |
|---|---|---|
| M1 `merge-core` and headless CLI | Done | §A4 |
| M2 `merge-view` | Done | §A5 |
| M3 GUI release 1, macOS | Done, except the spoken VoiceOver steps of the release checklist | §A6 |
| M4 tool integration, local macOS release | Done; release checklist step 6 needs a rerun on jj, since `jj diff --tool` saves the right side to the working copy (§A6.5) | §A7 |
| Beta, macOS | Open: testers build `triad.app` and install it in `/Applications` | §A7.4 |
| M4b Linux and Windows | Linux test machine and packages build; final checks need hardware, Windows a certificate | §E7, §3 |
| Merge semantics | Done: jj's merge, word level, refinements, syntax-aware boundaries | B |
| M5 chunk origin | Done, with revision chips | C |
| M6 native resolution | Done: git stages a resolved save; jj snapshots before the read and after each save | D |
| M7 syntax highlighting | Done: Rust, Go, C#, Python, Swift, HTML, JavaScript, TypeScript, TSX, JSON, YAML, TOML, CSS | F |
| p4merge parity | Done | E |
| Review and diff features | Bulk takes (§G3), line endings in comparison (§G4) and collapsed unchanged regions in both windows (§G2) done; the rest open | G |
| Diff editor: `jj split`, `jj squash -i`; `stage` for git | Planned | H |
| jj-native features | Done: conflicts across a stack (`stack`, `resolve -r`), pair reduction (`resolve --pair`), operation-log history (`history`, `diff --op`) and absorb by hunk (`absorb`); the stack window, with a preview of a save's effect on the descendants and a confirmation before a save changes them; the history window; the absorb window, by hunk; the three windows' lists follow `docs/STYLE.md` and work from the keys; the conflict badge names the sides a conflict keeps; absorb, `resolve -r` and the default revset aliases checked against jj's CLI (§I6) | I |

Every spec §9 target is met (§A6.7). Toolchain: stable Rust 1.100, with `rust-version` 1.99. Dependencies: jj-lib and jj-core 0.47.1 on gix 0.90; gpui-kit 0.8.1 with the vendored `gpui-base` patch; gpui-kit 0.9 is not adopted (`docs/gpui-kit-issues.md`).

CI: `triad.yml` tests on Linux, macOS and Windows; `bench.yml` and `audit.yml` run weekly. All three pass on GitHub's runners; Linux and macOS also under `act`. On the Windows runner, tests that build jj repositories run about 40 times slower than on Linux: `vcs-jj`'s jj tests take 292 s of test time there and 7 s on Linux, while `vcs-git`'s take 8 s. Defender is off in the runner image. The cause is open (§3). `release.yml` runs on a `v*` tag that matches the workspace version: it builds `triad.app` for macOS ARM64, a zip of both binaries for Windows x64 and the smoke-tested Linux x64 packages, and publishes them as a release. v0.1.0 is the first.

Build profiles (root `Cargo.toml`): `release` aborts on panic and strips symbols, 37.5 MB on macOS ARM64, 1 min clean; `script/macos-app`, `script/measure-app` and the benchmarks use it. `dist` adds fat LTO and one codegen unit, 35.4 MB, 3 min; `script/package`, `script/macos-app dist` and `release.yml` use it.

## 2. Next steps

In order. Each is one or more commits.

1. **jj-native review finding** (§3): the executable-bit rule.
2. **Close M3.** The spoken VoiceOver steps of `docs/release-checklist.md`, with the owner at the machine: every control reachable and named, a pick toggled from VoiceOver, the result's text read. Rerun checklist step 6 on jj.
3. **Review and diff features** (part G): the rest of the comparison filters (§G4), moved code in the two-way diff (§G5); then §G6–§G14.
4. **More grammars,** one commit each: C, C++, Java and Kotlin; then Markdown; then Bash, Ruby, PHP and SQL.
5. **M4b** (§3) once the owner has the machines and the certificate.
6. **Diff editor** (part H), in the order of §H12: `triad-merge edit`, file by file, first.

## 3. Outstanding work

| Work | Blocked on | Where |
|---|---|---|
| Release checklist: spoken VoiceOver steps | The owner | §2 |
| Linux: rendering, input and the accessibility tree on real GNOME Wayland and X11; the config path and file watcher there; release checklist and spec §9 on the Linux reference machine | A Linux desktop or a VM with GPU access | §E7.1 |
| Linux packages: triad's MIT licence file and dependency notices | The release | §E7.1 |
| Windows: tool runs from `git mergetool`, `git difftool`, `jj resolve` and `jj diff --tool` in a real console; MSI through `cargo-wix`, Authenticode signing, winget manifest; icon resource; rendering, IME and file watching on x64 and ARM64; release checklist and spec §9 | x64 and ARM64 machines, a certificate, permission to install `cargo-wix` | §E7.2 |
| Windows ARM64 in CI (`windows-11-arm`), before version 1.0 | — | `.github/workflows/triad.yml` |
| Windows CI: find why jj repository tests run about 40 times slower than on Linux. First check: fixtures on D: through `TMP` and `TEMP`; next, jj-lib's file lock under parallel tests | A CI run per check | §1, `.github/workflows/triad.yml` |
| Per-command scans in `MergeWindow::refresh`: bands, placeholders, status counts; a pick at 1M costs 4.4 / 6.3 ms mean / max | A measurement that calls for them | §A6.7 |
| Whole-text copies: `Recompute::run`, `MergeDocument::output`, 8-byte line starts | A measurement that calls for them | §A6.7 |
| `DiffScroll`: the diff window's eight scroll fields in one component, as `PaneScroll` | A diff scroll bug | §A13.3 |
| A histogram diff that gives git's hunks faster: 1.5 s against jj's 70 ms on the 100k-line `deep` triple | — | §B6 |
| A sideways scrollbar for the top panes: a `ScrollbarHandle` over `Sideways` | — | §E4.4 |
| A `script/measure-app` mode with a search query open | — | §A14 |
| More grammars | — | §2 |
| Razor | A grammar that covers its code blocks | §F1 |
| Fuzzing | After release 1; `cargo-fuzz` needs nightly | spec §8.9 |
| Accessibility gaps: no live announcements, region bands and gutter marks not exposed | gpui-kit | `docs/gpui-kit-issues.md` |
| gpui-kit 0.9: build, rebase the patch, measure | — | `docs/gpui-kit-issues.md` |
| Collapse unchanged regions in the merge window | — | §G2 |
| Ruler marks with folds: the marks keep their place in the whole file while the thumb follows display rows | A choice between the two | §G2 |
| A key that opens the fold next to the caret | — | §G2 |
| Review and diff features, §G4–§G14 | Owner's choices on §G15's open points | G |
| Diff editor, §H4–§H10 | Owner's choices on §H13's open points | `PLAN-diff-editor.md` |
| The absorb oracle's generator builds linear stacks of text files with the default `--into` only: no merge commits, no merge working copy, no renames, executable bits, symlinks, conflicted inputs, immutable commits or narrower `--into` | — | §I6 |
| jj-cli's config defaults other than the revset aliases: none checked for whether triad depends on one | — | §I6 |
| Absorb: split one hunk between commits | Part H's line selection (§H6) | §I5 |
| The stack, history and absorb windows checked on screen: layout, keys, focus and the accessibility tree; their tests run headless only | — | §I2, §I4, §I5 |
| The executable bit: `held_conflict` and `simplified` compare terms by file ID alone, `reduce_pair` simplifies by the whole tree value. A change to the bit alone passes the changed-conflict check | — | §I2, §I3 |
| `resolve --pair` has the least use for its cost among the jj-native features: conflicts of more than two sides are rare. It is the first to cut if part I is pruned again | Owner's choice | §I3 |

## 4. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| gpui-component API churn on upgrade | Rework in `merge-app` | Exact pin; upgrade as a separate task; `merge-app` stays thin |
| The vendored editor patch drifts from new gpui-kit releases | Rework on upgrade | Keep the patch small and listed in `PATCHES.md` |
| Editor slow paths: very long lines, many cursors | Missed spec §9 targets | Benchmarks and `script/measure-app`; intra-line diffs capped |
| Editor and core text drift apart | Wrong output bytes | The core is the source of truth; tests compare the editor's rope with the core's after each command |
| jj-lib API changes between releases | `vcs-jj` breaks on bump | Exact pin; the conformance suite gates each bump |
| `vcs-jj` re-implements jj-cli behaviour: the end of a transaction in `JjRepository::rewrite`, the default revset aliases, absorb's hunk split | A jj upgrade changes jj's behaviour and triad's silently departs from it | Oracles from jj's CLI, pinned to the jj-lib version in `Cargo.lock`, cover the aliases, absorb and `resolve -r` (§I6); regenerating them needs the pinned jj on `PATH` |
| gix and jj-lib disagree on gix version | Two gix copies | `vcs-git` pinned to jj-lib's gix; `script/check-workspace` fails on two |
| Windows GUI and console split surprises git or jj | Lost exit codes or stderr | §A4.13 tests in CI; manual check in M4b |
| Linux desktop rendering | Poor Linux experience | Real-desktop check in M4b; record in `gpui-kit-issues.md` |
| Build time and disk use grow with the dependency tree | Slow iteration | §A13.5; the quick pre-commit profile |

## G. Review and diff features

Features other merge and diff tools have and triad lacks, from a comparison with Beyond Compare, Araxis Merge, Kaleidoscope, Meld, KDiff3, IntelliJ and VS Code. None blocks the beta.

Order: G2–G5, then G6–G10, then G11–G14. Each section is one or more commits and updates the spec when it lands. Every comparison rule keeps spec §4.2's rule: it changes which lines count as equal, never the output bytes.

Out of scope (spec §1.3): image and binary comparison, encodings other than UTF-8 and UTF-16, printing, HTML reports.

### G2 Collapse unchanged regions

The main way to move through a large file with few changes; every tool compared has it. Spec §5 sets the behaviour; `DESIGN.md` §A6.5 holds the design.

- Diff window: done. Settings `collapse-unchanged` and `context-lines`, View → Collapse Unchanged (⌘U), fold rows that open on a click, edits and search matches that open their fold.
- Merge window: done. One fold row across the top panes, the result's lines hidden behind a spacer row, folds that open on a click in either pane, on Go to Line, and on a search match in hidden lines; ⌘U and the settings as in the diff window.
- Open: a key that opens the fold next to the caret; ruler marks in display rows while folds are collapsed; in the merge window, folding the untouched lines of an unchanged region whose result is edited, which needs a line diff of the region.

### G3 Bulk resolution

Done: `Command::PickAll` with `PickScope::Unresolved` and `PickScope::All` (spec §6.3), one undo step and one journal command each.

### G4 Comparison filters

Each filter applies when lines are interned (`diff::line_key`), under both semantics, beside the whitespace mode. Line endings are done (`merge-ignore-line-endings`, `diff-ignore-line-endings`; spec §4.2).

- `ignore-case`: lines compare after lower-casing. Open: Unicode lower case per character, or ASCII only as in search.
- `ignore-pattern = REGEX`, on as many lines as needed, as `keybind`: every match is cut out of the line before comparison (Beyond Compare's "unimportant text"). A pattern that does not compile, or one that matches the empty string, is a bad config line. `regex` is in the tree through `jj-core`.
- `ignore-blank-lines`: a change made only of blank lines is no difference. Two-way diff first; in a merge it changes which hunks touch, and so which regions conflict, so design it after the two-way case.
- Output bytes follow spec §4.2. The status bar shows one item per filter in force, `unlike git` or `unlike jj`. The git oracle runs without filters.
- Tests: per filter, the equality decision and the output bytes; filters with each whitespace mode; an invalid and an empty-matching pattern; a pattern that empties a line.

### G5 Moved code in the two-way diff

- Each deleted block is matched against the inserted blocks. A block that occurs, under the whitespace mode and filters, at exactly one inserted place, with at least `MOVE_MIN_LINES` non-blank lines, is a move. The matcher is Carry Edit's (§B5.7, item 4).
- Both ends show in a move tone with an `M` gutter mark; a click on the mark jumps to the other end. Next Difference visits both ends.
- `diff --inspect --json` gains the other end's region index (`docs/json-schema.md`).
- Later: near matches, and a moved block that was also edited, with an intra-line diff.
- Tests: a block moved up and down; a block with two candidate places (no move); a blank block; a block below `MOVE_MIN_LINES`; a move under `ignore-whitespace = all`.

### G6 Directory diff

`jj diff --tool` runs `file-by-file`, one window per file in turn.

- `triad-merge diff LEFT_DIR RIGHT_DIR`: one window with a list of changed files (added, deleted, modified; binary files listed and not opened). Selecting a file shows its diff; edits save per file.
- jj: `diff-invocation-mode = "dir"`; git: `difftool --dir-diff`. `install` writes the dir-mode entries.
- Native mode: `triad-merge diff` with no path lists the working copy's changes against its parent. `vcs` gains `Repository::changed_paths(from, to)`, in the conformance suite.
- Open: renames as delete and add, or by jj's and git's copy tracking.
- Folder merges stay out (spec §1.1).

### G7 Alignment points

- The user links a line in one pane to a line in another (right-click → Align With…, or a key on two carets). The diff runs on the pieces between links; no region crosses one. Links show as gutter marks, last for the session, and the journal records them.
- In a merge, links between base and a side constrain that side's diff, which changes which regions conflict, so the status bar shows `aligned by hand, unlike git` or `unlike jj`.
- A link that crosses another is refused.
- Tests: a link splits a region at the hand-computed place; crossing links; a link on lines already matched changes nothing; undo of a link.

### G8 Line picks inside a conflict

- Each line of a side in a conflict gets a hit target in the top pane. Picked lines go into the result in source order, then line order.
- Open: a new resolution, `Lines(Vec<(Source, Range)>)`, that toggles line by line and needs a journal version; or picks that build `Edited` text, which needs no model change but cannot be toggled back. Proposed: the new resolution.
- Tests: every order of picks across two sides; toggling a line off; a line without a newline before another pick (spec §4.4); undo.

### G9 Unified view in the diff window

View → Unified: one read-only pane, deleted and inserted lines interleaved, with both line-number columns. Editing stays side by side. Setting `diff-layout = side-by-side | unified`.

### G10 Visible whitespace

View → Show Whitespace, setting `show-whitespace`: tabs as `→`, spaces as `·`, `\r` as `␍`, and a mark for a missing final newline, in every pane. Check whether gpui-base draws these or needs a patch (§A6.1).

### G11 Patch export

`diff --inspect --unified[=N]` prints a unified diff with the pane titles as `---` and `+++` headers. The diff window's Edit → Copy as Patch copies the current region or the whole file. Oracle: `git diff --no-index` on the corpus pairs, histogram with the indent heuristic.

### G12 Swap sides

View → Swap Sides in the diff window: the panes, their read-only flags, titles and undo histories change places. The merge window has none: left and right come from the VCS.

### G13 Merge window reload

The merge window watches its inputs as the diff window does (spec §6.1). A change asks Keep Mine (first) or Reload; Reload rebuilds the merge and drops the resolutions. Low value: git and jj pass copies that do not change.

### G14 Standard input

`triad-merge diff - FILE` reads one side from stdin, read-only and titled `stdin`. The open dialog's Diff tab takes `Clipboard` in a slot.

### G15 Decisions

- **G2 default:** `collapse-unchanged` is `false`. The merge window moves by conflict, and next and previous conflict already pass the unchanged text; resolving a conflict needs the code around it. Folding in the result editor carries the most risk: edits in a fold, undo, search and scroll sync.
- **G2 order:** the diff window first, then the merge window. A diff with few changes is where folding helps most, and part H's diff editor needs it. Two editors and no aligned top panes make it the smaller step, and it settles the editor patch the merge window reuses.

Open: G4's case folding and blank lines in a merge, G6's renames, G8's resolution model.
