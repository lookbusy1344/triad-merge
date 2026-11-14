# Upgrading jj

The steps to take triad to a new jj release. triad links jj-lib and jj-core, pinned exactly, and copies parts of jj-cli's behaviour. A new release can change both. In a jj repository triad merges with jj's semantics only (spec §1.2), so triad follows each change in jj's behaviour, and the docs say what changed.

The upgrade is one task. Its commit holds the version bump, the API fixes and the regenerated oracles, so the full suite passes on it. A change of triad's behaviour that the new release calls for goes in a commit of its own after it.

## 1. Before you start

- **The release must be three days old.** `.cargo/config.toml` sets `registry.global-min-publish-age = "3 days"`; Cargo refuses a younger version.
- **The pinned jj CLI.** `script/gen-jj-cli-oracle` needs the jj binary of the same release on `PATH`, and git. Installing it is a tool install: ask the owner first. jj's GitHub release binaries and `cargo install jj-cli --version X.Y.Z --locked` both give an exact release.
- **Read jj's changelog** for every release between the pinned one and the new one. Note each entry that touches what triad uses: file merges and `merge.*` settings, conflict markers and their parsing, revsets and the default `revset-aliases`, `absorb`, `resolve`, merge and diff tools, the config files and their layering, snapshots and `snapshot.*`, the operation log, divergent changes, colocated git. Step 5 checks each.

## 2. Bump the pins

All in the root `Cargo.toml`:

| Crate | Pin | Rule |
|---|---|---|
| `jj-lib` | `=X.Y.Z` | The release. |
| `jj-core` | `=X.Y.Z` | Moves with jj-lib. |
| `gix` | `=A.B.C` | The version jj's workspace `Cargo.toml` names at the release tag, so the build holds one gix (`DESIGN.md` §A3). |

Then:

```sh
cargo update -p jj-lib -p jj-core -p gix
cargo tree -d -e normal | rg 'gix|jj-'      # no second copy of either
```

- Compare jj-lib's `rust-version` with `rust-toolchain.toml`. A newer one needs the toolchain raised; ask the owner.
- Run `cargo deny check` and `cargo audit`, and check Dependabot (`gh api -X GET repos/{owner}/{repo}/dependabot/alerts -f state=open`), as for any dependency change.

## 3. Build and fix the API

```sh
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

jj-lib's API changes with most releases. Fix each break in the crate that calls jj-lib: `vcs-jj`, and `merge-core` for jj-core. Follow jj-lib's own use of an API in jj-cli at the release tag when its documentation is thin. Keep `unsafe_code` forbidden and add no `#[allow]` to get past a break.

## 4. Regenerate the oracles

Both generators write under `fixtures/oracle/`. A changed corpus is a change in jj's behaviour. Read the diff, find the changelog entry or commit behind it, and make triad follow it.

| Corpus | Command | Needs | Caught when stale |
|---|---|---|---|
| `jj-corpus.jsonl`: jj-core's file merges under each `same_change` × `hunk_level` (`DESIGN.md` §B5.2) | `script/gen-jj-oracle` | nothing | Yes. Its first line names the jj-core release; `merge-core`'s `jj_oracle` tests compare it with `Cargo.lock`. |
| `jj-cli-aliases.json`, `jj-cli-absorb.json`, `jj-cli-resolve.json`: jj's default revset aliases, `jj absorb` and `jj resolve -r` (`DESIGN.md` §I5) | `script/gen-jj-cli-oracle` | the release's `jj` and git on `PATH` | Yes. `vcs-jj`'s `oracle` tests compare each file's version with `Cargo.lock`. |

Then run the oracle tests:

```sh
cargo nextest run --workspace --no-run
cargo nextest run --workspace -E 'package(merge-core) & test(jj_oracle)'
cargo nextest run --workspace -E 'package(vcs-jj) & test(/^oracle::/)'
```

A failure in the aliases test means jj's default `revset-aliases` changed: copy jj's set into `vcs_jj::native::DEFAULT_ALIASES`. A failure in the absorb or resolve tests means triad's rewrite departs from jj's: fix triad, never the corpus. The departures triad keeps on purpose are listed in `DESIGN.md` §I5 and applied by the tests.

## 5. Check the jj-cli behaviour triad copies

