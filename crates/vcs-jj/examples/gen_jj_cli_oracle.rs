//! Writes `fixtures/oracle/jj-cli-aliases.json` and `fixtures/oracle/jj-cli-absorb.json`
//! from jj's own CLI (`DESIGN.md` §I1, §I5). Dev-time only; `script/gen-jj-cli-oracle` runs
//! it. The `jj` on `PATH` must be the release whose jj-lib `Cargo.lock` pins.
//!
//! The aliases are jj's defaults for `revset-aliases`. Each absorb case is a seeded random
//! stack of commits on the root, `base` first, and a working copy on it; the case records
//! each commit's tree after `jj absorb`, and the commits it left conflicted.
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

const SEED: u64 = 500_135_715_172;
/// The inputs generated; the corpus keeps the first cases of each kind.
const POOL: usize = 200;
/// Cases where `jj absorb` leaves every commit resolved: triad must match them exactly.
const CLEAN_CASES: usize = 48;
/// Cases where `jj absorb` writes a conflict, which triad keeps out.
const CONFLICT_CASES: usize = 16;
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
}

/// Runs `jj` in a directory with an isolated config.
struct Jj<'a> {
    dir: &'a Path,
    config: &'a Path,
}

impl Jj<'_> {
    fn run(&self, args: &[&str]) -> String {
        let output = Command::new("jj")
            .args(args)
            .current_dir(self.dir)
            .env("JJ_CONFIG", self.config)
            .output()
            .expect("jj runs");
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
    working_copy: Tree,
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

fn absorb(dir: &Path, config: &Path, id: String, commits: &[Files], wc: &Files) -> Case {
    fs::create_dir(dir).expect("the case's directory is made");
    let jj = Jj { dir, config };
    jj.run(&["git", "init"]);
    let mut changes = Vec::with_capacity(commits.len());
    for (index, files) in commits.iter().enumerate() {
        jj.write(files);
        changes.push(jj.run(&["log", "--no-graph", "-r", "@", "-T", "change_id"]));
        jj.run(&["commit", "-m", &name(index)]);
    }
    jj.write(wc);
    let wc_change = jj.run(&["log", "--no-graph", "-r", "@", "-T", "change_id"]);
    jj.run(&["absorb"]);
    let conflicted: Vec<String> = jj
        .run(&["log", "--no-graph", "-r", "conflicts()", "-T", "change_id ++ \"\\n\""])
        .lines()
        .map(|change| {
            if change == wc_change {
                return "@".to_owned();
            }
            let index = changes
                .iter()
                .position(|c| c == change)
                .unwrap_or_else(|| panic!("conflicted change {change} is in the stack"));
            name(index)
        })
        .collect();
    let read =
        |change: &str, named: String| (!conflicted.contains(&named)).then(|| jj.tree(change));
    let after: Vec<Option<Tree>> =
        changes.iter().enumerate().map(|(index, change)| read(change, name(index))).collect();
    let working_copy = read("@", "@".to_owned());
    Case {
        id,
        commits: commits.iter().map(tree).collect(),
        working_copy: tree(wc),
        absorbed: Outcome { conflicted, commits: after, working_copy },
    }
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

    let mut generator = Generator { rng: ChaCha20Rng::seed_from_u64(SEED), lines: 0 };
    // A working copy that holds its parent's tree has no hunk to absorb.
    let inputs: Vec<(Vec<Files>, Files)> = (0..POOL)
        .map(|_| generator.case())
        .filter(|(commits, wc)| commits.last().map(tree) != Some(tree(wc)))
        .collect();
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let chunk = inputs.len().div_ceil(threads);
    let cases: Vec<Case> = std::thread::scope(|scope| {
        let handles: Vec<_> = inputs
            .chunks(chunk)
            .enumerate()
            .map(|(part, inputs)| {
                let (root, config) = (scratch.path(), &config);
                scope.spawn(move || {
                    inputs
                        .iter()
                        .enumerate()
                        .map(|(offset, (commits, wc))| {
                            let index = part * chunk + offset;
                            let dir = root.join(format!("case-{index}"));
                            absorb(&dir, config, format!("seed-{index}"), commits, wc)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().expect("a worker ends")).collect()
    });
    let clean = cases.iter().filter(|case| case.absorbed.conflicted.is_empty()).take(CLEAN_CASES);
    let conflicted =
        cases.iter().filter(|case| !case.absorbed.conflicted.is_empty()).take(CONFLICT_CASES);
    let kept: Vec<&Case> = clean.chain(conflicted).collect();
    assert_eq!(kept.len(), CLEAN_CASES + CONFLICT_CASES, "the pool holds enough of each kind");
    let lines: Vec<String> =
        kept.iter().map(|case| serde_json::to_string(case).expect("JSON")).collect();
    let body = format!(
        "{{\"jj\":{},\"seed\":{SEED},\"cases\":[\n{}\n]}}\n",
        serde_json::to_string(&version).expect("JSON"),
        lines.join(",\n")
    );
    fs::write(oracle.join("jj-cli-absorb.json"), body).expect("the absorb corpus is written");
    println!(
        "{CLEAN_CASES} absorb cases jj leaves resolved, {CONFLICT_CASES} where it writes a conflict"
    );
}
