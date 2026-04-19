use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

use super::helpers::{binary_f64, unary_f64};

#[stdlib_func]
pub async fn sqrt(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::sqrt)
}

#[stdlib_func]
pub async fn cbrt(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::cbrt)
}

#[stdlib_func]
pub async fn exp(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::exp)
}

#[stdlib_func]
pub async fn ln(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::ln)
}

#[stdlib_func]
pub async fn pow(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "base", "exp", f64::powf)
}

#[stdlib_func]
pub async fn sin(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::sin)
}

#[stdlib_func]
pub async fn cos(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::cos)
}

#[stdlib_func]
pub async fn tan(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::tan)
}

#[stdlib_func]
pub async fn arcsin(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::asin)
}

#[stdlib_func]
pub async fn arccos(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::acos)
}

#[stdlib_func]
pub async fn arctan(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::atan)
}

#[stdlib_func]
pub async fn arctan2(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "y", "x", f64::atan2)
}

#[stdlib_func]
pub async fn sinh(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::sinh)
}

#[stdlib_func]
pub async fn cosh(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::cosh)
}

#[stdlib_func]
pub async fn tanh(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::tanh)
}

#[stdlib_func]
pub async fn abs(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor.state.stack(stack_idx).read_at(-1) {
        Value::Integer(n) => Ok(Value::Integer(n.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            "x",
        )),
    }
}

#[stdlib_func]
pub async fn sign(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor.state.stack(stack_idx).read_at(-1) {
        Value::Integer(n) => Ok(Value::Integer(n.signum())),
        Value::Float(f) => Ok(Value::Float(f.signum())),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            "x",
        )),
    }
}

#[stdlib_func]
pub async fn floor(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = crate::read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.floor() as i64))
}

#[stdlib_func]
pub async fn ceil(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = crate::read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.ceil() as i64))
}

#[stdlib_func]
pub async fn round(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = crate::read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.round() as i64))
}

#[stdlib_func]
pub async fn trunc(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = crate::read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.trunc() as i64))
}

#[stdlib_func]
pub async fn mod_func(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = crate::read_float(executor, stack_idx, -2, "n")?;
    let m = crate::read_float(executor, stack_idx, -1, "m")?;
    if m == 0.0 {
        return Err(ExecutorError::DivisionByZero);
    }
    Ok(Value::Float(n.rem_euclid(m)))
}

#[stdlib_func]
pub async fn min(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "a", "b", f64::min)
}

#[stdlib_func]
pub async fn max(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "a", "b", f64::max)
}
