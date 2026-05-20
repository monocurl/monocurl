use executor::transcript::SectionTranscript;
use gpui::*;
use renderer::SceneRenderData;
use std::sync::Arc;

use crate::{
    services::ServiceManager,
    theme::{FontSet, ThemeSettings},
    timeline::{slide_label, slide_title_label, visual_slide_time},
    viewport::scene_renderer::{SceneImageRevision, retired_image_limit_for_presentation},
};

use super::{
    AspectRatioPreset, Viewport,
    camera::CameraDragMode,
    params::parameter_controls,
    style::{
        PARAM_PANEL_W, PRES_BG, PRES_BORDER, PRES_MUTED, PRES_PANEL_BG, PRES_TEXT, PRES_TOOLBAR_BG,
        PRES_TOOLBAR_H, RING_TRANSITION, RingStyle, TRANSPARENT, lerp_f32, lerp_rgba,
        ring_style_for,
    },
};

const VIEWPORT_FRAME_PADDING: f32 = 35.0;
const ASPECT_CONTROL_PANEL_PAD: f32 = 2.0;
const ASPECT_CONTROL_PANEL_GAP: f32 = 2.0;
const ASPECT_CONTROL_BUTTON_W: f32 = 18.0;
const ASPECT_CONTROL_BUTTON_H: f32 = 16.0;
const VIEWPORT_PREVIEW_CHROME_INSET_X: f32 = 8.0;
const VIEWPORT_PREVIEW_CHROME_INSET_Y: f32 = 6.0;
const PAUSE_HINT_TEXT: &str = "press shift + space to pause";
const PRES_LAYOUT_PAD: f32 = 16.0;
const PRES_LAYOUT_GAP: f32 = 16.0;
const PRES_MIN_NOTES_H: f32 = 140.0;
const AUDIENCE_ASPECT_RATIO: f32 = 16.0 / 9.0;
const AUDIENCE_LETTERBOX_BG: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const VIEWPORT_OVERSCAN_SCRIM: Rgba = Rgba {
    r: 0.5,
    g: 0.5,
    b: 0.5,
    a: 0.6,
};

#[derive(Clone, Copy)]
enum SceneStageMode {
    Preview {
        ring_style: RingStyle,
        aspect_ratio: f32,
    },
    Presentation {
        aspect_ratio: f32,
        interactive: bool,
    },
}

impl SceneStageMode {
    fn is_presentation(self) -> bool {
        matches!(self, Self::Presentation { .. })
    }

    fn is_interactive(self) -> bool {
        match self {
            Self::Preview { .. } => true,
            Self::Presentation { interactive, .. } => interactive,
        }
    }
}

#[derive(Clone, Copy)]
struct SceneStageLayout {
    image_bounds: Bounds<Pixels>,
    interaction_bounds: Bounds<Pixels>,
    projection_bounds: Bounds<Pixels>,
    preview_ring: Option<RingStyle>,
}

#[derive(Clone, Copy)]
enum SceneStageCache {
    Main,
    Audience,
}

impl Viewport {
    pub(crate) fn presentation_status_labels(
        &self,
        cx: &mut Context<Self>,
    ) -> (String, String, Option<String>, bool) {
        let execution = self.execution_state.read(cx);
        let (slide_label, time_label, title_label) = match visual_slide_time(
            execution.current_timestamp.slide,
            execution.current_timestamp.time,
            &execution.slide_durations,
        ) {
            None => (
                format!("Slide 0 / {}", execution.slide_count.max(1)),
                "0.00s".to_string(),
                None,
            ),
            Some((slide, time)) => (
                slide_label(slide, execution.slide_count.max(1)),
                format!("{:.2}s", time),
                slide_title_label(slide, &execution.slide_names),
            ),
        };

        (slide_label, time_label, title_label, self.show_pause_hint)
    }