No oracle covers these. Read jj's source at the new release tag against each, and change triad where jj changed. GitHub's comparison of the two tags lists the files that changed (`gh api repos/jj-vcs/jj/compare/vOLD...vNEW`); it lists at most 300 files, so fetch a file it leaves out at both tags and compare. The comments that copy jj-cli name the release they follow (`jj-cli 0.47`). After checking, set each to the new release. A name older than the pinned release marks a place not yet checked.

| triad | jj-cli | What to compare |
|---|---|---|
| `vcs-jj/src/config.rs` | `cli/src/config.rs`, `cli/src/config/*.toml` | Config layers and their order, the user and system paths, `JJ_CONFIG`, the `JJ_*` overrides, `--when` scopes, the snapshot defaults. |
| `Cargo.toml` comments on `etcetera` and `whoami` | `cli/src/config.rs` | The crates jj-cli uses to find the home and config directories, and the host and user names. |
| `vcs-jj/src/testing.rs` (`ToolOutcome`), `merge-session/src/semantics.rs`, `merge-session/src/origin.rs` | `cli/src/merge_tools/external.rs` | How `jj resolve` reads a merge tool's exit code and output, `$name` interpolation, the diff tool's temporary directory layout. |
| `vcs-jj/src/tool_config.rs`, `merge-cli`'s `install` | `cli/src/config/merge_tools.toml`, jj's config docs | The `merge-tools.*` keys and their values. |
| `vcs-jj/src/native.rs` (`JjRepository::rewrite`) | `cli/src/cli_util.rs` (the end of a transaction) | Snapshot, rebase of descendants, git `HEAD` and ref export, working-copy checkout, abandon of an emptied working copy. The resolve and absorb oracles cover the result. Read it anyway for steps they do not reach. |
| `vcs-jj/src/absorb.rs` | jj-lib's `absorb.rs` and `annotate.rs`, `cli/src/commands/absorb.rs` | The hunk split and the annotation rule (`DESIGN.md` §I4). |
| `merge-core` (`jj` mode, `DESIGN.md` §B2–§B5) | jj-core's `merge.rs`, `files.rs`, `diff.rs` and `conflicts.rs` | Conflict marker styles and lengths, the parse of markers back into a conflict, `merge.same-change`, `merge.hunk-level`. |

To list the places:

```sh
rg -n 'jj-cli [0-9]+\.[0-9]+' --glob '!target' --glob '!vendor/**'
```

## 6. Update the docs

Set each mention of the old version to the new one, and update the text where step 4 or 5 found a change:

```sh
rg -n '(jj-lib|jj-core|jj-cli|jj absorb|jj) [0-9]+\.[0-9]+' docs crates/*/README.md README.md Cargo.toml
```

- `docs/PLAN.md` §1: the dependencies line.
- `docs/DESIGN.md`: §A3 (the gix pin), §A7.1 (`vcs-jj`), §B2 (the heading), §I4 (`jj absorb`'s behaviour), and every section whose design changed.
- `docs/SPEC.md` when jj's behaviour, and so triad's, changed.
- `docs/USER-GUIDE.md` when a command's output or behaviour changed.
- `crates/vcs-jj/README.md` and `crates/vcs-git/README.md`.

## 7. Test

The quick profile leaves out the tests that build jj repositories, so run the full suite:

```sh
script/checks
```

- Linux: `act workflow_dispatch -W .github/workflows/triad.yml …` (`AGENTS.md` § Commands), or CI.
- Windows: CI. The jj repository tests are slow there (`PLAN.md` §3); compare the run time with the last green run.

## 8. Check by hand with the real jj

The tests reach jj through jj-lib only. These steps run the new jj binary against triad:

- `docs/release-checklist.md` § Merge and diff tool integration, the jj rows: `jj resolve` and `jj diff --tool` with triad, in a scratch repository and config.
- `script/gen-conflict-repo`, then `triad-merge stack`, `resolve -r`, `absorb --list` and `history` in it, and `jj log`, `jj op log` and `jj st` after each write: jj reads what triad wrote without complaint.

## 9. Commit

- Run `script/pre-commit`, then commit: `build(vcs-jj): upgrade jj-lib and jj-core to X.Y.Z`. The body names the gix version and each behaviour change triad followed.
- A behaviour change large enough to need its own tests and docs goes in a commit after the bump, `fix(...)` or `feat(...)`, each passing the full suite.
