use std::{future::Future, pin::Pin};

use executor::{error::ExecutorError, executor::Executor, heap::with_heap, value::Value};

mod anim;
mod color;
mod math;
mod mesh;
pub mod registry;
mod scene;
mod util;

pub(crate) const STRING_COMPATIBLE_DESC: &str = "string-compatible value";

pub(crate) fn stringify_value<'a>(
    executor: &'a mut Executor,
    value: Value,
) -> Pin<Box<dyn Future<Output = Result<String, ExecutorError>> + 'a>> {
    Box::pin(async move {
        match value.elide_wrappers_rec(executor).await? {
            Value::String(value) => Ok(value),
            Value::Integer(value) => Ok(value.to_string()),
            Value::Float(value) => Ok(value.to_string()),
            Value::Nil => Ok("nil".to_string()),
            Value::List(list) => {
                let mut out = String::new();
                for key in list.elements() {
                    let value = with_heap(|h| h.get(key.key()).clone());
                    out.push_str(&stringify_value(executor, value).await?);
                }
                Ok(out)
            }
            other => Err(ExecutorError::type_error(
                STRING_COMPATIBLE_DESC,
                other.type_name(),
            )),
        }
    })
}

fn read_float(
    executor: &Executor,
    stack: usize,
    index: i32,
    name: &'static str,
) -> Result<f64, ExecutorError> {
    match executor.state.stack(stack).read_at(index) {
        Value::Float(f) => Ok(*f),
        Value::Integer(n) => Ok(*n as f64),
        other => Err(ExecutorError::type_error_for(
            "float",
            other.type_name(),
            name,
        )),
    }
}
