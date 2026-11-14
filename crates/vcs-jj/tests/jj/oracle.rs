//! triad against jj's own CLI (`DESIGN.md` §I1, §I5): the default revset aliases and
//! `absorb`, from corpora `script/gen-jj-cli-oracle` writes with the jj release whose jj-lib
//! `Cargo.lock` pins.

use std::collections::BTreeMap;

use serde::Deserialize;
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture, Held, locked_jj_lib_version, without_conflict_labels};

const ALIASES: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/oracle/jj-cli-aliases.json");
const ABSORB: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/oracle/jj-cli-absorb.json");
/// jj's default aliases that triad leaves out: only `jj log` reads it.
const LOG_ONLY: &str = "builtin_log()";
/// The name the corpus gives the working-copy commit among the conflicted.
const WORKING_COPY: &str = "@";

/// A file tree: each path and its text.
type Tree = BTreeMap<String, String>;

#[derive(Deserialize)]
struct Aliases {
    jj: String,
    aliases: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Corpus {
    jj: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    /// A linear stack on the root commit, `base` first; each commit's whole tree.
    commits: Vec<Tree>,
    working_copy: Tree,
    absorbed: Outcome,
}

/// What `jj absorb` left.
#[derive(Deserialize)]
struct Outcome {
    /// The commits it left conflicted: `base`, `c1`, … or `@`.
    conflicted: Vec<String>,
    /// Each commit's tree, in the order of [`Case::commits`]; none for a conflicted one.
    commits: Vec<Option<Tree>>,
    working_copy: Option<Tree>,
}

fn read<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    serde_json::from_str(&std::fs::read_to_string(path).expect("the corpus reads"))
        .expect("the corpus parses")
}

