use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use executor::{
    camera::{
        CameraBasis, CameraSnapshot, DEFAULT_CAMERA_ASPECT, DEFAULT_CAMERA_FOV, parse_camera_arg,
    },
    error::ExecutorError,
    executor::Executor,
    heap::with_heap,
    value::Value,
};
use geo::{
    mesh::{Lin, Mesh, make_mesh_mut},
    mesh_build::{BoundaryEdge, IndexedSurface, SurfaceVertex},
    simd::{Float3, Float4},
};
use smallvec::{SmallVec, smallvec};
use stdlib_macros::stdlib_func;

use super::helpers::*;

fn read_scale_factor(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Float3, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::Integer(value) => Ok(Float3::splat(value as f32)),
        Value::Float(value) => Ok(Float3::splat(value as f32)),
        Value::List(_) => read_float3(executor, stack_idx, index, name),
        other => Err(ExecutorError::type_error_for(
            "float or list of length 3",
            other.type_name(),
            name,
        )),
    }
}

fn read_level(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<f32, ExecutorError> {
    Ok(crate::read_float(executor, stack_idx, index, name)?.clamp(0.0, 1.0) as f32)
}

fn recolor_mesh(mesh: &mut Mesh, color: geo::simd::Float4, level: f32) {
    for dot in &mut mesh.dots {
        dot.col = dot.col.lerp(color, level);
    }
    for lin in &mut mesh.lins {
        lin.a.col = lin.a.col.lerp(color, level);
        lin.b.col = lin.b.col.lerp(color, level);
    }
    for tri in &mut mesh.tris {
        tri.a.col = tri.a.col.lerp(color, level);
        tri.b.col = tri.b.col.lerp(color, level);
        tri.c.col = tri.c.col.lerp(color, level);
    }
}

fn transform_hint_normal(normal: Float3, x_unit: Float3, y_unit: Float3, z_unit: Float3) -> Float3 {
    let mapped = x_unit * normal.x + y_unit * normal.y + z_unit * normal.z;
    if mapped.len_sq() > 1e-12 {
        mapped.normalize()
    } else {
        normal
    }
}

fn decode_mesh_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

fn viewport_half_extents(depth: f32) -> (f32, f32) {
    let depth = depth.max(executor::camera::MIN_CAMERA_NEAR);
    let tan_half_fov = (DEFAULT_CAMERA_FOV * 0.5).tan().max(0.05);
    (
        depth * tan_half_fov * DEFAULT_CAMERA_ASPECT,
        depth * tan_half_fov,
    )
}

fn camera_space_placement_delta(
    tree: &MeshTree,
    camera: CameraBasis,
    side: Float3,
    buffer: f32,
) -> Option<Float3> {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut right_delta = f32::INFINITY;
    let mut left_delta = f32::NEG_INFINITY;
    let mut up_delta = f32::INFINITY;
    let mut down_delta = f32::NEG_INFINITY;
    let mut saw_vertex = false;

    for mesh in tree.iter() {
        for point in mesh_vertices(mesh) {
            saw_vertex = true;
            let relative = point - camera.position;
            let x = relative.dot(camera.right);
            let y = relative.dot(camera.up);
            let z = relative.dot(camera.forward).max(camera.near);
            let (half_width, half_height) = viewport_half_extents(z);
            let x_limit = (half_width - buffer).max(0.0);
            let y_limit = (half_height - buffer).max(0.0);

            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            right_delta = right_delta.min(x_limit - x);
            left_delta = left_delta.max(-x_limit - x);
            up_delta = up_delta.min(y_limit - y);
            down_delta = down_delta.max(-y_limit - y);
        }
    }

    if !saw_vertex {
        return None;
    }

    let center_x = (min_x + max_x) * 0.5;
    let center_y = (min_y + max_y) * 0.5;
    let delta_x = if side.x < 0.0 {
        left_delta
    } else if side.x > 0.0 {
        right_delta
    } else {
        -center_x
    };
    let delta_y = if side.y < 0.0 {
        down_delta
    } else if side.y > 0.0 {
        up_delta
    } else {
        -center_y
    };

    Some(camera.right * delta_x + camera.up * delta_y)
}

async fn read_camera_basis_or_default(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    target: &'static str,
) -> Result<CameraBasis, ExecutorError> {
    let value = executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue();
    if matches!(value, Value::Nil) {
        Ok(CameraSnapshot::default().basis())
    } else {
        Ok(parse_camera_arg(executor, stack_idx, index, target)
            .await?
            .basis())
    }
}

fn camera_space_coords(point: Float3, camera: CameraBasis) -> Float3 {
    let relative = point - camera.position;
    Float3::new(
        relative.dot(camera.right),
        relative.dot(camera.up),
        relative.dot(camera.forward),
    )
}

fn point_from_camera_space(coords: Float3, camera: CameraBasis) -> Float3 {
    camera.position + camera.right * coords.x + camera.up * coords.y + camera.forward * coords.z
}

fn remap_point_between_cameras(point: Float3, from: CameraBasis, to: CameraBasis) -> Float3 {
    point_from_camera_space(camera_space_coords(point, from), to)
}

fn remap_direction_between_cameras(
    direction: Float3,
    from: CameraBasis,
    to: CameraBasis,
) -> Float3 {
    to.right * direction.dot(from.right)
        + to.up * direction.dot(from.up)
        + to.forward * direction.dot(from.forward)
}

fn filtered_tree_view<'a>(
    executor: &'a mut Executor,
    tree: &'a MeshTree,
    filter: Option<&'a TagFilter>,
) -> Pin<Box<dyn Future<Output = Result<Option<MeshTree>, ExecutorError>> + 'a>> {
    Box::pin(async move {
        match filter {
            Some(filter) => filter_tree_by_tag_filter(executor, tree.clone(), filter).await,
            None => Ok(Some(tree.clone())),
        }
    })
}

