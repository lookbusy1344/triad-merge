//! The command line of spec §6.1, parsed into an [`Invocation`].

use std::ffi::OsString;
use std::path::PathBuf;

use clap::builder::{PossibleValue, PossibleValuesParser, TypedValueParser};
use clap::{ArgGroup, Parser, Subcommand};
use merge_core::diff::DiffOptions;
use merge_core::document::{Labels, MergeOptions};
use vcs::RepoKind;

use merge_config::named::Named;
use merge_config::settings::{Algorithm, Settings, Whitespace};
use merge_config::settings::{Choice, HunkLevelArg, SameChangeArg};
use merge_session::invocation::{
    Diff, DiffFlags, Merge, MergeFiles, Resolve, SemanticsFlags, Window,
};

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// A form that opens a window.
    Window(Window),
    /// A form that runs and exits without a window.
    Batch(Batch),
    /// Print the settings in effect in the config file's form, or the defaults.
    ShowConfig { defaults: bool, docs: bool },
}

/// A form that runs without a window, its report on stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Batch {
    /// The automatic merge, with the commands in `commands` applied, written without a window.
    Headless { merge: Merge, commands: Option<PathBuf> },
    /// `resolve --headless`: each conflicted file's automatic merge, with `commands`
    /// applied, written to its working-tree file.
    Resolve(Resolve),
    /// Print the merge's regions; no output file. `options` are the settings file's with the
    /// flags over them, as a merge's; the semantics come from `merge-semantics` when the run starts.
    Inspect { files: MergeFiles, options: MergeOptions, semantics: SemanticsFlags, json: bool },
    /// Print the two-way regions; no window.
    DiffInspect { left: PathBuf, right: PathBuf, diff: DiffOptions, json: bool },
    /// Write the merge and diff tool entries to `vcs`'s user config, or to `config`.
    Install { vcs: RepoKind, config: Option<PathBuf> },
    /// List the conflicted commits `revset` names, parents first (jj only).
    Stack { revset: Option<String>, json: bool },
}

/// A command line that parses but asks for no valid form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    #[error("a merge takes BASE LEFT RIGHT and an output, got {0} files")]
    MergeFiles(usize),
    #[error("give the output once: as -o OUTPUT or as the fourth file")]
    OutputTwice,
    #[error("a merge needs an output: -o OUTPUT or a fourth file")]
    NoOutput,
    #[error("--headless needs BASE LEFT RIGHT and an output")]
    HeadlessWithoutFiles,
    #[error("diff takes LEFT RIGHT, or one PATH, got {0} files")]
    DiffFiles(usize),
    #[error("--rev applies only to diff PATH")]
    RevWithTwoFiles,
    #[error("--inspect needs LEFT RIGHT")]
    InspectNative,
    #[error("--commands applies to one file: give resolve a PATH")]
    CommandsWithoutPath,
}

/// Parse the arguments, program name first, over the default settings.
///
/// # Errors
///
/// As [`parse_with`].
pub fn parse<I, T>(args: I) -> Result<Invocation, ParseError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    parse_with(args, &Settings::default())
}

/// Parse the arguments, program name first. Flags override `settings` (spec §6.4).
///
/// # Errors
///
/// [`clap::Error`] for arguments clap rejects, including `--help` and `--version`, whose
/// message is the help text. [`UsageError`] for a combination that matches no form.
pub fn parse_with<I, T>(args: I, settings: &Settings) -> Result<Invocation, ParseError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Cli::try_parse_from(args)?.invocation(settings).map_err(ParseError::Usage)
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error(transparent)]
    Clap(#[from] clap::Error),
    #[error(transparent)]
    Usage(UsageError),
}

/// Most positional files a form takes: the git form's BASE LEFT RIGHT OUTPUT.
const MAX_MERGE_FILES: usize = 4;
const MERGE_INPUTS: usize = 3;
const MIN_MARKER_LENGTH: usize = 1;
/// The longest conflict marker. Each conflict's output holds four markers of this length.
const MAX_MARKER_LENGTH: usize = 65_536;

/// A conflict marker length: a whole number from [`MIN_MARKER_LENGTH`] to [`MAX_MARKER_LENGTH`].
fn marker_length(value: &str) -> Result<usize, String> {
    match value.parse::<usize>() {
        Ok(n) if n < MIN_MARKER_LENGTH => Err(format!("must be at least {MIN_MARKER_LENGTH}")),
        Ok(n) if n > MAX_MARKER_LENGTH => Err(format!("must be at most {MAX_MARKER_LENGTH}")),
        Ok(n) => Ok(n),
        Err(error) => Err(error.to_string()),
    }
}

/// A parser of `T`'s values by name, case-sensitive, each listed in help with its doc comment
/// less the closing full stop, as clap shows a doc comment.
#[expect(clippy::expect_used, reason = "invariant: clap passes on only the names it was given")]
fn named<T: Named + Send + Sync>() -> impl TypedValueParser<Value = T> {
    let values = T::VARIANTS.iter().map(|&value| {
        let possible = PossibleValue::new(value.name());
        match value.help() {
            Some(help) => possible.help(help.strip_suffix('.').unwrap_or(help)),
            None => possible,
        }
    });
    PossibleValuesParser::new(values)
        .map(|name| T::from_name(&name).expect("clap accepts only the names `T` lists"))
}

