use std::path::PathBuf;

use gpui::{App, Global, ReadGlobal, UpdateGlobal};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatexBackendPreference {
    #[default]
    Bundled,
    System,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UserSettings {
    pub latex_backend: LatexBackendPreference,
    pub system_latex_path: Option<PathBuf>,
    pub system_dvisvgm_path: Option<PathBuf>,
}

impl Global for UserSettings {}

impl UserSettings {
    fn save_file() -> PathBuf {
        let mut path = dirs::data_local_dir().expect("Could not find local data directory");
        path.push("Monocurl");
        if !path.exists() {
            std::fs::create_dir_all(&path).expect("Could not create settings directory");
        }
        path.push("settings.json");
        path
    }

    pub fn load() -> Self {
        let path = Self::save_file();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str(&data).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let data = serde_json::to_string_pretty(self).expect("Could not serialize user settings");
        let path = Self::save_file();
        std::fs::write(path, data)
            .ok()
            .unwrap_or_else(|| log::warn!("Unable to save user settings"));
    }

    pub fn init(cx: &mut App) {
        let settings = Self::load();
        settings.apply_latex_backend();
        Self::set_global(cx, settings);
    }

    pub fn read(cx: &App) -> &Self {
        Self::global(cx)
    }

    pub fn update(cx: &mut App, update: impl FnOnce(&mut Self)) {
        Self::update_global(cx, |settings, _cx| {
            update(settings);
            settings.save();
            settings.apply_latex_backend();
        });
    }

    pub fn use_bundled_latex(&mut self) {
        self.latex_backend = LatexBackendPreference::Bundled;
    }

    pub fn use_system_latex(&mut self) {
        self.latex_backend = LatexBackendPreference::System;
        self.populate_missing_system_paths();
    }

    pub fn populate_missing_system_paths(&mut self) {
        let discovered = latex::discover_system_backend();
        if self.system_latex_path.is_none() {
            self.system_latex_path = discovered.latex;
        }
        if self.system_dvisvgm_path.is_none() {
            self.system_dvisvgm_path = discovered.dvisvgm;
        }
    }

    pub fn system_backend_config(&self) -> Option<latex::SystemBackendConfig> {
        Some(latex::SystemBackendConfig {
            latex: self.system_latex_path.clone()?,
            dvisvgm: self.system_dvisvgm_path.clone()?,
        })
    }

    pub fn apply_latex_backend(&self) {
        match self.latex_backend {
            LatexBackendPreference::Bundled => {
                latex::set_backend_config(latex::LatexBackendConfig::Bundled);
            }
            LatexBackendPreference::System => {
                if let Some(config) = self.system_backend_config() {
                    latex::set_backend_config(latex::LatexBackendConfig::System(config));
                } else {
                    latex::set_backend_config(latex::LatexBackendConfig::Bundled);
                }
            }
        }
    }
}
