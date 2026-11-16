# triad — plan

Where the project stands and what comes next. [SPEC.md](SPEC.md) sets the behaviour, [DESIGN.md](DESIGN.md) holds the design of what is built (parts A–F), [PLAN-diff-editor.md](PLAN-diff-editor.md) holds part H while it is open; part I, the jj-native features, is in `DESIGN.md`. Parts G and J are below. A reference such as §B5.8 names part B, section 5.8, in whichever file holds that part. jj holds the history.

## 1. Status

As of 2026-11-16.

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
| Review and diff features | Bulk takes (§G3), line endings, case, ignore patterns and two-way blank lines in comparison (§G4), collapsed unchanged regions in both windows (§G2) and moved blocks in the diff window and `diff --inspect` (§G5) done; §G6 done: the files that differ between two directories, or between a revision and the working copy (`diff`, `diff --rev`), in the directory window and printed by `diff --inspect`, and from `jj diff --tool` and `git difftool -d`, with renames paired as jj pairs them; invisible characters drawn as code points and odd spaces shaded in every pane, and escaped in terminal output (§A6.8); moved blocks in the merge window (§G16) next; the rest open | G |
| Diff editor: `jj split`, `jj squash -i`; `stage` for git | Planned | H |
| jj-native features | Done: conflicts across a stack (`stack`, `resolve -r`), operation-log history (`history`, `diff --op`) and absorb by hunk (`absorb`); the stack window, with a preview of a save's effect on the descendants and a confirmation before a save changes them; the history window; the absorb window, by hunk; these windows and the directory window share one frame after Zed's panels (`docs/STYLE.md`): a header with a count and the buttons, status icons and file-name-first paths, keys for the lists, and Escape and Cancel to close; the conflict badge names the sides a conflict keeps; absorb, `resolve -r` and the default revset aliases checked against jj's CLI (§I5); `stack`, `resolve -r`, `history`, `absorb` and a three-sided save checked through the built binary in scenario repositories (`script/test-jj-repos`) | I |

Every spec §9 target is met (§A6.7). Toolchain: stable Rust 1.100, with `rust-version` 1.99. Dependencies: jj-lib and jj-core 0.47.1 on gix 0.90; gpui-kit 0.8.1 with the vendored `gpui-base` patch; gpui-kit 0.9 is not adopted (`docs/gpui-kit-issues.md`).

CI: `triad.yml` tests on Linux, macOS and Windows; `bench.yml` and `audit.yml` run weekly. All three pass on GitHub's runners; Linux and macOS also under `act`. On the Windows runner, tests that build jj repositories run about 40 times slower than on Linux: `vcs-jj`'s jj tests take 292 s of test time there and 7 s on Linux, while `vcs-git`'s take 8 s. Defender is off in the runner image. The cause is open (§3). `release.yml` runs on a `v*` tag that matches the workspace version: it builds `triad.app` for macOS ARM64, a zip of both binaries for Windows x64 and the smoke-tested Linux x64 packages, and publishes them as a release. v0.1.0 is the first.

Build profiles (root `Cargo.toml`): `release` aborts on panic and strips symbols, 37.5 MB on macOS ARM64, 1 min clean; `script/macos-app`, `script/measure-app` and the benchmarks use it. `dist` adds fat LTO and one codegen unit, 35.4 MB, 3 min; `script/package`, `script/macos-app dist` and `release.yml` use it.

## 2. Next steps

In order. Each is one or more commits.