/// A revset definition with each run of whitespace as one space.
fn normalised(defn: &str) -> String {
    defn.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_corpora_come_from_the_jj_release_cargo_lock_pins() {
    let version = locked_jj_lib_version();
    assert_eq!(read::<Aliases>(ALIASES).jj, version, "regenerate with script/gen-jj-cli-oracle");
    assert_eq!(read::<Corpus>(ABSORB).jj, version, "regenerate with script/gen-jj-cli-oracle");
    assert_eq!(
        read::<ResolveCorpus>(RESOLVE).jj,
        version,
        "regenerate with script/gen-jj-cli-oracle"
    );
}

#[test]
fn the_default_aliases_are_jj_s() {
    let ours: BTreeMap<String, String> = vcs_jj::DEFAULT_ALIASES
        .iter()
        .map(|&(decl, defn)| (decl.to_owned(), normalised(defn)))
        .collect();
    let jj: BTreeMap<String, String> = read::<Aliases>(ALIASES)
        .aliases
        .into_iter()
        .filter(|(decl, _)| decl != LOG_ONLY)
        .map(|(decl, defn)| (decl, normalised(&defn)))
        .collect();
    assert_eq!(ours, jj);
}

/// Each path any tree of `case` holds.
fn paths(case: &Case) -> Vec<&str> {
    let mut paths: Vec<&str> = case
        .commits
        .iter()
        .chain([&case.working_copy])
        .chain(case.absorbed.commits.iter().flatten())
        .chain(&case.absorbed.working_copy)
        .flat_map(|tree| tree.keys().map(String::as_str))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

/// `tree` as the fixture reads it at each of `paths`.
fn expected(tree: &Tree, paths: &[&str]) -> Vec<Held> {
    paths
        .iter()
        .map(|path| tree.get(*path).map_or(Held::Absent, |text| Held::File(text.clone().into())))
        .collect()
}

fn held(fixture: &Fixture, commit: &Committed, paths: &[&str]) -> Vec<Held> {
    paths.iter().map(|path| fixture.held_in(commit, path)).collect()
}

/// Build `case`'s stack on the root commit of `fixture` and its working copy, absorb with
/// triad, and give each commit's held paths after, then the working copy's. The cases of one
/// fixture share only the root: absorb reaches the ancestors of the working copy alone.
fn absorbed_by_triad(
    fixture: &Fixture,
    case: &Case,
    paths: &[&str],
) -> (Vec<Vec<Held>>, Vec<Held>) {
    let mut stack: Vec<Committed> = Vec::with_capacity(case.commits.len());
    for (index, tree) in case.commits.iter().enumerate() {
        let files: Vec<(&str, &[u8])> =
            tree.iter().map(|(path, text)| (path.as_str(), text.as_bytes())).collect();
        let parents: Vec<&Committed> = stack.last().into_iter().collect();
        let committed = fixture.commit_on(&parents, &format!("c{index}"), &files);
        stack.push(committed);
    }
    fixture.check_out(&[stack.last().expect("a case has a commit")]);
    fixture.check_out_to_disk();
    for path in paths {
        let file = fixture.root().join(path);
        match case.working_copy.get(*path) {
            Some(text) => std::fs::write(&file, text).expect("a file is written"),
            None if file.exists() => std::fs::remove_file(&file).expect("a file is removed"),
            None => {}
        }
    }
    fixture
        .open()
        .with_config(ConfigEnv::default())
        .absorb(&[], None, &[], true, None)
        .expect("triad absorbs");
    let commits =
        stack.iter().map(|commit| held(fixture, &fixture.latest(commit), paths)).collect();
    (commits, held(fixture, &fixture.working_copy_commit(), paths))
}

/// Where triad departs from `case`'s `jj absorb`, in `fixture`; none when it matches.
fn departure(fixture: &Fixture, case: &Case) -> Option<String> {
    let paths = paths(case);
    let (commits, working_copy) = absorbed_by_triad(fixture, case, &paths);
    let input = expected(&case.working_copy, &paths);
    if !case.absorbed.conflicted.is_empty() {
        // jj wrote a conflict; triad keeps each hunk that would conflict in the working
        // copy, so no commit conflicts and the working copy holds what it held.
        let conflicted =
            commits.iter().chain([&working_copy]).flatten().any(|h| *h == Held::Conflict);
        return (conflicted || working_copy != input).then(|| {
            format!(
                "{}: jj conflicted {:?}; triad left {commits:?} and @ {working_copy:?}",
                case.id, case.absorbed.conflicted
            )
        });
    }
    let jj_commits: Vec<Vec<Held>> = case
        .absorbed
        .commits
        .iter()
        .map(|tree| expected(tree.as_ref().expect("a commit jj left resolved"), &paths))
        .collect();
    let jj_working_copy =
        expected(case.absorbed.working_copy.as_ref().expect("jj left @ resolved"), &paths);
    (commits != jj_commits || working_copy != jj_working_copy).then(|| {
        format!(
            "{} over {paths:?}:\n  jj    {jj_commits:?} @ {jj_working_copy:?}\n  triad {commits:?} @ {working_copy:?}",
            case.id
        )
    })
}

/// The corpus runs in this many tests, each case in the one its index picks, so each test
/// stays well inside nextest's limit under the full suite's load. A case costs about 180 ms
/// of a debug build's CPU, and threads gain nothing on a loaded machine.
const SHARDS: usize = 16;

/// The cases of shard `shard` that depart from `jj absorb`.
fn absorb_shard(shard: usize) {
    let corpus = read::<Corpus>(ABSORB);
    assert!(corpus.cases.len() >= SHARDS, "the corpus holds a case for each shard");
    let cases: Vec<&Case> = corpus.cases.iter().skip(shard).step_by(SHARDS).collect();
    for case in &cases {
        assert_eq!(case.absorbed.commits.len(), case.commits.len(), "{}", case.id);
        assert_eq!(
            case.absorbed.working_copy.is_none(),
            case.absorbed.conflicted.iter().any(|name| name == WORKING_COPY),
            "{}",
            case.id
        );
    }
    let fixture = Fixture::new();
    let departures: Vec<String> =
        cases.into_iter().filter_map(|case| departure(&fixture, case)).collect();
    assert!(
        departures.is_empty(),
        "{} cases of shard {shard} depart from jj absorb:\n{}",
        departures.len(),
        departures.join("\n")
    );
}

macro_rules! absorb_shards {
    ($($name:ident = $shard:literal),* $(,)?) => {$(
        /// `absorb` matches `jj absorb` on this shard of the corpus, except where jj writes
        /// a conflict.
        #[test]
        fn $name() {
            absorb_shard($shard);
        }
    )*};
}

absorb_shards!(
    absorb_matches_jj_0 = 0,
    absorb_matches_jj_1 = 1,
    absorb_matches_jj_2 = 2,
    absorb_matches_jj_3 = 3,
    absorb_matches_jj_4 = 4,
    absorb_matches_jj_5 = 5,
    absorb_matches_jj_6 = 6,
    absorb_matches_jj_7 = 7,
    absorb_matches_jj_8 = 8,
    absorb_matches_jj_9 = 9,
    absorb_matches_jj_10 = 10,
    absorb_matches_jj_11 = 11,
    absorb_matches_jj_12 = 12,
    absorb_matches_jj_13 = 13,
    absorb_matches_jj_14 = 14,
    absorb_matches_jj_15 = 15,
);

const RESOLVE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/oracle/jj-cli-resolve.json");
/// The name the corpus gives the root commit as a parent.
const ROOT: &str = "root";

#[derive(Deserialize)]
struct ResolveCorpus {
    jj: String,
    cases: Vec<ResolveCase>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Layout {
    Colocated,
    Internal,
}

/// A repository built step by step, then one conflict written into a commit: by
/// `jj resolve -r` with a tool that writes a fixed result, and by triad's `resolve -r`.
#[derive(Deserialize)]
struct ResolveCase {
    id: String,
    layout: Layout,
    /// Each commit in order: its parents' trees merged, `files` written over them.
    commits: Vec<Step>,
    /// Bookmarks and tags, each with the commit it names.
    bookmarks: BTreeMap<String, String>,
    tags: BTreeMap<String, String>,
    /// A new working-copy commit on `parents`, with `files` written to disk and not yet
    /// snapshotted.
    working_copy: Step,
    resolve: Resolve,
    /// Where triad leaves the repository otherwise than jj, on purpose.
    #[serde(default)]
    departure: Option<Departure>,
    after: After,
}

/// triad's departures from `jj resolve -r`.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum Departure {
    /// triad refuses an immutable commit before it snapshots the working copy, so the refusal
    /// writes no operation (`DESIGN.md` §I1); jj snapshots first.
    RefusedBeforeSnapshot,
    /// triad writes nothing for a result that reads back as the conflict the commit holds
    /// (spec §7.5.1); jj writes an operation.
    WritesNothing,
}

impl ResolveCase {
    /// jj's result as triad must leave the repository.
    fn expected(&self) -> After {
        let mut after = self.after.clone();
        match self.departure {
            None => {}
            Some(Departure::WritesNothing) => after.operations = 0,
            Some(Departure::RefusedBeforeSnapshot) => {
                after.operations = 0;
                // The working-copy commit keeps its parent's tree: nothing snapshotted.
                let [parent] = self.working_copy.parents.as_slice() else {
                    panic!("{}: one parent", self.id);
                };
                let tree = after.commits[parent].tree.clone();
                after.commits.get_mut(WORKING_COPY).expect("a working copy").tree = tree;
            }
        }
        after
    }
}

#[derive(Deserialize)]
struct Step {
    #[serde(default)]
    name: String,
    parents: Vec<String>,
    files: Tree,
}

#[derive(Deserialize)]
struct Resolve {
    /// A commit's name, or `@`.
    commit: String,
    path: String,
    result: String,
    marker_length: usize,
}

/// The repository after the write.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
struct After {
    refused: bool,
    /// The operations the write added, its snapshot included.
    operations: usize,
    /// Each visible commit by name, `@` for the working copy's.
    commits: BTreeMap<String, CommitAfter>,
    /// The working copy's files on disk, conflict markers without their labels.
    disk: Tree,
    /// The commit git's `HEAD` names; none in an internal repository.
    git_head: Option<String>,
    /// git's branches and tags, each with the commit it names.
    git_refs: Tree,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
struct CommitAfter {
    parents: Vec<String>,
    /// Each path's text, a conflict materialised in jj's `snapshot` style without labels.
    tree: Tree,
}

/// The files under `dir`, `.jj` and `.git` aside, by path relative to `root`, conflict
/// markers without their labels.
fn disk(root: &camino::Utf8Path, dir: &camino::Utf8Path, files: &mut Tree) {
    for entry in dir.read_dir_utf8().expect("the directory reads") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        if [".jj", ".git"].contains(&entry.file_name()) {
            continue;
        }
        if path.is_dir() {
            disk(root, path, files);
        } else {
            let relative = path.strip_prefix(root).expect("under the root");
            let text = std::fs::read_to_string(path).expect("a text file");
            files.insert(relative.as_str().replace('\\', "/"), without_conflict_labels(&text));
        }
    }
}

/// `fixture` as the corpus records a repository, `operations` the count the write added.
fn after(fixture: &Fixture, refused: bool, operations: usize) -> After {
    let visible = fixture.visible_commits();
    let mut names: BTreeMap<String, String> = visible
        .iter()
        .map(|v| {
            let name = if v.working_copy { WORKING_COPY.to_owned() } else { v.description.clone() };
            (v.commit.commit.clone(), name)
        })
        .collect();
    names.insert("0".repeat(40), ROOT.to_owned());
    let name = |id: &String| names.get(id).cloned().unwrap_or_else(|| format!("unknown {id}"));
    let commits = visible
        .iter()
        .map(|v| {
            let tree = fixture
                .tree_text(&v.commit)
                .into_iter()
                .map(|(path, text)| (path, without_conflict_labels(&text)))
                .collect();
            (
                name(&v.commit.commit),
                CommitAfter { parents: v.parents.iter().map(name).collect(), tree },
            )
        })
        .collect();
    let mut files = Tree::new();
    disk(fixture.root(), fixture.root(), &mut files);
    After {
        refused,
        operations,
        commits,
        disk: files,
        git_head: fixture.git_head().as_ref().map(name),
        git_refs: fixture.git_refs().into_iter().map(|(r, id)| (r, name(&id))).collect(),
    }
}

/// Build `case` through jj-lib, write its result with triad, and give the repository after.
fn resolved_by_triad(case: &ResolveCase) -> After {
    let fixture = match case.layout {
        Layout::Colocated => Fixture::colocated(),
        Layout::Internal => Fixture::internal(),
    };
    let mut made: BTreeMap<&str, Committed> = BTreeMap::new();
    for step in &case.commits {
        let files: Vec<(&str, &[u8])> =
            step.files.iter().map(|(path, text)| (path.as_str(), text.as_bytes())).collect();
        let parents: Vec<&Committed> = step.parents.iter().map(|p| &made[p.as_str()]).collect();
        let committed = if parents.is_empty() {
            fixture.commit_on(&[], &step.name, &files)
        } else {
            fixture.commit_merged(&parents, &step.name, &files)
        };
        made.insert(&step.name, committed);
    }
    for (bookmark, commit) in &case.bookmarks {
        fixture.set_bookmark(bookmark, &made[commit.as_str()]);
    }
    for (tag, commit) in &case.tags {
        fixture.set_tag(tag, &made[commit.as_str()]);
    }
    let parents: Vec<&Committed> =
        case.working_copy.parents.iter().map(|p| &made[p.as_str()]).collect();
    fixture.check_out(&parents);
    fixture.check_out_to_disk();
    for (path, text) in &case.working_copy.files {
        std::fs::write(fixture.root().join(path), text).expect("a file is written");
    }
    let target = if case.resolve.commit == WORKING_COPY {
        fixture.working_copy_commit()
    } else {
        made[case.resolve.commit.as_str()].clone()
    };
    let before = fixture.operation_count();
    let written = fixture.open().with_config(ConfigEnv::default()).write_conflict(
        &vcs::CommitId::new(target.commit),
        camino::Utf8Path::new(&case.resolve.path),
        case.resolve.result.as_bytes(),
        case.resolve.marker_length,
        false,
    );
    after(&fixture, written.is_err(), fixture.operation_count() - before)
}

/// The resolve corpus runs in this many tests, as the absorb corpus does ([`SHARDS`]): one
/// test of all nine cases takes 18 s under the full suite's load.
const RESOLVE_SHARDS: usize = 3;

/// The cases of resolve shard `shard` that depart from `jj resolve -r`.
fn resolve_shard(shard: usize) {
    let corpus = read::<ResolveCorpus>(RESOLVE);
    assert!(corpus.cases.len() >= RESOLVE_SHARDS, "the corpus holds a case for each shard");
    let departures: Vec<String> = corpus
        .cases
        .iter()
        .skip(shard)
        .step_by(RESOLVE_SHARDS)
        .filter_map(|case| {
            let triad = resolved_by_triad(case);
            let expected = case.expected();
            (triad != expected)
                .then(|| format!("{}:\n  jj    {expected:?}\n  triad {triad:?}", case.id))
        })
        .collect();
    assert!(departures.is_empty(), "{}", departures.join("\n"));
}

macro_rules! resolve_shards {
    ($($name:ident = $shard:literal),* $(,)?) => {$(
        /// `resolve -r` leaves the repository as `jj resolve -r` does on this shard of the
        /// corpus, outside triad's marked departures.
        #[test]
        fn $name() {
            resolve_shard($shard);
        }
    )*};
}

resolve_shards!(resolve_matches_jj_0 = 0, resolve_matches_jj_1 = 1, resolve_matches_jj_2 = 2,);
