# Changelog

What changed for users in each release, newest first. [docs/release.md](docs/release.md) says how to write a section.

## 0.2.0 — 2026-11-19

- jj-only commands, each with its own window: `stack` and `resolve -r` for conflicts in any commit, with a preview of a save's effect on the descendants and a confirmation before a save changes them; `history` over the operation log, with `diff --op`; `absorb`, by hunk. triad also merges two sides of a conflict of more than two and the copies of a divergent change.
- Directory diff: the files that differ between two directories, or between a revision and the working copy (`diff`, `diff --rev`), with renames and copies paired as jj pairs them; `jj diff --tool` and `git difftool -d` open it.
- Moved blocks: in the diff window, with a pill naming the other end and Go to Other End of Move; in the merge window and `inspect`, each side's moved blocks and the blocks a merge takes twice, with a suggestion to drop one copy.
- Line picks inside a region, from the top panes' line numbers or Pick Selected Lines, in the order clicked.
- A unified view in the diff window; Copy as Patch and Copy Difference as Patch; `diff --inspect --unified[=N]`.
- Collapsed unchanged regions in the merge and diff windows.
- Comparison options: ignore case, ignore patterns, and ignore changes made only of blank lines in the two-way diff.
- Visible whitespace in every pane, with a toolbar toggle; invisible characters drawn as code points and odd spaces shaded.
- Text that is not UTF-8 read and saved as Windows-1252.
- A reload from disk keeps the caret and the view on their lines; the merge window warns once when another program changes OUTPUT.
- The windows in 14 languages: English, Spanish, simplified and traditional Chinese, Japanese, Korean, German, French, Brazilian Portuguese, Russian, Ukrainian, Polish, Dutch and Swedish.
- Syntax highlighting for JSON, YAML, TOML, CSS, C, C++, Java, Kotlin, Markdown, Bash and Zig; a script's language from its `#!` line.
- A restyled look after Zed: the UI font, Zed's status bar, buttons, menus and surfaces, and on macOS the toolbar in the title bar. The toolbar shows picks and view toggles pressed and moves Base and Both into a More menu.
- Faster on large files: conflicts refine in parallel, and the merge output and origin labels build off the UI thread.

Fixes:

- A crashed merge in a jj commit is restored from its journal.
- Revsets take the user's aliases, and `mine()` matches the user's email.
- jj reads back the base of a refined conflict, and a last conflict without a final newline follows jj's rule.
- A save into a jj commit holds against triad's last save, writes into the copy of a divergent change that triad read, and refuses a write when a file's executable bit changed since the read.
- A replaced file's mode is set before its bytes are written.
- Terminal output escapes control, bidirectional and invisible characters.
- The two-way diff window marks the characters that differ.

## 0.1.0 — 2026-11-12

The first release, a beta.

- Three-way merge window and two-way diff window, with jj's merge semantics in a jj repository and `git merge-file`'s in a git repository.
- `triad-merge resolve` from jj's working-copy commit or git's index; `jj resolve`, `jj diff --tool`, `git mergetool` and `git difftool` through `triad-merge install`.
- Line origins, merge refinements and suggestions, syntax highlighting, a headless mode and `--json` output.