1. **Close M3.** Skipped for now by the owner. The spoken VoiceOver steps of `docs/release-checklist.md`, with the owner at the machine: every control reachable and named, a pick toggled from VoiceOver, the result's text read. Rerun checklist step 6 on jj.
2. **Screen-reader support** (part J). Skipped for now by the owner: live status, conflict regions, scrolling the row table, in the order of §J7.
3. **Moved code in the merge window** (§G16), in its steps: moves from base to a side first, then duplicates.
4. **Review and diff features** (part G): §G7–§G14.
5. **More grammars,** one commit each: C, C++, Java and Kotlin; then Markdown; then Bash, Ruby, PHP and SQL.
6. **M4b** (§3) once the owner has the machines and the certificate.
7. **Diff editor** (part H), in the order of §H12: `triad-merge edit`, file by file, first.

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
| The two-way diff under histogram on a line repeated on every few lines, such as `}`: 2.1 s at 100k lines against Myers's 29 ms (`diff-load-braced`) | — | §A4.7 |
| A histogram diff that gives git's hunks faster: 1.5 s against jj's 70 ms on the 100k-line `deep` triple | — | §B6 |
| A sideways scrollbar for the top panes: a `ScrollbarHandle` over `Sideways` | — | §E4.4 |
| A `script/measure-app` mode with a search query open | — | §A14 |
| More grammars | — | §2 |
| Razor | A grammar that covers its code blocks | §F1 |
| Unusual characters checked on screen with the platform shapers: badges and shades in the top panes and the editors, and no reordering around a masked bidirectional control | The owner at the machine | §A6.8 |
| Lookalike letters, such as Cyrillic а for Latin a: a confusables table and a rule for text in other scripts | Owner's choice | §A6.8 |
| Fuzzing | After release 1; `cargo-fuzz` needs nightly | spec §8.9 |
| Screen-reader support: status announcements, conflict regions, scrolling the row table, the accessibility tree in headless tests, a keyboard and contrast audit | §J1 needs the owner at the machine | J |
| gpui-kit 0.9: build, rebase the patch, measure | — | `docs/gpui-kit-issues.md` |
| Merge window folds: the untouched lines of an unchanged region whose result is edited | A line diff of the region | §G2 |
| Ruler marks with folds: the marks keep their place in the whole file while the thumb follows display rows | A choice between the two | §G2 |
| A key that opens the fold next to the caret | — | §G2 |
| Moved code in the merge window: moves, then duplicates | — | §G16 |
| Duplicate suggestion in the merge window | Owner's choice on §G15's open point for §G16 | §G16 |
| Review and diff features: blank lines in a merge (§G4), §G7–§G14 | Owner's choices on §G15's open points for §G4 and §G8 | G |
| Directory diff: copies detected, as jj finds them from modified files | — | §G6 |
| Diff editor, §H4–§H10 | Owner's choices on §H13's open points | `PLAN-diff-editor.md` |
| The absorb oracle's generator builds text files only: no renames, executable bits, symlinks, conflicted inputs or immutable commits | — | §I5 |
| jj-cli's config defaults other than the revset aliases: none checked for whether triad depends on one | — | §I5 |
| Absorb: split one hunk between commits | Part H's line selection (§H6) | §I4 |
| Internal jj repositories: a jj command that commits between triad's load and its commit leaves two operation heads, which triad refuses until jj's next command merges them | — | §D5.5 |
| The stack, history and absorb windows checked on screen: layout, keys, focus and the accessibility tree; their tests run headless only. `script/gen-jj-repo` builds a repository for each (`docs/test-repos.md`) | — | §I2, §I3, §I4 |
| The directory window checked on screen, from `jj diff --tool`, `git difftool -d` and `triad-merge diff`: layout, keys, focus and the accessibility tree | The owner at the machine | §G6 |

## 4. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| gpui-component API churn on upgrade | Rework in `merge-app` | Exact pin; upgrade as a separate task; `merge-app` stays thin |
| The vendored editor patch drifts from new gpui-kit releases | Rework on upgrade | Keep the patch small and listed in `PATCHES.md` |
| Editor slow paths: very long lines, many cursors | Missed spec §9 targets | Benchmarks and `script/measure-app`; intra-line diffs capped |
| Editor and core text drift apart | Wrong output bytes | The core is the source of truth; tests compare the editor's rope with the core's after each command |
| jj-lib API changes between releases | `vcs-jj` breaks on bump | Exact pin; the conformance suite gates each bump |
| `vcs-jj` re-implements jj-cli behaviour: the end of a transaction in `JjRepository::rewrite`, the default revset aliases, absorb's hunk split | A jj upgrade changes jj's behaviour and triad's silently departs from it | Oracles from jj's CLI, pinned to the jj-lib version in `Cargo.lock`, cover the aliases, absorb and `resolve -r` (§I5); regenerating them needs the pinned jj on `PATH` |
| gix and jj-lib disagree on gix version | Two gix copies | `vcs-git` pinned to jj-lib's gix; `script/check-workspace` fails on two |
| Windows GUI and console split surprises git or jj | Lost exit codes or stderr | §A4.13 tests in CI; manual check in M4b |
| Linux desktop rendering | Poor Linux experience | Real-desktop check in M4b; record in `gpui-kit-issues.md` |
| Build time and disk use grow with the dependency tree | Slow iteration | §A13.5; the quick pre-commit profile |

