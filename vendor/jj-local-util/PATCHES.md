# Vendored jj-local-util

This directory holds `jj-local-util` 0.47.1 from crates.io, licensed under Apache-2.0 (`LICENSE`). Upstream: <https://github.com/jj-vcs/jj>, path `local-util`, commit `d16ee1fc88a72c54142241b079a34bbf3b033bc0`. Triad's root `Cargo.toml` points at it with `[patch.crates-io]` and excludes it from the workspace, so it keeps its own lints and formatting.

## Changes

- The `skip-sync` feature leaves out the flush to disk in `persist_temp_file` and `persist_content_addressed_temp_file`, through the new `sync_content`. jj-lib persists every object, operation, view and index file through them. On macOS each flush is an `F_FULLFSYNC`, which serialises on the disk: with it, the tests that build jj repositories take 116 s; without it, the whole suite takes 17 s (DESIGN.md §A13.5). Off by default, the crate behaves as upstream. Files: `Cargo.toml`, `src/file_util.rs`, which carries the modification notice Apache-2.0 §4(b) requires.

Only `vcs-jj`'s `testing` feature turns `skip-sync` on, and only dev-dependencies turn `testing` on. `script/check-workspace` fails when a non-dev dependency turns `skip-sync` on, and when the build does not use this copy.

## Upgrading jj

Each jj upgrade (`docs/jj-upgrade.md`) replaces this copy with the new release's `jj-local-util` from `~/.cargo/registry/src/*/jj-local-util-<version>`: `src/`, `Cargo.toml`, `Cargo.toml.orig`, `LICENSE` and `README.md`. Then apply the change above again, and update the version, the upstream commit (`.cargo_vcs_info.json` in the registry copy) and the root `Cargo.toml` pin. Until then, the build fails: `vcs-jj` asks for a `skip-sync` feature the registry crate lacks.
