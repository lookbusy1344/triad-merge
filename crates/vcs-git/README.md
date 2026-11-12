# vcs-git

`Repository` for git, through gix. Pinned to the gix version jj-lib uses, so the build holds one copy.

Public API: `GitRepository::discover`, and `vcs::Repository` for it (DESIGN.md §A7.1). Conflicts come from the index's stages; a fully resolved save replaces them with one stage-0 entry, as `git add` does (spec §7.4). gix 0.90, pinned with jj-lib 0.47.1. `testing` feature: `testing::Fixture`, a repository built through gix in a temporary directory, or inside another directory with `new_in`.
