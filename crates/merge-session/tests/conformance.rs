//! The VCS conformance suite (spec §8.8, DESIGN.md §A7.1): one set of checks, written against the
//! `vcs` trait, run on git, colocated jj and internal jj repositories found through
//! `native::discover`.

#![expect(clippy::panic, reason = "test helpers outside `#[test]` panic on a failed check")]

use camino::{Utf8Path, Utf8PathBuf};
use merge_session::native;
use merge_session::repository::RunRepository;
use vcs::testing::Entry;
use vcs::{Annotation, CommitId, RepoKind, Repository, RevSpec, TermContent, Timestamp, VcsError};
use vcs_jj::config::ConfigEnv;

/// jj shows change IDs with at least this many characters.
const SHORT_ID: usize = 8;
/// git shortens commit IDs to at least this many characters (`core.abbrev`).
const GIT_SHORT_ID: usize = 7;
/// The jj fixture's commit timestamp, 2001-02-03T04:05:06+00:00, in seconds since the epoch.
const JJ_FIXTURE_SECONDS: i64 = 981_173_106;

enum Fixture {
    // Both boxed: their sizes differ by platform, and either may be the larger.
    Git(Box<vcs_git::testing::Fixture>),
    Jj(Box<vcs_jj::testing::Fixture>),
}

