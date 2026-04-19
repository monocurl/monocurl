use std::rc::Rc;

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::VRc,
    value::{Value, container::List},
};
use smallvec::smallvec;
use stdlib_macros::stdlib_func;

use super::helpers::read_list;

#[stdlib_func]
pub async fn dot(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let u = read_list(executor, stack_idx, -2, "u")?;
    let v = read_list(executor, stack_idx, -1, "v")?;
    if u.len() != v.len() {
        return Err(ExecutorError::ListLengthMismatch {
            op: "dot",
            lhs_len: u.len(),
            rhs_len: v.len(),
        });
    }
    Ok(Value::Float(
        u.iter().zip(v.iter()).map(|(a, b)| a * b).sum(),
    ))
}

#[stdlib_func]
pub async fn cross(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let u = read_list(executor, stack_idx, -2, "u")?;
    let v = read_list(executor, stack_idx, -1, "v")?;
    if u.len() != 3 || v.len() != 3 {
        return Err(ExecutorError::InvalidArgument {
            arg: "u",
            message: "cross product requires 3-vectors",
        });
    }
    let out = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    Ok(Value::List(Rc::new(List::new_with(smallvec![
        VRc::new(Value::Float(out[0])),
        VRc::new(Value::Float(out[1])),
        VRc::new(Value::Float(out[2])),
    ]))))
}
