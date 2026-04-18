use std::collections::HashMap;

use gpui::*;

use crate::{
    services::{ParameterValue, ServiceManager},
    theme::ThemeSettings,
};

// presentation overlay colors (always dark, independent of theme)
const PRES_BG: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
const PRES_TOOLBAR_BG: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.8 };
const PRES_PANEL_BG: Rgba = Rgba { r: 0.10, g: 0.10, b: 0.10, a: 1.0 };
const PRES_BORDER: Rgba = Rgba { r: 0.22, g: 0.22, b: 0.22, a: 1.0 };
const PRES_TEXT: Rgba = Rgba { r: 0.85, g: 0.85, b: 0.85, a: 1.0 };
const PRES_MUTED: Rgba = Rgba { r: 0.50, g: 0.50, b: 0.50, a: 1.0 };
const PRES_ACCENT: Rgba = Rgba { r: 0.47, g: 0.63, b: 0.87, a: 1.0 };
const SLIDER_TRACK_BG: Rgba = Rgba { r: 0.28, g: 0.28, b: 0.28, a: 1.0 };
const SLIDER_THUMB: Rgba = Rgba { r: 0.90, g: 0.90, b: 0.90, a: 1.0 };
const SLIDER_2D_BG: Rgba = Rgba { r: 0.18, g: 0.18, b: 0.18, a: 1.0 };
const SLIDER_2D_AXIS: Rgba = Rgba { r: 0.32, g: 0.32, b: 0.32, a: 1.0 };
const TRANSPARENT: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

const PRES_TOOLBAR_H: f32 = 32.0;
const PARAM_PANEL_MAX_H: f32 = 240.0;
const SLIDER_TRACK_H: f32 = 4.0;
const SLIDER_THUMB_R: f32 = 7.0;
const SLIDER_1D_ROW_H: f32 = 36.0;
const SLIDER_1D_MIN: f64 = -10.0;
const SLIDER_1D_MAX: f64 = 10.0;
const SLIDER_2D_SIZE: f32 = 120.0;
const SLIDER_2D_MIN: f64 = -1.0;
const SLIDER_2D_MAX: f64 = 1.0;
const SLIDER_2D_DOT_R: f32 = 5.0;

#[derive(Clone, Copy)]
enum Slider2dKind {
    Complex,
    VectorFloat,
}

pub struct Viewport {
    services: Entity<ServiceManager>,
    is_presenting: bool,
    show_params: bool,
    dragging_param: Option<String>,
    scroll_handle: ScrollHandle,
}

