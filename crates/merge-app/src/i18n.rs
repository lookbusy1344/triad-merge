//! The windows' text in each language (spec §6.5). Each language has a Project Fluent
//! catalogue, `locales/<tag>.ftl`, built into the binary. [`crate::strings`] is the only
//! caller: it names a message and its arguments, and this module formats it in the language
//! the run chose. A message missing from a catalogue, or one that fails to format, falls back
//! to English (`DESIGN.md` §E3.6).

use std::borrow::Cow;
use std::sync::OnceLock;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use gpui_kit::SharedString;
use merge_config::settings::UiLanguage;
use unic_langid::{LanguageIdentifier, langid};

/// A language with a catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Locale {
    En,
    Es,
    ZhCn,
    Ja,
    De,
    Fr,
    PtBr,
    Ru,
    Nl,
}

impl Locale {
    pub(crate) const ALL: [Self; 9] = [
        Self::En,
        Self::Es,
        Self::ZhCn,
        Self::Ja,
        Self::De,
        Self::Fr,
        Self::PtBr,
        Self::Ru,
        Self::Nl,
    ];

    /// The catalogue's text.
    const fn source(self) -> &'static str {
        match self {
            Self::En => include_str!("../locales/en.ftl"),
            Self::Es => include_str!("../locales/es.ftl"),
            Self::ZhCn => include_str!("../locales/zh-CN.ftl"),
            Self::Ja => include_str!("../locales/ja.ftl"),
            Self::De => include_str!("../locales/de.ftl"),
            Self::Fr => include_str!("../locales/fr.ftl"),
            Self::PtBr => include_str!("../locales/pt-BR.ftl"),
            Self::Ru => include_str!("../locales/ru.ftl"),
            Self::Nl => include_str!("../locales/nl.ftl"),
        }
    }

    /// The language whose plural rules the catalogue follows.
    fn id(self) -> LanguageIdentifier {
        match self {
            Self::En => langid!("en"),
            Self::Es => langid!("es"),
            Self::ZhCn => langid!("zh-CN"),
            Self::Ja => langid!("ja"),
            Self::De => langid!("de"),
            Self::Fr => langid!("fr"),
            Self::PtBr => langid!("pt-BR"),
            Self::Ru => langid!("ru"),
            Self::Nl => langid!("nl"),
        }
    }

    /// The language a setting holds; `auto` holds none.
    const fn of_setting(setting: UiLanguage) -> Option<Self> {
        match setting {
            UiLanguage::Auto => None,
            UiLanguage::En => Some(Self::En),
            UiLanguage::Es => Some(Self::Es),
            UiLanguage::ZhCn => Some(Self::ZhCn),
            UiLanguage::Ja => Some(Self::Ja),
            UiLanguage::De => Some(Self::De),
            UiLanguage::Fr => Some(Self::Fr),
            UiLanguage::PtBr => Some(Self::PtBr),
            UiLanguage::Ru => Some(Self::Ru),
            UiLanguage::Nl => Some(Self::Nl),
        }
    }

    /// The catalogue for a system locale such as `pt-BR`, `de_DE.UTF-8` or `zh-Hant-TW`,
    /// matched by its language subtag alone: every Chinese locale reads the simplified
    /// catalogue, every Portuguese one the Brazilian. None for a language without one.
    fn of_system(locale: &str) -> Option<Self> {
        let language = locale.split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
        match language.as_str() {
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            "zh" => Some(Self::ZhCn),
            "ja" => Some(Self::Ja),
            "de" => Some(Self::De),
            "fr" => Some(Self::Fr),
            "pt" => Some(Self::PtBr),
            "ru" => Some(Self::Ru),
            "nl" => Some(Self::Nl),
            _ => None,
        }
    }

    /// The language `setting` names; under `auto`, the system's `locale`, else English.
    pub(crate) fn choose(setting: UiLanguage, system: Option<&str>) -> Self {
        Self::of_setting(setting).or_else(|| system.and_then(Self::of_system)).unwrap_or(Self::En)
    }

    /// Its catalogue, parsed on first use.
    fn bundle(self) -> &'static Bundle {
        static BUNDLES: [OnceLock<Bundle>; Locale::ALL.len()] =
            [const { OnceLock::new() }; Locale::ALL.len()];
        BUNDLES[self as usize].get_or_init(|| bundle(self))
    }
}

