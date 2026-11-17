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
menu-toggle-left = Toggle Left
menu-toggle-base = Toggle Base
menu-toggle-right = Toggle Right
menu-clear-choice = Clear Choice
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