    pub(crate) fn render_presentation_audience(
        &mut self,
        interactive: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (background, scene_camera, meshes, scene_version) = {
            let execution = self.execution_state.read(cx);
            (
                execution.background,
                execution.camera.clone(),
                execution.meshes.clone(),
                execution.scene_version,
            )
        };
        let display_camera = self.display_camera(&scene_camera);
        let scene = SceneRenderData {
            background,
            camera: display_camera,
            meshes,
        };
        let scene_revision = SceneImageRevision::new(scene_version, self.viewport_camera_version);

        div()
            .size_full()
            .bg(AUDIENCE_LETTERBOX_BG)
            .child(render_scene_stage(
                scene,
                scene_revision,
                AUDIENCE_LETTERBOX_BG,
                SceneStageMode::Presentation {
                    aspect_ratio: AUDIENCE_ASPECT_RATIO,
                    interactive,
                },
                SceneStageCache::Audience,
                cx.weak_entity(),
            ))
            .into_any_element()
    }
}

impl Render for Viewport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let (
            status,
            is_loading,
            params,
            transcript,
            timestamp,
            slide_count,
            background,
            scene_camera,
            meshes,
            scene_version,
            aspect_preset,
        ) = {
            let execution = self.execution_state.read(cx);
            (
                execution.status,
                execution.is_loading,
                execution.parameters.clone(),
                execution.transcript.clone(),
                execution.current_timestamp,
                execution.slide_count,
                execution.background,
                execution.camera.clone(),
                execution.meshes.clone(),
                execution.scene_version,
                self.aspect_preset,
            )
        };
        let aspect_ratio = aspect_preset.aspect_ratio();

        let display_camera = self.display_camera(&scene_camera);
        let scene_revision = SceneImageRevision::new(scene_version, self.viewport_camera_version);
        let show_preview_reset = self.should_show_preview_reset();
        let preview_camera_summary = self.preview_camera_summary();
        let preview_camera_copied = preview_camera_summary
            .as_deref()
            .is_some_and(|summary| self.is_preview_camera_copied(summary));
        let show_presentation_reset = self.should_show_presentation_reset();
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
                .font_family(FontSet::UI)
                .bg(theme.viewport_background)
                .child(render_scene_stage(
                    scene,
                    scene_revision,
                    theme.viewport_stage_background,
                    SceneStageMode::Preview {
                        ring_style,
                        aspect_ratio,
                    },
                    SceneStageCache::Main,
                    weak_vp.clone(),
                ))
                .child(render_preview_aspect_controls(aspect_preset, weak_vp, cx))
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
        let stage_height = presentation_stage_height(window, aspect_ratio);
        let stage = div()
            .relative()
            .flex()
            .flex_1()
            .size_full()
            .border(px(1.0))
            .border_color(PRES_BORDER)
            .child(render_scene_stage(
                scene,
                scene_revision,
                presentation_stage_background,
                SceneStageMode::Presentation {
                    aspect_ratio,
                    interactive: true,
                },
                SceneStageCache::Main,
                weak_vp.clone(),
            ))
            .child(render_presentation_ring(
                previous_ring,
                ring_style,
                ring_animation_id,
            ));

        let services_weak: WeakEntity<ServiceManager> = self.services.downgrade();
        let controls = parameter_controls(self, params.as_ref(), services_weak, weak_vp.clone());
        let notes = current_slide_notes(&transcript, timestamp.slide, slide_count);
        let (slide_label, time_label, title_label, show_pause_hint) =
            self.presentation_status_labels(cx);

        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .h(px(PRES_TOOLBAR_H))
            .flex_shrink_0()
            .bg(PRES_TOOLBAR_BG)
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
            .children(show_pause_hint.then(|| div().flex_1()))
            .children(show_pause_hint.then(render_pause_hint));

        let notes_panel = render_notes_panel(notes);
        let params_panel = render_parameters_panel(controls, &self.scroll_handle);

        div()
            .flex()
            .flex_col()
            .size_full()
            .font_family(FontSet::UI)
            .bg(PRES_BG)
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .min_h_0()
                    .gap(px(PRES_LAYOUT_GAP))
                    .bg(PRES_BG)
                    .p(px(PRES_LAYOUT_PAD))
                    .child(params_panel)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .min_h_0()
                            .gap(px(PRES_LAYOUT_GAP))
                            .child(
                                div()
                                    .h(px(stage_height))
                                    .flex_shrink_0()
                                    .min_h_0()
                                    .child(stage),
                            )
                            .child(notes_panel),
                    ),
            )
            .into_any_element()
    }
}

