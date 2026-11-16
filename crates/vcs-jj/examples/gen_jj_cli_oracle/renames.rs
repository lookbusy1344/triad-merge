//! `jj-cli-renames.json`: seeded pairs of trees, `left` committed and `right` in the working
//! copy on it, with the changes `jj diff` names between them, renames and copies included.
//! Files are short and share lines, so many pairs score near jj's 50 % threshold; some
//! repeat another file's bytes, are empty or are binary, and some move to a directory with a
//! file of the same name, so ties fall to jj's order. The tie cases, from a second stream,
//! delete two files of the same bytes and add one edited copy that takes the name of one of
//! them: the scores tie, and the name decides. The copy cases, from a third stream, modify
//! most files and add files drawn from a modified file's old or new bytes, or an unchanged
//! file's, so jj's copies from modified files show against files that are no source.

use std::collections::BTreeMap;
use std::fs;
use std::ops::RangeInclusive;
use std::path::Path;

use rand::seq::{IndexedRandom as _, SliceRandom as _};
use rand::{RngExt as _, SeedableRng as _};
use rand_chacha::ChaCha20Rng;
use serde::Serialize;

use crate::{Jj, Tree};

const SEED: u64 = 500_135_715_175;
const CASES: usize = 160;
/// The seed of the tie cases' stream.
const TIE_SEED: u64 = 500_135_715_176;
const TIE_CASES: usize = 16;
/// The lines of a tie case's file: enough that one replaced line keeps more than half.
const TIE_LINES: RangeInclusive<usize> = 4..=8;
/// The seed of the copy cases' stream.
const COPY_SEED: u64 = 500_135_715_177;
const COPY_CASES: usize = 48;
/// The files new in a copy case's `right`.
const COPY_NEW: RangeInclusive<usize> = 1..=3;
/// A copy case's odds that a file is modified, else deleted, else moved, else kept.
const P_COPY_MODIFY: f64 = 0.6;
const P_COPY_DELETE: f64 = 0.1;
const P_COPY_MOVE: f64 = 0.1;
/// The files of a `left` tree.
const LEFT_FILES: RangeInclusive<usize> = 2..=6;
/// The lines of a new file.
const LINES: RangeInclusive<usize> = 1..=8;
/// The edits to a modified or moved file.
const EDITS: RangeInclusive<usize> = 1..=3;
/// The files new in `right`, at most.
const MAX_NEW: usize = 2;
/// Every path a case uses: few names in few directories, so names repeat across them.
const DIRS: [&str; 3] = ["", "a/", "b/c/"];
const NAMES: [&str; 4] = ["x.txt", "y.txt", "z.rs", "w"];
/// Shared lines `s0`.. are drawn from this many; the rest are unique to their file.
const SHARED_LINES: usize = 12;
const P_SHARED_LINE: f64 = 0.5;
/// The longest padding a line takes, so lines weigh differently in bytes.
const MAX_PAD: usize = 6;
const P_DUPLICATE: f64 = 0.15;
const P_EMPTY: f64 = 0.05;
const P_BINARY: f64 = 0.08;
const P_KEEP: f64 = 0.25;
const P_MODIFY: f64 = 0.2;
const P_DELETE: f64 = 0.15;
/// The share of moves that also edit the file.
const P_MOVE_EDIT: f64 = 0.6;
/// The share of new files that start from another file's bytes.
const P_NEW_FROM_EXISTING: f64 = 0.5;
const P_NEW_EDIT: f64 = 0.5;
/// The byte git and gix sniff for a binary file.
const NUL: char = '\0';
/// One entry of `jj diff`, as its template prints it: a JSON status, source and target.
const TEMPLATE: &str = r#"diff.files().map(|e| json(e.status()) ++ "\t" ++ json(e.source().path().display()) ++ "\t" ++ json(e.target().path().display()) ++ "\n").join("")"#;

#[derive(Serialize)]
struct Corpus<'a> {
    jj: &'a str,
    seed: u64,
    cases: Vec<Case>,
}

#[derive(Serialize)]
struct Case {
    id: String,
    left: Tree,
    right: Tree,
    /// `jj diff`'s entries: status, source path and target path.
    changes: Vec<(String, String, String)>,
}

struct Generator {
    rng: ChaCha20Rng,
    unique: usize,
}

