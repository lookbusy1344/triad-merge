# JSON output of `inspect` and `diff --inspect`, and merge commands

`triad-merge inspect BASE LEFT RIGHT --json`, `triad-merge diff --inspect LEFT RIGHT --json` and `triad-merge diff --inspect LEFT_DIR RIGHT_DIR --json` print one JSON object on stdout. `triad-merge --headless … --commands FILE` reads one merge command per line. This file defines both.

## Versioning

Every object carries `"schema": 1`. A change that removes or renames a field, or changes a field's meaning, raises the number. New fields may appear without a new number; readers ignore fields they don't know.

## Common fields

| Field | Type | Meaning |
|---|---|---|
| `schema` | integer | Schema version, `1`. |
| `command` | string | `"inspect"`, `"diff"`, `"diff-dirs"` or `"diff-changes"`. |
| `encodings` | object | Each input's encoding, keyed by input name. |
| `warnings` | array of strings | Warning codes, in the order below. Empty when there are none. |
| `regions` | array of objects | The regions in file order. Together they cover every line of every input. |

`"diff-dirs"` and `"diff-changes"` carry `schema` and `command` alone of these; their section lists their fields.

Encodings: `"utf-8"`, `"utf-8-bom"`, `"utf-16le"`, `"utf-16be"`. The UTF-16 encodings always have a byte order mark.

Warning codes:

- `"encoding-mismatch"`: the inputs do not all have the same encoding. UTF-8 with and without a byte order mark count as different.

Line ranges are two-element arrays `[start, end]` of 0-based line numbers, half-open: `[1, 2]` is the second line, and `[3, 3]` is the empty range before the fourth line. A line ends at `\n` and includes it (spec §4.1).

## `inspect`

The regions follow the merge semantics, same-change rule and hunk level chosen by `--merge-semantics`, `--same-change`, `--hunk-level`, the settings file and the working directory's repository (spec §6.1).

| Field | Type | Meaning |
|---|---|---|
| `output_encoding` | string | The encoding the output would use: base's, or left's when base is empty. |
| `line_endings` | string | The line endings the output would hold under the `output-line-ending` setting: `"lf"`, `"crlf"`, `"mixed"`, or `"none"` without a line break. |
| `conflicts` | integer | The number of `conflict` regions. |

`encodings` has the keys `base`, `left` and `right`. Each region has `kind` and the ranges `base`, `left` and `right`. A `word-merged` region also has `text`, its merged text. Kinds, from spec §4.3:

| `kind` | Meaning |
|---|---|
| `"unchanged"` | Neither side changed base's lines. |
| `"left-only"` | Only left changed them. |
| `"right-only"` | Only right changed them. |
| `"both-same"` | Both changed them to the same text. |
| `"word-merged"` | Both changed them to different text, and every word merges (`--hunk-level word`). |
| `"conflict"` | Both changed them to different text. |

The regions are those of the default merge, with conflict refinement on, under the given `--diff-algorithm` and `--ignore-whitespace`.

Exit code: 0 with no conflicts, 1 with conflicts, 3 on error.

```json
{
  "schema": 1,
  "command": "inspect",
  "encodings": { "base": "utf-8", "left": "utf-8", "right": "utf-8" },
  "output_encoding": "utf-8",
  "line_endings": "lf",
  "warnings": [],
  "conflicts": 1,
  "regions": [
    { "kind": "unchanged", "base": [0, 1], "left": [0, 1], "right": [0, 1] },
    { "kind": "conflict", "base": [1, 2], "left": [1, 2], "right": [1, 2] },
    { "kind": "unchanged", "base": [2, 3], "left": [2, 3], "right": [2, 3] }
  ]
}
```

## `diff --inspect`

| Field | Type | Meaning |
|---|---|---|
| `changes` | integer | The number of `changed` regions. |

`encodings` has the keys `left` and `right`. Each region has `kind`, `"unchanged"`, `"changed"` or `"ignored"`, and the ranges `left` and `right`. Either range of a `changed` or `ignored` region may be empty, for a pure insertion or deletion. An `ignored` region differs only in blank lines, under `diff-ignore-blank-lines` (spec §4.2), and is not counted in `changes`. Identical inputs give one `unchanged` region.

`moves` lists the moved blocks (spec §4.6), ordered by `from`. Each has `from`, the index of the `changed` region whose left lines moved, `to`, the index of the `changed` region that holds them in right, `left`, region `from`'s left lines, and `right`, the block's lines in right, inside region `to`'s. One region can be the `from` of one move and the `to` of others.

