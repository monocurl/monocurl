use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::VRc,
    value::{
        Value,
        container::{HashableKey, Map},
    },
};
use smallvec::{SmallVec, smallvec};
use stdlib_macros::stdlib_func;

use super::helpers::list_from;

fn key_to_value(k: &HashableKey) -> Value {
    match k {
        HashableKey::Integer(n) => Value::Integer(*n),
        HashableKey::Float(bits) => Value::Float(HashableKey::float_value(*bits)),
        HashableKey::String(s) => Value::String(s.clone()),
        HashableKey::Vector(v) => list_from(v.iter().map(key_to_value)),
    }
}

fn map_from(executor: &Executor, stack_idx: usize, index: i32) -> Result<Map, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_cached_wrappers_rec()
    {
        Value::Map(m) => Ok(m),
        other => Err(ExecutorError::type_error("map", other.type_name())),
    }
}

#[stdlib_func]
pub async fn map_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_cached_wrappers_rec()
    {
        Value::Map(map) => Ok(Value::Integer(map.len() as i64)),
        other => Err(ExecutorError::type_error("map", other.type_name())),
    }
}

#[stdlib_func]
pub async fn map_keys(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let m = map_from(executor, stack_idx, -1)?;
    Ok(list_from(m.insertion_order.iter().map(key_to_value)))
}

#[stdlib_func]
pub async fn map_values(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let m = map_from(executor, stack_idx, -1)?;
    let elements = m
        .iter()
        .map(|(_, v)| v.clone())
        .collect::<SmallVec<[VRc; 4]>>();
    Ok(Value::List(executor::value::container::List::new_with(
        elements,
    )))
}

#[stdlib_func]
pub async fn map_items(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let m = map_from(executor, stack_idx, -1)?;
    let elements = m
        .iter()
        .map(|(k, v)| {
            VRc::new(Value::List(executor::value::container::List::new_with(
                smallvec![VRc::new(key_to_value(k)), v.clone()],
            )))
        })
        .collect::<SmallVec<[VRc; 4]>>();
    Ok(Value::List(executor::value::container::List::new_with(
        elements,
    )))
}
