//! Reports of the jj-native commands (spec §7.5).

use std::fmt::Write as _;

use camino::Utf8Path;
use serde_json::json;
use vcs::IdKind;
use vcs_jj::{
    Absorbed, DescendantEffect, Divergence, FileVersion, Rewrite, SHORT_OPERATION, StackCommit,
    VersionState,
};

use crate::ExitCode;

/// What `stack` prints when no commit is conflicted.
pub const NO_CONFLICTED_COMMITS: &str = "no conflicted commits\n";

/// `stack`'s report: each commit, parents first, with its change, commit and summary on one
/// line and its conflicted paths below; or as JSON. 0 when nothing is conflicted, else 1.
///
/// # Errors
///
/// [`serde_json::Error`] when the JSON cannot be written.
pub fn stack_report(
    commits: &[StackCommit],
    json: bool,
) -> Result<(String, ExitCode), serde_json::Error> {
    let code = if commits.is_empty() { ExitCode::Resolved } else { ExitCode::Conflicts };
    if json {
        let listed: Vec<_> = commits
            .iter()
            .map(|commit| {
                json!({
                    "commit": commit.id.as_str(),
                    "change_id": commit.change.change_id,
                    "summary": commit.change.summary,
                    "paths": commit.paths.iter().map(|p| json!({
                        "path": p.path.as_str(),
                        "inherited": p.inherited,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        return Ok((format!("{}\n", serde_json::to_string_pretty(&listed)?), code));
    }
    if commits.is_empty() {
        return Ok((NO_CONFLICTED_COMMITS.to_owned(), code));
    }
    let mut text = String::new();
    for commit in commits {
        let heading = [
            Some(commit.change.id(IdKind::Change)),
            Some(commit.change.id(IdKind::Commit)),
            commit.change.summary.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        line(&mut text, format_args!("{heading}"));
        for p in &commit.paths {
            let mark = if p.inherited { " (inherited)" } else { "" };
            line(&mut text, format_args!("  {}{mark}", p.path));
        }
    }
    Ok((text, code))
}

/// What a write into a commit did, or on a dry run would do: one line for the commit, then
/// one per rebased descendant with its conflicted paths before and after.
#[must_use]
pub fn rewrite_report(path: &Utf8Path, rewrite: &Rewrite, dry_run: bool) -> String {
    let change = rewrite.target.id(IdKind::Change);
    let verb = match (rewrite.resolved, dry_run) {
        (true, false) => "resolved",
        (false, false) => "saved with conflicts",
        (true, true) => "would resolve",
        (false, true) => "would save with conflicts",
    };
    let heading = if rewrite.written || dry_run {
        format!("{verb} {path} in {change}\n")
    } else {
        format!("{path} in {change} is unchanged\n")
    };
    let mut text = heading;
    descendant_lines(&mut text, &rewrite.descendants);
    text
}

/// One line per descendant: its change and its conflicted paths before and after.
fn descendant_lines(text: &mut String, descendants: &[DescendantEffect]) {
    for d in descendants {
        let change = d.change.id(IdKind::Change);
        line(text, format_args!("  {change}: {} -> {}", list(&d.before), list(&d.after)));
    }
}

/// What `resolve --pair` prints when the pair's merge keeps conflicts, and nothing is
/// written.
#[must_use]
pub fn pair_unresolved(path: &Utf8Path, pair: usize) -> String {
    format!("{path} is unchanged: pair {pair} has conflicts\n")
}

/// `paths` joined by commas, or `none`.
fn list(paths: &[camino::Utf8PathBuf]) -> String {
    if paths.is_empty() {
        "none".to_owned()
    } else {
        paths
            .iter()
            .map(camino::Utf8PathBuf::as_path)
            .map(Utf8Path::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// `diverge`'s report: the kept and abandoned commits and the bases, the merged commit's
/// conflicts, a description changed two ways, then the rebased descendants; or as JSON. 0
/// when the merged commit is resolved, else 1.
///
/// # Errors
///
/// [`serde_json::Error`] when the JSON cannot be written.
pub fn divergence_report(
    divergence: &Divergence,
    dry_run: bool,
    json: bool,
) -> Result<(String, ExitCode), serde_json::Error> {
    let rewrite = &divergence.rewrite;
    let code = if rewrite.resolved { ExitCode::Resolved } else { ExitCode::Conflicts };
    let ids = |changes: &[vcs::ChangeInfo]| -> Vec<String> {
        changes.iter().map(|c| c.id(IdKind::Commit).to_owned()).collect()
    };
    let kept = rewrite.target.id(IdKind::Commit);
    if json {
        let descendants: Vec<_> = rewrite
            .descendants
            .iter()
            .map(|d| {
                json!({
                    "change_id": d.change.change_id,
                    "before": d.before.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
                    "after": d.after.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect();
        let value = json!({
            "change_id": rewrite.target.change_id,
            "kept": kept,
            "abandoned": ids(&divergence.abandoned),
            "bases": ids(&divergence.bases),
            "conflicted": divergence.conflicted.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "description_kept": divergence.description_kept,
            "written": rewrite.written,
            "descendants": descendants,
        });
        return Ok((format!("{}\n", serde_json::to_string_pretty(&value)?), code));
    }
    let verb = if dry_run { "would merge" } else { "merged" };
    let mut text = String::new();
    line(
        &mut text,
        format_args!(
            "{verb} change {}: kept {kept}, abandoned {} over {}",
            rewrite.target.id(IdKind::Change),
            ids(&divergence.abandoned).join(", "),
            ids(&divergence.bases).join(", "),
        ),
    );
    if !divergence.conflicted.is_empty() {
        line(&mut text, format_args!("  conflicted: {}", list(&divergence.conflicted)));
    }
    if divergence.description_kept {
        line(
            &mut text,
            format_args!("  the copies changed the description two ways; kept {kept}'s"),
        );
    }
    descendant_lines(&mut text, &rewrite.descendants);
    Ok((text, code))
}

/// `history`'s report: one line per version, newest first: the operation's short ID, its
/// time and description, and a mark when the file is not a file there; or as JSON.
///
/// # Errors
///
/// [`serde_json::Error`] when the JSON cannot be written.
pub fn history_report(
    path: &Utf8Path,
    versions: &[FileVersion],
    json: bool,
) -> Result<String, serde_json::Error> {
    let state = |state| match state {
        VersionState::File => "file",
        VersionState::Conflict => "conflict",
        VersionState::Absent => "absent",
        VersionState::Other => "other",
    };
    if json {
        let listed: Vec<_> = versions
            .iter()
            .map(|v| {
                json!({
                    "operation": v.operation,
                    "time": v.time,
                    "description": v.description,
                    "commit": v.commit.as_str(),
                    "state": state(v.state),
                })
            })
            .collect();
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({"path": path.as_str(), "versions": listed}))?
        ));
    }
    let mut text = String::new();
    for v in versions {
        let short: String = v.operation.chars().take(SHORT_OPERATION).collect();
        let mark = match v.state {
            VersionState::File => String::new(),
            other => format!(" ({})", state(other)),
        };
        line(&mut text, format_args!("{short} {} {}{mark}", v.time, v.description));
    }
    Ok(text)
}

/// `absorb`'s report: a listing first names the operation it read; then one line per hunk,
/// its number, path, line ranges and the commit it goes into or `stays`; then each skipped
/// path; with `apply`, a closing count and the rebased descendants. Or as JSON.
///
/// # Errors
///
/// [`serde_json::Error`] when the JSON cannot be written.
pub fn absorb_report(
    absorbed: &Absorbed,
    apply: bool,
    json: bool,
) -> Result<String, serde_json::Error> {
    let range = |r: &std::ops::Range<usize>| [r.start, r.end];
    if json {
        let hunks: Vec<_> = absorbed
            .hunks
            .iter()
            .map(|h| {
                json!({
                    "hunk": h.number,
                    "path": h.path.as_str(),
                    "left": range(&h.left),
                    "right": range(&h.right),
                    "target": h.target.as_ref().map(|t| json!({"commit": t.commit, "change_id": t.change_id})),
                })
            })
            .collect();
        let skipped: Vec<_> = absorbed
            .skipped
            .iter()
            .map(|(path, reason)| json!({"path": path.as_str(), "reason": reason}))
            .collect();
        let value = json!({
            "operation": absorbed.operation,
            "hunks": hunks,
            "skipped": skipped,
            "written": absorbed.rewrite.written,
        });
        return Ok(format!("{}\n", serde_json::to_string_pretty(&value)?));
    }
    let mut text = String::new();
    if !apply {
        let short: String = absorbed.operation.chars().take(SHORT_OPERATION).collect();
        line(&mut text, format_args!("at operation {short}"));
    }
    for h in &absorbed.hunks {
        let goes = h.target.as_ref().map_or_else(
            || "stays".to_owned(),
            |t| format!("-> {} {}", t.id(IdKind::Change), t.id(IdKind::Commit)),
        );
        line(
            &mut text,
            format_args!(
                "{} {} left {}..{} right {}..{} {goes}",
                h.number, h.path, h.left.start, h.left.end, h.right.start, h.right.end
            ),
        );
    }
    for (path, reason) in &absorbed.skipped {
        line(&mut text, format_args!("skipped {path}: {reason}"));
    }
    if apply {
        let moved: Vec<&vcs::ChangeInfo> =
            absorbed.hunks.iter().filter_map(|h| h.target.as_ref()).collect();
        let mut commits: Vec<&str> = moved.iter().map(|t| t.commit.as_str()).collect();
        commits.sort_unstable();
        commits.dedup();
        line(
            &mut text,
            format_args!("absorbed {} hunks into {} commits", moved.len(), commits.len()),
        );
        descendant_lines(&mut text, &absorbed.rewrite.descendants);
    }
    Ok(text)
}

/// Append `args` and a newline to `text`.
fn line(text: &mut String, args: std::fmt::Arguments<'_>) {
    // Writing to a `String` cannot fail.
    let _ = writeln!(text, "{args}");
}

#[cfg(test)]
mod tests {
    use vcs::ChangeInfo;
    use vcs_jj::{DescendantEffect, StackPath};

    use super::*;

    fn change(change_id: &str, summary: Option<&str>) -> ChangeInfo {
        ChangeInfo {
            commit: format!("c-{change_id}"),
            change_id: Some(change_id.to_owned()),
            summary: summary.map(str::to_owned),
        }
    }

    fn listed() -> Vec<StackCommit> {
        vec![
            StackCommit {
                id: vcs::CommitId::new("aaaa".to_owned()),
                change: change("kkkk", Some("merge")),
                paths: vec![StackPath { path: "a.txt".into(), inherited: false }],
            },
            StackCommit {
                id: vcs::CommitId::new("bbbb".to_owned()),
                change: change("llll", None),
                paths: vec![
                    StackPath { path: "a.txt".into(), inherited: true },
                    StackPath { path: "b.txt".into(), inherited: false },
                ],
            },
        ]
    }

    #[test]
    fn the_stack_prints_each_commit_then_its_paths_and_exits_one() {
        let (text, code) = stack_report(&listed(), false).expect("a report");
        assert_eq!(text, "kkkk c-kkkk merge\n  a.txt\nllll c-llll\n  a.txt (inherited)\n  b.txt\n");
        assert_eq!(code, ExitCode::Conflicts);
    }

    #[test]
    fn an_empty_stack_says_so_and_exits_zero() {
        assert_eq!(
            stack_report(&[], false).expect("a report"),
            (NO_CONFLICTED_COMMITS.to_owned(), ExitCode::Resolved)
        );
        assert_eq!(
            stack_report(&[], true).expect("a report"),
            ("[]\n".to_owned(), ExitCode::Resolved)
        );
    }

    #[test]
    fn the_stack_as_json_names_full_commits_and_inherited_paths() {
        let (text, _) = stack_report(&listed(), true).expect("a report");
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(value[1]["commit"], "bbbb");
        assert_eq!(value[1]["change_id"], "llll");
        assert_eq!(value[1]["summary"], serde_json::Value::Null);
        assert_eq!(value[1]["paths"][0], json!({"path": "a.txt", "inherited": true}));
    }

    fn rewrite(resolved: bool, written: bool) -> Rewrite {
        Rewrite {
            target: change("kkkk", Some("merge")),
            resolved,
            sides: Some(if resolved { 1 } else { 2 }),
            descendants: vec![DescendantEffect {
                change: change("llll", None),
                before: vec!["a.txt".into(), "b.txt".into()],
                after: vec![],
            }],
            written,
        }
    }

    fn divergence(conflicted: bool, kept: bool) -> Divergence {
        Divergence {
            rewrite: rewrite(!conflicted, true),
            bases: vec![change("bbbb", Some("change"))],
            abandoned: vec![change("mmmm", None)],
            conflicted: if conflicted { vec!["a.txt".into()] } else { vec![] },
            description_kept: kept,
        }
    }

    #[test]
    fn a_divergence_names_the_kept_abandoned_and_base_commits() {
        let (text, code) =
            divergence_report(&divergence(false, false), false, false).expect("a report");
        assert_eq!(
            text,
            "merged change kkkk: kept c-kkkk, abandoned c-mmmm over c-bbbb\n  llll: a.txt, b.txt -> none\n"
        );
        assert_eq!(code, ExitCode::Resolved);
        let (text, code) =
            divergence_report(&divergence(true, true), true, false).expect("a report");
        assert_eq!(
            text.lines().take(3).collect::<Vec<_>>(),
            [
                "would merge change kkkk: kept c-kkkk, abandoned c-mmmm over c-bbbb",
                "  conflicted: a.txt",
                "  the copies changed the description two ways; kept c-kkkk's",
            ]
        );
        assert_eq!(code, ExitCode::Conflicts);
    }

    #[test]
    fn a_divergence_as_json_lists_each_part() {
        let (text, _) = divergence_report(&divergence(true, true), false, true).expect("a report");
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(value["kept"], "c-kkkk");
        assert_eq!(value["abandoned"], json!(["c-mmmm"]));
        assert_eq!(value["bases"], json!(["c-bbbb"]));
        assert_eq!(value["conflicted"], json!(["a.txt"]));
        assert_eq!(value["description_kept"], true);
        assert_eq!(value["descendants"][0]["before"], json!(["a.txt", "b.txt"]));
    }

    fn version(operation: &str, description: &str, state: VersionState) -> FileVersion {
        FileVersion {
            operation: operation.to_owned(),
            time: "2026-11-13T10:00:00+01:00".to_owned(),
            description: description.to_owned(),
            commit: vcs::CommitId::new("cccc".to_owned()),
            state,
        }
    }

    #[test]
    fn the_history_prints_each_version_with_its_short_operation_and_a_mark_off_a_file() {
        let versions = [
            version("0123456789abcdef", "snapshot working copy", VersionState::File),
            version("fedcba9876543210", "new empty commit", VersionState::Absent),
            version("00112233445566778899", "merge", VersionState::Conflict),
        ];
        assert_eq!(
            history_report(Utf8Path::new("a.txt"), &versions, false).expect("a report"),
            "0123456789ab 2026-11-13T10:00:00+01:00 snapshot working copy\n\
             fedcba987654 2026-11-13T10:00:00+01:00 new empty commit (absent)\n\
             001122334455 2026-11-13T10:00:00+01:00 merge (conflict)\n"
        );
        let text = history_report(Utf8Path::new("a.txt"), &versions, true).expect("a report");
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(value["path"], "a.txt");
        assert_eq!(value["versions"][1]["operation"], "fedcba9876543210");
        assert_eq!(value["versions"][1]["state"], "absent");
        assert_eq!(value["versions"][2]["commit"], "cccc");
    }

    fn absorbed() -> Absorbed {
        let hunk = |number, target| vcs_jj::AbsorbHunk {
            number,
            path: "a.txt".into(),
            left: 2..3,
            right: 2..4,
            target,
        };
        Absorbed {
            operation: "0123456789abcdef0123".to_owned(),
            hunks: vec![
                hunk(1, Some(change("kkkk", None))),
                hunk(2, None),
                hunk(3, Some(change("kkkk", None))),
            ],
            skipped: vec![("b.bin".into(), "conflicted in the parents".to_owned())],
            rewrite: rewrite(true, true),
        }
    }

    #[test]
    fn absorb_lists_each_hunk_with_its_commit_then_the_skipped_paths() {
        assert_eq!(
            absorb_report(&absorbed(), false, false).expect("a report"),
            "at operation 0123456789ab\n\
             1 a.txt left 2..3 right 2..4 -> kkkk c-kkkk\n\
             2 a.txt left 2..3 right 2..4 stays\n\
             3 a.txt left 2..3 right 2..4 -> kkkk c-kkkk\n\
             skipped b.bin: conflicted in the parents\n"
        );
        let applied = absorb_report(&absorbed(), true, false).expect("a report");
        assert_eq!(
            applied.lines().skip(4).collect::<Vec<_>>(),
            ["absorbed 2 hunks into 1 commits", "  llll: a.txt, b.txt -> none"]
        );
        let value: serde_json::Value =
            serde_json::from_str(&absorb_report(&absorbed(), false, true).expect("a report"))
                .expect("JSON");
        assert_eq!(value["hunks"][0]["left"], json!([2, 3]));
        assert_eq!(value["hunks"][0]["target"]["commit"], "c-kkkk");
        assert_eq!(value["hunks"][1]["target"], serde_json::Value::Null);
        assert_eq!(value["skipped"][0]["path"], "b.bin");
        assert_eq!(value["written"], true);
        assert_eq!(value["operation"], "0123456789abcdef0123");
    }

    #[test]
    fn a_rewrite_names_the_commit_then_each_descendant_s_conflicts_before_and_after() {
        let path = Utf8Path::new("a.txt");
        assert_eq!(
            rewrite_report(path, &rewrite(true, true), false),
            "resolved a.txt in kkkk\n  llll: a.txt, b.txt -> none\n"
        );
        assert_eq!(
            rewrite_report(path, &rewrite(false, true), false).lines().next(),
            Some("saved with conflicts a.txt in kkkk")
        );
        assert_eq!(
            rewrite_report(path, &rewrite(true, false), true).lines().next(),
            Some("would resolve a.txt in kkkk")
        );
        assert_eq!(
            rewrite_report(path, &rewrite(false, false), false).lines().next(),
            Some("a.txt in kkkk is unchanged")
        );
    }
}
