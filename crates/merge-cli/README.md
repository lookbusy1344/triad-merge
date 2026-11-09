# merge-cli

Argument parsing, headless merge, `inspect` and `diff --inspect`, `install` and logging, over `merge-session`. A library with no GUI dependency.

Public API: `run` (the whole command line), `args::parse` into an `Invocation`, `Invocation::into_window` for the forms that open a window, `ExitCode` and `WindowForm` (from `merge-session`), `headless`, `inspect::{inspect, diff_inspect}`, whose JSON `docs/json-schema.md` defines, `install`, `logging::init`, and `forward::forward` for the Windows console binary.
