use std::collections::HashMap;

use executor::{
    error::ExecutorError,
    executor::{Executor, TextRenderQuality},
    heap::with_heap,
    value::Value,
};
use geo::{
    mesh_build::SurfaceVertex,
    simd::{Float2, Float3, Float4},
};
use smallvec::{SmallVec, smallvec};
use stdlib_macros::stdlib_func;

use super::{
    constructors::{VectorLikeStyle, vector_like_mesh_with_style},
    helpers::*,
};

const MAX_CURVE_SAMPLES: usize = 1 << 14;
const MAX_AXIS_TICKS: usize = 1 << 12;
const MAX_GRID_CELLS: usize = 1 << 16;
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

fn ensure_grid_cells(kind: &str, nx: usize, ny: usize) -> Result<usize, ExecutorError> {
    checked_product(kind, nx, ny, MAX_GRID_CELLS)
}

fn ensure_surface_triangles(kind: &str, tris: usize) -> Result<(), ExecutorError> {
    ensure_limit(kind, tris, MAX_SURFACE_TRIANGLES)
}

fn text_render_quality(executor: &Executor) -> latex::RenderQuality {
    match executor.text_render_quality() {
        TextRenderQuality::Normal => latex::RenderQuality::Normal,
        TextRenderQuality::High => latex::RenderQuality::High,
    }
}

fn normalize_or(vec: Float3, fallback: Float3) -> Float3 {
    if vec.len_sq() > 1e-8 {
        vec.normalize()
    } else {
        fallback
    }
}

fn open_polyline(points: &[Float3], normal: Float3) -> Vec<geo::mesh::Lin> {
    let mut out = Vec::with_capacity(points.len().saturating_sub(1));
    push_open_polyline(&mut out, points, normal);
    out
}

fn tick_count(min: f32, max: f32, step: f32) -> usize {
    if max < min {
        0
    } else {
        (((max - min) / step).ceil() as usize).saturating_add(1)
    }
}

fn axis_tick_values(min: f32, max: f32, step: f32) -> Vec<(i64, f32)> {
    if max < min {
        return Vec::new();
    }

    let first = (min / step).ceil() as i64;
    let last = (max / step).floor() as i64;
    (first..=last)
        .map(|tick| (tick, tick as f32 * step))
        .collect()
}

#[derive(Clone, Copy)]
struct AxisRange {
    min: f32,
    max: f32,
    tick_step: f32,
}

fn axis_number_from_value(value: Value, name: &'static str) -> Result<f32, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::Integer(value) => Ok(value as f32),
        Value::Float(value) => Ok(value as f32),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            name,
        )),
    }
}

fn checked_axis_tick_step(value: f32, name: &'static str) -> Result<f32, ExecutorError> {
    if !value.is_finite() || value == 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "tick step must be a non-zero finite number",
        });
    }
    Ok(value.abs().max(1e-3))
}

fn checked_axis_range(
    min: f32,
    max: f32,
    tick_step: f32,
    name: &'static str,
) -> Result<AxisRange, ExecutorError> {
    if !min.is_finite() || !max.is_finite() {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "range bounds must be finite numbers",
        });
    }
    Ok(AxisRange {
        min,
        max,
        tick_step: checked_axis_tick_step(tick_step, name)?,
    })
}

fn axis_radius_range(radius: f32, name: &'static str) -> Result<AxisRange, ExecutorError> {
    if !radius.is_finite() || radius < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "radius must be a non-negative finite number",
        });
    }
    checked_axis_range(-radius, radius, DEFAULT_AXIS_TICK_STEP, name)
}

fn read_axis_range(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<AxisRange, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::Integer(radius) => axis_radius_range(radius as f32, name),
        Value::Float(radius) => axis_radius_range(radius as f32, name),
        Value::List(list) => match list.elements().len() {
            1 => {
                let radius = axis_number_from_value(
                    with_heap(|h| h.get(list.elements()[0].key()).clone()),
                    name,
                )?;
                axis_radius_range(radius, name)
            }
            2 | 3 => {
                let min = axis_number_from_value(
                    with_heap(|h| h.get(list.elements()[0].key()).clone()),
                    name,
                )?;
                let max = axis_number_from_value(
                    with_heap(|h| h.get(list.elements()[1].key()).clone()),
                    name,
                )?;
                let tick_step = if list.elements().len() == 3 {
                    axis_number_from_value(
                        with_heap(|h| h.get(list.elements()[2].key()).clone()),
                        name,
                    )?
                } else {
                    DEFAULT_AXIS_TICK_STEP
                };
                checked_axis_range(min, max, tick_step, name)
            }
            len => Err(ExecutorError::invalid_operation(format!(
                "{name}: expected a number or a list [min, max, tick_step], got list of length {len}"
            ))),
        },
        other => Err(ExecutorError::type_error_for(
            "number or list",
            other.type_name(),
            name,
        )),
    }
}

fn read_axis_basis(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Float3, ExecutorError> {
    let basis = read_float3(executor, stack_idx, index, name)?;
    checked_axis_basis(basis, name)
}

fn checked_axis_basis(basis: Float3, name: &'static str) -> Result<Float3, ExecutorError> {
    if !basis.x.is_finite()
        || !basis.y.is_finite()
        || !basis.z.is_finite()
        || basis.len_sq() <= 1e-8
    {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "must be a non-zero finite vector",
        });
    }
    Ok(basis)
}

fn read_axis_basis_list(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
    axis_names: &[&'static str],
) -> Result<Vec<Float3>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::List(list) if list.elements().len() == axis_names.len() => list
            .elements()
            .iter()
            .zip(axis_names.iter().copied())
            .map(|(key, axis_name)| {
                checked_axis_basis(
                    float3_from_value(with_heap(|h| h.get(key.key()).clone()), axis_name)?,
                    axis_name,
                )
            })
            .collect(),
        Value::List(list) => Err(ExecutorError::invalid_operation(format!(
            "{name}: expected list of length {}, got list of length {}",
            axis_names.len(),
            list.elements().len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "axis basis list",
            other.type_name(),
            name,
        )),
    }
}

