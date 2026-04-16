use executor::{error::ExecutorError, executor::Executor, value::Value};

pub mod registry;
mod anim;
mod mesh;
mod util;

fn read_float(executor: &Executor, stack: usize, index: i32, name: &'static str) -> Result<f64, ExecutorError> {
    match executor
        .state
        .stack(stack)
        .read_at(index)
    {
        Value::Float(f) => Ok(*f),
        Value::Integer(n) => Ok(*n as f64),
        other => Err(ExecutorError::type_error_for(
            "float",
            other.type_name(),
            name
        )),
    }
}
