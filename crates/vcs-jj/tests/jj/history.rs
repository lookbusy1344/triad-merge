//! `history` and `working_copy_at` on jj: a file's versions in the working-copy commit
//! across the operation log (spec §7.5.2).

use vcs::{Repository as _, VcsError};
use vcs_jj::config::ConfigEnv;
use vcs_jj::testing::Fixture;
use vcs_jj::{JjError, JjRepository, SHORT_OPERATION, VersionState};

fn open(fixture: &Fixture) -> JjRepository {
    fixture.open().with_config(ConfigEnv::default())
}

/// Write `bytes` to `a.txt` on disk and snapshot it; the new head operation's ID.
fn snapshot(fixture: &Fixture, bytes: Option<&[u8]>) -> String {
    let file = fixture.root().join("a.txt");
    match bytes {
        Some(bytes) => std::fs::write(&file, bytes).expect("a.txt is written"),
        None => std::fs::remove_file(&file).expect("a.txt is removed"),
    }
    open(fixture).snapshot().expect("the snapshot");
    fixture.operation_ids()[0].clone()
}

fn summary(versions: &[vcs_jj::FileVersion]) -> Vec<(String, VersionState)> {
    versions.iter().map(|v| (v.operation.clone(), v.state)).collect()
}

#[test]
fn each_snapshot_of_a_change_is_a_version_newest_first() {
    for fixture in [Fixture::colocated(), Fixture::internal()] {
        fixture.check_out_to_disk();
        let one = snapshot(&fixture, Some(b"1\n"));
        let two = snapshot(&fixture, Some(b"2\n"));
        let three = snapshot(&fixture, Some(b"3\n"));
        let versions = open(&fixture).history("a.txt".into(), None).expect("the history");
        let first_absent = versions.last().expect("versions").operation.clone();
        assert_eq!(
            summary(&versions),
            [
                (three, VersionState::File),
                (two, VersionState::File),
                (one, VersionState::File),
                (first_absent, VersionState::Absent),
            ]
        );
        // The root operation has no working copy; the one after it, which adds the
        // workspace, is where the absent file appeared.
        let ids = fixture.operation_ids();
        assert_eq!(versions[3].operation, ids[ids.len() - 2]);
        assert_eq!(versions[0].description, "snapshot working copy");
    }
}

#[test]
fn operations_that_keep_the_version_fold_into_the_one_where_it_appeared() {
    let fixture = Fixture::new();
    fixture.check_out_to_disk();
    let one = snapshot(&fixture, Some(b"1\n"));
    fixture.commit_on(&[], "unrelated", &[("b.txt", b"b\n")]);
    fixture.commit_on(&[], "another", &[("c.txt", b"c\n")]);
    let versions = open(&fixture).history("a.txt".into(), None).expect("the history");
    assert_eq!(versions.len(), 2, "{versions:?}");
    assert_eq!(
        (versions[0].operation.as_str(), versions[0].state),
        (one.as_str(), VersionState::File)
    );
}

#[test]
fn a_deleted_file_shows_absent_between_its_versions() {
    let fixture = Fixture::new();
    fixture.check_out_to_disk();
    let one = snapshot(&fixture, Some(b"1\n"));
    let gone = snapshot(&fixture, None);
    let back = snapshot(&fixture, Some(b"1\n"));
    let versions = open(&fixture).history("a.txt".into(), None).expect("the history");
    assert_eq!(
        summary(&versions[..3]),
        [(back, VersionState::File), (gone, VersionState::Absent), (one, VersionState::File)]
    );
}

#[test]
fn an_edit_on_disk_is_snapshotted_into_the_newest_version() {
    let fixture = Fixture::new();
    fixture.check_out_to_disk();
    snapshot(&fixture, Some(b"1\n"));
    std::fs::write(fixture.root().join("a.txt"), b"2\n").expect("a.txt is written");
    let versions = open(&fixture).history("a.txt".into(), None).expect("the history");
    assert_eq!(versions[0].operation, fixture.operation_ids()[0]);
    assert_eq!(versions.len(), 3, "{versions:?}");
}

#[test]
fn the_working_copy_at_an_operation_is_its_commit_then() {
    let fixture = Fixture::new();
    fixture.check_out_to_disk();
    let one = snapshot(&fixture, Some(b"1\n"));
    let commit_then = fixture.working_copy_id();
    snapshot(&fixture, Some(b"2\n"));
    assert_ne!(fixture.working_copy_id(), commit_then);
    let (short, commit) =
        open(&fixture).working_copy_at(&one[..8]).expect("the operation resolves");
    assert_eq!(short, one[..SHORT_OPERATION]);
    assert_eq!(commit.as_str(), commit_then);
    let (_, now) = open(&fixture).working_copy_at("@").expect("the head");
    assert_eq!(now.as_str(), fixture.working_copy_id());
}

#[test]
fn an_unknown_operation_is_refused() {
    let fixture = Fixture::new();
    match open(&fixture).working_copy_at("0123456789ab") {
        Err(VcsError::Backend { source, .. }) => assert!(
            matches!(source.downcast_ref::<JjError>(), Some(JjError::UnknownOperation { op, .. }) if op == "0123456789ab"),
            "{source}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_limit_keeps_the_newest_versions_each_named_where_it_appeared() {
    let fixture = Fixture::new();
    fixture.check_out_to_disk();
    let one = snapshot(&fixture, Some(b"1\n"));
    let two = snapshot(&fixture, Some(b"2\n"));
    // An operation that keeps the newest version: the version still appeared at `two`.
    fixture.commit_on(&[], "unrelated", &[("b.txt", b"b\n")]);
    let history = |limit| open(&fixture).history("a.txt".into(), Some(limit)).expect("the history");
    assert_eq!(
        summary(&history(2)),
        [(two.clone(), VersionState::File), (one, VersionState::File)]
    );
    assert_eq!(summary(&history(1)), [(two, VersionState::File)]);
    assert_eq!(summary(&history(0)), []);
}
