use executor::{
    camera::{CameraBasis, DEFAULT_CAMERA_FOV, initial_camera_snapshot, parse_camera_arg},
    error::ExecutorError,
    executor::Executor,
    value::Value,
};
use geo::simd::Float3;
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::*;

use super::*;

fn viewport_half_extents(depth: f32, aspect_ratio: f32) -> (f32, f32) {
    let depth = depth.max(executor::camera::MIN_CAMERA_NEAR);
    let tan_half_fov = (DEFAULT_CAMERA_FOV * 0.5).tan().max(0.05);
    (depth * tan_half_fov * aspect_ratio, depth * tan_half_fov)
}

fn camera_space_placement_delta(
    tree: &MeshTree,
    camera: CameraBasis,
    aspect_ratio: f32,
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
            let (half_width, half_height) = viewport_half_extents(z, aspect_ratio);
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
        Ok(initial_camera_snapshot().basis())
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

fn camera_oriented_basis(camera: CameraBasis) -> (Float3, Float3, Float3) {
    let z_unit = -camera.forward;
    let y_projected = camera.up - z_unit * camera.up.dot(z_unit);
    let y_hint = if y_projected.len_sq() > 1e-12 {
        y_projected.normalize()
    } else {
        polygon_basis(z_unit).1
    };
    let x_unit = y_hint.cross(z_unit).normalize();
    let y_unit = z_unit.cross(x_unit).normalize();
    (x_unit, y_unit, z_unit)
}

#[stdlib_func(sync = op_shift_sync)]
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

fn op_shift_sync(executor: &mut Executor, stack_idx: usize) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -3)?;
    if !has_no_tag_filter(executor, stack_idx, -1) {
        return None;
    }
    Some((|| {
        let delta = read_float3(executor, stack_idx, -2, "delta")?;
        if delta.len_sq() > 1e-12 {
            tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
        }
        Ok(tree.into_value())
    })())
}

#[stdlib_func]
pub async fn op_scale(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let factor = read_scale_factor(executor, stack_idx, -2, "factor")?;
    if (factor - Float3::splat(1.0)).len_sq() <= 1e-12 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let Some(center) = affected_tree_center(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
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
        Value::Nil => {
            let Some(center) = affected_tree_center(executor, &tree, filter.as_ref()).await? else {
                return Ok(tree.into_value());
            };
            center
        }
        value => float3_from_value(value, "pivot")?,
    };
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| pivot + rotate_about_axis(p - pivot, axis, angle));
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
pub async fn op_orient_to_camera(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let camera = parse_camera_arg(executor, stack_idx, -3, "camera")
        .await?
        .basis();
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(center) = affected_tree_center(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let (x_unit, y_unit, z_unit) = camera_oriented_basis(camera);

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        blend_mesh_positions(mesh, level, |point| {
            let rel = point - center;
            center + x_unit * rel.x + y_unit * rel.y + z_unit * rel.z
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

#[stdlib_func(sync = op_centered_sync)]
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
    let Some(center) = affected_tree_center(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let delta = (at - center) * level;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        transform_mesh_positions(mesh, |p| p + delta)
    })
    .await?;
    Ok(tree.into_value())
}

fn op_centered_sync(
    executor: &mut Executor,
    stack_idx: usize,
) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -4)?;
    if !has_no_tag_filter(executor, stack_idx, -2) {
        return None;
    }
    Some((|| {
        let level = read_level(executor, stack_idx, -1, "level")?;
        if level > 0.0 {
            let at = read_float3(executor, stack_idx, -3, "at")?;
            if let Some(center) = tree_center(&tree) {
                let delta = (at - center) * level;
                tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
            }
        }
        Ok(tree.into_value())
    })())
}

#[stdlib_func]
pub async fn op_to_side(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -6, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let side = read_float3(executor, stack_idx, -5, "dir")?;
    let buffer = crate::read_float(executor, stack_idx, -4, "buffer")? as f32;
    let camera = read_camera_basis_or_default(executor, stack_idx, -3, "camera").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let delta = camera_space_placement_delta(&view, camera, executor.aspect_ratio(), side, buffer)
        .unwrap_or(Float3::ZERO)
        * level;
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
    let buffer = crate::read_float(executor, stack_idx, -4, "buffer")? as f32;
    let camera = read_camera_basis_or_default(executor, stack_idx, -3, "camera").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let Some(view) = filtered_tree_view(executor, &tree, filter.as_ref()).await? else {
        return Ok(tree.into_value());
    };
    let delta = camera_space_placement_delta(&view, camera, executor.aspect_ratio(), side, buffer)
        .unwrap_or(Float3::ZERO)
        * level;
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
