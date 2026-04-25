use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::VRc,
    value::{
        Value,
        container::{HashableKey, List, Map},
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

fn value_list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::List(List::new_with(values.into_iter().map(VRc::new).collect()))
}

fn tagged_map(
    kind: &'static str,
    fields: impl IntoIterator<Item = (&'static str, Value)>,
) -> Value {
    let mut map = Map::new();
    map.insert(
        HashableKey::String("kind".to_string()),
        VRc::new(Value::String(kind.to_string())),
    );
    for (key, value) in fields {
        map.insert(HashableKey::String(key.to_string()), VRc::new(value));
    }
    Value::Map(map)
}

fn camera_value(
    executor: &Executor,
    stack_idx: usize,
    position_idx: i32,
    look_at_idx: i32,
    up_idx: i32,
    near_idx: i32,
    far_idx: i32,
) -> Result<Value, ExecutorError> {
    Ok(tagged_map(
        "camera",
        [
            (
                "position",
                read_value(executor, stack_idx, position_idx, "position"),
            ),
            (
                "look_at",
                read_value(executor, stack_idx, look_at_idx, "look_at"),
            ),
            ("up", read_value(executor, stack_idx, up_idx, "up")),
            (
                "near",
                Value::Float(read_float(executor, stack_idx, near_idx, "near")?),
            ),
            (
                "far",
                Value::Float(read_float(executor, stack_idx, far_idx, "far")?),
            ),
        ],
    ))
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
                value_list([Value::Integer(0), Value::Integer(0), Value::Integer(-4)]),
            ),
            (
                "look_at",
                value_list([Value::Integer(0), Value::Integer(0), Value::Integer(0)]),
            ),
            (
                "up",
                value_list([Value::Integer(0), Value::Integer(1), Value::Integer(0)]),
            ),
            ("near", Value::Float(0.1)),
            ("far", Value::Integer(100)),
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
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(1),
            ]),
        )],
    ))
}

#[stdlib_func]
pub async fn mk_camera(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    camera_value(executor, stack_idx, -5, -4, -3, -2, -1)
}
