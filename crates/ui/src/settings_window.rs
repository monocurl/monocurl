use std::path::{Path, PathBuf};

use gpui::*;

use crate::{
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
        let window_size = size(px(420.0), px(280.0));
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
                title: Some("Settings".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(window_size, cx)),
            window_min_size: Some(window_size),
            is_resizable: false,
            focus: true,
            ..Default::default()
        };
        if let Ok(handle) =
            cx.open_window(options, |_window, cx| cx.new(|cx| SettingsWindow::new(cx)))
        {
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

        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    fn backend_button(
        &self,
        label: &'static str,
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
            .id(format!("latex-backend-{label}"))
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
        let status = path.map_or(
            "Not set",
            |path| {
                if path.is_file() { "Ready" } else { "Missing" }
            },
        );
        let path_text = path
            .map(|path| compact_path(path))
            .unwrap_or_else(|| "No binary selected".into());

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
                    .id(format!("choose-system-binary-{}", binary.label()))
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
                    .child("Choose...")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.choose_binary(binary, window, cx);
                    })),
            )
            .into_any_element()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let settings = UserSettings::read(cx).clone();
        let use_system = settings.latex_backend == LatexBackendPreference::System;

        div()
            .font_family(FontSet::UI)
            .size_full()
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
                            .gap(px(3.0))
                            .child(div().text_size(px(20.0)).child("Settings"))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child("Some settings may require a restart to take effect."),
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
                                    .child("LaTeX backend"),
                            )
                            .child(div().flex().flex_row().gap(px(8.0)).children([
                                self.backend_button(
                                    "Bundled",
                                    !use_system,
                                    LatexBackendPreference::Bundled,
                                    cx,
                                ),
                                self.backend_button(
                                    "System latex + dvisvgm",
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
            )
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
