use gpui::*;
use renderer::SceneRenderData;

use crate::{
    services::ServiceManager,
    theme::ThemeSettings,
    timeline::{slide_label, slide_title_label, visual_slide_time},
    viewport::scene_renderer::{SceneImageRevision, retired_image_limit_for_presentation},
};

use super::{
    Viewport,
    camera::CameraDragMode,
    params::parameter_controls,
    style::{
        PARAM_PANEL_W, PRES_BG, PRES_BORDER, PRES_MUTED, PRES_PANEL_BG, PRES_TEXT, PRES_TOOLBAR_BG,
        PRES_TOOLBAR_H, RING_TRANSITION, RingStyle, lerp_f32, lerp_rgba, ring_style_for,
    },
};

const VIEWPORT_FRAME_ASPECT: f32 = 16.0 / 9.0;
const VIEWPORT_FRAME_PADDING: f32 = 35.0;
const VIEWPORT_PREVIEW_CHROME_INSET_X: f32 = 8.0;
const VIEWPORT_PREVIEW_CHROME_INSET_Y: f32 = 6.0;
const PAUSE_HINT_TEXT: &str = "press shift + space to pause";
const VIEWPORT_OVERSCAN_SCRIM: Rgba = Rgba {
    r: 0.5,
    g: 0.5,
    b: 0.5,
    a: 0.6,
};

#[derive(Clone, Copy)]
enum SceneStageMode {
    Preview { ring_style: RingStyle },
    Presentation,
}

#[derive(Clone, Copy)]
struct SceneStageLayout {
    image_bounds: Bounds<Pixels>,
    interaction_bounds: Bounds<Pixels>,
    projection_bounds: Bounds<Pixels>,
    preview_ring: Option<RingStyle>,
}

impl Render for Viewport {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let (
            status,
            is_loading,
            params,
            timestamp,
            slide_count,
            slide_names,
            durations,
            background,
            scene_camera,
            meshes,
            scene_version,
        ) = {
            let execution = self.execution_state.read(cx);
            (
                execution.status,
                execution.is_loading,
                execution.parameters.clone(),
                execution.current_timestamp,
                execution.slide_count,
                execution.slide_names.clone(),
                execution.slide_durations.clone(),
                execution.background,
                execution.camera.clone(),
                execution.meshes.clone(),
                execution.scene_version,
            )
        };

        let display_camera = self.display_camera(&scene_camera);
        let scene_revision = SceneImageRevision::new(scene_version, self.viewport_camera_version);
        let show_preview_reset = self.should_show_preview_reset();
        let preview_camera_summary = self.preview_camera_summary();
        let preview_camera_copied = preview_camera_summary
            .as_deref()
            .is_some_and(|summary| self.is_preview_camera_copied(summary));
        let show_presentation_reset = self.should_show_presentation_reset(&scene_camera);
        let weak_vp = cx.weak_entity();
        let presentation_stage_background = Rgba {
            r: background.color.0,
            g: background.color.1,
            b: background.color.2,
            a: background.color.3,
        };
        let scene = SceneRenderData {
            background,
            camera: display_camera,
            meshes,
        };

        let target_ring = ring_style_for(status, self.is_presenting, is_loading, theme);
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

        if !self.is_presenting {
            let preview_chrome = render_preview_camera_chrome(
                show_preview_reset,
                preview_camera_summary,
                preview_camera_copied,
                weak_vp.clone(),
                cx,
            );
            return div()
                .relative()
                .size_full()
                .bg(theme.viewport_background)
                .child(render_scene_stage(
                    scene,
                    scene_revision,
                    theme.viewport_stage_background,
                    SceneStageMode::Preview { ring_style },
                    weak_vp.clone(),
                ))
                .child(
                    div()
                        .absolute()
                        .top(px(VIEWPORT_PREVIEW_CHROME_INSET_Y))
                        .left(px(VIEWPORT_PREVIEW_CHROME_INSET_X))
                        .right(px(VIEWPORT_PREVIEW_CHROME_INSET_X))
                        .child(preview_chrome),
                )
                .into_any_element();
        }

        let previous_ring = self.ring_previous;
        let ring_animation_id = format!("viewport-ring-{}", self.ring_animation_nonce);
        let stage = div()
            .relative()
            .flex()
            .flex_1()
            .size_full()
            .child(render_scene_stage(
                scene,
                scene_revision,
                presentation_stage_background,
                SceneStageMode::Presentation,
                weak_vp.clone(),
            ))
            .child(render_presentation_ring(
                previous_ring,
                ring_style,
                ring_animation_id,
            ));

