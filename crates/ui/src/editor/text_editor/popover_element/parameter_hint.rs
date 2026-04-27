use super::*;

fn parameter_hint_arg_label(arg: &ParameterHintArg) -> String {
    let mut label = String::new();
    if arg.is_reference {
        label.push('&');
    }
    label.push_str(&arg.name);
    if arg.has_default {
        label.push('=');
    }
    label
}

impl PopoverElement {
    pub(super) fn build_parameter_hint_popover(
        &self,
        parameter_hint_state: &Rc<RefCell<ParameterPositionState>>,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let item_padding_x = px(12.0);
        let item_padding_y = px(6.0);
        let min_w = px(150.0);

        let state = parameter_hint_state.borrow();
        let hint = state.hint.as_ref().unwrap();

        div()
            .flex()
            .absolute()
            .bg(styles.popover_background_color)
            .rounded_md()
            .border_1()
            .border_color(styles.popover_border_color)
            .min_w(min_w)
            .shadow(vec![BoxShadow {
                offset: Point {
                    x: px(0.),
                    y: px(0.),
                },
                blur_radius: px(1.),
                spread_radius: px(1.),
                color: styles.popover_shadow_color,
            }])
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(item_padding_x)
                    .py(item_padding_y)
                    .text_sm()
                    .child(
                        div()
                            .text_color(styles.popover_title_color)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(hint.name.clone()),
                    )
                    .child(
                        div()
                            .text_color(styles.popover_title_color)
                            .child(if hint.is_operator { "{" } else { "(" }),
                    )
                    .children(hint.args.iter().enumerate().flat_map(|(i, arg)| {
                        let is_active = i == hint.active_index;
                        let mut elements = vec![
                            div()
                                .when(is_active, |this| {
                                    this.text_color(styles.popover_active_argument_color)
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .underline()
                                })
                                .when(!is_active, |this| {
                                    this.text_color(styles.popover_inactive_argument_color)
                                })
                                .child(parameter_hint_arg_label(arg))
                                .into_any_element(),
                        ];

                        if i < hint.args.len() - 1 {
                            elements.push(
                                div()
                                    .text_color(styles.popover_title_color)
                                    .child(", ")
                                    .into_any_element(),
                            );
                        }

                        elements
                    }))
                    .child(
                        div()
                            .text_color(styles.popover_title_color)
                            .child(if hint.is_operator { "}" } else { ")" }),
                    ),
            )
            .into_any_element()
    }
}
