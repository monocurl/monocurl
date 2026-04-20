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
    let candidates = stack.read_at(-5).clone();
    let rate = stack.read_at(-3).clone();
    let embed = stack.read_at(-2).clone().elide_lvalue();
    let lerp = stack.read_at(-1).clone().elide_lvalue();
    let time = read_time(executor, stack_idx, -4)?;

    let embed = match embed {
        Value::Nil => None,
        Value::Lambda(_) | Value::Operator(_) => Some(Box::new(embed)),
        other => {
            return Err(ExecutorError::type_error_for(
                "lambda / operator / nil",
                other.type_name(),
                "embed",
            ));
        }
    };

    let lerp = match lerp {
        Value::Nil => None,
        Value::Lambda(_) | Value::Operator(_) => Some(Box::new(lerp)),
        other => {
            return Err(ExecutorError::type_error_for(
                "lambda / operator / nil",
                other.type_name(),
                "lerp",
            ));
        }
    };

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
        candidates: Box::new(candidates),
        time,
        progression: progression_from(rate),
        embed,
        lerp,
    }))
}

#[stdlib_func]
pub async fn wait(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let time = read_time(executor, stack_idx, -1)?;
    Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait { time }))
}
