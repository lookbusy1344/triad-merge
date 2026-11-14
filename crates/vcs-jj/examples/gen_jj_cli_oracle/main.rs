//! Writes `fixtures/oracle/jj-cli-aliases.json`, `jj-cli-absorb.json` and
//! `jj-cli-resolve.json` from jj's own CLI (`DESIGN.md` §I5). Dev-time only; `script/gen-jj-cli-oracle` runs
//! it. The `jj` on `PATH` must be the release whose jj-lib `Cargo.lock` pins.
//!
//! The aliases are jj's defaults for `revset-aliases`. Each absorb case is a seeded random
//! history of commits on the root, `base` first, and a working copy on it; the case records
//! each commit's tree after `jj absorb`, and the commits it left conflicted. Three kinds of
//! case come from three seeded streams: linear stacks with jj's default `--into`, histories
//! that merge two branches, in a commit or in the working copy, and linear stacks with a
//! narrower `--into`. The resolve cases are in `resolve.rs`.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "dev-time generator: a failure stops it, named by its message"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha20Rng;
use serde::Serialize;
use vcs_jj::testing::locked_jj_lib_version;

mod resolve;

const SEED: u64 = 500_135_715_172;
/// The seed of the merge cases' stream.
const MERGE_SEED: u64 = 500_135_715_173;
/// The seed of the `--into` cases' stream.
const INTO_SEED: u64 = 500_135_715_174;
/// The inputs generated of each kind; the corpus keeps the first cases of each outcome.
const POOL: usize = 200;
/// Linear cases where `jj absorb` leaves every commit resolved: triad must match them exactly.
const CLEAN_CASES: usize = 48;
/// Linear cases where `jj absorb` writes a conflict, which triad keeps out.
const CONFLICT_CASES: usize = 16;
/// Merge cases, resolved and conflicted, as above.
const MERGE_CLEAN_CASES: usize = 16;
const MERGE_CONFLICT_CASES: usize = 4;
/// `--into` cases, resolved and conflicted, as above.
const INTO_CLEAN_CASES: usize = 16;
const INTO_CONFLICT_CASES: usize = 4;
/// The share of merge cases whose working copy is the merge, with no merge commit.
const P_MERGE_WORKING_COPY: f64 = 0.5;
/// The share of `--into` cases that name two commits.
const P_INTO_TWO: f64 = 0.3;
/// The commits a case stacks on `base`, at most.
const MAX_COMMITS: usize = 3;
/// The lines of a file in `base`.
const BASE_LINES: std::ops::RangeInclusive<usize> = 3..=8;
/// The edits one commit makes to a file it touches.
const EDITS: std::ops::RangeInclusive<usize> = 1..=3;
/// A line many files share, which makes the line diff ambiguous.
const SHARED_LINE: &str = "}";
const P_SHARED_LINE: f64 = 0.1;
const P_SECOND_FILE: f64 = 0.5;
const P_ADD_SECOND_FILE: f64 = 0.2;
const P_TOUCH: f64 = 0.7;
const P_WC_TOUCH: f64 = 0.8;
const P_WC_DELETE: f64 = 0.05;
const P_WC_NEW_FILE: f64 = 0.1;
const P_NO_FINAL_NEWLINE: f64 = 0.1;
const P_TOGGLE_FINAL_NEWLINE: f64 = 0.05;
const FILES: [&str; 3] = ["a.txt", "b.txt", "c.txt"];
/// The revset of the working copy, which `-` steps back from.
const WORKING_COPY: &str = "@";
const CONFIG: &str = r#"
user.name = "triad"
user.email = "triad@example.com"
ui.paginate = "never"
"#;

/// A file as lines, with or without a newline after the last.
#[derive(Clone)]
struct File {
    lines: Vec<String>,
    final_newline: bool,
}

impl File {
    fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        if self.final_newline && !self.lines.is_empty() {
            text.push('\n');
        }
        text
    }
}

type Files = BTreeMap<&'static str, File>;
type Tree = BTreeMap<String, String>;

fn tree(files: &Files) -> Tree {
    files.iter().map(|(path, file)| ((*path).to_owned(), file.text())).collect()
}

struct Generator {
    rng: ChaCha20Rng,
    lines: usize,
}

/// One case's input: each commit's tree and parents, by index, `base` first on the root; the
/// working copy's tree and parents; and `--into` when narrower than jj's default.
struct Input {
    commits: Vec<Files>,
    parents: Vec<Vec<usize>>,
    working_copy: Files,
    working_copy_parents: Vec<usize>,
    into: Option<String>,
}

impl Input {
    /// A stack: each commit on the one before, and the working copy on the last.
    fn linear((commits, working_copy): (Vec<Files>, Files)) -> Self {
        let parents = (0..commits.len()).map(|index| index.checked_sub(1).into_iter().collect());
        Self {
            parents: parents.collect(),
            working_copy_parents: vec![commits.len() - 1],
            commits,
            working_copy,
            into: None,
        }
    }

