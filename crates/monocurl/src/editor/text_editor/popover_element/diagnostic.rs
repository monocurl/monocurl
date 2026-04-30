use super::*;

impl PopoverElement {
    pub(super) fn build_diagnostic_popover(
        &self,
        diagnostic: &Diagnostic,
        styles: &TextEditorStyles,
        is_copied: bool,
    ) -> AnyElement {
        let color = diagnostic.color(styles);
        let padding = px(8.0);
        let margin = px(4.0);
        let max_w = px(600.0);
        let copy_text = Self::diagnostic_copy_text(diagnostic);
        let editor = self.editor.clone();
        let copy_action = if is_copied {
            div()
                .px(px(5.0))
                .py(px(1.0))
                .rounded_sm()
                .border_1()
                .border_color(styles.popover_border_color)
                .text_size(px(11.0))
                .text_color(styles.popover_text_color)
                .opacity(0.6)
                .child("copied")
                .into_any_element()
        } else {
            div()
                .px(px(5.0))
                .py(px(1.0))
                .rounded_sm()
                .border_1()
                .border_color(styles.popover_border_color)
                .text_size(px(11.0))
                .text_color(styles.popover_text_color)
                .opacity(0.72)
                .hover({
                    let hover = styles.popover_hover_background_color;
                    move |this| this.opacity(0.95).bg(hover)
                })
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                    editor.update(cx, |editor, cx| {
                        editor.copied_hover_message = Some(copy_text.clone());
                        cx.notify();
                    });
                })
                .child("copy")
                .into_any_element()
        };

        div()
            .flex()
            .absolute()
            .max_w(max_w)
            .pb(margin)
            .pt(margin)
            .child(
                div()
                    .p(padding)
                    .flex()
                    .flex_col()
                    .max_w(max_w)
                    .bg(styles.popover_background_color)
                    .rounded_md()
                    .border_1()
                    .border_color(color)
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
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(styles.popover_title_color)
                                    .child(diagnostic.title.clone()),
                            )
                            .child(copy_action),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(styles.popover_text_color)
                            .child(diagnostic.message.clone()),
                    ),
            )
            .on_mouse_move(|_, window, app| {
                window.prevent_default();
                app.stop_propagation();
            })
            .into_any_element()
    }

    fn diagnostic_copy_text(diagnostic: &Diagnostic) -> String {
        format!("{}\n{}", diagnostic.title, diagnostic.message)
    }

    pub(super) fn is_diagnostic_copied(&self, diagnostic: &Diagnostic, cx: &App) -> bool {
        self.editor.read(cx).copied_hover_message.as_deref()
            == Some(Self::diagnostic_copy_text(diagnostic).as_str())
    }
}
