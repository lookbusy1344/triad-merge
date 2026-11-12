//! The config file's format (`DESIGN.md` §E3.1–3.2): `key = value` lines, `#` comments,
//! and `config-file` includes read in place. Keys are not checked here; `settings` knows them.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The key that reads another file in place.
pub const INCLUDE_KEY: &str = "config-file";
/// The prefix of an include path that may be missing.
const OPTIONAL: char = '?';
const COMMENT: char = '#';
const QUOTE: char = '"';
const BOM: char = '\u{feff}';

/// One `key = value` line, after includes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    /// The value, trimmed and without its quotes. Empty means the key's default.
    pub value: String,
    pub origin: Arc<Path>,
    /// 1-based.
    pub line: usize,
}

/// A config file and the files it includes.
#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    /// Every entry in reading order, so a later one wins.
    pub entries: Vec<Entry>,
    pub errors: Vec<ConfigError>,
    /// The main file, then each include as it is reached, missing optional ones too.
    pub files: Vec<Arc<Path>>,
}

/// A bad line. The other lines still apply.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{}:{line}: expected `key = value`", path.display())]
    NoEquals { path: Arc<Path>, line: usize },
    #[error("{}:{line}: no key before `=`", path.display())]
    NoKey { path: Arc<Path>, line: usize },
    #[error("{}:{line}: cannot read {}: {source}", path.display(), include.display())]
    Include {
        path: Arc<Path>,
        line: usize,
        include: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{}:{line}: {} includes itself", path.display(), include.display())]
    Cycle { path: Arc<Path>, line: usize, include: PathBuf },
}

impl PartialEq for ConfigError {
    /// Equal when the variant, place and include match; an I/O cause compares by kind.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NoEquals { path, line }, Self::NoEquals { path: p2, line: l2 })
            | (Self::NoKey { path, line }, Self::NoKey { path: p2, line: l2 }) => {
                path == p2 && line == l2
            }
            (
                Self::Include { path, line, include, source },
                Self::Include { path: p2, line: l2, include: i2, source: s2 },
            ) => path == p2 && line == l2 && include == i2 && source.kind() == s2.kind(),
            (
                Self::Cycle { path, line, include },
                Self::Cycle { path: p2, line: l2, include: i2 },
            ) => path == p2 && line == l2 && include == i2,
            _ => false,
        }
    }
}

/// Where included files come from: the disk, or a map in tests.
pub trait Files {
    /// The text of `path`.
    ///
    /// # Errors
    ///
    /// The I/O error; [`io::ErrorKind::NotFound`] for a missing file.
    fn read(&self, path: &Path) -> io::Result<String>;

    /// The name that tells two paths to one file apart, for cycle checks.
    fn identity(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }
}

/// The real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct Disk;

impl Files for Disk {
    fn read(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    /// The canonical path, so a link or `..` cannot hide a cycle. A path that cannot be
    /// canonicalised is unreadable, and its read reports why.
    fn identity(&self, path: &Path) -> PathBuf {
        dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// An in-memory set of files.
impl<S: BuildHasher> Files for HashMap<PathBuf, String, S> {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.get(path).cloned().ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
}

/// Parse `text`, the contents of `origin`, reading its includes through `files`.
pub fn parse(text: &str, origin: &Path, files: &impl Files) -> Parsed {
    let mut reader = Reader { files, parsed: Parsed::default(), open: Vec::new() };
    let origin: Arc<Path> = Arc::from(origin);
    reader.watch(&origin);
    reader.file(text, &origin);
    reader.parsed
}

/// A value as written: its text, and whether quotes held it.
struct Value<'a> {
    text: &'a str,
    quoted: bool,
}

impl<'a> Value<'a> {
    /// The trimmed text after `=`, without a pair of enclosing quotes.
    fn of(raw: &'a str) -> Self {
        let raw = raw.trim();
        match raw.strip_prefix(QUOTE).and_then(|inner| inner.strip_suffix(QUOTE)) {
            Some(text) => Self { text, quoted: true },
            None => Self { text: raw, quoted: false },
        }
    }
}

struct Reader<'f, F> {
    files: &'f F,
    parsed: Parsed,
    /// The identities of the files being read, outermost first.
    open: Vec<PathBuf>,
}

impl<F: Files> Reader<'_, F> {
    fn file(&mut self, text: &str, origin: &Arc<Path>) {
        self.open.push(self.files.identity(origin));
        let text = text.strip_prefix(BOM).unwrap_or(text);
        for (line, content) in (1..).zip(text.lines()) {
            self.line(content, origin, line);
        }
        self.open.pop();
    }

