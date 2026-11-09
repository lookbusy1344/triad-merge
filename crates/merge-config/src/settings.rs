//! The settings (spec §6.4): a config file (`config`) in the user's config directory, read key
//! by key. The command line's merge options hold over it for one run.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use strum::{EnumMessage, IntoStaticStr, VariantArray};

use crate::named::Named;
use merge_core::diff::{self, DiffOptions, LineEndingRule};
use merge_core::document::{MarkerStyle, OutputEnding};
use merge_core::merge3::{HunkLevel, Refine, SameChange};

use crate::config::{self, ConfigError, Disk, Entry, Files};
use crate::keybind::{KeyBindings, Keybind, KeybindError, Platform};

/// The config file, in place of the one in the user's config directory.
pub const FILE_VAR: &str = "TRIAD_CONFIG";
/// The config file's directory under the user's config directory.
const DIR_NAME: &str = "triad";
const FILE_NAME: &str = "config.triad";
/// The user's config directory under the home directory, without `XDG_CONFIG_HOME`.
const DOT_CONFIG: &str = ".config";
const QUOTE: char = '"';
/// Parts the values of a key of many values.
const VALUE_SEPARATOR: char = '\n';
pub const DEFAULT_FONT_SIZE: f32 = 10.;
pub const DEFAULT_TAB_WIDTH: usize = 4;
pub const FONT_SIZES: RangeInclusive<f32> = 6.0..=72.0;
/// The toolbar's text size in points: its labels, its conflict badge and its menus.
pub const DEFAULT_TOOLBAR_FONT_SIZE: f32 = 8.4;
/// Toolbar text sizes that fit the toolbar's buttons and menu rows.
pub const TOOLBAR_FONT_SIZES: RangeInclusive<f32> = 6.0..=12.0;
pub const TAB_WIDTHS: RangeInclusive<usize> = 1..=16;
/// The size the merge and diff windows open at, in points.
pub const DEFAULT_WINDOW_SIZE: WindowSize = WindowSize { width: 1400, height: 900 };
/// Window widths and heights, in points.
pub const WINDOW_SIZES: RangeInclusive<u16> = 400..=16_384;
/// Parts a window size's width from its height.
const SIZE_SEPARATOR: char = 'x';
/// What `window-size` takes.
const SIZE_FORM: &str = "WIDTHxHEIGHT, in points";
pub const DEFAULT_LINE_HEIGHT: f32 = 1.5;
/// Line heights, as multiples of the text size.
pub const LINE_HEIGHTS: RangeInclusive<f32> = 1.0..=3.0;
/// Band opacities: 0 is clear, 1 opaque.
pub const BAND_OPACITIES: RangeInclusive<f32> = 0.0..=1.0;
/// The digits of a colour after its `#`.
const COLOUR_DIGITS: usize = 6;
const HEX_RADIX: u32 = 16;
const COLOUR_PREFIX: char = '#';
/// The theme names of a pair, `light:A,dark:B`.
const LIGHT: &str = "light";
const DARK: &str = "dark";
const PAIR_SEPARATOR: char = ',';
const THEME_SEPARATOR: char = ':';
/// What a colour key takes.
const COLOUR_FORMS: &str = "#rrggbb or light:#rrggbb,dark:#rrggbb";
/// What `band-opacity` takes in place of a malformed pair.
const OPACITY_PAIR: &str = "light:N,dark:N";
const BAND_OPACITY_KEY: &str = "band-opacity";
const FONT_SIZE_KEY: &str = "font-size";
const TOOLBAR_FONT_SIZE_KEY: &str = "toolbar-font-size";
const WINDOW_SIZE_KEY: &str = "window-size";
const TAB_WIDTH_KEY: &str = "tab-width";
const SYNTAX_WHOLE_FILE_MAX_LINES_KEY: &str = "syntax-whole-file-max-lines";
const SYNTAX_BOUNDARIES_KEY: &str = "syntax-boundaries";
const MERGE_IGNORE_LINE_ENDINGS_KEY: &str = "merge-ignore-line-endings";
const DIFF_IGNORE_LINE_ENDINGS_KEY: &str = "diff-ignore-line-endings";
/// An editor's whole syntax tree costs about 90 bytes per byte of text
/// (`docs/PLAN.md` §F6.3): some 60 MB for 20 000 lines of Rust. A longer file parses a
/// chunk at a time (§4.1).
pub const DEFAULT_SYNTAX_MAX_LINES: usize = 20_000;

/// Every setting, with its default where the file has none.
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is one `true`/`false` key of the file, set and read on its own"
)]
pub struct Settings {
    /// Whose merge semantics apply; `auto` follows the repository.
    pub semantics: Choice,
    /// Whether a change every side made resolves; without it, jj's config under jj's
    /// semantics, else `accept`.
    pub same_change: Option<SameChange>,
    /// Whether conflicts merge word by word; without it, jj's config under jj's semantics,
    /// else `line`.
    pub hunk_level: Option<HunkLevel>,
    /// The merge's diff options; its line-ending rule is the `merge-ignore-line-endings` key.
    pub diff: DiffOptions,
    /// Whether the two-way diff compares line endings: the `diff-ignore-line-endings` key.
    pub diff_line_endings: LineEndingRule,
    /// How far conflicts are refined (spec §4.3): `true`, `false` or `"deep"` in the file.
    pub refine: Refine,
    /// Whether deep refinement and the word merge keep off the inside of syntax nodes (spec
    /// §4.3).
    pub syntax_boundaries: bool,
    pub tab_width: usize,
    /// The editor font family; the theme's monospace font without one.
    pub font: Option<String>,
    /// Text size in points.
    pub font_size: f32,
    /// The toolbar's text size in points: its labels, its conflict badge and its menus.
    pub toolbar_font_size: f32,
    /// The size the merge and diff windows open at.
    pub window_size: WindowSize,
    /// The panes share one scroll position (spec §5).
    pub scroll_sync: bool,
    /// When the panes show their scrollbars (spec §5).
    pub scrollbars: Scrollbars,
    /// The merge window opens with the base pane showing.
    pub base_pane: bool,
    /// The line origin column (View → Blame) shows once a window's origins load (spec §5, §7.3).
    pub origin_column: bool,
    /// The ID that names a jj revision in the pane headings and the origin column (spec §6.4).
    pub revision_id: RevisionId,
    /// The windows' light or dark appearance.
    pub theme: Theme,
    /// Line height as a multiple of the text size.
    pub line_height: f32,
    /// The tones' colours and the bands' opacity.
    pub colours: Colours,
    /// The markers of a two-sided conflict under git semantics; without one, git's
    /// `merge.conflictStyle`, else diff3.
    pub conflict_style: Option<ConflictStyle>,
    /// The line endings of the saved merge.
    pub line_ending: OutputEnding,
    /// Save and Exit asks first while conflicts are unresolved.
    pub confirm_unresolved_save: bool,
    /// Each pane colours its text by its language's syntax.
    pub syntax: bool,
    /// The longest file, in lines, the result and diff editors colour by syntax.
    pub syntax_max_lines: usize,
    /// Long lines wrap in the result, input and diff panes when a window opens; else they scroll
    /// sideways.
    pub soft_wrap: bool,
    /// The keys and the commands they run (`docs/PLAN.md` §E9).
    pub key_bindings: KeyBindings,
}

/// A window's width and height in points, `WIDTHxHEIGHT` in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSize {
    pub width: u16,
    pub height: u16,
}

impl fmt::Display for WindowSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{SIZE_SEPARATOR}{}", self.width, self.height)
    }
}

/// The markers of a two-sided conflict, as git's `merge.conflictStyle` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum ConflictStyle {
    Diff3,
    Merge,
}

impl From<ConflictStyle> for MarkerStyle {
    fn from(style: ConflictStyle) -> Self {
        match style {
            ConflictStyle::Diff3 => Self::Diff3,
            ConflictStyle::Merge => Self::Merge,
        }
    }
}

/// The `output-line-ending` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum LineEndingArg {
    Keep,
    Base,
    Lf,
    Crlf,
}

impl From<LineEndingArg> for OutputEnding {
    fn from(arg: LineEndingArg) -> Self {
        match arg {
            LineEndingArg::Keep => Self::Keep,
            LineEndingArg::Base => Self::Base,
            LineEndingArg::Lf => Self::Lf,
            LineEndingArg::Crlf => Self::CrLf,
        }
    }
}

impl From<OutputEnding> for LineEndingArg {
    fn from(ending: OutputEnding) -> Self {
        match ending {
            OutputEnding::Keep => Self::Keep,
            OutputEnding::Base => Self::Base,
            OutputEnding::Lf => Self::Lf,
            OutputEnding::CrLf => Self::Crlf,
        }
    }
}

/// An opaque sRGB colour, `#rrggbb` in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Colour {
    /// The colour `0xrrggbb`; the top byte is ignored.
    #[must_use]
    pub const fn rgb(hex: u32) -> Self {
        let [_, r, g, b] = hex.to_be_bytes();
        Self { r, g, b }
    }

    /// The colour as `0xrrggbb`.
    #[must_use]
    pub const fn hex(self) -> u32 {
        u32::from_be_bytes([0, self.r, self.g, self.b])
    }
}

impl fmt::Display for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{COLOUR_PREFIX}{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// A value for each theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Themed<T> {
    pub light: T,
    pub dark: T,
}

impl<T: Copy> Themed<T> {
    /// `value` in both themes.
    #[must_use]
    pub const fn both(value: T) -> Self {
        Self { light: value, dark: value }
    }

    /// The dark theme's value when `dark`, else the light theme's.
    #[must_use]
    pub const fn get(self, dark: bool) -> T {
        if dark { self.dark } else { self.light }
    }
}

/// The colour of each tone (spec §5) and of the marks beside them, and the opacity of the
/// bands behind changed rows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colours {
    pub left: Themed<Colour>,
    pub right: Themed<Colour>,
    pub base: Themed<Colour>,
    pub conflict: Themed<Colour>,
    pub edited: Themed<Colour>,
    pub both: Themed<Colour>,
    pub word_merged: Themed<Colour>,
    /// The current region's bars and frame.
    pub current: Themed<Colour>,
    /// Deletion notes and marks.
    pub deleted: Themed<Colour>,
    /// The conflict badge once no conflict is left.
    pub resolved: Themed<Colour>,
    pub band_opacity: Themed<f32>,
}

/// A colour in each theme: `light` and `dark` as `0xrrggbb`.
const fn themed_colour(light: u32, dark: u32) -> Themed<Colour> {
    Themed { light: Colour::rgb(light), dark: Colour::rgb(dark) }
}

/// The colours of spec §6.4: each tone's hue at HSL saturation 0.7 and lightness 0.45 in the
/// light theme, 0.65 in the dark; the current region's lime at saturation 0.85 and lightness
/// 0.4 (light), 0.75 and 0.55 (dark).
pub const DEFAULT_COLOURS: Colours = Colours {
    left: themed_colour(0x22_73_c3, 0x67_a6_e4),
    right: themed_colour(0xc3_22_73, 0xe4_67_a6),
    base: themed_colour(0xc3_b6_22, 0xe4_da_67),
    conflict: themed_colour(0xc3_65_22, 0xe4_9b_67),
    edited: themed_colour(0x80_22_c3, 0xb0_67_e4),
    both: themed_colour(0x22_c3_c3, 0x67_e4_e4),
    word_merged: themed_colour(0x22_c3_22, 0x67_e4_67),
    current: themed_colour(0x6c_bd_0f, 0x92_e2_36),
    deleted: themed_colour(0xc3_22_22, 0xe4_67_67),
    resolved: themed_colour(0x22_c3_58, 0x67_e4_91),
    band_opacity: Themed { light: 0.18, dark: 0.28 },
};

