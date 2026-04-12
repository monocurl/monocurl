use executor::{error::ExecutorError, state::ExecutionState, value::Value};
use stdlib_macros::stdlib_func;

#[stdlib_func]
pub async fn initial_camera(_state: &mut ExecutionState, _stack_idx: usize) -> Result<Value, ExecutorError> {
    // TODO: return default camera mesh
    Ok(Value::Nil)
}

#[stdlib_func]
pub async fn initial_background(_state: &mut ExecutionState, _stack_idx: usize) -> Result<Value, ExecutorError> {
    // TODO: return default background color
    Ok(Value::Nil)
}

#[stdlib_func]
pub async fn vector_len(state: &mut ExecutionState, stack_idx: usize) -> Result<Value, ExecutorError> {
    match state.stack(stack_idx).peek().clone().elide_lvalue() {
        Value::List(list) => Ok(Value::Integer(list.len() as i64)),
        other => Err(ExecutorError::type_error("list", other.type_name())),
    }
}

#[stdlib_func]
pub async fn map_len(state: &mut ExecutionState, stack_idx: usize) -> Result<Value, ExecutorError> {
    match state.stack(stack_idx).peek().clone().elide_lvalue() {
        Value::Map(map) => Ok(Value::Integer(map.len() as i64)),
        other => Err(ExecutorError::type_error("map", other.type_name())),
    }
}
