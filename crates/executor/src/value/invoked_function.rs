use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    executor::{Executor, invoke::fill_defaults},
    value::Value,
};

/// result of calling a function, possibly with labeled arguments.
/// labeled invocations store enough information to recompute the result
/// when a labeled argument changes (e.g. `f(a: 2, 1).a = 3`).
pub struct InvokedFunction {
    pub lambda: Box<Value>,
    pub arguments: SmallVec<[Value; 8]>,
    /// (argument_index, label_name) pairs for labeled args
    pub labels: SmallVec<[(usize, String); 4]>,
    pub cached_result: Cell<Option<Box<Value>>>,
}

impl Clone for InvokedFunction {
    fn clone(&self) -> Self {
        let cached_result = self.cached_result.take();
        let cloned_cached_result =
            cached_result.as_ref().map(|result| Box::new((**result).clone()));
        self.cached_result.set(cached_result);

        Self {
            lambda: self.lambda.clone(),
            arguments: self.arguments.clone(),
            labels: self.labels.clone(),
            cached_result: Cell::new(cloned_cached_result),
        }
    }
}

impl InvokedFunction {
    pub fn value<'a>(
        this: &'a Rc<Self>,
        executor: &'a mut Executor,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let cached = this.cached_result.take();
            let result = match cached {
                Some(result) => result,
                None => {
                    let lambda = match this.lambda.as_ref().clone().elide_lvalue() {
                    Value::Lambda(lambda) => lambda,
                    other => {
                        return Err(ExecutorError::type_error("lambda", other.type_name()));
                    }
                    };

                    let full_args = fill_defaults(this.arguments.iter().cloned().collect(), &lambda);
                    Box::new(executor.eagerly_invoke_lambda(&lambda, &full_args).await?)
                }
            };

            let live = match result.as_ref() {
                Value::InvokedFunction(inv) => InvokedFunction::value(inv, executor).await?,
                Value::InvokedOperator(inv) => crate::value::invoked_operator::InvokedOperator::value(inv, executor).await?,
                other => other.clone(),
            };

            this.cached_result.set(Some(result));
            Ok(live)
        })
    }
}
