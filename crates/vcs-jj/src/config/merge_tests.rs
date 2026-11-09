//! [`merge_settings`] on config files in temporary directories (spec §6.1, `PLAN.md` §B3).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    CONFIG_ID_LENGTH, ConfigEnv, ConfigError, FileMergeHunkLevel, HUNK_LEVEL, MergeSettings,
    SAME_CHANGE, SameChange, merge_settings,
};

/// A home directory with `.config` as jj's user config directory, and a workspace whose
/// `.jj/repo` is a bare directory: reading merge settings loads no repository.
struct World {
    dir: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        let world = Self { dir: tempfile::tempdir().unwrap() };
        fs::create_dir_all(world.workspace().join(".jj/repo")).unwrap();
        world
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn platform(&self) -> PathBuf {
        self.home().join(".config/jj")
    }

    fn workspace(&self) -> PathBuf {
        self.dir.path().join("ws")
    }

    fn env(&self) -> ConfigEnv {
        ConfigEnv {
            home_dir: Some(self.home()),
            config_dir: Some(self.home().join(".config")),
            jj_system_config_dir: Some(self.dir.path().join("etc/jj")),
            ..ConfigEnv::default()
        }
    }

    fn with_jj_config(&self, list: Vec<PathBuf>) -> ConfigEnv {
        let list = std::env::join_paths(list).unwrap().into_string().unwrap();
        ConfigEnv { environment: HashMap::from([("JJ_CONFIG".to_owned(), list)]), ..self.env() }
    }

    fn read(&self, env: &ConfigEnv) -> Option<SameChange> {
        same_change(env, Some(&self.workspace()))
    }

    fn error(&self) -> ConfigError {
        merge_settings(&self.env(), Some(&self.workspace())).unwrap_err()
    }
}

fn same_change(env: &ConfigEnv, workspace: Option<&Path>) -> Option<SameChange> {
    merge_settings(env, workspace).unwrap().same_change
}

/// A config file at `path` setting `merge.same-change` to `value`.
fn write(path: &Path, value: &str) {
    write_text(path, &format!("[merge]\nsame-change = \"{value}\"\n"));
}

fn write_text(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn no_config_leaves_the_key_unset() {
    let w = World::new();
    assert_eq!(w.read(&w.env()), None);
    assert_eq!(same_change(&w.env(), None), None);
}

#[test]
fn the_user_config_sets_the_key() {
    let w = World::new();
    write(&w.platform().join("config.toml"), "keep");
    assert_eq!(w.read(&w.env()), Some(SameChange::Keep));
    assert_eq!(same_change(&w.env(), None), Some(SameChange::Keep));
}

#[test]
fn later_user_files_win_and_conf_d_comes_last() {
    let w = World::new();
    write(&w.home().join(".jjconfig.toml"), "keep");
    assert_eq!(w.read(&w.env()), Some(SameChange::Keep));
    write(&w.platform().join("config.toml"), "accept");
    assert_eq!(w.read(&w.env()), Some(SameChange::Accept));
    write(&w.platform().join("conf.d/10-merge.toml"), "keep");
    assert_eq!(w.read(&w.env()), Some(SameChange::Keep));
}

#[test]
fn the_system_config_yields_to_the_user_config() {
    let w = World::new();
    write(&w.dir.path().join("etc/jj/config.toml"), "keep");
    assert_eq!(w.read(&w.env()), Some(SameChange::Keep));
    write(&w.platform().join("config.toml"), "accept");
    assert_eq!(w.read(&w.env()), Some(SameChange::Accept));
}

#[test]
fn jj_config_replaces_the_user_and_system_files() {
    let w = World::new();
    write(&w.platform().join("config.toml"), "keep");
    write(&w.dir.path().join("etc/jj/config.toml"), "keep");
    let only = w.dir.path().join("only.toml");
    write(&only, "accept");
    let dir = w.dir.path().join("dir");
    write(&dir.join("a.toml"), "keep");
    assert_eq!(w.read(&w.with_jj_config(vec![only.clone()])), Some(SameChange::Accept));
    assert_eq!(w.read(&w.with_jj_config(vec![only.clone(), dir.clone()])), Some(SameChange::Keep));
    assert_eq!(w.read(&w.with_jj_config(vec![dir, only])), Some(SameChange::Accept));
    assert_eq!(w.read(&w.with_jj_config(vec![w.dir.path().join("missing.toml")])), None);
}

#[test]
fn the_repo_config_by_id_wins_over_the_user_config() {
    let w = World::new();
    write(&w.platform().join("config.toml"), "keep");
    fs::write(w.workspace().join(".jj/repo/config-id"), "a422d4a1224a24b4b7d3").unwrap();
    write(&w.platform().join("repos/a422d4a1224a24b4b7d3/config.toml"), "accept");
    // The legacy file is ignored once an ID exists.
    write(&w.workspace().join(".jj/repo/config.toml"), "keep");
    assert_eq!(w.read(&w.env()), Some(SameChange::Accept));
}

#[test]
fn reading_writes_nothing() {
    // jj-lib's secure config would write metadata beside the ID's file and migrate the legacy
    // workspace file; a merge only reads.
    let w = World::new();
    let id = "a422d4a1224a24b4b7d3";
    fs::write(w.workspace().join(".jj/repo/config-id"), id).unwrap();
    let repo_config = w.platform().join("repos").join(id).join("config.toml");
    write(&repo_config, "accept");
    let legacy = w.workspace().join(".jj/workspace-config.toml");
    write(&legacy, "keep");
    assert_eq!(w.read(&w.env()), Some(SameChange::Keep));
    let listing = |dir: &Path| {
        let mut names: Vec<_> =
            fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name()).collect();
        names.sort();
        names
    };
    assert_eq!(listing(repo_config.parent().unwrap()), ["config.toml"]);
    assert_eq!(listing(&w.platform()), ["repos"]);
    assert!(legacy.is_file() && !legacy.is_symlink());
}

