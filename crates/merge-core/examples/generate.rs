//! Write a generated triple to `DIR/{base,left,right}.txt`, for measuring the app on large
//! files (DESIGN.md §A6.7). The same line count and seed give the same files.
//!
//! Usage: `cargo run --release -p merge-core --features testing --example generate -- LINES DIR [SEED]`

use std::path::PathBuf;
use std::process::ExitCode;

use merge_core::testing::generate;

/// The benchmarks' seed, so the files match what `cargo bench` measures.
const DEFAULT_SEED: u64 = 0x5EED;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (lines, dir, seed) = match args.as_slice() {
        [lines, dir] => (lines, dir, Ok(DEFAULT_SEED)),
        [lines, dir, seed] => (lines, dir, seed.parse()),
        _ => return usage(),
    };
    let (Ok(lines), Ok(seed)) = (lines.parse::<usize>(), seed) else {
        return usage();
    };
    let dir = PathBuf::from(dir);
    let triple = generate(lines, seed);
    let written = std::fs::create_dir_all(&dir).and_then(|()| {
        [("base.txt", &triple.base), ("left.txt", &triple.left), ("right.txt", &triple.right)]
            .into_iter()
            .try_for_each(|(name, text)| std::fs::write(dir.join(name), text))
    });
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("generate: {}: {error}", dir.display());
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: generate LINES DIR [SEED]");
    ExitCode::FAILURE
}