impl Fixture {
    fn root(&self) -> &Utf8Path {
        match self {
            Self::Git(fixture) => fixture.root(),
            Self::Jj(fixture) => fixture.root(),
        }
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Utf8PathBuf {
        match self {
            Self::Git(fixture) => fixture.write(path, bytes),
            Self::Jj(fixture) => fixture.write(path, bytes),
        }
    }

    fn kind(&self) -> RepoKind {
        match self {
            Self::Git(_) => RepoKind::Git,
            Self::Jj(_) => RepoKind::Jj,
        }
    }

    /// Commit `files` as the whole tree, leaving the working copy clean on top of it. Returns
    /// the title the parent's `Term` should give: the branch for git, the change ID and
    /// summary for jj.
    fn commit(&self, message: &str, files: &[(&str, &[u8])]) -> String {
        match self {
            Self::Git(fixture) => {
                fixture.commit(message, files);
                fixture.stage(files);
                "main".to_owned()
            }
            Self::Jj(fixture) => {
                let committed = fixture.commit(message, files);
                format!("{} {message}", &committed.change_id[..SHORT_ID])
            }
        }
    }

    /// The commit `back` generations before the latest, in the backend's syntax.
    fn rev(&self, back: usize) -> RevSpec {
        RevSpec::Expr(self.expr(back))
    }

    fn expr(&self, back: usize) -> String {
        match self {
            Self::Git(_) => format!("HEAD~{back}"),
            Self::Jj(_) => format!("@-{}", "-".repeat(back)),
        }
    }

    /// Commit `files` as the whole tree, as [`Fixture::commit`] does. Returns the full commit
    /// ID in hex, and for jj the change ID as jj shows it.
    fn commit_ids(&self, message: &str, files: &[(&str, &[u8])]) -> (String, Option<String>) {
        match self {
            Self::Git(fixture) => (fixture.commit(message, files).to_string(), None),
            Self::Jj(fixture) => {
                let committed = fixture.commit(message, files);
                let change = committed.change_id[..SHORT_ID].to_owned();
                (committed.commit, Some(change))
            }
        }
    }

    /// Commit `base` as `one`, then `left` on it as the latest commit's first parent and
    /// `right` on it beside, then their merge `merged` as the latest commit. Returns the full
    /// IDs of `one`, `left` and `right`.
    fn fork_and_merge(&self, path: &str, [base, left, right, merged]: [&[u8]; 4]) -> [String; 3] {
        match self {
            Self::Git(fixture) => {
                let one = fixture.commit("one", &[(path, base)]);
                let left = fixture.commit("left", &[(path, left)]);
                let right = fixture.commit_on(&[one], "right", &[(path, right)]);
                fixture.merge(&[left, right], "merge", &[(path, merged)]);
                [one, left, right].map(|id| id.to_string())
            }
            Self::Jj(fixture) => {
                let one = fixture.commit("one", &[(path, base)]);
                let left = fixture.commit("left", &[(path, left)]);
                let right = fixture.commit_on(&[&one], "right", &[(path, right)]);
                let merge = fixture.commit_on(&[&left, &right], "merge", &[(path, merged)]);
                fixture.check_out(&[&merge]);
                [one, left, right].map(|committed| committed.commit)
            }
        }
    }

    /// A merge stopped on `conflicts`, each a file's base, left and right (`None`: absent),
    /// with `clean` the same in every term. git: `HEAD` holds the left side and the index
    /// the stages; jj: the working copy is a merge of the left and right commits.
    fn stop_on_conflicts(&self, clean: &[(&str, &[u8])], conflicts: &[(&str, [Option<Entry>; 3])]) {
        let term = |n: usize| -> Vec<(&str, Entry)> {
            Entry::files(clean)
                .into_iter()
                .chain(
                    conflicts
                        .iter()
                        .filter_map(|&(path, terms)| terms[n].map(|entry| (path, entry))),
                )
                .collect()
        };
        let [base, left, right] = [0, 1, 2].map(term);
        match self {
            Self::Git(fixture) => {
                fixture.commit_entries("base", &base);
                fixture.commit_entries("left", &left);
                fixture.stage_conflicts(clean, conflicts);
            }
            Self::Jj(fixture) => {
                let base = fixture.commit_entries_on(&[], "base", &base);
                let left = fixture.commit_entries_on(&[&base], "left", &left);
                let right = fixture.commit_entries_on(&[&base], "right", &right);
                fixture.check_out(&[&left, &right]);
            }
        }
    }

    /// [`Fixture::stop_on_conflicts`], with jj's working copy written to disk as jj leaves
    /// it, so a snapshot finds it fresh.
    fn stop_on_disk(&self, clean: &[(&str, &[u8])], conflicts: &[(&str, [Option<Entry>; 3])]) {
        self.stop_on_conflicts(clean, conflicts);
        if let Self::Jj(fixture) = self {
            fixture.check_out_to_disk();
        }
    }

    /// A criss-cross: `one`, its children `b1` and `c1`, then two merges of them, `b2` and
    /// `c2`. Returns the full IDs of `b1`, `c1`, `b2` and `c2`.
    fn criss_cross(&self) -> [String; 4] {
        let file = |bytes: &'static [u8]| [("a.txt", bytes)];
        match self {
            Self::Git(fixture) => {
                let one = fixture.commit("one", &file(b"a\n"));
                let b1 = fixture.commit_on(&[one], "b1", &file(b"b\n"));
                let c1 = fixture.commit_on(&[one], "c1", &file(b"c\n"));
                let b2 = fixture.commit_on(&[b1, c1], "b2", &file(b"b\n"));
                let c2 = fixture.commit_on(&[c1, b1], "c2", &file(b"c\n"));
                [b1, c1, b2, c2].map(|id| id.to_string())
            }
            Self::Jj(fixture) => {
                let one = fixture.commit_on(&[], "one", &file(b"a\n"));
                let b1 = fixture.commit_on(&[&one], "b1", &file(b"b\n"));
                let c1 = fixture.commit_on(&[&one], "c1", &file(b"c\n"));
                let b2 = fixture.commit_on(&[&b1, &c1], "b2", &file(b"b\n"));
                let c2 = fixture.commit_on(&[&c1, &b1], "c2", &file(b"c\n"));
                [b1, c1, b2, c2].map(|committed| committed.commit)
            }
        }
    }

    /// The fixture's commit time.
    fn time(&self) -> Timestamp {
        let seconds = match self {
            Self::Git(_) => 0,
            Self::Jj(_) => JJ_FIXTURE_SECONDS,
        };
        Timestamp { seconds, offset_seconds: 0 }
    }

    fn open(&self) -> Box<dyn Repository> {
        native::discover(self.root()).expect("the repository is found")
    }

    /// The repository as `resolve` opens it to write, with jj's default config.
    fn open_run(&self) -> RunRepository {
        RunRepository::find([self.root().as_std_path()]).with_jj_config(ConfigEnv::default())
    }
}

fn read(repo: &dyn Repository, path: &str, rev: &RevSpec) -> Option<Vec<u8>> {
    repo.file_at(Utf8Path::new(path), rev).expect("the read succeeds")
}

fn discovery_climbs_from_a_subdirectory(fixture: &Fixture) {
    fixture.commit("one", &[("src/deep/a.txt", b"a\n")]);
    let deep = fixture.write("src/deep/a.txt", b"a\n");
    let repo =
        native::discover(deep.parent().expect("a directory")).expect("the repository is found");
    assert_eq!(repo.kind(), fixture.kind());
    assert_eq!(
        repo.root(),
        Utf8PathBuf::from_path_buf(dunce::canonicalize(fixture.root()).expect("the root exists"))
            .expect("UTF-8")
    );
}

fn a_missing_path_or_a_directory_is_none(fixture: &Fixture) {
    fixture.commit("one", &[("src/a.txt", b"a\n")]);
    let repo = fixture.open();
    let latest = fixture.rev(0);
    assert_eq!(read(repo.as_ref(), "missing.txt", &latest), None);
    assert_eq!(read(repo.as_ref(), "src/missing.txt", &latest), None);
    assert_eq!(read(repo.as_ref(), "src", &latest), None);
}

fn binary_bytes_come_back_unchanged(fixture: &Fixture) {
    let bytes: &[u8] = &[0, 0xff, b'\r', b'\n', 0x80, 0];
    fixture.commit("one", &[("blob.bin", bytes)]);
    assert_eq!(read(fixture.open().as_ref(), "blob.bin", &fixture.rev(0)).as_deref(), Some(bytes));
}

fn a_renamed_file_is_found_only_under_its_name_at_each_revision(fixture: &Fixture) {
    fixture.commit("one", &[("old.txt", b"same\n")]);
    fixture.commit("rename", &[("new.txt", b"same\n")]);
    let repo = fixture.open();
    let (latest, previous) = (fixture.rev(0), fixture.rev(1));
    assert_eq!(read(repo.as_ref(), "new.txt", &latest).as_deref(), Some(&b"same\n"[..]));
    assert_eq!(read(repo.as_ref(), "old.txt", &latest), None);
    assert_eq!(read(repo.as_ref(), "new.txt", &previous), None);
    assert_eq!(read(repo.as_ref(), "old.txt", &previous).as_deref(), Some(&b"same\n"[..]));
}

fn the_parent_holds_the_last_commit_and_is_titled_by_the_vcs(fixture: &Fixture) {
    fixture.commit("first", &[("a.txt", b"one\n")]);
    let title = fixture.commit("second", &[("a.txt", b"two\n")]);
    let repo = fixture.open();
    let parent = repo.default_parent(Utf8Path::new("a.txt")).expect("the parent is found");
    assert_eq!(parent.term.title().as_deref(), Some(title.as_str()));
    assert_eq!(read(repo.as_ref(), "a.txt", &parent.rev).as_deref(), Some(&b"two\n"[..]));
}

fn an_unknown_revision_is_named(fixture: &Fixture) {
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let nope = RevSpec::Expr("nope".to_owned());
    match fixture.open().file_at(Utf8Path::new("a.txt"), &nope) {
        Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, "nope"),
        other => panic!("expected UnknownRevision, got {other:?}"),
    }
}

