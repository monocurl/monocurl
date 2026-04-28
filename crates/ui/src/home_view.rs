use std::{ops::Range, path::PathBuf};

use gpui::*;
use structs::assets::Assets;
use ui_cli_shared::doc_type::DocumentType;

use crate::{
    components::{buttons::link_button, latex_warning::render_latex_warning},
    navbar_view::Navbar,
    state::{user_settings::UserSettings, window_state::WindowState},
    theme::ThemeSettings,
};

const SHOULD_PROMPT_ON_DELETE: bool = true;
const HOME_PROJECTS_TARGET_MIN_WIDTH: f32 = 420.0;
const HOME_LOGO_WIDE_FRACTION: f32 = 0.58;
const HOME_LOGO_MIN_EXPANDED_WIDTH: f32 = 520.0;
const HOME_LOGO_MAX_WIDTH: f32 = 960.0;
const HOME_LOGO_CARD_MAX_WIDTH: f32 = 430.0;

#[derive(Clone, Copy)]
struct LogoMetrics {
    sidebar_width: f32,
    divider_width: f32,
    card_width: f32,
    card_padding: f32,
    logo_size: f32,
    logo_padding: f32,
    title_size: f32,
    links_height: f32,
    links_opacity: f32,
}

impl LogoMetrics {
    fn for_window_width(window_width: Pixels) -> Self {
        let window_width = f32::from(window_width);
        let expanded_width = (window_width * HOME_LOGO_WIDE_FRACTION)
            .clamp(HOME_LOGO_MIN_EXPANDED_WIDTH, HOME_LOGO_MAX_WIDTH);
        let available_width = (window_width - HOME_PROJECTS_TARGET_MIN_WIDTH).max(0.0);
        let sidebar_width = expanded_width.min(available_width);
        let expanded_progress = (sidebar_width / HOME_LOGO_MIN_EXPANDED_WIDTH).clamp(0.0, 1.0);
        let card_width = (sidebar_width - 90.0).clamp(0.0, HOME_LOGO_CARD_MAX_WIDTH);
        let logo_size = (card_width - 92.0).clamp(0.0, 320.0);
        let logo_padding = (logo_size * 0.125).clamp(0.0, 40.0);
        let card_padding = (card_width * 0.074).clamp(0.0, 32.0);
        let title_size = 12.0 + 12.0 * expanded_progress;
        let links_progress = ((card_width - 300.0) / 130.0).clamp(0.0, 1.0);
        let divider_width = 0.5 * (sidebar_width / 80.0).clamp(0.0, 1.0);

        Self {
            sidebar_width,
            divider_width,
            card_width,
            card_padding,
            logo_size,
            logo_padding,
            title_size,
            links_height: 16.0 * links_progress,
            links_opacity: links_progress,
        }
    }
}

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
        cx.observe_global::<UserSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        let navbar = cx.new(|cx| Navbar::new(state.downgrade(), cx));

        Self { navbar, state }
    }

    fn open(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Opening project {:?}", path);

        self.state.update(cx, move |state, cx| {
            state.navigate_to(path.clone(), cx.entity(), window, cx);
        });
    }

    fn import(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) -> Result<(), String> {
        log::info!("Adding project {:?}", path);

        self.state.update(cx, move |state, _cx| state.import(path))
    }

    fn create_default(&mut self, dtype: DocumentType, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Creating default {:?}", dtype);

        let directory = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let name = format!("Untitled.{}", dtype.extension());
        let path = cx.prompt_for_new_path(&directory, Some(&name));
        let state = self.state.clone();

        window
            .spawn(cx, async move |window_cx| {
                let Some(path) = path.await.ok().map(|s| s.ok()).flatten().flatten() else {
                    return;
                };

                let _ = window_cx.update(move |window, cx| {
                    let _ = state.update(cx, |state, cx| {
                        if let Err(err) = state.create_new_document(dtype, path.clone()) {
                            log::error!("{err}");
                            return;
                        }
                        state.navigate_to(path, cx.entity(), window, cx);
                    });
                });
            })
            .detach();
    }

    fn forget(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        log::info!("Forgetting project {:?}", path);

        self.state.update(cx, move |state, _cx| {
            state.forget_project(&path);
        });
    }

    fn render_logo(&self, metrics: LogoMetrics, cx: &mut Context<Self>) -> AnyElement {
        let theme = ThemeSettings::theme(cx);

        div()
            .flex()
            .flex_none()
            .flex_col()
            .justify_center()
            .items_center()
            .overflow_hidden()
            .child(
                div().child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .overflow_hidden()
                        .child(
                            img(Assets::image("monocurl-1024.png"))
                                .w(px(metrics.logo_size))
                                .h(px(metrics.logo_size))
                                .p(px(metrics.logo_padding)),
                        )
                        .child(
                            div()
                                .child("Monocurl")
                                .text_size(px(metrics.title_size))
                                .text_color(gpui::white()),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .h(px(metrics.links_height))
                                .opacity(metrics.links_opacity)
                                .overflow_hidden()
                                .child(link_button(
                                    "Website",
                                    theme.link_text,
                                    cx.listener(|_, _, _, _| {
                                        let _ = open::that("https://monocurl.com");
                                    }),
                                ))
                                .child(link_button(
                                    "GitHub",
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
                        .p(px(metrics.card_padding))
                        .w(px(metrics.card_width))
                        .max_w(px(metrics.card_width))
                        .overflow_hidden(),
                ),
            )
            .bg(theme.home_sidebar_background)
            .min_w(px(0.0))
            .w(px(metrics.sidebar_width))
            .max_w(px(metrics.sidebar_width))
            .into_any_element()
    }

    fn single_project(
        &self,
        project_path: std::path::PathBuf,
        cx: &Context<HomeView>,
    ) -> impl IntoElement + use<> {
        let theme = ThemeSettings::theme(cx);
        let path = sub_home_dir(&project_path)
            .unwrap_or(project_path.to_path_buf())
            .to_string_lossy()
            .to_string();

        let path_for_open = project_path.clone();
        let path_for_remove = project_path.clone();

        let name = project_path
            .file_name()
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
                        this.open(path_for_open.clone(), window, cx);
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
                                        .file_name()
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

                                    let path_copy = path_for_remove.clone();
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
                                    this.forget(path_for_remove.clone(), cx);
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
                            .map(|p| this.single_project(p.path.clone(), cx))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let divider_color = theme.accent;
        let logo_metrics = LogoMetrics::for_window_width(window.bounds().size.width);

        let body = div()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(self.render_logo(logo_metrics, cx))
            .child(
                div()
                    .w(px(logo_metrics.divider_width))
                    .h_full()
                    .bg(divider_color),
            )
            .child(div().flex_1().min_w_0().child(self.render_projects(cx)));

        div()
            .flex()
            .flex_col()
            .children(render_latex_warning(UserSettings::read(cx), theme))
            .child(self.navbar.clone())
            .child(body)
            .bg(theme.app_background)
            .text_color(theme.text_primary)
            .size_full()
    }
}
