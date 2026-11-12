//! The user's jj configuration, layered as jj-cli 0.46 layers it, and the snapshot options it
//! gives. A snapshot triad takes (`DESIGN.md` §D5.5) tracks and ignores the files
//! the user's own jj would.
//!
//! The layers, lowest first: jj-lib's defaults, jj-cli's snapshot defaults, the host and user
//! names, the system files (`/etc/jj/config.toml`, `/etc/jj/conf.d`), the user files
//! (`~/.jjconfig.toml`, `<config>/jj/config.toml`, `<config>/jj/conf.d`, or `JJ_CONFIG`'s list in
//! place of the system and user files), the repository's and the workspace's files in jj's
//! secure config directory, then the `JJ_*` overrides. `--when` scopes resolve against the
//! repository, the workspace, the host and the environment, with no command.
//!
//! [`merge_settings`] reads `merge.same-change` and `merge.hunk-level` from the same layers
//! for a merge with jj's semantics (`DESIGN.md` §B3). It only reads: the repository's and the
//! workspace's files are found by their config IDs, or else at their legacy paths, and nothing
//! is migrated or written.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use etcetera::BaseStrategy as _;
use jj_lib::config::{
    ConfigGetError, ConfigLayer, ConfigLoadError, ConfigResolutionContext, ConfigSource,
    ConfigUpdateError, StackedConfig,
};
use jj_lib::default_backend_factories::{
    default_backend_factories, default_working_copy_factories,
};
use jj_lib::file_util::expand_home_path;
pub use jj_lib::files::FileMergeHunkLevel;
use jj_lib::fileset::{self, FilesetAliasesMap, FilesetDiagnostics, FilesetParseContext};
use jj_lib::git::get_git_backend;
use jj_lib::gitignore::{GitIgnoreError, GitIgnoreFile};
use jj_lib::matchers::{Matcher, NothingMatcher};
pub use jj_lib::merge::SameChange;
use jj_lib::repo_path::RepoPath;
use jj_lib::secure_config::{SecureConfig, SecureConfigError};
use jj_lib::settings::{HumanByteSize, UserSettings};
use jj_lib::store::Store;
use jj_lib::ui_path::RepoPathUiConverter;
use jj_lib::working_copy::SnapshotOptions;
use jj_lib::workspace::{
    DefaultWorkspaceLoaderFactory, Workspace, WorkspaceLoadError, WorkspaceLoaderFactory as _,
};
use rand::SeedableRng as _;
use rand_chacha::ChaCha20Rng;

use crate::JJ_DIR;

/// jj-cli 0.46's defaults for the snapshot keys, which jj-lib's defaults leave unset
/// (`cli/src/config/misc.toml`).
const CLI_DEFAULTS: &str = r#"
[snapshot]
max-new-file-size = "1MiB"
auto-track = "all()"
"#;

const AUTO_TRACK: &str = "snapshot.auto-track";
const MAX_NEW_FILE_SIZE: &str = "snapshot.max-new-file-size";
const FSMONITOR_BACKEND: &str = "fsmonitor.backend";
/// The only file-system monitor triad's jj-lib has: it is built without watchman, and a scan
/// gives the snapshot a monitor would.
const NO_FSMONITOR: &str = "none";
const FILESET_ALIASES: &str = "fileset-aliases";
const OPERATION_HOSTNAME: &str = "operation.hostname";
const OPERATION_USERNAME: &str = "operation.username";

/// jj's directory below the platform's config directory and below `/etc`.
const JJ_CONFIG_DIR: &str = "jj";
const CONFIG_FILE: &str = "config.toml";
const CONFIG_DROP_IN_DIR: &str = "conf.d";
const HOME_CONFIG_FILE: &str = ".jjconfig.toml";
const REPO_CONFIG_DIR: &str = "repos";
const WORKSPACE_CONFIG_DIR: &str = "workspaces";
const UNIX_SYSTEM_CONFIG_DIR: &str = "/etc";
const REPO_DIR: &str = "repo";
const REPO_CONFIG_ID: &str = "config-id";
const WORKSPACE_CONFIG_ID: &str = "workspace-config-id";
const LEGACY_WORKSPACE_CONFIG: &str = "workspace-config.toml";
/// jj-lib's config ID: 10 bytes in hex (`secure_config::CONFIG_ID_BYTES`, private).
const CONFIG_ID_LENGTH: usize = 20;
pub const SAME_CHANGE: &str = "merge.same-change";
pub const HUNK_LEVEL: &str = "merge.hunk-level";

