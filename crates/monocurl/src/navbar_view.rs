use gpui::*;

use crate::{
    components::buttons::link_button,
    document_view::OpenDocument,
    i18n::{Language, Localization},
    state::{
        user_settings::UserSettings,
        window_state::{ActiveScreen, WindowState},
    },
    theme::{ThemeMode, ThemeSettings},
};

const NAVBAR_HEIGHT: f32 = 30.0;
const LANGUAGE_TRIGGER_WIDTH: f32 = 48.0;
const LANGUAGE_MENU_WIDTH: f32 = 132.0;

pub struct Navbar {
    window_state: WeakEntity<WindowState>,
    tab_scroll: ScrollHandle,
    language_menu_open: bool,
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
            language_menu_open: false,
        }
    }

    fn render_language_picker(
        &self,
        weak_navbar: WeakEntity<Self>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = ThemeSettings::theme(cx);
        let current = Localization::language(cx);
        let popup = self.language_menu_open.then(|| {
            deferred(
                div()
                    .id("language-menu")
                    .absolute()
                    .top(px(NAVBAR_HEIGHT))
                    .left(px(0.0))
                    .w(px(LANGUAGE_MENU_WIDTH))
                    .py_1()
                    .border_1()
                    .border_color(theme.navbar_border)
                    .bg(theme.tab_background)
                    .child(Self::render_outside_language_tracker(weak_navbar))
                    .children(
                        Language::ALL
                            .into_iter()
                            .filter(|language| Localization::is_available(*language))
                            .map(|language| {
                                let is_active = language == current;
                                div()
                                    .id((ElementId::from("language-option"), language.code()))
                                    .w_full()
                                    .px_3()
                                    .py_1()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(12.0))
                                    .text_color(if is_active {
                                        theme.accent
                                    } else {
                                        theme.text_primary
                                    })
                                    .cursor_pointer()
                                    .hover({
                                        let hover = theme.row_hover_overlay;
                                        move |style| style.bg(hover)
                                    })
                                    .child(language.native_name())
                                    .children(is_active.then_some("✓"))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        window.prevent_default();
                                        cx.stop_propagation();
                                        this.language_menu_open = false;
                                        UserSettings::update(cx, |settings| {
                                            settings.set_language(language);
                                        });
                                        cx.notify();
                                    }))
                            }),
                    ),
            )
            .with_priority(1)
        });

        div()
            .relative()
            .h_full()
            .flex_none()
            .child(
                div()
                    .id("language-picker")
                    .h_full()
                    .w(px(LANGUAGE_TRIGGER_WIDTH))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .border_r(px(0.5))
                    .border_color(theme.navbar_border)
                    .bg(if self.language_menu_open {
                        theme.tab_active_background
                    } else {
                        theme.navbar_background
                    })
                    .text_size(px(11.0))
                    .text_color(theme.link_text)
                    .cursor_pointer()
                    .hover({
                        let hover = theme.tab_active_background;
                        move |style| style.bg(hover)
                    })
                    .child(current.code().to_uppercase())
                    .child(if self.language_menu_open {
                        "▴"
                    } else {
                        "▾"
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.language_menu_open = !this.language_menu_open;
                        cx.notify();
                    })),
            )
            .children(popup)
    }

    fn render_outside_language_tracker(weak_navbar: WeakEntity<Self>) -> impl IntoElement {
        canvas(
            |bounds, _, _| bounds,
            move |popup_bounds, _, window, _cx| {
                let trigger_bounds = Bounds::new(
                    point(px(0.0), popup_bounds.origin.y - px(NAVBAR_HEIGHT)),
                    size(px(LANGUAGE_TRIGGER_WIDTH), px(NAVBAR_HEIGHT)),
                );

                window.on_mouse_event(move |event: &MouseDownEvent, phase, _window, cx| {
                    if phase == DispatchPhase::Capture
                        && !trigger_bounds.contains(&event.position)
                        && !popup_bounds.contains(&event.position)
                    {
                        weak_navbar
                            .update(cx, |this, cx| {
                                this.language_menu_open = false;
                                cx.notify();
                            })
                            .ok();
                    }
                });
            },
        )
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
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

impl Render for Navbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language_picker = self.render_language_picker(cx.weak_entity(), cx);
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
                    .child(language_picker)
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
