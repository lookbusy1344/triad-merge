# The windows' text in English (spec §6.5). Every other catalogue in this directory holds
# the same messages with the same variables; `i18n::tests` checks it.
#
# A count selects its plural form by the language's CLDR rules: `[one]`, `[few]`, `[many]`
# and `*[other]` (the default), or an exact number such as `[0]`. Product, VCS and command
# names stay as they are: triad, git, jj, Jujutsu, git mergetool, jj diff --tool.

## About window

about-title = About triad
about-description = Three-way merge and two-way diff
about-vcs-support = Built for Jujutsu, with full Git support
about-version = Version { $version }
about-build-commit = Commit { $commit }
about-build-commit-change = Commit { $commit }, change { $change }

## Window parts

result-title = Result
merge-toolbar = Merge tools
diff-toolbar = Diff tools
input-lines = Input lines
go-to-line-field = Line number
no-matches = No matches
close = Close
settings-errors = The config file has errors: { $errors }

## Save, cancel and restore questions

prompt-save-unresolved = Save with unresolved conflicts?
prompt-save-with-conflicts = Save with Conflicts
prompt-keep-editing = Keep Editing
prompt-discard-detail = Discard writes nothing, and the merge stays unresolved in git or jj.
prompt-discard = Discard
prompt-cannot-write = Cannot write the output
prompt-cannot-write-copy = Cannot write the copy
prompt-cannot-record = The repository did not record the save
prompt-no-output = This window has no output file.
prompt-quit = Quit
prompt-try-again = Try Again
prompt-restore-session = Restore the previous session?
prompt-restore-detail = A session on these files ended before its merge was resolved. Its picks and edits can come back.
prompt-restore = Restore
prompt-start-over = Start Over
prompt-cannot-restore = Cannot restore the session
prompt-save-changes = Save your changes?
prompt-cannot-save = Cannot save the file
prompt-ok = OK
prompt-changed-detail = Reload it and lose your edits, or keep your version and save over it later.
prompt-reload = Reload
prompt-output-changed-detail = Another program changed it since the merge opened. Save & Exit replaces its changes with the merge.
prompt-keep-mine = Keep Mine
prompt-changed-on-disk = { $name } changed on disk.
prompt-descendants-changed =
    { $count ->
        [one] Saving changes the conflicts of { $count } descendant.
       *[other] Saving changes the conflicts of { $count } descendants.
    }
prompt-unresolved-detail =
    { $count ->
        [one] { $count } conflict keeps its markers, and git or jj sees the merge as unresolved.
       *[other] { $count } conflicts keep their markers, and git or jj sees the merge as unresolved.
    }
prompt-unresolved-detail-git =
    { $count ->
        [one] { $count } conflict keeps its markers, and git or jj sees the merge as unresolved. git mergetool then puts back its own copy of the file; run it again to restore this session.
       *[other] { $count } conflicts keep their markers, and git or jj sees the merge as unresolved. git mergetool then puts back its own copy of the file; run it again to restore this session.
    }

## Counts

count-commits =
    { $count ->
        [one] { $count } commit
       *[other] { $count } commits
    }
count-files =
    { $count ->
        [one] { $count } file
       *[other] { $count } files
    }
count-versions =
    { $count ->
        [one] { $count } version
       *[other] { $count } versions
    }
count-hunks =
    { $count ->
        [one] { $count } hunk
       *[other] { $count } hunks
    }
count-differences =
    { $count ->
        [one] { $count } difference
       *[other] { $count } differences
    }
count-conflicts =
    { $count ->
        [one] { $count } conflict
       *[other] { $count } conflicts
    }
count-unchanged-lines =
    { $count ->
        [one] { $count } unchanged line
       *[other] { $count } unchanged lines
    }
count-duplicated-blocks =
    { $count ->
        [one] { $count } duplicated block
       *[other] { $count } duplicated blocks
    }

## Conflicted commits (stack window)

no-paths = none
stack-title = Conflicted commits
stack-empty = No conflicted commits.
refresh = Refresh
stack-preview = Preview Save
preview-running = Working out what the save would do…
working-copy = working copy
no-description = (no description set)
commit-tooltip =
    Change ID: { $change }
    Commit ID: { $commit }
    Date: { $date }
inherited = (inherited)
stack-resolved = Resolved { $path } in { $change }.
stack-saved-conflicts = Saved { $path } in { $change } with conflicts.
stack-error = The merge of { $path } in { $change } ended with an error.
preview-resolves = Saving resolves the path in { $change }.
preview-keeps = Saving keeps conflicts in { $change }.
preview-no-rebase = No descendant is rebased.

## Status icons

status-added = Added
status-modified = Modified
status-deleted = Deleted
status-moved = Moved
status-copied = Copied
status-conflicted = Conflicted

