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
    pub text_color: Hsla,

    pub cursor_color: Hsla,

    pub gutter_font: Font,
    pub gutter_text_color: Hsla,
    pub gutter_active_color: Hsla,

    pub selection_color: Hsla,
    pub active_line_color: Hsla,
}

impl Default for TextEditorStyles {
    fn default() -> Self {
        Self {
            bg_color: rgba(0xf0eee7ff).into(),
            text_font: gpui::font(FontSet::MONOSPACE),
            text_size: px(14.0),
            line_height: px(20.0),
            text_color: gpui::hsla(0.0, 0.0, 0.1, 1.0),
            cursor_color: gpui::blue(),
            gutter_font: gpui::font(FontSet::MONOSPACE),
            gutter_text_color: gpui::hsla(0.0, 0.0, 0.3, 1.0),
            gutter_active_color: gpui::hsla(0.05, 0.0, 0.3, 1.0),
            selection_color: rgba(0x3311ff30).into(),
            active_line_color: rgba(0xffff0030).into(),
        }
    }
}
