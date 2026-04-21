use std::path::PathBuf;

use gpui::{App, Font, Global, Hsla, Pixels, ReadGlobal, Rgba, UpdateGlobal, px};
use serde::{Deserialize, Serialize};

use crate::services::ExecutionStatus;

const fn rgba(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla { h, s, l, a }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

impl ThemeMode {
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::default(),
        }
    }
}

impl Global for ThemeSettings {}

impl ThemeSettings {
    fn save_file() -> PathBuf {
        let mut path = dirs::data_local_dir().expect("Could not find local data directory");
        path.push("Monocurl");
        if !path.exists() {
            std::fs::create_dir_all(&path).expect("Could not create settings directory");
        }
        path.push("theme.json");
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
        let data = serde_json::to_string_pretty(self).expect("Could not serialize theme settings");
        let path = Self::save_file();
        std::fs::write(path, data)
            .ok()
            .unwrap_or_else(|| log::warn!("Unable to save theme settings"));
    }

    pub fn init(cx: &mut App) {
        Self::set_global(cx, Self::load());
    }

    pub fn read(cx: &App) -> &Self {
        Self::global(cx)
    }

    pub fn theme(cx: &App) -> Theme {
        Theme::for_mode(Self::read(cx).mode)
    }

    pub fn toggle(cx: &mut App) {
        Self::update_global(cx, |settings, _cx| {
            settings.mode = settings.mode.toggled();
            settings.save();
        });
    }
}

pub struct FontSet;

impl FontSet {
    pub const MONOSPACE: &'static str = "IBM Plex Mono";
    pub const UI: &'static str = "Lilex";
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,
    pub app_background: Rgba,
    pub document_background: Rgba,
    pub viewport_background: Rgba,
    pub viewport_stage_background: Rgba,
    pub viewport_status_playing: Rgba,
    pub viewport_status_loading: Rgba,
    pub viewport_status_paused: Rgba,
    pub viewport_status_runtime_error: Rgba,
    pub viewport_status_compile_error: Rgba,

    pub text_primary: Rgba,
    pub text_muted: Rgba,
    pub text_inverse: Rgba,
    pub link_text: Rgba,
    pub danger: Rgba,

    pub accent: Rgba,

    pub navbar_background: Rgba,
    pub navbar_border: Rgba,
    pub tab_background: Rgba,
    pub tab_active_background: Rgba,
    pub tab_close_hover_background: Rgba,

    pub home_sidebar_background: Rgba,
    pub home_panel_background: Rgba,
    pub row_hover_overlay: Rgba,

    pub split_divider: Rgba,

    pub timeline_background: Hsla,
    pub timeline_toolbar_background: Rgba,
    pub timeline_slide_background: Rgba,
    pub timeline_active_border: Rgba,
    pub timeline_inactive_border: Rgba,
    pub timeline_connector: Rgba,
    pub timeline_tick: Rgba,
    pub timeline_text: Rgba,
    pub timeline_subtext: Rgba,
    pub timeline_status_error: Rgba,
    pub timeline_status_ok: Rgba,
    pub timeline_playhead: Rgba,
}

impl Theme {
    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            app_background: rgba(0xE8ECF2),
            document_background: rgba(0xF5F7FA),
            viewport_background: rgba(0xDDE2EB),
            viewport_stage_background: rgba(0xFAFBFD),
            viewport_status_playing: rgba(0x11111B),
            viewport_status_loading: rgba(0x1E66F5),
            viewport_status_paused: rgba(0x11111B),
            viewport_status_runtime_error: rgba(0x8839EF),
            viewport_status_compile_error: rgba(0xD20F39),

            text_primary: rgba(0x4C4F69),
            text_muted: rgba(0x6C6F85),
            text_inverse: rgba(0xEFF1F5),
            link_text: rgba(0x1E66F5),
            danger: rgba(0xD20F39),

            accent: rgba(0x1E66F5),

            navbar_background: rgba(0xD8DDE6),
            navbar_border: rgba(0x9CA0B0),
            tab_background: rgba(0xCDD3DE),
            tab_active_background: rgba(0xECEFF5),
            tab_close_hover_background: rgba(0xBCC0CC),

            home_sidebar_background: rgba(0xE1E6EE),
            home_panel_background: rgba(0xECEFF5),
            row_hover_overlay: Rgba {
                a: 0.08,
                ..rgba(0x11111B)
            },

            split_divider: rgba(0x9CA0B0),