        let services_weak: WeakEntity<ServiceManager> = self.services.downgrade();
        let controls = parameter_controls(self, params.as_ref(), services_weak, weak_vp.clone());
        let (slide_label, time_label, title_label) =
            match visual_slide_time(timestamp.slide, timestamp.time, &durations) {
                None => (
                    format!("Slide 0 / {}", slide_count.max(1)),
                    "0.00s".to_string(),
                    None,
                ),
                Some((slide, time)) => (
                    slide_label(slide, slide_count.max(1)),
                    format!("{:.2}s", time),
                    slide_title_label(slide, &slide_names),
                ),
            };
        let params_button = render_toolbar_button(
            "pres-params-btn",
            "Parameters",
            cx.listener(|viewport, _, _, cx| viewport.toggle_params(cx)),
        );
        if self.show_params {
            let reset_button = show_presentation_reset.then(|| {
                div()
                    .flex()
                    .pb(px(8.0))
                    .child(render_toolbar_button(
                        "pres-camera-reset-btn",
                        "Reset Camera",
                        cx.listener(|viewport, _, _, cx| viewport.sync_viewport_camera(cx)),
                    ))
                    .into_any_element()
            });
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
                )
                .children(self.show_pause_hint.then(|| div().flex_1()))
                .children(self.show_pause_hint.then(render_pause_hint));