impl Viewport {
    pub fn new(services: Entity<ServiceManager>, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self {
            services,
            is_presenting: false,
            show_params: false,
            dragging_param: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn set_presenting(&mut self, presenting: bool, cx: &mut Context<Self>) {
        self.is_presenting = presenting;
        if !presenting {
            self.show_params = false;
            self.dragging_param = None;
        }
        cx.notify();
    }

    pub fn toggle_params(&mut self, cx: &mut Context<Self>) {
        self.show_params = !self.show_params;
        cx.notify();
    }
}

// --- slider helpers ---

fn render_slider_1d(
    name: String,
    value: f64,
    is_int: bool,
    services: WeakEntity<ServiceManager>,
    weak_vp: WeakEntity<Viewport>,
) -> impl IntoElement {
    let pct = ((value - SLIDER_1D_MIN) / (SLIDER_1D_MAX - SLIDER_1D_MIN)).clamp(0.0, 1.0) as f32;
    let value_text = if is_int {
        format!("{}", value as i64)
    } else {
        format!("{:.2}", value)
    };
    let name_for_canvas = name.clone();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .h(px(SLIDER_1D_ROW_H))
        .child(
            div()
                .w(px(80.0))
                .flex_shrink_0()
                .text_color(PRES_TEXT)
                .text_size(px(12.0))
                .child(name.clone()),
        )
        .child(
            div().flex_1().h_full().child(
                canvas(
                    move |bounds, _, _| bounds,
                    {
                        let name = name_for_canvas.clone();
                        let services = services.clone();
                        let weak_vp = weak_vp.clone();
                        move |_, bounds: Bounds<Pixels>, window, _cx| {
                            let w = f32::from(bounds.size.width);
                            let h = f32::from(bounds.size.height);
                            let ox = bounds.origin.x;
                            let oy = bounds.origin.y;
                            let track_y = h / 2.0 - SLIDER_TRACK_H / 2.0;

                            window.paint_quad(fill(
                                Bounds::new(
                                    point(ox, oy + px(track_y)),
                                    size(px(w), px(SLIDER_TRACK_H)),
                                ),
                                SLIDER_TRACK_BG,
                            ));
                            if pct > 0.0 {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(ox, oy + px(track_y)),
                                        size(px(w * pct), px(SLIDER_TRACK_H)),
                                    ),
                                    PRES_ACCENT,
                                ));
                            }
                            window.paint_quad(quad(
                                Bounds::new(
                                    point(
                                        ox + px(w * pct - SLIDER_THUMB_R),
                                        oy + px(h / 2.0 - SLIDER_THUMB_R),
                                    ),
                                    size(px(SLIDER_THUMB_R * 2.0), px(SLIDER_THUMB_R * 2.0)),
                                ),
                                px(SLIDER_THUMB_R),
                                SLIDER_THUMB,
                                px(0.0),
                                TRANSPARENT,
                                BorderStyle::Solid,
                            ));

                            let make_pv = move |local_x: f32| {
                                let p = (local_x / w).clamp(0.0, 1.0) as f64;
                                let raw = SLIDER_1D_MIN + p * (SLIDER_1D_MAX - SLIDER_1D_MIN);
                                let val = if is_int { raw.round() } else { raw };
                                if is_int {
                                    ParameterValue::Int(val as i64)
                                } else {
                                    ParameterValue::Float(val)
                                }
                            };

                            {
                                let name = name.clone();
                                let services = services.clone();
                                let weak_vp = weak_vp.clone();
                                window.on_mouse_event(
                                    move |ev: &MouseDownEvent, phase, _, cx| {
                                        if phase != DispatchPhase::Bubble
                                            || !bounds.contains(&ev.position)
                                        {
                                            return;
                                        }
                                        let pv = make_pv(f32::from(
                                            ev.position.x - bounds.origin.x,
                                        ));
                                        weak_vp
                                            .update(cx, |vp, cx| {
                                                vp.dragging_param = Some(name.clone());
                                                cx.notify();
                                            })
                                            .ok();
                                        services
                                            .update(cx, |s, _| {
                                                s.update_parameters(HashMap::from([(
                                                    name.clone(),
                                                    pv,
                                                )]))
                                            })
                                            .ok();
                                    },
                                );
                            }
                            {
                                let name = name.clone();
                                let services = services.clone();
                                let weak_vp = weak_vp.clone();
                                window.on_mouse_event(
                                    move |ev: &MouseMoveEvent, phase, _, cx| {
                                        if phase != DispatchPhase::Bubble {
                                            return;
                                        }
                                        let dragging = weak_vp
                                            .upgrade()
                                            .map(|e| {
                                                e.read(cx).dragging_param.as_deref()
                                                    == Some(name.as_str())
                                            })
                                            .unwrap_or(false);
                                        if !dragging {
                                            return;
                                        }
                                        let pv = make_pv(f32::from(
                                            ev.position.x - bounds.origin.x,
                                        ));
                                        services
                                            .update(cx, |s, _| {
                                                s.update_parameters(HashMap::from([(
                                                    name.clone(),
                                                    pv,
                                                )]))
                                            })
                                            .ok();
                                    },
                                );
                            }
                            {
                                let weak_vp = weak_vp.clone();
                                window.on_mouse_event(
                                    move |_: &MouseUpEvent, phase, _, cx| {
                                        if phase != DispatchPhase::Bubble {
                                            return;
                                        }
                                        weak_vp
                                            .update(cx, |vp, cx| {
                                                vp.dragging_param = None;
                                                cx.notify();
                                            })
                                            .ok();
                                    },
                                );
                            }
                        }
                    },
                )
                .w_full()
                .h(px(SLIDER_1D_ROW_H)),
            ),
        )
        .child(
            div()
                .w(px(44.0))
                .flex_shrink_0()
                .text_color(PRES_MUTED)
                .text_size(px(11.0))
                .child(value_text),
        )
}

