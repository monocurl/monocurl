use executor::{error::ExecutorError, executor::Executor, value::{Value, primitive_anim::PrimitiveAnim}};
use stdlib_macros::stdlib_func;

#[stdlib_func]
pub async fn wait(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let time = match executor
           .state
           .stack(stack_idx)
           .peek()
       {
           Value::Float(time) => *time,
           other => return Err(ExecutorError::type_error("float", other.type_name())),
       };

    Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait {
        time
    } ))
}
