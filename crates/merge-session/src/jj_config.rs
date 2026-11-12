//! `merge.same-change` and `merge.hunk-level` from jj's config, for merges with jj's semantics
//! (`docs/DESIGN.md` §B3). `vcs_jj::config` layers jj's files as jj-cli does; this module maps
//! the process environment onto it and its values onto the engine's.

use std::collections::HashMap;
use std::path::Path;

use merge_core::merge3::{HunkLevel, SameChange};
pub use vcs_jj::config::ConfigEnv;
use vcs_jj::config::{self, ConfigError, FileMergeHunkLevel};

use crate::env::Env;

/// The variable naming the files that replace jj's system and user config.
const JJ_CONFIG: &str = "JJ_CONFIG";
/// The variable jj reads git's global ignore file below.
const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

/// jj's merge settings, each none when no file sets it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JjMerge {
    pub same_change: Option<SameChange>,
    pub hunk_level: Option<HunkLevel>,
}

/// jj's merge settings for a merge in the workspace at `workspace`, or with user and system
/// config only without one.
///
/// # Errors
///
/// [`ConfigError`] when a config file cannot be read or parsed, or sets a key to a value jj
/// does not accept; the error names the file and key.
pub fn merge_config(env: &Env, workspace: Option<&Path>) -> Result<JjMerge, ConfigError> {
    let settings = config::merge_settings(&config_env(env), workspace)?;
    Ok(JjMerge {
        same_change: settings.same_change.map(|value| match value {
            config::SameChange::Accept => SameChange::Accept,
            config::SameChange::Keep => SameChange::Keep,
        }),
        hunk_level: settings.hunk_level.map(|value| match value {
            FileMergeHunkLevel::Line => HunkLevel::Line,
            FileMergeHunkLevel::Word => HunkLevel::Word,
        }),
    })
}

/// What jj's snapshots for a run in `env` read: the process's host, user and `JJ_*`
/// variables, with `env`'s directories, `JJ_CONFIG` and `XDG_CONFIG_HOME` in place of the
/// process's.
#[must_use]
pub fn snapshot_config(env: &Env) -> ConfigEnv {
    let process = ConfigEnv::from_process();
    let dirs = config_env(env);
    let mut environment: HashMap<String, String> = process
        .environment
        .into_iter()
        .filter(|(name, _)| name != JJ_CONFIG && name != XDG_CONFIG_HOME)
        .chain(dirs.environment.clone())
        .collect();
    if let Some(dir) = env.xdg_config_home.as_ref().and_then(|dir| dir.to_str()) {
        environment.insert(XDG_CONFIG_HOME.to_owned(), dir.to_owned());
    }
    ConfigEnv { environment, hostname: process.hostname, username: process.username, ..dirs }
}

