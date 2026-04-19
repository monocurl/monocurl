use std::rc::Rc;

use executor::{
    error::ExecutorError,
    executor::Executor,
    value::{
        Value,
        container::{HashableKey, List, Map},
        rc_value,
    },
};
use stdlib_macros::stdlib_func;

use crate::read_float;

fn read_value(executor: &Executor, stack_idx: usize, index: i32, _name: &'static str) -> Value {
    executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
}

fn read_int_flag(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<i64, ExecutorError> {
    match executor.state.stack(stack_idx).read_at(index) {
        Value::Integer(n) => Ok(*n),
        Value::Float(f) if f.fract() == 0.0 => Ok(*f as i64),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

fn value_list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::List(Rc::new(List {
        elements: values.into_iter().map(rc_value).collect(),
    }))
}

fn tagged_map(
    kind: &'static str,
    fields: impl IntoIterator<Item = (&'static str, Value)>,
) -> Value {
    let mut map = Map::new();
    map.insert(
        HashableKey::String("kind".to_string()),
        rc_value(Value::String(kind.to_string())),
    );
    for (key, value) in fields {
        map.insert(HashableKey::String(key.to_string()), rc_value(value));
    }
    Value::Map(Rc::new(map))
}

#[stdlib_func]
pub async fn initial_camera(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    Ok(tagged_map(
        "camera",
        [
            (
                "position",
                value_list([Value::Integer(0), Value::Integer(0), Value::Integer(-10)]),
            ),
            (
                "look_at",
                value_list([Value::Integer(0), Value::Integer(0), Value::Integer(0)]),
            ),
            (
                "up",
                value_list([Value::Integer(0), Value::Integer(1), Value::Integer(0)]),
            ),
            ("fov", Value::Float(0.6981317007977318)),
            ("near", Value::Float(0.1)),
            ("far", Value::Integer(100)),
            ("ortho", Value::Integer(0)),
        ],
    ))
}

#[stdlib_func]
pub async fn initial_background(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    Ok(tagged_map(
        "solid_background",
        [(
            "color",
            value_list([
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(1),
            ]),
        )],
    ))
}

#[stdlib_func]
pub async fn mk_camera(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    Ok(tagged_map(
        "camera",
        [
            ("position", read_value(executor, stack_idx, -7, "position")),
            ("look_at", read_value(executor, stack_idx, -6, "look_at")),
            ("up", read_value(executor, stack_idx, -5, "up")),
            (
                "fov",
                Value::Float(read_float(executor, stack_idx, -4, "fov")?),
            ),
            (
                "near",
                Value::Float(read_float(executor, stack_idx, -3, "near")?),
            ),
            (
                "far",
                Value::Float(read_float(executor, stack_idx, -2, "far")?),
            ),
            (
                "ortho",
                Value::Integer(read_int_flag(executor, stack_idx, -1, "ortho")?),
            ),
        ],
    ))
}
