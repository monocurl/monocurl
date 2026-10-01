use std::{
    path::{Path, PathBuf},
};

use gpui::*;
use structs::assets::Assets;
use ui_cli_shared::doc_type::DocumentType;

use crate::{
    auto_update::{AutoUpdateStatus, AutoUpdater, CURRENT_VERSION},
    components::{buttons::link_button, latex_warning::render_latex_warning},
    navbar_view::Navbar,
    state::{
        user_settings::UserSettings,
        window_state::{ActiveScreen, WindowState},
    },
    theme::{FontSet, ThemeSettings},
    thumbnails::{THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH, Thumbnails},
};

const SHOULD_PROMPT_ON_DELETE: bool = true;
const HOME_PROJECTS_TARGET_MIN_WIDTH: f32 = 420.0;
const HOME_LOGO_WIDE_FRACTION: f32 = 0.58;
const HOME_LOGO_MIN_EXPANDED_WIDTH: f32 = 520.0;
const HOME_LOGO_MAX_WIDTH: f32 = 960.0;
const HOME_LOGO_CARD_MAX_WIDTH: f32 = 430.0;
const PROJECT_THUMBNAIL_WIDTH: f32 = 96.0;
const PROJECT_THUMBNAIL_HEIGHT: f32 =
    PROJECT_THUMBNAIL_WIDTH * THUMBNAIL_HEIGHT as f32 / THUMBNAIL_WIDTH as f32;

#[derive(Clone, Copy)]
struct LogoMetrics {
    panel_width: f32,
    divider_width: f32,
    card_width: f32,
    card_padding: f32,
    logo_size: f32,
    logo_padding: f32,
    title_size: f32,
    links_height: f32,
    links_opacity: f32,
}

#[derive(Clone, Copy)]
enum UpdateStatusAction {
    Restart,
    Retry,
}

