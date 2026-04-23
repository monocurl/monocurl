use gpui::*;
use renderer::SceneRenderData;

use crate::{services::ServiceManager, theme::ThemeSettings};

use super::{
    Viewport,
    camera::CameraDragMode,
    params::parameter_controls,
    style::{
        PARAM_PANEL_W, PRES_BG, PRES_BORDER, PRES_MUTED, PRES_PANEL_BG, PRES_TEXT, PRES_TOOLBAR_BG,
        PRES_TOOLBAR_H, RING_TRANSITION, lerp_f32, lerp_rgba, ring_style_for,
    },
};

impl Render for Viewport {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let (status, params, timestamp, slide_count, background, scene_camera, meshes) = {
            let execution = self.execution_state.read(cx);
            (
                execution.status,
                execution.parameters.clone(),
                execution.current_timestamp,
                execution.slide_count,
                execution.background,
                execution.camera.clone(),
                execution.meshes.clone(),
            )
        };

        let display_camera = self.display_camera(&scene_camera);
        let show_preview_reset = self.should_show_preview_reset();
        let preview_camera_summary = self.preview_camera_summary();
        let preview_camera_copied = preview_camera_summary
            .as_deref()
            .is_some_and(|summary| self.is_preview_camera_copied(summary));
        let show_presentation_reset = self.should_show_presentation_reset(&scene_camera);
        let weak_vp = cx.weak_entity();
        let scene = SceneRenderData {
            background,
            camera: display_camera,
            meshes,
        };

        let target_ring = ring_style_for(status, self.is_presenting, theme);
        match self.ring_style {
            Some(current) if current != target_ring => {
                self.ring_previous = current;
                self.ring_style = Some(target_ring);
                self.ring_animation_nonce = self.ring_animation_nonce.wrapping_add(1);
            }
            Some(_) => {}
            None => {
                self.ring_previous = target_ring;
                self.ring_style = Some(target_ring);
            }
        }
        let ring_style = self.ring_style.expect("ring style should be initialized");
        let previous_ring = self.ring_previous;
        let ring_animation_id = format!("viewport-ring-{}", self.ring_animation_nonce);

        let stage = div()
            .flex()
            .flex_1()
            .size_full()
            .bg(ring_style.color)
            .p(px(ring_style.width))
            .child(render_scene_stage(
                scene,
                theme.viewport_stage_background,
                weak_vp.clone(),
            ))
            .with_animation(
                ring_animation_id,
                Animation::new(RING_TRANSITION).with_easing(ease_in_out),
                move |stage, delta| {
                    stage
                        .bg(lerp_rgba(previous_ring.color, ring_style.color, delta))
                        .p(px(lerp_f32(previous_ring.width, ring_style.width, delta)))
                },
            );

        if !self.is_presenting {
            let preview_chrome = render_preview_camera_chrome(
                show_preview_reset,
                preview_camera_summary,
                preview_camera_copied,
                weak_vp.clone(),
                cx,
            );
            return div()
                .flex()
                .flex_col()
                .size_full()
                .bg(theme.viewport_background)
                .p(px(24.0))
                .child(preview_chrome)
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(stage),
                )
                .into_any_element();
        }

        let services_weak: WeakEntity<ServiceManager> = self.services.downgrade();
        let controls = parameter_controls(self, params.as_ref(), services_weak, weak_vp.clone());
        let slide_label = format!(
            "Slide {} / {}",
            (timestamp.slide + 1).min(slide_count.max(1)),
            slide_count.max(1)
        );
        let time_label = format!("{:.2}s", timestamp.time);
        let params_button = render_toolbar_button(
            "pres-params-btn",
            "Parameters",
            cx.listener(|viewport, _, _, cx| viewport.toggle_params(cx)),
        );
        let reset_button = show_presentation_reset.then(|| {
            render_small_toolbar_button(
                "pres-camera-reset-btn",
                "Reset Camera",
                cx.listener(|viewport, _, _, cx| viewport.sync_viewport_camera(cx)),
            )
        });

        if self.show_params {
            let sidebar_header = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .h(px(PRES_TOOLBAR_H))
                .flex_shrink_0()
                .bg(PRES_TOOLBAR_BG)
                .border_b(px(1.0))
                .border_color(PRES_BORDER)
                .child(params_button)
                .children(reset_button)
                .child(
                    div()
                        .text_color(PRES_TEXT)
                        .text_size(px(12.0))
                        .child(slide_label),
                )
                .child(
                    div()
                        .text_color(PRES_MUTED)
                        .text_size(px(11.0))
                        .child(time_label),
                );

            let params_body = if controls.is_empty() {
                div()
                    .id("pres-params-list")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_color(PRES_MUTED)
                            .text_size(px(12.0))
                            .child("No active parameters"),
                    )
                    .into_any_element()
            } else {
                div()
                    .id("pres-params-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .px(px(12.0))
                    .py(px(8.0))
                    .children(controls)
                    .into_any_element()
            };

            let sidebar = div()
                .flex()
                .flex_col()
                .w(px(PARAM_PANEL_W))
                .flex_shrink_0()
                .h_full()
                .bg(PRES_BG)
                .border_r(px(1.0))
                .border_color(PRES_BORDER)
                .child(sidebar_header)
                .child(params_body);

            return div()
                .flex()
                .flex_row()
                .size_full()
                .bg(PRES_BG)
                .child(sidebar)
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(PRES_BG)
                        .p(px(24.0))
                        .child(stage),
                )
                .into_any_element();
        }

        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .h(px(PRES_TOOLBAR_H))
            .flex_shrink_0()
            .bg(PRES_TOOLBAR_BG)
            .child(params_button)
            .children(reset_button)
            .child(
                div()
                    .text_color(PRES_TEXT)
                    .text_size(px(12.0))
                    .child(slide_label),
            )
            .child(
                div()
                    .text_color(PRES_MUTED)
                    .text_size(px(11.0))
                    .child(time_label),
            );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(PRES_BG)
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(PRES_BG)
                    .p(px(24.0))
                    .child(stage),
            )
            .into_any_element()
    }
}