fn label_rate_from_value(value: Value, name: &'static str) -> Result<usize, ExecutorError> {
    let rate = match value.elide_lvalue_leader_rec() {
        Value::Integer(value) => value,
        Value::Float(value) if value.fract() == 0.0 => value as i64,
        other => {
            return Err(ExecutorError::type_error_for(
                "int",
                other.type_name(),
                name,
            ));
        }
    };
    if rate < 0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "must be non-negative",
        });
    }
    Ok(rate as usize)
}

fn read_label_rate_list(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
    rate_names: &[&'static str],
) -> Result<Vec<usize>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::List(list) if list.elements().len() == rate_names.len() => list
            .elements()
            .iter()
            .zip(rate_names.iter().copied())
            .map(|(key, rate_name)| {
                label_rate_from_value(with_heap(|h| h.get(key.key()).clone()), rate_name)
            })
            .collect(),
        Value::List(list) => Err(ExecutorError::invalid_operation(format!(
            "{name}: expected list of length {}, got list of length {}",
            rate_names.len(),
            list.elements().len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "label rate list",
            other.type_name(),
            name,
        )),
    }
}

fn format_axis_number(value: f32) -> String {
    let value = if value.abs() < 1e-6 { 0.0 } else { value };
    let abs = value.abs();
    if abs >= 10_000.0 || (abs > 0.0 && abs < 0.001) {
        return format!("{value:.4e}");
    }

    let mut out = format!("{value:.6}");
    while out.contains('.') && out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
    if out == "-0" {
        out.clear();
        out.push('0');
    }
    out
}

fn point_samples(samples: i64) -> usize {
    samples.max(1) as usize
}

fn grid_axis_samples(samples: i64) -> usize {
    samples.max(2) as usize
}

fn sample_index_value(ix: usize, iy: usize) -> Value {
    list_value([Value::Integer(ix as i64), Value::Integer(iy as i64)])
}

async fn read_optional_string(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Option<String>, ExecutorError> {
    let value = executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_wrappers_rec(executor)
        .await?;
    if matches!(value, Value::Nil) {
        return Ok(None);
    }

    crate::stringify_value(executor, value)
        .await
        .map(Some)
        .map_err(|error| match error {
            ExecutorError::TypeError { got, .. } => {
                ExecutorError::type_error_for(crate::STRING_COMPATIBLE_DESC, got, name)
            }
            other => other,
        })
}

fn read_label_rate(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<usize, ExecutorError> {
    let value = read_int(executor, stack_idx, index, name)?;
    if value < 0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "must be non-negative",
        });
    }
    Ok(value as usize)
}

fn axis_text_basis(right: Float3, normal: Float3) -> (Float3, Float3, Float3) {
    let normal = normalize_or(normal, Float3::Z);
    let projected_right = right - normal * right.dot(normal);
    let right = if projected_right.len_sq() > 1e-8 {
        projected_right.normalize()
    } else {
        polygon_basis(normal).0
    };
    let up = normalize_or(normal.cross(right), polygon_basis(normal).1);
    (right, up, normal)
}

fn orient_text_tree(tree: &mut MeshTree, right: Float3, normal: Float3) {
    let (right, up, normal) = axis_text_basis(right, normal);
    tree.for_each_mut(&mut |mesh| {
        transform_mesh_positions(mesh, |p| right * p.x + up * p.y + normal * p.z)
    });
}

fn place_tree_next_to_point(tree: &mut MeshTree, anchor: Float3, dir: Float3, buffer: f32) {
    let dir = normalize_or(dir, Float3::X);
    let center = tree_center(tree).unwrap_or(Float3::ZERO);
    let label_face = extremal_point(tree, -dir).unwrap_or(center).dot(dir);
    let orth = (anchor - center) - dir * (anchor - center).dot(dir);
    let delta = dir * (anchor.dot(dir) + buffer - label_face) + orth;
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
}

fn render_axis_tex_tree(
    executor: &Executor,
    tex: &str,
    scale: f32,
    name: &'static str,
) -> Result<Option<MeshTree>, ExecutorError> {
    let meshes = latex::render_tex_with_quality(tex, scale, text_render_quality(executor))
        .map_err(|error| {
            ExecutorError::invalid_invocation(format!("{name} render failed: {error:#}"))
        })?;
    if meshes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(MeshTree::List(
            meshes.into_iter().map(MeshTree::Mesh).collect(),
        )))
    }
}

fn styled_line_mesh(lins: Vec<geo::mesh::Lin>, stroke_radius: f32, alpha: f32) -> Option<Value> {
    if lins.is_empty() {
        return None;
    }

    let Value::Mesh(mesh) = mesh_from_parts(vec![], lins, vec![]) else {
        unreachable!("mesh_from_parts always returns a mesh")
    };
    let mut mesh = (*mesh).clone();
    mesh.uniform.stroke_radius = stroke_radius;
    for lin in &mut mesh.lins {
        lin.a.col.w *= alpha;
        lin.b.col.w *= alpha;
    }
    Some(Value::Mesh(std::sync::Arc::new(mesh)))
}

fn push_styled_line_meshes(
    out: &mut Vec<Value>,
    small_lins: Vec<geo::mesh::Lin>,
    large_lins: Vec<geo::mesh::Lin>,
    grid: bool,
) {
    let (small_width, small_alpha, large_width, large_alpha) = if grid {
        (
            SMALL_TICK_WIDTH,
            SMALL_TICK_GRID_OPACITY,
            LARGE_TICK_GRID_WIDTH,
            LARGE_TICK_GRID_OPACITY,
        )
    } else {
        (
            SMALL_TICK_WIDTH,
            SMALL_TICK_OPACITY,
            LARGE_TICK_WIDTH,
            LARGE_TICK_OPACITY,
        )
    };

    if let Some(mesh) = styled_line_mesh(small_lins, small_width, small_alpha) {
        out.push(mesh);
    }
    if let Some(mesh) = styled_line_mesh(large_lins, large_width, large_alpha) {
        out.push(mesh);
    }
}

