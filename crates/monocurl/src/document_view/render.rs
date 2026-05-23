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
                services.clone(),
                path.clone(),
                dirty.clone(),
                window,
                cx,
            )
        });
        let viewport = cx.new(|cx| Viewport::new(services.clone(), cx));
        let timeline = cx.new(|cx| Timeline::new(services.clone(), editor.downgrade(), cx));

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
            presentation_window: None,
            is_headless: false,
            controls_window: None,
            window_state: window_state.clone(),
            state,
            services,
            navbar: cx.new(move |cx| Navbar::new(window_state, cx)),
            editor: editor.clone(),
            viewport: viewport.clone(),
            timeline,
            export_overlay: ExportOverlayState::default(),
            export_settings_modal: None,
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
        let kind = self.export_overlay.kind?;
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

    fn render_export_settings_modal(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let modal = self.export_settings_modal.as_ref()?;
        let theme = ThemeSettings::theme(cx);
        let settings = modal.settings();
        let size_label = format!(
            "{}x{}",
            settings.render_size.width, settings.render_size.height
        );
        let aspect_label = format!("{:.3}", modal.aspect_ratio);
        let title = format!("{} Export Settings", modal.kind.action_label());

        let resolution_buttons = ExportResolutionPreset::ALL
            .into_iter()
            .map(|preset| {
                let selected = preset == modal.resolution;
                div()
                    .id(match preset {
                        ExportResolutionPreset::Small => "export-resolution-small",
                        ExportResolutionPreset::Medium => "export-resolution-medium",
                        ExportResolutionPreset::Large => "export-resolution-large",
                    })
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(if selected {
                        theme.accent
                    } else {
                        theme.navbar_border
                    })
                    .bg(if selected {
                        theme.accent
                    } else {
                        theme.tab_active_background
                    })
                    .text_sm()
                    .text_color(if selected {
                        theme.viewport_stage_background
                    } else {
                        theme.text_primary
                    })
                    .hover({
                        let hover = theme.row_hover_overlay;
                        move |style| {
                            if selected { style } else { style.bg(hover) }
                        }
                    })
                    .cursor_pointer()
                    .child(preset.label())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.set_export_resolution(preset, cx);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let fps_controls = modal.kind.uses_video_settings().then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.text_muted)
                        .child("Frame Rate"),
                )
                .child(div().flex().flex_row().gap(px(6.0)).children(
                    [30_u32, 60_u32].into_iter().map(|fps| {
                        let selected = fps == modal.fps;
                        div()
                            .id(if fps == 30 {
                                "export-fps-30"
                            } else {
                                "export-fps-60"
                            })
                            .px(px(10.0))
                            .py(px(5.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(if selected {
                                theme.accent
                            } else {
                                theme.navbar_border
                            })
                            .bg(if selected {
                                theme.accent
                            } else {
                                theme.tab_active_background
                            })
                            .text_sm()
                            .text_color(if selected {
                                theme.viewport_stage_background
                            } else {
                                theme.text_primary
                            })
                            .hover({
                                let hover = theme.row_hover_overlay;
                                move |style| {
                                    if selected { style } else { style.bg(hover) }
                                }
                            })
                            .cursor_pointer()
                            .child(format!("{fps} fps"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.set_export_fps(fps, cx);
                            }))
                            .into_any_element()
                    }),
                ))
                .into_any_element()
        });

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.35,
                })
                .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .w(px(420.0))
                        .max_w(px(520.0))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(theme.navbar_border)
                        .bg(theme.tab_active_background)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(div().text_sm().text_color(theme.text_primary).child(title))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child(modal.output_path.display().to_string()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_muted)
                                        .child("Resolution"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap(px(6.0))
                                        .children(resolution_buttons),
                                ),
                        )
                        .children(fps_controls)
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_between()
                                .gap(px(10.0))
                                .child(
                                    div().text_xs().text_color(theme.text_muted).child(format!(
                                        "{} px, aspect {}",
                                        size_label, aspect_label
                                    )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .id("export-settings-cancel")
                                                .px(px(10.0))
                                                .py(px(5.0))
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
                                                    this.dismiss_export_settings(cx);
                                                })),
                                        )
                                        .child(
                                            div()
                                                .id("export-settings-confirm")
                                                .px(px(10.0))
                                                .py(px(5.0))
                                                .rounded(px(4.0))
                                                .border_1()
                                                .border_color(theme.accent)
                                                .bg(theme.accent)
                                                .text_sm()
                                                .text_color(theme.viewport_stage_background)
                                                .hover(|style| style.opacity(0.9))
                                                .cursor_pointer()
                                                .child("Export")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    window.prevent_default();
                                                    cx.stop_propagation();
                                                    this.confirm_export_settings(cx);
                                                })),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_presentation(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let controls_open = self.controls_window_is_open(cx);
        let audience = self.viewport.update(cx, |viewport, cx| {
            viewport.render_presentation_audience(!controls_open, cx)
        });
        let audience_status = if controls_open {
            None
        } else {
            Some(self.viewport.update(cx, |viewport, cx| {
                (
                    viewport.presentation_status_labels(cx),
                    viewport.has_presentation_camera_override(),
                )
            }))
        };

        div()
            .relative()
            .size_full()
            .font_family(FontSet::UI)
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
                    .on_action(cx.listener(Self::export_slides_as_videos))
                    .child(audience),
            )
            .children(audience_status.map(|(status, show_reset)| {
                render_presentation_audience_toolbar(
                    status,
                    cx.listener(Self::open_controls_window_action),
                    cx.listener(Self::reset_presentation_camera_action),
                    show_reset,
                )
            }))
            .children(self.render_export_overlay(cx))
            .children(self.render_export_settings_modal(cx))
    }

    fn viewport_timeline(&self, divider_color: impl Into<Hsla>) -> Split {
        Split::new(
            Axis::Vertical,
            self.viewport.clone().into_any_element(),
            self.timeline.clone().into_any_element(),
        )
        .divider_color(divider_color)
    }

    fn render_editor(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("main-editor-panel")
            .size_full()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                let editor_contains_focus = this.editor.read(cx).contains_focused(window, cx);
                if editor_contains_focus {
                    this.focus_document_shell(window, cx);
                }
            }))
            .child(self.editor.clone())
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
                self.render_editor(cx).into_any_element(),
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
            .on_action(cx.listener(Self::open_find))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_custom_path))
            .on_action(cx.listener(Self::export_image))
            .on_action(cx.listener(Self::export_video))
            .on_action(cx.listener(Self::export_slides_as_videos))
            .on_action(cx.listener(Self::close_document))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .children(self.render_export_overlay(cx))
            .children(self.render_export_settings_modal(cx))
    }
}

