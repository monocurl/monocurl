//! graphing constructors: axes, grids, and the constructors that sample a
//! user function. limits, sampling helpers and shared geometry live here

mod axes;
mod grids;
mod sampling;

use executor::executor::TextRenderQuality;
use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::simd::{Float3, Float4};

use super::{constructors::VectorLikeStyle, helpers::*};

const MAX_CURVE_SAMPLES: usize = 1 << 14;

const MAX_AXIS_TICKS: usize = 1 << 12;

const MAX_GRID_CELLS: usize = 1 << 18;

const MAX_LINE_GRID_POINTS: usize = 1 << 16;

const MAX_FIELD_SAMPLES: usize = 1 << 16;

const MAX_SURFACE_TRIANGLES: usize = 1 << 17;

const DEFAULT_AXIS_TICK_STEP: f32 = 0.25;

const AXIS_BUFFER: f32 = 0.2;

const AXIS_TITLE_SCALE: f32 = 0.6;

const AXIS_TICK_LABEL_SCALE: f32 = 0.5;

const AXIS_TITLE_BUFFER: f32 = 0.18;

const AXIS_TICK_LABEL_BUFFER: f32 = 0.08;

const AXIS_ZERO_TICK_LABEL_OFFSET: f32 = 0.15;

const LARGE_TICK_EXTEND: f32 = 0.075;

const LARGE_TICK_WIDTH: f32 = 1.0;

const LARGE_TICK_GRID_WIDTH: f32 = 1.0;

const LARGE_TICK_OPACITY: f32 = 0.7;

const LARGE_TICK_GRID_OPACITY: f32 = 0.6;

const SMALL_TICK_EXTEND: f32 = 0.05;

const SMALL_TICK_WIDTH: f32 = 0.5;

const SMALL_TICK_OPACITY: f32 = 0.6;

const SMALL_TICK_GRID_OPACITY: f32 = 0.4;

const AXIS_ARROW_STROKE_WIDTH: f32 = 0.0;

const AXIS_ARROW_STYLE: VectorLikeStyle = VectorLikeStyle {
    max_head_radius: 0.0425,
    head_radius_over_length: 0.3,
    stem_radius_over_head_radius: 0.275,
    head_width_over_radius: 1.0,
    head_depth_over_radius: 1.732_050_8,
    max_stem_radius_over_length: 0.04,
    max_head_half_width_over_length: 0.18,
    max_head_depth_over_length: 0.32,
    head_len_scale: 1.0,
    head_width_scale: 1.0,
    double_headed: false,
};

fn mesh_limit_error(kind: &str, actual: usize, limit: usize) -> ExecutorError {
    ExecutorError::invalid_invocation(format!("{kind} is too large ({actual}, limit {limit})"))
}

fn ensure_limit(kind: &str, actual: usize, limit: usize) -> Result<(), ExecutorError> {
    if actual > limit {
        Err(mesh_limit_error(kind, actual, limit))
    } else {
        Ok(())
    }
}

fn checked_product(kind: &str, a: usize, b: usize, limit: usize) -> Result<usize, ExecutorError> {
    let total = a
        .checked_mul(b)
        .ok_or_else(|| mesh_limit_error(kind, usize::MAX, limit))?;
    ensure_limit(kind, total, limit)?;
    Ok(total)
}

fn checked_sum(kind: &str, a: usize, b: usize, limit: usize) -> Result<usize, ExecutorError> {
    let total = a
        .checked_add(b)
        .ok_or_else(|| mesh_limit_error(kind, usize::MAX, limit))?;
    ensure_limit(kind, total, limit)?;
    Ok(total)
}

fn ensure_grid_cells(kind: &str, nx: usize, ny: usize) -> Result<usize, ExecutorError> {
    checked_product(kind, nx, ny, MAX_GRID_CELLS)
}

