use std::collections::HashMap;

use gpui::*;

use crate::services::{ParameterSnapshot, ParameterValue, ServiceManager};

use super::{
    Viewport,
    style::{
        ESCAPE_SPEED_FAR_PX, ESCAPE_SPEED_MAX_MULT, ESCAPE_SPEED_NEAR_PX, OVERDRAG_STEP_1D,
        OVERDRAG_STEP_2D, PRES_ACCENT, PRES_MUTED, PRES_TEXT, SLIDER_1D_CANVAS_H,
        SLIDER_1D_EDGE_GAP, SLIDER_1D_MAX, SLIDER_1D_MIN, SLIDER_1D_W, SLIDER_2D_BG,
        SLIDER_2D_DOT_R, SLIDER_2D_EDGE_GAP, SLIDER_2D_GRID, SLIDER_2D_GRID_DIVISIONS,
        SLIDER_2D_MAX, SLIDER_2D_MIN, SLIDER_2D_SIZE, SLIDER_FILL_LOCKED, SLIDER_THUMB,
        SLIDER_THUMB_LOCKED, SLIDER_THUMB_R, SLIDER_TRACK_BG, SLIDER_TRACK_H, TRANSPARENT,
        with_alpha,
    },
};

const HIDDEN_PARAMS: [&str; 2] = ["camera", "background"];

#[derive(Clone)]
pub(super) enum Slider2dKind {
    Complex,
    VectorFloat(Vec<f64>),
    VectorInt(Vec<i64>),
}

#[derive(Clone, Copy)]
pub(super) struct AxisDrag {
    value: f64,
    min: f64,
    max: f64,
    overdrag_dir: i8,
    escape_distance: f32,
}

impl AxisDrag {
    fn bounds(self) -> (f64, f64) {
        (self.min, self.max)
    }
}

#[derive(Clone)]
pub(super) enum DragState {
    Scalar {
        name: String,
        axis: AxisDrag,
        is_int: bool,
    },
    Plane {
        name: String,
        x_axis: AxisDrag,
        y_axis: AxisDrag,
        kind: Slider2dKind,
    },
}

impl DragState {
    fn name(&self) -> &str {
        match self {
            Self::Scalar { name, .. } | Self::Plane { name, .. } => name,
        }
    }

    fn display_value(&self) -> ParameterValue {
        match self {
            Self::Scalar { axis, is_int, .. } => scalar_parameter_value(axis.value, *is_int),
            Self::Plane {
                x_axis,
                y_axis,
                kind,
                ..
            } => plane_parameter_value(x_axis.value, y_axis.value, kind),
        }
    }
}

impl Viewport {
    fn is_dragging(&self, name: &str) -> bool {
        self.drag_state
            .as_ref()
            .is_some_and(|state| state.name() == name)
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = &self.drag_state {
            let (name, bounds) = match state {
                DragState::Scalar { name, axis, .. } => (name, [axis.min, axis.max, 0.0, 0.0]),
                DragState::Plane {
                    name,
                    x_axis,
                    y_axis,
                    ..
                } => (name, [x_axis.min, x_axis.max, y_axis.min, y_axis.max]),
            };
            self.slider_bounds.insert(name.clone(), bounds);
        }
        self.drag_state = None;
        cx.notify();
    }

    fn display_parameter_value(&self, name: &str, fallback: &ParameterValue) -> ParameterValue {
        self.drag_state
            .as_ref()
            .filter(|state| state.name() == name)
            .map(DragState::display_value)
            .unwrap_or_else(|| fallback.clone())
    }

    fn display_parameter_bounds(&self, name: &str, fallback: [f64; 4]) -> [f64; 4] {
        match self
            .drag_state
            .as_ref()
            .filter(|state| state.name() == name)
        {
            Some(DragState::Scalar { axis, .. }) => {
                let (min, max) = axis.bounds();
                [min, max, 0.0, 0.0]
            }
            Some(DragState::Plane { x_axis, y_axis, .. }) => {
                let (x_min, x_max) = x_axis.bounds();
                let (y_min, y_max) = y_axis.bounds();
                [x_min, x_max, y_min, y_max]
            }
            None => fallback,
        }
    }

