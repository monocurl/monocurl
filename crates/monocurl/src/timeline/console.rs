use executor::transcript::TranscriptEntry;
use gpui::*;

use crate::theme::{FontSet, Theme};

pub(super) fn render_console(
    entries: Vec<TranscriptEntry>,
    runtime_errors: Vec<String>,
    scroll: &ScrollHandle,
    theme: Theme,
) -> impl IntoElement {
    let font = font(FontSet::MONOSPACE);
    let mut rows = Vec::new();

    for entry in entries {
        rows.push(
            div()
                .w_full()
                .child(entry.text().to_string())
                .into_any_element(),
        );
    }

    if !runtime_errors.is_empty() {
        if !rows.is_empty() {
            rows.push(div().h(px(4.0)).into_any_element());
        }

        for message in runtime_errors {
            let mut emitted = false;
            for (line_ix, line) in message.lines().enumerate() {
                emitted = true;
                let text = if line_ix == 0 {
                    format!("runtime error: {}", line)
                } else if line.is_empty() {
                    " ".into()
                } else {
                    format!("  {}", line)
                };
                rows.push(
                    div()
                        .w_full()
                        .text_color(theme.viewport_status_runtime_error)
                        .child(text)
                        .into_any_element(),
                );
            }

            if !emitted {
                rows.push(
                    div()
                        .w_full()
                        .text_color(theme.viewport_status_runtime_error)
                        .child("runtime error")
                        .into_any_element(),
                );
            }
        }
    }

    if rows.is_empty() {
        rows.push(
            div()
                .text_color(theme.timeline_subtext)
                .child("(no print output)")
                .into_any_element(),
        );
    }

    div()
        .id("tl-console-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .h_full()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .bg(theme.timeline_background)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .w_full()
                .px(px(8.0))
                .py(px(4.0))
                .gap(px(2.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(theme.timeline_text)
                .font(font)
                .children(rows),
        )
}
