use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::simd::Float3;
use stdlib_macros::stdlib_func;

#[stdlib_func]
pub async fn bend_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    super::trans::trans_embed(executor, stack_idx).await
}

#[stdlib_func]
pub async fn tag_bend_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    super::trans::tag_trans_embed(executor, stack_idx).await
}

#[stdlib_func]
pub async fn mesh_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = executor.state.stack(stack_idx).read_at(-3).clone();
    let end = executor.state.stack(stack_idx).read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;
    super::trans::mesh_tree_patharc_lerp(&start, &end, t, Float3::ZERO)
}

#[stdlib_func]
pub async fn bend_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = executor.state.stack(stack_idx).read_at(-3).clone();
    let end = executor.state.stack(stack_idx).read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;
    super::trans::mesh_tree_patharc_lerp(&start, &end, t, Float3::ZERO)
}
