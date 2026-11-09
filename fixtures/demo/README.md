# Demo merge

A three-way merge for trying triad by hand. `base.txt` is a snapshot of `crates/merge-core/src/merge3.rs` (980 lines). Left changes 30% of its lines and right 28%, in hunks that rename identifiers, reword comments, change numbers, insert lines and delete lines. The merge has 51 left-only, 53 right-only and 16 same-on-both-sides changes, and 20 conflicts.

`script/gen-demo [source]` writes the three files again from a source file. It is seeded, so the same source gives the same files. The files end in `.txt` so that formatters leave them alone: the sides do not parse as Rust.

```sh
cargo build --workspace
target/debug/triad-merge inspect fixtures/demo/{base,left,right}.txt
target/debug/triad-merge --headless fixtures/demo/{base,left,right}.txt -o target/demo-merged.txt   # exit 1: 20 conflicts
target/debug/triad-merge fixtures/demo/{base,left,right}.txt target/demo-merged.txt               # the merge window
```
