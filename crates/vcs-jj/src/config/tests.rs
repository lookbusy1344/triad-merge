use std::collections::HashMap;
use std::path::{Path, PathBuf};

use jj_lib::git::get_git_backend;
use jj_lib::repo_path::RepoPath;
use jj_lib::secure_config::{SecureConfig, SecureConfigError};
use rand::SeedableRng as _;
use rand_chacha::ChaCha20Rng;
use tempfile::TempDir;

use super::{ConfigEnv, ConfigError, JjConfig, excludes_file, load};
use crate::testing::Fixture;

const MEBIBYTE: u64 = 1024 * 1024;
const SEED: u64 = 7;

/// A home with `.config` and `etc` beside it, none of them the real ones.
struct Home {
    dir: TempDir,
    env: ConfigEnv,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let env = ConfigEnv {
            home_dir: Some(dir.path().to_owned()),
            config_dir: Some(dir.path().join(".config")),
            jj_system_config_dir: Some(dir.path().join("etc/jj")),
            hostname: Some("host-a".to_owned()),
            username: Some("user-a".to_owned()),
            environment: HashMap::from([("JJ_RANDOMNESS_SEED".to_owned(), SEED.to_string())]),
        };
        Self { dir, env }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    /// Write `text` at `relative` below the home, making its directories.
    fn write(&self, relative: &str, text: &str) -> PathBuf {
        write(&self.path(relative), text)
    }

    fn set(&mut self, var: &str, value: &str) {
        self.env.environment.insert(var.to_owned(), value.to_owned());
    }

    fn load(&self, fixture: &Fixture) -> JjConfig {
        load(&self.env, fixture.root().as_std_path()).unwrap()
    }

    fn max_size(&self, fixture: &Fixture) -> u64 {
        self.load(fixture).snapshot.max_new_file_size
    }
}

fn write(path: &Path, text: &str) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
    path.to_owned()
}

fn size(bytes: u64) -> String {
    format!("snapshot.max-new-file-size = {bytes}\n")
}

/// The repository's or the workspace's config file in jj's secure config directory, made as
/// `jj config edit --repo` makes it.
fn secure_file(home: &Home, secure: &SecureConfig, dir: &str) -> PathBuf {
    let mut rng = ChaCha20Rng::seed_from_u64(SEED);
    secure.load_config(&mut rng, &home.path(".config/jj").join(dir)).unwrap().config_file.unwrap()
}

fn tracks(config: &JjConfig, path: &str) -> bool {
    config.snapshot.auto_track.matches(RepoPath::from_internal_string(path).unwrap())
}

fn ignores(config: &JjConfig, path: &str) -> bool {
    config.snapshot.base_ignores.matches_file(RepoPath::from_internal_string(path).unwrap())
}

#[test]
fn defaults_track_every_new_file_up_to_one_mebibyte() {
    let fixture = Fixture::new();
    let config = Home::new().load(&fixture);
    assert_eq!(config.snapshot.max_new_file_size, MEBIBYTE);
    assert!(tracks(&config, "a.txt"));
    assert!(tracks(&config, "dir/b.rs"));
}

#[test]
fn platform_config_sets_the_size_limit() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", "snapshot.max-new-file-size = \"2KiB\"\n");
    assert_eq!(home.max_size(&fixture), 2048);
}

#[test]
fn a_zero_size_limit_is_no_limit() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", &size(0));
    assert_eq!(home.max_size(&fixture), u64::MAX);
}

#[test]
fn home_config_loads_below_platform_config() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".jjconfig.toml", &size(3));
    assert_eq!(home.max_size(&fixture), 3);
    home.write(".config/jj/config.toml", &size(4));
    assert_eq!(home.max_size(&fixture), 4);
}

#[test]
fn home_config_is_read_without_a_platform_directory() {
    let fixture = Fixture::new();
    let mut home = Home::new();
    home.env.config_dir = None;
    home.write(".jjconfig.toml", &size(3));
    assert_eq!(home.max_size(&fixture), 3);
}

#[test]
fn drop_in_files_load_above_platform_config_in_name_order() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", &size(4));
    home.write(".config/jj/conf.d/b.toml", &size(6));
    home.write(".config/jj/conf.d/a.toml", &size(5));
    assert_eq!(home.max_size(&fixture), 6);
}