fn blend_mesh_positions(mesh: &mut Mesh, level: f32, map: impl Fn(Float3) -> Float3) {
    for dot in &mut mesh.dots {
        let original = dot.pos;
        dot.pos = original.lerp(map(original), level);
    }
    for lin in &mut mesh.lins {
        let original = lin.a.pos;
        lin.a.pos = original.lerp(map(original), level);
        let original = lin.b.pos;
        lin.b.pos = original.lerp(map(original), level);
    }
    for tri in &mut mesh.tris {
        let original = tri.a.pos;
        tri.a.pos = original.lerp(map(original), level);
        let original = tri.b.pos;
        tri.b.pos = original.lerp(map(original), level);
        let original = tri.c.pos;
        tri.c.pos = original.lerp(map(original), level);
    }
}

fn midpoint_vertex(
    vertices: &mut Vec<SurfaceVertex>,
    edge_midpoints: &mut HashMap<(usize, usize), usize>,
    a: usize,
    b: usize,
) -> usize {
    let key = if a <= b { (a, b) } else { (b, a) };
    if let Some(&idx) = edge_midpoints.get(&key) {
        return idx;
    }

    let idx = vertices.len();
    vertices.push(SurfaceVertex {
        pos: vertices[a].pos.lerp(vertices[b].pos, 0.5),
        col: vertices[a].col.lerp(vertices[b].col, 0.5),
        uv: vertices[a].uv.lerp(vertices[b].uv, 0.5),
    });
    edge_midpoints.insert(key, idx);
    idx
}

fn subdivide_indexed_surface(surface: &IndexedSurface) -> IndexedSurface {
    let mut vertices = surface.vertices.clone();
    let mut edge_midpoints = HashMap::<(usize, usize), usize>::new();
    let mut faces = Vec::with_capacity(surface.faces.len() * 4);

    for &[a, b, c] in &surface.faces {
        let ab = midpoint_vertex(&mut vertices, &mut edge_midpoints, a, b);
        let bc = midpoint_vertex(&mut vertices, &mut edge_midpoints, b, c);
        let ca = midpoint_vertex(&mut vertices, &mut edge_midpoints, c, a);
        faces.push([a, ab, ca]);
        faces.push([ab, b, bc]);
        faces.push([ca, bc, c]);
        faces.push([ab, bc, ca]);
    }

    let mut boundary_edges = HashMap::with_capacity(surface.boundary_edges.len() * 2);
    for (&(a, b), template) in &surface.boundary_edges {
        let mid = edge_midpoints[&if a <= b { (a, b) } else { (b, a) }];
        boundary_edges.insert(
            (a, mid),
            BoundaryEdge {
                a_col: template.a_col,
                b_col: template.a_col.lerp(template.b_col, 0.5),
                norm: template.norm,
            },
        );
        boundary_edges.insert(
            (mid, b),
            BoundaryEdge {
                a_col: template.a_col.lerp(template.b_col, 0.5),
                b_col: template.b_col,
                norm: template.norm,
            },
        );
    }

    IndexedSurface {
        vertices,
        faces,
        boundary_edges,
    }
}

fn clear_surface_line_refs(mesh: &mut Mesh) {
    for tri in &mut mesh.tris {
        for edge in [&mut tri.ab, &mut tri.bc, &mut tri.ca] {
            if *edge < -1 {
                *edge = -1;
            }
        }
    }
}

fn linked_prev(lines: &[Lin], idx: usize) -> Option<usize> {
    let prev = lines[idx].prev;
    (prev >= 0)
        .then_some(prev as usize)
        .filter(|&prev| prev < lines.len() && lines[prev].next == idx as i32)
}

fn linked_next(lines: &[Lin], idx: usize) -> Option<usize> {
    let next = lines[idx].next;
    (next >= 0)
        .then_some(next as usize)
        .filter(|&next| next < lines.len() && lines[next].prev == idx as i32)
}

fn line_paths(lines: &[Lin]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; lines.len()];
    let mut out = Vec::new();

    let mut walk_path = |start: usize, visited: &mut [bool]| {
        let mut path = Vec::new();
        let mut cursor = start;
        loop {
            if visited[cursor] {
                break;
            }
            visited[cursor] = true;
            path.push(cursor);

            let Some(next) = linked_next(lines, cursor) else {
                break;
            };
            if next == start || visited[next] {
                break;
            }
            cursor = next;
        }

        if !path.is_empty() {
            out.push(path);
        }
    };

    for idx in 0..lines.len() {
        if visited[idx] || linked_prev(lines, idx).is_some() {
            continue;
        }
        walk_path(idx, &mut visited);
    }

    for idx in 0..lines.len() {
        if visited[idx] {
            continue;
        }
        walk_path(idx, &mut visited);
    }

    out
}

fn snap_line_t(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 1e-5 {
        0.0
    } else if (1.0 - t).abs() <= 1e-5 {
        1.0
    } else {
        t
    }
}

fn push_dashed_segment(
    out: &mut Vec<Lin>,
    current_piece_last: &mut Option<usize>,
    template: &Lin,
    t0: f32,
    t1: f32,
) {
    let t0 = snap_line_t(t0);
    let t1 = snap_line_t(t1);
    if (t1 - t0).abs() <= 1e-6 {
        return;
    }

    let point_at = |start: Float3, end: Float3, t: f32| match t {
        0.0 => start,
        1.0 => end,
        _ => start.lerp(end, t),
    };
    let color_at = |start: Float4, end: Float4, t: f32| match t {
        0.0 => start,
        1.0 => end,
        _ => start.lerp(end, t),
    };

    let mut segment = default_lin(
        point_at(template.a.pos, template.b.pos, t0),
        point_at(template.a.pos, template.b.pos, t1),
        template.norm,
    );
    segment.a.col = color_at(template.a.col, template.b.col, t0);
    segment.b.col = color_at(template.a.col, template.b.col, t1);

    if let Some(prev) = *current_piece_last {
        if float3_key(out[prev].b.pos) == float3_key(segment.a.pos)
            && float3_key(out[prev].norm) == float3_key(segment.norm)
        {
            segment.prev = prev as i32;
            out[prev].next = out.len() as i32;
        }
    }

    *current_piece_last = Some(out.len());
    out.push(segment);
}

