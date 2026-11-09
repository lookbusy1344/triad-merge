//! The cost of `resolve`'s repository steps against the size of the working copy, on jj and
//! git (`docs/PLAN.md` §D5.5): loading a conflict, which snapshots jj's working copy first;
//! listing the conflicts, which the queue does before each file; and recording a save, which
//! snapshots jj's working copy again or stages git's file. Each repository holds `files`
//! clean files in directories of `FILES_PER_DIR`, and one conflict in `a.txt`. The window
//! runs these steps on the UI thread; spec §9's targets apply to the steps around them.

use std::hint::black_box;
use std::path::Path;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use merge_session::ExitCode;
use merge_session::jj_config::ConfigEnv;
use merge_session::repository::RunRepository;
use merge_session::resolve::{self, Record};
use vcs::testing::Entry;

const SIZES: [usize; 2] = [10_000, 100_000];
const FILES_PER_DIR: usize = 100;
const SAMPLES: usize = 10;
const CONFLICT: &str = "a.txt";
const EDITED: &str = "edit.txt";
const BASE: &[u8] = b"a\n";
const LEFT: &[u8] = b"left\n";
const RIGHT: &[u8] = b"right\n";
const MERGED: &[u8] = b"merged\n";

/// The clean files: their paths and contents.
fn clean_files(files: usize) -> Vec<(String, Vec<u8>)> {
    (0..files)
        .map(|n| (format!("d{}/f{n}.txt", n / FILES_PER_DIR), format!("file {n}\n").into_bytes()))
        .collect()
}

fn borrowed(files: &[(String, Vec<u8>)]) -> Vec<(&str, &[u8])> {
    files.iter().map(|(path, bytes)| (path.as_str(), bytes.as_slice())).collect()
}

/// The run's repository at `root`, as `resolve` on `a.txt` opens it, with jj's defaults.
fn run(root: &Path) -> RunRepository {
    RunRepository::of_resolve(root, Some(Path::new(CONFLICT))).with_jj_config(ConfigEnv::default())
}

fn load(root: &Path) -> resolve::Loaded {
    resolve::load(&mut run(root), root, Path::new(CONFLICT), resolve::Snapshot::Take)
        .expect("the conflict loads")
}

fn record(root: &Path) -> Record {
    load(root).record.expect("the VCS holds the conflict")
}

/// A jj conflict in `a.txt` beside `files` clean files, checked out to disk.
fn jj_repo(files: usize) -> vcs_jj::testing::Fixture {
    let fixture = vcs_jj::testing::Fixture::new();
    let clean = clean_files(files);
    let with = |bytes: &'static [u8]| {
        let mut all = borrowed(&clean);
        all.push((CONFLICT, bytes));
        all
    };
    let base = fixture.commit_on(&[], "base", &with(BASE));
    let left = fixture.commit_on(&[&base], "left", &with(LEFT));
    let right = fixture.commit_on(&[&base], "right", &with(RIGHT));
    fixture.check_out(&[&left, &right]);
    fixture.check_out_to_disk();
    fixture
}

/// A git merge stopped on `a.txt`, with `files` clean files in the index and on disk.
fn git_repo(files: usize) -> vcs_git::testing::Fixture {
    let fixture = vcs_git::testing::Fixture::new();
    let clean = clean_files(files);
    let clean = borrowed(&clean);
    fixture.commit("one", &clean);
    for &(path, bytes) in &clean {
        fixture.write(path, bytes);
    }
    let terms = [BASE, LEFT, RIGHT].map(|bytes| Some(Entry::File(bytes)));
    fixture.stage_conflicts(&clean, &[(CONFLICT, terms)]);
    fixture
}

fn jj(c: &mut Criterion) {
    for files in SIZES {
        let fixture = jj_repo(files);
        let root = fixture.root().as_std_path();
        let mut group = c.benchmark_group("resolve-jj");
        group.sample_size(SAMPLES);
        group.bench_with_input(BenchmarkId::new("load", files), &files, |b, _| {
            b.iter(|| black_box(load(root)));
        });
        group.bench_with_input(BenchmarkId::new("list", files), &files, |b, _| {
            b.iter(|| {
                let mut run = RunRepository::find([root]).with_jj_config(ConfigEnv::default());
                black_box(resolve::conflicted(&mut run).expect("listed"))
            });
        });
        let record = record(root);
        let mut edit = 0_usize;
        group.bench_with_input(BenchmarkId::new("save", files), &files, |b, _| {
            b.iter_batched(
                || {
                    edit += 1;
                    fixture.write(EDITED, format!("edit {edit}\n").as_bytes());
                },
                |()| record.saved(ExitCode::Conflicts).expect("recorded"),
                BatchSize::PerIteration,
            );
        });
        group.finish();
    }
}

fn git(c: &mut Criterion) {
    for files in SIZES {
        let fixture = git_repo(files);
        let root = fixture.root().as_std_path();
        let index = fixture.root().join(".git/index");
        let stopped = std::fs::read(&index).expect("the index reads");
        let mut group = c.benchmark_group("resolve-git");
        group.sample_size(SAMPLES);
        group.bench_with_input(BenchmarkId::new("load", files), &files, |b, _| {
            b.iter(|| black_box(load(root)));
        });
        let record = record(root);
        group.bench_with_input(BenchmarkId::new("save", files), &files, |b, _| {
            b.iter_batched(
                || {
                    std::fs::write(&index, &stopped).expect("the index is put back");
                    fixture.write(CONFLICT, MERGED);
                },
                |()| record.saved(ExitCode::Resolved).expect("recorded"),
                BatchSize::PerIteration,
            );
        });
        group.finish();
    }
}

criterion_group!(benches, jj, git);
criterion_main!(benches);