#[test]
fn system_config_loads_below_user_config() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write("etc/jj/config.toml", &size(9));
    assert_eq!(home.max_size(&fixture), 9);
    home.write("etc/jj/conf.d/a.toml", &size(10));
    assert_eq!(home.max_size(&fixture), 10);
    home.write(".config/jj/config.toml", &size(4));
    assert_eq!(home.max_size(&fixture), 4);
}

#[test]
fn jj_config_replaces_system_and_user_files_and_skips_empty_entries() {
    let fixture = Fixture::new();
    let mut home = Home::new();
    home.write("etc/jj/config.toml", "snapshot.auto-track = \"none()\"\n");
    home.write(".config/jj/config.toml", &size(4));
    let first = home.write("first.toml", &size(6));
    let second = home.write("second.toml", &size(7));
    let list = std::env::join_paths([first, PathBuf::new(), second]).unwrap();
    home.set("JJ_CONFIG", list.to_str().unwrap());
    let config = home.load(&fixture);
    assert_eq!(config.snapshot.max_new_file_size, 7);
    assert!(tracks(&config, "a.txt"), "the system file is not read");
}

#[test]
fn repo_config_overrides_user_and_workspace_overrides_repo() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", &size(4));
    let jj = fixture.root().as_std_path().join(".jj");
    let repo = secure_file(&home, &SecureConfig::new_repo(jj.join("repo")), "repos");
    write(&repo, &size(12));
    assert_eq!(home.max_size(&fixture), 12);
    let workspace = secure_file(&home, &SecureConfig::new_workspace(jj), "workspaces");
    write(&workspace, &size(13));
    assert_eq!(home.max_size(&fixture), 13);
}

#[test]
fn a_scope_applies_only_to_its_repository() {
    let fixture = Fixture::new();
    let other = Fixture::new();
    let home = Home::new();
    let root = fixture.root().as_std_path().join(".jj").join("repo");
    let scoped = format!(
        "[[--scope]]\n--when.repositories = [{:?}]\n[--scope.snapshot]\nmax-new-file-size = 11\n",
        root.to_str().unwrap()
    );
    home.write(".config/jj/config.toml", &scoped);
    assert_eq!(home.max_size(&fixture), 11);
    assert_eq!(home.max_size(&other), MEBIBYTE);
}

#[test]
fn a_scope_applies_only_on_its_host() {
    let fixture = Fixture::new();
    let mut home = Home::new();
    home.write(
        ".config/jj/config.toml",
        "[[--scope]]\n--when.hostnames = [\"host-a\"]\n[--scope.snapshot]\nmax-new-file-size = 14\n",
    );
    assert_eq!(home.max_size(&fixture), 14);
    home.env.hostname = Some("host-b".to_owned());
    assert_eq!(home.max_size(&fixture), MEBIBYTE);
}

#[test]
fn auto_track_none_tracks_no_new_file() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", "snapshot.auto-track = \"none()\"\n");
    let config = home.load(&fixture);
    assert!(!tracks(&config, "a.txt"));
}

#[test]
fn auto_track_globs_match_from_the_workspace_root() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", "snapshot.auto-track = \"glob:src/*.rs\"\n");
    let config = home.load(&fixture);
    assert!(tracks(&config, "src/a.rs"));
    assert!(!tracks(&config, "a.rs"));
    assert!(!tracks(&config, "src/a.txt"));
}

#[test]
fn auto_track_takes_fileset_aliases_from_every_layer() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(
        ".config/jj/config.toml",
        "snapshot.auto-track = \"sources\"\n[fileset-aliases]\nsources = \"glob:*.txt\"\n",
    );
    home.write(
        ".config/jj/conf.d/a.toml",
        "[fileset-aliases]\nsources = { definition = \"glob:*.rs\" }\n",
    );
    let config = home.load(&fixture);
    assert!(tracks(&config, "a.rs"));
    assert!(!tracks(&config, "a.txt"));
}

#[test]
fn a_bad_auto_track_is_refused_with_its_text() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", "snapshot.auto-track = \"all(\"\n");
    let error = load(&home.env, fixture.root().as_std_path()).err().unwrap();
    assert!(matches!(&error, ConfigError::AutoTrack { text, .. } if text == "all("), "{error:?}");
}

