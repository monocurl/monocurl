use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::with_heap,
    value::{Value, primitive_anim::PrimitiveAnim},
};
use stdlib_macros::stdlib_func;

use super::helpers::{
    delay_primitive, eval_unit_map, list_value, progression_from, read_time, scale_primitive_time,
};

#[stdlib_func]
pub async fn lagged_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let anims = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_cached_wrappers_rec();
    let average_offset = read_time(executor, stack_idx, -2)?;
    let unit_map = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();

    let Value::List(anims) = anims else {
        return Err(ExecutorError::type_error_for(
            "list",
            anims.type_name(),
            "anims",
        ));
    };

    let count = anims.len();
    let mut out = Vec::with_capacity(count);
    for (i, elem) in anims.elements().iter().enumerate() {
        let elem = with_heap(|h| h.get(elem.key()).clone());
        let u = if count <= 1 {
            0.0
        } else {
            i as f64 / (count - 1) as f64
        };
        let mapped = match &unit_map {
            Value::Lambda(_) | Value::Operator(_) => eval_unit_map(executor, &unit_map, u).await?,
            other => {
                return Err(ExecutorError::type_error_for(
                    "lambda / operator",
                    other.type_name(),
                    "unit_map",
                ));
            }
        };
        let delay = mapped * average_offset * (count.saturating_sub(1) as f64);
        out.push(delay_primitive(elem, delay)?);
    }

    Ok(list_value(out))
}

#[stdlib_func]
pub async fn anim_time_scale(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let anim = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_cached_wrappers_rec();
    let factor = read_time(executor, stack_idx, -1)?;
    scale_primitive_time(anim, factor)
}

#[stdlib_func]
pub async fn anim_delayed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let anim = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_cached_wrappers_rec();
    let delay = read_time(executor, stack_idx, -1)?;
    delay_primitive(anim, delay)
}

#[stdlib_func]
pub async fn anim_with_rate(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let anim = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_cached_wrappers_rec();
    let rate = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();
    let rate_ty = rate.type_name();
    let Some(rate) = progression_from(rate) else {
        return Err(ExecutorError::type_error_for(
            "lambda / operator",
            rate_ty,
            "rate",
        ));
    };

    match anim {
        Value::PrimitiveAnim(PrimitiveAnim::Lerp {
            candidates,
            time,
            embed,
            lerp,
            ..
        }) => Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
            candidates,
            time,
            progression: Some(rate),
            embed,
            lerp,
        })),
        Value::PrimitiveAnim(PrimitiveAnim::Set { candidates }) => {
            Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
                candidates,
                time: 0.0,
                progression: Some(rate),
                embed: None,
                lerp: None,
            }))
        }
        Value::PrimitiveAnim(PrimitiveAnim::Wait { time }) => {
            Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait { time }))
        }
        other => Err(ExecutorError::type_error_for(
            "primitive_anim",
            other.type_name(),
            "target",
        )),
    }
}