fn axis_result(axis_meshes: Vec<Value>, labels: Vec<Value>) -> Value {
    let mut values = axis_meshes;
    values.extend(labels);
    if values.len() == 1 {
        values.pop().expect("length checked")
    } else {
        list_value(values)
    }
}

fn axis_arrow_mesh(tail: Float3, delta: Float3, normal: Float3) -> Result<Value, ExecutorError> {
    let Value::Mesh(mesh) =
        vector_like_mesh_with_style(tail, delta, normal, 0.0, AXIS_ARROW_STYLE)?
    else {
        unreachable!("vector_like_mesh_with_style always returns a mesh")
    };
    let mut mesh = (*mesh).clone();
    mesh.uniform.stroke_radius = AXIS_ARROW_STROKE_WIDTH;
    Ok(Value::Mesh(std::sync::Arc::new(mesh)))
}

fn push_axis_arrows(
    out: &mut Vec<Value>,
    center: Float3,
    basis: Float3,
    range: AxisRange,
    normal: Float3,
) -> Result<(), ExecutorError> {
    let dir = basis.normalize();
    out.push(axis_arrow_mesh(
        center,
        basis * range.max + dir * AXIS_BUFFER,
        normal,
    )?);
    out.push(axis_arrow_mesh(
        center,
        basis * range.min - dir * AXIS_BUFFER,
        normal,
    )?);
    Ok(())
}

fn axis_title_anchor(center: Float3, basis: Float3, max: f32) -> Float3 {
    center + basis * max + basis.normalize() * AXIS_BUFFER
}

fn axis_tick_lins(
    ticks: &[(i64, f32)],
    center: Float3,
    basis: Float3,
    side: Float3,
    normal: Float3,
) -> (Vec<geo::mesh::Lin>, Vec<geo::mesh::Lin>) {
    let side = normalize_or(side, polygon_basis(basis.normalize()).1);
    let mut small = Vec::new();
    let mut large = Vec::new();
    for &(tick, value) in ticks {
        let p = center + basis * value;
        let target = if tick % 4 == 0 {
            &mut large
        } else {
            &mut small
        };
        let extend = if tick % 4 == 0 {
            LARGE_TICK_EXTEND
        } else {
            SMALL_TICK_EXTEND
        };
        target.push(default_lin(p - side * extend, p + side * extend, normal));
    }
    (small, large)
}

fn axis_grid_lins(
    ticks: &[(i64, f32)],
    center: Float3,
    basis: Float3,
    cross_basis: Float3,
    cross_range: AxisRange,
    normal: Float3,
) -> (Vec<geo::mesh::Lin>, Vec<geo::mesh::Lin>) {
    let mut small = Vec::new();
    let mut large = Vec::new();
    for &(tick, value) in ticks {
        let p = center + basis * value;
        let target = if tick % 4 == 0 {
            &mut large
        } else {
            &mut small
        };
        target.push(default_lin(
            p + cross_basis * cross_range.min,
            p + cross_basis * cross_range.max,
            normal,
        ));
    }
    (small, large)
}

fn push_axis_title(
    executor: &Executor,
    out: &mut Vec<Value>,
    label: Option<String>,
    anchor: Float3,
    dir: Float3,
    text_right: Float3,
    normal: Float3,
) -> Result<(), ExecutorError> {
    let Some(label) = label else {
        return Ok(());
    };
    let Some(mut tree) = render_axis_tex_tree(executor, &label, AXIS_TITLE_SCALE, "axis label")?
    else {
        return Ok(());
    };
    orient_text_tree(&mut tree, text_right, normal);
    place_tree_next_to_point(&mut tree, anchor, dir, AXIS_TITLE_BUFFER);
    out.push(tree.into_value());
    Ok(())
}

fn push_axis_tick_labels(
    executor: &Executor,
    out: &mut Vec<Value>,
    ticks: &[(i64, f32)],
    center: Float3,
    basis: Float3,
    side: Float3,
    text_right: Float3,
    normal: Float3,
    label_rate: usize,
    label_zero: bool,
    zero_offset: f32,
) -> Result<(), ExecutorError> {
    if label_rate == 0 {
        return Ok(());
    }

    let label_rate = label_rate as i64;
    let dir = basis.normalize();
    for &(tick, value) in ticks {
        if tick % label_rate != 0 || (!label_zero && value.abs() <= 1e-4) {
            continue;
        }
        let mut anchor = center + basis * value;
        if value.abs() <= 1e-4 {
            anchor = anchor - dir * zero_offset;
        }
        let text = format_axis_number(value);
        let Some(mut tree) =
            render_axis_tex_tree(executor, &text, AXIS_TICK_LABEL_SCALE, "axis tick label")?
        else {
            continue;
        };
        orient_text_tree(&mut tree, text_right, normal);
        place_tree_next_to_point(&mut tree, anchor, side, AXIS_TICK_LABEL_BUFFER);
        out.push(tree.into_value());
    }
    Ok(())
}