fn a_commit_reads_by_its_id_alone(fixture: &Fixture) {
    let (first, _) = fixture.commit_ids("one", &[("a.txt", b"one\n")]);
    let (second, _) = fixture.commit_ids("two", &[("a.txt", b"two\n")]);
    let repo = fixture.open();
    let at =
        |id: &str| read(repo.as_ref(), "a.txt", &RevSpec::Commit(CommitId::new(id.to_owned())));
    assert_eq!(at(&first).as_deref(), Some(&b"one\n"[..]));
    assert_eq!(at(&second).as_deref(), Some(&b"two\n"[..]));
    // A revision expression or a prefix is no commit ID, though the VCS would resolve it.
    for id in [MISSING_COMMIT, &fixture.expr(0), "HEAD", "@", &first[..SHORT_ID]] {
        let rev = RevSpec::Commit(CommitId::new(id.to_owned()));
        match repo.file_at(Utf8Path::new("a.txt"), &rev) {
            Err(VcsError::UnknownRevision(named)) => assert_eq!(named, id),
            other => panic!("expected UnknownRevision for {id}, got {other:?}"),
        }
    }
}

fn a_path_outside_the_repository_is_refused(fixture: &Fixture) {
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let repo = fixture.open();
    let latest = commit_at(repo.as_ref(), fixture, 0);
    for outside in ["../a.txt", "src/../../a.txt", "src/../a.txt", "./a.txt", "/a.txt"] {
        let path = Utf8Path::new(outside);
        let refused = |result: Result<(), VcsError>, call: &str| match result {
            Err(VcsError::OutsideRepository(named)) => assert_eq!(named, outside, "{call}"),
            other => panic!("{call} of {outside}: expected OutsideRepository, got {other:?}"),
        };
        refused(repo.file_at(path, &fixture.rev(0)).map(drop), "file_at");
        refused(repo.annotate(path, &latest).map(drop), "annotate");
        refused(repo.read_conflict(path).map(drop), "read_conflict");
        refused(repo.recorded(path).map(drop), "recorded");
    }
}

