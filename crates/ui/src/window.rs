use gpui::*;

use crate::{
    app_menu_bar::AppMenuBar,
    document_view::OpenDocument,
    home_view::HomeView,
    state::window_state::{ActiveScreen, WindowState},
    theme::{FontSet, ThemeSettings},
};

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
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self {
            state: state,
            home: home,
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
}

impl Render for MonocurlWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let is_presenting = match &state.screen {
            ActiveScreen::Document(document) => document.view.read(cx).is_presenting(),
            ActiveScreen::Home => false,
        };

        let screen = match &state.screen {
            ActiveScreen::Home => self.render_home(cx).into_any_element(),
            ActiveScreen::Document(document) => self.render_editor(document, cx).into_any_element(),
        };

        let content = div().flex_1().min_h_0().child(screen);

        #[cfg(not(target_os = "macos"))]
        {
            if !is_presenting {
                return div()
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
                    )
                    .into_any_element();
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
