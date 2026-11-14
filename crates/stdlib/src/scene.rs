use executor::{
    camera::DEFAULT_CAMERA_FOV,
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
    Value::List(List::new_with(values.into_iter().map(VRc::new)))
}

fn tagged_map(
    kind: &'static str,
    fields: impl IntoIterator<Item = (&'static str, Value)>,
) -> Value {
    let mut map = Map::new();
    map.insert(
        HashableKey::String("kind".to_string()),
        VRc::new(Value::String(kind.into())),
    );
    for (key, value) in fields {
        map.insert(HashableKey::String(key.to_string()), VRc::new(value));
    }
    Value::Map(map)
}

/// the last argument is the projection's size: fov for perspective, height for orthographic
fn camera_value(
    executor: &Executor,
    stack_idx: usize,
    projection: &'static str,
    size_name: &'static str,
) -> Result<Value, ExecutorError> {
    Ok(tagged_map(
        "camera",
        [
            ("position", read_value(executor, stack_idx, -6, "position")),
            ("look_at", read_value(executor, stack_idx, -5, "look_at")),
            ("up", read_value(executor, stack_idx, -4, "up")),
            (
                "near",
                Value::Float(read_float(executor, stack_idx, -3, "near")?),
            ),
            (
                "far",
                Value::Float(read_float(executor, stack_idx, -2, "far")?),
            ),
            ("projection", Value::String(projection.into())),
            (
                size_name,
                Value::Float(read_float(executor, stack_idx, -1, size_name)?),
            ),
        ],
    ))
}

#[stdlib_func]
pub fn initial_camera(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    Ok(tagged_map(
        "camera",
        [
            (
                "position",
                value_list([Value::Integer(0), Value::Integer(0), Value::Integer(4)]),
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
            ("projection", Value::String("perspective".into())),
            ("fov", Value::Float(DEFAULT_CAMERA_FOV as f64)),
        ],
    ))
}

#[stdlib_func]
pub fn initial_background(
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
pub fn aspect_ratio(
    executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    Ok(Value::Float(executor.aspect_ratio() as f64))
}

#[stdlib_func]
pub fn mk_camera(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    camera_value(executor, stack_idx, "perspective", "fov")
}

#[stdlib_func]
pub fn mk_orthographic_camera(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    camera_value(executor, stack_idx, "orthographic", "height")
}
