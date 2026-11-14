//! `jj-cli-resolve.json`: repositories built step by step with jj's CLI, then one conflict
//! written into a commit by `jj resolve -r` with a tool that copies a fixed result. Each case
//! records the repository after: the commits, the files on disk, git's `HEAD` and refs, and
//! the operations the write added.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use vcs_jj::testing::without_conflict_labels;

use crate::{Jj, Tree};

/// The working-copy commit's name, and a resolve of it.
const WORKING_COPY: &str = "@";
/// jj's root commit ID in a git store, and the name the corpus gives it.
const ROOT_ID: &str = "0000000000000000000000000000000000000000";
const ROOT: &str = "root";
/// jj's shortest conflict markers, which these conflicts take.
const MARKER_LENGTH: usize = 7;
/// A result that keeps the conflict with side 1 changed, in jj's `snapshot` marker style.
/// A result whose markers parse back as the conflict the commit holds.
const SAME_CONFLICT: &str = "<<<<<<< conflict 1 of 1\n+++++++ side #1\nleft\n------- base #1\na\n+++++++ side #2\nright\n>>>>>>> conflict 1 of 1 ends\n";
const KEEPS_CONFLICT: &str = "<<<<<<< conflict 1 of 1\n+++++++ side #1\nleft2\n------- base #1\na\n+++++++ side #2\nright\n>>>>>>> conflict 1 of 1 ends\n";

fn commit(name: &str, parents: &[&str], files: &[(&str, &str)]) -> Value {
    json!({ "name": name, "parents": parents, "files": files.iter().copied().collect::<BTreeMap<_, _>>() })
}

/// `base` writes `a.txt` and `b.txt`; `L` and `R` change `a.txt` on it; `M` merges them, so
/// `a.txt` conflicts; `D` on `M` changes `b.txt` and inherits the conflict. Bookmark `m` names
/// `M`, `d` names `D`.
fn stack() -> Value {
    json!([
        commit("base", &[], &[("a.txt", "a\n"), ("b.txt", "b\n")]),
        commit("L", &["base"], &[("a.txt", "left\n")]),
        commit("R", &["base"], &[("a.txt", "right\n")]),
        commit("M", &["L", "R"], &[]),
        commit("D", &["M"], &[("b.txt", "b2\n")]),
    ])
}

fn case(
    id: &str,
    layout: &str,
    working_copy: &Value,
    tags: &Value,
    target: &str,
    result: &str,
) -> Value {
    json!({
        "id": id,
        "layout": layout,
        "commits": stack(),
        "bookmarks": { "m": "M", "d": "D" },
        "tags": tags,
        "working_copy": working_copy,
        "resolve": { "commit": target, "path": "a.txt", "result": result, "marker_length": MARKER_LENGTH },
    })
}

fn working_copy(parents: &[&str], files: &[(&str, &str)]) -> Value {
    commit("", parents, files)
}

/// `spec` marked with a departure of triad's from jj, which the test applies to jj's result.
fn departing(mut spec: Value, departure: &str) -> Value {
    spec["departure"] = json!(departure);
    spec
}

fn cases() -> Vec<Value> {
    const MERGED: &str = "merged\n";
    let none = json!({});
    vec![
        case(
            "descendants rebase and @ moves",
            "colocated",
            &working_copy(&["D"], &[]),
            &none,
            "M",
            MERGED,
        ),
        case(
            "an edit on disk is snapshotted first",
            "colocated",
            &working_copy(&["D"], &[("c.txt", "new\n")]),
            &none,
            "M",
            MERGED,
        ),
        case(
            "the target is @-",
            "colocated",
            &working_copy(&["M"], &[("b.txt", "b3\n")]),
            &none,
            "M",
            MERGED,
        ),
        case(
            "the target is @",
            "colocated",
            &working_copy(&["L", "R"], &[]),
            &none,
            WORKING_COPY,
            MERGED,
        ),
        case(
            "markers keep the conflict",
            "colocated",
            &working_copy(&["D"], &[]),
            &none,
            "M",
            KEEPS_CONFLICT,
        ),
        case(
            "a tagged commit is immutable",
            "colocated",
            &working_copy(&["D"], &[]),
            &json!({ "t": "M" }),
            "M",
            MERGED,
        ),
        departing(
            case(
                "a tagged commit is immutable, with an edit on disk",
                "colocated",
                &working_copy(&["D"], &[("c.txt", "new\n")]),
                &json!({ "t": "M" }),
                "M",
                MERGED,
            ),
            "refused-before-snapshot",
        ),
        departing(
            case(
                "markers equal to the conflict",
                "colocated",
                &working_copy(&["D"], &[]),
                &none,
                "M",
                SAME_CONFLICT,
            ),
            "writes-nothing",
        ),
        case(
            "an internal repository",
            "internal",
            &working_copy(&["D"], &[("c.txt", "new\n")]),
            &none,
            "M",
            MERGED,
        ),
    ]
}