fn a_path_is_read_by_its_components(fixture: &Fixture) {
    fixture.commit("one", &[("src/a.txt", b"a\n")]);
    let repo = fixture.open();
    let latest = fixture.rev(0);
    for path in ["src/a.txt", "src/./a.txt", "src//a.txt"] {
        assert_eq!(read(repo.as_ref(), path, &latest).as_deref(), Some(&b"a\n"[..]), "{path}");
    }
    for root in [".", ""] {
        assert_eq!(read(repo.as_ref(), root, &latest), None, "{root:?}");
    }
}

fn a_new_repository_has_no_conflicts_and_an_empty_parent(fixture: &Fixture) {
    let repo = fixture.open();
    assert_eq!(repo.conflicts().expect("the conflicts"), [] as [&str; 0]);
    let parent = repo.default_parent(Utf8Path::new("a.txt")).expect("the parent");
    assert_eq!(read(repo.as_ref(), "a.txt", &parent.rev), None);
}

/// The full ID of the commit `fixture.expr(back)` names.
fn commit_at(repo: &dyn Repository, fixture: &Fixture, back: usize) -> CommitId {
    let commits = repo.commits(&fixture.expr(back)).expect("the revision resolves");
    let [commit] = <[CommitId; 1]>::try_from(commits).expect("the revision names one commit");
    commit
}

/// `path` annotated at the commit `back` generations before the latest.
fn annotate(fixture: &Fixture, path: &str, back: usize) -> Option<Annotation> {
    let repo = fixture.open();
    let commit = commit_at(repo.as_ref(), fixture, back);
    repo.annotate(Utf8Path::new(path), &commit).expect("the annotation succeeds")
}

/// Each run's lines and the summary of its commit.
fn runs(annotation: &Annotation) -> Vec<(std::ops::Range<usize>, &str)> {
    annotation
        .runs
        .iter()
        .map(|run| {
            let summary = annotation.origins[run.origin].change.summary.as_deref();
            (run.lines.clone(), summary.expect("every fixture commit has a summary"))
        })
        .collect()
}

fn each_line_names_the_last_commit_that_changed_it(fixture: &Fixture) {
    fixture.commit("one", &[("a.txt", b"a\nb\nc\n")]);
    fixture.commit("two", &[("a.txt", b"a\nB\nc\nd\n")]);
    fixture.commit("three", &[("a.txt", b"a\nB!\nc\nd\n")]);
    let annotation = annotate(fixture, "a.txt", 0).expect("the file exists");
    assert_eq!(runs(&annotation), [(0..1, "one"), (1..2, "three"), (2..3, "one"), (3..4, "two")]);
    assert_eq!(annotation.origins.len(), 3);
}

fn an_earlier_revision_is_annotated_as_it_was(fixture: &Fixture) {
    fixture.commit("one", &[("a.txt", b"a\nb\n")]);
    fixture.commit("two", &[("a.txt", b"a\nB\n")]);
    let annotation = annotate(fixture, "a.txt", 1).expect("the file exists");
    assert_eq!(runs(&annotation), [(0..2, "one")]);
}

fn a_merge_names_the_commit_on_each_parent(fixture: &Fixture) {
    fixture.fork_and_merge("a.txt", [b"a\nb\nc\n", b"A\nb\nc\n", b"a\nb\nC\n", b"A\nb\nC\n"]);
    let annotation = annotate(fixture, "a.txt", 0).expect("the file exists");
    assert_eq!(runs(&annotation), [(0..1, "left"), (1..2, "one"), (2..3, "right")]);
}

fn an_origin_holds_the_commit_change_author_and_time(fixture: &Fixture) {
    let (commit, change_id) = fixture.commit_ids("one", &[("a.txt", b"a\n")]);
    let annotation = annotate(fixture, "a.txt", 0).expect("the file exists");
    let [origin] = annotation.origins.as_slice() else {
        panic!("one commit, got {:?}", annotation.origins);
    };
    let min_short = match fixture.kind() {
        RepoKind::Git => GIT_SHORT_ID,
        RepoKind::Jj => SHORT_ID,
    };
    assert!(
        origin.change.commit.len() >= min_short && commit.starts_with(&origin.change.commit),
        "{} shortens {commit}",
        origin.change.commit
    );
    assert_eq!(origin.change.change_id, change_id);
    assert_eq!(origin.change.summary.as_deref(), Some("one"));
    assert_eq!(origin.author, "triad");
    assert_eq!(origin.time, fixture.time());
}