/// A path list that replaces the system and user config files.
const JJ_CONFIG: &str = "JJ_CONFIG";
const JJ_RANDOMNESS_SEED: &str = "JJ_RANDOMNESS_SEED";
const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";
const USER: &str = "USER";
/// The environment variables jj-cli 0.46 lets override every config file, and their keys.
/// `JJ_EDITOR` and `JJ_PAGER` are left out: they set nothing a repository write reads.
const ENV_OVERRIDES: [(&str, &str); 6] = [
    ("JJ_USER", "user.name"),
    ("JJ_EMAIL", "user.email"),
    ("JJ_TIMESTAMP", "debug.commit-timestamp"),
    ("JJ_OP_TIMESTAMP", "debug.operation-timestamp"),
    ("JJ_OP_HOSTNAME", OPERATION_HOSTNAME),
    ("JJ_OP_USERNAME", OPERATION_USERNAME),
];
const RANDOMNESS_SEED: &str = "debug.randomness-seed";

/// git's config key naming a global ignore file.
const EXCLUDES_FILE: &str = "core.excludesFile";
/// git's default global ignore file, below the XDG config directory.
const GIT_DIR: &str = "git";
const GIT_IGNORE_FILE: &str = "ignore";
/// The repository's own ignore file, below the git directory.
const GIT_INFO_EXCLUDE: [&str; 2] = ["info", "exclude"];

