# Test repositories

Scripts that build disposable jj repositories to try triad in by hand, and a script that checks triad's jj-enabled commands in them end to end. Each repository takes jj's default layout. Identity, timestamps and change IDs are fixed, so every run gives the same commit and change IDs. The scripts ignore the user's jj config and need `jj` on `PATH`.

| Script | Builds or checks |
|---|---|
| `script/gen-conflict-repo [dest]` | A merge with conflicts in two Rust files in the working copy (`fixtures/conflict-repo/README.md`) |
| `script/gen-jj-repo SCENARIO [dest]` | One repository per feature: `stack`, `sides`, `history`, `absorb`, `dirs` |
| `script/test-jj-repos [SCENARIO...]` | Builds each `gen-jj-repo` scenario in a temporary directory and runs triad's commands in it |

Each generator prints the new repository's path, under `$TMPDIR` without `dest`, and fails when jj does not see the state it meant to build. The repositories stay until you delete them (`docs/residue.md`).

## Quick start

```sh
cd "$(script/gen-jj-repo stack)"     # build a scenario and enter it
script/test-jj-repos                 # from the repo root, after cargo build --workspace
```

| Scenario | What it holds | Try |
|---|---|---|
| `stack` | A stack of conflicted commits; later commits inherit the earlier conflict, and `stack-3` adds its own | `triad-merge stack`; `triad-merge resolve a.txt -r stack-1`, with `--headless --dry-run` to change nothing |
| `sides` | A three-sided conflict in the working copy | `triad-merge resolve s.txt`: one pane per side; the badge shows `3 of 3 sides`, then `2 of 3 sides` |
| `history` | `notes.txt` across nine operations, a deletion and a conflict among them | `triad-merge history notes.txt`; `triad-merge diff notes.txt --op OP` |
| `absorb` | Working-copy fixes over commits A, B and C, on an immutable `base` | `triad-merge absorb`: hunks go to A or B, or stay: `base` wrote the lines, a new file, or it would conflict in A |
| `dirs` | The directory demo's change: `base` holds `fixtures/demo/dirs/left`, the working copy `right` | `triad-merge diff`; `jj diff --tool triad` after `triad-merge install --jj` |

`test-jj-repos` needs the jj release that jj-lib is pinned to, and takes about 5 s.

## gen-jj-repo scenarios

Bookmarks name every commit but `@`. In each repository, `jj log` shows the history below.

### stack: conflicts across a stack

```
@        (no description)          a.txt, b.txt inherited
○  stack-3  change b5               a.txt inherited, b.txt its own
○  stack-2  change c2               a.txt inherited
×  stack-1  change a3               a.txt
○  left     change a3 and b5
○  base     a.txt, b.txt, c.txt
```

`stack-1` to `stack-3` were written on `base` and rebased onto `left`, so `stack-1` conflicts with `left` at a3, and `stack-3` at b5. Try:

- `triad-merge stack`: the window lists the four commits, newest first as `jj log` shows them, with `(inherited)` on each path a parent holds conflicted too. Open `a.txt` under `stack-1`, pick left and use Preview Save: `stack-2` loses its conflict, `stack-3` and `@` keep only `b.txt`. Save & Exit asks first, since the save changes descendants.
- `triad-merge resolve a.txt -r stack-1`: the same from the command line.
- `triad-merge resolve a.txt -r stack-1 --headless --dry-run`: changes nothing.
- After each write, `jj log`, `jj op log` and `jj st`: jj reads what triad wrote without complaint, and `jj undo` reverts it.

### sides: a three-sided conflict

```
@        merge one, two and three   s.txt, 3-sided conflict
├─┬─╮
│ │ ○  three  change s2
│ ○ │  two    change s2 and s7
○ │ │  one    change s2 and s7
├─┴─╯
○  base   s.txt, lines s1 to s9
```

Line s2 differs on all three sides. Line s7 differs on `one` and `two`; `three` keeps base, so jj cancels that side. Try `triad-merge resolve s.txt`: one top pane per side, and a conflict badge of `3 of 3 sides` on the first conflict and `2 of 3 sides` on the second.

