use super::*;
use parser::documentation::Documentation;

impl PopoverElement {
    pub(super) fn build_documentation_popover(
        &self,
        documentation: &Documentation,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let max_w = px(600.0);
        let mut content = div()
            .p(px(8.0))
            .flex()
            .flex_col()
            .gap(px(5.0))
            .max_w(max_w)
            .bg(styles.popover_background_color)
            .rounded_md()
            .border_1()
            .border_color(styles.popover_border_color)
            .shadow(vec![BoxShadow {
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(2.0),
                spread_radius: px(2.0),
                color: styles.popover_shadow_color,
            }])
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(styles.popover_title_color)
                    .child(documentation.name.clone()),
            );

        if !documentation.overview.is_empty() {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(styles.popover_text_color)
                    .child(documentation.overview.clone()),
            );
        }
        if !documentation.description.is_empty() {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(styles.popover_text_color)
                    .child(documentation.description.clone()),
            );
        }
        if !documentation.parameters.is_empty() {
            let mut parameters = div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .text_xs()
                .text_color(styles.popover_text_color);
            for parameter in &documentation.parameters {
                parameters =
                    parameters.child(format!("{}: {}", parameter.name, parameter.description));
            }
            content = content.child(parameters);
        }
        if let Some(example) = documentation.examples.first() {
            content = content.child(
                div()
                    .text_xs()
                    .text_color(styles.popover_text_color)
                    .opacity(0.8)
                    .child(example_without_fence(example)),
            );
        }
        content = content.child(
            div()
                .text_xs()
                .text_color(styles.popover_text_color)
                .opacity(0.55)
                .child(documentation.source.clone()),
        );

        div()
            .flex()
            .absolute()
            .max_w(max_w)
            .py(px(4.0))
            .child(content)
            .on_mouse_move(|_, window, app| {
                window.prevent_default();
                app.stop_propagation();
            })
            .into_any_element()
    }
}

fn example_without_fence(example: &str) -> String {
    let mut lines = example.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    if !first.trim_start().starts_with("```") {
        return example.to_string();
    }

    let mut content: Vec<_> = lines.collect();
    if content.last().is_some_and(|line| line.trim() == "```") {
        content.pop();
    }
    content.join("\n")
}

#[cfg(test)]
mod tests {
    use super::example_without_fence;

    #[test]
    fn removes_markdown_fence_from_examples() {
        assert_eq!(
            example_without_fence("```mcl\nhex(\"009ee0\")\n```"),
            "hex(\"009ee0\")"
        );
    }
}