/// Why the user's jj configuration cannot be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read jj's config {}: {source}", path.display())]
    Load {
        path: PathBuf,
        #[source]
        source: ConfigLoadError,
    },
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{}: not a jj config id", path.display())]
    BadId { path: PathBuf },
    #[error("{}{key} in jj's config: {source}", place(.path.as_deref()))]
    Value {
        key: &'static str,
        path: Option<PathBuf>,
        #[source]
        source: ConfigGetError,
    },
    #[error("reading a jj config value")]
    Get(#[from] ConfigGetError),
    #[error("setting a jj config value")]
    Update(#[from] ConfigUpdateError),
    #[error("finding the repository's or workspace's jj config")]
    Secure(#[from] SecureConfigError),
    #[error("`snapshot.auto-track = {text:?}` is not a fileset")]
    AutoTrack {
        text: String,
        #[source]
        source: Box<fileset::FilesetParseError>,
    },
    #[error("reading git's ignore files")]
    Ignores(#[from] GitIgnoreError),
    #[error("loading the jj workspace")]
    Workspace(#[from] WorkspaceLoadError),
}

/// What the user's jj reads its configuration from: directories and the environment. Tests
/// build one from temporary directories; the product takes [`ConfigEnv::from_process`].
#[derive(Debug, Clone, Default)]
pub struct ConfigEnv {
    pub home_dir: Option<PathBuf>,
    /// The platform's user config directory; jj's files sit in its `jj` directory.
    pub config_dir: Option<PathBuf>,
    /// jj's system config directory: `/etc/jj` on Unix. Unused when `JJ_CONFIG` is set.
    pub jj_system_config_dir: Option<PathBuf>,
    pub hostname: Option<String>,
    pub username: Option<String>,
    pub environment: HashMap<String, String>,
}

impl ConfigEnv {
    /// The process's directories, host, user and environment, found as jj-cli 0.46 finds them.
    #[must_use]
    pub fn from_process() -> Self {
        let home_dir = etcetera::home_dir()
            .inspect_err(|error| tracing::warn!(%error, "no home directory: jj's ~ stays as is"))
            .ok()
            .map(|dir| {
                // jj canonicalises the home as it does the working directory; a home that
                // cannot be canonicalised is used as given, as jj uses it.
                dunce::canonicalize(&dir).unwrap_or(dir)
            });
        let config_dir = etcetera::choose_base_strategy()
            .inspect_err(|error| tracing::warn!(%error, "no user config directory for jj"))
            .ok()
            .map(|strategy| strategy.config_dir());
        let environment: HashMap<String, String> = std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect();
        let username = whoami::username()
            .inspect_err(|error| tracing::warn!(%error, "no user name: trying $USER"))
            .ok()
            .or_else(|| environment.get(USER).cloned());
        Self {
            home_dir,
            config_dir,
            jj_system_config_dir: cfg!(unix)
                .then(|| Path::new(UNIX_SYSTEM_CONFIG_DIR).join(JJ_CONFIG_DIR)),
            hostname: whoami::hostname()
                .inspect_err(|error| tracing::warn!(%error, "no host name for jj's operations"))
                .ok(),
            username,
            environment,
        }
    }

    fn var(&self, name: &str) -> Option<&str> {
        self.environment.get(name).map(String::as_str)
    }

    fn jj_config_dir(&self) -> Option<PathBuf> {
        self.config_dir.as_ref().map(|dir| dir.join(JJ_CONFIG_DIR))
    }

    /// The system files and directories that exist, in load order.
    fn system_paths(&self) -> Vec<PathBuf> {
        match (&self.jj_system_config_dir, self.var(JJ_CONFIG)) {
            (Some(jj), None) => existing([jj.join(CONFIG_FILE), jj.join(CONFIG_DROP_IN_DIR)]),
            _ => Vec::new(),
        }
    }

    /// The user files and directories that exist, in load order: `JJ_CONFIG`'s list, else
    /// `~/.jjconfig.toml`, `<config>/jj/config.toml` and `<config>/jj/conf.d`.
    fn user_paths(&self) -> Vec<PathBuf> {
        if let Some(list) = self.var(JJ_CONFIG) {
            return existing(std::env::split_paths(OsStr::new(list)));
        }
        let home = self.home_dir.as_ref().map(|home| home.join(HOME_CONFIG_FILE));
        let platform = self
            .jj_config_dir()
            .into_iter()
            .flat_map(|dir| [dir.join(CONFIG_FILE), dir.join(CONFIG_DROP_IN_DIR)]);
        existing(home.into_iter().chain(platform))
    }

    /// The host and user names jj's operations record, below every config file.
    fn base_layer(&self) -> Result<ConfigLayer, ConfigUpdateError> {
        let mut layer = ConfigLayer::empty(ConfigSource::EnvBase);
        if let Some(hostname) = &self.hostname {
            layer.set_value(OPERATION_HOSTNAME, hostname.as_str())?;
        }
        if let Some(username) = &self.username {
            layer.set_value(OPERATION_USERNAME, username.as_str())?;
        }
        Ok(layer)
    }

    /// The `JJ_*` variables, above every config file.
    fn overrides_layer(&self) -> Result<ConfigLayer, ConfigUpdateError> {
        let mut layer = ConfigLayer::empty(ConfigSource::EnvOverrides);
        for (var, key) in ENV_OVERRIDES {
            if let Some(value) = self.var(var) {
                layer.set_value(key, value)?;
            }
        }
        if let Some(seed) = self.seed() {
            layer.set_value(RANDOMNESS_SEED, seed)?;
        }
        Ok(layer)
    }

    /// `JJ_RANDOMNESS_SEED` as a config value. jj-lib reads the key as a `u64`, and a TOML
    /// integer stops at `i64::MAX`; a value outside both is no seed, as in [`Self::rng`].
    fn seed(&self) -> Option<i64> {
        let seed: u64 = self.var(JJ_RANDOMNESS_SEED)?.parse().ok()?;
        seed.try_into().ok()
    }

    /// The generator for a new secure config ID: seeded by `JJ_RANDOMNESS_SEED` when it is a
    /// number, as jj seeds it.
    fn rng(&self) -> ChaCha20Rng {
        self.var(JJ_RANDOMNESS_SEED)
            .and_then(|seed| seed.parse::<u64>().ok())
            .map_or_else(rand::make_rng, ChaCha20Rng::seed_from_u64)
    }

    /// `$XDG_CONFIG_HOME` when set and not empty, else `~/.config`, as jj-cli finds git's
    /// default global ignore file.
    fn xdg_config_home(&self) -> Option<PathBuf> {
        match self.var(XDG_CONFIG_HOME) {
            Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
            _ => self.home_dir.as_ref().map(|home| home.join(".config")),
        }
    }
}

fn existing(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|path| path.exists()).collect()
}

fn load_path(
    config: &mut StackedConfig,
    source: ConfigSource,
    path: &Path,
) -> Result<(), ConfigError> {
    let loaded =
        if path.is_dir() { config.load_dir(source, path) } else { config.load_file(source, path) };
    loaded.map_err(|error| ConfigError::Load { path: path.to_owned(), source: error })
}

fn place(path: Option<&Path>) -> String {
    path.map_or_else(String::new, |path| format!("{}: ", path.display()))
}

/// `config` with the system and user files added: every file layer below the repository's.
fn user_files(env: &ConfigEnv, mut config: StackedConfig) -> Result<StackedConfig, ConfigError> {
    for path in env.system_paths() {
        load_path(&mut config, ConfigSource::System, &path)?;
    }
    for path in env.user_paths() {
        load_path(&mut config, ConfigSource::User, &path)?;
    }
    Ok(config)
}

/// `config` with its `--when` scopes resolved, for the workspace at `workspace_root` whose
/// repository is at `repo_path`, or outside any workspace.
fn resolve(
    env: &ConfigEnv,
    config: &StackedConfig,
    workspace: Option<(&Path, &Path)>,
) -> Result<StackedConfig, ConfigError> {
    let context = ConfigResolutionContext {
        home_dir: env.home_dir.as_deref(),
        repo_path: workspace.map(|(_, repo)| repo),
        workspace_path: workspace.map(|(root, _)| root),
        command: None,
        hostname: env.hostname.as_deref().unwrap_or_default(),
        environment: &env.environment,
    };
    Ok(jj_lib::config::resolve(config, &context)?)
}

/// The settings jj would use in the workspace at `workspace_root`, whose repository is at
/// `repo_path`.
fn settings(
    env: &ConfigEnv,
    workspace_root: &Path,
    repo_path: &Path,
) -> Result<UserSettings, ConfigError> {
    #[expect(
        clippy::expect_used,
        reason = "invariant: `CLI_DEFAULTS` is a constant, which every settings test parses"
    )]
    let cli_defaults = ConfigLayer::parse(ConfigSource::Default, CLI_DEFAULTS)
        .expect("jj-cli's snapshot defaults are TOML");
    let mut defaults = StackedConfig::with_defaults();
    defaults.add_layer(cli_defaults);
    defaults.add_layer(env.base_layer()?);
    let mut config = user_files(env, defaults)?;
    if let Some(root) = env.jj_config_dir() {
        let mut rng = env.rng();
        let secure = [
            (SecureConfig::new_repo(repo_path.to_owned()), ConfigSource::Repo, REPO_CONFIG_DIR),
            (
                SecureConfig::new_workspace(workspace_root.join(JJ_DIR)),
                ConfigSource::Workspace,
                WORKSPACE_CONFIG_DIR,
            ),
        ];
        for (secure, source, dir) in secure {
            let loaded = secure.maybe_load_config(&mut rng, &root.join(dir))?;
            for warning in &loaded.warnings {
                tracing::warn!(%warning, "jj's {source} config");
            }
            if let Some(file) = loaded.config_file.filter(|file| file.exists()) {
                load_path(&mut config, source, &file)?;
            }
        }
    }
    config.add_layer(env.overrides_layer()?);
    let mut fsmonitor = ConfigLayer::empty(ConfigSource::CommandArg);
    fsmonitor.set_value(FSMONITOR_BACKEND, NO_FSMONITOR)?;
    config.add_layer(fsmonitor);
    let resolved = resolve(env, &config, Some((workspace_root, repo_path)))?;
    Ok(UserSettings::from_config_and_home_dir(resolved, env.home_dir.clone())?)
}

/// jj's merge settings, each none when no layer sets it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeSettings {
    pub same_change: Option<SameChange>,
    pub hunk_level: Option<FileMergeHunkLevel>,
}