### history: one file across the operation log

`notes.txt` goes through these states in the working-copy commit, oldest first, each step snapshotted by a jj command: absent; `v1`, committed as `one`; `v2`; `v3`; deleted; `v4`, committed as `four`; `v1` again, as `@` moves onto `one`; `other`, committed as `other`; conflicted, as `@` becomes the merge of `four` and `other`. Try:

- `triad-merge history notes.txt`: nine versions, newest first, the deletion and the start marked `(absent)` and the merge `(conflict)`. Open `v2` against the working copy and copy a line back.
- `triad-merge diff notes.txt --op OP`, with an ID from `history --list`.

### absorb: working-copy fixes over a stack

```
@        (no description)           fixes in f.txt, new.txt added
○  C     change f13
○  B     change f9
○  A     change f3, f6 and f12
◆  base  f.txt, lines f1 to f14     immutable
```

The repository's config makes `base` immutable (`revset-aliases."immutable_heads()"`). The working copy changes f1, f3, f6, f9 and f12, and adds `new.txt`. Try `triad-merge absorb`:

| Hunk | Goes | Why |
|---|---|---|
| f1 | stays | `base` wrote it, and `base` is outside `--into` (`mutable()`) |
| f3, f6 | `A` | `A` wrote them |
| f9 | `B` | `B` wrote it |
| f12 | stays: would conflict in `A` | `C` changed f13, next to it |
| `new.txt` | stays, unlisted | New files stay |

Send f12 to `A` anyway and Apply: `A` and its descendants then hold a conflict, and the report names them.

### dirs: changed files against a commit

```
@        (no description)           fixtures/demo/dirs/right
○  base  the demo's left directory  fixtures/demo/dirs/left
```

The working copy holds twelve changes, two of them renames and one a copy; `fixtures/demo/README.md` lists each. `jj diff --summary` names the same renames and copy as triad. Try:

- `triad-merge diff`: the directory window, against `@-`. Open `invoice/io/parser.py`: `invoice/parser.py` at `@-` on the left, read-only. Copy a line across, save and close: the list comes back, read again after a snapshot.
- `jj diff --tool triad`: jj's dir mode passes two directories of the changed files; the same list opens.

## What test-jj-repos checks

Expected output comes from each scenario's design, written by hand; change and commit IDs come from jj, by bookmark. jj's CLI reads back every write. The script needs a built binary (`cargo build --workspace`) and the jj release that jj-lib is pinned to, and refuses any other. `script/checks` runs it on a full run and when `gen-jj-repo`, the script or `fixtures/demo/dirs` changes. It takes about 5 s.

| Scenario | Checks |
|---|---|
| stack | `stack --list` text and exit 1. `resolve a.txt -r stack-1` with a pick of left, as a dry run and as a write: the report, exit 0, a dry run changes nothing, each commit's conflicts after the write, `stack-1`'s text, `@` keeps its change, one head, and one operation, `resolve a.txt in CHANGE in triad` (spec §7.5.1) |
| sides | A headless save exits 1 and writes jj's snapshot-style markers, sides and bases in turn (spec §4.4); jj reads them back as the same conflict, so `@` keeps its commit and no operation is written (spec §7.4) |
| history | `history --list` gives nine versions in order with their marks; jj, at each listed operation, shows the text the list implies |
| absorb | `absorb --list` text, and `--apply`: the report, the text of `A` and `B`, what `@` keeps, no conflicted commit, one operation, `absorb in triad` |
| dirs | `diff --inspect` text and exit 1, with and without `--rev base`; the renames and copy `jj diff --summary` names; `jj diff --tool` in dir mode with triad's `diff --inspect` as the tool prints the same list |

The Rust tests cover the same features through jj-lib, case by case (`DESIGN.md` part I). These scripts add the built binary, its argument parsing and report text, and jj's own CLI reading the result.
