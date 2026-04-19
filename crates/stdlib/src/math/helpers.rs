use executor::{error::ExecutorError, executor::Executor, heap::with_heap, value::Value};

pub(super) fn unary_f64(
    executor: &Executor,
    stack: usize,
    name: &'static str,
    f: impl Fn(f64) -> f64,
) -> Result<Value, ExecutorError> {
    let x = crate::read_float(executor, stack, -1, name)?;
    Ok(Value::Float(f(x)))
}

pub(super) fn binary_f64(
    executor: &Executor,
    stack: usize,
    lhs: &'static str,
    rhs: &'static str,
    f: impl Fn(f64, f64) -> f64,
) -> Result<Value, ExecutorError> {
    let a = crate::read_float(executor, stack, -2, lhs)?;
    let b = crate::read_float(executor, stack, -1, rhs)?;
    Ok(Value::Float(f(a, b)))
}

pub(super) fn read_int(
    executor: &Executor,
    stack: usize,
    index: i32,
    name: &'static str,
) -> Result<i64, ExecutorError> {
    match executor.state.stack(stack).read_at(index) {
        Value::Integer(n) => Ok(*n),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn read_list(
    executor: &Executor,
    stack: usize,
    index: i32,
    name: &'static str,
) -> Result<Vec<f64>, ExecutorError> {
    match executor
        .state
        .stack(stack)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::List(list) => list
            .elements()
            .iter()
            .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
                Value::Float(f) => Ok(f),
                Value::Integer(n) => Ok(n as f64),
                other => Err(ExecutorError::type_error_for(
                    "number",
                    other.type_name(),
                    name,
                )),
            })
            .collect(),
        other => Err(ExecutorError::type_error_for(
            "list",
            other.type_name(),
            name,
        )),
    }
}