/// `merge.same-change` and `merge.hunk-level` as jj would read them in the workspace at
/// `workspace`, or from the user's and system's files outside any workspace. Only reads.
///
/// # Errors
///
/// [`ConfigError::Load`] for a file that cannot be read or is not TOML,
/// [`ConfigError::BadId`] for a config ID other than 20 hex digits, [`ConfigError::Read`] for
/// an ID file or repository pointer that cannot be read, and [`ConfigError::Value`] for a value
/// jj does not accept; each names the file.
pub fn merge_settings(
    env: &ConfigEnv,
    workspace: Option<&Path>,
) -> Result<MergeSettings, ConfigError> {
    // No defaults: a key is set only when a file sets it.
    let mut config = user_files(env, StackedConfig::empty())?;
    let paths =
        workspace.map(|root| repo_dir(&root.join(JJ_DIR)).map(|repo| (root, repo))).transpose()?;
    if let Some((root, repo)) = &paths {
        for (source, path) in env.secure_paths(root, repo)? {
            if path.exists() {
                load_path(&mut config, source, &path)?;
            }
        }
    }
    config.add_layer(env.overrides_layer()?);
    let resolved =
        resolve(env, &config, paths.as_ref().map(|(root, repo)| (*root, repo.as_path())))?;
    Ok(MergeSettings {
        same_change: value(&resolved, SAME_CHANGE)?,
        hunk_level: value(&resolved, HUNK_LEVEL)?,
    })
}

