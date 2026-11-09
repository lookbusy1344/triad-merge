//! Logging (spec §10): `tracing` to stderr and to a daily file in the platform cache
//! directory, both filtered by `TRIAD_LOG`.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use directories::ProjectDirs;
use tracing::Subscriber;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::prelude::*;

/// The filter, in `tracing-subscriber`'s `EnvFilter` syntax, such as `debug` or
/// `merge_cli=trace`.
pub const FILTER_VAR: &str = "TRIAD_LOG";
/// The directory for log files, in place of the platform cache directory.
pub const DIR_VAR: &str = "TRIAD_LOG_DIR";
const DEFAULT_FILTER: &str = "warn";
/// Daily files kept; older ones are deleted.
const KEPT_LOG_FILES: usize = 7;
const FILE_PREFIX: &str = "triad";
const FILE_SUFFIX: &str = "log";
pub(crate) const APPLICATION: &str = "triad";

/// Where logs go and what passes the filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogSettings {
    /// The `TRIAD_LOG` value. `None` or empty uses the default, `warn`.
    pub filter: Option<String>,
    /// The log file directory, created if missing. `None` writes no file.
    pub dir: Option<PathBuf>,
}

impl LogSettings {
    /// From `TRIAD_LOG` and `TRIAD_LOG_DIR`.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_vars(
            std::env::var_os(FILTER_VAR),
            std::env::var_os(DIR_VAR),
            ProjectDirs::from("", "", APPLICATION).map(|d| d.cache_dir().join("logs")),
        )
    }

    /// From the two variables' values. Without `TRIAD_LOG_DIR`, logs go to `cache_dir`.
    #[must_use]
    pub fn from_vars(
        filter: Option<OsString>,
        dir: Option<OsString>,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            filter: filter.map(|f| f.to_string_lossy().into_owned()),
            dir: dir.map(PathBuf::from).or(cache_dir),
        }
    }
}

/// A setting that could not be used. Logging carries on without it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LogProblem {
    #[error("ignoring {FILTER_VAR}={spec:?}: {reason}")]
    BadFilter { spec: String, reason: String },
    #[error("no log file in {}: {reason}", dir.display())]
    NoFile { dir: PathBuf, reason: String },
}

/// A subscriber that writes to `stderr` and, when `settings.dir` can hold logs, to a daily
/// file there. Also the settings it could not use.
pub fn subscriber<W>(
    settings: &LogSettings,
    stderr: W,
) -> (impl Subscriber + Send + Sync + use<W>, Vec<LogProblem>)
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let (filter, bad_filter) = filter(settings.filter.as_deref());
    let (file, no_file) = match &settings.dir {
        Some(dir) => match appender(dir.clone()) {
            Ok(appender) => (Some(appender), None),
            Err(reason) => (None, Some(LogProblem::NoFile { dir: dir.clone(), reason })),
        },
        None => (None, None),
    };
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(stderr).with_target(false).without_time())
        .with(file.map(|f| fmt::layer().with_writer(f)));
    (subscriber, bad_filter.into_iter().chain(no_file).collect())
}

/// Install the subscriber for the process from the environment, then report what could not
/// be used as warnings. Does nothing if a subscriber is already installed.
pub fn init() {
    let (subscriber, problems) = subscriber(&LogSettings::from_env(), io::stderr);
    if tracing::subscriber::set_global_default(subscriber).is_ok() {
        for problem in problems {
            tracing::warn!("{problem}");
        }
    }
}

fn filter(spec: Option<&str>) -> (EnvFilter, Option<LogProblem>) {
    let default = || EnvFilter::new(DEFAULT_FILTER);
    match spec.filter(|s| !s.is_empty()) {
        None => (default(), None),
        Some(spec) => match EnvFilter::try_new(spec) {
            Ok(filter) => (filter, None),
            Err(e) => (
                default(),
                Some(LogProblem::BadFilter { spec: spec.to_owned(), reason: e.to_string() }),
            ),
        },
    }
}

