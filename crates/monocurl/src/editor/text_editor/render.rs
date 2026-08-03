use super::*;

use executor::time::Timestamp;
use structs::assets::Assets;

use crate::state::diagnostics::DiagnosticType;

const SLIDE_JUMP_ICON_SIZE: f32 = 14.0;
const SLIDE_JUMP_ICON_MARGIN_LEFT: f32 = 10.0;
const FOLD_ELLIPSIS_MARGIN_LEFT: f32 = 7.0;
const FOLD_ELLIPSIS_WIDTH: f32 = 22.0;
const FOLD_ELLIPSIS_HEIGHT: f32 = 16.0;
const FOLD_ELLIPSIS_GAP: f32 = 6.0;
const FOLD_ELLIPSIS_TEXT_NUDGE_Y: f32 = -3.0;

impl TextEditor {
    fn visible_slide_range(&self, slides: &[SlideInfo], line_count: usize) -> Range<usize> {
        let visible = self.visible_lines();
        let visible_start = visible.start.min(line_count);
        let visible_end = visible.end.min(line_count);
        let start = slides.partition_point(|slide| slide.line < visible_start);
        let end = slides.partition_point(|slide| slide.line < visible_end);
        start..end
    }

    fn render_slide_jump_icons(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let services = self.services.clone();
        let line_height = self.line_height;
        let icon_inset = (line_height - px(SLIDE_JUMP_ICON_SIZE)) / 2.0;

        let mut icons = Vec::new();
        let line_count = self.line_map.line_count();
        let state = self.state.read(cx);
        let slides = state.slides();
        let visible_slides = self.visible_slide_range(slides, line_count);
        let slides: Vec<_> = slides[visible_slides.clone()]
            .iter()
            .enumerate()
            .map(|(offset, info)| (visible_slides.start + offset, info))
            .filter(|(_, info)| info.header_end.row < line_count)
            .map(|(idx, info)| (idx, info.header_end, self.is_slide_folded(info)))
            .collect();

        for (slide_idx, location, is_folded) in slides {
            let Point { x, y } = self.line_map.point_for_location(location);
            let services = services.clone();
            let header_margin = if is_folded {
                FOLD_ELLIPSIS_MARGIN_LEFT + FOLD_ELLIPSIS_WIDTH + FOLD_ELLIPSIS_GAP
            } else {
                SLIDE_JUMP_ICON_MARGIN_LEFT
            };
            let element = div()
                .id(("editor-slide-jump", slide_idx))
                .absolute()
                .left(self.gutter_width + x + px(header_margin))
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

    fn render_fold_icons(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let editor = cx.entity();
        let line_height = self.line_height;
        let icon_inset = (line_height - px(FOLD_ICON_SIZE)) / 2.0;
        let line_count = self.line_map.line_count();
        let state = self.state.read(cx);
        let slides = state.slides();
        let visible_slides = self.visible_slide_range(slides, line_count);

        let slides: Vec<_> = slides[visible_slides.clone()]
            .iter()
            .enumerate()
            .map(|(offset, slide)| (visible_slides.start + offset, slide))
            .filter(|(idx, slide)| {
                slide.header_end.row < line_count
                    && !Self::slide_body_line_range(slides, *idx, line_count).is_empty()
            })
            .map(|(_, slide)| (slide.start_offset, slide.line, self.is_slide_folded(slide)))
            .collect();

        slides
            .into_iter()
            .map(|(start_offset, line, is_folded)| {
                let y = self.line_map.y_range(line..line + 1).start;
                let editor = editor.clone();
                div()
                    .id(("editor-slide-fold", start_offset))
                    .absolute()
                    .left(self.gutter_width - px(FOLD_ICON_RIGHT + FOLD_ICON_SIZE))
                    .top(y + icon_inset)
                    .w(px(FOLD_ICON_SIZE))
                    .h(px(FOLD_ICON_SIZE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_color(self.text_styles.gutter_text_color)
                    .opacity(0.72)
                    .hover(|s| {
                        s.text_color(self.text_styles.gutter_active_color)
                            .opacity(1.0)
                    })
                    .child(
                        svg()
                            .path(Assets::image_resource(if is_folded {
                                "editor/fold-closed.svg"
                            } else {
                                "editor/fold-open.svg"
                            }))
                            .text_color(self.text_styles.gutter_text_color)
                            .w(px(FOLD_ICON_SIZE))
                            .h(px(FOLD_ICON_SIZE)),
                    )
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        editor.update(cx, |editor, cx| {
                            editor.toggle_slide_fold_at_start(start_offset, cx);
                        });
                    })
                    .into_any_element()
            })
            .collect()
    }

    fn render_fold_ellipses(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let editor = cx.entity();
        let line_height = self.line_height;
        let inset = (line_height - px(FOLD_ELLIPSIS_HEIGHT)) / 2.0;
        let line_count = self.line_map.line_count();
        let slides: Vec<_> = {
            let state = self.state.read(cx);
            let slides = state.slides();
            let visible_slides = self.visible_slide_range(slides, line_count);

            slides[visible_slides.clone()]
                .iter()
                .enumerate()
                .map(|(offset, slide)| (visible_slides.start + offset, slide))
                .filter(|(_, slide)| {
                    slide.header_end.row < line_count && self.is_slide_folded(slide)
                })
                .map(|(idx, slide)| (idx, slide.start_offset, slide.header_end))
                .collect()
        };

        slides
            .into_iter()
            .map(|(slide_idx, start_offset, location)| {
                let diagnostic = self.folded_slide_diagnostic_type(slide_idx, cx);
                let Point { x, y } = self.line_map.point_for_location(location);
                let editor = editor.clone();
                let base_color = diagnostic
                    .map(|dtype| match dtype {
                        DiagnosticType::RuntimeError => self.text_styles.runtime_error_color,
                        DiagnosticType::CompileTimeError => {
                            self.text_styles.compile_time_error_color
                        }
                        DiagnosticType::CompileTimeWarning => {
                            self.text_styles.compile_time_warning_color
                        }
                    })
                    .unwrap_or(self.text_styles.gutter_text_color);
                let mut text_color = base_color;
                text_color.a = if diagnostic.is_some() { 0.92 } else { 0.68 };
                let mut bg_color = base_color;
                bg_color.a = if diagnostic.is_some() { 0.16 } else { 0.08 };
                let mut border_color = base_color;
                border_color.a = if diagnostic.is_some() { 0.38 } else { 0.18 };

                div()
                    .id(("editor-slide-fold-ellipsis", start_offset))
                    .absolute()
                    .left(self.gutter_width + x + px(FOLD_ELLIPSIS_MARGIN_LEFT))
                    .top(y + inset)
                    .w(px(FOLD_ELLIPSIS_WIDTH))
                    .h(px(FOLD_ELLIPSIS_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(border_color)
                    .bg(bg_color)
                    .text_size(px(11.0))
                    .line_height(px(FOLD_ELLIPSIS_HEIGHT))
                    .text_color(text_color)
                    .cursor_pointer()
                    .child(
                        div()
                            .relative()
                            .top(px(FOLD_ELLIPSIS_TEXT_NUDGE_Y))
                            .child("..."),
                    )
                    .hover(move |s| {
                        let mut hover_bg = bg_color;
                        hover_bg.a = (hover_bg.a + 0.08).min(0.34);
                        s.bg(hover_bg)
                    })
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        editor.update(cx, |editor, cx| {
                            editor.toggle_slide_fold_at_start(start_offset, cx);
                        });
                    })
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for TextEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reshape_lines_needing_layout(window, cx);
        self.sync_folds_to_slides(cx);
        self.reshape_visible_lines_with_stale_attributes(window, cx);

        let total_height = self.line_map.total_height() + px(BOTTOM_SCROLL_PADDING);
        let fold_icons = self.render_fold_icons(cx);
        let fold_ellipses = self.render_fold_ellipses(cx);
        let slide_icons = self.render_slide_jump_icons(cx);
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .key_context(match (self.search.visible, self.go_to_line_visible) {
                (true, _) => "editor find-panel",
                (false, true) => "editor go-to-line",
                (false, false) => "editor",
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
            .on_action(cx.listener(Self::toggle_slide_fold))
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
            .on_action(cx.listener(Self::open_go_to_line))
            .on_action(cx.listener(Self::close_go_to_line))
            .on_action(cx.listener(Self::confirm_go_to_line))
            .on_action(cx.listener(Self::find_next))
            .on_action(cx.listener(Self::find_previous))
            .on_action(cx.listener(Self::next_diagnostic))
            .on_action(cx.listener(Self::previous_diagnostic))
            .on_action(cx.listener(Self::next_slide_header))
            .on_action(cx.listener(Self::previous_slide_header))
            .on_action(cx.listener(Self::go_to_definition))
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
                    .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
                    .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(total_height)
                            .child(TextElement {
                                editor: cx.entity(),
                            })
                            .children(fold_icons)
                            .children(fold_ellipses)
                            .children(slide_icons),
                    ),
            )
            .child(PopoverElement::new(cx.entity()))
            .children(self.render_find_panel(cx))
            .children(self.render_go_to_line_panel(cx))
    }
}
