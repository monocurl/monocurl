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


pub struct TextEditorStyles {
    pub bg_color: Hsla,

    pub text_font: Font,
    pub text_size: Pixels,
    pub text_color: Hsla,

    pub gutter_font: Font,

    pub selection_color: Rgba,
    pub active_line_color: Rgba,
}

impl Default for TextEditorStyles {
    fn default() -> Self {
        Self {
            bg_color: gpui::white(),
            text_font: gpui::font(FontSet::MONOSPACE),
            text_size: px(14.0),
            text_color: gpui::black(),
            gutter_font: gpui::font(FontSet::MONOSPACE),
            selection_color: rgba(0x3311ff30),
            active_line_color: rgba(0xeeeeee55),
        }
    }
}