fn render_scene_stage(
    scene: SceneRenderData,
    stage_background: Rgba,
    weak_vp: WeakEntity<Viewport>,
) -> impl IntoElement {
    div()
        .id("viewport-scene-stage")
        .relative()
        .size_full()
        .bg(stage_background)
        .child(
            canvas(move |bounds, _, _| bounds, {
                let scene = scene.clone();
                let weak_vp = weak_vp.clone();
                move |_, bounds: Bounds<Pixels>, window, _cx| {
                    if let Ok(Some(image)) = weak_vp.update(_cx, |viewport, _cx| {
                        viewport.scene_image_cache.image_for(
                            &mut viewport.renderer,
                            &scene,
                            bounds,
                            window.scale_factor(),
                            window,
                        )
                    }) {
                        let _ = window.paint_image(bounds, Corners::all(px(0.0)), image, 0, false);
                    }

                    {
                        let weak_vp = weak_vp.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                            if phase != DispatchPhase::Bubble || !bounds.contains(&event.position) {
                                return;
                            }
                            let mode = match event.button {
                                MouseButton::Left if event.modifiers.shift => CameraDragMode::Pan,
                                MouseButton::Left => CameraDragMode::Orbit,
                                _ => return,
                            };
                            weak_vp
                                .update(cx, |viewport, cx| {
                                    viewport.begin_camera_drag(
                                        mode,
                                        event.position,
                                        bounds.size,
                                        cx,
                                    );
                                })
                                .ok();
                            cx.stop_propagation();
                        });
                    }

                    {
                        let weak_vp = weak_vp.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            weak_vp
                                .update(cx, |viewport, cx| {
                                    viewport.update_camera_drag(event.position, cx);
                                })
                                .ok();
                        });
                    }

                    {
                        let weak_vp = weak_vp.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            match event.button {
                                MouseButton::Left => {}
                                _ => return,
                            }
                            weak_vp
                                .update(cx, |viewport, cx| {
                                    viewport.end_camera_drag(cx);
                                })
                                .ok();
                        });
                    }
                }
            })
            .size_full(),
        )
}

fn render_toolbar_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(10.0))
        .py(px(3.0))
        .rounded(px(3.0))
        .bg(PRES_PANEL_BG)
        .border(px(1.0))
        .border_color(PRES_BORDER)
        .text_color(PRES_TEXT)
        .text_size(px(12.0))
        .cursor_pointer()
        .hover(|style| style.opacity(0.75))
        .child(label)
        .on_click(on_click)
}

fn render_small_toolbar_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(8.0))
        .py(px(2.0))
        .rounded(px(3.0))
        .bg(PRES_PANEL_BG)
        .border(px(1.0))
        .border_color(PRES_BORDER)
        .text_color(PRES_TEXT)
        .text_size(px(10.0))
        .cursor_pointer()
        .hover(|style| style.opacity(0.75))
        .child(label)
        .on_click(on_click)
}

fn render_preview_camera_chrome(
    show_preview_reset: bool,
    preview_camera_summary: Option<String>,
    preview_camera_copied: bool,
    weak_vp: WeakEntity<Viewport>,
    cx: &mut Context<Viewport>,
) -> AnyElement {
    let reset_button = show_preview_reset
        .then(|| {
            render_small_toolbar_button(
                "viewport-camera-reset",
                "Reset Camera",
                cx.listener(|viewport, _, _, cx| viewport.sync_viewport_camera(cx)),
            )
            .into_any_element()
        })
        .unwrap_or_else(|| div().w(px(78.0)).into_any_element());
    let camera_summary = preview_camera_summary.as_ref().map(|summary| {
        div()
            .text_color(PRES_MUTED)
            .text_size(px(10.0))
            .child(summary.clone())
            .into_any_element()
    });
    let copy_button = preview_camera_summary.map(|summary| {
        render_preview_copy_button(summary, preview_camera_copied, weak_vp).into_any_element()
    });

    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(24.0))
        .pl(px(4.0))
        .pb(px(8.0))
        .child(reset_button)
        .child(div().w(px(18.0)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w(px(0.0))
                .children(camera_summary)
                .children(copy_button),
        )
        .into_any_element()
}

fn render_preview_copy_button(
    copy_text: String,
    copied: bool,
    weak_vp: WeakEntity<Viewport>,
) -> Stateful<Div> {
    let button = div()
        .id("viewport-camera-copy")
        .px(px(4.0))
        .py(px(1.0))
        .justify_center()
        .rounded_sm()
        .border_1()
        .border_color(PRES_BORDER)
        .bg(PRES_BG)
        .text_size(px(10.0))
        .text_color(PRES_TEXT);

    if copied {
        button.child("copied")
    } else {
        button
            .hover({
                let hover = PRES_BORDER;
                move |this| this.opacity(0.95).bg(hover)
            })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                weak_vp
                    .update(cx, |viewport, cx| {
                        viewport.mark_preview_camera_copied(copy_text.clone(), cx);
                    })
                    .ok();
            })
            .child("copy")
    }
}
