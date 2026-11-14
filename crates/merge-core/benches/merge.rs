//! Spec §9 targets on triples from the seeded generator, in git and jj mode, and on word
//! conflicts at jj's word level (`docs/DESIGN.md` §B6). `script/check-bench` compares the results against the
//! targets.
#![expect(
    clippy::expect_used,
    reason = "benchmark: a failed setup stops the run, named by its message"
)]

use merge_core::index::RegionIx;
use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use merge_core::document::{Command, EditBatch, Labels, MergeDocument, MergeOptions, TextPatch};
use merge_core::merge3::{HunkLevel, Inputs, LEFT, RIGHT, Semantics, Source, align};
use merge_core::testing::{Triple, generate, generate_word_conflicts};
use merge_core::text::Text;

const SEED: u64 = 0x5EED;
const SIZES: [usize; 3] = [10_000, 100_000, 1_000_000];
/// Keystrokes typed into one fresh document before the next is taken.
const KEYSTROKES: u64 = 1_000;
const SAMPLES: usize = 10;
/// Word-level benches: one conflict every this many lines (many small conflicts), or on
/// every line (one conflict as large as the file).
const WORD_SPACINGS: [(&str, usize); 2] = [("small", 20), ("large", 1)];

fn decode(triple: &Triple) -> Inputs {
    let text = |s: &str| Text::decode(s.as_bytes()).expect("generated text decodes");
    Inputs::three_way(text(&triple.base), text(&triple.left), text(&triple.right))
}

fn load(triple: &Triple, options: MergeOptions) -> MergeDocument {
    MergeDocument::new(decode(triple), options, Labels::three_way("left", "base", "right"))
        .expect("three labels fit")
}

fn jj() -> MergeOptions {
    MergeOptions { semantics: Semantics::Jj, ..MergeOptions::default() }
}

fn jj_by_word() -> MergeOptions {
    MergeOptions { hunk_level: HunkLevel::Word, ..jj() }
}

fn benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge");
    group.sample_size(SAMPLES);
    for lines in SIZES {
        let triple = generate(lines, SEED);
        let doc = load(&triple, MergeOptions::default());
        let middle = doc.regions().len() / 2;

        group.bench_with_input(BenchmarkId::new("load", lines), &triple, |b, t| {
            b.iter(|| load(black_box(t), MergeOptions::default()));
        });
        group.bench_with_input(BenchmarkId::new("load-jj", lines), &triple, |b, t| {
            b.iter(|| load(black_box(t), jj()));
        });
        for (size, spacing) in WORD_SPACINGS {
            let triple = generate_word_conflicts(lines, spacing);
            let id = BenchmarkId::new(format!("load-word-{size}"), lines);
            group.bench_with_input(id, &triple, |b, t| {
                b.iter(|| load(black_box(t), jj_by_word()));
            });
        }
        group.bench_with_input(BenchmarkId::new("decode", lines), &triple, |b, t| {
            b.iter(|| decode(black_box(t)));
        });
        let inputs = decode(&triple);
        for (name, options) in [("align", MergeOptions::default()), ("align-jj", jj())] {
            group.bench_with_input(BenchmarkId::new(name, lines), &inputs, |b, i| {
                b.iter(|| align(black_box(i), &options.alignment()));
            });
        }
        group.bench_with_input(BenchmarkId::new("output", lines), &doc, |b, d| {
            b.iter(|| d.output());
        });
        group.bench_with_input(BenchmarkId::new("toggle", lines), &doc, |b, d| {
            let mut doc = d.clone();
            let sides = [Source::Side(LEFT), Source::Side(RIGHT)];
            let mut turn = 0;
            b.iter(|| {
                turn ^= 1;
                doc.apply(Command::Toggle { region: RegionIx::new(middle), source: sides[turn] })
            });
        });
        // Each sample types runs of KEYSTROKES characters at the caret into a fresh copy.
        group.bench_with_input(BenchmarkId::new("keystroke", lines), &doc, |b, d| {
            let start = d.region_bytes(RegionIx::new(middle)).start;
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                let mut left = iters;
                while left > 0 {
                    let run = left.min(KEYSTROKES);
                    let mut doc = d.clone();
                    let timer = Instant::now();
                    for at in start..start + usize::try_from(run).expect("run fits usize") {
                        let key = TextPatch { range: at..at, text: "x".to_owned() };
                        black_box(doc.apply(Command::Edit(EditBatch(vec![key]))))
                            .expect("keystroke fits");
                    }
                    total += timer.elapsed();
                    left -= run;
                }
                total
            });
        });
    }
    group.finish();
}

criterion_group!(merge, benches);
criterion_main!(merge);
