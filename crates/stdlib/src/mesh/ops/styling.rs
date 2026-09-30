use std::collections::HashSet;

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh::{DEFAULT_DOT_RADIUS, Dot, Mesh},
    simd::{Float3, Float4},
};
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::*;

use super::*;

#[stdlib_func(sync = op_fade_sync)]
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

fn op_fade_sync(executor: &mut Executor, stack_idx: usize) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -3)?;
    if !has_no_tag_filter(executor, stack_idx, -1) {
        return None;
    }
    Some((|| {
        let alpha = crate::read_float(executor, stack_idx, -2, "opacity")?;
        if (alpha - 1.0).abs() > 1e-12 {
            tree.for_each_mut(&mut |mesh| mesh.uniform.alpha *= alpha);
        }
        Ok(tree.into_value())
    })())
}

fn restroke_mesh(mesh: &mut Mesh, color: Float4, level: f32) {
    edit_keeping_positions(&mut mesh.lins, |lins| {
        for lin in lins {
            lin.a.col = lin.a.col.lerp(color, level);
            lin.b.col = lin.b.col.lerp(color, level);
        }
    });
}

fn refill_mesh(mesh: &mut Mesh, color: Float4, level: f32) {
    edit_keeping_positions(&mut mesh.tris, |tris| {
        for tri in tris {
            tri.a.col = tri.a.col.lerp(color, level);
            tri.b.col = tri.b.col.lerp(color, level);
            tri.c.col = tri.c.col.lerp(color, level);
        }
    });
}

#[stdlib_func(sync = op_restroke_sync)]
pub async fn op_restroke(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let color = read_float4(executor, stack_idx, -4, "color").await?;
    let stroke_radius = match executor.state.stack(stack_idx).read_at(-3) {
        Value::Nil => None,
        value => match value.clone().elide_cached_wrappers_rec() {
            Value::Nil => None,
            _ => Some(crate::read_float(executor, stack_idx, -3, "stroke_width")? as f32),
        },
    }
    .map(|stroke_width| stroke_width.max(0.0));
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if let Some(stroke_radius) = stroke_radius {
        tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
            mesh.uniform.stroke_radius = stroke_radius;
        })
        .await?;
    }
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        restroke_mesh(mesh, color, level)
    })
    .await?;
    Ok(tree.into_value())
}

fn op_restroke_sync(
    executor: &mut Executor,
    stack_idx: usize,
) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -5)?;
    let color = try_read_float4_arg(executor, stack_idx, -4)?;
    if !has_no_tag_filter(executor, stack_idx, -2) {
        return None;
    }
    let width = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_cached_wrappers_rec();
    let stroke_radius = match width {
        Value::Nil => None,
        Value::Integer(value) => Some((value as f32).max(0.0)),
        Value::Float(value) => Some((value as f32).max(0.0)),
        _ => return None,
    };
    Some((|| {
        let level = read_level(executor, stack_idx, -1, "level")?;
        if let Some(radius) = stroke_radius {
            tree.for_each_mut(&mut |mesh| mesh.uniform.stroke_radius = radius);
        }
        if level > 0.0 {
            tree.for_each_mut(&mut |mesh| restroke_mesh(mesh, color, level));
        }
        Ok(tree.into_value())
    })())
}

#[stdlib_func(sync = op_refill_sync)]
pub async fn op_refill(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let color = read_float4(executor, stack_idx, -3, "color").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        refill_mesh(mesh, color, level)
    })
    .await?;
    Ok(tree.into_value())
}

fn op_refill_sync(
    executor: &mut Executor,
    stack_idx: usize,
) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -4)?;
    if !has_no_tag_filter(executor, stack_idx, -2) {
        return None;
    }
    let level = match read_level(executor, stack_idx, -1, "level") {
        Ok(level) => level,
        Err(error) => return Some(Err(error)),
    };
    if level > 0.0 {
        let color = try_read_float4_arg(executor, stack_idx, -3)?;
        tree.for_each_mut(&mut |mesh| refill_mesh(mesh, color, level));
    }
    Some(Ok(tree.into_value()))
}

fn op_redot_sync(
    executor: &mut Executor,
    stack_idx: usize,
) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -4)?;
    if !has_no_tag_filter(executor, stack_idx, -2) {
        return None;
    }
    let level = match read_level(executor, stack_idx, -1, "level") {
        Ok(level) => level,
        Err(error) => return Some(Err(error)),
    };
    if level > 0.0 {
        let color = try_read_float4_arg(executor, stack_idx, -3)?;
        tree.for_each_mut(&mut |mesh| redot_mesh(mesh, color, level));
    }
    Some(Ok(tree.into_value()))
}