impl Default for Colours {
    fn default() -> Self {
        DEFAULT_COLOURS
    }
}

/// The ID that names a jj revision: its change ID or its commit ID. git names commits alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum RevisionId {
    #[default]
    Change,
    Commit,
}

impl From<RevisionId> for vcs::IdKind {
    fn from(id: RevisionId) -> Self {
        match id {
            RevisionId::Change => Self::Change,
            RevisionId::Commit => Self::Commit,
        }
    }
}

/// The windows' appearance: the system's, or held light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// When the panes show their scrollbars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum Scrollbars {
    #[default]
    Always,
    /// While the pointer is over the pane.
    Hover,
    /// While the pane scrolls; they fade once it stops.
    WhenScrolling,
}

impl Settings {
    /// The two-way diff's options: the merge's, with the diff window's line-ending rule.
    #[must_use]
    pub fn two_way(&self) -> DiffOptions {
        DiffOptions { line_endings: self.diff_line_endings, ..self.diff }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            semantics: Choice::Auto,
            same_change: None,
            hunk_level: None,
            diff: DiffOptions::default(),
            diff_line_endings: LineEndingRule::Ignore,
            refine: Refine::Ends,
            syntax_boundaries: false,
            tab_width: DEFAULT_TAB_WIDTH,
            font: None,
            font_size: DEFAULT_FONT_SIZE,
            toolbar_font_size: DEFAULT_TOOLBAR_FONT_SIZE,
            window_size: DEFAULT_WINDOW_SIZE,
            scroll_sync: true,
            scrollbars: Scrollbars::Always,
            base_pane: false,
            origin_column: false,
            revision_id: RevisionId::Change,
            theme: Theme::System,
            line_height: DEFAULT_LINE_HEIGHT,
            colours: DEFAULT_COLOURS,
            conflict_style: None,
            line_ending: OutputEnding::Keep,
            confirm_unresolved_save: true,
            syntax: true,
            syntax_max_lines: DEFAULT_SYNTAX_MAX_LINES,
            soft_wrap: false,
            key_bindings: KeyBindings::default(),
        }
    }
}

/// `--merge-semantics` and the settings file's `merge-semantics`.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    VariantArray,
    IntoStaticStr,
    EnumMessage,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum Choice {
    /// The caller's VCS, found from the paths.
    #[default]
    Auto,
    Git,
    Jj,
}

/// `--same-change` and the settings file's `same-change`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, VariantArray, IntoStaticStr, EnumMessage, Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum SameChangeArg {
    Accept,
    Keep,
}

impl From<SameChangeArg> for SameChange {
    fn from(arg: SameChangeArg) -> Self {
        match arg {
            SameChangeArg::Accept => SameChange::Accept,
            SameChangeArg::Keep => SameChange::Keep,
        }
    }
}

/// `--hunk-level` and the settings file's `hunk-level`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, VariantArray, IntoStaticStr, EnumMessage, Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum HunkLevelArg {
    Line,
    Word,
}

impl From<HunkLevelArg> for HunkLevel {
    fn from(arg: HunkLevelArg) -> Self {
        match arg {
            HunkLevelArg::Line => HunkLevel::Line,
            HunkLevelArg::Word => HunkLevel::Word,
        }
    }
}

/// The diff algorithm, as the file and the command line name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum Algorithm {
    Histogram,
    Myers,
    MyersMinimal,
}

impl From<Algorithm> for diff::Algorithm {
    fn from(algorithm: Algorithm) -> Self {
        match algorithm {
            Algorithm::Histogram => diff::Algorithm::Histogram,
            Algorithm::Myers => diff::Algorithm::Myers,
            Algorithm::MyersMinimal => diff::Algorithm::MyersMinimal,
        }
    }
}

/// The whitespace mode, as the file and the command line name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, VariantArray, IntoStaticStr, EnumMessage)]
#[strum(serialize_all = "kebab-case")]
pub enum Whitespace {
    None,
    Trailing,
    Amount,
    All,
}

impl From<Whitespace> for diff::Whitespace {
    fn from(whitespace: Whitespace) -> Self {
        match whitespace {
            Whitespace::None => diff::Whitespace::None,
            Whitespace::Trailing => diff::Whitespace::Trailing,
            Whitespace::Amount => diff::Whitespace::Amount,
            Whitespace::All => diff::Whitespace::All,
        }
    }
}

/// Why a line of the config file, or the file itself, could not be used. The other lines
/// still apply.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("{}: cannot read it: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{}:{line}: {problem}", path.display())]
    Value { path: Arc<Path>, line: usize, problem: Problem },
}

/// What is wrong with one `key = value` line.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Problem {
    #[error("unknown setting `{0}`")]
    UnknownKey(String),
    #[error("{key} = {value}: expected {expected}")]
    Invalid { key: &'static str, value: String, expected: String },
    #[error(
        "font-size {0} is outside {start}–{end}",
        start = FONT_SIZES.start(),
        end = FONT_SIZES.end()
    )]
    FontSize(f32),
    #[error(
        "toolbar-font-size {0} is outside {start}–{end}",
        start = TOOLBAR_FONT_SIZES.start(),
        end = TOOLBAR_FONT_SIZES.end()
    )]
    ToolbarFontSize(f32),
    #[error(
        "tab-width {0} is outside {start}–{end}",
        start = TAB_WIDTHS.start(),
        end = TAB_WIDTHS.end()
    )]
    TabWidth(usize),
    #[error("{key} {value} is outside {start}–{end}")]
    OutOfRange {
        key: &'static str,
        /// The value as written.
        value: String,
        start: f32,
        end: f32,
    },
    #[error(transparent)]
    Keybind(#[from] KeybindError),
}

/// The settings a config file gives, and the lines it could not use.
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    pub errors: Vec<SettingsError>,
}

/// Where the config file is looked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locations {
    /// `TRIAD_CONFIG`.
    pub config: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
    /// `APPDATA`, on Windows.
    pub app_data: Option<PathBuf>,
}

impl Locations {
    /// This process's. Empty variables count as unset.
    #[must_use]
    pub fn from_process() -> Self {
        let var = |name| {
            std::env::var_os(name).filter(|value: &OsString| !value.is_empty()).map(PathBuf::from)
        };
        Self {
            config: var(FILE_VAR),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            home: directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned()),
            app_data: var("APPDATA"),
        }
    }

    /// The config file under this platform's rules.
    #[must_use]
    pub fn path(&self) -> Option<PathBuf> {
        if cfg!(windows) { self.windows_path() } else { self.unix_path() }
    }

    /// `TRIAD_CONFIG`; else `triad/config.triad` in `XDG_CONFIG_HOME` when it is absolute, as
    /// the XDG base directory rules ask, or in `~/.config`.
    fn unix_path(&self) -> Option<PathBuf> {
        self.config.clone().or_else(|| {
            let dir = self
                .xdg_config_home
                .clone()
                .filter(|dir| dir.is_absolute())
                .or_else(|| self.home.as_ref().map(|home| home.join(DOT_CONFIG)))?;
            Some(dir.join(DIR_NAME).join(FILE_NAME))
        })
    }

    /// `TRIAD_CONFIG`; else `triad\config.triad` in `%APPDATA%`.
    fn windows_path(&self) -> Option<PathBuf> {
        self.config
            .clone()
            .or_else(|| self.app_data.as_ref().map(|dir| dir.join(DIR_NAME).join(FILE_NAME)))
    }
}

/// The config file of this process: `TRIAD_CONFIG`, or the one in the user's config
/// directory.
#[must_use]
pub fn path() -> Option<PathBuf> {
    Locations::from_process().path()
}

/// Read the config file at `path` and its includes. A missing file gives the defaults.
#[must_use]
pub fn load(path: &Path) -> Loaded {
    load_with(path, &Disk)
}

/// [`load`], reading files through `files`.
pub fn load_with(path: &Path, files: &impl Files) -> Loaded {
    let text = match files.read(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Loaded {
                settings: Settings::default(),
                errors: vec![SettingsError::Read { path: path.to_path_buf(), source }],
            };
        }
    };
    let parsed = config::parse(&text, path, files);
    let (settings, problems) = from_entries(&parsed.entries);
    let mut errors: Vec<SettingsError> = parsed.errors.into_iter().map(Into::into).collect();
    errors.extend(problems);
    Loaded { settings, errors }
}

/// Settings from `entries` in order, so a later entry wins. A bad entry leaves its key as
/// it was and is reported.
#[must_use]
pub fn from_entries(entries: &[Entry]) -> (Settings, Vec<SettingsError>) {
    let mut settings = Settings::default();
    let errors = entries
        .iter()
        .filter_map(|entry| {
            apply(&mut settings, &entry.key, &entry.value).err().map(|problem| {
                SettingsError::Value { path: Arc::clone(&entry.origin), line: entry.line, problem }
            })
        })
        .collect();
    (settings, errors)
}

/// Set `key` to `value` in `settings`; an empty value sets the key's default.
///
/// # Errors
///
/// [`Problem`] for a key that does not exist or a value it does not take; `settings` is
/// left as it was.
pub fn apply(settings: &mut Settings, key: &str, value: &str) -> Result<(), Problem> {
    let key =
        KEYS.iter().find(|k| k.name == key).ok_or_else(|| Problem::UnknownKey(key.to_owned()))?;
    (key.set)(settings, value)
}

/// A setting: its name in the file, what it does, and how a value reads and writes.
struct Key {
    name: &'static str,
    description: &'static str,
    set: fn(&mut Settings, &str) -> Result<(), Problem>,
    /// The value as [`Key::set`] reads it; empty for none. A key of many values gives one
    /// per line.
    get: fn(&Settings) -> String,
}

/// The key `color-…` for the colour in `Colours::$field`.
macro_rules! colour_key {
    ($name:literal, $field:ident, $what:literal) => {
        Key {
            name: $name,
            description: concat!($what, ": #rrggbb, or light:#rrggbb,dark:#rrggbb."),
            set: |s, v| {
                s.colours.$field = or_default(v, DEFAULT_COLOURS.$field, |v| {
                    themed($name, v, COLOUR_FORMS, |part| {
                        colour(part).ok_or_else(|| invalid($name, v, COLOUR_FORMS))
                    })
                })?;
                Ok(())
            },
            get: |s| render_themed(s.colours.$field),
        }
    };
}

