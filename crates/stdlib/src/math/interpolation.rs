use executor::{error::ExecutorError, executor::Executor, heap::with_heap, value::Value};
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

    let Value::List(keyframes) = keyframes else {
        return Err(ExecutorError::type_error_for(
            "list",
            keyframes.type_name(),
            "keyframes",
        ));
    };
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