fn annotating_a_missing_path_or_an_unknown_revision(fixture: &Fixture) {
    fixture.commit("one", &[("src/a.txt", b"a\n")]);
    assert_eq!(annotate(fixture, "missing.txt", 0), None);
    assert_eq!(annotate(fixture, "src", 0), None);
    let repo = fixture.open();
    // A revision expression is no commit ID, though the VCS would resolve it.
    for id in [MISSING_COMMIT, &fixture.expr(0), "HEAD", "@", "nope"] {
        match repo.annotate(Utf8Path::new("src/a.txt"), &CommitId::new(id.to_owned())) {
            Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, id),
            other => panic!("expected UnknownRevision for {id}, got {other:?}"),
        }
    }
}

fn an_empty_file_has_no_runs(fixture: &Fixture) {
    fixture.commit("one", &[("empty.txt", b"")]);
    let annotation = annotate(fixture, "empty.txt", 0).expect("the file exists");
    assert_eq!(annotation, Annotation::default());
}

/// A full commit ID, in hex, that no fixture holds.
const MISSING_COMMIT: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

fn a_commit_is_described_by_its_change_author_and_time(fixture: &Fixture) {
    let (first, first_change) = fixture.commit_ids("one", &[("a.txt", b"a\n")]);
    let (second, second_change) = fixture.commit_ids("two\n\nbody", &[("a.txt", b"b\n")]);
    let repo = fixture.open();
    for (commit, change_id, summary) in
        [(&first, first_change, "one"), (&second, second_change, "two")]
    {
        let origin = repo.describe(&CommitId::new(commit.clone())).expect("the commit exists");
        let min_short = match fixture.kind() {
            RepoKind::Git => GIT_SHORT_ID,
            RepoKind::Jj => SHORT_ID,
        };
        assert!(
            origin.change.commit.len() >= min_short && commit.starts_with(&origin.change.commit),
            "{} shortens {commit}",
            origin.change.commit
        );
        assert_eq!(origin.change.change_id, change_id);
        assert_eq!(origin.change.summary.as_deref(), Some(summary));
        assert_eq!(origin.author, "triad");
        assert_eq!(origin.time, fixture.time());
    }
}

fn describing_a_missing_commit_names_it(fixture: &Fixture) {
    fixture.commit("one", &[("a.txt", b"a\n")]);
    let repo = fixture.open();
    // A revision expression is no commit ID, though the VCS would resolve it.
    for id in [MISSING_COMMIT, &fixture.expr(0), "HEAD", "@"] {
        match repo.describe(&CommitId::new(id.to_owned())) {
            Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, id),
            other => panic!("expected UnknownRevision for {id}, got {other:?}"),
        }
    }
}

fn a_revision_names_its_commits_by_full_id(fixture: &Fixture) {
    let (first, _) = fixture.commit_ids("one", &[("a.txt", b"a\n")]);
    let (second, _) = fixture.commit_ids("two", &[("a.txt", b"b\n")]);
    let repo = fixture.open();
    assert_eq!(repo.commits(&fixture.expr(0)).expect("latest"), [CommitId::new(second)]);
    assert_eq!(repo.commits(&fixture.expr(1)).expect("previous"), [CommitId::new(first.clone())]);
    assert_eq!(repo.commits(&first).expect("a full ID"), [CommitId::new(first)]);
    match repo.commits("nope") {
        Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, "nope"),
        other => panic!("expected UnknownRevision, got {other:?}"),
    }
}

fn the_merge_base_is_where_two_lines_fork(fixture: &Fixture) {
    let [one, left, right] =
        fixture.fork_and_merge("a.txt", [b"a\nb\nc\n", b"A\nb\nc\n", b"a\nb\nC\n", b"A\nb\nC\n"]);
    let repo = fixture.open();
    let base = |a: &str, b: &str| {
        repo.merge_base(&CommitId::new(a.to_owned()), &CommitId::new(b.to_owned()))
            .expect("the merge base is found")
            .map(CommitId::into_string)
    };
    assert_eq!(base(&left, &right), Some(one.clone()));
    assert_eq!(base(&right, &left), Some(one.clone()));
    assert_eq!(base(&one, &left), Some(one.clone()), "an ancestor is the base");
    assert_eq!(base(&left, &left), Some(left.clone()));
    let left = CommitId::new(left);
    for id in [MISSING_COMMIT, &fixture.expr(0), "nope"] {
        let named = CommitId::new(id.to_owned());
        for (one, two) in [(&left, &named), (&named, &left)] {
            match repo.merge_base(one, two) {
                Err(VcsError::UnknownRevision(rev)) => assert_eq!(rev, id),
                other => panic!("expected UnknownRevision for {id}, got {other:?}"),
            }
        }
    }
}