/// The help text's note on the config file.
const SETTINGS_HELP: &str = "Every setting, these options among them, is in the config file. \
`triad-merge show-config --docs` lists them.";

#[derive(Debug, Parser)]
#[command(
    name = "triad-merge",
    version,
    about = "Three-way merge and two-way diff",
    args_conflicts_with_subcommands = true,
    after_help = SETTINGS_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Write the automatic merge without opening a window.
    #[arg(long)]
    headless: bool,
    /// BASE LEFT RIGHT, and OUTPUT in the positional form git uses.
    #[arg(value_name = "FILE", num_args = 0..=MAX_MERGE_FILES)]
    files: Vec<PathBuf>,
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,
    /// With --headless: apply the merge commands in FILE, one JSON object per line, before
    /// writing the output.
    #[arg(long, value_name = "FILE", requires = "headless")]
    commands: Option<PathBuf>,
    #[command(flatten)]
    diff: DiffArgs,
    #[command(flatten)]
    semantics: SemanticsArgs,
    /// Conflict marker length. Markers grow past marker-like input lines.
    #[arg(long, value_name = "N", value_parser = marker_length)]
    marker_length: Option<usize>,
    /// Window title.
    #[arg(long)]
    title: Option<String>,
    #[arg(long, value_name = "L")]
    label_left: Option<String>,
    #[arg(long, value_name = "R")]
    label_right: Option<String>,
    #[arg(long, value_name = "B")]
    label_base: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the merge's regions and kinds. Writes no output.
    Inspect {
        #[arg(value_name = "FILE", num_args = MERGE_INPUTS, required = true)]
        files: Vec<PathBuf>,
        #[command(flatten)]
        diff: DiffArgs,
        #[command(flatten)]
        semantics: SemanticsArgs,
        #[arg(long)]
        json: bool,
    },
    /// Two-way diff: LEFT RIGHT, or PATH against its parent revision.
    Diff(DiffCli),
    /// Resolve a conflicted file from the repository; with no PATH, the first of them.
    Resolve(ResolveCli),
    /// Register triad as git's or jj's merge and diff tool.
    Install(InstallCli),
    /// List the conflicted commits, parents first (jj only).
    Stack {
        /// The commits to list; default `conflicts() & mutable()`.
        #[arg(value_name = "REVSET")]
        revset: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print the settings in effect, in the config file's form.
    ShowConfig {
        /// Print the defaults instead.
        #[arg(long = "default")]
        defaults: bool,
        /// Put each setting's description above it.
        #[arg(long)]
        docs: bool,
    },
}

#[derive(Debug, clap::Args)]
struct ResolveCli {
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    /// Write the automatic merge to the working-tree file without opening a window.
    #[arg(long)]
    headless: bool,
    /// With --headless and a PATH: apply the merge commands in FILE, one JSON object per
    /// line, before writing.
    #[arg(long, value_name = "FILE", requires = "headless")]
    commands: Option<PathBuf>,
    /// Resolve PATH in the commit REV names, without checking it out (jj only).
    #[arg(short = 'r', long = "revision", value_name = "REV", requires = "path")]
    revision: Option<String>,
    /// With --headless and --revision: report what the save would do; write nothing.
    #[arg(long, requires_all = ["headless", "revision"])]
    dry_run: bool,
    #[command(flatten)]
    diff: DiffArgs,
    #[command(flatten)]
    semantics: SemanticsArgs,
    /// Conflict marker length. Markers grow past marker-like input lines.
    #[arg(long, value_name = "N", value_parser = marker_length)]
    marker_length: Option<usize>,
}

impl ResolveCli {
    fn invocation(self, settings: &Settings) -> Result<Invocation, UsageError> {
        if self.commands.is_some() && self.path.is_none() {
            return Err(UsageError::CommandsWithoutPath);
        }
        let flags = self.diff.flags();
        let chosen = MergeOptions {
            marker_length: self.marker_length.unwrap_or(MergeOptions::default().marker_length),
            ..MergeOptions::default()
        };
        let resolve = Resolve {
            path: self.path,
            target: None,
            commands: self.commands,
            options: flags.merge_options(settings, chosen),
            flags,
            semantics: self.semantics.flags(),
            rev: self.revision,
            dry_run: self.dry_run,
        };
        Ok(if self.headless {
            Invocation::Batch(Batch::Resolve(resolve))
        } else {
            Invocation::Window(Window::Resolve(resolve))
        })
    }
}

#[derive(Debug, clap::Args)]
#[command(group(ArgGroup::new("vcs").required(true)))]
struct InstallCli {
    /// Write to git's global config.
    #[arg(long, group = "vcs")]
    git: bool,
    /// Write to jj's user config.
    #[arg(long, group = "vcs")]
    jj: bool,
    /// Write to this file instead of the user's config.
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
}

impl InstallCli {
    fn invocation(self) -> Invocation {
        let InstallCli { git, jj, config } = self;
        debug_assert!(git != jj, "clap requires exactly one VCS");
        let vcs = if jj { RepoKind::Jj } else { RepoKind::Git };
        Invocation::Batch(Batch::Install { vcs, config })
    }
}