#[stdlib_func]
pub async fn mk_color_grid(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let x0 = crate::read_float(executor, stack_idx, -8, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -7, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -6, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -5, "y1")? as f32;
    let x_samples = grid_axis_samples(read_int(executor, stack_idx, -4, "x_samples")?);
    let y_samples = grid_axis_samples(read_int(executor, stack_idx, -3, "y_samples")?);
    let mask = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue();
    let color_at = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();

    let nx = x_samples - 1;
    let ny = y_samples - 1;
    let cell_count = ensure_grid_cells("color grid cells", nx, ny)?;
    ensure_surface_triangles("color grid triangles", cell_count.saturating_mul(2))?;
    let mut vertices = Vec::<SurfaceVertex>::new();
    let mut faces = Vec::<[usize; 3]>::new();
    let mut positions = Vec::with_capacity(x_samples * y_samples);
    let mut color_args = Vec::<SmallVec<[Value; 2]>>::with_capacity(x_samples * y_samples);
    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let pos = Float3::new(x, y, 0.0);
            positions.push(pos);
            color_args.push(smallvec![point_value(pos), sample_index_value(ix, iy)]);
        }
    }
    let colors = invoke_callable_many(executor, &color_at, &color_args, "color_at").await?;
    for (pos, color) in positions.into_iter().zip(colors) {
        vertices.push(SurfaceVertex {
            pos,
            col: float4_from_value(color, "color_at")?,
            uv: Float2::ZERO,
        });
    }
    let grid_vertex = |ix: usize, iy: usize| ix * y_samples + iy;

    let mut centers = Vec::with_capacity(nx * ny);
    let mut mask_args = Vec::<SmallVec<[Value; 2]>>::with_capacity(nx * ny);
    for ix in 0..nx {
        for iy in 0..ny {
            let xa = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let xb = x0 + (x1 - x0) * (ix + 1) as f32 / nx as f32;
            let ya = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let yb = y0 + (y1 - y0) * (iy + 1) as f32 / ny as f32;
            let center = Float3::new((xa + xb) / 2.0, (ya + yb) / 2.0, 0.0);
            centers.push((ix, iy));
            mask_args.push(smallvec![point_value(center)]);
        }
    }
    let mask_values = invoke_callable_many(executor, &mask, &mask_args, "mask").await?;
    for ((ix, iy), mask_value) in centers.into_iter().zip(mask_values) {
        if !mask_value.check_truthy()? {
            continue;
        }

        let a = grid_vertex(ix, iy);
        let b = grid_vertex(ix + 1, iy);
        let c = grid_vertex(ix + 1, iy + 1);
        let d = grid_vertex(ix, iy + 1);
        faces.push([a, b, c]);
        faces.push([a, c, d]);
    }

    let (lins, tris) = build_indexed_surface(&vertices, &faces, &HashMap::new());
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_field(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x0 = crate::read_float(executor, stack_idx, -8, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -7, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -6, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -5, "y1")? as f32;
    let x_samples = point_samples(read_int(executor, stack_idx, -4, "x_samples")?);
    let y_samples = point_samples(read_int(executor, stack_idx, -3, "y_samples")?);
    let mask = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue();
    let mesh_at = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();

    let sample_count = checked_product("field samples", x_samples, y_samples, MAX_FIELD_SAMPLES)?;
    let mut out = Vec::with_capacity(sample_count);
    let nx = x_samples.saturating_sub(1).max(1);
    let ny = y_samples.saturating_sub(1).max(1);

    let mut samples = Vec::with_capacity(sample_count);
    let mut mask_args = Vec::<SmallVec<[Value; 2]>>::with_capacity(sample_count);
    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let pos = Float3::new(x, y, 0.0);
            samples.push((pos, ix, iy));
            mask_args.push(smallvec![point_value(pos)]);
        }
    }
    let mask_values = invoke_callable_many(executor, &mask, &mask_args, "mask").await?;
    let mut mesh_args = Vec::<SmallVec<[Value; 2]>>::new();
    for ((pos, ix, iy), mask_value) in samples.into_iter().zip(mask_values) {
        if mask_value.check_truthy()? {
            mesh_args.push(smallvec![point_value(pos), sample_index_value(ix, iy)]);
        }
    }
    out.extend(invoke_callable_many(executor, &mesh_at, &mesh_args, "mesh_at").await?);

    Ok(list_value(out))
}

#[stdlib_func]
pub async fn mk_axis1d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -6, "center")?;
    let axis = read_axis_basis(executor, stack_idx, -5, "axis")?;
    let axis_dir = axis.normalize();
    let normal = read_float3(executor, stack_idx, -4, "normal")?;
    let range = read_axis_range(executor, stack_idx, -3, "range")?;
    let label = read_optional_string(executor, stack_idx, -2, "label").await?;
    let label_rate = read_label_rate(executor, stack_idx, -1, "label_rate")?;
    ensure_limit(
        "axis ticks",
        tick_count(range.min, range.max, range.tick_step),
        MAX_AXIS_TICKS,
    )?;
    let tick_dir = {
        let dir = normal.cross(axis_dir);
        if dir.len_sq() > 1e-6 {
            dir.normalize()
        } else {
            polygon_basis(axis_dir).1
        }
    };
    let ticks = axis_tick_values(range.min, range.max, range.tick_step);
    let mut axis_meshes = Vec::new();
    push_axis_arrows(&mut axis_meshes, center, axis, range, normal)?;
    let (small_ticks, large_ticks) = axis_tick_lins(&ticks, center, axis, tick_dir, normal);
    push_styled_line_meshes(&mut axis_meshes, small_ticks, large_ticks, false);

    let mut labels = Vec::new();
    push_axis_tick_labels(
        executor,
        &mut labels,
        &ticks,
        center,
        axis,
        -tick_dir,
        axis_dir,
        normal,
        label_rate,
        true,
        AXIS_ZERO_TICK_LABEL_OFFSET,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        label,
        axis_title_anchor(center, axis, range.max),
        axis_dir,
        axis_dir,
        normal,
    )?;
    Ok(axis_result(axis_meshes, labels))
}