/// Every setting, in the order of spec §6.4.
const KEYS: &[Key] = &[
    Key {
        name: "merge-semantics",
        description: "Whose merge rules apply: auto follows the repository, or git or jj.",
        set: |s, v| {
            s.semantics =
                or_default(v, Settings::default().semantics, |v| named("merge-semantics", v))?;
            Ok(())
        },
        get: |s| name(s.semantics),
    },
    Key {
        name: "same-change",
        description: "Whether a change every side made resolves (accept) or conflicts (keep). \
                      Empty: jj's merge.same-change under jj semantics, else accept.",
        set: |s, v| {
            s.same_change =
                optional(v, |v| named::<SameChangeArg>("same-change", v))?.map(Into::into);
            Ok(())
        },
        get: |s| match s.same_change {
            None => String::new(),
            Some(SameChange::Accept) => name(SameChangeArg::Accept),
            Some(SameChange::Keep) => name(SameChangeArg::Keep),
        },
    },
    Key {
        name: "hunk-level",
        description: "Whether conflicts merge line by line or word by word. \
                      Empty: jj's merge.hunk-level under jj semantics, else line.",
        set: |s, v| {
            s.hunk_level = optional(v, |v| named::<HunkLevelArg>("hunk-level", v))?.map(Into::into);
            Ok(())
        },
        get: |s| match s.hunk_level {
            None => String::new(),
            Some(HunkLevel::Line) => name(HunkLevelArg::Line),
            Some(HunkLevel::Word) => name(HunkLevelArg::Word),
        },
    },
    Key {
        name: "diff-algorithm",
        description: "The diff algorithm: histogram, myers or myers-minimal.",
        set: |s, v| {
            s.diff.algorithm = or_default(v, Settings::default().diff.algorithm, |v| {
                named::<Algorithm>("diff-algorithm", v).map(Into::into)
            })?;
            Ok(())
        },
        get: |s| {
            name(match s.diff.algorithm {
                diff::Algorithm::Histogram => Algorithm::Histogram,
                diff::Algorithm::Myers => Algorithm::Myers,
                diff::Algorithm::MyersMinimal => Algorithm::MyersMinimal,
            })
        },
    },
    Key {
        name: "ignore-whitespace",
        description: "The whitespace changes the diff ignores: none, trailing, amount or all.",
        set: |s, v| {
            s.diff.whitespace = or_default(v, Settings::default().diff.whitespace, |v| {
                named::<Whitespace>("ignore-whitespace", v).map(Into::into)
            })?;
            Ok(())
        },
        get: |s| {
            name(match s.diff.whitespace {
                diff::Whitespace::None => Whitespace::None,
                diff::Whitespace::Trailing => Whitespace::Trailing,
                diff::Whitespace::Amount => Whitespace::Amount,
                diff::Whitespace::All => Whitespace::All,
            })
        },
    },
    Key {
        name: MERGE_IGNORE_LINE_ENDINGS_KEY,
        description: "Merges compare \\r\\n, \\n and a missing final newline as equal. \
                      The saved bytes stay each line's.",
        set: |s, v| {
            s.diff.line_endings = or_default(v, Settings::default().diff.line_endings, |v| {
                line_ending_rule(MERGE_IGNORE_LINE_ENDINGS_KEY, v)
            })?;
            Ok(())
        },
        get: |s| ignores(s.diff.line_endings),
    },
    Key {
        name: DIFF_IGNORE_LINE_ENDINGS_KEY,
        description: "The diff window and diff --inspect compare \\r\\n, \\n and a missing \
                      final newline as equal. Saves keep each line's bytes.",
        set: |s, v| {
            s.diff_line_endings = or_default(v, Settings::default().diff_line_endings, |v| {
                line_ending_rule(DIFF_IGNORE_LINE_ENDINGS_KEY, v)
            })?;
            Ok(())
        },
        get: |s| ignores(s.diff_line_endings),
    },
    Key {
        name: "refine-conflicts",
        description: "How far conflicts are refined: false, true (lines every side shares \
                      split off at a conflict's ends) or deep (inside it too).",
        set: |s, v| {
            s.refine = or_default(v, Settings::default().refine, refine)?;
            Ok(())
        },
        get: |s| {
            match s.refine {
                Refine::Off => "false",
                Refine::Ends => "true",
                Refine::Deep => "deep",
            }
            .to_owned()
        },
    },
    Key {
        name: SYNTAX_BOUNDARIES_KEY,
        description: "Keep deep refinement's splits and word merges off the inside of syntax \
                      nodes, in files of a known language.",
        set: |s, v| {
            s.syntax_boundaries = or_default(v, Settings::default().syntax_boundaries, |v| {
                boolean(SYNTAX_BOUNDARIES_KEY, v)
            })?;
            Ok(())
        },
        get: |s| s.syntax_boundaries.to_string(),
    },
    Key {
        name: TAB_WIDTH_KEY,
        description: "Columns between tab stops, 1–16. The Tab key types that many spaces.",
        set: |s, v| {
            s.tab_width = or_default(v, DEFAULT_TAB_WIDTH, tab_width)?;
            Ok(())
        },
        get: |s| s.tab_width.to_string(),
    },
    Key {
        name: "font",
        description: "The editor font family. Empty: the theme's monospace font.",
        set: |s, v| {
            s.font = (!v.is_empty()).then(|| v.to_owned());
            Ok(())
        },
        get: |s| s.font.clone().unwrap_or_default(),
    },
    Key {
        name: FONT_SIZE_KEY,
        description: "Text size in points, 6–72.",
        set: |s, v| {
            s.font_size = or_default(v, DEFAULT_FONT_SIZE, font_size)?;
            Ok(())
        },
        get: |s| s.font_size.to_string(),
    },
    Key {
        name: TOOLBAR_FONT_SIZE_KEY,
        description: "The toolbar's text size in points, 6–12: its labels, its conflict badge \
                      and its menus.",
        set: |s, v| {
            s.toolbar_font_size = or_default(v, DEFAULT_TOOLBAR_FONT_SIZE, toolbar_font_size)?;
            Ok(())
        },
        get: |s| s.toolbar_font_size.to_string(),
    },
    Key {
        name: WINDOW_SIZE_KEY,
        description: "The size the merge and diff windows open at, WIDTHxHEIGHT in points, \
                      each 400–16384. A window opens no larger than its display.",
        set: |s, v| {
            s.window_size = or_default(v, DEFAULT_WINDOW_SIZE, window_size)?;
            Ok(())
        },
        get: |s| s.window_size.to_string(),
    },
    Key {
        name: "scroll-sync",
        description: "The panes share one scroll position.",
        set: |s, v| {
            s.scroll_sync =
                or_default(v, Settings::default().scroll_sync, |v| boolean("scroll-sync", v))?;
            Ok(())
        },
        get: |s| s.scroll_sync.to_string(),
    },
    Key {
        name: "scrollbars",
        description: "When the panes show their scrollbars: always, hover (while the pointer \
                      is over the pane) or when-scrolling (while the pane scrolls).",
        set: |s, v| {
            s.scrollbars = or_default(v, Scrollbars::default(), |v| named("scrollbars", v))?;
            Ok(())
        },
        get: |s| name(s.scrollbars),
    },
    Key {
        name: "show-base-pane",
        description: "The merge window opens with the base pane showing.",
        set: |s, v| {
            s.base_pane =
                or_default(v, Settings::default().base_pane, |v| boolean("show-base-pane", v))?;
            Ok(())
        },
        get: |s| s.base_pane.to_string(),
    },
    Key {
        name: "show-origin-column",
        description: "The line origin column (View → Blame) shows once a window's origins load.",
        set: |s, v| {
            s.origin_column = or_default(v, Settings::default().origin_column, |v| {
                boolean("show-origin-column", v)
            })?;
            Ok(())
        },
        get: |s| s.origin_column.to_string(),
    },
    Key {
        name: "revision-id",
        description: "The ID that names a jj revision in the pane headings and the origin column: change or commit. git shows commits.",
        set: |s, v| {
            s.revision_id = or_default(v, RevisionId::default(), |v| named("revision-id", v))?;
            Ok(())
        },
        get: |s| name(s.revision_id),
    },
    Key {
        name: "theme",
        description: "The windows' appearance: system follows the system, or light or dark.",
        set: |s, v| {
            s.theme = or_default(v, Theme::default(), |v| named("theme", v))?;
            Ok(())
        },
        get: |s| name(s.theme),
    },
    Key {
        name: "line-height",
        description: "Line height as a multiple of the text size, 1–3.",
        set: |s, v| {
            s.line_height =
                or_default(v, DEFAULT_LINE_HEIGHT, |v| number_in("line-height", v, &LINE_HEIGHTS))?;
            Ok(())
        },
        get: |s| s.line_height.to_string(),
    },
    colour_key!("color-left", left, "The left side's colour"),
    colour_key!("color-right", right, "The right side's colour"),
    colour_key!("color-base", base, "The base's colour"),
    colour_key!("color-conflict", conflict, "The colour of conflicts"),
    colour_key!("color-result-edit", edited, "The colour of the result's edits"),
    colour_key!("color-same-change", both, "The colour of changes every side made"),
    colour_key!("color-word-merged", word_merged, "The colour of conflicts merged word by word"),
    colour_key!(
        "color-current-region",
        current,
        "The current region's bars and frame, and the current search match's blink"
    ),
    colour_key!("color-deleted", deleted, "The colour of deletion notes and marks"),
    colour_key!("color-resolved", resolved, "The conflict badge's colour once no conflict is left"),
    Key {
        name: BAND_OPACITY_KEY,
        description: "The opacity of the bands behind changed rows, 0–1, or light:N,dark:N.",
        set: |s, v| {
            s.colours.band_opacity = or_default(v, DEFAULT_COLOURS.band_opacity, |v| {
                themed(BAND_OPACITY_KEY, v, OPACITY_PAIR, |part| {
                    number_in(BAND_OPACITY_KEY, part, &BAND_OPACITIES)
                })
            })?;
            Ok(())
        },
        get: |s| render_themed(s.colours.band_opacity),
    },
    Key {
        name: "conflict-style",
        description: "The markers of a conflict under git semantics: diff3 keeps the base \
                      section, merge drops it. Empty: git's merge.conflictStyle, else diff3.",
        set: |s, v| {
            s.conflict_style = optional(v, |v| named("conflict-style", v))?;
            Ok(())
        },
        get: |s| s.conflict_style.map(name).unwrap_or_default(),
    },
    Key {
        name: "output-line-ending",
        description: "The line endings of the saved merge: keep each line's, base's dominant \
                      ending, lf or crlf. Markers included; the result pane shows the inputs' \
                      bytes.",
        set: |s, v| {
            s.line_ending = or_default(v, Settings::default().line_ending, |v| {
                named::<LineEndingArg>("output-line-ending", v).map(Into::into)
            })?;
            Ok(())
        },
        get: |s| name(LineEndingArg::from(s.line_ending)),
    },
    Key {
        name: "confirm-unresolved-save",
        description: "Save and Exit asks first while conflicts are unresolved.",
        set: |s, v| {
            s.confirm_unresolved_save =
                or_default(v, Settings::default().confirm_unresolved_save, |v| {
                    boolean("confirm-unresolved-save", v)
                })?;
            Ok(())
        },
        get: |s| s.confirm_unresolved_save.to_string(),
    },
    Key {
        name: "keybind",
        description: "A key and the menu command it runs: cmd+j=next-conflict, cmd+k=unbind to make a key do nothing, or clear to drop every binding so far. Each line adds to the ones before it.",
        set: |s, v| {
            if v.is_empty() {
                s.key_bindings = KeyBindings::default();
                return Ok(());
            }
            let lines = v
                .lines()
                .map(|line| Keybind::parse(line, Platform::CURRENT))
                .collect::<Result<Vec<_>, _>>()?;
            for line in lines {
                s.key_bindings.apply(line);
            }
            Ok(())
        },
        get: |s| s.key_bindings.to_string(),
    },
    Key {
        name: "syntax-highlighting",
        description: "Colour each pane's text by its language's syntax when a window opens.",
        set: |s, v| {
            s.syntax =
                or_default(v, Settings::default().syntax, |v| boolean("syntax-highlighting", v))?;
            Ok(())
        },
        get: |s| s.syntax.to_string(),
    },
    Key {
        name: SYNTAX_WHOLE_FILE_MAX_LINES_KEY,
        description: "The longest file the result and diff editors parse whole for syntax colours, in lines; a longer one parses a chunk at a time.",
        set: |s, v| {
            s.syntax_max_lines = or_default(v, DEFAULT_SYNTAX_MAX_LINES, |v| {
                v.parse().map_err(|_| invalid(SYNTAX_WHOLE_FILE_MAX_LINES_KEY, v, "a whole number"))
            })?;
            Ok(())
        },
        get: |s| s.syntax_max_lines.to_string(),
    },
    Key {
        name: "soft-wrap",
        description: "Long lines wrap in the result, input and diff panes when a window opens; false scrolls them sideways.",
        set: |s, v| {
            s.soft_wrap =
                or_default(v, Settings::default().soft_wrap, |v| boolean("soft-wrap", v))?;
            Ok(())
        },
        get: |s| s.soft_wrap.to_string(),
    },
];

