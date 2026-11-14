//! `stack` and `write_conflict` on jj: conflicts in commits other than the working copy's,
//! listed parents first and resolved in place (spec §7.5.1).

use camino::Utf8Path;
use vcs::{IdKind, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::{Committed, Fixture, Held};
use vcs_jj::{CommitEffect, JjRepository, StackPath};

const BASE: &[u8] = b"a\n";
const LEFT: &[u8] = b"left\n";
const RIGHT: &[u8] = b"right\n";
const MERGED: &[u8] = b"merged\n";
/// jj's shortest conflict markers.
const MARKER_LENGTH: usize = 7;
/// git's diff3 markers for the conflict of `LEFT` and `RIGHT` over `BASE`, as triad saves
/// an unresolved conflict.
const MARKERS: &[u8] = b"<<<<<<< side 1\nleft\n||||||| base\na\n=======\nright\n>>>>>>> side 2\n";

/// A stack: `merge` of `left` and `right` conflicts in `a.txt`; `child` adds `b.txt` on it,
/// and the working copy sits on `child`, checked out to disk.
struct Stack {
    fixture: Fixture,
    base: Committed,
    left: Committed,
    right: Committed,
    merge: Committed,
    child: Committed,
}

fn stack(fixture: Fixture) -> Stack {
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT)]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", RIGHT)]);
    let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
    let child = fixture.commit_merged(&[&merge], "child", &[("b.txt", b"b\n")]);
    fixture.check_out(&[&child]);
    fixture.check_out_to_disk();
    Stack { fixture, base, left, right, merge, child }
}

fn both_stores() -> [(Stack, &'static str); 2] {
    [(stack(Fixture::colocated()), "colocated"), (stack(Fixture::internal()), "internal")]
}

/// The repository with jj's defaults and no user config.
fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

fn id(commit: &Committed) -> vcs::CommitId {
    vcs::CommitId::new(commit.commit.clone())
}

fn path(name: &str, inherited: bool) -> StackPath {
    StackPath { path: name.into(), inherited }
}

fn a_txt() -> &'static Utf8Path {
    Utf8Path::new("a.txt")
}

/// Each descendant's change ID with its conflicted paths before and after.
fn effects(descendants: &[CommitEffect]) -> Vec<(String, Vec<String>, Vec<String>)> {
    let names = |paths: &[camino::Utf8PathBuf]| paths.iter().map(ToString::to_string).collect();
    descendants
        .iter()
        .map(|d| (d.change.id(IdKind::Change).to_owned(), names(&d.before), names(&d.after)))
        .collect()
}

fn short(change_id: &str, effect: &str) -> bool {
    change_id.starts_with(effect)
}

/// The one visible commit described `description`.
fn described(fixture: &Fixture, description: &str) -> Committed {
    let found: Vec<Committed> = fixture
        .visible_commits()
        .into_iter()
        .filter(|visible| visible.description == description)
        .map(|visible| visible.commit)
        .collect();
    match <[Committed; 1]>::try_from(found) {
        Ok([commit]) => commit,
        Err(found) => panic!("{} commits described {description}", found.len()),
    }
}

#[test]
fn the_stack_lists_conflicted_commits_parents_first_marking_inherited_paths() {
    for (Stack { fixture, merge, child, .. }, store) in both_stores() {
        let wc = fixture.working_copy_commit();
        for revset in [Some("conflicts()"), None] {
            let listed = open(&fixture).stack(revset).expect("the stack is listed");
            let ids: Vec<&str> = listed.iter().map(|c| c.id.as_str()).collect();
            assert_eq!(ids, [&merge.commit, &child.commit, &wc.commit], "{store} {revset:?}");
            assert_eq!(listed[0].paths, [path("a.txt", false)], "{store}");
            assert_eq!(listed[1].paths, [path("a.txt", true)], "{store}");
            assert_eq!(listed[2].paths, [path("a.txt", true)], "{store}");
            assert_eq!(listed[1].change.summary.as_deref(), Some("child"), "{store}");
        }
    }
}

