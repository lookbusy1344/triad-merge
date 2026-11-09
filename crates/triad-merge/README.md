# triad-merge

The binaries: `triad-merge`, and `triad-merge-gui` for Windows (spec §6.1). `triad-merge` routes headless commands to `merge-cli` and GUI runs to `merge-app`, so `merge-cli` never links GPUI. `triad-merge-gui` is the Windows GUI-subsystem entry point; it takes the same command line and runs window forms itself (PLAN.md §A4.13).
