# Conflict repository

The sources of a disposable jj repository whose working copy holds unresolved conflicts. The script runs `jj git init` without a layout flag, so the repository takes jj's default layout: colocated. Use it to try `triad-merge resolve` and the jj conflict paths by hand.

```sh
script/gen-conflict-repo                   # prints the new repository's path, under $TMPDIR
script/gen-conflict-repo /some/empty/dir
```

`base/` is a small Rust crate, `warehouse`, with `src/inventory.rs` (141 lines) and `src/config.rs` (120 lines). `left/` and `right/` hold the files each side changes. Each side compiles and passes its own tests.

| File | Left | Right | Conflict regions |
|---|---|---|---|
| `src/inventory.rs` | 11% of lines: SKU length constant and trimming, `ZeroCount` error, `skus()`, sorted reorder list, a test | 11%: SKU character check, reworded error, `reorder_at` renamed, `remove()`, a test | 4 |
| `src/config.rs` | 22%: `[section]` headers, `;` comments, `BadSection` error, doc comments, `section()`, a test | 22%: duplicate keys report the first line, quoted values, `get_or` deleted, `get_bool()`, tests | 4 |

Each file also has changes on one side only, which merge cleanly. The merge commit sits on bookmarks `left` and `right`, both children of `base`. Identity, timestamps and change IDs are fixed, so every run gives the same commit IDs.

The files end in `.txt` so that cargo, rustfmt, clippy and `jj fix` leave them alone. The script drops the suffix as it copies them in. The sources are rustfmt-clean. Check a change to them by copying a side out and running `cargo test` and `cargo clippy` there.
