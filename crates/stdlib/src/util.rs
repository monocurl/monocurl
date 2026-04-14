use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

#[stdlib_func]
pub async fn initial_camera(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    // TODO: return default camera mesh
    Ok(Value::Nil)
}

#[stdlib_func]
pub async fn initial_background(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    // TODO: return default background color
    Ok(Value::Nil)
}

#[stdlib_func]
pub async fn vector_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::List(list) => Ok(Value::Integer(list.len() as i64)),
        other => Err(ExecutorError::type_error("list", other.type_name())),
    }
}

#[stdlib_func]
pub async fn map_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::Map(map) => Ok(Value::Integer(map.len() as i64)),
        other => Err(ExecutorError::type_error("map", other.type_name())),
    }
}

#[stdlib_func]
pub async fn lerp(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let stack = executor.state.stack(stack_idx);
    let alpha = stack.read_at(-3).clone();
    let beta = stack.read_at(-2).clone();
    let t = match stack.read_at(-1) {
        Value::Float(f) => *f,
        other => {
            return Err(ExecutorError::type_error_for(
                "float",
                other.type_name(),
                "t",
            ))
        }
    };

    executor.lerp(alpha, beta, t).await
}
