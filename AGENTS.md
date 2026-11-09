# triad — agent notes

triad is a three-way merge and two-way diff tool. The binary is `triad-merge`. `docs/SPEC.md` sets the behaviour. `docs/PLAN.md` holds the status, the next steps and the design notes. Cite the plan by part and section: `PLAN.md` §B5.8 is part B, section 5.8.

When a task finishes, update `PLAN.md` §1–§3 in the same change. §1 states where the project stands, never a log of changes: jj holds the history. When a change finds the spec wrong or incomplete, update `docs/SPEC.md` in the same change.

Docs describe the current design in the present tense. No before-and-after stories; a measurement is a present fact.

You have skills for Rust development and jj version control.

## jj first, git parity

triad is built for jj. git has full parity and comes second.

- jj leads design, defaults, examples, docs and test fixtures. Design each VCS feature for jj, then give git the same feature, never a lesser one.
- The `vcs` types follow jj's model: n sides, n − 1 bases, optional labels (spec §1.2). git fills the same types.
- Every VCS feature passes the conformance suite on colocated jj, internal jj and git repositories.
- The product reaches jj and git only through jj-lib and gix, never their executables.
- In a jj repository, colocated or not, triad merges with jj's semantics only.

**jj means colocated.** Every jj repository takes jj's default layout, colocated with git, unless a test, script or doc names another. This covers fixtures, scripts, docs and manual checks. The default is named in one place, so a change to jj's default is a one-line change:

- Fixtures: `Fixture::new` follows `vcs_jj::testing::DEFAULT_STORE`.
- Scripts and docs: `jj git init` with neither `--colocate` nor `--no-colocate`.
- A test about one layout names it: `Fixture::colocated` or `Fixture::internal`.
- An internal jj repository keeps its git store inside `.jj`. Create one only on purpose (`Fixture::internal`, `jj git init --no-colocate`) and call it internal.

## Version control

Before the first VCS command, run `jj --ignore-working-copy root` on its own. The checkout is jj on one machine and plain git on another.

- **IMPORTANT:** if it succeeds, use `jj` for every VCS command, `log`, `show`, `status` and `diff` included. Never run `git` on a jj repo. The session's `gitStatus` snapshot is no reason to use git.
- **IMPORTANT:** never create, move or delete a bookmark or branch without explicit permission, `main` included. `jj commit` leaves bookmarks where they are; leave them there.
- Commits are conventional commits with scope `triad`, made through jj: `feat(triad): add text decoding`. One task or sub-task per commit.
- No PII in commits, messages or bookmark names: real names, email addresses, usernames, machine paths such as `/Users/<name>/`, hostnames, tokens, credentials. Check the diff before `jj commit`, `jj describe` (finalising) and `git commit`.

## Commit gate

jj has no commit hooks. Run the gate before `jj commit`, `jj describe` (finalising) and `jj squash`:

```sh
script/pre-commit              # from the repo root
```

**The gate takes under 30 s on a warm build.** An edit to `merge-app` takes 18 s. A run over 30 s is a defect, not a slow run. Stop, report it to the owner with the time of each step, and fix the cause before going on. Never wait it out or raise a limit. Check `target/debug/deps` first: a huge file count there slows every rustc run (`PLAN.md` §A13.5).

The gate reads the changes in `@` (jj) or against `HEAD` (git). It skips the checks when no build file changed: `.rs`, `Cargo.toml`, `Cargo.lock`, `deny.toml`, toolchain and format config, `script/`, `fixtures/`. Otherwise it runs `script/checks --quick` on the changed paths:

```sh
cargo fmt --all --check
script/check-workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --profile quick -E '<changed crates and their dependents>'
cargo deny check                  # when a manifest, Cargo.lock or deny.toml changed
<vendor/gpui-base unit tests>     # when vendor/ changed
```

- Every step passes with zero warnings. On a format failure, run `cargo fmt --all` and rerun.
- A change to `docs/` runs no tests. A change to `vendor/` runs the tests of the crates that depend on `gpui-kit` and their dependents. Any other path outside `crates/` runs the whole profile.
- The `quick` profile (`.config/nextest.toml`) leaves out the tests that build jj repositories and the scrolling tests; git conformance stays in. A change to `vcs-jj`, to jj behaviour or to scrolling runs the full suite before commit: `script/checks`.
- What keeps it fast: `cargo nextest run --no-run` builds only what the tests need; clippy always covers the whole workspace, so every run shares one feature set and build cache; each crate links one integration-test binary, `tests/main.rs` (`autotests = false`); dev builds carry no debug info.
- In a plain git checkout, install the gate once: `ln -sf ../../script/pre-commit .git/hooks/pre-commit`.

## Tests

