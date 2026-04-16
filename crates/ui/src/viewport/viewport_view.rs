use gpui::*;

use crate::theme::ThemeSettings;

pub struct Viewport;

impl Viewport {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self
    }
}

impl Render for Viewport {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);

        gpui::div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .bg(theme.viewport_background)
            .text_color(theme.text_muted)
            .child("Viewport")
    }
}