#[stdlib_func]
pub async fn mk_axis2d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -8, "center")?;
    let axes = read_axis_basis_list(executor, stack_idx, -7, "axes", &["x_axis", "y_axis"])?;
    let [x_axis, y_axis]: [Float3; 2] = axes.try_into().expect("length checked");
    let x_range = read_axis_range(executor, stack_idx, -6, "x")?;
    let y_range = read_axis_range(executor, stack_idx, -5, "y")?;
    let x_label = read_optional_string(executor, stack_idx, -4, "x_label").await?;
    let y_label = read_optional_string(executor, stack_idx, -3, "y_label").await?;
    let label_rates = read_label_rate_list(
        executor,
        stack_idx,
        -2,
        "tick_label_rates",
        &["x_label_rate", "y_label_rate"],
    )?;
    let [x_label_rate, y_label_rate]: [usize; 2] = label_rates.try_into().expect("length checked");
    let grid = read_flag(executor, stack_idx, -1, "grid")?;
    ensure_limit(
        "axis x ticks",
        tick_count(x_range.min, x_range.max, x_range.tick_step),
        MAX_AXIS_TICKS,
    )?;
    ensure_limit(
        "axis y ticks",
        tick_count(y_range.min, y_range.max, y_range.tick_step),
        MAX_AXIS_TICKS,
    )?;
    let x_dir = x_axis.normalize();
    let y_dir = y_axis.normalize();
    let normal = x_dir.cross(y_dir);
    if normal.len_sq() <= 1e-8 {
        return Err(ExecutorError::InvalidArgument {
            arg: "axes",
            message: "x_axis and y_axis must not be parallel",
        });
    }
    let normal = normal.normalize();
    let x_ticks = axis_tick_values(x_range.min, x_range.max, x_range.tick_step);
    let y_ticks = axis_tick_values(y_range.min, y_range.max, y_range.tick_step);
    let mut axis_meshes = Vec::new();
    push_axis_arrows(&mut axis_meshes, center, x_axis, x_range, normal)?;
    push_axis_arrows(&mut axis_meshes, center, y_axis, y_range, normal)?;
    if grid {
        let (small_x, large_x) = axis_grid_lins(&x_ticks, center, x_axis, y_axis, y_range, normal);
        let (small_y, large_y) = axis_grid_lins(&y_ticks, center, y_axis, x_axis, x_range, normal);
        push_styled_line_meshes(&mut axis_meshes, small_x, large_x, true);
        push_styled_line_meshes(&mut axis_meshes, small_y, large_y, true);
    } else {
        let (small_x, large_x) = axis_tick_lins(&x_ticks, center, x_axis, y_dir, normal);
        let (small_y, large_y) = axis_tick_lins(&y_ticks, center, y_axis, x_dir, normal);
        push_styled_line_meshes(&mut axis_meshes, small_x, large_x, false);
        push_styled_line_meshes(&mut axis_meshes, small_y, large_y, false);
    }

    let mut labels = Vec::new();
    push_axis_tick_labels(
        executor,
        &mut labels,
        &x_ticks,
        center,
        x_axis,
        -y_dir,
        x_dir,
        normal,
        x_label_rate,
        true,
        AXIS_ZERO_TICK_LABEL_OFFSET,
    )?;
    push_axis_tick_labels(
        executor,
        &mut labels,
        &y_ticks,
        center,
        y_axis,
        -x_dir,
        x_dir,
        normal,
        y_label_rate,
        false,
        0.0,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        x_label,
        axis_title_anchor(center, x_axis, x_range.max),
        x_dir,
        x_dir,
        normal,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        y_label,
        axis_title_anchor(center, y_axis, y_range.max),
        y_dir,
        x_dir,
        normal,
    )?;
    Ok(axis_result(axis_meshes, labels))
}

