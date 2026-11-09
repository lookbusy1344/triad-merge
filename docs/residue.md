# Residue

Files that triad, its tools and its development scripts leave behind, and how to clear them. Paths use `$TMPDIR` for the system temp directory and `~` for the home directory.

## The app

| What | Where | Lifetime |
|---|---|---|
| Recovery journals, one per unfinished merge (spec §6.2) | macOS `~/Library/Caches/triad/journal/`; Linux `$XDG_CACHE_HOME/triad/journal/` (default `~/.cache/triad/journal/`); Windows `%LOCALAPPDATA%\triad\cache\journal\` | A resolved Save & Exit and Discard delete the journal. A save with conflicts keeps it. Each window deletes journals older than 30 days as it opens. |
| Daily logs, `triad.YYYY-MM-DD.log` | `logs/` beside `journal/`, or `TRIAD_LOG_DIR` | The last 7 files are kept. Most are empty: the default filter is `warn`. |
| Settings file `config.triad` (spec §6.4) | `TRIAD_CONFIG`; else `$XDG_CONFIG_HOME/triad/` or `~/.config/triad/` on macOS and Linux; `%APPDATA%\triad\` on Windows | User state, never deleted by triad. |
| Tool registration from `triad-merge install` | `merge.tool`, `[mergetool "triad"]`, `diff.tool`, `[difftool "triad"]` in git's global config; `ui.merge-editor` and `[merge-tools.triad]` in jj's user config | Stays until removed by hand. |
| The installed app | `/Applications/triad.app` (README § Build and install) | Stays until removed by hand. |

A journal for a merge that no longer matters can go at once. Deleting the whole cache directory also clears the logs; triad makes it again on the next run.

## git and jj

| What | Where | Lifetime |
|---|---|---|
| jj's resolve directories, `jj-resolve-<random>` | `$TMPDIR` | jj deletes each one after the tool exits. An interrupted `jj resolve` can leave one behind, usually empty. |
| `git mergetool` backups, `FILE.orig`, and its `FILE_BASE_*`, `FILE_LOCAL_*`, `FILE_REMOTE_*` copies | Beside the conflicted file | git deletes the copies after the tool exits. The backups stay when `mergetool.keepBackup` is true, git's default. |

## Development

| What | Where | Lifetime |
|---|---|---|
| Conflict repositories from `script/gen-conflict-repo` | `$TMPDIR/triad-conflicts.<random>/` | Kept on purpose for manual testing. Delete each by its path when done. |
| Test fixtures (gix and jj-lib repositories, files) | `$TMPDIR`, through `tempfile` | Deleted when the test ends. A test killed by a timeout or a signal can leave one behind. |
| Throwaway clones from `script/pre-push` on plain git | `$TMPDIR/triad-pre-push.<random>/` | Deleted when the check ends, fails, or stops on Ctrl-C, SIGHUP or SIGTERM (`script/test-pre-push`). Only SIGKILL leaves one behind. |
| Build output | `target/` | Grows with every build; `cargo clean` clears it. `target/release/triad.app` comes from `script/macos-app`. |
| Measurement inputs and log | `target/measure/` | Written by `script/measure-app`; kept for reruns. |
| Benchmark results | `target/criterion/`, `target/tmp/memory.json` | Kept for comparison by `script/check-bench`. |
| Vendored crate's test build | `target/vendor/` | Written by `script/checks`. |
| Linux packages | `target/linux/` | Written by `script/linux/run`. Packaging and smoke tests run in a container started with `--rm`, so their temp files go with it. |
| Agent scratch space (Claude Code) | A per-session directory under the system temp directory | Holds logs and copies made while working. Safe to delete once the session ends. |

## Clearing it

Delete each item by its full path, one at a time, after listing what is there:

```sh
ls -d "$TMPDIR"/triad-conflicts.* "$TMPDIR"/triad-pre-push.* "$TMPDIR"/jj-resolve-* 2>/dev/null
ls -la ~/Library/Caches/triad/journal ~/Library/Caches/triad/logs   # macOS
```