#[test]
fn a_revset_without_conflicts_lists_nothing() {
    let Stack { fixture, left, .. } = stack(Fixture::new());
    let listed = open(&fixture).stack(Some(&left.commit)).expect("the stack is listed");
    assert_eq!(listed, []);
}

#[test]
fn a_resolution_in_an_ancestor_clears_the_inherited_conflicts_and_moves_the_working_copy() {
    for (Stack { fixture, merge, child, .. }, store) in both_stores() {
        let wc = fixture.working_copy_commit();
        let rewrite = open(&fixture)
            .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false)
            .expect("the resolution is written");
        assert!(rewrite.written && rewrite.resolved, "{store}: {rewrite:?}");
        assert!(short(&merge.change_id, rewrite.target.id(IdKind::Change)), "{store}");
        let effects = effects(&rewrite.rewritten);
        assert_eq!(effects.len(), 2, "{store}: {effects:?}");
        for ((change, before, after), expected) in effects.iter().zip([&child, &wc]) {
            assert!(short(&expected.change_id, change), "{store}: {change}");
            assert_eq!((before.as_slice(), after.as_slice()), (&["a.txt".to_owned()][..], &[][..]));
        }
        for commit in [&merge, &child, &wc] {
            let latest = fixture.latest(commit);
            assert_eq!(fixture.held_in(&latest, "a.txt"), Held::File(MERGED.to_vec()), "{store}");
        }
        assert_eq!(fixture.held_in(&fixture.latest(&child), "b.txt"), Held::File(b"b\n".to_vec()));
        let on_disk = std::fs::read(fixture.root().join("a.txt")).expect("a.txt reads");
        assert_eq!(on_disk, MERGED, "{store}: the working copy is checked out");
        let change = rewrite.target.id(IdKind::Change);
        assert_eq!(
            fixture.operations()[0].description,
            format!("resolve a.txt in {change} in triad"),
            "{store}"
        );
        // jj reads its working copy as fresh: a snapshot finds nothing to record.
        let operations = fixture.operation_count();
        vcs::Repository::snapshot(&mut open(&fixture)).expect("the snapshot runs");
        assert_eq!(fixture.operation_count(), operations, "{store}");
    }
}

#[test]
fn git_s_head_follows_the_rebased_parent_of_the_working_copy() {
    let Stack { fixture, merge, child, .. } = stack(Fixture::colocated());
    open(&fixture)
        .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false)
        .expect("the resolution is written");
    let git = gix::open(fixture.root()).expect("git's repository opens");
    let head = git.head_id().expect("git has a HEAD").to_string();
    assert_eq!(head, fixture.latest(&child).commit);
}

#[test]
fn markers_saved_back_unchanged_keep_the_conflict_and_write_no_operation() {
    for (Stack { fixture, merge, .. }, store) in both_stores() {
        let operations = fixture.operation_count();
        let rewrite = open(&fixture)
            .write_conflict(&id(&merge), a_txt(), MARKERS, MARKER_LENGTH, false)
            .expect("the markers are written");
        assert!(!rewrite.written && !rewrite.resolved, "{store}: {rewrite:?}");
        assert_eq!(rewrite.sides, Some(2), "{store}");
        assert_eq!(fixture.held_in(&fixture.latest(&merge), "a.txt"), Held::Conflict, "{store}");
        assert_eq!(fixture.operation_count(), operations, "{store}");
    }
}

#[test]
fn markers_with_one_side_edited_stay_a_conflict_with_the_edit() {
    let Stack { fixture, merge, .. } = stack(Fixture::new());
    let edited = String::from_utf8(MARKERS.to_vec()).expect("UTF-8").replace("left\n", "LEFT\n");
    let rewrite = open(&fixture)
        .write_conflict(&id(&merge), a_txt(), edited.as_bytes(), MARKER_LENGTH, false)
        .expect("the markers are written");
    assert!(rewrite.written && !rewrite.resolved, "{rewrite:?}");
    let conflict = open(&fixture)
        .conflict_at(&id(&fixture.latest(&merge)), a_txt())
        .expect("the conflict reads")
        .expect("a.txt stays conflicted");
    let sides: Vec<_> = conflict.sides.iter().map(|side| side.content.clone()).collect();
    let file = |bytes: &[u8]| vcs::TermContent::File { bytes: bytes.to_vec(), executable: false };
    assert_eq!(sides, [file(b"LEFT\n"), file(RIGHT)]);
}