    /// The working copy holds the merge of its parents' trees: no hunk to absorb.
    fn is_empty(&self) -> bool {
        match self.working_copy_parents.as_slice() {
            [parent] => tree(&self.commits[*parent]) == tree(&self.working_copy),
            _ => false,
        }
    }
}

impl Generator {
    fn line(&mut self, tag: &str) -> String {
        if self.rng.random_bool(P_SHARED_LINE) {
            return SHARED_LINE.to_owned();
        }
        self.lines += 1;
        format!("{tag}{}", self.lines)
    }

    fn file(&mut self, tag: &str) -> File {
        let count = self.rng.random_range(BASE_LINES);
        File {
            lines: (0..count).map(|_| self.line(tag)).collect(),
            final_newline: !self.rng.random_bool(P_NO_FINAL_NEWLINE),
        }
    }

    /// Replace, insert or delete lines of `file`.
    fn edit(&mut self, file: &mut File, tag: &str) {
        for _ in 0..self.rng.random_range(EDITS) {
            let len = file.lines.len();
            match self.rng.random_range(0..3) {
                0 if len > 0 => {
                    let at = self.rng.random_range(0..len);
                    file.lines[at] = self.line(tag);
                }
                2 if len > 1 => {
                    let at = self.rng.random_range(0..len);
                    file.lines.remove(at);
                }
                _ => {
                    let at = self.rng.random_range(0..=len);
                    let line = self.line(tag);
                    file.lines.insert(at, line);
                }
            }
        }
        if self.rng.random_bool(P_TOGGLE_FINAL_NEWLINE) {
            file.final_newline = !file.final_newline;
        }
    }

    /// The trees of `base` and each commit on it, and the working copy's.
    fn case(&mut self) -> (Vec<Files>, Files) {
        let mut files = Files::new();
        files.insert(FILES[0], self.file("base-"));
        if self.rng.random_bool(P_SECOND_FILE) {
            files.insert(FILES[1], self.file("base-"));
        }
        let mut commits = vec![files.clone()];
        for index in 1..=self.rng.random_range(1..=MAX_COMMITS) {
            let tag = format!("c{index}-");
            for file in files.values_mut() {
                if self.rng.random_bool(P_TOUCH) {
                    self.edit(file, &tag);
                }
            }
            if !files.contains_key(FILES[1]) && self.rng.random_bool(P_ADD_SECOND_FILE) {
                files.insert(FILES[1], self.file(&tag));
            }
            commits.push(files.clone());
        }
        for file in files.values_mut() {
            if self.rng.random_bool(P_WC_TOUCH) {
                self.edit(file, "w-");
            }
        }
        if files.len() > 1 && self.rng.random_bool(P_WC_DELETE) {
            files.remove(FILES[0]);
        }
        if self.rng.random_bool(P_WC_NEW_FILE) {
            files.insert(FILES[2], self.file("w-"));
        }
        (commits, files)
    }

    /// `base` with `a.txt` and `b.txt`; `c1` and `c2` on it, each editing one of them, so
    /// they merge without a conflict; then either a merge commit `c3` on both, maybe with
    /// edits, and the working copy on it, or the working copy on both.
    fn merge_case(&mut self) -> Input {
        let mut base = Files::new();
        for path in &FILES[..2] {
            base.insert(path, self.file("base-"));
        }
        let mut branches = Vec::with_capacity(2);
        for (index, path) in FILES[..2].iter().enumerate() {
            let mut files = base.clone();
            let file = files.get_mut(path).expect("base holds both files");
            self.edit(file, &format!("c{}-", index + 1));
            branches.push(files);
        }
        let mut merged = base.clone();
        for (branch, path) in branches.iter().zip(&FILES[..2]) {
            merged.insert(path, branch[path].clone());
        }
        let mut commits = vec![base];
        commits.extend(branches);
        let mut parents = vec![vec![], vec![0], vec![0]];
        let working_copy_parents = if self.rng.random_bool(P_MERGE_WORKING_COPY) {
            vec![1, 2]
        } else {
            for file in merged.values_mut() {
                if self.rng.random_bool(P_TOUCH) {
                    self.edit(file, "c3-");
                }
            }
            commits.push(merged.clone());
            parents.push(vec![1, 2]);
            vec![3]
        };
        for file in merged.values_mut() {
            if self.rng.random_bool(P_WC_TOUCH) {
                self.edit(file, "w-");
            }
        }
        Input { commits, parents, working_copy: merged, working_copy_parents, into: None }
    }

