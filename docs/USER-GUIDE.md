# triad user guide

How to merge, diff and resolve with triad. jj comes first, then git, then plain files. `docs/SPEC.md` sets the exact behaviour; this guide shows the commands.

Keys are macOS keys. On Linux and Windows, ⌘ is Ctrl and ⌥ is Alt.

## Contents

1. [Set up](#1-set-up)
2. [The windows](#2-the-windows)
3. [jj](#3-jj)
4. [git](#4-git)
5. [Files outside a repository](#5-files-outside-a-repository)
6. [Scripts and agents](#6-scripts-and-agents)
7. [Settings](#7-settings)
8. [Exit codes](#8-exit-codes)

## 1. Set up

Install triad as the README says, then register it once per VCS:

```sh
triad-merge install --jj     # jj resolve, jj diff --tool triad
triad-merge install --git    # git mergetool, git difftool
```

`install` names the binary it runs from. Rerun it after moving the binary.

triad finds the repository from the working directory. It reads and writes jj and git repositories in-process, so the commands in this guide work without `jj` or `git` installed. A directory with both `.jj` and `.git` is a jj repository.

## 2. The windows

### The merge window

Left and right sit side by side, each showing its changes from base. The editable result sits below. Changes on one side, or the same change on both, apply on their own. A conflict, marked `!`, waits for you; until then the result previews both sides, and a save writes conflict markers there.

Resolve a conflict by picking sources, or type in the result. Each region's first row has a pick target per source: `●` picked, `◐` previewed in an unresolved conflict, `○` not picked. Several picks write in the order you click them. The toolbar's Left and Right buttons toggle the same picks in the current region and show pressed while it holds them. Its More menu holds Base, ticked while picked, both sides in either order, and None, which picks nothing.

| Action | Key |
|---|---|
| Next / previous conflict | ⌘] / ⌘[ |
| Next / previous difference | ⌘↓ / ⌘↑ |
| Go to the other end of a moved block | ⇧⌘M |
| Toggle left / base / right in the current region | ⌘1 / ⌘2 / ⌘3 |
| Take left then right / right then left | ⌘4 / ⌘5 |
| Clear the pick | ⌘0 |
| Undo / redo (picks and edits share one history) | ⌘Z / ⇧⌘Z |
| Show the base pane / the line origins | ⌘B / ⌘I |
| Collapse unchanged regions | ⌘U |
| Find / go to line | ⌘F / ⌘L |
| Save and exit / cancel | ⌘S / ⌘W |
| Save a copy elsewhere, leaving the merge open | ⇧⌘S |

The toolbar can take one side for every conflict left. The ruler at the right marks each change; click a mark to go there. Closing without a save exits 2; the VCS keeps its conflict.

#### Moved and duplicated blocks

A block a side moved from one place to another shows teal at both ends in that side's pane, with a pill on each naming the other end, such as `↓ L40` or `↑ B1`. `B` names a line of base. With the base pane shown, the block's base lines take a pill for each side that moved it. Click the pill, or press ⇧⌘M in a region that holds an end, to go to the other end; going to a base line shows the base pane. The result pane shows no moves: a moved block there is marked with the side it came from, like any other text. When both sides add the same block at different places, by moving it or as new text, the merge holds it twice and reports no conflict. The status bar then counts it, `1 duplicated block`, and the ruler outlines both copies in the conflict colour. Right-click a copy for `Suggested: Drop Duplicate of Line N`, which deletes that copy and keeps the one on line N, in one undo step. Each copy offers it, so you choose which place keeps the block.

### The diff window

Two aligned, editable panes, each with copy to the other side. A block deleted in one place and inserted once in another shows teal at both ends, with an `M` beside it and a pill naming the other end's line, such as `R5 →`; click the `M` or the pill, or press ⇧⌘M with the caret in the block, to go to the other end. ⌘S writes each edited side to its own file and keeps the window open. Each side has its own undo. A file changed on disk reloads, or asks first when the pane has unsaved edits. When one side is a past revision, that side is read-only.

### The directory list

Given two directories, `diff` lists the files that differ, counted in its header. An icon shows how each file changed: added, modified, deleted or moved; a copy shows the added icon. Hover the icon to name the change. The file name comes first, then its directory. Enter or a click opens a file in the diff window; closing that window lists the files again. Binary files are listed and do not open. A file renamed or moved, alone or with edits that keep half its bytes, is listed once, ends with `from old.rs`, and opens against its old file, as `jj diff` pairs renames. A new file that keeps half the bytes of a modified file ends with `copied from a.rs` and opens against that file's old side, as `jj diff` finds copies.

### Line origins

Line origins (⌘I) show the commit, author and jj change that wrote each line, in both windows.

### Text encodings

A UTF-8, UTF-16 LE or UTF-16 BE BOM sets a file's encoding. Without one, valid UTF-8 is UTF-8 and anything else is Windows-1252, which maps every byte to a character and back, so Latin-1 files merge to the bytes git and jj give. A file in another legacy encoding, such as Shift-JIS, shows garbled, and its bytes come back unchanged where no one edits them. A save refuses a character the file's encoding cannot hold, such as `ł` in a Windows-1252 file, names it and its line, and writes nothing. A file with a NUL byte in its first 8000 bytes is binary and does not open.

### Unusual characters

Both windows show each character that draws nothing, or changes how the text around it draws, as a grey badge with its code point, such as `U+202E`. These are control characters, bidirectional controls, zero-width characters, tag characters and similar; spec §5 lists them. A bidirectional control reorders nothing, so text that hides a Trojan Source trick reads in its true order. A space other than an ordinary space, such as a no-break space, draws on the same grey. A badge is one character: the caret steps over it in one move, and copying copies the character. `fixtures/demo/README.md` has a merge to try.

## 3. jj

Sections without a mark work the same in git (§4). Those marked *jj-enabled* rest on what jj records: conflicts in commits, the operation log and the commits that wrote each line. git has no form of them.

### From jj's own commands

`triad-merge install --jj` makes triad jj's merge editor (`ui.merge-editor`) and adds it as the diff tool `triad`. jj then calls triad itself:

```sh
jj resolve                          # each conflicted file in @, in turn
jj resolve src/lib.rs               # one file
jj resolve -r xyz src/lib.rs        # a conflict in another commit; jj rewrites it and rebases the descendants
jj resolve --list                   # the conflicted files, without opening triad
jj diff --tool triad                # the files @ changed, in a list; each opens in the diff window
jj diff --tool triad src/lib.rs
jj diff --tool triad -r xyz         # what commit xyz changed
jj diff --tool triad --from main --to xyz src/lib.rs   # any two revisions
```

Under `jj resolve`, a save with conflicts left exits 1. jj reads the saved markers back and keeps the rest of the conflict in the commit. Cancel (exit 2) leaves the conflict as it was.

`install --jj` sets `diff-invocation-mode = "dir"`, so jj passes two directories holding the changed files and triad lists them (§2). `jj diff --tool` passes temporary copies. When a right-side file is the working copy, triad opens the working-copy file instead, so edits there persist; every other side's edits are lost, and a banner says so. This needs `diff-do-chdir = false`, which `install --jj` writes.

`jj split -i`, `jj squash -i` and `jj diffedit` use jj's own diff editor (`docs/PLAN-diff-editor.md`).

The `triad-merge` commands below do the same work and more, without the `jj` binary.

### Resolve conflicts in the working copy

```sh
triad-merge resolve           # the first conflicted file, then the next after each save
triad-merge resolve src/lib.rs
```

A conflict of more than two sides shows each side in a pane of its own. Sides count as jj shows them: jj drops pairs of terms that cancel. The conflict badge in the toolbar names the sides the current conflict keeps: `Conflict 3 of 10 · 2 of 3 sides · 5 unresolved`.

Each save is its own jj operation, `save PATH in triad`; `jj undo` reverts it. A save with conflicts left exits 1 and keeps them in the commit as a conflict with fewer hunks. Build on it and come back later:

```sh
triad-merge resolve a.rs      # resolve two of three conflicts and save
jj new                        # work on top
jj edit xyz                   # back to the conflicted commit
triad-merge resolve a.rs      # finish
```

Each save in a commit with descendants rebases them, as any jj rewrite does. A descendant of a partly resolved commit can show more than two sides for a conflict: jj [rebases a conflicted file](https://docs.jj-vcs.dev/latest/technical/conflicts/) by combining whole versions of it, and triad shows each version in a pane of its own. Resolving in the ancestor clears it in every descendant.

Without a window: `triad-merge resolve --headless src/lib.rs` writes the automatic merge, with markers for the conflicts.

### Diff the working copy

```sh
triad-merge diff src/lib.rs        # against @-; edits save to the working copy
triad-merge diff                   # the changed files against @-, in a list
triad-merge diff --rev 'trunk()'   # the changed files against trunk()
```

`triad-merge diff` with no file snapshots the working copy first, as jj's commands do, then lists the files that differ. Each opens against the revision; edits save to the working copy.

When `@` is a merge, the left side is the file in the merge of the parents' trees, as `jj diff` shows it.

### Diff against older revisions

`--rev` takes any revset, with your revset aliases. A revset of several commits reads from the merge of their trees.

```sh
triad-merge diff src/lib.rs --rev 'trunk()'
triad-merge diff src/lib.rs --rev xyz
triad-merge diff src/lib.rs --rev '@--'
```

The left side is read-only; the right is the working-copy file, and edits save there. A file absent at the revision shows empty, its title ending in "(no file)".

To compare two revisions, neither the working copy, use `jj diff --tool triad --from abc --to xyz` (§3, "From jj's own commands").

### Diff against an earlier state of the working copy (jj-enabled)

jj snapshots the working copy at every jj command, and its operation log keeps each snapshot. Code you overwrote between two commits is still there.

```sh
triad-merge history src/lib.rs                  # a window on each version of the file, newest first
triad-merge history src/lib.rs --limit 20       # the newest 20; stops reading the log there
triad-merge history src/lib.rs --list           # print the list instead
triad-merge diff src/lib.rs --op f5a69619a1f9   # one version against the working copy
```

`history` opens a window listing each version, under the operation where it first appeared. Click one, or select it with ↑ and ↓ and press Enter, to open it in the diff window against the working-copy file: the version on the left, read-only, the file on the right. Copy lines back and save to restore them. One diff window is open at a time. When it closes, the list loads again and a save shows as a new version at the top. Closing the history window, or Escape or ⌘W, exits 0.

`--list` prints the same list:

```
15eaadcad368 2026-11-13T15:35:02+00:00 snapshot working copy
f5a69619a1f9 2026-11-13T15:35:02+00:00 snapshot working copy
f09f484b2665 2026-11-13T15:35:02+00:00 add workspace 'default' (absent)
```

`(absent)`, `(conflict)` and `(other)` mark an operation where the path held no file, a conflict, or a symlink or directory. `diff --op` opens one version without the window; `--op` takes what `jj --at-op` takes: an ID prefix, `@`, `@-`. The history is as fine as jj's snapshots: one per jj command, or more with jj's file watcher.

### Resolve conflicts in any commit (jj-enabled)

jj records conflicts in commits, so a rebase never stops halfway. triad resolves them where they sit, without `jj edit`.

```sh
triad-merge stack                     # a window of the conflicted mutable commits, newest first
triad-merge stack 'trunk()..@'        # any revset
triad-merge stack --list              # print the list instead
triad-merge resolve src/lib.rs -r xyz # resolve in commit xyz; @ stays where it is
```

`stack` opens a window that lists each commit's change, commit and summary, then its conflicted paths. `(inherited)` marks a path whose parent holds a conflict there too: resolve the parent, listed below it, first and the child often clears with it. Click a path, or select it with ↑ and ↓ and press Enter, to open its conflict in the merge window, as `resolve -r` does. One merge window is open at a time. Each save lists the commits again and says what it did. With a merge window open, Preview Save shows what saving its result would do to each descendant's conflicts. Like any jj command it snapshots the working copy first; it changes nothing else. Closing the stack window, or Escape or ⌘W, ends the run: 0 when no conflicted commit is left, 1 otherwise. With nothing conflicted, `stack` opens no window and exits 0.

`stack --list` prints the same list and exits the same way.

`-r` takes any revset that names one commit, with your revset aliases, as `stack` does, and snapshots the working copy first.

Each save to `-r` rewrites the commit and rebases its descendants, as any jj rewrite does. The headless report lists each descendant with its conflicted paths before and after:

```sh
triad-merge resolve src/lib.rs -r xyz --headless --dry-run   # what the automatic merge would do; changes nothing
triad-merge resolve src/lib.rs -r xyz --headless
```

```
resolved src/lib.rs in xyz
  pqr: src/lib.rs -> none
```

In the window, Save & Exit first works out what the save would do, changing nothing. When it changes the conflicts of any descendant, it asks first, and lists each such descendant as above: Save writes, Keep Editing writes nothing.

jj refuses a write to an immutable commit, and so does triad, with your `revset-aliases`. A save is also refused when the conflict in the commit changed after triad read it, or after triad's last save, for example by a `jj rebase` while the window was open; nothing is written, and you open it again. If the working copy cannot be updated after a write, the write stands and triad tells you to run `jj workspace update-stale`.

### Absorb working-copy edits into the commits that wrote the lines (jj-enabled)

Fix lines across a stack of commits in the working copy, then send each hunk to the commit that last changed the lines it replaces.

```sh
triad-merge absorb                  # a window of the hunks and the commits they go into
triad-merge absorb src/ --into @-   # only these paths, only these commits
triad-merge absorb --list           # print the list instead
```

`absorb` opens a window listing each hunk, its lines and the commit it goes into or why it stays, beside the commits a hunk may go into and Working copy. Click a hunk, then a commit to send it there or Working copy to keep it; the next hunk is then selected. From the keys: ↑ and ↓ select a hunk, Enter moves to the commits, ↑ and ↓ pick one, Enter sends the hunk there and Escape comes back without a change. Each commit shows how many hunks go into it. Apply writes the choices, says how many hunks went into how many commits and names each commit whose conflicts changed, and lists what is left. Apply refuses when the repository moved on since the list: Refresh and choose again. Closing the window, ⌘W, or Escape in the hunks exits 0. With nothing to absorb, `absorb` opens no window and exits 0.

`absorb --list` prints the list; it writes only with `--apply`:

```
at operation 6c1e0e2b7f3a
1 lib.rs left 0..1 right 0..1 -> womuzyvr 9843b8e4
2 notes.txt left 2..3 right 2..3 -> qpnysvsn 14ede62f
3 notes.txt left 9..10 right 9..11 stays: would conflict in qpnysvsn
```

The first line names the operation the listing read. Each further line is a hunk: its number, the path, the lines it replaces and the lines it holds now (from 0, end excluded), then the commit it goes into, or `stays:` and why. Steer it, then apply:

```sh
triad-merge absorb src/                      # only these paths
triad-merge absorb --into 'trunk()..@'       # only these commits (default mutable())
triad-merge absorb --keep 2                  # hunk 2 stays in the working copy
triad-merge absorb --move 1=xyz              # hunk 1 into xyz, a mutable ancestor
triad-merge absorb --apply                   # write, as listed
triad-merge absorb --keep 2 --apply --listed-at 6c1e0e2b7f3a   # write with choices
```

Hunk numbers hold only for the listing that printed them. `--move` and `--keep` with `--apply` need `--listed-at`, the listing's operation, and triad refuses when the repository has moved past it: an edit or a jj command since the listing. List again and choose again.

```
absorbed 1 hunks into 1 commits
  xyz: none -> none
```

After the count come the commits the hunks went into and their descendants, each with its conflicted paths before and after.

A hunk stays when it spans lines from two commits, or when it is an insertion at the seam between two commits' lines. A deletion over two commits' lines stays too, where `jj absorb` splits it and leaves both commits conflicted. A hunk also stays when a commit outside `--into` wrote its lines, and when it touches lines a later commit wrote or removed: sent into its commit, with the hunks before it that go there, it would conflict there. Here triad differs from `jj absorb`, which moves the hunk and leaves the commit conflicted. triad keeps it and says so: `stays: would conflict in CHANGE`. `--move N=CHANGE` sends it anyway, conflict and all. New files stay. Conflicted paths are listed as `skipped`. With `--apply` each target takes its hunks, the descendants rebase, and the working copy keeps the rest; an empty working copy without a description is replaced by a new one. It is one operation, `absorb in triad`, which `jj undo` reverts.

### Merge semantics

In a jj repository triad merges as jj does: jj's diff, `merge.same-change` and `merge.hunk-level` from your jj config. `--merge-semantics git` is refused there.

## 4. git

The features of §3 that rest on what jj records have no git form: conflicts in commits (`stack`, `resolve -r`), the operation log (`history`, `diff --op`) and `absorb`. In a git repository each says `COMMAND needs a jj repository` and exits 3. Everything else works the same in git.

### From git's own commands

`triad-merge install --git` sets `merge.tool` and `diff.tool` to triad in git's global config, with `mergetool.triad.trustExitCode`, so git reads triad's exit code:

```sh
git mergetool                       # each conflicted file, after a merge, rebase, cherry-pick or stash pop
git mergetool src/lib.rs
git difftool                        # working tree against the index, one window per file
git difftool --staged               # index against HEAD
git difftool HEAD~1 -- src/lib.rs   # working tree against a revision
git difftool main feature -- src/lib.rs   # two revisions
git difftool -y                     # without git's prompt per file
git difftool -d                     # every changed file in one list; each opens in the diff window
git difftool -d HEAD~1              # against a revision
```

Under `git mergetool`, a resolved save exits 0 and git stages the file. A save with conflicts left exits 1; git counts the file unresolved and restores its own copy, and triad offers to restore the session on the next run. Cancel exits 2.

`git difftool` passes the real working-tree file on the right, so edits there persist. The left side is a temporary copy and read-only. When neither side is the working tree, as with `--staged` or two revisions, the right side is a temporary copy too: edits to it are lost, and no banner says so. `git difftool -R` puts the working-tree file on the read-only left. `git difftool -d` links each working-tree file into its right directory, so edits there persist too.

In a jj repository use jj's commands: `git mergetool` there exits 3, since triad merges with jj's rules only.

### Resolve conflicts

```sh
triad-merge resolve           # the same, from git's index, without the git binary
triad-merge resolve src/lib.rs
triad-merge resolve --headless src/lib.rs
```

A resolved save is staged, as `git add` does. A save with conflicts left writes markers to the file and keeps the index stages, so the next `resolve` opens what is left.

triad reads git's `merge.conflictStyle` for the markers it writes.

### Diff the working tree

```sh
triad-merge diff src/lib.rs   # against the index when the file has staged changes, else HEAD
triad-merge diff               # the changed files against HEAD, staged or not, in a list
triad-merge diff --rev main    # the changed files against main
```

The list leaves untracked files out, as `git diff` does. Each file opens against the revision; edits save to the working tree.

### Diff against older revisions

`--rev` takes git's revision syntax.

```sh
triad-merge diff src/lib.rs --rev HEAD~3
triad-merge diff src/lib.rs --rev main
triad-merge diff src/lib.rs --rev v0.1.0
```

The left side is read-only and the working-tree file is on the right. To compare two revisions, use `git difftool main feature -- src/lib.rs`.

### Merge semantics

In a git repository triad merges as `git merge-file` does, byte for byte.

## 5. Files outside a repository

```sh
triad-merge                                       # the open dialog: Merge and Diff tabs
triad-merge BASE LEFT RIGHT -o OUTPUT             # the merge window
triad-merge BASE LEFT RIGHT OUTPUT                # the same, as git passes it
triad-merge diff LEFT RIGHT                       # the diff window; each side saves to its own file
triad-merge diff LEFT RIGHT --readonly-left
triad-merge diff LEFT_DIR RIGHT_DIR               # the files that differ; each opens in the diff window
```

`fixtures/demo` holds inputs to try (`fixtures/demo/README.md`):

```sh
triad-merge fixtures/demo/{base,left,right}.txt /tmp/out.txt   # 980 lines, 20 conflicts
triad-merge diff fixtures/demo/dirs/{left,right}               # the directory list
```

Outside a repository triad merges as git does. `--merge-semantics jj` merges as jj does. The two can differ: this triple leaves one conflict as jj merges and none as git does.

```sh
cd "$(mktemp -d)"
printf 'a\nb\nc\na\nb\nc\na\nb\nc\n' > b; printf 'a\nb\n a\na\nb\n c\n' > l; printf 'a \nb\nc\na\nb\n c\n' > r
triad-merge inspect --merge-semantics jj b l r    # exit 1
triad-merge inspect --merge-semantics git b l r   # exit 0
```

## 6. Scripts and agents

Every command has a form without a window.

```sh
triad-merge --headless BASE LEFT RIGHT -o OUTPUT                   # the automatic merge, markers for conflicts
triad-merge --headless BASE LEFT RIGHT -o OUTPUT --commands FILE   # then FILE's picks and edits
triad-merge inspect --json BASE LEFT RIGHT                         # the merge's regions
triad-merge diff --inspect --json LEFT RIGHT                       # the diff's regions
triad-merge diff --inspect --json LEFT_DIR RIGHT_DIR               # the files that differ between two directories
triad-merge diff --inspect --json [--rev REV]                      # the working copy's changed files
triad-merge resolve PATH -r REV --headless --commands FILE --dry-run   # jj: picks applied in a commit, reported, not written
triad-merge stack --list
triad-merge stack --json
triad-merge history PATH --list
triad-merge history PATH --json
triad-merge absorb --list
triad-merge absorb --json
```

`--commands` takes JSON Lines, one command per line, each naming a region by its index in `inspect`'s output. `docs/json-schema.md` defines the commands and every JSON output.

Everything triad prints, reports and errors alike, writes each unusual character as a JSON escape, `\u202e`, so a path or commit description cannot rewrite the terminal.

## 7. Settings

The config file is `~/.config/triad/config.triad` (`$XDG_CONFIG_HOME` if set, or `TRIAD_CONFIG`), one `key = value` per line. Settings… (⌘,) opens it. Five keys are also flags that override the file for one run: `--diff-algorithm`, `--ignore-whitespace`, `--merge-semantics`, `--same-change` and `--hunk-level`. The diff window takes the first two.

```sh
triad-merge show-config --default --docs   # every key, its default and what it does
triad-merge show-config                    # the settings in effect
```

Bind a key with `keybind = CHORD=ACTION`, for example `keybind = cmd+j=next-conflict`.

## 8. Exit codes

| Code | Merge, `resolve` | `diff --inspect` | `stack` | Other commands |
|---|---|---|---|---|
| 0 | All resolved | Equal | No conflicted commit | Done |
| 1 | Conflicts left | Different | Conflicted commits listed or left | — |
| 2 | Cancelled | — | — | — |
| 3 | Error, or a jj-enabled command in git | Error | Error | Error |
