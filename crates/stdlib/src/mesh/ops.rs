use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use executor::{
    camera::{CameraBasis, DEFAULT_CAMERA_ASPECT, DEFAULT_CAMERA_FOV, parse_camera_arg},
    error::ExecutorError,
    executor::Executor,
    heap::with_heap,
    value::Value,
};
use geo::{mesh::Mesh, simd::Float3};
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

#[stdlib_func]
pub async fn op_shift(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let delta = read_float3(executor, stack_idx, -2, "delta")?;
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
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        let center = mesh_center(mesh).unwrap_or(Float3::ZERO);
        transform_mesh_positions(mesh, |p| center + (p - center) * factor);
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_rotate(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let angle = crate::read_float(executor, stack_idx, -4, "radians")? as f32;
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
pub async fn op_fade(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let alpha = crate::read_float(executor, stack_idx, -2, "opacity")?;
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
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let color = read_float4(executor, stack_idx, -3, "color")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
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
    let color = read_float4(executor, stack_idx, -3, "color")?;
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
    let color = read_float4(executor, stack_idx, -3, "color")?;
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
    let image = read_string(executor, stack_idx, -2, "image")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        mesh.uniform.img = Some(image.clone().into());
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
    read_mesh_tree_arg(executor, stack_idx, -1, "target")
        .await
        .map(MeshTree::into_value)
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
                    let mesh = Arc::make_mut(arc);
                    for dot in &mut mesh.dots {
                        let original = dot.pos;
                        let mapped = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(original)], "f")
                                .await?,
                            "f",
                        )?;
                        dot.pos = original.lerp(mapped, level);
                    }
                    for lin in &mut mesh.lins {
                        let original = lin.a.pos;
                        let mapped = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(original)], "f")
                                .await?,
                            "f",
                        )?;
                        lin.a.pos = original.lerp(mapped, level);

                        let original = lin.b.pos;
                        let mapped = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(original)], "f")
                                .await?,
                            "f",
                        )?;
                        lin.b.pos = original.lerp(mapped, level);
                    }
                    for tri in &mut mesh.tris {
                        let original = tri.a.pos;
                        let mapped = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(original)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.a.pos = original.lerp(mapped, level);

                        let original = tri.b.pos;
                        let mapped = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(original)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.b.pos = original.lerp(mapped, level);

                        let original = tri.c.pos;
                        let mapped = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(original)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.c.pos = original.lerp(mapped, level);
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
                    let mesh = Arc::make_mut(arc);
                    for dot in &mut mesh.dots {
                        let original = dot.col;
                        let mapped = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(dot.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        dot.col = original.lerp(mapped, level);
                    }
                    for lin in &mut mesh.lins {
                        let original = lin.a.col;
                        let mapped = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(lin.a.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        lin.a.col = original.lerp(mapped, level);

                        let original = lin.b.col;
                        let mapped = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(lin.b.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        lin.b.col = original.lerp(mapped, level);
                    }
                    for tri in &mut mesh.tris {
                        let original = tri.a.col;
                        let mapped = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.a.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.a.col = original.lerp(mapped, level);

                        let original = tri.b.col;
                        let mapped = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.b.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.b.col = original.lerp(mapped, level);

                        let original = tri.c.col;
                        let mapped = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.c.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.c.col = original.lerp(mapped, level);
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
                    let mesh = Arc::make_mut(arc);
                    for tri in &mut mesh.tris {
                        let original = tri.a.uv;
                        let mapped = float2_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.a.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.a.uv = original.lerp(mapped, level);

                        let original = tri.b.uv;
                        let mapped = float2_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.b.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.b.uv = original.lerp(mapped, level);

                        let original = tri.c.uv;
                        let mapped = float2_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.c.pos)], "f")
                                .await?,
                            "f",
                        )?;
                        tri.c.uv = original.lerp(mapped, level);
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
                    let mesh = Arc::make_mut(arc);
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
        debug_assert!(mesh.has_consistent_topology());
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
        debug_assert!(mesh.has_consistent_topology());
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
            let mut lins = Vec::with_capacity(mesh.lins.len() * factor);
            for (lin_idx, lin) in mesh.lins.iter().enumerate() {
                let base = lin_idx * factor;
                for i in 0..factor {
                    let u = i as f32 / factor as f32;
                    let v = (i + 1) as f32 / factor as f32;
                    let a = lin.a.pos.lerp(lin.b.pos, u);
                    let b = lin.a.pos.lerp(lin.b.pos, v);
                    let mut out = default_lin(a, b, lin.norm);
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
                    out.anti = if lin.anti >= 0 {
                        (lin.anti as usize * factor + i) as i32
                    } else {
                        lin.anti
                    };
                    out.is_dom_sib = lin.is_dom_sib;
                    lins.push(out);
                }
            }
            mesh.lins = lins;
        }
        debug_assert!(mesh.has_consistent_topology());
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
        debug_assert!(mesh.has_consistent_topology());
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
        if mesh.lins.iter().any(|lin| lin.inv == -1) {
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
        debug_assert!(mesh.has_consistent_topology());
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
        debug_assert!(mesh.has_consistent_topology());
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
    let camera = parse_camera_arg(executor, stack_idx, -4, "camera")
        .await?
        .basis();
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
    let camera = parse_camera_arg(executor, stack_idx, -4, "camera")
        .await?
        .basis();
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
        })
    })
    .await?;
    Ok(tree.into_value())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use geo::{
        mesh::{Mesh, Uniforms},
        simd::{Float2, Float3, Float4},
    };

    use super::{
        BoundaryEdge, IndexedSurface, SurfaceVertex, build_indexed_surface,
        subdivide_indexed_surface,
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
        };

        assert_eq!(mesh.tris.len(), 8);
        assert_eq!(mesh.lins.len(), 8);
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
        };

        assert!(!mesh.lins.is_empty());
        assert!(mesh.has_consistent_topology());
    }
}
