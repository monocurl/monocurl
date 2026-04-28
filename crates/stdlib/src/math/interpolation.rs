use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::with_heap,
    value::{
        Value,
        container::{HashableKey, List, Map},
    },
};
use stdlib_macros::stdlib_func;

#[stdlib_func]
pub async fn lerp(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let stack = executor.state.stack(stack_idx);
    let alpha = stack.read_at(-3).clone();
    let beta = stack.read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")?;
    executor.lerp(alpha, beta, t).await
}

#[stdlib_func]
pub async fn keyframe_lerp(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let keyframes = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue_leader_rec();
    let t = crate::read_float(executor, stack_idx, -1, "t")?;

    let parsed = match keyframes {
        Value::List(keyframes) => parse_keyframe_list(&keyframes)?,
        Value::Map(keyframes) => parse_keyframe_map(&keyframes)?,
        other => {
            return Err(ExecutorError::type_error_for(
                "map or list",
                other.type_name(),
                "keyframes",
            ));
        }
    };

    lerp_keyframes(executor, parsed, t).await
}

fn parse_keyframe_list(keyframes: &List) -> Result<Vec<(f64, Value)>, ExecutorError> {
    if keyframes.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "keyframes",
            message: "cannot interpolate empty keyframe list",
        });
    }

    let mut parsed = Vec::with_capacity(keyframes.len());
    for keyframe in keyframes.elements() {
        let pair = with_heap(|h| h.get(keyframe.key()).clone()).elide_lvalue_leader_rec();
        let Value::List(pair) = pair else {
            return Err(ExecutorError::type_error_for(
                "list",
                pair.type_name(),
                "keyframe",
            ));
        };
        if pair.len() != 2 {
            return Err(ExecutorError::InvalidArgument {
                arg: "keyframe",
                message: "each keyframe must be [time, value]",
            });
        }

        let time = match with_heap(|h| h.get(pair.elements()[0].key()).clone()) {
            Value::Float(f) => f,
            Value::Integer(n) => n as f64,
            other => {
                return Err(ExecutorError::type_error_for(
                    "number",
                    other.type_name(),
                    "time",
                ));
            }
        };
        let value = with_heap(|h| h.get(pair.elements()[1].key()).clone());
        parsed.push((time, value));
    }

    Ok(parsed)
}

fn parse_keyframe_map(keyframes: &Map) -> Result<Vec<(f64, Value)>, ExecutorError> {
    if keyframes.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "keyframes",
            message: "cannot interpolate empty keyframe map",
        });
    }

    let mut parsed = Vec::with_capacity(keyframes.len());
    for (time, value) in keyframes.iter() {
        let time = match time {
            HashableKey::Integer(n) => *n as f64,
            HashableKey::Float(bits) => HashableKey::float_value(*bits),
            HashableKey::String(_) | HashableKey::Vector(_) => {
                return Err(ExecutorError::InvalidArgument {
                    arg: "keyframes",
                    message: "map keys must be numeric keyframe times",
                });
            }
        };
        let value = with_heap(|h| h.get(value.key()).clone());
        parsed.push((time, value));
    }

    parsed.sort_by(|(a, _), (b, _)| a.total_cmp(b));
    Ok(parsed)
}

async fn lerp_keyframes(
    executor: &mut Executor,
    mut parsed: Vec<(f64, Value)>,
    t: f64,
) -> Result<Value, ExecutorError> {
    if t <= parsed[0].0 {
        return Ok(parsed.remove(0).1);
    }
    if t >= parsed[parsed.len() - 1].0 {
        return Ok(parsed.pop().unwrap().1);
    }

    for window in parsed.windows(2) {
        let [(t0, v0), (t1, v1)] = window else {
            unreachable!()
        };
        if t <= *t1 {
            if *t1 == *t0 {
                return Ok(v1.clone());
            }
            return executor
                .lerp(v0.clone(), v1.clone(), (t - *t0) / (*t1 - *t0))
                .await;
        }
    }

    Ok(parsed.pop().unwrap().1)
}