type Bundle = FluentBundle<FluentResource>;

/// The catalogue of `locale`. A syntax error drops only the entry it is in, so the rest of
/// the catalogue still serves; `tests` keep every catalogue free of them.
fn bundle(locale: Locale) -> Bundle {
    let resource =
        FluentResource::try_new(locale.source().to_owned()).unwrap_or_else(|(resource, errors)| {
            tracing::error!(
                ?locale,
                ?errors,
                "the catalogue has syntax errors; its other messages serve"
            );
            resource
        });
    let mut bundle = FluentBundle::new_concurrent(vec![locale.id()]);
    // No right-to-left language yet: isolation marks would only show as stray characters.
    bundle.set_use_isolating(false);
    if let Err(errors) = bundle.add_resource(resource) {
        tracing::error!(
            ?locale,
            ?errors,
            "the catalogue repeats messages; the first of each serves"
        );
    }
    bundle
}

/// The language of this run's windows. English until [`set_locale`].
static LOCALE: OnceLock<Locale> = OnceLock::new();

/// Fix the language of this run's windows. A run sets it once, before its first window;
/// a second call keeps the first language.
pub(crate) fn set_locale(locale: Locale) {
    if LOCALE.set(locale).is_err() && current() != locale {
        tracing::warn!(?locale, kept = ?current(), "the language is already set; it stays");
    }
}

/// The language of this run's windows.
pub(crate) fn current() -> Locale {
    LOCALE.get().copied().unwrap_or(Locale::En)
}

/// A value a message places in its text.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Arg<'a> {
    Text(&'a str),
    /// A count, which also selects the plural form.
    Count(usize),
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(text: &'a str) -> Self {
        Self::Text(text)
    }
}

impl<'a> From<&'a String> for Arg<'a> {
    fn from(text: &'a String) -> Self {
        Self::Text(text)
    }
}

impl<'a> From<&'a SharedString> for Arg<'a> {
    fn from(text: &'a SharedString) -> Self {
        Self::Text(text)
    }
}

impl From<usize> for Arg<'_> {
    fn from(count: usize) -> Self {
        Self::Count(count)
    }
}

/// Message `id` without arguments in this run's language.
pub(crate) fn text(id: &str) -> SharedString {
    shared(format_in(current(), id, &[]))
}

/// `text` as a [`SharedString`], without a copy when it borrows from a catalogue.
pub(crate) fn shared(text: Cow<'static, str>) -> SharedString {
    match text {
        Cow::Borrowed(text) => SharedString::new_static(text),
        Cow::Owned(text) => text.into(),
    }
}

