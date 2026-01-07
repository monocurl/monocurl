use gpui::*;

use crate::{document_view::OpenDocument, home_view::HomeView, state::window_state::{ActiveScreen, WindowState}, theme::FontSet};

pub struct MonocurlWindow {
    state: Entity<WindowState>,
    home: Entity<HomeView>,
}

impl MonocurlWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| WindowState::new(window, cx));
        let home = cx.new(|cx| HomeView::new(cx, state.clone()));

        Self {
            state: state,
            home: home
        }
    }

    pub fn render_screen(&self, view: impl IntoElement) -> impl IntoElement {
        div()
            .child(view)
            .font_family(FontSet::UI)
            .size_full()
    }

    pub fn render_home(&self, _cx: &Context<Self>) -> impl IntoElement {
        self.render_screen(self.home.clone())
    }

    pub fn render_editor(&self, document: &OpenDocument, _cx: &Context<Self>) -> impl IntoElement {
        self.render_screen(document.view.clone())
    }

}

impl Render for MonocurlWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        match &state.screen {
            ActiveScreen::Home => {
                self.render_home(cx).into_any_element()
            },
            ActiveScreen::Document(document ) => {
                self.render_editor(document, cx).into_any_element()
            }
        }
    }
}