#[stdlib_func]
pub async fn mk_axis3d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -10, "center")?;
    let axes = read_axis_basis_list(
        executor,
        stack_idx,
        -9,
        "axes",
        &["x_axis", "y_axis", "z_axis"],
    )?;
    let [x_axis, y_axis, z_axis]: [Float3; 3] = axes.try_into().expect("length checked");
    let x_range = read_axis_range(executor, stack_idx, -8, "x")?;
    let y_range = read_axis_range(executor, stack_idx, -7, "y")?;
    let z_range = read_axis_range(executor, stack_idx, -6, "z")?;
    let x_label = read_optional_string(executor, stack_idx, -5, "x_label").await?;
    let y_label = read_optional_string(executor, stack_idx, -4, "y_label").await?;
    let z_label = read_optional_string(executor, stack_idx, -3, "z_label").await?;
    let label_rates = read_label_rate_list(
        executor,
        stack_idx,
        -2,
        "tick_label_rates",
        &["x_label_rate", "y_label_rate", "z_label_rate"],
    )?;
    let [x_label_rate, y_label_rate, z_label_rate]: [usize; 3] =
        label_rates.try_into().expect("length checked");
    let grid = read_flag(executor, stack_idx, -1, "grid")?;
    ensure_limit(
        "axis x ticks",
        tick_count(x_range.min, x_range.max, x_range.tick_step),
        MAX_AXIS_TICKS,
    )?;
    ensure_limit(
        "axis y ticks",
        tick_count(y_range.min, y_range.max, y_range.tick_step),
        MAX_AXIS_TICKS,
    )?;
    ensure_limit(
        "axis z ticks",
        tick_count(z_range.min, z_range.max, z_range.tick_step),
        MAX_AXIS_TICKS,
    )?;
    let x_dir = x_axis.normalize();
    let y_dir = y_axis.normalize();
    let z_dir = z_axis.normalize();
    if x_dir.cross(y_dir).len_sq() <= 1e-8
        || x_dir.cross(z_dir).len_sq() <= 1e-8
        || y_dir.cross(z_dir).len_sq() <= 1e-8
    {
        return Err(ExecutorError::InvalidArgument {
            arg: "axes",
            message: "axis basis vectors must not be parallel",
        });
    }
    let xy_normal = normalize_or(x_dir.cross(y_dir), z_dir);
    let xz_normal = y_dir;
    let yz_normal = normalize_or(y_dir.cross(z_dir), x_dir);
    let z_normal = normalize_or(x_dir.cross(z_dir), xy_normal);
    let x_ticks = axis_tick_values(x_range.min, x_range.max, x_range.tick_step);
    let y_ticks = axis_tick_values(y_range.min, y_range.max, y_range.tick_step);
    let z_ticks = axis_tick_values(z_range.min, z_range.max, z_range.tick_step);
    let mut axis_meshes = Vec::new();
    push_axis_arrows(&mut axis_meshes, center, x_axis, x_range, xy_normal)?;
    push_axis_arrows(&mut axis_meshes, center, y_axis, y_range, xy_normal)?;
    push_axis_arrows(&mut axis_meshes, center, z_axis, z_range, z_normal)?;
    if grid {
        let (small_x_xy, large_x_xy) =
            axis_grid_lins(&x_ticks, center, x_axis, y_axis, y_range, xy_normal);
        let (small_y_xy, large_y_xy) =
            axis_grid_lins(&y_ticks, center, y_axis, x_axis, x_range, xy_normal);
        let (small_x_xz, large_x_xz) =
            axis_grid_lins(&x_ticks, center, x_axis, z_axis, z_range, xz_normal);
        let (small_z_xz, large_z_xz) =
            axis_grid_lins(&z_ticks, center, z_axis, x_axis, x_range, xz_normal);
        let (small_y_yz, large_y_yz) =
            axis_grid_lins(&y_ticks, center, y_axis, z_axis, z_range, yz_normal);
        let (small_z_yz, large_z_yz) =
            axis_grid_lins(&z_ticks, center, z_axis, y_axis, y_range, yz_normal);
        push_styled_line_meshes(&mut axis_meshes, small_x_xy, large_x_xy, true);
        push_styled_line_meshes(&mut axis_meshes, small_y_xy, large_y_xy, true);
        push_styled_line_meshes(&mut axis_meshes, small_x_xz, large_x_xz, true);
        push_styled_line_meshes(&mut axis_meshes, small_z_xz, large_z_xz, true);
        push_styled_line_meshes(&mut axis_meshes, small_y_yz, large_y_yz, true);
        push_styled_line_meshes(&mut axis_meshes, small_z_yz, large_z_yz, true);
    } else {
        let (small_x, large_x) = axis_tick_lins(&x_ticks, center, x_axis, y_dir, xy_normal);
        let (small_y, large_y) = axis_tick_lins(&y_ticks, center, y_axis, x_dir, xy_normal);
        let (small_z, large_z) = axis_tick_lins(&z_ticks, center, z_axis, x_dir, z_normal);
        push_styled_line_meshes(&mut axis_meshes, small_x, large_x, false);
        push_styled_line_meshes(&mut axis_meshes, small_y, large_y, false);
        push_styled_line_meshes(&mut axis_meshes, small_z, large_z, false);
    }

    let mut labels = Vec::new();
    push_axis_tick_labels(
        executor,
        &mut labels,
        &x_ticks,
        center,
        x_axis,
        -y_dir,
        x_dir,
        xy_normal,
        x_label_rate,
        true,
        AXIS_ZERO_TICK_LABEL_OFFSET,
    )?;
    push_axis_tick_labels(
        executor,
        &mut labels,
        &y_ticks,
        center,
        y_axis,
        -x_dir,
        x_dir,
        xy_normal,
        y_label_rate,
        false,
        0.0,
    )?;
    push_axis_tick_labels(
        executor,
        &mut labels,
        &z_ticks,
        center,
        z_axis,
        -x_dir,
        x_dir,
        z_normal,
        z_label_rate,
        false,
        0.0,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        x_label,
        axis_title_anchor(center, x_axis, x_range.max),
        x_dir,
        x_dir,
        xy_normal,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        y_label,
        axis_title_anchor(center, y_axis, y_range.max),
        y_dir,
        x_dir,
        xy_normal,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        z_label,
        axis_title_anchor(center, z_axis, z_range.max),
        z_dir,
        x_dir,
        z_normal,
    )?;
    Ok(axis_result(axis_meshes, labels))
}

#[stdlib_func]
pub async fn mk_polar_axis(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -9, "center")?;
    let theta_min = crate::read_float(executor, stack_idx, -8, "theta_min")? as f32;
    let theta_max = crate::read_float(executor, stack_idx, -7, "theta_max")? as f32;
    let theta_step = crate::read_float(executor, stack_idx, -6, "theta_step")?
        .abs()
        .max(1e-3) as f32;
    let radius_min = crate::read_float(executor, stack_idx, -4, "radius_min")?.max(0.0) as f32;
    let radius_max =
        crate::read_float(executor, stack_idx, -3, "radius_max")?.max(radius_min as f64) as f32;
    let radius_step = crate::read_float(executor, stack_idx, -2, "radius_step")?
        .abs()
        .max(1e-3) as f32;
    ensure_limit(
        "polar axis rings",
        tick_count(radius_min.max(radius_step), radius_max, radius_step),
        MAX_AXIS_TICKS,
    )?;
    ensure_limit(
        "polar axis rays",
        tick_count(theta_min, theta_max, theta_step),
        MAX_AXIS_TICKS,
    )?;
    let (x, y, normal) = polygon_basis(Float3::Z);
    let mut lins = Vec::new();

    let mut r = radius_min.max(radius_step);
    while r <= radius_max + 1e-4 {
        let samples = 64usize;
        let points: Vec<_> = (0..samples)
            .map(|i| {
                let theta = std::f32::consts::TAU * i as f32 / samples as f32;
                center + x * (r * theta.cos()) + y * (r * theta.sin())
            })
            .collect();
        push_closed_polyline(&mut lins, &points, normal);
        r += radius_step;
    }

    let mut theta = theta_min;
    while theta <= theta_max + 1e-4 {
        let end = center + x * (radius_max * theta.cos()) + y * (radius_max * theta.sin());
        lins.push(default_lin(
            center + x * (radius_min * theta.cos()) + y * (radius_min * theta.sin()),
            end,
            normal,
        ));
        theta += theta_step;
    }
    Ok(mesh_from_parts(vec![], lins, vec![]))
}