#[derive(Debug, clap::Args)]
#[expect(clippy::struct_excessive_bools, reason = "each bool is a command-line flag")]
struct DiffCli {
    #[arg(value_name = "FILE", num_args = 1..)]
    files: Vec<PathBuf>,
    /// The revision to compare PATH with.
    #[arg(long)]
    rev: Option<String>,
    /// Print the regions; no window.
    #[arg(long, conflicts_with_all = ["title", "readonly_left", "readonly_right"])]
    inspect: bool,
    #[arg(long, requires = "inspect")]
    json: bool,
    #[command(flatten)]
    diff: DiffArgs,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    readonly_left: bool,
    #[arg(long)]
    readonly_right: bool,
}

/// Without a flag, the setting holds: histogram and no whitespace ignored by default.
#[derive(Debug, Clone, Copy, clap::Args)]
struct DiffArgs {
    #[arg(long = "diff-algorithm", value_parser = named::<Algorithm>(), value_name = "A")]
    algorithm: Option<Algorithm>,
    #[arg(long = "ignore-whitespace", value_parser = named::<Whitespace>(), value_name = "W")]
    whitespace: Option<Whitespace>,
}

/// Without a flag, the setting holds: `auto`, and jj's config or `accept` for the same change.
#[derive(Debug, Clone, Copy, clap::Args)]
struct SemanticsArgs {
    /// Merge as git or jj does; `auto` follows the repository.
    #[arg(long = "merge-semantics", value_parser = named::<Choice>(), value_name = "S")]
    semantics: Option<Choice>,
    /// Whether a change every side made resolves (`accept`) or stays a conflict (`keep`).
    #[arg(long, value_parser = named::<SameChangeArg>(), value_name = "C")]
    same_change: Option<SameChangeArg>,
    /// Merge conflicts word by word (`word`), keeping each unless every word resolves.
    #[arg(long, value_parser = named::<HunkLevelArg>(), value_name = "H")]
    hunk_level: Option<HunkLevelArg>,
}

impl SemanticsArgs {
    fn flags(self) -> SemanticsFlags {
        SemanticsFlags {
            choice: self.semantics,
            same_change: self.same_change.map(Into::into),
            hunk_level: self.hunk_level.map(Into::into),
        }
    }
}

impl DiffArgs {
    fn flags(self) -> DiffFlags {
        DiffFlags {
            algorithm: self.algorithm.map(Into::into),
            whitespace: self.whitespace.map(Into::into),
        }
    }

    /// `settings` with the flags given applied.
    fn over(self, settings: DiffOptions) -> DiffOptions {
        self.flags().over(settings)
    }
}

impl Cli {
    fn invocation(self, settings: &Settings) -> Result<Invocation, UsageError> {
        match self.command {
            Some(Command::Inspect { files, diff, semantics, json }) => {
                let [base, left, right] = <[PathBuf; MERGE_INPUTS]>::try_from(files)
                    .map_err(|files| UsageError::MergeFiles(files.len()))?;
                Ok(Invocation::Batch(Batch::Inspect {
                    files: MergeFiles { base, left, right },
                    options: diff.flags().merge_options(settings, MergeOptions::default()),
                    semantics: semantics.flags(),
                    json,
                }))
            }
            Some(Command::Diff(diff)) => diff.invocation(settings),
            Some(Command::Resolve(resolve)) => resolve.invocation(settings),
            Some(Command::Install(install)) => Ok(install.invocation()),
            Some(Command::Stack { revset, json }) => {
                Ok(Invocation::Batch(Batch::Stack { revset, json }))
            }
            Some(Command::ShowConfig { defaults, docs }) => {
                Ok(Invocation::ShowConfig { defaults, docs })
            }
            None => self.merge(settings),
        }
    }

    fn merge(self, settings: &Settings) -> Result<Invocation, UsageError> {
        let count = self.files.len();
        let mut files = self.files.into_iter();
        let (base, left, right, fourth) =
            match (files.next(), files.next(), files.next(), files.next(), files.next()) {
                (None, ..) if self.headless => return Err(UsageError::HeadlessWithoutFiles),
                (None, ..) => return Ok(Invocation::Window(Window::OpenDialog)),
                (Some(base), Some(left), Some(right), fourth, None) => (base, left, right, fourth),
                _ => return Err(UsageError::MergeFiles(count)),
            };
        let output = match (fourth, self.output) {
            (Some(_), Some(_)) => return Err(UsageError::OutputTwice),
            (Some(output), None) | (None, Some(output)) => output,
            (None, None) => return Err(UsageError::NoOutput),
        };
        let label = |l: &Option<String>| l.clone().unwrap_or_default();
        let mut merge = Merge {
            files: MergeFiles { base, left, right },
            output,
            options: MergeOptions {
                marker_length: self.marker_length.unwrap_or(MergeOptions::default().marker_length),
                ..MergeOptions::default()
            },
            flags: self.diff.flags(),
            semantics: self.semantics.flags(),
            labels: Labels::three_way(
                &label(&self.label_left),
                &label(&self.label_base),
                &label(&self.label_right),
            ),
            title: self.title,
        };
        merge.options = merge.options_with(settings);
        Ok(if self.headless {
            Invocation::Batch(Batch::Headless { merge, commands: self.commands })
        } else {
            Invocation::Window(Window::Merge(merge))
        })
    }
}

