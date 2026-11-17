# Changelog

What changed for users in each release, newest first. [docs/release.md](docs/release.md) says how to write a section.

## 0.1.0 — 2026-11-12

The first release, an early beta tested on macOS on Apple Silicon. Linux x64 and Windows x64 builds pass CI.

- Three-way merge window and two-way diff window, with jj's merge semantics in a jj repository and `git merge-file`'s in a git repository.
- `triad-merge resolve` from jj's working-copy commit or git's index; `jj resolve`, `jj diff --tool`, `git mergetool` and `git difftool -d` through `triad-merge install`.
- jj-only commands: `stack` and `resolve -r` for conflicts in any commit, `history` over the operation log, and `absorb`.
- Directory diff, line origins, merge refinements and suggestions, syntax highlighting, a headless mode and `--json` output.