fn write_files(dir: &Path, files: &Value) {
    for (path, text) in files.as_object().expect("files") {
        fs::write(dir.join(path), text.as_str().expect("text")).expect("a file is written");
    }
}

fn strs(value: &Value) -> Vec<&str> {
    value.as_array().expect("an array").iter().map(|v| v.as_str().expect("a string")).collect()
}

fn operations(jj: &Jj<'_>) -> usize {
    jj.run(&["op", "log", "--ignore-working-copy", "--no-graph", "-T", "id ++ \"\\n\""])
        .lines()
        .count()
}

/// The files under `dir`, `.jj` and `.git` aside, by path relative to `root`, conflict
/// markers without their labels.
fn disk(root: &Path, dir: &Path, files: &mut Tree) {
    for entry in fs::read_dir(dir).expect("the directory reads") {
        let path = entry.expect("an entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).expect("a UTF-8 name");
        if name == ".jj" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            disk(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).expect("under the root");
            let text = fs::read_to_string(&path).expect("a text file");
            files.insert(
                relative.to_str().expect("UTF-8").to_owned(),
                without_conflict_labels(&text),
            );
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).expect("git prints UTF-8")
}

/// The repository in `dir` after the write, as the corpus records it.
fn after(jj: &Jj<'_>, dir: &Path, colocated: bool, refused: bool, operations: usize) -> Value {
    const LOG: &str = r#"commit_id ++ "\t" ++ description.first_line() ++ "\t" ++ parents.map(|c| c.commit_id()).join(",") ++ "\t" ++ if(current_working_copy, "@", "-") ++ "\n""#;
    let listed =
        jj.run(&["log", "--ignore-working-copy", "--no-graph", "-r", "all() ~ root()", "-T", LOG]);
    let rows: Vec<Vec<&str>> = listed.lines().map(|line| line.split('\t').collect()).collect();
    let mut names: BTreeMap<String, String> = rows
        .iter()
        .map(|row| {
            let name = if row[3] == WORKING_COPY { WORKING_COPY } else { row[1] };
            (row[0].to_owned(), name.to_owned())
        })
        .collect();
    names.insert(ROOT_ID.to_owned(), ROOT.to_owned());
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| format!("unknown {id}"));
    let snapshot = "ui.conflict-marker-style=\"snapshot\"";
    let commits: BTreeMap<String, Value> = rows
        .iter()
        .map(|row| {
            let id = row[0];
            let tree: Tree = jj
                .run(&["file", "list", "--ignore-working-copy", "-r", id])
                .lines()
                .map(|path| {
                    let text = jj.run(&[
                        "file",
                        "show",
                        "--ignore-working-copy",
                        "--config",
                        snapshot,
                        "-r",
                        id,
                        path,
                    ]);
                    (path.to_owned(), without_conflict_labels(&text))
                })
                .collect();
            let parents: Vec<String> = row[2].split(',').map(&name).collect();
            (name(id), json!({ "parents": parents, "tree": tree }))
        })
        .collect();
    let mut files = Tree::new();
    disk(dir, dir, &mut files);
    let (git_head, git_refs) = if colocated {
        let head = name(git(dir, &["rev-parse", "HEAD"]).trim());
        let refs: Tree = git(
            dir,
            &["for-each-ref", "--format=%(refname) %(objectname)", "refs/heads", "refs/tags"],
        )
        .lines()
        .map(|line| {
            let (reference, id) = line.split_once(' ').expect("a ref and its commit");
            (reference.to_owned(), name(id))
        })
        .collect();
        (Some(head), refs)
    } else {
        (None, Tree::new())
    };
    json!({
        "refused": refused,
        "operations": operations,
        "commits": commits,
        "disk": files,
        "git_head": git_head,
        "git_refs": git_refs,
    })
}

