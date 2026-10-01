use gpui::*;

use crate::{
    auto_update::{AutoUpdateStatus, AutoUpdater},
    components::buttons::link_button,
    document_view::OpenDocument,
    i18n::Localization,
    state::window_state::{ActiveScreen, WindowState},
    theme::{ThemeMode, ThemeSettings},
};

const NAVBAR_HEIGHT: f32 = 30.0;

pub struct Navbar {
    window_state: WeakEntity<WindowState>,
    tab_scroll: ScrollHandle,
}

impl Navbar {
    pub fn new(state: WeakEntity<WindowState>, cx: &mut Context<Self>) -> Self {
        if let Some(window_state) = state.upgrade() {
            cx.observe(&window_state, |_this, _, cx| {
                cx.notify();
            })
            .detach();
        }
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<Localization>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self {
            window_state: state,
            tab_scroll: ScrollHandle::new(),
        }
    }

    fn render_tab(
        &self,
        doc: &OpenDocument,
        is_active: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let filename = doc
            .path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or(
                "Untitled".to_string()
                    + &doc
                        .path
                        .extension()
                        .map(|e| ".".to_string() + e.to_string_lossy().as_ref())
                        .unwrap_or_default(),
            );

        let path_for_close = doc.path.clone();
        let path_for_open = doc.path.clone();
        let theme = ThemeSettings::theme(cx);

        let bg = if is_active {
            theme.tab_active_background
        } else {
            theme.tab_background
        };

        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap_2()
            .pl_3()
            .pr_1()
            .h_full()
            .border_r(px(0.5))
            .border_color(theme.navbar_border)
            .h(px(NAVBAR_HEIGHT))
            .bg(bg)
            .text_color(theme.text_primary)
            .child(filename)
            .id(SharedString::new(doc.path.to_string_lossy().to_string()))
            .child(
                div()
                    .size_3()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .hover({
                        let hover = theme.tab_close_hover_background;
                        move |style| style.bg(hover)
                    })
                    .child("×")
                    .id("close-button")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let state = this.window_state.upgrade().unwrap();
                        let path = path_for_close.clone();
                        state.update(cx, move |wstate, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            wstate.close_tab(&path, cx, window);
                            cx.notify();
                        })
                    })),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                let state = this.window_state.upgrade().unwrap();
                let statec = state.clone();
                let path = path_for_open.clone();
                state.update(cx, move |wstate, cx| {
                    wstate.navigate_to(path.clone(), statec, window, cx);
                    cx.notify();
                })
            }))
            .cursor_pointer()
    }

    fn render_theme_toggle(&self, is_dark: bool, cx: &Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);

        let switch = if is_dark {
            div()
                .w(px(34.0))
                .h(px(18.0))
                .px(px(0.5))
                .flex()
                .items_center()
                .justify_start()
                .rounded_full()
                .border_1()
                .border_color(theme.accent)
                .bg(theme.navbar_background)
                .child(
                    div()
                        .w(px(12.0))
                        .h(px(12.0))
                        .ml(px(16.0))
                        .rounded_full()
                        .bg(theme.accent),
                )
        } else {
            div()
                .w(px(34.0))
                .h(px(18.0))
                .px(px(0.5))
                .flex()
                .items_center()
                .justify_start()
                .rounded_full()
                .border_1()
                .border_color(theme.accent)
                .bg(theme.navbar_background)
                .child(
                    div()
                        .w(px(12.0))
                        .h(px(12.0))
                        .ml(px(3.0))
                        .rounded_full()
                        .bg(theme.accent),
                )
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .h_full()
            .border_t(px(0.5))
            .border_color(theme.navbar_border)
            .flex_none()
            .text_color(theme.text_muted)
            .child(div().text_xs().child(Localization::text(cx, "nav.dark")))
            .child(switch)
            .cursor_pointer()
            .hover(|style| style.opacity(0.92))
            .id("theme-toggle")
            .on_scroll_wheel(|_event, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(cx.listener(|_this, _, _, cx| {
                ThemeSettings::toggle(cx);
            }))
    }
}