#[test]
fn a_path_resolved_beside_another_conflict_reports_resolved() {
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE), ("c.txt", BASE)]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT), ("c.txt", LEFT)]);
    let right = fixture.commit_on(&[&base], "right", &[("a.txt", RIGHT), ("c.txt", RIGHT)]);
    let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
    let rewrite = open(&fixture)
        .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false)
        .expect("the resolution is written");
    assert!(rewrite.written && rewrite.resolved, "{rewrite:?}");
    assert_eq!(rewrite.sides, Some(1));
    assert_eq!(fixture.held_in(&fixture.latest(&merge), "c.txt"), Held::Conflict);
}

#[test]
fn an_immutable_commit_is_refused_before_anything_is_written() {
    let Stack { fixture, merge, .. } = stack(Fixture::new());
    fixture.set_tag("v1", &merge);
    // An edit on disk a snapshot would record.
    fixture.write("b.txt", b"edited\n");
    let operations = fixture.operation_count();
    let refused = open(&fixture).write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false);
    assert!(
        matches!(&refused, Err(VcsError::Immutable(commit)) if merge.commit.starts_with(commit.as_str())),
        "{refused:?}"
    );
    assert_eq!(fixture.held_in(&merge, "a.txt"), Held::Conflict);
    assert_eq!(fixture.operation_count(), operations);
}

#[test]
fn the_user_s_immutable_heads_replace_jj_s_default() {
    let Stack { fixture, merge, child, .. } = stack(Fixture::new());
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = dir.path().join("config.toml");
    let with = |toml: &str| {
        std::fs::write(&config, toml).expect("the config is written");
        let environment = std::collections::HashMap::from([(
            "JJ_CONFIG".to_owned(),
            config.to_str().expect("UTF-8").to_owned(),
        )]);
        fixture.open().with_config(ConfigEnv { environment, ..ConfigEnv::default() })
    };
    let toml = format!("[revset-aliases]\n\"immutable_heads()\" = \"{}\"\n", child.commit);
    let refused = with(&toml).write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false);
    assert!(matches!(refused, Err(VcsError::Immutable(_))), "{refused:?}");
    fixture.set_tag("v1", &merge);
    let toml = "[revset-aliases]\n\"immutable_heads()\" = \"none()\"\n";
    let rewrite = with(toml)
        .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false)
        .expect("a tag no longer makes the commit immutable");
    assert!(rewrite.written);
}

#[test]
fn mine_in_the_user_s_immutable_heads_names_the_user_s_commits() {
    // jj's documented setting: commits by others are immutable. The fixture's commits are
    // by triad@example.com.
    const OTHERS_IMMUTABLE: &str = "[revset-aliases]\n\"immutable_heads()\" = \
                                    \"builtin_immutable_heads() | (trunk().. & ~mine())\"\n";
    let Stack { fixture, merge, child, .. } = stack(Fixture::new());
    let wc = fixture.working_copy_commit();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config = dir.path().join("config.toml");
    let as_user = |email: &str| {
        let toml = format!("{OTHERS_IMMUTABLE}[user]\nemail = \"{email}\"\n");
        std::fs::write(&config, toml).expect("the config is written");
        let environment = std::collections::HashMap::from([(
            "JJ_CONFIG".to_owned(),
            config.to_str().expect("UTF-8").to_owned(),
        )]);
        fixture.open().with_config(ConfigEnv { environment, ..ConfigEnv::default() })
    };
    let refused = as_user("other@example.com").write_conflict(
        &id(&merge),
        a_txt(),
        MERGED,
        MARKER_LENGTH,
        true,
    );
    assert!(matches!(refused, Err(VcsError::Immutable(_))), "{refused:?}");
    let listed = as_user("triad@example.com").stack(None).expect("the stack lists");
    assert_eq!(
        listed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        [merge.commit.as_str(), child.commit.as_str(), wc.commit.as_str()]
    );
    let rewrite = as_user("triad@example.com")
        .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, true)
        .expect("the user's own commit is mutable");
    assert!(rewrite.resolved, "{rewrite:?}");
}