impl DiffCli {
    fn invocation(self, settings: &Settings) -> Result<Invocation, UsageError> {
        let count = self.files.len();
        let mut files = self.files.into_iter();
        let paths = (files.next(), files.next(), files.next());
        match (paths, self.rev, self.inspect) {
            ((Some(_), None, None), _, true) => Err(UsageError::InspectNative),
            ((Some(path), None, None), rev, false) => Ok(Invocation::Window(Window::DiffNative {
                path,
                rev,
                diff: self.diff.over(settings.two_way()),
                title: self.title,
            })),
            ((Some(_), Some(_), None), Some(_), _) => Err(UsageError::RevWithTwoFiles),
            ((Some(left), Some(right), None), None, inspect) => Ok(if inspect {
                Invocation::Batch(Batch::DiffInspect {
                    left,
                    right,
                    diff: self.diff.over(settings.two_way()),
                    json: self.json,
                })
            } else {
                Invocation::Window(Window::Diff(Diff {
                    left,
                    right,
                    diff: self.diff.over(settings.two_way()),
                    title: self.title,
                    readonly_left: self.readonly_left,
                    readonly_right: self.readonly_right,
                }))
            }),
            _ => Err(UsageError::DiffFiles(count)),
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;
    use merge_core::diff::{self, LineEndingRule};
    use merge_core::merge3::{HunkLevel, Refine, SameChange};

    use super::*;
    use merge_config::settings::Settings;

    fn parsed(args: &[&str]) -> Result<Invocation, ParseError> {
        parse(std::iter::once("triad-merge").chain(args.iter().copied()))
    }

    fn ok(args: &[&str]) -> Invocation {
        parsed(args).unwrap_or_else(|e| panic!("{args:?}: {e}"))
    }

    fn usage(args: &[&str]) -> UsageError {
        match parsed(args) {
            Err(ParseError::Usage(e)) => e,
            other => panic!("{args:?}: expected a usage error, got {other:?}"),
        }
    }

    fn clap_kind(args: &[&str]) -> ErrorKind {
        match parsed(args) {
            Err(ParseError::Clap(e)) => e.kind(),
            other => panic!("{args:?}: expected a clap error, got {other:?}"),
        }
    }

    fn files() -> MergeFiles {
        MergeFiles { base: "b".into(), left: "l".into(), right: "r".into() }
    }

    #[test]
    fn settings_options_keep_the_runs_semantics_same_change_and_marker_length() {
        let chosen = MergeOptions {
            semantics: merge_core::merge3::Semantics::Jj,
            same_change: SameChange::Keep,
            marker_length: 11,
            ..MergeOptions::default()
        };
        let settings = Settings { refine: Refine::Off, ..Settings::default() };
        let options = DiffFlags::default().merge_options(&settings, chosen);
        assert_eq!(options, MergeOptions { refine: Refine::Off, ..chosen });
    }

    fn merge(output: &str) -> Merge {
        Merge {
            files: files(),
            output: output.into(),
            options: MergeOptions::default(),
            flags: DiffFlags::default(),
            semantics: SemanticsFlags::default(),
            labels: Labels::three_way("", "", ""),
            title: None,
        }
    }

    #[test]
    fn the_shortest_marker_length_is_one() {
        let Invocation::Window(Window::Merge(merge)) =
            ok(&["b", "l", "r", "o", "--marker-length", "1"])
        else {
            panic!("a merge");
        };
        assert_eq!(merge.options.marker_length, 1);
    }

    #[test]
    fn the_longest_marker_length_is_accepted() {
        let Invocation::Window(Window::Merge(merge)) =
            ok(&["b", "l", "r", "o", "--marker-length", "65536"])
        else {
            panic!("a merge");
        };
        assert_eq!(merge.options.marker_length, 65_536);
    }

    #[test]
    fn a_marker_length_above_the_longest_is_refused() {
        for value in ["65537", &usize::MAX.to_string()] {
            let error = marker_length(value).expect_err("too long");
            assert_eq!(error, "must be at most 65536");
        }
    }

    #[test]
    fn no_arguments_open_the_dialog() {
        assert_eq!(ok(&[]), Invocation::Window(Window::OpenDialog));
    }

    #[test]
    fn the_positional_git_form_and_the_output_flag_give_the_same_merge() {
        assert_eq!(ok(&["b", "l", "r", "o"]), Invocation::Window(Window::Merge(merge("o"))));
        assert_eq!(ok(&["b", "l", "r", "-o", "o"]), Invocation::Window(Window::Merge(merge("o"))));
        assert_eq!(
            ok(&["b", "l", "r", "--output", "o"]),
            Invocation::Window(Window::Merge(merge("o")))
        );
    }

    #[test]
    fn headless_takes_either_output_form() {
        assert_eq!(ok(&["--headless", "b", "l", "r", "-o", "o"]), headless("o", None));
        assert_eq!(ok(&["b", "l", "r", "o", "--headless"]), headless("o", None));
    }

    fn headless(output: &str, commands: Option<&str>) -> Invocation {
        Invocation::Batch(Batch::Headless {
            merge: merge(output),
            commands: commands.map(PathBuf::from),
        })
    }

    #[test]
    fn headless_takes_a_command_file() {
        assert_eq!(
            ok(&["--headless", "b", "l", "r", "-o", "o", "--commands", "c"]),
            headless("o", Some("c"))
        );
        assert_eq!(
            ok(&["--commands=c", "b", "l", "r", "o", "--headless"]),
            headless("o", Some("c"))
        );
    }

    #[test]
    fn a_command_file_needs_headless() {
        assert_eq!(
            clap_kind(&["b", "l", "r", "o", "--commands", "c"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            clap_kind(&["inspect", "b", "l", "r", "--commands", "c"]),
            ErrorKind::UnknownArgument
        );
    }

    #[test]
    fn merge_options_labels_and_title_are_read() {
        let args = [
            "b",
            "l",
            "r",
            "-o",
            "o",
            "--diff-algorithm",
            "myers-minimal",
            "--ignore-whitespace",
            "amount",
            "--marker-length",
            "9",
            "--title",
            "T",
            "--label-left",
            "ours",
            "--label-base",
            "anc",
            "--label-right",
            "theirs",
        ];
        let expected = Merge {
            options: MergeOptions {
                diff: DiffOptions {
                    algorithm: diff::Algorithm::MyersMinimal,
                    whitespace: diff::Whitespace::Amount,
                    ..DiffOptions::default()
                },
                refine: Refine::Ends,
                marker_length: 9,
                ..MergeOptions::default()
            },
            flags: DiffFlags {
                algorithm: Some(diff::Algorithm::MyersMinimal),
                whitespace: Some(diff::Whitespace::Amount),
            },
            labels: Labels::three_way("ours", "anc", "theirs"),
            title: Some("T".to_owned()),
            ..merge("o")
        };
        assert_eq!(ok(&args), Invocation::Window(Window::Merge(expected)));
    }

    #[test]
    fn every_algorithm_and_whitespace_mode_maps_to_the_core() {
        let options = |flag: &str, value: &str| match ok(&["inspect", "b", "l", "r", flag, value]) {
            Invocation::Batch(Batch::Inspect { options, .. }) => options.diff,
            other => panic!("{other:?}"),
        };
        for (value, algorithm) in [
            ("histogram", diff::Algorithm::Histogram),
            ("myers", diff::Algorithm::Myers),
            ("myers-minimal", diff::Algorithm::MyersMinimal),
        ] {
            assert_eq!(options("--diff-algorithm", value).algorithm, algorithm);
        }
        for (value, whitespace) in [
            ("none", diff::Whitespace::None),
            ("trailing", diff::Whitespace::Trailing),
            ("amount", diff::Whitespace::Amount),
            ("all", diff::Whitespace::All),
        ] {
            assert_eq!(options("--ignore-whitespace", value).whitespace, whitespace);
        }
    }

    #[test]
    fn malformed_merges_are_usage_errors() {
        assert_eq!(usage(&["b", "l"]), UsageError::MergeFiles(2));
        assert_eq!(usage(&["b"]), UsageError::MergeFiles(1));
        assert_eq!(usage(&["b", "l", "r"]), UsageError::NoOutput);
        assert_eq!(usage(&["b", "l", "r", "o", "-o", "p"]), UsageError::OutputTwice);
        assert_eq!(usage(&["--headless"]), UsageError::HeadlessWithoutFiles);
    }

    /// Option names given up for clearer ones, without aliases, each with a value its new
    /// name takes.
    const RETIRED_FLAGS: &[(&str, &str)] =
        &[("--algorithm", "myers"), ("--whitespace", "all"), ("--semantics", "jj")];

    #[test]
    fn a_retired_option_name_is_an_unknown_argument() {
        for (retired, value) in RETIRED_FLAGS {
            for form in
                [&["b", "l", "r", "o"][..], &["inspect", "b", "l", "r"], &["diff", "l", "r"]]
            {
                let args = [form, &[retired, value]].concat();
                assert_eq!(clap_kind(&args), ErrorKind::UnknownArgument, "{args:?}");
            }
        }
    }

    #[test]
    fn rejected_values_are_clap_errors() {
        assert_eq!(clap_kind(&["b", "l", "r", "o", "p"]), ErrorKind::TooManyValues);
        for bad in ["0", "-1", "x", ""] {
            let flag = format!("--marker-length={bad}");
            assert_eq!(
                clap_kind(&["b", "l", "r", "o", &flag]),
                ErrorKind::ValueValidation,
                "{bad:?}"
            );
        }
        assert_eq!(
            clap_kind(&["b", "l", "r", "o", "--diff-algorithm", "patience"]),
            ErrorKind::InvalidValue
        );
        assert_eq!(clap_kind(&["inspect", "b", "l"]), ErrorKind::WrongNumberOfValues);
        assert_eq!(clap_kind(&["--bogus"]), ErrorKind::UnknownArgument);
        assert_eq!(clap_kind(&["--help"]), ErrorKind::DisplayHelp);
        assert_eq!(clap_kind(&["--version"]), ErrorKind::DisplayVersion);
    }

    #[test]
    fn inspect_takes_three_files() {
        assert_eq!(
            ok(&["inspect", "b", "l", "r", "--json"]),
            Invocation::Batch(Batch::Inspect {
                files: files(),
                options: MergeOptions { refine: Refine::Ends, ..MergeOptions::default() },
                semantics: SemanticsFlags::default(),
                json: true,
            })
        );
        assert_eq!(
            ok(&["inspect", "b", "l", "r"]),
            Invocation::Batch(Batch::Inspect {
                files: files(),
                options: MergeOptions { refine: Refine::Ends, ..MergeOptions::default() },
                semantics: SemanticsFlags::default(),
                json: false,
            })
        );
    }

    #[test]
    fn diff_forms() {
        assert_eq!(
            ok(&[
                "diff",
                "l",
                "r",
                "--readonly-left",
                "--title",
                "T",
                "--ignore-whitespace",
                "all"
            ]),
            Invocation::Window(Window::Diff(Diff {
                left: "l".into(),
                right: "r".into(),
                diff: DiffOptions {
                    whitespace: diff::Whitespace::All,
                    ..Settings::default().two_way()
                },
                title: Some("T".to_owned()),
                readonly_left: true,
                readonly_right: false,
            }))
        );
        assert_eq!(
            ok(&["diff", "src/a.rs", "--rev", "main"]),
            Invocation::Window(Window::DiffNative {
                path: "src/a.rs".into(),
                rev: Some("main".to_owned()),
                diff: Settings::default().two_way(),
                title: None,
            })
        );
        assert_eq!(
            ok(&["diff", "src/a.rs"]),
            Invocation::Window(Window::DiffNative {
                path: "src/a.rs".into(),
                rev: None,
                diff: Settings::default().two_way(),
                title: None,
            })
        );
        assert_eq!(
            ok(&["diff", "src/a.rs", "--ignore-whitespace", "amount", "--title", "T"]),
            Invocation::Window(Window::DiffNative {
                path: "src/a.rs".into(),
                rev: None,
                diff: DiffOptions {
                    whitespace: diff::Whitespace::Amount,
                    ..Settings::default().two_way()
                },
                title: Some("T".to_owned()),
            })
        );
        assert_eq!(
            ok(&["diff", "--inspect", "l", "r", "--json"]),
            Invocation::Batch(Batch::DiffInspect {
                left: "l".into(),
                right: "r".into(),
                diff: Settings::default().two_way(),
                json: true,
            })
        );
    }

    #[test]
    fn malformed_diffs_are_rejected() {
        assert_eq!(usage(&["diff", "a", "b", "c"]), UsageError::DiffFiles(3));
        assert_eq!(usage(&["diff", "a", "b", "--rev", "x"]), UsageError::RevWithTwoFiles);
        assert_eq!(
            usage(&["diff", "--inspect", "a", "b", "--rev", "x"]),
            UsageError::RevWithTwoFiles
        );
        assert_eq!(usage(&["diff", "--inspect", "a"]), UsageError::InspectNative);
        assert_eq!(clap_kind(&["diff", "a", "b", "--json"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            clap_kind(&["diff", "--inspect", "a", "b", "--readonly-left"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(usage(&["diff"]), UsageError::DiffFiles(0));
    }

    #[test]
    fn a_named_value_is_matched_case_sensitively() {
        for value in ["Myers", "MYERS", "myers_minimal"] {
            assert_eq!(
                clap_kind(&["b", "l", "r", "o", "--diff-algorithm", value]),
                ErrorKind::InvalidValue,
                "{value}"
            );
        }
        assert_eq!(
            clap_kind(&["b", "l", "r", "o", "--merge-semantics", "JJ"]),
            ErrorKind::InvalidValue
        );
    }

    #[test]
    fn help_lists_each_flags_values_and_their_help() {
        let ParseError::Clap(help) = parsed(&["--help"]).expect_err("help") else {
            panic!("clap prints help");
        };
        let text = help.render().to_string();
        for listed in [
            "auto: The caller's VCS, found from the paths\n",
            "myers-minimal",
            "trailing",
            "accept",
            "word",
        ] {
            assert!(text.contains(listed), "{listed} in {text}");
        }
    }

    #[test]
    fn help_points_at_the_config_file() {
        let ParseError::Clap(help) = parsed(&["--help"]).expect_err("help") else {
            panic!("clap prints help");
        };
        assert!(
            help.render()
                .to_string()
                .contains("Every setting, these options among them, is in the config file."),
            "{}",
            help.render()
        );
    }

    #[test]
    fn show_config_takes_default_and_docs() {
        assert_eq!(ok(&["show-config"]), Invocation::ShowConfig { defaults: false, docs: false });
        assert_eq!(
            ok(&["show-config", "--default", "--docs"]),
            Invocation::ShowConfig { defaults: true, docs: true }
        );
        assert_eq!(clap_kind(&["show-config", "--nonsense"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn install_names_the_vcs_and_optionally_the_config_file() {
        assert_eq!(
            ok(&["install", "--git"]),
            Invocation::Batch(Batch::Install { vcs: RepoKind::Git, config: None })
        );
        assert_eq!(
            ok(&["install", "--git", "--config", "c.toml"]),
            Invocation::Batch(Batch::Install { vcs: RepoKind::Git, config: Some("c.toml".into()) })
        );
        assert_eq!(
            ok(&["install", "--jj"]),
            Invocation::Batch(Batch::Install { vcs: RepoKind::Jj, config: None })
        );
        assert_eq!(clap_kind(&["install", "--git", "--jj"]), ErrorKind::ArgumentConflict);
        assert_eq!(clap_kind(&["install"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(clap_kind(&["install", "--git", "extra"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn resolve_takes_an_optional_path_and_the_merge_flags() {
        assert_eq!(
            ok(&["resolve"]),
            Invocation::Window(Window::Resolve(Resolve {
                path: None,
                target: None,
                commands: None,
                options: MergeOptions::default(),
                flags: DiffFlags::default(),
                semantics: SemanticsFlags::default(),
                rev: None,
                dry_run: false,
            }))
        );
        let args = [
            "resolve",
            "src/a.rs",
            "--ignore-whitespace",
            "amount",
            "--merge-semantics",
            "git",
            "--same-change",
            "keep",
            "--hunk-level",
            "word",
            "--marker-length",
            "9",
        ];
        assert_eq!(
            ok(&args),
            Invocation::Window(Window::Resolve(Resolve {
                path: Some("src/a.rs".into()),
                target: None,
                commands: None,
                options: MergeOptions {
                    diff: DiffOptions {
                        whitespace: diff::Whitespace::Amount,
                        ..DiffOptions::default()
                    },
                    marker_length: 9,
                    ..MergeOptions::default()
                },
                flags: DiffFlags { algorithm: None, whitespace: Some(diff::Whitespace::Amount) },
                semantics: SemanticsFlags {
                    choice: Some(Choice::Git),
                    same_change: Some(SameChange::Keep),
                    hunk_level: Some(HunkLevel::Word),
                },
                rev: None,
                dry_run: false,
            }))
        );
    }

    #[test]
    fn resolve_in_a_revision_needs_a_path_and_a_dry_run_needs_headless() {
        let Invocation::Batch(Batch::Resolve(resolve)) =
            ok(&["resolve", "a.txt", "-r", "kx", "--headless", "--dry-run"])
        else {
            panic!("a headless resolve");
        };
        assert_eq!((resolve.rev.as_deref(), resolve.dry_run), (Some("kx"), true));
        let Invocation::Window(Window::Resolve(resolve)) =
            ok(&["resolve", "a.txt", "--revision", "kx"])
        else {
            panic!("a resolve window");
        };
        assert_eq!((resolve.rev.as_deref(), resolve.dry_run), (Some("kx"), false));
        assert_eq!(clap_kind(&["resolve", "-r", "kx"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            clap_kind(&["resolve", "a.txt", "-r", "kx", "--dry-run"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            clap_kind(&["resolve", "a.txt", "--headless", "--dry-run"]),
            ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn stack_takes_an_optional_revset_and_json() {
        assert_eq!(ok(&["stack"]), Invocation::Batch(Batch::Stack { revset: None, json: false }));
        assert_eq!(
            ok(&["stack", "::@", "--json"]),
            Invocation::Batch(Batch::Stack { revset: Some("::@".to_owned()), json: true })
        );
        assert_eq!(clap_kind(&["stack", "a", "b"]), ErrorKind::UnknownArgument);
    }

    #[test]
    fn resolve_runs_headless_with_commands_for_one_path() {
        let Invocation::Batch(Batch::Resolve(resolve)) =
            ok(&["resolve", "a.txt", "--headless", "--commands", "c.jsonl"])
        else {
            panic!("a headless resolve");
        };
        assert_eq!(resolve.commands, Some(PathBuf::from("c.jsonl")));
        let Invocation::Window(Window::Resolve(_)) = ok(&["resolve", "a.txt"]) else {
            panic!("a resolve window");
        };
        assert_eq!(
            usage(&["resolve", "--headless", "--commands", "c.jsonl"]),
            UsageError::CommandsWithoutPath
        );
        assert_eq!(
            clap_kind(&["resolve", "a.txt", "--commands", "c.jsonl"]),
            ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn resolve_takes_one_path_and_its_options_from_the_settings() {
        assert_eq!(clap_kind(&["resolve", "a", "b"]), ErrorKind::UnknownArgument);
        assert_eq!(clap_kind(&["resolve", "--marker-length", "0"]), ErrorKind::ValueValidation);
        let Invocation::Window(Window::Resolve(resolve)) = with(&configured(), &["resolve"]) else {
            panic!("a resolve");
        };
        assert_eq!(resolve.options.diff, configured().diff);
        assert_eq!(resolve.options.refine, configured().refine);
    }

    #[test]
    fn a_subcommand_name_after_a_merge_flag_is_a_file() {
        let Invocation::Batch(Batch::Headless { merge, .. }) =
            ok(&["--headless", "inspect", "b", "l", "r"])
        else {
            panic!("a headless merge");
        };
        assert_eq!(merge.files.base, PathBuf::from("inspect"));
        assert_eq!(merge.output, PathBuf::from("r"));
    }

    /// Trailing whitespace, Myers, no refinement, syntax boundaries: none of them the
    /// defaults.
    fn configured() -> Settings {
        Settings {
            diff: DiffOptions {
                algorithm: diff::Algorithm::Myers,
                whitespace: diff::Whitespace::Trailing,
                ..DiffOptions::default()
            },
            refine: Refine::Off,
            syntax_boundaries: true,
            ..Settings::default()
        }
    }

    fn with(settings: &Settings, args: &[&str]) -> Invocation {
        parse_with(std::iter::once("triad-merge").chain(args.iter().copied()), settings)
            .unwrap_or_else(|e| panic!("{args:?}: {e}"))
    }

    #[test]
    fn the_settings_give_the_diff_options_without_flags() {
        let settings = configured();
        let Invocation::Window(Window::Merge(merge)) = with(&settings, &["b", "l", "r", "o"])
        else {
            panic!("a merge");
        };
        assert_eq!(merge.options.diff, settings.diff);
        assert_eq!(merge.options.refine, Refine::Off);
        assert!(merge.options.syntax_boundaries);
        let Invocation::Window(Window::Diff(d)) = with(&settings, &["diff", "l", "r"]) else {
            panic!("a diff");
        };
        assert_eq!(d.diff, settings.two_way());
        let Invocation::Batch(Batch::Inspect { options, .. }) =
            with(&settings, &["inspect", "b", "l", "r"])
        else {
            panic!("inspect");
        };
        assert_eq!(options.diff, settings.diff);
        assert_eq!(options.refine, settings.refine);
        assert!(options.syntax_boundaries);
        let Invocation::Batch(Batch::DiffInspect { diff, .. }) =
            with(&settings, &["diff", "--inspect", "l", "r"])
        else {
            panic!("diff --inspect");
        };
        assert_eq!(diff, settings.two_way());
    }

    #[test]
    fn two_way_diffs_ignore_line_endings_and_merges_compare_them_by_default() {
        let settings = Settings::default();
        let rule = |invocation| match invocation {
            Invocation::Window(Window::Merge(merge)) => merge.options.diff.line_endings,
            Invocation::Batch(Batch::Inspect { options, .. }) => options.diff.line_endings,
            Invocation::Batch(Batch::DiffInspect { diff, .. })
            | Invocation::Window(
                Window::DiffNative { diff, .. } | Window::Diff(Diff { diff, .. }),
            ) => diff.line_endings,
            other => panic!("{other:?}"),
        };
        let merges = [&["b", "l", "r", "o"][..], &["inspect", "b", "l", "r"]];
        for args in merges {
            assert_eq!(rule(with(&settings, args)), LineEndingRule::Compare, "{args:?}");
        }
        let diffs = [&["diff", "l", "r"][..], &["diff", "--inspect", "l", "r"], &["diff", "path"]];
        for args in diffs {
            assert_eq!(rule(with(&settings, args)), LineEndingRule::Ignore, "{args:?}");
        }
        let flipped = Settings {
            diff: DiffOptions { line_endings: LineEndingRule::Ignore, ..DiffOptions::default() },
            diff_line_endings: LineEndingRule::Compare,
            ..Settings::default()
        };
        let merge = with(&flipped, &["b", "l", "r", "o"]);
        assert_eq!(rule(merge), LineEndingRule::Ignore);
        assert_eq!(rule(with(&flipped, &["diff", "l", "r"])), LineEndingRule::Compare);
    }

    #[test]
    fn a_changed_settings_file_keeps_the_flags_and_marker_length() {
        let Invocation::Window(Window::Merge(merge)) = with(
            &Settings::default(),
            &["b", "l", "r", "o", "--ignore-whitespace", "all", "--marker-length", "9"],
        ) else {
            panic!("a merge");
        };
        assert_eq!(
            merge.options_with(&configured()),
            MergeOptions {
                diff: DiffOptions {
                    algorithm: diff::Algorithm::Myers,
                    whitespace: diff::Whitespace::All,
                    ..DiffOptions::default()
                },
                refine: Refine::Off,
                syntax_boundaries: true,
                marker_length: 9,
                ..MergeOptions::default()
            }
        );
        let Invocation::Window(Window::Merge(plain)) =
            with(&Settings::default(), &["b", "l", "r", "o"])
        else {
            panic!("a merge");
        };
        assert_eq!(
            plain.options_with(&configured()),
            MergeOptions {
                diff: configured().diff,
                refine: Refine::Off,
                syntax_boundaries: true,
                ..MergeOptions::default()
            }
        );
    }

    #[test]
    fn flags_override_the_settings_one_at_a_time() {
        let settings = configured();
        let Invocation::Window(Window::Merge(merge)) =
            with(&settings, &["b", "l", "r", "o", "--ignore-whitespace", "none"])
        else {
            panic!("a merge");
        };
        assert_eq!(merge.options.diff.whitespace, diff::Whitespace::None);
        assert_eq!(merge.options.diff.algorithm, diff::Algorithm::Myers, "kept");
        let Invocation::Window(Window::Diff(d)) =
            with(&settings, &["diff", "l", "r", "--diff-algorithm", "histogram"])
        else {
            panic!("a diff");
        };
        assert_eq!(d.diff.algorithm, diff::Algorithm::Histogram);
        assert_eq!(d.diff.whitespace, diff::Whitespace::Trailing, "kept");
    }
}
