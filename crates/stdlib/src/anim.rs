use executor::{
    error::ExecutorError,
    executor::Executor,
    value::{Value, primitive_anim::PrimitiveAnim},
};
use stdlib_macros::stdlib_func;

use crate::read_float;

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
    let candidates = stack.read_at(-2).clone();
    let time = read_float(executor, stack_idx, -1, "time")?;
    if time < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "time",
            message: "must be non-negative",
        });
    }

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
        candidates: Box::new(candidates),
        time,
        progression: None,
    }))
}

#[stdlib_func]
pub async fn wait(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let time = read_float(executor, stack_idx, -1, "time")?;
    if time < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "time",
            message: "must be non-negative",
        });
    }

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait { time }))
}
