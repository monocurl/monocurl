use std::path::PathBuf;

use gpui::{App, Font, Global, Hsla, Pixels, ReadGlobal, Rgba, UpdateGlobal, px};
use serde::{Deserialize, Serialize};

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
        std::fs::write(path, data).ok()
            .unwrap_or_else(|| {
                log::warn!("Unable to save theme settings")
            });
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
    pub timeline_divider: Rgba,
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
            app_background: rgba(0xEFF1F5),
            document_background: rgba(0x2B2E2F),
            viewport_background: rgba(0xDCE0E8),

            text_primary: rgba(0x4C4F69),
            text_muted: rgba(0x6C6F85),
            text_inverse: rgba(0xEFF1F5),
            link_text: rgba(0x1E66F5),
            danger: rgba(0xD20F39),

            accent: rgba(0x74C0FC),

            navbar_background: rgba(0xDDE0E7),
            navbar_border: rgba(0x9CA0B0),
            tab_background: rgba(0xCCD0DA),
            tab_active_background: rgba(0xEFF1F5),
            tab_close_hover_background: rgba(0xBCC0CC),

            home_sidebar_background: rgba(0xE6E9EF),
            home_panel_background: rgba(0xEFF1F5),
            row_hover_overlay: Rgba { a: 0.08, ..rgba(0x11111B) },

            split_divider: rgba(0x9CA0B0),

            timeline_background: hsla(0.61, 0.21, 0.87, 1.0),
            timeline_toolbar_background: rgba(0xE6E9EF),
            timeline_slide_background: rgba(0xCCD0DA),
            timeline_active_border: rgba(0x1E66F5),
            timeline_inactive_border: rgba(0x8C8FA1),
            timeline_connector: rgba(0xBCC0CC),
            timeline_tick: rgba(0x7C7F93),
            timeline_text: rgba(0x4C4F69),
            timeline_subtext: rgba(0x6C6F85),
            timeline_divider: rgba(0xDCE0E8),
            timeline_status_error: rgba(0xD20F39),
            timeline_status_ok: rgba(0x179299),
            timeline_playhead: rgba(0x007fff),
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            app_background: rgba(0x252525),
            document_background: rgba(0x252525),
            viewport_background: rgba(0x2D2D2D),

            text_primary: rgba(0xD4D4D4),
            text_muted: rgba(0x8A8A8A),
            text_inverse: rgba(0xFFFFFF),
            link_text: rgba(0x78AEDE),
            danger: rgba(0xE06C6C),

            accent: rgba(0x78AEDE),

            navbar_background: rgba(0x333435),
            navbar_border: rgba(0x3C3C3C),
            tab_background: rgba(0x333435),
            tab_active_background: rgba(0x252525),
            tab_close_hover_background: rgba(0x3A3A3A),

            home_sidebar_background: rgba(0x1E1E1E),
            home_panel_background: rgba(0x252525),
            row_hover_overlay: Rgba { a: 0.05, ..rgba(0xFFFFFF) },

            split_divider: rgba(0x3C3C3C),

            timeline_background: hsla(0.0, 0.0, 0.16, 1.0),
            timeline_toolbar_background: rgba(0x1E1E1E),
            timeline_slide_background: rgba(0x2E2E2E),
            timeline_active_border: rgba(0x78AEDE),
            timeline_inactive_border: rgba(0x555555),
            timeline_connector: rgba(0x606060),
            timeline_tick: rgba(0x606060),
            timeline_text: rgba(0xD4D4D4),
            timeline_subtext: rgba(0x8A8A8A),
            timeline_divider: rgba(0x3C3C3C),
            timeline_status_error: rgba(0xE06C6C),
            timeline_status_ok: rgba(0x6EC490),
            timeline_playhead: rgba(0xD4D4D4),
        }
    }

    pub fn text_editor_styles(self) -> TextEditorStyles {
        TextEditorStyles::for_mode(self.mode)
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
            bg_color: hsla(0.61, 0.23, 0.9, 1.0),
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
            selection_color: hsla(0.05, 0.44, 0.80, 0.3),
            active_line_color: hsla(0.61, 0.18, 0.83, 0.40),
            scroll_color: hsla(0.61, 0.13, 0.40, 0.30),
            scroll_background_color: hsla(0.61, 0.11, 0.74, 0.20),
            popover_background_color: rgba(0xE6E9EF),
            popover_border_color: rgba(0xCCD0DA),
            popover_shadow_color: hsla(0.0, 0.0, 0.0, 0.10),
            popover_title_color: rgba(0x11111B),
            popover_text_color: rgba(0x313244),
            popover_highlight_color: rgba(0x1E66F5),
            popover_selected_background_color: rgba(0xDCE0E8),
            popover_hover_background_color: rgba(0xCCD0DA),
            popover_active_argument_color: rgba(0x1E66F5),
            popover_inactive_argument_color: rgba(0x6C6F85),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_color: hsla(0.61, 0.091, 0.191, 1.0),
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
            runtime_error_color: hsla(0.96, 0.70, 0.75, 1.0),
            compile_time_error_color: hsla(0.96, 0.70, 0.75, 1.0),
            compile_time_warning_color: hsla(0.13, 0.58, 0.70, 1.0),
            cursor_color: hsla(0.00, 0.00, 0.84, 1.0),
            gutter_font: gpui::font(FontSet::MONOSPACE),
            gutter_text_color: hsla(0.0, 0.0, 0.44, 1.0),
            gutter_active_color: hsla(0.96, 0.70, 0.75, 1.0),
            selection_color: hsla(0.57, 0.38, 0.62, 0.28),
            active_line_color: hsla(0.0, 0.0, 0.18, 1.0),
            scroll_color: hsla(0.0, 0.0, 0.58, 0.32),
            scroll_background_color: hsla(0.0, 0.0, 0.22, 0.48),
            popover_background_color: rgba(0x1E1E1E),
            popover_border_color: rgba(0x3C3C3C),
            popover_shadow_color: hsla(0.0, 0.0, 0.0, 0.30),
            popover_title_color: rgba(0xD4D4D4),
            popover_text_color: rgba(0xBBBBBB),
            popover_highlight_color: rgba(0x78AEDE),
            popover_selected_background_color: rgba(0x2E2E2E),
            popover_hover_background_color: rgba(0x383838),
            popover_active_argument_color: rgba(0xB08AE0),
            popover_inactive_argument_color: rgba(0x8A8A8A),
        }
    }
}

impl Default for TextEditorStyles {
    fn default() -> Self {
        Self::for_mode(ThemeMode::default())
    }
}