fn presentation_stage_height(window: &Window, aspect_ratio: f32) -> f32 {
    let window_size = window.bounds().size;
    let width = f32::from(window_size.width).max(1.0);
    let height = f32::from(window_size.height).max(1.0);
    let param_width = PARAM_PANEL_W + 48.0;
    let stage_width = (width - PRES_LAYOUT_PAD * 2.0 - PRES_LAYOUT_GAP - param_width).max(1.0);
    let body_height = (height - PRES_TOOLBAR_H - PRES_LAYOUT_PAD * 2.0).max(1.0);
    let ideal_stage_height = stage_width / aspect_ratio.max(0.1);
    let max_stage_height = (body_height - PRES_LAYOUT_GAP - PRES_MIN_NOTES_H).max(1.0);

    ideal_stage_height.min(max_stage_height).max(1.0)
}

fn current_slide_notes(
    transcript: &[Arc<SectionTranscript>],
    slide: usize,
    slide_count: usize,
) -> Vec<String> {
    let slide = if slide_count == 0 {
        slide
    } else {
        slide.min(slide_count)
    };

    transcript
        .iter()
        .flat_map(|section| section.entries.iter())
        .filter(|entry| entry.root_slide_index == Some(slide))
        .map(|entry| entry.text().to_string())
        .collect()
}

fn render_panel_header(label: &'static str) -> impl IntoElement {
    div()
        .h(px(32.0))
        .flex()
        .items_center()
        .px(px(10.0))
        .flex_shrink_0()
        .border_b(px(1.0))
        .border_color(PRES_BORDER)
        .bg(PRES_TOOLBAR_BG)
        .text_color(PRES_TEXT)
        .text_size(px(12.0))
        .child(label)
}