fn a_criss_cross_has_no_single_merge_base(fixture: &Fixture) {
    let [b1, c1, b2, c2] = fixture.criss_cross().map(CommitId::new);
    let repo = fixture.open();
    assert_eq!(repo.merge_base(&b2, &c2).expect("the bases are found"), None);
    assert_eq!(
        repo.merge_base(&b1, &c2).expect("the base is found"),
        Some(b1.clone()),
        "b1 is an ancestor of c2"
    );
    assert_eq!(repo.merge_base(&c1, &b2).expect("the base is found"), Some(c1.clone()));
}

fn conflicts(repo: &dyn Repository) -> Vec<String> {
    repo.conflicts().expect("the conflicts are read").into_iter().map(String::from).collect()
}

const BASE: &[u8] = b"a\n";
const LEFT: &[u8] = b"left\n";
const RIGHT: &[u8] = b"right\n";
/// A file both sides changed.
const BOTH_CHANGED: [Option<Entry>; 3] =
    [Some(Entry::File(BASE)), Some(Entry::File(LEFT)), Some(Entry::File(RIGHT))];

/// Plain files for the terms present.
fn files(terms: [Option<&[u8]>; 3]) -> [Option<Entry<'_>>; 3] {
    terms.map(|bytes| bytes.map(Entry::File))
}

fn a_clean_working_copy_has_no_conflicts(fixture: &Fixture) {
    fixture.commit("one", &[("a.txt", BASE)]);
    assert_eq!(conflicts(fixture.open().as_ref()), Vec::<String>::new());
}

fn a_conflicted_file_among_clean_ones_is_listed(fixture: &Fixture) {
    fixture.stop_on_conflicts(&[("clean.txt", b"c\n")], &[("a.txt", BOTH_CHANGED)]);
    assert_eq!(conflicts(fixture.open().as_ref()), ["a.txt"]);
}

fn conflicts_come_in_path_order(fixture: &Fixture) {
    fixture.stop_on_conflicts(
        &[],
        &[
            ("z.txt", BOTH_CHANGED),
            ("a.txt", BOTH_CHANGED),
            ("dir/sub/c.txt", BOTH_CHANGED),
            ("a/b.txt", BOTH_CHANGED),
        ],
    );
    // Component by component: the directory `a` sorts before the file `a.txt`.
    assert_eq!(conflicts(fixture.open().as_ref()), ["a/b.txt", "a.txt", "dir/sub/c.txt", "z.txt"]);
}

fn a_file_deleted_on_one_side_and_changed_on_the_other_is_listed(fixture: &Fixture) {
    fixture.stop_on_conflicts(
        &[],
        &[
            ("left-changed.txt", files([Some(BASE), Some(LEFT), None])),
            ("right-changed.txt", files([Some(BASE), None, Some(RIGHT)])),
        ],
    );
    assert_eq!(conflicts(fixture.open().as_ref()), ["left-changed.txt", "right-changed.txt"]);
}

fn a_file_added_on_both_sides_is_listed(fixture: &Fixture) {
    fixture.stop_on_conflicts(
        &[("clean.txt", BASE)],
        &[("new.txt", files([None, Some(LEFT), Some(RIGHT)]))],
    );
    assert_eq!(conflicts(fixture.open().as_ref()), ["new.txt"]);
}

/// The bases' and the sides' contents of the conflict at `path`, if any.
fn read_contents(
    repo: &dyn Repository,
    path: &str,
) -> Option<(Vec<TermContent>, Vec<TermContent>)> {
    let conflict = repo.read_conflict(Utf8Path::new(path)).expect("the conflict is read")?;
    assert_eq!(conflict.path, path);
    let contents = |terms: Vec<vcs::ConflictTerm>| terms.into_iter().map(|t| t.content).collect();
    Some((contents(conflict.bases), contents(conflict.sides)))
}

fn file(bytes: &[u8]) -> TermContent {
    TermContent::File { bytes: bytes.to_vec(), executable: false }
}

fn a_conflict_reads_its_base_and_both_sides(fixture: &Fixture) {
    fixture.stop_on_conflicts(&[("clean.txt", b"c\n")], &[("a.txt", BOTH_CHANGED)]);
    assert_eq!(
        read_contents(fixture.open().as_ref(), "a.txt"),
        Some((vec![file(BASE)], vec![file(LEFT), file(RIGHT)]))
    );
}