## G. Review and diff features

Features other merge and diff tools have and triad lacks, from a comparison with Beyond Compare, Araxis Merge, Kaleidoscope, Meld, KDiff3, IntelliJ and VS Code. None blocks the beta.

Order: G2–G6, then G16, then G7–G10, then G11–G14. Each section is one or more commits and updates the spec when it lands. Every comparison rule keeps spec §4.2's rule: it changes which lines count as equal, never the output bytes.

Out of scope (spec §1.3): image and binary comparison, encodings other than UTF-8 and UTF-16, printing, HTML reports.

### G2 Collapse unchanged regions

The main way to move through a large file with few changes; every tool compared has it. Spec §5 sets the behaviour; `DESIGN.md` §A6.5 holds the design.

- Diff window: done. Settings `collapse-unchanged` and `context-lines`, View → Collapse Unchanged (⌘U), fold rows that open on a click, edits and search matches that open their fold.
- Merge window: done. One fold row across the top panes, the result's lines hidden behind a spacer row, folds that open on a click in either pane, on Go to Line, and on a search match in hidden lines; ⌘U and the settings as in the diff window.
- Open: a key that opens the fold next to the caret; ruler marks in display rows while folds are collapsed; in the merge window, folding the untouched lines of an unchanged region whose result is edited, which needs a line diff of the region.

### G3 Bulk resolution

Done: `Command::PickAll` with `PickScope::Unresolved` and `PickScope::All` (spec §6.3), one undo step and one journal command each.

### G4 Comparison filters

Each filter applies when lines are interned (`diff::line_key`), under both semantics, beside the whitespace mode. Done: line endings (`merge-ignore-line-endings`, `diff-ignore-line-endings`), `ignore-case`, full Unicode case folding, and `ignore-pattern`, regular expressions whose matches lines drop before comparison; each has its own status bar item in both windows (spec §4.2). `diff-ignore-blank-lines`: in the two-way diff, a change made only of blank lines is an `Ignored` region, no difference (spec §4.2, §4.6).

- Open: blank lines in a merge. There they change which hunks touch, and so which regions conflict.
- Output bytes follow spec §4.2. The status bar shows one item per filter in force, `unlike git` or `unlike jj`. The git oracle runs without filters.
- Tests: per filter, the equality decision and the output bytes; filters with each whitespace mode; an invalid and an empty-matching pattern; a pattern that empties a line.

### G5 Moved code in the two-way diff

Done: the matcher, `diff2::moves` (spec §4.6, `DESIGN.md` §A4.7), recomputed and installed with the regions; `moves` in `diff --inspect` (`docs/json-schema.md`); both ends in the move tone on the bands and the ruler, with an `M` gutter mark and a pill naming the other end's line, each of which, as Go to Other End of Move (⇧⌘M) does, goes to the block's first line at the other end (spec §5, `DESIGN.md` §A6.5).

- Later: near matches, and a moved block that was also edited, with an intra-line diff.

### G6 Directory diff

- Done: `merge_session::dir_diff` lists the files that differ (added, deleted, modified; text, binary, undecodable or special) and `diff --inspect LEFT_DIR RIGHT_DIR` prints them (spec §6.1, `docs/json-schema.md`).
- Done: `triad-merge diff LEFT_DIR RIGHT_DIR` opens the directory window, a list window as the history window is (spec §6.1, `DESIGN.md` §A6.9). A text file opens in the diff window, one at a time; closing it lists the files again. Edits save per file; a right file that copies jj's working copy saves to the working copy.
- Done: `install --jj` writes `diff-invocation-mode = "dir"`, so `jj diff --tool triad` opens the directory window. `git difftool -d` runs git's existing `cmd` with two directories and an empty `$MERGED`, and its right directory links the working-tree files at absolute paths; checked with jj 0.47.1 and git through `diff --inspect` as the tool (spec §7.1).
- Done: `triad-merge diff [--rev REV]` lists the working copy's changes against `@-`, `HEAD` or REV in the directory window, and `diff --inspect` prints them. jj snapshots first; git counts staged and unstaged changes to tracked files. `Repository::changed_paths` gives the candidates, in the conformance suite; the session compares bytes (spec §6.1, `DESIGN.md` §A6.9).
- Done: renames and moves, in every mode, by `merge_core::rename`: jj's rule through gix, ordered by git's blob ID, so the listing matches `jj diff --summary`; checked against jj 0.47.1 on ties, the 50 % boundary, empty and binary files (spec §6.1, `DESIGN.md` §A6.9).
- Later: copies, which jj finds from modified files.
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

