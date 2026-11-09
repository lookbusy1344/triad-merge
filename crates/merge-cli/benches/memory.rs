//! Peak heap of load, merge and save at spec §9's sizes, on triples from the seeded
//! generator (PLAN.md §A6.7), in git and then jj mode (`docs/PLAN.md` §B6). Writes `target/tmp/memory.json`, which `script/check-bench` compares
//! with spec §9's memory targets.
//!
//! The allocator's peak cannot be reset, so the sizes run smallest first, jj mode after git
//! mode at each size, and each phase must raise the peak; a phase that does not is reported as an error. A child process writes each
//! triple, so the generator's own peak stays out of the measurement.

use std::alloc::System;
use std::path::Path;
use std::process::Command;

use cap::Cap;
use merge_core::document::{Labels, MergeOptions, OutputEnding};
use merge_core::merge3::Semantics;
use merge_core::testing::generate;
use merge_session::invocation::MergeFiles;

#[global_allocator]
static ALLOCATOR: Cap<System> = Cap::new(System, usize::MAX);

const SEED: u64 = 0x5EED;
const SIZES: [usize; 2] = [100_000, 1_000_000];
const BYTES_PER_MB: usize = 1 << 20;
const RESULTS: &str = concat!(env!("CARGO_TARGET_TMPDIR"), "/memory.json");
/// `memory generate LINES DIR` writes one triple into DIR and exits.
const GENERATE: &str = "generate";

/// Each mode's phases carry its suffix in the results: `load`, `load-jj`.
const MODES: [(Semantics, &str); 2] = [(Semantics::Git, ""), (Semantics::Jj, "-jj")];

/// The heap a phase holds at its peak, above what was live before it.
struct Peak {
    phase: &'static str,
    mode: &'static str,
    lines: usize,
    bytes: usize,
}

impl Peak {
    /// The result's name, such as `session-jj/100000`.
    fn key(&self) -> String {
        format!("{}{}/{}", self.phase, self.mode, self.lines)
    }
}

fn triple_files(dir: &Path) -> MergeFiles {
    MergeFiles {
        base: dir.join("base.txt"),
        left: dir.join("left.txt"),
        right: dir.join("right.txt"),
    }
}

fn write_triple(lines: usize, dir: &Path) {
    let triple = generate(lines, SEED);
    let files = triple_files(dir);
    for (path, text) in
        [(&files.base, &triple.base), (&files.left, &triple.left), (&files.right, &triple.right)]
    {
        std::fs::write(path, text).expect("the temporary directory takes the inputs");
    }
}

/// Write the triple in a child process: this process's peak then starts below load's.
fn write_triple_apart(lines: usize, dir: &Path) -> MergeFiles {
    let program = std::env::current_exe().expect("the bench knows its own path");
    let status = Command::new(program)
        .arg(GENERATE)
        .arg(lines.to_string())
        .arg(dir)
        .status()
        .expect("the bench starts a copy of itself");
    assert!(status.success(), "writing the {lines}-line triple failed: {status}");
    triple_files(dir)
}

/// The peak since `live` bytes were held, once `phase` has raised it past `previous_peak`.
fn peak_since(live: usize, previous_peak: usize, phase: &str) -> usize {
    let peak = ALLOCATOR.max_allocated();
    assert!(
        peak > previous_peak,
        "{phase} did not raise the peak of {previous_peak} bytes, so its own is unknown"
    );
    peak - live
}

fn measure(lines: usize, (semantics, mode): (Semantics, &'static str)) -> [Peak; 2] {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let files = write_triple_apart(lines, dir.path());
    let live = ALLOCATOR.allocated();
    let before = ALLOCATOR.max_allocated();
    let options = MergeOptions { semantics, ..MergeOptions::default() };
    let doc =
        merge_session::load(&files, options, Labels::none(), None).expect("generated inputs load");
    let load = peak_since(live, before, "load");
    merge_session::EncodedOutput::new(&doc, OutputEnding::Keep)
        .write(&dir.path().join("out.txt"))
        .expect("the output writes");
    // The peak of the whole session: save's own, or load's when save stays below it.
    let session = ALLOCATOR.max_allocated() - live;
    [
        Peak { phase: "load", mode, lines, bytes: load },
        Peak { phase: "session", mode, lines, bytes: session },
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let [_, mode, lines, dir] = args.as_slice()
        && mode == GENERATE
    {
        let lines = lines.parse().expect("the line count is a number");
        write_triple(lines, Path::new(dir));
        return;
    }
    let peaks: Vec<Peak> = SIZES
        .into_iter()
        .flat_map(|lines| MODES.into_iter().flat_map(move |mode| measure(lines, mode)))
        .collect();
    for peak in &peaks {
        println!("memory/{}: {} MB", peak.key(), peak.bytes / BYTES_PER_MB);
    }
    let json: serde_json::Map<_, _> = peaks.iter().map(|p| (p.key(), p.bytes.into())).collect();
    let json = serde_json::to_string_pretty(&json).expect("a map of numbers serialises");
    std::fs::write(RESULTS, json).expect("the target directory takes the results");
}
