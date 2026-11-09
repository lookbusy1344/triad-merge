//! A merge and diff tool's entries in a git config file (spec §7.1), and the file
//! `git config --global` writes to.

use std::path::{Path, PathBuf};

const GITCONFIG: &str = ".gitconfig";
const XDG_GIT_DIR: &str = "git";
const XDG_CONFIG_FILE: &str = "config";

/// A tool git runs for `git mergetool` and `git difftool`. Each command is one `sh` command
/// line, as git runs a tool's `cmd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool<'a> {
    pub name: &'a str,
    pub merge_cmd: &'a str,
    pub diff_cmd: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolConfigError {
    #[error("the config is not UTF-8")]
    NotUtf8(#[source] std::str::Utf8Error),
    #[error("git cannot parse the config")]
    Parse(#[source] gix::config::parse::Error),
    #[error("cannot set {key}")]
    Set {
        key: String,
        #[source]
        source: gix::Error,
    },
    #[error("cannot write the config")]
    Write(#[source] std::io::Error),
}

/// `config`, a git config file's bytes, with `tool` as the merge and diff tool: `merge.tool`,
/// `mergetool.<name>.cmd`, `mergetool.<name>.trustExitCode`, `diff.tool` and
/// `difftool.<name>.cmd`. Everything else in it is kept, comments included.
///
/// # Errors
///
/// [`ToolConfigError`] when git cannot parse `config` or a key cannot be set.
pub fn set_tool(config: &[u8], tool: &Tool<'_>) -> Result<Vec<u8>, ToolConfigError> {
    let text = std::str::from_utf8(config).map_err(ToolConfigError::NotUtf8)?;
    let mut file = gix::config::File::try_from(text).map_err(ToolConfigError::Parse)?;
    let merge_cmd = format!("mergetool.{}.cmd", tool.name);
    let trust = format!("mergetool.{}.trustExitCode", tool.name);
    let diff_cmd = format!("difftool.{}.cmd", tool.name);
    let entries = [
        ("merge.tool", tool.name),
        (merge_cmd.as_str(), tool.merge_cmd),
        (trust.as_str(), "true"),
        ("diff.tool", tool.name),
        (diff_cmd.as_str(), tool.diff_cmd),
    ];
    for (key, value) in entries {
        file.set_raw_value(key, value)
            .map_err(|source| ToolConfigError::Set { key: key.to_owned(), source })?;
    }
    let mut after = Vec::with_capacity(config.len());
    file.write_to(&mut after).map_err(ToolConfigError::Write)?;
    Ok(after)
}

/// The file `git config --global` writes to: `global` (`GIT_CONFIG_GLOBAL`); else
/// `~/.gitconfig`, unless only `<xdg_config_dir>/git/config` exists. None without either
/// `global` or `home`.
#[must_use]
pub fn global_config_path(
    global: Option<&Path>,
    home: Option<&Path>,
    xdg_config_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = global {
        return Some(path.to_owned());
    }
    let gitconfig = home.map(|home| home.join(GITCONFIG));
    let xdg = xdg_config_dir.map(|dir| dir.join(XDG_GIT_DIR).join(XDG_CONFIG_FILE));
    match (gitconfig, xdg) {
        (Some(gitconfig), _) if gitconfig.exists() => Some(gitconfig),
        (_, Some(xdg)) if xdg.exists() => Some(xdg),
        (gitconfig, _) => gitconfig,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOOL: Tool<'static> = Tool { name: "t", merge_cmd: "m $MERGED", diff_cmd: "d $LOCAL" };

    fn set(config: &str) -> String {
        String::from_utf8(set_tool(config.as_bytes(), &TOOL).expect("set")).expect("UTF-8")
    }

    #[test]
    fn an_empty_config_takes_the_five_entries() {
        let file = gix::config::File::try_from(set("").as_str()).expect("parses");
        let value = |key: &str| file.string(key).map(|value| value.to_string());
        assert_eq!(value("merge.tool").as_deref(), Some("t"));
        assert_eq!(value("mergetool.t.cmd").as_deref(), Some("m $MERGED"));
        assert_eq!(file.boolean("mergetool.t.trustExitCode").ok(), Some(Some(true)));
        assert_eq!(value("diff.tool").as_deref(), Some("t"));
        assert_eq!(value("difftool.t.cmd").as_deref(), Some("d $LOCAL"));
    }

    #[test]
    fn comments_and_other_keys_are_kept() {
        let after = set("# mine\n[user]\n\tname = x\n");
        assert!(after.starts_with("# mine\n[user]\n\tname = x\n"), "{after}");
    }

    #[test]
    fn setting_twice_gives_the_same_bytes() {
        let once = set("");
        assert_eq!(set(&once), once);
    }

    #[test]
    fn a_config_git_cannot_parse_is_a_parse_error() {
        let error = set_tool(b"[merge\n", &TOOL).expect_err("refused");
        assert!(matches!(error, ToolConfigError::Parse(_)), "{error:?}");
    }

    #[test]
    fn a_config_that_is_not_utf8_is_refused() {
        let error = set_tool(&[b'#', 0xff, b'\n'], &TOOL).expect_err("refused");
        assert!(matches!(error, ToolConfigError::NotUtf8(_)), "{error:?}");
    }

    #[test]
    fn the_global_config_wins_and_none_without_a_home() {
        let global = Path::new("/elsewhere");
        assert_eq!(
            global_config_path(Some(global), Some(Path::new("/h")), None),
            Some(global.to_owned())
        );
        assert_eq!(global_config_path(None, None, Some(Path::new("/x"))), None);
    }

    #[test]
    fn without_either_file_the_home_gitconfig_is_named() {
        let home = tempfile::tempdir().expect("dir");
        assert_eq!(
            global_config_path(None, Some(home.path()), Some(home.path())),
            Some(home.path().join(GITCONFIG))
        );
    }

    #[test]
    fn only_the_xdg_file_existing_names_it() {
        let home = tempfile::tempdir().expect("dir");
        let xdg = home.path().join(XDG_GIT_DIR).join(XDG_CONFIG_FILE);
        std::fs::create_dir_all(xdg.parent().expect("parent")).expect("dirs");
        std::fs::write(&xdg, "").expect("written");
        assert_eq!(global_config_path(None, Some(home.path()), Some(home.path())), Some(xdg));
        let gitconfig = home.path().join(GITCONFIG);
        std::fs::write(&gitconfig, "").expect("written");
        assert_eq!(
            global_config_path(None, Some(home.path()), Some(home.path())),
            Some(gitconfig),
            "an existing ~/.gitconfig wins"
        );
    }
}