impl Generator {
    fn line(&mut self) -> String {
        let pad = "-".repeat(self.rng.random_range(0..=MAX_PAD));
        if self.rng.random_bool(P_SHARED_LINE) {
            return format!("s{}{pad}\n", self.rng.random_range(0..SHARED_LINES));
        }
        self.unique += 1;
        format!("u{}{pad}\n", self.unique)
    }

    fn file(&mut self) -> String {
        if self.rng.random_bool(P_EMPTY) {
            return String::new();
        }
        let text: String = (0..self.rng.random_range(LINES)).map(|_| self.line()).collect();
        if self.rng.random_bool(P_BINARY) { format!("{NUL}{text}") } else { text }
    }

    /// `text` with lines replaced, inserted or deleted.
    fn edit(&mut self, text: &str) -> String {
        let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
        for _ in 0..self.rng.random_range(EDITS) {
            let len = lines.len();
            match self.rng.random_range(0..3) {
                0 if len > 0 => {
                    let at = self.rng.random_range(0..len);
                    lines[at] = self.line();
                }
                1 if len > 1 => {
                    let at = self.rng.random_range(0..len);
                    lines.remove(at);
                }
                _ => {
                    let at = self.rng.random_range(0..=len);
                    let line = self.line();
                    lines.insert(at, line);
                }
            }
        }
        lines.concat()
    }

    /// Two deleted files of the same text in two directories, under two names, and one
    /// added file in a third directory, under one of the names, with one line replaced.
    fn tie_case(&mut self) -> (Tree, Tree) {
        let mut dirs = DIRS.to_vec();
        let mut names = NAMES.to_vec();
        dirs.shuffle(&mut self.rng);
        names.shuffle(&mut self.rng);
        let text: String = (0..self.rng.random_range(TIE_LINES)).map(|_| self.line()).collect();
        let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
        let at = self.rng.random_range(0..lines.len());
        lines[at] = self.line();
        let kept = *names[..2].choose(&mut self.rng).expect("two names");
        let left = Tree::from([
            (format!("{}{}", dirs[0], names[0]), text.clone()),
            (format!("{}{}", dirs[1], names[1]), text),
        ]);
        let right = Tree::from([(format!("{}{kept}", dirs[2]), lines.concat())]);
        (left, right)
    }

    /// Files modified, deleted, moved or kept, then new files drawn from a modified file's new
    /// bytes, its old bytes or a kept file's, edited or not, or fresh.
    fn copy_case(&mut self) -> (Tree, Tree) {
        let mut left = Tree::new();
        for _ in 0..self.rng.random_range(LEFT_FILES) {
            let Some(path) = self.free_path(&left, &Tree::new()) else { break };
            let text = self.file();
            left.insert(path, text);
        }
        let mut right = Tree::new();
        // The bytes a new file can start from: the new and old sides of each modified file,
        // and each kept file.
        let mut sources = Vec::new();
        for (path, text) in &left {
            let roll: f64 = self.rng.random();
            if roll < P_COPY_MODIFY {
                let edited = self.edit(text);
                sources.push(edited.clone());
                sources.push(text.clone());
                right.insert(path.clone(), edited);
            } else if roll < P_COPY_MODIFY + P_COPY_DELETE {
            } else if roll < P_COPY_MODIFY + P_COPY_DELETE + P_COPY_MOVE
                && let Some(to) = self.free_path(&left, &right)
            {
                right.insert(to, text.clone());
            } else {
                sources.push(text.clone());
                right.insert(path.clone(), text.clone());
            }
        }
        for _ in 0..self.rng.random_range(COPY_NEW) {
            let Some(path) = self.free_path(&left, &right) else { break };
            let text = match sources.choose(&mut self.rng) {
                Some(text) if !self.rng.random_bool(P_EMPTY) => {
                    if self.rng.random_bool(P_NEW_EDIT) { self.edit(text) } else { text.clone() }
                }
                _ => self.file(),
            };
            right.insert(path, text);
        }
        (left, right)
    }

    /// A path in neither tree, if one is left.
    fn free_path(&mut self, left: &Tree, right: &Tree) -> Option<String> {
        let free: Vec<String> = DIRS
            .iter()
            .flat_map(|dir| NAMES.iter().map(move |name| format!("{dir}{name}")))
            .filter(|path| !left.contains_key(path) && !right.contains_key(path))
            .collect();
        free.choose(&mut self.rng).cloned()
    }

