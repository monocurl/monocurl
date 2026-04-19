use std::rc::Rc;

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, with_heap},
    value::{
        Value,
        container::{HashableKey, List, Map},
    },
};
use smallvec::smallvec;
use stdlib_macros::stdlib_func;

use crate::read_float;

// ── helpers ──────────────────────────────────────────────────────────────────

fn read_string(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<String, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::String(s) => Ok(s),
        other => Err(ExecutorError::type_error_for(
            "string",
            other.type_name(),
            name,
        )),
    }
}

fn read_int(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<i64, ExecutorError> {
    match executor.state.stack(stack_idx).read_at(index) {
        Value::Integer(n) => Ok(*n),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

fn list_from<I: IntoIterator<Item = Value>>(values: I) -> Value {
    Value::List(Rc::new(List::new_with(
        values.into_iter().map(VRc::new).collect(),
    )))
}

fn read_rc_list(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Rc<List>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::List(list) => Ok(list),
        other => Err(ExecutorError::type_error_for(
            "list",
            other.type_name(),
            name,
        )),
    }
}

// ── lengths & shapes ─────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn vector_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::List(list) => Ok(Value::Integer(list.len() as i64)),
        other => Err(ExecutorError::type_error("list", other.type_name())),
    }
}

#[stdlib_func]
pub async fn map_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::Map(map) => Ok(Value::Integer(map.len() as i64)),
        other => Err(ExecutorError::type_error("map", other.type_name())),
    }
}

#[stdlib_func]
pub async fn len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::List(list) => list.len(),
        Value::Map(map) => map.len(),
        Value::String(s) => s.chars().count(),
        other => {
            return Err(ExecutorError::type_error(
                "list / map / string",
                other.type_name(),
            ));
        }
    };
    Ok(Value::Integer(n as i64))
}

#[stdlib_func]
pub async fn depth(_executor: &mut Executor, _stack_idx: usize) -> Result<Value, ExecutorError> {
    todo!("maximum nesting depth of a list")
}

// ── list manipulation ────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn range(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let start = read_float(executor, stack_idx, -3, "start")?;
    let stop = read_float(executor, stack_idx, -2, "stop")?;
    let step = read_float(executor, stack_idx, -1, "step")?;
    if step == 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "step",
            message: "must be non-zero",
        });
    }

    let all_ints = start.fract() == 0.0 && stop.fract() == 0.0 && step.fract() == 0.0;

    let mut elements = smallvec![];
    let mut x = start;
    while (step > 0.0 && x < stop) || (step < 0.0 && x > stop) {
        elements.push(VRc::new(if all_ints {
            Value::Integer(x as i64)
        } else {
            Value::Float(x)
        }));
        x += step;
    }
    Ok(Value::List(Rc::new(List::new_with(elements))))
}

#[stdlib_func]
pub async fn reverse(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_lvalue()
    {
        Value::List(list) => {
            let mut elements = list
                .elements()
                .iter()
                .cloned()
                .collect::<smallvec::SmallVec<[VRc; 4]>>();
            elements.reverse();
            Ok(Value::List(Rc::new(List::new_with(elements))))
        }
        Value::String(s) => Ok(Value::String(s.chars().rev().collect())),
        other => Err(ExecutorError::type_error(
            "list / string",
            other.type_name(),
        )),
    }
}

#[stdlib_func]
pub async fn sort(_executor: &mut Executor, _stack_idx: usize) -> Result<Value, ExecutorError> {
    todo!("stable sort with custom key lambda")
}

#[stdlib_func]
pub async fn zip(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let u = read_rc_list(executor, stack_idx, -2, "u")?;
    let v = read_rc_list(executor, stack_idx, -1, "v")?;
    let elements = u
        .elements()
        .iter()
        .zip(v.elements().iter())
        .map(|(a_key, b_key)| {
            VRc::new(Value::List(Rc::new(List::new_with(smallvec![
                a_key.clone(),
                b_key.clone(),
            ]))))
        })
        .collect::<smallvec::SmallVec<[VRc; 4]>>();
    Ok(Value::List(Rc::new(List::new_with(elements))))
}

#[stdlib_func]
pub async fn enumerate(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -1, "v")?;
    let elements = list
        .elements()
        .iter()
        .enumerate()
        .map(|(i, elem_key)| {
            VRc::new(Value::List(Rc::new(List::new_with(smallvec![
                VRc::new(Value::Integer(i as i64)),
                elem_key.clone(),
            ]))))
        })
        .collect::<smallvec::SmallVec<[VRc; 4]>>();
    Ok(Value::List(Rc::new(List::new_with(elements))))
}

#[stdlib_func]
pub async fn take(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -2, "v")?;
    let n = read_int(executor, stack_idx, -1, "n")?.max(0) as usize;
    let elements = list
        .elements()
        .iter()
        .take(n)
        .cloned()
        .collect::<smallvec::SmallVec<[VRc; 4]>>();
    Ok(Value::List(Rc::new(List::new_with(elements))))
}

