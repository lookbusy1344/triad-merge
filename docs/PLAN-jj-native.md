# triad — jj-native features plan

Inputs: [SPEC.md](SPEC.md), [PLAN.md](PLAN.md), [DESIGN.md](DESIGN.md). Rules for changes: `AGENTS.md`.

This file is part I of the plan, kept apart while the work is open. Cite it by part and section: §I3 is section 3 here. [PLAN.md](PLAN.md) §1–§3 carry its status. When every step lands, the spec holds the behaviour, `DESIGN.md` part I holds the design, and this file goes.

## I1 Goal

Features built on what jj records and git does not: conflicts stored in commits, change IDs that survive a rewrite, the evolog and the operation log. Spec §1.2 allows them: where jj and git share a capability, both get it; where only jj has one, triad builds on it for jj alone.

| Step | Command | jj fact it rests on |
|---|---|---|
| §I2 | `stack`, `resolve -r REV PATH` | Conflicts in commits; descendants rebase on a rewrite |
| §I3 | `diverge CHANGE` | Change IDs; the evolog's predecessors |
| §I4 | `resolve PATH --pair N` | Conflicts of more than two sides |
| §I5 | `history PATH`, `diff PATH --op OP` | The operation log's working-copy snapshots |
| §I6 | `diff PATH --evolog` | The evolog |
| §I7 | `absorb` | Annotation; descendants rebase on a rewrite |

Every step ships a command line with `--json` or `--headless` for scripts, tests and agents. A step that fits a window triad has opens it; a new kind of window is open work (§I9).

## I2 Step 1: conflicts across a stack

Done: spec §7.5.1, `DESIGN.md` §I1–§I2.

- `triad-merge stack [REVSET] [--json]` lists the conflicted commits in `REVSET`, parents first, each with its conflicted paths. Default revset: `conflicts() & mutable()`. A path is *inherited* when a parent holds a conflict at the same path; resolving the first commit often clears it. Exit 0 with no conflicted commit, 1 with some.
- `triad-merge resolve -r REV PATH` reads the conflict at `PATH` in the commit `REV` names, without checking it out. The merge window opens on it, or `--headless` applies `--commands`. The result goes to a temporary file; each save writes it into the commit, as `jj resolve -r` does: markers jj parses back stay a conflict. The descendants rebase; a moved `@` is checked out to disk. One operation per save: `resolve PATH in CHANGE in triad`.
- The working copy is snapshotted first, as in spec §7.4, so no edit on disk is lost.
- An immutable commit is refused before anything is written. `immutable_heads()` comes from the user's jj config, else jj's default.
- Each save reports, per descendant, the conflicted paths before and after. `--headless --dry-run` reports without writing.
- Tests: the list's order and inherited marks; a resolution in an ancestor clears an inherited conflict in a child; markers saved back stay a conflict; an immutable commit is refused; `@` moves on disk when an ancestor changes; dry run writes no operation.

## I3 Step 2: divergent changes

Done: spec §7.5.2, `DESIGN.md` §I3.

- `triad-merge diverge CHANGE [--dry-run] [--json]` merges the visible commits of a divergent change. The base is their nearest common predecessor in the evolog: the commit each copy was rewritten from. The tree is jj's merge of the two trees over the base's; a conflicting file stays conflicted in the commit, for `resolve -r`.
- The first commit, in the order the change's index lists them, keeps its parents and change. The others are abandoned; their descendants rebase onto the merged commit.
- Descriptions merge three ways over the base's. When both copies change the description differently, the first copy's wins and the report says so.
- More than two copies merge in turn. A change with one visible commit is refused, as is a change whose copies share no predecessor.
- Tests: one file changed on each copy merges clean; the same line changed two ways stays a conflict; the description rule; the descendant of an abandoned copy moves; no common predecessor is refused.

## I4 Step 3: reduce a conflict of more than two sides

Done: spec §7.5.3, `DESIGN.md` §I4.

- jj orders a conflict's terms as side 1, base 1, side 2, base 2, …, side n. `triad-merge resolve PATH --pair N` merges sides N and N + 1 over base N as a three-way merge, in the merge window or `--headless`. A resolved save replaces the three terms with the result: the conflict keeps n − 1 sides. A save with markers writes nothing.
- Works with `-r REV` (§I2). Refused for a conflict of two sides, where it is plain `resolve`, and for an `N` out of range.
- Tests: a three-sided conflict reduced at each pair holds the expected two sides and base; a reduction that leaves one side resolves the file; out-of-range and two-sided refusals.

## I5 Step 4: a file's history in the operation log

Done: spec §7.5.4, `DESIGN.md` §I5.

- `triad-merge history PATH [--json]` lists the distinct versions of `PATH` in the working-copy commit across the operation log, newest first: the operation's short ID, time and description, and whether the file is absent there. Adjacent operations with the same version show once.
- `triad-merge diff PATH --op OP` compares the working-copy file with its version in the working-copy commit at operation `OP`. Edits save to the working copy, as for `--rev`.
- jj records the working copy when a command runs, or when its watcher snapshots. Coverage is that fine.
- Tests: three snapshots of a file give three versions in order; an unchanged operation folds; a deleted file shows absent; an unknown operation is refused.

## I6 Step 5: a change's patch across rewrites

- `triad-merge diff PATH --evolog [-r REV] [--from COMMIT]` compares two versions of one change's patch. Left: the predecessor (`--from`, else the latest earlier version whose tree differs) rebased onto the change's current parents. Right: the change now. Rebase noise drops out; what remains is what the rewrite changed in the patch. Both sides are read-only.
- A predecessor whose rebase conflicts at `PATH` shows jj's markers on the left.
- `--inspect` prints the regions, as `diff --inspect` does.
- Tests: a rebase that changes only the parent shows no difference; an amended line shows as one region; no predecessor is refused.

## I7 Step 6: absorb by hunk

- `triad-merge absorb [PATH…] [--into REVSET] [--json]` lists each hunk of `@` against its parent with the commit it goes into: the one commit that last changed every line the hunk removes, or for an insertion the commit holding the lines on both sides. A hunk with no such commit stays in `@`. `--into` limits the targets; default `mutable() & ::@-`.
- `--move N=REV` sends hunk N to another commit; `--keep N` keeps it in `@`. `--apply` writes: each target takes its hunks, descendants rebase, `@` keeps the rest, in one operation `absorb in triad`.
- Tests: two hunks go to the two commits that wrote their lines; an insertion between one commit's lines goes there; a hunk across two commits' lines stays; `--move` and `--keep`; the apply moves the hunks and leaves `@` with the rest.

## I8 Order

One commit per step, in the order of §I2–§I7. Each lands with its spec text, its `DESIGN.md` part I section and its tests. The final step moves the design into `DESIGN.md` and deletes this file.

## I9 Open work

- A window for `stack`: the list of conflicted commits beside the merge window, and the effect of a resolution on the descendants before it is saved.
- The number of sides per region in the merge window, after jj's simplification.
- A window for `history`: a timeline of versions to compare and restore from.
- A window for `absorb`: hunks dragged onto commits. Part H's selection model comes first.
