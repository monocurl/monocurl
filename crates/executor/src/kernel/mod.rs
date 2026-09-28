//! the kernel tier: a second execution engine for pure numeric lambdas.
//!
//! the interpreter keeps every local in a heap slot and copies tagged values
//! through a stack, which is the right shape for the scene language but far
//! too slow for a function sampled once per vertex or per pixel. a lambda whose
//! body only does arithmetic, list construction, indexing, control flow, calls
//! to other such lambdas and calls to pure natives is translated once into
//! register code (`compile`) and then run by a small machine (`run`) over plain
//! values (`value`) that owe nothing to the thread-local heap, so a batch of
//! calls can be spread over worker threads (`batch`, `pool`). `tier` owns the
//! compiled bodies and the single-call entry point the interpreter uses.
//!
//! the tier is speculative and never the source of truth: anything it cannot
//! model faults, and a fault hands the whole batch back to the interpreter,
//! which produces the real result or the real error. `KernelMode::Verify`
//! runs both and compares, which is how the tier is tested

mod batch;
pub mod compile;
pub mod convert;
pub mod ir;
pub mod pool;
pub mod run;
mod tier;
pub mod value;

use crate::{heap::with_heap, value::Value};

pub use self::convert::to_value as kernel_value_to_value;
pub use self::ir::KernelIntrinsic;
pub use self::value::KVal;
pub(crate) use self::batch::BatchOutcome;
pub use self::tier::{KernelMode, KernelStats};
pub(crate) use self::tier::KernelTier;

/// equality that distinguishes `1` from `1.0`, which `values_equal` does not:
/// the tier must reproduce the interpreter's result types exactly
pub fn strictly_equal(a: &Value, b: &Value) -> bool {
    match (
        &a.clone().elide_cached_wrappers_rec(),
        &b.clone().elide_cached_wrappers_rec(),
    ) {
        (Value::Nil, Value::Nil) => true,
        (Value::Integer(x), Value::Integer(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()),
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len()
                && x.elements().iter().zip(y.elements()).all(|(a, b)| {
                    with_heap(|heap| strictly_equal(&heap.get(a.key()), &heap.get(b.key())))
                })
        }
        (a, b) => Value::values_equal(a, b),
    }
}