    fn line(&mut self, content: &str, origin: &Arc<Path>, line: usize) {
        let content = content.trim();
        if content.is_empty() || content.starts_with(COMMENT) {
            return;
        }
        let error = match content.split_once('=') {
            None => ConfigError::NoEquals { path: Arc::clone(origin), line },
            Some((key, _)) if key.trim().is_empty() => {
                ConfigError::NoKey { path: Arc::clone(origin), line }
            }
            Some((key, raw)) => {
                let (key, value) = (key.trim(), Value::of(raw));
                if key == INCLUDE_KEY {
                    self.include(&value, origin, line);
                } else {
                    self.parsed.entries.push(Entry {
                        key: key.to_owned(),
                        value: value.text.to_owned(),
                        origin: Arc::clone(origin),
                        line,
                    });
                }
                return;
            }
        };
        self.parsed.errors.push(error);
    }

    fn include(&mut self, value: &Value, origin: &Arc<Path>, line: usize) {
        let (target, optional) = match value.text.strip_prefix(OPTIONAL) {
            Some(rest) if !value.quoted => (rest, true),
            _ => (value.text, false),
        };
        if target.is_empty() {
            return;
        }
        let path: Arc<Path> = match origin.parent() {
            Some(dir) => Arc::from(dir.join(target)),
            None => Arc::from(Path::new(target)),
        };
        self.watch(&path);
        if self.open.contains(&self.files.identity(&path)) {
            self.parsed.errors.push(ConfigError::Cycle {
                path: Arc::clone(origin),
                line,
                include: path.to_path_buf(),
            });
            return;
        }
        match self.files.read(&path) {
            Ok(text) => self.file(&text, &path),
            Err(e) if optional && e.kind() == io::ErrorKind::NotFound => {}
            Err(source) => self.parsed.errors.push(ConfigError::Include {
                path: Arc::clone(origin),
                line,
                include: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Add `path` to the watched files once.
    fn watch(&mut self, path: &Arc<Path>) {
        if !self.parsed.files.contains(path) {
            self.parsed.files.push(Arc::clone(path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The directory the main file sits in: absolute on every platform.
    #[cfg(unix)]
    const ROOT: &str = "/c";
    #[cfg(windows)]
    const ROOT: &str = r"C:\c";
    const MAIN: &str = "config.triad";

    /// `rel` under [`ROOT`].
    fn at(rel: &str) -> PathBuf {
        Path::new(ROOT).join(rel)
    }

    fn path(rel: &str) -> Arc<Path> {
        Arc::from(at(rel))
    }

    fn entry(key: &str, value: &str, origin: &str, line: usize) -> Entry {
        Entry { key: key.to_owned(), value: value.to_owned(), origin: path(origin), line }
    }

    fn files(list: &[(&str, &str)]) -> HashMap<PathBuf, String> {
        list.iter().map(|&(p, text)| (at(p), text.to_owned())).collect()
    }

    fn parse_main(text: &str) -> Parsed {
        parse(text, &at(MAIN), &HashMap::new())
    }

    /// The (key, value) pairs of `text`'s entries.
    fn pairs(text: &str) -> Vec<(String, String)> {
        let parsed = parse_main(text);
        assert_eq!(parsed.errors, Vec::new(), "no errors in {text:?}");
        parsed.entries.into_iter().map(|e| (e.key, e.value)).collect()
    }

    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_owned(), value.to_owned())
    }

    #[test]
    fn blank_and_comment_lines_give_no_entries() {
        assert_eq!(pairs(""), Vec::new());
        assert_eq!(pairs("# a comment\n   # indented\n"), Vec::new());
        assert_eq!(pairs("\n   \n\t\n"), Vec::new());
    }

    #[test]
    fn spaces_around_the_key_and_value_are_trimmed() {
        for text in ["key=value", "key = value", "  key   =   value  ", "\tkey\t=\tvalue\t"] {
            assert_eq!(pairs(text), vec![pair("key", "value")], "{text:?}");
        }
    }

    #[test]
    fn entries_carry_their_file_and_line() {
        assert_eq!(
            parse_main("# head\na = 1\n\nb = 2").entries,
            vec![entry("a", "1", MAIN, 2), entry("b", "2", MAIN, 4)]
        );
    }

    #[test]
    fn quotes_keep_inner_spaces_and_a_hash_belongs_to_the_value() {
        assert_eq!(pairs(r#"font = "  Mono  ""#), vec![pair("font", "  Mono  ")]);
        assert_eq!(pairs("font = a # b"), vec![pair("font", "a # b")]);
        assert_eq!(pairs(r#"font = """#), vec![pair("font", "")]);
    }

    #[test]
    fn a_lone_or_inner_quote_is_part_of_the_value() {
        assert_eq!(pairs(r#"font = ""#), vec![pair("font", "\"")]);
        assert_eq!(pairs(r#"font = a "b" c"#), vec![pair("font", r#"a "b" c"#)]);
        assert_eq!(pairs(r#"font = "a"#), vec![pair("font", "\"a")]);
    }

    #[test]
    fn the_value_runs_from_the_first_equals_sign() {
        assert_eq!(pairs("keybind = cmd+j=next"), vec![pair("keybind", "cmd+j=next")]);
    }

    #[test]
    fn an_empty_value_is_kept_for_the_default() {
        assert_eq!(pairs("font-size ="), vec![pair("font-size", "")]);
    }

    #[test]
    fn a_repeated_key_keeps_both_entries_in_order() {
        assert_eq!(
            pairs("font-size = 12\nfont-size = 14"),
            vec![pair("font-size", "12"), pair("font-size", "14")]
        );
    }

    #[test]
    fn crlf_reads_as_lf_and_a_leading_bom_is_skipped() {
        assert_eq!(
            parse_main("\u{feff}a = 1\r\nb = 2\r\n").entries,
            vec![entry("a", "1", MAIN, 1), entry("b", "2", MAIN, 2)]
        );
    }

    #[test]
    fn keys_keep_their_case() {
        assert_eq!(pairs("Font-Size = 12"), vec![pair("Font-Size", "12")]);
    }

    #[test]
    fn a_line_without_equals_or_key_is_an_error_and_the_rest_applies() {
        let parsed = parse_main("a = 1\nnonsense\n = 2\nb = 3");
        assert_eq!(parsed.entries, vec![entry("a", "1", MAIN, 1), entry("b", "3", MAIN, 4)]);
        assert_eq!(
            parsed.errors,
            vec![
                ConfigError::NoEquals { path: path(MAIN), line: 2 },
                ConfigError::NoKey { path: path(MAIN), line: 3 },
            ]
        );
    }

    #[test]
    fn an_include_reads_in_place_relative_to_the_including_file() {
        let disk = files(&[("local.conf", "b = inner\nc = inner")]);
        let parsed = parse("a = 1\nconfig-file = local.conf\nb = outer", &at(MAIN), &disk);
        assert_eq!(parsed.errors, Vec::new());
        assert_eq!(
            parsed.entries,
            vec![
                entry("a", "1", MAIN, 1),
                entry("b", "inner", "local.conf", 1),
                entry("c", "inner", "local.conf", 2),
                entry("b", "outer", MAIN, 3),
            ]
        );
        assert_eq!(parsed.files, vec![path(MAIN), path("local.conf")]);
    }

    #[test]
    fn a_nested_include_is_relative_to_its_own_includer() {
        let disk =
            files(&[("sub/one.conf", "config-file = two.conf"), ("sub/two.conf", "k = deep")]);
        let parsed = parse("config-file = sub/one.conf", &at(MAIN), &disk);
        assert_eq!(parsed.errors, Vec::new());
        assert_eq!(parsed.entries, vec![entry("k", "deep", "sub/two.conf", 1)]);
    }

    #[test]
    fn an_absolute_include_is_read_as_given() {
        let elsewhere = at("etc/triad.conf");
        let disk = HashMap::from([(elsewhere.clone(), "k = v".to_owned())]);
        let text = format!("config-file = {}", elsewhere.display());
        let parsed = parse(&text, &at("sub/config.triad"), &disk);
        assert_eq!(parsed.entries, vec![entry("k", "v", "etc/triad.conf", 1)]);
    }

    #[test]
    fn a_missing_optional_include_is_skipped_but_watched() {
        let parsed = parse_main("config-file = ?local.conf\na = 1");
        assert_eq!(parsed.errors, Vec::new());
        assert_eq!(parsed.entries, vec![entry("a", "1", MAIN, 2)]);
        assert_eq!(parsed.files, vec![path(MAIN), path("local.conf")]);
    }

    #[test]
    fn a_present_optional_include_is_read() {
        let disk = files(&[("local.conf", "k = v")]);
        let parsed = parse("config-file = ?local.conf", &at(MAIN), &disk);
        assert_eq!(parsed.entries, vec![entry("k", "v", "local.conf", 1)]);
    }

    #[test]
    fn a_missing_required_include_is_an_error_naming_it() {
        let parsed = parse_main("a = 1\nconfig-file = local.conf\nb = 2");
        assert_eq!(parsed.entries, vec![entry("a", "1", MAIN, 1), entry("b", "2", MAIN, 3)]);
        assert_eq!(
            parsed.errors,
            vec![ConfigError::Include {
                path: path(MAIN),
                line: 2,
                include: at("local.conf"),
                source: io::ErrorKind::NotFound.into(),
            }]
        );
        assert_eq!(parsed.files, vec![path(MAIN), path("local.conf")]);
    }

    #[test]
    fn a_quoted_question_mark_is_part_of_a_required_path() {
        let disk = files(&[("?odd", "k = v")]);
        let parsed = parse(r#"config-file = "?odd""#, &at(MAIN), &disk);
        assert_eq!(parsed.errors, Vec::new());
        assert_eq!(parsed.entries, vec![entry("k", "v", "?odd", 1)]);

        let missing = parse_main(r#"config-file = "?odd""#);
        assert_eq!(
            missing.errors,
            vec![ConfigError::Include {
                path: path(MAIN),
                line: 1,
                include: at("?odd"),
                source: io::ErrorKind::NotFound.into(),
            }]
        );
    }

    #[test]
    fn an_empty_include_reads_nothing() {
        let parsed = parse_main("config-file =\na = 1");
        assert_eq!(parsed.errors, Vec::new());
        assert_eq!(parsed.entries, vec![entry("a", "1", MAIN, 2)]);
        assert_eq!(parsed.files, vec![path(MAIN)]);
    }

    #[test]
    fn includes_are_not_entries() {
        let disk = files(&[("local.conf", "")]);
        let parsed = parse("config-file = local.conf", &at(MAIN), &disk);
        assert_eq!(parsed.entries, Vec::new());
    }

    #[test]
    fn a_file_including_itself_is_a_cycle() {
        let disk = files(&[(MAIN, "config-file = config.triad\nk = v")]);
        let parsed = parse("config-file = config.triad\nk = v", &at(MAIN), &disk);
        assert_eq!(
            parsed.errors,
            vec![ConfigError::Cycle { path: path(MAIN), line: 1, include: at(MAIN) }]
        );
        assert_eq!(parsed.entries, vec![entry("k", "v", MAIN, 2)]);
    }

    #[test]
    fn a_cycle_through_two_files_names_the_closing_include() {
        let disk = files(&[
            ("a.conf", "config-file = b.conf\nfrom = a"),
            ("b.conf", "config-file = a.conf\nfrom = b"),
        ]);
        let parsed = parse("config-file = a.conf", &at(MAIN), &disk);
        assert_eq!(
            parsed.errors,
            vec![ConfigError::Cycle { path: path("b.conf"), line: 1, include: at("a.conf") }]
        );
        assert_eq!(
            parsed.entries,
            vec![entry("from", "b", "b.conf", 2), entry("from", "a", "a.conf", 2),]
        );
    }

    #[test]
    fn a_file_included_twice_without_a_cycle_reads_twice() {
        let disk = files(&[("x.conf", "k = x")]);
        let parsed =
            parse("config-file = x.conf\nk = main\nconfig-file = x.conf", &at(MAIN), &disk);
        assert_eq!(parsed.errors, Vec::new());
        assert_eq!(
            parsed.entries,
            vec![
                entry("k", "x", "x.conf", 1),
                entry("k", "main", MAIN, 2),
                entry("k", "x", "x.conf", 1),
            ]
        );
        assert_eq!(parsed.files, vec![path(MAIN), path("x.conf")]);
    }

    #[test]
    fn errors_in_an_include_name_its_file() {
        let disk = files(&[("x.conf", "ok = 1\nbroken")]);
        let parsed = parse("config-file = x.conf", &at(MAIN), &disk);
        assert_eq!(parsed.errors, vec![ConfigError::NoEquals { path: path("x.conf"), line: 2 }]);
        assert_eq!(
            parsed.errors[0].to_string(),
            format!("{}:2: expected `key = value`", at("x.conf").display())
        );
    }

    #[test]
    fn a_bom_is_skipped_in_an_include_too() {
        let disk = files(&[("x.conf", "\u{feff}k = v")]);
        let parsed = parse("config-file = x.conf", &at(MAIN), &disk);
        assert_eq!(parsed.entries, vec![entry("k", "v", "x.conf", 1)]);
    }
}