#[test]
fn a_path_resolved_in_the_commit_is_refused() {
    let Stack { fixture, left, .. } = stack(Fixture::new());
    let refused = open(&fixture).write_conflict(&id(&left), a_txt(), MERGED, MARKER_LENGTH, false);
    assert!(
        matches!(&refused, Err(VcsError::NotConflicted { path, .. }) if path == "a.txt"),
        "{refused:?}"
    );
}

#[test]
fn a_dry_run_reports_the_effect_and_writes_nothing() {
    let Stack { fixture, merge, child, .. } = stack(Fixture::new());
    let operations = fixture.operation_count();
    let rewrite = open(&fixture)
        .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, true)
        .expect("the dry run runs");
    assert!(!rewrite.written && rewrite.resolved, "{rewrite:?}");
    let effects = effects(&rewrite.rewritten);
    assert!(short(&child.change_id, &effects[0].0), "{effects:?}");
    assert_eq!(
        (effects[0].1.as_slice(), effects[0].2.as_slice()),
        (&["a.txt".to_owned()][..], &[][..])
    );
    assert_eq!(fixture.operation_count(), operations);
    assert_eq!(fixture.held_in(&merge, "a.txt"), Held::Conflict);
}

fn changed_refusal(result: Result<vcs_jj::Rewrite, VcsError>) -> String {
    match result {
        Err(VcsError::Backend { source, .. }) => match source.downcast_ref::<vcs_jj::JjError>() {
            Some(error @ vcs_jj::JjError::ConflictChanged { .. }) => error.to_string(),
            _ => panic!("expected ConflictChanged, got {source}"),
        },
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_conflict_changed_since_it_was_read_is_refused_and_nothing_is_written() {
    let Stack { fixture, merge, .. } = stack(Fixture::new());
    // Another writer edits one side of the conflict after triad read it from `merge`.
    let edited = String::from_utf8(MARKERS.to_vec()).expect("UTF-8").replace("left\n", "LEFT\n");
    open(&fixture)
        .write_conflict(&id(&merge), a_txt(), edited.as_bytes(), MARKER_LENGTH, false)
        .expect("the edit is written");
    let operations = fixture.operation_count();
    let refused = changed_refusal(open(&fixture).write_conflict(
        &id(&merge),
        a_txt(),
        MERGED,
        MARKER_LENGTH,
        false,
    ));
    let change = refused
        .strip_prefix("a.txt in ")
        .and_then(|rest| rest.strip_suffix(" changed since triad read it; nothing is written"))
        .unwrap_or_else(|| panic!("{refused}"));
    assert!(merge.change_id.starts_with(change), "{change} names {}", merge.change_id);
    assert_eq!(fixture.held_in(&fixture.latest(&merge), "a.txt"), Held::Conflict);
    assert_eq!(fixture.operation_count(), operations);
}

#[test]
fn a_conflict_resolved_since_it_was_read_is_refused() {
    let Stack { fixture, merge, .. } = stack(Fixture::new());
    fixture.amend(&merge, &[("a.txt", b"other\n")]);
    fixture.check_out_to_disk();
    let refused = changed_refusal(open(&fixture).write_conflict(
        &id(&merge),
        a_txt(),
        MERGED,
        MARKER_LENGTH,
        false,
    ));
    assert!(refused.starts_with("a.txt in "), "{refused}");
    assert_eq!(fixture.held_in(&fixture.latest(&merge), "a.txt"), Held::File(b"other\n".to_vec()));
}

#[test]
fn a_commit_rewritten_elsewhere_with_the_conflict_unchanged_takes_the_save() {
    let Stack { fixture, merge, .. } = stack(Fixture::new());
    fixture.edit(&merge);
    fixture.check_out_to_disk();
    // The snapshot before the write rewrites `merge` with this file; a.txt's conflict stays.
    fixture.write("c.txt", b"c\n");
    let rewrite = open(&fixture)
        .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false)
        .expect("the resolution is written");
    assert!(rewrite.written && rewrite.resolved, "{rewrite:?}");
    let latest = fixture.latest(&merge);
    assert_eq!(fixture.held_in(&latest, "a.txt"), Held::File(MERGED.to_vec()));
    assert_eq!(fixture.held_in(&latest, "c.txt"), Held::File(b"c\n".to_vec()));
}

#[test]
fn the_stack_snapshots_the_working_copy_first() {
    let Stack { fixture, merge, child, .. } = stack(Fixture::new());
    // Resolved on disk, with no jj command since: jj would list the working copy as clear.
    std::fs::write(fixture.root().join("a.txt"), MERGED).expect("a.txt is written");
    let listed = open(&fixture).stack(None).expect("the stack is listed");
    let ids: Vec<&str> = listed.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, [&merge.commit, &child.commit]);
}