#[stdlib_func]
pub async fn mk_parametric(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-4)
        .clone()
        .elide_lvalue();
    let t0 = crate::read_float(executor, stack_idx, -3, "t0")?;
    let t1 = crate::read_float(executor, stack_idx, -2, "t1")?;
    let samples = read_int(executor, stack_idx, -1, "samples")?.max(2) as usize;
    ensure_limit("parametric samples", samples, MAX_CURVE_SAMPLES)?;
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(samples);
    for i in 0..samples {
        let t = if samples == 1 {
            t0
        } else {
            t0 + (t1 - t0) * i as f64 / (samples - 1) as f64
        };
        args.push(smallvec![Value::Float(t)]);
    }
    let values = invoke_callable_many(executor, &f, &args, "f").await?;
    let points = values
        .into_iter()
        .map(|value| float3_from_value(value, "f"))
        .collect::<Result<Vec<_>, _>>()?;
    let normal = points
        .windows(3)
        .find_map(|w| {
            let cross = (w[1] - w[0]).cross(w[2] - w[1]);
            (cross.len_sq() > 1e-6).then(|| cross.normalize())
        })
        .unwrap_or(Float3::Z);
    Ok(mesh_from_parts(
        vec![],
        open_polyline(&points, normal),
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_explicit(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-4)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -3, "x0")?;
    let x1 = crate::read_float(executor, stack_idx, -2, "x1")?;
    let samples = read_int(executor, stack_idx, -1, "samples")?.max(2) as usize;
    ensure_limit("explicit samples", samples, MAX_CURVE_SAMPLES)?;
    let mut xs = Vec::with_capacity(samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(samples);
    for i in 0..samples {
        let x = x0 + (x1 - x0) * i as f64 / (samples - 1) as f64;
        xs.push(x);
        args.push(smallvec![Value::Float(x)]);
    }
    let values = invoke_callable_many(executor, &f, &args, "f").await?;
    let mut points = Vec::with_capacity(samples);
    for (x, value) in xs.into_iter().zip(values) {
        let y = match value {
            Value::Float(v) => v as f32,
            Value::Integer(v) => v as f32,
            other => {
                return Err(ExecutorError::type_error_for(
                    "float",
                    other.type_name(),
                    "f",
                ));
            }
        };
        points.push(Float3::new(x as f32, y, 0.0));
    }
    Ok(mesh_from_parts(
        vec![],
        open_polyline(&points, Float3::Z),
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_explicit2d(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-7)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -6, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -5, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -4, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -3, "y1")? as f32;
    let x_samples = grid_axis_samples(read_int(executor, stack_idx, -2, "x_samples")?);
    let y_samples = grid_axis_samples(read_int(executor, stack_idx, -1, "y_samples")?);
    let nx = x_samples - 1;
    let ny = y_samples - 1;
    let cell_count = ensure_grid_cells("explicit surface cells", nx, ny)?;
    ensure_surface_triangles("explicit surface triangles", cell_count.saturating_mul(2))?;
    let mut grid = vec![vec![Float3::ZERO; y_samples]; x_samples];
    let mut coords = Vec::with_capacity(x_samples * y_samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(x_samples * y_samples);
    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            coords.push((ix, iy, x, y));
            args.push(smallvec![Value::Float(x as f64), Value::Float(y as f64)]);
        }
    }
    let values = invoke_callable_many(executor, &f, &args, "f").await?;
    for ((ix, iy, x, y), value) in coords.into_iter().zip(values) {
        let z = match value {
            Value::Float(v) => v as f32,
            Value::Integer(v) => v as f32,
            other => {
                return Err(ExecutorError::type_error_for(
                    "float",
                    other.type_name(),
                    "f",
                ));
            }
        };
        grid[ix][iy] = Float3::new(x, y, z);
    }
    let index = |ix: usize, iy: usize| ix * (ny + 1) + iy;
    let vertices: Vec<_> = grid.iter().flat_map(|col| col.iter().copied()).collect();
    let mut faces = Vec::with_capacity(nx * ny * 2);
    for ix in 0..nx {
        for iy in 0..ny {
            faces.push([index(ix, iy), index(ix + 1, iy), index(ix + 1, iy + 1)]);
            faces.push([index(ix, iy), index(ix + 1, iy + 1), index(ix, iy + 1)]);
        }
    }
    let surface_vertices: Vec<_> = vertices
        .into_iter()
        .map(|pos| SurfaceVertex {
            pos,
            col: Float4::new(0.0, 0.0, 0.0, 1.0),
            uv: Float2::ZERO,
        })
        .collect();
    let (lins, tris) = build_indexed_surface(&surface_vertices, &faces, &HashMap::new());
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_implicit2d(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-7)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -6, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -5, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -4, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -3, "y1")? as f32;
    let x_samples = grid_axis_samples(read_int(executor, stack_idx, -2, "x_samples")?);
    let y_samples = grid_axis_samples(read_int(executor, stack_idx, -1, "y_samples")?);
    let nx = x_samples - 1;
    let ny = y_samples - 1;
    ensure_grid_cells("implicit surface cells", nx, ny)?;
    let mut vals = vec![vec![0.0f32; y_samples]; x_samples];
    let mut coords = Vec::with_capacity(x_samples * y_samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(x_samples * y_samples);
    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            coords.push((ix, iy));
            args.push(smallvec![Value::Float(x as f64), Value::Float(y as f64)]);
        }
    }
    let values = invoke_callable_many(executor, &f, &args, "f").await?;
    for ((ix, iy), value) in coords.into_iter().zip(values) {
        vals[ix][iy] = match value {
            Value::Float(v) => v as f32,
            Value::Integer(v) => v as f32,
            other => {
                return Err(ExecutorError::type_error_for(
                    "float",
                    other.type_name(),
                    "f",
                ));
            }
        };
    }

    let mut lins = Vec::new();
    for ix in 0..nx {
        for iy in 0..ny {
            let xa = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let xb = x0 + (x1 - x0) * (ix + 1) as f32 / nx as f32;
            let ya = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let yb = y0 + (y1 - y0) * (iy + 1) as f32 / ny as f32;
            let corners = [
                (Float3::new(xa, ya, 0.0), vals[ix][iy]),
                (Float3::new(xb, ya, 0.0), vals[ix + 1][iy]),
                (Float3::new(xb, yb, 0.0), vals[ix + 1][iy + 1]),
                (Float3::new(xa, yb, 0.0), vals[ix][iy + 1]),
            ];
            let mut hits = Vec::new();
            for edge in [(0usize, 1usize), (1, 2), (2, 3), (3, 0)] {
                let (pa, va) = corners[edge.0];
                let (pb, vb) = corners[edge.1];
                if (va <= 0.0 && vb >= 0.0) || (va >= 0.0 && vb <= 0.0) {
                    let denom = (vb - va).abs().max(1e-6);
                    let t = (-va / denom).clamp(0.0, 1.0);
                    hits.push(pa.lerp(pb, t));
                }
            }
            if hits.len() >= 2 {
                lins.push(default_lin(hits[0], hits[1], Float3::Z));
            }
            if hits.len() >= 4 {
                lins.push(default_lin(hits[2], hits[3], Float3::Z));
            }
        }
    }
    Ok(mesh_from_parts(vec![], lins, vec![]))
}