#[test]
fn a_bad_size_is_refused() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", "snapshot.max-new-file-size = \"lots\"\n");
    let error = load(&home.env, fixture.root().as_std_path()).err().unwrap();
    assert!(matches!(error, ConfigError::Get(_)), "{error:?}");
}

#[test]
fn a_file_that_is_not_toml_is_refused() {
    let fixture = Fixture::new();
    let home = Home::new();
    let file = home.write(".config/jj/config.toml", "snapshot = [\n");
    let error = load(&home.env, fixture.root().as_std_path()).err().unwrap();
    assert!(matches!(&error, ConfigError::Load { path, .. } if *path == file), "{error:?}");
}

#[test]
fn a_config_id_that_is_not_hex_is_refused() {
    let fixture = Fixture::new();
    let home = Home::new();
    let repo = fixture.root().as_std_path().join(".jj/repo");
    write(&repo.join("config-id"), "not-a-config-id");
    let error = load(&home.env, fixture.root().as_std_path()).err().unwrap();
    assert!(matches!(error, ConfigError::Secure(SecureConfigError::BadConfigIdError)), "{error:?}");
}

#[cfg(unix)]
#[test]
fn an_exclude_file_that_cannot_be_read_is_refused_with_its_path() {
    use jj_lib::gitignore::GitIgnoreError;
    use std::os::unix::fs::PermissionsExt as _;
    const NO_ACCESS: u32 = 0o000;

    let fixture = Fixture::new();
    let home = Home::new();
    let exclude = git_dir(&home, &fixture).join("info").join("exclude");
    write(&exclude, "*.tmp\n");
    std::fs::set_permissions(&exclude, std::fs::Permissions::from_mode(NO_ACCESS)).unwrap();
    let error = load(&home.env, fixture.root().as_std_path()).err().unwrap();
    assert!(
        matches!(
            &error,
            ConfigError::Ignores(GitIgnoreError::ReadFile { path, .. }) if *path == exclude
        ),
        "{error:?}"
    );
}

#[test]
fn a_directory_without_jj_is_refused() {
    let home = Home::new();
    let dir = tempfile::tempdir().unwrap();
    let error = load(&home.env, dir.path()).err().unwrap();
    assert!(matches!(error, ConfigError::Workspace(_)), "{error:?}");
}

#[test]
fn operations_record_the_host_and_user_unless_config_or_jj_variables_say_otherwise() {
    let fixture = Fixture::new();
    let mut home = Home::new();
    let settings = home.load(&fixture).settings;
    assert_eq!(settings.operation_hostname(), "host-a");
    assert_eq!(settings.operation_username(), "user-a");
    home.write(
        ".config/jj/config.toml",
        "operation.hostname = \"host-c\"\nuser.name = \"Config Name\"\n",
    );
    let settings = home.load(&fixture).settings;
    assert_eq!(settings.operation_hostname(), "host-c");
    assert_eq!(settings.user_name(), "Config Name");
    home.set("JJ_OP_HOSTNAME", "host-d");
    home.set("JJ_OP_USERNAME", "user-d");
    home.set("JJ_USER", "Env Name");
    home.set("JJ_EMAIL", "env@example.com");
    let settings = home.load(&fixture).settings;
    assert_eq!(settings.operation_hostname(), "host-d");
    assert_eq!(settings.operation_username(), "user-d");
    assert_eq!(settings.user_name(), "Env Name");
    assert_eq!(settings.user_email(), "env@example.com");
}

#[test]
fn a_randomness_seed_reaches_the_settings_only_when_jj_can_read_it() {
    const SEED_KEY: &str = "debug.randomness-seed";
    let fixture = Fixture::new();
    let mut home = Home::new();
    let seed = |home: &Home| home.load(&fixture).settings.get_int(SEED_KEY).ok();
    assert_eq!(seed(&home), Some(SEED.cast_signed()));
    for unusable in ["-3", "not a number", "18446744073709551615"] {
        home.set("JJ_RANDOMNESS_SEED", unusable);
        assert_eq!(seed(&home), None, "{unusable}");
    }
    home.set("JJ_RANDOMNESS_SEED", "9223372036854775807");
    assert_eq!(seed(&home), Some(i64::MAX));
}