/// The daily file appender. It is given an existing directory: `tracing-appender` would make
/// a missing one itself, but first prints an error to stderr while looking for old files.
fn appender(dir: PathBuf) -> Result<RollingFileAppender, String> {
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(KEPT_LOG_FILES)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix(FILE_SUFFIX)
        .build(dir)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Captures what the stderr layer writes.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("lock").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().expect("lock").clone()).expect("utf-8")
        }
    }

    fn settings(filter: Option<&str>, dir: Option<&Path>) -> LogSettings {
        LogSettings { filter: filter.map(str::to_owned), dir: dir.map(Path::to_owned) }
    }

    /// Log one event at each level through a subscriber, and give what stderr got.
    fn log_all(settings: &LogSettings) -> (String, Vec<LogProblem>) {
        let stderr = Captured::default();
        let writer = stderr.clone();
        let (subscriber, problems) = subscriber(settings, move || writer.clone());
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!("at debug");
            tracing::info!("at info");
            tracing::warn!("at warn");
            tracing::error!("at error");
        });
        (stderr.text(), problems)
    }

    fn log_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("log dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_default_filter_passes_warnings_and_errors() {
        let (stderr, problems) = log_all(&settings(None, None));
        assert_eq!(stderr, " WARN at warn\nERROR at error\n");
        assert_eq!(problems, []);
        let (stderr, _) = log_all(&settings(Some(""), None));
        assert_eq!(stderr, " WARN at warn\nERROR at error\n");
    }

    #[test]
    fn triad_log_sets_the_filter() {
        let (stderr, _) = log_all(&settings(Some("debug"), None));
        assert_eq!(stderr, "DEBUG at debug\n INFO at info\n WARN at warn\nERROR at error\n");
        let (stderr, _) = log_all(&settings(Some("error"), None));
        assert_eq!(stderr, "ERROR at error\n");
    }

    #[test]
    fn a_bad_filter_falls_back_to_the_default_and_says_why() {
        let (stderr, problems) = log_all(&settings(Some("=[bad"), None));
        assert_eq!(stderr, " WARN at warn\nERROR at error\n");
        match problems.as_slice() {
            [LogProblem::BadFilter { spec, .. }] => assert_eq!(spec, "=[bad"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn logs_also_go_to_a_daily_file_without_colour_codes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("logs");
        let (_, problems) = log_all(&settings(Some("info"), Some(&dir)));
        assert_eq!(problems, []);
        let names = log_files(&dir);
        let [name] = names.as_slice() else {
            panic!("one log file, got {names:?}");
        };
        assert!(
            name.starts_with("triad.")
                && Path::new(name).extension() == Some(std::ffi::OsStr::new("log")),
            "{name}"
        );
        let text = fs::read_to_string(dir.join(name)).expect("log file");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        for (line, message) in lines.iter().zip(["at info", "at warn", "at error"]) {
            assert!(line.ends_with(message), "{line}");
        }
        assert!(!text.contains('\u{1b}'), "{text:?}");
    }

    #[test]
    fn an_unusable_directory_drops_only_the_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let file = tmp.path().join("file");
        fs::write(&file, b"").expect("file");
        let dir = file.join("logs");
        let (stderr, problems) = log_all(&settings(None, Some(&dir)));
        assert_eq!(stderr, " WARN at warn\nERROR at error\n");
        match problems.as_slice() {
            [LogProblem::NoFile { dir: reported, .. }] => assert_eq!(reported, &dir),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn settings_come_from_the_variables_then_the_cache_directory() {
        let cache = Some(PathBuf::from("/cache/logs"));
        assert_eq!(
            LogSettings::from_vars(Some("debug".into()), Some("/tmp/l".into()), cache.clone()),
            settings(Some("debug"), Some(Path::new("/tmp/l")))
        );
        assert_eq!(
            LogSettings::from_vars(None, None, cache),
            settings(None, Some(Path::new("/cache/logs")))
        );
        assert_eq!(LogSettings::from_vars(None, None, None), settings(None, None));
    }
}