#[test]
fn an_unreadable_config_id_fails_and_names_the_file() {
    // Only a missing ID file means "use the legacy config"; any other read error stops.
    let w = World::new();
    let id = w.workspace().join(".jj/repo/config-id");
    fs::create_dir(&id).unwrap();
    write(&w.workspace().join(".jj/repo/config.toml"), "accept");
    let error = w.error();
    assert!(matches!(&error, ConfigError::Read { path, .. } if *path == id), "{error:?}");
}

/// The ID file `file` of the workspace, holding `id`.
fn with_id(w: &World, file: &str, id: &str) -> PathBuf {
    let path = w.workspace().join(file);
    fs::write(&path, id).unwrap();
    path
}

#[test]
fn an_id_that_is_not_twenty_hex_digits_fails_and_names_the_file() {
    let too_short = "a".repeat(CONFIG_ID_LENGTH - 1);
    let too_long = "a".repeat(CONFIG_ID_LENGTH + 1);
    let not_hex = "g".repeat(CONFIG_ID_LENGTH);
    let newline = format!("{}\n", "a".repeat(CONFIG_ID_LENGTH));
    let same_length_path = format!("../../{}", "a".repeat(CONFIG_ID_LENGTH - "../../".len()));
    let bad = [
        "",
        too_short.as_str(),
        too_long.as_str(),
        not_hex.as_str(),
        newline.as_str(),
        same_length_path.as_str(),
        "../../../outside",
        "/etc/passwd",
    ];
    for file in [".jj/repo/config-id", ".jj/workspace-config-id"] {
        for id in bad {
            let w = World::new();
            let path = with_id(&w, file, id);
            let error = w.error();
            assert!(
                matches!(&error, ConfigError::BadId { path: named } if *named == path),
                "{file} holding {id:?}: {error:?}"
            );
        }
    }
}

#[test]
fn an_id_cannot_reach_a_config_outside_jjs_directories() {
    let w = World::new();
    write(&w.dir.path().join("planted/config.toml"), "keep");
    fs::create_dir_all(w.platform().join("repos")).unwrap();
    with_id(&w, ".jj/repo/config-id", "../../../../planted");
    assert!(matches!(w.error(), ConfigError::BadId { .. }));
}

#[test]
fn an_id_in_upper_case_hex_is_an_id() {
    let w = World::new();
    let id = "A422D4A1224A24B4B7D3";
    with_id(&w, ".jj/repo/config-id", id);
    write(&w.platform().join("repos").join(id).join("config.toml"), "keep");
    assert_eq!(w.read(&w.env()), Some(SameChange::Keep));
}

#[test]
fn the_legacy_repo_config_counts_without_an_id() {
    let w = World::new();
    write(&w.platform().join("config.toml"), "keep");
    write(&w.workspace().join(".jj/repo/config.toml"), "accept");
    assert_eq!(w.read(&w.env()), Some(SameChange::Accept));
}

