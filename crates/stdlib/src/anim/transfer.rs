use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

use super::helpers::{
    build_set, empty_mesh_tree, follower_value, materialize_current_value, merge_transfer_value,
    replace_leader_and_follower, resolve_targets,
};

async fn transfer_impl(
    executor: &mut Executor,
    stack_idx: usize,
    copy: bool,
) -> Result<Value, ExecutorError> {
    let from = executor.state.stack(stack_idx).read_at(-2).clone();
    let into = executor.state.stack(stack_idx).read_at(-1).clone();
    let src_value = materialize_current_value(&from)?;
    let dst_targets = resolve_targets(executor, stack_idx, &into, None)?;

    for target in &dst_targets {
        let dst_value = follower_value(target)?;
        let merged = merge_transfer_value(dst_value, src_value.clone());
        replace_leader_and_follower(executor, stack_idx, target, merged)?;
    }

    if !copy {
        let src_targets = resolve_targets(executor, stack_idx, &from, None)?;
        for target in &src_targets {
            replace_leader_and_follower(executor, stack_idx, target, empty_mesh_tree())?;
        }
        let mut all = dst_targets.clone();
        all.extend(src_targets);
        return Ok(build_set(&all));
    }

    Ok(build_set(&dst_targets))
}

#[stdlib_func]
pub async fn transfer_anim(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    transfer_impl(executor, stack_idx, false).await
}

#[stdlib_func]
pub async fn copy_anim(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    transfer_impl(executor, stack_idx, true).await
}