            let params_body = if controls.is_empty() && reset_button.is_none() {
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
                            .child("No active mesh controls"),
                    )
                    .into_any_element()
            } else if controls.is_empty() {
                div()
                    .id("pres-params-list")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .px(px(12.0))
                    .py(px(8.0))
                    .children(reset_button)
                    .child(
                        div().flex_1().flex().items_center().justify_center().child(
                            div()
                                .text_color(PRES_MUTED)
                                .text_size(px(12.0))
                                .child("No active mesh controls"),
                        ),
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
                    .children(reset_button)
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
            .children(show_presentation_reset.then(|| {
                render_small_toolbar_button(
                    "pres-camera-reset-btn",
                    "Reset Camera",
                    cx.listener(|viewport, _, _, cx| viewport.sync_viewport_camera(cx)),
                )
            }))
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
            )
            .children(title_label.map(|title| {
                div()
                    .text_color(PRES_MUTED)
                    .text_size(px(11.0))
                    .child(title)
            }))
            .children(self.show_pause_hint.then(|| div().flex_1()))
            .children(self.show_pause_hint.then(render_pause_hint));

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
    scene_revision: SceneImageRevision,
    stage_background: Rgba,
    mode: SceneStageMode,
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
                let scene_revision = scene_revision;
                let weak_vp = weak_vp.clone();
                move |_, bounds: Bounds<Pixels>, window, _cx| {
                    let layout = scene_stage_layout(bounds, mode);
                    let retired_image_limit = retired_image_limit_for_presentation(matches!(
                        mode,
                        SceneStageMode::Presentation
                    ));
                    let scene_image = weak_vp
                        .update(_cx, |viewport, _cx| {
                            viewport.scene_image_cache.image_for(
                                &mut viewport.renderer,
                                &scene,
                                scene_revision,
                                layout.image_bounds,
                                layout.projection_bounds,
                                window.scale_factor(),
                                retired_image_limit,
                                window,
                            )
                        })
                        .ok()
                        .flatten();

                    if let Some(image) = scene_image {
                        let _ = window.paint_image(
                            layout.image_bounds,
                            Corners::all(px(0.0)),
                            image,
                            0,
                            false,
                        );
                    }
                    if let Some(ring_style) = layout.preview_ring {
                        let frame_bounds = layout.interaction_bounds;
                        paint_overscan_mask(window, bounds, frame_bounds);
                        paint_preview_frame_border(window, frame_bounds, ring_style);
                    }

                    {
                        let weak_vp = weak_vp.clone();
                        window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                            let frame_bounds = scene_stage_layout(bounds, mode).interaction_bounds;
                            if phase != DispatchPhase::Bubble
                                || !frame_bounds.contains(&event.position)
                            {
                                return;
                            }
                            let mode = match event.button {
                                MouseButton::Left if event.modifiers.shift => CameraDragMode::Pan,
                                MouseButton::Left => CameraDragMode::Orbit,
                                _ => return,
                            };
                            let local_position = point(
                                event.position.x - frame_bounds.origin.x,
                                event.position.y - frame_bounds.origin.y,
                            );
                            weak_vp
                                .update(cx, |viewport, cx| {
                                    viewport.begin_camera_drag(
                                        mode,
                                        local_position,
                                        frame_bounds.size,
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
                            let frame_bounds = scene_stage_layout(bounds, mode).interaction_bounds;
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            let local_position = point(
                                event.position.x - frame_bounds.origin.x,
                                event.position.y - frame_bounds.origin.y,
                            );
                            weak_vp
                                .update(cx, |viewport, cx| {
                                    viewport.update_camera_drag(local_position, cx);
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

fn scene_stage_layout(bounds: Bounds<Pixels>, mode: SceneStageMode) -> SceneStageLayout {
    match mode {
        SceneStageMode::Preview { ring_style } => {
            let frame_bounds = aspect_frame_bounds(bounds, VIEWPORT_FRAME_PADDING);
            SceneStageLayout {
                image_bounds: bounds,
                interaction_bounds: frame_bounds,
                projection_bounds: frame_bounds,
                preview_ring: Some(ring_style),
            }
        }
        SceneStageMode::Presentation => {
            let frame_bounds = aspect_frame_bounds(bounds, 0.0);
            SceneStageLayout {
                image_bounds: frame_bounds,
                interaction_bounds: frame_bounds,
                projection_bounds: frame_bounds,
                preview_ring: None,
            }
        }
    }
}

fn aspect_frame_bounds(bounds: Bounds<Pixels>, padding: f32) -> Bounds<Pixels> {
    let width = f32::from(bounds.size.width).max(1.0);
    let height = f32::from(bounds.size.height).max(1.0);
    let available_width = (width - padding * 2.0).max(1.0);
    let available_height = (height - padding * 2.0).max(1.0);

    let frame_width = available_width.min(available_height * VIEWPORT_FRAME_ASPECT);
    let frame_height = frame_width / VIEWPORT_FRAME_ASPECT;
    let offset_x = (width - frame_width) * 0.5;
    let offset_y = (height - frame_height) * 0.5;
    Bounds::new(
        point(
            bounds.origin.x + px(offset_x),
            bounds.origin.y + px(offset_y),
        ),
        size(px(frame_width), px(frame_height)),
    )
}

fn paint_overscan_mask(window: &mut Window, bounds: Bounds<Pixels>, frame_bounds: Bounds<Pixels>) {
    let left_w = frame_bounds.origin.x - bounds.origin.x;
    if left_w > px(0.0) {
        window.paint_quad(fill(
            Bounds::new(bounds.origin, size(left_w, bounds.size.height)),
            VIEWPORT_OVERSCAN_SCRIM,
        ));
    }

    let right_x = frame_bounds.origin.x + frame_bounds.size.width;
    let right_w = bounds.origin.x + bounds.size.width - right_x;
    if right_w > px(0.0) {
        window.paint_quad(fill(
            Bounds::new(
                point(right_x, bounds.origin.y),
                size(right_w, bounds.size.height),
            ),
            VIEWPORT_OVERSCAN_SCRIM,
        ));
    }

    let top_h = frame_bounds.origin.y - bounds.origin.y;
    if top_h > px(0.0) {
        window.paint_quad(fill(
            Bounds::new(
                point(frame_bounds.origin.x, bounds.origin.y),
                size(frame_bounds.size.width, top_h),
            ),
            VIEWPORT_OVERSCAN_SCRIM,
        ));
    }

    let bottom_y = frame_bounds.origin.y + frame_bounds.size.height;
    let bottom_h = bounds.origin.y + bounds.size.height - bottom_y;
    if bottom_h > px(0.0) {
        window.paint_quad(fill(
            Bounds::new(
                point(frame_bounds.origin.x, bottom_y),
                size(frame_bounds.size.width, bottom_h),
            ),
            VIEWPORT_OVERSCAN_SCRIM,
        ));
    }
}

fn paint_preview_frame_border(
    window: &mut Window,
    frame_bounds: Bounds<Pixels>,
    ring_style: RingStyle,
) {
    if ring_style.width <= 0.0 || ring_style.color.a <= f32::EPSILON {
        return;
    }

    let border_px = px(ring_style.width.max(1.0));
    let top = Bounds::new(
        frame_bounds.origin,
        size(frame_bounds.size.width, border_px),
    );
    let bottom = Bounds::new(
        point(
            frame_bounds.origin.x,
            frame_bounds.origin.y + frame_bounds.size.height - border_px,
        ),
        size(frame_bounds.size.width, border_px),
    );
    let left = Bounds::new(
        frame_bounds.origin,
        size(border_px, frame_bounds.size.height),
    );
    let right = Bounds::new(
        point(
            frame_bounds.origin.x + frame_bounds.size.width - border_px,
            frame_bounds.origin.y,
        ),
        size(border_px, frame_bounds.size.height),
    );
    for edge in [top, bottom, left, right] {
        window.paint_quad(fill(edge, ring_style.color));
    }
}

fn render_presentation_ring(
    previous_ring: RingStyle,
    ring_style: RingStyle,
    animation_id: String,
) -> impl IntoElement {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w_full()
        .h_full()
        .border(px(ring_style.width))
        .border_color(ring_style.color)
        .with_animation(
            ElementId::Name(animation_id.into()),
            Animation::new(RING_TRANSITION).with_easing(ease_in_out),
            move |ring, delta| {
                ring.border(px(lerp_f32(previous_ring.width, ring_style.width, delta)))
                    .border_color(lerp_rgba(previous_ring.color, ring_style.color, delta))
            },
        )
}

fn render_pause_hint() -> impl IntoElement {
    div()
        .text_color(PRES_MUTED)
        .text_size(px(11.0))
        .child(PAUSE_HINT_TEXT)
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
