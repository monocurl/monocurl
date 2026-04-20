use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    executor::{Executor, fill_defaults},
    value::{Value, invoked_function::InvokedFunction},
};

use super::rc_cached::RcCached;

#[derive(Clone)]
pub struct InvokedOperatorBody {
    pub operator: Box<Value>,
    pub operand: Box<Value>,
    pub arguments: SmallVec<[Value; 8]>,
    pub labels: SmallVec<[(usize, String); 4]>,
}

pub struct InvOpCache {
    /// operator(operand, args)[0]: the identity embed (needed for lerp rule 5)
    pub unmodified: Cell<Option<Box<Value>>>,
    /// operator(operand, args)[1]: the live transformed value
    pub cached_result: Cell<Option<Box<Value>>>,
}

impl Clone for InvOpCache {
    fn clone(&self) -> Self {
        let cached_result = self.cached_result.take();
        let cloned_cached_result = cached_result.as_ref().map(|v| Box::new((**v).clone()));
        self.cached_result.set(cached_result);

        let unmodified = self.unmodified.take();
        let cloned_unmodified = unmodified.as_ref().map(|v| Box::new((**v).clone()));
        self.unmodified.set(unmodified);

        InvOpCache {
            cached_result: Cell::new(cloned_cached_result),
            unmodified: Cell::new(cloned_unmodified),
        }
    }
}

pub type InvokedOperator = RcCached<InvokedOperatorBody, InvOpCache>;

pub fn make_invoked_operator(
    operator: Value,
    operand: Value,
    arguments: SmallVec<[Value; 8]>,
    labels: SmallVec<[(usize, String); 4]>,
    initial: Value,
    modified: Value,
) -> InvokedOperator {
    RcCached {
        body: Rc::new(InvokedOperatorBody {
            operator: Box::new(operator),
            operand: Box::new(operand),
            arguments,
            labels,
        }),
        cache: InvOpCache {
            unmodified: Cell::new(Some(Box::new(initial))),
            cached_result: Cell::new(Some(Box::new(modified))),
        },
    }
}

/// invalidate both cached values; call when a labeled argument changes.
pub fn invalidate_invoked_operator_cache(inv: &InvokedOperator) {
    inv.cache.cached_result.take();
    inv.cache.unmodified.take();
}

impl InvokedOperator {
    pub fn value<'a>(
        this: &'a InvokedOperator,
        executor: &'a mut Executor,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let cached = this.cache.cached_result.take();
            let result = match cached {
                Some(result) => result,
                None => {
                    let operator = match this.body.operator.as_ref().clone().elide_lvalue() {
                        Value::Operator(op) => op,
                        other => {
                            return Err(ExecutorError::type_error("operator", other.type_name()));
                        }
                    };

                    let mut full_args = Vec::with_capacity(this.body.arguments.len() + 1);
                    full_args.push(this.body.operand.as_ref().clone());
                    full_args.extend(this.body.arguments.iter().cloned());
                    let full_args = fill_defaults(full_args, &operator.0);
                    let trace_parent_idx = Some(executor.state.last_stack_idx);

                    let raw = executor
                        .eagerly_invoke_lambda(&operator.0, &full_args, trace_parent_idx)
                        .await?;
                    let (initial, modified) = extract_operator_result(raw)?;
                    this.cache.unmodified.set(Some(Box::new(initial)));
                    Box::new(modified)
                }
            };

            let live = match result.as_ref() {
                Value::InvokedFunction(inv) => InvokedFunction::value(inv, executor).await?,
                Value::InvokedOperator(inv) => InvokedOperator::value(inv, executor).await?,
                other => other.clone(),
            };

            this.cache.cached_result.set(Some(result));
            Ok(live)
        })
    }
}

/// split a `[initial, modified]` list returned by an operator lambda.
pub fn extract_operator_result(result: Value) -> Result<(Value, Value), ExecutorError> {
    match result {
        Value::List(list) if list.elements.len() == 2 => {
            use crate::heap::with_heap;
            let initial = with_heap(|h| h.get(list.elements[0].key()).clone());
            let modified = with_heap(|h| h.get(list.elements[1].key()).clone());
            Ok((initial, modified))
        }
        Value::List(ref list) => Err(ExecutorError::invalid_invocation(format!(
            "operator must return a 2-element [initial, modified] list, got {} elements",
            list.elements.len()
        ))),
        other => Err(ExecutorError::type_error(
            "[initial, modified] list",
            other.type_name(),
        )),
    }
}
