use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

#[stdlib_func]
pub async fn to_string(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::String(s) => s,
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Complex { re, im } => format!("{} + {}i", re, im),
        Value::Nil => "nil".to_string(),
        other => {
            return Err(ExecutorError::type_error("primitive", other.type_name()));
        }
    };
    Ok(Value::String(s))
}

#[stdlib_func]
pub async fn to_int(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::Integer(n) => Ok(Value::Integer(n)),
        Value::Float(f) => Ok(Value::Integer(f as i64)),
        Value::String(s) => s.trim().parse::<i64>().map(Value::Integer).map_err(|_| {
            ExecutorError::InvalidArgument {
                arg: "x",
                message: "cannot parse as int",
            }
        }),
        other => Err(ExecutorError::type_error(
            "number / string",
            other.type_name(),
        )),
    }
}

#[stdlib_func]
pub async fn to_float(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::Integer(n) => Ok(Value::Float(n as f64)),
        Value::Float(f) => Ok(Value::Float(f)),
        Value::String(s) => {
            s.trim()
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|_| ExecutorError::InvalidArgument {
                    arg: "x",
                    message: "cannot parse as float",
                })
        }
        other => Err(ExecutorError::type_error(
            "number / string",
            other.type_name(),
        )),
    }
}
