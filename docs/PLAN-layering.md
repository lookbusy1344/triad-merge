# triad — layering plan

Inputs: [SPEC.md](SPEC.md), [PLAN.md](PLAN.md), [DESIGN.md](DESIGN.md), [ARCHITECTURE.md](ARCHITECTURE.md). Rules for changes: `AGENTS.md`.

This file is part K of the plan, kept apart while the work is open. Cite it by part and section: §K3 is section 3 here. [PLAN.md](PLAN.md) §1–§3 carry its status. Each section is one commit, in order, and follows the ground rules of §A1.

## K0 Goal

The pure crates hold up: `merge-core`, `merge-syntax` and `merge-view` have no GPUI, no file system, no process and no environment; the dependency graph has no cycle; the front ends reach repositories only through `RunRepository` and `dyn vcs::Repository`. The leaks are above them. `merge-session` is not yet the whole layer under the front ends, so the CLI and the GUI each assemble runs, and the GUI holds spec rules and one defect. This part closes those leaks.

## K1 A save into a commit takes the bytes — done

`resolve::Destination` names where a result goes: a file and its record, or a jj commit's conflict. A save into a commit hands the encoded bytes to `InCommit::write`; nothing writes a file. A refusal is `SaveError::Commit`, a failed write.

## K2 No temporary file for a commit's conflict — done

`merge-app::run::exit` calls `std::process::exit` inside the GPUI loop, and on macOS the platform's own quit, `terminate:`, exits the process the same way, so no destructor runs at the end of a run. A commit's conflict therefore holds no file: `Destination::file` is none for it, and `Destination::name`, the conflicted path, gives the merge window its result heading and language. The journal and jj's line origins take the conflicted file under the root (`InCommit::worktree_file`).

## K3 The run's repository comes from the environment — done

`repository::Discovery` holds the jj config of a run, built once from its `Env`. Every repository a front end opens comes from its constructors (`find`, `of_merge`, `of_diff_tool`, `of_file`, `of_resolve`), and the jj-native calls and `resolve::load_at_commit` take it. The front ends hold no `ConfigEnv`. A test reads a revset alias from the `Env`'s `JJ_CONFIG` through each constructor and through a repository found from the run's discovery.

## K4 One step applies the chosen semantics — done

`merge_session::apply_semantics` chooses the semantics and markers and sets them in a form's `MergeOptions`; `merge`, `resolve` and `stack` take it. The stack, absorb and history forms list once before the window opens and hand the listing on in `WindowForm::listing` (`jj_native::Listing`); the window shows it first and lists again only on a refresh. A test per window opens it on a narrower listing than its own and finds that listing shown at once and kept.

## K5 Spec rules leave the GUI — done

`merge_session::diff_tool::read_pair` holds spec §7.1's rule for `jj diff --tool`: the right side opens the working-copy file and the left turns read-only. `diff_tool::save` writes a diff window's sides and snapshots jj's working copy when a write succeeded. The GUI maps the sides onto panes and takes the save's results. Headless tests cover both rules, an absent side, an unreadable side and a failed write.

## K6 One jj facade — done

The front ends take the jj-native report types from `merge_session::jj_native` only. `vcs-jj` and `vcs-git` are dev-dependencies of `merge-cli` and `merge-app`, for fixtures; `script/check-workspace` fails on a normal dependency.

## K7 MergeWindow in parts — done

Each job's methods live in a child module of `window`: `output`, `origins`, `inputs`, `measuring`, `scrolling`, `journal`, `save` and `search`. `window.rs` keeps the type, its construction and its rendering, 3 300 lines. The state stays on `MergeWindow`, where the render reads it; a job whose state no other job reads can move it into a struct of its own when it next changes. No behaviour changes; the existing GUI tests cover it.

## K8 Highlight queries in `merge-syntax` — done

`Language::highlight_query` gives the C#, Swift, Kotlin and C++ queries; `merge-app` registers them with the editors' registry and drops its five grammar dependencies. TSX's query comes from the registry's TypeScript entry, so it stays with the registration in `merge-app`. Tests compile each query against its grammar, find each sample's opening comment coloured as a comment, and find C's lines in the C++ query.

## K9 Small edges

`merge-config` depends on `vcs` only for `IdKind`; the setting converts at the session layer instead. A stale doc comment in `merge-session::repository` names an equality `RunRepository` does not have.