    fn begin_scalar_drag(
        &mut self,
        name: &str,
        fallback_value: f64,
        is_int: bool,
        local_x: f32,
        width: f32,
        fallback_bounds: [f64; 4],
        cx: &mut Context<Self>,
    ) -> ParameterValue {
        let axis = match &self.drag_state {
            Some(DragState::Scalar {
                name: drag_name,
                axis,
                ..
            }) if drag_name == name => *axis,
            _ => AxisDrag {
                value: fallback_value,
                min: fallback_bounds[0],
                max: fallback_bounds[1],
                overdrag_dir: 0,
                escape_distance: 0.0,
            },
        };
        let axis = axis_drag_target(local_x, width, axis);
        self.drag_state = Some(DragState::Scalar {
            name: name.to_string(),
            axis,
            is_int,
        });
        cx.notify();
        scalar_parameter_value(axis.value, is_int)
    }

    fn update_scalar_drag(
        &mut self,
        name: &str,
        fallback_value: f64,
        is_int: bool,
        local_x: f32,
        width: f32,
        fallback_bounds: [f64; 4],
        cx: &mut Context<Self>,
    ) -> Option<ParameterValue> {
        if !self.is_dragging(name) {
            return None;
        }
        Some(self.begin_scalar_drag(
            name,
            fallback_value,
            is_int,
            local_x,
            width,
            fallback_bounds,
            cx,
        ))
    }

    fn begin_plane_drag(
        &mut self,
        name: &str,
        fallback_x: f64,
        fallback_y: f64,
        kind: &Slider2dKind,
        pos: Point<Pixels>,
        canvas: Bounds<Pixels>,
        fallback_bounds: [f64; 4],
        cx: &mut Context<Self>,
    ) -> ParameterValue {
        let (x_axis, y_axis) = match &self.drag_state {
            Some(DragState::Plane {
                name: drag_name,
                x_axis,
                y_axis,
                ..
            }) if drag_name == name => (*x_axis, *y_axis),
            _ => (
                AxisDrag {
                    value: fallback_x,
                    min: fallback_bounds[0],
                    max: fallback_bounds[1],
                    overdrag_dir: 0,
                    escape_distance: 0.0,
                },
                AxisDrag {
                    value: fallback_y,
                    min: fallback_bounds[2],
                    max: fallback_bounds[3],
                    overdrag_dir: 0,
                    escape_distance: 0.0,
                },
            ),
        };
        let local_x = f32::from(pos.x - canvas.origin.x);
        let local_y = f32::from(pos.y - canvas.origin.y);
        let x_axis = axis_drag_target(local_x, f32::from(canvas.size.width), x_axis);
        let y_axis = axis_drag_target_inverted(local_y, f32::from(canvas.size.height), y_axis);
        self.drag_state = Some(DragState::Plane {
            name: name.to_string(),
            x_axis,
            y_axis,
            kind: kind.clone(),
        });
        cx.notify();
        plane_parameter_value(x_axis.value, y_axis.value, kind)
    }

    fn update_plane_drag(
        &mut self,
        name: &str,
        fallback_x: f64,
        fallback_y: f64,
        kind: &Slider2dKind,
        pos: Point<Pixels>,
        canvas: Bounds<Pixels>,
        fallback_bounds: [f64; 4],
        cx: &mut Context<Self>,
    ) -> Option<ParameterValue> {
        if !self.is_dragging(name) {
            return None;
        }
        Some(self.begin_plane_drag(
            name,
            fallback_x,
            fallback_y,
            kind,
            pos,
            canvas,
            fallback_bounds,
            cx,
        ))
    }

