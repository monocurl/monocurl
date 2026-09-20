use std::path::{Path, PathBuf};

use gpui::*;

use crate::{
    i18n::{Language, Localization},
    state::user_settings::{LatexBackendPreference, UserSettings},
    theme::{FontSet, ThemeSettings},
};

#[derive(Default)]
struct SettingsWindowHandle {
    window: Option<AnyWindowHandle>,
}

impl Global for SettingsWindowHandle {}

#[derive(Clone, Copy)]
enum SystemBinary {
    Latex,
    Dvisvgm,
}

impl SystemBinary {
    fn label(self) -> &'static str {
        match self {
            Self::Latex => "latex",
            Self::Dvisvgm => "dvisvgm",
        }
    }
}

pub struct SettingsWindow {
    focus_handle: FocusHandle,
}

impl SettingsWindow {
    pub fn open(cx: &mut App) {
        let window_size = size(px(420.0), px(430.0));
        if !cx.has_global::<SettingsWindowHandle>() {
            cx.set_global(SettingsWindowHandle::default());
        }

        let existing = cx.global::<SettingsWindowHandle>().window;
        if let Some(handle) = existing
            && handle
                .update(cx, |_, window, _cx| window.activate_window())
                .is_ok()
        {
            return;
        }

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(Localization::text(cx, "settings.title").into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(window_size, cx)),
            window_min_size: Some(window_size),
            is_resizable: false,
            #[cfg(target_os = "linux")]
            window_decorations: Some(WindowDecorations::Client),
            focus: true,
            ..Default::default()
        };
        if let Ok(handle) = cx.open_window(options, |_window, cx| cx.new(SettingsWindow::new)) {
            cx.update_global::<SettingsWindowHandle, _>(|settings, _cx| {
                settings.window = Some(handle.into());
            });
        }
    }

    fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<UserSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<Localization>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    fn language_button(&self, language: Language, active: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = ThemeSettings::theme(cx);
        let bg = if active {
            theme.accent
        } else {
            theme.tab_active_background
        };
        let text = if active {
            theme.viewport_stage_background
        } else {
            theme.text_primary
        };

        div()
            .id(ElementId::Name(format!("settings-language-{}", language.code()).into()))
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(if active {
                theme.accent
            } else {
                theme.navbar_border
            })
            .bg(bg)
            .text_size(px(12.0))
            .text_color(text)
            .cursor_pointer()
            .hover(|style| style.opacity(0.92))
            .child(language.native_name())
            .on_click(move |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                UserSettings::update(cx, |settings| {
                    settings.set_language(language);
                });
            })
            .into_any_element()
    }

    fn backend_button(
        &self,
        label: String,
        active: bool,
        preference: LatexBackendPreference,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = ThemeSettings::theme(cx);
        let bg = if active {
            theme.accent
        } else {
            theme.tab_active_background
        };
        let text = if active {
            theme.viewport_stage_background
        } else {
            theme.text_primary
        };

        div()
            .id(ElementId::Name(format!("latex-backend-{label}").into()))
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(if active {
                theme.accent
            } else {
                theme.navbar_border
            })
            .bg(bg)
            .text_size(px(12.0))
            .text_color(text)
            .cursor_pointer()
            .hover(|style| style.opacity(0.92))
            .child(label)
            .on_click(move |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                UserSettings::update(cx, |settings| match preference {
                    LatexBackendPreference::Bundled => settings.use_bundled_latex(),
                    LatexBackendPreference::System => settings.use_system_latex(),
                });
            })
            .into_any_element()
    }

    fn choose_binary(
        &mut self,
        binary: SystemBinary,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(format!("Choose {}", binary.label()).into()),
        };
        let path = cx.prompt_for_paths(options);

        cx.spawn(async move |_this, app| {
            let Some(path) = path
                .await
                .ok()
                .and_then(|result| result.ok())
                .flatten()
                .and_then(|paths| paths.into_iter().next())
            else {
                return;
            };

            let _ = app.update(move |app| {
                UserSettings::update(app, |settings| match binary {
                    SystemBinary::Latex => settings.system_latex_path = Some(path.clone()),
                    SystemBinary::Dvisvgm => settings.system_dvisvgm_path = Some(path.clone()),
                });
            });
        })
        .detach();
    }

    fn path_row(
        &self,
        binary: SystemBinary,
        path: Option<&PathBuf>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = ThemeSettings::theme(cx);
        let status = path.map_or_else(
            || Localization::text(cx, "settings.not_set"),
            |path| {
                if path.is_file() {
                    Localization::text(cx, "settings.ready")
                } else {
                    Localization::text(cx, "settings.missing")
                }
            },
        );
        let path_text = path
            .map(|path| compact_path(path))
            .unwrap_or_else(|| Localization::text(cx, "settings.no_binary"));

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .opacity(if enabled { 1.0 } else { 0.55 })
            .child(
                div()
                    .w(px(72.0))
                    .text_size(px(12.0))
                    .text_color(theme.text_primary)
                    .child(binary.label()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_primary)
                            .overflow_hidden()
                            .child(path_text),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .child(status),
                    ),
            )
            .child(
                div()
                    .id(ElementId::Name(
                        format!("choose-system-binary-{}", binary.label()).into(),
                    ))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(theme.navbar_border)
                    .text_size(px(11.0))
                    .text_color(theme.text_primary)
                    .cursor_pointer()
                    .hover({
                        let hover = theme.row_hover_overlay;
                        move |style| style.bg(hover)
                    })
                    .child(Localization::text(cx, "settings.choose"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.choose_binary(binary, window, cx);
                    })),
            )
            .into_any_element()
    }

    fn auto_update_toggle(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = ThemeSettings::theme(cx);
        let next_enabled = !enabled;
        let track = div()
            .id("auto-update-toggle")
            .w(px(38.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(if enabled {
                theme.accent
            } else {
                theme.navbar_border
            })
            .bg(if enabled {
                theme.accent
            } else {
                theme.tab_active_background
            })
            .flex()
            .items_center()
            .cursor_pointer()
            .hover(|style| style.opacity(0.92));
        let track = if enabled {
            track.justify_end()
        } else {
            track.justify_start()
        };

        track
            .child(
                div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded(px(7.0))
                    .bg(theme.viewport_stage_background),
            )
            .on_click(move |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                UserSettings::update(cx, |settings| {
                    settings.auto_update = next_enabled;
                });
            })
            .into_any_element()
    }
}