### G16 Moved code in the merge window

Shows how blocks moved, from base to a side or between the sides, in the merge window's top panes, and warns where a clean merge holds a block twice. Builds on §G5's matcher and Carry Edit's rule (spec §4.4), so all three agree on what a move is.

**Kinds of move.**

1. **Base to one side.** A side deletes a block of base lines and inserts it at one other place. `diff2::moves` runs on the base→left changes and on the base→right changes, under the merge's whitespace mode and filters. A block moved on one side and edited on the other is Carry Edit's case and keeps its suggestion; the move shows as well.
2. **The same move on both sides, to different places.** Both sides delete the block, which merges cleanly at the origin, and each inserts it at a different place, each insert clean. The result holds the block twice and no region conflicts; git and jj merge it the same way. Both moves show, and the pair is a duplicate.
3. **The same new text on both sides, at different places.** No origin in base. Left's inserted blocks are matched against right's inserted lines as kind 1 matches; a match at exactly one place is a duplicate. Inserts the merge already joins as `BothSame` are one region and no duplicate.

**Drawing.**

- Each end takes the move tone on the bands and the ruler, in place of its side's colour, as in the diff window.
- No `M` gutter mark: the top panes' gutter holds the pick targets.
- Pills sit in the pane of the side that moved, at both ends: on the origin's first filler row, `→ L40`, and on the destination's first row, `← B12`. The base pane is hidden by default, so a pill there would rarely show. A move on both sides has a pill in each side's pane. Pill size, place, colours and the rule that it shows only while its row lies wholly in view follow spec §5's two-way mode.
- A press on a pill, or Go to Other End of Move (⇧⌘M) with the current region at one end, goes to the other end as a jump does, with its blink.
- The result pane draws no move: it is an editor, and its regions follow the edits. A duplicate is the exception, since it is about the result's own text: both copies mark the ruler in the conflict tone's outline, and the status bar counts them, `2 duplicated blocks`.

**Data.** `merge_core` computes the moves with the regions in each recompute and installs them together, as `diff2` does (`DESIGN.md` §A4.7). A move names its side, its origin region and line range, or none for kind 3, and its destination region and line range. `merge-view` keys move ends by pane and region, since the merge window has three panes and the diff window two. `inspect` prints `moves` and `duplicates` in its JSON (`docs/json-schema.md`).

**Steps.**

1. Kinds 1 and 2 as moves: matcher, view model, pills, ruler, ⇧⌘M, `inspect`.
2. Duplicates, kinds 2 and 3: the result's ruler marks, the status bar count, `inspect`.
3. A suggestion that drops one copy of a duplicate, if §G15 decides for it.

**Tests.** Expected moves come from hand-built triples in `fixtures/core`: a move on the left, on the right, on both sides to the same place (`BothSame`, no duplicate) and to different places (duplicate), new text on both sides at different places, a block that matches at two places (no move), a block of blank lines (no move), a move with an edit on the other side (move and Carry Edit both). Kind 3 under jj's semantics with more than two sides. Moves under each whitespace mode and filter. A move whose regions lag the text: no pill and no jump. The view's pill placement against the row table.

### G15 Decisions

- **G2 default:** `collapse-unchanged` is `false`. The merge window moves by conflict, and next and previous conflict already pass the unchanged text; resolving a conflict needs the code around it. Folding in the result editor carries the most risk: edits in a fold, undo, search and scroll sync.
- **G2 order:** the diff window first, then the merge window. A diff with few changes is where folding helps most, and part H's diff editor needs it. Two editors and no aligned top panes make it the smaller step, and it settles the editor patch the merge window reuses.

