use std::{collections::HashMap, time::Duration};

use gpui::*;

use crate::{
    services::{ExecutionStatus, ParameterValue, ServiceManager},
    theme::ThemeSettings,
    viewport::debug_scene_view::DebugSceneView,
};

// presentation overlay colors (always dark, independent of theme)
const PRES_BG: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const PRES_TOOLBAR_BG: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.8,
};
const PRES_PANEL_BG: Rgba = Rgba {
    r: 0.10,
    g: 0.10,
    b: 0.10,
    a: 1.0,
};
const PRES_BORDER: Rgba = Rgba {
    r: 0.22,
    g: 0.22,
    b: 0.22,
    a: 1.0,
};
const PRES_TEXT: Rgba = Rgba {
    r: 0.85,
    g: 0.85,
    b: 0.85,
    a: 1.0,
};
const PRES_MUTED: Rgba = Rgba {
    r: 0.50,
    g: 0.50,
    b: 0.50,
    a: 1.0,
};
const PRES_ACCENT: Rgba = Rgba {
    r: 0.47,
    g: 0.63,
    b: 0.87,
    a: 1.0,
};
const SLIDER_TRACK_BG: Rgba = Rgba {
    r: 0.28,
    g: 0.28,
    b: 0.28,
    a: 1.0,
};
const SLIDER_THUMB: Rgba = Rgba {
    r: 0.90,
    g: 0.90,
    b: 0.90,
    a: 1.0,
};
const SLIDER_THUMB_LOCKED: Rgba = Rgba {
    r: 0.45,
    g: 0.45,
    b: 0.45,
    a: 1.0,
};
const SLIDER_FILL_LOCKED: Rgba = Rgba {
    r: 0.30,
    g: 0.30,
    b: 0.38,
    a: 1.0,
};
const SLIDER_2D_BG: Rgba = Rgba {
    r: 0.18,
    g: 0.18,
    b: 0.18,
    a: 1.0,
};
const SLIDER_2D_AXIS: Rgba = Rgba {
    r: 0.32,
    g: 0.32,
    b: 0.32,
    a: 1.0,
};
const TRANSPARENT: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

const PRES_TOOLBAR_H: f32 = 32.0;
const PARAM_PANEL_W: f32 = 260.0;
const SLIDER_TRACK_H: f32 = 4.0;
const SLIDER_THUMB_R: f32 = 7.0;
const SLIDER_1D_CANVAS_H: f32 = 28.0;
const SLIDER_1D_W: f32 = 110.0;
const SLIDER_1D_MIN: f64 = -10.0;
const SLIDER_1D_MAX: f64 = 10.0;
const SLIDER_2D_SIZE: f32 = 120.0;
const SLIDER_2D_MIN: f64 = -1.0;
const SLIDER_2D_MAX: f64 = 1.0;
const SLIDER_2D_DOT_R: f32 = 5.0;
const RING_TRANSITION: Duration = Duration::from_millis(140);
const OVERDRAG_TICK: Duration = Duration::from_nanos(8_333_333);
const HIDDEN_PARAMS: [&str; 2] = ["camera", "background"];

// fixed per-tick overdrag step; cursor position only selects direction,
// which avoids the runaway acceleration from mapping arbitrary cursor distance
const OVERDRAG_STEP_1D: f64 = 0.025;
const OVERDRAG_STEP_2D: f64 = 0.0125;
// absolute value-units past the current bounds before the bounds start to expand
const BOUNDS_EXPAND_THRESH_1D: f64 = 0.5;
const BOUNDS_EXPAND_THRESH_2D: f64 = 0.05;
const BOUNDS_EXPAND_THRESH_FRAC: f64 = 0.05;
// fraction of current range added as padding when bounds expand
const BOUNDS_EXPAND_PAD: f64 = 0.1;

#[derive(Clone)]
enum Slider2dKind {
    Complex,
    VectorFloat(Vec<f64>),
    VectorInt(Vec<i64>),
}