## Changed files (directory window)

dirs-title = Changed files
dirs-empty = No files differ.
dirs-against = { $files } against { $rev }
file-added = added
file-deleted = deleted
file-modified = modified
file-renamed = renamed from { $from }
file-copied = copied from { $from }
file-from = from { $from }
file-undecodable = undecodable
file-binary = binary
file-special = special
file-label = { $path } { $change }
file-label-why = { $path } { $change }, { $why }
at-operation = @ at operation { $operation }
no-file = { $name } (no file)

## History window

history-title = History of { $path }
history-empty = No versions.
version-conflict = (conflict)
version-absent = (absent)
version-other = (other)

## Absorb window

absorb-title = Absorb
absorb-empty = Nothing to absorb.
absorb-apply = Apply
absorb-keep = Working copy
absorb-note = Absorbed { $hunks } into { $commits }.
absorb-stale = The working copy is stale: run jj workspace update-stale.
absorb-skipped = Skipped { $path }: { $reason }

## Open dialog

open-title-merge = Open a merge
open-title-diff = Open a diff
mode-merge = Merge
mode-diff = Diff
start-merge = Merge
start-diff = Compare
slot-base = Base
slot-left = Left
slot-right = Right
slot-output = Output
choose-base = Choose the base file
choose-left = Choose the left file
choose-right = Choose the right file
choose-output = Choose the output file
missing-slot = Choose a file for { $slot }.

## Menus

menu-settings = Settings…
menu-save = Save
menu-save-and-exit = Save & Exit
menu-save-copy-as = Save Copy As…
menu-cancel = Cancel
menu-edit = Edit
menu-merge = Merge
menu-go = Go
menu-view = View
menu-undo = Undo
menu-redo = Redo
menu-find = Find…
menu-go-to-line = Go to Line…
menu-copy-as-patch = Copy as Patch
menu-copy-difference-as-patch = Copy Difference as Patch
menu-toggle-left = Toggle Left
menu-toggle-base = Toggle Base
menu-toggle-right = Toggle Right
menu-clear-choice = Clear Choice
menu-pick-selected-lines = Pick Selected Lines
menu-take-base = Take Base
menu-take-left = Take Left
menu-take-right = Take Right
menu-take-left-then-right = Take Left then Right
menu-take-right-then-left = Take Right then Left
menu-take-sorted-union = Take Sorted Union
menu-take-left-all-unresolved = Take Left for All Unresolved
menu-take-right-all-unresolved = Take Right for All Unresolved
menu-take-whole-left = Take Whole Left File
menu-take-whole-right = Take Whole Right File
menu-carry-moved-edit = Carry Edit to Moved Block
menu-drop-duplicate = Drop Duplicate
menu-next-difference = Next Difference
menu-previous-difference = Previous Difference
menu-next-conflict = Next Conflict
menu-previous-conflict = Previous Conflict
menu-go-to-move-end = Go to Other End of Move
menu-base-pane = Base Pane
menu-blame = Blame
menu-syntax = Syntax Highlighting
menu-soft-wrap = Soft Wrap
menu-whitespace = Show Whitespace
menu-collapse-unchanged = Collapse Unchanged
menu-unified = Unified
menu-scroll-sync = Sync Result Scrolling

## Toolbar

tool-label-difference = Difference
tool-label-conflict = Conflict
tool-label-left = Left
tool-label-right = Right
more = More
more-menu = More picks
more-base = Base
more-left-then-right = Left then right
more-right-then-left = Right then left
more-none = None
tool-previous-difference = Previous difference
tool-next-difference = Next difference
tool-previous-conflict = Previous conflict
tool-next-conflict = Next conflict
tool-left = Toggle left in the current region
tool-base = Toggle base in the current region
tool-right = Toggle right in the current region
tool-left-then-right = Take left then right in the current region
tool-right-then-left = Take right then left in the current region
tool-clear = Clear the current region's choice
tool-undo = Undo
tool-redo = Redo
tool-base-pane = Show or hide the base pane
tool-find = Find
tool-go-to-line = Go to a line in the result
tool-syntax = Turn syntax colours on or off
tool-soft-wrap = Turn soft wrap on or off
tool-whitespace = Show or hide whitespace

## Conflict badge and pick targets

pick-label = { $title } in region { $region } of { $regions }
pick-label-unresolved = { $title } in region { $region } of { $regions }, unresolved
line-pick-label = Line { $line } of { $title }
badge-conflict = Conflict { $number } of { $count }
badge-sides = { $kept } of { $sides } sides
badge-all-resolved = all resolved
badge-resolved-one = Conflict resolved
badge-resolved-all = All { $count } resolved
badge-unresolved-of = { $unresolved } of { $count } unresolved

## Panes

