use std::{future::Future, pin::Pin};

use smallvec::SmallVec;

use super::Labels;

use crate::{
    error::ExecutorError,
    executor::{Executor, fill_defaults, prepare_eager_call_args},
    value::Value,
};

use super::rc_cached::{CacheCell, RcCached};

#[derive(Clone)]
pub struct InvokedFunctionBody {
    pub lambda: Box<Value>,
    pub arguments: Vec<Value>,
    pub boxed_arguments: SmallVec<[bool; 8]>,
    pub labels: Labels,
}

#[derive(Clone)]
pub struct InvFuncCache(pub CacheCell);

pub type InvokedFunction = RcCached<InvokedFunctionBody, InvFuncCache>;

pub fn make_invoked_function(
    lambda: Value,
    arguments: SmallVec<[Value; 8]>,
    labels: Labels,
    cached_result: Option<Value>,
) -> InvokedFunction {
    let mut boxed_arguments = SmallVec::with_capacity(arguments.len());
    boxed_arguments.resize(arguments.len(), false);
    RcCached::new(
        InvokedFunctionBody {
            lambda: Box::new(lambda),
            arguments: arguments.into_vec(),
            boxed_arguments,
            labels,
        },
        InvFuncCache(CacheCell::new(cached_result)),
    )
}

#[inline(always)]
fn normalize_argument(body: &InvokedFunctionBody, arg_idx: usize) -> Value {
    let arg = body.arguments[arg_idx].clone();
    if body.boxed_arguments.get(arg_idx).copied().unwrap_or(false) {
        arg.elide_lvalue()
    } else {
        arg
    }
}

impl InvokedFunction {
    pub fn value<'a>(
        this: &'a InvokedFunction,
        executor: &'a mut Executor,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            if let Some(cached) = this.cache.0.cloned() {
                return Ok(cached);
            }
            let result = {
                let lambda = match this.body.lambda.as_ref().clone().elide_lvalue() {
                    Value::Lambda(lambda) => lambda,
                    other => {
                        return Err(ExecutorError::type_error("lambda", other.type_name()));
                    }
                };

                let full_args = fill_defaults(
                    (0..this.body.arguments.len())
                        .map(|arg_idx| normalize_argument(&this.body, arg_idx))
                        .collect(),
                    &lambda,
                );
                let prepared_args = prepare_eager_call_args(full_args, &lambda)?;
                let trace_parent_idx = Some(executor.state.last_stack_idx);
                let raw = executor
                    .eagerly_invoke_lambda(&lambda, &prepared_args, trace_parent_idx)
                    .await?;
                executor.materialize_cached_value(raw).await?
            };
            this.cache.0.fill(result.clone());
            Ok(result)
        })
    }
}
