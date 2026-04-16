use executor::{error::ExecutorError, executor::Executor, value::{Value, primitive_anim::PrimitiveAnim}};
use stdlib_macros::stdlib_func;

use crate::read_float;

#[stdlib_func]
pub async fn wait(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let time = read_float(executor, stack_idx, -1, "time")?;
    if time < 0.0 {
        return Err(ExecutorError::InvalidArgument { arg: "time", message: "must be non-negative" });
    }

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait {
        time
    } ))
}