role-left = Left
role-right = Right
role-base = Base
role-side = Side { $number }
role-base-of = Base { $number }
role-result = Result
cell-line = { $title }, line { $line }: { $text }
cell-no-line = { $title }, no line
cell-no-line-deleted = { $title }, no line: { $deleted }
placeholder-nothing =
    { $count ->
        [one] Nothing picked ({ $count } line dropped)
       *[other] Nothing picked ({ $count } lines dropped)
    }
placeholder-deleted =
    { $count ->
        [one] { $count } line deleted ({ $roles })
       *[other] { $count } lines deleted ({ $roles })
    }
lines-deleted =
    { $count ->
        [one] { $count } line deleted
       *[other] { $count } lines deleted
    }
previous-match = Previous match in { $title }
next-match = Next match in { $title }
match-count = { $number } of { $count }
go-to-line-prompt = Go to line (1–{ $last }):
go-to-line-current = Current line { $line } of { $last }
go-to-line-target = Go to line { $line }
jj-diff-banner = These are temporary copies from jj diff --tool. Edits here are lost when the window closes.
output-changed-banner = { $name } changed on disk since the merge opened. Save & Exit replaces it.
copy-to-right = Copy this difference to the right
copy-to-left = Copy this difference to the left
initial-base = B
initial-left = L
initial-right = R
moved-to-base = Moved block: go to its other end, base line { $line }
moved-to-left = Moved block: go to its other end, left line { $line }
moved-to-right = Moved block: go to its other end, right line { $line }
diff-pane-left = Left: { $name }
diff-pane-right = Right: { $name }
diff-pane-unified = Unified: { $left } against { $right }

## Status bar

repo-git = git repo
repo-jj = jj repo
repo-none = no repo
mode-git = git merge
mode-jj = jj merge
mode-git-word = git merge by word
mode-jj-word = jj merge by word
encoding-utf8-bom = UTF-8 with BOM
inputs-differ = { $encoding } (inputs differ)
word-merged = { $count } word-merged
unresolved = { $count } unresolved
mixed-endings = mixed endings
compare-whitespace = whitespace significant
compare-line-endings = line endings ignored
compare-trailing = trailing whitespace ignored
compare-amount = whitespace amount ignored
compare-all = all whitespace ignored
case-ignored = case ignored
patterns-ignored = patterns ignored
blank-lines-ignored = blank lines ignored
diverge-whitespace-jj = whitespace compared unlike jj
diverge-case-git = case ignored, unlike git
diverge-case-jj = case ignored, unlike jj
diverge-patterns-git = patterns ignored, unlike git
diverge-patterns-jj = patterns ignored, unlike jj
diverge-same-change-git = same changes kept, unlike git
diverge-word-git = words merged, unlike git
diverge-deep-git = conflicts split inside, unlike git
diverge-deep-jj = conflicts split inside, unlike jj
diverge-syntax-git = syntax boundaries kept, unlike git
diverge-syntax-jj = syntax boundaries kept, unlike jj
copy-saved =
    { $count ->
        [0] Copy saved, resolved
        [one] Copy saved with { $count } conflict
       *[other] Copy saved with { $count } conflicts
    }

## Result pane's right-click menu

choice-none = None
choice-base = Base
choice-left = Left
choice-right = Right
choice-left-then-right = Left + Right
choice-right-then-left = Right + Left
action-sorted-union = Sorted Union
suggested = Suggested: { $action }
suggested-carry = Suggested: Carry { $from }'s Edit to Line { $line }
suggested-drop = Suggested: Drop Duplicate of Line { $line }
edit-cut = Cut
edit-copy = Copy
edit-paste = Paste
edit-select-all = Select All

## Error details. Each reads as the error's English `Display` does; `error_text.rs` names
## them. Paths, revisions, IDs and commands stay as they are. A system error reads by its
## kind, without the system's error number.