fn ensure_surface_triangles(kind: &str, tris: usize) -> Result<(), ExecutorError> {
    ensure_limit(kind, tris, MAX_SURFACE_TRIANGLES)
}

fn text_render_quality(executor: &Executor) -> text::RenderQuality {
    match executor.text_render_quality() {
        TextRenderQuality::Normal => text::RenderQuality::Normal,
        TextRenderQuality::High => text::RenderQuality::High,
    }
}

fn normalize_or(vec: Float3, fallback: Float3) -> Float3 {
    if vec.len_sq() > 1e-8 {
        vec.normalize()
    } else {
        fallback
    }
}

/// Append an open polyline to `out`, split into separate contours wherever the
/// sample list contains a `None` (a domain gap / discontinuity). Contiguous
/// runs of `Some` points of length >= 2 each become their own contour.
fn push_segmented_open_polyline(
    out: &mut Vec<geo::mesh::Lin>,
    points: &[Option<Float3>],
    normal: Float3,
) {
    let mut run: Vec<Float3> = Vec::new();
    for point in points {
        match point {
            Some(p) => run.push(*p),
            None => {
                if run.len() >= 2 {
                    push_open_polyline(out, &run, normal);
                }
                run.clear();
            }
        }
    }
    if run.len() >= 2 {
        push_open_polyline(out, &run, normal);
    }
}

fn segmented_open_polyline(points: &[Option<Float3>], normal: Float3) -> Vec<geo::mesh::Lin> {
    let mut out = Vec::with_capacity(points.len().saturating_sub(1));
    push_segmented_open_polyline(&mut out, points, normal);
    out
}

fn push_subdivided_line(out: &mut Vec<geo::mesh::Lin>, a: Float3, b: Float3, subdivision: usize) {
    let base = out.len();
    for i in 0..subdivision {
        let u = i as f32 / subdivision as f32;
        let v = (i + 1) as f32 / subdivision as f32;
        let mut line = default_lin(a.lerp(b, u), a.lerp(b, v), Float3::Z);
        line.prev = if i == 0 { -1 } else { (base + i - 1) as i32 };
        line.next = if i + 1 == subdivision {
            -1
        } else {
            (base + i + 1) as i32
        };
        out.push(line);
    }
}

fn point_samples(samples: i64) -> usize {
    samples.max(1) as usize
}

fn grid_axis_samples(samples: i64) -> usize {
    samples.max(2) as usize
}

fn grid_point(
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    nx: usize,
    ny: usize,
    ix: usize,
    iy: usize,
) -> Float3 {
    let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
    let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
    Float3::new(x, y, 0.0)
}

fn grid_cell_center(
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    nx: usize,
    ny: usize,
    ix: usize,
    iy: usize,
) -> Float3 {
    (grid_point(x0, x1, y0, y1, nx, ny, ix, iy)
        + grid_point(x0, x1, y0, y1, nx, ny, ix + 1, iy + 1))
        / 2.0
}

fn sample_index_value(ix: usize, iy: usize) -> Value {
    list_value([Value::Integer(ix as i64), Value::Integer(iy as i64)])
}

async fn read_optional_color(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Option<Float4>, ExecutorError> {
    let value = executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_wrappers_rec(executor)
        .await?;
    if matches!(value, Value::Nil) {
        Ok(None)
    } else {
        float4_from_value(value, name).map(Some)
    }
}

fn recolor_mesh(mesh: &mut geo::mesh::Mesh, color: Float4) {
    for dot in &mut mesh.dots {
        dot.col = color;
    }
    for lin in &mut mesh.lins {
        lin.a.col = color;
        lin.b.col = color;
    }
    for tri in &mut mesh.tris {
        tri.a.col = color;
        tri.b.col = color;
        tri.c.col = color;
    }
}

fn recolor_tree(tree: &mut MeshTree, color: Float4) {
    tree.for_each_mut(&mut |mesh| recolor_mesh(mesh, color));
}