fn render_presentation_controls_button(
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id("presentation-controls-button")
        .px(px(8.0))
        .py(px(3.0))
        .rounded(px(2.0))
        .bg(Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.42,
        })
        .border(px(1.0))
        .border_color(Rgba {
            r: 0.55,
            g: 0.55,
            b: 0.55,
            a: 0.38,
        })
        .text_color(Rgba {
            r: 0.86,
            g: 0.86,
            b: 0.86,
            a: 0.72,
        })
        .text_size(px(11.0))
        .cursor_pointer()
        .opacity(0.86)
        .hover(|style| style.opacity(1.0))
        .child("Controls")
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_click(on_click)
}

fn render_presentation_audience_toolbar(
    status: (String, String, Option<String>, bool),
    on_controls_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_reset_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    show_reset: bool,
) -> impl IntoElement {
    let (slide_label, time_label, title_label, show_pause_hint) = status;

    div()
        .id("presentation-audience-toolbar")
        .absolute()
        .top(px(8.0))
        .left(px(8.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .font_family(FontSet::UI)
        .child(render_presentation_controls_button(on_controls_click))
        .children(show_reset.then(|| render_presentation_reset_button(on_reset_click)))
        .child(
            div()
                .text_color(Rgba {
                    r: 0.86,
                    g: 0.86,
                    b: 0.86,
                    a: 0.78,
                })
                .text_size(px(12.0))
                .child(slide_label),
        )
        .child(
            div()
                .text_color(Rgba {
                    r: 0.68,
                    g: 0.68,
                    b: 0.68,
                    a: 0.78,
                })
                .text_size(px(11.0))
                .child(time_label),
        )
        .children(title_label.map(|title| {
            div()
                .text_color(Rgba {
                    r: 0.68,
                    g: 0.68,
                    b: 0.68,
                    a: 0.72,
                })
                .text_size(px(11.0))
                .child(title)
        }))
        .children(show_pause_hint.then(|| div().flex_1()))
        .children(show_pause_hint.then(|| {
            div()
                .text_color(Rgba {
                    r: 0.68,
                    g: 0.68,
                    b: 0.68,
                    a: 0.72,
                })
                .text_size(px(11.0))
                .child("press shift + space to pause")
        }))
}

fn render_presentation_reset_button(
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id("presentation-reset-camera-button")
        .px(px(8.0))
        .py(px(3.0))
        .rounded(px(2.0))
        .bg(Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.42,
        })
        .border(px(1.0))
        .border_color(Rgba {
            r: 0.55,
            g: 0.55,
            b: 0.55,
            a: 0.38,
        })
        .text_color(Rgba {
            r: 0.86,
            g: 0.86,
            b: 0.86,
            a: 0.72,
        })
        .text_size(px(11.0))
        .cursor_pointer()
        .opacity(0.86)
        .hover(|style| style.opacity(1.0))
        .child("Reset Camera")
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_click(on_click)
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
            self.close_controls_window(window, cx);
            self.render_editing(window, cx).into_any_element()
        }
    }
}
