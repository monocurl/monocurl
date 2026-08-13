use std::{collections::BTreeMap, sync::OnceLock};

use gpui::{App, Global, ReadGlobal, UpdateGlobal};
use serde::{Deserialize, Serialize};

use crate::state::user_settings::UserSettings;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    English,
    Spanish,
    French,
    German,
    Japanese,
    Chinese,
}

impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Spanish,
        Self::French,
        Self::German,
        Self::Japanese,
        Self::Chinese,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
            Self::French => "fr",
            Self::German => "de",
            Self::Japanese => "ja",
            Self::Chinese => "zh",
        }
    }

    pub const fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Spanish => "Español",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Japanese => "日本語",
            Self::Chinese => "简体中文",
        }
    }
}

pub struct Localization {
    language: Language,
}

impl Global for Localization {}

impl Localization {
    pub fn init(cx: &mut App) {
        Self::set_global(
            cx,
            Self {
                language: UserSettings::read(cx).language,
            },
        );
    }

    pub fn language(cx: &App) -> Language {
        Self::global(cx).language
    }

    pub fn set_language(cx: &mut App, language: Language) {
        Self::update_global(cx, |localization, _cx| {
            localization.language = language;
        });
        crate::MonocurlLauncher::setup_menus(cx);
    }

    pub fn text(cx: &App, key: &str) -> String {
        let language = Self::language(cx);
        let catalogs = catalogs();
        catalogs
            .get(&language)
            .and_then(|catalog| catalog.get(key))
            .or_else(|| {
                catalogs
                    .get(&Language::English)
                    .and_then(|catalog| catalog.get(key))
            })
            .cloned()
            .unwrap_or_else(|| {
                log::warn!("missing localization key: {key}");
                key.to_string()
            })
    }

    pub fn is_available(language: Language) -> bool {
        language == Language::English || !catalogs()[&language].is_empty()
    }
}

fn catalogs() -> &'static BTreeMap<Language, BTreeMap<String, String>> {
    static CATALOGS: OnceLock<BTreeMap<Language, BTreeMap<String, String>>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        [
            (Language::English, include_str!("i18n/en.json")),
            (Language::Spanish, include_str!("i18n/es.json")),
            (Language::French, include_str!("i18n/fr.json")),
            (Language::German, include_str!("i18n/de.json")),
            (Language::Japanese, include_str!("i18n/ja.json")),
            (Language::Chinese, include_str!("i18n/zh.json")),
        ]
        .into_iter()
        .map(|(language, source)| {
            let catalog = serde_json::from_str(source)
                .unwrap_or_else(|error| panic!("invalid {} catalog: {error}", language.code()));
            (language, catalog)
        })
        .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_catalog_has_menu_labels() {
        let catalog = &catalogs()[&Language::English];
        assert!(catalog.contains_key("menu.file"));
        assert!(catalog.contains_key("settings.language"));
    }

    #[test]
    fn translated_catalogs_are_available_in_the_picker() {
        assert!(Localization::is_available(Language::Spanish));
    }
}