    pub(super) fn tick_overdrag(&mut self, cx: &mut Context<Self>) {
        let Some(mut state) = self.drag_state.take() else {
            return;
        };

        let update = match &mut state {
            DragState::Scalar {
                name, axis, is_int, ..
            } => {
                if !tick_axis_overdrag(axis, OVERDRAG_STEP_1D) {
                    self.drag_state = Some(state);
                    return;
                }
                (name.clone(), scalar_parameter_value(axis.value, *is_int))
            }
            DragState::Plane {
                name,
                x_axis,
                y_axis,
                kind,
            } => {
                let x_changed = tick_axis_overdrag(x_axis, OVERDRAG_STEP_2D);
                let y_changed = tick_axis_overdrag(y_axis, OVERDRAG_STEP_2D);
                if !x_changed && !y_changed {
                    self.drag_state = Some(state);
                    return;
                }
                (
                    name.clone(),
                    plane_parameter_value(x_axis.value, y_axis.value, kind),
                )
            }
        };
        self.drag_state = Some(state);

        self.services.update(cx, |services, _| {
            services.update_parameters(HashMap::from([update]))
        });
        cx.notify();
    }
}

pub(super) fn parameter_controls(
    viewport: &Viewport,
    params: Option<&ParameterSnapshot>,
    services: WeakEntity<ServiceManager>,
    weak_vp: WeakEntity<Viewport>,
) -> Vec<AnyElement> {
    let sorted: Vec<(String, ParameterValue, bool)> = params
        .map(|params| {
            params
                .param_order
                .iter()
                .rev()
                .filter_map(|name| {
                    if is_hidden_param(name) {
                        return None;
                    }
                    let is_locked = params.locked_params.contains(name);
                    params.parameters.get(name).map(|value| {
                        (
                            name.clone(),
                            viewport.display_parameter_value(name, value),
                            is_locked,
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    sorted
        .iter()
        .map(|(name, value, is_locked)| {
            let stored_bounds = viewport
                .slider_bounds
                .get(name.as_str())
                .copied()
                .unwrap_or_else(|| default_bounds_for_value(value));
            let bounds = viewport.display_parameter_bounds(name, stored_bounds);
            render_param_control(
                name,
                value,
                *is_locked,
                bounds,
                services.clone(),
                weak_vp.clone(),
            )
        })
        .collect()
}

fn is_hidden_param(name: &str) -> bool {
    HIDDEN_PARAMS.contains(&name)
}

fn scalar_parameter_value(value: f64, is_int: bool) -> ParameterValue {
    if is_int {
        ParameterValue::Int(value.round() as i64)
    } else {
        ParameterValue::Float(value)
    }
}

fn plane_parameter_value(x: f64, y: f64, kind: &Slider2dKind) -> ParameterValue {
    match kind {
        Slider2dKind::Complex => ParameterValue::Complex { re: x, im: y },
        Slider2dKind::VectorFloat(tail) => {
            let mut values = Vec::with_capacity(tail.len() + 2);
            values.push(x);
            values.push(y);
            values.extend(tail.iter().copied());
            ParameterValue::VectorFloat(values)
        }
        Slider2dKind::VectorInt(tail) => {
            let mut values = Vec::with_capacity(tail.len() + 2);
            values.push(x.round() as i64);
            values.push(y.round() as i64);
            values.extend(tail.iter().copied());
            ParameterValue::VectorInt(values)
        }
    }
}

fn axis_default_bounds(value: f64, default_min: f64, default_max: f64, round: bool) -> (f64, f64) {
    if (default_min..=default_max).contains(&value) {
        return (default_min, default_max);
    }

    let span = (default_max - default_min).abs().max(value.abs().max(1.0));
    let half = span * 0.5;
    let mut min = value - half;
    let mut max = value + half;
    if round {
        min = min.floor();
        max = max.ceil();
    }
    (min, max)
}

fn default_bounds_for_value(value: &ParameterValue) -> [f64; 4] {
    match value {
        ParameterValue::Float(v) => {
            let (min, max) = axis_default_bounds(*v, SLIDER_1D_MIN, SLIDER_1D_MAX, false);
            [min, max, 0.0, 0.0]
        }
        ParameterValue::Int(v) => {
            let (min, max) = axis_default_bounds(*v as f64, SLIDER_1D_MIN, SLIDER_1D_MAX, true);
            [min, max, 0.0, 0.0]
        }
        ParameterValue::Complex { re, im } => {
            let (x_min, x_max) = axis_default_bounds(*re, SLIDER_2D_MIN, SLIDER_2D_MAX, false);
            let (y_min, y_max) = axis_default_bounds(*im, SLIDER_2D_MIN, SLIDER_2D_MAX, false);
            [x_min, x_max, y_min, y_max]
        }
        ParameterValue::VectorFloat(values) if values.len() >= 2 => {
            let (x_min, x_max) =
                axis_default_bounds(values[0], SLIDER_2D_MIN, SLIDER_2D_MAX, false);
            let (y_min, y_max) =
                axis_default_bounds(values[1], SLIDER_2D_MIN, SLIDER_2D_MAX, false);
            [x_min, x_max, y_min, y_max]
        }
        ParameterValue::VectorInt(values) if values.len() >= 2 => {
            let (x_min, x_max) =
                axis_default_bounds(values[0] as f64, SLIDER_2D_MIN, SLIDER_2D_MAX, true);
            let (y_min, y_max) =
                axis_default_bounds(values[1] as f64, SLIDER_2D_MIN, SLIDER_2D_MAX, true);
            [x_min, x_max, y_min, y_max]
        }
        _ => [SLIDER_2D_MIN, SLIDER_2D_MAX, SLIDER_2D_MIN, SLIDER_2D_MAX],
    }
}

fn axis_drag_target(local_pos: f32, span: f32, axis: AxisDrag) -> AxisDrag {
    if span <= 0.0 {
        return axis;
    }

    let raw_p = (local_pos / span) as f64;
    if raw_p < 0.0 {
        AxisDrag {
            value: axis.min,
            overdrag_dir: -1,
            escape_distance: -local_pos,
            ..axis
        }
    } else if raw_p > 1.0 {
        AxisDrag {
            value: axis.max,
            overdrag_dir: 1,
            escape_distance: local_pos - span,
            ..axis
        }
    } else {
        AxisDrag {
            value: axis.min + raw_p * (axis.max - axis.min),
            overdrag_dir: 0,
            escape_distance: 0.0,
            ..axis
        }
    }
}

fn axis_drag_target_inverted(local_pos: f32, span: f32, axis: AxisDrag) -> AxisDrag {
    axis_drag_target(span - local_pos, span, axis)
}

fn tick_axis_overdrag(axis: &mut AxisDrag, step: f64) -> bool {
    if axis.overdrag_dir == 0 {
        return false;
    }

    let delta = axis.overdrag_dir as f64 * step * axis_escape_speed_multiplier(axis);
    axis.min += delta;
    axis.max += delta;
    axis.value = if axis.overdrag_dir < 0 {
        axis.min
    } else {
        axis.max
    };
    true
}

fn axis_escape_speed_multiplier(axis: &AxisDrag) -> f64 {
    let distance = axis.escape_distance.max(0.0);
    if distance <= ESCAPE_SPEED_NEAR_PX {
        return 1.0;
    }
    if distance >= ESCAPE_SPEED_FAR_PX {
        return ESCAPE_SPEED_MAX_MULT;
    }

    let t = (distance - ESCAPE_SPEED_NEAR_PX) / (ESCAPE_SPEED_FAR_PX - ESCAPE_SPEED_NEAR_PX);
    1.0 + (ESCAPE_SPEED_MAX_MULT - 1.0) * t as f64
}

fn format_bound(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

fn axis_fraction(value: f64, min: f64, max: f64) -> f32 {
    let span = max - min;
    if span.abs() <= f64::EPSILON {
        0.5
    } else {
        ((value - min) / span).clamp(0.0, 1.0) as f32
    }
}

fn inset_axis_position(span: f32, fraction: f32, gap: f32) -> f32 {
    if span <= 2.0 * gap {
        span * 0.5
    } else {
        gap + fraction.clamp(0.0, 1.0) * (span - 2.0 * gap)
    }
}

fn paint_slider_2d_grid(window: &mut Window, bounds: Bounds<Pixels>) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let ox = bounds.origin.x;
    let oy = bounds.origin.y;
    let grid_color = with_alpha(SLIDER_2D_GRID, 0.55);

    for step in 1..SLIDER_2D_GRID_DIVISIONS {
        let t = step as f32 / SLIDER_2D_GRID_DIVISIONS as f32;
        window.paint_quad(fill(
            Bounds::new(point(ox + px(w * t - 0.5), oy), size(px(1.0), px(h))),
            grid_color,
        ));
        window.paint_quad(fill(
            Bounds::new(point(ox, oy + px(h * t - 0.5)), size(px(w), px(1.0))),
            grid_color,
        ));
    }
}

fn render_slider_1d(
    name: String,
    value: f64,
    is_int: bool,
    is_locked: bool,
    bounds: (f64, f64),
    services: WeakEntity<ServiceManager>,
    weak_vp: WeakEntity<Viewport>,
) -> impl IntoElement {
    let (min, max) = bounds;
    let fallback_bounds = [min, max, 0.0, 0.0];
    let pct = axis_fraction(value, min, max);
    let base_value = if is_int {
        format!("{}", value as i64)
    } else {
        format!("{:.2}", value)
    };
    let value_text = if is_locked {
        format!("{} (locked)", base_value)
    } else {
        base_value
    };
    let min_text = format_bound(min);
    let max_text = format_bound(max);
    let fill_color = if is_locked {
        SLIDER_FILL_LOCKED
    } else {
        PRES_ACCENT
    };
    let thumb_color = if is_locked {
        SLIDER_THUMB_LOCKED
    } else {
        SLIDER_THUMB
    };
    let label_color = if is_locked { PRES_MUTED } else { PRES_TEXT };
    let name_for_canvas = name.clone();

    div()
        .flex()
        .flex_col()
        .py(px(4.0))
        .child(
            div()
                .flex()
                .flex_row()
                .justify_center()
                .items_baseline()
                .gap(px(8.0))
                .mb(px(2.0))
                .child(
                    div()
                        .text_color(label_color)
                        .text_size(px(12.0))
                        .child(name.clone()),
                )
                .child(
                    div()
                        .w(px(92.0))
                        .flex_shrink_0()
                        .text_color(PRES_MUTED)
                        .text_size(px(11.0))
                        .child(value_text),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .h(px(SLIDER_1D_CANVAS_H))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .justify_end()
                        .text_color(PRES_MUTED)
                        .text_size(px(10.0))
                        .child(min_text),
                )
                .child(
                    div().w(px(SLIDER_1D_W)).h_full().child(
                        canvas(move |bounds, _, _| bounds, {
                            let name = name_for_canvas.clone();
                            let services = services.clone();
                            let weak_vp = weak_vp.clone();
                            move |_, bounds: Bounds<Pixels>, window, _cx| {
                                let w = f32::from(bounds.size.width);
                                let h = f32::from(bounds.size.height);
                                let ox = bounds.origin.x;
                                let oy = bounds.origin.y;
                                let track_y = h / 2.0 - SLIDER_TRACK_H / 2.0;
                                let track_x = SLIDER_1D_EDGE_GAP;
                                let track_w = (w - 2.0 * SLIDER_1D_EDGE_GAP).max(0.0);
                                let thumb_x = inset_axis_position(w, pct, SLIDER_1D_EDGE_GAP);

                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(ox + px(track_x), oy + px(track_y)),
                                        size(px(track_w), px(SLIDER_TRACK_H)),
                                    ),
                                    SLIDER_TRACK_BG,
                                ));
                                if pct > 0.0 {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(ox + px(track_x), oy + px(track_y)),
                                            size(px(track_w * pct), px(SLIDER_TRACK_H)),
                                        ),
                                        fill_color,
                                    ));
                                }
                                window.paint_quad(quad(
                                    Bounds::new(
                                        point(
                                            ox + px(thumb_x - SLIDER_THUMB_R),
                                            oy + px(h / 2.0 - SLIDER_THUMB_R),
                                        ),
                                        size(px(SLIDER_THUMB_R * 2.0), px(SLIDER_THUMB_R * 2.0)),
                                    ),
                                    px(SLIDER_THUMB_R),
                                    thumb_color,
                                    px(0.0),
                                    TRANSPARENT,
                                    BorderStyle::Solid,
                                ));

                                if is_locked {
                                    return;
                                }

                                {
                                    let name = name.clone();
                                    let services = services.clone();
                                    let weak_vp = weak_vp.clone();
                                    window.on_mouse_event(
                                        move |event: &MouseDownEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble
                                                || !bounds.contains(&event.position)
                                            {
                                                return;
                                            }
                                            let local_x =
                                                f32::from(event.position.x - bounds.origin.x);
                                            let value = weak_vp
                                                .update(cx, |viewport, cx| {
                                                    viewport.begin_scalar_drag(
                                                        &name,
                                                        value,
                                                        is_int,
                                                        local_x,
                                                        w,
                                                        fallback_bounds,
                                                        cx,
                                                    )
                                                })
                                                .ok();
                                            if let Some(value) = value {
                                                services
                                                    .update(cx, |services, _| {
                                                        services.update_parameters(HashMap::from([
                                                            (name.clone(), value),
                                                        ]))
                                                    })
                                                    .ok();
                                            }
                                        },
                                    );
                                }

                                {
                                    let name = name.clone();
                                    let services = services.clone();
                                    let weak_vp = weak_vp.clone();
                                    window.on_mouse_event(
                                        move |event: &MouseMoveEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble {
                                                return;
                                            }
                                            let dragging = weak_vp
                                                .upgrade()
                                                .map(|entity| {
                                                    entity.read(cx).is_dragging(name.as_str())
                                                })
                                                .unwrap_or(false);
                                            if !dragging {
                                                return;
                                            }
                                            let local_x =
                                                f32::from(event.position.x - bounds.origin.x);
                                            let value = weak_vp
                                                .update(cx, |viewport, cx| {
                                                    viewport.update_scalar_drag(
                                                        &name,
                                                        value,
                                                        is_int,
                                                        local_x,
                                                        w,
                                                        fallback_bounds,
                                                        cx,
                                                    )
                                                })
                                                .ok()
                                                .flatten();
                                            if let Some(value) = value {
                                                services
                                                    .update(cx, |services, _| {
                                                        services.update_parameters(HashMap::from([
                                                            (name.clone(), value),
                                                        ]))
                                                    })
                                                    .ok();
                                            }
                                        },
                                    );
                                }

                                {
                                    let weak_vp = weak_vp.clone();
                                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                        if phase != DispatchPhase::Bubble {
                                            return;
                                        }
                                        weak_vp
                                            .update(cx, |viewport, cx| {
                                                viewport.end_drag(cx);
                                            })
                                            .ok();
                                    });
                                }
                            }
                        })
                        .w(px(SLIDER_1D_W))
                        .h(px(SLIDER_1D_CANVAS_H)),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(PRES_MUTED)
                        .text_size(px(10.0))
                        .child(max_text),
                ),
        )
}

