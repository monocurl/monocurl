#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    borrow::Cow,
    env, fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
    process,
};

use crate::{
    actions::{
        CheckForUpdates, Copy, Cut, EpsilonBackward, EpsilonForward, ExportImage,
        ExportSlidesAsVideos, ExportVideo, NextSlide, OpenKeyBindings, OpenSettings, Paste,
        PrevSlide, Quit, Redo, SaveActiveDocument, SaveActiveDocumentCustomPath, SceneEnd,
        SceneStart, ToggleHeadlessMode, TogglePlaying, TogglePresentationMode, ToggleSlideFold,
        Undo,
    },
    auto_update::AutoUpdater,
    editor::text_editor,
    i18n::Localization,
    keybindings_window::KeyBindingsWindow,
    settings_window::SettingsWindow,
    state::{user_settings::UserSettings, window_state::WindowState},
    theme::ThemeSettings,
    window::MonocurlWindow,
};
use gpui::*;
use structs::assets::Assets;

mod actions;
#[cfg(not(target_os = "macos"))]
mod app_menu_bar;
mod auto_update;
mod components;
mod document_view;
mod editor;
mod home_view;
mod i18n;
mod keybindings_window;
mod navbar_view;
mod services;
mod settings_window;
mod state;
mod theme;
mod timeline;
mod viewport;
mod window;

pub struct MonocurlLauncher;

struct MonocurlAssetSource;

impl MonocurlAssetSource {
    fn resolve(path: &str) -> Option<PathBuf> {
        let path = Path::new(path);
        if path.is_absolute() {
            return Some(path.to_path_buf());
        }

        let mut clean_path = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => clean_path.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }

        Some(Assets::asset(clean_path))
    }
}

impl AssetSource for MonocurlAssetSource {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let Some(path) = Self::resolve(path) else {
            return Ok(None);
        };

        match fs::read(path) {
            Ok(data) => Ok(Some(Cow::Owned(data))),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let Some(path) = Self::resolve(path) else {
            return Ok(Vec::new());
        };

        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        Ok(entries
            .filter_map(|entry| {
                entry
                    .ok()
                    .and_then(|entry| entry.file_name().into_string().ok())
                    .map(SharedString::from)
            })
            .collect())
    }
}

impl MonocurlLauncher {
    fn clean_latex_file_cache() {
        if let Err(error) = text::clean_stale_file_cache() {
            log::warn!("unable to clean stale LaTeX SVG cache: {error:#}");
        }
    }

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
        cx.on_action(|_: &OpenSettings, cx| SettingsWindow::open(cx));
        cx.on_action(|_: &OpenKeyBindings, cx| KeyBindingsWindow::open(cx));
        cx.on_action(|_: &CheckForUpdates, cx| {
            AutoUpdater::check_for_updates(cx.active_window(), cx);
        });
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    }

    pub(crate) fn setup_menus(cx: &mut App) {
        let text = |key| Localization::text(cx, key);
        cx.set_menus(vec![
            Menu {
                name: "Monocurl".into(),
                items: vec![
                    #[cfg(target_os = "macos")]
                    MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action(text("menu.app.quit"), Quit),
                ],
            },
            Menu {
                name: text("menu.file").into(),
                items: vec![
                    MenuItem::action(text("menu.file.settings"), OpenSettings),
                    MenuItem::separator(),
                    MenuItem::action(text("menu.file.save"), SaveActiveDocument),
                    MenuItem::action(text("menu.file.save_as"), SaveActiveDocumentCustomPath),
                    MenuItem::separator(),
                    MenuItem::action(text("menu.file.export_image"), ExportImage),
                    MenuItem::action(text("menu.file.export_video"), ExportVideo),
                    MenuItem::action(text("menu.file.export_slides"), ExportSlidesAsVideos),
                    MenuItem::separator(),
                    MenuItem::action(text("menu.file.present"), TogglePresentationMode),
                    MenuItem::action(text("menu.file.toggle_headless"), ToggleHeadlessMode),
                ],
            },
            Menu {
                name: text("menu.edit").into(),
                items: vec![
                    MenuItem::os_action(text("menu.edit.undo"), Undo, OsAction::Undo),
                    MenuItem::os_action(text("menu.edit.redo"), Redo, OsAction::Redo),
                    MenuItem::separator(),
                    MenuItem::os_action(text("menu.edit.cut"), Cut, OsAction::Cut),
                    MenuItem::os_action(text("menu.edit.copy"), Copy, OsAction::Copy),
                    MenuItem::os_action(text("menu.edit.paste"), Paste, OsAction::Paste),
                ],
            },
            Menu {
                name: text("menu.editor").into(),
                items: vec![
                    MenuItem::action(text("menu.editor.toggle_playing"), TogglePlaying),
                    MenuItem::action(text("menu.editor.epsilon_forward"), EpsilonForward),
                    MenuItem::action(text("menu.editor.epsilon_backward"), EpsilonBackward),
                    MenuItem::action(text("menu.editor.next_slide"), NextSlide),
                    MenuItem::action(text("menu.editor.previous_slide"), PrevSlide),
                    MenuItem::action(text("menu.editor.fold_slide"), ToggleSlideFold),
                    MenuItem::action(text("menu.editor.scene_start"), SceneStart),
                    MenuItem::action(text("menu.editor.scene_end"), SceneEnd),
                ],
            },
            Menu {
                name: text("menu.help").into(),
                items: vec![
                    MenuItem::action(text("menu.help.key_bindings"), OpenKeyBindings),
                    MenuItem::separator(),
                    MenuItem::action(text("menu.help.check_updates"), CheckForUpdates),
                ],
            },
        ]);
    }

    fn setup_modules(cx: &mut App) {
        components::prompt::init(cx);
        document_view::init(cx);
        text_editor::init(cx);
    }

    fn create_window(cx: &mut App) {
        let window_min_size = size(px(520.0), px(420.0));
        let window_bounds =
            WindowState::initial_window_bounds(window_min_size, size(px(1280.0), px(720.0)), cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Monocurl".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            window_min_size: Some(window_min_size),
            #[cfg(target_os = "linux")]
            // temporary linux csd preview
            window_decorations: Some(WindowDecorations::Client),
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
        Application::new()
            .with_assets(MonocurlAssetSource)
            .run(|cx: &mut App| {
                Self::clean_latex_file_cache();
                Self::setup_fonts(cx);
                ThemeSettings::init(cx);
                UserSettings::init(cx);
                Localization::init(cx);
                AutoUpdater::init(cx);
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

    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if !args.is_empty() {
        process::exit(cli::run(args, auto_update::CURRENT_VERSION));
    }

    MonocurlLauncher::launch();
}