fn dashed_lines(source_lines: &[Lin], dash_length: f32, gap_length: f32, offset: f32) -> Vec<Lin> {
    let period = dash_length + gap_length;
    let mut out = Vec::new();

    for path in line_paths(source_lines) {
        let mut current_piece_last = None;
        let mut distance = 0.0f32;

        for &line_idx in &path {
            let line = &source_lines[line_idx];
            let length = (line.b.pos - line.a.pos).len();
            if length <= 1e-6 {
                continue;
            }

            let mut local = 0.0f32;
            while local < length - 1e-6 {
                let global = distance + local;
                let phase = (global + offset).rem_euclid(period);

                if phase < dash_length - 1e-6 || gap_length <= 1e-6 {
                    let visible = (dash_length - phase).max(0.0).min(length - local);
                    if visible <= 1e-6 {
                        break;
                    }
                    push_dashed_segment(
                        &mut out,
                        &mut current_piece_last,
                        line,
                        local / length,
                        (local + visible) / length,
                    );
                    local += visible;
                } else {
                    current_piece_last = None;
                    let hidden = (period - phase).max(1e-6).min(length - local);
                    local += hidden;
                }
            }

            distance += length;
        }
    }

    out
}

fn stroke_source_lines(mesh: &Mesh) -> Vec<Lin> {
    if !mesh.lins.is_empty() {
        return mesh
            .lins
            .iter()
            .copied()
            .filter(|line| line.is_dom_sib)
            .collect();
    }
    if mesh.tris.is_empty() {
        return Vec::new();
    }

    let surface = mesh_to_indexed_surface(mesh);
    build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges)
        .0
        .into_iter()
        .map(|mut line| {
            line.inv = -1;
            line
        })
        .collect()
}

fn dashed_mesh(mesh: &Mesh, dash_length: f32, gap_length: f32, offset: f32) -> MeshTree {
    let dashed_lins = dashed_lines(&stroke_source_lines(mesh), dash_length, gap_length, offset);
    let has_base_geometry =
        mesh.dots.iter().any(|dot| dot.col.w > f32::EPSILON) || !mesh.tris.is_empty();

    if !has_base_geometry {
        if dashed_lins.is_empty() {
            return MeshTree::Mesh(Arc::new(mesh.clone()));
        }

        let dashed = Mesh {
            dots: Vec::new(),
            lins: dashed_lins,
            tris: Vec::new(),
            uniform: mesh.uniform.clone(),
            tag: mesh.tag.clone(),
            version: Mesh::fresh_version(),
        };
        let mut dashed = dashed;
        dashed.normalize_line_dot_topology();
        dashed.debug_assert_consistent_topology();
        return MeshTree::Mesh(Arc::new(dashed));
    }

    let mut children = Vec::with_capacity(2);

    let mut base = mesh.clone();
    if !base.lins.is_empty() {
        base.lins.clear();
    }
    if !base.tris.is_empty() {
        clear_surface_line_refs(&mut base);
    }
    base.debug_assert_consistent_topology();
    children.push(MeshTree::Mesh(Arc::new(base)));

    if !dashed_lins.is_empty() {
        let mut dashed = Mesh {
            dots: Vec::new(),
            lins: dashed_lins,
            tris: Vec::new(),
            uniform: mesh.uniform.clone(),
            tag: mesh.tag.clone(),
            version: Mesh::fresh_version(),
        };
        dashed.normalize_line_dot_topology();
        dashed.debug_assert_consistent_topology();
        children.push(MeshTree::Mesh(Arc::new(dashed)));
    }

    MeshTree::List(children)
}

fn dashed_tree<'a>(
    executor: &'a mut Executor,
    tree: MeshTree,
    dash_length: f32,
    gap_length: f32,
    offset: f32,
    filter: Option<&'a TagFilter>,
) -> Pin<Box<dyn Future<Output = Result<MeshTree, ExecutorError>> + 'a>> {
    Box::pin(async move {
        match tree {
            MeshTree::Mesh(mesh) => {
                let keep = match filter {
                    Some(filter) => mesh_matches_tag_filter(executor, filter, &mesh).await?,
                    None => true,
                };
                if keep {
                    Ok(dashed_mesh(&mesh, dash_length, gap_length, offset))
                } else {
                    Ok(MeshTree::Mesh(mesh))
                }
            }
            MeshTree::List(children) => {
                let mut out = Vec::with_capacity(children.len());
                for child in children {
                    out.push(
                        dashed_tree(executor, child, dash_length, gap_length, offset, filter)
                            .await?,
                    );
                }
                Ok(MeshTree::List(out))
            }
        }
    })
}

