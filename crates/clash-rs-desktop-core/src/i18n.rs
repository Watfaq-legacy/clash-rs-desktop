//! Two UI languages, picked once per process (and again when the user
//! changes it). Strings stay inline next to the code that uses them:
//! `t("已连接", "Connected")`, or `tr!("{n} 个节点", "{n} nodes")` for
//! formatted text.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// The user's choice; `System` follows the OS display language.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    System,
    Zh,
    En,
}

impl Language {
    /// Resolve to a concrete language: Chinese for any `zh*` locale,
    /// English otherwise.
    pub fn is_english(self) -> bool {
        match self {
            Language::Zh => false,
            Language::En => true,
            Language::System => !sys_locale::get_locale()
                .is_some_and(|locale| locale.to_ascii_lowercase().starts_with("zh")),
        }
    }
}

/// Apply a language choice to every later `t`/`tr!` call.
pub fn set_language(language: Language) {
    ENGLISH.store(language.is_english(), Ordering::Relaxed);
}

pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn t(zh: &'static str, en: &'static str) -> &'static str {
    if is_english() { en } else { zh }
}

/// `format!` in the current language: `tr!("{n} 个", "{n} items")`.
#[macro_export]
macro_rules! tr {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_english() {
            format!($en $(, $arg)*)
        } else {
            format!($zh $(, $arg)*)
        }
    };
}