            timeline_background: hsla(0.61, 0.21, 0.87, 1.0),
            timeline_toolbar_background: rgba(0xE1E6EE),
            timeline_slide_background: rgba(0xCDD3DE),
            timeline_active_border: rgba(0x1E66F5),
            timeline_inactive_border: rgba(0x8C8FA1),
            timeline_connector: rgba(0xBCC0CC),
            timeline_tick: rgba(0x7C7F93),
            timeline_text: rgba(0x4C4F69),
            timeline_subtext: rgba(0x6C6F85),
            timeline_status_error: rgba(0xD20F39),
            timeline_status_ok: rgba(0x179299),
            timeline_playhead: rgba(0x000000),
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            app_background: rgba(0x2B2C2E),
            document_background: rgba(0x2E2F31),
            viewport_background: rgba(0x66666B),
            viewport_stage_background: rgba(0xF2E9E4),
            viewport_status_playing: rgba(0xFFFFFF),
            viewport_status_loading: rgba(0x7DBEFF),
            viewport_status_paused: rgba(0xFFFFFF),
            viewport_status_runtime_error: rgba(0xC18FFF),
            viewport_status_compile_error: rgba(0xF07A7A),

            text_primary: rgba(0xECECF0),
            text_muted: rgba(0xB7B8BE),
            text_inverse: rgba(0xFFFFFF),
            link_text: rgba(0x5AA8FF),
            danger: rgba(0xF07A7A),

            accent: rgba(0xE3C318),

            navbar_background: rgba(0x343538),
            navbar_border: rgba(0x64666B),
            tab_background: rgba(0x303134),
            tab_active_background: rgba(0x3A3B3E),
            tab_close_hover_background: rgba(0x4A4B50),

            home_sidebar_background: rgba(0x000000),
            home_panel_background: rgba(0x191A1C),
            row_hover_overlay: Rgba {
                a: 0.06,
                ..rgba(0xFFFFFF)
            },

            split_divider: rgba(0x131416),

            timeline_background: hsla(0.0, 0.0, 0.18, 1.0),
            timeline_toolbar_background: rgba(0x000000),
            timeline_slide_background: rgba(0x2F3032),
            timeline_active_border: rgba(0xE3C318),
            timeline_inactive_border: rgba(0xD1B11A),
            timeline_connector: rgba(0xC0C1C8),
            timeline_tick: rgba(0xC0C1C8),
            timeline_text: rgba(0xF3F3F6),
            timeline_subtext: rgba(0xC8C9CF),
            timeline_status_error: rgba(0xF07A7A),
            timeline_status_ok: rgba(0x7AD7A4),
            timeline_playhead: rgba(0xECECF1),
        }
    }

    pub fn text_editor_styles(self) -> TextEditorStyles {
        TextEditorStyles::for_mode(self.mode)
    }

    pub fn viewport_status_ring(self, status: ExecutionStatus) -> Rgba {
        match status {
            ExecutionStatus::Playing => self.viewport_status_playing,
            ExecutionStatus::Paused => self.viewport_status_paused,
            ExecutionStatus::Seeking => self.viewport_status_loading,
            ExecutionStatus::RuntimeError => self.viewport_status_runtime_error,
            ExecutionStatus::CompileError => self.viewport_status_compile_error,
        }
    }
}

#[derive(Clone)]
pub struct TextEditorStyles {
    pub bg_color: Hsla,

    pub text_font: Font,
    pub italic_text_font: Font,
    pub text_size: Pixels,
    pub line_height: Pixels,

    pub control_flow_color: Hsla,
    pub non_control_flow_color: Hsla,
    pub text_literal_color: Hsla,
    pub comment_color: Hsla,
    pub numeric_literal_color: Hsla,
    pub argument_label_color: Hsla,
    pub identifier_color: Hsla,
    pub operator_color: Hsla,
    pub punctuation_color: Hsla,
    pub default_text_color: Hsla,

    pub runtime_error_color: Hsla,
    pub compile_time_error_color: Hsla,
    pub compile_time_warning_color: Hsla,

    pub cursor_color: Hsla,

    pub gutter_font: Font,
    pub gutter_text_color: Hsla,
    pub gutter_active_color: Hsla,

    pub selection_color: Hsla,
    pub active_line_color: Hsla,

    pub scroll_color: Hsla,
    pub scroll_background_color: Hsla,

    pub popover_background_color: Rgba,
    pub popover_border_color: Rgba,
    pub popover_shadow_color: Hsla,
    pub popover_title_color: Rgba,
    pub popover_text_color: Rgba,
    pub popover_highlight_color: Rgba,
    pub popover_selected_background_color: Rgba,
    pub popover_hover_background_color: Rgba,
    pub popover_active_argument_color: Rgba,
    pub popover_inactive_argument_color: Rgba,
}

