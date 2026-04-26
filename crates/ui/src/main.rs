use std::borrow::Cow;

use crate::{
    actions::{
        Copy, Cut, EpsilonBackward, EpsilonForward, ExportImage, ExportVideo, NextSlide, Paste,
        PrevSlide, Quit, Redo, SaveActiveDocument, SaveActiveDocumentCustomPath, SceneEnd,
        SceneStart, ToggleHeadlessMode, TogglePlaying, TogglePresentationMode, Undo,
    },
    editor::text_editor,
    theme::ThemeSettings,
    window::MonocurlWindow,
};
use gpui::*;
use structs::assets::Assets;

mod actions;
mod components;
mod document_view;
mod editor;
mod home_view;
mod navbar_view;
mod services;
mod state;
mod theme;
mod timeline;
mod viewport;
mod window;

pub struct MonocurlLauncher;

impl MonocurlLauncher {
    fn setup_fonts(cx: &mut App) {
        cx.text_system()
            .add_fonts(vec![
                Cow::Owned(std::fs::read(Assets::font("IBMPlexMono-Regular.ttf")).unwrap()),
                Cow::Owned(std::fs::read(Assets::font("IBMPlexMono-Italic.ttf")).unwrap()),
            ])
            .unwrap();

        cx.text_system()
            .add_fonts(vec![Cow::Owned(
                std::fs::read(Assets::font("Lilex-Regular.ttf")).unwrap(),
            )])
            .unwrap();
    }

    fn setup_global_actions(cx: &mut App) {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    }

    fn setup_menus(cx: &mut App) {
        cx.set_menus(vec![
            Menu {
                name: "Monocurl".into(),
                items: vec![
                    #[cfg(target_os = "macos")]
                    MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit Monocurl", Quit),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Save", SaveActiveDocument),
                    MenuItem::action("Save As", SaveActiveDocumentCustomPath),
                    MenuItem::separator(),
                    MenuItem::action("Export as Image", ExportImage),
                    MenuItem::action("Export as Video", ExportVideo),
                    MenuItem::separator(),
                    MenuItem::action("Present", TogglePresentationMode),
                    MenuItem::action("Toggle Headless Mode", ToggleHeadlessMode),
                ],
            },
            Menu {
                name: "Edit".into(),
                items: vec![
                    MenuItem::os_action("Undo", Undo, OsAction::Undo),
                    MenuItem::os_action("Redo", Redo, OsAction::Redo),
                    MenuItem::separator(),
                    MenuItem::os_action("Cut", Cut, OsAction::Cut),
                    MenuItem::os_action("Copy", Copy, OsAction::Copy),
                    MenuItem::os_action("Paste", Paste, OsAction::Paste),
                ],
            },
            Menu {
                name: "Editor".into(),
                items: vec![
                    MenuItem::action("Toggle Playing", TogglePlaying),
                    MenuItem::action("Epsilon Forward", EpsilonForward),
                    MenuItem::action("Epsilon Backward", EpsilonBackward),
                    MenuItem::action("Next Slide", NextSlide),
                    MenuItem::action("Previous Slide", PrevSlide),
                    MenuItem::action("Scene Start", SceneStart),
                    MenuItem::action("Scene End", SceneEnd),
                ],
            },
            Menu {
                name: "Help".into(),
                items: vec![],
            },
        ]);
    }

    fn setup_modules(cx: &mut App) {
        document_view::init(cx);
        text_editor::init(cx);
    }

    fn create_window(cx: &mut App) {
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Monocurl".into()),
                ..Default::default()
            }),
            window_min_size: Some(size(px(520.), px(420.))),
            focus: true,
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            cx.new(|cx| MonocurlWindow::new(window, cx))
        })
        .unwrap();

        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    }

    fn launch() {
        Application::new().run(|cx: &mut App| {
            Self::setup_fonts(cx);
            ThemeSettings::init(cx);
            Self::setup_modules(cx);
            Self::setup_global_actions(cx);
            Self::setup_menus(cx);
            Self::create_window(cx);
        });
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    MonocurlLauncher::launch();
}
