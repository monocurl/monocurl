use std::cmp::Ordering;

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, with_heap},
    value::Value,
};
use smallvec::{SmallVec, smallvec};
use stdlib_macros::stdlib_func;

use crate::read_float;

use super::helpers::{compare_values, invoke_key_lambda, list_depth, read_int, read_rc_list};

#[stdlib_func]
pub async fn list_len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_cached_wrappers_rec()
    {
        Value::List(list) => Ok(Value::Integer(list.len() as i64)),
        other => Err(ExecutorError::type_error("list", other.type_name())),
    }
}

#[stdlib_func]
pub async fn len(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_cached_wrappers_rec()
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
pub async fn depth(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let value = executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_cached_wrappers_rec();
    Ok(Value::Integer(list_depth(&value) as i64))
}

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
        executor.tick_yielder().await;

        elements.push(VRc::new(if all_ints {
            Value::Integer(x as i64)
        } else {
            Value::Float(x)
        }));
        x += step;
    }
    Ok(Value::List(executor::value::container::List::new_with(
        elements,
    )))
}

fn sampled_number(value: f64) -> Value {
    if value.fract() == 0.0 {
        Value::Integer(value as i64)
    } else {
        Value::Float(value)
    }
}

async fn sample_with_endpoint(
    executor: &mut Executor,
    stack_idx: usize,
    closed: bool,
) -> Result<Value, ExecutorError> {
    let start = read_float(executor, stack_idx, -3, "start")?;
    let stop = read_float(executor, stack_idx, -2, "stop")?;
    let sample_count = read_int(executor, stack_idx, -1, "sample_count")?;
    let Ok(sample_count) = usize::try_from(sample_count) else {
        return Err(ExecutorError::InvalidArgument {
            arg: "sample_count",
            message: "must be non-negative",
        });
    };

    let mut elements: SmallVec<[VRc; 4]> = SmallVec::with_capacity(sample_count);
    for i in 0..sample_count {
        executor.tick_yielder().await;

        let x = if closed {
            if sample_count == 1 {
                start
            } else if i + 1 == sample_count {
                stop
            } else {
                start + (stop - start) * i as f64 / (sample_count - 1) as f64
            }
        } else {
            start + (stop - start) * i as f64 / sample_count as f64
        };
        elements.push(VRc::new(sampled_number(x)));
    }

    Ok(Value::List(executor::value::container::List::new_with(
        elements,
    )))
}

#[stdlib_func]
pub async fn sample(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    sample_with_endpoint(executor, stack_idx, true).await
}

#[stdlib_func]
pub async fn sample_clopen(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    sample_with_endpoint(executor, stack_idx, false).await
}

#[stdlib_func]
pub async fn reverse(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .peek()
        .clone()
        .elide_cached_wrappers_rec()
    {
        Value::List(list) => {
            let mut elements = list
                .elements()
                .iter()
                .cloned()
                .collect::<SmallVec<[VRc; 4]>>();
            elements.reverse();
            Ok(Value::List(executor::value::container::List::new_with(
                elements,
            )))
        }
        Value::String(s) => Ok(Value::String(s.chars().rev().collect())),
        other => Err(ExecutorError::type_error(
            "list / string",
            other.type_name(),
        )),
    }
}

#[stdlib_func]
pub async fn sort(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -2, "v")?;
    let key = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();
    let Value::Lambda(lambda) = key else {
        return Err(ExecutorError::type_error_for(
            "lambda",
            key.type_name(),
            "key",
        ));
    };

    let mut keyed = Vec::with_capacity(list.len());
    for value_key in list.elements() {
        let value = with_heap(|h| h.get(value_key.key()).clone());
        let sort_key = invoke_key_lambda(executor, &lambda, value.clone()).await?;
        keyed.push((value, sort_key));
    }

    for i in 1..keyed.len() {
        let mut j = i;
        while j > 0 {
            let ordering = compare_values(&keyed[j - 1].1, &keyed[j].1)?;
            if ordering != Ordering::Greater {
                break;
            }
            keyed.swap(j - 1, j);
            j -= 1;
        }
    }

    Ok(super::helpers::list_from(
        keyed.into_iter().map(|(value, _)| value),
    ))
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
            VRc::new(Value::List(executor::value::container::List::new_with(
                smallvec![a_key.clone(), b_key.clone()],
            )))
        })
        .collect::<SmallVec<[VRc; 4]>>();
    Ok(Value::List(executor::value::container::List::new_with(
        elements,
    )))
}

#[stdlib_func]
pub async fn enumerate(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -1, "v")?;
    let elements = list
        .elements()
        .iter()
        .enumerate()
        .map(|(i, elem_key)| {
            VRc::new(Value::List(executor::value::container::List::new_with(
                smallvec![VRc::new(Value::Integer(i as i64)), elem_key.clone()],
            )))
        })
        .collect::<SmallVec<[VRc; 4]>>();
    Ok(Value::List(executor::value::container::List::new_with(
        elements,
    )))
}

#[stdlib_func]
pub async fn take(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -2, "v")?;
    let n = read_int(executor, stack_idx, -1, "n")?.max(0) as usize;
    Ok(Value::List(executor::value::container::List::new_with(
        list.elements()
            .iter()
            .take(n)
            .cloned()
            .collect::<SmallVec<[VRc; 4]>>(),
    )))
}

#[stdlib_func]
pub async fn drop(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -2, "v")?;
    let n = read_int(executor, stack_idx, -1, "n")?.max(0) as usize;
    Ok(Value::List(executor::value::container::List::new_with(
        list.elements()
            .iter()
            .skip(n)
            .cloned()
            .collect::<SmallVec<[VRc; 4]>>(),
    )))
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

    Ok(Value::List(executor::value::container::List::new_with(
        elements,
    )))
}

async fn extremum_of(
    executor: &mut Executor,
    stack_idx: usize,
    pick_max: bool,
) -> Result<Value, ExecutorError> {
    let list = read_rc_list(executor, stack_idx, -2, "v")?;
    if list.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "v",
            message: "cannot take extremum of empty list",
        });
    }

    let key = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();
    let Value::Lambda(lambda) = key else {
        return Err(ExecutorError::type_error_for(
            "lambda",
            key.type_name(),
            "key",
        ));
    };

    let first = with_heap(|h| h.get(list.elements()[0].key()).clone());
    let mut best_value = first.clone();
    let mut best_key = invoke_key_lambda(executor, &lambda, first).await?;

    for elem_key in &list.elements()[1..] {
        let value = with_heap(|h| h.get(elem_key.key()).clone());
        let key = invoke_key_lambda(executor, &lambda, value.clone()).await?;
        let ordering = compare_values(&key, &best_key)?;
        if (pick_max && ordering == Ordering::Greater) || (!pick_max && ordering == Ordering::Less)
        {
            best_value = value;
            best_key = key;
        }
    }

    Ok(best_value)
}

#[stdlib_func]
pub async fn min_of(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    extremum_of(executor, stack_idx, false).await
}

#[stdlib_func]
pub async fn max_of(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    extremum_of(executor, stack_idx, true).await
}