/// Build `spec` with jj's CLI in `dir`, write its result with `jj resolve -r`, and give the
/// spec with the repository after.
fn run(scratch: &Path, index: usize, config: &Path, mut spec: Value) -> Value {
    let dir = scratch.join(format!("resolve-{index}"));
    fs::create_dir(&dir).expect("the case's directory is made");
    let jj = Jj { dir: &dir, config };
    let colocated = spec["layout"] == "colocated";
    jj.run(if colocated { &["git", "init"] } else { &["git", "init", "--no-colocate"] });
    let mut changes: BTreeMap<String, String> = BTreeMap::new();
    for step in spec["commits"].as_array().expect("commits") {
        let parents: Vec<&str> =
            strs(&step["parents"]).iter().map(|p| changes[*p].as_str()).collect();
        let mut args = vec!["new"];
        args.extend(if parents.is_empty() { vec!["root()"] } else { parents });
        jj.run(&args);
        write_files(&dir, &step["files"]);
        let name = step["name"].as_str().expect("a name");
        jj.run(&["describe", "-m", name]);
        changes
            .insert(name.to_owned(), jj.run(&["log", "--no-graph", "-r", "@", "-T", "change_id"]));
    }
    for (kind, command) in [("bookmarks", ["bookmark", "create"]), ("tags", ["tag", "set"])] {
        for (name, commit) in spec[kind].as_object().expect("refs") {
            let change = &changes[commit.as_str().expect("a commit")];
            jj.run(&[command[0], command[1], name, "-r", change]);
        }
    }
    let wc_parents: Vec<&str> =
        strs(&spec["working_copy"]["parents"]).iter().map(|p| changes[*p].as_str()).collect();
    let mut args = vec!["new"];
    args.extend(wc_parents);
    jj.run(&args);
    write_files(&dir, &spec["working_copy"]["files"]);
    let before = operations(&jj);
    let result = scratch.join(format!("result-{index}"));
    fs::write(&result, spec["resolve"]["result"].as_str().expect("a result")).expect("written");
    let target = spec["resolve"]["commit"].as_str().expect("a commit");
    let rev = if target == WORKING_COPY { WORKING_COPY } else { changes[target].as_str() };
    let merge_args = format!(
        "merge-tools.fixed.merge-args=[{}, \"$output\"]",
        serde_json::to_string(result.to_str().expect("UTF-8")).expect("JSON")
    );
    let path = spec["resolve"]["path"].as_str().expect("a path").to_owned();
    let status = jj.output(&[
        "resolve",
        "-r",
        rev,
        &path,
        "--tool",
        "fixed",
        "--config",
        "merge-tools.fixed.program=\"cp\"",
        "--config",
        &merge_args,
        "--config",
        "merge-tools.fixed.merge-tool-edits-conflict-markers=true",
    ]);
    let refused = !status.status.success();
    let added = operations(&jj) - before;
    spec["after"] = after(&jj, &dir, colocated, refused, added);
    spec
}

/// The corpus, as JSON with one case per line.
pub fn corpus(scratch: &Path, config: &Path, version: &str) -> String {
    let lines: Vec<String> = cases()
        .into_iter()
        .enumerate()
        .map(|(index, spec)| {
            serde_json::to_string(&run(scratch, index, config, spec)).expect("JSON")
        })
        .collect();
    format!(
        "{{\"jj\":{},\"cases\":[\n{}\n]}}\n",
        serde_json::to_string(version).expect("JSON"),
        lines.join(",\n")
    )
}