impl ConfigEnv {
    /// The repository's and the workspace's files, read-only: in the user config directory
    /// under the config ID, or without an ID at the legacy path inside `.jj`.
    fn secure_paths(
        &self,
        workspace_root: &Path,
        repo: &Path,
    ) -> Result<Vec<(ConfigSource, PathBuf)>, ConfigError> {
        let jj_dir = workspace_root.join(JJ_DIR);
        let secure = [
            (
                ConfigSource::Repo,
                repo.join(REPO_CONFIG_ID),
                REPO_CONFIG_DIR,
                repo.join(CONFIG_FILE),
            ),
            (
                ConfigSource::Workspace,
                jj_dir.join(WORKSPACE_CONFIG_ID),
                WORKSPACE_CONFIG_DIR,
                jj_dir.join(LEGACY_WORKSPACE_CONFIG),
            ),
        ];
        let mut paths = Vec::with_capacity(secure.len());
        for (source, id_file, dir, legacy) in secure {
            let path = match read_id(&id_file)? {
                Some(id) => {
                    self.jj_config_dir().map(|config| config.join(dir).join(id).join(CONFIG_FILE))
                }
                None => Some(legacy),
            };
            paths.extend(path.map(|path| (source, path)));
        }
        Ok(paths)
    }
}

/// The repository directory: `.jj/repo`, or in a secondary workspace the directory its
/// `.jj/repo` file names, relative to `.jj`.
fn repo_dir(jj_dir: &Path) -> Result<PathBuf, ConfigError> {
    let repo = jj_dir.join(REPO_DIR);
    if repo.is_file() {
        let target = std::fs::read_to_string(&repo)
            .map_err(|source| ConfigError::Read { path: repo.clone(), source })?;
        Ok(jj_dir.join(target.trim_end()))
    } else {
        Ok(repo)
    }
}