#[derive(Clone)]
enum DragState {
    Scalar {
        name: String,
        value: f64,
        is_int: bool,
        overdrag_dir: i8,
    },
    Plane {
        name: String,
        x: f64,
        y: f64,
        kind: Slider2dKind,
        x_overdrag_dir: i8,
        y_overdrag_dir: i8,
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
            Self::Scalar { value, is_int, .. } => scalar_parameter_value(*value, *is_int),
            Self::Plane { x, y, kind, .. } => plane_parameter_value(*x, *y, kind),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct RingStyle {
    color: Rgba,
    width: f32,
}

pub struct Viewport {
    services: Entity<ServiceManager>,
    scene: Entity<DebugSceneView>,
    is_presenting: bool,
    show_params: bool,
    drag_state: Option<DragState>,
    scroll_handle: ScrollHandle,
    // per-parameter value-space bounds: [x_min, x_max, y_min, y_max]
    slider_bounds: HashMap<String, [f64; 4]>,
    ring_style: Option<RingStyle>,
    ring_previous: RingStyle,
    ring_animation_nonce: usize,
}

impl Viewport {
    pub fn new(services: Entity<ServiceManager>, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();
        let execution_state = services.read(cx).execution_state().clone();
        let scene = cx.new(|cx| DebugSceneView::new(execution_state, cx));

        let viewport = Self {
            services,
            scene,
            is_presenting: false,
            show_params: false,
            drag_state: None,
            scroll_handle: ScrollHandle::new(),
            slider_bounds: HashMap::new(),
            ring_style: None,
            ring_previous: RingStyle {
                color: TRANSPARENT,
                width: 0.0,
            },
            ring_animation_nonce: 0,
        };

        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor().timer(OVERDRAG_TICK).await;
                let should_continue = weak
                    .update(cx, |viewport, cx| {
                        viewport.tick_overdrag(cx);
                    })
                    .is_ok();
                if !should_continue {
                    break;
                }
            }
        })
        .detach();

