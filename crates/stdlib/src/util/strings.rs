use executor::{error::ExecutorError, executor::Executor, heap::with_heap, value::Value};
use stdlib_macros::stdlib_func;

use super::helpers::{list_from, read_rc_list, read_string};

#[stdlib_func]
pub async fn str_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -1, "s").await?;
    Ok(Value::Integer(s.chars().count() as i64))
}

#[stdlib_func]
pub async fn str_replace(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -3, "s").await?;
    let needle = read_string(executor, stack_idx, -2, "needle").await?;
    let with = read_string(executor, stack_idx, -1, "with").await?;
    Ok(Value::String(s.replace(&needle, &with).into()))
}

#[stdlib_func]
pub async fn str_split(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -2, "s").await?;
    let sep = read_string(executor, stack_idx, -1, "sep").await?;
    Ok(list_from(
        s.split(&sep).map(|p| Value::String(p.to_string().into())),
    ))
}

#[stdlib_func]
pub async fn str_upper(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -1, "s").await?;
    Ok(Value::String(s.to_uppercase().into()))
}

#[stdlib_func]
pub async fn str_lower(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -1, "s").await?;
    Ok(Value::String(s.to_lowercase().into()))
}

#[stdlib_func]
pub async fn str_join(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let parts = read_rc_list(executor, stack_idx, -2, "parts")?;
    let sep = read_string(executor, stack_idx, -1, "sep").await?;
    let strings = parts
        .elements()
        .iter()
        .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
            Value::String(s) => Ok(s.to_string()),
            other => Err(ExecutorError::type_error("string", other.type_name())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::String(strings.join(&sep).into()))
}