/// The config ID in `path`, or none when the file does not exist. As in jj-lib, an ID is
/// exactly [`CONFIG_ID_LENGTH`] hex digits: it names a directory, so it cannot be a path.
fn read_id(path: &Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(id) if id.len() == CONFIG_ID_LENGTH && id.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(Some(id))
        }
        Ok(_) => Err(ConfigError::BadId { path: path.to_owned() }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::Read { path: path.to_owned(), source }),
    }
}

/// The value at `key`, or none when no layer sets it.
fn value<T: serde::de::DeserializeOwned>(
    config: &StackedConfig,
    key: &'static str,
) -> Result<Option<T>, ConfigError> {
    match config.get::<T>(key) {
        Ok(value) => Ok(Some(value)),
        Err(ConfigGetError::NotFound { .. }) => Ok(None),
        Err(source) => {
            let path = match &source {
                ConfigGetError::Type { source_path, .. } => source_path.clone(),
                ConfigGetError::NotFound { .. } => None,
            };
            Err(ConfigError::Value { key, path, source })
        }
    }
}

/// The `[fileset-aliases]` of every layer, later layers replacing earlier ones. A bad alias is
/// logged and skipped, as jj warns and goes on: a fileset that uses it fails to parse.
fn fileset_aliases(config: &StackedConfig) -> FilesetAliasesMap {
    let mut aliases = FilesetAliasesMap::new();
    for layer in config.layers() {
        let table = match layer.look_up_table(FILESET_ALIASES) {
            Ok(Some(table)) => table,
            Ok(None) => continue,
            Err(item) => {
                tracing::warn!(kind = item.type_name(), "`{FILESET_ALIASES}` is not a table");
                continue;
            }
        };
        for (decl, item) in table.iter() {
            let definition = item.as_table_like().map_or_else(
                || item.as_str(),
                |table| table.get("definition").and_then(|def| def.as_str()),
            );
            let inserted = definition
                .ok_or("not a string or a table with a `definition` string")
                .map_err(ToString::to_string)
                .and_then(|def| aliases.insert(decl, def, None).map_err(|error| error.to_string()));
            if let Err(error) = inserted {
                tracing::warn!(%error, "skipping `{FILESET_ALIASES}.{decl}`");
            }
        }
    }
    aliases
}

/// The snapshot options the user's jj would use: which new files to track, the size above
/// which a new file stays untracked, and git's ignore files.
pub struct SnapshotConfig {
    auto_track: Box<dyn Matcher>,
    max_new_file_size: u64,
    base_ignores: Arc<GitIgnoreFile>,
}

impl SnapshotConfig {
    /// # Errors
    ///
    /// [`ConfigError::AutoTrack`] when `snapshot.auto-track` is not a fileset,
    /// [`ConfigError::Get`] when `snapshot.max-new-file-size` is not a size, and
    /// [`ConfigError::Ignores`] when an ignore file cannot be read or parsed.
    pub fn new(
        settings: &UserSettings,
        env: &ConfigEnv,
        store: &Store,
        workspace_root: &Path,
    ) -> Result<Self, ConfigError> {
        let aliases = fileset_aliases(settings.config());
        // Config filesets name paths from the workspace root.
        let path_converter = RepoPathUiConverter::Fs { cwd: PathBuf::new(), base: PathBuf::new() };
        let context =
            FilesetParseContext { aliases_map: &aliases, path_converter: &path_converter };
        let text = settings.get_string(AUTO_TRACK)?;
        let mut diagnostics = FilesetDiagnostics::new();
        let auto_track = fileset::parse(&mut diagnostics, &text, &context)
            .map_err(|source| ConfigError::AutoTrack {
                text: text.clone(),
                source: Box::new(source),
            })?
            .to_matcher();
        for diagnostic in &diagnostics {
            tracing::warn!(%diagnostic, "in `{AUTO_TRACK}`");
        }
        let HumanByteSize(size) = settings.get_value_with(MAX_NEW_FILE_SIZE, TryInto::try_into)?;
        Ok(Self {
            auto_track,
            // jj reads 0 as no limit.
            max_new_file_size: if size == 0 { u64::MAX } else { size },
            base_ignores: base_ignores(env, store, workspace_root)?,
        })
    }