impl Navbar {
    /// the update state next to the home button, so people editing see it:
    /// a loud pill once an update is ready, a quiet one while it downloads
    fn render_update_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = ThemeSettings::theme(cx);
        let (label, ready, errored) = match AutoUpdater::status(cx) {
            AutoUpdateStatus::Idle => return None,
            AutoUpdateStatus::Checking => ("Checking for updates".to_string(), false, false),
            AutoUpdateStatus::Downloading { version } => {
                (format!("Downloading v{version}"), false, false)
            }
            AutoUpdateStatus::Installing { version } => {
                (format!("Installing v{version}"), false, false)
            }
            AutoUpdateStatus::ReadyToRestart { version } => {
                (format!("Update v{version} ready: restart"), true, false)
            }
            AutoUpdateStatus::Errored { .. } => ("Update failed: retry".to_string(), false, true),
        };
        let pill = div()
            .id("navbar-update")
            .flex_none()
            .ml_2()
            .px_2()
            .py(px(2.0))
            .rounded(px(10.0))
            .text_size(px(11.0))
            .line_height(px(14.0))
            .child(label);
        let pill = if ready {
            pill.bg(theme.accent)
                .text_color(theme.app_background)
                .cursor_pointer()
                .hover(|style| style.opacity(0.85))
                .on_click(|_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    AutoUpdater::restart_from_update_status(cx);
                })
        } else if errored {
            pill.border_1()
                .border_color(theme.danger)
                .text_color(theme.danger)
                .cursor_pointer()
                .hover(|style| style.opacity(0.85))
                .on_click(|_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    AutoUpdater::check_for_updates(Some(window.window_handle()), cx);
                })
        } else {
            pill.text_color(theme.text_muted)
        };
        Some(pill.into_any_element())
    }
}

impl Render for Navbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let update_pill = self.render_update_button(cx);
        let entity = self.window_state.upgrade().unwrap();
        let state = entity.read(cx);
        let theme = ThemeSettings::theme(cx);
        let is_home = matches!(state.screen, ActiveScreen::Home);

        let active = match state.screen {
            ActiveScreen::Home => None,
            ActiveScreen::Document(ref open_document) => Some(open_document.path.clone()),
        };

        let tabs: Vec<_> = state
            .open_documents()
            .map(|doc| self.render_tab(doc, Some(&doc.path) == active.as_ref(), cx))
            .collect();

        let document_list = if tabs.is_empty() {
            div()
                .id("document-list")
                .h_full()
                .flex_1()
                .min_w_0()
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_w_0()
                .h_full()
                .id("document-list")
                .border_l(px(0.5))
                .border_t(px(0.5))
                .border_b(px(0.5))
                .border_color(theme.navbar_border)
                .children(tabs)
                .text_size(px(12.0))
                .overflow_x_scroll()
                .track_scroll(&self.tab_scroll)
                .on_scroll_wheel(|_event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .into_any_element()
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .relative()
            .h(px(NAVBAR_HEIGHT))
            .bg(theme.navbar_background)
            .border_color(theme.navbar_border)
            .border_b(px(0.5))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    // room for the macos traffic lights, which sit on the navbar
                    .pl(px(if cfg!(target_os = "macos") { 72.0 } else { 0.0 }))
                    .child(
                        div()
                            .bg(if is_home {
                                theme.tab_active_background
                            } else {
                                theme.tab_background
                            })
                            .child(link_button(
                                Localization::text(cx, "nav.home"),
                                theme.link_text,
                                cx.listener(|this, _, _, cx| {
                                    let state = this.window_state.upgrade().unwrap();
                                    state.update(cx, |state, cx| {
                                        state.navigate_to_home();
                                        cx.notify();
                                    })
                                }),
                            ))
                            .px_3()
                            .h_full()
                            .flex()
                            .flex_none()
                            .items_center(),
                    )
                    .children(update_pill)
                    .child(document_list),
            )
            .child(
                self.render_theme_toggle(
                    matches!(ThemeSettings::read(cx).mode, ThemeMode::Dark),
                    cx,
                ),
            )
    }
}
