use gpui::*;

use crate::theme::ThemeSettings;

pub fn init(cx: &mut App) {
    cx.set_prompt_builder(|level, message, detail, actions, handle, window, cx| {
        let prompt = cx.new(|cx| AppPrompt {
            level,
            message: message.into(),
            detail: detail.map(str::to_owned),
            actions: actions.to_vec(),
            focus_handle: cx.focus_handle(),
        });
        handle.with_view(prompt, window, cx)
    });
}

struct AppPrompt {
    level: PromptLevel,
    message: String,
    detail: Option<String>,
    actions: Vec<PromptButton>,
    focus_handle: FocusHandle,
}

impl EventEmitter<PromptResponse> for AppPrompt {}

impl Focusable for AppPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppPrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let title_color = match self.level {
            PromptLevel::Critical => theme.viewport_status_runtime_error,
            PromptLevel::Warning => theme.accent,
            PromptLevel::Info => theme.text_primary,
        };

        let prompt = div()
            .track_focus(&self.focus_handle)
            .w(px(440.0))
            .max_w(px(520.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.navbar_border)
            .bg(theme.app_background)
            .p(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .whitespace_normal()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(title_color)
                    .child(self.message.clone()),
            )
            .children(self.detail.clone().map(|detail| {
                div()
                    .w_full()
                    .whitespace_normal()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(theme.text_muted)
                    .child(detail)
            }))
            .child(div().flex().flex_row().justify_end().gap(px(8.0)).children(
                self.actions.iter().enumerate().map(|(ix, action)| {
                    div()
                        .id(ix)
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(theme.navbar_border)
                        .bg(theme.tab_active_background)
                        .text_size(px(12.0))
                        .text_color(theme.text_primary)
                        .cursor_pointer()
                        .hover(|style| style.opacity(0.85))
                        .child(action.label().clone())
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(PromptResponse(ix));
                            cx.stop_propagation();
                        }))
                }),
            ));

        div()
            .relative()
            .size_full()
            .child(div().absolute().size_full().bg(Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.45,
            }))
            .child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(prompt),
            )
    }
}