fn render_notes_panel(notes: Vec<String>) -> AnyElement {
    let rows = if notes.is_empty() {
        vec![
            div()
                .text_color(PRES_MUTED)
                .child("(no transcript)")
                .into_any_element(),
        ]
    } else {
        notes
            .into_iter()
            .flat_map(|note| {
                let mut rows = Vec::new();
                let mut emitted = false;
                for line in note.lines() {
                    emitted = true;
                    rows.push(
                        div()
                            .w_full()
                            .text_color(PRES_TEXT)
                            .child(if line.is_empty() {
                                " ".to_string()
                            } else {
                                line.to_string()
                            })
                            .into_any_element(),
                    );
                }
                if !emitted {
                    rows.push(
                        div()
                            .w_full()
                            .text_color(PRES_TEXT)
                            .child(" ")
                            .into_any_element(),
                    );
                }
                rows
            })
            .collect()
    };

    div()
        .id("pres-notes-panel")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .border(px(1.0))
        .border_color(PRES_BORDER)
        .bg(PRES_PANEL_BG)
        .child(render_panel_header("Slide Transcript"))
        .child(
            div()
                .id("pres-notes-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(10.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .line_height(px(17.0))
                .children(rows),
        )
        .into_any_element()
}

fn render_parameters_panel(controls: Vec<AnyElement>, scroll: &ScrollHandle) -> AnyElement {
    let body = if controls.is_empty() {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .px(px(12.0))
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
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(scroll)
            .px(px(12.0))
            .py(px(8.0))
            .children(controls)
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .w(px(PARAM_PANEL_W + 48.0))
        .flex_shrink_0()
        .min_h_0()
        .border(px(1.0))
        .border_color(PRES_BORDER)
        .bg(PRES_BG)
        .child(render_panel_header("Parameters"))
        .child(body)
        .into_any_element()
}

fn render_scene_stage(
    scene: SceneRenderData,
    scene_revision: SceneImageRevision,
    stage_background: Rgba,
    mode: SceneStageMode,
    cache: SceneStageCache,
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
                    let layout = scene_stage_layout(bounds, mode);
                    let retired_image_limit =
                        retired_image_limit_for_presentation(mode.is_presentation());
                    let scene_image = weak_vp
                        .update(_cx, |viewport, _cx| {
                            let (renderer, image_cache) = match cache {
                                SceneStageCache::Main => {
                                    (&mut viewport.renderer, &mut viewport.scene_image_cache)
                                }
                                SceneStageCache::Audience => (
                                    &mut viewport.audience_renderer,
                                    &mut viewport.audience_scene_image_cache,
                                ),
                            };
                            image_cache.image_for(
                                renderer,
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

                    if mode.is_interactive() {
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

                    if mode.is_interactive() {
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

                    if mode.is_interactive() {
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
        SceneStageMode::Preview {
            ring_style,
            aspect_ratio,
        } => {
            let frame_bounds = aspect_frame_bounds(bounds, VIEWPORT_FRAME_PADDING, aspect_ratio);
            SceneStageLayout {
                image_bounds: bounds,
                interaction_bounds: frame_bounds,
                projection_bounds: frame_bounds,
                preview_ring: Some(ring_style),
            }
        }
        SceneStageMode::Presentation { aspect_ratio, .. } => {
            let frame_bounds = aspect_frame_bounds(bounds, 0.0, aspect_ratio);
            SceneStageLayout {
                image_bounds: frame_bounds,
                interaction_bounds: frame_bounds,
                projection_bounds: frame_bounds,
                preview_ring: None,
            }
        }
    }
}

fn aspect_frame_bounds(bounds: Bounds<Pixels>, padding: f32, aspect_ratio: f32) -> Bounds<Pixels> {
    let width = f32::from(bounds.size.width).max(1.0);
    let height = f32::from(bounds.size.height).max(1.0);
    let available_width = (width - padding * 2.0).max(1.0);
    let available_height = (height - padding * 2.0).max(1.0);

    let aspect_ratio = aspect_ratio.max(0.1);
    let frame_width = available_width.min(available_height * aspect_ratio);
    let frame_height = frame_width / aspect_ratio;
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

fn render_preview_aspect_controls(
    active: AspectRatioPreset,
    weak_vp: WeakEntity<Viewport>,
    cx: &mut Context<Viewport>,
) -> impl IntoElement {
    let theme = ThemeSettings::theme(cx);
    canvas(move |bounds, _, _| bounds, {
        let weak_vp = weak_vp.clone();
        move |_, bounds: Bounds<Pixels>, window, _cx| {
            let panel_bounds = aspect_control_panel_bounds(bounds);

            window.paint_quad(quad(
                panel_bounds,
                px(4.0),
                with_alpha(theme.tab_active_background, 0.62),
                px(1.0),
                with_alpha(theme.navbar_border, 0.6),
                BorderStyle::Solid,
            ));

            for preset in AspectRatioPreset::ALL {
                let selected = preset == active;
                let button_bounds = aspect_control_button_bounds(panel_bounds, preset);
                let icon_bounds = aspect_control_icon_bounds(button_bounds, preset);

                if selected {
                    window.paint_quad(quad(
                        button_bounds,
                        px(2.0),
                        with_alpha(theme.accent, 0.16),
                        px(0.0),
                        TRANSPARENT,
                        BorderStyle::Solid,
                    ));
                }

                window.paint_quad(quad(
                    icon_bounds,
                    px(1.0),
                    with_alpha(theme.viewport_stage_background, 0.52),
                    px(1.0),
                    if selected {
                        with_alpha(theme.accent, 0.92)
                    } else {
                        with_alpha(theme.text_muted, 0.72)
                    },
                    BorderStyle::Solid,
                ));
            }

            {
                let weak_vp = weak_vp.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                        return;
                    }

                    for preset in AspectRatioPreset::ALL {
                        if aspect_control_button_bounds(panel_bounds, preset)
                            .contains(&event.position)
                        {
                            weak_vp
                                .update(cx, |viewport, cx| viewport.set_aspect_preset(preset, cx))
                                .ok();
                            cx.stop_propagation();
                            return;
                        }
                    }
                });
            }
        }
    })
    .absolute()
    .top(px(0.0))
    .bottom(px(0.0))
    .left(px(0.0))
    .right(px(0.0))
}

fn aspect_control_panel_bounds(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let bounds_w = f32::from(bounds.size.width);
    let bounds_h = f32::from(bounds.size.height);
    let panel_w = ASPECT_CONTROL_BUTTON_W + 2.0 * ASPECT_CONTROL_PANEL_PAD;
    let panel_h = ASPECT_CONTROL_BUTTON_H * AspectRatioPreset::ALL.len() as f32
        + ASPECT_CONTROL_PANEL_GAP * (AspectRatioPreset::ALL.len().saturating_sub(1)) as f32
        + 2.0 * ASPECT_CONTROL_PANEL_PAD;

    let right_offset = ((VIEWPORT_FRAME_PADDING - panel_w) * 0.5).max(0.0);
    let panel_x = (bounds_w - right_offset - panel_w).clamp(0.0, (bounds_w - panel_w).max(0.0));
    let panel_y = ((bounds_h - panel_h) * 0.5).clamp(0.0, (bounds_h - panel_h).max(0.0));

    Bounds::new(
        point(bounds.origin.x + px(panel_x), bounds.origin.y + px(panel_y)),
        size(px(panel_w), px(panel_h)),
    )
}

fn aspect_control_button_bounds(
    panel_bounds: Bounds<Pixels>,
    preset: AspectRatioPreset,
) -> Bounds<Pixels> {
    let index = aspect_preset_index(preset) as f32;
    Bounds::new(
        point(
            panel_bounds.origin.x + px(ASPECT_CONTROL_PANEL_PAD),
            panel_bounds.origin.y
                + px(ASPECT_CONTROL_PANEL_PAD
                    + index * (ASPECT_CONTROL_BUTTON_H + ASPECT_CONTROL_PANEL_GAP)),
        ),
        size(px(ASPECT_CONTROL_BUTTON_W), px(ASPECT_CONTROL_BUTTON_H)),
    )
}

fn aspect_control_icon_bounds(
    button_bounds: Bounds<Pixels>,
    preset: AspectRatioPreset,
) -> Bounds<Pixels> {
    let (icon_w, icon_h) = aspect_preset_icon_size(preset);
    Bounds::new(
        point(
            button_bounds.origin.x + px((ASPECT_CONTROL_BUTTON_W - icon_w) * 0.5),
            button_bounds.origin.y + px((ASPECT_CONTROL_BUTTON_H - icon_h) * 0.5),
        ),
        size(px(icon_w), px(icon_h)),
    )
}

fn aspect_preset_index(preset: AspectRatioPreset) -> usize {
    match preset {
        AspectRatioPreset::Wide => 0,
        AspectRatioPreset::Standard => 1,
        AspectRatioPreset::Square => 2,
        AspectRatioPreset::FeedPortrait => 3,
        AspectRatioPreset::Portrait => 4,
        AspectRatioPreset::Ultrawide => 5,
    }
}

fn aspect_preset_icon_size(preset: AspectRatioPreset) -> (f32, f32) {
    match preset {
        AspectRatioPreset::Wide => (14.0, 8.0),
        AspectRatioPreset::Standard => (13.0, 10.0),
        AspectRatioPreset::Square => (10.0, 10.0),
        AspectRatioPreset::FeedPortrait => (9.0, 12.0),
        AspectRatioPreset::Portrait => (7.0, 12.0),
        AspectRatioPreset::Ultrawide => (15.0, 6.0),
    }
}

fn with_alpha(color: Rgba, a: f32) -> Rgba {
    Rgba { a, ..color }
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
    let reset_button = if show_preview_reset {
        {
            render_small_toolbar_button(
                "viewport-camera-reset",
                "Reset Camera",
                cx.listener(|viewport, _, _, cx| viewport.sync_viewport_camera(cx)),
            )
            .into_any_element()
        }
    } else {
        div().w(px(78.0)).into_any_element()
    };
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