- Run tests with `cargo nextest run`, never `cargo test`. The exception is doctests, which nextest cannot run: `cargo test --doc --workspace`. `script/checks` and CI run them.
- Build and test separately: `cargo nextest run --workspace --no-run`, then targeted tests while iterating: `cargo nextest run --workspace -E 'package(<crate>) & test(<filter>)'`. A filtered run takes seconds once built. nextest stops a test after 30 s.
- Never `-p` for a local dev build, run or test run; the Linux container keeps its own target directory. It resolves a narrower feature set than the workspace, rebuilds the tree-sitter grammars, and the next gate rebuilds them again, past 30 s.
- On macOS, a first run after a relink that stalls before any test starts is Gatekeeper scanning the new binaries. README § Build and test gives the fix.
- Unit tests sit next to the code in `#[cfg(test)]` modules; integration tests in each crate's `tests/`.

## Push

Push only on explicit request. Before `jj git push` or moving a shared bookmark, run `script/pre-push`. It does nothing when there is no unpushed revision.

- Default: formats `**/*.rs` outside `vendor/` in the tip (newest non-empty mutable revision) with `jj fix`, then runs `script/checks` on it with the full suite.
- `--full`: formats every mutable revision in `::@` and checks each in its own checkout through `jj run`.
- Plain git: checks `HEAD` (`--full`: each commit in `@{upstream}..HEAD`), without formatting. Install once: `ln -sf ../../script/pre-push .git/hooks/pre-push`.

`script/agent-pre-push-hook` runs the tip check on every `jj git push` and `git push` and blocks the push on failure. Claude Code calls it as a `PreToolUse` hook from `.claude/settings.json`. It checks the tip of `@`, not the pushed bookmark, so push only the bookmark on `@-`, with an empty `@` above it.

## Rules

- **No unsafe code.** The workspace sets `unsafe_code = "forbid"`, which `#[allow]` cannot override. FFI or a platform call goes through a third-party crate with a safe API (spec §2.3).
- **Stable toolchain only.** `rust-toolchain.toml` names the stable channel. No nightly toolchains, tools or features.
- **No tool installs without permission.** Ask before `cargo install`, `rustup toolchain install`, `rustup target add`, `brew install` or any other system or toolchain change.
- **No reports to other projects.** No issues, pull requests, comments or posts upstream. Record dependency problems in `docs/gpui-kit-issues.md` or a similar file under `docs/`.
- **Workspace lints in every crate.** Each `Cargo.toml` sets `[lints] workspace = true`; `script/check-workspace` enforces it.
- **Clippy pedantic, zero warnings.** A lint is turned off for the workspace only in the root `Cargo.toml`, with a comment giving the reason. A local `#[expect(clippy::…)]` carries `reason = "…"`; prefer `expect` to `allow`.
- **Tests first.** Write the tests, watch them fail, implement, then break the code to confirm the tests catch it.
  - Expected values come from the spec, a hand calculation or the oracle corpora, never from the code's output.
  - Assert exact values and specific errors.
  - Cover boundaries, invalid input, error paths and state after a failure.
  - A bug fix starts with a test that reproduces it.
  - Mock only external boundaries. No sleeps, no unseeded randomness.
  - Never delete a test or weaken an assertion to get a run passing.
- **Typed errors.** A function that can fail returns `Result<T, E>`.
  - Each crate defines its error enums with `thiserror`. Use `anyhow` only where a dependency returns `anyhow::Error`, such as GPUI's `open_window`, and wrap it in a typed variant there.
  - Propagate with `?`. Add context with `map_err` or a `#[source]` field, and keep the cause.
  - `unwrap`, `expect` and `panic!` only for invariants the code guarantees; each `expect` message states the invariant. Tests may unwrap.
  - Never drop an error silently. Handle it, return it, or log it with `tracing` and say why going on is safe.
  - `Option` for a value that may be absent, never an error or a sentinel.
- **Named constants.** No magic numbers.
- **UI style.** Views follow `docs/STYLE.md`: 12 px chrome text, `.small()` controls, menus from `split_menu`, a 4 px grid, theme colours. Zed's `ui` crate is the reference. A view that departs from it says why in a comment.
- **Borrow before you copy.** triad holds whole files, often several copies of a large one.
  - Parameters take `&str`, `&[T]` and `&Path`. Take ownership only to store or consume the value.
  - Return borrows, slices or iterators when the data outlives the call; an owned value only when the function builds it.
  - Never `.clone()`, `.to_owned()`, `.to_string()` or `.collect()` to quiet the borrow checker. Restructure. A clone of a large value carries a comment giving the reason.
  - Address text by range into one buffer, never a `Vec<String>` of lines. Share whole texts with `Arc<str>` or a rope.
  - Keep iterators lazy. Use `Cow` for values usually borrowed and sometimes changed (`diff::line_key`).
  - Move values not used again; `std::mem::take` or `std::mem::replace` out of a `&mut`.
  - `with_capacity` when the length is known. Small `Copy` types go by value.
