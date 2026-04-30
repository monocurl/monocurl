use gpui::*;

use crate::{
    state::user_settings::{LatexBackendPreference, UserSettings},
    theme::{Theme, ThemeMode},
};

fn latex_install_url() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "https://tug.org/mactex/"
    }

    #[cfg(target_os = "windows")]
    {
        "https://miktex.org/download"
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        "https://www.latex-project.org/get/"
    }
}

fn missing_latex_tools(status: latex::SystemBackendStatus) -> &'static str {
    match (status.latex, status.dvisvgm) {
        (true, true) => "",
        (false, true) => "latex",
        (true, false) => "dvisvgm",
        (false, false) => "latex and dvisvgm",
    }
}

fn latex_warning_palette(theme: Theme) -> (Rgba, Rgba, Rgba) {
    match theme.mode {
        ThemeMode::Light => (
            Rgba {
                r: 1.0,
                g: 0.97,
                b: 0.88,
                a: 1.0,
            },
            Rgba {
                r: 0.83,
                g: 0.67,
                b: 0.16,
                a: 1.0,
            },
            Rgba {
                r: 0.92,
                g: 0.74,
                b: 0.14,
                a: 1.0,
            },
        ),
        ThemeMode::Dark => (
            Rgba {
                r: 0.20,
                g: 0.16,
                b: 0.05,
                a: 1.0,
            },
            Rgba {
                r: 0.76,
                g: 0.58,
                b: 0.12,
                a: 1.0,
            },
            Rgba {
                r: 0.94,
                g: 0.76,
                b: 0.22,
                a: 1.0,
            },
        ),
    }
}

pub fn render_latex_warning(settings: &UserSettings, theme: Theme) -> Option<AnyElement> {
    if settings.latex_backend == LatexBackendPreference::Bundled {
        return None;
    }

    let Some(config) = settings.system_backend_config() else {
        return Some(render_warning_banner(
            "System LaTeX paths not set".to_string(),
            "System backend is enabled, but latex and dvisvgm paths are incomplete. Monocurl will use the bundled backend until both paths are set.".to_string(),
            false,
            theme,
        ));
    };
    let status = latex::system_backend_status(&config);
    if status.is_available() {
        return None;
    }
    let missing = missing_latex_tools(status);
    Some(render_warning_banner(
        "System LaTeX tools not available".to_string(),
        format!(
            "Configured system backend is missing or cannot start: {missing}. Choose valid latex and dvisvgm binaries in Settings."
        ),
        true,
        theme,
    ))
}

fn render_warning_banner(
    title: String,
    message: String,
    show_install: bool,
    theme: Theme,
) -> AnyElement {
    let install_url = latex_install_url();
    let (banner_bg, banner_border, accent) = latex_warning_palette(theme);

    div()
        .w_full()
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .gap(px(16.0))
        .px(px(14.0))
        .py(px(10.0))
        .border_b(px(1.0))
        .border_color(banner_border)
        .bg(banner_bg)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .items_start()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(18.0))
                        .h(px(18.0))
                        .mt(px(1.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(accent)
                        .text_color(gpui::black())
                        .font_weight(FontWeight::BOLD)
                        .child("!"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_primary)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child(message),
                        ),
                ),
        )
        .children(show_install.then(|| {
            div()
                .id("install-latex-link")
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(banner_border)
                .bg(accent)
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(gpui::black())
                .hover(|style| style.opacity(0.92))
                .cursor_pointer()
                .child("Install LaTeX")
                .on_click(move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let _ = open::that(install_url);
                })
                .into_any_element()
        }))
        .into_any_element()
}
