# Release checklist

Manual checks before a release. A run that changes a measured figure replaces it in the DESIGN.md section named; PLAN.md §1 states what still needs a run.

## Performance on the reference machine (DESIGN.md §A6.7, spec §9)

1. Plug in to power. Close other apps.
2. From the repository root, run `script/measure-app 60`, or pass the display's refresh rate.
3. Keep each window frontmost until it closes. Do not touch the mouse or keyboard. macOS slows a background app, which shows as frame intervals of 41.7 ms and slower keystrokes.
4. The script prints a table and exits 0 when every target is met. On a failure, rerun the failing mode alone to rule out interference:
   `TRIAD_MEASURE=<mode> target/release/triad-merge target/measure/<lines>/{base,left,right}.txt target/measure/<lines>/out.txt`
5. Full reports are in `target/measure/measure-app.log`. Replace the table in DESIGN.md §A6.7 and the measured table in spec §9.

| Check | 100k lines | 1M lines |
|---|---|---|
| Load and merge | < 100 ms | < 1 s |
| First frame | < 300 ms | < 1.5 s |
| Scroll frame p95 | < 1.5 refresh periods | same |
| Pick, max | < 16 ms | < 16 ms |
| Keystroke, max | < 8 ms | < 8 ms |
| Peak RSS | < 300 MB | < 2 GB |

## Accessibility (DESIGN.md §A6.6, spec §6.5)

### Accessibility tree

`script/ax-tree.swift` prints the tree that VoiceOver reads, after pressing the elements named on its command line. The terminal needs Accessibility permission.

```sh
target/release/triad-merge fixtures/demo/{base,left,right}.txt /tmp/out.txt & pid=$!
swift script/ax-tree.swift $pid "Next conflict" "Toggle left in the current region"
kill $pid
```

Check each window:

- Merge window: a `Merge tools` toolbar of named buttons; an `Input lines` table whose cells are named for their pane, line number and text; pick targets as `AXCheckBox/AXToggle` named "<pane> in region n of m", with ", unresolved" and value 0 on an unresolved conflict, value 1 after the press above; a `Result` text area; the status bar as `AXGroup/AXApplicationStatus`, showing one fewer unresolved conflict after the press.
- Diff window (`triad-merge diff A B`): `Left: A` and `Right: B` text areas, named copy buttons, the status bar.
- Diff window on a jj temporary copy (a path under a `jj-diff-<random>/left` directory): the banner as `AXGroup/AXApplicationAlert`.
- Open dialog (`triad-merge` with no arguments): four named text fields, four `Choose the … file` buttons, `Merge`. Press `Merge` with the fields empty: `Choose a file for Base.` appears as `AXGroup/AXApplicationAlert`.

### VoiceOver

Turn VoiceOver on (Command-F5) and open the demo merge.

1. Move through the window with VO-Right. Every control is reached and spoken by name.
2. Interact with the input table (VO-Shift-Down). A cell speaks its pane, line number and text.
3. On an unresolved conflict, reach a pick toggle. VoiceOver speaks its name with "unresolved", and not as selected. Press VO-Space: VoiceOver speaks it as selected; the result and status bar change.
4. Move to the `Result` text area, interact, and read a line with VO-L. The text is the merge result.
5. In the open dialog, press `Merge` with an empty field. VoiceOver speaks the error.

Known gaps, recorded in `docs/gpui-kit-issues.md`: status changes are not announced; region bands and gutter marks are not exposed; only the rows on screen are in the table.

## Merge and diff tool integration (DESIGN.md §A7.3, spec §7.1)

`tests/tool_integration.rs` covers the command lines and jj's reading of the output. These steps cover what needs the window. Run them once for git and once for jj, in a scratch repository and with a scratch config, so your own config is untouched. A failed row becomes a bug with a test that reproduces it; PLAN.md §1 states which rows need a rerun.

### Setup

Install the local build (DESIGN.md §A7.4), with `triad-merge` on `PATH` as README § Build and install says, and point both VCSs at scratch configs:

```sh
script/macos-app && rm -rf /Applications/triad.app && cp -R target/release/triad.app /Applications/
export T=$(mktemp -d)
export GIT_CONFIG_GLOBAL=$T/gitconfig JJ_CONFIG=$T/jj.toml
git config --global user.name triad; git config --global user.email triad@example.com
jj config set --user user.name triad; jj config set --user user.email triad@example.com
```

