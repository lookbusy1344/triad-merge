//! Argument parsing, headless merge, `inspect` and `diff --inspect`, `install` and the exit
//! codes, over `merge-session`. A library with no GUI dependency.

pub mod args;
pub mod forward;
pub mod inspect;
pub mod install;
pub mod jj_native;
pub mod logging;

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use merge_config::settings::{self, Settings};
use merge_core::document::OutputEnding;
use merge_session::env::Env;
use merge_session::invocation::{Absorb, History, Merge, Resolve, SemanticsFlags, Stack, Window};
use merge_session::repository::RunRepository;
use merge_session::{
    EncodedOutput, SaveError, SessionError, choose_marker_style, choose_semantics, jj_config, load,
    replay, resolve,
};

pub use merge_session::{ExitCode, WindowForm};

use crate::args::{Batch, Invocation, JjBatch, ParseError, parse_with};

/// What the binary does next.
#[derive(Debug)]
pub enum Outcome {
    /// Finished: exit with this code.
    Exit(ExitCode),
    /// The invocation needs a window, which `merge-cli` does not link.
    Window(Box<WindowForm>),
}

impl Outcome {
    /// The code to exit with, when the run finished without a window.
    #[must_use]
    pub fn exit_code(&self) -> Option<ExitCode> {
        match self {
            Self::Exit(code) => Some(*code),
            Self::Window(_) => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("cannot write the report as JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Install(#[from] install::InstallError),
    #[error(transparent)]
    Resolve(#[from] resolve::ResolveError),
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error("cannot find the working directory")]
    NoWorkingDirectory,
    /// The result is written, but the repository refused it.
    #[error("{path}: {source}")]
    Rewrite { path: camino::Utf8PathBuf, source: vcs::VcsError },
}

/// [`choose_semantics`] and [`choose_marker_style`] for `merge`, set in its options, reading
/// the run's repository `repo`.
fn choose_for(
    merge: &mut Merge,
    settings: &Settings,
    cwd: Option<&Path>,
    repo: &mut RunRepository,
    env: &Env,
) -> Result<(), CliError> {
    let mode = choose_semantics(merge.semantics, settings, repo, Some(&merge.output), cwd, env)?;
    merge.options.semantics = mode.semantics;
    merge.options.same_change = mode.same_change;
    merge.options.hunk_level = mode.hunk_level;
    merge.options.marker_style = choose_marker_style(mode.semantics, settings, repo);
    Ok(())
}

/// Prefix of every error line on stderr.
const PROGRAM: &str = "triad-merge";

/// Run the command line, program name first, with the settings file at `settings`
/// ([`settings::path`]; none for the defaults). Help and version go to `stdout`; errors go
/// to `stderr` and give [`ExitCode::Error`].
pub fn run<I, T>(
    args: I,
    settings: Option<&Path>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Outcome
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let (settings, settings_errors) = settings
        .map_or_else(|| (Settings::default(), Vec::new()), |path| read_settings(path, stderr));
    let invocation = match parse_with(args, &settings) {
        Ok(invocation) => invocation,
        Err(ParseError::Clap(e)) if !e.use_stderr() => {
            return Outcome::Exit(report(stdout, &e.render().to_string(), ExitCode::Resolved));
        }
        Err(ParseError::Clap(e)) => {
            return Outcome::Exit(report(stderr, &e.render().to_string(), ExitCode::Error));
        }
        Err(ParseError::Usage(e)) => {
            return Outcome::Exit(report(stderr, &format!("{PROGRAM}: {e}\n"), ExitCode::Error));
        }
    };
    tracing::debug!(?invocation, "parsed the command line");
    let cwd = || {
        std::env::current_dir()
            .inspect_err(|error| {
                tracing::warn!(%error, "no working directory; repositories come from the files'");
            })
            .ok()
    };
    let env = Env::from_process();
    match invocation {
        Invocation::ShowConfig { defaults, docs } => {
            let shown = if defaults { &Settings::default() } else { &settings };
            let code =
                if settings_errors.is_empty() { ExitCode::Resolved } else { ExitCode::Error };
            Outcome::Exit(report(stdout, &settings::render(shown, docs), code))
        }
        Invocation::Window(window) => {
            window_form(window, settings, settings_errors, cwd(), &env, stdout, stderr)
        }
        Invocation::Batch(batch) => {
            let result = batch_form(batch, &settings, cwd().as_deref(), &env, stdout, stderr);
            Outcome::Exit(
                result.unwrap_or_else(|e| {
                    report(stderr, &format!("{PROGRAM}: {e}\n"), ExitCode::Error)
                }),
            )
        }
    }
}

/// Settle a window form's repository, and for a merge its semantics, before the window
/// opens. `resolve` first finds its target, and exits when there is none.
fn window_form(
    window: Window,
    settings: Settings,
    settings_errors: Vec<String>,
    cwd: Option<PathBuf>,
    env: &Env,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Outcome {
    let (window, repo) = match window {
        Window::Merge(mut merge) => {
            let mut repo = RunRepository::of_merge(cwd.as_deref(), Some(&merge.output));
            if let Err(error) = choose_for(&mut merge, &settings, cwd.as_deref(), &mut repo, env) {
                return Outcome::Exit(report(
                    stderr,
                    &format!("{PROGRAM}: {error}\n"),
                    ExitCode::Error,
                ));
            }
            (Window::Merge(merge), repo)
        }
        Window::Resolve(mut resolve) => {
            let result = cwd.as_deref().ok_or(CliError::NoWorkingDirectory).and_then(|cwd| {
                let (targets, repo) = resolve_prepare(&mut resolve, &settings, cwd, env, stdout)?;
                Ok(targets.first().cloned().map(|first| (first, repo)))
            });
            match result {
                Ok(Some((first, repo))) => {
                    resolve.target = Some(first);
                    (Window::Resolve(resolve), repo)
                }
                Ok(None) => return Outcome::Exit(ExitCode::Resolved),
                Err(error) => {
                    return Outcome::Exit(report(
                        stderr,
                        &format!("{PROGRAM}: {error}\n"),
                        ExitCode::Error,
                    ));
                }
            }
        }
        Window::Diff(diff) => {
            let repo = RunRepository::of_diff_tool(cwd.as_deref(), &diff.right);
            (Window::Diff(diff), repo)
        }
        Window::DiffNative { path, rev, op, diff, title } => {
            let repo = RunRepository::of_file(&path);
            (Window::DiffNative { path, rev, op, diff, title }, repo)
        }
        Window::DiffEvolog(evolog) => (Window::DiffEvolog(evolog), RunRepository::none()),
        Window::OpenDialog => (Window::OpenDialog, RunRepository::none()),
        Window::History(history) => {
            let result = cwd
                .as_deref()
                .ok_or(CliError::NoWorkingDirectory)
                .and_then(|cwd| history_prepare(&history, cwd, env));
            match result {
                Ok(repo) => (Window::History(history), repo),
                Err(error) => {
                    return Outcome::Exit(report(
                        stderr,
                        &format!("{PROGRAM}: {error}\n"),
                        ExitCode::Error,
                    ));
                }
            }
        }
        Window::Absorb(absorb) => {
            let result = cwd
                .as_deref()
                .ok_or(CliError::NoWorkingDirectory)
                .and_then(|cwd| absorb_prepare(&absorb, cwd, env));
            match result {
                Ok(Some(repo)) => (Window::Absorb(absorb), repo),
                Ok(None) => return Outcome::Exit(ExitCode::Resolved),
                Err(error) => {
                    return Outcome::Exit(report(
                        stderr,
                        &format!("{PROGRAM}: {error}\n"),
                        ExitCode::Error,
                    ));
                }
            }
        }
        Window::Stack(mut stack) => {
            let result = cwd
                .as_deref()
                .ok_or(CliError::NoWorkingDirectory)
                .and_then(|cwd| stack_prepare(&mut stack, &settings, cwd, env));
            match result {
                Ok(Some(repo)) => (Window::Stack(stack), repo),
                Ok(None) => return Outcome::Exit(ExitCode::Resolved),
                Err(error) => {
                    return Outcome::Exit(report(
                        stderr,
                        &format!("{PROGRAM}: {error}\n"),
                        ExitCode::Error,
                    ));
                }
            }
        }
    };
    Outcome::Window(Box::new(WindowForm { window, settings, settings_errors, cwd, repo }))
}

/// Run a form that needs no window, with its report on `stdout`.
///
/// # Errors
///
/// [`CliError`] when the form fails.
fn batch_form(
    batch: Batch,
    settings: &Settings,
    cwd: Option<&Path>,
    env: &Env,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<ExitCode, CliError> {
    match batch {
        Batch::Headless { mut merge, commands } => {
            let mut repo = RunRepository::of_merge(cwd, Some(&merge.output));
            choose_for(&mut merge, settings, cwd, &mut repo, env)?;
            headless(&merge, commands.as_deref(), settings.line_ending)
        }
        Batch::Resolve(mut resolve) => {
            let cwd = cwd.ok_or(CliError::NoWorkingDirectory)?;
            let (targets, mut repo) = resolve_prepare(&mut resolve, settings, cwd, env, stdout)?;
            if targets.is_empty() {
                return Ok(ExitCode::Resolved);
            }
            if let (Some(rev), Some(path)) = (resolve.in_commit(), &resolve.path) {
                return resolve_in_commit(
                    &resolve,
                    rev,
                    path,
                    settings.line_ending,
                    cwd,
                    env,
                    stdout,
                );
            }
            resolve_headless(&resolve, settings.line_ending, cwd, &targets, &mut repo, stderr)
        }
        Batch::Inspect { files, options, semantics, json } => {
            let chosen = choose_semantics(
                semantics,
                settings,
                &RunRepository::of_merge(cwd, None),
                None,
                cwd,
                env,
            )?;
            let (text, code) =
                inspect::inspect(&files, options, chosen, settings.line_ending, json)?;
            Ok(report(stdout, &text, code))
        }
        Batch::DiffInspect { left, right, diff, json } => {
            let (text, code) = inspect::diff_inspect(&left, &right, diff, json)?;
            Ok(report(stdout, &text, code))
        }
        Batch::Install { vcs, config } => {
            let message = install::run(vcs, config.as_deref())?;
            Ok(report(stdout, &message, ExitCode::Resolved))
        }
        Batch::Jj(jj) => {
            let cwd = cwd.ok_or(CliError::NoWorkingDirectory)?;
            jj_native_batch(jj, cwd, env, stdout)
        }
    }
}

/// Run a jj-native form that needs no window (spec §7.5), with its report on `stdout`.
///
/// # Errors
///
/// [`CliError`] when the form fails.
fn jj_native_batch(
    batch: JjBatch,
    cwd: &Path,
    env: &Env,
    stdout: &mut impl Write,
) -> Result<ExitCode, CliError> {
    let jj_config = || Some(jj_config::snapshot_config(env));
    match batch {
        JjBatch::DiffEvologInspect { evolog, json } => {
            let texts = merge_session::jj_native::evolog(
                cwd,
                &evolog.path,
                evolog.rev.as_deref(),
                evolog.from.as_deref(),
                jj_config(),
            )?;
            let (text, code) =
                inspect::diff_inspect_texts(&texts.left, &texts.right, evolog.diff, json)?;
            Ok(report(stdout, &text, code))
        }
        JjBatch::Stack { revset, json } => {
            let commits = merge_session::jj_native::stack(cwd, revset.as_deref(), jj_config())?;
            let (text, code) = jj_native::stack_report(&commits, json)?;
            Ok(report(stdout, &text, code))
        }
        JjBatch::History { path, limit, json } => {
            let (path, versions) =
                merge_session::jj_native::history(cwd, &path, limit, jj_config())?;
            let text = jj_native::history_report(&path, &versions, json)?;
            Ok(report(stdout, &text, ExitCode::Resolved))
        }
        JjBatch::Absorb { paths, into, choices, apply, listed_at, json } => {
            let absorbed = merge_session::jj_native::absorb(
                cwd,
                &paths,
                into.as_deref(),
                &choices,
                apply,
                listed_at.as_deref(),
                jj_config(),
            )?;
            let text = jj_native::absorb_report(&absorbed, apply, json)?;
            Ok(report(stdout, &text, ExitCode::Resolved))
        }
        JjBatch::Diverge { change, dry_run, json } => {
            let divergence = merge_session::jj_native::diverge(cwd, &change, dry_run, jj_config())?;
            let (text, code) = jj_native::divergence_report(&divergence, dry_run, json)?;
            Ok(report(stdout, &text, code))
        }
    }
}

/// Open `resolve`'s repository and list its targets ([`resolve_targets`]); with a target,
/// choose the semantics and markers in `resolve`'s options for the first. The repository
/// stays open for the steps after.
///
/// # Errors
///
/// [`CliError`] when the repository, its conflicts or jj's config cannot be read, or the
/// semantics do not fit it.
fn resolve_prepare(
    resolve: &mut Resolve,
    settings: &Settings,
    cwd: &Path,
    env: &Env,
    stdout: &mut impl Write,
) -> Result<(Vec<PathBuf>, RunRepository), CliError> {
    let mut repo = RunRepository::of_resolve(cwd, resolve.path.as_deref())
        .with_jj_config(jj_config::snapshot_config(env));
    let targets = resolve_targets(&mut repo, cwd, resolve.path.as_deref(), stdout)?;
    if let Some(first) = targets.first() {
        let mode =
            choose_semantics(resolve.semantics, settings, &repo, Some(first), Some(cwd), env)?;
        resolve.options.semantics = mode.semantics;
        resolve.options.same_change = mode.same_change;
        resolve.options.hunk_level = mode.hunk_level;
        resolve.options.marker_style = choose_marker_style(mode.semantics, settings, &mut repo);
    }
    Ok((targets, repo))
}

/// Settle `stack`'s merge options before its window opens, in the jj repository holding
/// `cwd`, and give that repository. None when `stack`'s revset names no conflicted commit:
/// no window opens, and the run exits with 0, as `stack --list` does.
///
/// # Errors
///
/// [`CliError`] outside a jj repository, when the revset does not resolve, or when the
/// semantics settings are invalid.
fn stack_prepare(
    stack: &mut Stack,
    settings: &Settings,
    cwd: &Path,
    env: &Env,
) -> Result<Option<RunRepository>, CliError> {
    let config = jj_config::snapshot_config(env);
    let listed =
        merge_session::jj_native::stack(cwd, stack.revset.as_deref(), Some(config.clone()))?;
    if listed.is_empty() {
        return Ok(None);
    }
    let mut repo = RunRepository::of_resolve(cwd, None).with_jj_config(config);
    let mode = choose_semantics(SemanticsFlags::default(), settings, &repo, None, Some(cwd), env)?;
    stack.options.semantics = mode.semantics;
    stack.options.same_change = mode.same_change;
    stack.options.hunk_level = mode.hunk_level;
    stack.options.marker_style = choose_marker_style(mode.semantics, settings, &mut repo);
    Ok(Some(repo))
}

/// List `absorb`'s hunks once before its window opens. None when there is no hunk, so no
/// window opens; else the run's repository, with the user's jj config.
///
/// # Errors
///
/// [`CliError`] outside a jj repository, for a path outside it, or when `--into` does not
/// resolve.
fn absorb_prepare(
    absorb: &Absorb,
    cwd: &Path,
    env: &Env,
) -> Result<Option<RunRepository>, CliError> {
    const LIST_ONLY: bool = false;
    let config = jj_config::snapshot_config(env);
    let listed = merge_session::jj_native::absorb(
        cwd,
        &absorb.paths,
        absorb.into.as_deref(),
        &[],
        LIST_ONLY,
        None,
        Some(config.clone()),
    )?;
    Ok((!listed.hunks.is_empty())
        .then(|| RunRepository::of_resolve(cwd, None).with_jj_config(config)))
}

/// Check that `history PATH` can list before its window opens: a jj repository holding
/// `cwd`, and the path inside it. Gives the run's repository, with the user's jj config.
///
/// # Errors
///
/// [`CliError`] outside a jj repository, for a path outside it, or when the operation log
/// cannot be read.
fn history_prepare(history: &History, cwd: &Path, env: &Env) -> Result<RunRepository, CliError> {
    const ONE: Option<usize> = Some(1);
    let config = jj_config::snapshot_config(env);
    merge_session::jj_native::file_history(cwd, &history.path, ONE, Some(config.clone()))?;
    Ok(RunRepository::of_resolve(cwd, None).with_jj_config(config))
}

/// `resolve -r REV PATH --headless`: the automatic merge of the conflict in `rev`'s commit,
/// with `resolve`'s commands applied, written into the commit; on a dry run, only reported.
/// The report goes to `stdout`: 0 when the save resolves the conflict, else 1.
///
/// # Errors
///
/// [`CliError`] when the conflict cannot be loaded or changed by its commands, the result
/// cannot be written, or the repository refuses it.
fn resolve_in_commit(
    resolve: &Resolve,
    rev: &str,
    path: &Path,
    ending: OutputEnding,
    cwd: &Path,
    env: &Env,
    stdout: &mut impl Write,
) -> Result<ExitCode, CliError> {
    let loaded = resolve::load_at_commit(
        cwd,
        rev,
        path,
        resolve.pair,
        Some(jj_config::snapshot_config(env)),
    )?;
    let mut opened = loaded.open(resolve.options).map_err(SessionError::from)?;
    if let Some(commands) = &resolve.commands {
        replay::apply_file(&mut opened.doc, commands)?;
    }
    let output = EncodedOutput::new(&opened.doc, ending);
    let code = output.write(opened.file.as_std_path()).map_err(SaveError::from)?;
    let rewrite = opened
        .record
        .as_ref()
        .map(|record| record.rewrite(code == ExitCode::Resolved, resolve.dry_run))
        .transpose()
        .map_err(|source| CliError::Rewrite { path: opened.path.clone(), source })?
        .flatten();
    match (rewrite, resolve.pair) {
        (Some(rewrite), _) => {
            report(
                stdout,
                &jj_native::rewrite_report(&opened.path, &rewrite, resolve.dry_run),
                code,
            );
        }
        (None, Some(pair)) => {
            report(stdout, &jj_native::pair_unresolved(&opened.path, pair), code);
        }
        (None, None) => {}
    }
    Ok(code)
}

/// Write each target's automatic merge, with `resolve`'s commands applied, to its
/// working-tree file: 0 when every one resolved, else 1. Without a path, a target `resolve`
/// refuses is skipped, its reason on `stderr`, and counts as unresolved.
///
/// # Errors
///
/// [`CliError`] for the first target that cannot be loaded, changed by its commands, or
/// written, refusals aside; the targets before it stay written.
fn resolve_headless(
    resolve: &Resolve,
    ending: OutputEnding,
    cwd: &Path,
    targets: &[PathBuf],
    repo: &mut RunRepository,
    stderr: &mut impl Write,
) -> Result<ExitCode, CliError> {
    // Without a path, the list snapshotted `repo`, and each save is recorded through it, so
    // each load reads at the step before without scanning the working tree again.
    let snapshot =
        if resolve.path.is_none() { resolve::Snapshot::Taken } else { resolve::Snapshot::Take };
    targets.iter().try_fold(ExitCode::Resolved, |code, target| {
        let loaded = match resolve::load(repo, cwd, target, snapshot) {
            Err(error) if resolve.path.is_none() && error.is_refusal() => {
                return Ok(report(stderr, &format!("{PROGRAM}: {error}\n"), ExitCode::Conflicts));
            }
            loaded => loaded?,
        };
        let mut opened = loaded.open(resolve.options).map_err(SessionError::from)?;
        if let Some(path) = &resolve.commands {
            replay::apply_file(&mut opened.doc, path)?;
        }
        let written = EncodedOutput::new(&opened.doc, ending).save(
            opened.file.as_std_path(),
            opened.record.as_ref(),
            Some(repo),
        )?;
        Ok(if written == ExitCode::Resolved { code } else { written })
    })
}

/// The files `resolve` works on: `path`, relative to `cwd`; without one, every conflicted
/// file of the run's repository `repo`, after the list of them, one per line, relative to the
/// root, on `stdout`. None, after `no conflicted files`, when there are none. `repo` stays
/// open for the steps after.
///
/// # Errors
///
/// [`CliError::Resolve`] when there is no repository or its conflicts cannot be read.
pub fn resolve_targets(
    repo: &mut RunRepository,
    cwd: &Path,
    path: Option<&Path>,
    stdout: &mut impl Write,
) -> Result<Vec<PathBuf>, CliError> {
    if let Some(path) = path {
        return Ok(vec![cwd.join(path)]);
    }
    let listed = resolve::conflicted(repo)?;
    if listed.paths.is_empty() {
        report(stdout, NO_CONFLICTS, ExitCode::Resolved);
    }
    for path in &listed.paths {
        report(stdout, &format!("{path}\n"), ExitCode::Resolved);
    }
    Ok(listed.paths.iter().map(|path| listed.root.join(path).into_std_path_buf()).collect())
}

/// What `resolve` without a path prints when nothing is conflicted.
const NO_CONFLICTS: &str = "no conflicted files\n";

/// The config file's settings, and the lines it could not use. Each such line is reported on
/// `stderr` and leaves its key as it was, so a merge started by git or jj still runs.
fn read_settings(path: &Path, stderr: &mut impl Write) -> (Settings, Vec<String>) {
    let loaded = settings::load(path);
    let errors: Vec<_> = loaded.errors.iter().map(ToString::to_string).collect();
    for error in &errors {
        report(stderr, &format!("{PROGRAM}: {error}\n"), ExitCode::Resolved);
    }
    (loaded.settings, errors)
}

/// Write `message` and give `code`. A stream that cannot be written leaves only the code.
fn report(stream: &mut impl Write, message: &str, code: ExitCode) -> ExitCode {
    // Nothing is left to tell the user if stdout or stderr is closed.
    let _ = stream.write_all(message.as_bytes());
    code
}

/// Write the automatic merge, with the commands in the file at `commands` applied, and
/// conflict markers for unresolved regions, its line endings made as `ending` says.
///
/// # Errors
///
/// [`CliError`] when an input or the command file cannot be read, an input cannot be decoded,
/// the labels do not fit, a command does not parse or apply, or the output cannot be written.
/// Only the last leaves the output changed, and then only by the atomic write's rules.
pub fn headless(
    merge: &Merge,
    commands: Option<&Path>,
    ending: OutputEnding,
) -> Result<ExitCode, CliError> {
    let mut document = load(&merge.files, merge.options, merge.labels.clone(), merge.language())?;
    if let Some(path) = commands {
        replay::apply_file(&mut document, path)?;
    }
    Ok(EncodedOutput::new(&document, ending).write(&merge.output).map_err(SessionError::from)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_args(args: &[&str]) -> (Outcome, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let outcome =
            run(std::iter::once(PROGRAM).chain(args.iter().copied()), None, &mut out, &mut err);
        let text = |b: Vec<u8>| String::from_utf8(b).expect("utf-8");
        (outcome, text(out), text(err))
    }

    #[test]
    fn resolve_without_a_working_directory_fails_with_3() {
        let invocation = args::parse([PROGRAM, "resolve", "--headless"]).expect("valid arguments");
        let Invocation::Batch(batch @ Batch::Resolve(_)) = invocation else {
            panic!("a headless resolve, got {invocation:?}");
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let result =
            batch_form(batch, &Settings::default(), None, &Env::default(), &mut out, &mut err);
        assert!(matches!(result, Err(CliError::NoWorkingDirectory)), "{result:?}");
        assert_eq!((out, err), (Vec::new(), Vec::new()), "the caller reports the error");

        let invocation = args::parse([PROGRAM, "resolve"]).expect("valid arguments");
        let Invocation::Window(window @ Window::Resolve(_)) = invocation else {
            panic!("a resolve window, got {invocation:?}");
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let settings = Settings::default();
        let outcome =
            window_form(window, settings, Vec::new(), None, &Env::default(), &mut out, &mut err);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Error));
        assert_eq!(out, b"");
        assert_eq!(
            String::from_utf8(err).expect("utf-8"),
            "triad-merge: cannot find the working directory\n"
        );
    }

    #[test]
    fn labels_that_do_not_fit_are_an_error() {
        use merge_core::document::{LabelCountError, Labels, MergeOptions};
        use merge_session::invocation::{DiffFlags, MergeFiles, SemanticsFlags};

        let fixture = |f: &str| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/core/clean-merge")
                .join(f)
        };
        let merge = Merge {
            files: MergeFiles {
                base: fixture("base"),
                left: fixture("left"),
                right: fixture("right"),
            },
            output: fixture("never-written"),
            options: MergeOptions::default(),
            flags: DiffFlags::default(),
            semantics: SemanticsFlags::default(),
            labels: Labels::new(vec!["l".to_owned()], vec![]),
            title: None,
        };
        let error =
            headless(&merge, None, OutputEnding::Keep).expect_err("one label for two sides");
        assert!(
            matches!(
                error,
                CliError::Session(SessionError::Labels(LabelCountError {
                    label_sides: 1,
                    label_bases: 0,
                    sides: 2
                }))
            ),
            "{error:?}"
        );
        assert!(!merge.output.exists());
    }

    #[test]
    fn help_goes_to_stdout_and_exits_0() {
        let (outcome, out, err) = run_args(&["--help"]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Resolved));
        assert!(out.starts_with("Three-way merge and two-way diff\n"), "{out}");
        assert_eq!(err, "");
    }

    #[test]
    fn a_rejected_argument_goes_to_stderr_and_exits_3() {
        let (outcome, out, err) = run_args(&["--bogus"]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Error));
        assert_eq!(out, "");
        assert!(err.starts_with("error: unexpected argument '--bogus' found\n"), "{err}");
    }

    #[test]
    fn a_usage_error_names_the_program_and_exits_3() {
        let (outcome, out, err) = run_args(&["b", "l", "r"]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Error));
        assert_eq!(out, "");
        assert_eq!(err, "triad-merge: a merge needs an output: -o OUTPUT or a fourth file\n");
    }

    #[test]
    fn window_forms_go_back_to_the_binary() {
        let (outcome, out, err) = run_args(&["b", "l", "r", "o"]);
        assert!(
            matches!(&outcome, Outcome::Window(form) if matches!(form.window, Window::Merge(_))),
            "{outcome:?}"
        );
        assert_eq!((out.as_str(), err.as_str()), ("", ""));
        let (outcome, _, _) = run_args(&[]);
        let Outcome::Window(form) = outcome else {
            panic!("an open dialog, got {outcome:?}");
        };
        let WindowForm { window, settings, settings_errors, cwd, repo } = *form;
        assert_eq!(window, Window::OpenDialog);
        assert_eq!(settings, Settings::default());
        assert_eq!(settings_errors, Vec::<String>::new());
        assert_eq!(cwd, std::env::current_dir().ok());
        assert_eq!(repo.found(), RunRepository::none().found());
    }

    #[test]
    fn a_diff_of_one_file_carries_that_files_repository_to_the_window() {
        let git = vcs_git::testing::Fixture::new();
        let file = git.write("src/a.txt", b"a\n");
        let (outcome, _, err) = run_args(&["diff", file.as_str()]);
        let Outcome::Window(mut form) = outcome else {
            panic!("{outcome:?} {err}");
        };
        let repo = form.repo.require().expect("the file's repository");
        assert_eq!(repo.kind(), vcs::RepoKind::Git);
        assert_eq!(repo.root().as_std_path(), dunce::canonicalize(git.root()).unwrap());
    }

    #[test]
    fn a_window_form_carries_the_settings_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.triad");
        std::fs::write(&path, "font-size = 14\nscroll-sync = false\n").expect("written");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let outcome = run([PROGRAM], Some(&path), &mut out, &mut err);
        let Outcome::Window(form) = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            form.settings,
            Settings { font_size: 14., scroll_sync: false, ..Settings::default() }
        );
    }