impl TextEditorStyles {
    pub fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        }
    }

    pub fn light() -> Self {
        Self {
            bg_color: hsla(0.61, 0.16, 0.97, 1.0),
            text_font: gpui::font(FontSet::MONOSPACE),
            italic_text_font: gpui::font(FontSet::MONOSPACE).italic(),

            text_size: px(14.0),
            line_height: px(20.0),
            control_flow_color: hsla(0.76, 0.59, 0.52, 1.0),
            non_control_flow_color: hsla(0.98, 0.62, 0.47, 1.0),
            comment_color: hsla(0.61, 0.13, 0.49, 0.65),
            text_literal_color: hsla(0.36, 0.29, 0.44, 1.0),
            numeric_literal_color: hsla(0.07, 0.99, 0.45, 1.0),
            identifier_color: hsla(0.61, 0.16, 0.23, 1.0),
            argument_label_color: hsla(0.61, 0.91, 0.54, 1.0),
            operator_color: hsla(0.54, 0.59, 0.45, 1.0),
            punctuation_color: hsla(0.61, 0.13, 0.40, 1.0),
            default_text_color: hsla(0.61, 0.16, 0.23, 1.0),
            runtime_error_color: hsla(0.85, 0.76, 0.56, 1.0),
            compile_time_error_color: hsla(0.01, 0.76, 0.56, 1.0),
            compile_time_warning_color: hsla(0.13, 0.91, 0.62, 1.0),
            cursor_color: hsla(0.03, 0.59, 0.65, 1.0),
            gutter_font: gpui::font(FontSet::MONOSPACE),
            gutter_text_color: hsla(0.61, 0.13, 0.49, 1.0),
            gutter_active_color: hsla(0.0, 0.59, 0.54, 1.0),
            selection_color: hsla(0.05, 0.40, 0.76, 0.24),
            active_line_color: hsla(0.61, 0.13, 0.91, 1.0),
            scroll_color: hsla(0.61, 0.13, 0.40, 0.30),
            scroll_background_color: hsla(0.61, 0.10, 0.82, 0.34),
            popover_background_color: rgba(0xECEFF5),
            popover_border_color: rgba(0xD6DBE5),
            popover_shadow_color: hsla(0.0, 0.0, 0.0, 0.10),
            popover_title_color: rgba(0x11111B),
            popover_text_color: rgba(0x313244),
            popover_highlight_color: rgba(0x1E66F5),
            popover_selected_background_color: rgba(0xDEE4EC),
            popover_hover_background_color: rgba(0xD1D7E2),
            popover_active_argument_color: rgba(0x1E66F5),
            popover_inactive_argument_color: rgba(0x6C6F85),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_color: hsla(0.0, 0.0, 0.22, 1.0),
            text_font: gpui::font(FontSet::MONOSPACE),
            italic_text_font: gpui::font(FontSet::MONOSPACE).italic(),

            text_size: px(14.0),
            line_height: px(20.0),
            control_flow_color: hsla(0.96, 0.70, 0.75, 1.0),
            non_control_flow_color: hsla(0.75, 0.62, 0.74, 1.0),
            comment_color: hsla(0.0, 0.0, 0.46, 1.0),
            text_literal_color: hsla(0.36, 0.46, 0.66, 1.0),
            numeric_literal_color: hsla(0.52, 0.62, 0.73, 1.0),
            identifier_color: hsla(0.00, 0.00, 0.84, 1.0),
            argument_label_color: hsla(0.54, 0.65, 0.73, 1.0),
            operator_color: hsla(0.00, 0.00, 0.84, 1.0),
            punctuation_color: hsla(0.00, 0.00, 0.68, 1.0),
            default_text_color: hsla(0.00, 0.00, 0.84, 1.0),
            runtime_error_color: hsla(0.96, 0.82, 0.74, 1.0),
            compile_time_error_color: hsla(0.96, 0.82, 0.74, 1.0),
            compile_time_warning_color: hsla(0.13, 0.58, 0.70, 1.0),
            cursor_color: hsla(0.00, 0.00, 0.84, 1.0),
            gutter_font: gpui::font(FontSet::MONOSPACE),
            gutter_text_color: hsla(0.0, 0.0, 0.56, 1.0),
            gutter_active_color: hsla(0.96, 0.70, 0.75, 1.0),
            selection_color: hsla(0.59, 0.55, 0.64, 0.16),
            active_line_color: hsla(0.0, 0.0, 0.25, 1.0),
            scroll_color: hsla(0.0, 0.0, 0.76, 0.24),
            scroll_background_color: hsla(0.0, 0.0, 0.18, 0.68),
            popover_background_color: rgba(0x323336),
            popover_border_color: rgba(0x54565C),
            popover_shadow_color: hsla(0.0, 0.0, 0.0, 0.42),
            popover_title_color: rgba(0xD8D8DE),
            popover_text_color: rgba(0xC2C2C9),
            popover_highlight_color: rgba(0x7DBEFF),
            popover_selected_background_color: rgba(0x3B3C40),
            popover_hover_background_color: rgba(0x424347),
            popover_active_argument_color: rgba(0xC18FFF),
            popover_inactive_argument_color: rgba(0x8C8A96),
        }
    }
}

impl Default for TextEditorStyles {
    fn default() -> Self {
        Self::for_mode(ThemeMode::default())
    }
}