#[stdlib_func]
pub async fn op_shift(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let delta = read_float3(executor, stack_idx, -2, "delta")?;
    if delta.len_sq() <= 1e-12 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| p + delta)
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_scale(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let factor = read_scale_factor(executor, stack_idx, -2, "factor")?;
    if (factor - Float3::splat(1.0)).len_sq() <= 1e-12 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let center = tree_center(&view).unwrap_or(Float3::ZERO);
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| center + (p - center) * factor);
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_rotate(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let angle = crate::read_float(executor, stack_idx, -4, "radians")? as f32;
    if angle.abs() <= 1e-12 {
        return Ok(tree.into_value());
    }
    let axis = read_float3(executor, stack_idx, -3, "axis")?;
    let axis = if axis.len_sq() <= 1e-12 {
        Float3::Z
    } else {
        axis.normalize()
    };
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let pivot = match executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::Nil => None,
        value => Some(float3_from_value(value, "pivot")?),
    };
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        let center = pivot.unwrap_or_else(|| mesh_center(mesh).unwrap_or(Float3::ZERO));
        transform_mesh_positions(mesh, |p| {
            center + rotate_about_axis(p - center, axis, angle)
        });
        for dot in &mut mesh.dots {
            dot.norm = rotate_about_axis(dot.norm, axis, angle);
        }
        for lin in &mut mesh.lins {
            lin.norm = rotate_about_axis(lin.norm, axis, angle);
        }
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_camera_transfer(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let original_camera = parse_camera_arg(executor, stack_idx, -4, "original_camera")
        .await?
        .basis();
    let live_camera = parse_camera_arg(executor, stack_idx, -3, "live_camera")
        .await?
        .basis();
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        blend_mesh_positions(mesh, level, |point| {
            remap_point_between_cameras(point, original_camera, live_camera)
        });
        for dot in &mut mesh.dots {
            let target = remap_direction_between_cameras(dot.norm, original_camera, live_camera);
            dot.norm = dot.norm.lerp(target, level);
        }
        for lin in &mut mesh.lins {
            let target = remap_direction_between_cameras(lin.norm, original_camera, live_camera);
            lin.norm = lin.norm.lerp(target, level);
        }
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_fade(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let alpha = crate::read_float(executor, stack_idx, -2, "opacity")?;
    if (alpha - 1.0).abs() <= 1e-12 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        mesh.uniform.alpha *= alpha;
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_restroke(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let color = read_float4(executor, stack_idx, -4, "color").await?;
    let stroke_width = crate::read_float(executor, stack_idx, -3, "stroke_width")? as f32;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    let stroke_radius = stroke_width.max(0.0);
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        mesh.uniform.stroke_radius = stroke_radius;
    })
    .await?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        for lin in &mut mesh.lins {
            lin.a.col = lin.a.col.lerp(color, level);
            lin.b.col = lin.b.col.lerp(color, level);
        }
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_refill(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let color = read_float4(executor, stack_idx, -3, "color").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        for tri in &mut mesh.tris {
            tri.a.col = tri.a.col.lerp(color, level);
            tri.b.col = tri.b.col.lerp(color, level);
            tri.c.col = tri.c.col.lerp(color, level);
        }
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_redot(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let color = read_float4(executor, stack_idx, -3, "color").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        for dot in &mut mesh.dots {
            dot.col = dot.col.lerp(color, level);
        }
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_recolor(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let color = read_float4(executor, stack_idx, -3, "color").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        recolor_mesh(mesh, color, level);
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_normal_hint(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let normal = read_float3(executor, stack_idx, -3, "normal")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        for dot in &mut mesh.dots {
            dot.norm = dot.norm.lerp(normal, level);
        }
        for lin in &mut mesh.lins {
            lin.norm = lin.norm.lerp(normal, level);
        }
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_retextured(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let image = read_string(executor, stack_idx, -2, "image").await?;
    let image = resolve_image_path(executor, stack_idx, &image)?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        mesh.uniform.img = Some(image.clone());
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_with_zindex(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let z_index = read_int(executor, stack_idx, -2, "z_index")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        mesh.uniform.z_index = z_index as i32;
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_gloss(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let gloss = crate::read_float(executor, stack_idx, -2, "gloss")? as f32;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        mesh.uniform.gloss = gloss.max(0.0);
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_point_map(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
        level: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let positions = {
                        let mesh = arc.as_ref();
                        let mut positions = Vec::with_capacity(
                            mesh.dots.len() + mesh.lins.len() * 2 + mesh.tris.len() * 3,
                        );
                        positions.extend(mesh.dots.iter().map(|dot| dot.pos));
                        for lin in &mesh.lins {
                            positions.push(lin.a.pos);
                            positions.push(lin.b.pos);
                        }
                        for tri in &mesh.tris {
                            positions.push(tri.a.pos);
                            positions.push(tri.b.pos);
                            positions.push(tri.c.pos);
                        }
                        positions
                    };
                    let args = positions
                        .iter()
                        .map(|pos| smallvec![point_value(*pos)])
                        .collect::<Vec<SmallVec<[Value; 2]>>>();
                    let mapped = invoke_callable_many(executor, func, &args, "f")
                        .await?
                        .into_iter()
                        .map(|value| float3_from_value(value, "f"))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mesh = make_mesh_mut(arc);
                    let mut mapped_iter = mapped.into_iter();
                    for dot in &mut mesh.dots {
                        let original = dot.pos;
                        dot.pos = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for lin in &mut mesh.lins {
                        let original = lin.a.pos;
                        lin.a.pos = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = lin.b.pos;
                        lin.b.pos = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for tri in &mut mesh.tris {
                        let original = tri.a.pos;
                        tri.a.pos = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.b.pos;
                        tri.b.pos = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.c.pos;
                        tri.c.pos = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter, level).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref(), level).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_color_map(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
        level: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let positions = {
                        let mesh = arc.as_ref();
                        let mut positions = Vec::with_capacity(
                            mesh.dots.len() + mesh.lins.len() * 2 + mesh.tris.len() * 3,
                        );
                        positions.extend(mesh.dots.iter().map(|dot| dot.pos));
                        for lin in &mesh.lins {
                            positions.push(lin.a.pos);
                            positions.push(lin.b.pos);
                        }
                        for tri in &mesh.tris {
                            positions.push(tri.a.pos);
                            positions.push(tri.b.pos);
                            positions.push(tri.c.pos);
                        }
                        positions
                    };
                    let args = positions
                        .iter()
                        .map(|pos| smallvec![point_value(*pos)])
                        .collect::<Vec<SmallVec<[Value; 2]>>>();
                    let mapped = invoke_callable_many(executor, func, &args, "f")
                        .await?
                        .into_iter()
                        .map(|value| float4_from_value(value, "f"))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mesh = make_mesh_mut(arc);
                    let mut mapped_iter = mapped.into_iter();
                    for dot in &mut mesh.dots {
                        let original = dot.col;
                        dot.col = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for lin in &mut mesh.lins {
                        let original = lin.a.col;
                        lin.a.col = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = lin.b.col;
                        lin.b.col = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for tri in &mut mesh.tris {
                        let original = tri.a.col;
                        tri.a.col = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.b.col;
                        tri.b.col = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.c.col;
                        tri.c.col = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter, level).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref(), level).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_uv_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
        level: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let positions = {
                        let mesh = arc.as_ref();
                        let mut positions = Vec::with_capacity(mesh.tris.len() * 3);
                        for tri in &mesh.tris {
                            positions.push(tri.a.pos);
                            positions.push(tri.b.pos);
                            positions.push(tri.c.pos);
                        }
                        positions
                    };
                    let args = positions
                        .iter()
                        .map(|pos| smallvec![point_value(*pos)])
                        .collect::<Vec<SmallVec<[Value; 2]>>>();
                    let mapped = invoke_callable_many(executor, func, &args, "f")
                        .await?
                        .into_iter()
                        .map(|value| float2_from_value(value, "f"))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mesh = make_mesh_mut(arc);
                    let mut mapped_iter = mapped.into_iter();
                    for tri in &mut mesh.tris {
                        let original = tri.a.uv;
                        tri.a.uv = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.b.uv;
                        tri.b.uv = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.c.uv;
                        tri.c.uv = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter, level).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref(), level).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_retagged(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn parse_tags(value: Value) -> Result<Vec<isize>, ExecutorError> {
        match value.elide_lvalue_leader_rec() {
            Value::Integer(tag) => Ok(vec![tag as isize]),
            Value::Float(tag) if tag.fract() == 0.0 => Ok(vec![tag as isize]),
            Value::List(list) => list
                .elements()
                .iter()
                .map(|key| {
                    int_from_value(with_heap(|h| h.get(key.key()).clone()), "f")
                        .map(|tag| tag as isize)
                })
                .collect(),
            other => Err(ExecutorError::type_error_for(
                "int / list",
                other.type_name(),
                "f",
            )),
        }
    }

    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let mesh = make_mesh_mut(arc);
                    let tags = list_value(
                        mesh.tag
                            .iter()
                            .copied()
                            .map(|tag| Value::Integer(tag as i64)),
                    );
                    mesh.tag = parse_tags(invoke_callable(executor, func, vec![tags], "f").await?)?;
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref()).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_tag_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: MeshTree,
        func: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(mesh) => {
                    let tags = list_value(
                        mesh.tag
                            .iter()
                            .copied()
                            .map(|tag| Value::Integer(tag as i64)),
                    );
                    invoke_callable(executor, func, vec![tags, Value::Mesh(mesh)], "f").await
                }
                MeshTree::List(children) => {
                    let mut out = Vec::with_capacity(children.len());
                    for child in children {
                        out.push(recurse(executor, child, func).await?);
                    }
                    Ok(list_value(out))
                }
            }
        })
    }

    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();
    recurse(executor, tree, &func).await
}

#[stdlib_func]
pub async fn op_subset_map(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
    ) -> Pin<Box<dyn Future<Output = Result<MeshTree, ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(mesh) => {
                    let keep = match filter {
                        Some(filter) => mesh_matches_tag_filter(executor, filter, &mesh).await?,
                        None => true,
                    };
                    if !keep {
                        return Ok(MeshTree::Mesh(mesh));
                    }

                    let mapped =
                        invoke_callable(executor, func, vec![Value::Mesh(mesh.clone())], "f")
                            .await?;
                    read_mesh_tree(executor, mapped, "f").await
                }
                MeshTree::List(children) => {
                    let mut mapped = Vec::with_capacity(children.len());
                    for child in children {
                        mapped.push(recurse(executor, child, func, filter).await?);
                    }
                    Ok(MeshTree::List(mapped))
                }
            }
        })
    }

    let tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let filter = read_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();
    recurse(executor, tree, &func, Some(&filter))
        .await
        .map(MeshTree::into_value)
}

#[stdlib_func]
pub async fn op_uprank(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let mut tessellation_error = None;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        match uprank_mesh(mesh) {
            Ok(Some(upranked)) => *mesh = upranked,
            Ok(None) => {}
            Err(err) => tessellation_error = Some(err),
        }
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    if let Some(err) = tessellation_error {
        return Err(err);
    }
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_downrank(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if !mesh.tris.is_empty() {
            if !mesh.lins.is_empty() {
                mesh.lins = mesh
                    .lins
                    .iter()
                    .map(|lin| {
                        let mut out = *lin;
                        out.inv = -1;
                        out
                    })
                    .collect();
            } else {
                let surface = mesh_to_indexed_surface(mesh);
                mesh.lins = build_indexed_surface(
                    &surface.vertices,
                    &surface.faces,
                    &surface.boundary_edges,
                )
                .0
                .into_iter()
                .map(|lin| {
                    let mut out = lin;
                    out.inv = -1;
                    out
                })
                .collect();
            }
            mesh.tris.clear();
        } else if !mesh.lins.is_empty() {
            mesh.dots = mesh
                .lins
                .iter()
                .flat_map(|lin| {
                    [
                        default_dot(lin.a.pos, lin.norm),
                        default_dot(lin.b.pos, lin.norm),
                    ]
                })
                .collect();
            mesh.lins.clear();
        }
        mesh.normalize_line_dot_topology();
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_wireframe(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    op_downrank(executor, stack_idx).await
}

#[stdlib_func]
pub async fn op_dashed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let dash_length = crate::read_float(executor, stack_idx, -4, "dash_length")? as f32;
    let gap_length = crate::read_float(executor, stack_idx, -3, "gap_length")? as f32;
    let offset = crate::read_float(executor, stack_idx, -2, "offset")? as f32;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;

    if dash_length <= 1e-6 {
        return Err(ExecutorError::InvalidArgument {
            arg: "dash_length",
            message: "dash length must be positive",
        });
    }
    if gap_length < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "gap_length",
            message: "gap length must be non-negative",
        });
    }

    dashed_tree(
        executor,
        tree,
        dash_length,
        gap_length,
        offset,
        filter.as_ref(),
    )
    .await
    .map(MeshTree::into_value)
}

