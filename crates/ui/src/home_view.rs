use std::{ops::Range, path::PathBuf};

use gpui::*;
use latex::SystemBackendStatus;
use structs::assets::Assets;
use ui_cli_shared::doc_type::DocumentType;

use crate::{
    components::buttons::link_button,
    navbar_view::Navbar,
    state::window_state::WindowState,
    theme::{Theme, ThemeMode, ThemeSettings},
};

const SHOULD_PROMPT_ON_DELETE: bool = true;

fn sub_home_dir(raw: &std::path::Path) -> Option<PathBuf> {
    let home_dir = dirs::home_dir()?;
    raw.strip_prefix(&home_dir).ok().map(|p| {
        let mut pb = PathBuf::from("~");
        pb.push(p);
        pb
    })
}

pub struct HomeView {
    navbar: Entity<Navbar>,
    state: Entity<WindowState>,
    latex_backend_status: SystemBackendStatus,
}

impl HomeView {
    pub fn new(cx: &mut Context<HomeView>, state: Entity<WindowState>) -> Self {
        cx.observe(&state, |_this, _, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        let navbar = cx.new(|cx| Navbar::new(state.downgrade(), cx));
        let latex_backend_status = latex::system_backend_status();

        Self {
            navbar,
            state,
            latex_backend_status,
        }
    }

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

    fn missing_latex_tools(status: SystemBackendStatus) -> &'static str {
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

    fn render_latex_warning(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.latex_backend_status.is_available() {
            return None;
        }

        let theme = ThemeSettings::theme(cx);
        let missing = Self::missing_latex_tools(self.latex_backend_status);
        let message = format!(
            "Missing on PATH: {missing}. Monocurl will use a limited MathJax fallback for Tex(...); Text(...) and Latex(...) still require the system LaTeX toolchain."
        );
        let install_url = Self::latex_install_url();
        let (banner_bg, banner_border, accent) = Self::latex_warning_palette(theme);

        Some(
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
                                        .child("System LaTeX tools not found"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.text_muted)
                                        .child(message),
                                ),
                        ),
                )
                .child(
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
                        }),
                )
                .into_any_element(),
        )
    }

    fn open(
        &mut self,
        internal_path: std::path::PathBuf,
        user_path: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut App,
    ) {
        log::info!("Opening project {:?}", user_path);

        self.state.update(cx, move |state, cx| {
            state.navigate_to(
                user_path.clone(),
                internal_path.clone(),
                cx.entity(),
                window,
                cx,
            );
        });
    }

    fn import(&mut self, path: std::path::PathBuf, cx: &mut App) -> Result<(), String> {
        log::info!("Adding project {:?}", path);

        self.state.update(cx, move |state, _cx| state.import(path))
    }

    fn create_default(&mut self, dtype: DocumentType, window: &mut Window, cx: &mut App) {
        log::info!("Creating default {:?}", dtype);

        let path = self
            .state
            .update(cx, move |state, _cx| state.create_new_document(dtype));

        self.state.update(cx, move |state, cx| {
            state.navigate_to(None, path, cx.entity(), window, cx);
        });
    }

    fn forget(&mut self, internal_path: std::path::PathBuf, cx: &mut App) {
        log::info!("Forgetting project {:?}", internal_path);

        self.state.update(cx, move |state, _cx| {
            state.forget_project(&internal_path);
        });
    }

    fn render_logo(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);

        div()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .child(
                div().child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            img(Assets::image("monocurl-1024.png"))
                                .w(px(300.))
                                .h(px(300.))
                                .p_10(),
                        )
                        .child(div().child("Monocurl").text_2xl().text_color(gpui::white()))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .child(link_button(
                                    "Website",
                                    theme.link_text,
                                    cx.listener(|_, _, _, _| {
                                        let _ = open::that("https://monocurl.com");
                                    }),
                                ))
                                .child(link_button(
                                    "Source Code",
                                    theme.link_text,
                                    cx.listener(|_, _, _, _| {
                                        let _ = open::that("https://github.com/monocurl/monocurl");
                                    }),
                                ))
                                .child(link_button(
                                    "Discord",
                                    theme.link_text,
                                    cx.listener(|_, _, _, _| {
                                        let _ = open::that("https://discord.com/invite/7g94JR3SAD");
                                    }),
                                ))
                                .gap_3(),
                        )
                        .rounded(px(6.))
                        .bg(gpui::black())
                        .p_8()
                        .w(px(400.)),
                ),
            )
            .bg(theme.home_sidebar_background)
            .min_w(px(600.))
            .w(relative(0.5))
            .max_w(px(800.))
    }

    fn single_project(
        &self,
        internal_path: std::path::PathBuf,
        user_path: Option<std::path::PathBuf>,
        cx: &Context<HomeView>,
    ) -> impl IntoElement + use<> {
        let theme = ThemeSettings::theme(cx);
        let path = if let Some(ref path) = user_path {
            sub_home_dir(&path)
                .unwrap_or(path.to_path_buf())
                .to_string_lossy()
                .to_string()
        } else {
            "Untitled".to_string()
        };

        let path_for_open = user_path.clone();
        let path_for_remove = user_path.clone();

        let internal_path = internal_path;
        let internal_path_for_remove = internal_path.clone();

        let name = user_path
            .as_ref()
            .and_then(|f| f.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or("Untitled".to_string());

        let id: SharedString = format!("project {}", path).into();
        let group_name: SharedString = "project-group".into();

        div()
            .cursor_pointer()
            .w_full()
            .relative()
            .group(group_name.clone())
            .child(
                div()
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .size_full()
                    .bg(Rgba {
                        a: 0.0,
                        ..theme.row_hover_overlay
                    })
                    .group_hover(group_name.clone(), {
                        let overlay = theme.row_hover_overlay;
                        move |this| this.bg(overlay)
                    }),
            )
            .child(
                div()
                    .id(id)
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .p_2()
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.open(internal_path.clone(), path_for_open.clone(), window, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .child(name.clone())
                                    .text_size(px(13.0))
                                    .text_color(theme.text_primary),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(theme.text_muted)
                                    .child(path)
                                    .truncate(),
                            ),
                    )
                    .child(
                        div()
                            .id("close-btn")
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(20.))
                            .h(px(20.))
                            .rounded(px(3.))
                            .opacity(0.0)
                            .group_hover(group_name.clone(), |this| this.opacity(1.0))
                            .hover({
                                let danger = theme.danger;
                                move |this| this.text_color(danger)
                            })
                            .cursor_pointer()
                            .child(div().text_xs().child("×"))
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();

                                if SHOULD_PROMPT_ON_DELETE {
                                    let name = path_for_remove
                                        .as_ref()
                                        .and_then(|f| f.file_name())
                                        .map(|f| f.to_string_lossy().to_string())
                                        .unwrap_or("Untitled".into());
                                    let confirm = window.prompt(
                                        PromptLevel::Warning,
                                        &format!("Forget \"{}\"?", name),
                                        None,
                                        &[
                                            PromptButton::Cancel("Cancel".into()),
                                            PromptButton::Ok("Forget Project".into()),
                                        ],
                                        cx,
                                    );

                                    let path_copy = internal_path_for_remove.clone();
                                    cx.spawn(async move |this, app| {
                                        let Some(this) = this.upgrade() else {
                                            return;
                                        };

                                        if confirm.await == Ok(1) {
                                            let _ = app.update(move |cx| {
                                                let _ = this.update(cx, move |this, cx| {
                                                    this.forget(path_copy, cx);
                                                });
                                            });
                                        }
                                    })
                                    .detach();
                                } else {
                                    this.forget(internal_path_for_remove.clone(), cx);
                                }
                            })),
                    ),
            )
    }

    fn projects_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let projects = &self.state.read(cx).recently_opened;
        div()
            .text_sm()
            .size_full()
            .p_2()
            .overflow_hidden()
            .child(if projects.is_empty() {
                div()
                    .text_center()
                    .child("No recent projects")
                    .into_any_element()
            } else {
                uniform_list(
                    "project-list",
                    projects.len(),
                    cx.processor(move |this, range: Range<usize>, _, cx| {
                        this.state.read(cx).recently_opened[range]
                            .iter()
                            .map(|p| {
                                this.single_project(
                                    p.internal_path.clone(),
                                    p.user_path.clone(),
                                    cx,
                                )
                            })
                            .collect()
                    }),
                )
                .size_full()
                .pb_10()
                .into_any_element()
            })
    }

    fn render_projects(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let divider_color = theme.accent;

        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_center()
            .child(
                div()
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(div().child("Projects").text_xl().p_1())
                    .child(link_button(
                        "Import",
                        theme.link_text,
                        cx.listener(|_, _, _, cx| {
                            let options = PathPromptOptions {
                                files: true,
                                directories: false,
                                multiple: false,
                                prompt: None,
                            };
                            let path = cx.prompt_for_paths(options);

                            cx.spawn(async move |this, app| {
                                let Some(this) = this.upgrade() else {
                                    return;
                                };
                                let Some(path) = path
                                    .await
                                    .ok()
                                    .map(|s| s.ok())
                                    .flatten()
                                    .flatten()
                                    .map(|ps| ps.into_iter().next())
                                    .flatten()
                                else {
                                    return;
                                };

                                let _ = app.update(move |app| {
                                    let _ = this.update(app, |this, cx| {
                                        let _ = this.import(path, cx);
                                    });
                                });
                            })
                            .detach();
                        }),
                    ))
                    .child(link_button(
                        "New Scene",
                        theme.link_text,
                        cx.listener(move |this, _, window, cx| {
                            this.create_default(DocumentType::Scene, window, cx);
                        }),
                    ))
                    .child(link_button(
                        "New Library",
                        theme.link_text,
                        cx.listener(move |this, _, window, cx| {
                            this.create_default(DocumentType::Library, window, cx);
                        }),
                    ))
                    .gap_2()
                    .p_2(),
            )
            .child(div().h(px(0.5)).w_full().bg(divider_color))
            .child(self.projects_list(cx))
            .bg(theme.home_panel_background)
    }
}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let divider_color = theme.accent;

        div()
            .flex()
            .flex_col()
            .children(self.render_latex_warning(cx))
            .child(self.navbar.clone())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .child(self.render_logo(cx))
                    .child(div().w(px(0.5)).h_full().bg(divider_color))
                    .child(self.render_projects(cx)),
            )
            .bg(theme.app_background)
            .text_color(theme.text_primary)
            .size_full()
    }
}