Exit code, as `diff(1)`: 0 if the inputs are equal under the whitespace mode, `diff-ignore-line-endings`, `ignore-case`, `ignore-pattern` and `diff-ignore-blank-lines` (spec §4.2), 1 if they differ, 3 on error.

```json
{
  "schema": 1,
  "command": "diff",
  "encodings": { "left": "utf-8", "right": "utf-8" },
  "warnings": [],
  "changes": 1,
  "regions": [
    { "kind": "unchanged", "left": [0, 1], "right": [0, 1] },
    { "kind": "changed", "left": [1, 2], "right": [1, 2] },
    { "kind": "unchanged", "left": [2, 3], "right": [2, 3] }
  ],
  "moves": []
}
```

## `diff --inspect` on two directories, or with no file

`"diff-dirs"` lists two directories; `"diff-changes"`, with no file, lists the working copy's changes against a revision (spec §6.1). Both take these fields.

| Field | Type | Meaning |
|---|---|---|
| `changes` | integer | The number of entries in `files`. |
| `files` | array of objects | The files that differ, by path, component by component (spec §6.1). |

Each file has `path`, relative to both directories, or to the repository's root, with `/` between components on every platform; `change`, `"added"` (right only, or new since the revision), `"deleted"` (left only, or gone from disk) or `"modified"`; and `content`, `"text"`, `"binary"`, `"undecodable"` or `"special"` (spec §6.1). A path component that is not UTF-8 shows with U+FFFD in place of its bad bytes.

Exit code: 0 if no file differs, 1 if one does, 3 on error.

```json
{
  "schema": 1,
  "command": "diff-dirs",
  "changes": 2,
  "files": [
    { "path": "img.png", "change": "added", "content": "binary" },
    { "path": "src/a.rs", "change": "modified", "content": "text" }
  ]
}
```

## Merge commands

`--headless --commands FILE` (spec §6.1) reads FILE as JSON Lines: one command per line. The recovery journal stores its commands in the same encoding. The format follows the schema number above.

| Command | JSON | Effect (spec §4.4) |
|---|---|---|
| Pick | `{"Pick":{"region":1,"sources":[{"Side":1},{"Side":0}]}}` | The region takes these sources' texts in this order, each at most once. |
| Toggle | `{"Toggle":{"region":1,"source":{"Base":0}}}` | Adds the source to the end of the region's pick, or removes it. Removing a conflict's last source leaves it unresolved. |
| Clear | `{"Clear":{"region":1}}` | The region contributes no text. |
| PickAll | `{"PickAll":{"scope":"Unresolved","sources":[{"Side":0}]}}` | Every region the scope names takes these sources, as one command. `"Unresolved"`: each unresolved conflict. `"All"`: every region, so one source gives that input's text. Regions that already hold the pick stay. |
| Edit | `{"Edit":[{"range":{"start":2,"end":3},"text":"B"}]}` | Replaces byte ranges of the result text. |
| Undo | `"Undo"` | Undoes the last command. |
| Redo | `"Redo"` | Redoes the last undone command. |

- `region` is a 0-based index into `inspect`'s `regions` for the same inputs and options. Picks, clears and edits can merge or split regions, so each command sees the regions the commands before it left.
- A source is `{"Side":0}` (left), `{"Side":1}` (right) or `{"Base":0}` (base).
- An edit's patches are sorted and do not overlap. Each `range` is a half-open byte range `[start, end)` of the result text before any of the batch's patches, in UTF-8, on character boundaries. The result text is the result pane's: an unresolved conflict shows each side's text in order, without markers. Inserted line breaks take the result's line ending (spec §4.1).

## Text output

Without `--json`, the same facts print one region per line, then a count. A warning prints first, as `warning: <code>`. Ranges print as `start..end`.

```
unchanged base 0..1 left 0..1 right 0..1
conflict base 1..2 left 1..2 right 1..2
unchanged base 2..3 left 2..3 right 2..3
conflicts: 1
```

```
unchanged left 0..1 right 0..1
changed left 1..2 right 1..2
unchanged left 2..3 right 2..3
changes: 1
```

A move prints after the regions:

```
changed left 0..2 right 0..0
unchanged left 2..5 right 0..3
changed left 5..5 right 3..5
move from 0 left 0..2 to 2 right 3..5
changes: 2
```

Two directories print one file per line, change, content and path, then a count:

```
added binary img.png
modified text src/a.rs
files: 2
```

The text form is for people. Scripts use `--json`.
