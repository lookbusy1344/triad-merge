# Demo merge

A three-way merge for trying triad by hand. `base.txt` is a snapshot of `crates/merge-core/src/merge3.rs` (980 lines). Left changes 30% of its lines and right 28%, in hunks that rename identifiers, reword comments, change numbers, insert lines and delete lines. The merge has 51 left-only, 53 right-only and 16 same-on-both-sides changes, and 20 conflicts.

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
