use std::{cmp::Ordering, rc::Rc};

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, with_heap},
    value::{Value, container::List, lambda::Lambda},
};
use stdlib_macros::stdlib_func;

pub(super) fn list_from<I: IntoIterator<Item = Value>>(values: I) -> Value {
    Value::List(Rc::new(List::new_with(
        values.into_iter().map(VRc::new).collect(),
    )))
}

pub(super) fn read_string(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<String, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::String(s) => Ok(s),
        other => Err(ExecutorError::type_error_for(
            "string",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn read_int(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<i64, ExecutorError> {
    match executor.state.stack(stack_idx).read_at(index) {
        Value::Integer(n) => Ok(*n),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn read_rc_list(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Rc<List>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::List(list) => Ok(list),
        other => Err(ExecutorError::type_error_for(
            "list",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn list_depth(value: &Value) -> usize {
    match value {
        Value::List(list) => {
            1 + list
                .elements()
                .iter()
                .map(|key| list_depth(&with_heap(|h| h.get(key.key()).clone())))
                .max()
                .unwrap_or(0)
        }
        _ => 0,
    }
}

pub(super) async fn invoke_key_lambda(
    executor: &mut Executor,
    lambda: &Rc<Lambda>,
    value: Value,
) -> Result<Value, ExecutorError> {
    executor
        .invoke_lambda(lambda, vec![value])
        .await?
        .elide_wrappers(executor)
        .await
}

pub(super) fn compare_values(lhs: &Value, rhs: &Value) -> Result<Ordering, ExecutorError> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(a.cmp(b)),
        (Value::Float(a), Value::Float(b)) => Ok(a.total_cmp(b)),
        (Value::Integer(a), Value::Float(b)) => Ok((*a as f64).total_cmp(b)),
        (Value::Float(a), Value::Integer(b)) => Ok(a.total_cmp(&(*b as f64))),
        (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
        (Value::List(a), Value::List(b)) => {
            for (a_key, b_key) in a.elements().iter().zip(b.elements().iter()) {
                let ordering = compare_values(
                    &with_heap(|h| h.get(a_key.key()).clone()),
                    &with_heap(|h| h.get(b_key.key()).clone()),
                )?;
                if ordering != Ordering::Equal {
                    return Ok(ordering);
                }
            }
            Ok(a.len().cmp(&b.len()))
        }
        _ => Err(ExecutorError::Other(format!(
            "cannot compare {} and {}",
            lhs.type_name(),
            rhs.type_name()
        ))),
    }
}

#[stdlib_func]
pub async fn lambda_fallthrough_error(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    Err(ExecutorError::Other(
        "lambda reached end without explicit return".into(),
    ))
}