fn a_file_added_on_both_sides_has_an_absent_base(fixture: &Fixture) {
    fixture.stop_on_conflicts(&[], &[("new.txt", files([None, Some(LEFT), Some(RIGHT)]))]);
    assert_eq!(
        read_contents(fixture.open().as_ref(), "new.txt"),
        Some((vec![TermContent::Absent], vec![file(LEFT), file(RIGHT)]))
    );
}

fn a_side_that_deleted_the_file_is_absent(fixture: &Fixture) {
    fixture.stop_on_conflicts(
        &[],
        &[
            ("left-changed.txt", files([Some(BASE), Some(LEFT), None])),
            ("dir/right-changed.txt", files([Some(BASE), None, Some(RIGHT)])),
        ],
    );
    let repo = fixture.open();
    assert_eq!(
        read_contents(repo.as_ref(), "left-changed.txt"),
        Some((vec![file(BASE)], vec![file(LEFT), TermContent::Absent]))
    );
    assert_eq!(
        read_contents(repo.as_ref(), "dir/right-changed.txt"),
        Some((vec![file(BASE)], vec![TermContent::Absent, file(RIGHT)]))
    );
}

fn executable(bytes: &[u8]) -> TermContent {
    TermContent::File { bytes: bytes.to_vec(), executable: true }
}

fn each_term_reads_its_own_executable_bit(fixture: &Fixture) {
    fixture.stop_on_conflicts(
        &[],
        &[
            (
                "made-executable.sh",
                [Some(Entry::File(BASE)), Some(Entry::Executable(LEFT)), Some(Entry::File(RIGHT))],
            ),
            (
                "made-plain.sh",
                [
                    Some(Entry::Executable(BASE)),
                    Some(Entry::Executable(LEFT)),
                    Some(Entry::File(RIGHT)),
                ],
            ),
        ],
    );
    let repo = fixture.open();
    assert_eq!(
        read_contents(repo.as_ref(), "made-executable.sh"),
        Some((vec![file(BASE)], vec![executable(LEFT), file(RIGHT)]))
    );
    assert_eq!(
        read_contents(repo.as_ref(), "made-plain.sh"),
        Some((vec![executable(BASE)], vec![executable(LEFT), file(RIGHT)]))
    );
}

fn a_symlink_or_submodule_term_is_another_kind(fixture: &Fixture) {
    use vcs::EntryKind::{Submodule, Symlink};
    fixture.stop_on_conflicts(
        &[],
        &[
            (
                "now-link",
                [Some(Entry::File(BASE)), Some(Entry::Symlink("target")), Some(Entry::File(RIGHT))],
            ),
            (
                "was-link",
                [Some(Entry::Symlink("target")), Some(Entry::File(LEFT)), Some(Entry::File(RIGHT))],
            ),
            ("sub", [None, Some(Entry::File(LEFT)), Some(Entry::Submodule)]),
        ],
    );
    let repo = fixture.open();
    assert_eq!(conflicts(repo.as_ref()), ["now-link", "sub", "was-link"]);
    assert_eq!(
        read_contents(repo.as_ref(), "now-link"),
        Some((vec![file(BASE)], vec![TermContent::Other(Symlink), file(RIGHT)]))
    );
    assert_eq!(
        read_contents(repo.as_ref(), "was-link"),
        Some((vec![TermContent::Other(Symlink)], vec![file(LEFT), file(RIGHT)]))
    );
    assert_eq!(
        read_contents(repo.as_ref(), "sub"),
        Some((vec![TermContent::Absent], vec![file(LEFT), TermContent::Other(Submodule)]))
    );
}

fn binary_terms_come_back_unchanged(fixture: &Fixture) {
    let terms: [&[u8]; 3] = [b"\0base\xff", b"\0left\xfe", b"\0right\xfd"];
    fixture.stop_on_conflicts(&[], &[("a.bin", files(terms.map(Some)))]);
    assert_eq!(
        read_contents(fixture.open().as_ref(), "a.bin"),
        Some((vec![file(terms[0])], vec![file(terms[1]), file(terms[2])]))
    );
}

fn a_clean_or_missing_path_is_no_conflict(fixture: &Fixture) {
    fixture.stop_on_conflicts(&[("clean.txt", b"c\n")], &[("a.txt", BOTH_CHANGED)]);
    let repo = fixture.open();
    assert_eq!(read_contents(repo.as_ref(), "clean.txt"), None);
    assert_eq!(read_contents(repo.as_ref(), "missing.txt"), None);
    assert_eq!(read_contents(repo.as_ref(), "dir/missing.txt"), None);
}

