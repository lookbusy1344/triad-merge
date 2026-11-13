//! Reports of the jj-native commands (spec §1.2, `docs/PLAN-jj-native.md`).

use std::fmt::Write as _;

use camino::Utf8Path;
use serde_json::json;
use vcs::IdKind;
use vcs_jj::{Rewrite, StackCommit};

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
    let list = |paths: &[camino::Utf8PathBuf]| {
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
    };
    let mut text = heading;
    for d in &rewrite.descendants {
        let change = d.change.id(IdKind::Change);
        line(&mut text, format_args!("  {change}: {} -> {}", list(&d.before), list(&d.after)));
    }
    text
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
            descendants: vec![DescendantEffect {
                change: change("llll", None),
                before: vec!["a.txt".into(), "b.txt".into()],
                after: vec![],
            }],
            written,
        }
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
