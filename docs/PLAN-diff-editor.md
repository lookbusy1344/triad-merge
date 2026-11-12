# triad — diff editor plan

Inputs: [SPEC.md](SPEC.md), [PLAN.md](PLAN.md), [DESIGN.md](DESIGN.md). Rules for changes: `AGENTS.md`.

This file is part H of the plan, kept apart while the work is open. Cite it by part and section: §H4 is section 4 here; §A–§F are in DESIGN.md. [PLAN.md](PLAN.md) §1–§3 carry its status and next steps. Each section is one or more commits, lands with its spec change, and follows the ground rules of §A1.

## H1 Goal

triad becomes jj's diff editor: the tool `jj split`, `jj squash -i`, `jj commit -i`, `jj restore -i` and `jj diffedit` run to choose which changes go where. git gets the same feature as native commands that write the index (§H9).

| Command | Today | After this plan |
|---|---|---|
| `jj resolve`, `git mergetool` | Done | — |
| `jj diff --tool`, `jj interdiff --tool`, `git difftool` | Done, one file per run | Optional directory view (§H10) |
| `jj split`, `jj squash -i`, `jj commit -i`, `jj restore -i`, `jj diffedit` | Not supported | §H4–§H8 |
| `git add -p`, `git reset -p`, `git restore -p` | No hook in git | `triad-merge stage`, `unstage`, `discard` (§H9) |

The selection is the core of the feature: per change, per line, and per file, with undo, in triad's merge window.

## H2 jj's diff editor contract

From jj's source at 0.47.1 (`cli/src/merge_tools/external.rs`, `diff_working_copies.rs`) and `docs/config.md`.

- jj names the tool in `ui.diff-editor`, or in `--tool` on the command. It runs `program` with `merge-tools.<tool>.edit-args`, in the user's working directory, with absolute paths. The default is `["$left", "$right"]`. `edit-args = []` refuses the tool for diff editing.
- jj checks out the two trees into a temporary directory named `jj-diff-<random>`: `left` holds the tree before, `right` the tree after. Only the changed paths are checked out, through sparse patterns. Each tree gets a `<name>_state` directory beside it.
- With `$output` in `edit-args` jj also checks out `output`, a copy of `right`. This is the three-pane form. jj makes every file in `left` read-only, and in the three-pane form every file in `right` too. Directories stay writable.
- jj reads the result back from `output`, else from `right`, with a working-copy snapshot: content, added and deleted files, the executable bit, symlinks. Ignored files count by the repository's ignore rules.
- Conflicted files are checked out with conflict markers in the configured style. The snapshot parses markers back in a file that was a conflict.
- `edit-invocation-mode`: `dir` (default) runs the tool once with the three directories. `file-by-file` runs it once per checked-out path, with `$left`, `$right` and `$output` naming that path in each tree. A path absent from a tree names a file that does not exist.
- A non-zero exit aborts the jj command and discards the result. In `file-by-file`, the first non-zero exit stops the loop.
- `JJ-INSTRUCTIONS` is written into `output`, else `right`, unless `ui.diff-instructions = false`. In the three-pane form `right` gets one too. Its text names the command and the commits, such as "You are moving changes from: X into commit: Y". jj deletes the file before the snapshot, so edits to it are lost. It is not a checked-out path, so `file-by-file` never passes it.
- jj gives the tool no labels and no command name beyond that text.

## H3 Selection as a merge

A diff edit is a merge where base and left are the same text:

- Base and left: the `$left` file. Right: the `$right` file. Output: the `$output` file.
- Every change is `RightOnly`, so the automatic result is `$right`, which is jj's default: no change to the selection.
- Picking right keeps a change in the selection. Picking left drops it and restores the left text. An edit in the result pane is an `Edited` region, as in a merge.
- The resolution model (spec §4.4), undo, the recovery journal, the ruler, Take Whole Left File (drop everything) and Take Whole Right File (keep everything) work unchanged.
- The edit runs under jj semantics with hunk level `line`, whitespace mode `none` and line endings compared. jj's diff gives the hunks `jj diff` shows. An ignored difference would be kept without being shown, so the comparison settings do not apply.
- triad uses the three-pane form: `edit-args = ["edit", "$left", "$right", "$output"]`. Left and right stay read-only, and `$right` stays intact for the whole session.

The window is the merge window in a selection layout: left titled Parent, right titled Change, the result titled Selected. The base pane and its toggle are hidden. A pick target on the right pane keeps a change; one on the left drops it. The status bar counts kept changes: `4 of 9 changes kept`. No region is a conflict, so Save & Exit asks no confirmation.

## H4 Step 1: `triad-merge edit`, file by file

