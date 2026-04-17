use executor::{
    error::ExecutorError,
    executor::Executor,
    value::{Value, primitive_anim::PrimitiveAnim},
};
use stdlib_macros::stdlib_func;

use crate::read_float;

fn read_time(executor: &Executor, stack_idx: usize, index: i32) -> Result<f64, ExecutorError> {
    let time = read_float(executor, stack_idx, index, "time")?;
    if time < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "time",
            message: "must be non-negative",
        });
    }
    Ok(time)
}

fn progression_from(value: Value) -> Option<Box<Value>> {
    matches!(value, Value::Lambda(_) | Value::Operator(_)).then(|| Box::new(value))
}

// ── primitives ───────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn set(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-1).clone();

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Set {
        candidates: Box::new(candidates),
    }))
}

#[stdlib_func]
pub async fn lerp_anim(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let stack = executor.state.stack(stack_idx);
    let candidates = stack.read_at(-3).clone();
    let rate = stack.read_at(-1).clone();
    let time = read_time(executor, stack_idx, -2)?;

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
        candidates: Box::new(candidates),
        time,
        progression: progression_from(rate),
    }))
}

#[stdlib_func]
pub async fn wait(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let time = read_time(executor, stack_idx, -1)?;
    Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait { time }))
}

// ── follower animations ──────────────────────────────────────────────────────

#[stdlib_func]
pub async fn grow_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("grow-from-centroid follower animation")
}

#[stdlib_func]
pub async fn fade_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("opacity-fade follower animation")
}

#[stdlib_func]
pub async fn write_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("tracing / writing follower animation")
}

#[stdlib_func]
pub async fn transform_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("point-matched transform animation")
}

#[stdlib_func]
pub async fn tag_transform_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("tag-matched transform animation")
}

#[stdlib_func]
pub async fn camera_lerp_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("camera-aware lerp (slerps orientation)")
}

// ── indication ───────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn highlight_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("flash-to-color-and-back indication animation")
}

#[stdlib_func]
pub async fn flash_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("brief write-then-unwrite indication animation")
}

// ── transfer ─────────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn transfer_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("move content from one leader into another by tag")
}

#[stdlib_func]
pub async fn copy_anim(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("duplicate content into another leader by tag")
}

// ── composition ──────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn lagged_map(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("start a list of animations in parallel with a lag between each")
}

#[stdlib_func]
pub async fn anim_time_scale(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("scale the duration of an animation by a factor")
}

#[stdlib_func]
pub async fn anim_delayed(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("offset an animation by a prepended wait")
}

#[stdlib_func]
pub async fn anim_with_rate(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("override the rate function on a primitive animation")
}

// ── rate-function natives (heavy ease curves) ────────────────────────────────

#[stdlib_func]
pub async fn rate_bounce(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let t = read_float(executor, stack_idx, -1, "t")?;
    // stolen from the d3-ease / css easing "easeOutBounce" schedule.
    let out = if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    };
    Ok(Value::Float(out))
}

#[stdlib_func]
pub async fn rate_elastic(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let t = read_float(executor, stack_idx, -1, "t")?;
    let out = if t == 0.0 || t == 1.0 {
        t
    } else {
        let p = 0.3;
        let s = p / 4.0;
        2f64.powf(-10.0 * t) * ((t - s) * std::f64::consts::TAU / p).sin() + 1.0
    };
    Ok(Value::Float(out))
}
