use gpui::Rgba;

pub struct ColorSet;

impl ColorSet {

    pub const BLUE: Rgba = Rgba {
        r: 90.0 / 255.0,
        g: 134.0 / 255.0,
        b: 238.0 / 255.0,
        a: 1.0
    };

    pub const PURPLE: Rgba = Rgba {
        r: 35.0 / 255.0,
        g: 16.0 / 255.0,
        b: 44.0 / 255.0,
        a:  1.0
    };

    pub const LIGHT_GRAY: Rgba = Rgba {
        r: 180.0 / 255.0,
        g: 180.0 / 255.0,
        b: 180.0 / 255.0,
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
