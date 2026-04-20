use std::collections::HashMap;

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

// fixed value-units per slider-width (or canvas-size) when dragging outside bounds;
// using a constant instead of proportional-to-range prevents exponential acceleration
const OVERDRAG_RATE_1D: f64 = 4.0;
const OVERDRAG_RATE_2D: f64 = 0.4;
// absolute value-units past the current bounds before the bounds start to expand
const BOUNDS_EXPAND_THRESH_1D: f64 = 0.5;
const BOUNDS_EXPAND_THRESH_2D: f64 = 0.05;
// fraction of current range added as padding when bounds expand
const BOUNDS_EXPAND_PAD: f64 = 0.1;

#[derive(Clone, Copy)]
enum Slider2dKind {
    Complex,
    VectorFloat,
}

pub struct Viewport {
    services: Entity<ServiceManager>,
    scene: Entity<DebugSceneView>,
    is_presenting: bool,
    show_params: bool,
    dragging_param: Option<String>,
    scroll_handle: ScrollHandle,
    // per-parameter value-space bounds: [x_min, x_max, y_min, y_max]
    slider_bounds: HashMap<String, [f64; 4]>,
}

impl Viewport {
    pub fn new(services: Entity<ServiceManager>, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();
        let execution_state = services.read(cx).execution_state().clone();
        let scene = cx.new(|cx| DebugSceneView::new(execution_state, cx));

        Self {
            services,
            scene,
            is_presenting: false,
            show_params: false,
            dragging_param: None,
            scroll_handle: ScrollHandle::new(),
            slider_bounds: HashMap::new(),
        }
    }

    pub fn set_presenting(&mut self, presenting: bool, cx: &mut Context<Self>) {
        self.is_presenting = presenting;
        if !presenting {
            self.show_params = false;
            self.dragging_param = None;
            self.slider_bounds.clear();
        }
        cx.notify();
    }

    pub fn toggle_params(&mut self, cx: &mut Context<Self>) {
        self.show_params = !self.show_params;
        cx.notify();
    }
}

// --- drag computation helpers ---