/// what `dot{}` does to one mesh: every line endpoint gets a dot, dots take
/// the colour, and topology dots (authored with radius 0) become visible
fn redot_mesh(mesh: &mut Mesh, color: Float4, level: f32) {
    add_line_vertex_dots(mesh);
    for dot in &mut mesh.dots {
        dot.col = dot.col.lerp(color, level);
    }
    let radius = mesh.uniform.dot_radius;
    let visible = if radius > 0.0 {
        radius
    } else {
        DEFAULT_DOT_RADIUS
    };
    mesh.uniform.dot_radius = radius + (visible - radius) * level;
}

#[stdlib_func(sync = op_redot_sync)]
pub async fn op_redot(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let color = read_float4(executor, stack_idx, -3, "color").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        redot_mesh(mesh, color, level)
    })
    .await?;
    Ok(tree.into_value())
}

/// give every line vertex a dot. endpoints already carry a topology dot, so only
/// vertices joining two lines get a new standalone one, and a vertex that already
/// has a standalone dot is left alone
fn add_line_vertex_dots(mesh: &mut Mesh) {
    let point_key = |pos: Float3| [pos.x.to_bits(), pos.y.to_bits(), pos.z.to_bits()];
    let mut dotted: HashSet<_> = mesh
        .dots
        .iter()
        .filter(|dot| dot.is_dom_sib && dot.inv >= 0)
        .map(|dot| point_key(dot.pos))
        .collect();

    let vertex_dots: Vec<Dot> = mesh
        .lins
        .iter()
        .filter(|lin| lin.is_dom_sib && lin.prev >= 0 && dotted.insert(point_key(lin.a.pos)))
        .map(|lin| Dot {
            pos: lin.a.pos,
            norm: lin.norm,
            col: lin.a.col,
            inv: -1,
            is_dom_sib: true,
        })
        .collect();
    if vertex_dots.is_empty() {
        return;
    }

    mesh.dots.extend(vertex_dots);
    // pairs each new dot with its inverse sibling
    mesh.normalize_line_dot_topology();
    mesh.debug_assert_consistent_topology();
}

fn op_recolor_sync(
    executor: &mut Executor,
    stack_idx: usize,
) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -4)?;
    if !has_no_tag_filter(executor, stack_idx, -2) {
        return None;
    }
    let level = match read_level(executor, stack_idx, -1, "level") {
        Ok(level) => level,
        Err(error) => return Some(Err(error)),
    };
    if level > 0.0 {
        let color = try_read_float4_arg(executor, stack_idx, -3)?;
        tree.for_each_mut(&mut |mesh| recolor_mesh(mesh, color, level));
    }
    Some(Ok(tree.into_value()))
}

#[stdlib_func(sync = op_recolor_sync)]
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

#[cfg(target_arch = "wasm32")]
#[stdlib_func]
pub fn op_retextured(_executor: &mut Executor, _stack_idx: usize) -> Result<Value, ExecutorError> {
    Err(ExecutorError::invalid_invocation(
        "retextured{...} is not supported in the WebAssembly runtime yet",
    ))
}

#[cfg(not(target_arch = "wasm32"))]
#[stdlib_func]
pub async fn op_retextured(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let image = read_string(executor, stack_idx, -2, "image").await?;
    let image = resolve_file_path(executor, stack_idx, &image)?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        mesh.uniform.img = Some(geo::mesh::TextureSource::File(image.clone()));
    })
    .await?;
    Ok(tree.into_value())
}

fn op_with_zindex_sync(
    executor: &mut Executor,
    stack_idx: usize,
) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -3)?;
    if !has_no_tag_filter(executor, stack_idx, -1) {
        return None;
    }
    Some((|| {
        let z_index = read_int(executor, stack_idx, -2, "z_index")?;
        tree.for_each_mut(&mut |mesh| mesh.uniform.z_index = z_index as i32);
        Ok(tree.into_value())
    })())
}

#[stdlib_func(sync = op_with_zindex_sync)]
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

fn op_gloss_sync(
    executor: &mut Executor,
    stack_idx: usize,
) -> Option<Result<Value, ExecutorError>> {
    let mut tree = try_read_mesh_tree_arg(executor, stack_idx, -3)?;
    if !has_no_tag_filter(executor, stack_idx, -1) {
        return None;
    }
    Some((|| {
        let gloss = crate::read_float(executor, stack_idx, -2, "gloss")? as f32;
        tree.for_each_mut(&mut |mesh| mesh.uniform.gloss = gloss.max(0.0));
        Ok(tree.into_value())
    })())
}

#[stdlib_func(sync = op_gloss_sync)]
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
