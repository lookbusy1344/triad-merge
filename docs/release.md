# Release procedure

The steps to cut a release vX.Y.Z. A release happens only on the owner's request. The gates do the checking: the commit gate (`script/pre-commit`) runs before each commit, and the push hook runs `script/pre-push`, the full suite, on the push. Run neither by hand on top of that. [release-checklist.md](release-checklist.md) holds the manual checks on the machine; run the parts a release's changes touch.

## Version numbers

The version lives once, in `[workspace.package]` of the root `Cargo.toml`; every crate takes it from there. Before 1.0, a release with new features bumps the minor version (0.1.0 → 0.2.0) and one with fixes alone bumps the patch (0.1.0 → 0.1.1). `release.yml` fails when the tag does not match the version.

## 1. Start from main

```sh
jj git fetch
jj new main
gh run list --workflow triad.yml --branch main --limit 1    # the last CI run on main passed
```

CI covers Linux, macOS and Windows; a local gate covers only the platform it runs on.

## 2. Update dependencies

```sh
cargo update
```

`.cargo/config.toml` takes only versions at least three days old. Exact pins stay put: jj-lib, jj-core and gix move only through [jj-upgrade.md](jj-upgrade.md); gpui-kit through `docs/gpui-kit-issues.md`.

Then check for advisories:

```sh
cargo audit
gh api -X GET repos/{owner}/{repo}/dependabot/alerts -f state=open
```

Fix or record each one. AGENTS.md § Rules lists the warnings that need no action. `cargo deny check` runs in the commit gate when `Cargo.lock` changes.

Commit, with the gate first:

```sh
script/pre-commit
jj commit -m "build: update dependencies"
```

Skip the commit when `cargo update` changed nothing.

## 3. Bump the version

1. Set `version` in `[workspace.package]` of the root `Cargo.toml`.
2. `cargo update --workspace` writes the new version into `Cargo.lock` and moves nothing else. The gate reads the lock file with `--locked`, so do this before it runs.
3. Update the docs that name the release: README § Status names the minor version; `docs/PLAN.md` §1 names the latest release.
4. Run `script/code-size` and update README § Code size from its output: the total, the three parts and the product code of `merge-app` and `merge-core`, each rounded to the nearest thousand.
5. Add a section to the top of `CHANGELOG.md`, headed `## X.Y.Z — YYYY-MM-DD`: a short list of what changed for users since the last tag, new features first, then fixes. Leave out refactors, tests, CI and docs. `jj log -r 'tags()..main'` lists the commits; `feat` and `fix` commits are the place to start. The release notes take this section (§6).
6. Commit:

```sh
script/pre-commit
jj commit -m "chore(release): vX.Y.Z"
```

## 4. Push main

```sh
jj bookmark set main -r @-
jj git push --bookmark main
```

The push hook runs the full suite on the tip and blocks the push on a failure. Keep `@` empty above the release commit: the hook checks the tip of `@`. Push in its own command, after the commit; the hook runs before the command, so a commit in the same command is not yet there to check.

Wait for CI on the pushed commit:

```sh
gh run watch "$(gh run list --workflow triad.yml --branch main --limit 1 --json databaseId -q '.[0].databaseId')"
```

## 5. Tag and push the tag

```sh
jj tag set vX.Y.Z -r main
jj git push --tag vX.Y.Z
```

The push hook runs `script/pre-push` again, but main is now immutable, so its stack is empty and it checks nothing.

## 6. Watch the release build

The tag starts `.github/workflows/release.yml`. It runs `triad.yml`'s checks and tests on macOS ARM64 and on Linux and Windows, x64 and ARM64, first; a failure on any platform builds and publishes nothing. Then it builds `triad.app` for macOS ARM64, and the Windows zips and Linux packages for x64 and ARM64, and publishes them with notes GitHub writes from the commits.

```sh
gh run watch "$(gh run list --workflow release.yml --limit 1 --json databaseId -q '.[0].databaseId')"
```

Then replace the generated notes with the release's section of `CHANGELOG.md`:

```sh
awk '/^## /{p = ($2 == "X.Y.Z"); next} p' CHANGELOG.md > "${TMPDIR:-/tmp}/notes.md"
gh release edit vX.Y.Z --notes-file "${TMPDIR:-/tmp}/notes.md"
```

## 7. Check the published release

1. Download the macOS zip and compare its SHA-256 (`shasum -a 256`) with the digest on the release page.
2. Clear the quarantine, install it as README § Install says, and run `triad-merge --version`: it names X.Y.Z.
3. Open a merge from `fixtures/demo` and save it.

## When a release goes wrong

Once a release is published, never move or reuse its tag: fix the fault on main and release the next patch version. A tag whose build failed before `gh release create` published nothing; delete it, fix main, and tag again:

```sh
jj tag delete vX.Y.Z
jj git push --deleted --dry-run    # pushes every deleted bookmark and tag: check it names only vX.Y.Z
jj git push --deleted
```
