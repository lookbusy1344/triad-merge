# vcs-jj

`Repository` for jj, through jj-lib. It writes only working-copy snapshots: one before `resolve` reads a conflict, and one per save, `save <path> in triad`, which `jj undo` reverts (spec §7.4).

Public API: `JjRepository::discover`, `JjError`, and `vcs::Repository` for it (PLAN.md §A7.1). jj-lib 0.47.1, pinned with gix 0.90. `testing` feature: `testing::Fixture`, a workspace built through jj-lib in a temporary directory. `Fixture::new` lays it out as `jj git init` does by default (`testing::DEFAULT_STORE`); `Fixture::colocated` and `Fixture::internal` name a layout.