        viewport
    }

    pub fn set_presenting(&mut self, presenting: bool, cx: &mut Context<Self>) {
        self.is_presenting = presenting;
        if !presenting {
            self.show_params = false;
            self.drag_state = None;
            self.slider_bounds.clear();
        }
        cx.notify();
    }

    pub fn toggle_params(&mut self, cx: &mut Context<Self>) {
        self.show_params = !self.show_params;
        cx.notify();
    }

    fn is_dragging(&self, name: &str) -> bool {
        self.drag_state
            .as_ref()
            .is_some_and(|state| state.name() == name)
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
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
        let [min, max, _, _] = self.current_bounds(name, fallback_bounds);
        let current = match &self.drag_state {
            Some(DragState::Scalar {
                name: drag_name,
                value,
                ..
            }) if drag_name == name => *value,
            _ => fallback_value,
        };
        let (value, overdrag_dir) = scalar_drag_target(local_x, width, min, max, current);
        self.drag_state = Some(DragState::Scalar {
            name: name.to_string(),
            value,
            is_int,
            overdrag_dir,
        });
        if let Some((new_min, new_max)) = expand_1d_bounds(min, max, value, is_int) {
            self.slider_bounds
                .insert(name.to_string(), [new_min, new_max, 0.0, 0.0]);
        }
        cx.notify();
        scalar_parameter_value(value, is_int)
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
        let [x_min, x_max, y_min, y_max] = self.current_bounds(name, fallback_bounds);
        let (current_x, current_y) = match &self.drag_state {
            Some(DragState::Plane {
                name: drag_name,
                x,
                y,
                ..
            }) if drag_name == name => (*x, *y),
            _ => (fallback_x, fallback_y),
        };
        let ((x, y), (x_overdrag_dir, y_overdrag_dir)) = plane_drag_target(
            pos, canvas, x_min, x_max, y_min, y_max, current_x, current_y,
        );
        self.drag_state = Some(DragState::Plane {
            name: name.to_string(),
            x,
            y,
            kind: kind.clone(),
            x_overdrag_dir,
            y_overdrag_dir,
        });
        if let Some(((nx_min, nx_max), (ny_min, ny_max))) =
            expand_2d_bounds(x_min, x_max, y_min, y_max, x, y)
        {
            self.slider_bounds
                .insert(name.to_string(), [nx_min, nx_max, ny_min, ny_max]);
        }
        cx.notify();
        plane_parameter_value(x, y, kind)
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

    fn current_bounds(&self, name: &str, fallback: [f64; 4]) -> [f64; 4] {
        self.slider_bounds.get(name).copied().unwrap_or(fallback)
    }

    fn tick_overdrag(&mut self, cx: &mut Context<Self>) {
        let Some(mut state) = self.drag_state.take() else {
            return;
        };

        let update = match &mut state {
            DragState::Scalar {
                name,
                value,
                is_int,
                overdrag_dir,
            } => {
                if *overdrag_dir == 0 {
                    self.drag_state = Some(state);
                    return;
                }
                *value += *overdrag_dir as f64 * OVERDRAG_STEP_1D;
                let [min, max, _, _] = self.current_bounds(
                    name.as_str(),
                    default_bounds_for_value(&scalar_parameter_value(*value, *is_int)),
                );
                if let Some((new_min, new_max)) = expand_1d_bounds(min, max, *value, *is_int) {
                    self.slider_bounds
                        .insert(name.clone(), [new_min, new_max, 0.0, 0.0]);
                }
                (name.clone(), scalar_parameter_value(*value, *is_int))
            }
            DragState::Plane {
                name,
                x,
                y,
                kind,
                x_overdrag_dir,
                y_overdrag_dir,
            } => {
                if *x_overdrag_dir == 0 && *y_overdrag_dir == 0 {
                    self.drag_state = Some(state);
                    return;
                }
                *x += *x_overdrag_dir as f64 * OVERDRAG_STEP_2D;
                *y += *y_overdrag_dir as f64 * OVERDRAG_STEP_2D;
                let [x_min, x_max, y_min, y_max] = self.current_bounds(
                    name.as_str(),
                    default_bounds_for_value(&plane_parameter_value(*x, *y, kind)),
                );
                if let Some(((nx_min, nx_max), (ny_min, ny_max))) =
                    expand_2d_bounds(x_min, x_max, y_min, y_max, *x, *y)
                {
                    self.slider_bounds
                        .insert(name.clone(), [nx_min, nx_max, ny_min, ny_max]);
                }
                (name.clone(), plane_parameter_value(*x, *y, kind))
            }
        };
        self.drag_state = Some(state);

        self.services.update(cx, |services, _| {
            services.update_parameters(HashMap::from([update]))
        });
        cx.notify();
    }
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

fn lerp_f32(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

fn lerp_rgba(start: Rgba, end: Rgba, t: f32) -> Rgba {
    Rgba {
        r: lerp_f32(start.r, end.r, t),
        g: lerp_f32(start.g, end.g, t),
        b: lerp_f32(start.b, end.b, t),
        a: lerp_f32(start.a, end.a, t),
    }
}

fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..color }
}