fn compute_1d_drag(
    local_x: f32,
    w: f32,
    min: f64,
    max: f64,
    is_int: bool,
) -> (ParameterValue, Option<(f64, f64)>) {
    let raw_p = (local_x / w) as f64;
    // outside bounds: fixed rate independent of current range to avoid acceleration
    let val_raw = if raw_p < 0.0 {
        min + raw_p * OVERDRAG_RATE_1D
    } else if raw_p > 1.0 {
        max + (raw_p - 1.0) * OVERDRAG_RATE_1D
    } else {
        min + raw_p * (max - min)
    };
    let val = if is_int { val_raw.round() } else { val_raw };
    let pv = if is_int {
        ParameterValue::Int(val as i64)
    } else {
        ParameterValue::Float(val)
    };

    let range = max - min;
    let mut new_min = if val_raw < min - BOUNDS_EXPAND_THRESH_1D {
        val_raw - BOUNDS_EXPAND_PAD * range
    } else {
        min
    };
    let mut new_max = if val_raw > max + BOUNDS_EXPAND_THRESH_1D {
        val_raw + BOUNDS_EXPAND_PAD * range
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
    (pv, new_bounds)
}

fn compute_2d_drag(
    pos: Point<Pixels>,
    canvas: Bounds<Pixels>,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    kind: Slider2dKind,
) -> (ParameterValue, Option<((f64, f64), (f64, f64))>) {
    let w = f32::from(canvas.size.width);
    let h = f32::from(canvas.size.height);
    let raw_xp = (f32::from(pos.x - canvas.origin.x) / w) as f64;
    let raw_yp = (f32::from(pos.y - canvas.origin.y) / h) as f64;

    let xv = if raw_xp < 0.0 {
        x_min + raw_xp * OVERDRAG_RATE_2D
    } else if raw_xp > 1.0 {
        x_max + (raw_xp - 1.0) * OVERDRAG_RATE_2D
    } else {
        x_min + raw_xp * (x_max - x_min)
    };
    // y-axis flipped: top = y_max, bottom = y_min
    let yv = if raw_yp < 0.0 {
        y_max + (-raw_yp) * OVERDRAG_RATE_2D
    } else if raw_yp > 1.0 {
        y_min - (raw_yp - 1.0) * OVERDRAG_RATE_2D
    } else {
        y_max - raw_yp * (y_max - y_min)
    };

    let pv = match kind {
        Slider2dKind::Complex => ParameterValue::Complex { re: xv, im: yv },
        Slider2dKind::VectorFloat => ParameterValue::VectorFloat(vec![xv, yv]),
    };

    let x_range = x_max - x_min;
    let y_range = y_max - y_min;
    let new_x_min = if xv < x_min - BOUNDS_EXPAND_THRESH_2D {
        xv - BOUNDS_EXPAND_PAD * x_range
    } else {
        x_min
    };
    let new_x_max = if xv > x_max + BOUNDS_EXPAND_THRESH_2D {
        xv + BOUNDS_EXPAND_PAD * x_range
    } else {
        x_max
    };
    let new_y_min = if yv < y_min - BOUNDS_EXPAND_THRESH_2D {
        yv - BOUNDS_EXPAND_PAD * y_range
    } else {
        y_min
    };
    let new_y_max = if yv > y_max + BOUNDS_EXPAND_THRESH_2D {
        yv + BOUNDS_EXPAND_PAD * y_range
    } else {
        y_max
    };

    let new_bounds =
        if new_x_min != x_min || new_x_max != x_max || new_y_min != y_min || new_y_max != y_max {
            Some(((new_x_min, new_x_max), (new_y_min, new_y_max)))
        } else {
            None
        };
    (pv, new_bounds)
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
                                            let (pv, new_bounds) =
                                                compute_1d_drag(local_x, w, min, max, is_int);
                                            weak_vp
                                                .update(cx, |vp, cx| {
                                                    if let Some((new_min, new_max)) = new_bounds {
                                                        vp.slider_bounds.insert(
                                                            name.clone(),
                                                            [new_min, new_max, 0.0, 0.0],
                                                        );
                                                    }
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
                                            let local_x =
                                                f32::from(ev.position.x - bounds.origin.x);
                                            let (pv, new_bounds) =
                                                compute_1d_drag(local_x, w, min, max, is_int);
                                            if let Some((new_min, new_max)) = new_bounds {
                                                let name = name.clone();
                                                weak_vp
                                                    .update(cx, |vp, cx| {
                                                        vp.slider_bounds.insert(
                                                            name,
                                                            [new_min, new_max, 0.0, 0.0],
                                                        );
                                                        cx.notify();
                                                    })
                                                    .ok();
                                            }
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
                                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                        if phase != DispatchPhase::Bubble {
                                            return;
                                        }
                                        weak_vp
                                            .update(cx, |vp, cx| {
                                                vp.dragging_param = None;
                                                cx.notify();
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
    let px_pct = ((x - x_min) / (x_max - x_min)).clamp(0.0, 1.0) as f32;
    let py_pct = 1.0 - ((y - y_min) / (y_max - y_min)).clamp(0.0, 1.0) as f32;
    // zero-crossing axis positions track true zero even when bounds shift
    let x_zero = ((-x_min) / (x_max - x_min)).clamp(0.0, 1.0) as f32;
    let y_zero = 1.0 - ((-y_min) / (y_max - y_min)).clamp(0.0, 1.0) as f32;
    let value_text = match kind {
        Slider2dKind::Complex => {
            if y < 0.0 {
                format!("{:.2} - {:.2}i", x, y.abs())
            } else {
                format!("{:.2} + {:.2}i", x, y)
            }
        }
        Slider2dKind::VectorFloat => format!("({:.2}, {:.2})", x, y),
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
                                    let services = services.clone();
                                    let weak_vp = weak_vp.clone();
                                    window.on_mouse_event(
                                        move |ev: &MouseDownEvent, phase, _, cx| {
                                            if phase != DispatchPhase::Bubble
                                                || !bounds.contains(&ev.position)
                                            {
                                                return;
                                            }
                                            let (pv, new_bounds) = compute_2d_drag(
                                                ev.position,
                                                bounds,
                                                x_min,
                                                x_max,
                                                y_min,
                                                y_max,
                                                kind,
                                            );
                                            weak_vp
                                                .update(cx, |vp, cx| {
                                                    if let Some((
                                                        (nx_min, nx_max),
                                                        (ny_min, ny_max),
                                                    )) = new_bounds
                                                    {
                                                        vp.slider_bounds.insert(
                                                            name.clone(),
                                                            [nx_min, nx_max, ny_min, ny_max],
                                                        );
                                                    }
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
                                            let (pv, new_bounds) = compute_2d_drag(
                                                ev.position,
                                                bounds,
                                                x_min,
                                                x_max,
                                                y_min,
                                                y_max,
                                                kind,
                                            );
                                            if let Some(((nx_min, nx_max), (ny_min, ny_max))) =
                                                new_bounds
                                            {
                                                let name = name.clone();
                                                weak_vp
                                                    .update(cx, |vp, cx| {
                                                        vp.slider_bounds.insert(
                                                            name,
                                                            [nx_min, nx_max, ny_min, ny_max],
                                                        );
                                                        cx.notify();
                                                    })
                                                    .ok();
                                            }
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
                                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                        if phase != DispatchPhase::Bubble {
                                            return;
                                        }
                                        weak_vp
                                            .update(cx, |vp, cx| {
                                                vp.dragging_param = None;
                                                cx.notify();
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
            Slider2dKind::VectorFloat,
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
        let (ring_color, ring_width, params, timestamp, slide_count) = {
            let exec = self.services.read(cx).execution_state().read(cx);
            let show_ring = if self.is_presenting {
                exec.has_error()
            } else {
                true
            };
            let ring_width = if show_ring {
                if matches!(exec.status, ExecutionStatus::Playing | ExecutionStatus::Paused) {
                    px(1.0)
                } else {
                    px(4.0)
                }
            } else {
                px(0.0)
            };
            (
                theme.viewport_status_ring(exec.status),
                ring_width,
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
            .p(ring_width)
            .child(
                div()
                    .size_full()
                    .bg(theme.viewport_stage_background)
                    .child(self.scene.clone()),
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
                        let is_locked = p.locked_params.contains(name);
                        p.parameters
                            .get(name)
                            .map(|v| (name.clone(), v.clone(), is_locked))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let controls: Vec<AnyElement> = sorted
            .iter()
            .map(|(name, value, is_locked)| {
                let bounds =
                    slider_bounds
                        .get(name.as_str())
                        .copied()
                        .unwrap_or_else(|| match value {
                            ParameterValue::Float(_) | ParameterValue::Int(_) => {
                                [SLIDER_1D_MIN, SLIDER_1D_MAX, 0.0, 0.0]
                            }
                            _ => [SLIDER_2D_MIN, SLIDER_2D_MAX, SLIDER_2D_MIN, SLIDER_2D_MAX],
                        });
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
