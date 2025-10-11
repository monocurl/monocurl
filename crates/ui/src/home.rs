use std::{ops::Range, path::PathBuf};

use gpui::*;
use log::{log, Level};
use server::doc_type::DocumentType;
use structs::assets::Assets;

use crate::{state::{ActiveScreen, WindowState}, theme::ColorSet, util::link_button};

const SHOULD_PROMPT_ON_DELETE: bool = false;

fn sub_home_dir(raw: &std::path::Path) -> Option<PathBuf> {
    let home_dir = dirs::home_dir()?;
    raw.strip_prefix(&home_dir).ok().map(|p| {
        let mut pb = PathBuf::from("~");
        pb.push(p);
        pb
    })
}

pub struct HomeView {
    state: Entity<WindowState>
}

impl HomeView {
    pub fn new(_cx: &mut Context<HomeView>, state: Entity<WindowState>) -> Self {
        Self {
            state
        }
    }

    fn open(&mut self, path: std::path::PathBuf, cx: &mut App) {
        log::info!("Opening project {:?}", path);

        self.state.update(cx, move |state, cx| {
            state.navigate_to(path.clone(), cx.entity(), cx);
        });
    }

    fn add(&mut self, path: std::path::PathBuf, cx: &mut App) -> Result<(), String> {
        log::info!("Adding project {:?}", path);

        self.state.update(cx, move |state, _cx| {
            state.add(path)
        })
    }

    fn create_default(&mut self, path: std::path::PathBuf, dtype: DocumentType, _cx: &mut App) {
        log::info!("Creating default {:?} at {:?}", dtype, path);

        let content = dtype.default_file();
        let _ = std::fs::write(&path, content);
    }

    fn delete(&mut self, path: std::path::PathBuf, cx: &mut App) {
        log::info!("Deleting project {:?}", path);

        self.state.update(cx, move |state, _cx| {
            state.remove(path);
        });
    }

    fn render_logo(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .child(
                img(Assets::image("monocurl-1024.png"))
                    .w(px(400.))
                    .h(px(400.))
                    .p_10()
            )
            .child(
                div()
                    .child("Monocurl")
                    .text_2xl()
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
            .min_w(px(500.))
            .w(relative(0.5))
            .max_w(px(600.))
            .pt_32()
    }

    fn single_project(&self, raw_path: &std::path::Path, cx: &Context<Self>) -> impl IntoElement {
        let path = sub_home_dir(raw_path)
            .unwrap_or(raw_path.to_path_buf())
            .to_string_lossy().to_string();
        let path_for_open = raw_path.to_path_buf();
        let path_for_remove = raw_path.to_path_buf();

        let name = raw_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or("<unknown>".to_string());

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
                    .group_hover(group_name.clone(), |this| this.bg(black().opacity(0.3)))
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
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.open(path_for_open.clone(), cx);
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
                                    .text_color(gpui::white())
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(ColorSet::LIGHT_GRAY)
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
                                    let confirm = window.prompt(
                                        PromptLevel::Warning,
                                        &format!("Forget \"{}\"?", path_for_remove.file_name().map(
                                            |f| f.to_string_lossy()).unwrap_or("Unknown".into())),
                                        Some("This action will not delete any files on your disk."),
                                        &[PromptButton::Cancel("Cancel".into()), PromptButton::Ok("Forget Project".into())],
                                        cx);

                                    let path_copy = path_for_remove.clone();
                                    cx.spawn(async move |this, app| {
                                        let Some(this) = this.upgrade() else {
                                            return;
                                        };

                                        if confirm.await == Ok(1) {
                                            let _ = app.update(move |cx| {
                                                let _ = this.update(cx, move |this, cx| {
                                                    this.delete(path_copy, cx);
                                                });
                                            });
                                        }
                                    }).detach();
                                }
                                else {
                                    this.delete(path_for_remove.clone(), cx);
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
                            let projects = &this.state.read(cx).recently_opened;

                            projects[range]
                                .iter()
                                .map(|p| this.single_project(p, cx))
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
        fn new_file(dtype: DocumentType, cx: &Context<HomeView>) {
            let dir = dirs::home_dir().unwrap_or_default();
            let path = cx.prompt_for_new_path(dir.as_ref(), Some(&("Untitled.".to_string() + dtype.extension())));

            cx.spawn(async move |this, app| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let Ok(Ok(Some(path))) = path.await else {
                    return;
                };

                let _ = app.update(move |cx| {
                    let _ = this.update(cx, move |this, cx| {
                        this.create_default(path.clone(), dtype, cx);
                        let _ = this.add(path, cx);
                    });
                });
            }).detach();
        };

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
                                        let _ = this.add(path, cx);
                                    });
                                });
                            }).detach();
                        }))
                    )
                    .child(
                        link_button("New Scene", cx.listener(move |_, _, _, cx| {
                            new_file(DocumentType::Scene, cx);
                        }))
                    )
                    .child(
                        link_button("New Library", cx.listener(move |_, _, _, cx| {
                            new_file(DocumentType::Library, cx);
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
            .bg(ColorSet::SUPER_DARK_GRAY)
    }
}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .text_color(gpui::white())
            .size_full()
    }
}
