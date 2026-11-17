# Demo merge

A three-way merge for trying triad by hand. `base.txt` is a snapshot of `crates/merge-core/src/merge3.rs` (980 lines). Left changes 30% of its lines and right 28%, in hunks that rename identifiers, reword comments, change numbers, insert lines and delete lines. `inspect` finds 50 left-only, 52 right-only and 15 same-on-both-sides regions, and 20 conflicts.

`script/gen-demo [source]` writes the three files again from a source file. It is seeded, so the same source gives the same files. The files end in `.txt` so that formatters leave them alone: the sides do not parse as Rust.

```sh
cargo build --workspace
target/debug/triad-merge inspect fixtures/demo/{base,left,right}.txt
target/debug/triad-merge --headless fixtures/demo/{base,left,right}.txt -o target/demo-merged.txt   # exit 1: 20 conflicts
target/debug/triad-merge fixtures/demo/{base,left,right}.txt target/demo-merged.txt               # the merge window
```

## Unusual characters

`unusual_base.py`, `unusual_left.py` and `unusual_right.py` are a 15-line Python module with CRLF line endings, for checking how triad shows unusual characters (spec §5). Left hides a Trojan Source comparison, with a right-to-left override and isolates inside a string, a zero-width space in `is_admin` and a run of tag characters after the first comment. Right indents `greet` with no-break spaces and puts an ideographic space, a byte order mark, a zero-width joiner, a lone carriage return and an escape sequence inside lines. Its last line holds 40 zero-width spaces, which wrap in a narrow result pane. Both sides change the comparison on line 4, so the merge has one conflict. Each invisible character draws as its code point on a grey box and each odd space on grey; the carriage returns that end the lines draw nothing.

`script/gen-unusual-demo` writes the three files again. It names each character by its code point, so its source holds none of them.

```sh
cargo build --workspace
target/debug/triad-merge fixtures/demo/unusual_{base,left,right}.py target/unusual-merged.py   # the merge window
target/debug/triad-merge diff fixtures/demo/unusual_{left,right}.py                             # the diff window
target/debug/triad-merge inspect fixtures/demo/unusual_{base,left,right}.py                     # exit 1: 1 conflict
```

## Moved code

`moved_base.py`, `moved_left.py` and `moved_right.py` are a 70-line Python module of small functions, for checking moved and duplicated blocks (spec §4.3, §4.6, §5). Each function is a block of four lines; a function neither side touches stands between any two changes, so each change is a region of its own.

| Function | Left | Right | Shows |
|---|---|---|---|
| `parse_header` | moves it down, after `close_file` | leaves it | A left move: teal at both ends in the left pane, a pill on each |
| `format_total` | leaves it | moves it to the top | A right move |
| `round_cents` | moves it after `log_line` | the same | A move on each side to one place: the merge takes it once |
| `apply_discount` | moves it after `open_file` | moves it after `count_rows` | A move on each side to two places: the merge takes it twice |
| `clamp` | adds it after the moved `tax_rate` | adds it after `parse_args` | New text at two places: taken twice; right's copy lies inside left's larger insert |
| `tax_rate` | moves it after `write_report` | changes its default to 0.25 where it was | A conflict, with Carry Right's Edit in the result's right-click menu |

The merge has one conflict and two duplicated blocks, which the status bar counts and the ruler outlines. Press ⇧⌘M in a region that holds a move's end, or click a pill, to go to the other end. Diffing base against left shows left's four moves in the diff window.

`script/gen-moved-demo` writes the three files again; `script/gen-moved-demo --layout` prints each function's lines in each file, as `inspect` counts them.

```sh
cargo build --workspace
target/debug/triad-merge inspect fixtures/demo/moved_{base,left,right}.py                     # exit 1: 1 conflict, 7 moves, 2 duplicates
target/debug/triad-merge fixtures/demo/moved_{base,left,right}.py target/moved-merged.py      # the merge window
target/debug/triad-merge diff fixtures/demo/moved_{base,left}.py                              # the diff window
```

## Directories

`dirs/left` and `dirs/right` are a small Python project before and after a change, for trying the directory diff (spec §6.1). `script/gen-dirs-demo` writes them again; it names the binary and Latin-1 bytes by escape.

| Path in `right` | Shown as | Why |
|---|---|---|
| `invoice/io/parser.py` | renamed from `invoice/parser.py` | Moved, with two lines changed: it keeps most of its bytes |
| `invoice/helpers.py` | renamed from `invoice/util.py` | The same bytes under a new name |
| `invoice/totals.py` | modified | |
| `invoice/summary.py` | copied from `invoice/totals.py` | Keeps most of the edited `totals.py`'s bytes; opens against `totals.py` in `left` |
| `docs/design.md`, and `docs/notes.md` | added, and deleted | It keeps two of the notes' eight lines, under half their bytes |
| `legacy.txt` | deleted | Only in `left` |
| `invoice/cli.py` | added | Only in `right` |
| `assets/logo.png`, `assets/icon.png` | modified, added; binary | NUL bytes; listed, greyed, and do not open |
| `names-latin1.txt` | added | Latin-1 bytes: not UTF-8, so they decode as Windows-1252; opens, and the status bar says Windows-1252 |
| `names-bad-mark.txt` | added, undecodable | Latin-1 bytes after a UTF-8 mark: the mark says UTF-8, and the bytes are not; greyed, and does not open |
| `README.md` | left out | The same on both sides |

```sh
cargo build --workspace
target/debug/triad-merge diff --inspect fixtures/demo/dirs/{left,right}   # exit 1: 12 files
target/debug/triad-merge diff fixtures/demo/dirs/{left,right}             # the directory window
```

Edits in the diff window save to the files under `fixtures/demo/dirs`; `script/gen-dirs-demo` puts them back. The same change as a jj repository, for `triad-merge diff` and `jj diff --tool triad`, is `script/gen-jj-repo dirs` (`docs/test-repos.md`).

