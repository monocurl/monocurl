use gpui::*;


pub struct Viewport {

}

impl Viewport {
    pub fn new(_cx: &mut gpui::Context<Self>) -> Self {
        Self {

        }
    }
}

impl Render for Viewport {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui::div()
            .child("Viewport")
    }
}
