use executor::{error::ExecutorError, executor::Executor, state::LeaderKind, value::Value};
use stdlib_macros::stdlib_func;

use super::helpers::{
    build_lerp, map_mesh_tree, progression_from, read_float4_value, read_time, replace_follower,
    resolve_targets, write_start_mesh, leader_value,
};

#[stdlib_func]
pub async fn highlight_anim(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-4).clone();
    let color = read_float4_value(executor.state.stack(stack_idx).read_at(-3).clone(), "color")?;
    let time = read_time(executor, stack_idx, -2)?;
    let rate = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Mesh))?;

    for target in &targets {
        let current = leader_value(target)?;
        let highlighted = map_mesh_tree(&current, &mut |mesh| {
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
        replace_follower(target, highlighted)?;
    }

    Ok(build_lerp(&targets, time, progression_from(rate)))
}

#[stdlib_func]
pub async fn flash_anim(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-4).clone();
    let time = read_time(executor, stack_idx, -3)?;
    let _lead = executor.state.stack(stack_idx).read_at(-2).clone();
    let _trail = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Mesh))?;

    for target in &targets {
        let current = leader_value(target)?;
        let flash = map_mesh_tree(&current, &mut write_start_mesh)?;
        replace_follower(target, flash)?;
    }

    Ok(build_lerp(&targets, time, None))
}
