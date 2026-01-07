use gpui::*;

use crate::theme::ColorSet;

pub fn link_button(text: &'static str, action: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> impl IntoElement {
    div()
        .id(text)
        .child(text)
        .text_xs()
        .text_color(ColorSet::BLUE)
        .hover(|this| this.opacity(0.925))
        .cursor_pointer()
        .on_click(action)
}