#[test]
fn the_workspace_config_wins_over_the_repo_config() {
    let w = World::new();
    write(&w.workspace().join(".jj/repo/config.toml"), "keep");
    fs::write(w.workspace().join(".jj/workspace-config-id"), "8a87ff4a66c7c545b5cc").unwrap();
    write(&w.platform().join("workspaces/8a87ff4a66c7c545b5cc/config.toml"), "accept");
    assert_eq!(w.read(&w.env()), Some(SameChange::Accept));
}

#[test]
fn a_secondary_workspace_reads_the_repo_its_pointer_names() {
    let w = World::new();
    write(&w.workspace().join(".jj/repo/config.toml"), "keep");
    let second = w.dir.path().join("second");
    fs::create_dir_all(second.join(".jj")).unwrap();
    fs::write(second.join(".jj/repo"), "../../ws/.jj/repo").unwrap();
    assert_eq!(same_change(&w.env(), Some(&second)), Some(SameChange::Keep));
}

/// A user config whose `[[--scope]]` sets `merge.same-change = "keep"` for the repositories
/// below `dir`.
fn scoped_to(w: &World, dir: &Path) {
    write_text(
        &w.platform().join("config.toml"),
        &format!(
            "[[--scope]]\n--when.repositories = [{:?}]\n[--scope.merge]\nsame-change = \"keep\"\n",
            dir.display().to_string()
        ),
    );
}

#[test]
fn a_scope_for_the_workspace_applies() {
    let w = World::new();
    scoped_to(&w, &w.workspace());
    assert_eq!(w.read(&w.env()), Some(SameChange::Keep));
}

#[test]
fn a_scope_for_another_repository_or_outside_any_workspace_does_not_apply() {
    let w = World::new();
    scoped_to(&w, &w.dir.path().join("elsewhere"));
    assert_eq!(w.read(&w.env()), None);
    scoped_to(&w, &w.workspace());
    assert_eq!(same_change(&w.env(), None), None);
}

#[test]
fn a_bad_value_names_the_file_and_key() {
    let w = World::new();
    let file = w.platform().join("config.toml");
    write(&file, "sometimes");
    let error = merge_settings(&w.env(), None).unwrap_err();
    assert!(
        matches!(&error, ConfigError::Value { key: SAME_CHANGE, path: Some(p), .. } if *p == file),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "{}: merge.same-change in jj's config: Invalid type or value for merge.same-change",
            file.display()
        )
    );
}

#[test]
fn a_file_that_is_not_toml_names_the_file() {
    let w = World::new();
    let file = w.platform().join("config.toml");
    write_text(&file, "[merge\n");
    let error = merge_settings(&w.env(), None).unwrap_err();
    assert!(matches!(&error, ConfigError::Load { path, .. } if *path == file), "{error:?}");
    assert!(
        error.to_string().starts_with(&format!("cannot read jj's config {}: ", file.display())),
        "{error}"
    );
}

#[test]
fn the_hunk_level_is_read_with_the_same_precedence() {
    let w = World::new();
    let read = || merge_settings(&w.env(), Some(&w.workspace())).unwrap();
    assert_eq!(read(), MergeSettings::default());
    write_text(
        &w.platform().join("config.toml"),
        "[merge]\nhunk-level = \"word\"\nsame-change = \"keep\"\n",
    );
    assert_eq!(
        read(),
        MergeSettings {
            same_change: Some(SameChange::Keep),
            hunk_level: Some(FileMergeHunkLevel::Word),
        }
    );
    write_text(&w.workspace().join(".jj/repo/config.toml"), "merge.hunk-level = \"line\"\n");
    assert_eq!(
        read(),
        MergeSettings {
            same_change: Some(SameChange::Keep),
            hunk_level: Some(FileMergeHunkLevel::Line),
        },
        "the repository's value wins; the user's same-change stays"
    );
}

#[test]
fn a_bad_hunk_level_names_its_key() {
    let w = World::new();
    let file = w.platform().join("config.toml");
    write_text(&file, "merge.hunk-level = \"char\"\n");
    let error = merge_settings(&w.env(), None).unwrap_err();
    assert!(
        matches!(&error, ConfigError::Value { key: HUNK_LEVEL, path: Some(p), .. } if *p == file),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .starts_with(&format!("{}: merge.hunk-level in jj's config: ", file.display())),
        "{error}"
    );
}
