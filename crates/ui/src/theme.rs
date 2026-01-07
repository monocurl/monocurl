use gpui::{Font, Hsla, Pixels, Rgba, px, rgba};

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
    pub const MONOSPACE: &'static str = "IBMPlex Mono";
    pub const UI: &'static str = "Lilex";
}

#[derive(Clone)]
pub struct TextEditorStyles {
    pub bg_color: Hsla,

    pub text_font: Font,
    pub text_size: Pixels,
    pub line_height: Pixels,

    pub control_flow_color: Hsla,
    pub non_control_flow_keyword_color: Hsla,
    pub text_literal_color: Hsla,
    pub comment_color: Hsla,
    pub numeric_literal_color: Hsla,
    pub identifier_color: Hsla,
    pub operator_color: Hsla,
    pub punctuation_color: Hsla,
    pub default_text_color: Hsla,

    pub cursor_color: Hsla,

    pub gutter_font: Font,
    pub gutter_text_color: Hsla,
    pub gutter_active_color: Hsla,

    pub selection_color: Hsla,
    pub active_line_color: Hsla,

    pub scroll_color: Hsla,
}

impl Default for TextEditorStyles {
    fn default() -> Self {
        Self {
            bg_color: gpui::hsla(0.1, 0.1, 0.98, 1.0),

            text_font: gpui::font(FontSet::MONOSPACE),
            text_size: px(14.0),
            line_height: px(20.0),

            control_flow_color: gpui::hsla(0.58, 0.65, 0.35, 1.0),
            non_control_flow_keyword_color: gpui::hsla(0.0, 0.7, 0.35, 1.0),
            text_literal_color: gpui::hsla(0.15, 0.6, 0.45, 1.0),
            comment_color: gpui::hsla(0.1, 0.05, 0.5, 0.6),
            numeric_literal_color: gpui::hsla(0.08, 0.7, 0.5, 1.0),
            identifier_color: gpui::hsla(0.0, 0.0, 0.1, 1.0),
            operator_color: gpui::hsla(0.0, 0.0, 0.2, 1.0),
            punctuation_color: gpui::hsla(0.0, 0.0, 0.2, 1.0),
            default_text_color: gpui::hsla(0.0, 0.0, 0.1, 1.0),

            cursor_color: gpui::hsla(0.58, 0.9, 0.4, 1.0),

            gutter_font: gpui::font(FontSet::MONOSPACE),
            gutter_text_color: gpui::hsla(0.0, 0.0, 0.5, 1.0),
            gutter_active_color: gpui::hsla(0.58, 0.65, 0.35, 1.0),

            selection_color: gpui::hsla(0.58, 0.9, 0.9, 0.4),
            active_line_color: gpui::hsla(0.1, 0.1, 0.85, 0.2),
            scroll_color: gpui::hsla(0.0, 0.0, 0.3, 0.2),
        }
    }
}
