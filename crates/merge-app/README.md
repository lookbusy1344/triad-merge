# merge-app

GPUI shell: windows, panes, toolbar, menus and shortcuts. A thin renderer over `merge-view`, reading and writing files through `merge-session`.

Public API: `run`, `init` (key bindings), `open` and `MergeWindow` (the merge window, PLAN.md §A6.2), `diff_window` (the two-way diff window, PLAN.md §A6.5), `open_dialog`, `about`, `Settings`, `measure` (frame measurement under `TRIAD_MEASURE`, PLAN.md §A6.7).
