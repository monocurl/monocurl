use gpui::*;

pub fn link_button(text: &'static str, text_color: impl Into<Hsla>, action: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> impl IntoElement {
    div()
        .id(text)
        .child(text)
        .text_xs()
        .text_color(text_color)
        .hover(|this| this.opacity(0.925))
        .cursor_pointer()
        .on_click(action)
}
