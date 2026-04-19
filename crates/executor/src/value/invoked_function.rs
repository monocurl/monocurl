use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    executor::{Executor, fill_defaults},
    value::Value,
};

use super::rc_cached::RcCached;

#[derive(Clone)]
pub struct InvokedFunctionBody {
    pub lambda: Box<Value>,
    pub arguments: SmallVec<[Value; 8]>,
    pub labels: SmallVec<[(usize, String); 4]>,
}

pub struct InvFuncCache(pub Cell<Option<Box<Value>>>);

impl Clone for InvFuncCache {
    fn clone(&self) -> Self {
        let cached = self.0.take();
        let cloned = cached.as_ref().map(|v| Box::new((**v).clone()));
        self.0.set(cached);
        InvFuncCache(Cell::new(cloned))
    }
}

pub type InvokedFunction = RcCached<InvokedFunctionBody, InvFuncCache>;

pub fn make_invoked_function(
    lambda: Value,
    arguments: SmallVec<[Value; 8]>,
    labels: SmallVec<[(usize, String); 4]>,
    cached_result: Option<Value>,
) -> InvokedFunction {
    RcCached {
        body: Rc::new(InvokedFunctionBody {
            lambda: Box::new(lambda),
            arguments,
            labels,
        }),
        cache: InvFuncCache(Cell::new(cached_result.map(Box::new))),
    }
}

impl InvokedFunction {
    pub fn value<'a>(
        this: &'a InvokedFunction,
        executor: &'a mut Executor,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let cached = this.cache.0.take();
            let result = match cached {
                Some(result) => result,
                None => {
                    let lambda = match this.body.lambda.as_ref().clone().elide_lvalue() {
                        Value::Lambda(lambda) => lambda,
                        other => {
                            return Err(ExecutorError::type_error("lambda", other.type_name()));
                        }
                    };

                    let full_args =
                        fill_defaults(this.body.arguments.iter().cloned().collect(), &lambda);
                    let trace_parent_idx = Some(executor.state.last_stack_idx);
                    Box::new(
                        executor
                            .eagerly_invoke_lambda(&lambda, &full_args, trace_parent_idx)
                            .await?,
                    )
                }
            };

            let live = match result.as_ref() {
                Value::InvokedFunction(inv) => InvokedFunction::value(inv, executor).await?,
                Value::InvokedOperator(inv) => {
                    crate::value::invoked_operator::InvokedOperator::value(inv, executor).await?
                }
                other => other.clone(),
            };

            this.cache.0.set(Some(result));
            Ok(live)
        })
    }
}
