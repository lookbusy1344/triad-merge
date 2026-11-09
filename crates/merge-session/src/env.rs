//! The process environment that decides where git's, jj's and triad's config files live.

use std::ffi::OsString;
use std::path::PathBuf;

/// The process environment that decides where config files live.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub home: Option<PathBuf>,
    /// `XDG_CONFIG_HOME`.
    pub xdg_config_home: Option<PathBuf>,
    /// `GIT_CONFIG_GLOBAL`.
    pub git_config_global: Option<PathBuf>,
    /// `JJ_CONFIG`.
    pub jj_config: Option<OsString>,
    /// The platform's user config directory as jj finds it: `XDG_CONFIG_HOME` or `~/.config`
    /// on Unix and macOS, `%APPDATA%` on Windows.
    pub user_config_dir: Option<PathBuf>,
    /// jj's system config directory: `/etc/jj` on Unix, none on Windows.
    pub jj_system_config_dir: Option<PathBuf>,
}

/// jj's system config directory on Unix.
const JJ_SYSTEM_CONFIG_DIR: &str = "/etc/jj";

impl Env {
    /// The environment of this process. Empty variables count as unset, as git and jj treat
    /// them.
    #[must_use]
    pub fn from_process() -> Self {
        Self::from_vars(
            |name| std::env::var_os(name),
            || directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned()),
        )
    }

    /// The environment that `lookup` gives, with `platform_home` as the home directory when
    /// `HOME` is unset.
    fn from_vars(
        lookup: impl Fn(&str) -> Option<OsString>,
        platform_home: impl FnOnce() -> Option<PathBuf>,
    ) -> Self {
        let var = |name| lookup(name).filter(|value| !value.is_empty());
        let mut env = Self {
            home: var("HOME").map(PathBuf::from).or_else(platform_home),
            xdg_config_home: var("XDG_CONFIG_HOME").map(PathBuf::from),
            git_config_global: var("GIT_CONFIG_GLOBAL").map(PathBuf::from),
            jj_config: var("JJ_CONFIG"),
            user_config_dir: None,
            jj_system_config_dir: cfg!(unix).then(|| PathBuf::from(JJ_SYSTEM_CONFIG_DIR)),
        };
        env.user_config_dir = if cfg!(windows) {
            directories::BaseDirs::new().map(|dirs| dirs.config_dir().to_owned())
        } else {
            // jj, through etcetera, takes XDG_CONFIG_HOME only when it is absolute.
            env.xdg_config_home
                .as_ref()
                .filter(|dir| dir.is_absolute())
                .cloned()
                .or_else(|| env.home.as_ref().map(|home| home.join(".config")))
        };
        env
    }

    /// `XDG_CONFIG_HOME`, or `~/.config` without it.
    #[must_use]
    pub fn xdg_config_dir(&self) -> Option<PathBuf> {
        self.xdg_config_home.clone().or_else(|| self.home.as_ref().map(|home| home.join(".config")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`Env::from_vars`] over `vars`, with `/platform/home` as the platform's home.
    fn env_of(vars: &[(&str, &str)]) -> Env {
        Env::from_vars(
            |name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| OsString::from(v)),
            || Some(PathBuf::from("/platform/home")),
        )
    }

    #[cfg(unix)]
    #[test]
    fn the_environment_gives_each_config_location() {
        let env = env_of(&[
            ("HOME", "/h"),
            ("XDG_CONFIG_HOME", "/x"),
            ("GIT_CONFIG_GLOBAL", "/g/config"),
            ("JJ_CONFIG", "/j.toml"),
        ]);
        assert_eq!(
            env,
            Env {
                home: Some(PathBuf::from("/h")),
                xdg_config_home: Some(PathBuf::from("/x")),
                git_config_global: Some(PathBuf::from("/g/config")),
                jj_config: Some(OsString::from("/j.toml")),
                user_config_dir: Some(PathBuf::from("/x")),
                jj_system_config_dir: Some(PathBuf::from("/etc/jj")),
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn empty_variables_count_as_unset() {
        let env = env_of(&[
            ("HOME", ""),
            ("XDG_CONFIG_HOME", ""),
            ("GIT_CONFIG_GLOBAL", ""),
            ("JJ_CONFIG", ""),
        ]);
        assert_eq!(env.home, Some(PathBuf::from("/platform/home")));
        assert_eq!([&env.xdg_config_home, &env.git_config_global], [&None, &None]);
        assert_eq!(env.jj_config, None);
        assert_eq!(env.user_config_dir, Some(PathBuf::from("/platform/home/.config")));
    }

    #[cfg(unix)]
    #[test]
    fn a_relative_xdg_config_home_is_not_jjs_user_config_dir() {
        let env = env_of(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "rel")]);
        assert_eq!(env.xdg_config_home, Some(PathBuf::from("rel")));
        assert_eq!(env.user_config_dir, Some(PathBuf::from("/h/.config")));
    }
}
