use std::collections::HashMap;

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh_build::SurfaceVertex,
    simd::{Float2, Float3, Float4},
};
use stdlib_macros::stdlib_func;

use super::helpers::*;

const MAX_POLYGON_POINTS: usize = 1 << 13;
const MAX_CURVE_SAMPLES: usize = 1 << 14;
const MAX_AXIS_TICKS: usize = 1 << 12;
const MAX_GRID_CELLS: usize = 1 << 16;
const MAX_FIELD_SAMPLES: usize = 1 << 16;
const MAX_SURFACE_TRIANGLES: usize = 1 << 17;
const DEFAULT_ARROW_PATH_SAMPLES: usize = 64;
const MAX_ARROW_HEAD_RADIUS: f32 = 0.065;
const ARROW_HEAD_RADIUS_OVER_LENGTH: f32 = 0.4;
const ARROW_STEM_RADIUS_OVER_HEAD_RADIUS: f32 = 0.33;
const ARROW_HEAD_WIDTH_OVER_RADIUS: f32 = 1.2;
const ARROW_HEAD_DEPTH_OVER_RADIUS: f32 = 2.1;
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

fn tick_count(min: f32, max: f32, step: f32) -> usize {
    if max < min {
        0
    } else {
        (((max - min) / step).ceil() as usize).saturating_add(1)
    }
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

fn closed_polyline(points: &[Float3], normal: Float3) -> Vec<geo::mesh::Lin> {
    let mut out = Vec::with_capacity(points.len());
    push_closed_polyline(&mut out, points, normal);
    out
}

fn open_polyline(points: &[Float3], normal: Float3) -> Vec<geo::mesh::Lin> {
    let mut out = Vec::with_capacity(points.len().saturating_sub(1));
    push_open_polyline(&mut out, points, normal);
    out
}

fn mesh_ref(idx: usize) -> i32 {
    super::helpers::mesh_ref(idx)
}

fn latex_meshes_to_value(meshes: Vec<std::sync::Arc<geo::mesh::Mesh>>) -> Value {
    list_value(meshes.into_iter().map(Value::Mesh))
}

fn read_text_scale(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<f32, ExecutorError> {
    let scale = crate::read_float(executor, stack_idx, index, name)? as f32;
    if !scale.is_finite() || scale <= 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "must be a positive finite number",
        });
    }
    Ok(scale)
}

fn read_nonnegative_float(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<f32, ExecutorError> {
    let value = crate::read_float(executor, stack_idx, index, name)? as f32;
    if !value.is_finite() || value < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "must be a non-negative finite number",
        });
    }
    Ok(value)
}

fn normalize_or(vec: Float3, fallback: Float3) -> Float3 {
    if vec.len_sq() > 1e-8 {
        vec.normalize()
    } else {
        fallback
    }
}

fn resolve_arrow_plane_normal(tangent: Float3, normal: Float3) -> Float3 {
    let normal = normalize_or(normal, Float3::Z);
    if normal.cross(tangent).len_sq() > 1e-8 {
        return normal;
    }

    let helper = if tangent.z.abs() < 0.9 {
        Float3::Z
    } else {
        Float3::X
    };
    normalize_or(helper.cross(tangent), Float3::Y)
}

fn path_arc_point(start: Float3, t: f32, end: Float3, path_arc: Float3) -> Float3 {
    if path_arc.len_sq() <= 1e-12 {
        return start.lerp(end, t);
    }

    let delta = end - start;
    if delta.len_sq() <= 1e-12 {
        return start.lerp(end, t);
    }

    let cross = path_arc.cross(delta);
    let cross_len = cross.len();
    if cross_len <= 1e-6 {
        return start.lerp(end, t);
    }

    let alpha = path_arc.len();
    let tan_half = (alpha / 2.0).tan();
    if !alpha.is_finite() || alpha.abs() <= 1e-6 || !tan_half.is_finite() || tan_half.abs() <= 1e-6
    {
        return start.lerp(end, t);
    }

    let pivot = (start + end) / 2.0 + cross * (delta.len() / (2.0 * tan_half * cross_len));
    let radius_vec = start - pivot;
    let radius = radius_vec.len();
    if radius <= 1e-6 {
        return start.lerp(end, t);
    }

    let a_prime = radius_vec / radius;
    let a_prime_norm = path_arc.cross(a_prime);
    let a_prime_norm_len = a_prime_norm.len();
    if a_prime_norm_len <= 1e-6 {
        return start.lerp(end, t);
    }

    let theta = t * alpha;
    pivot
        + a_prime * (theta.cos() * radius)
        + (a_prime_norm / a_prime_norm_len) * (theta.sin() * radius)
}

fn orient_contour_to_normal(contour: &mut [Float3], normal: Float3) {
    let mut area_normal = Float3::ZERO;
    for i in 0..contour.len() {
        area_normal = area_normal + contour[i].cross(contour[(i + 1) % contour.len()]);
    }
    if area_normal.dot(normal) < 0.0 {
        contour.reverse();
    }
}