fn ring_style_for(
    status: ExecutionStatus,
    is_presenting: bool,
    theme: crate::theme::Theme,
) -> RingStyle {
    if is_presenting
        && !matches!(
            status,
            ExecutionStatus::RuntimeError | ExecutionStatus::CompileError
        )
    {
        return RingStyle {
            color: with_alpha(theme.viewport_status_ring(status), 0.0),
            width: 0.0,
        };
    }

    match status {
        ExecutionStatus::Playing | ExecutionStatus::Paused => RingStyle {
            color: theme.viewport_status_ring(status),
            width: 1.0,
        },
        ExecutionStatus::Seeking => RingStyle {
            color: with_alpha(theme.viewport_status_loading, 0.9),
            width: 1.5,
        },
        ExecutionStatus::RuntimeError => RingStyle {
            color: theme.viewport_status_runtime_error,
            width: 3.0,
        },
        ExecutionStatus::CompileError => RingStyle {
            color: with_alpha(theme.viewport_status_compile_error, 0.72),
            width: 2.0,
        },
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

fn expand_threshold(range: f64, base: f64) -> f64 {
    base.max(range.abs().max(1.0) * BOUNDS_EXPAND_THRESH_FRAC)
}

// --- drag computation helpers ---

fn expand_1d_bounds(min: f64, max: f64, value: f64, is_int: bool) -> Option<(f64, f64)> {
    let range = (max - min).abs().max(1.0);
    let threshold = expand_threshold(range, BOUNDS_EXPAND_THRESH_1D);
    let mut new_min = if value < min - threshold {
        value - BOUNDS_EXPAND_PAD * range
    } else {
        min
    };
    let mut new_max = if value > max + threshold {
        value + BOUNDS_EXPAND_PAD * range
    } else {
        max
    };
    // integer sliders keep integer-valued bounds
    if is_int {
        new_min = new_min.floor();
        new_max = new_max.ceil();
    }
    let new_bounds = if new_min != min || new_max != max {
        Some((new_min, new_max))
    } else {
        None
    };
    new_bounds
}

fn scalar_drag_target(local_x: f32, width: f32, min: f64, max: f64, current: f64) -> (f64, i8) {
    let raw_p = (local_x / width) as f64;
    if raw_p < 0.0 {
        (current.min(min), -1)
    } else if raw_p > 1.0 {
        (current.max(max), 1)
    } else {
        (min + raw_p * (max - min), 0)
    }
}

fn plane_drag_target(
    pos: Point<Pixels>,
    canvas: Bounds<Pixels>,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    current_x: f64,
    current_y: f64,
) -> ((f64, f64), (i8, i8)) {
    let w = f32::from(canvas.size.width);
    let h = f32::from(canvas.size.height);
    let raw_xp = (f32::from(pos.x - canvas.origin.x) / w) as f64;
    let raw_yp = (f32::from(pos.y - canvas.origin.y) / h) as f64;

    let (x, x_dir) = if raw_xp < 0.0 {
        (current_x.min(x_min), -1)
    } else if raw_xp > 1.0 {
        (current_x.max(x_max), 1)
    } else {
        (x_min + raw_xp * (x_max - x_min), 0)
    };

    let (y, y_dir) = if raw_yp < 0.0 {
        (current_y.max(y_max), 1)
    } else if raw_yp > 1.0 {
        (current_y.min(y_min), -1)
    } else {
        (y_max - raw_yp * (y_max - y_min), 0)
    };

    ((x, y), (x_dir, y_dir))
}

fn expand_2d_bounds(
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    x: f64,
    y: f64,
) -> Option<((f64, f64), (f64, f64))> {
    let x_range = (x_max - x_min).abs().max(1.0);
    let y_range = (y_max - y_min).abs().max(1.0);
    let x_threshold = expand_threshold(x_range, BOUNDS_EXPAND_THRESH_2D);
    let y_threshold = expand_threshold(y_range, BOUNDS_EXPAND_THRESH_2D);
    let new_x_min = if x < x_min - x_threshold {
        x - BOUNDS_EXPAND_PAD * x_range
    } else {
        x_min
    };
    let new_x_max = if x > x_max + x_threshold {
        x + BOUNDS_EXPAND_PAD * x_range
    } else {
        x_max
    };
    let new_y_min = if y < y_min - y_threshold {
        y - BOUNDS_EXPAND_PAD * y_range
    } else {
        y_min
    };
    let new_y_max = if y > y_max + y_threshold {
        y + BOUNDS_EXPAND_PAD * y_range
    } else {
        y_max
    };

    let new_bounds =
        if new_x_min != x_min || new_x_max != x_max || new_y_min != y_min || new_y_max != y_max {
            Some(((new_x_min, new_x_max), (new_y_min, new_y_max)))
        } else {
            None
        };
    new_bounds
}

fn format_bound(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

// --- slider helpers ---

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
    let pct = ((value - min) / (max - min)).clamp(0.0, 1.0) as f32;
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
        // row 1: name + value centered together, value has fixed width to prevent layout shift
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
        // row 2: [min flex_1 right-align] [slider fixed] [max flex_1]
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
                                        fill_color,
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
                                        move |ev: &MouseDownEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble
                                                || !bounds.contains(&ev.position)
                                            {
                                                return;
                                            }
                                            let local_x =
                                                f32::from(ev.position.x - bounds.origin.x);
                                            let pv = weak_vp
                                                .update(cx, |vp, cx| {
                                                    vp.begin_scalar_drag(
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
                                            if let Some(pv) = pv {
                                                services
                                                    .update(cx, |s, _| {
                                                        s.update_parameters(HashMap::from([(
                                                            name.clone(),
                                                            pv,
                                                        )]))
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
                                        move |ev: &MouseMoveEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble {
                                                return;
                                            }
                                            let dragging = weak_vp
                                                .upgrade()
                                                .map(|e| e.read(cx).is_dragging(name.as_str()))
                                                .unwrap_or(false);
                                            if !dragging {
                                                return;
                                            }
                                            let local_x =
                                                f32::from(ev.position.x - bounds.origin.x);
                                            let pv = weak_vp
                                                .update(cx, |vp, cx| {
                                                    vp.update_scalar_drag(
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
                                            if let Some(pv) = pv {
                                                services
                                                    .update(cx, |s, _| {
                                                        s.update_parameters(HashMap::from([(
                                                            name.clone(),
                                                            pv,
                                                        )]))
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
                                            .update(cx, |vp, cx| {
                                                vp.end_drag(cx);
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
    let px_pct = ((x - x_min) / (x_max - x_min)).clamp(0.0, 1.0) as f32;
    let py_pct = 1.0 - ((y - y_min) / (y_max - y_min)).clamp(0.0, 1.0) as f32;
    // zero-crossing axis positions track true zero even when bounds shift
    let x_zero = ((-x_min) / (x_max - x_min)).clamp(0.0, 1.0) as f32;
    let y_zero = 1.0 - ((-y_min) / (y_max - y_min)).clamp(0.0, 1.0) as f32;
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

    let locked_suffix = if is_locked { " (locked)" } else { "" };
    let header_text = format!("{}{}", value_text, locked_suffix);

    div()
        .flex()
        .flex_col()
        .py(px(8.0))
        // name + value centered, fixed-width value to avoid layout shift
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
        // canvas centered horizontally
        .child(
            div().flex().justify_center().child(
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

                                window.paint_quad(fill(
                                    Bounds::new(bounds.origin, size(px(w), px(h))),
                                    SLIDER_2D_BG,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(ox + px(w * x_zero - 0.5), oy),
                                        size(px(1.0), px(h)),
                                    ),
                                    SLIDER_2D_AXIS,
                                ));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(ox, oy + px(h * y_zero - 0.5)),
                                        size(px(w), px(1.0)),
                                    ),
                                    SLIDER_2D_AXIS,
                                ));
                                window.paint_quad(quad(
                                    Bounds::new(
                                        point(
                                            ox + px(w * px_pct - SLIDER_2D_DOT_R),
                                            oy + px(h * py_pct - SLIDER_2D_DOT_R),
                                        ),
                                        size(px(SLIDER_2D_DOT_R * 2.0), px(SLIDER_2D_DOT_R * 2.0)),
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
                                        move |ev: &MouseDownEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble
                                                || !bounds.contains(&ev.position)
                                            {
                                                return;
                                            }
                                            let pv = weak_vp
                                                .update(cx, |vp, cx| {
                                                    vp.begin_plane_drag(
                                                        &name,
                                                        x,
                                                        y,
                                                        &kind,
                                                        ev.position,
                                                        bounds,
                                                        fallback_bounds,
                                                        cx,
                                                    )
                                                })
                                                .ok();
                                            if let Some(pv) = pv {
                                                services
                                                    .update(cx, |s, _| {
                                                        s.update_parameters(HashMap::from([(
                                                            name.clone(),
                                                            pv,
                                                        )]))
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
                                        move |ev: &MouseMoveEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble {
                                                return;
                                            }
                                            let dragging = weak_vp
                                                .upgrade()
                                                .map(|e| e.read(cx).is_dragging(name.as_str()))
                                                .unwrap_or(false);
                                            if !dragging {
                                                return;
                                            }
                                            let pv = weak_vp
                                                .update(cx, |vp, cx| {
                                                    vp.update_plane_drag(
                                                        &name,
                                                        x,
                                                        y,
                                                        &kind,
                                                        ev.position,
                                                        bounds,
                                                        fallback_bounds,
                                                        cx,
                                                    )
                                                })
                                                .ok()
                                                .flatten();
                                            if let Some(pv) = pv {
                                                services
                                                    .update(cx, |s, _| {
                                                        s.update_parameters(HashMap::from([(
                                                            name.clone(),
                                                            pv,
                                                        )]))
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
                                            .update(cx, |vp, cx| {
                                                vp.end_drag(cx);
                                            })
                                            .ok();
                                    });
                                }
                            }
                        })
                        .w(px(SLIDER_2D_SIZE))
                        .h(px(SLIDER_2D_SIZE)),
                    ),
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
        ParameterValue::Float(v) => render_slider_1d(
            name.to_string(),
            *v,
            false,
            is_locked,
            (bounds[0], bounds[1]),
            services,
            weak_vp,
        )
        .into_any_element(),
        ParameterValue::Int(v) => render_slider_1d(
            name.to_string(),
            *v as f64,
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
        ParameterValue::VectorFloat(v) if v.len() >= 2 => render_slider_2d(
            name.to_string(),
            v[0],
            v[1],
            Slider2dKind::VectorFloat(v[2..].to_vec()),
            is_locked,
            (bounds[0], bounds[1]),
            (bounds[2], bounds[3]),
            services,
            weak_vp,
        )
        .into_any_element(),
        ParameterValue::VectorInt(v) if v.len() >= 2 => render_slider_2d(
            name.to_string(),
            v[0] as f64,
            v[1] as f64,
            Slider2dKind::VectorInt(v[2..].to_vec()),
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

impl Render for Viewport {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let (status, params, timestamp, slide_count) = {
            let exec = self.services.read(cx).execution_state().read(cx);
            (
                exec.status,
                exec.parameters.clone(),
                exec.current_timestamp,
                exec.slide_count,
            )
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
            .child(
                div()
                    .size_full()
                    .bg(theme.viewport_stage_background)
                    .child(self.scene.clone()),
            )
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
        let slider_bounds = self.slider_bounds.clone();

        let slide_label = format!(
            "Slide {} / {}",
            (timestamp.slide + 1).min(slide_count.max(1)),
            slide_count.max(1)
        );
        let time_label = format!("{:.2}s", timestamp.time);

        // collect parameters newest-first
        let sorted: Vec<(String, ParameterValue, bool)> = params
            .map(|p| {
                p.param_order
                    .iter()
                    .rev()
                    .filter_map(|name| {
                        if is_hidden_param(name) {
                            return None;
                        }
                        let is_locked = p.locked_params.contains(name);
                        p.parameters.get(name).map(|v| {
                            (
                                name.clone(),
                                self.display_parameter_value(name, v),
                                is_locked,
                            )
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let controls: Vec<AnyElement> = sorted
            .iter()
            .map(|(name, value, is_locked)| {
                let bounds = slider_bounds
                    .get(name.as_str())
                    .copied()
                    .unwrap_or_else(|| default_bounds_for_value(value));
                render_param_control(
                    name,
                    value,
                    *is_locked,
                    bounds,
                    services_weak.clone(),
                    weak_vp.clone(),
                )
            })
            .collect();
        let params_btn = div()
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
            .on_click(cx.listener(|vp, _, _, cx| vp.toggle_params(cx)));

        if show_params {
            // sidebar (pure black bg) + bare stage
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
                .child(params_btn)
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
            } else {
                div()
                    .id("pres-params-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&scroll_handle)
                    .px(px(12.0))
                    .py(px(8.0))
                    .children(controls)
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

            div()
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
                .into_any_element()
        } else {
            // no sidebar: toolbar above stage, slide/time top-left
            let toolbar = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .h(px(PRES_TOOLBAR_H))
                .flex_shrink_0()
                .bg(PRES_TOOLBAR_BG)
                .child(params_btn)
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
}