impl LogoMetrics {
    fn for_window_width(window_width: Pixels) -> Self {
        let window_width = f32::from(window_width);
        let expanded_width = (window_width * HOME_LOGO_WIDE_FRACTION)
            .clamp(HOME_LOGO_MIN_EXPANDED_WIDTH, HOME_LOGO_MAX_WIDTH);
        let available_width = (window_width - HOME_PROJECTS_TARGET_MIN_WIDTH).max(0.0);
        let panel_width = expanded_width.min(available_width);
        let expanded_progress = (panel_width / HOME_LOGO_MIN_EXPANDED_WIDTH).clamp(0.0, 1.0);
        let card_width = (panel_width - 90.0).clamp(0.0, HOME_LOGO_CARD_MAX_WIDTH);
        let logo_size = (card_width - 92.0).clamp(0.0, 320.0);
        let logo_padding = (logo_size * 0.125).clamp(0.0, 40.0);
        let card_padding = (card_width * 0.074).clamp(0.0, 32.0);
        let title_size = 12.0 + 12.0 * expanded_progress;
        let links_progress = ((card_width - 300.0) / 130.0).clamp(0.0, 1.0);
        let divider_width = 0.5 * (panel_width / 80.0).clamp(0.0, 1.0);

        Self {
            panel_width,
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

/// the folder line under a project name: bundled examples say so, home
/// paths start with `~`, and long paths lose leading folders rather than
/// their tail, which is the part that tells projects apart
fn display_folder(raw: &std::path::Path) -> String {
    const MAX_CHARS: usize = 48;
    if raw.starts_with(Assets::default_scene("")) {
        return "Built-in example".to_string();
    }
    let folder = raw.parent().unwrap_or(raw);
    let folder = sub_home_dir(folder).unwrap_or_else(|| folder.to_path_buf());
    let mut parts: Vec<String> = folder
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .filter(|c| c != "/")
        .collect();
    let joined = |parts: &[String]| parts.join("/");
    let mut shortened = false;
    while parts.len() > 2 && joined(&parts).len() + 2 > MAX_CHARS {
        parts.remove(0);
        shortened = true;
    }
    match (shortened, folder.has_root()) {
        (true, _) => format!("…/{}", joined(&parts)),
        (false, true) => format!("/{}", joined(&parts)),
        (false, false) => joined(&parts),
    }
}

pub struct HomeView {
    navbar: Entity<Navbar>,
    state: Entity<WindowState>,
    thumbnails: Option<Entity<Thumbnails>>,
    /// projects whose thumbnails were last requested while home was showing
    thumbnails_requested: Option<Vec<PathBuf>>,
}

impl HomeView {
    pub fn new(cx: &mut Context<HomeView>, state: Entity<WindowState>) -> Self {
        cx.observe(&state, |this, _, cx| {
            this.refresh_thumbnails(cx);
            cx.notify();
        })
        .detach();
        let thumbnails = Thumbnails::get(cx);
        if let Some(thumbnails) = &thumbnails {
            cx.observe(thumbnails, |_this, _, cx| cx.notify()).detach();
        }
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<UserSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();
        if let Some(updater) = AutoUpdater::get(cx) {
            cx.observe(&updater, |_this, _, cx| {
                cx.notify();
            })
            .detach();
        }

        let navbar = cx.new(|cx| Navbar::new(state.downgrade(), cx));

        let mut this = Self {
            navbar,
            state,
            thumbnails,
            thumbnails_requested: None,
        };
        this.refresh_thumbnails(cx);
        this
    }

    /// queues missing or stale thumbnails whenever home is shown or its list changes
    fn refresh_thumbnails(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !matches!(state.screen, ActiveScreen::Home) {
            self.thumbnails_requested = None;
            return;
        }

        let projects: Vec<_> = state
            .recently_opened
            .iter()
            .map(|recent| recent.path.clone())
            .collect();
        if self.thumbnails_requested.as_ref() == Some(&projects) {
            return;
        }
        if let Some(thumbnails) = &self.thumbnails {
            thumbnails.update(cx, |thumbnails, cx| {
                thumbnails.refresh(projects.iter().map(PathBuf::as_path), cx);
            });
        }
        self.thumbnails_requested = Some(projects);
    }

    fn render_update_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = ThemeSettings::theme(cx);
        let status = AutoUpdater::status(cx);

        let (label, action_label, action) = match status {
            AutoUpdateStatus::Idle => return None,
            AutoUpdateStatus::Checking => ("Checking for updates".to_string(), None, None),
            AutoUpdateStatus::Downloading { version } => {
                (format!("Downloading v{version}"), None, None)
            }
            AutoUpdateStatus::Installing { version } => {
                (format!("Installing v{version}"), None, None)
            }
            AutoUpdateStatus::ReadyToRestart { version } => (
                format!("Update v{version} ready"),
                Some("Restart"),
                Some(UpdateStatusAction::Restart),
            ),
            AutoUpdateStatus::Errored { .. } => (
                "Update failed".to_string(),
                Some("Retry"),
                Some(UpdateStatusAction::Retry),
            ),
        };

        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap(px(7.0))
            .max_w(px(240.0))
            .text_size(px(11.0))
            .line_height(px(14.0))
            .text_color(theme.text_muted)
            .child(div().truncate().child(label));

        let row = if let (Some(action_label), Some(action)) = (action_label, action) {
            row.child(
                div()
                    .id(ElementId::Name(
                        format!("update-status-{action_label}").into(),
                    ))
                    .text_color(theme.link_text)
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.9))
                    .child(action_label)
                    .on_click(move |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        match action {
                            UpdateStatusAction::Restart => {
                                AutoUpdater::restart_from_update_status(cx)
                            }
                            UpdateStatusAction::Retry => {
                                AutoUpdater::check_for_updates(Some(window.window_handle()), cx);
                            }
                        }
                    }),
            )
        } else {
            row
        };

