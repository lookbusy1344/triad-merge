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
use merge_core::document::{MergeDocument, OutputEnding};
use merge_session::dir_diff::{self, Pair};
use merge_session::env::Env;
use merge_session::invocation::{Absorb, Diff, Merge, Resolve, SemanticsFlags, Window};
use merge_session::jj_native::{Absorbed, Listing, file_history, stacked};
use merge_session::repository::{Discovery, RunRepository};
use merge_session::resolve::Destination;
use merge_session::{
    EncodedOutput, SaveError, SessionError, apply_semantics, choose_semantics, load, replay,
    resolve,
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
    DirDiff(#[from] dir_diff::DirDiffError),
    #[error(transparent)]
    Native(#[from] merge_session::native::NativeError),
    #[error(transparent)]
    Install(#[from] install::InstallError),
    #[error(transparent)]
    Resolve(#[from] resolve::ResolveError),
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error("cannot find the working directory")]
    NoWorkingDirectory,
    #[error("--unified needs two files, not two directories")]
    UnifiedDirectories,
    /// The result is written, but the repository refused it.
    #[error("{path}: {source}")]
    Rewrite { path: camino::Utf8PathBuf, source: vcs::VcsError },
}

/// [`apply_semantics`] for `merge`, reading the run's repository `repo`.
fn choose_for(
    merge: &mut Merge,
    settings: &Settings,
    cwd: Option<&Path>,
    repo: &mut RunRepository,
    run: Run<'_>,
) -> Result<(), CliError> {
    let output = Some(merge.output.as_path());
    apply_semantics(&mut merge.options, merge.semantics, settings, repo, output, cwd, run.env)?;
    Ok(())
}

/// A window form settled before its window opens: the run's repository, and what a
/// jj-native form listed.
struct Prepared {
    window: Window,
    repo: RunRepository,
    listing: Option<Listing>,
}

impl Prepared {
    fn new(window: Window, repo: RunRepository) -> Self {
        Self { window, repo, listing: None }
    }
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
    let discovery = Discovery::from_env(&env);
    match invocation {
        Invocation::ShowConfig { defaults, docs } => {
            let shown = if defaults { &Settings::default() } else { &settings };
            let code =
                if settings_errors.is_empty() { ExitCode::Resolved } else { ExitCode::Error };
            Outcome::Exit(report(stdout, &settings::render(shown, docs), code))
        }
        Invocation::Window(window) => {
            let run = Run { env: &env, discovery: &discovery };
            window_form(window, settings, settings_errors, cwd(), run, stdout, stderr)
        }
        Invocation::Batch(batch) => {
            let run = Run { env: &env, discovery: &discovery };
            let result = batch_form(batch, &settings, cwd().as_deref(), run, stdout, stderr);
            Outcome::Exit(
                result.unwrap_or_else(|e| {
                    report(stderr, &format!("{PROGRAM}: {e}\n"), ExitCode::Error)
                }),
            )
        }
    }
}

/// The process environment a run reads, and how it finds its repositories.
#[derive(Clone, Copy)]
struct Run<'a> {
    env: &'a Env,
    discovery: &'a Discovery,
}

/// Settle a window form's repository, and for a merge its semantics, before the window
/// opens. `resolve` first finds its target, and exits when there is none.
fn window_form(
    window: Window,
    settings: Settings,
    settings_errors: Vec<String>,
    cwd: Option<PathBuf>,
    run: Run<'_>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Outcome {
    match prepare_window(window, &settings, cwd.as_deref(), run, stdout) {
        Ok(Some(Prepared { window, repo, listing })) => Outcome::Window(Box::new(WindowForm {
            window,
            settings,
            settings_errors,
            cwd,
            repo,
            listing,
        })),
        Ok(None) => Outcome::Exit(ExitCode::Resolved),
        Err(error) => {
            Outcome::Exit(report(stderr, &format!("{PROGRAM}: {error}\n"), ExitCode::Error))
        }
    }
}

/// The window `window` opens, settled; none when there is nothing to open.
///
/// # Errors
///
/// [`CliError`] when the form's repository or target cannot be settled.
fn prepare_window(
    window: Window,
    settings: &Settings,
    cwd: Option<&Path>,
    run: Run<'_>,
    stdout: &mut impl Write,
) -> Result<Option<Prepared>, CliError> {
    let need_cwd = || cwd.ok_or(CliError::NoWorkingDirectory);
    Ok(Some(match window {
        Window::Merge(mut merge) => {
            let mut repo = run.discovery.of_merge(cwd, Some(&merge.output));
            choose_for(&mut merge, settings, cwd, &mut repo, run)?;
            Prepared::new(Window::Merge(merge), repo)
        }
        Window::Resolve(mut resolve) => {
            let (targets, repo) =
                resolve_prepare(&mut resolve, settings, need_cwd()?, run, stdout)?;
            let Some(first) = targets.first().cloned() else {
                return Ok(None);
            };
            resolve.target = Some(first);
            Prepared::new(Window::Resolve(resolve), repo)
        }
        Window::Diff(diff) | Window::DiffDirs(diff) => {
            let (window, repo) = diff_form(diff, cwd, run.discovery)?;
            Prepared::new(window, repo)
        }
        Window::DiffChanges { rev, diff, title } => {
            let repo = changes_repository(need_cwd()?, run.discovery)?;
            Prepared::new(Window::DiffChanges { rev, diff, title }, repo)
        }
        Window::DiffNative { path, rev, op, diff, title } => {
            let repo = run.discovery.of_file(&path);
            Prepared::new(Window::DiffNative { path, rev, op, diff, title }, repo)
        }
        Window::OpenDialog => Prepared::new(Window::OpenDialog, run.discovery.find([])),
        Window::DiffSince(since) => {
            let cwd = need_cwd()?;
            let (old, rev) = (since.since.as_str(), since.rev.as_deref());
            let listing = match &since.path {
                Some(path) => Listing::InterdiffFile(merge_session::native::interdiff_path(
                    cwd,
                    path,
                    old,
                    rev,
                    run.discovery,
                )?),
                None => Listing::Interdiff(merge_session::native::interdiff(
                    cwd,
                    old,
                    rev,
                    run.discovery,
                )?),
            };
            Prepared {
                window: Window::DiffSince(since),
                repo: run.discovery.of_resolve(cwd, None),
                listing: Some(listing),
            }
        }
        Window::Evolog(evolog) => {
            let cwd = need_cwd()?;
            let listed = merge_session::jj_native::evolog(
                cwd,
                evolog.rev.as_deref(),
                evolog.limit,
                run.discovery,
            )?;
            Prepared {
                window: Window::Evolog(evolog),
                repo: run.discovery.of_resolve(cwd, None),
                listing: Some(Listing::Evolog(listed)),
            }
        }
        Window::History(history) => {
            let cwd = need_cwd()?;
            let listed = file_history(cwd, &history.path, history.limit, run.discovery)?;
            Prepared {
                window: Window::History(history),
                repo: run.discovery.of_resolve(cwd, None),
                listing: Some(Listing::History(listed)),
            }
        }
        Window::Absorb(absorb) => {
            let cwd = need_cwd()?;
            let listed = absorb_list(&absorb, cwd, run.discovery)?;
            if listed.hunks.is_empty() {
                return Ok(None);
            }
            Prepared {
                window: Window::Absorb(absorb),
                repo: run.discovery.of_resolve(cwd, None),
                listing: Some(Listing::Absorb(listed)),
            }
        }
        Window::Stack(mut stack) => {
            let cwd = need_cwd()?;
            let listed = stacked(cwd, stack.revset.as_deref(), run.discovery)?;
            if listed.commits.is_empty() {
                return Ok(None);
            }
            let mut repo = run.discovery.of_resolve(cwd, None);
            let (flags, cwd) = (SemanticsFlags::default(), Some(cwd));
            apply_semantics(&mut stack.options, flags, settings, &mut repo, None, cwd, run.env)?;
            Prepared { window: Window::Stack(stack), repo, listing: Some(Listing::Stack(listed)) }
        }
    }))
}

/// The window `diff LEFT RIGHT` opens and its repository: the diff window on two files, in
/// the repository of the working directory or else of the right file, or the directory window
/// on two directories, where each file finds its own repository as it opens.
///
/// # Errors
///
/// [`CliError::DirDiff`] when one side is a directory and the other is not.
fn diff_form(
    diff: Diff,
    cwd: Option<&Path>,
    discovery: &Discovery,
) -> Result<(Window, RunRepository), CliError> {
    Ok(match dir_diff::pair(&diff.left, &diff.right)? {
        Pair::Files => {
            let repo = discovery.of_diff_tool(cwd, &diff.right);
            (Window::Diff(diff), repo)
        }
        Pair::Directories => (Window::DiffDirs(diff), discovery.find([])),
    })
}

/// The repository `diff` with no path lists the changes of: the one holding `cwd`, opened
/// now so a run outside any repository ends before a window opens. jj's snapshots read the
/// user's config.
///
/// # Errors
///
/// [`CliError::Native`] when no repository holds `cwd` or it cannot be opened.
fn changes_repository(cwd: &Path, discovery: &Discovery) -> Result<RunRepository, CliError> {
    let mut repo = discovery.find([cwd]);
    repo.require().map_err(merge_session::native::NativeError::from)?;
    Ok(repo)
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
    run: Run<'_>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<ExitCode, CliError> {
    match batch {
        Batch::Headless { mut merge, commands } => {
            let mut repo = run.discovery.of_merge(cwd, Some(&merge.output));
            choose_for(&mut merge, settings, cwd, &mut repo, run)?;
            headless(&merge, commands.as_deref(), settings.line_ending)
        }
        Batch::Resolve(mut resolve) => {
            let cwd = cwd.ok_or(CliError::NoWorkingDirectory)?;
            let (targets, mut repo) = resolve_prepare(&mut resolve, settings, cwd, run, stdout)?;
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
                    run.discovery,
                    stdout,
                );
            }
            resolve_headless(&resolve, settings.line_ending, cwd, &targets, &mut repo, stderr)
        }
        Batch::Inspect { files, options, semantics, json } => {
            let chosen = choose_semantics(
                semantics,
                settings,
                &run.discovery.of_merge(cwd, None),
                None,
                cwd,
                run.env,
            )?;
            let (text, code) =
                inspect::inspect(&files, options, chosen, settings.line_ending, json)?;
            Ok(report(stdout, &text, code))
        }
        Batch::DiffInspect { left, right, diff, json, unified } => {
            let (text, code) = match (dir_diff::pair(&left, &right)?, unified) {
                (Pair::Directories, None) => inspect::dirs_inspect(&left, &right, json)?,
                (Pair::Directories, Some(_)) => return Err(CliError::UnifiedDirectories),
                (Pair::Files, None) => inspect::diff_inspect(&left, &right, &diff, json)?,
                (Pair::Files, Some(context)) => {
                    inspect::diff_unified(&left, &right, &diff, context)?
                }
            };
            Ok(report(stdout, &text, code))
        }
        Batch::ChangesInspect { rev, json } => {
            let cwd = cwd.ok_or(CliError::NoWorkingDirectory)?;
            let mut repo = changes_repository(cwd, run.discovery)?;
            let listing = merge_session::native::changed_files(&mut repo, rev.as_deref())?;
            let (text, code) = inspect::files_report("diff-changes", &listing.files, json)?;
            Ok(report(stdout, &text, code))
        }
        Batch::Install { vcs, config } => {
            let message = install::run(vcs, config.as_deref())?;
            Ok(report(stdout, &message, ExitCode::Resolved))
        }
        Batch::Jj(jj) => {
            let cwd = cwd.ok_or(CliError::NoWorkingDirectory)?;
            jj_native_batch(jj, cwd, run.discovery, stdout)
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
    discovery: &Discovery,
    stdout: &mut impl Write,
) -> Result<ExitCode, CliError> {
    match batch {
        JjBatch::Stack { revset, json } => {
            let commits = merge_session::jj_native::stack(cwd, revset.as_deref(), discovery)?;
            let (text, code) = jj_native::stack_report(&commits, json)?;
            Ok(report(stdout, &text, code))
        }
        JjBatch::History { path, limit, json } => {
            let (path, versions) = merge_session::jj_native::history(cwd, &path, limit, discovery)?;
            let text = jj_native::history_report(&path, &versions, json)?;
            Ok(report(stdout, &text, ExitCode::Resolved))
        }
        JjBatch::Evolog { rev, limit, json } => {
            let evolog = merge_session::jj_native::evolog(cwd, rev.as_deref(), limit, discovery)?;
            let text = jj_native::evolog_report(&evolog, json)?;
            Ok(report(stdout, &text, ExitCode::Resolved))
        }
        JjBatch::Interdiff { since, rev, json } => {
            let listing = merge_session::native::interdiff(cwd, &since, rev.as_deref(), discovery)?;
            let (text, code) = inspect::files_report("diff-changes", &listing.files, json)?;
            Ok(report(stdout, &text, code))
        }
        JjBatch::Absorb { paths, into, choices, apply, listed_at, json } => {
            let absorbed = merge_session::jj_native::absorb(
                cwd,
                &paths,
                into.as_deref(),
                &choices,
                apply,
                listed_at.as_deref(),
                discovery,
            )?;
            let text = jj_native::absorb_report(&absorbed, apply, json)?;
            Ok(report(stdout, &text, ExitCode::Resolved))
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
    run: Run<'_>,
    stdout: &mut impl Write,
) -> Result<(Vec<PathBuf>, RunRepository), CliError> {
    let mut repo = run.discovery.of_resolve(cwd, resolve.path.as_deref());
    let targets = resolve_targets(&mut repo, cwd, resolve.path.as_deref(), stdout)?;
    if let Some(first) = targets.first() {
        let (flags, output) = (resolve.semantics, Some(first.as_path()));
        apply_semantics(
            &mut resolve.options,
            flags,
            settings,
            &mut repo,
            output,
            Some(cwd),
            run.env,
        )?;
    }
    Ok((targets, repo))
}

/// `absorb`'s hunks, listed once before its window opens, which shows them first.
///
/// # Errors
///
/// [`CliError`] outside a jj repository, for a path outside it, or when `--into` does not
/// resolve.
fn absorb_list(absorb: &Absorb, cwd: &Path, discovery: &Discovery) -> Result<Absorbed, CliError> {
    const LIST_ONLY: bool = false;
    let into = absorb.into.as_deref();
    Ok(merge_session::jj_native::absorb(cwd, &absorb.paths, into, &[], LIST_ONLY, None, discovery)?)
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
    discovery: &Discovery,
    stdout: &mut impl Write,
) -> Result<ExitCode, CliError> {
    let loaded = resolve::load_at_commit(cwd, rev, path, discovery)?;
    let mut opened = loaded.open(resolve.options.clone()).map_err(SessionError::from)?;
    if let Some(commands) = &resolve.commands {
        replay::apply_file(&mut opened.doc, commands)?;
    }
    let output = encoded(&opened.doc, ending, opened.path.as_std_path())?;
    let commit = match &opened.destination {
        Destination::Commit(commit) => commit,
        file @ Destination::File { .. } => return Ok(output.save(file, None)?),
    };
    let rewrite = commit
        .write(output.bytes(), resolve.dry_run)
        .map_err(|source| CliError::Rewrite { path: opened.path.clone(), source })?;
    let code = output.code();
    report(stdout, &jj_native::rewrite_report(&opened.path, &rewrite, resolve.dry_run), code);
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
        let mut opened = loaded.open(resolve.options.clone()).map_err(SessionError::from)?;
        if let Some(path) = &resolve.commands {
            replay::apply_file(&mut opened.doc, path)?;
        }
        let written = encoded(&opened.doc, ending, opened.destination.name())?
            .save(&opened.destination, Some(repo))?;
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
/// Write `message` to `stream` with its unusual characters escaped, so text from a file or a
/// repository shows as text and runs no terminal escape sequence (spec §6.1).
fn report(stream: &mut impl Write, message: &str, code: ExitCode) -> ExitCode {
    // Nothing is left to tell the user if stdout or stderr is closed.
    let _ = stream.write_all(merge_core::unusual::escaped(message).as_bytes());
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
    let mut document =
        load(&merge.files, merge.options.clone(), merge.labels.clone(), merge.language())?;
    if let Some(path) = commands {
        replay::apply_file(&mut document, path)?;
    }
    let output = encoded(&document, ending, &merge.output)?;
    Ok(output.write(&merge.output).map_err(SessionError::from)?)
}

/// `document`'s output as a save to `path` writes it.
///
/// # Errors
///
/// [`SessionError::Encode`], naming `path`, when the output encoding cannot hold the result.
fn encoded(
    document: &MergeDocument,
    ending: OutputEnding,
    path: &Path,
) -> Result<EncodedOutput, SessionError> {
    EncodedOutput::new(document, ending)
        .map_err(|source| SessionError::Encode { path: path.to_owned(), source })
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
    fn an_error_shows_the_escape_sequences_of_a_path_as_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path().join("a\u{1B}[2Jb\u{202E}");
        let base = base.to_str().expect("utf-8");
        let (outcome, out, err) = run_args(&["inspect", base, base, base]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Error));
        assert_eq!(out, "");
        assert!(err.contains(r"a\u001b[2Jb\u202e"), "{err:?}");
        assert!(!err.contains(['\u{1B}', '\u{202E}']), "{err:?}");
    }

    #[test]
    fn resolve_without_a_working_directory_fails_with_3() {
        let invocation = args::parse([PROGRAM, "resolve", "--headless"]).expect("valid arguments");
        let Invocation::Batch(batch @ Batch::Resolve(_)) = invocation else {
            panic!("a headless resolve, got {invocation:?}");
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let (env, discovery) = (Env::default(), Discovery::default());
        let run = Run { env: &env, discovery: &discovery };
        let result = batch_form(batch, &Settings::default(), None, run, &mut out, &mut err);
        assert!(matches!(result, Err(CliError::NoWorkingDirectory)), "{result:?}");
        assert_eq!((out, err), (Vec::new(), Vec::new()), "the caller reports the error");

        let invocation = args::parse([PROGRAM, "resolve"]).expect("valid arguments");
        let Invocation::Window(window @ Window::Resolve(_)) = invocation else {
            panic!("a resolve window, got {invocation:?}");
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let settings = Settings::default();
        let outcome = window_form(window, settings, Vec::new(), None, run, &mut out, &mut err);
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
        let WindowForm { window, settings, settings_errors, cwd, repo, listing } = *form;
        assert_eq!(window, Window::OpenDialog);
        assert_eq!(settings, Settings::default());
        assert_eq!(settings_errors, Vec::<String>::new());
        assert_eq!(cwd, std::env::current_dir().ok());
        assert_eq!(repo.found(), RunRepository::none().found());
        assert!(listing.is_none(), "the open dialog lists nothing");
    }

    #[test]
    fn a_diff_of_two_directories_opens_the_directory_window() {
        let dir = tempfile::tempdir().expect("tempdir");
        let [left, right] = ["l", "r"].map(|side| dir.path().join(side));
        for side in [&left, &right] {
            std::fs::create_dir(side).expect("dir");
        }
        let [l, r] = [&left, &right].map(|p| p.to_str().expect("utf-8"));
        let (outcome, out, err) = run_args(&["diff", l, r, "--readonly-left"]);
        let Outcome::Window(form) = outcome else {
            panic!("{outcome:?} {err}");
        };
        let Window::DiffDirs(diff) = &form.window else {
            panic!("the directory window, got {:?}", form.window);
        };
        assert_eq!((&diff.left, &diff.right, diff.readonly_left), (&left, &right, true));
        assert_eq!((out.as_str(), err.as_str()), ("", ""));
    }

    #[test]
    fn a_diff_of_a_directory_and_a_file_exits_3() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("f");
        std::fs::write(&file, "x\n").expect("written");
        let [d, f] = [dir.path(), &file].map(|p| p.to_str().expect("utf-8"));
        let (outcome, out, err) = run_args(&["diff", f, d]);
        assert_eq!(outcome.exit_code(), Some(ExitCode::Error));
        assert_eq!(out, "");
        assert_eq!(
            err,
            format!(
                "triad-merge: diff takes two files or two directories: {d} is a directory, {f} is not\n"
            )
        );
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
