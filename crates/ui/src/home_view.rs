use std::{ops::Range, path::PathBuf};

use gpui::*;
use ui_cli_shared::doc_type::DocumentType;
use structs::assets::Assets;

use crate::{components::buttons::link_button, navbar_view::Navbar, state::window_state::WindowState, theme::ColorSet};

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
    state: Entity<WindowState>
}

impl HomeView {
    pub fn new(cx: &mut Context<HomeView>, state: Entity<WindowState>) -> Self {
        let navbar = cx.new(|cx| {
            Navbar::new(state.downgrade(), cx)
        });

        Self {
            navbar,
            state
        }
    }

    fn open(&mut self, internal_path: std::path::PathBuf, user_path: Option<std::path::PathBuf>, window: &mut Window, cx: &mut App) {
        log::info!("Opening project {:?}", user_path);

        self.state.update(cx, move |state, cx| {
            state.navigate_to(user_path.clone(), internal_path.clone(), cx.entity(), window, cx);
        });
    }

    fn import(&mut self, path: std::path::PathBuf, cx: &mut App) -> Result<(), String> {
        log::info!("Adding project {:?}", path);

        self.state.update(cx, move |state, _cx| {
            state.import(path)
        })
    }

    fn create_default(&mut self, dtype: DocumentType, window: &mut Window, cx: &mut App) {
        log::info!("Creating default {:?}", dtype);

        let path = self.state.update(cx, move |state, _cx| {
            state.create_new_document(dtype)
        });

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
        div()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .child(
                div()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                img(Assets::image("monocurl-1024.png"))
                                    .w(px(300.))
                                    .h(px(300.))
                                    .p_10()
                            )
                            .child(
                                div()
                                    .child("Monocurl")
                                    .text_2xl()
                                    .text_color(white())
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .child(link_button("Website", cx.listener(|_, _, _, _| {
                                        let _ = open::that("https://monocurl.com");
                                    })))
                                    .child(link_button("Source Code", cx.listener(|_, _, _, _| {
                                        let _ =open::that("https://github.com/monocurl/monocurl");
                                    })))
                                    .child(link_button("Discord", cx.listener(|_, _, _, _| {
                                        let _ = open::that("https://discord.com/invite/7g94JR3SAD");
                                    })))
                                    .gap_3()
                            )
                            .rounded(px(6.))
                            .bg(black())
                            .p_8()
                            .w(px(400.))
                    )
            )
            .bg(ColorSet::SIDE_PANEL_GRAY)
            .min_w(px(500.))
            .w(relative(0.5))
            .max_w(px(600.))
    }

    fn single_project(&self, internal_path: std::path::PathBuf, user_path: Option<std::path::PathBuf>, cx: &Context<HomeView>) -> impl IntoElement + use<> {
        let path = if let Some(ref path) = user_path {
            sub_home_dir(&path)
            .unwrap_or(path.to_path_buf())
            .to_string_lossy().to_string()
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
                    .bg(black().opacity(0.0))
                    .group_hover(group_name.clone(), |this| this.bg(black().opacity(0.1)))
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
                                    .text_sm()
                                    .text_color(gpui::black())
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(ColorSet::GRAY)
                                    .child(path)
                                    .truncate()
                            )
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
                            .hover(|this| this.text_color(gpui::red()))
                            .cursor_pointer()
                            .child(
                                div()
                                    .text_xs()
                                    .child("×")
                            )
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
                                        &[PromptButton::Cancel("Cancel".into()), PromptButton::Ok("Forget Project".into())],
                                        cx);

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
                                    }).detach();
                                }
                                else {
                                    this.forget(internal_path_for_remove.clone(), cx);
                                }
                            }))
                    )
            )
    }

    fn projects_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let projects = &self.state.read(cx).recently_opened;
        div()
            .text_sm()
            .size_full()
            .p_2()
            .overflow_hidden()
            .child(
                if projects.is_empty() {
                    div()
                        .text_center()
                        .child("No recent projects")
                        .into_any_element()
                } else {
                    uniform_list(
                        "project-list",
                        projects.len(),
                        cx.processor(move |this, range: Range<usize>, _, cx| {
                            this.state.read(cx)
                                .recently_opened[range]
                                .iter()
                                .map(|p| this.single_project(p.internal_path.clone(), p.user_path.clone(), cx))
                                .collect()
                        })
                    )
                    .size_full()
                    .pb_10()
                    .into_any_element()
                }
            )
    }

    fn render_projects(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(
                        div()
                            .child("Projects")
                            .text_xl()
                            .p_1()
                    )
                    .child(
                        link_button("Import", cx.listener(|_, _, _, cx| {
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
                                let Some(path) = path.await
                                    .ok().map(|s| s.ok())
                                    .flatten().flatten()
                                    .map(|ps| ps.into_iter().next())
                                    .flatten() else {
                                    return;
                                };

                                let _ = app.update(move |app| {
                                    let _ = this.update(app, |this, cx| {
                                        let _ = this.import(path, cx);
                                    });
                                });
                            }).detach();
                        }))
                    )
                    .child(
                        link_button("New Scene", cx.listener(move |this, _, window, cx| {
                            this.create_default(DocumentType::Scene, window, cx);
                        }))
                    )
                    .child(
                        link_button("New Library", cx.listener(move |this, _, window, cx| {
                            this.create_default(DocumentType::Library, window, cx);
                        }))
                    )
                    .gap_2()
                    .p_2()
            )
            .child(
                div()
                    .h(px(2.))
                    .w_full()
                    .bg(ColorSet::PURPLE)
            )
            .child(self.projects_list(cx))
            .bg(ColorSet::SUPER_LIGHT_GRAY)
    }
}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_col()
            .child(
                self.navbar.clone()
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .child(self.render_logo(cx))
                    .child(
                        div()
                            .w(px(4.))
                            .h_full()
                            .bg(ColorSet::PURPLE)
                    )
                    .child(self.render_projects(cx))
                    .size_full()
            )
            .text_color(gpui::black())
            .size_full()
    }
}
