use std::rc::Rc;

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::VRc,
    value::{Value, container::List},
};
use smallvec::smallvec;
use stdlib_macros::stdlib_func;

use crate::read_float;

#[stdlib_func]
pub async fn hsv(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let h = read_float(executor, stack_idx, -4, "h")?;
    let s = read_float(executor, stack_idx, -3, "s")?;
    let v = read_float(executor, stack_idx, -2, "v")?;
    let a = read_float(executor, stack_idx, -1, "a")?;

    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    Ok(Value::List(Rc::new(List::new_with(smallvec![
        VRc::new(Value::Float(r + m)),
        VRc::new(Value::Float(g + m)),
        VRc::new(Value::Float(b + m)),
        VRc::new(Value::Float(a)),
    ]))))
}