fn render_slider_2d(
    name: String,
    x: f64,
    y: f64,
    kind: Slider2dKind,
    is_locked: bool,
    x_bounds: (f64, f64),
    y_bounds: (f64, f64),
    services: WeakEntity<ServiceManager>,
    weak_vp: WeakEntity<Viewport>,
) -> impl IntoElement {
    let (x_min, x_max) = x_bounds;
    let (y_min, y_max) = y_bounds;
    let fallback_bounds = [x_min, x_max, y_min, y_max];
    let px_pct = axis_fraction(x, x_min, x_max);
    let py_pct = 1.0 - axis_fraction(y, y_min, y_max);
    let value_text = match &kind {
        Slider2dKind::Complex => {
            if y < 0.0 {
                format!("{:.2} - {:.2}i", x, y.abs())
            } else {
                format!("{:.2} + {:.2}i", x, y)
            }
        }
        Slider2dKind::VectorFloat(_) => format!("({:.2}, {:.2})", x, y),
        Slider2dKind::VectorInt(_) => format!("({}, {})", x.round() as i64, y.round() as i64),
    };
    let dot_color = if is_locked {
        SLIDER_THUMB_LOCKED
    } else {
        PRES_ACCENT
    };
    let label_color = if is_locked { PRES_MUTED } else { PRES_TEXT };
    let name_for_canvas = name.clone();
    let x_min_text = format_bound(x_min);
    let x_max_text = format_bound(x_max);
    let y_min_text = format_bound(y_min);
    let y_max_text = format_bound(y_max);
    let locked_suffix = if is_locked { " (locked)" } else { "" };
    let header_text = format!("{}{}", value_text, locked_suffix);

    div()
        .flex()
        .flex_col()
        .py(px(8.0))
        .child(
            div()
                .flex()
                .flex_row()
                .justify_center()
                .items_baseline()
                .gap(px(8.0))
                .mb(px(6.0))
                .child(
                    div()
                        .text_color(label_color)
                        .text_size(px(12.0))
                        .child(name.clone()),
                )
                .child(
                    div()
                        .w(px(120.0))
                        .flex_shrink_0()
                        .text_color(PRES_MUTED)
                        .text_size(px(11.0))
                        .child(header_text),
                ),
        )
        .child(
            div().flex().justify_center().mb(px(4.0)).child(
                div()
                    .w(px(SLIDER_2D_SIZE))
                    .flex()
                    .justify_center()
                    .text_color(PRES_MUTED)
                    .text_size(px(10.0))
                    .child(y_max_text),
            ),
        )
        .child(
            div().flex().justify_center().child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(34.0))
                            .text_color(PRES_MUTED)
                            .text_size(px(10.0))
                            .flex()
                            .justify_end()
                            .child(x_min_text),
                    )
                    .child(
                        div()
                            .w(px(SLIDER_2D_SIZE))
                            .h(px(SLIDER_2D_SIZE))
                            .flex_shrink_0()
                            .child(
                                canvas(move |bounds, _, _| bounds, {
                                    let name = name_for_canvas.clone();
                                    let kind = kind.clone();
                                    let services = services.clone();
                                    let weak_vp = weak_vp.clone();
                                    move |_, bounds: Bounds<Pixels>, window, _cx| {
                                        let w = f32::from(bounds.size.width);
                                        let h = f32::from(bounds.size.height);
                                        let ox = bounds.origin.x;
                                        let oy = bounds.origin.y;
                                        let dot_x =
                                            inset_axis_position(w, px_pct, SLIDER_2D_EDGE_GAP);
                                        let dot_y =
                                            inset_axis_position(h, py_pct, SLIDER_2D_EDGE_GAP);

                                        window.paint_quad(fill(
                                            Bounds::new(bounds.origin, size(px(w), px(h))),
                                            SLIDER_2D_BG,
                                        ));
                                        paint_slider_2d_grid(window, bounds);
                                        window.paint_quad(quad(
                                            Bounds::new(
                                                point(
                                                    ox + px(dot_x - SLIDER_2D_DOT_R),
                                                    oy + px(dot_y - SLIDER_2D_DOT_R),
                                                ),
                                                size(
                                                    px(SLIDER_2D_DOT_R * 2.0),
                                                    px(SLIDER_2D_DOT_R * 2.0),
                                                ),
                                            ),
                                            px(SLIDER_2D_DOT_R),
                                            dot_color,
                                            px(0.0),
                                            TRANSPARENT,
                                            BorderStyle::Solid,
                                        ));

                                        if is_locked {
                                            return;
                                        }

                                        {
                                            let name = name.clone();
                                            let kind = kind.clone();
                                            let services = services.clone();
                                            let weak_vp = weak_vp.clone();
                                            window.on_mouse_event(
                                                move |event: &MouseDownEvent, phase, _, cx| {
                                                    if phase != DispatchPhase::Bubble
                                                        || !bounds.contains(&event.position)
                                                    {
                                                        return;
                                                    }
                                                    let value = weak_vp
                                                        .update(cx, |viewport, cx| {
                                                            viewport.begin_plane_drag(
                                                                &name,
                                                                x,
                                                                y,
                                                                &kind,
                                                                event.position,
                                                                bounds,
                                                                fallback_bounds,
                                                                cx,
                                                            )
                                                        })
                                                        .ok();
                                                    if let Some(value) = value {
                                                        services
                                                            .update(cx, |services, _| {
                                                                services.update_parameters(
                                                                    HashMap::from([(
                                                                        name.clone(),
                                                                        value,
                                                                    )]),
                                                                )
                                                            })
                                                            .ok();
                                                    }
                                                },
                                            );
                                        }

                                        {
                                            let name = name.clone();
                                            let kind = kind.clone();
                                            let services = services.clone();
                                            let weak_vp = weak_vp.clone();
                                            window.on_mouse_event(
                                                move |event: &MouseMoveEvent, phase, _, cx| {
                                                    if phase != DispatchPhase::Bubble {
                                                        return;
                                                    }
                                                    let dragging = weak_vp
                                                        .upgrade()
                                                        .map(|entity| {
                                                            entity
                                                                .read(cx)
                                                                .is_dragging(name.as_str())
                                                        })
                                                        .unwrap_or(false);
                                                    if !dragging {
                                                        return;
                                                    }
                                                    let value = weak_vp
                                                        .update(cx, |viewport, cx| {
                                                            viewport.update_plane_drag(
                                                                &name,
                                                                x,
                                                                y,
                                                                &kind,
                                                                event.position,
                                                                bounds,
                                                                fallback_bounds,
                                                                cx,
                                                            )
                                                        })
                                                        .ok()
                                                        .flatten();
                                                    if let Some(value) = value {
                                                        services
                                                            .update(cx, |services, _| {
                                                                services.update_parameters(
                                                                    HashMap::from([(
                                                                        name.clone(),
                                                                        value,
                                                                    )]),
                                                                )
                                                            })
                                                            .ok();
                                                    }
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
                                                        .update(cx, |viewport, cx| {
                                                            viewport.end_drag(cx);
                                                        })
                                                        .ok();
                                                },
                                            );
                                        }
                                    }
                                })
                                .w(px(SLIDER_2D_SIZE))
                                .h(px(SLIDER_2D_SIZE)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(34.0))
                            .text_color(PRES_MUTED)
                            .text_size(px(10.0))
                            .child(x_max_text),
                    ),
            ),
        )
        .child(
            div().flex().justify_center().mt(px(4.0)).child(
                div()
                    .w(px(SLIDER_2D_SIZE))
                    .flex()
                    .justify_center()
                    .text_color(PRES_MUTED)
                    .text_size(px(10.0))
                    .child(y_min_text),
            ),
        )
}

