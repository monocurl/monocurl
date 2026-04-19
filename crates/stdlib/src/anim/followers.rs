use std::collections::HashMap;

use executor::{error::ExecutorError, executor::Executor, state::LeaderKind, value::Value};
use geo::simd::Float3;
use stdlib_macros::stdlib_func;

use super::helpers::{
    build_lerp, collapse_mesh, fade_start_mesh, flatten_mesh_leaves, follower_value, leader_value,
    map_mesh_tree, mesh_center, mesh_tag_map, progression_from, read_time, rebuild_mesh_value_like,
    rebuild_mesh_value_like_by_tag, replace_follower, resolve_targets, targets_to_value,
    write_start_mesh,
};

#[stdlib_func]
pub async fn grow_anim(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-3).clone();
    let time = read_time(executor, stack_idx, -2)?;
    let rate = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Mesh))?;

    for target in &targets {
        let target_value = leader_value(target)?;
        let center = {
            let mut leaves = Vec::new();
            flatten_mesh_leaves(&target_value, &mut leaves)?;
            let mut min = None::<Float3>;
            let mut max = None::<Float3>;
            for leaf in leaves {
                let c = mesh_center(&leaf);
                min = Some(min.map(|m| Float3::new(m.x.min(c.x), m.y.min(c.y), m.z.min(c.z))).unwrap_or(c));
                max = Some(max.map(|m| Float3::new(m.x.max(c.x), m.y.max(c.y), m.z.max(c.z))).unwrap_or(c));
            }
            (min.unwrap_or(Float3::ZERO) + max.unwrap_or(Float3::ZERO)) / 2.0
        };
        let start = map_mesh_tree(&target_value, &mut |mesh| collapse_mesh(mesh, center))?;
        replace_follower(target, start)?;
    }

    Ok(build_lerp(&targets, time, progression_from(rate)))
}

#[stdlib_func]
pub async fn fade_anim(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-4).clone();
    let time = read_time(executor, stack_idx, -3)?;
    let rate = executor.state.stack(stack_idx).read_at(-2).clone();
    let delta = match executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::List(list) if list.elements().len() == 3 => {
            let comps = list
                .elements()
                .iter()
                .map(|key| match executor::heap::with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => Ok(n as f32),
                    Value::Float(f) => Ok(f as f32),
                    other => Err(ExecutorError::type_error("number", other.type_name())),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Float3::new(comps[0], comps[1], comps[2])
        }
        other => {
            return Err(ExecutorError::type_error_for(
                "3-vector",
                other.type_name(),
                "delta",
            ));
        }
    };
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Mesh))?;

    for target in &targets {
        let target_value = leader_value(target)?;
        let start = map_mesh_tree(&target_value, &mut |mesh| fade_start_mesh(mesh, delta))?;
        replace_follower(target, start)?;
    }

    Ok(build_lerp(&targets, time, progression_from(rate)))
}

#[stdlib_func]
pub async fn write_anim(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-3).clone();
    let time = read_time(executor, stack_idx, -2)?;
    let rate = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Mesh))?;

    for target in &targets {
        let target_value = leader_value(target)?;
        let start = map_mesh_tree(&target_value, &mut write_start_mesh)?;
        replace_follower(target, start)?;
    }

    Ok(build_lerp(&targets, time, progression_from(rate)))
}

#[stdlib_func]
pub async fn transform_anim(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-4).clone();
    let time = read_time(executor, stack_idx, -3)?;
    let rate = executor.state.stack(stack_idx).read_at(-2).clone();
    let _path_arc = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Mesh))?;

    for target in &targets {
        let end = leader_value(target)?;
        let start = follower_value(target)?;
        let mut start_leaves = Vec::new();
        flatten_mesh_leaves(&start, &mut start_leaves)?;
        let rebuilt = rebuild_mesh_value_like(&start_leaves, &end)?;
        replace_follower(target, rebuilt)?;
    }

    Ok(build_lerp(&targets, time, progression_from(rate)))
}

#[stdlib_func]
pub async fn tag_transform_anim(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-4).clone();
    let time = read_time(executor, stack_idx, -3)?;
    let rate = executor.state.stack(stack_idx).read_at(-2).clone();
    let _path_arc = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Mesh))?;

    for target in &targets {
        let end = leader_value(target)?;
        let start = follower_value(target)?;
        let mut start_leaves = Vec::new();
        flatten_mesh_leaves(&start, &mut start_leaves)?;
        let by_tag: HashMap<_, _> = mesh_tag_map(&start)?;
        let rebuilt = rebuild_mesh_value_like_by_tag(&by_tag, &start_leaves, &end)?;
        replace_follower(target, rebuilt)?;
    }

    Ok(build_lerp(&targets, time, progression_from(rate)))
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
    let _ = targets_to_value(&targets);
    Ok(build_lerp(&targets, time, progression_from(rate)))
}
