mod bend;
mod camera;
mod trans;
mod write;

use std::sync::Arc;

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh::{Mesh, Uniforms},
    simd::Float3,
};
use stdlib_macros::stdlib_func;

use super::helpers::{
    collapse_mesh, decompose_mesh_tree, fade_start_mesh, list_value, map_mesh_tree,
    materialize_live_value, mesh_center, pack_mesh_tree, prefer_single_mesh_tree_value,
};

#[stdlib_func]
pub async fn grow_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let start_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let start = materialize_live_value(executor, &start_value).await?;
    let destination = materialize_live_value(executor, &destination_value).await?;
    let decomposition = decompose_mesh_tree(&start, &destination)?;
    let insert_center = mesh_group_center(&decomposition.insert);
    let delete_center = mesh_group_center(&decomposition.delete);
    let prefer_single = prefer_single_mesh_tree_value(&start, &destination);

    let start_tree = pack_mesh_tree(
        decomposition
            .insert
            .iter()
            .map(|mesh| Arc::new(collapse_mesh(mesh, insert_center)))
            .chain(decomposition.delete.iter().cloned())
            .chain(decomposition.constant.iter().cloned())
            .collect(),
        prefer_single,
    );
    let end_tree = pack_mesh_tree(
        decomposition
            .insert
            .iter()
            .cloned()
            .chain(
                decomposition
                    .delete
                    .iter()
                    .map(|mesh| Arc::new(collapse_mesh(mesh, delete_center))),
            )
            .chain(decomposition.constant.iter().cloned())
            .collect(),
        prefer_single,
    );

    Ok(embed_triplet(start_tree, end_tree, Value::Nil))
}

#[stdlib_func]
pub async fn fade_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let start_value = executor.state.stack(stack_idx).read_at(-3).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let start = materialize_live_value(executor, &start_value).await?;
    let destination = materialize_live_value(executor, &destination_value).await?;
    let delta = read_vec3_value(executor.state.stack(stack_idx).read_at(-1).clone(), "delta")?;
    let decomposition = decompose_mesh_tree(&start, &destination)?;
    let prefer_single = prefer_single_mesh_tree_value(&start, &destination);

    let start_tree = pack_mesh_tree(
        decomposition
            .insert
            .iter()
            .map(|mesh| Arc::new(fade_start_mesh(mesh, delta)))
            .chain(decomposition.delete.iter().cloned())
            .chain(decomposition.constant.iter().cloned())
            .collect(),
        prefer_single,
    );
    let end_tree = pack_mesh_tree(
        decomposition
            .insert
            .iter()
            .cloned()
            .chain(
                decomposition
                    .delete
                    .iter()
                    .map(|mesh| Arc::new(fade_start_mesh(mesh, delta))),
            )
            .chain(decomposition.constant.iter().cloned())
            .collect(),
        prefer_single,
    );

    Ok(embed_triplet(start_tree, end_tree, Value::Nil))
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

pub(super) fn embed_triplet(start: Value, destination: Value, state: Value) -> Value {
    list_value([start, destination, state])
}

fn mesh_group_center(meshes: &[Arc<Mesh>]) -> Float3 {
    let mut min = None::<Float3>;
    let mut max = None::<Float3>;
    for mesh in meshes {
        let c = mesh_center(mesh);
        min = Some(
            min.map(|m| Float3::new(m.x.min(c.x), m.y.min(c.y), m.z.min(c.z)))
                .unwrap_or(c),
        );
        max = Some(
            max.map(|m| Float3::new(m.x.max(c.x), m.y.max(c.y), m.z.max(c.z)))
                .unwrap_or(c),
        );
    }
    (min.unwrap_or(Float3::ZERO) + max.unwrap_or(Float3::ZERO)) / 2.0
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
        stroke_miter_radius_scale: start.stroke_miter_radius_scale
            + (end.stroke_miter_radius_scale - start.stroke_miter_radius_scale) * t,
        stroke_radius: start.stroke_radius + (end.stroke_radius - start.stroke_radius) * t,
        dot_radius: start.dot_radius + (end.dot_radius - start.dot_radius) * t,
        dot_vertex_count: ((1.0 - t) * start.dot_vertex_count as f32
            + t * end.dot_vertex_count as f32) as u16,
        smooth: if t < 0.5 { start.smooth } else { end.smooth },
        gloss: start.gloss + (end.gloss - start.gloss) * t,
        ..end.clone()
    }
}