fn subdivide_line_mesh(mesh: &mut Mesh, factor: usize) {
    let original_lins = mesh.lins.clone();
    let mut lins = Vec::with_capacity(original_lins.len() * factor);

    for (lin_idx, lin) in original_lins.iter().enumerate() {
        let base = lin_idx * factor;
        for i in 0..factor {
            let u = i as f32 / factor as f32;
            let v = (i + 1) as f32 / factor as f32;
            let a = if i == 0 {
                lin.a.pos
            } else {
                lin.a.pos.lerp(lin.b.pos, u)
            };
            let b = if i + 1 == factor {
                lin.b.pos
            } else {
                lin.a.pos.lerp(lin.b.pos, v)
            };
            let mut out = default_lin(a, b, lin.norm);
            out.a.col = lin.a.col.lerp(lin.b.col, u);
            out.b.col = lin.a.col.lerp(lin.b.col, v);
            out.prev = if i == 0 {
                if lin.prev >= 0 {
                    (lin.prev as usize * factor + (factor - 1)) as i32
                } else {
                    lin.prev
                }
            } else {
                (base + i - 1) as i32
            };
            out.next = if i + 1 == factor {
                if lin.next >= 0 {
                    (lin.next as usize * factor) as i32
                } else {
                    lin.next
                }
            } else {
                (base + i + 1) as i32
            };
            out.inv = if lin.inv >= 0 {
                (lin.inv as usize * factor + (factor - 1 - i)) as i32
            } else {
                lin.inv
            };
            out.is_dom_sib = lin.is_dom_sib;
            lins.push(out);
        }
    }

    for line_idx in 0..original_lins.len() {
        let inv_idx = original_lins[line_idx].inv;
        if inv_idx < 0 {
            continue;
        }
        let inv_idx = inv_idx as usize;
        if inv_idx >= original_lins.len() || line_idx >= inv_idx {
            continue;
        }

        for i in 0..factor {
            let line_piece_idx = line_idx * factor + i;
            let inv_piece_idx = inv_idx * factor + (factor - 1 - i);
            let line_piece = lins[line_piece_idx];
            lins[inv_piece_idx].a.pos = line_piece.b.pos;
            lins[inv_piece_idx].b.pos = line_piece.a.pos;
        }
    }

    for (dot_idx, dot) in mesh.dots.iter_mut().enumerate() {
        let Some(line_idx) = decode_mesh_ref(dot.inv) else {
            continue;
        };
        let Some(line) = original_lins.get(line_idx) else {
            continue;
        };
        let dot_ref = mesh_ref(dot_idx);
        if line.prev == dot_ref {
            dot.inv = mesh_ref(line_idx * factor);
        } else if line.next == dot_ref {
            dot.inv = mesh_ref(line_idx * factor + factor - 1);
        }
    }

    mesh.lins = lins;
}

