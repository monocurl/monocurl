use gpui::*;

use crate::{
    document_view::OpenDocument,
    home_view::HomeView,
    state::window_state::{ActiveScreen, WindowState},
    theme::{FontSet, ThemeSettings},
};
#[cfg(target_os = "linux")]
use structs::assets::Assets;

#[cfg(not(target_os = "macos"))]
use crate::app_menu_bar::AppMenuBar;

pub struct MonocurlWindow {
    state: Entity<WindowState>,
    home: Entity<HomeView>,
    #[cfg(not(target_os = "macos"))]
    app_menu_bar: Entity<AppMenuBar>,
}

impl MonocurlWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| WindowState::new(window, cx));
        let home = cx.new(|cx| HomeView::new(cx, state.clone()));
        #[cfg(not(target_os = "macos"))]
        let app_menu_bar = cx.new(|cx| AppMenuBar::new(cx));
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        cx.observe_window_bounds(window, |this, window, cx| {
            this.state.update(cx, |state, _| {
                state.save_window_bounds(window);
            });
        })
        .detach();
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self {
            state,
            home,
            #[cfg(not(target_os = "macos"))]
            app_menu_bar,
        }
    }

    pub fn render_screen(&self, view: impl IntoElement, cx: &Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        div()
            .child(view)
            .font_family(FontSet::UI)
            .bg(theme.app_background)
            .text_color(theme.text_primary)
            .size_full()
    }

    pub fn render_home(&self, cx: &Context<Self>) -> impl IntoElement {
        self.render_screen(self.home.clone(), cx)
    }

    pub fn render_editor(&self, document: &OpenDocument, cx: &Context<Self>) -> impl IntoElement {
        self.render_screen(document.view.clone(), cx)
    }

    #[cfg(target_os = "linux")]
    fn has_client_decorations(window: &Window) -> bool {
        matches!(window.window_decorations(), Decorations::Client { .. })
    }

    #[cfg(target_os = "linux")]
    fn window_control_icon(name: &str, color: Rgba) -> impl IntoElement {
        svg()
            .path(Assets::image_resource(name))
            .text_color(color)
            .w(px(16.0))
            .h(px(16.0))
    }

    #[cfg(target_os = "linux")]
    fn render_title_drag_region(&self, window: &Window) -> Option<AnyElement> {
        if !Self::has_client_decorations(window) {
            return None;
        }

        Some(
            div()
                .id("client-titlebar-drag-region")
                .absolute()
                .top(px(0.0))
                .left(px(220.0))
                .right(px(130.0))
                .h(px(24.0))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    cx.stop_propagation();
                    window.start_window_move();
                })
                .on_click(|event, window, cx| {
                    cx.stop_propagation();
                    if event.click_count() == 2 {
                        window.zoom_window();
                    } else if event.is_right_click() {
                        window.show_window_menu(event.position());
                    }
                })
                .into_any_element(),
        )
    }

    #[cfg(target_os = "linux")]
    fn render_window_controls(&self, window: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        if !Self::has_client_decorations(window) {
            return None;
        }

        let theme = ThemeSettings::theme(cx);
        let controls = window.window_controls();

        Some(
            div()
                .id("client-window-controls")
                .absolute()
                .top(px(0.0))
                .right(px(0.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .bg(theme.navbar_background)
                .child(
                    div()
                        .id("window-minimize")
                        .w(px(42.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_primary)
                        .cursor_pointer()
                        .hover({
                            let hover = theme.tab_active_background;
                            move |this| this.bg(hover)
                        })
                        .child(Self::window_control_icon(
                            "window/window-minimize-symbolic.svg",
                            theme.text_primary,
                        ))
                        .on_click(move |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            if controls.minimize {
                                window.minimize_window();
                            }
                        }),
                )
                .child(
                    div()
                        .id("window-maximize")
                        .w(px(42.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_primary)
                        .cursor_pointer()
                        .hover({
                            let hover = theme.tab_active_background;
                            move |this| this.bg(hover)
                        })
                        .child(Self::window_control_icon(
                            "window/window-maximize-symbolic.svg",
                            theme.text_primary,
                        ))
                        .on_click(move |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            if controls.maximize {
                                window.zoom_window();
                            }
                        }),
                )
                .child(
                    div()
                        .id("window-close")
                        .w(px(46.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.text_primary)
                        .cursor_pointer()
                        .hover({
                            let danger = theme.danger;
                            move |this| this.bg(danger)
                        })
                        .child(Self::window_control_icon(
                            "window/window-close-symbolic.svg",
                            theme.text_primary,
                        ))
                        .on_click(|_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            window.remove_window();
                        }),
                )
                .into_any_element(),
        )
    }
}

impl Render for MonocurlWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);

        let screen = match &state.screen {
            ActiveScreen::Home => self.render_home(cx).into_any_element(),
            ActiveScreen::Document(document) => self.render_editor(document, cx).into_any_element(),
        };

        let content = div().flex_1().min_h_0().child(screen);

        #[cfg(not(target_os = "macos"))]
        {
            let is_presenting = match &state.screen {
                ActiveScreen::Document(document) => document.view.read(cx).is_presenting(),
                ActiveScreen::Home => false,
            };

            if !is_presenting {
                let root = div()
                    .relative()
                    .flex()
                    .flex_col()
                    .size_full()
                    .child(div().h(px(24.0)).flex_none())
                    .child(content)
                    .child(
                        div()
                            .absolute()
                            .top(px(0.0))
                            .left(px(0.0))
                            .w_full()
                            .child(self.app_menu_bar.clone()),
                    );

                #[cfg(target_os = "linux")]
                let root = root
                    .children(self.render_title_drag_region(_window))
                    .children(self.render_window_controls(_window, cx));

                return root.into_any_element();
            }
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .child(content)
            .into_any_element()
    }
}