    /// `show-config` with `extra` arguments over a config file holding `text`: the exit code,
    /// stdout and stderr.
    fn show_config(text: &str, extra: &[&str]) -> (ExitCode, String, String, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.triad");
        std::fs::write(&path, text).expect("written");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args = [PROGRAM, "show-config"].into_iter().chain(extra.iter().copied());
        let Outcome::Exit(code) = run(args, Some(&path), &mut out, &mut err) else {
            panic!("show-config opens no window");
        };
        let text = |bytes| String::from_utf8(bytes).expect("UTF-8");
        (code, text(out), text(err), path)
    }

    #[test]
    fn show_config_prints_the_settings_in_effect() {
        let (code, out, err, _) = show_config("font-size = 14\nshow-base-pane = true\n", &[]);
        assert_eq!((code, err.as_str()), (ExitCode::Resolved, ""));
        let expected = settings::render(
            &Settings { font_size: 14., base_pane: true, ..Settings::default() },
            false,
        );
        assert_eq!(out, expected);
        assert!(out.contains("\nfont-size = 14\n"), "{out}");
    }

    #[test]
    fn show_config_default_ignores_the_file_and_docs_adds_descriptions() {
        let (code, out, _, _) = show_config("font-size = 14\n", &["--default"]);
        assert_eq!(code, ExitCode::Resolved);
        assert_eq!(out, settings::render(&Settings::default(), false));
        assert!(out.contains("\nfont-size = 10\n"), "{out}");
        let (_, docs, _, _) = show_config("", &["--default", "--docs"]);
        assert!(docs.starts_with("# Whose merge rules apply"), "{docs}");
    }