#[stdlib_func]
pub async fn drop(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -2, "v")?;
    let n = read_int(executor, stack_idx, -1, "n")?.max(0) as usize;
    let elements = list
        .elements()
        .iter()
        .skip(n)
        .cloned()
        .collect::<smallvec::SmallVec<[VRc; 4]>>();
    Ok(Value::List(Rc::new(List::new_with(elements))))
}

#[stdlib_func]
pub async fn list_subset(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let src = read_rc_list(executor, stack_idx, -2, "src")?;
    let indexes = read_rc_list(executor, stack_idx, -1, "indexes")?;

    let mut elements = smallvec![];
    for index_key in indexes.elements() {
        let idx = match with_heap(|h| h.get(index_key.key()).clone()) {
            Value::Integer(n) => n,
            other => {
                return Err(ExecutorError::type_error_for(
                    "int",
                    other.type_name(),
                    "index",
                ));
            }
        };

        if idx < 0 {
            return Err(ExecutorError::InvalidArgument {
                arg: "index",
                message: "must be non-negative",
            });
        }

        let idx = idx as usize;
        if idx >= src.elements().len() {
            return Err(ExecutorError::IndexOutOfBounds {
                index: idx,
                len: src.elements().len(),
            });
        }

        elements.push(src.elements()[idx].clone());
    }

    Ok(Value::List(Rc::new(List::new_with(elements))))
}

#[stdlib_func]
pub async fn min_of(_executor: &mut Executor, _stack_idx: usize) -> Result<Value, ExecutorError> {
    todo!("min over a list with custom key lambda")
}

#[stdlib_func]
pub async fn max_of(_executor: &mut Executor, _stack_idx: usize) -> Result<Value, ExecutorError> {
    todo!("max over a list with custom key lambda")
}

// ── map helpers ──────────────────────────────────────────────────────────────

fn key_to_value(k: &HashableKey) -> Value {
    match k {
        HashableKey::Integer(n) => Value::Integer(*n),
        HashableKey::String(s) => Value::String(s.clone()),
        HashableKey::Vector(v) => list_from(v.iter().map(key_to_value)),
    }
}

fn map_from(executor: &Executor, stack_idx: usize, index: i32) -> Result<Rc<Map>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::Map(m) => Ok(m),
        other => Err(ExecutorError::type_error("map", other.type_name())),
    }
}

#[stdlib_func]
pub async fn map_keys(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let m = map_from(executor, stack_idx, -1)?;
    Ok(list_from(m.insertion_order.iter().map(key_to_value)))
}

#[stdlib_func]
pub async fn map_values(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let m = map_from(executor, stack_idx, -1)?;
    let elements = m
        .iter()
        .map(|(_, v)| v.clone())
        .collect::<smallvec::SmallVec<[VRc; 4]>>();
    Ok(Value::List(Rc::new(List::new_with(elements))))
}

#[stdlib_func]
pub async fn map_items(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let m = map_from(executor, stack_idx, -1)?;
    let elements = m
        .iter()
        .map(|(k, v)| {
            VRc::new(Value::List(Rc::new(List::new_with(smallvec![
                VRc::new(key_to_value(k)),
                v.clone(),
            ]))))
        })
        .collect::<smallvec::SmallVec<[VRc; 4]>>();
    Ok(Value::List(Rc::new(List::new_with(elements))))
}

// ── string operations ────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn str_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -1, "s")?;
    Ok(Value::Integer(s.chars().count() as i64))
}

#[stdlib_func]
pub async fn str_replace(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -3, "s")?;
    let needle = read_string(executor, stack_idx, -2, "needle")?;
    let with = read_string(executor, stack_idx, -1, "with")?;
    Ok(Value::String(s.replace(&needle, &with)))
}

#[stdlib_func]
pub async fn str_split(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -2, "s")?;
    let sep = read_string(executor, stack_idx, -1, "sep")?;
    Ok(list_from(
        s.split(&sep).map(|p| Value::String(p.to_string())),
    ))
}

#[stdlib_func]
pub async fn str_upper(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -1, "s")?;
    Ok(Value::String(s.to_uppercase()))
}

#[stdlib_func]
pub async fn str_lower(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let s = read_string(executor, stack_idx, -1, "s")?;
    Ok(Value::String(s.to_lowercase()))
}

#[stdlib_func]
pub async fn str_join(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let parts = read_rc_list(executor, stack_idx, -2, "parts")?;
    let sep = read_string(executor, stack_idx, -1, "sep")?;
    let strings = parts
        .elements()
        .iter()
        .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
            Value::String(s) => Ok(s),
            other => Err(ExecutorError::type_error("string", other.type_name())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::String(strings.join(&sep)))
}

// ── type coercion ────────────────────────────────────────────────────────────

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
