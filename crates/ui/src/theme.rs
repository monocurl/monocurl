use gpui::{rgb, Rgba};

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
