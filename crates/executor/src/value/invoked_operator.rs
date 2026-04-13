use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    executor::{Executor, invoke::fill_defaults},
    value::{Value, invoked_function::InvokedFunction},
};

/// result of applying an operator to an operand, possibly with labeled arguments.
/// stores enough information to recompute when labels change.
pub struct InvokedOperator {
    pub operator: Box<Value>,
    pub arguments: SmallVec<[Value; 8]>,
    pub operand: Box<Value>,
    /// (argument_index, label_name) pairs
    pub labels: SmallVec<[(usize, String); 4]>,
    pub cached_result: Cell<Option<Box<Value>>>,
}

impl Clone for InvokedOperator {
    fn clone(&self) -> Self {
        let cached_result = self.cached_result.take();
        let cloned_cached_result =
            cached_result.as_ref().map(|result| Box::new((**result).clone()));
        self.cached_result.set(cached_result);

        Self {
            operator: self.operator.clone(),
            arguments: self.arguments.clone(),
            operand: self.operand.clone(),
            labels: self.labels.clone(),
            cached_result: Cell::new(cloned_cached_result),
        }
    }
}


impl InvokedOperator {
    pub fn value<'a>(
        this: &'a Rc<Self>,
        executor: &'a mut Executor,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let cached = this.cached_result.take();
            let result = match cached {
                Some(result) => result,
                None => {
                    let operator = match this.operator.as_ref().clone().elide_lvalue() {
                    Value::Operator(operator) => operator,
                    other => {
                        return Err(ExecutorError::type_error("operator", other.type_name()));
                    }
                    };

                    let mut full_args = Vec::with_capacity(this.arguments.len() + 1);
                    full_args.push(this.operand.as_ref().clone());
                    full_args.extend(this.arguments.iter().cloned());
                    let full_args = fill_defaults(full_args, &operator.0);

                    Box::new(executor.eagerly_invoke_lambda(&operator.0, &full_args).await?)
                }
            };

            let live = match result.as_ref() {
                Value::InvokedFunction(inv) => InvokedFunction::value(inv, executor).await?,
                Value::InvokedOperator(inv) => InvokedOperator::value(inv, executor).await?,
                other => other.clone(),
            };

            this.cached_result.set(Some(result));
            Ok(live)
        })
    }
}
