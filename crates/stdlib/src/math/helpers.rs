use executor::{error::ExecutorError, executor::Executor, heap::with_heap, value::Value};

pub(super) enum NumberPair {
    Int(i64, i64),
    Float(f64, f64),
}

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

pub(super) fn read_number_pair(
    executor: &Executor,
    stack: usize,
    lhs: &'static str,
    rhs: &'static str,
) -> Result<NumberPair, ExecutorError> {
    let lhs_value = executor
        .state
        .stack(stack)
        .read_at(-2)
        .clone()
        .elide_lvalue();
    let rhs_value = executor
        .state
        .stack(stack)
        .read_at(-1)
        .clone()
        .elide_lvalue();

    match (lhs_value, rhs_value) {
        (Value::Integer(a), Value::Integer(b)) => Ok(NumberPair::Int(a, b)),
        (Value::Integer(a), Value::Float(b)) => Ok(NumberPair::Float(a as f64, b)),
        (Value::Float(a), Value::Integer(b)) => Ok(NumberPair::Float(a, b as f64)),
        (Value::Float(a), Value::Float(b)) => Ok(NumberPair::Float(a, b)),
        (other, _) if !matches!(other, Value::Integer(_) | Value::Float(_)) => Err(
            ExecutorError::type_error_for("number", other.type_name(), lhs),
        ),
        (_, other) => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            rhs,
        )),
    }
}
