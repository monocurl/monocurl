use quarve::core::{MSlock, StandardConstEnv};
use quarve::prelude::{Menu, WindowMenu};

pub fn menu(env: &StandardConstEnv, s: MSlock) -> WindowMenu {
    WindowMenu::standard(
        env,
        Menu::new("File"),
        Menu::new("Edit"),
        Menu::new("View"),
        Menu::new("Help"),
        s
    )
}