fn render_slider_2d(
    name: String,
    x: f64,
    y: f64,
    kind: Slider2dKind,
    services: WeakEntity<ServiceManager>,
    weak_vp: WeakEntity<Viewport>,
) -> impl IntoElement {
    let px_pct =
        ((x - SLIDER_2D_MIN) / (SLIDER_2D_MAX - SLIDER_2D_MIN)).clamp(0.0, 1.0) as f32;
    // y-axis is flipped: top = +1, bottom = -1
    let py_pct =
        1.0 - ((y - SLIDER_2D_MIN) / (SLIDER_2D_MAX - SLIDER_2D_MIN)).clamp(0.0, 1.0) as f32;
    let value_text = format!("({:.2}, {:.2})", x, y);
    let name_for_canvas = name.clone();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .py(px(8.0))
        .child(
            div()
                .w(px(80.0))
                .flex_shrink_0()
                .text_color(PRES_TEXT)
                .text_size(px(12.0))
                .child(name.clone()),
        )
        .child(
            div()
                .w(px(SLIDER_2D_SIZE))
                .h(px(SLIDER_2D_SIZE))
                .flex_shrink_0()
                .child(
                    canvas(
                        move |bounds, _, _| bounds,
                        {
                            let name = name_for_canvas.clone();
                            let services = services.clone();
                            let weak_vp = weak_vp.clone();
                            move |_, bounds: Bounds<Pixels>, window, _cx| {
                                let w = f32::from(bounds.size.width);
                                let h = f32::from(bounds.size.height);
                                let ox = bounds.origin.x;
                                let oy = bounds.origin.y;

                                window.paint_quad(fill(
                                    Bounds::new(bounds.origin, size(px(w), px(h))),
                                    SLIDER_2D_BG,
                                ));
                                // crosshair axes
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(ox + px(w / 2.0 - 0.5), oy),
                                        size(px(1.0), px(h)),
                                    ),
                                    SLIDER_2D_AXIS,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(ox, oy + px(h / 2.0 - 0.5)),
                                        size(px(w), px(1.0)),
                                    ),
                                    SLIDER_2D_AXIS,
                                ));
                                // dot
                                window.paint_quad(quad(
                                    Bounds::new(
                                        point(
                                            ox + px(w * px_pct - SLIDER_2D_DOT_R),
                                            oy + px(h * py_pct - SLIDER_2D_DOT_R),
                                        ),
                                        size(px(SLIDER_2D_DOT_R * 2.0), px(SLIDER_2D_DOT_R * 2.0)),
                                    ),
                                    px(SLIDER_2D_DOT_R),
                                    PRES_ACCENT,
                                    px(0.0),
                                    TRANSPARENT,
                                    BorderStyle::Solid,
                                ));

                                let make_pv = move |pos: Point<Pixels>| {
                                    let xp = (f32::from(pos.x - bounds.origin.x) / w)
                                        .clamp(0.0, 1.0) as f64;
                                    let yp = (f32::from(pos.y - bounds.origin.y) / h)
                                        .clamp(0.0, 1.0) as f64;
                                    let xv = SLIDER_2D_MIN + xp * (SLIDER_2D_MAX - SLIDER_2D_MIN);
                                    let yv = SLIDER_2D_MAX - yp * (SLIDER_2D_MAX - SLIDER_2D_MIN);
                                    match kind {
                                        Slider2dKind::Complex => {
                                            ParameterValue::Complex { re: xv, im: yv }
                                        }
                                        Slider2dKind::VectorFloat => {
                                            ParameterValue::VectorFloat(vec![xv, yv])
                                        }
                                    }
                                };

                                {
                                    let name = name.clone();
                                    let services = services.clone();
                                    let weak_vp = weak_vp.clone();
                                    window.on_mouse_event(
                                        move |ev: &MouseDownEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble
                                                || !bounds.contains(&ev.position)
                                            {
                                                return;
                                            }
                                            let pv = make_pv(ev.position);
                                            weak_vp
                                                .update(cx, |vp, cx| {
                                                    vp.dragging_param = Some(name.clone());
                                                    cx.notify();
                                                })
                                                .ok();
                                            services
                                                .update(cx, |s, _| {
                                                    s.update_parameters(HashMap::from([(
                                                        name.clone(),
                                                        pv,
                                                    )]))
                                                })
                                                .ok();
                                        },
                                    );
                                }
                                {
                                    let name = name.clone();
                                    let services = services.clone();
                                    let weak_vp = weak_vp.clone();
                                    window.on_mouse_event(
                                        move |ev: &MouseMoveEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble {
                                                return;
                                            }
                                            let dragging = weak_vp
                                                .upgrade()
                                                .map(|e| {
                                                    e.read(cx).dragging_param.as_deref()
                                                        == Some(name.as_str())
                                                })
                                                .unwrap_or(false);
                                            if !dragging {
                                                return;
                                            }
                                            let pv = make_pv(ev.position);
                                            services
                                                .update(cx, |s, _| {
                                                    s.update_parameters(HashMap::from([(
                                                        name.clone(),
                                                        pv,
                                                    )]))
                                                })
                                                .ok();
                                        },
                                    );
                                }
                                {
                                    let weak_vp = weak_vp.clone();
                                    window.on_mouse_event(
                                        move |_: &MouseUpEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble {
                                                return;
                                            }
                                            weak_vp
                                                .update(cx, |vp, cx| {
                                                    vp.dragging_param = None;
                                                    cx.notify();
                                                })
                                                .ok();
                                        },
                                    );
                                }
                            }
                        },
                    )
                    .w(px(SLIDER_2D_SIZE))
                    .h(px(SLIDER_2D_SIZE)),
                ),
        )
        .child(
            div()
                .text_color(PRES_MUTED)
                .text_size(px(11.0))
                .child(value_text),
        )
}

