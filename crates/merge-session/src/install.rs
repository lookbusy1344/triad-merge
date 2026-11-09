//! The merge and diff tool entries of spec §7.1 in the user's git or jj config, written by
//! the backend's `tool_config`. `merge_cli::install` names the program and reports.

use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use vcs::RepoKind;

use crate::ExitCode;
use crate::env::Env;
use crate::io::{WriteError, write_atomic};

/// The tool name the entries register.
pub const TOOL: &str = "triad";

/// A config file after an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub path: PathBuf,
    /// Whether the file changed: false when it held the entries already.
    pub changed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("cannot find the home directory")]
    NoHome,
    #[error("{} is a directory; name a config file", .0.display())]
    ConfigDirectory(PathBuf),
    #[error("{} is not a UTF-8 path", .0.display())]
    NotUtf8(PathBuf),
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot parse {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("cannot set {key} in {}: {source}", path.display())]
    Set {
        path: PathBuf,
        key: String,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("cannot make {}: {source}", path.display())]
    Dir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Write(#[from] WriteError),
}

/// The file `git config --global` writes to: `GIT_CONFIG_GLOBAL`; else `~/.gitconfig`, unless
/// only the XDG file (`$XDG_CONFIG_HOME/git/config`) exists.
///
/// # Errors
///
/// [`InstallError::NoHome`] when neither `GIT_CONFIG_GLOBAL` nor a home directory is known.
pub fn git_config_path(env: &Env) -> Result<PathBuf, InstallError> {
    vcs_git::tool_config::global_config_path(
        env.git_config_global.as_deref(),
        env.home.as_deref(),
        env.xdg_config_dir().as_deref(),
    )
    .ok_or(InstallError::NoHome)
}

/// `program` and `args` as one `sh` command line, the form git runs a tool's `cmd` in on
/// every platform, Git for Windows included.
#[must_use]
pub fn git_cmd(program: &str, args: &str) -> String {
    format!("'{}' {args}", program.replace('\'', r"'\''"))
}

/// git's arguments for each tool, from spec §7.1.
const GIT_MERGE_ARGS: &str = r#""$BASE" "$LOCAL" "$REMOTE" "$MERGED" --merge-semantics git"#;
/// `$MERGED`, the file's path in the repository, titles the window and names the file whose
/// line origins two copies find (spec §7.3). `$LOCAL` is a temporary copy git deletes after
/// the run, so its pane is read-only.
const GIT_DIFF_ARGS: &str = r#"diff "$LOCAL" "$REMOTE" --title "$MERGED" --readonly-left"#;

/// Write the spec §7.1 git entries for `program` to the config file at `config`, keeping
/// everything else in it.
///
/// # Errors
///
/// [`InstallError`] when the file cannot be read, parsed or written, or `program` is not
/// UTF-8. The file is then unchanged.
pub fn git(config: &Path, program: &Path) -> Result<Installed, InstallError> {
    use vcs_git::tool_config::{Tool, ToolConfigError, set_tool};

    let program = utf8(program)?;
    let before = read_or_empty(config)?;
    let merge_cmd = git_cmd(program, GIT_MERGE_ARGS);
    let diff_cmd = git_cmd(program, GIT_DIFF_ARGS);
    let tool = Tool { name: TOOL, merge_cmd: &merge_cmd, diff_cmd: &diff_cmd };
    let after = set_tool(&before, &tool).map_err(|error| match error {
        ToolConfigError::Set { key, source } => set_error(config, key, source),
        ToolConfigError::Write(source) => InstallError::Read { path: config.to_owned(), source },
        error @ (ToolConfigError::NotUtf8(_) | ToolConfigError::Parse(_)) => {
            parse_error(config, error)
        }
    })?;
    save(config, &before, &after)
}

fn utf8(path: &Path) -> Result<&str, InstallError> {
    path.to_str().ok_or_else(|| InstallError::NotUtf8(path.to_owned()))
}

fn parse_error(path: &Path, error: impl Error + Send + Sync + 'static) -> InstallError {
    InstallError::Parse { path: path.to_owned(), source: Box::new(error) }
}

fn set_error(path: &Path, key: String, error: impl Error + Send + Sync + 'static) -> InstallError {
    InstallError::Set { path: path.to_owned(), key, source: Box::new(error) }
}

/// The file's bytes, or none when it does not exist yet.
fn read_or_empty(path: &Path) -> Result<Vec<u8>, InstallError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(InstallError::Read { path: path.to_owned(), source }),
    }
}

