use super::*;

use executor::time::Timestamp;
use structs::assets::Assets;

const SLIDE_JUMP_ICON_SIZE: f32 = 14.0;
const SLIDE_JUMP_ICON_MARGIN_LEFT: f32 = 10.0;

impl TextEditor {
    fn render_slide_jump_icons(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let services = self.services.clone();
        let line_height = self.line_height;
        let icon_inset = (line_height - px(SLIDE_JUMP_ICON_SIZE)) / 2.0;

        let mut icons = Vec::new();
        let line_count = self.line_map.line_count();
        let slides: Vec<_> = self
            .state
            .read(cx)
            .slides()
            .iter()
            .enumerate()
            .filter(|(_, info)| info.header_end.row < line_count)
            .map(|(idx, info)| (idx, info.header_end))
            .collect();

        for (slide_idx, location) in slides {
            let Point { x, y } = self.line_map.point_for_location(location);
            let services = services.clone();
            let element = div()
                .id(("editor-slide-jump", slide_idx))
                .absolute()
                .left(self.gutter_width + x + px(SLIDE_JUMP_ICON_MARGIN_LEFT))
                .top(y + icon_inset)
                .w(px(SLIDE_JUMP_ICON_SIZE))
                .h(px(SLIDE_JUMP_ICON_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(self.text_styles.gutter_text_color)
                .opacity(0.7)
                .hover(|s| {
                    s.text_color(self.text_styles.gutter_active_color)
                        .opacity(1.0)
                })
                .child(
                    svg()
                        .path(Assets::image_resource("editor/jump-to-slide.svg"))
                        .text_color(self.text_styles.gutter_text_color)
                        .w(px(SLIDE_JUMP_ICON_SIZE))
                        .h(px(SLIDE_JUMP_ICON_SIZE)),
                )
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    services.update(cx, |s, cx| {
                        s.seek_to(Timestamp::at_end_of_slide(slide_idx), cx)
                    });
                })
                .into_any_element();
            icons.push(element);
        }
        icons
    }
}

impl Render for TextEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reshape_lines_needing_layout(window, cx);
        self.reshape_visible_lines_with_stale_attributes(window, cx);

        let total_height = self.line_map.total_height() + px(BOTTOM_SCROLL_PADDING);
        let slide_icons = self.render_slide_jump_icons(cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .key_context(if self.search.visible {
                "editor find-panel"
            } else {
                "editor"
            })
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::backspace_word))
            .on_action(cx.listener(Self::backspace_line))
            .on_action(cx.listener(Self::delete_word))
            .on_action(cx.listener(Self::delete_line))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::untab))
            .on_action(cx.listener(Self::toggle_comment))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::left_word))
            .on_action(cx.listener(Self::right_word))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_left_word))
            .on_action(cx.listener(Self::select_right_word))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::open_find))
            .on_action(cx.listener(Self::close_find))
            .on_action(cx.listener(Self::find_next))
            .on_action(cx.listener(Self::find_previous))
            .on_action(cx.listener(Self::replace_current))
            .on_action(cx.listener(Self::replace_all))
            .child(
                div()
                    .id("text-editor-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .cursor(CursorStyle::IBeam)
                    .bg(self.text_styles.bg_color)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_move(cx.listener(Self::on_mouse_move))
                    .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(total_height)
                            .child(TextElement {
                                editor: cx.entity(),
                            })
                            .children(slide_icons),
                    ),
            )
            .child(PopoverElement::new(cx.entity()))
            .children(self.render_find_panel(cx))
    }
}
