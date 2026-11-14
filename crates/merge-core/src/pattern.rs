//! `ignore-pattern`: regular expressions whose matches a line drops before comparison (spec
//! §4.2). The patterns change which lines compare equal and never the bytes.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A pattern the config or a journal names that cannot be used.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PatternError {
    #[error("`{pattern}` is not a regular expression")]
    Invalid {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    /// Cutting an empty match changes nothing, so such a pattern is a mistake.
    #[error("`{pattern}` matches empty text")]
    MatchesEmpty { pattern: String },
}

/// The patterns in force, in the order given. Cheap to clone; equal when their sources are.
#[derive(Clone, Default)]
pub struct Patterns(Option<Arc<[Pattern]>>);

struct Pattern {
    source: Box<str>,
    regex: Regex,
}

/// No patterns: what a comparison without filters borrows.
pub static NONE: Patterns = Patterns::empty();

impl Patterns {
    /// No patterns.
    #[must_use]
    pub const fn empty() -> Self {
        Self(None)
    }

    /// Compile `sources`, in order.
    ///
    /// # Errors
    /// The first source that does not compile, or that matches empty text.
    pub fn new<S: AsRef<str>>(sources: impl IntoIterator<Item = S>) -> Result<Self, PatternError> {
        let compiled = sources
            .into_iter()
            .map(|source| compile(source.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self((!compiled.is_empty()).then(|| compiled.into())))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.patterns().len()
    }

    /// The patterns as written, in order.
    pub fn sources(&self) -> impl ExactSizeIterator<Item = &str> {
        self.patterns().iter().map(|p| &*p.source)
    }

    /// `text` with every match of each pattern cut out, pattern by pattern; borrowed when none
    /// matches.
    #[must_use]
    pub fn cut<'t>(&self, text: &'t str) -> Cow<'t, str> {
        self.patterns().iter().fold(Cow::Borrowed(text), |text, pattern| {
            let cut = match pattern.regex.replace_all(&text, "") {
                Cow::Owned(cut) => Some(cut),
                Cow::Borrowed(_) => None,
            };
            cut.map_or(text, Cow::Owned)
        })
    }

    fn patterns(&self) -> &[Pattern] {
        self.0.as_deref().unwrap_or_default()
    }
}

fn compile(source: &str) -> Result<Pattern, PatternError> {
    let regex = Regex::new(source)
        .map_err(|error| PatternError::Invalid { pattern: source.to_owned(), source: error })?;
    if regex.is_match("") {
        return Err(PatternError::MatchesEmpty { pattern: source.to_owned() });
    }
    Ok(Pattern { source: source.into(), regex })
}

impl PartialEq for Patterns {
    fn eq(&self, other: &Self) -> bool {
        self.sources().eq(other.sources())
    }
}

impl Eq for Patterns {}

impl fmt::Debug for Patterns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.sources()).finish()
    }
}

impl Serialize for Patterns {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.sources())
    }
}

impl<'de> Deserialize<'de> for Patterns {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let sources = Vec::<String>::deserialize(deserializer)?;
        Self::new(&sources).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(sources: &[&str]) -> Patterns {
        Patterns::new(sources).expect("valid patterns")
    }

    #[test]
    fn every_match_of_a_pattern_is_cut() {
        assert_eq!(patterns(&[r"\d+"]).cut("v12 and 3"), "v and ");
    }

    #[test]
    fn patterns_cut_in_order_each_on_the_text_the_last_left() {
        // `#.*` leaves "a = 1 ", which ` +$` then trims; in the other order ` +$` finds nothing.
        assert_eq!(patterns(&["#.*", " +$"]).cut("a = 1 # note"), "a = 1");
        assert_eq!(patterns(&[" +$", "#.*"]).cut("a = 1 # note"), "a = 1 ");
    }

    #[test]
    fn a_pattern_may_cut_the_whole_text() {
        assert_eq!(patterns(&["//.*"]).cut("// all comment"), "");
    }

    #[test]
    fn text_without_a_match_stays_borrowed() {
        assert!(matches!(patterns(&[r"\d"]).cut("abc"), Cow::Borrowed("abc")));
        assert!(matches!(Patterns::default().cut("a1"), Cow::Borrowed("a1")));
    }

    #[test]
    fn no_sources_are_no_patterns() {
        let none = Patterns::new(std::iter::empty::<&str>()).expect("empty");
        assert!(none.is_empty());
        assert_eq!(none.len(), 0);
        assert_eq!(none, NONE);
        assert!(!patterns(&["x"]).is_empty());
    }

    #[test]
    fn a_source_that_does_not_compile_is_refused_by_name() {
        let error = Patterns::new(["x", "(", "["]).expect_err("an unclosed group");
        assert!(
            matches!(&error, PatternError::Invalid { pattern, .. } if pattern == "("),
            "{error:?}"
        );
        assert_eq!(error.to_string(), "`(` is not a regular expression");
    }

    #[test]
    fn a_source_that_matches_empty_text_is_refused() {
        for source in ["a*", "^", "$", "(x)?", "^$"] {
            assert_eq!(
                Patterns::new([source]),
                Err(PatternError::MatchesEmpty { pattern: source.to_owned() }),
                "{source}"
            );
        }
        assert_eq!(
            PatternError::MatchesEmpty { pattern: "a*".to_owned() }.to_string(),
            "`a*` matches empty text"
        );
    }

    #[test]
    fn patterns_are_equal_by_their_sources_in_order() {
        assert_eq!(patterns(&["a", "b"]), patterns(&["a", "b"]));
        assert_ne!(patterns(&["a", "b"]), patterns(&["b", "a"]));
        assert_ne!(patterns(&["a"]), Patterns::default());
        assert_eq!(patterns(&["a", "b"]).sources().collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn patterns_serialise_as_their_sources_and_read_back() {
        let json = serde_json::to_string(&patterns(&[r"\d+", "x"])).expect("serialises");
        assert_eq!(json, r#"["\\d+","x"]"#);
        let back: Patterns = serde_json::from_str(&json).expect("reads back");
        assert_eq!(back, patterns(&[r"\d+", "x"]));
    }

    #[test]
    fn a_stored_pattern_that_does_not_compile_fails_to_read() {
        let error = serde_json::from_str::<Patterns>(r#"["("]"#).expect_err("invalid");
        assert!(error.to_string().starts_with("`(` is not a regular expression"), "{error}");
    }
}
