//! Syntax boundaries' cost on valid Rust (`docs/PLAN.md` §B5.8, task 6): a word
//! conflict, or a conflict deep refinement splits at a shared `}`, every few functions, with the
//! setting off and on, under jj's semantics. `script/check-bench` holds the `bounded` runs to spec §9's load target.

use std::fmt::Write as _;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use merge_core::boundaries::Boundaries;
use merge_core::document::{Labels, MergeDocument, MergeOptions};
use merge_core::merge3::{HunkLevel, Inputs, Kind, Refine, Semantics};
use merge_core::text::Text;
use merge_syntax::boundaries::SyntaxBoundaries;
use merge_syntax::language::Language;

const SIZES: [usize; 2] = [100_000, 1_000_000];
const SAMPLES: usize = 10;
/// Each function takes four lines with the blank line after it, so it starts at the top level.
const FUNCTION_LINES: usize = 4;
/// One conflict every this many functions: about one every 20 lines.
const CONFLICT_EVERY: usize = 5;

struct Triple {
    base: String,
    left: String,
    right: String,
}

/// Writes the base, left and right functions of a conflict, for the function's number.
type Conflict = fn(&mut Triple, usize);

/// `lines` lines of functions; every [`CONFLICT_EVERY`]th one is changed on both sides by
/// `conflict`.
fn rust(lines: usize, conflict: Conflict) -> Triple {
    let mut triple = Triple { base: String::new(), left: String::new(), right: String::new() };
    for n in 0..lines / FUNCTION_LINES {
        if n.is_multiple_of(CONFLICT_EVERY) {
            conflict(&mut triple, n);
        } else {
            for text in [&mut triple.base, &mut triple.left, &mut triple.right] {
                write!(text, "fn item_{n}(value: u64) -> u64 {{\n    value * {n}\n}}\n\n")
                    .expect("a String takes writes");
            }
        }
    }
    triple
}

/// Left renames the function, right changes its operator: a word merge that cuts no token.
fn word(triple: &mut Triple, n: usize) {
    let function = |name: &str, op: &str| {
        format!("fn {name}_{n}(value: u64) -> u64 {{\n    value {op} {n}\n}}\n\n")
    };
    triple.base.push_str(&function("item", "*"));
    triple.left.push_str(&function("left", "*"));
    triple.right.push_str(&function("item", "+"));
}

/// Each side wraps a call in its own block; the blocks' closing `}` lines match.
fn brace(triple: &mut Triple, n: usize) {
    write!(triple.base, "fn item_{n}() {{\n    a();\n}}\n\n").expect("a String takes writes");
    write!(triple.left, "fn item_{n}() {{\n    if p {{\n        b();\n    }}\n    c();\n}}\n\n")
        .expect("a String takes writes");
    write!(
        triple.right,
        "fn item_{n}() {{\n    while q {{\n        d();\n    }}\n    e();\n}}\n\n"
    )
    .expect("a String takes writes");
}

fn load(triple: &Triple, options: MergeOptions, boundaries: &dyn Boundaries) -> MergeDocument {
    let text = |s: &str| Text::decode(s.as_bytes()).expect("generated text decodes");
    let inputs = Inputs::three_way(text(&triple.base), text(&triple.left), text(&triple.right));
    MergeDocument::new_bounded(inputs, options, Labels::none(), Some(boundaries))
        .expect("no labels")
}

fn benches(c: &mut Criterion) {
    let rust_boundaries = SyntaxBoundaries::new(Language::Rust);
    let by_word = MergeOptions {
        semantics: Semantics::Jj,
        hunk_level: HunkLevel::Word,
        ..MergeOptions::default()
    };
    // jj's semantics: git's histogram costs 1.5 s on these 100k lines whatever the refinement
    // (`docs/PLAN.md` §B6), which would hide the boundaries' cost.
    let deep =
        MergeOptions { semantics: Semantics::Jj, refine: Refine::Deep, ..MergeOptions::default() };
    let cases: [(&str, Conflict, MergeOptions); 2] =
        [("word-small", word, by_word), ("deep", brace, deep)];
    let mut group = c.benchmark_group("merge");
    group.sample_size(SAMPLES);
    for lines in SIZES {
        for (name, conflict, options) in cases {
            let triple = rust(lines, conflict);
            let conflicts = |syntax_boundaries| {
                let options = MergeOptions { syntax_boundaries, ..options };
                load(&triple, options, &rust_boundaries)
                    .regions()
                    .iter()
                    .filter(|region| region.kind == Kind::Conflict)
                    .count()
            };
            let functions = lines / FUNCTION_LINES / CONFLICT_EVERY;
            // The word merges cut no token; the deep splits cut the sides' blocks.
            let expected = match name {
                "deep" => (2 * functions, functions),
                _ => (0, 0),
            };
            assert_eq!((conflicts(false), conflicts(true)), expected, "{name}/{lines}");
            for (setting, syntax_boundaries) in [("", false), ("-bounded", true)] {
                let options = MergeOptions { syntax_boundaries, ..options };
                let id = BenchmarkId::new(format!("load-{name}-rust{setting}"), lines);
                group.bench_with_input(id, &triple, |b, t| {
                    b.iter(|| load(black_box(t), options, &rust_boundaries));
                });
            }
        }
    }
    group.finish();
}

criterion_group!(merge, benches);
criterion_main!(merge);
