//! Soft wrap in the result, the top panes and the diff editors (spec §5, §6.4).

use crate::window::SOFT_WRAP_MAX_LINES;

/// Whether an editor wraps long lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wrap {
    On,
    Off,
    /// A text has more than [`SOFT_WRAP_MAX_LINES`] lines, so the editors never wrap.
    TooLong,
}

impl Wrap {
    /// Wrap if `wanted`, unless a text of `line_counts` is too long to wrap.
    pub(crate) fn new(wanted: bool, mut line_counts: impl Iterator<Item = usize>) -> Self {
        if line_counts.any(|lines| lines > SOFT_WRAP_MAX_LINES) {
            Self::TooLong
        } else if wanted {
            Self::On
        } else {
            Self::Off
        }
    }

    #[must_use]
    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::On,
            Self::TooLong => Self::TooLong,
        }
    }

    pub(crate) fn wraps(self) -> bool {
        self == Self::On
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_up_to_the_limit_wrap_when_asked() {
        let at_limit = [1, SOFT_WRAP_MAX_LINES];
        assert_eq!(Wrap::new(true, at_limit.into_iter()), Wrap::On);
        assert_eq!(Wrap::new(false, at_limit.into_iter()), Wrap::Off);
        assert_eq!(Wrap::new(true, std::iter::empty()), Wrap::On);
    }

    #[test]
    fn one_text_past_the_limit_never_wraps() {
        let past = [1, SOFT_WRAP_MAX_LINES + 1];
        assert_eq!(Wrap::new(true, past.into_iter()), Wrap::TooLong);
        assert_eq!(Wrap::new(false, past.into_iter()), Wrap::TooLong);
    }

    #[test]
    fn a_toggle_flips_wrap_except_past_the_limit() {
        assert_eq!(Wrap::On.toggled(), Wrap::Off);
        assert_eq!(Wrap::Off.toggled(), Wrap::On);
        assert_eq!(Wrap::TooLong.toggled(), Wrap::TooLong);
        assert!(Wrap::On.wraps());
        assert!(!Wrap::Off.wraps());
        assert!(!Wrap::TooLong.wraps());
    }
}