    /// A linear case with `--into` one or two of its commits, named back from the working
    /// copy: `@-` is the last.
    fn narrow_case(&mut self) -> Input {
        let mut input = Input::linear(self.case());
        let len = input.commits.len();
        let back = |rng: &mut ChaCha20Rng| {
            format!("{WORKING_COPY}{}", "-".repeat(rng.random_range(1..=len)))
        };
        let first = back(&mut self.rng);
        let into = if len > 1 && self.rng.random_bool(P_INTO_TWO) {
            let second = back(&mut self.rng);
            format!("{first} | {second}")
        } else {
            first
        };
        input.into = Some(into);
        input
    }
}

/// Runs `jj` in a directory with an isolated config.
struct Jj<'a> {
    dir: &'a Path,
    config: &'a Path,
}

impl Jj<'_> {
    /// Run jj; its output whether it succeeds or not.
    fn output(&self, args: &[&str]) -> std::process::Output {
        Command::new("jj")
            .args(args)
            .current_dir(self.dir)
            .env("JJ_CONFIG", self.config)
            .output()
            .expect("jj runs")
    }

    fn run(&self, args: &[&str]) -> String {
        let output = self.output(args);
        assert!(
            output.status.success(),
            "jj {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("jj prints UTF-8")
    }

    fn tree(&self, rev: &str) -> Tree {
        self.run(&["file", "list", "-r", rev])
            .lines()
            .map(|path| (path.to_owned(), self.run(&["file", "show", "-r", rev, path])))
            .collect()
    }

    fn write(&self, files: &Files) {
        for path in FILES {
            let file = self.dir.join(path);
            match files.get(path) {
                Some(contents) => fs::write(&file, contents.text()).expect("a file is written"),
                None if file.exists() => fs::remove_file(&file).expect("a file is removed"),
                None => {}
            }
        }
    }
}

#[derive(Serialize)]
struct Case {
    id: String,
    commits: Vec<Tree>,
    /// Each commit's parents, by index into `commits`; none for `base`, on the root.
    parents: Vec<Vec<usize>>,
    working_copy: Tree,
    working_copy_parents: Vec<usize>,
    /// `--into`; none for jj's default.
    into: Option<String>,
    absorbed: Outcome,
}

#[derive(Serialize)]
struct Outcome {
    conflicted: Vec<String>,
    commits: Vec<Option<Tree>>,
    working_copy: Option<Tree>,
}

fn name(index: usize) -> String {
    if index == 0 { "base".to_owned() } else { format!("c{index}") }
}

fn absorb(dir: &Path, config: &Path, id: String, input: &Input) -> Case {
    fs::create_dir(dir).expect("the case's directory is made");
    let jj = Jj { dir, config };
    jj.run(&["git", "init"]);
    let change_of = |rev: &str| jj.run(&["log", "--no-graph", "-r", rev, "-T", "change_id"]);
    let mut changes: Vec<String> = Vec::with_capacity(input.commits.len());
    let new_on = |changes: &[String], parents: &[usize]| {
        let mut args = vec!["new"];
        args.extend(parents.iter().map(|&parent| changes[parent].as_str()));
        jj.run(&args);
    };
    for (index, (files, parents)) in input.commits.iter().zip(&input.parents).enumerate() {
        // `jj git init` leaves an empty working copy on the root, which `base` takes.
        if !parents.is_empty() {
            new_on(&changes, parents);
        }
        jj.write(files);
        jj.run(&["describe", "-m", &name(index)]);
        changes.push(change_of("@"));
    }
    new_on(&changes, &input.working_copy_parents);
    jj.write(&input.working_copy);
    let wc_change = change_of("@");
    let mut args = vec!["absorb"];
    if let Some(into) = &input.into {
        args.extend(["--into", into]);
    }
    jj.run(&args);
    let conflicted: Vec<String> = jj
        .run(&["log", "--no-graph", "-r", "conflicts()", "-T", "change_id ++ \"\\n\""])
        .lines()
        .map(|change| {
            if change == wc_change {
                return WORKING_COPY.to_owned();
            }
            let index = changes
                .iter()
                .position(|c| c == change)
                .unwrap_or_else(|| panic!("conflicted change {change} is in the history"));
            name(index)
        })
        .collect();
    let read =
        |change: &str, named: String| (!conflicted.contains(&named)).then(|| jj.tree(change));
    let after: Vec<Option<Tree>> =
        changes.iter().enumerate().map(|(index, change)| read(change, name(index))).collect();
    let working_copy = read(WORKING_COPY, WORKING_COPY.to_owned());
    Case {
        id,
        commits: input.commits.iter().map(tree).collect(),
        parents: input.parents.clone(),
        working_copy: tree(&input.working_copy),
        working_copy_parents: input.working_copy_parents.clone(),
        into: input.into.clone(),
        absorbed: Outcome { conflicted, commits: after, working_copy },
    }
}

