use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

use super::helpers::read_list;

#[stdlib_func]
pub async fn mean(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let v = read_list(executor, stack_idx, -1, "v")?;
    if v.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "v",
            message: "cannot compute mean of empty list",
        });
    }
    Ok(Value::Float(v.iter().sum::<f64>() / v.len() as f64))
}

#[stdlib_func]
pub async fn variance(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let v = read_list(executor, stack_idx, -1, "v")?;
    if v.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "v",
            message: "cannot compute variance of empty list",
        });
    }
    let mu = v.iter().sum::<f64>() / v.len() as f64;
    Ok(Value::Float(
        v.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / v.len() as f64,
    ))
}