#[stdlib_func]
pub async fn mk_explicit_diff(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-9)
        .clone()
        .elide_lvalue();
    let g = executor
        .state
        .stack(stack_idx)
        .read_at(-8)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -7, "x0")?;
    let x1 = crate::read_float(executor, stack_idx, -6, "x1")?;
    let samples = read_int(executor, stack_idx, -5, "samples")?.max(2) as usize;
    ensure_limit("explicit diff samples", samples, MAX_CURVE_SAMPLES)?;
    ensure_surface_triangles(
        "explicit diff triangles",
        samples.saturating_sub(1).saturating_mul(2),
    )?;
    let fill0 = read_float4(executor, stack_idx, -4, "fill0").await?;
    let fill1 = read_float4(executor, stack_idx, -3, "fill1").await?;
    let tag0 = read_tags(executor, stack_idx, -2, "tag0")?;
    let tag1 = read_tags(executor, stack_idx, -1, "tag1")?;

    let mut upper = Vec::with_capacity(samples);
    let mut lower = Vec::with_capacity(samples);
    let mut xs = Vec::with_capacity(samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(samples);
    for i in 0..samples {
        let x = x0 + (x1 - x0) * i as f64 / (samples - 1) as f64;
        xs.push(x);
        args.push(smallvec![Value::Float(x)]);
    }
    let upper_values = invoke_callable_many(executor, &f, &args, "f").await?;
    let lower_values = invoke_callable_many(executor, &g, &args, "g").await?;
    for ((x, upper_value), lower_value) in xs.into_iter().zip(upper_values).zip(lower_values) {
        let yf = match upper_value {
            Value::Float(v) => v as f32,
            Value::Integer(v) => v as f32,
            other => {
                return Err(ExecutorError::type_error_for(
                    "float",
                    other.type_name(),
                    "f",
                ));
            }
        };
        let yg = match lower_value {
            Value::Float(v) => v as f32,
            Value::Integer(v) => v as f32,
            other => {
                return Err(ExecutorError::type_error_for(
                    "float",
                    other.type_name(),
                    "g",
                ));
            }
        };
        upper.push(Float3::new(x as f32, yf, 0.0));
        lower.push(Float3::new(x as f32, yg, 0.0));
    }

    // split contiguous same-sign columns into shared strips so interior columns
    // do not leave stroked seams between identical regions
    let mut pos_verts: Vec<Float3> = Vec::new();
    let mut pos_faces: Vec<[usize; 3]> = Vec::new();
    let mut neg_verts: Vec<Float3> = Vec::new();
    let mut neg_faces: Vec<[usize; 3]> = Vec::new();

    let interval_is_pos =
        |i: usize| (upper[i].y + upper[i + 1].y) * 0.5 >= (lower[i].y + lower[i + 1].y) * 0.5;
    let append_strip =
        |verts: &mut Vec<Float3>, faces: &mut Vec<[usize; 3]>, start: usize, end: usize| {
            let base = verts.len();
            for idx in start..=end {
                verts.push(lower[idx]);
                verts.push(upper[idx]);
            }
            for idx in 0..end - start {
                let col = base + idx * 2;
                faces.push([col, col + 1, col + 3]);
                faces.push([col, col + 3, col + 2]);
            }
        };

    let mut run_start = 0usize;
    let mut is_pos = interval_is_pos(0);
    for i in 1..samples - 1 {
        let next_is_pos = interval_is_pos(i);
        if next_is_pos != is_pos {
            if is_pos {
                append_strip(&mut pos_verts, &mut pos_faces, run_start, i);
            } else {
                append_strip(&mut neg_verts, &mut neg_faces, run_start, i);
            }
            run_start = i;
            is_pos = next_is_pos;
        }
    }
    if is_pos {
        append_strip(&mut pos_verts, &mut pos_faces, run_start, samples - 1);
    } else {
        append_strip(&mut neg_verts, &mut neg_faces, run_start, samples - 1);
    }

    let build_region = |verts: Vec<Float3>, faces: Vec<[usize; 3]>, fill: Float4| {
        let vertices: Vec<_> = verts
            .into_iter()
            .map(|pos| SurfaceVertex {
                pos,
                col: fill,
                uv: Float2::ZERO,
            })
            .collect();
        build_indexed_surface(&vertices, &faces, &HashMap::new())
    };

    let (pos_lins, pos_tris) = build_region(pos_verts, pos_faces, fill0);
    let (neg_lins, neg_tris) = build_region(neg_verts, neg_faces, fill1);

    let make_tagged_mesh = |lins, tris, tag: Vec<isize>| {
        let mesh = geo::mesh::Mesh {
            dots: vec![],
            lins,
            tris,
            uniform: geo::mesh::Uniforms::default(),
            tag,
        };
        mesh.debug_assert_consistent_topology();
        Value::Mesh(std::sync::Arc::new(mesh))
    };

    let pos_val = make_tagged_mesh(pos_lins, pos_tris, tag0);
    let neg_val = make_tagged_mesh(neg_lins, neg_tris, tag1);

    let mut lins = open_polyline(&upper, Float3::Z);
    push_open_polyline(&mut lins, &lower, Float3::Z);
    let outline_val = mesh_from_parts(vec![], lins, vec![]);

    Ok(list_value([pos_val, neg_val, outline_val]))
}
