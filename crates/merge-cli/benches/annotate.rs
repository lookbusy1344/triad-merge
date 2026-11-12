//! The cost of `Repository::annotate` against the length of history, on git and jj
//! (`docs/DESIGN.md` §C5.5). Each history holds one 100-line file; each commit after
//! the first changes one line, in turn, and line 0 only ever comes from the first commit, so
//! each annotation walks the whole history. Spec §9 sets no target; the results are recorded
//! in the plan.
#![expect(
    clippy::expect_used,
    reason = "benchmark: a failed setup stops the run, named by its message"
)]

use std::fmt::Write as _;
use std::hint::black_box;

use camino::Utf8Path;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use vcs::Repository;

const PATH: &str = "a.txt";
const LINES: usize = 100;
const COMMITS: [usize; 2] = [1_000, 10_000];
const SAMPLES: usize = 10;

/// The file at commit `n`: line `1 + (n - 1) % 99` says which commit wrote it last.
fn file(n: usize, lines: &mut [String]) -> Vec<u8> {
    if n > 0 {
        let changed = 1 + (n - 1) % (LINES - 1);
        lines[changed] = format!("line {changed} from commit {n}");
    }
    lines
        .iter()
        .fold(String::new(), |mut text, line| {
            writeln!(text, "{line}").expect("a String takes any write");
            text
        })
        .into_bytes()
}

fn first_lines() -> Vec<String> {
    (0..LINES).map(|line| format!("line {line}")).collect()
}

/// Make `commits` commits through `commit`, and return the last one's ID.
fn history(commits: usize, mut commit: impl FnMut(&str, &[u8]) -> String) -> String {
    let mut lines = first_lines();
    (0..commits)
        .map(|n| commit(&format!("commit {n}"), &file(n, &mut lines)))
        .last()
        .expect("at least one commit")
}

fn bench(c: &mut Criterion, group: &str, repo: &dyn Repository, tip: String, commits: usize) {
    let tip = vcs::CommitId::new(tip);
    let mut group = c.benchmark_group(group);
    group.sample_size(SAMPLES);
    group.bench_with_input(BenchmarkId::from_parameter(commits), &commits, |b, _| {
        b.iter(|| {
            let annotation = repo
                .annotate(Utf8Path::new(PATH), &tip)
                .expect("the annotation runs")
                .expect("the file exists");
            black_box(annotation)
        });
    });
    group.finish();
}

fn annotate(c: &mut Criterion) {
    for commits in COMMITS {
        let git = vcs_git::testing::Fixture::new();
        let tip =
            history(commits, |message, bytes| git.commit(message, &[(PATH, bytes)]).to_string());
        bench(c, "annotate-git", &git.open(), tip, commits);

        let jj = vcs_jj::testing::Fixture::new();
        let tip = history(commits, |message, bytes| jj.commit(message, &[(PATH, bytes)]).commit);
        bench(c, "annotate-jj", &jj.open(), tip, commits);
    }
}

criterion_group!(benches, annotate);
criterion_main!(benches);
