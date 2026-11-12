<picture>
  <source media="(prefers-color-scheme: dark)" srcset="images/triad-logo-dark.svg">
  <img src="images/triad-logo.svg" alt="triad" width="280">
</picture>

# triad-merge

[![Build & test](https://github.com/lookbusy1344/triad-merge/actions/workflows/triad.yml/badge.svg)](https://github.com/lookbusy1344/triad-merge/actions/workflows/triad.yml)
[![CodeQL](https://github.com/lookbusy1344/triad-merge/actions/workflows/github-code-scanning/codeql/badge.svg)](https://github.com/lookbusy1344/triad-merge/actions/workflows/github-code-scanning/codeql)
[![Audit](https://github.com/lookbusy1344/triad-merge/actions/workflows/audit.yml/badge.svg)](https://github.com/lookbusy1344/triad-merge/actions/workflows/audit.yml)
[![crates.io](https://img.shields.io/crates/v/triad-merge.svg)](https://crates.io/crates/triad-merge)

triad is a fast, native three-way merge and two-way diff tool, written in Rust and GPUI, built for [Jujutsu](https://github.com/jj-vcs/jj) with full Git support. It opens a million-line merge in under a second, merges exactly as your VCS does, and resolves conflicts straight from the repository.

The binary is `triad-merge`.

## Platforms

The initial beta targets macOS 27 on Apple Silicon. Linux x64 and Windows 11 will follow before release 1.0.

## Why triad

- **jj first.** triad merges with jj's own rules in a jj repository: jj's diff, its trivial resolution, `merge.same-change` and `merge.hunk-level`, and conflicts with any number of sides. It reads jj's working-copy commit through jj-lib.
- **Full Git parity.** In a git repository triad merges as `git merge-file` does, byte for byte, and reads conflicts from git's index through gix. Every feature works in both.
- **Exact semantics, checked.** Two oracle corpora pin the engine: about 1,950 triples from `git merge-file` and about 1,970 merges from jj-lib. The tests compare triad's output with each.
- **Fast on large files.** Panes draw only the rows on screen. Scrolling holds the display's refresh rate at 1M lines.
- **No unsafe code** 100% safe Rust.
- **No git or jj binaries needed.** triad reads and writes repositories in-process through gix and jj-lib, never their executables.
- **Scriptable.** A headless mode and `inspect --json` expose the whole merge model to scripts, tests and agents.

## Features

**Merge window.** Left and right sit side by side, each showing its changes from base; the editable result sits below. Changes on one side, or the same change on both, apply automatically. You resolve each conflict by picking sources in order (left, base, right, or several) or by typing in the result. One undo history covers picks and edits.

**Diff window.** Two editable, aligned panes with copy to the other side. `triad-merge diff PATH` compares a working-copy file with its parent revision and saves edits to the working copy.

**Resolve from the repository.** `triad-merge resolve` lists the conflicted files and opens each in turn, from jj's working-copy commit or git's index stages. Each save is recorded: git stages a resolved file as `git add` does; jj snapshots the save in an operation of its own, which `jj undo` reverts.

**Line origins.** A blame column shows the commit, author and jj change that introduced each line. Each pane's heading names its input's commit.

**Merge refinements**, all opt-in except the first:

- Conflict refinement trims lines both sides share off the ends of a conflict, as git's `zdiff3` does.
- Word-level merging retries each conflict word by word, as jj's `merge.hunk-level = "word"` does.
- Deep refinement splits shared blocks out of the middle of a conflict.
- Syntax boundaries keep both refinements from cutting inside a syntax node.
- Suggestions offer, never apply, resolutions for common shapes: both sides inserting, sorted lists, a block moved on one side and edited on the other.

**Editor.** Syntax highlighting for Rust, Go, C#, Python, Swift, HTML, JavaScript, TypeScript and TSX. Overview ruler, search and replace, soft wrap, scroll sync, whitespace modes, line-ending control, recovery journals for unfinished merges, and a plain-text config file.

## Performance

Release build on an M4 Pro, 120 Hz display, plain text. Spec §9 holds the targets; `script/measure-app` and the benchmarks check them.

| Operation | 100k lines | 1M lines | Target, 100k / 1M |
|---|---|---|---|
| Load and merge | 46–50 ms | 521–548 ms | 100 ms / 1 s |
| Command to first frame | 210–223 ms | 727–754 ms | 300 ms / 1.5 s |
| Scrolling, frame interval p95 | 8.4 ms | 8.4 ms | refresh rate |
| Pick a side, max | 0.6 ms | 6.3 ms | 16 ms |
| Keystroke, max | 0.5 ms | 5.2 ms | 8 ms |
| Peak resident memory | 221 MB | 869 MB | 300 MB / 2 GB |

An idle window draws nothing, and the GPU driver hands back its memory.

## Build and install

You need an Apple Silicon Mac on macOS 27 or later, the Xcode Command Line Tools (`xcode-select --install`) and `rustup`, which fetches the toolchain on the first build. Run every command from the repository root.

The [crates.io package](https://crates.io/crates/triad-merge) is a placeholder that holds the name. It prints a message and exits; `cargo install triad-merge` does not install triad. Build from source.

```sh
script/macos-app                                                   # builds target/release/triad.app
rm -rf /Applications/triad.app
cp -R target/release/triad.app /Applications/
/Applications/triad.app/Contents/MacOS/triad-merge install --jj    # jj resolve, jj diff --tool triad
/Applications/triad.app/Contents/MacOS/triad-merge install --git   # git mergetool, git difftool
```

`install` registers the binary it runs from: `ui.merge-editor` and `[merge-tools.triad]` in jj's user config, `merge.tool` and `diff.tool` in git's global config. It leaves other settings and comments alone, and a second run changes nothing. `--config FILE` writes to another file. Rerun it after moving the binary.

To run `triad-merge` by name, link it onto your `PATH`:

```sh
ln -sf /Applications/triad.app/Contents/MacOS/triad-merge ~/.local/bin/triad-merge
```

The link follows each new copy of the app, so make it once. Make it again after deleting or renaming the app. `cargo install` puts its own `triad-merge` in `~/.cargo/bin`, which runs in place of the link when it comes first on your `PATH`. `which -a triad-merge` lists every copy; delete the stale ones.

After install:

| Path | What it is | On update |
|---|---|---|
| `target/release/triad.app` | The build | `script/macos-app` rewrites it |
| `/Applications/triad.app` | The installed app; the binary is `Contents/MacOS/triad-merge` | Replaced by the copy |
| `~/.local/bin/triad-merge` | Link to the installed binary, for the shell | Stays; points at the new binary |
| jj user config, git global config | Name the installed binary's full path | Stays |

The bundle gives the Dock triad's icon; a bare binary shows the generic one. To update, pull and repeat the first three commands. The registration names the path in `/Applications`, so it stays valid.

## Use

### From jj

```sh
jj resolve                    # opens each conflicted file in triad, in turn
jj resolve src/lib.rs         # one file
jj diff --tool triad          # each changed file in the diff window
jj diff --tool triad -r @-    # any revision jj diff takes
```

A save that leaves conflicts exits 1; jj reads the saved markers back and keeps the conflict. `jj diff --tool` passes temporary copies. When the right side shows the working copy, triad opens the working-copy file instead, so saved edits persist; the left copy is read-only. Edits to other revisions are lost, and the window says so in a banner. This needs `diff-do-chdir = false`, which `triad-merge install --jj` writes; rerun it over an existing install.

### From git

```sh
git mergetool                 # each conflicted file in turn; a resolved save is staged
git mergetool src/lib.rs
git difftool                  # working tree against the index; edits to the right pane save to the file
git difftool HEAD~1 -- src/lib.rs
```

In a jj repository, use `jj resolve`: `git mergetool` there exits 3, since triad merges with jj's semantics only (§ Merge semantics).

### From the repository, without the VCS command

`triad-merge` finds the repository from the working directory, jj or git, and reads it directly. Neither `jj` nor `git` needs to be installed.

```sh
triad-merge resolve                      # the first conflicted file, then the next after each save
triad-merge resolve src/lib.rs           # one file
triad-merge resolve --headless src/lib.rs   # write the automatic merge to the file, markers for conflicts
triad-merge diff src/lib.rs              # a file against its parent revision; edits save to the working copy
triad-merge diff --rev main src/lib.rs   # against another revision
```

A save from `resolve` is recorded. git stages a resolved file as `git add` does. jj snapshots the save in an operation of its own, `save <path> in triad`, which `jj undo` reverts.

### Resolving a jj conflict in steps

jj can [record a conflict in a commit](https://docs.jj-vcs.dev/latest/conflicts/), for resolution later, so a rebase or merge never stops halfway. You can edit and build on that commit as on any other, and resolve the conflict when it suits you. triad works the same way. Resolve some of a file's conflicts, save, and the rest stay in the commit as a conflict; the save exits 1. Then carry on with jj as usual, and come back later.

```sh
jj edit xyz                   # a conflicted commit, here or further down the history
triad-merge resolve a.rs      # resolve one of three conflicts and save
jj new                        # work on top; the two left show as markers
jj edit xyz                   # back to the conflicted commit
triad-merge resolve a.rs      # resolve the rest
```

Each save in a commit with descendants rebases them, as any jj command does. A descendant's own edits stay when they are clear of the conflicts. A descendant of a partly resolved commit can show more than two sides for a conflict: jj [rebases a conflicted file](https://docs.jj-vcs.dev/latest/technical/conflicts/) by combining whole versions of it, not line by line, and triad shows each version in a pane of its own. Resolving in the ancestor clears it in every descendant. git keeps a partial save as markers in the file, with the index stages in place, and the next `resolve` opens the conflicts left.

### On files

```sh
triad-merge                                        # the open dialog
triad-merge BASE LEFT RIGHT OUTPUT                 # the merge window
triad-merge BASE LEFT RIGHT -o OUTPUT              # the same
triad-merge --headless BASE LEFT RIGHT -o OUTPUT   # the automatic merge, with markers for conflicts
triad-merge inspect --json BASE LEFT RIGHT         # the merge's regions, as JSON
triad-merge diff LEFT RIGHT                        # the diff window
triad-merge diff --inspect --json LEFT RIGHT       # the diff's regions, as JSON
```

`fixtures/demo` holds a 980-line merge with 20 conflicts to try: `triad-merge fixtures/demo/{base,left,right}.txt /tmp/out.txt`.

Exit codes: 0 all conflicts resolved, 1 conflicts left, 2 cancelled, 3 error. `diff --inspect` exits 0 for equal files and 1 for different ones, as `diff(1)` does. `docs/json-schema.md` defines the JSON and the `--commands` file that drives a headless merge.

### Merge semantics

triad follows the repository: jj's semantics in a jj repository and in the directories `jj resolve` passes, git's elsewhere. `--merge-semantics jj|git` overrides it. triad refuses git's semantics in a jj repository: `git mergetool` there exits 3. Merge with jj and resolve with `jj resolve`.

The two can differ. Outside any repository, this triple leaves one conflict as jj merges and none as git does:

```sh
cd "$(mktemp -d)"
printf 'a\nb\nc\na\nb\nc\na\nb\nc\n' > b; printf 'a\nb\n a\na\nb\n c\n' > l; printf 'a \nb\nc\na\nb\n c\n' > r
triad-merge inspect --merge-semantics jj b l r    # exit 1
triad-merge inspect --merge-semantics git b l r   # exit 0
```

### The merge window

A conflict, marked `!`, waits for a decision. Until you decide, the result pane previews every side, left then right. The preview resolves nothing: saving writes conflict markers in its place, and the merge exits 1.

Each region's first row has a pick target per source at the left of each top pane: `●` in the pick, `◐` previewed in an unresolved conflict, `○` not in the pick. A click toggles the source; ⌘1, ⌘2 and ⌘3 toggle left, base and right in the current region. Several sources write in click order. The toolbar badge counts the conflicts left. The overview ruler at the right of the result marks every change; click a mark to go there. Save Copy As… (⇧⌘S) writes the result elsewhere and leaves the merge as it was.

### Settings

`~/.config/triad/config.triad` (`$XDG_CONFIG_HOME` if set, or `TRIAD_CONFIG`), one `key = value` per line. Settings… (⌘,) opens it. Every key is also a flag of the same name, which overrides the file for one run.

```sh
triad-merge show-config --default --docs   # every key, its default and what it does
triad-merge show-config                    # the settings in effect
```

### Known limits

- Under `git mergetool`, a save that leaves conflicts exits 1 and git restores its own copy. Run `git mergetool` again; triad offers to restore the session.
- VoiceOver support is unchecked by ear.

## Report a problem

Rerun with `TRIAD_LOG=debug` and send the command and what you expected, `triad-merge --version`, `sw_vers -productVersion`, the day's log from `~/Library/Caches/triad/logs/`, and the input files if you can share them. The log names files and paths; it holds none of their text.

## Build and test

```sh
cargo build --workspace                                         # target/debug/triad-merge
cargo nextest run --workspace --no-run                          # build the tests
cargo nextest run --workspace -E 'package(merge-core) & test(merge3)'   # filtered; never plain `cargo test`
script/pre-commit                                               # the gate before each commit; under 35 s warm
script/checks                                                   # everything CI runs, with the full suite
```

`script/gen-conflict-repo` builds a disposable jj repository with conflicts in two Rust files, for trying `jj resolve` and `triad-merge resolve`.

On macOS, Gatekeeper scans each newly linked binary on its first run, so the first test run after a rebuild can stall before any test starts, with `syspolicyd` at full CPU. Add your terminal and IDE under System Settings → Privacy & Security → Developer Tools and restart them. `spctl developer-mode enable-terminal` shows that pane if it is missing.

## Documents

| File | Contents |
|---|---|
| `docs/SPEC.md` | Behaviour |
| `docs/PLAN.md` | Status, next steps and open work |
| `docs/DESIGN.md` | Design notes: reasons, decisions, measurements |
| `AGENTS.md` | Rules for changes, the commit gate and commands |
| `docs/STYLE.md` | UI style |
| `docs/json-schema.md` | JSON output and merge commands |
| `docs/release-checklist.md` | Manual checks before a release |
| `docs/gpui-kit-issues.md` | UI toolkit problems and triad's answers |
| `docs/scroll-debugging.md` | Trackpad scrolling: failure modes and how to reproduce them |
| `docs/residue.md` | Files triad and its scripts leave behind |
| `fixtures/demo/README.md`, `fixtures/conflict-repo/README.md` | Inputs for trying triad by hand |

## Licence

MIT.