/// `settings` in the file's form, one `key = value` line per key in the order of spec §6.4.
/// With `docs`, each key's description stands above it as a comment, and a blank line
/// separates the keys. The text reads back as `settings`.
#[must_use]
pub fn render(settings: &Settings, docs: bool) -> String {
    let lines = KEYS.iter().map(|key| {
        let value = (key.get)(settings);
        let lines = value
            .split(VALUE_SEPARATOR)
            .map(|value| match quoted(value) {
                value if value.is_empty() => format!("{} =\n", key.name),
                value => format!("{} = {value}\n", key.name),
            })
            .collect::<String>();
        if docs { format!("# {}\n{lines}", key.description) } else { lines }
    });
    let separator = if docs { "\n" } else { "" };
    lines.collect::<Vec<_>>().join(separator)
}

/// The first line of a new config file.
const TEMPLATE_HEAD: &str = "# triad config. Remove the `# ` before a setting to change it.\n\n";

/// The text of a new config file: each key's description and the key at its default, commented
/// out. The text gives the defaults.
#[must_use]
pub fn template() -> String {
    let body = render(&Settings::default(), true)
        .lines()
        .map(|line| match line {
            "" => "\n".to_owned(),
            line if line.starts_with('#') => format!("{line}\n"),
            line => format!("# {line}\n"),
        })
        .collect::<String>();
    format!("{TEMPLATE_HEAD}{body}")
}

/// Write [`template`] to `path`, making its directory, unless the file exists. Gives whether
/// it wrote the file.
///
/// # Errors
///
/// The I/O error when the directory cannot be made or the file cannot be written.
pub fn write_template(path: &Path) -> io::Result<bool> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::File::create_new(path) {
        Ok(mut file) => {
            io::Write::write_all(&mut file, template().as_bytes())?;
            Ok(true)
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

/// The name the file and the command line give `value`.
pub(crate) fn name(value: impl Named) -> String {
    value.name().to_owned()
}

/// `value`, in quotes when the file would otherwise trim or strip it.
fn quoted(value: &str) -> String {
    let trimmed = value.trim() != value;
    let in_quotes = value.len() > 1 && value.starts_with(QUOTE) && value.ends_with(QUOTE);
    if trimmed || in_quotes { format!("{QUOTE}{value}{QUOTE}") } else { value.to_owned() }
}

/// `default` for an empty value, else the value `parse` reads.
fn or_default<T>(
    value: &str,
    default: T,
    parse: impl FnOnce(&str) -> Result<T, Problem>,
) -> Result<T, Problem> {
    if value.is_empty() { Ok(default) } else { parse(value) }
}

/// None for an empty value, else the value `parse` reads.
fn optional<T>(
    value: &str,
    parse: impl FnOnce(&str) -> Result<T, Problem>,
) -> Result<Option<T>, Problem> {
    (!value.is_empty()).then(|| parse(value)).transpose()
}

fn invalid(key: &'static str, value: &str, expected: impl Into<String>) -> Problem {
    Problem::Invalid { key, value: value.to_owned(), expected: expected.into() }
}

/// The variant of `T` that `value` names, as the command line names it.
fn named<T: Named>(key: &'static str, value: &str) -> Result<T, Problem> {
    T::from_name(value).ok_or_else(|| {
        let names: Vec<&str> = T::names().collect();
        invalid(key, value, one_of(&names))
    })
}

/// `a`, `a or b`, `a, b or c`.
fn one_of(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
    }
}

fn boolean(key: &'static str, value: &str) -> Result<bool, Problem> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(invalid(key, value, "true or false")),
    }
}

/// `true` for [`LineEndingRule::Ignore`], `false` for [`LineEndingRule::Compare`].
fn line_ending_rule(key: &'static str, value: &str) -> Result<LineEndingRule, Problem> {
    Ok(if boolean(key, value)? { LineEndingRule::Ignore } else { LineEndingRule::Compare })
}

/// A line-ending rule as its key writes it.
fn ignores(rule: LineEndingRule) -> String {
    (rule == LineEndingRule::Ignore).to_string()
}

/// `true` for [`Refine::Ends`], `false` for [`Refine::Off`], `deep` for [`Refine::Deep`].
fn refine(value: &str) -> Result<Refine, Problem> {
    match value {
        "true" => Ok(Refine::Ends),
        "false" => Ok(Refine::Off),
        "deep" => Ok(Refine::Deep),
        _ => Err(invalid("refine-conflicts", value, "true, false or deep")),
    }
}

fn tab_width(value: &str) -> Result<usize, Problem> {
    let width: usize = value.parse().map_err(|_| invalid("tab-width", value, "a whole number"))?;
    TAB_WIDTHS.contains(&width).then_some(width).ok_or(Problem::TabWidth(width))
}

/// A number in `range`.
fn number_in(key: &'static str, value: &str, range: &RangeInclusive<f32>) -> Result<f32, Problem> {
    let number: f32 = value.parse().map_err(|_| invalid(key, value, "a number"))?;
    range.contains(&number).then_some(number).ok_or(Problem::OutOfRange {
        key,
        value: value.to_owned(),
        start: *range.start(),
        end: *range.end(),
    })
}

/// `#rrggbb`, hex digits in either case.
fn colour(value: &str) -> Option<Colour> {
    let digits = value.strip_prefix(COLOUR_PREFIX)?;
    let hex = digits.len() == COLOUR_DIGITS && digits.bytes().all(|b| b.is_ascii_hexdigit());
    hex.then(|| u32::from_str_radix(digits, HEX_RADIX).ok()).flatten().map(Colour::rgb)
}

/// One value for both themes, which `one` reads; or `light:A,dark:B` in either order, with
/// spaces around `,` and `:` trimmed and each theme named once. A malformed pair is refused
/// as expecting `expected`.
fn themed<T: Copy>(
    key: &'static str,
    value: &str,
    expected: &str,
    one: impl Fn(&str) -> Result<T, Problem>,
) -> Result<Themed<T>, Problem> {
    if !value.contains(THEME_SEPARATOR) {
        return one(value).map(Themed::both);
    }
    let refused = || invalid(key, value, expected);
    let (mut light, mut dark) = (None, None);
    for part in value.split(PAIR_SEPARATOR) {
        let (theme, part) = part.split_once(THEME_SEPARATOR).ok_or_else(refused)?;
        let slot = match theme.trim() {
            LIGHT => &mut light,
            DARK => &mut dark,
            _ => return Err(refused()),
        };
        if slot.replace(one(part.trim())?).is_some() {
            return Err(refused());
        }
    }
    match (light, dark) {
        (Some(light), Some(dark)) => Ok(Themed { light, dark }),
        _ => Err(refused()),
    }
}

/// One value when both themes share it, else `light:A,dark:B`.
fn render_themed<T: Copy + PartialEq + fmt::Display>(value: Themed<T>) -> String {
    if value.light == value.dark {
        value.light.to_string()
    } else {
        format!(
            "{LIGHT}{THEME_SEPARATOR}{}{PAIR_SEPARATOR}{DARK}{THEME_SEPARATOR}{}",
            value.light, value.dark
        )
    }
}

/// `WIDTHxHEIGHT`, each in [`WINDOW_SIZES`].
fn window_size(value: &str) -> Result<WindowSize, Problem> {
    let malformed = || invalid(WINDOW_SIZE_KEY, value, SIZE_FORM);
    let (width, height) = value.split_once(SIZE_SEPARATOR).ok_or_else(malformed)?;
    let side = |side: &str| side.parse::<u32>().map_err(|_| malformed());
    let (width, height) = (side(width)?, side(height)?);
    let in_range = |side: u32| u16::try_from(side).ok().filter(|side| WINDOW_SIZES.contains(side));
    in_range(width)
        .zip(in_range(height))
        .map(|(width, height)| WindowSize { width, height })
        .ok_or_else(|| Problem::OutOfRange {
            key: WINDOW_SIZE_KEY,
            value: value.to_owned(),
            start: f32::from(*WINDOW_SIZES.start()),
            end: f32::from(*WINDOW_SIZES.end()),
        })
}

fn toolbar_font_size(value: &str) -> Result<f32, Problem> {
    let size: f32 = value.parse().map_err(|_| invalid(TOOLBAR_FONT_SIZE_KEY, value, "a number"))?;
    TOOLBAR_FONT_SIZES.contains(&size).then_some(size).ok_or(Problem::ToolbarFontSize(size))
}