- **G4 case:** full Unicode case folding (`CaseFolding.txt` C and F), not ASCII only. Search folds ASCII only; it may follow later.
- **G4 blank lines:** two-way diff only, under the key `diff-ignore-blank-lines`, named as `diff-ignore-line-endings` is. Blank lines in a merge change which regions conflict and wait for their own design.
- **G6 renames:** one matcher in triad for every mode, following jj where jj and git differ; renames only, at jj's fixed 50 %. jj's dir mode and git's dir diff pass two plain directories with no copy information, so matching by content serves all three.
- **G6 layout:** a list window that opens the diff window, as the history and stack windows do. An in-window list needs the diff window to swap files in place, and can follow.
- **G5 minimum:** Carry Edit's rule, so the two matchers agree: a block with at least one non-blank line, found at exactly one place. `MOVE_MIN_LINES` is 1.

- **G16 duplicates:** open. A duplicate is reported, in the ruler, the status bar and `inspect`. Whether it is also offered as a suggestion, `Drop Duplicate at Line N`, which deletes one copy as one edit and one undo as Carry Edit does, waits for the owner. Proposed: report first, then one suggestion per copy, so the user picks which place keeps the block.

Open: G4's blank lines in a merge, G8's resolution model, G16's duplicate suggestion.

## J. Screen-reader support

triad gives every control a role and a label (spec §6.5, `DESIGN.md` §A6.6). The gaps are in `docs/gpui-kit-issues.md`. None needs a change to GPUI except §J5: `A11ySubtreeBuilder::parent_node` hands an element's raw `accesskit::Node` to `a11y_synthetic_children`, so triad can set any AccessKit property, and `Window::on_a11y_action` takes any AccessKit action. Zed's GPUI has the same API and no more; Zed's editor exposes nothing to a screen reader, so it gives triad no pattern for its panes.

### J1 Baseline

Run the spoken VoiceOver steps of `docs/release-checklist.md` before any change (§2 step 1). Record what VoiceOver reads for the status bar, a conflict in the result, and the row table past the visible rows.

### J2 Status announcements

The status bars and the conflict badge take `Live::Polite`; the jj banner and the open dialog's error take `Live::Assertive`. AccessKit's macOS adapter (`accesskit_macos` 0.26.3, `src/event.rs`) announces a live node when it is added with a value or when its value changes; a change of label alone is silent. These nodes therefore carry their text as value as well as label. Tests: the node's `live` and `value` after a pick, a save and an error. Check by ear on macOS; check AT-SPI and UIA when M4b has the machines.

### J3 Conflict regions in the result

The result pane adds one synthetic node per region: its kind, its number ("region 2 of 3") and its state (resolved, unresolved, edited), with the line range. Its gutter letters follow from the same node. The diff window's change bands get the same treatment. Next and previous conflict move the screen reader's focus to the region's node. Tests: node count, names and order for `fixtures/demo`; names after a pick and after an undo.

### J4 Scrolling the row table

`uniform_list` builds only the rows on screen. The row table answers `ScrollDown`, `ScrollUp` and `ScrollIntoView` through `on_a11y_action`, and moves the merge window's scroll as a key would. Row and column indices stay in whole-file terms. Tests: dispatch each action and check the first visible row and the indices.

### J5 The tree in headless tests

GPUI's test window never turns accessibility on, so `Window::debug_a11y_tree_json` stays empty and tests read `test_support()` records. A patch to the test platform that turns it on lets `tests/a11y.rs` check the real tree, `live`, values and synthetic nodes included. Optional: §J2–§J4 can test through `test_support()` and a captured node, as `vendor/gpui-base/src/button.rs` does.

### J6 Keyboard and contrast audit

Every action reachable without the mouse in each window: merge, diff, open dialog, stack, history, absorb, settings banner. Region colours and gutter letters against WCAG 2.2 AA contrast (4.5:1 text, 3:1 non-text) in both themes. Findings become rows in §3.

### J7 Order

J1, J2, J3, J4, J6; J5 when a test of J2–J4 needs the real tree.