A conflict in `a.txt`, for git:

```sh
git init -q $T/git && cd $T/git
printf '1\n2\n3\n' > a.txt && git add a.txt && git commit -qm base
git checkout -qb side && printf '1\nside\n3\n' > a.txt && git commit -qam side
git checkout -q - && printf '1\nmain\n3\n' > a.txt && git commit -qam main
git merge side   # reports a conflict in a.txt
```

The same conflict, for jj:

```sh
jj --quiet git init $T/jj && cd $T/jj   # colocated, jj's default
printf '1\n2\n3\n' > a.txt && jj commit -m base
printf '1\nside\n3\n' > a.txt && jj commit -m side
jj new 'description(substring:base)' && printf '1\nmain\n3\n' > a.txt && jj commit -m main
# main first, so it is the left side, as git's LOCAL is.
jj new 'description(substring:main)' 'description(substring:side)'   # @ has a conflict in a.txt
```

Between steps 3 to 5, recreate the conflict: `git merge --abort && git merge side`, or `jj undo` after a resolve that changed the file.

### Steps

The same steps for both. `TOOL` is `git mergetool a.txt` or `jj resolve a.txt`; `DIFF` is `git difftool -- a.txt` or `jj diff --tool triad a.txt`.

| # | Step | git | jj |
|---|---|---|---|
| 1 | `triad-merge install --git` / `--jj`, then again | Prints `installed in $T/gitconfig`, then `already installed in …` | Same, with `$T/jj.toml` |
| 2 | Open the config | `merge.tool`, `[mergetool "triad"]`, `diff.tool`, `[difftool "triad"]` as spec §7.1, with the installed binary's absolute path (`/Applications/triad.app/Contents/MacOS/triad-merge`) | `ui.merge-editor` and `[merge-tools.triad]` as spec §7.1 |
| 3 | `TOOL`; change nothing; Cancel (⌘W) | The window shows left and right, and base after ⌘B (hidden by default, spec §5); titles are git's temporary file names. git reports the merge failed; `a.txt` still has git's markers | Titles are jj's file names; the window title is `a.txt`. jj reports the tool failed; `jj resolve --list` still lists `a.txt` |
| 4 | `TOOL`; Save & Exit (⌘S) with the conflict unresolved; confirm | git reports the merge failed and restores `a.txt` from its backup, with git's own markers: `git mergetool` discards a tool's output on a nonzero exit (spec §6.2); `git status` shows it unmerged. Running `TOOL` again offers to restore the session | `jj resolve --list` still lists `a.txt`; `jj file show -r @ a.txt` shows jj's markers for the same conflict |
| 5 | `TOOL`; pick left (⌘1), which acts on the conflict the window opens on; Save & Exit | `a.txt` is `1 main 3`, staged; `git status` shows no unmerged path | `jj resolve --list` is empty; `jj file show -r @ a.txt` is `1 main 3` |
| 6 | Change `a.txt` in an editor; `DIFF`; edit the right pane; Save (⌘S); close | Left is git's temporary copy of the staged file; right is `a.txt` itself, and the edit persists in the working tree | Left is jj's read-only copy of `@-`; right is `a.txt` itself, no banner shows, and the edit persists in the working copy. With `-r @-`, the jj banner says edits will be lost, and after closing the working copy is unchanged |
| 7 | `triad-merge diff a.txt` after editing `a.txt` | Left titled with the branch (`main`) or `index` when staged | Left titled `@-`; its revision chip shows `@-`'s change ID, not git's detached `HEAD`, and the chip's hover card the description |
| 8 | Step 7 in an internal jj repo (`jj --quiet git init --no-colocate`) | — | Same as jj: the chip shows the change ID |
| 9 | Run 3–5 with a path that has a space (`my file.txt`) | Same results | Same results |
| 10 | `TOOL`; pick left (⌘1); close the window with its close button | The prompt asks "Save your changes?" with Save & Exit as the default; Enter writes `1 main 3` and stages it, as step 5 | Same: `jj resolve --list` is empty |
