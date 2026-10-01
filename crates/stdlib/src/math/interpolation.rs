use std::cell::Ref;

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{HeapKey, VRc, VirtualHeap, with_heap},
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
    let t = crate::read_float(executor, stack_idx, -1, "t")?;
    let keyframes = executor.state.stack(stack_idx).read_at(-2).clone();
    // palettes are read in place: eliding a captured map copies every entry
    // and this runs once per vertex in color callbacks
    let window = with_heap(|heap| with_keyframe_map(heap, &keyframes, |map| keyframe_window(map, t)))?;
    match window? {
        Window::At(value) => Ok(elide_slot(&value)),
        Window::Between(a, b, alpha) => {
            let a = elide_slot(&a).elide_cached_wrappers_rec();
            let b = elide_slot(&b).elide_cached_wrappers_rec();
            // built outside the heap borrow: allocating slots needs the heap mutably
            if let Some(lerped) = with_heap(|heap| lerp_numeric_lists(heap, &a, &b, alpha)) {
                return Ok(Value::List(List::new_with(lerped.into_iter().map(VRc::new))));
            }
            executor.lerp(a, b, alpha).await
        }
    }
}

enum Window {
    At(VRc),
    Between(VRc, VRc, f64),
}

fn with_keyframe_map<R>(
    heap: &VirtualHeap,
    value: &Value,
    f: impl FnOnce(&Map) -> R,
) -> Result<R, ExecutorError> {
    let slot;
    let value = match reference_key(value) {
        Some(key) => {
            slot = resolve_slot(heap, key);
            &*slot
        }
        None => value,
    };
    match value {
        Value::Map(map) => Ok(f(map)),
        other => Err(ExecutorError::type_error_for(
            "map",
            other.type_name(),
            "keyframes",
        )),
    }
}

fn reference_key(value: &Value) -> Option<HeapKey> {
    match value {
        Value::Lvalue(reference) => Some(reference.key()),
        Value::WeakLvalue(reference) => Some(reference.key()),
        Value::Leader(leader) => Some(leader.leader_rc.key()),
        _ => None,
    }
}

/// the slot at the end of a reference chain, borrowed in place
fn resolve_slot(heap: &VirtualHeap, mut key: HeapKey) -> Ref<'_, Value> {
    loop {
        let slot = heap.get(key);
        match reference_key(&slot) {
            Some(next) => key = next,
            None => return slot,
        }
    }
}

fn keyframe_window(keyframes: &Map, t: f64) -> Result<Window, ExecutorError> {
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
            HashableKey::String(_) | HashableKey::List(_) => {
                return Err(ExecutorError::InvalidArgument {
                    arg: "keyframes",
                    message: "map keys must be numeric keyframe times",
                });
            }
        };
        parsed.push((time, value.clone()));
    }
    parsed.sort_by(|(a, _), (b, _)| a.total_cmp(b));

    let (first, last) = (&parsed[0], &parsed[parsed.len() - 1]);
    if t <= first.0 {
        return Ok(Window::At(first.1.clone()));
    }
    if t >= last.0 {
        return Ok(Window::At(last.1.clone()));
    }
    let window = parsed
        .windows(2)
        .find(|window| t <= window[1].0)
        .map(|window| {
            let [(t0, v0), (t1, v1)] = window else {
                unreachable!()
            };
            if t1 == t0 {
                Window::At(v1.clone())
            } else {
                Window::Between(v0.clone(), v1.clone(), (t - t0) / (t1 - t0))
            }
        });
    Ok(window.unwrap_or_else(|| Window::At(last.1.clone())))
}

fn elide_slot(value: &VRc) -> Value {
    with_heap(|heap| resolve_slot(heap, value.key()).clone()).elide_lvalue_leader_rec()
}

/// two lists of numbers (colors, positions) lerp without the generic lerp walk
fn lerp_numeric_lists(heap: &VirtualHeap, a: &Value, b: &Value, t: f64) -> Option<Vec<Value>> {
    let (Value::List(a), Value::List(b)) = (a, b) else {
        return None;
    };
    if a.elements().len() != b.elements().len() {
        return None;
    }
    let number = |element: &VRc| match &*resolve_slot(heap, element.key()) {
        number @ (Value::Integer(_) | Value::Float(_)) => Some(number.clone()),
        _ => None,
    };
    a.elements()
        .iter()
        .zip(b.elements())
        .map(|(x, y)| {
            let (x, y) = (number(x)?, number(y)?);
            Some(if Value::values_equal(&x, &y) {
                x
            } else {
                Value::Float((1.0 - t) * as_f64(&x) + t * as_f64(&y))
            })
        })
        .collect()
}

fn as_f64(value: &Value) -> f64 {
    match value {
        Value::Integer(n) => *n as f64,
        Value::Float(f) => *f,
        _ => unreachable!("numeric lists hold only numbers"),
    }
}
