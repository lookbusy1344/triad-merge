//! A merge and diff tool's entries in a jj config file (spec §7.1), written through jj-lib's
//! config layer, and the user config file jj writes to.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use jj_lib::config::{ConfigLayer, ConfigSource, ConfigValue};

const HOME_CONFIG_FILE: &str = ".jjconfig.toml";
const JJ_DIR: &str = "jj";
const CONFIG_FILE: &str = "config.toml";

/// How jj passes a diff tool its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffInvocation {
    /// One run per changed file.
    FileByFile,
    /// One run over two directories.
    Dir,
}

impl DiffInvocation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::FileByFile => "file-by-file",
            Self::Dir => "dir",
        }
    }
}

/// The conflict markers jj writes for a tool that edits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerStyle {
    Diff,
    Snapshot,
    Git,
}

impl MarkerStyle {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Snapshot => "snapshot",
            Self::Git => "git",
        }
    }
}

/// A tool jj runs as `ui.merge-editor` and as `jj diff --tool <name>`: its
/// `merge-tools.<name>` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool<'a> {
    pub name: &'a str,
    pub program: &'a str,
    pub merge_args: &'a [&'a str],
    pub diff_args: &'a [&'a str],
    /// The tool may leave conflict markers in its output, in `marker_style`, and exits with one
    /// of `conflict_exit_codes` when it does.
    pub marker_style: MarkerStyle,
    pub conflict_exit_codes: &'a [i64],
    pub diff_invocation: DiffInvocation,
    /// jj runs the diff tool in its temporary directory, with paths relative to it. `false`
    /// keeps the user's working directory and passes absolute paths.
    pub diff_do_chdir: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolConfigError {
    #[error("the config is not UTF-8")]
    NotUtf8(#[source] std::str::Utf8Error),
    #[error("jj cannot parse the config")]
    Parse(#[source] jj_lib::config::ConfigLoadError),
    #[error("cannot set {key}")]
    Set {
        key: String,
        #[source]
        source: jj_lib::config::ConfigUpdateError,
    },
}

/// `config`, a jj config file's text, with `tool` as the merge editor and its
/// `merge-tools.<name>` table set. Everything else in it is kept.
///
/// # Errors
///
/// [`ToolConfigError`] when jj cannot parse `config`, or a key cannot be set because the file
/// holds a value where jj needs a table.
pub fn set_tool(config: &[u8], tool: &Tool<'_>) -> Result<String, ToolConfigError> {
    let text = std::str::from_utf8(config).map_err(ToolConfigError::NotUtf8)?;
    let mut layer = ConfigLayer::parse(ConfigSource::User, text).map_err(ToolConfigError::Parse)?;
    let strings = |args: &[&str]| args.iter().copied().collect::<ConfigValue>();
    set(&mut layer, &["ui", "merge-editor"], tool.name.into())?;
    let table: [(&str, ConfigValue); 8] = [
        ("program", tool.program.into()),
        ("merge-args", strings(tool.merge_args)),
        ("merge-tool-edits-conflict-markers", true.into()),
        ("merge-conflict-exit-codes", tool.conflict_exit_codes.iter().copied().collect()),
        ("conflict-marker-style", tool.marker_style.as_str().into()),
        ("diff-args", strings(tool.diff_args)),
        ("diff-invocation-mode", tool.diff_invocation.as_str().into()),
        ("diff-do-chdir", tool.diff_do_chdir.into()),
    ];
    for (name, value) in table {
        set(&mut layer, &["merge-tools", tool.name, name], value)?;
    }
    Ok(layer.data.to_string())
}

fn set(layer: &mut ConfigLayer, path: &[&str], value: ConfigValue) -> Result<(), ToolConfigError> {
    layer
        .set_value(path, value)
        .map(drop)
        .map_err(|source| ToolConfigError::Set { key: path.join("."), source })
}

/// Why no user config file can be named.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UserConfigError {
    #[error("{} is a directory; name a config file", .0.display())]
    Directory(PathBuf),
    #[error("JJ_CONFIG names no file, and no home or config directory is known")]
    NoPath,
}

/// The user config file jj writes to: the first path in `jj_config` (`JJ_CONFIG`); else
/// `~/.jjconfig.toml` when it exists or there is no `user_config_dir`; else
/// `<user_config_dir>/jj/config.toml`.
///
/// # Errors
///
/// [`UserConfigError::Directory`] when `JJ_CONFIG`'s first path is a directory, and
/// [`UserConfigError::NoPath`] when no file can be named.
pub fn user_config_path(
    jj_config: Option<&OsStr>,
    home: Option<&Path>,
    user_config_dir: Option<&Path>,
) -> Result<PathBuf, UserConfigError> {
    if let Some(list) = jj_config {
        let first = std::env::split_paths(list)
            .find(|path| !path.as_os_str().is_empty())
            .ok_or(UserConfigError::NoPath)?;
        if first.is_dir() {
            return Err(UserConfigError::Directory(first));
        }
        return Ok(first);
    }
    let home = home.map(|home| home.join(HOME_CONFIG_FILE));
    let platform = user_config_dir.map(|dir| dir.join(JJ_DIR).join(CONFIG_FILE));
    match (home, platform) {
        (Some(home), None) => Ok(home),
        (Some(home), Some(_)) if home.exists() => Ok(home),
        (_, Some(platform)) => Ok(platform),
        (None, None) => Err(UserConfigError::NoPath),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOL: Tool<'static> = Tool {
        name: "t",
        program: "/bin/t",
        merge_args: &["$left", "$right"],
        diff_args: &["$left"],
        marker_style: MarkerStyle::Git,
        conflict_exit_codes: &[3],
        diff_invocation: DiffInvocation::FileByFile,
        diff_do_chdir: false,
    };

    fn set(config: &str) -> String {
        set_tool(config.as_bytes(), &TOOL).expect("set")
    }

    #[test]
    fn an_empty_config_takes_the_tool_table() {
        let after = set("");
        let layer = ConfigLayer::parse(ConfigSource::User, &after).expect("parses");
        let value =
            |key: &'static str| layer.look_up_item(key).expect("found").map(ToString::to_string);
        let trimmed = |key: &'static str| value(key).map(|value| value.trim().to_owned());
        assert_eq!(trimmed("ui.merge-editor").as_deref(), Some(r#""t""#));
        assert_eq!(trimmed("merge-tools.t.program").as_deref(), Some(r#""/bin/t""#));
        assert_eq!(trimmed("merge-tools.t.merge-args").as_deref(), Some(r#"["$left", "$right"]"#));
        assert_eq!(
            trimmed("merge-tools.t.merge-tool-edits-conflict-markers").as_deref(),
            Some("true")
        );
        assert_eq!(trimmed("merge-tools.t.merge-conflict-exit-codes").as_deref(), Some("[3]"));
        assert_eq!(trimmed("merge-tools.t.conflict-marker-style").as_deref(), Some(r#""git""#));
        assert_eq!(trimmed("merge-tools.t.diff-args").as_deref(), Some(r#"["$left"]"#));
        assert_eq!(
            trimmed("merge-tools.t.diff-invocation-mode").as_deref(),
            Some(r#""file-by-file""#)
        );
        assert_eq!(trimmed("merge-tools.t.diff-do-chdir").as_deref(), Some("false"));
    }

    #[test]
    fn other_keys_are_kept() {
        let after = set("[user]\nname = \"x\"\n");
        assert!(after.contains("name = \"x\""), "{after}");
    }

    #[test]
    fn setting_twice_gives_the_same_text() {
        let once = set("");
        assert_eq!(set(&once), once);
    }

    #[test]
    fn a_value_where_jj_needs_a_table_names_the_key() {
        let error = set_tool(b"merge-tools = 1\n", &TOOL).expect_err("refused");
        match error {
            ToolConfigError::Set { key, .. } => assert_eq!(key, "merge-tools.t.program"),
            other => panic!("expected Set, got {other:?}"),
        }
    }

    #[test]
    fn a_config_jj_cannot_parse_is_a_parse_error() {
        let error = set_tool(b"[ui\n", &TOOL).expect_err("refused");
        assert!(matches!(error, ToolConfigError::Parse(_)), "{error:?}");
    }

    #[test]
    fn jj_config_names_its_first_non_empty_path() {
        let list = std::env::join_paths(["", "/a.toml", "/b.toml"]).expect("joins");
        assert_eq!(user_config_path(Some(&list), None, None), Ok(PathBuf::from("/a.toml")));
        assert_eq!(
            user_config_path(Some(OsStr::new("")), Some(Path::new("/h")), None),
            Err(UserConfigError::NoPath)
        );
    }

    #[test]
    fn a_directory_in_jj_config_is_refused() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(
            user_config_path(Some(dir.path().as_os_str()), None, None),
            Err(UserConfigError::Directory(dir.path().to_owned()))
        );
    }

    #[test]
    fn the_platform_file_unless_the_home_file_exists() {
        let home = tempfile::tempdir().expect("dir");
        let platform = Path::new("/cfg");
        assert_eq!(
            user_config_path(None, Some(home.path()), Some(platform)),
            Ok(platform.join(JJ_DIR).join(CONFIG_FILE))
        );
        std::fs::write(home.path().join(HOME_CONFIG_FILE), "").expect("written");
        assert_eq!(
            user_config_path(None, Some(home.path()), Some(platform)),
            Ok(home.path().join(HOME_CONFIG_FILE))
        );
        assert_eq!(user_config_path(None, None, None), Err(UserConfigError::NoPath));
    }
}