```
triad-merge edit LEFT RIGHT OUTPUT [--title T]
triad-merge edit --headless LEFT RIGHT OUTPUT --commands FILE
```

- A new form in `merge-cli`, `merge-session` and `merge-app`. Text files only at this step; §H5 adds the other entries.
- jj config, from `install --jj` (§H8):

  ```toml
  [merge-tools.triad]
  edit-args = ["edit", "$left", "$right", "$output"]
  edit-invocation-mode = "file-by-file"
  ```

- The title is the path below `jj-diff-<random>/output`, found as `diff` finds jj's paths (spec §7.1). `edit` never redirects to the working copy and shows no "edits are lost" banner.
- Save & Exit writes OUTPUT with the atomic write of spec §6.2 and exits with 0. The write keeps OUTPUT's permissions, so the executable bit survives. Cancel exits with 2 and leaves OUTPUT as jj wrote it; jj then aborts the command.
- A missing side reads as empty and is handled by §H5's entry rules; this step refuses it with exit code 3.
- `--headless --commands` applies merge commands as spec §6.1 describes, for scripts and the CLI tests.
- Line origins (spec §7.3) are off: jj's temporary trees name no revision.

## H5 Step 2: whole-file entries

Each path is an entry. An entry is a regular file, an executable file, a symlink or absent in each tree. An entry carries a text selection only when both sides are text files or absent.

| Entry | Choice |
|---|---|
| Text file changed | Regions, as §H3 |
| Added text file (left absent) | Regions against empty left; dropping every region makes the output absent |
| Deleted text file (right absent) | Regions against empty right; keeping every region makes the output absent |
| Executable bit changed | Keep or drop the mode change, beside the text selection |
| Empty file added or deleted, binary file, symlink, type change | Keep or drop the whole entry |

- An absent side and an empty file differ. The output is absent when the selection takes the absent side's text in every region, or when the entry is dropped whole.
- Dropping a deleted file recreates it with left's bytes and mode, and creates its parent directories.
- Binary is spec §4.1's rule. A binary entry shows the two sizes; it never exits with 3, which would abort the jj command.
- A file with conflict markers is plain text here. jj parses markers back on snapshot, so an edit to a marker line changes the conflict. The window shows a banner on such a file.
- Tests: every row of the table, each kept and dropped, with the output tree checked byte for byte and mode for mode; an added file with every region dropped is absent, not empty; a deleted file kept whole recreates its directory.

## H6 Step 3: line selection

Hunk selection is the minimum. Line selection lets a change be split inside a hunk.

- A changed region holds removed lines (from left) and added lines (from right). Each line is kept or dropped on its own.
- The region's output is the removed lines that are dropped from the selection, in order, then the added lines that are kept, in order. This is the rule of git gui's Stage Lines: a dropped removal becomes context, a dropped addition disappears.
- Core: a new resolution, `Lines { removed, added }`, two bit sets over the region's lines. Every line kept equals a pick of right; every line dropped equals a pick of left; the core normalises both. Journal format version rises by one.
- UI: a check mark in each pane's gutter on a changed line. Click toggles one line, shift-click a range, Space the caret's line or the selection's lines. A region with mixed lines shows a mixed mark. Each toggle is one undo step.
- Tests: hand-computed outputs for a region of 2 removed and 3 added lines under each of several selections; normalisation to a plain pick; undo across a partial selection; journal replay.

## H7 Step 4: directory mode

One window for the whole change, with `edit-invocation-mode = "dir"`.

- `triad-merge edit LEFTDIR RIGHTDIR OUTPUTDIR`. `merge-session` pairs the three trees into entries (§H5) in path order, component by component, as the conflicted paths are ordered (spec §8.8). It skips `JJ-INSTRUCTIONS` at the top of each tree and the `_state` directories beside them.
- A file list sits left of the panes: one row per entry, with its path, its kind (added, deleted, mode, binary, symlink) and its state: all kept, some kept, none kept. The list is a uniform list, so 10 000 entries cost no more than 100. Keyboard: next and previous file; next change crosses into the next file. Keep File and Drop File act on the selected rows.
- Each text entry opens as a `MergeDocument` when first shown. A document with no picks is dropped from memory when it leaves the view and rebuilt when shown again.
- `JJ-INSTRUCTIONS` from OUTPUTDIR shows in a collapsible note above the list. It names the command and the commits; the panes keep the titles of §H3.
- Save & Exit writes every entry whose selection differs from right, then exits with 0. Cancel exits with 2 and writes nothing. A failed write shows an alert and leaves the window open; entries already written stay written, and the next save writes the rest.
- The recovery journal keys on the hashes of every entry's inputs and holds each document's command log.
- Spec §9 gains targets: open a 1 000-file change, step between files, save.
- `install --jj` switches to `edit-invocation-mode = "dir"` when this lands.

