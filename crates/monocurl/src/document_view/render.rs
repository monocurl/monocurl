use super::*;
use crate::components::latex_warning::render_latex_warning;

impl DocumentView {
    pub fn new(
        path: PathBuf,
        window_state: WeakEntity<WindowState>,
        dirty: Entity<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = DocumentState::new(cx);
        let services = cx.new(|cx| {
            ServiceManager::new(
                state.textual_state.clone(),
                state.execution_state.clone(),
                path.clone(),
                cx,
            )
        });

        let editor = cx.new(|cx| {
            Editor::new(
                state.textual_state.clone(),
                path.clone(),
                dirty.clone(),
                window,
                cx,
            )
        });
        let viewport = cx.new(|cx| Viewport::new(services.clone(), cx));
        let timeline = cx.new(|cx| Timeline::new(services.clone(), cx));

        let document_path = path.clone();
        let window_state_up = window_state.upgrade().unwrap();
        cx.observe(&window_state_up, move |dv, ws, cx| {
            ws.update(cx, |window_state, cx| {
                if let ActiveScreen::Document(doc) = &window_state.screen
                    && doc.path == document_path
                {
                    dv.on_imports_may_have_changed(window_state, cx);
                }
            });
        })
        .detach();
        cx.observe(&window_state_up, |_dv, _, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<ThemeSettings>(|_dv, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<UserSettings>(|dv, cx| {
            if let Some(window_state) = dv.window_state.upgrade() {
                window_state.update(cx, |window_state, cx| {
                    dv.on_imports_may_have_changed(window_state, cx);
                });
            }
            cx.notify();
        })
        .detach();

        dirty.update(cx, |dirty, _| *dirty = false);

        Self {
            path,
            was_fullscreen_before_presenting: false,
            is_presenting: false,
            is_headless: false,
            window_state: window_state.clone(),
            state,
            services,
            navbar: cx.new(move |cx| Navbar::new(window_state, cx)),
            editor: editor.clone(),
            viewport: viewport.clone(),
            timeline,
            export_overlay: ExportOverlayState::default(),
            export_cancel_flag: None,
            export_poll_task: None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn render_export_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.export_overlay.visible() {
            return None;
        }

        let theme = ThemeSettings::theme(cx);
        let Some(kind) = self.export_overlay.kind else {
            return None;
        };
        let progress = self.export_overlay.progress_ratio();
        let is_error = self.export_overlay.error.is_some();
        let is_success = self.export_overlay.succeeded();
        let is_cancelled = self.export_overlay.cancelled();
        let bar_bg = Rgba {
            a: 0.14,
            ..theme.text_primary
        };
        let bar_fill = if is_cancelled {
            theme.text_muted
        } else if is_error {
            theme.danger
        } else {
            theme.accent
        };
        let title = if self.export_overlay.running {
            kind.progress_title()
        } else if is_success {
            kind.success_title()
        } else if is_cancelled {
            kind.canceled_title()
        } else {
            kind.failure_title()
        };
        let status = if is_error {
            self.export_overlay
                .error
                .clone()
                .unwrap_or_else(|| "Export failed".to_string())
        } else {
            self.export_overlay.message.clone()
        };
        let status_lines = status
            .lines()
            .map(|line| {
                div()
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child(line.to_string())
                    .into_any_element()
            })
            .collect::<Vec<_>>();
        let counter = (self.export_overlay.total > 0).then(|| {
            format!(
                "{} / {}",
                self.export_overlay.completed, self.export_overlay.total
            )
        });
        let output_path = self.export_overlay.output_path.as_ref().map(|path| {
            div()
                .text_xs()
                .text_color(theme.text_muted)
                .child(path.display().to_string())
                .into_any_element()
        });
        let open_button = is_success.then(|| {
            div()
                .id("open-export-output")
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(theme.accent)
                .bg(theme.accent)
                .text_sm()
                .text_color(theme.viewport_stage_background)
                .hover(|style| style.opacity(0.9))
                .cursor_pointer()
                .child(kind.open_label())
                .on_click(cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_export_output(cx);
                }))
                .into_any_element()
        });
        let cancel_button = self.export_overlay.running.then(|| {
            if self.export_overlay.cancel_requested {
                div()
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.navbar_border)
                    .text_sm()
                    .text_color(theme.text_muted)
                    .child("Cancelling...")
                    .into_any_element()
            } else {
                div()
                    .id("cancel-export-overlay")
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(theme.navbar_border)
                    .text_sm()
                    .text_color(theme.text_primary)
                    .hover({
                        let hover = theme.row_hover_overlay;
                        move |style| style.bg(hover)
                    })
                    .cursor_pointer()
                    .child("Cancel")
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.request_cancel_export(window, cx);
                    }))
                    .into_any_element()
            }
        });
        let dismiss = (!self.export_overlay.running).then(|| {
            div()
                .id("dismiss-export-overlay")
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(theme.navbar_border)
                .text_sm()
                .text_color(theme.text_primary)
                .hover({
                    let hover = theme.row_hover_overlay;
                    move |style| style.bg(hover)
                })
                .cursor_pointer()
                .child("Dismiss")
                .on_click(cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.clear_export_state(cx);
                }))
                .into_any_element()
        });

        Some(
            div()
                .absolute()
                .right(px(16.0))
                .bottom(px(16.0))
                .child(
                    div()
                        .w(px(420.0))
                        .min_w(px(320.0))
                        .max_w(px(520.0))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(theme.navbar_border)
                        .bg(theme.tab_active_background)
                        .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_between()
                                .items_start()
                                .gap(px(12.0))
                                .child(div().text_sm().text_color(theme.text_primary).child(title))
                                .children(counter.map(|counter| {
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(counter)
                                        .into_any_element()
                                })),
                        )
                        .child(div().flex().flex_col().gap(px(4.0)).children(status_lines))
                        .child(
                            div().flex().flex_col().gap(px(6.0)).child(
                                div()
                                    .h(px(8.0))
                                    .w_full()
                                    .rounded(px(999.0))
                                    .bg(bar_bg)
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(progress))
                                            .rounded(px(999.0))
                                            .bg(bar_fill),
                                    ),
                            ),
                        )
                        .children(output_path)
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(8.0))
                                .justify_end()
                                .children(cancel_button)
                                .children(open_button)
                                .children(dismiss),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_presentation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .size_full()
                    .key_context("document presenter")
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(Self::toggle_presentation))
                    .on_action(cx.listener(Self::toggle_params_panel))
                    .on_action(cx.listener(Self::sync_viewport_camera))
                    .on_action(cx.listener(Self::play_or_show_pause_hint))
                    .on_action(cx.listener(Self::toggle_playing))
                    .on_action(cx.listener(Self::prev_slide))
                    .on_action(cx.listener(Self::next_slide))
                    .on_action(cx.listener(Self::scene_start))
                    .on_action(cx.listener(Self::scene_end))
                    .on_action(cx.listener(Self::epsilon_forward))
                    .on_action(cx.listener(Self::epsilon_backward))
                    .on_action(cx.listener(Self::export_image))
                    .on_action(cx.listener(Self::export_video))
                    .child(self.viewport.clone()),
            )
            .children(self.render_export_overlay(cx))
    }

    fn viewport_timeline(&self, divider_color: impl Into<Hsla>) -> Split {
        Split::new(
            Axis::Vertical,
            self.viewport.clone().into_any_element(),
            self.timeline.clone().into_any_element(),
        )
        .divider_color(divider_color)
    }

    fn render_editing(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let should_hide_editor =
            self.is_headless || window.bounds().size.width < px(AUTO_HEADLESS_WINDOW_WIDTH);
        let workspace = if should_hide_editor {
            self.viewport_timeline(theme.split_divider)
                .into_any_element()
        } else {
            Split::new(
                Axis::Horizontal,
                self.editor.clone().into_any_element(),
                self.viewport_timeline(theme.split_divider)
                    .into_any_element(),
            )
            .default_flex(0.5)
            .divider_color(theme.split_divider)
            .into_any_element()
        };

        div()
            .relative()
            .flex()
            .flex_col()
            .children(render_latex_warning(UserSettings::read(cx), theme))
            .child(self.navbar.clone())
            .child(workspace)
            .text_color(theme.text_primary)
            .bg(theme.document_background)
            .size_full()
            .key_context("document")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::toggle_presentation))
            .on_action(cx.listener(Self::toggle_timeline_console))
            .on_action(cx.listener(Self::play_or_show_pause_hint))
            .on_action(cx.listener(Self::toggle_playing))
            .on_action(cx.listener(Self::sync_viewport_camera))
            .on_action(cx.listener(Self::toggle_headless))
            .on_action(cx.listener(Self::unfocus_editor))
            .on_action(cx.listener(Self::prev_slide))
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::scene_start))
            .on_action(cx.listener(Self::scene_end))
            .on_action(cx.listener(Self::epsilon_forward))
            .on_action(cx.listener(Self::epsilon_backward))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_custom_path))
            .on_action(cx.listener(Self::export_image))
            .on_action(cx.listener(Self::export_video))
            .on_action(cx.listener(Self::close_document))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .children(self.render_export_overlay(cx))
    }
}

impl Render for DocumentView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        if window.focused(cx).is_none() {
            self.focus(window);
        }

        if self.is_presenting {
            self.render_presentation(cx).into_any_element()
        } else {
            self.render_editing(window, cx).into_any_element()
        }
    }
}