impl SettingsWindow {
    #[cfg(target_os = "linux")]
    fn render_linux_titlebar(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let theme = ThemeSettings::theme(cx);
        let controls = window.window_controls();

        div()
            .id("settings-titlebar")
            .h(px(28.0))
            .flex_none()
            .flex()
            .items_center()
            .bg(theme.navbar_background)
            .border_b_1()
            .border_color(theme.navbar_border)
            .child(
                div()
                    .id("settings-titlebar-drag")
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Localization::text(cx, "settings.title"))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_move();
                    })
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        if event.click_count() == 2 && controls.maximize {
                            window.zoom_window();
                        }
                    }),
            )
            .child(
                div()
                    .id("settings-window-close")
                    .w(px(42.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(18.0))
                    .cursor_pointer()
                    .hover({
                        let danger = theme.danger;
                        move |this| this.bg(danger)
                    })
                    .child("×")
                    .on_click(|_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        window.remove_window();
                    }),
            )
            .into_any_element()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(not(target_os = "linux"))]
        let _ = window;

        let theme = ThemeSettings::theme(cx);
        let settings = UserSettings::read(cx).clone();
        let use_system = settings.latex_backend == LatexBackendPreference::System;

        let content = div()
            .font_family(FontSet::UI)
            .flex_1()
            .min_h_0()
            .bg(theme.app_background)
            .text_color(theme.text_primary)
            .key_context("settings")
            .track_focus(&self.focus_handle)
            .p(px(18.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(9.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(Localization::text(cx, "settings.language")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .child(Localization::text(cx, "settings.language.description")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .gap(px(8.0))
                                    .pt(px(2.0))
                                    .children(
                                        Language::ALL
                                            .into_iter()
                                            .filter(|language| Localization::is_available(*language))
                                            .map(|language| {
                                                self.language_button(
                                                    language,
                                                    language == settings.language,
                                                    cx,
                                                )
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(9.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(Localization::text(cx, "settings.updates")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(theme.text_primary)
                                                    .child(Localization::text(
                                                        cx,
                                                        "settings.auto_update",
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(theme.text_muted)
                                                    .child(if settings.auto_update {
                                                        Localization::text(cx, "settings.enabled")
                                                    } else {
                                                        Localization::text(cx, "settings.disabled")
                                                    }),
                                            ),
                                    )
                                    .child(self.auto_update_toggle(settings.auto_update, cx)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .child(Localization::text(cx, "settings.title")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(Localization::text(cx, "settings.restart_note")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(Localization::text(cx, "settings.latex_backend")),
                            )
                            .child(div().flex().flex_row().gap(px(8.0)).children([
                                self.backend_button(
                                    Localization::text(cx, "settings.bundled"),
                                    !use_system,
                                    LatexBackendPreference::Bundled,
                                    cx,
                                ),
                                self.backend_button(
                                    Localization::text(cx, "settings.system_latex"),
                                    use_system,
                                    LatexBackendPreference::System,
                                    cx,
                                ),
                            ]))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .pt(px(6.0))
                                    .child(self.path_row(
                                        SystemBinary::Latex,
                                        settings.system_latex_path.as_ref(),
                                        use_system,
                                        cx,
                                    ))
                                    .child(self.path_row(
                                        SystemBinary::Dvisvgm,
                                        settings.system_dvisvgm_path.as_ref(),
                                        use_system,
                                        cx,
                                    )),
                            ),
                    ),
            );

        #[cfg(target_os = "linux")]
        {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_linux_titlebar(window, cx))
                .child(content);
        }

        #[cfg(not(target_os = "linux"))]
        content
    }
}

fn compact_path(path: &Path) -> String {
    let Some(home) = dirs::home_dir() else {
        return path.display().to_string();
    };
    path.strip_prefix(&home).map_or_else(
        |_| path.display().to_string(),
        |suffix| {
            let mut out = PathBuf::from("~");
            out.push(suffix);
            out.display().to_string()
        },
    )
}