    fn case(&mut self) -> (Tree, Tree) {
        let mut left = Tree::new();
        for _ in 0..self.rng.random_range(LEFT_FILES) {
            let Some(path) = self.free_path(&left, &Tree::new()) else { break };
            let existing: Vec<String> = left.values().cloned().collect();
            let text = match existing.choose(&mut self.rng) {
                Some(text) if self.rng.random_bool(P_DUPLICATE) => text.clone(),
                _ => self.file(),
            };
            left.insert(path, text);
        }
        let mut right = Tree::new();
        for (path, text) in &left {
            let roll: f64 = self.rng.random();
            if roll < P_KEEP {
                right.insert(path.clone(), text.clone());
            } else if roll < P_KEEP + P_MODIFY {
                let edited = self.edit(text);
                right.insert(path.clone(), edited);
            } else if roll < P_KEEP + P_MODIFY + P_DELETE {
            } else if let Some(to) = self.free_path(&left, &right) {
                let moved =
                    if self.rng.random_bool(P_MOVE_EDIT) { self.edit(text) } else { text.clone() };
                right.insert(to, moved);
            } else {
                right.insert(path.clone(), text.clone());
            }
        }
        for _ in 0..self.rng.random_range(0..=MAX_NEW) {
            let Some(path) = self.free_path(&left, &right) else { break };
            let existing: Vec<String> = left.values().cloned().collect();
            let text = match existing.choose(&mut self.rng) {
                Some(text) if self.rng.random_bool(P_NEW_FROM_EXISTING) => {
                    if self.rng.random_bool(P_NEW_EDIT) { self.edit(text) } else { text.clone() }
                }
                _ => self.file(),
            };
            right.insert(path, text);
        }
        (left, right)
    }
}

/// Make the files below `dir` hold `tree`, removing the files of `before` it lacks.
fn write(dir: &Path, before: &Tree, tree: &Tree) {
    for path in before.keys().filter(|path| !tree.contains_key(*path)) {
        fs::remove_file(dir.join(path)).expect("a file is removed");
    }
    for (path, text) in tree {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().expect("a file has a parent")).expect("dirs are made");
        fs::write(file, text).expect("a file is written");
    }
}

fn run(dir: &Path, config: &Path, id: String, left: Tree, right: Tree) -> Case {
    fs::create_dir(dir).expect("the case's directory is made");
    let jj = Jj { dir, config };
    jj.run(&["git", "init"]);
    write(dir, &Tree::new(), &left);
    jj.run(&["commit", "-m", "left"]);
    write(dir, &left, &right);
    let changes = jj
        .run(&["log", "--no-graph", "-r", "@", "-T", TEMPLATE])
        .lines()
        .map(|line| {
            let fields: Vec<String> = line
                .split('\t')
                .map(|field| serde_json::from_str(field).expect("a JSON string"))
                .collect();
            let [status, source, target] = <[String; 3]>::try_from(fields).expect("three fields");
            (status, source, target)
        })
        .collect();
    Case { id, left, right, changes }
}

/// The corpus, as JSON.
pub fn corpus(scratch: &Path, config: &Path, version: &str) -> String {
    let mut generator = Generator { rng: ChaCha20Rng::seed_from_u64(SEED), unique: 0 };
    let root = scratch.join("renames");
    fs::create_dir(&root).expect("the corpus directory is made");
    let mut ties = Generator { rng: ChaCha20Rng::seed_from_u64(TIE_SEED), unique: 0 };
    let mut copies = Generator { rng: ChaCha20Rng::seed_from_u64(COPY_SEED), unique: 0 };
    let inputs = (0..CASES)
        .map(|index| (format!("rename-{index}"), generator.case()))
        .chain((0..TIE_CASES).map(|index| (format!("tie-{index}"), ties.tie_case())))
        .chain((0..COPY_CASES).map(|index| (format!("copy-{index}"), copies.copy_case())));
    let cases =
        inputs.map(|(id, (left, right))| run(&root.join(&id), config, id, left, right)).collect();
    let corpus = Corpus { jj: version, seed: SEED, cases };
    format!("{}\n", serde_json::to_string_pretty(&corpus).expect("JSON"))
}

/// The cases' changes, counted by status, for the generator's report.
pub fn summary(corpus: &str) -> BTreeMap<String, usize> {
    let value: serde_json::Value = serde_json::from_str(corpus).expect("JSON");
    let mut counts = BTreeMap::new();
    for case in value["cases"].as_array().expect("cases") {
        for change in case["changes"].as_array().expect("changes") {
            *counts.entry(change[0].as_str().expect("a status").to_owned()).or_insert(0) += 1;
        }
    }
    counts
}