/// Write `after` over `path` when it differs from `before`, making its directory.
fn save(path: &Path, before: &[u8], after: &[u8]) -> Result<Installed, InstallError> {
    let changed = before != after;
    if changed {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|source| InstallError::Dir { path: dir.to_owned(), source })?;
        }
        write_atomic(path, after)?;
    }
    Ok(Installed { path: path.to_owned(), changed })
}

/// The user config file jj writes to: the first path in `JJ_CONFIG`; else
/// `~/.jjconfig.toml` when it exists; else `jj/config.toml` in the user config directory.
///
/// # Errors
///
/// [`InstallError::ConfigDirectory`] when `JJ_CONFIG` names a directory, and
/// [`InstallError::NoHome`] when no file can be named.
pub fn jj_config_path(env: &Env) -> Result<PathBuf, InstallError> {
    use vcs_jj::tool_config::{UserConfigError, user_config_path};

    user_config_path(env.jj_config.as_deref(), env.home.as_deref(), env.user_config_dir.as_deref())
        .map_err(|error| match error {
            UserConfigError::Directory(path) => InstallError::ConfigDirectory(path),
            UserConfigError::NoPath => InstallError::NoHome,
        })
}

/// jj's arguments for each tool, from spec §7.1.
const JJ_MERGE_ARGS: [&str; 11] = [
    "$base",
    "$left",
    "$right",
    "-o",
    "$output",
    "--title",
    "$path",
    "--marker-length",
    "$marker_length",
    "--merge-semantics",
    "jj",
];
const JJ_DIFF_ARGS: [&str; 3] = ["diff", "$left", "$right"];
/// [`ExitCode::Conflicts`]: jj reads the output after it.
const JJ_CONFLICT_EXIT: i64 = ExitCode::Conflicts as i64;

/// Write the spec §7.1 jj entries for `program` to the config file at `config`, through
/// jj-lib's config layer, keeping everything else in it.
///
/// # Errors
///
/// [`InstallError`] when the file cannot be read, parsed or written, a key cannot be set
/// because the file holds a value where jj needs a table, or `program` is not UTF-8. The file
/// is then unchanged.
pub fn jj(config: &Path, program: &Path) -> Result<Installed, InstallError> {
    use vcs_jj::tool_config::{DiffInvocation, MarkerStyle, Tool, ToolConfigError, set_tool};

    let program = utf8(program)?;
    let before = read_or_empty(config)?;
    let tool = Tool {
        name: TOOL,
        program,
        merge_args: &JJ_MERGE_ARGS,
        diff_args: &JJ_DIFF_ARGS,
        marker_style: MarkerStyle::Git,
        conflict_exit_codes: &[JJ_CONFLICT_EXIT],
        diff_invocation: DiffInvocation::FileByFile,
    };
    let after = set_tool(&before, &tool).map_err(|error| match error {
        ToolConfigError::Set { key, source } => set_error(config, key, source),
        error @ (ToolConfigError::NotUtf8(_) | ToolConfigError::Parse(_)) => {
            parse_error(config, error)
        }
    })?;
    save(config, &before, after.as_bytes())
}

/// Write the entries for `program` to `config`, or to the user's config file `env` names.
///
/// # Errors
///
/// As [`git`] and [`jj`], and as [`git_config_path`] and [`jj_config_path`] without `config`.
pub fn install(
    vcs: RepoKind,
    config: Option<&Path>,
    program: &Path,
    env: &Env,
) -> Result<Installed, InstallError> {
    let path = match (config, vcs) {
        (Some(path), _) => path.to_owned(),
        (None, RepoKind::Git) => git_config_path(env)?,
        (None, RepoKind::Jj) => jj_config_path(env)?,
    };
    match vcs {
        RepoKind::Git => git(&path, program),
        RepoKind::Jj => jj(&path, program),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_path_is_single_quoted_before_the_arguments() {
        assert_eq!(
            git_cmd("/opt/triad/triad-merge", r#""$LOCAL" "$REMOTE""#),
            r#"'/opt/triad/triad-merge' "$LOCAL" "$REMOTE""#
        );
    }

    #[test]
    fn a_windows_path_keeps_its_backslashes_and_spaces() {
        assert_eq!(
            git_cmd(r"C:\Program Files\triad\triad-merge.exe", r#"diff "$LOCAL" "$REMOTE""#),
            r#"'C:\Program Files\triad\triad-merge.exe' diff "$LOCAL" "$REMOTE""#
        );
    }

    #[test]
    fn a_single_quote_in_the_path_is_escaped_for_sh() {
        assert_eq!(git_cmd("/home/o'brien/triad-merge", "x"), r"'/home/o'\''brien/triad-merge' x");
    }
}