#[stdlib_func]
pub async fn op_subdivide(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let factor = read_int(executor, stack_idx, -2, "factor")?.max(1) as usize;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if !mesh.tris.is_empty() {
            let mut surface = mesh_to_indexed_surface(mesh);
            for _ in 1..factor {
                surface = subdivide_indexed_surface(&surface);
            }
            let (lins, tris) =
                build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
            mesh.lins = lins;
            mesh.tris = tris;
        } else if !mesh.lins.is_empty() && factor > 1 {
            subdivide_line_mesh(mesh, factor);
        }
        mesh.debug_assert_consistent_topology();
    })
    .await?;

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_tesselated(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let depth = read_int(executor, stack_idx, -2, "depth")?.max(0) as usize;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if !mesh.tris.is_empty() {
            let mut surface = mesh_to_indexed_surface(mesh);
            for _ in 0..depth {
                surface = subdivide_indexed_surface(&surface);
            }
            let (lins, tris) =
                build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
            mesh.lins = lins;
            mesh.tris = tris;
        }
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_extrude(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let delta = read_float3(executor, stack_idx, -2, "delta")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let mut extrude_error = None;

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if extrude_error.is_some() {
            return;
        }
        if mesh.tris.is_empty() {
            extrude_error = Some(ExecutorError::invalid_operation(
                "can only extrude meshes that have triangles",
            ));
            return;
        }
        if mesh.lins.iter().any(|lin| lin.is_dom_sib && lin.inv >= 0) {
            extrude_error = Some(ExecutorError::invalid_operation(
                "cannot extrude meshes that have standalone line loops; try upranking first",
            ));
            return;
        }

        let surface = mesh_to_indexed_surface(mesh);
        let base_vertex_count = surface.vertices.len();
        let mut vertices = surface.vertices.clone();
        vertices.extend(surface.vertices.iter().map(|vertex| SurfaceVertex {
            pos: vertex.pos + delta,
            col: vertex.col,
            uv: vertex.uv,
        }));

        let mut faces =
            Vec::with_capacity(surface.faces.len() * 2 + surface.boundary_edges.len() * 2);
        faces.extend(surface.faces.iter().copied());
        faces.extend(surface.faces.iter().map(|&[a, b, c]| {
            [
                c + base_vertex_count,
                b + base_vertex_count,
                a + base_vertex_count,
            ]
        }));
        for &(a, b) in surface.boundary_edges.keys() {
            faces.push([b, a, a + base_vertex_count]);
            faces.push([b, a + base_vertex_count, b + base_vertex_count]);
        }

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &HashMap::new());
        mesh.lins = lins;
        mesh.tris = tris;
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    if let Some(err) = extrude_error {
        return Err(err);
    }

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_revolve(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let rotation = read_float3(executor, stack_idx, -2, "rotation")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let angle = rotation.len();
    let axis = if angle <= 1e-6 {
        Float3::Y
    } else {
        rotation / angle
    };
    let full_turn = angle >= std::f32::consts::TAU - 1e-3;
    let steps = ((angle.abs() / (std::f32::consts::TAU / 24.0)).ceil() as usize).max(1);
    let mut revolve_error = None;

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if revolve_error.is_some() {
            return;
        }
        if mesh.lins.is_empty() || !mesh.tris.is_empty() {
            revolve_error = Some(ExecutorError::invalid_operation(
                "can only revolve meshes that are line meshes",
            ));
            return;
        }

        let profile = mesh_to_indexed_lines(mesh);
        let ring_count = if full_turn { steps } else { steps + 1 };
        let mut vertices = Vec::with_capacity(profile.vertices.len() * ring_count);
        for step in 0..ring_count {
            let theta = angle * step as f32 / steps as f32;
            for vertex in &profile.vertices {
                vertices.push(SurfaceVertex {
                    pos: rotate_about_axis(vertex.pos, axis, theta),
                    col: vertex.col,
                    uv: vertex.uv,
                });
            }
        }

        let mut faces = Vec::with_capacity(profile.segments.len() * steps * 2);
        let ring_stride = profile.vertices.len();
        let ring_vertex = |step: usize, vertex: usize| step * ring_stride + vertex;
        for &[a, b] in &profile.segments {
            for step in 0..steps {
                let next = if full_turn {
                    (step + 1) % ring_count
                } else {
                    step + 1
                };
                let a0 = ring_vertex(step, a);
                let b0 = ring_vertex(step, b);
                let a1 = ring_vertex(next, a);
                let b1 = ring_vertex(next, b);
                faces.push([a0, b0, b1]);
                faces.push([a0, b1, a1]);
            }
        }

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &HashMap::new());
        mesh.lins = lins;
        mesh.tris = tris;
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    if let Some(err) = revolve_error {
        return Err(err);
    }

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_centered(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let at = read_float3(executor, stack_idx, -3, "at")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let center = tree_center(&view).unwrap_or(Float3::ZERO);
    let delta = (at - center) * level;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| p + delta)
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_to_side(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -6, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let side = read_float3(executor, stack_idx, -5, "dir")?;
    let camera = read_camera_basis_or_default(executor, stack_idx, -4, "camera").await?;
    let buffer = crate::read_float(executor, stack_idx, -3, "buffer")? as f32;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let delta =
        camera_space_placement_delta(&view, camera, side, buffer).unwrap_or(Float3::ZERO) * level;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| p + delta)
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_to_corner(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -6, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let mut side = read_float3(executor, stack_idx, -5, "dir")?;
    if side.x == 0.0 {
        side.x = 1.0;
    }
    if side.y == 0.0 {
        side.y = 1.0;
    }
    let camera = read_camera_basis_or_default(executor, stack_idx, -4, "camera").await?;
    let buffer = crate::read_float(executor, stack_idx, -3, "buffer")? as f32;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let delta =
        camera_space_placement_delta(&view, camera, side, buffer).unwrap_or(Float3::ZERO) * level;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| p + delta)
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_matched_edge(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let reference = read_mesh_tree_arg(executor, stack_idx, -4, "ref").await?;
    let dir = read_float3(executor, stack_idx, -3, "dir")?.normalize();
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let our = extremal_point(&view, dir).unwrap_or(Float3::ZERO).dot(dir);
    let their = extremal_point(&reference, dir)
        .unwrap_or(Float3::ZERO)
        .dot(dir);
    let delta = dir * (their - our) * level;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| p + delta)
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_next_to(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -6, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let reference = read_mesh_tree_arg(executor, stack_idx, -5, "ref").await?;
    let dir = read_float3(executor, stack_idx, -4, "dir")?.normalize();
    let buffer = crate::read_float(executor, stack_idx, -3, "buffer")? as f32;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let our_center = tree_center(&view).unwrap_or(Float3::ZERO);
    let ref_center = tree_center(&reference).unwrap_or(Float3::ZERO);
    let our_face = extremal_point(&view, -dir).unwrap_or(our_center).dot(dir);
    let ref_face = extremal_point(&reference, dir)
        .unwrap_or(ref_center)
        .dot(dir);
    let orth = (ref_center - our_center) - dir * (ref_center - our_center).dot(dir);
    let delta = (dir * (ref_face - our_face + buffer) + orth) * level;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| p + delta)
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_projected(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let screen = read_mesh_tree_arg(executor, stack_idx, -4, "screen").await?;
    let ray = read_float3(executor, stack_idx, -3, "ray")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let ray = if ray.len_sq() <= 1e-12 {
        Float3::Z
    } else {
        ray.normalize()
    };
    let screen_tris: Vec<_> = screen
        .iter()
        .flat_map(|mesh| {
            mesh.tris
                .iter()
                .map(|tri| (tri.a.pos, tri.b.pos, tri.c.pos))
        })
        .collect();

    let cast = |point: Float3| {
        screen_tris
            .iter()
            .filter_map(|&(a, b, c)| {
                ray_triangle_intersection(point, ray, a, b, c).map(|t| (t, point + ray * t))
            })
            .min_by(|(ta, _), (tb, _)| ta.total_cmp(tb))
            .map(|(_, hit)| hit)
            .unwrap_or(point)
    };

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        blend_mesh_positions(mesh, level, cast)
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_in_space(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -7, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let axis_center = read_float3(executor, stack_idx, -6, "axis_center")?;
    let x_unit = read_float3(executor, stack_idx, -5, "x_unit")?;
    let y_unit = read_float3(executor, stack_idx, -4, "y_unit")?;
    let z_unit = read_float3(executor, stack_idx, -3, "z_unit")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        blend_mesh_positions(mesh, level, |p| {
            axis_center + x_unit * p.x + y_unit * p.y + z_unit * p.z
        });
        for dot in &mut mesh.dots {
            let target = transform_hint_normal(dot.norm, x_unit, y_unit, z_unit);
            dot.norm = dot.norm.lerp(target, level);
        }
        for lin in &mut mesh.lins {
            let target = transform_hint_normal(lin.norm, x_unit, y_unit, z_unit);
            lin.norm = lin.norm.lerp(target, level);
        }
    })
    .await?;
    Ok(tree.into_value())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use geo::{
        mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
        mesh_build::mesh_ref,
        simd::{Float2, Float3, Float4},
    };

    use super::{
        BoundaryEdge, IndexedSurface, MeshTree, SurfaceVertex, build_indexed_surface, dashed_lines,
        dashed_mesh, default_lin, push_dashed_segment, recolor_mesh, subdivide_indexed_surface,
        subdivide_line_mesh,
    };

    fn square_surface() -> IndexedSurface {
        let vertices = vec![
            SurfaceVertex {
                pos: Float3::new(-1.0, -1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, -1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(-1.0, 1.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
        ];
        let faces = vec![[0, 1, 2], [0, 2, 3]];
        let boundary_edges = HashMap::from([
            (
                (0, 1),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
            (
                (1, 2),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
            (
                (2, 3),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
            (
                (3, 0),
                BoundaryEdge {
                    a_col: Float4::ONE,
                    b_col: Float4::ONE,
                    norm: Float3::Z,
                },
            ),
        ]);

        IndexedSurface {
            vertices,
            faces,
            boundary_edges,
        }
    }

    #[test]
    fn subdivide_surface_authors_consistent_boundary_topology() {
        let surface = subdivide_indexed_surface(&square_surface());
        let (lins, tris) =
            build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
        let mesh = Mesh {
            dots: vec![],
            lins,
            tris,
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert_eq!(mesh.tris.len(), 8);
        assert_eq!(mesh.lins.len(), 8);
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn subdivide_lines_remaps_endpoint_dot_backrefs() {
        let mut mesh = Mesh {
            dots: vec![],
            lins: vec![default_lin(
                Float3::new(-2.0, -2.0, 0.0),
                Float3::new(-1.0, -2.0, 0.0),
                Float3::Z,
            )],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        mesh.normalize_line_dot_topology();

        subdivide_line_mesh(&mut mesh, 3);

        assert_eq!(mesh.lins.len(), 6);
        assert_eq!(mesh.dots.len(), 4);
        assert_eq!(mesh.dots[0].inv, mesh_ref(0));
        assert_eq!(mesh.dots[1].inv, mesh_ref(2));
        assert_eq!(mesh.dots[2].inv, mesh_ref(3));
        assert_eq!(mesh.dots[3].inv, mesh_ref(5));
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn revolved_strip_authors_consistent_boundary_topology() {
        let vertices = vec![
            SurfaceVertex {
                pos: Float3::new(0.0, 0.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 0.0, 0.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(0.0, 0.0, 1.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 0.0, 1.0),
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
        ];
        let faces = vec![[0, 1, 3], [0, 3, 2]];
        let (lins, tris) = build_indexed_surface(&vertices, &faces, &HashMap::new());
        let mesh = Mesh {
            dots: vec![],
            lins,
            tris,
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert!(!mesh.lins.is_empty());
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn recolor_mesh_updates_dots_lines_and_tris() {
        let start = Float4::new(0.2, 0.3, 0.4, 1.0);
        let target = Float4::new(0.8, 0.1, 0.6, 1.0);
        let mut mesh = Mesh {
            dots: vec![Dot {
                pos: Float3::ZERO,
                norm: Float3::Z,
                col: start,
                inv: -1,
                is_dom_sib: false,
            }],
            lins: vec![Lin {
                a: LinVertex {
                    pos: Float3::ZERO,
                    col: start,
                },
                b: LinVertex {
                    pos: Float3::X,
                    col: start,
                },
                norm: Float3::Z,
                prev: -1,
                next: -1,
                inv: -1,
                is_dom_sib: false,
            }],
            tris: vec![Tri {
                a: TriVertex {
                    pos: Float3::ZERO,
                    col: start,
                    uv: Float2::ZERO,
                },
                b: TriVertex {
                    pos: Float3::X,
                    col: start,
                    uv: Float2::ZERO,
                },
                c: TriVertex {
                    pos: Float3::Y,
                    col: start,
                    uv: Float2::ZERO,
                },
                ab: -1,
                bc: -1,
                ca: -1,
                is_dom_sib: false,
            }],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        recolor_mesh(&mut mesh, target, 1.0);

        let approx_eq =
            |lhs: Float4, rhs: Float4| (lhs - rhs).to_array().into_iter().all(|x| x.abs() < 1e-6);

        assert!(approx_eq(mesh.dots[0].col, target));
        assert!(approx_eq(mesh.lins[0].a.col, target));
        assert!(approx_eq(mesh.lins[0].b.col, target));
        assert!(approx_eq(mesh.tris[0].a.col, target));
        assert!(approx_eq(mesh.tris[0].b.col, target));
        assert!(approx_eq(mesh.tris[0].c.col, target));
    }

    #[test]
    fn dashed_lines_keep_continuous_visible_piece_links() {
        let mut first = default_lin(Float3::ZERO, Float3::X, Float3::Z);
        let mut second = default_lin(Float3::X, Float3::new(2.0, 0.0, 0.0), Float3::Z);
        first.next = 1;
        second.prev = 0;

        let dashed = dashed_lines(&[first, second], 1.5, 0.5, 0.0);
        let mesh = Mesh {
            dots: vec![],
            lins: dashed.clone(),
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert_eq!(dashed.len(), 2);
        assert_eq!(dashed[0].next, 1);
        assert_eq!(dashed[1].prev, 0);
        assert_eq!(dashed[0].a.pos, Float3::ZERO);
        assert_eq!(dashed[0].b.pos, Float3::X);
        assert_eq!(dashed[1].a.pos, Float3::X);
        assert_eq!(dashed[1].b.pos, Float3::new(1.5, 0.0, 0.0));
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn dashed_segments_only_link_when_endpoints_match() {
        let mut out = Vec::new();
        let mut current_piece_last = None;
        let first = default_lin(Float3::ZERO, Float3::X, Float3::Z);
        let second = default_lin(
            Float3::new(2.0, 0.0, 0.0),
            Float3::new(3.0, 0.0, 0.0),
            Float3::Z,
        );

        push_dashed_segment(&mut out, &mut current_piece_last, &first, 0.0, 1.0);
        push_dashed_segment(&mut out, &mut current_piece_last, &second, 0.0, 1.0);

        assert_eq!(out[0].next, -1);
        assert_eq!(out[1].prev, -1);
    }

    #[test]
    fn dashed_segments_snap_near_endpoints_before_linking() {
        let mut out = Vec::new();
        let mut current_piece_last = None;
        let first = default_lin(Float3::ZERO, Float3::X, Float3::Z);
        let second = default_lin(Float3::X, Float3::new(2.0, 0.0, 0.0), Float3::Z);

        push_dashed_segment(&mut out, &mut current_piece_last, &first, 0.0, 0.99999994);
        push_dashed_segment(&mut out, &mut current_piece_last, &second, 0.0, 0.5);

        assert_eq!(out[0].b.pos, Float3::X);
        assert_eq!(out[0].next, 1);
        assert_eq!(out[1].prev, 0);
        assert_eq!(out[1].a.pos, Float3::X);
    }

    #[test]
    fn dashed_surface_splits_fill_and_stroke_meshes() {
        let surface = square_surface();
        let (lins, tris) =
            build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
        let mesh = Mesh {
            dots: vec![],
            lins,
            tris,
            uniform: Uniforms::default(),
            tag: vec![7],
            version: Mesh::fresh_version(),
        };
        let MeshTree::List(children) = dashed_mesh(&mesh, 0.6, 0.4, 0.0) else {
            panic!("expected dashed surface to split into child meshes");
        };
        assert_eq!(children.len(), 2);

        let MeshTree::Mesh(base) = &children[0] else {
            panic!("expected base mesh");
        };
        let MeshTree::Mesh(stroke) = &children[1] else {
            panic!("expected dashed stroke mesh");
        };

        assert_eq!(base.tris.len(), mesh.tris.len());
        assert!(base.lins.is_empty());
        assert!(stroke.tris.is_empty());
        assert!(stroke.lins.len() > mesh.lins.len());
        assert_eq!(base.tag, mesh.tag);
        assert_eq!(stroke.tag, mesh.tag);
        assert!(base.has_consistent_topology());
        assert!(stroke.has_consistent_topology());
    }
}