fn record(run: &mut RunRepository, path: &str, resolved: bool) {
    run.require_mut()
        .expect("the repository opens")
        .record_save(Utf8Path::new(path), resolved)
        .expect("the save is recorded");
}

fn a_resolved_save_clears_its_conflict_and_leaves_the_others(fixture: &Fixture) {
    fixture.stop_on_disk(
        &[("clean.txt", b"c\n")],
        &[("a.txt", BOTH_CHANGED), ("b.txt", BOTH_CHANGED)],
    );
    let mut run = fixture.open_run();
    run.require_mut().expect("the repository opens").snapshot().expect("the snapshot is taken");
    fixture.write("a.txt", b"merged\n");
    record(&mut run, "a.txt", true);
    assert_eq!(conflicts(fixture.open().as_ref()), ["b.txt"]);
    assert_eq!(read_contents(fixture.open().as_ref(), "a.txt"), None);
}

fn a_save_with_markers_leaves_its_conflict(fixture: &Fixture) {
    fixture.stop_on_disk(&[], &[("a.txt", BOTH_CHANGED)]);
    let mut run = fixture.open_run();
    run.require_mut().expect("the repository opens").snapshot().expect("the snapshot is taken");
    let markers: &[u8] = match fixture {
        Fixture::Git(_) => b"<<<<<<< ours\nleft\n=======\nright\n>>>>>>> theirs\n",
        Fixture::Jj(_) => &std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads"),
    };
    fixture.write("a.txt", markers);
    record(&mut run, "a.txt", false);
    assert_eq!(conflicts(fixture.open().as_ref()), ["a.txt"]);
    assert_eq!(
        read_contents(fixture.open().as_ref(), "a.txt"),
        Some((vec![file(BASE)], vec![file(LEFT), file(RIGHT)]))
    );
}

macro_rules! conformance {
    ($backend:ident, $fixture:expr, [$($check:ident),* $(,)?]) => {
        mod $backend {
            $(
                #[test]
                fn $check() {
                    super::$check(&$fixture);
                }
            )*
        }
    };
}

macro_rules! suite {
    ($($backend:ident => $fixture:expr),* $(,)?) => {
        $(
            conformance!($backend, $fixture, [
                discovery_climbs_from_a_subdirectory,
                a_missing_path_or_a_directory_is_none,
                binary_bytes_come_back_unchanged,
                a_renamed_file_is_found_only_under_its_name_at_each_revision,
                the_parent_holds_the_last_commit_and_is_titled_by_the_vcs,
                an_unknown_revision_is_named,
                a_commit_reads_by_its_id_alone,
                a_path_outside_the_repository_is_refused,
                a_path_is_read_by_its_components,
                a_new_repository_has_no_conflicts_and_an_empty_parent,
                each_line_names_the_last_commit_that_changed_it,
                an_earlier_revision_is_annotated_as_it_was,
                a_merge_names_the_commit_on_each_parent,
                an_origin_holds_the_commit_change_author_and_time,
                annotating_a_missing_path_or_an_unknown_revision,
                an_empty_file_has_no_runs,
                a_revision_names_its_commits_by_full_id,
                a_commit_is_described_by_its_change_author_and_time,
                describing_a_missing_commit_names_it,
                the_merge_base_is_where_two_lines_fork,
                a_criss_cross_has_no_single_merge_base,
                a_clean_working_copy_has_no_conflicts,
                a_conflicted_file_among_clean_ones_is_listed,
                conflicts_come_in_path_order,
                a_file_deleted_on_one_side_and_changed_on_the_other_is_listed,
                a_file_added_on_both_sides_is_listed,
                a_conflict_reads_its_base_and_both_sides,
                a_file_added_on_both_sides_has_an_absent_base,
                a_side_that_deleted_the_file_is_absent,
                each_term_reads_its_own_executable_bit,
                a_symlink_or_submodule_term_is_another_kind,
                binary_terms_come_back_unchanged,
                a_clean_or_missing_path_is_no_conflict,
                a_resolved_save_clears_its_conflict_and_leaves_the_others,
                a_save_with_markers_leaves_its_conflict,
            ]);
        )*
    };
}

suite! {
    git => super::Fixture::Git(Box::new(vcs_git::testing::Fixture::new())),
    jj_colocated => super::Fixture::Jj(Box::new(vcs_jj::testing::Fixture::colocated())),
    jj_internal => super::Fixture::Jj(Box::new(vcs_jj::testing::Fixture::internal())),
}
