//! The directory listing's renames and copies against `jj diff`'s, from `fixtures/oracle/jj-cli-renames.json`
//! (spec §6.1, `DESIGN.md` §I5): each case's `left` and `right` trees, written as two
//! directories, list the changes jj names between the same trees committed in a repository.

#![expect(clippy::panic, reason = "a helper outside `#[test]` panics on a corpus it cannot read")]

use std::fs;
use std::path::Path;

use merge_session::dir_diff::{self, Change};
use serde_json::Value;

const RENAMES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/oracle/jj-cli-renames.json");
/// The cases the generator writes: 160 random, 16 that tie, 48 that copy.
const CASES: usize = 224;

/// One change as status, source path and target path, in `jj diff`'s template terms.
type Entry = (String, String, String);

fn entry(status: &str, source: &str, target: &str) -> Entry {
    (status.to_owned(), source.to_owned(), target.to_owned())
}

/// jj's changes in the listing's terms.
fn expected(changes: &Value) -> Vec<Entry> {
    let mut entries: Vec<Entry> = changes
        .as_array()
        .expect("changes")
        .iter()
        .map(|change| {
            let field = |at: usize| change[at].as_str().expect("a string");
            let (status, source, target) = (field(0), field(1), field(2));
            match status {
                "renamed" | "copied" => entry(status, source, target),
                "added" | "modified" => entry(status, target, target),
                "removed" => entry(status, source, source),
                other => panic!("an unknown status {other}"),
            }
        })
        .collect();
    entries.sort();
    entries
}

fn write(dir: &Path, tree: &Value) {
    fs::create_dir(dir).expect("a side's directory");
    for (path, text) in tree.as_object().expect("a tree") {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().expect("a parent")).expect("directories");
        fs::write(file, text.as_str().expect("text")).expect("a file");
    }
}

fn listed(case: &Value) -> Vec<Entry> {
    let root = tempfile::tempdir().expect("a temporary directory");
    let (left, right) = (root.path().join("left"), root.path().join("right"));
    write(&left, &case["left"]);
    write(&right, &case["right"]);
    let mut entries: Vec<Entry> = dir_diff::changed_files(&left, &right)
        .expect("the directories list")
        .iter()
        .map(|file| {
            let path = file.display_path();
            match &file.change {
                Change::Renamed { from } => entry("renamed", &dir_diff::display(from), &path),
                Change::Copied { from } => entry("copied", &dir_diff::display(from), &path),
                Change::Added => entry("added", &path, &path),
                Change::Deleted => entry("removed", &path, &path),
                Change::Modified => entry("modified", &path, &path),
            }
        })
        .collect();
    entries.sort();
    entries
}

#[test]
fn the_listing_names_the_changes_renames_and_copies_jj_names() {
    let corpus: Value =
        serde_json::from_str(&fs::read_to_string(RENAMES).expect("the corpus reads"))
            .expect("the corpus is JSON");
    assert_eq!(corpus["jj"].as_str(), Some(vcs_jj::testing::locked_jj_lib_version().as_str()));
    let cases = corpus["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), CASES);
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let (want, got) = (expected(&case["changes"]), listed(case));
            (want != got).then(|| format!("{}:\n  jj    {want:?}\n  triad {got:?}", case["id"]))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {CASES} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
