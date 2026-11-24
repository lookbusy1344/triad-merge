# Development

`AGENTS.md` holds the rules for changes, the commit gate and the full command list.

## Build and test

```sh
cargo build --workspace                                                # target/debug/triad-merge
cargo nextest run --workspace --no-run                                 # build the tests
cargo nextest run --workspace -E 'package(merge-core) & test(merge3)'  # filtered; never plain `cargo test`
script/pre-commit                                                      # the gate before each commit; under 35 s warm
script/checks                                                          # everything CI runs, with the full suite
script/macos-app dist                                                  # target/dist/triad.app, fat LTO, as a release builds it
script/windows-app.ps1 dist                                            # target\dist\triad-merge*.exe, the same on Windows, in PowerShell
```

`script/gen-conflict-repo` and `script/gen-jj-repo SCENARIO` build disposable jj repositories to try triad in; `script/test-jj-repos` checks triad in each (`docs/test-repos.md`). `docs/release.md` gives the steps of a release.

## Windows

The gate scripts run under Git Bash, which Git for Windows installs. They need `jq` (`winget install jqlang.jq`), `cargo-deny` (`cargo install --locked cargo-deny`) and Python 3 on `PATH`. The gate has no time limit on Windows: the quick test profile takes about 100 s there, against 9 s on macOS (`DESIGN.md` §A13.5).

NTFS has no executable bit, so a new script under `script/` lands without one. Set it with `jj file chmod x PATH`; on macOS and Linux `script/check-workspace` fails on a script with a `#!` line that is not executable.

## Gatekeeper on macOS

Gatekeeper scans each newly linked binary on its first run, so the first test run after a rebuild can stall before any test starts, with `syspolicyd` at full CPU. Add your terminal and IDE under System Settings → Privacy & Security → Developer Tools and restart them. `spctl developer-mode enable-terminal` shows that pane if it is missing.