fn font_size(value: &str) -> Result<f32, Problem> {
    let size: f32 = value.parse().map_err(|_| invalid("font-size", value, "a number"))?;
    FONT_SIZES.contains(&size).then_some(size).ok_or(Problem::FontSize(size))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[cfg(unix)]
    const MAIN: &str = "/c/config.triad";
    #[cfg(windows)]
    const MAIN: &str = r"C:\c\config.triad";

    /// The settings and errors of a config file holding `text`, with no includes.
    fn read(text: &str) -> Loaded {
        let files = HashMap::from([(PathBuf::from(MAIN), text.to_owned())]);
        load_with(Path::new(MAIN), &files)
    }

    /// The settings of `text`, which has no errors.
    fn settings(text: &str) -> Settings {
        let loaded = read(text);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        loaded.settings
    }

    /// The one problem in `text`, and the line it is on.
    fn problem(text: &str) -> (usize, Problem) {
        let mut errors = read(text).errors;
        assert_eq!(errors.len(), 1, "{errors:?}");
        match errors.remove(0) {
            SettingsError::Value { line, problem, path, .. } => {
                assert_eq!(&*path, Path::new(MAIN));
                (line, problem)
            }
            other => panic!("not a value error: {other:?}"),
        }
    }

    fn invalid(key: &'static str, value: &str, expected: &str) -> Problem {
        Problem::Invalid { key, value: value.to_owned(), expected: expected.to_owned() }
    }

    #[test]
    fn an_empty_file_gives_the_defaults() {
        assert_eq!(
            settings(""),
            Settings {
                semantics: Choice::Auto,
                same_change: None,
                hunk_level: None,
                diff: DiffOptions {
                    algorithm: diff::Algorithm::Histogram,
                    whitespace: diff::Whitespace::None,
                    line_endings: LineEndingRule::Compare,
                },
                diff_line_endings: LineEndingRule::Ignore,
                refine: Refine::Ends,
                syntax_boundaries: false,
                tab_width: 4,
                font: None,
                font_size: 10.,
                toolbar_font_size: 8.4,
                window_size: WindowSize { width: 1400, height: 900 },
                scroll_sync: true,
                scrollbars: Scrollbars::Always,
                base_pane: false,
                origin_column: false,
                revision_id: RevisionId::Change,
                theme: Theme::System,
                line_height: 1.5,
                colours: DEFAULT_COLOURS,
                conflict_style: None,
                line_ending: OutputEnding::Keep,
                confirm_unresolved_save: true,
                syntax: true,
                syntax_max_lines: 20_000,
                soft_wrap: false,
                key_bindings: KeyBindings::default(),
            }
        );
    }

    #[test]
    fn every_setting_reads() {
        let text = "
            merge-semantics = jj
            same-change = keep
            hunk-level = word
            diff-algorithm = myers-minimal
            ignore-whitespace = amount
            merge-ignore-line-endings = true
            diff-ignore-line-endings = false
            refine-conflicts = false
            syntax-boundaries = true
            tab-width = 8
            font = Iosevka Term
            font-size = 13.5
            toolbar-font-size = 10.5
            window-size = 1024x768
            scroll-sync = false
            scrollbars = hover
            show-base-pane = true
            show-origin-column = true
            revision-id = commit
            theme = light
            line-height = 2
            color-left = #010203
            color-right = #040506
            color-base = #070809
            color-conflict = #0a0b0c
            color-result-edit = #0d0e0f
            color-same-change = #101112
            color-word-merged = #131415
            color-current-region = #161718
            color-deleted = #191a1b
            color-resolved = #1c1d1e
            band-opacity = 0.5
            conflict-style = merge
            output-line-ending = crlf
            confirm-unresolved-save = false
            syntax-highlighting = false
            syntax-whole-file-max-lines = 0
            soft-wrap = true
        ";
        assert_eq!(
            settings(text),
            Settings {
                semantics: Choice::Jj,
                same_change: Some(SameChange::Keep),
                hunk_level: Some(HunkLevel::Word),
                diff: DiffOptions {
                    algorithm: diff::Algorithm::MyersMinimal,
                    whitespace: diff::Whitespace::Amount,
                    line_endings: LineEndingRule::Ignore,
                },
                diff_line_endings: LineEndingRule::Compare,
                refine: Refine::Off,
                syntax_boundaries: true,
                tab_width: 8,
                font: Some("Iosevka Term".to_owned()),
                font_size: 13.5,
                toolbar_font_size: 10.5,
                window_size: WindowSize { width: 1024, height: 768 },
                scroll_sync: false,
                scrollbars: Scrollbars::Hover,
                base_pane: true,
                origin_column: true,
                revision_id: RevisionId::Commit,
                theme: Theme::Light,
                line_height: 2.,
                colours: Colours {
                    left: Themed::both(Colour::rgb(0x01_02_03)),
                    right: Themed::both(Colour::rgb(0x04_05_06)),
                    base: Themed::both(Colour::rgb(0x07_08_09)),
                    conflict: Themed::both(Colour::rgb(0x0a_0b_0c)),
                    edited: Themed::both(Colour::rgb(0x0d_0e_0f)),
                    both: Themed::both(Colour::rgb(0x10_11_12)),
                    word_merged: Themed::both(Colour::rgb(0x13_14_15)),
                    current: Themed::both(Colour::rgb(0x16_17_18)),
                    deleted: Themed::both(Colour::rgb(0x19_1a_1b)),
                    resolved: Themed::both(Colour::rgb(0x1c_1d_1e)),
                    band_opacity: Themed::both(0.5),
                },
                conflict_style: Some(ConflictStyle::Merge),
                line_ending: OutputEnding::CrLf,
                confirm_unresolved_save: false,
                syntax: false,
                syntax_max_lines: 0,
                soft_wrap: true,
                key_bindings: KeyBindings::default(),
            }
        );
    }

    #[test]
    fn an_empty_value_restores_the_default() {
        let text = "
            merge-semantics = git
            same-change = keep
            hunk-level = word
            diff-algorithm = myers
            ignore-whitespace = all
            refine-conflicts = deep
            tab-width = 2
            font = Menlo
            font-size = 12
            scroll-sync = false
            scrollbars = hover
            show-base-pane = true
            show-origin-column = true
            theme = dark
            line-height = 2
            color-left = #000000
            color-resolved = light:#000000,dark:#ffffff
            band-opacity = 1
            conflict-style = diff3
            output-line-ending = lf
            confirm-unresolved-save = false
            syntax-highlighting = false
            syntax-whole-file-max-lines = 5
            merge-semantics =
            same-change =
            hunk-level =
            diff-algorithm =
            ignore-whitespace =
            refine-conflicts =
            tab-width =
            font =
            font-size =
            scroll-sync =
            scrollbars =
            show-base-pane =
            show-origin-column =
            theme =
            line-height =
            color-left =
            color-resolved =
            band-opacity =
            conflict-style =
            output-line-ending =
            confirm-unresolved-save =
            syntax-highlighting =
            syntax-whole-file-max-lines =
        ";
        assert_eq!(settings(text), Settings::default());
    }

    #[test]
    fn a_later_line_wins() {
        assert_eq!(settings("font-size = 12\nfont-size = 14").font_size, 14.);
    }

    #[test]
    fn syntax_boundaries_read_true_or_false() {
        assert!(settings("syntax-boundaries = true").syntax_boundaries);
        assert!(!settings("syntax-boundaries = false").syntax_boundaries);
        assert!(!settings("").syntax_boundaries, "off by default");
        assert_eq!(
            problem("syntax-boundaries = yes"),
            (1, invalid("syntax-boundaries", "yes", "true or false"))
        );
    }

    #[test]
    fn refinement_reads_true_false_and_deep() {
        assert_eq!(settings("refine-conflicts = true").refine, Refine::Ends);
        assert_eq!(settings("refine-conflicts = false").refine, Refine::Off);
        assert_eq!(settings("refine-conflicts = deep").refine, Refine::Deep);
        assert_eq!(
            problem("refine-conflicts = shallow"),
            (1, invalid("refine-conflicts", "shallow", "true, false or deep"))
        );
    }

    #[test]
    fn a_quoted_font_keeps_its_spaces() {
        assert_eq!(settings(r#"font = "  Mono  ""#).font.as_deref(), Some("  Mono  "));
    }

    #[test]
    fn an_unknown_key_is_refused_by_name_and_the_key_it_meant_keeps_its_value() {
        let loaded = read("scroll-sync = false\nscrol-sync = true");
        assert!(!loaded.settings.scroll_sync);
        assert_eq!(
            problem("scrol-sync = false"),
            (1, Problem::UnknownKey("scrol-sync".to_owned()))
        );
        assert_eq!(problem("Font-Size = 12"), (1, Problem::UnknownKey("Font-Size".to_owned())));
    }

    #[test]
    fn the_current_region_colour_is_read_under_its_full_name_alone() {
        let loaded = read("color-current-region = #102030");
        assert_eq!(loaded.settings.colours.current, Themed::both(Colour::rgb(0x10_20_30)));
        assert_eq!(
            problem("color-current = #102030"),
            (1, Problem::UnknownKey("color-current".to_owned()))
        );
    }

    /// Key names given up for clearer ones, without aliases.
    const RETIRED_KEYS: &[&str] = &[
        "algorithm",
        "whitespace",
        "ignore-line-endings",
        "refine",
        "syntax",
        "syntax-max-lines",
        "line-ending",
        "base-pane",
        "origin-column",
        "color-both",
        "color-edited",
        "semantics",
    ];

    #[test]
    fn a_retired_key_name_is_unknown_in_the_file() {
        for old in RETIRED_KEYS {
            assert_eq!(
                problem(&format!("{old} = x")),
                (1, Problem::UnknownKey((*old).to_owned())),
                "{old}"
            );
        }
    }

    #[test]
    fn a_bad_line_keeps_the_value_before_it_and_the_good_lines_apply() {
        let loaded = read("tab-width = 8\ntab-width = 0\nshow-base-pane = true");
        assert_eq!(loaded.settings.tab_width, 8);
        assert!(loaded.settings.base_pane);
        assert_eq!(
            problem("tab-width = 8\ntab-width = 0\nshow-base-pane = true"),
            (2, Problem::TabWidth(0))
        );
    }

    #[test]
    fn unknown_names_are_refused_with_the_names_allowed() {
        assert_eq!(
            problem("diff-algorithm = patience"),
            (1, invalid("diff-algorithm", "patience", "histogram, myers or myers-minimal"))
        );
        assert_eq!(
            problem("ignore-whitespace = some"),
            (1, invalid("ignore-whitespace", "some", "none, trailing, amount or all"))
        );
        assert_eq!(
            problem("merge-semantics = hg"),
            (1, invalid("merge-semantics", "hg", "auto, git or jj"))
        );
        assert_eq!(
            problem("same-change = maybe"),
            (1, invalid("same-change", "maybe", "accept or keep"))
        );
        assert_eq!(
            problem("hunk-level = char"),
            (1, invalid("hunk-level", "char", "line or word"))
        );
    }

    #[test]
    fn names_are_case_sensitive() {
        assert_eq!(
            problem("diff-algorithm = Myers"),
            (1, invalid("diff-algorithm", "Myers", "histogram, myers or myers-minimal"))
        );
    }

    #[test]
    fn a_revision_id_is_change_or_commit() {
        assert_eq!(settings("revision-id = commit").revision_id, RevisionId::Commit);
        assert_eq!(settings("revision-id = change").revision_id, RevisionId::Change);
        assert_eq!(
            settings("revision-id = commit\nrevision-id =").revision_id,
            RevisionId::Change,
            "an empty value gives the default"
        );
        assert_eq!(
            problem("revision-id = hash"),
            (1, invalid("revision-id", "hash", "change or commit"))
        );
    }

    #[test]
    fn booleans_are_true_or_false_only() {
        for value in ["yes", "1", "True"] {
            assert_eq!(
                problem(&format!("show-origin-column = {value}")),
                (1, invalid("show-origin-column", value, "true or false")),
                "{value}"
            );
        }
        assert!(settings(r#"show-origin-column = "true""#).origin_column, "quotes drop");
    }

    #[test]
    fn syntax_max_lines_reads_a_whole_number() {
        assert_eq!(settings("syntax-whole-file-max-lines = 0").syntax_max_lines, 0);
        assert_eq!(settings("syntax-whole-file-max-lines = 1000000").syntax_max_lines, 1_000_000);
        for value in ["-1", "1.5", "many"] {
            assert_eq!(
                problem(&format!("syntax-whole-file-max-lines = {value}")),
                (1, invalid("syntax-whole-file-max-lines", value, "a whole number")),
                "{value}"
            );
        }
    }

    #[test]
    fn merges_compare_line_endings_and_the_diff_window_ignores_them_by_default() {
        let defaults = Settings::default();
        assert_eq!(defaults.diff.line_endings, LineEndingRule::Compare);
        assert_eq!(defaults.two_way().line_endings, LineEndingRule::Ignore);
        assert_eq!(settings(""), defaults);
    }

    #[test]
    fn each_line_ending_key_sets_its_own_rule() {
        let both = settings("merge-ignore-line-endings = true\ndiff-ignore-line-endings = false");
        assert_eq!(both.diff.line_endings, LineEndingRule::Ignore);
        assert_eq!(both.two_way().line_endings, LineEndingRule::Compare);
        let reset = settings("merge-ignore-line-endings = true\nmerge-ignore-line-endings =");
        assert_eq!(reset.diff.line_endings, LineEndingRule::Compare);
        let reset = settings("diff-ignore-line-endings = false\ndiff-ignore-line-endings =");
        assert_eq!(reset.two_way().line_endings, LineEndingRule::Ignore);
        assert_eq!(
            problem("merge-ignore-line-endings = yes"),
            (1, invalid("merge-ignore-line-endings", "yes", "true or false"))
        );
        assert_eq!(
            problem("diff-ignore-line-endings = 1"),
            (1, invalid("diff-ignore-line-endings", "1", "true or false"))
        );
    }

    #[test]
    fn the_two_way_options_keep_the_merges_algorithm_and_whitespace() {
        let file = settings(
            "diff-algorithm = myers\nignore-whitespace = all\nmerge-ignore-line-endings = true",
        );
        assert_eq!(
            file.two_way(),
            DiffOptions {
                algorithm: diff::Algorithm::Myers,
                whitespace: diff::Whitespace::All,
                line_endings: LineEndingRule::Ignore,
            }
        );
        let file = settings("merge-ignore-line-endings = true\ndiff-ignore-line-endings = false");
        assert_eq!(file.two_way().line_endings, LineEndingRule::Compare);
    }

    #[test]
    fn scrollbars_read_each_name_and_show_always_by_default() {
        assert_eq!(Settings::default().scrollbars, Scrollbars::Always);
        assert_eq!(settings("scrollbars = always").scrollbars, Scrollbars::Always);
        assert_eq!(settings("scrollbars = hover").scrollbars, Scrollbars::Hover);
        assert_eq!(settings("scrollbars = when-scrolling").scrollbars, Scrollbars::WhenScrolling);
        assert_eq!(settings("scrollbars = hover\nscrollbars =").scrollbars, Scrollbars::Always);
        assert_eq!(
            problem("scrollbars = never"),
            (1, invalid("scrollbars", "never", "always, hover or when-scrolling"))
        );
        assert_eq!(
            problem("scrollbars = Hover"),
            (1, invalid("scrollbars", "Hover", "always, hover or when-scrolling"))
        );
    }

    #[test]
    fn soft_wrap_reads_a_boolean_and_is_off_by_default() {
        assert!(!Settings::default().soft_wrap);
        assert!(settings("soft-wrap = true").soft_wrap);
        assert!(!settings("soft-wrap = true\nsoft-wrap =").soft_wrap);
        assert_eq!(problem("soft-wrap = yes"), (1, invalid("soft-wrap", "yes", "true or false")));
    }

    #[test]
    fn syntax_reads_a_boolean() {
        assert!(!settings("syntax-highlighting = false").syntax);
        assert!(settings("syntax-highlighting = false\nsyntax-highlighting = true").syntax);
        assert_eq!(
            problem("syntax-highlighting = off"),
            (1, invalid("syntax-highlighting", "off", "true or false"))
        );
    }

    #[test]
    fn numbers_that_do_not_read_are_refused() {
        assert_eq!(problem("tab-width = -1"), (1, invalid("tab-width", "-1", "a whole number")));
        assert_eq!(problem("tab-width = 4.5"), (1, invalid("tab-width", "4.5", "a whole number")));
        assert_eq!(problem("font-size = big"), (1, invalid("font-size", "big", "a number")));
    }

    #[test]
    fn the_toolbar_font_size_is_read_and_refused_outside_6_to_12() {
        assert_eq!(settings("toolbar-font-size = 11").toolbar_font_size, 11.);
        let ends = settings("toolbar-font-size = 6");
        assert_eq!(ends.toolbar_font_size, 6.);
        let ends = settings("toolbar-font-size = 12");
        assert_eq!(ends.toolbar_font_size, 12.);
        let refused = |text| match problem(text) {
            (_, Problem::ToolbarFontSize(size)) => size,
            other => panic!("{other:?}"),
        };
        assert_eq!(refused("toolbar-font-size = 5.9").to_bits(), 5.9_f32.to_bits());
        assert_eq!(refused("toolbar-font-size = 12.1").to_bits(), 12.1_f32.to_bits());
        assert!(refused("toolbar-font-size = nan").is_nan());
        assert_eq!(
            problem("toolbar-font-size = big"),
            (1, invalid("toolbar-font-size", "big", "a number"))
        );
        assert_eq!(
            Problem::ToolbarFontSize(13.).to_string(),
            "toolbar-font-size 13 is outside 6–12"
        );
        // An empty value gives the default; a refused one keeps the value before it.
        assert_eq!(settings("toolbar-font-size = 11\ntoolbar-font-size =").toolbar_font_size, 8.4);
        let kept = read("toolbar-font-size = 11\ntoolbar-font-size = 20");
        assert_eq!(kept.errors.len(), 1);
        assert_eq!(kept.settings.toolbar_font_size, 11.);
    }

    #[test]
    fn the_window_size_reads_width_by_height_in_points() {
        assert_eq!(Settings::default().window_size, WindowSize { width: 1400, height: 900 });
        assert_eq!(
            settings("window-size = 1280x800").window_size,
            WindowSize { width: 1280, height: 800 }
        );
        assert_eq!(
            settings("window-size = 400x16384").window_size,
            WindowSize { width: 400, height: 16_384 },
            "both ends of the range"
        );
        assert_eq!(
            settings("window-size = 1280x800\nwindow-size =").window_size,
            WindowSize { width: 1400, height: 900 },
            "an empty value gives the default"
        );
        for value in
            ["1400", "x900", "1400x", "1400x900x2", "-1x900", "1400.5x900", "big", "1400X900"]
        {
            assert_eq!(
                problem(&format!("window-size = {value}")),
                (1, invalid("window-size", value, "WIDTHxHEIGHT, in points")),
                "{value}"
            );
        }
        for value in ["399x900", "1400x399", "16385x900", "1400x16385", "0x0"] {
            assert_eq!(
                problem(&format!("window-size = {value}")),
                (
                    1,
                    Problem::OutOfRange {
                        key: "window-size",
                        value: value.to_owned(),
                        start: 400.,
                        end: 16_384.,
                    }
                ),
                "{value}"
            );
        }
        let kept = read("window-size = 1280x800\nwindow-size = 10x10");
        assert_eq!(kept.errors.len(), 1);
        assert_eq!(
            kept.settings.window_size,
            WindowSize { width: 1280, height: 800 },
            "a refused value keeps the one before it"
        );
    }

    #[test]
    fn sizes_and_widths_out_of_range_are_refused_at_both_ends() {
        let font_size = |text| match problem(text) {
            (_, Problem::FontSize(size)) => size,
            other => panic!("{other:?}"),
        };
        assert_eq!(font_size("font-size = 5.9").to_bits(), 5.9_f32.to_bits());
        assert_eq!(font_size("font-size = 72.1").to_bits(), 72.1_f32.to_bits());
        assert_eq!(font_size("font-size = 5").to_bits(), 5_f32.to_bits());
        assert_eq!(font_size("font-size = 73").to_bits(), 73_f32.to_bits());
        assert!(font_size("font-size = nan").is_nan());
        assert_eq!(font_size("font-size = inf"), f32::INFINITY);
        assert_eq!(problem("tab-width = 0"), (1, Problem::TabWidth(0)));
        assert_eq!(problem("tab-width = 17"), (1, Problem::TabWidth(17)));
        let ends = settings("font-size = 6\ntab-width = 16");
        assert_eq!((ends.font_size, ends.tab_width), (6., 16));
        let ends = settings("font-size = 72\ntab-width = 1");
        assert_eq!((ends.font_size, ends.tab_width), (72., 1));
    }

    #[test]
    fn themes_read_by_name_and_others_are_refused() {
        assert_eq!(settings("theme = light").theme, Theme::Light);
        assert_eq!(settings("theme = dark").theme, Theme::Dark);
        assert_eq!(settings("theme = dark\ntheme =").theme, Theme::System);
        assert_eq!(
            problem("theme = sepia"),
            (1, invalid("theme", "sepia", "system, light or dark"))
        );
    }

    #[test]
    fn line_heights_read_from_1_to_3() {
        assert_eq!(settings("line-height = 1").line_height, 1.);
        assert_eq!(settings("line-height = 3").line_height, 3.);
        assert_eq!(settings("line-height = 1.75").line_height, 1.75);
        for value in ["0.9", "3.1", "nan"] {
            let text = format!("line-height = {value}");
            assert_eq!(
                read(&text).errors[0].to_string(),
                format!("{MAIN}:1: line-height {value} is outside 1–3")
            );
        }
        assert_eq!(problem("line-height = tall"), (1, invalid("line-height", "tall", "a number")));
    }

    #[test]
    fn line_endings_read_by_name_and_others_are_refused() {
        let expected = [
            ("keep", OutputEnding::Keep),
            ("base", OutputEnding::Base),
            ("lf", OutputEnding::Lf),
            ("crlf", OutputEnding::CrLf),
        ];
        for (value, ending) in expected {
            assert_eq!(settings(&format!("output-line-ending = {value}")).line_ending, ending);
        }
        assert_eq!(
            settings("output-line-ending = lf\noutput-line-ending =").line_ending,
            OutputEnding::Keep
        );
        for value in ["cr", "CRLF", "native"] {
            assert_eq!(
                problem(&format!("output-line-ending = {value}")),
                (1, invalid("output-line-ending", value, "keep, base, lf or crlf"))
            );
        }
    }

    #[test]
    fn conflict_styles_read_by_name_and_zdiff3_is_refused() {
        assert_eq!(settings("conflict-style = merge").conflict_style, Some(ConflictStyle::Merge));
        assert_eq!(settings("conflict-style = diff3").conflict_style, Some(ConflictStyle::Diff3));
        assert_eq!(settings("conflict-style = merge\nconflict-style =").conflict_style, None);
        for value in ["zdiff3", "Merge"] {
            assert_eq!(
                problem(&format!("conflict-style = {value}")),
                (1, invalid("conflict-style", value, "diff3 or merge")),
                "{value}"
            );
        }
    }

    /// What a colour key's refusal expects.
    const COLOUR: &str = "#rrggbb or light:#rrggbb,dark:#rrggbb";

    #[test]
    fn a_colour_reads_in_either_case_for_both_themes() {
        let left = Themed::both(Colour { r: 0x22, g: 0x73, b: 0xc3 });
        assert_eq!(settings("color-left = #2273C3").colours.left, left);
        assert_eq!(settings("color-left = #2273c3").colours.left, left);
    }

    #[test]
    fn a_pair_gives_each_theme_its_colour_in_either_order() {
        let pair = Themed { light: Colour::rgb(0x00_00_00), dark: Colour::rgb(0xff_ff_ff) };
        for value in [
            "light:#000000,dark:#ffffff",
            "dark:#ffffff, light:#000000",
            "light : #000000 , dark : #FFFFFF",
        ] {
            let text = format!("color-conflict = {value}");
            assert_eq!(settings(&text).colours.conflict, pair, "{value}");
        }
    }

    #[test]
    fn bad_colours_are_refused_naming_the_line() {
        for value in [
            "#2273c",
            "#2273c3f",
            "2273c3",
            "#gg0000",
            "#+22733",
            "#2273c3 ",
            "#é2273",
            "light:#000000",
            "dark:#000000",
            "light:#000000,light:#ffffff",
            "dim:#000000,dark:#ffffff",
            "light:#000000,dark:#ffffff,",
            "light:#000000,dark:#ffffff,dark:#ffffff",
            "light:#000000,dark:ffffff",
            "light:,dark:#ffffff",
            "light#000000,dark:#ffffff",
        ] {
            let text = format!("tab-width = 2\ncolor-left = \"{value}\"");
            assert_eq!(problem(&text), (2, invalid("color-left", value, COLOUR)), "{value}");
            assert_eq!(read(&text).settings.colours.left, DEFAULT_COLOURS.left);
        }
    }

    #[test]
    fn every_colour_key_sets_its_own_colour() {
        let colours = settings(
            "color-right = #000001\ncolor-base = #000002\ncolor-result-edit = #000003\n\
             color-same-change = #000004\ncolor-word-merged = #000005\ncolor-current-region = #000006\n\
             color-deleted = #000007",
        )
        .colours;
        let got = [
            colours.right,
            colours.base,
            colours.edited,
            colours.both,
            colours.word_merged,
            colours.current,
            colours.deleted,
        ];
        let want = (1..=7).map(|n| Themed::both(Colour::rgb(n)));
        assert!(got.into_iter().eq(want), "{got:?}");
        assert_eq!(colours.left, DEFAULT_COLOURS.left);
        assert_eq!(colours.conflict, DEFAULT_COLOURS.conflict);
        assert_eq!(colours.resolved, DEFAULT_COLOURS.resolved);
    }

    #[test]
    fn band_opacity_reads_from_0_to_1_alone_or_as_a_pair() {
        assert_eq!(settings("band-opacity = 0").colours.band_opacity, Themed::both(0.));
        assert_eq!(settings("band-opacity = 1").colours.band_opacity, Themed::both(1.));
        assert_eq!(
            settings("band-opacity = dark:0.5, light:0.25").colours.band_opacity,
            Themed { light: 0.25, dark: 0.5 }
        );
        for (value, shown) in [("-0.1", "-0.1"), ("1.1", "1.1"), ("light:0.2,dark:1.5", "1.5")] {
            let text = format!("band-opacity = {value}");
            assert_eq!(
                read(&text).errors[0].to_string(),
                format!("{MAIN}:1: band-opacity {shown} is outside 0–1"),
                "{value}"
            );
        }
        assert_eq!(problem("band-opacity = dim"), (1, invalid("band-opacity", "dim", "a number")));
        assert_eq!(
            problem("band-opacity = light:0.2"),
            (1, invalid("band-opacity", "light:0.2", "light:N,dark:N"))
        );
    }

    /// Spec §6.4: the tones' hues as `#rrggbb`, light then dark.
    #[test]
    fn the_default_colours_are_the_spec_values() {
        let pair = |light, dark| Themed { light: Colour::rgb(light), dark: Colour::rgb(dark) };
        assert_eq!(
            DEFAULT_COLOURS,
            Colours {
                left: pair(0x22_73_c3, 0x67_a6_e4),
                right: pair(0xc3_22_73, 0xe4_67_a6),
                base: pair(0xc3_b6_22, 0xe4_da_67),
                conflict: pair(0xc3_65_22, 0xe4_9b_67),
                edited: pair(0x80_22_c3, 0xb0_67_e4),
                both: pair(0x22_c3_c3, 0x67_e4_e4),
                word_merged: pair(0x22_c3_22, 0x67_e4_67),
                current: pair(0x6c_bd_0f, 0x92_e2_36),
                deleted: pair(0xc3_22_22, 0xe4_67_67),
                resolved: pair(0x22_c3_58, 0x67_e4_91),
                band_opacity: Themed { light: 0.18, dark: 0.28 },
            }
        );
    }

    #[test]
    fn a_value_error_names_the_file_line_and_range() {
        let errors = read("\nfont-size = 100").errors;
        assert_eq!(
            errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec![format!("{MAIN}:2: font-size 100 is outside 6–72")]
        );
        let errors = read("tab-width = 20").errors;
        assert_eq!(errors[0].to_string(), format!("{MAIN}:1: tab-width 20 is outside 1–16"));
        let errors = read("diff-algorithm = patience").errors;
        assert_eq!(
            errors[0].to_string(),
            format!(
                "{MAIN}:1: diff-algorithm = patience: expected histogram, myers or myers-minimal"
            )
        );
    }

    #[test]
    fn format_errors_come_before_value_errors_and_both_are_kept() {
        let loaded = read("tab-width = 0\nnonsense");
        let text: Vec<_> = loaded.errors.iter().map(ToString::to_string).collect();
        assert_eq!(
            text,
            vec![
                format!("{MAIN}:2: expected `key = value`"),
                format!("{MAIN}:1: tab-width 0 is outside 1–16"),
            ]
        );
    }

    #[test]
    fn included_settings_apply_in_place() {
        let local = Path::new(MAIN).with_file_name("local.conf");
        let files = HashMap::from([
            (
                PathBuf::from(MAIN),
                "font-size = 11\nconfig-file = local.conf\nshow-base-pane = false".to_owned(),
            ),
            (local.clone(), "font-size = 14\nshow-base-pane = true".to_owned()),
        ]);
        let loaded = load_with(Path::new(MAIN), &files);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert_eq!(loaded.settings.font_size, 14.);
        assert!(!loaded.settings.base_pane);
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        let loaded = load(&path);
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    }

    #[test]
    fn a_file_on_disk_reads() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        std::fs::write(&path, "show-base-pane = true\n").expect("written");
        let loaded = load(&path);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert!(loaded.settings.base_pane);
    }

    #[test]
    fn a_directory_in_place_of_the_file_cannot_be_read_and_gives_the_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let loaded = load(dir.path());
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.errors.len(), 1, "{:?}", loaded.errors);
        let SettingsError::Read { path, .. } = &loaded.errors[0] else {
            panic!("{:?}", loaded.errors);
        };
        assert_eq!(path, dir.path());
    }

    /// A root that makes a path absolute on this platform.
    const ROOT: &str = if cfg!(windows) { r"C:\" } else { "/" };

    fn absolute(relative: &str) -> PathBuf {
        Path::new(ROOT).join(relative)
    }

    fn locations() -> Locations {
        Locations {
            config: None,
            xdg_config_home: Some(absolute("xdg")),
            home: Some(absolute("home").join("u")),
            app_data: Some(PathBuf::from(r"C:\Users\u\AppData\Roaming")),
        }
    }

    #[test]
    fn triad_config_names_the_file_on_every_platform() {
        let env = Locations { config: Some(PathBuf::from("/elsewhere/mine.triad")), ..locations() };
        assert_eq!(env.unix_path(), Some(PathBuf::from("/elsewhere/mine.triad")));
        assert_eq!(env.windows_path(), Some(PathBuf::from("/elsewhere/mine.triad")));
    }

    #[test]
    fn unix_takes_xdg_config_home_else_dot_config() {
        assert_eq!(
            locations().unix_path(),
            Some(absolute("xdg").join("triad").join("config.triad"))
        );
        let without = Locations { xdg_config_home: None, ..locations() };
        assert_eq!(
            without.unix_path(),
            Some(absolute("home").join("u").join(".config").join("triad").join("config.triad"))
        );
        let neither = Locations { xdg_config_home: None, home: None, ..locations() };
        assert_eq!(neither.unix_path(), None);
    }

    #[test]
    fn unix_ignores_a_relative_xdg_config_home() {
        let relative = Locations { xdg_config_home: Some(PathBuf::from("xdg")), ..locations() };
        assert_eq!(
            relative.unix_path(),
            Some(absolute("home").join("u").join(".config").join("triad").join("config.triad"))
        );
    }

    #[test]
    fn windows_takes_app_data() {
        assert_eq!(
            locations().windows_path(),
            Some(Path::new(r"C:\Users\u\AppData\Roaming").join("triad").join("config.triad"))
        );
        let without = Locations { app_data: None, ..locations() };
        assert_eq!(without.windows_path(), None);
    }

    const DEFAULTS: &str = "\
merge-semantics = auto
same-change =
hunk-level =
diff-algorithm = histogram
ignore-whitespace = none
merge-ignore-line-endings = false
diff-ignore-line-endings = true
refine-conflicts = true
syntax-boundaries = false
tab-width = 4
font =
font-size = 10
toolbar-font-size = 8.4
window-size = 1400x900
scroll-sync = true
scrollbars = always
show-base-pane = false
show-origin-column = false
revision-id = change
theme = system
line-height = 1.5
color-left = light:#2273c3,dark:#67a6e4
color-right = light:#c32273,dark:#e467a6
color-base = light:#c3b622,dark:#e4da67
color-conflict = light:#c36522,dark:#e49b67
color-result-edit = light:#8022c3,dark:#b067e4
color-same-change = light:#22c3c3,dark:#67e4e4
color-word-merged = light:#22c322,dark:#67e467
color-current-region = light:#6cbd0f,dark:#92e236
color-deleted = light:#c32222,dark:#e46767
color-resolved = light:#22c358,dark:#67e491
band-opacity = light:0.18,dark:0.28
conflict-style =
output-line-ending = keep
confirm-unresolved-save = true
keybind = clear
keybind = cmd+z=undo
keybind = cmd+shift+z=redo
keybind = cmd+down=next-difference
keybind = cmd+up=previous-difference
keybind = cmd+alt+down=next-conflict
keybind = cmd+alt+up=previous-conflict
keybind = cmd+1=toggle-left
keybind = cmd+2=toggle-base
keybind = cmd+3=toggle-right
keybind = cmd+0=clear-choice
keybind = cmd+4=take-left-then-right
keybind = cmd+5=take-right-then-left
keybind = cmd+b=toggle-base-pane
keybind = cmd+f=find
keybind = cmd+l=go-to-line
keybind = cmd+s=save-and-exit
keybind = cmd+s=save
keybind = cmd+shift+s=save-copy-as
keybind = cmd+q=cancel
keybind = cmd+w=cancel
keybind = cmd+,=open-settings
syntax-highlighting = true
syntax-whole-file-max-lines = 20000
soft-wrap = false
";

    /// [`DEFAULTS`] on this platform: its chords are macOS's.
    fn defaults() -> String {
        if cfg!(target_os = "macos") {
            DEFAULTS.to_owned()
        } else {
            DEFAULTS.replace("cmd+", "ctrl+")
        }
    }

    /// `ctrl+j=next-conflict`, which reads on every platform.
    fn added_binding() -> KeyBindings {
        let mut bindings = KeyBindings::default();
        bindings.apply(Keybind::parse("ctrl+j=next-conflict", Platform::CURRENT).unwrap());
        bindings
    }

    /// A value off its default for every key.
    fn every_key_changed() -> Settings {
        Settings {
            semantics: Choice::Jj,
            same_change: Some(SameChange::Keep),
            hunk_level: Some(HunkLevel::Word),
            diff: DiffOptions {
                algorithm: diff::Algorithm::MyersMinimal,
                whitespace: diff::Whitespace::Trailing,
                line_endings: LineEndingRule::Ignore,
            },
            diff_line_endings: LineEndingRule::Compare,
            refine: Refine::Deep,
            syntax_boundaries: true,
            tab_width: 2,
            font: Some("Iosevka Term".to_owned()),
            font_size: 12.5,
            toolbar_font_size: 9.5,
            window_size: WindowSize { width: 1600, height: 1000 },
            scroll_sync: false,
            scrollbars: Scrollbars::WhenScrolling,
            base_pane: true,
            origin_column: true,
            revision_id: RevisionId::Commit,
            theme: Theme::Dark,
            line_height: 2.25,
            colours: Colours {
                left: Themed::both(Colour::rgb(0xab_cd_ef)),
                right: Themed { light: Colour::rgb(0x01_02_03), dark: Colour::rgb(0xfe_fd_fc) },
                base: Themed::both(Colour::rgb(0x00_00_01)),
                conflict: Themed::both(Colour::rgb(0x00_00_02)),
                edited: Themed::both(Colour::rgb(0x00_00_03)),
                both: Themed::both(Colour::rgb(0x00_00_04)),
                word_merged: Themed::both(Colour::rgb(0x00_00_05)),
                current: Themed::both(Colour::rgb(0x00_00_06)),
                deleted: Themed::both(Colour::rgb(0x00_00_07)),
                resolved: Themed::both(Colour::rgb(0x00_00_08)),
                band_opacity: Themed { light: 0.5, dark: 0.75 },
            },
            conflict_style: Some(ConflictStyle::Merge),
            line_ending: OutputEnding::Base,
            confirm_unresolved_save: false,
            syntax: false,
            syntax_max_lines: 123,
            soft_wrap: true,
            key_bindings: added_binding(),
        }
    }

    #[test]
    fn the_defaults_render_as_written_and_read_back() {
        assert_eq!(render(&Settings::default(), false), defaults());
        assert_eq!(settings(&defaults()), Settings::default());
    }

    #[test]
    fn every_key_renders_its_value_and_reads_back() {
        let text = render(&every_key_changed(), false);
        let default_keybinds: String = defaults()
            .lines()
            .filter(|line| line.starts_with("keybind = "))
            .flat_map(|line| [line, "\n"])
            .collect();
        assert_eq!(
            text,
            "\
merge-semantics = jj
same-change = keep
hunk-level = word
diff-algorithm = myers-minimal
ignore-whitespace = trailing
merge-ignore-line-endings = true
diff-ignore-line-endings = false
refine-conflicts = deep
syntax-boundaries = true
tab-width = 2
font = Iosevka Term
font-size = 12.5
toolbar-font-size = 9.5
window-size = 1600x1000
scroll-sync = false
scrollbars = when-scrolling
show-base-pane = true
show-origin-column = true
revision-id = commit
theme = dark
line-height = 2.25
color-left = #abcdef
color-right = light:#010203,dark:#fefdfc
color-base = #000001
color-conflict = #000002
color-result-edit = #000003
color-same-change = #000004
color-word-merged = #000005
color-current-region = #000006
color-deleted = #000007
color-resolved = #000008
band-opacity = light:0.5,dark:0.75
conflict-style = merge
output-line-ending = base
confirm-unresolved-save = false
DEFAULT_KEYBINDS
keybind = ctrl+j=next-conflict
syntax-highlighting = false
syntax-whole-file-max-lines = 123
soft-wrap = true
"
            .replace("DEFAULT_KEYBINDS\n", &default_keybinds)
        );
        assert_eq!(settings(&text), every_key_changed());
    }

    #[test]
    fn the_other_values_of_each_name_read_back() {
        for (same, hunk, algorithm, whitespace, refine) in [
            (
                SameChange::Accept,
                HunkLevel::Line,
                diff::Algorithm::Myers,
                diff::Whitespace::Amount,
                Refine::Off,
            ),
            (
                SameChange::Keep,
                HunkLevel::Word,
                diff::Algorithm::Histogram,
                diff::Whitespace::All,
                Refine::Ends,
            ),
        ] {
            let value = Settings {
                semantics: Choice::Git,
                same_change: Some(same),
                hunk_level: Some(hunk),
                diff: DiffOptions { algorithm, whitespace, ..DiffOptions::default() },
                refine,
                ..Settings::default()
            };
            assert_eq!(settings(&render(&value, false)), value);
        }
    }

    #[test]
    fn a_font_the_file_would_trim_or_unquote_renders_in_quotes() {
        for (font, line) in [
            ("  Mono  ", r#"font = "  Mono  ""#),
            (r#""Mono""#, r#"font = ""Mono"""#),
            ("a \"b\" c", r#"font = a "b" c"#),
        ] {
            let value = Settings { font: Some(font.to_owned()), ..Settings::default() };
            let text = render(&value, false);
            assert!(text.contains(&format!("\n{line}\n")), "{text}");
            assert_eq!(settings(&text).font.as_deref(), Some(font));
        }
    }

    #[test]
    fn docs_put_each_description_above_its_key() {
        let text = render(&Settings::default(), true);
        assert!(
            text.starts_with(
                "\
# Whose merge rules apply: auto follows the repository, or git or jj.
merge-semantics = auto

# Whether a change every side made resolves (accept) or conflicts (keep). \
Empty: jj's merge.same-change under jj semantics, else accept.
same-change =

"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "\
# The diff algorithm: histogram, myers or myers-minimal.
diff-algorithm = histogram
"
            ),
            "{text}"
        );
        assert!(
            text.ends_with(
                "\
# The longest file the result and diff editors parse whole for syntax colours, in lines; a longer one parses a chunk at a time.
syntax-whole-file-max-lines = 20000

# Long lines wrap in the result, input and diff panes when a window opens; false scrolls them sideways.
soft-wrap = false
"
            ),
            "{text}"
        );
        assert_eq!(settings(&text), Settings::default());
        assert_eq!(settings(&render(&every_key_changed(), true)), every_key_changed());
    }

    #[test]
    fn every_key_has_a_description_and_a_unique_name() {
        for key in KEYS {
            assert!(!key.description.is_empty(), "{}", key.name);
            assert_eq!(KEYS.iter().filter(|k| k.name == key.name).count(), 1, "{}", key.name);
        }
        let bindings = DEFAULTS.lines().filter(|l| l.starts_with("keybind = cmd+"));
        assert_eq!(KEYS.len(), DEFAULTS.lines().count() - bindings.count());
    }

    #[test]
    fn a_font_with_spaces_reads_back() {
        let shown = Settings { font: Some("  Mono  ".to_owned()), ..Settings::default() };
        let mut loaded = Settings::default();
        let key = KEYS.iter().find(|k| k.name == "font").expect("font");
        (key.set)(&mut loaded, &(key.get)(&shown)).expect("reads back");
        assert_eq!(loaded.font.as_deref(), Some("  Mono  "));
    }

    #[test]
    fn each_keybind_line_adds_a_binding() {
        let loaded = settings("keybind = ctrl+j=next-conflict\nkeybind = ctrl+k=take-left\n");
        let mut expected = added_binding();
        expected.apply(Keybind::parse("ctrl+k=take-left", Platform::CURRENT).unwrap());
        assert_eq!(loaded.key_bindings, expected);
    }

    #[test]
    fn a_bad_keybind_line_names_its_line_and_the_others_apply() {
        let text = "font-size = 14\n\
            keybind = ctrl+j=nonsense\n\
            keybind = ctrl+=next-conflict\n\
            keybind = ctrl+j=next-conflict\n";
        let Loaded { settings, errors } = read(text);
        let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                format!("{MAIN}:2: keybind: unknown action `nonsense`"),
                format!("{MAIN}:3: keybind: malformed chord `ctrl+`"),
            ]
        );
        assert_eq!(settings.font_size, 14.);
        assert_eq!(settings.key_bindings, added_binding());
    }

    #[test]
    fn a_modifier_the_platform_lacks_is_a_bad_line() {
        let lacking = if cfg!(target_os = "macos") { "super" } else { "cmd" };
        let mut settings = Settings::default();
        let problem = apply(&mut settings, "keybind", &format!("{lacking}+j=take-left"));
        assert_eq!(
            problem,
            Err(Problem::Keybind(KeybindError::Modifier {
                modifier: lacking.to_owned(),
                chord: format!("{lacking}+j"),
            }))
        );
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn an_empty_keybind_gives_the_default_bindings() {
        let loaded = settings("keybind = clear\nkeybind =\n");
        assert_eq!(loaded.key_bindings, KeyBindings::default());
    }

    #[test]
    fn every_key_reads_back_the_value_it_writes() {
        let changed = every_key_changed();
        let mut copy = Settings::default();
        for key in KEYS {
            (key.set)(&mut copy, &(key.get)(&changed)).expect(key.name);
        }
        assert_eq!(copy, changed);
    }

    #[test]
    fn the_template_comments_out_every_key_at_its_default() {
        let text = template();
        assert!(
            text.starts_with(
                "\
# triad config. Remove the `# ` before a setting to change it.

# Whose merge rules apply: auto follows the repository, or git or jj.
# merge-semantics = auto

"
            ),
            "{text}"
        );
        assert!(text.ends_with("# soft-wrap = false\n"), "{text}");
        for line in defaults().lines() {
            assert!(text.contains(&format!("\n# {line}\n")), "{line}");
        }
        let loaded = read(&text);
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn removing_a_hash_in_the_template_sets_that_key() {
        let text = template().replace("# font-size = 10\n", "font-size = 14\n");
        assert_eq!(settings(&text).font_size, 14.);
    }

    #[test]
    fn the_template_is_written_once_with_its_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("triad").join(FILE_NAME);
        assert!(write_template(&path).expect("written"));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), template());
        std::fs::write(&path, "font-size = 14\n").expect("written");
        assert!(!write_template(&path).expect("left alone"));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "font-size = 14\n");
    }

    #[test]
    fn a_template_that_cannot_be_written_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").expect("written");
        let error = write_template(&blocker.join(FILE_NAME)).expect_err("a file is no directory");
        // `create_dir_all` finds a file where the directory goes.
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists, "{error:?}");
    }
}