## H8 Step 5: install

- `install --jj` writes `edit-args` and `edit-invocation-mode` into `[merge-tools.triad]`, beside the existing keys, and sets `ui.diff-editor = "triad"`, as it sets `ui.merge-editor`. `--no-diff-editor` leaves `ui.diff-editor` alone.
- `ui.diff-instructions` stays as the user set it: the directory window shows the instructions (§H7), and `file-by-file` never sees them.
- Tests: as spec §7.1's install tests: a second run leaves the file byte for byte as it was, and other tools' sections stay.

## H9 Step 6: git parity

git has no diff editor hook: `git add -p`, `git reset -p` and `git restore -p` select in the terminal. triad gives git the same selection as native commands, through gix, as `resolve` does (spec §7.4).

```
triad-merge stage [PATH...]      # left: index, right: working tree, output: index
triad-merge unstage [PATH...]    # left: HEAD, right: index, output: index (what stays staged)
triad-merge discard [PATH...]    # left: index, right: working tree, output: working tree (what stays)
```

- The window is §H7's, over the changed paths, or the named ones.
- `stage` and `unstage` write blobs and index entries through gix, as `resolve` stages a resolved save (part D). The working tree is untouched. `discard` writes the working tree with the atomic write.
- In a jj repository these commands exit with 3 and name the jj command to use: jj's own commands select changes and run triad as their editor.
- `git difftool --dir-diff` and `jj diff --tool` in `dir` mode open the same window read-only (§H10).
- Tests: in git repositories built with gix: stage part of a hunk, a line, an added file, a deleted file, a mode change; unstage part of a staged change; discard a line; the index entry's blob and mode checked after each, and the working tree's bytes.

## H10 Step 7, optional: directory diff view

The directory window of §H7 without a result pane: two panes and the file list, for `jj diff --tool`, `jj interdiff --tool` and `git difftool --dir-diff` with `diff-invocation-mode = "dir"`. Edits follow spec §7.1's rules for where they land. This replaces one window per file with one window per diff. It waits until §H7 has been used.

## H11 Tests

- `merge-core`: §H3's mapping gives `RightOnly` regions whose automatic output is right; dropping every region gives left; §H6's line rule.
- `merge-session`: tree pairing from directories laid out as jj lays them out: `jj-diff-<random>/{left,right,output}`, read-only files, `JJ-INSTRUCTIONS`, `_state` directories. Symlinks, modes and binary files as §H5 lists.
- `merge-cli`: `edit --headless --commands` for each §H5 entry kind, with output bytes, modes and exit codes; Cancel leaves OUTPUT as it was.
- `merge-app`: the selection layout's titles, counts and pick targets; the file list's states and keyboard; save with a failing write.
- Conformance: a fixture builds the commit in jj, internal jj and git repositories through jj-lib and gix, checks out the trees as jj does, runs `edit --headless`, and checks the tree jj would snapshot. The test never runs the jj executable.
- Release checklist: `jj split`, `jj squash -i`, `jj commit -i`, `jj restore -i` and `jj diffedit` with triad as `ui.diff-editor`, by hand, in a jj repository, file by file and in `dir` mode; `stage`, `unstage` and `discard` in a git repository.

## H12 Order

1. §H3 and §H4: a usable `jj split` and `jj squash -i` on text files, file by file.
2. §H5: every entry kind, so no jj command aborts on a binary or deleted file.
3. §H8 for `file-by-file`.
4. §H6: line selection.
5. §H7, then §H8's switch to `dir`.
6. §H9.
7. §H10, if wanted.

## H13 Open points

- Pane titles. jj passes no labels. Parent and Change fit `split`, `squash -i` and `commit -i`; `restore -i` and `diffedit` read slightly off. Reading command names out of `JJ-INSTRUCTIONS` would be fragile, so the instructions show as text and the titles stay fixed.
- Whether §H6's line rule needs a mode that keeps a dropped removal beside its replacement, for a replaced line pair split one way and the other. git gui and jj's built-in editor use the rule above.
- Whether `install --jj` sets `ui.diff-editor` by default (§H8) or only on request. It replaces jj's built-in editor.
- Whether `unstage` and `discard` ship with `stage` or follow it.

## H14 Out of scope

- Selecting inside a binary file or a symlink target.
- Rename detection. jj and git hand over paths; a rename is a delete and an add.
- Resolving the conflicts inside a conflicted file during a diff edit; `resolve` does that.
