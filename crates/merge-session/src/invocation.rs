//! The forms a merge or diff takes, as the command line gives them (spec §6.1), and the ones
//! that open a window.

use std::path::{Path, PathBuf};

use merge_core::diff::{self, DiffOptions};
use merge_core::document::{Labels, MergeOptions};
use merge_core::merge3::{HunkLevel, SameChange};
use merge_syntax::language::Language;

use merge_config::settings::Choice;
use merge_config::settings::Settings;

/// A form that needs a window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Window {
    /// No arguments: the open dialog.
    OpenDialog,
    /// The merge window.
    Merge(Merge),
    /// The two-way diff window.
    Diff(Diff),
    /// A working-copy file against its parent revision, or `rev`.
    DiffNative { path: PathBuf, rev: Option<String>, diff: DiffOptions, title: Option<String> },
    /// A conflict read from the repository at the working directory: `path`'s, or the first
    /// conflicted file's.
    Resolve(Resolve),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeFiles {
    pub base: PathBuf,
    pub left: PathBuf,
    pub right: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merge {
    pub files: MergeFiles,
    pub output: PathBuf,
    pub options: MergeOptions,
    /// The diff flags given, which hold over the settings file for the whole run.
    pub flags: DiffFlags,
    /// `--merge-semantics` and `--same-change`, where given. The run chooses the semantics once,
    /// before the merge opens, and sets them in `options`.
    pub semantics: SemanticsFlags,
    pub labels: Labels,
    pub title: Option<String>,
}

impl MergeFiles {
    /// The inputs' paths: left, base, right.
    #[must_use]
    pub fn paths(&self) -> [&Path; 3] {
        [&self.left, &self.base, &self.right]
    }
}

impl Merge {
    /// The language the file names give, as [`Language::of_run`] finds it.
    #[must_use]
    pub fn language(&self) -> Option<Language> {
        Language::of_run(self.files.paths(), Some(&self.output), self.title.as_deref())
    }

    /// The options this merge takes from `settings`: the settings file's, with the command
    /// line's flags and marker length over them (spec §6.4).
    #[must_use]
    pub fn options_with(&self, settings: &Settings) -> MergeOptions {
        self.flags.merge_options(settings, self.options)
    }
}

/// `--merge-semantics` and `--same-change`, where given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SemanticsFlags {
    pub choice: Option<Choice>,
    pub same_change: Option<SameChange>,
    pub hunk_level: Option<HunkLevel>,
}

/// `--diff-algorithm` and `--ignore-whitespace`, where given.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffFlags {
    pub algorithm: Option<diff::Algorithm>,
    pub whitespace: Option<diff::Whitespace>,
}

impl DiffFlags {
    /// `settings` with the flags given applied.
    #[must_use]
    pub fn over(self, settings: DiffOptions) -> DiffOptions {
        DiffOptions {
            algorithm: self.algorithm.unwrap_or(settings.algorithm),
            whitespace: self.whitespace.unwrap_or(settings.whitespace),
            line_endings: settings.line_endings,
        }
    }

    /// A merge's options from `settings`, with these flags over them. The marker length,
    /// semantics and same-change rule stay `chosen`'s: they hold for the whole run.
    #[must_use]
    pub fn merge_options(self, settings: &Settings, chosen: MergeOptions) -> MergeOptions {
        MergeOptions {
            diff: self.over(settings.diff),
            refine: settings.refine,
            syntax_boundaries: settings.syntax_boundaries,
            ..chosen
        }
    }
}

/// `resolve [PATH]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolve {
    /// The path given; without one, each conflicted file in turn.
    pub path: Option<PathBuf>,
    /// The file to open first, absolute or relative to the working directory: set before
    /// the window opens.
    pub target: Option<PathBuf>,
    /// With `--headless`: the merge commands applied to each file before it is written.
    pub commands: Option<PathBuf>,
    pub options: MergeOptions,
    /// The diff flags given, which hold over the settings file for the whole run.
    pub flags: DiffFlags,
    /// As [`Merge::semantics`].
    pub semantics: SemanticsFlags,
    /// `-r`: the jj revision whose commit holds the conflict; none for the working copy.
    pub rev: Option<String>,
    /// `--pair N`: merge sides N and N + 1 of a jj conflict of more than two sides, counted
    /// from 1.
    pub pair: Option<usize>,
    /// `--dry-run`: report what writing into the commit would do, and write nothing.
    pub dry_run: bool,
}

/// The revision `resolve --pair` reads without `-r`: the working copy's commit.
pub const WORKING_COPY_REV: &str = "@";

impl Resolve {
    /// The revision whose commit `resolve` writes into: `-r`'s, or the working copy's for
    /// `--pair`. None for a conflict on disk.
    #[must_use]
    pub fn in_commit(&self) -> Option<&str> {
        self.rev.as_deref().or(self.pair.map(|_| WORKING_COPY_REV))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub left: PathBuf,
    pub right: PathBuf,
    pub diff: DiffOptions,
    pub title: Option<String>,
    pub readonly_left: bool,
    pub readonly_right: bool,
}
