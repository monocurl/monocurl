mod bend;
mod trans;
mod write;

use executor::{error::ExecutorError, executor::Executor, state::LeaderKind, value::Value};
use geo::{
    mesh::{Mesh, Uniforms},
    simd::Float3,
};
use stdlib_macros::stdlib_func;

use super::helpers::{
    build_lerp, collapse_mesh, fade_start_mesh, flatten_mesh_leaves, list_value, map_mesh_tree,
    materialize_live_value, mesh_center, progression_from, read_time, resolve_targets,
};

#[stdlib_func]
pub async fn grow_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let center = mesh_tree_center(&destination)?;
    let start = map_mesh_tree(&destination, &mut |mesh| collapse_mesh(mesh, center))?;
    Ok(embed_triplet(start, destination, Value::Nil))
}

#[stdlib_func]
pub async fn fade_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-3).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let delta = read_vec3_value(executor.state.stack(stack_idx).read_at(-1).clone(), "delta")?;
    let start = map_mesh_tree(&destination, &mut |mesh| fade_start_mesh(mesh, delta))?;
    Ok(embed_triplet(start, destination, Value::Nil))
}

#[stdlib_func]
pub async fn highlight_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-3).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let color = super::helpers::read_float4_value(
        executor.state.stack(stack_idx).read_at(-1).clone(),
        "color",
    )?;
    let highlighted = map_mesh_tree(&destination, &mut |mesh| {
        let mut mesh = mesh.clone();
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
        mesh
    })?;
    Ok(embed_triplet(highlighted, destination, Value::Nil))
}

#[stdlib_func]
pub async fn flash_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let start = map_mesh_tree(&destination, &mut write_start_mesh)?;
    Ok(embed_triplet(start, destination, Value::Nil))
}

#[stdlib_func]
pub async fn camera_lerp_anim(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-3).clone();
    let time = read_time(executor, stack_idx, -2)?;
    let rate = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Param))?;
    Ok(build_lerp(
        &targets,
        time,
        progression_from(rate),
        None,
        None,
    ))
}

pub(super) fn embed_triplet(start: Value, destination: Value, state: Value) -> Value {
    list_value([start, destination, state])
}

pub(super) fn mesh_tree_center(value: &Value) -> Result<Float3, ExecutorError> {
    let mut leaves = Vec::new();
    flatten_mesh_leaves(value, &mut leaves)?;
    let mut min = None::<Float3>;
    let mut max = None::<Float3>;
    for leaf in leaves {
        let c = mesh_center(&leaf);
        min = Some(
            min.map(|m| Float3::new(m.x.min(c.x), m.y.min(c.y), m.z.min(c.z)))
                .unwrap_or(c),
        );
        max = Some(
            max.map(|m| Float3::new(m.x.max(c.x), m.y.max(c.y), m.z.max(c.z)))
                .unwrap_or(c),
        );
    }
    Ok((min.unwrap_or(Float3::ZERO) + max.unwrap_or(Float3::ZERO)) / 2.0)
}

pub(super) fn read_vec3_value(value: Value, name: &'static str) -> Result<Float3, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::List(list) if list.elements().len() == 3 => {
            let comps = list
                .elements()
                .iter()
                .map(
                    |key| match executor::heap::with_heap(|h| h.get(key.key()).clone()) {
                        Value::Integer(n) => Ok(n as f32),
                        Value::Float(f) => Ok(f as f32),
                        other => Err(ExecutorError::type_error_for(
                            "number",
                            other.type_name(),
                            name,
                        )),
                    },
                )
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Float3::new(comps[0], comps[1], comps[2]))
        }
        other => Err(ExecutorError::type_error_for(
            "list of length 3",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn read_path_arc_value(value: Value) -> Result<Float3, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::Integer(0) => Ok(Float3::ZERO),
        Value::Float(f) if f == 0.0 => Ok(Float3::ZERO),
        other => read_vec3_value(other, "path_arc"),
    }
}

pub(super) fn write_start_mesh(mesh: &Mesh) -> Mesh {
    let mut out = mesh.clone();
    out.uniform.alpha = 0.0;
    for lin in &mut out.lins {
        lin.a.col.w = 0.0;
        lin.b.col.w = 0.0;
    }
    for tri in &mut out.tris {
        tri.a.col.w = 0.0;
        tri.b.col.w = 0.0;
        tri.c.col.w = 0.0;
    }
    for dot in &mut out.dots {
        dot.col.w = 0.0;
    }
    debug_assert!(out.has_consistent_line_links());
    out
}

pub(super) fn lerp_uniforms(start: &Uniforms, end: &Uniforms, t: f32) -> Uniforms {
    Uniforms {
        alpha: start.alpha + (end.alpha - start.alpha) * t as f64,
        ..end.clone()
    }
}
