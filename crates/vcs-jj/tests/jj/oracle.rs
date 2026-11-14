//! triad against jj's own CLI (`DESIGN.md` §I1, §I5): the default revset aliases and
//! `absorb`, from corpora `script/gen-jj-cli-oracle` writes with the jj release whose jj-lib
//! `Cargo.lock` pins.

use std::collections::BTreeMap;

use serde::Deserialize;
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture, Held, locked_jj_lib_version};

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
