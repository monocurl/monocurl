use gpui::*;

use crate::{
    components::buttons::link_button,
    document_view::OpenDocument,
    state::window_state::{ActiveScreen, WindowState},
    theme::{ThemeMode, ThemeSettings},
};

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
                        .map(|e| ".".to_string() + &e.to_string_lossy().to_string())
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
            .h(px(30.0))
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
            .flex_none()
            .text_color(theme.text_muted)
            .child(div().text_xs().child("Dark"))
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

impl Render for Navbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = self.window_state.upgrade().unwrap();
        let state = entity.read(cx);
        let theme = ThemeSettings::theme(cx);

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
            .h(px(30.0))
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
                    .child(
                        div()
                            .bg(if matches!(state.screen, ActiveScreen::Home) {
                                theme.tab_active_background
                            } else {
                                theme.tab_background
                            })
                            .child(link_button(
                                "Home",
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
