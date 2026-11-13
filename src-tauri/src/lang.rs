//! Languages. Three of them, kept apart:
//! - the interface's (settings, `ui`): labels, errors, progress lines;
//! - the lessons' (`CourseMeta::language`, per lesson `LessonMeta::language`): what is spoken in
//!   the recordings, for Whisper and the cleanup (which never translates);
//! - the materials' (`CourseMeta::materials_language`): summary, map, study, chat, exports.
//!
//! A course or lesson written before languages existed has no field: it is Italian.
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    It,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::It => "it",
            Lang::En => "en",
        }
    }

    /// `it` for Italian, `en` for English.
    pub fn pick<T>(self, it: T, en: T) -> T {
        match self {
            Lang::It => it,
            Lang::En => en,
        }
    }

    /// The language's name, written in `in_lang` ("inglese", "English").
    pub fn name_in(self, in_lang: Lang) -> &'static str {
        match (self, in_lang) {
            (Lang::It, Lang::It) => "italiano",
            (Lang::En, Lang::It) => "inglese",
            (Lang::It, Lang::En) => "Italian",
            (Lang::En, Lang::En) => "English",
        }
    }

    /// From a locale like "it-IT", "en_US.UTF-8": Italian only for Italian, English otherwise.
    pub fn from_locale(l: &str) -> Lang {
        if l.trim().to_ascii_lowercase().starts_with("it") { Lang::It } else { Lang::En }
    }
}

/// The interface language chosen in the settings.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UiLang {
    #[default]
    System,
    It,
    En,
}

impl UiLang {
    pub fn resolve(self) -> Lang {
        match self {
            UiLang::It => Lang::It,
            UiLang::En => Lang::En,
            UiLang::System => system(),
        }
    }
}

/// The system's language as Rust sees it, until the interface says it (from `navigator.language`,
/// which is right also on macOS, where an app opened from the Finder has no `LANG`).
pub fn system() -> Lang {
    ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty() && s != "C" && s != "POSIX")).map(|l| Lang::from_locale(&l)).unwrap_or(Lang::It)
}

static UI: AtomicU8 = AtomicU8::new(0);

/// The interface language now (for messages built in Rust).
pub fn ui() -> Lang {
    if UI.load(Ordering::Relaxed) == 1 { Lang::En } else { Lang::It }
}

pub fn set_ui(l: Lang) {
    UI.store(if l == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

/// A message in the interface language: `tr!("… {x} …", "… {x} …")`, with `format!` arguments.
#[macro_export]
macro_rules! tr {
    ($it:literal, $en:literal $(,)?) => {
        match $crate::lang::ui() { $crate::lang::Lang::It => format!($it), $crate::lang::Lang::En => format!($en) }
    };
    ($it:literal, $en:literal, $($arg:tt)+) => {
        match $crate::lang::ui() { $crate::lang::Lang::It => format!($it, $($arg)+), $crate::lang::Lang::En => format!($en, $($arg)+) }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tr_captures_variables() {
        let n = 3;
        assert_eq!(crate::tr!("{n} parti", "{n} parts"), "3 parti");
        assert_eq!(crate::tr!("{} parti", "{} parts", n + 1), "4 parti");
    }

    #[test]
    fn locales() {
        assert_eq!(Lang::from_locale("it-IT"), Lang::It);
        assert_eq!(Lang::from_locale("it_CH.UTF-8"), Lang::It);
        assert_eq!(Lang::from_locale("en-GB"), Lang::En);
        assert_eq!(Lang::from_locale("de-DE"), Lang::En);
    }

    #[test]
    fn missing_is_italian() {
        #[derive(Deserialize)]
        struct C {
            #[serde(default)]
            l: Lang,
        }
        assert_eq!(serde_json::from_str::<C>("{}").unwrap().l, Lang::It);
        assert_eq!(serde_json::from_str::<C>(r#"{"l":"en"}"#).unwrap().l, Lang::En);
    }
}