#[test]
fn watchman_falls_back_to_a_scan() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", "fsmonitor.backend = \"watchman\"\n");
    let settings = home.load(&fixture).settings;
    assert_eq!(settings.get_string("fsmonitor.backend").unwrap(), "none");
}

#[test]
fn excludes_file_without_a_value_is_git_default_below_xdg_config() {
    let mut home = Home::new();
    let root = Path::new("/work");
    assert_eq!(excludes_file(None, &home.env, root), Some(home.path(".config/git/ignore")));
    home.set("XDG_CONFIG_HOME", "");
    assert_eq!(excludes_file(None, &home.env, root), Some(home.path(".config/git/ignore")));
    home.set("XDG_CONFIG_HOME", "/xdg");
    assert_eq!(excludes_file(None, &home.env, root), Some(PathBuf::from("/xdg/git/ignore")));
}

#[test]
fn excludes_file_without_a_value_or_home_is_none() {
    let mut home = Home::new();
    home.env.home_dir = None;
    assert_eq!(excludes_file(None, &home.env, Path::new("/work")), None);
}

#[test]
fn excludes_file_expands_home_and_joins_relative_paths_to_the_root() {
    let home = Home::new();
    let root = Path::new("/work");
    assert_eq!(excludes_file(Some(b"~/ignore"), &home.env, root), Some(home.path("ignore")));
    assert_eq!(
        excludes_file(Some(b"rel/ignore"), &home.env, root),
        Some(PathBuf::from("/work/rel/ignore"))
    );
    assert_eq!(
        excludes_file(Some(b"/abs/ignore"), &home.env, root),
        Some(PathBuf::from("/abs/ignore"))
    );
    assert_eq!(excludes_file(Some(b"\xff"), &home.env, root), None);
}

/// git's directory for the fixture's store.
fn git_dir(home: &Home, fixture: &Fixture) -> PathBuf {
    let config = home.load(fixture);
    get_git_backend(config.workspace.repo_loader().store()).unwrap().git_repo_path().to_owned()
}

/// `path` as a quoted git config value. Backslashes, as in Windows paths, and quotes are
/// escapes there, so they are escaped.
fn git_config_string(path: &Path) -> String {
    let escaped = path.display().to_string().replace('\\', r"\\").replace('"', r#"\""#);
    format!("\"{escaped}\"")
}

fn ignores_from_git(fixture: &Fixture) {
    let home = Home::new();
    let git = git_dir(&home, fixture);
    let global = home.write("global-ignore", "*.log\n");
    let config = std::fs::read_to_string(git.join("config")).unwrap();
    let excludes = format!("{config}\n[core]\n\texcludesFile = {}\n", git_config_string(&global));
    std::fs::write(git.join("config"), excludes).unwrap();
    write(&git.join("info").join("exclude"), "*.tmp\n");
    let config = home.load(fixture);
    assert!(ignores(&config, "a.log"));
    assert!(ignores(&config, "dir/b.tmp"));
    assert!(!ignores(&config, "c.txt"));
}

#[test]
fn ignores_come_from_core_excludes_file_and_info_exclude_in_an_internal_store() {
    ignores_from_git(&Fixture::internal());
}

#[test]
fn ignores_come_from_a_colocated_git_directory() {
    let fixture = Fixture::colocated();
    let home = Home::new();
    assert_eq!(
        git_dir(&home, &fixture),
        dunce::canonicalize(fixture.root().as_std_path().join(".git")).unwrap()
    );
    ignores_from_git(&fixture);
}

#[test]
fn options_carry_the_size_limit_and_matchers() {
    let fixture = Fixture::new();
    let home = Home::new();
    home.write(".config/jj/config.toml", "snapshot.auto-track = \"none()\"\n");
    let config = home.load(&fixture);
    let options = config.snapshot.options();
    assert_eq!(options.max_new_file_size, MEBIBYTE);
    let path = RepoPath::from_internal_string("a.txt").unwrap();
    assert!(!options.start_tracking_matcher.matches(path));
    assert!(!options.force_tracking_matcher.matches(path));
}
