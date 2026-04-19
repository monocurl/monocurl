use executor::{
    error::ExecutorError,
    executor::Executor,
    value::{Value, primitive_anim::PrimitiveAnim},
};
use stdlib_macros::stdlib_func;

use super::helpers::{progression_from, read_time};

#[stdlib_func]
pub async fn set(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-1).clone();
    Ok(Value::PrimitiveAnim(PrimitiveAnim::Set {
        candidates: Box::new(candidates),
    }))
}

#[stdlib_func]
pub async fn lerp_anim(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let stack = executor.state.stack(stack_idx);
    let candidates = stack.read_at(-3).clone();
    let rate = stack.read_at(-1).clone();
    let time = read_time(executor, stack_idx, -2)?;

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
        candidates: Box::new(candidates),
        time,
        progression: progression_from(rate),
    }))
}

#[stdlib_func]
pub async fn wait(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let time = read_time(executor, stack_idx, -1)?;
    Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait { time }))
}
