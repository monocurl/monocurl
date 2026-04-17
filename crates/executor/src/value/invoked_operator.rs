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
    /// operator(operand, args)[0]: the identity embed (needed for lerp rule 5)
    pub unmodified: Cell<Option<Box<Value>>>,
    /// operator(operand, args)[1]: the live transformed value
    pub cached_result: Cell<Option<Box<Value>>>,
}

impl Clone for InvokedOperator {
    fn clone(&self) -> Self {
        let cached_result = self.cached_result.take();
        let cloned_cached_result = cached_result.as_ref().map(|v| Box::new((**v).clone()));
        self.cached_result.set(cached_result);

        let unmodified = self.unmodified.take();
        let cloned_unmodified = unmodified.as_ref().map(|v| Box::new((**v).clone()));
        self.unmodified.set(unmodified);

        Self {
            operator: self.operator.clone(),
            arguments: self.arguments.clone(),
            operand: self.operand.clone(),
            labels: self.labels.clone(),
            unmodified: Cell::new(cloned_unmodified),
            cached_result: Cell::new(cloned_cached_result),
        }
    }
}

impl InvokedOperator {
    /// invalidate both cached values; call when a labeled argument changes.
    pub fn invalidate_cache(&self) {
        self.cached_result.take();
        self.unmodified.take();
    }

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
                        Value::Operator(op) => op,
                        other => {
                            return Err(ExecutorError::type_error("operator", other.type_name()));
                        }
                    };

                    let mut full_args = Vec::with_capacity(this.arguments.len() + 1);
                    full_args.push(this.operand.as_ref().clone());
                    full_args.extend(this.arguments.iter().cloned());
                    let full_args = fill_defaults(full_args, &operator.0);

                    let raw = executor
                        .eagerly_invoke_lambda(&operator.0, &full_args, None)
                        .await?;
                    let (initial, modified) = extract_operator_result(raw)?;
                    this.unmodified.set(Some(Box::new(initial)));
                    Box::new(modified)
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

/// split a `[initial, modified]` list returned by an operator lambda.
pub fn extract_operator_result(result: Value) -> Result<(Value, Value), ExecutorError> {
    match result {
        Value::List(list) if list.elements.len() == 2 => {
            let initial = list.elements[0].borrow().clone();
            let modified = list.elements[1].borrow().clone();
            Ok((initial, modified))
        }
        Value::List(ref list) => Err(ExecutorError::Other(format!(
            "operator must return a 2-element [initial, modified] list, got {} elements",
            list.elements.len()
        ))),
        other => Err(ExecutorError::type_error(
            "[initial, modified] list",
            other.type_name(),
        )),
    }
}

/// build an InvokedOperator with already-computed initial/modified values.
pub fn build_invoked_operator(
    operator: Value,
    operand: Value,
    arguments: SmallVec<[Value; 8]>,
    labels: SmallVec<[(usize, String); 4]>,
    initial: Value,
    modified: Value,
) -> InvokedOperator {
    InvokedOperator {
        operator: Box::new(operator),
        operand: Box::new(operand),
        arguments,
        labels,
        unmodified: Cell::new(Some(Box::new(initial))),
        cached_result: Cell::new(Some(Box::new(modified))),
    }
}