/// Run `jj absorb` on each of `inputs`, named `{kind}-{index}`, across the machine's threads.
fn run_cases(root: &Path, config: &Path, kind: &str, inputs: &[Input]) -> Vec<Case> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let chunk = inputs.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let handles: Vec<_> = inputs
            .chunks(chunk)
            .enumerate()
            .map(|(part, inputs)| {
                scope.spawn(move || {
                    inputs
                        .iter()
                        .enumerate()
                        .map(|(offset, input)| {
                            let index = part * chunk + offset;
                            let dir = root.join(format!("{kind}-{index}"));
                            absorb(&dir, config, format!("{kind}-{index}"), input)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().expect("a worker ends")).collect()
    })
}

/// The first `clean` cases `jj absorb` left resolved and the first `conflicted` it did not.
fn keep(cases: &[Case], clean: usize, conflicted: usize) -> Vec<&Case> {
    let resolved = cases.iter().filter(|case| case.absorbed.conflicted.is_empty()).take(clean);
    let conflicts =
        cases.iter().filter(|case| !case.absorbed.conflicted.is_empty()).take(conflicted);
    let kept: Vec<&Case> = resolved.chain(conflicts).collect();
    assert_eq!(kept.len(), clean + conflicted, "the pool holds enough of each outcome");
    kept
}

/// `POOL` inputs from `generate`, less those with nothing to absorb.
fn pool(seed: u64, mut generate: impl FnMut(&mut Generator) -> Input) -> Vec<Input> {
    let mut generator = Generator { rng: ChaCha20Rng::seed_from_u64(seed), lines: 0 };
    (0..POOL).map(|_| generate(&mut generator)).filter(|input| !input.is_empty()).collect()
}

fn jj_version() -> String {
    let output = Command::new("jj").arg("--version").output().expect("jj runs");
    let text = String::from_utf8(output.stdout).expect("jj prints UTF-8");
    text.trim().strip_prefix("jj ").expect("jj names its version").to_owned()
}

fn aliases(dir: &Path, config: &Path, version: &str) -> String {
    let jj = Jj { dir, config };
    let listed = jj.run(&[
        "config",
        "list",
        "--include-defaults",
        "-T",
        r#"json(name) ++ "\t" ++ json(value) ++ "\n""#,
        "revset-aliases",
    ]);
    let aliases: BTreeMap<String, String> = listed
        .lines()
        .map(|line| {
            let (name, value) = line.split_once('\t').expect("a name and a value");
            let name: String = serde_json::from_str(name).expect("a JSON name");
            let decl = name
                .strip_prefix("revset-aliases.")
                .expect("an alias")
                .trim_matches('"')
                .to_owned();
            (decl, serde_json::from_str(value).expect("a JSON string"))
        })
        .collect();
    let value = serde_json::json!({ "jj": version, "aliases": aliases });
    format!("{}\n", serde_json::to_string_pretty(&value).expect("JSON"))
}

fn main() {
    let version = jj_version();
    let locked = locked_jj_lib_version();
    assert_eq!(version, locked, "the jj on PATH must be the release Cargo.lock pins");
    let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/oracle");
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let config = scratch.path().join("config.toml");
    fs::write(&config, CONFIG).expect("the config is written");
    fs::write(oracle.join("jj-cli-aliases.json"), aliases(scratch.path(), &config, &version))
        .expect("the aliases are written");

    let root = scratch.path();
    let linear = run_cases(root, &config, "seed", &pool(SEED, |g| Input::linear(g.case())));
    let merges = run_cases(root, &config, "merge", &pool(MERGE_SEED, Generator::merge_case));
    let intos = run_cases(root, &config, "into", &pool(INTO_SEED, Generator::narrow_case));
    let kept: Vec<&Case> = keep(&linear, CLEAN_CASES, CONFLICT_CASES)
        .into_iter()
        .chain(keep(&merges, MERGE_CLEAN_CASES, MERGE_CONFLICT_CASES))
        .chain(keep(&intos, INTO_CLEAN_CASES, INTO_CONFLICT_CASES))
        .collect();
    let lines: Vec<String> =
        kept.iter().map(|case| serde_json::to_string(case).expect("JSON")).collect();
    let body = format!(
        "{{\"jj\":{},\"seed\":{SEED},\"cases\":[\n{}\n]}}\n",
        serde_json::to_string(&version).expect("JSON"),
        lines.join(",\n")
    );
    fs::write(oracle.join("jj-cli-absorb.json"), body).expect("the absorb corpus is written");
    let resolved = resolve::corpus(scratch.path(), &config, &version);
    fs::write(oracle.join("jj-cli-resolve.json"), resolved).expect("the resolve corpus is written");
    println!("{} absorb cases", kept.len());
}