        Some(row.into_any_element())
    }

    fn open(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Opening project {:?}", path);

        self.state.update(cx, move |state, cx| {
            state.navigate_to(path.clone(), cx.entity(), window, cx);
        });
    }

    fn import_many(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        log::info!("Adding projects {:?}", paths);

        self.state.update(cx, move |state, cx| {
            let result = state.import_many(paths);
            cx.notify();
            result
        })
    }

    fn create_default(&mut self, dtype: DocumentType, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("Creating default {:?}", dtype);

        let directory = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let name = format!("Untitled.{}", dtype.extension());
        let path = cx.prompt_for_new_path(&directory, Some(&name));
        let state = self.state.clone();

        window
            .spawn(cx, async move |window_cx| {
                let Some(path) = path.await.ok().and_then(|s| s.ok()).flatten() else {
                    return;
                };

                let _ = window_cx.update(move |window, cx| {
                    state.update(cx, |state, cx| {
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

        self.state.update(cx, move |state, cx| {
            state.forget_project(&path);
            cx.notify();
        });
    }

    fn render_logo(&self, metrics: LogoMetrics, cx: &mut Context<Self>) -> AnyElement {
        let theme = ThemeSettings::theme(cx);
        let version = format!("v{CURRENT_VERSION}");

        div()
            .flex()
            .flex_none()
            .flex_col()
            .justify_center()
            .items_center()
            .relative()
            .overflow_hidden()
            .child(
                div().child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .overflow_hidden()
                        .child(
                            img(Assets::image("monocurl.png"))
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
            .child(
                div()
                    .absolute()
                    .right(px(18.0))
                    .bottom(px(14.0))
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(3.0))
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .text_color(theme.text_muted)
                    .opacity(metrics.links_opacity)
                    .children(self.render_update_status(cx))
                    .child(version),
            )
            .bg(theme.home_sidebar_background)
            .min_w(px(0.0))
            .w(px(metrics.panel_width))
            .max_w(px(metrics.panel_width))
            .into_any_element()
    }

    fn thumbnail(&self, project_path: &Path, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = ThemeSettings::theme(cx);
        let radius = px(4.);
        let tile = div()
            .flex_none()
            .w(px(PROJECT_THUMBNAIL_WIDTH))
            .h(px(PROJECT_THUMBNAIL_HEIGHT))
            .rounded(radius)
            .overflow_hidden()
            .border(px(0.5))
            .border_color(theme.navbar_border)
            .bg(theme.viewport_stage_background);

        match self
            .thumbnails
            .as_ref()
            .and_then(|thumbnails| thumbnails.read(cx).image(project_path))
        {
            Some(image) => tile.child(img(image).size_full().rounded(radius)),
            None => tile.flex().items_center().justify_center().child(
                div()
                    .font_family(FontSet::MONOSPACE)
                    .text_size(px(10.))
                    .text_color(theme.text_muted)
                    .opacity(0.6)
                    .child(
                        project_path
                            .extension()
                            .map(|ext| format!(".{}", ext.to_string_lossy()))
                            .unwrap_or_default(),
                    ),
            ),
        }
    }

    fn single_project(
        &self,
        project_path: std::path::PathBuf,
        cx: &Context<HomeView>,
    ) -> impl IntoElement + use<> {
        let theme = ThemeSettings::theme(cx);
        let path = display_folder(&project_path);

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
                    .gap_3()
                    .w_full()
                    .p_2()
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.open(path_for_open.clone(), window, cx);
                    }))
                    .child(self.thumbnail(&project_path, cx))
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
                                                this.update(cx, move |this, cx| {
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
        let paths: Vec<PathBuf> = self
            .state
            .read(cx)
            .recently_opened
            .iter()
            .map(|p| p.path.clone())
            .collect();
        div()
            .text_sm()
            .size_full()
            .p_2()
            .overflow_hidden()
            .child(if paths.is_empty() {
                div()
                    .text_center()
                    .child("No recent projects")
                    .into_any_element()
            } else {
                // the list is short, so it scrolls as plain children: a
                // virtualised list measured against the bottom padding made
                // the last row pop in and out near the end of the scroll
                div()
                    .id("project-list")
                    .size_full()
                    .overflow_y_scroll()
                    .pb_10()
                    .children(paths.into_iter().map(|path| self.single_project(path, cx)))
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
                                multiple: true,
                                prompt: None,
                            };
                            let path = cx.prompt_for_paths(options);

                            cx.spawn(async move |this, app| {
                                let Some(this) = this.upgrade() else {
                                    return;
                                };
                                let Some(paths) = path.await.ok().and_then(|s| s.ok()).flatten()
                                else {
                                    return;
                                };

                                if paths.is_empty() {
                                    return;
                                }

                                let _ = app.update(move |app| {
                                    this.update(app, |this, cx| {
                                        if let Err(err) = this.import_many(paths, cx) {
                                            log::error!("{err}");
                                        }
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
            .child(div().flex_1().min_w_0().child(self.render_projects(cx)))
            .child(
                div()
                    .w(px(logo_metrics.divider_width))
                    .h_full()
                    .bg(divider_color),
            )
            .child(self.render_logo(logo_metrics, cx));

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

#[cfg(test)]
mod tests {
    use super::display_folder;
    use std::path::Path;

    #[test]
    fn folders_shorten_from_the_left() {
        let long = Path::new("/one/two/three/four/five/six/seven/eight/nine/ten/eleven/scene.mcs");
        let shown = display_folder(long);
        assert!(shown.starts_with("…/"), "{shown}");
        assert!(shown.ends_with("/eleven"), "{shown}");
        assert!(shown.chars().count() <= 50, "{shown}");
        assert_eq!(display_folder(Path::new("/a/b/scene.mcs")), "/a/b");
    }
}