/// The directories and `JJ_CONFIG` of `env`, as jj's config reads them. Without a host name,
/// `--when.hostnames` scopes do not apply.
fn config_env(env: &Env) -> ConfigEnv {
    let jj_config = env.jj_config.as_ref().and_then(|list| {
        list.to_str()
            .or_else(|| {
                tracing::warn!("JJ_CONFIG is not UTF-8; jj's default config files are read");
                None
            })
            .map(|list| (JJ_CONFIG.to_owned(), list.to_owned()))
    });
    ConfigEnv {
        home_dir: env.home.clone(),
        config_dir: env.user_config_dir.clone(),
        jj_system_config_dir: env.jj_system_config_dir.clone(),
        environment: jj_config.into_iter().collect(),
        ..ConfigEnv::default()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    /// A home with `.config` as jj's user config directory, `etc/jj` as its system directory,
    /// and a workspace.
    struct World {
        dir: tempfile::TempDir,
    }

    impl World {
        fn new() -> Self {
            let world = Self { dir: tempfile::tempdir().unwrap() };
            fs::create_dir_all(world.workspace().join(".jj/repo")).unwrap();
            world
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.dir.path().join(relative)
        }

        fn workspace(&self) -> PathBuf {
            self.path("ws")
        }

        fn env(&self) -> Env {
            Env {
                home: Some(self.path("home")),
                user_config_dir: Some(self.path("home/.config")),
                jj_system_config_dir: Some(self.path("etc/jj")),
                ..Env::default()
            }
        }

        fn read(&self, env: &Env) -> JjMerge {
            merge_config(env, Some(&self.workspace())).unwrap()
        }
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn each_jj_value_maps_to_the_engines() {
        let w = World::new();
        let user = w.path("home/.config/jj/config.toml");
        for (text, expected) in [
            (
                "merge.same-change = \"keep\"\nmerge.hunk-level = \"word\"\n",
                JjMerge { same_change: Some(SameChange::Keep), hunk_level: Some(HunkLevel::Word) },
            ),
            (
                "merge.same-change = \"accept\"\nmerge.hunk-level = \"line\"\n",
                JjMerge {
                    same_change: Some(SameChange::Accept),
                    hunk_level: Some(HunkLevel::Line),
                },
            ),
            ("", JjMerge::default()),
        ] {
            write(&user, text);
            assert_eq!(w.read(&w.env()), expected, "{text}");
        }
    }

    #[test]
    fn the_home_and_system_directories_come_from_the_environment() {
        let w = World::new();
        write(&w.path("etc/jj/config.toml"), "merge.same-change = \"keep\"\n");
        assert_eq!(w.read(&w.env()).same_change, Some(SameChange::Keep));
        write(&w.path("home/.jjconfig.toml"), "merge.same-change = \"accept\"\n");
        assert_eq!(w.read(&w.env()).same_change, Some(SameChange::Accept));
    }

    #[test]
    fn jj_config_from_the_environment_replaces_the_user_and_system_files() {
        let w = World::new();
        write(&w.path("home/.config/jj/config.toml"), "merge.same-change = \"keep\"\n");
        let only = w.path("only.toml");
        write(&only, "merge.hunk-level = \"word\"\n");
        let env = Env { jj_config: Some(only.into_os_string()), ..w.env() };
        assert_eq!(w.read(&env), JjMerge { same_change: None, hunk_level: Some(HunkLevel::Word) });
    }

    #[test]
    fn an_error_names_the_file_and_key() {
        let w = World::new();
        let file = w.path("home/.config/jj/config.toml");
        write(&file, "merge.same-change = \"sometimes\"\n");
        let error = merge_config(&w.env(), None).unwrap_err();
        assert!(
            matches!(&error, ConfigError::Value { key: config::SAME_CHANGE, path: Some(p), .. } if *p == file),
            "{error:?}"
        );
    }

    #[test]
    fn a_snapshot_reads_the_runs_directories_jj_config_and_xdg_home() {
        let world = World::new();
        let env = Env {
            jj_config: Some("/run/one.toml:/run/two.toml".into()),
            xdg_config_home: Some(world.path("xdg")),
            ..world.env()
        };
        let config = snapshot_config(&env);
        assert_eq!(config.home_dir, Some(world.path("home")));
        assert_eq!(config.config_dir, Some(world.path("home/.config")));
        assert_eq!(config.jj_system_config_dir, Some(world.path("etc/jj")));
        assert_eq!(
            config.environment.get(JJ_CONFIG).map(String::as_str),
            Some("/run/one.toml:/run/two.toml")
        );
        assert_eq!(
            config.environment.get(XDG_CONFIG_HOME).map(PathBuf::from),
            Some(world.path("xdg"))
        );
    }

    #[test]
    fn without_them_a_snapshot_reads_no_jj_config_or_xdg_home_from_the_process() {
        let config = snapshot_config(&World::new().env());
        assert_eq!(config.environment.get(JJ_CONFIG), None);
        assert_eq!(config.environment.get(XDG_CONFIG_HOME), None);
    }

    #[test]
    fn a_snapshot_keeps_the_processs_host_and_user() {
        let process = ConfigEnv::from_process();
        let config = snapshot_config(&World::new().env());
        assert_eq!(config.hostname, process.hostname);
        assert_eq!(config.username, process.username);
    }
}
