use gpui::*;

pub mod text_editor;
mod backing;

pub struct Editor {

}

impl Editor {
    pub fn new(_cx: &mut gpui::Context<Self>) -> Self {
        Self {

        }
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .child("Editor")
    }
}
