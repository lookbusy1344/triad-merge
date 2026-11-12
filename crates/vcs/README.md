# vcs

The `Repository` trait and the types shared by the git and jj backends. Types follow jj's model: n sides, n − 1 bases, an optional label per term.

Public API: `Repository`, which reads files at a revision, annotates lines (`annotate`, `describe`), lists and reads conflicts (`conflicts`, `read_conflict`, `recorded`), and records a save (`snapshot`, `record_save`); `RepoKind`, `RevSpec`, `CommitId`, `Parent`, `Term` and its `title`, `Conflict`, `ConflictTerm`, `ChangeInfo`, `Annotation`, `VcsError`. `discover` is `merge_session::native::discover`, above both backends (DESIGN.md §A7.1).
