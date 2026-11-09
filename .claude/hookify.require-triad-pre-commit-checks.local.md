---
name: require-triad-pre-commit-checks
enabled: true
event: bash
pattern: git\s+commit|jj\s+(commit|describe|squash)
action: warn
---

**Pre-commit check required for triad changes.**

Run `script/pre-commit` from the repo root before committing. It works in jj and plain git repos. It skips the checks when the change touches no build file (`.rs`, `Cargo.toml`, `Cargo.lock`, `deny.toml`, toolchain and format config, `script/`, `fixtures/`). Otherwise it runs `script/checks`: fmt check, workspace rules, build, clippy with `-D warnings`, nextest, doc tests and `cargo deny`.

It must pass with zero warnings or errors. If it already passed in this session and nothing changed since, proceed.