fn vector_like_mesh(
    tail: Float3,
    delta: Float3,
    normal: Float3,
    path_arc: f64,
) -> Result<Value, ExecutorError> {
    let len = delta.len();
    if len <= 1e-6 {
        return Ok(mesh_from_parts(
            vec![default_dot(tail, normalize_or(normal, Float3::Z))],
            vec![],
            vec![],
        ));
    }

    let tangent = delta / len;
    let normal = resolve_arrow_plane_normal(tangent, normal);
    let path_arc = path_arc as f32;
    let alpha = path_arc.abs();
    let samples = if alpha <= 1e-6 {
        2
    } else {
        DEFAULT_ARROW_PATH_SAMPLES
    };
    ensure_limit("arrow samples", samples, MAX_CURVE_SAMPLES)?;

    let head_radius = (len * ARROW_HEAD_RADIUS_OVER_LENGTH).min(MAX_ARROW_HEAD_RADIUS);
    let stem_radius = head_radius * ARROW_STEM_RADIUS_OVER_HEAD_RADIUS;
    let head_half_width = head_radius * ARROW_HEAD_WIDTH_OVER_RADIUS;
    let head_depth = head_radius * ARROW_HEAD_DEPTH_OVER_RADIUS;
    let sinc = if alpha <= 1e-6 {
        1.0
    } else {
        (alpha / 2.0).sin() / (alpha / 2.0)
    };
    let modded_length = if sinc.abs() <= 1e-6 { len } else { len / sinc };
    let shaft_end = if modded_length <= 1e-6 {
        0.0
    } else {
        ((modded_length - head_depth).max(0.0) / modded_length).clamp(0.0, 1.0)
    };

    let start = tail;
    let end = tail + delta;
    let path_arc_vec = normal * path_arc;
    let mut centers = Vec::with_capacity(samples);
    for i in 0..samples {
        let t = i as f32 / (samples - 1) as f32;
        centers.push(path_arc_point(start, t * shaft_end, end, path_arc_vec));
    }

    let mut offsets = Vec::with_capacity(samples);
    for i in 0..samples {
        let prev = centers[i.saturating_sub(1)];
        let next = centers[(i + 1).min(samples - 1)];
        let seg = normalize_or(next - prev, tangent);
        offsets.push(normal.cross(seg).normalize() * stem_radius);
    }

    let tip_dir = normalize_or(end - centers[samples - 1], tangent);
    let side_dir = normalize_or(offsets[samples - 1], normal.cross(tip_dir));
    let mut contour = Vec::with_capacity(samples * 2 + 3);
    for (center, offset) in centers.iter().zip(offsets.iter()) {
        contour.push(*center + *offset);
    }
    contour.push(centers[samples - 1] + side_dir * head_half_width);
    contour.push(end);
    contour.push(centers[samples - 1] - side_dir * head_half_width);
    for (center, offset) in centers.iter().zip(offsets.iter()).rev() {
        contour.push(*center - *offset);
    }
    orient_contour_to_normal(&mut contour, normal);

    let (lins, tris) = tessellate_planar_loops(&[contour], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[cfg(test)]
fn fan_tris(points: &[Float3], boundary_lins: &mut [geo::mesh::Lin]) -> Vec<geo::mesh::Tri> {
    let mut out = Vec::new();
    if points.len() >= 3 {
        for i in 1..points.len() - 1 {
            let mut tri = default_tri(points[0], points[i], points[i + 1]);
            tri.ab = if i == 1 { mesh_ref(0) } else { (i - 2) as i32 };
            tri.bc = mesh_ref(i);
            tri.ca = if i + 1 == points.len() - 1 {
                mesh_ref(points.len() - 1)
            } else {
                i as i32
            };
            out.push(tri);
        }

        if !boundary_lins.is_empty() {
            boundary_lins[0].inv = mesh_ref(0);
            for i in 1..points.len() - 1 {
                boundary_lins[i].inv = mesh_ref(i - 1);
            }
            boundary_lins[points.len() - 1].inv = mesh_ref(points.len() - 3);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use executor::value::Value;
    use geo::simd::Float3;

    use super::{closed_polyline, fan_tris, mesh_ref, open_polyline, vector_like_mesh};

    #[test]
    fn closed_polyline_sets_reciprocal_links() {
        let lines = closed_polyline(&[Float3::X, Float3::Y, Float3::Z, Float3::ZERO], Float3::Z);
        for (i, line) in lines.iter().enumerate() {
            assert_eq!(lines[line.prev as usize].next, i as i32);
            assert_eq!(lines[line.next as usize].prev, i as i32);
        }
    }

    #[test]
    fn open_polyline_sets_endpoints_to_negative_one() {
        let lines = open_polyline(&[Float3::X, Float3::Y, Float3::Z], Float3::Z);
        assert_eq!(lines[0].prev, -1);
        assert_eq!(lines[0].next, 1);
        assert_eq!(lines[1].prev, 0);
        assert_eq!(lines[1].next, -1);
    }

    #[test]
    fn fan_tris_sets_boundary_and_neighbor_refs() {
        let points = [Float3::ZERO, Float3::X, Float3::Y, Float3::Z];
        let mut lines = closed_polyline(&points, Float3::Z);
        let tris = fan_tris(&points, &mut lines);

        assert_eq!(tris.len(), 2);
        assert_eq!(tris[0].ab, mesh_ref(0));
        assert_eq!(tris[0].bc, mesh_ref(1));
        assert_eq!(tris[0].ca, 1);
        assert_eq!(tris[1].ab, 0);
        assert_eq!(tris[1].bc, mesh_ref(2));
        assert_eq!(tris[1].ca, mesh_ref(3));
        assert_eq!(lines[0].inv, mesh_ref(0));
        assert_eq!(lines[1].inv, mesh_ref(0));
        assert_eq!(lines[2].inv, mesh_ref(1));
        assert_eq!(lines[3].inv, mesh_ref(1));
    }

    #[test]
    fn vector_like_mesh_builds_connected_arrow_surface() {
        let Value::Mesh(mesh) =
            vector_like_mesh(Float3::ZERO, Float3::new(1.0, 0.0, 0.0), Float3::Z, 0.0).unwrap()
        else {
            panic!("expected mesh");
        };

        assert!(mesh.has_consistent_topology());
        assert_eq!(mesh.dots.len(), 0);
        assert!(mesh.lins.len() >= 6);
        assert!(mesh.tris.len() >= 4);
    }

    #[test]
    fn vector_like_mesh_supports_curved_arrow_paths() {
        let Value::Mesh(mesh) =
            vector_like_mesh(Float3::ZERO, Float3::new(1.0, 0.0, 0.0), Float3::Z, 0.8).unwrap()
        else {
            panic!("expected mesh");
        };

        assert!(mesh.has_consistent_topology());
        assert!(mesh.lins.len() > 16);
        assert!(mesh.tris.len() > 16);
    }
}

#[stdlib_func]
pub async fn mk_dot(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let point = read_float3(executor, stack_idx, -1, "point")?;
    Ok(mesh_from_parts(
        vec![default_dot(point, Float3::Z)],
        vec![],
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_circle(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -3, "center")?;
    let radius = crate::read_float(executor, stack_idx, -2, "radius")? as f32;
    let samples = read_int(executor, stack_idx, -1, "samples")?.max(3) as usize;
    ensure_limit("circle samples", samples, MAX_POLYGON_POINTS)?;
    let (x, y, normal) = polygon_basis(Float3::Z);
    let points: Vec<_> = (0..samples)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / samples as f32;
            center + x * (radius * theta.cos()) + y * (radius * theta.sin())
        })
        .collect();
    let (lins, tris) = tessellate_planar_loops(&[points], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_annulus(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -3, "center")?;
    let inner = crate::read_float(executor, stack_idx, -2, "inner")? as f32;
    let outer = crate::read_float(executor, stack_idx, -1, "outer")? as f32;
    let samples = 64usize;
    let (x, y, normal) = polygon_basis(Float3::Z);
    let inner_pts: Vec<_> = (0..samples)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / samples as f32;
            center + x * (inner * theta.cos()) + y * (inner * theta.sin())
        })
        .collect();
    let outer_pts: Vec<_> = (0..samples)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / samples as f32;
            center + x * (outer * theta.cos()) + y * (outer * theta.sin())
        })
        .collect();
    let mut inner_pts = inner_pts;
    inner_pts.reverse();
    let (lins, tris) = tessellate_planar_loops(&[outer_pts, inner_pts], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_square(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -2, "center")?;
    let width = crate::read_float(executor, stack_idx, -1, "width")? as f32;
    let normal = Float3::Z;
    let half = width / 2.0;
    let (x, y, _) = polygon_basis(normal);
    let corners = vec![
        center - x * half - y * half,
        center + x * half - y * half,
        center + x * half + y * half,
        center - x * half + y * half,
    ];
    let (lins, tris) = tessellate_planar_loops(&[corners], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_rect(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -3, "center")?;
    let width = crate::read_float(executor, stack_idx, -2, "width")? as f32;
    let height = crate::read_float(executor, stack_idx, -1, "height")? as f32;
    let normal = Float3::Z;
    let (x, y, _) = polygon_basis(normal);
    let corners = vec![
        center - x * (width / 2.0) - y * (height / 2.0),
        center + x * (width / 2.0) - y * (height / 2.0),
        center + x * (width / 2.0) + y * (height / 2.0),
        center - x * (width / 2.0) + y * (height / 2.0),
    ];
    let (lins, tris) = tessellate_planar_loops(&[corners], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_regular_polygon(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -3, "center")?;
    let n = read_int(executor, stack_idx, -2, "n")?.max(3) as usize;
    ensure_limit("regular polygon sides", n, MAX_POLYGON_POINTS)?;
    let radius = crate::read_float(executor, stack_idx, -1, "circumradius")? as f32;
    let (x, y, normal) = polygon_basis(Float3::Z);
    let points: Vec<_> = (0..n)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / n as f32;
            center + x * (radius * theta.cos()) + y * (radius * theta.sin())
        })
        .collect();
    let (lins, tris) = tessellate_planar_loops(&[points], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_polygon(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let vertices = read_float3_list(executor, stack_idx, -2, "vertices")?;
    ensure_limit("polygon vertices", vertices.len(), MAX_POLYGON_POINTS)?;
    let normal = read_float3(executor, stack_idx, -1, "normal_hint")?;
    let (lins, tris) = tessellate_planar_loops(&[vertices], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_polyline(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let vertices = read_float3_list(executor, stack_idx, -2, "vertices")?;
    ensure_limit("polyline vertices", vertices.len(), MAX_CURVE_SAMPLES)?;
    let normal = read_float3(executor, stack_idx, -1, "normal_hint")?;
    Ok(mesh_from_parts(
        vec![],
        open_polyline(&vertices, normal),
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_line(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let start = read_float3(executor, stack_idx, -3, "start")?;
    let end = read_float3(executor, stack_idx, -2, "end")?;
    let normal = read_float3(executor, stack_idx, -1, "normal")?;
    Ok(mesh_from_parts(
        vec![],
        vec![default_lin(start, end, normal)],
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_arrow(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let start = read_float3(executor, stack_idx, -4, "start")?;
    let end = read_float3(executor, stack_idx, -3, "end")?;
    let normal = read_float3(executor, stack_idx, -2, "normal")?;
    let path_arc = crate::read_float(executor, stack_idx, -1, "path_arc")?;
    vector_like_mesh(start, end - start, normal, path_arc)
}

#[stdlib_func]
pub async fn mk_arc(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -4, "center")?;
    let radius = crate::read_float(executor, stack_idx, -3, "radius")? as f32;
    let theta0 = crate::read_float(executor, stack_idx, -2, "theta0")? as f32;
    let theta1 = crate::read_float(executor, stack_idx, -1, "theta1")? as f32;
    let (x, y, normal) = polygon_basis(Float3::Z);
    let steps = (((theta1 - theta0).abs() / std::f32::consts::TAU) * 64.0).ceil() as usize + 2;
    ensure_limit("arc samples", steps, MAX_CURVE_SAMPLES)?;
    let points: Vec<_> = (0..steps)
        .map(|i| {
            let t = i as f32 / (steps - 1) as f32;
            let theta = theta0 + (theta1 - theta0) * t;
            center + x * (radius * theta.cos()) + y * (radius * theta.sin())
        })
        .collect();
    Ok(mesh_from_parts(
        vec![],
        open_polyline(&points, normal),
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_capsule(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let start_center = read_float3(executor, stack_idx, -5, "start_center")?;
    let end_center = read_float3(executor, stack_idx, -4, "end_center")?;
    let start_radius = crate::read_float(executor, stack_idx, -3, "start_radius")? as f32;
    let end_radius = crate::read_float(executor, stack_idx, -2, "end_radius")? as f32;
    let normal = read_float3(executor, stack_idx, -1, "normal")?;
    let axis_delta = end_center - start_center;
    let axis = if axis_delta.len_sq() > 1e-8 {
        axis_delta.normalize()
    } else if normal.len_sq() > 1e-8 {
        let normal = normal.normalize();
        let alt = if normal.x.abs() < 0.9 {
            Float3::X
        } else {
            Float3::Y
        };
        alt.cross(normal).normalize()
    } else {
        Float3::X
    };
    let side_raw = normal.cross(axis);
    let side = if side_raw.len_sq() > 1e-8 {
        side_raw.normalize()
    } else {
        // normal ∥ axis; pick any perpendicular
        let alt = if axis.x.abs() < 0.9 {
            Float3::X
        } else {
            Float3::Y
        };
        alt.cross(axis).normalize()
    };
    let steps = 16usize;
    let mut points = Vec::with_capacity((steps + 1) * 2);
    for i in 0..=steps {
        let theta = std::f32::consts::PI * (0.5 + i as f32 / steps as f32);
        points.push(
            start_center
                + axis * (start_radius * theta.cos())
                + side * (start_radius * theta.sin()),
        );
    }
    for i in 0..=steps {
        let theta = std::f32::consts::PI * (1.5 + i as f32 / steps as f32);
        points.push(
            end_center + axis * (end_radius * theta.cos()) + side * (end_radius * theta.sin()),
        );
    }
    let (lins, tris) = tessellate_planar_loops(&[points], normal)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_triangle(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let p = read_float3(executor, stack_idx, -4, "p")?;
    let q = read_float3(executor, stack_idx, -3, "q")?;
    let r = read_float3(executor, stack_idx, -2, "r")?;
    let normal = read_float3(executor, stack_idx, -1, "normal_hint")?;
    let mut lins = vec![
        default_lin(p, q, normal),
        default_lin(q, r, normal),
        default_lin(r, p, normal),
    ];
    let mut tri = default_tri(p, q, r);
    tri.ab = mesh_ref(0);
    tri.bc = mesh_ref(1);
    tri.ca = mesh_ref(2);
    for lin in &mut lins {
        lin.inv = mesh_ref(0);
    }
    Ok(mesh_from_parts(vec![], lins, vec![tri]))
}

#[stdlib_func]
pub async fn mk_sphere(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -3, "center")?;
    let radius = crate::read_float(executor, stack_idx, -2, "radius")? as f32;
    let depth = read_int(executor, stack_idx, -1, "sample_depth")?.max(0) as usize;
    let lat_steps = (8usize << depth).max(8);
    let lon_steps = (lat_steps * 2).max(16);
    let sphere_tris = 2 * lon_steps * lat_steps.saturating_sub(1);
    ensure_surface_triangles("sphere triangles", sphere_tris)?;
    let top = center + Float3::new(0.0, radius, 0.0);
    let bottom = center + Float3::new(0.0, -radius, 0.0);
    let mut vertices = vec![SurfaceVertex {
        pos: top,
        col: Float4::new(0.0, 0.0, 0.0, 1.0),
        uv: Float2::ZERO,
    }];
    for i in 1..lat_steps {
        let phi = std::f32::consts::PI * i as f32 / lat_steps as f32;
        for j in 0..lon_steps {
            let theta = std::f32::consts::TAU * j as f32 / lon_steps as f32;
            vertices.push(SurfaceVertex {
                pos: center
                    + Float3::new(
                        radius * phi.sin() * theta.cos(),
                        radius * phi.cos(),
                        radius * phi.sin() * theta.sin(),
                    ),
                col: Float4::new(0.0, 0.0, 0.0, 1.0),
                uv: Float2::ZERO,
            });
        }
    }
    let bottom_idx = vertices.len();
    vertices.push(SurfaceVertex {
        pos: bottom,
        col: Float4::new(0.0, 0.0, 0.0, 1.0),
        uv: Float2::ZERO,
    });

    let ring = |lat: usize, lon: usize| 1 + (lat - 1) * lon_steps + lon % lon_steps;
    let mut faces = Vec::with_capacity(lat_steps * lon_steps * 2);
    for j in 0..lon_steps {
        faces.push([0, ring(1, j), ring(1, j + 1)]);
    }
    for i in 1..lat_steps - 1 {
        for j in 0..lon_steps {
            let p00 = ring(i, j);
            let p01 = ring(i, j + 1);
            let p10 = ring(i + 1, j);
            let p11 = ring(i + 1, j + 1);
            faces.push([p00, p10, p11]);
            faces.push([p00, p11, p01]);
        }
    }
    for j in 0..lon_steps {
        faces.push([
            ring(lat_steps - 1, j),
            bottom_idx,
            ring(lat_steps - 1, j + 1),
        ]);
    }

    let (lins, tris) = build_indexed_surface(&vertices, &faces, &HashMap::new());
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_rect_prism(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -2, "center")?;
    let dims = read_float3(executor, stack_idx, -1, "dimensions")?;
    let hx = dims.x / 2.0;
    let hy = dims.y / 2.0;
    let hz = dims.z / 2.0;
    let pts = [
        center + Float3::new(-hx, -hy, -hz),
        center + Float3::new(hx, -hy, -hz),
        center + Float3::new(hx, hy, -hz),
        center + Float3::new(-hx, hy, -hz),
        center + Float3::new(-hx, -hy, hz),
        center + Float3::new(hx, -hy, hz),
        center + Float3::new(hx, hy, hz),
        center + Float3::new(-hx, hy, hz),
    ];
    let faces = [
        [0, 1, 2],
        [0, 2, 3],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    let tris = build_indexed_tris(&pts, &faces);
    Ok(mesh_from_parts(vec![], vec![], tris))
}

#[stdlib_func]
pub async fn mk_cylinder(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -5, "center")?;
    let radius = crate::read_float(executor, stack_idx, -4, "radius")? as f32;
    let height = crate::read_float(executor, stack_idx, -3, "height")? as f32;
    let direction = read_float3(executor, stack_idx, -2, "direction")?;
    let samples = read_int(executor, stack_idx, -1, "sample_count")?.max(3) as usize;
    ensure_limit("cylinder samples", samples, MAX_POLYGON_POINTS)?;
    ensure_surface_triangles("cylinder triangles", samples.saturating_mul(4))?;
    let axis = if direction.len_sq() <= 1e-12 {
        Float3::Y
    } else {
        direction.normalize()
    };
    let (x, z, _) = polygon_basis(axis);
    let half = axis * (height / 2.0);
    let top_center = center + half;
    let bottom_center = center - half;
    let top: Vec<_> = (0..samples)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / samples as f32;
            top_center + x * (radius * theta.cos()) + z * (radius * theta.sin())
        })
        .collect();
    let bottom: Vec<_> = (0..samples)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / samples as f32;
            bottom_center + x * (radius * theta.cos()) + z * (radius * theta.sin())
        })
        .collect();

    let mut vertices = Vec::with_capacity(samples * 2 + 2);
    vertices.extend(bottom.iter().copied());
    vertices.extend(top.iter().copied());
    let top_center_idx = vertices.len();
    vertices.push(top_center);
    let bottom_center_idx = vertices.len();
    vertices.push(bottom_center);

    let mut faces = Vec::with_capacity(samples * 4);
    for i in 0..samples {
        let next = (i + 1) % samples;
        faces.push([i, next, samples + next]);
        faces.push([i, samples + next, samples + i]);
        faces.push([top_center_idx, samples + i, samples + next]);
        faces.push([bottom_center_idx, next, i]);
    }

    let tris = build_indexed_tris(&vertices, &faces);
    Ok(mesh_from_parts(vec![], vec![], tris))
}

#[stdlib_func]
pub async fn mk_cone(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let apex = read_float3(executor, stack_idx, -4, "apex")?;
    let base = read_float3(executor, stack_idx, -3, "base")?;
    let radius = crate::read_float(executor, stack_idx, -2, "radius")? as f32;
    let samples = read_int(executor, stack_idx, -1, "sample_count")?.max(3) as usize;
    ensure_limit("cone samples", samples, MAX_POLYGON_POINTS)?;
    ensure_surface_triangles("cone triangles", samples.saturating_mul(2))?;
    let axis = (apex - base).normalize();
    let (x, y, _) = polygon_basis(axis);
    let ring: Vec<_> = (0..samples)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / samples as f32;
            base + x * (radius * theta.cos()) + y * (radius * theta.sin())
        })
        .collect();

    let mut vertices = ring.clone();
    let apex_idx = vertices.len();
    vertices.push(apex);
    let base_idx = vertices.len();
    vertices.push(base);

    let mut faces = Vec::with_capacity(samples * 2);
    for i in 0..samples {
        let next = (i + 1) % samples;
        faces.push([i, next, apex_idx]);
        faces.push([base_idx, next, i]);
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
pub async fn mk_torus(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let major = crate::read_float(executor, stack_idx, -4, "major_radius")? as f32;
    let minor = crate::read_float(executor, stack_idx, -3, "minor_radius")? as f32;
    let major_samples = read_int(executor, stack_idx, -2, "major_samples")?.max(3) as usize;
    let minor_samples = read_int(executor, stack_idx, -1, "minor_samples")?.max(3) as usize;
    ensure_limit("torus major samples", major_samples, MAX_POLYGON_POINTS)?;
    ensure_limit("torus minor samples", minor_samples, MAX_POLYGON_POINTS)?;
    let torus_quads = checked_product("torus cells", major_samples, minor_samples, MAX_GRID_CELLS)?;
    ensure_surface_triangles("torus triangles", torus_quads.saturating_mul(2))?;
    let (x, y, n) = polygon_basis(Float3::Z);
    let mut vertices = Vec::with_capacity(major_samples * minor_samples);

    let point = |u: usize, v: usize| {
        let theta = std::f32::consts::TAU * u as f32 / major_samples as f32;
        let phi = std::f32::consts::TAU * v as f32 / minor_samples as f32;
        let ring_center = x * (major * theta.cos()) + y * (major * theta.sin());
        let ring_normal = (x * theta.cos() + y * theta.sin()).normalize();
        ring_center + ring_normal * (minor * phi.cos()) + n * (minor * phi.sin())
    };

    for u in 0..major_samples {
        for v in 0..minor_samples {
            vertices.push(point(u, v));
        }
    }

    let index = |u: usize, v: usize| u * minor_samples + v;
    let mut faces = Vec::with_capacity(major_samples * minor_samples * 2);
    for u in 0..major_samples {
        let un = (u + 1) % major_samples;
        for v in 0..minor_samples {
            let vn = (v + 1) % minor_samples;
            faces.push([index(u, v), index(un, v), index(un, vn)]);
            faces.push([index(u, v), index(un, vn), index(u, vn)]);
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
pub async fn mk_plane(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let dist = crate::read_float(executor, stack_idx, -3, "dist")? as f32;
    let width = crate::read_float(executor, stack_idx, -2, "width")? as f32;
    let height = crate::read_float(executor, stack_idx, -1, "height")? as f32;
    let (_, _, n) = polygon_basis(Float3::Z);
    let center = n * dist;
    let (x, y, _) = polygon_basis(n);
    let corners = vec![
        center - x * (width / 2.0) - y * (height / 2.0),
        center + x * (width / 2.0) - y * (height / 2.0),
        center + x * (width / 2.0) + y * (height / 2.0),
        center - x * (width / 2.0) + y * (height / 2.0),
    ];
    let (lins, tris) = tessellate_planar_loops(&[corners], n)?;
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_bezier(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let control_points = read_float3_list(executor, stack_idx, -1, "control_points")?;
    if control_points.len() < 2 {
        return Ok(mesh_from_parts(vec![], vec![], vec![]));
    }
    let samples = 64usize;
    let mut points = Vec::with_capacity(samples);
    for i in 0..samples {
        let t = i as f32 / (samples - 1) as f32;
        let mut layer = control_points.clone();
        while layer.len() > 1 {
            layer = layer.windows(2).map(|w| w[0].lerp(w[1], t)).collect();
        }
        points.push(layer[0]);
    }
    let normal = if control_points.len() >= 3 {
        let a = control_points[1] - control_points[0];
        let b = control_points[2] - control_points[1];
        let cross = a.cross(b);
        if cross.len_sq() > 1e-6 {
            cross.normalize()
        } else {
            Float3::Z
        }
    } else {
        Float3::Z
    };
    Ok(mesh_from_parts(
        vec![],
        open_polyline(&points, normal),
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_vector(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tail = read_float3(executor, stack_idx, -3, "tail")?;
    let delta = read_float3(executor, stack_idx, -2, "delta")?;
    let normal = read_float3(executor, stack_idx, -1, "normal")?;
    vector_like_mesh(tail, delta, normal, 0.0)
}

#[stdlib_func]
pub async fn mk_half_vector(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tail = read_float3(executor, stack_idx, -3, "tail")?;
    let delta = read_float3(executor, stack_idx, -2, "delta")?;
    let normal = read_float3(executor, stack_idx, -1, "normal")?;
    Ok(mesh_from_parts(
        vec![],
        vec![default_lin(tail, tail + delta, normal)],
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_image(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let image = read_string(executor, stack_idx, -4, "name").await?;
    let center = read_float3(executor, stack_idx, -3, "center")?;
    let width = crate::read_float(executor, stack_idx, -2, "width")? as f32;
    let height = crate::read_float(executor, stack_idx, -1, "height")? as f32;
    let normal = Float3::Z;
    let (x, y, _) = polygon_basis(normal);
    let corners = [
        center - x * (width / 2.0) - y * (height / 2.0),
        center + x * (width / 2.0) - y * (height / 2.0),
        center + x * (width / 2.0) + y * (height / 2.0),
        center - x * (width / 2.0) + y * (height / 2.0),
    ];
    let (lins, tris) = tessellate_planar_loops(&[corners.to_vec()], normal)?;
    let mut mesh = match mesh_from_parts(vec![], lins, tris) {
        Value::Mesh(mesh) => (*mesh).clone(),
        _ => unreachable!(),
    };
    set_triangle_uv_rect(&mut mesh, corners[0], corners[2], x, y);
    mesh.uniform.img = Some(image.into());
    Ok(Value::Mesh(std::sync::Arc::new(mesh)))
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
    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let pos = Float3::new(x, y, 0.0);
            let col = float4_from_value(
                invoke_callable(
                    executor,
                    &color_at,
                    vec![point_value(pos), sample_index_value(ix, iy)],
                    "color_at",
                )
                .await?,
                "color_at",
            )?;
            vertices.push(SurfaceVertex {
                pos,
                col,
                uv: Float2::ZERO,
            });
        }
    }
    let grid_vertex = |ix: usize, iy: usize| ix * y_samples + iy;

    for ix in 0..nx {
        for iy in 0..ny {
            let xa = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let xb = x0 + (x1 - x0) * (ix + 1) as f32 / nx as f32;
            let ya = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let yb = y0 + (y1 - y0) * (iy + 1) as f32 / ny as f32;
            let center = Float3::new((xa + xb) / 2.0, (ya + yb) / 2.0, 0.0);
            if !invoke_callable(executor, &mask, vec![point_value(center)], "mask")
                .await?
                .check_truthy()?
            {
                continue;
            }

            let a = grid_vertex(ix, iy);
            let b = grid_vertex(ix + 1, iy);
            let c = grid_vertex(ix + 1, iy + 1);
            let d = grid_vertex(ix, iy + 1);
            faces.push([a, b, c]);
            faces.push([a, c, d]);
        }
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

    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let pos = Float3::new(x, y, 0.0);
            if !invoke_callable(executor, &mask, vec![point_value(pos)], "mask")
                .await?
                .check_truthy()?
            {
                continue;
            }
            out.push(
                invoke_callable(
                    executor,
                    &mesh_at,
                    vec![point_value(pos), sample_index_value(ix, iy)],
                    "mesh_at",
                )
                .await?,
            );
        }
    }

    Ok(list_value(out))
}

#[stdlib_func]
pub async fn mk_text(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let text = read_string(executor, stack_idx, -2, "text").await?;
    let scale = read_text_scale(executor, stack_idx, -1, "scale")?;
    let meshes = latex::render_text(&text, scale).map_err(|error| {
        ExecutorError::invalid_invocation(format!("text render failed: {error:#}"))
    })?;
    Ok(latex_meshes_to_value(meshes))
}

#[stdlib_func]
pub async fn mk_tex(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tex = read_string(executor, stack_idx, -2, "tex").await?;
    let scale = read_text_scale(executor, stack_idx, -1, "scale")?;
    let meshes = latex::render_tex(&tex, scale).map_err(|error| {
        ExecutorError::invalid_invocation(format!("tex render failed: {error:#}"))
    })?;
    Ok(latex_meshes_to_value(meshes))
}

#[stdlib_func]
pub async fn mk_latex(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let latex = read_string(executor, stack_idx, -2, "latex").await?;
    let scale = read_text_scale(executor, stack_idx, -1, "scale")?;
    let meshes = latex::render_latex(&latex, scale).map_err(|error| {
        ExecutorError::invalid_invocation(format!("latex render failed: {error:#}"))
    })?;
    Ok(latex_meshes_to_value(meshes))
}

#[stdlib_func]
pub async fn mk_brace(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("curly brace spanning a mesh along a direction")
}

#[stdlib_func]
pub async fn mk_measure(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    const MEASURE_EXTRUSION: f32 = 0.05;

    let tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let direction = read_float3(executor, stack_idx, -2, "dir")?;
    let buffer = read_nonnegative_float(executor, stack_idx, -1, "buffer")?;
    if direction.len_sq() <= 1e-12 {
        return Err(ExecutorError::InvalidArgument {
            arg: "dir",
            message: "must be non-zero",
        });
    }

    let direction = direction.normalize();
    let normal = if direction.cross(Float3::Z).len_sq() > 1e-6 {
        Float3::Z
    } else {
        Float3::Y
    };
    let right = normal.cross(direction).normalize();
    let left = -right;

    let cutoff = extremal_point(&tree, direction)
        .ok_or(ExecutorError::InvalidArgument {
            arg: "target",
            message: "mesh tree must contain at least one vertex",
        })?
        .dot(direction);
    let right_d = extremal_point(&tree, right)
        .unwrap_or(Float3::ZERO)
        .dot(right);
    let left_d = extremal_point(&tree, left)
        .unwrap_or(Float3::ZERO)
        .dot(left);

    let forward_delta = direction * (cutoff + buffer - MEASURE_EXTRUSION);
    let pivot_delta = direction * (cutoff + buffer);
    let back_delta = direction * (cutoff + buffer + MEASURE_EXTRUSION);

    let right_ortho = right * right_d;
    let left_ortho = left * left_d;

    let right_pivot = pivot_delta + right_ortho;
    let right_forward = forward_delta + right_ortho;
    let right_back = back_delta + right_ortho;
    let left_pivot = pivot_delta + left_ortho;
    let left_forward = forward_delta + left_ortho;
    let left_back = back_delta + left_ortho;

    let points = [
        right_back,
        right_forward,
        right_pivot,
        left_pivot,
        left_forward,
        left_back,
        left_pivot,
        right_pivot,
        right_back,
    ];
    let lins = points
        .windows(2)
        .map(|segment| default_lin(segment[0], segment[1], normal))
        .collect();

    Ok(mesh_from_parts(vec![], lins, vec![]))
}

#[stdlib_func]
pub async fn mk_label(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let target = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let str = read_string(executor, stack_idx, -4, "str").await?;
    let scale = read_text_scale(executor, stack_idx, -3, "scale")?;
    let dir = read_float3(executor, stack_idx, -2, "dir")?;
    let buffer = read_nonnegative_float(executor, stack_idx, -1, "buffer")?;
    if dir.len_sq() <= 1e-12 {
        return Err(ExecutorError::InvalidArgument {
            arg: "dir",
            message: "must be non-zero",
        });
    }

    let meshes = latex::render_latex(&str, scale).map_err(|error| {
        ExecutorError::invalid_invocation(format!("latex render failed: {error:#}"))
    })?;
    let mut label = MeshTree::List(meshes.into_iter().map(MeshTree::Mesh).collect());
    if label.iter().next().is_none() {
        return Ok(label.into_value());
    }

    let dir = dir.normalize();
    let target_center = tree_center(&target).ok_or(ExecutorError::InvalidArgument {
        arg: "target",
        message: "mesh tree must contain at least one vertex",
    })?;
    let label_center = tree_center(&label).unwrap_or(Float3::ZERO);
    let label_face = extremal_point(&label, -dir)
        .unwrap_or(label_center)
        .dot(dir);
    let target_face = extremal_point(&target, dir)
        .unwrap_or(target_center)
        .dot(dir);
    let orth = (target_center - label_center) - dir * (target_center - label_center).dot(dir);
    let delta = dir * (target_face - label_face + buffer) + orth;

    label.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |point| point + delta));
    Ok(label.into_value())
}

#[stdlib_func]
pub async fn mk_number(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("number rendered with fixed precision, usable for counters")
}

#[stdlib_func]
pub async fn mk_axis1d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -9, "center")?;
    let axis = read_float3(executor, stack_idx, -8, "axis")?.normalize();
    let normal = read_float3(executor, stack_idx, -7, "normal")?;
    let min = crate::read_float(executor, stack_idx, -6, "min")? as f32;
    let max = crate::read_float(executor, stack_idx, -5, "max")? as f32;
    let unit = crate::read_float(executor, stack_idx, -4, "unit")? as f32;
    let tick_step = crate::read_float(executor, stack_idx, -3, "tick_step")?
        .abs()
        .max(1e-3) as f32;
    ensure_limit(
        "axis ticks",
        tick_count(min, max, tick_step),
        MAX_AXIS_TICKS,
    )?;
    let tick_dir = {
        let dir = normal.cross(axis);
        if dir.len_sq() > 1e-6 {
            dir.normalize()
        } else {
            polygon_basis(axis).1
        }
    };
    let tick_half = 0.08 * unit.max(0.2);
    let mut lins = vec![default_lin(
        center + axis * (min * unit),
        center + axis * (max * unit),
        normal,
    )];
    let mut v = (min / tick_step).ceil() * tick_step;
    while v <= max + 1e-4 {
        let p = center + axis * (v * unit);
        lins.push(default_lin(
            p - tick_dir * tick_half,
            p + tick_dir * tick_half,
            normal,
        ));
        v += tick_step;
    }
    Ok(mesh_from_parts(vec![], lins, vec![]))
}

#[stdlib_func]
pub async fn mk_axis2d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -16, "center")?;
    let x_axis = read_float3(executor, stack_idx, -15, "x_axis")?.normalize();
    let y_axis = read_float3(executor, stack_idx, -14, "y_axis")?.normalize();
    let x_min = crate::read_float(executor, stack_idx, -13, "x_min")? as f32;
    let x_max = crate::read_float(executor, stack_idx, -12, "x_max")? as f32;
    let x_unit = crate::read_float(executor, stack_idx, -11, "x_unit")? as f32;
    let x_tick = crate::read_float(executor, stack_idx, -10, "x_tick")?
        .abs()
        .max(1e-3) as f32;
    let y_min = crate::read_float(executor, stack_idx, -9, "y_min")? as f32;
    let y_max = crate::read_float(executor, stack_idx, -8, "y_max")? as f32;
    let y_unit = crate::read_float(executor, stack_idx, -7, "y_unit")? as f32;
    let y_tick = crate::read_float(executor, stack_idx, -6, "y_tick")?
        .abs()
        .max(1e-3) as f32;
    let grid = read_flag(executor, stack_idx, -1, "grid")?;
    ensure_limit(
        "axis x ticks",
        tick_count(x_min, x_max, x_tick),
        MAX_AXIS_TICKS,
    )?;
    ensure_limit(
        "axis y ticks",
        tick_count(y_min, y_max, y_tick),
        MAX_AXIS_TICKS,
    )?;
    let normal = x_axis.cross(y_axis).normalize();
    let tick_half = 0.06 * x_unit.min(y_unit).max(0.2);
    let mut lins = vec![
        default_lin(
            center + x_axis * (x_min * x_unit),
            center + x_axis * (x_max * x_unit),
            normal,
        ),
        default_lin(
            center + y_axis * (y_min * y_unit),
            center + y_axis * (y_max * y_unit),
            normal,
        ),
    ];
    let mut x = (x_min / x_tick).ceil() * x_tick;
    while x <= x_max + 1e-4 {
        let p = center + x_axis * (x * x_unit);
        lins.push(default_lin(
            p - y_axis * tick_half,
            p + y_axis * tick_half,
            normal,
        ));
        if grid && x.abs() > 1e-4 {
            lins.push(default_lin(
                p + y_axis * (y_min * y_unit),
                p + y_axis * (y_max * y_unit),
                normal,
            ));
        }
        x += x_tick;
    }
    let mut y = (y_min / y_tick).ceil() * y_tick;
    while y <= y_max + 1e-4 {
        let p = center + y_axis * (y * y_unit);
        lins.push(default_lin(
            p - x_axis * tick_half,
            p + x_axis * tick_half,
            normal,
        ));
        if grid && y.abs() > 1e-4 {
            lins.push(default_lin(
                p + x_axis * (x_min * x_unit),
                p + x_axis * (x_max * x_unit),
                normal,
            ));
        }
        y += y_tick;
    }
    Ok(mesh_from_parts(vec![], lins, vec![]))
}

#[stdlib_func]
pub async fn mk_axis3d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -18, "center")?;
    let x_axis = read_float3(executor, stack_idx, -17, "x_axis")?.normalize();
    let y_axis = read_float3(executor, stack_idx, -16, "y_axis")?.normalize();
    let z_axis = read_float3(executor, stack_idx, -15, "z_axis")?.normalize();
    let x_min = crate::read_float(executor, stack_idx, -14, "x_min")? as f32;
    let x_max = crate::read_float(executor, stack_idx, -13, "x_max")? as f32;
    let x_unit = crate::read_float(executor, stack_idx, -12, "x_unit")? as f32;
    let y_min = crate::read_float(executor, stack_idx, -10, "y_min")? as f32;
    let y_max = crate::read_float(executor, stack_idx, -9, "y_max")? as f32;
    let y_unit = crate::read_float(executor, stack_idx, -8, "y_unit")? as f32;
    let z_min = crate::read_float(executor, stack_idx, -6, "z_min")? as f32;
    let z_max = crate::read_float(executor, stack_idx, -5, "z_max")? as f32;
    let z_unit = crate::read_float(executor, stack_idx, -4, "z_unit")? as f32;
    let grid = read_flag(executor, stack_idx, -1, "grid")?;
    let mut lins = vec![
        default_lin(
            center + x_axis * (x_min * x_unit),
            center + x_axis * (x_max * x_unit),
            Float3::Z,
        ),
        default_lin(
            center + y_axis * (y_min * y_unit),
            center + y_axis * (y_max * y_unit),
            Float3::Z,
        ),
        default_lin(
            center + z_axis * (z_min * z_unit),
            center + z_axis * (z_max * z_unit),
            Float3::Z,
        ),
    ];
    if grid {
        lins.push(default_lin(
            center + x_axis * (x_min * x_unit),
            center + x_axis * (x_max * x_unit) + y_axis * (y_max * y_unit),
            Float3::Z,
        ));
        lins.push(default_lin(
            center + y_axis * (y_min * y_unit),
            center + y_axis * (y_max * y_unit) + z_axis * (z_max * z_unit),
            Float3::Z,
        ));
        lins.push(default_lin(
            center + z_axis * (z_min * z_unit),
            center + z_axis * (z_max * z_unit) + x_axis * (x_max * x_unit),
            Float3::Z,
        ));
    }
    Ok(mesh_from_parts(vec![], lins, vec![]))
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
    let mut points = Vec::with_capacity(samples);
    for i in 0..samples {
        let t = if samples == 1 {
            t0
        } else {
            t0 + (t1 - t0) * i as f64 / (samples - 1) as f64
        };
        points.push(float3_from_value(
            invoke_callable(executor, &f, vec![Value::Float(t)], "f").await?,
            "f",
        )?);
    }
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
    let mut points = Vec::with_capacity(samples);
    for i in 0..samples {
        let x = x0 + (x1 - x0) * i as f64 / (samples - 1) as f64;
        let y = match invoke_callable(executor, &f, vec![Value::Float(x)], "f").await? {
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
    for (ix, col) in grid.iter_mut().enumerate() {
        for (iy, point) in col.iter_mut().enumerate() {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let z = match invoke_callable(
                executor,
                &f,
                vec![Value::Float(x as f64), Value::Float(y as f64)],
                "f",
            )
            .await?
            {
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
            *point = Float3::new(x, y, z);
        }
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
    for (ix, col) in vals.iter_mut().enumerate() {
        for (iy, val) in col.iter_mut().enumerate() {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            *val = match invoke_callable(
                executor,
                &f,
                vec![Value::Float(x as f64), Value::Float(y as f64)],
                "f",
            )
            .await?
            {
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
    let fill0 = read_float4(executor, stack_idx, -4, "fill0")?;
    let fill1 = read_float4(executor, stack_idx, -3, "fill1")?;
    let tag0 = read_int(executor, stack_idx, -2, "tag0")? as isize;
    let tag1 = read_int(executor, stack_idx, -1, "tag1")? as isize;

    let mut upper = Vec::with_capacity(samples);
    let mut lower = Vec::with_capacity(samples);
    for i in 0..samples {
        let x = x0 + (x1 - x0) * i as f64 / (samples - 1) as f64;
        let yf = match invoke_callable(executor, &f, vec![Value::Float(x)], "f").await? {
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
        let yg = match invoke_callable(executor, &g, vec![Value::Float(x)], "g").await? {
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

    let make_tagged_mesh = |lins, tris, tag: isize| {
        let mesh = geo::mesh::Mesh {
            dots: vec![],
            lins,
            tris,
            uniform: geo::mesh::Uniforms::default(),
            tag: vec![tag],
        };
        debug_assert!(mesh.has_consistent_topology());
        Value::Mesh(std::sync::Arc::new(mesh))
    };

    let pos_val = make_tagged_mesh(pos_lins, pos_tris, tag0);
    let neg_val = make_tagged_mesh(neg_lins, neg_tris, tag1);

    let mut lins = open_polyline(&upper, Float3::Z);
    push_open_polyline(&mut lins, &lower, Float3::Z);
    let outline_val = mesh_from_parts(vec![], lins, vec![]);

    Ok(list_value([pos_val, neg_val, outline_val]))
}

#[stdlib_func]
pub async fn mk_stack(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut items = read_mesh_tree_list_arg(executor, stack_idx, -3, "meshes").await?;
    let dir = read_float3(executor, stack_idx, -2, "dir")?.normalize();
    let align_dir = read_float3(executor, stack_idx, -1, "align_dir");
    let align_dir = align_dir.unwrap_or(Float3::ZERO);
    let mut cursor = 0.0f32;
    for (i, item) in items.iter_mut().enumerate() {
        let center = tree_center(item).unwrap_or(Float3::ZERO);
        let backward = extremal_point(item, -dir).unwrap_or(center).dot(dir);
        let forward = extremal_point(item, dir).unwrap_or(center).dot(dir);
        let align = if align_dir.len_sq() > 0.0 {
            extremal_point(item, align_dir)
                .unwrap_or(center)
                .dot(align_dir)
        } else {
            0.0
        };
        let target = if i == 0 {
            -backward
        } else {
            cursor - backward + 0.1
        };
        let align_shift = if align_dir.len_sq() > 0.0 {
            align_dir.normalize() * -align
        } else {
            Float3::ZERO
        };
        let delta = dir * target + align_shift;
        item.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
        cursor = target + forward;
    }
    Ok(list_value(items.into_iter().map(MeshTree::into_value)))
}

#[stdlib_func]
pub async fn mk_grid(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let rows = read_mesh_tree_list_arg(executor, stack_idx, -1, "mesh_array").await?;
    Ok(list_value(rows.into_iter().map(MeshTree::into_value)))
}

#[stdlib_func]
pub async fn mk_table(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    mk_grid(executor, stack_idx).await
}

#[stdlib_func]
pub async fn mk_bounding_box(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let buffer = crate::read_float(executor, stack_idx, -1, "buffer")? as f32;
    let (min, max) = bounds_of(&tree).ok_or(ExecutorError::InvalidArgument {
        arg: "target",
        message: "mesh tree must contain at least one vertex",
    })?;
    let center = (min + max) / 2.0;
    let size = max - min + Float3::splat(buffer * 2.0);
    let corners = vec![
        Float3::new(center.x - size.x / 2.0, center.y - size.y / 2.0, center.z),
        Float3::new(center.x + size.x / 2.0, center.y - size.y / 2.0, center.z),
        Float3::new(center.x + size.x / 2.0, center.y + size.y / 2.0, center.z),
        Float3::new(center.x - size.x / 2.0, center.y + size.y / 2.0, center.z),
    ];
    Ok(mesh_from_parts(
        vec![],
        closed_polyline(&corners, Float3::Z),
        vec![],
    ))
}
