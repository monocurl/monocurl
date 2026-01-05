use gpui::*;

pub struct Timeline {

}

impl Timeline {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {

        }
    }
}

impl Render for Timeline {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui::div()
            .child("Timeline")
            .text_color(gpui::white())
    }
}
