//! Enums whose values the config file, the key bindings and the command line name by their
//! kebab-case names (spec §6.3, §6.4).
//!
//! Each enum derives [`strum::VariantArray`], [`strum::IntoStaticStr`] and
//! [`strum::EnumMessage`] with `#[strum(serialize_all = "kebab-case")]`, so no list of names
//! can miss a variant. A front end builds its parser from [`Named::names`] and [`Named::help`].

use strum::{EnumMessage, VariantArray};

/// An enum named by its values' kebab-case names.
pub trait Named: Copy + VariantArray + EnumMessage + Into<&'static str> + 'static {
    /// The value's name.
    #[must_use]
    fn name(self) -> &'static str {
        self.into()
    }

    /// The value named `name`, compared case-sensitively.
    #[must_use]
    fn from_name(name: &str) -> Option<Self> {
        Self::VARIANTS.iter().copied().find(|value| value.name() == name)
    }

    /// Every value's name, in declaration order.
    fn names() -> impl Iterator<Item = &'static str> {
        Self::VARIANTS.iter().map(|&value| value.name())
    }

    /// The value's doc comment, which help shows beside its name.
    #[must_use]
    fn help(self) -> Option<&'static str> {
        self.get_documentation()
    }
}

impl<T: Copy + VariantArray + EnumMessage + Into<&'static str> + 'static> Named for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybind::{KeyAction, Modifier};
    use crate::settings::Choice;
    use crate::settings::{Algorithm, LineEndingArg};

    #[test]
    fn a_value_is_named_in_kebab_case() {
        assert_eq!(Algorithm::MyersMinimal.name(), "myers-minimal");
        assert_eq!(LineEndingArg::Crlf.name(), "crlf");
        assert_eq!(KeyAction::TakeLeftThenRight.name(), "take-left-then-right");
        assert_eq!(Modifier::Cmd.name(), "cmd");
    }

    #[test]
    fn every_value_is_listed_in_order() {
        assert_eq!(Algorithm::names().collect::<Vec<_>>(), ["histogram", "myers", "myers-minimal"]);
        assert_eq!(LineEndingArg::names().collect::<Vec<_>>(), ["keep", "base", "lf", "crlf"]);
    }

    #[test]
    fn a_name_reads_back_case_sensitively() {
        assert_eq!(Algorithm::from_name("myers-minimal"), Some(Algorithm::MyersMinimal));
        assert_eq!(Algorithm::from_name("Myers"), None);
        assert_eq!(Algorithm::from_name("myers_minimal"), None);
        assert_eq!(Algorithm::from_name(""), None);
    }

    #[test]
    fn a_documented_value_has_help() {
        assert_eq!(Choice::Auto.help(), Some("The caller's VCS, found from the paths."));
        assert_eq!(Choice::Git.help(), None);
    }
}
