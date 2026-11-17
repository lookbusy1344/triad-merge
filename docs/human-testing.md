# Human testing

A script for testing triad by hand: plain diff and merge first, then each jj feature, ending with stacks and absorb. Each step gives the commands, the output they print and what the window shows. A step whose output differs from this page is a failure: note the step number and what you saw.

Every printed result below is real output from the fixtures and the scenario repositories. Window behaviour follows `docs/USER-GUIDE.md`. git is out of scope here; `docs/release-checklist.md` covers it.

Keys are macOS keys. On Linux and Windows, ⌘ is Ctrl and ⌥ is Alt.

## Contents

0. [Set up](#0-set-up)
1. [Diff two files](#1-diff-two-files)
2. [Merge three files](#2-merge-three-files)
3. [Unusual characters](#3-unusual-characters)
4. [Moved and duplicated blocks](#4-moved-and-duplicated-blocks)
5. [Diff two directories](#5-diff-two-directories)
6. [jj and git merge rules](#6-jj-and-git-merge-rules)
7. [Resolve conflicts in the jj working copy](#7-resolve-conflicts-in-the-jj-working-copy)
8. [Resolve from `jj resolve`](#8-resolve-from-jj-resolve)
9. [Diff the jj working copy](#9-diff-the-jj-working-copy)
10. [A three-sided conflict](#10-a-three-sided-conflict)
11. [File history from the operation log](#11-file-history-from-the-operation-log)
12. [Conflicts across a stack](#12-conflicts-across-a-stack)
13. [Absorb](#13-absorb)
14. [Clean up](#14-clean-up)

## 0. Set up

Needs `jj` 0.47.1, the release jj-lib is pinned to, on `PATH`. Run every command in this page in one shell; the variables carry from step to step.

```sh
cd path/to/triad-merge          # the repository root
jj --version                    # jj 0.47.1
cargo build --workspace

export TRIAD_ROOT=$PWD
export PATH="$TRIAD_ROOT/target/debug:$PATH"
export TEST=$(mktemp -d "${TMPDIR:-/tmp}/triad-human.XXXXXX")

# A jj config and a triad config of their own, so neither your settings nor
# `install` touch your real ones.
export JJ_CONFIG=$TEST/jj-config.toml
export TRIAD_CONFIG=$TEST/config.triad
cat >"$JJ_CONFIG" <<'EOF'
[user]
name = "triad Tester"
email = "tester@example.invalid"
[ui]
paginate = "never"
EOF

triad-merge install --jj
```

`install` prints `triad-merge: installed in …/jj-config.toml` and adds to that file:

```toml
[ui]
merge-editor = "triad"

[merge-tools.triad]
program = "…/target/debug/triad-merge"
merge-args = ["$base", "$left", "$right", "-o", "$output", "--title", "$path", "--marker-length", "$marker_length", "--merge-semantics", "jj"]
merge-tool-edits-conflict-markers = true
merge-conflict-exit-codes = [1]
conflict-marker-style = "git"
diff-args = ["diff", "$left", "$right"]
diff-invocation-mode = "dir"
diff-do-chdir = false
```

The scenario repositories come from `script/gen-jj-repo` and `script/gen-conflict-repo`. Their commits carry the author `fixture@example.invalid` and fixed change and commit IDs, so the IDs on this page match yours. Two kinds of ID differ per run: operation IDs, and the commit IDs and timestamps of commits rewritten during a test. Steps that need an operation ID read it from triad's output.

`echo $?` after a command prints its exit code. triad's codes: 0 done or resolved, 1 conflicts left or files differ, 2 cancelled, 3 error.

## 1. Diff two files

### 1.1 Equal files

```sh
cd "$TRIAD_ROOT"
triad-merge diff --inspect fixtures/demo/base.txt fixtures/demo/base.txt; echo $?
```

```
unchanged left 0..980 right 0..980
changes: 0
0
```

Ranges count lines from 0, end excluded.

### 1.2 A moved block

`moved_base.py` is a 70-line module of four-line functions. `moved_left.py` moves four of them: `parse_header`, `tax_rate`, `round_cents` and `apply_discount`.

```sh
cp fixtures/demo/moved_{base,left}.py "$TEST/"
triad-merge diff --inspect "$TEST"/moved_{base,left}.py; echo $?
```

```
unchanged left 0..6 right 0..6
changed left 6..10 right 6..6
unchanged left 10..14 right 6..10
changed left 14..18 right 10..10
unchanged left 18..22 right 10..14
changed left 22..26 right 14..14
unchanged left 26..30 right 14..18
changed left 30..34 right 18..18
unchanged left 34..46 right 18..30
changed left 46..46 right 30..38
unchanged left 46..50 right 38..42
changed left 50..50 right 42..46
unchanged left 50..54 right 46..50
changed left 54..54 right 50..54
unchanged left 54..58 right 54..58
changed left 58..58 right 58..62
unchanged left 58..70 right 62..74
move from 1 left 6..10 to 15 right 58..62
move from 3 left 14..18 to 9 right 30..34
move from 5 left 22..26 to 11 right 42..46
move from 7 left 30..34 to 13 right 50..54
changes: 8
1
```

`move from 1 left 6..10 to 15 right 58..62` is `parse_header`: base lines 7–10 (1-based) land at right lines 59–62, after `close_file`. Region 9, `left 46..46 right 30..38`, is the new `clamp` plus the moved `tax_rate`; only `tax_rate`'s four lines are a move.

Open the window on the copies, so saves leave the fixtures alone:

```sh
triad-merge diff "$TEST"/moved_{base,left}.py
```

- [ ] Two panes, aligned. The status bar starts `no repo · 8 differences`.
- [ ] `parse_header` (left lines 7–10) is teal, with an `M` and a pill `R59 →`. The right copy at lines 59–62 is teal with a pill back to `L7`.
- [ ] Click the pill: the view jumps to the other end. ⇧⌘M with the caret in the block does the same.
- [ ] Copy a block from left to right with the arrow, then ⌘Z: the right pane goes back. Each side has its own undo.
- [ ] Type in the right pane and press ⌘S: `$TEST/moved_left.py` holds the edit and the window stays open.
- [ ] ⌘W closes the window.

## 2. Merge three files

`fixtures/demo/base.txt` is a 980-line Rust source. Left and right each change about 30% of it.

### 2.1 The regions

```sh
cd "$TRIAD_ROOT"
triad-merge inspect fixtures/demo/{base,left,right}.txt > "$TEST/demo-inspect.txt"; echo $?
awk '{print $1}' "$TEST/demo-inspect.txt" | sort | uniq -c
tail -1 "$TEST/demo-inspect.txt"
```

```
1
  16 both-same
  20 conflict
   1 conflicts:
  50 left-only
  52 right-only
 138 unchanged
conflicts: 20
```

The first lines of the inspect output:

```
unchanged base 0..3 left 0..3 right 0..3
left-only base 3..8 left 3..8 right 3..8
unchanged base 8..15 left 8..15 right 8..15
both-same base 15..15 left 15..16 right 15..16
conflict base 15..15 left 16..17 right 16..17
```

### 2.2 The automatic merge, without a window

```sh
triad-merge --headless fixtures/demo/{base,left,right}.txt -o "$TEST/demo-merged.txt"; echo $?
grep -c '^<<<<<<<' "$TEST/demo-merged.txt"
sed -n '15,24p' "$TEST/demo-merged.txt"
```

```
1
20
/// Side index of left in a three-way merge.
<<<<<<<

// Checked against git merge-file.
|||||||
=======

tracing::trace!("right path");
>>>>>>>
pub const LEFT: usize = 0;
```

Both sides inserted a blank line after line 15, then each added its own line. Outside a repository triad merges as `git merge-file` does, and git folds the shared blank line into the conflict.

### 2.3 Reference results

Two headless runs that take one side in every conflict, to compare against the window's saves:

```sh
echo '{"PickAll":{"scope":"Unresolved","sources":[{"Side":0}]}}' > "$TEST/all-left.jsonl"
echo '{"PickAll":{"scope":"Unresolved","sources":[{"Side":1}]}}' > "$TEST/all-right.jsonl"
triad-merge --headless fixtures/demo/{base,left,right}.txt -o "$TEST/ref-left.txt" --commands "$TEST/all-left.jsonl"; echo $?
triad-merge --headless fixtures/demo/{base,left,right}.txt -o "$TEST/ref-right.txt" --commands "$TEST/all-right.jsonl"; echo $?
wc -l < "$TEST/ref-left.txt"; wc -l < "$TEST/ref-right.txt"
```

```
0
0
     944
     958
```

### 2.4 The merge window

```sh
triad-merge fixtures/demo/{base,left,right}.txt "$TEST/window-left.txt"; echo $?
```

- [ ] Left and right sit side by side; the result sits below.
- [ ] The status bar reads `no repo · git merge · 138 differences · 20 conflicts · 20 unresolved · UTF-8 · LF · whitespace significant`.
- [ ] ⌘] goes to the first conflict, result line 16. The badge reads `Conflict 1 of 20 · 20 unresolved`. Left shows `// Checked against git merge-file.`, right `tracing::trace!("right path");`.
- [ ] The conflict's first row carries `◐` targets: both sides previewed, none picked.
- [ ] ⌘1 picks left: the target turns `●`, the result holds the left line, the badge drops to `19 unresolved`. ⌘Z undoes it; ⇧⌘Z redoes it.
- [ ] ⌘4 takes left then right; ⌘5 right then left; ⌘0 clears the pick.
- [ ] ⌘B shows the base pane; ⌘U folds the unchanged regions; ⌘L 500 goes to line 500.
- [ ] Click a mark on the ruler at the right: the view goes there.
- [ ] Use the toolbar to take left for every conflict left. The status bar reads `0 unresolved`. ⌘S saves and exits; the shell prints `0`.

```sh
cmp "$TEST/window-left.txt" "$TEST/ref-left.txt" && echo same
```

```
same
```

Repeat with the toolbar's right side into `$TEST/window-right.txt` and compare with `ref-right.txt`.

- [ ] Open the merge again, pick left in one conflict and close with ⌘W. A prompt offers Discard, which writes nothing; choose it. The shell prints `2` and the output file is not written.
- [ ] A later run on the same files may ask `Restore the previous session?`. Restore brings back the picks; Start Over drops them.
- [ ] Open again, resolve two conflicts, press ⇧⌘S and save a copy to `$TEST/copy.txt`. The status bar says `Copy saved with 18 conflicts` and the merge stays open.

## 3. Unusual characters

`unusual_base.py` is a 15-line Python module with CRLF line endings. Line 4 of each file, with invisible characters written as escapes:

```
base:  '    if access_level != "user":  # Check if admin\r\n'
left:  '    if access_level != "none‮ ⁦# Check if admin⁩ ⁦":\r\n'
right: '    if access_level　!= "user":  # Check if admin\r\n'
```

Left hides a Trojan Source trick: a right-to-left override and isolates move the comment so the string reads as `"user"`. Left also puts a zero-width space in `is_ad​min` and a run of tag characters after the first line's comment. Right adds an ideographic space on line 4, no-break-space indentation on line 10, a byte order mark and a zero-width joiner on line 13, a lone carriage return and an escape sequence on lines 14–15, and 40 zero-width spaces on its last line.

```sh
cd "$TRIAD_ROOT"
triad-merge inspect fixtures/demo/unusual_{base,left,right}.py; echo $?
```

```
left-only base 0..2 left 0..2 right 0..2
unchanged base 2..3 left 2..3 right 2..3
conflict base 3..4 left 3..4 right 3..4
unchanged base 4..9 left 4..9 right 4..9
right-only base 9..10 left 9..10 right 9..10
unchanged base 10..12 left 10..12 right 10..12
right-only base 12..15 left 12..15 right 12..15
conflicts: 1
1
```

```sh
triad-merge fixtures/demo/unusual_{base,left,right}.py "$TEST/unusual-merged.py"
```

- [ ] The status bar ends `· 1 unresolved · UTF-8 · CRLF · …`. The CRLF endings draw nothing.
- [ ] Left line 4 reads in its true order: `"none` then a grey badge `U+202E`, a space, `U+2066`, `# Check if admin`, `U+2069`, a space, `U+2066`, `":`. The comment sits inside the string.
- [ ] Left line 2 shows `is_ad`, a `U+200B` badge, `min`. The caret steps over the badge in one move.
- [ ] Left line 1 ends with a run of `U+E00xx` tag badges.
- [ ] Right line 4 shows the ideographic space on grey; right line 10's four no-break spaces are shaded grey.
- [ ] Right line 13 shows `U+FEFF` and `U+200D` badges; line 14 a `U+000D` badge mid-line; line 15 `U+001B` before `[2J`.
- [ ] The last line's 40 `U+200B` badges wrap in a narrow result pane.
- [ ] Cancel with ⌘W.

The diff window shows the same badges:

```sh
triad-merge diff --inspect fixtures/demo/unusual_{left,right}.py; echo $?
```

```
changed left 0..2 right 0..2
unchanged left 2..3 right 2..3
changed left 3..4 right 3..4
unchanged left 4..9 right 4..9
changed left 9..10 right 9..10
unchanged left 10..12 right 10..12
changed left 12..15 right 12..15
changes: 4
1
```

## 4. Moved and duplicated blocks

The same 70-line module as §1.2. Each function is four lines; `script/gen-moved-demo --layout` prints where each one sits.

| Function | Left | Right | Merge shows |
|---|---|---|---|
| `parse_header` | moves it after `close_file` | leaves it | A left move |
| `format_total` | leaves it | moves it to the top | A right move |
| `round_cents` | moves it after `log_line` | the same | Taken once |
| `apply_discount` | moves it after `open_file` | moves it after `count_rows` | Taken twice: a duplicate |
| `clamp` | adds it after the moved `tax_rate` | adds it after `parse_args` | Taken twice: a duplicate |
| `tax_rate` | moves it after `write_report` | changes `0.2` to `0.25` in place | The one conflict |

```sh
cd "$TRIAD_ROOT"
triad-merge inspect fixtures/demo/moved_{base,left,right}.py > "$TEST/moved-inspect.txt"; echo $?
tail -10 "$TEST/moved-inspect.txt"
```

```
1
move left base 6..10 to 58..62
move left base 14..18 to 30..34
move left base 22..26 to 42..46
move left base 30..34 to 50..54
move right base 22..26 to 42..46
move right base 30..34 to 58..62
move right base 38..42 to 2..6
duplicate left 34..38 right 66..70
duplicate left 50..54 right 58..62
conflicts: 1
```

The headless merge holds `tax_rate` twice: the conflict at its old place, and left's moved copy after `write_report`.

```sh
triad-merge --headless fixtures/demo/moved_{base,left,right}.py -o "$TEST/moved-merged.py"; echo $?
sed -n '15,26p;43,46p' "$TEST/moved-merged.py"
```

```
1
<<<<<<<
|||||||
def tax_rate(region):
    """The tax rate for a region."""
    return RATES.get(region, 0.2)

=======
def tax_rate(region):
    """The tax rate for a region."""
    return RATES.get(region, 0.25)

>>>>>>>
def tax_rate(region):
    """The tax rate for a region."""
    return RATES.get(region, 0.2)

```

```sh
triad-merge fixtures/demo/moved_{base,left,right}.py "$TEST/moved-window.py"
```

- [ ] The status bar reads `no repo · git merge · 12 differences · 1 conflict · 2 duplicated blocks · 1 unresolved · UTF-8 · LF · …`.
- [ ] In the left pane, `parse_header` is teal at both ends, with pills such as `↓ L59` and `↑ B7`. Click a pill: the view goes to the other end. A `B` pill shows the base pane.
- [ ] In the right pane, `format_total` is teal at the top and at its old place.
- [ ] The result shows no teal: moved text carries the colour of the side it came from.
- [ ] The ruler outlines both copies of `apply_discount` and both copies of `clamp` in the conflict colour.
- [ ] Right-click a copy of `apply_discount`: the menu offers `Suggested: Drop Duplicate of Line N`, N the other copy's line. Choose it: one copy goes, the status bar shows `1 duplicated block`. ⌘Z brings it back in one step.
- [ ] Right-click the `tax_rate` conflict: the menu offers `Suggested: Carry Right's Edit to Line N`. Choose it: the moved copy takes `0.25`, the conflict resolves, the status bar shows `0 unresolved`.
- [ ] ⌘S exits with `0`.

## 5. Diff two directories

`fixtures/demo/dirs/left` and `right` are a small Python project before and after a change.

```sh
cd "$TRIAD_ROOT"
cp -R fixtures/demo/dirs "$TEST/dirs"
triad-merge diff --inspect "$TEST"/dirs/{left,right}; echo $?
```

```
added binary assets/icon.png
modified binary assets/logo.png
added text docs/design.md
deleted text docs/notes.md
added text invoice/cli.py
renamed text invoice/util.py => invoice/helpers.py
renamed text invoice/parser.py => invoice/io/parser.py
copied text invoice/totals.py => invoice/summary.py
modified text invoice/totals.py
deleted text legacy.txt
added undecodable names-bad-mark.txt
added text names-latin1.txt
files: 12
1
```

`README.md` is the same on both sides and is left out.

```sh
triad-merge diff "$TEST"/dirs/{left,right}
```

- [ ] The header counts 12 files. Each row shows an icon for added, modified, deleted or moved; hover an icon to name the change. The file name comes first, then its directory.
- [ ] `parser.py` in `invoice/io` ends with `from invoice/parser.py`. `summary.py` ends with `copied from invoice/totals.py`.
- [ ] `icon.png`, `logo.png` and `names-bad-mark.txt` are greyed. Enter on them opens nothing.
- [ ] Open `names-latin1.txt`: the status bar names `Windows-1252`.
- [ ] Open `invoice/io/parser.py`: the left pane holds `left/invoice/parser.py`. Close the diff window: the list comes back.
- [ ] Escape or ⌘W closes the list.

## 6. jj and git merge rules

The same three files merge with one conflict under jj's rules and none under git's.

```sh
mkdir "$TEST/rules" && cd "$TEST/rules"
printf 'a\nb\nc\na\nb\nc\na\nb\nc\n' > b
printf 'a\nb\n a\na\nb\n c\n' > l
printf 'a \nb\nc\na\nb\n c\n' > r
triad-merge inspect --merge-semantics jj b l r; echo $?
triad-merge inspect --merge-semantics git b l r; echo $?
```

```
conflict base 0..9 left 0..3 right 0..3
both-same base 9..9 left 3..6 right 3..6
conflicts: 1
1
right-only base 0..1 left 0..1 right 0..1
unchanged base 1..2 left 1..2 right 1..2
left-only base 2..3 left 2..3 right 2..3
unchanged base 3..5 left 3..5 right 3..5
both-same base 5..9 left 5..6 right 5..6
conflicts: 0
0
```

```sh
triad-merge --headless --merge-semantics git b l r -o out; echo $?; cat out
```

```
0
a 
b
 a
a
b
 c
```

In a jj repository triad uses jj's rules only (§7.6).

## 7. Resolve conflicts in the jj working copy

`script/gen-conflict-repo` builds a small Rust crate, `warehouse`, with a merge of two branches in the working copy. Each of its two files holds four conflicts.

### 7.1 What jj shows

```sh
cd "$("$TRIAD_ROOT/script/gen-conflict-repo" "$TEST/conflicts")"
jj log
```

```
@    vruxwmqv fixture@example.invalid 2026-01-01 12:00:00 4580eab1 (conflict)
├─╮  (empty) merge left and right
│ ○  mzvwutvl fixture@example.invalid 2026-01-01 12:00:00 right eddb9f83
│ │  right: quoted values, duplicate-line report, SKU charset, field rename
○ │  qpvuntsm fixture@example.invalid 2026-01-01 12:00:00 left b6b63e96
├─╯  left: INI sections, zero-count check, sorted SKUs
○  oxmtprsl fixture@example.invalid 2026-01-01 12:00:00 base a6c265d7
│  base: warehouse crate with inventory and config modules
◆  zzzzzzzz root() 00000000
```

```sh
jj resolve --list
sed -n '15,24p' src/inventory.rs
```

```
src/config.rs       2-sided conflict
src/inventory.rs    2-sided conflict
<<<<<<< conflict 1 of 4
%%%%%%% diff from: oxmtprsl a6c265d7 "base: warehouse crate with inventory and config modules"
\\\\\\\        to: qpvuntsm b6b63e96 "left: INI sections, zero-count check, sorted SKUs"
-        if code.is_empty() || code.len() > 12 {
+        let code = code.trim();
+        if code.is_empty() || code.len() > MAX_SKU_LEN {
+++++++ mzvwutvl eddb9f83 "right: quoted values, duplicate-line report, SKU charset, field rename"
        let valid = code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if code.is_empty() || code.len() > 12 || !valid {
>>>>>>> conflict 1 of 4 ends
```

That is jj's own marker style: a diff from base to left, then right's text. triad reads the conflict from the commit, not from these markers.

### 7.2 The regions triad sees

`jj file show` gives each input, and `inspect` numbers the regions. The line number `nl -v0` prints is the region's index, which merge commands name.

```sh
mkdir "$TEST/inv"
for r in base left right; do jj file show -r $r src/inventory.rs > "$TEST/inv/$r.rs"; done
triad-merge inspect "$TEST"/inv/{base,left,right}.rs | nl -v0 | grep conflict
```

```
     6	conflict base 11..12 left 14..16 right 11..13
    24	conflict base 85..85 left 94..96 right 86..89
    27	conflict base 89..91 left 102..103 right 95..97
    32	conflict base 140..140 left 154..159 right 148..157
    35	conflicts: 4
```

### 7.3 The automatic merge, without a window

```sh
triad-merge resolve --headless src/inventory.rs; echo $?
sed -n '15,23p' src/inventory.rs
jj resolve --list
```

```
1
<<<<<<< qpvuntsm b6b63e96 "left: INI sections, zero-count check, sorted SKUs"
        let code = code.trim();
        if code.is_empty() || code.len() > MAX_SKU_LEN {
||||||| oxmtprsl a6c265d7 "base: warehouse crate with inventory and config modules"
        if code.is_empty() || code.len() > 12 {
=======
        let valid = code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if code.is_empty() || code.len() > 12 || !valid {
>>>>>>> mzvwutvl eddb9f83 "right: quoted values, duplicate-line report, SKU charset, field rename"
src/config.rs       2-sided conflict
src/inventory.rs    2-sided conflict
```

triad wrote git-style markers with jj's labels. jj reads them back as the same conflict: `jj st` shows no change and `jj op log` no new operation.

### 7.4 Scripted picks

Take left in each of the four conflicts, by region index:

```sh
for i in 6 24 27 32; do echo "{\"Toggle\":{\"region\":$i,\"source\":{\"Side\":0}}}"; done > "$TEST/inv-left.jsonl"
triad-merge resolve --headless --commands "$TEST/inv-left.jsonl" src/inventory.rs; echo $?
jj st
jj op log -n 1 --no-graph -T 'description ++ "\n"'
```

```
0
Working copy changes:
M src/inventory.rs
Working copy  (@) : vruxwmqv 0e6e9ae0 (conflict) merge left and right
Parent commit (@-): qpvuntsm b6b63e96 left | left: INI sections, zero-count check, sorted SKUs
Parent commit (@-): mzvwutvl eddb9f83 right | right: quoted values, duplicate-line report, SKU charset, field rename
Warning: There are unresolved conflicts at these paths:
src/config.rs    2-sided conflict
save src/inventory.rs in triad
```

The result is left's file plus right's changes outside the conflicts:

```sh
diff <(jj file show -r left src/inventory.rs) src/inventory.rs
```

```
45c45
<                 write!(f, "item {sku}: wanted {wanted}, only {held} held")
---
>                 write!(f, "cannot take {wanted} of {sku}: {held} in stock")
57c57
<     reorder_at: u32,
---
>     reorder_threshold: u32,
61c61
<     pub fn new(reorder_at: u32) -> Self {
---
>     pub fn new(reorder_threshold: u32) -> Self {
64c64
<             reorder_at,
---
>             reorder_threshold,
69c69
<         *self.levels.entry(sku).or_insert(0) += count;
---
>         *self.levels.entry(sku).or_default() += count;
```

Each save is its own jj operation:

```sh
jj undo
jj resolve --list
```

`jj undo` names `save src/inventory.rs in triad`, and both files are conflicted again.

### 7.5 The merge window

```sh
triad-merge resolve; echo $?
```

- [ ] The window opens on `src/config.rs`, the first conflicted file. The status bar starts `jj repo · jj merge`. The badge, at the first conflict, reads `Conflict 1 of 4 · 4 unresolved`.
- [ ] Line origins (⌘I) show the change ID, author and commit beside each line: `qpvuntsm` for left's lines, `mzvwutvl` for right's, `oxmtprsl` for base's.
- [ ] Take left for every conflict from the toolbar and press ⌘S. The window then opens `src/inventory.rs`.
- [ ] Resolve one conflict of `inventory.rs` and press ⌘S. The prompt `Save with unresolved conflicts?` offers Save with Conflicts and Keep Editing; choose Save with Conflicts. The shell prints `1`.

```sh
jj resolve --list
jj op log -n 2 --no-graph -T 'description ++ "\n"'
```

```
src/inventory.rs    2-sided conflict
save src/inventory.rs in triad
save src/config.rs in triad
```

- [ ] `grep -c '^<<<<<<<' src/inventory.rs` prints `3`: jj holds the rest as a conflict with fewer hunks.
- [ ] Build on it and come back. `jj new`, then `jj edit @-`, then `triad-merge resolve src/inventory.rs`: the window shows three conflicts.

### 7.6 Refusals

```sh
triad-merge resolve --headless --merge-semantics git src/inventory.rs; echo $?
```

```
triad-merge: …/conflicts is a jj repository, where triad merges with jj's semantics only; resolve with `jj resolve`
3
```

## 8. Resolve from `jj resolve`

`install --jj` made triad jj's merge editor (§0). Start from a fresh copy:

```sh
cd "$("$TRIAD_ROOT/script/gen-conflict-repo" "$TEST/jj-resolve")"
jj resolve src/inventory.rs
```

- [ ] The window title is `src/inventory.rs`. The four conflicts match §7.5.
- [ ] Take left for all from the toolbar and Save & Exit. jj prints:

```
Resolving conflicts in: src/inventory.rs
Working copy  (@) now at: vruxwmqv … (conflict) merge left and right
…
Warning: There are unresolved conflicts at these paths:
src/config.rs    2-sided conflict
```

```sh
jj resolve src/config.rs
```

- [ ] Resolve one conflict, Save & Exit, and choose Save with Conflicts. triad exits 1; jj reads the markers back and keeps the other three: `jj resolve --list` still lists `src/config.rs`, and `grep -c '^<<<<<<<' src/config.rs` prints `3`.
- [ ] `jj resolve src/config.rs` again, pick a side and close with ⌘W, choosing Discard. triad exits 2 and jj prints the lines below; `src/config.rs` keeps its three conflicts.

```
Resolving conflicts in: src/config.rs
Error: Failed to resolve conflicts
Caused by: Tool exited with exit status: 2 (run with --debug to see the exact invocation)
```

## 9. Diff the jj working copy

The `dirs` scenario commits `fixtures/demo/dirs/left` as `base` and puts `right` in the working copy.

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" dirs "$TEST/jj-dirs")"
jj log
jj diff --summary
```

```
@  qpvuntsm fixture@example.invalid 2026-01-01 12:00:00 c54c4cab
│  (no description set)
○  oxmtprsl fixture@example.invalid 2026-01-01 12:00:00 base 90cea740
│  base: the demo's left directory
◆  zzzzzzzz root() 00000000
A assets/icon.png
M assets/logo.png
A docs/design.md
D docs/notes.md
A invoice/cli.py
R invoice/{util.py => helpers.py}
R invoice/{ => io}/parser.py
C invoice/{totals.py => summary.py}
M invoice/totals.py
D legacy.txt
A names-bad-mark.txt
A names-latin1.txt
```

### 9.1 The changed files

```sh
triad-merge diff --inspect; echo $?
triad-merge diff --inspect --rev base; echo $?
```

Both print the list of §5, ending `files: 12`, and exit `1`. triad pairs the same renames and copy as `jj diff --summary`.

```sh
triad-merge diff
```

- [ ] The directory window of §5, against `@-`.
- [ ] Open `invoice/io/parser.py`: `invoice/parser.py` at `@-` sits on the left, read-only. Copy a line from left to right, ⌘S, close the diff window. The list loads again.
- [ ] `jj diff invoice/io/parser.py` shows the copied line: the edit went to the working copy.

### 9.2 One file

```sh
jj diff --git invoice/totals.py
```

```
@@ -1,4 +1,4 @@
-from invoice.parser import parse
+from invoice.io.parser import parse
 
 TAX_RATE = 0.2
 
@@ -7,10 +7,11 @@ def total(invoices):
     return sum(invoice["amount"] for invoice in invoices)
 
 
-def with_tax(amount):
-    return amount * (1 + TAX_RATE)
+def with_tax(amount, rate=TAX_RATE):
+    return amount * (1 + rate)
 
 
 def report(path):
     invoices = parse(path)
     print(f"{len(invoices)} invoices, total {with_tax(total(invoices)):.2f}")
+    print(f"largest {max(invoice['amount'] for invoice in invoices):.2f}")
```

```sh
triad-merge diff invoice/totals.py
triad-merge diff invoice/totals.py --rev base
triad-merge diff legacy.txt --rev base
```

- [ ] `totals.py` shows the three changes `jj diff` shows: line 1, lines 10–11, and the line added at 17. The status bar starts `jj repo · 3 differences`.
- [ ] The left side is read-only; the right is the working-copy file, and ⌘S writes there.
- [ ] `legacy.txt`, deleted in the working copy, shows its text on the left and an empty right whose title ends `(no file)`.

### 9.3 From `jj diff --tool`

```sh
jj diff --tool triad
jj diff --tool triad invoice/totals.py
jj diff --tool triad --from 'root()' --to base
```

- [ ] The first opens the same directory window, 12 files. Edits to a right-side file save to the working copy.
- [ ] The second lists only `invoice/totals.py`.
- [ ] The third lists every file of `base` as added, and a banner says edits are lost: neither side is the working copy.

## 10. A three-sided conflict

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" sides "$TEST/sides")"
jj log
jj resolve --list
```

```
@      kmkuslsw fixture@example.invalid 2026-01-01 12:00:00 90c7c593 (conflict)
├─┬─╮  (empty) merge one, two and three
│ │ ○  yostqsxw fixture@example.invalid 2026-01-01 12:00:00 three 417c035f
│ │ │  three: change s2
│ ○ │  royxmykx fixture@example.invalid 2026-01-01 12:00:00 two 3d16be97
│ ├─╯  two: change s2 and s7
○ │  kkmpptxz fixture@example.invalid 2026-01-01 12:00:00 one 2366a5f6
├─╯  one: change s2 and s7
○  oxmtprsl fixture@example.invalid 2026-01-01 12:00:00 base bec36941
│  base: one file
◆  zzzzzzzz root() 00000000
s.txt    3-sided conflict
```

`s.txt` is `s1` to `s9`. All three sides change `s2`. `one` and `two` change `s7`; `three` keeps it, so jj cancels that side there:

```sh
sed -n '15,25p' s.txt
```

```
<<<<<<< conflict 2 of 2
%%%%%%% diff from: oxmtprsl bec36941 "base: one file"
\\\\\\\        to: kkmpptxz 2366a5f6 "one: change s2 and s7"
-s7
+s7 one
+++++++ royxmykx 3d16be97 "two: change s2 and s7"
s7 two
%%%%%%% diff from: oxmtprsl bec36941 "base: one file"
\\\\\\\        to: yostqsxw 417c035f "three: change s2"
 s7
>>>>>>> conflict 2 of 2 ends
```

```sh
triad-merge resolve s.txt
```

- [ ] Three top panes, one per side: `s2 one`, `s2 two` and `s2 three` at the first conflict.
- [ ] The first conflict's badge reads `Conflict 1 of 2 · 3 of 3 sides · 2 unresolved`; the second's `Conflict 2 of 2 · 2 of 3 sides · 2 unresolved`.
- [ ] ⌘W with no picks closes at once: exit `2`, `s.txt` unchanged.

Without a window, more than two sides write jj's snapshot style:

```sh
triad-merge resolve s.txt --headless; echo $?
sed -n '1,14p' s.txt
jj st
```

```
1
s1
<<<<<<< conflict
+++++++ kkmpptxz 2366a5f6 "one: change s2 and s7"
s2 one
------- oxmtprsl bec36941 "base: one file"
s2
+++++++ royxmykx 3d16be97 "two: change s2 and s7"
s2 two
------- oxmtprsl bec36941 "base: one file"
s2
+++++++ yostqsxw 417c035f "three: change s2"
s2 three
>>>>>>> conflict ends
The working copy has no changes.
…
s.txt    3-sided conflict
```

jj reads the markers back as the same conflict: `@` keeps commit `90c7c593` and no operation is written.

## 11. File history from the operation log

jj snapshots the working copy at each command, and its operation log keeps every snapshot. The `history` scenario takes `notes.txt` through nine states.

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" history "$TEST/history")"
jj log
jj op log --no-graph -T 'description ++ "\n"'
```

```
@    kpqxywon fixture@example.invalid 2026-01-01 12:00:00 1ca32cca (conflict)
├─╮  (empty) merge four and other
│ ○  vruxwmqv fixture@example.invalid 2026-01-01 12:00:00 other f1bed228
│ │  notes other
○ │  qpvuntsm fixture@example.invalid 2026-01-01 12:00:00 four 6b524a82
├─╯  notes v4
○  oxmtprsl fixture@example.invalid 2026-01-01 12:00:00 one 157de4d7
│  notes v1
◆  zzzzzzzz root() 00000000
new empty commit
create bookmark other pointing to commit f1bed228b4355d1cf9d30961b5bc909f396b4694
commit 5a91ca175362ac659161a3221e07f4b452440cc6
snapshot working copy
new empty commit
create bookmark four pointing to commit 6b524a8245c6d1478fa234dc0e7f4f1b4b8d6b6e
commit ec7789444507674d62efa239b73369f246176aed
snapshot working copy
snapshot working copy
snapshot working copy
snapshot working copy
create bookmark one pointing to commit 157de4d7e6c5cba9a99d2a8ea8773ee5e9f8dbc4
commit b69732a69ecb9a763bf0bfead86101e109f090cd
snapshot working copy
add workspace 'default'
```

`jj log` holds only `v1`, `v4` and `other`. `v2` and `v3` never reached a commit, but the snapshots keep them.

```sh
triad-merge history notes.txt --list; echo $?
```

```
<op>         2026-01-01T12:00:00+00:00 new empty commit (conflict)
<op>         2026-01-01T12:00:00+00:00 snapshot working copy
<op>         2026-01-01T12:00:00+00:00 new empty commit
<op>         2026-01-01T12:00:00+00:00 snapshot working copy
<op>         2026-01-01T12:00:00+00:00 snapshot working copy (absent)
<op>         2026-01-01T12:00:00+00:00 snapshot working copy
<op>         2026-01-01T12:00:00+00:00 snapshot working copy
<op>         2026-01-01T12:00:00+00:00 snapshot working copy
b6c48cf70d31 2026-01-01T12:00:00+00:00 add workspace 'default' (absent)
0
```

`<op>` is a 12-character operation ID that differs per run. Read each version with jj itself:

```sh
for n in 1 2 3 4 5 6 7 8 9; do
  op=$(triad-merge history notes.txt --list | sed -n ${n}p | cut -d' ' -f1)
  printf '%s: ' $n; jj --at-op $op file show -r @ notes.txt 2>&1 | paste -sd'|' - | cut -c1-40
done
```

```
1: notes|<<<<<<< conflict 1 of 1|%%%%%%% di
2: notes|other
3: notes|v1
4: notes|v4
5: Error: No such path: notes.txt
6: notes|v3
7: notes|v2
8: notes|v1
9: Error: No such path: notes.txt
```

- [ ] `triad-merge history notes.txt --limit 3 --list` prints the first three rows only.

```sh
triad-merge history notes.txt
```

- [ ] A window lists the nine versions newest first, each under its operation. Rows 5 and 9 are marked absent, row 1 conflict.
- [ ] Select row 7 (`v2`) with ↓ and press Enter: the diff window opens, `notes` / `v2` on the left, read-only, and the working-copy file with its conflict markers on the right.
- [ ] Copy `v2` across, ⌘S, close the diff window. The list loads again with a new version at the top.
- [ ] Escape closes the window; the shell prints `0`.

One version without the window, with row 7's ID:

```sh
OP=$(triad-merge history notes.txt --list | sed -n 7p | cut -d' ' -f1)
triad-merge diff notes.txt --op "$OP"
```

- [ ] The diff window shows `v2` on the left against the working copy.

## 12. Conflicts across a stack

jj stores conflicts in commits, so a rebase never stops halfway. The `stack` scenario writes three commits on `base` and rebases them onto `left`, which changed the same lines.

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" stack "$TEST/stack")"
jj log
```

```
@  znkkpsqq fixture@example.invalid 2026-01-01 12:00:00 8f8bb208 (conflict)
│  (empty) (no description set)
×  vruxwmqv fixture@example.invalid 2026-01-01 12:00:00 stack-3 49e1d442 (conflict)
│  stack 3: change b5
×  royxmykx fixture@example.invalid 2026-01-01 12:00:00 stack-2 5b8855de (conflict)
│  stack 2: change c2
×  mzvwutvl fixture@example.invalid 2026-01-01 12:00:00 stack-1 2b5549b5 (conflict)
│  stack 1: change a3
○  qpvuntsm fixture@example.invalid 2026-01-01 12:00:00 left 3206d1de
│  left: change a3 and b5
○  oxmtprsl fixture@example.invalid 2026-01-01 12:00:00 base 3dfa7d65
│  base: three files
◆  zzzzzzzz root() 00000000
```

```sh
jj resolve --list -r stack-1
jj resolve --list -r stack-3
jj file show -r stack-1 a.txt
```

```
a.txt    2-sided conflict
a.txt    2-sided conflict
b.txt    2-sided conflict
a1
a2
<<<<<<< conflict 1 of 1
%%%%%%% diff from: oxmtprsl 3dfa7d65 "base: three files" (parents of rebased revision)
\\\\\\\        to: qpvuntsm 3206d1de "left: change a3 and b5" (rebase destination)
-a3
+a3 left
+++++++ mzvwutvl e7a082bf "stack 1: change a3" (rebased revision)
a3 stack
>>>>>>> conflict 1 of 1 ends
a4
a5
a6
a7
a8
```

`stack-2` and `stack-3` inherit `a.txt`'s conflict from `stack-1`. `stack-3` adds its own in `b.txt`. In plain jj, fixing `stack-1` takes `jj new stack-1`, `jj resolve`, `jj squash`. triad resolves it in place.

### 12.1 The list

```sh
triad-merge stack --list; echo $?
```

```
znkkpsqq 8f8bb208
  a.txt (inherited)
  b.txt (inherited)
vruxwmqv 49e1d442 stack 3: change b5
  a.txt (inherited)
  b.txt
royxmykx 5b8855de stack 2: change c2
  a.txt (inherited)
mzvwutvl 2b5549b5 stack 1: change a3
  a.txt
1
```

`(inherited)` marks a path whose parent holds the conflict too. Resolve the oldest first.

### 12.2 A dry run

Region 1 of `a.txt` is the `a3` conflict. Pick left there:

```sh
echo '{"Toggle":{"region":1,"source":{"Side":0}}}' > "$TEST/pick-left.jsonl"
triad-merge resolve a.txt -r stack-1 --headless --dry-run; echo $?
triad-merge resolve a.txt -r stack-1 --headless --dry-run --commands "$TEST/pick-left.jsonl"; echo $?
jj resolve --list -r stack-1
```

```
would save with conflicts a.txt in mzvwutvl
1
would resolve a.txt in mzvwutvl
  royxmykx: a.txt -> none
  vruxwmqv: a.txt, b.txt -> b.txt
  znkkpsqq: a.txt, b.txt -> b.txt
0
a.txt    2-sided conflict
```

Each descendant line gives its conflicted paths before and after. The dry run writes nothing: `stack-1` still conflicts.

### 12.3 The write

```sh
triad-merge resolve a.txt -r stack-1 --headless --commands "$TEST/pick-left.jsonl"; echo $?
```

```
resolved a.txt in mzvwutvl
  royxmykx: a.txt -> none
  vruxwmqv: a.txt, b.txt -> b.txt
  znkkpsqq: a.txt, b.txt -> b.txt
0
```

```sh
jj log
jj op log -n 1 --no-graph -T 'description ++ "\n"'
jj file show -r stack-1 a.txt | paste -sd' ' -
triad-merge stack --list; echo $?
```

```
@  znkkpsqq fixture@example.invalid … (conflict)
│  (empty) (no description set)
×  vruxwmqv fixture@example.invalid … stack-3 … (conflict)
│  stack 3: change b5
○  royxmykx fixture@example.invalid … stack-2 …
│  stack 2: change c2
○  mzvwutvl fixture@example.invalid … stack-1 …
│  (empty) stack 1: change a3
○  qpvuntsm fixture@example.invalid 2026-01-01 12:00:00 left 3206d1de
│  left: change a3 and b5
○  oxmtprsl fixture@example.invalid 2026-01-01 12:00:00 base 3dfa7d65
│  base: three files
◆  zzzzzzzz root() 00000000
resolve a.txt in mzvwutvl in triad
a1 a2 a3 left a4 a5 a6 a7 a8
znkkpsqq …
  b.txt (inherited)
vruxwmqv … stack 3: change b5
  b.txt
1
```

- [ ] `stack-1` is now `(empty)`: taking left undid its only change.
- [ ] `@` keeps its change ID `znkkpsqq`; `jj log -r 'heads(all())'` shows one head.
- [ ] `jj undo` names `resolve a.txt in mzvwutvl in triad`, and `jj log -r 'conflicts()'` lists all four commits again.

### 12.4 Refusals

```sh
triad-merge resolve a.txt -r base --headless; echo $?
triad-merge resolve a.txt -r 'stack-1|stack-2' --headless --dry-run; echo $?
```

```
triad-merge: a.txt is not conflicted
3
triad-merge: revision stack-1|stack-2 is ambiguous
3
```

### 12.5 The stack window

Start from a fresh copy:

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" stack "$TEST/stack-window")"
triad-merge stack
```

- [ ] The window lists the four commits of §12.1, newest first. `znkkpsqq` carries `@` on lime. Each path shows `(inherited)` as in the list.
- [ ] Hover `mzvwutvl`: a tooltip shows the full change and commit IDs, the date and the description.
- [ ] Select `a.txt` under `stack 1: change a3` and press Enter: the merge window opens on its conflict, left `a3 left`, right `a3 stack`.
- [ ] Pick left, then Preview Save. It shows `royxmykx: a.txt -> none`, `vruxwmqv: a.txt, b.txt -> b.txt` and `znkkpsqq: a.txt, b.txt -> b.txt`, and writes nothing.
- [ ] Save & Exit. A prompt lists the same three descendants, with Save and Keep Editing. Keep Editing writes nothing; Save writes.
- [ ] The list reloads with two commits, `znkkpsqq` and `vruxwmqv`, each holding `b.txt`.
- [ ] Open `b.txt` under `stack 3: change b5`, pick right (`b5 stack`), Save & Exit. Saving changes `@`'s conflicts, so the prompt lists `znkkpsqq: b.txt -> none`; choose Save. `triad-merge resolve b.txt -r stack-3 --headless --dry-run` with that pick prints the same:

```
would resolve b.txt in vruxwmqv
  znkkpsqq: b.txt -> none
```
- [ ] The window says no conflicted commit is left. Close it: the shell prints `0`.
- [ ] `jj log -r 'conflicts()'` prints nothing. `jj file show -r @ b.txt` shows `b5 stack` on line 5.

`stack` takes any revset. On a fresh copy, `triad-merge stack 'stack-2::' --list` lists `znkkpsqq`, `vruxwmqv` and `royxmykx`, leaving out `stack-1`.

## 13. Absorb

Fix lines across a stack in the working copy, then send each hunk to the commit that last wrote the lines it replaces.

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" absorb "$TEST/absorb")"
jj log
```

```
@  vruxwmqv fixture@example.invalid 2026-01-01 12:00:00 9a9a1023
│  (no description set)
○  royxmykx fixture@example.invalid 2026-01-01 12:00:00 C d6973528
│  C: change f13
○  zsuskuln fixture@example.invalid 2026-01-01 12:00:00 B baf45b58
│  B: change f9
○  qpvuntsm fixture@example.invalid 2026-01-01 12:00:00 A b87f6a05
│  A: change f3, f6 and f12
◆  oxmtprsl fixture@example.invalid 2026-01-01 12:00:00 base 03bc82e7
│  base: one file
◆  zzzzzzzz root() 00000000
```

The repository's config makes `base` immutable (`◆`). Who wrote each line, and what the working copy changes:

```sh
jj file annotate -r @- f.txt -T 'commit.change_id().short(8) ++ " " ++ content'
jj diff --git f.txt | tail -n +6
```

```
oxmtprsl f1
oxmtprsl f2
qpvuntsm f3 A
oxmtprsl f4
oxmtprsl f5
qpvuntsm f6 A
oxmtprsl f7
oxmtprsl f8
zsuskuln f9 B
oxmtprsl f10
oxmtprsl f11
qpvuntsm f12 A
royxmykx f13 C
oxmtprsl f14
-f1
+f1 fixed
 f2
-f3 A
+f3 A fixed
 f4
 f5
-f6 A
+f6 A fixed
 f7
 f8
-f9 B
+f9 B fixed
 f10
 f11
-f12 A
+f12 A fixed
 f13 C
 f14
```

The working copy also adds `new.txt`.

### 13.1 The list

```sh
triad-merge absorb --list; echo $?
```

```
at operation <op>
1 f.txt left 0..1 right 0..1 stays: no one commit in --into wrote its lines
2 f.txt left 2..3 right 2..3 -> qpvuntsm b87f6a05
3 f.txt left 5..6 right 5..6 -> qpvuntsm b87f6a05
4 f.txt left 8..9 right 8..9 -> zsuskuln baf45b58
5 f.txt left 11..12 right 11..12 stays: would conflict in qpvuntsm
0
```

| Hunk | Line | Goes | Why |
|---|---|---|---|
| 1 | f1 | stays | `base` wrote it, and `base` is outside `--into` (`mutable()`) |
| 2, 3 | f3, f6 | `A` | `A` wrote them |
| 4 | f9 | `B` | `B` wrote it |
| 5 | f12 | stays | `A` wrote it, but `C` changed f13 next to it: in `A` it would conflict |
| — | `new.txt` | stays, unlisted | New files stay |

`--keep` changes the plan without writing:

```sh
triad-merge absorb --list --keep 3
```

Hunk 3 now reads `3 f.txt left 5..6 right 5..6 stays: kept`.

### 13.2 Apply

```sh
triad-merge absorb --apply; echo $?
```

```
1 f.txt left 0..1 right 0..1 stays: no one commit in --into wrote its lines
2 f.txt left 2..3 right 2..3 -> qpvuntsm b87f6a05
3 f.txt left 5..6 right 5..6 -> qpvuntsm b87f6a05
4 f.txt left 8..9 right 8..9 -> zsuskuln baf45b58
5 f.txt left 11..12 right 11..12 stays: would conflict in qpvuntsm
absorbed 3 hunks into 2 commits
  qpvuntsm: none -> none
  zsuskuln: none -> none
  royxmykx: none -> none
0
```

```sh
jj op log -n 1 --no-graph -T 'description ++ "\n"'
jj file show -r A f.txt | paste -sd' ' -
jj diff --git f.txt | grep -E '^[-+][^-+]'
jj log -r 'conflicts()'
```

```
absorb in triad
f1 f2 f3 A fixed f4 f5 f6 A fixed f7 f8 f9 f10 f11 f12 A f13 f14
-f1
+f1 fixed
-f12 A
+f12 A fixed
```

`A` holds f3 and f6 fixed, `B` f9, and the working copy keeps f1, f12 and `new.txt`. No commit conflicts. `jj undo` reverts the whole absorb.

### 13.3 Force a hunk, and a stale listing

Start from a fresh copy:

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" absorb "$TEST/absorb-force")"
OP=$(triad-merge absorb --list | head -1 | cut -d' ' -f3)
triad-merge absorb --move 5=A --apply; echo $?
```

```
triad-merge: --move and --keep with --apply need --listed-at OP, the operation the listing printed
3
```

```sh
triad-merge absorb --move 5=A --apply --listed-at "$OP"; echo $?
jj log -r 'conflicts()' --no-graph -T 'change_id.short(8) ++ " " ++ bookmarks ++ "\n"'
```

```
1 f.txt left 0..1 right 0..1 stays: no one commit in --into wrote its lines
2 f.txt left 2..3 right 2..3 -> qpvuntsm b87f6a05
3 f.txt left 5..6 right 5..6 -> qpvuntsm b87f6a05
4 f.txt left 8..9 right 8..9 -> zsuskuln baf45b58
5 f.txt left 11..12 right 11..12 -> qpvuntsm b87f6a05
absorbed 4 hunks into 2 commits
  qpvuntsm: none -> f.txt
  zsuskuln: none -> f.txt
  royxmykx: none -> none
0
zsuskuln B
qpvuntsm A
```

`A` and `B` now conflict, as the listing warned. `C` clears it again: its own f13 change resolves the conflict there.

A listing goes stale once the repository moves on:

```sh
jj undo
echo x >> new.txt
triad-merge absorb --keep 1 --apply --listed-at "$OP"; echo $?
```

```
triad-merge: checking the listing's operation: the repository changed since operation <op>; list the hunks again
3
```

Plain `jj absorb` moves hunk 5 without asking, and leaves `A` and `B` conflicted:

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" absorb "$TEST/absorb-jj")"
jj absorb
```

```
Absorbed changes into 2 revisions:
  zsuskuln … B* | (conflict) B: change f9
  qpvuntsm … A* | (conflict) A: change f3, f6 and f12
Rebased 2 descendant commits.
…
New conflicts appeared in 2 commits:
…
```

### 13.4 The absorb window

```sh
cd "$("$TRIAD_ROOT/script/gen-jj-repo" absorb "$TEST/absorb-window")"
triad-merge absorb
```

- [ ] The left list holds hunks 1–5 with their lines and targets, as in §13.1. The right list holds `A`, `B`, `C` and Working copy, each with a count of hunks going there: `A` 2, `B` 1.
- [ ] Select hunk 5 with ↓, press Enter, pick `A` with ↑ or ↓ and press Enter. `A`'s count becomes 3.
- [ ] Select hunk 3, Enter, pick Working copy, Enter: `A` drops to 2.
- [ ] Apply. The window says 3 hunks went into 2 commits and names `A` and `B`, whose conflicts changed. The CLI form, on a fresh copy, gives the same report:

```sh
OP=$(triad-merge absorb --list | head -1 | cut -d' ' -f3)
triad-merge absorb --move 5=A --keep 3 --apply --listed-at "$OP"
```

```
absorbed 3 hunks into 2 commits
  qpvuntsm: none -> f.txt
  zsuskuln: none -> f.txt
  royxmykx: none -> none
```

- [ ] What is left is listed: the f1 and f6 hunks. `jj diff --git f.txt` in the working copy shows the same two.
- [ ] `jj op log -n 1` names `absorb in triad`; `jj undo` reverts it.
- [ ] Run `triad-merge absorb` again, then in another terminal `touch new.txt; jj st`. Apply refuses: Refresh and choose again.
- [ ] Escape in the hunk list closes the window; the shell prints `0`.

## 14. Clean up

```sh
cd "$TRIAD_ROOT"
rm -rf "$TEST"
unset JJ_CONFIG TRIAD_CONFIG
```

The fixtures under `fixtures/demo` are unchanged: every step that writes works on copies under `$TEST`. `docs/residue.md` lists the logs triad leaves behind.