- **Licences.** Dependencies pass `cargo deny check`.
- **Security audits.** Run `cargo audit` once per working session and after any dependency change. Fix or report every vulnerability. Before adding or bumping a dependency, also check Dependabot (`gh api -X GET repos/{owner}/{repo}/dependabot/alerts -f state=open`): RustSec, which `cargo audit` and `cargo deny` read, lags GitHub advisories. Known and needing no action: unmaintained-crate warnings deep in the GPUI tree, and the future-incompatibility notice for `block 0.1.6`, which GPUI's macOS backend pulls in through `cocoa` and `metal`, until a `gpui-kit` release drops it.

## Commands

| Task | Command |
|---|---|
| Format | `cargo fmt --all` |
| Try a merge by hand | `cargo build --workspace`, then `target/debug/triad-merge inspect fixtures/demo/{base,left,right}.txt` (`fixtures/demo/README.md`) |
| A disposable jj repository with conflicts in two Rust files | `script/gen-conflict-repo [dest]` (`fixtures/conflict-repo/README.md`) |
| Build `target/release/triad.app` | `script/macos-app` |
| Benchmarks against spec §9 | `cargo bench -p merge-core --bench merge`, `cargo bench -p merge-syntax --bench boundaries`, `cargo bench -p merge-cli --bench memory`, then `script/check-bench` |
| Cost of `resolve`'s repository steps on 10k and 100k files (`PLAN.md` §D5.5) | `cargo bench -p merge-cli --bench resolve` |
| Measure the running app against spec §9 (`PLAN.md` §A6.7) | `script/measure-app [refresh-hz] [output-extension]` (`rs` adds Rust highlighting); one mode: `TRIAD_MEASURE=<open\|scroll-rows\|scroll-result\|type\|pick\|syntax> target/release/triad-merge target/measure/<lines>/{base,left,right}.txt target/measure/<lines>/out.txt` |
| Check that `script/pre-push` removes its git clone on every exit; `script/checks` runs it when either script changes | `script/test-pre-push` |
| Rewrite `images/` from the mark's geometry | `swift script/gen-images.swift` |
| Print the accessibility tree of a running window | `swift script/ax-tree.swift PID [NAME...]` (`docs/release-checklist.md`) |
| Linux test machine in OrbStack (`PLAN.md` §E7.1) | `script/linux/run image` once; then `script/linux/run cargo build -p triad-merge`, `script/linux/run cargo nextest run --workspace --profile linux-emulated`, `script/linux/run script/linux/smoke wayland` or `x11` |
| Linux CI locally (needs OrbStack running) | `act workflow_dispatch -W .github/workflows/<triad\|bench\|audit>.yml [-j <job>] -P ubuntu-latest=catthehacker/ubuntu:act-latest --container-architecture linux/arm64`; `triad.yml` runs only its Linux matrix entry under act. act runs as root, so the four tests of unreadable or unwritable paths fail there; GitHub's runners do not run as root |
| Linux packages into `target/linux/dist` | `script/linux/run script/package x86_64-unknown-linux-gnu` |

## Layout

| Path | Contents |
|---|---|
| `crates/merge-core` | Engine: text, diff, alignment, resolution, edits, undo, output. `testing` feature: seeded fixture generator. |
| `crates/merge-syntax` | Languages, tree-sitter grammars, highlight spans, syntax-aware merge boundaries. No UI. |
| `crates/merge-view` | View model. No GPUI. |
| `crates/merge-config` | The config file's format, settings, key bindings and the names of their values. No repository. |
| `crates/merge-session` | The layer under both front ends: the merge session, semantics, repository discovery, conflict loading, line origins, file reads and writes, the recovery journal, exit codes. |
| `crates/merge-cli` | CLI library: arguments, headless modes, `inspect`, `install`. No GUI. |
| `crates/merge-app` | GPUI shell. |
| `crates/triad-merge` | Binaries `triad-merge` and `triad-merge-gui`. |
| `crates/vcs`, `vcs-jj`, `vcs-git` | Repository trait and its jj-lib and gix backends. |
| `fixtures/core` | Hand-written triples with expected outputs, named for the case they cover. |
| `fixtures/oracle` | `jj-corpus.jsonl`: jj-lib merges with jj seeds, from `script/gen-jj-oracle`. `corpus.jsonl`: `git merge-file` triples, from `script/gen-oracle`. |
| `fixtures/conflict-repo` | Sources of a disposable jj repository with two conflicted Rust files, for `script/gen-conflict-repo`. |
| `fixtures/demo` | A 980-line Rust merge with 20 conflicts, from `script/gen-demo`. |
| `images` | Mark, logo and app icon as SVG, PNG, ICNS and ICO, from `script/gen-images.swift`. |
| `packaging/linux` | Desktop entry of the Linux packages. |
| `docs` | Spec, plan, UI style, JSON schema, release checklist, gpui-kit issues, `scroll-debugging.md`, `mark-colours.md`: the mark's colour schemes, and `residue.md`: the files triad and its scripts leave behind. |
| `vendor/gpui-base` | `gpui-base` 0.8.1 with triad's editor patch, through `[patch.crates-io]`. Outside the workspace: no workspace lints, `cargo fmt` or `jj fix`. See its `PATCHES.md`. |
