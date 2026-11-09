# merge-view

View model: aligned rows for each pane, scroll mapping between panes, navigation, highlight spans, status and line origins. Pure Rust, no GPUI.

Public API: `rows::RowMap`, `scroll`, `nav`, `highlight`, `marks`, `status`, `tabs`, `diff` for the two-way window, and `origin::Origins`, which gives each pane row and result line the commit that last changed it (spec §7.3), with `carry` and `commit_runs`. It depends on `vcs` for the origin types only.
