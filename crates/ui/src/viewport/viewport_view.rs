use gpui::*;

use crate::{services::ServiceManager, theme::ThemeSettings};

pub struct Viewport {
    services: Entity<ServiceManager>,
}

impl Viewport {
    pub fn new(services: Entity<ServiceManager>, cx: &mut gpui::Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self { services }
    }
}

impl Render for Viewport {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let exec = self.services.read(cx).execution_state().read(cx);
        let ring_color = theme.viewport_status_ring(exec.status);

        gpui::div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .bg(theme.viewport_background)
            .p(px(24.0))
            .child(
                gpui::div()
                    .flex()
                    .flex_1()
                    .size_full()
                    .bg(ring_color)
                    .p(px(1.5))
                    .child(
                        gpui::div()
                            .size_full()
                            .bg(theme.viewport_stage_background)
                    )
            )
    }
}