    /// The options for `LockedWorkingCopy::snapshot`.
    #[must_use]
    pub fn options(&self) -> SnapshotOptions<'_> {
        SnapshotOptions {
            base_ignores: Arc::clone(&self.base_ignores),
            progress: None,
            start_tracking_matcher: self.auto_track.as_ref(),
            force_tracking_matcher: &NothingMatcher,
            max_new_file_size: self.max_new_file_size,
        }
    }
}

/// git's global ignore file: `core.excludesFile` from the workspace root with `~/` expanded,
/// else `<XDG config>/git/ignore`. A value that is not UTF-8 names none, as in jj-cli.
fn excludes_file(value: Option<&[u8]>, env: &ConfigEnv, workspace_root: &Path) -> Option<PathBuf> {
    match value {
        Some(bytes) => {
            let value = std::str::from_utf8(bytes).ok()?;
            Some(workspace_root.join(expand_home_path(value, env.home_dir.as_deref())))
        }
        None => env.xdg_config_home().map(|dir| dir.join(GIT_DIR).join(GIT_IGNORE_FILE)),
    }
}

/// The ignore files below the workspace's own `.gitignore`s: git's global file, then the
/// repository's `info/exclude`.
fn base_ignores(
    env: &ConfigEnv,
    store: &Store,
    workspace_root: &Path,
) -> Result<Arc<GitIgnoreFile>, ConfigError> {
    let ignores = GitIgnoreFile::empty();
    match get_git_backend(store) {
        Ok(backend) => {
            let repo = backend.git_repo();
            let config = repo.config_snapshot();
            let value = config.string(EXCLUDES_FILE);
            let ignores =
                match excludes_file(value.as_deref().map(AsRef::as_ref), env, workspace_root) {
                    Some(file) => ignores.chain_with_file(RepoPath::root(), file)?,
                    None => ignores,
                };
            let exclude = GIT_INFO_EXCLUDE
                .iter()
                .fold(backend.git_repo_path().to_owned(), |path, part| path.join(part));
            Ok(ignores.chain_with_file(RepoPath::root(), exclude)?)
        }
        Err(error) => {
            // A store without git has no `info/exclude`; jj reads git's global config for the
            // global file.
            tracing::debug!(%error, "the store is not git's");
            let global = gix::config::File::from_globals()
                .inspect_err(|error| tracing::warn!(%error, "git's global config is unreadable"))
                .ok();
            let value = global.as_ref().and_then(|config| config.string(EXCLUDES_FILE));
            match excludes_file(value.as_deref().map(AsRef::as_ref), env, workspace_root) {
                Some(file) => Ok(ignores.chain_with_file(RepoPath::root(), file)?),
                None => Ok(ignores),
            }
        }
    }
}

/// A jj workspace loaded with the user's settings, and its snapshot options.
pub struct JjConfig {
    pub workspace: Workspace,
    pub settings: UserSettings,
    pub snapshot: SnapshotConfig,
}

/// The workspace at `root`, loaded with the settings the user's jj would use there.
///
/// Finding the repository's and the workspace's config files follows jj-lib's secure config,
/// which writes to the user's config directory in the cases jj writes there: a config ID with
/// no metadata, and a config file from jj older than 0.37.
///
/// # Errors
///
/// [`ConfigError::Workspace`] when `root` is not a jj workspace, and the others when a config
/// file or value is bad, as [`SnapshotConfig::new`] lists.
pub fn load(env: &ConfigEnv, root: &Path) -> Result<JjConfig, ConfigError> {
    let loader = DefaultWorkspaceLoaderFactory.create(root)?;
    let settings = settings(env, loader.workspace_root(), loader.repo_path())?;
    let workspace =
        loader.load(&settings, &default_backend_factories(), &default_working_copy_factories())?;
    let snapshot = SnapshotConfig::new(
        &settings,
        env,
        workspace.repo_loader().store(),
        workspace.workspace_root(),
    )?;
    Ok(JjConfig { workspace, settings, snapshot })
}

#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod tests;