    #[test]
    fn show_config_with_a_bad_line_prints_the_rest_and_exits_3() {
        let (code, out, err, path) = show_config("font-size = 14\nnonsense\n", &[]);
        assert_eq!(code, ExitCode::Error);
        assert!(out.contains("\nfont-size = 14\n"), "{out}");
        assert_eq!(err, format!("{PROGRAM}: {}:2: expected `key = value`\n", path.display()));
    }

    #[test]
    fn a_window_form_carries_the_config_file_s_bad_lines_for_the_banner() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.triad");
        std::fs::write(&path, "nonsense\nfont-size = 14\nshade = red\n").expect("written");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let Outcome::Window(form) = run([PROGRAM], Some(&path), &mut out, &mut err) else {
            panic!("{}", String::from_utf8_lossy(&err));
        };
        let shown = path.display();
        assert_eq!(
            form.settings_errors,
            vec![
                format!("{shown}:1: expected `key = value`"),
                format!("{shown}:3: unknown setting `shade`"),
            ]
        );
        assert_eq!(form.settings.font_size, 14.);
    }

    #[test]
    fn a_window_form_from_a_clean_config_file_has_no_bad_lines() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let Outcome::Window(form) = run([PROGRAM], None, &mut out, &mut err) else {
            panic!("{}", String::from_utf8_lossy(&err));
        };
        assert_eq!(form.settings_errors, Vec::<String>::new());
    }

    /// The five merge options of spec §6.4: the only settings the command line takes.
    const MERGE_OPTIONS: [&str; 5] =
        ["merge-semantics", "same-change", "hunk-level", "diff-algorithm", "ignore-whitespace"];

    #[test]
    fn only_the_merge_options_are_flags() {
        let changed = Settings {
            semantics: merge_config::settings::Choice::Git,
            same_change: Some(merge_core::merge3::SameChange::Keep),
            hunk_level: Some(merge_core::merge3::HunkLevel::Word),
            ..Settings::default()
        };
        for line in settings::render(&changed, false).lines() {
            let (key, value) = line.split_once(" =").expect("a key = value line");
            let flag = format!("--{key}={}", value.trim_start());
            let parsed =
                parse_with(os(&[PROGRAM, &flag, "b", "l", "r", "o"]), &Settings::default());
            if MERGE_OPTIONS.contains(&key) {
                assert!(
                    matches!(parsed, Ok(Invocation::Window(Window::Merge(_)))),
                    "{flag}: {parsed:?}"
                );
            } else {
                let Err(ParseError::Clap(error)) = parsed else {
                    panic!("{flag}: {parsed:?}");
                };
                assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument, "{flag}");
            }
        }
    }

    #[test]
    fn a_config_key_on_the_command_line_exits_3_and_writes_nothing_to_stdout() {
        let (outcome, out, err) = run_args(&["--font-size=12", "b", "l", "r", "o"]);
        assert_eq!((outcome.exit_code(), out.as_str()), (Some(ExitCode::Error), ""));
        assert!(err.contains("unexpected argument '--font-size'"), "{err}");
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }
}