/// Message `id` with `args` in this run's language.
pub(crate) fn format(id: &str, args: &[(&str, Arg<'_>)]) -> String {
    format_in(current(), id, args).into_owned()
}

/// Message `id` with `args` in `locale`, else in English, else the id itself.
pub(crate) fn format_in(locale: Locale, id: &str, args: &[(&str, Arg<'_>)]) -> Cow<'static, str> {
    try_format(locale, id, args)
        .or_else(|| {
            tracing::warn!(?locale, id, "no such message, or it failed to format; English serves");
            try_format(Locale::En, id, args)
        })
        .unwrap_or_else(|| {
            tracing::error!(id, "no English message; its id serves");
            Cow::Owned(id.to_owned())
        })
}

/// Message `id` in `locale`, when the catalogue has it and it formats without error.
fn try_format(locale: Locale, id: &str, args: &[(&str, Arg<'_>)]) -> Option<Cow<'static, str>> {
    let bundle = locale.bundle();
    let pattern = bundle.get_message(id)?.value()?;
    let args = (!args.is_empty()).then(|| {
        args.iter()
            .map(|&(name, arg)| {
                let value = match arg {
                    Arg::Text(text) => FluentValue::from(text),
                    Arg::Count(count) => FluentValue::from(count),
                };
                (name, value)
            })
            .collect::<FluentArgs<'_>>()
    });
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args.as_ref(), &mut errors);
    errors.is_empty().then_some(text)
}

/// Message `id`, with arguments as `name = value` pairs, in this run's language: a
/// [`SharedString`] without arguments, a `String` with them.
macro_rules! t {
    ($id:literal) => {
        $crate::i18n::text($id)
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::format($id, &[$((stringify!($name), $crate::i18n::Arg::from($value))),+])
    };
}
pub(crate) use t;

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

    use super::*;

    /// Each message's variables, by id.
    fn messages(locale: Locale) -> BTreeMap<String, BTreeSet<String>> {
        let resource = FluentResource::try_new(locale.source().to_owned())
            .unwrap_or_else(|(_, errors)| panic!("{locale:?} has syntax errors: {errors:?}"));
        resource
            .entries()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some((
                    message.id.name.to_owned(),
                    message.value.as_ref().map_or_default(variables),
                )),
                _ => None,
            })
            .collect()
    }

    fn variables(pattern: &Pattern<&str>) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        for element in &pattern.elements {
            if let PatternElement::Placeable { expression } = element {
                expression_variables(expression, &mut found);
            }
        }
        found
    }

    fn expression_variables(expression: &Expression<&str>, found: &mut BTreeSet<String>) {
        match expression {
            Expression::Inline(inline) => inline_variables(inline, found),
            Expression::Select { selector, variants } => {
                inline_variables(selector, found);
                for variant in variants {
                    found.extend(variables(&variant.value));
                }
            }
        }
    }

    fn inline_variables(inline: &InlineExpression<&str>, found: &mut BTreeSet<String>) {
        match inline {
            InlineExpression::VariableReference { id } => {
                found.insert(id.name.to_owned());
            }
            InlineExpression::Placeable { expression } => expression_variables(expression, found),
            _ => {}
        }
    }

    #[test]
    fn every_catalogue_holds_the_english_messages_with_the_same_variables() {
        let english = messages(Locale::En);
        assert!(english.len() > 200, "the English catalogue parsed whole: {}", english.len());
        for locale in Locale::ALL {
            assert_eq!(messages(locale), english, "{locale:?}");
        }
    }

    #[test]
    fn every_catalogue_parses_into_a_bundle_without_repeats() {
        for locale in Locale::ALL {
            let resource = FluentResource::try_new(locale.source().to_owned()).expect("parses");
            let mut bundle = FluentBundle::new_concurrent(vec![locale.id()]);
            assert_eq!(bundle.add_resource(resource).err(), None, "{locale:?}");
        }
    }

    /// Every message formats in every language, given a text for each variable and the
    /// counts that pick each plural form somewhere: 0, 1, 2, 5, 21 and 22.
    #[test]
    fn every_message_formats_in_every_language_for_every_plural_form() {
        for locale in Locale::ALL {
            for (id, names) in messages(locale) {
                for count in [0, 1, 2, 5, 21, 22] {
                    let args: Vec<(&str, Arg<'_>)> = names
                        .iter()
                        .map(|name| {
                            let arg = match name.as_str() {
                                "count" | "number" | "region" | "regions" | "line" | "last"
                                | "kept" | "sides" | "unresolved" => Arg::Count(count),
                                _ => Arg::Text("x"),
                            };
                            (name.as_str(), arg)
                        })
                        .collect();
                    let formatted = try_format(locale, &id, &args);
                    assert!(
                        formatted.as_deref().is_some_and(|text| !text.trim().is_empty()),
                        "{locale:?} {id} with {count}: {formatted:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_count_picks_the_plural_form_of_its_language() {
        let lines =
            |locale, count: usize| format_in(locale, "lines-deleted", &[("count", count.into())]);
        assert_eq!(lines(Locale::En, 1), "1 line deleted");
        assert_eq!(lines(Locale::En, 2), "2 lines deleted");
        assert_eq!(lines(Locale::En, 0), "0 lines deleted");
        // Russian has three forms: one (1, 21), few (2–4, 22) and many (5–20, 0).
        assert_eq!(lines(Locale::Ru, 1), "Удалена 1 строка");
        assert_eq!(lines(Locale::Ru, 21), "Удалена 21 строка");
        assert_eq!(lines(Locale::Ru, 3), "Удалено 3 строки");
        assert_eq!(lines(Locale::Ru, 22), "Удалено 22 строки");
        assert_eq!(lines(Locale::Ru, 5), "Удалено 5 строк");
        assert_eq!(lines(Locale::Ru, 11), "Удалено 11 строк");
        // Japanese has one form for every count.
        assert_eq!(lines(Locale::Ja, 1), "1 行削除");
        assert_eq!(lines(Locale::Ja, 7), "7 行削除");
        // French counts 0 as one.
        assert_eq!(lines(Locale::Fr, 0), "0 ligne supprimée");
        assert_eq!(lines(Locale::Fr, 2), "2 lignes supprimées");
        assert_eq!(lines(Locale::Nl, 1), "1 regel verwijderd");
        assert_eq!(lines(Locale::Nl, 0), "0 regels verwijderd");
    }

    #[test]
    fn an_exact_count_wins_over_its_plural_form() {
        let saved =
            |locale, count: usize| format_in(locale, "copy-saved", &[("count", count.into())]);
        assert_eq!(saved(Locale::En, 0), "Copy saved, resolved");
        assert_eq!(saved(Locale::En, 1), "Copy saved with 1 conflict");
        assert_eq!(saved(Locale::Fr, 0), "Copie enregistrée, résolue");
        assert_eq!(saved(Locale::Fr, 1), "Copie enregistrée avec 1 conflit");
    }

    #[test]
    fn a_message_places_text_without_isolation_marks() {
        assert_eq!(
            format_in(Locale::De, "history-title", &[("path", "src/a.rs".into())]),
            "Verlauf von src/a.rs"
        );
        assert_eq!(
            format_in(Locale::ZhCn, "history-title", &[("path", "src/a.rs".into())]),
            "src/a.rs 的历史"
        );
    }

    #[test]
    fn a_message_without_arguments_borrows_from_its_catalogue() {
        assert!(matches!(format_in(Locale::Ja, "close", &[]), Cow::Borrowed("閉じる")));
        assert!(matches!(format_in(Locale::Es, "menu-save", &[]), Cow::Borrowed("Guardar")));
    }

    #[test]
    fn a_missing_argument_falls_back_to_english_then_to_the_id() {
        // Without its path, the German message fails to format, and so does the English.
        assert_eq!(format_in(Locale::De, "history-title", &[]), "history-title");
        assert_eq!(format_in(Locale::Fr, "no-such-message", &[]), "no-such-message");
    }

    #[test]
    fn a_setting_names_its_language_and_auto_follows_the_system() {
        assert_eq!(Locale::choose(UiLanguage::Ja, Some("de-DE")), Locale::Ja);
        assert_eq!(Locale::choose(UiLanguage::En, Some("fr")), Locale::En);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("de-DE")), Locale::De);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("pt_PT.UTF-8")), Locale::PtBr);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("zh-Hant-TW")), Locale::ZhCn);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("ZH-cn")), Locale::ZhCn);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("ru_RU@euro")), Locale::Ru);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("nl-BE")), Locale::Nl);
        assert_eq!(Locale::choose(UiLanguage::Nl, Some("en-GB")), Locale::Nl);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("ko-KR")), Locale::En);
        assert_eq!(Locale::choose(UiLanguage::Auto, Some("")), Locale::En);
        assert_eq!(Locale::choose(UiLanguage::Auto, None), Locale::En);
    }

    #[test]
    fn the_tests_run_in_english() {
        assert_eq!(current(), Locale::En);
        assert_eq!(t!("close"), "Close");
        assert_eq!(t!("count-files", count = 3_usize), "3 files");
    }
}
