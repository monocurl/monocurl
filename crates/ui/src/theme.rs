use gpui::{Font, Hsla, Pixels, Rgba, px};

pub struct ColorSet;

impl ColorSet {

    pub const BLUE: Rgba = Rgba {
        r: 90.0 / 255.0,
        g: 134.0 / 255.0,
        b: 238.0 / 255.0,
        a: 1.0
    };

    pub const PURPLE: Rgba = Rgba {
        r: 135.0 / 255.0,
        g: 116.0 / 255.0,
        b: 144.0 / 255.0,
        a:  1.0
    };

    pub const SIDE_PANEL_GRAY: Rgba = Rgba {
        r: 230.0 / 255.0,
        g: 233.0 / 255.0,
        b: 238.0 / 255.0,
        a: 1.0
    };

    pub const TOOLBAR_GRAY: Rgba = Rgba {
        r: 211.0 / 255.0,
        g: 215.0 / 255.0,
        b: 225.0 / 255.0,
        a: 1.0
    };

    pub const SUPER_LIGHT_GRAY: Rgba = Rgba {
        r: 239.0 / 255.0,
        g: 241.0 / 255.0,
        b: 245.0 / 255.0,
        a: 1.0
    };

    pub const LIGHT_GRAY: Rgba = Rgba {
        r: 221.0 / 255.0,
        g: 224.0 / 255.0,
        b: 231.0 / 255.0,
        a: 1.0
    };

    pub const GRAY: Rgba = Rgba {
        r: 85.0 / 255.0,
        g: 85.0 / 255.0,
        b: 85.0 / 255.0,
        a: 1.0
    };

    pub const DARK_GRAY: Rgba = Rgba {
        r: 43.0 / 255.0,
        g: 46.0 / 255.0,
        b: 47.0 / 255.0,
        a: 1.0
    };

    pub const SUPER_DARK_GRAY: Rgba = Rgba {
        r: 20.0 / 255.0,
        g: 20.0 / 255.0,
        b: 20.0 / 255.0,
        a: 1.0
    };

}

pub struct FontSet;

impl FontSet {
    pub const MONOSPACE: &'static str = "IBM Plex Mono";
    pub const UI: &'static str = "Lilex";
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
}

impl Default for TextEditorStyles {
    fn default() -> Self {
        Self {
            bg_color: gpui::hsla(0.61, 0.23, 0.9, 1.0),
            text_font: gpui::font(FontSet::MONOSPACE),
            italic_text_font: gpui::font(FontSet::MONOSPACE).italic(),

            text_size: px(14.0),
            line_height: px(20.0),
            control_flow_color: gpui::hsla(0.76, 0.59, 0.52, 1.0),
            non_control_flow_color: gpui::hsla(0.98, 0.62, 0.47, 1.0),
            comment_color: gpui::hsla(0.61, 0.13, 0.49, 0.65),
            text_literal_color: gpui::hsla(0.36, 0.29, 0.44, 1.0),
            numeric_literal_color: gpui::hsla(0.07, 0.99, 0.45, 1.0),
            identifier_color: gpui::hsla(0.61, 0.16, 0.23, 1.0),
            argument_label_color: gpui::hsla(0.61, 0.91, 0.54, 1.0),
            operator_color: gpui::hsla(0.54, 0.59, 0.45, 1.0),
            punctuation_color: gpui::hsla(0.61, 0.13, 0.40, 1.0),
            default_text_color: gpui::hsla(0.61, 0.16, 0.23, 1.0),
            runtime_error_color: gpui::hsla(0.85, 0.76, 0.56, 1.0),
            compile_time_error_color: gpui::hsla(0.01, 0.76, 0.56, 1.0),
            compile_time_warning_color: gpui::hsla(0.13, 0.91, 0.62, 1.0),
            cursor_color: gpui::hsla(0.03, 0.59, 0.65, 1.0),
            gutter_font: gpui::font(FontSet::MONOSPACE),
            gutter_text_color: gpui::hsla(0.61, 0.13, 0.49, 1.0),
            gutter_active_color: gpui::hsla(0.0, 0.59, 0.54, 1.0),
            selection_color: gpui::hsla(0.05, 0.44, 0.80, 0.3),
            active_line_color: gpui::hsla(0.61, 0.18, 0.83, 0.40),
            scroll_color: gpui::hsla(0.61, 0.13, 0.40, 0.30),
            scroll_background_color: gpui::hsla(0.61, 0.11, 0.74, 0.20),
        }
    }
}
