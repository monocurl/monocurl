use executor::{error::ExecutorError, value::Value};
use stdlib_macros::stdlib_func;

#[stdlib_func]
pub async fn initial_camera(_args: Vec<Value>) -> Result<Value, ExecutorError> {
    // TODO: return default camera mesh
    Ok(Value::Nil)
}

#[stdlib_func]
pub async fn initial_background(_args: Vec<Value>) -> Result<Value, ExecutorError> {
    // TODO: return default background color
    Ok(Value::Nil)
}

#[stdlib_func]
pub async fn vector_len(args: Vec<Value>) -> Result<Value, ExecutorError> {
    match args.first() {
        Some(Value::List(list)) => Ok(Value::Integer(list.len() as i64)),
        Some(other) => Err(ExecutorError::type_error("list", other.type_name())),
        None => Err(ExecutorError::MissingArgument("vector_len")),
    }
}

#[stdlib_func]
pub async fn map_len(args: Vec<Value>) -> Result<Value, ExecutorError> {
    match args.first() {
        Some(Value::Map(map)) => Ok(Value::Integer(map.len() as i64)),
        Some(other) => Err(ExecutorError::type_error("map", other.type_name())),
        None => Err(ExecutorError::MissingArgument("map_len")),
    }
}