fn render_param_control(
    name: &str,
    value: &ParameterValue,
    services: WeakEntity<ServiceManager>,
    weak_vp: WeakEntity<Viewport>,
) -> AnyElement {
    match value {
        ParameterValue::Float(v) => {
            render_slider_1d(name.to_string(), *v, false, services, weak_vp).into_any_element()
        }
        ParameterValue::Int(v) => {
            render_slider_1d(name.to_string(), *v as f64, true, services, weak_vp)
                .into_any_element()
        }
        ParameterValue::Complex { re, im } => render_slider_2d(
            name.to_string(),
            *re,
            *im,
            Slider2dKind::Complex,
            services,
            weak_vp,
        )
        .into_any_element(),
        ParameterValue::VectorFloat(v) if v.len() >= 2 => render_slider_2d(
            name.to_string(),
            v[0],
            v[1],
            Slider2dKind::VectorFloat,
            services,
            weak_vp,
        )
        .into_any_element(),
        _ => div().into_any_element(),
    }
}

impl Render for Viewport {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let (ring_color, params, timestamp, slide_count) = {
            let exec = self.services.read(cx).execution_state().read(cx);
            (
                theme.viewport_status_ring(exec.status),
                exec.parameters.clone(),
                exec.current_timestamp,
                exec.slide_count,
            )
        };

        let stage = div()
            .flex()
            .flex_1()
            .size_full()
            .bg(ring_color)
            .p(px(1.0))
            .child(div().size_full().bg(theme.viewport_stage_background));

        if !self.is_presenting {
            return div()
                .flex()
                .items_center()
                .justify_center()
                .size_full()
                .bg(theme.viewport_background)
                .p(px(24.0))
                .child(stage)
                .into_any_element();
        }

        // --- presentation layout ---
        let services_weak = self.services.downgrade();
        let weak_vp = cx.weak_entity();
        let show_params = self.show_params;
        let scroll_handle = self.scroll_handle.clone();

        let slide_label = format!(
            "Slide {} / {}",
            (timestamp.slide + 1).min(slide_count.max(1)),
            slide_count.max(1)
        );
        let time_label = format!("{:.2}s", timestamp.time);

        // collect and sort parameters
        let mut sorted: Vec<(String, ParameterValue)> = params
            .map(|p| p.parameters.into_iter().collect())
            .unwrap_or_default();
        sorted.sort_by(|(a, _), (b, _)| a.cmp(b));

        let controls: Vec<AnyElement> = sorted
            .iter()
            .map(|(name, value)| {
                render_param_control(name, value, services_weak.clone(), weak_vp.clone())
            })
            .collect();

        // top-left toolbar: [Parameters] [Slide X/Y] [time]
        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .h(px(PRES_TOOLBAR_H))
            .bg(PRES_TOOLBAR_BG)
            .child(
                div()
                    .id("pres-params-btn")
                    .px(px(10.0))
                    .py(px(3.0))
                    .rounded(px(3.0))
                    .bg(PRES_PANEL_BG)
                    .border(px(1.0))
                    .border_color(PRES_BORDER)
                    .text_color(PRES_TEXT)
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.75))
                    .child("Parameters")
                    .on_click(cx.listener(|vp, _, _, cx| vp.toggle_params(cx))),
            )
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

        let base = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(PRES_BG)
            .child(toolbar);

        let base = if show_params {
            base.child(
                div()
                    .id("pres-params-panel")
                    .max_h(px(PARAM_PANEL_MAX_H))
                    .w_full()
                    .bg(PRES_PANEL_BG)
                    .border_b(px(1.0))
                    .border_color(PRES_BORDER)
                    .overflow_y_scroll()
                    .track_scroll(&scroll_handle)
                    .px(px(16.0))
                    .py(px(4.0))
                    .children(controls),
            )
        } else {
            base
        };

        base.child(
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