err-cannot-read = cannot read { $path }: { $detail }
err-cannot-read-plain = cannot read { $path }
err-cannot-write = cannot write { $path }: { $detail }
err-cannot-save = cannot save { $path }: { $detail }
err-cannot-find = cannot find { $path }: { $detail }
err-in = { $path }: { $detail }
err-at-rev = { $path } at { $rev }: { $detail }
err-not-recorded = { $path } is written, but the repository did not record the save: { $detail }
err-save-writing = a save is writing the merge
err-no-output = the merge has no output to write
err-save-running = a save is running
err-encode = { $encoding } cannot hold { $character } (U+{ $code }) on line { $line }
err-binary = binary file: a NUL byte within the first { $bytes } bytes
err-invalid-utf8 = invalid UTF-8 at byte { $offset }
err-invalid-utf16 = unpaired UTF-16 surrogate at byte { $offset }
err-odd-utf16 = UTF-16 file with an odd number of bytes
err-not-a-repository = { $path } is not inside a git or jj repository
err-not-utf8-path = { $path } is not a UTF-8 path
err-no-working-copy = { $path } has no working copy
err-outside-repository = { $path } is outside the repository
err-stale-file = { $path } on disk is not the working-copy commit's; run jj workspace update-stale and try again
err-stale-workspace = the working copy at { $path } is stale; run jj workspace update-stale and try again
err-unknown-revision = unknown revision { $rev }
err-ambiguous-revision = revision { $rev } is ambiguous
err-conflicted-at = { $path } is conflicted at { $rev }
err-no-index = { $name } has no index
err-immutable = commit { $rev } is immutable
err-not-conflicted-at = { $path } is not conflicted at { $rev }
err-bad-config = { $key } has the unknown value { $value }
err-backend = { $context }: { $detail }
err-concurrent-operations = the repository has { $count } concurrent operations; any jj command reconciles them
err-not-a-file-conflict = { $path } is not a conflict of files
err-unknown-operation = no operation { $op }: { $reason }
err-no-working-copy-at = the workspace has no working copy at operation { $op }
err-no-hunk = no hunk { $hunk }; there are { $count }
err-not-a-destination = { $rev } is not a mutable ancestor of the working copy in --into
err-repository-moved = the repository changed since operation { $op }; list the hunks again
err-conflict-changed = { $path } in { $change } changed since triad read it; nothing is written
err-sides-changed = { $path }: jj read back a conflict of { $after } sides for one of { $before }
err-jj-config-load = cannot read jj's config { $path }: { $detail }
err-jj-config-id = { $path }: not a jj config id
err-jj-config-value = { $key } in jj's config: { $detail }
err-jj-config-value-in = { $path }: { $key } in jj's config: { $detail }
err-jj-config-get = reading a jj config value
err-jj-config-set = setting a jj config value
err-jj-config-find = finding the repository's or workspace's jj config
err-auto-track = `snapshot.auto-track = { $text }` is not a fileset
err-git-ignores = reading git's ignore files
err-jj-workspace = loading the jj workspace
err-needs-jj = { $command } needs a jj repository
err-not-conflicted = { $path } is not conflicted
err-term-not-text = { $path }: { $term } is not text: { $detail }
err-term-not-a-file = { $path }: { $term } is a { $kind }; only text conflicts can be resolved
err-kind-symlink = symlink
err-kind-submodule = submodule
err-kind-directory = directory
err-deleted-everywhere = { $path } is deleted on every side; nothing to merge
err-resolved-in-tree = { $path } has no conflict markers left in the working tree, so it looks resolved; mark it resolved, or put its markers back to merge it again
err-tree-not-text = { $path }: the working-tree file is not text ({ $detail }), so it may hold edits a save would overwrite; put text with its conflict markers back, or resolve it by hand
err-temporary = cannot make a temporary file for the result: { $detail }
err-not-a-directory = { $path } is not a directory
err-mixed = diff takes two files or two directories: { $dir } is a directory, { $other } is not
err-git-in-jj = { $path } is a jj repository, where triad merges with jj's semantics only; resolve with `jj resolve`
err-git-in-jj-resolve = { $path } is jj's resolve directory, where triad merges with jj's semantics only; resolve with `jj resolve`
err-no-markers = no conflict markers
err-unclosed = line { $line }: the conflict is not closed
err-nested = line { $line }: a conflict starts inside another
err-out-of-order = line { $line }: the marker is out of order
err-no-separator = line { $line }: the conflict ends before its =======
err-bad-diff-line = line { $line }: a diff line starts with neither ' ', '-' nor '+'
err-marker-shape = line { $line }: a conflict needs at least two sides and one base fewer, got { $sides } sides and { $bases } bases
err-mixed-arity = line { $line }: a conflict of { $found } sides after conflicts of { $expected }
err-shape = a merge needs at least two sides and one base fewer than sides, got { $sides } sides and { $bases } bases
err-journal-header = the journal has no whole header line
err-journal-line = line { $index } of the journal does not parse: { $detail }
err-journal-version = the journal has format { $found }, not { $expected }
err-journal-inputs = the journal was written for other inputs
err-journal-alignment = the journal was written for another alignment of these inputs, such as one by another version of triad
err-journal-command = command { $index } of the journal does not apply: { $detail }
err-cannot-open-window = cannot open the window: { $detail }
err-io-not-found = no such file or directory
err-io-permission = permission denied
err-io-exists = the file already exists
err-io-is-directory = it is a directory
err-io-not-directory = not a directory
err-io-full = no space left on the device
err-io-read-only = the file system is read-only

## gpui-component's text: the search panel's Replace buttons and a field's Clear button.
## A field's right-click menu reads the Edit messages above.

component-replace = Replace
component-replace-all = Replace All
component-clear = Clear
