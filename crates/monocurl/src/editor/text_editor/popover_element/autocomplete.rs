use super::*;

impl PopoverElement {
    fn render_highlighted_text(
        text: &str,
        highlight_indices: &[usize],
        is_selected: bool,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let base_color = if is_selected {
            styles.popover_title_color
        } else {
            styles.popover_text_color
        };
        let highlight_color = styles.popover_highlight_color;

        let mut segments = Vec::new();
        let mut current_segment = String::new();
        let mut current_is_highlighted = false;
        text.char_indices()
            .map(|(i, ch)| (highlight_indices.contains(&i), ch))
            .for_each(|(is_highlighted, ch)| {
                if current_segment.is_empty() {
                    current_segment.push(ch);
                    current_is_highlighted = is_highlighted;
                } else if is_highlighted == current_is_highlighted && false {
                    // disabled for now since it causes visual glitches
                    current_segment.push(ch);
                } else {
                    segments.push((current_segment.clone(), current_is_highlighted));
                    current_segment.clear();
                    current_segment.push(ch);
                    current_is_highlighted = is_highlighted;
                }
            });

        if !current_segment.is_empty() {
            segments.push((current_segment, current_is_highlighted));
        }

        let container =
            div()
                .flex()
                .items_center()
                .text_size(px(14.))
                .children(segments.into_iter().map(|(segment_text, is_highlighted)| {
                    div()
                        .text_color(if is_highlighted {
                            highlight_color
                        } else {
                            base_color
                        })
                        .when(is_highlighted, |this| {
                            this.font_weight(FontWeight::BOLD).underline()
                        })
                        .child(segment_text)
                }));

        container.into_any_element()
    }

    pub(super) fn build_autocomplete_popover(
        &self,
        autocomplete_state: &Rc<RefCell<AutoCompleteState>>,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let padding = px(4.0);
        let item_padding_x = px(8.0);
        let item_padding_y = px(3.0);
        let min_w = px(200.0);
        let max_w = px(400.0);

        let ac = autocomplete_state.borrow();

        div()
            .flex()
            .absolute()
            .p(padding)
            .bg(styles.popover_background_color)
            .rounded_md()
            .border_1()
            .border_color(styles.popover_border_color)
            .max_w(max_w)
            .shadow(vec![BoxShadow {
                offset: Point {
                    x: px(0.),
                    y: px(0.),
                },
                blur_radius: px(2.),
                spread_radius: px(2.),
                color: styles.popover_shadow_color,
            }])
            .child(
                div()
                    .min_w(min_w)
                    .flex()
                    .flex_col()
                    .max_h(px(200.0))
                    .id("autocomplete-bar")
                    .overflow_y_scroll()
                    .track_scroll(&ac.scroll_handle)
                    .on_scroll_wheel(|_scroll, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .children(ac.filtered_items.iter().map(|(index, highlights)| {
                        let index_copy = *index;
                        let item1 = ac.items[*index].clone();
                        let head = item1.head.clone();
                        let category = item1.category;
                        let is_selected = *index == ac.selected_index;

                        let ac_copy = autocomplete_state.clone();
                        let editor_copy = self.editor.clone();

                        div().child(
                            div()
                                .px(item_padding_x)
                                .py(item_padding_y)
                                .rounded_sm()
                                .when(is_selected, |this| {
                                    this.bg(styles.popover_selected_background_color)
                                })
                                .when(!is_selected, |this| {
                                    this.bg(styles.popover_background_color).hover({
                                        let hover = styles.popover_hover_background_color;
                                        move |style| style.bg(hover)
                                    })
                                })
                                .cursor_pointer()
                                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    editor_copy.update(cx, |editor, cx| {
                                        AutoCompleteState::apply_index(
                                            &ac_copy,
                                            index_copy,
                                            editor,
                                            editor.state.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                })
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(12.0))
                                        .child(div().flex_1().child(Self::render_highlighted_text(
                                            &head,
                                            highlights,
                                            is_selected,
                                            styles,
                                        )))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(styles.popover_inactive_argument_color)
                                                .child(category.label()),
                                        ),
                                ),
                        )
                    })),
            )
            .into_any_element()
    }
}
