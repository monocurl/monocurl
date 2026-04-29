use std::time::Duration;

use gpui::Rgba;

use crate::{services::ExecutionStatus, theme::Theme};

pub(super) const PRES_BG: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
pub(super) const PRES_TOOLBAR_BG: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.8,
};
pub(super) const PRES_PANEL_BG: Rgba = Rgba {
    r: 0.10,
    g: 0.10,
    b: 0.10,
    a: 1.0,
};
pub(super) const PRES_BORDER: Rgba = Rgba {
    r: 0.22,
    g: 0.22,
    b: 0.22,
    a: 1.0,
};
pub(super) const PRES_TEXT: Rgba = Rgba {
    r: 0.85,
    g: 0.85,
    b: 0.85,
    a: 1.0,
};
pub(super) const PRES_MUTED: Rgba = Rgba {
    r: 0.50,
    g: 0.50,
    b: 0.50,
    a: 1.0,
};
pub(super) const PRES_ACCENT: Rgba = Rgba {
    r: 0.47,
    g: 0.63,
    b: 0.87,
    a: 1.0,
};
pub(super) const SLIDER_TRACK_BG: Rgba = Rgba {
    r: 0.28,
    g: 0.28,
    b: 0.28,
    a: 1.0,
};
pub(super) const SLIDER_THUMB: Rgba = Rgba {
    r: 0.90,
    g: 0.90,
    b: 0.90,
    a: 1.0,
};
pub(super) const SLIDER_THUMB_LOCKED: Rgba = Rgba {
    r: 0.45,
    g: 0.45,
    b: 0.45,
    a: 1.0,
};
pub(super) const SLIDER_FILL_LOCKED: Rgba = Rgba {
    r: 0.30,
    g: 0.30,
    b: 0.38,
    a: 1.0,
};
pub(super) const SLIDER_2D_BG: Rgba = Rgba {
    r: 0.18,
    g: 0.18,
    b: 0.18,
    a: 1.0,
};
pub(super) const SLIDER_2D_GRID: Rgba = Rgba {
    r: 0.32,
    g: 0.32,
    b: 0.32,
    a: 1.0,
};
pub(super) const TRANSPARENT: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

pub(super) const PRES_TOOLBAR_H: f32 = 40.0;
pub(super) const PARAM_PANEL_W: f32 = 260.0;
pub(super) const SLIDER_TRACK_H: f32 = 4.0;
pub(super) const SLIDER_THUMB_R: f32 = 7.0;
pub(super) const SLIDER_1D_CANVAS_H: f32 = 28.0;
pub(super) const SLIDER_1D_W: f32 = 110.0;
pub(super) const SLIDER_1D_MIN: f64 = -10.0;
pub(super) const SLIDER_1D_MAX: f64 = 10.0;
pub(super) const SLIDER_2D_SIZE: f32 = 120.0;
pub(super) const SLIDER_2D_MIN: f64 = -1.0;
pub(super) const SLIDER_2D_MAX: f64 = 1.0;
pub(super) const SLIDER_2D_DOT_R: f32 = 5.0;
pub(super) const SLIDER_2D_GRID_DIVISIONS: usize = 4;
pub(super) const SLIDER_1D_EDGE_GAP: f32 = SLIDER_THUMB_R + 1.0;
pub(super) const SLIDER_2D_EDGE_GAP: f32 = SLIDER_2D_DOT_R + 2.0;
pub(super) const RING_TRANSITION: Duration = Duration::from_millis(140);
pub(super) const OVERDRAG_TICK: Duration = Duration::from_nanos(8_333_333);
pub(super) const ESCAPE_SPEED_NEAR_PX: f32 = 8.0;
pub(super) const ESCAPE_SPEED_FAR_PX: f32 = 96.0;
pub(super) const ESCAPE_SPEED_MAX_MULT: f64 = 5.0;
pub(super) const OVERDRAG_STEP_1D: f64 = 0.02;
pub(super) const OVERDRAG_STEP_2D: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RingStyle {
    pub color: Rgba,
    pub width: f32,
}

pub(super) fn lerp_f32(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

pub(super) fn lerp_rgba(start: Rgba, end: Rgba, t: f32) -> Rgba {
    Rgba {
        r: lerp_f32(start.r, end.r, t),
        g: lerp_f32(start.g, end.g, t),
        b: lerp_f32(start.b, end.b, t),
        a: lerp_f32(start.a, end.a, t),
    }
}

pub(super) fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..color }
}

pub(super) fn ring_style_for(
    status: ExecutionStatus,
    _is_presenting: bool,
    is_loading: bool,
    theme: Theme,
) -> RingStyle {
    if is_loading && matches!(status, ExecutionStatus::Playing | ExecutionStatus::Paused) {
        return RingStyle {
            color: with_alpha(theme.viewport_status_loading, 0.9),
            width: 1.5,
        };
    }

    match status {
        ExecutionStatus::Playing | ExecutionStatus::Paused => RingStyle {
            color: theme.viewport_status_ring(status),
            width: 1.0,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_mode_uses_same_status_rings_as_preview() {
        let theme = Theme::dark();

        assert_eq!(
            ring_style_for(ExecutionStatus::Playing, true, false, theme),
            ring_style_for(ExecutionStatus::Playing, false, false, theme)
        );
        assert_eq!(
            ring_style_for(ExecutionStatus::Paused, true, false, theme),
            ring_style_for(ExecutionStatus::Paused, false, false, theme)
        );
        assert_eq!(
            ring_style_for(ExecutionStatus::Playing, true, true, theme),
            ring_style_for(ExecutionStatus::Playing, false, true, theme)
        );
        assert_eq!(
            ring_style_for(ExecutionStatus::RuntimeError, true, false, theme),
            ring_style_for(ExecutionStatus::RuntimeError, false, false, theme)
        );
        assert_eq!(
            ring_style_for(ExecutionStatus::CompileError, true, false, theme),
            ring_style_for(ExecutionStatus::CompileError, false, false, theme)
        );
    }
}