#[test]
fn a_conflict_of_a_file_and_a_symlink_is_refused_as_not_a_conflict_of_files() {
    use vcs::testing::Entry;
    let fixture = Fixture::new();
    let base = fixture.commit_on(&[], "base", &[("a.txt", BASE)]);
    let left = fixture.commit_on(&[&base], "left", &[("a.txt", LEFT)]);
    let right =
        fixture.commit_entries_on(&[&base], "right", &[("a.txt", Entry::Symlink("elsewhere"))]);
    let merge = fixture.commit_merged(&[&left, &right], "merge", &[]);
    let result = open(&fixture).write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false);
    let message = match result {
        Err(VcsError::Backend { source, .. }) => source.to_string(),
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert_eq!(message, "a.txt is not a conflict of files");
}

#[test]
fn a_save_into_one_copy_of_a_divergent_change_rewrites_that_copy_alone() {
    for (Stack { fixture, left, right, merge, .. }, store) in both_stores() {
        let copy = fixture.divergent_copy(&merge, &[&left, &right], "merge copy");
        let rewrite = open(&fixture)
            .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, false)
            .expect("the resolution is written");
        assert!(rewrite.written && rewrite.resolved, "{store}: {rewrite:?}");
        let merged = Held::File(MERGED.to_vec());
        assert_eq!(fixture.held_in(&described(&fixture, "merge"), "a.txt"), merged, "{store}");
        assert_eq!(fixture.held_in(&described(&fixture, "child"), "a.txt"), merged, "{store}");
        let untouched = described(&fixture, "merge copy");
        assert_eq!(untouched.commit, copy.commit, "{store}: the other copy is not rewritten");
        assert_eq!(fixture.held_in(&untouched, "a.txt"), Held::Conflict, "{store}");
    }
}

#[test]
fn each_copy_of_a_divergent_descendant_reports_its_own_conflicts() {
    let Stack { fixture, base, merge, child, .. } = stack(Fixture::new());
    let wc = fixture.working_copy_commit();
    // The copy merges `merge` with a third side of a.txt: it stays conflicted after `merge`
    // resolves, while `child` resolves.
    let other = fixture.commit_on(&[&base], "other", &[("a.txt", b"other\n")]);
    fixture.divergent_copy(&child, &[&merge, &other], "child copy");
    let rewrite = open(&fixture)
        .write_conflict(&id(&merge), a_txt(), MERGED, MARKER_LENGTH, true)
        .expect("the dry run runs");
    let named = |change: &str| {
        if short(&child.change_id, change) {
            "child"
        } else if short(&wc.change_id, change) {
            "working copy"
        } else {
            panic!("{change} is neither child nor the working copy")
        }
    };
    let mut found: Vec<_> = effects(&rewrite.rewritten)
        .into_iter()
        .map(|(change, before, after)| (named(&change), before, after))
        .collect();
    found.sort();
    let a = || vec!["a.txt".to_owned()];
    assert_eq!(found, [("child", a(), vec![]), ("child", a(), a()), ("working copy", a(), vec![])]);
}