fn render_param_control(
    name: &str,
    value: &ParameterValue,
    is_locked: bool,
    bounds: [f64; 4],
    services: WeakEntity<ServiceManager>,
    weak_vp: WeakEntity<Viewport>,
) -> AnyElement {
    match value {
        ParameterValue::Float(value) => render_slider_1d(
            name.to_string(),
            *value,
            false,
            is_locked,
            (bounds[0], bounds[1]),
            services,
            weak_vp,
        )
        .into_any_element(),
        ParameterValue::Int(value) => render_slider_1d(
            name.to_string(),
            *value as f64,
            true,
            is_locked,
            (bounds[0], bounds[1]),
            services,
            weak_vp,
        )
        .into_any_element(),
        ParameterValue::Complex { re, im } => render_slider_2d(
            name.to_string(),
            *re,
            *im,
            Slider2dKind::Complex,
            is_locked,
            (bounds[0], bounds[1]),
            (bounds[2], bounds[3]),
            services,
            weak_vp,
        )
        .into_any_element(),
        ParameterValue::VectorFloat(values) if values.len() >= 2 => render_slider_2d(
            name.to_string(),
            values[0],
            values[1],
            Slider2dKind::VectorFloat(values[2..].to_vec()),
            is_locked,
            (bounds[0], bounds[1]),
            (bounds[2], bounds[3]),
            services,
            weak_vp,
        )
        .into_any_element(),
        ParameterValue::VectorInt(values) if values.len() >= 2 => render_slider_2d(
            name.to_string(),
            values[0] as f64,
            values[1] as f64,
            Slider2dKind::VectorInt(values[2..].to_vec()),
            is_locked,
            (bounds[0], bounds[1]),
            (bounds[2], bounds[3]),
            services,
            weak_vp,
        )
        .into_any_element(),
        _ => div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .py(px(4.0))
            .child(
                div()
                    .text_color(PRES_MUTED)
                    .text_size(px(12.0))
                    .child(name.to_string()),
            )
            .child(
                div()
                    .text_color(PRES_MUTED)
                    .text_size(px(10.0))
                    .child("(unsupported type)"),
            )
            .into_any_element(),
    }
}
