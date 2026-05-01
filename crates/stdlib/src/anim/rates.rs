use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

use super::helpers::read_time;

#[stdlib_func]
pub async fn rate_bounce(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let t = read_time(executor, stack_idx, -1)?;
    let out = if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    };
    Ok(Value::Float(out))
}

#[stdlib_func]
pub async fn rate_elastic(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let t = read_time(executor, stack_idx, -1)?;
    let out = if t == 0.0 || t == 1.0 {
        t
    } else {
        let p = 0.3;
        let s = p / 4.0;
        2f64.powf(-10.0 * t) * ((t - s) * std::f64::consts::TAU / p).sin() + 1.0
    };
    Ok(Value::Float(out))
}
