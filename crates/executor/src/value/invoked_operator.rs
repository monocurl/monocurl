use std::{cell::Cell, future::Future, pin::Pin};

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    executor::{Executor, fill_defaults, prepare_eager_call_args},
    value::Value,
};

use super::rc_cached::RcCached;

#[derive(Clone)]
pub struct InvokedOperatorBody {
    pub operator: Box<Value>,
    pub operand: Box<Value>,
    pub boxed_operand: bool,
    pub arguments: Vec<Value>,
    pub boxed_arguments: SmallVec<[bool; 8]>,
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
    let mut boxed_arguments = SmallVec::with_capacity(arguments.len());
    boxed_arguments.resize(arguments.len(), false);
    RcCached {
        body: InvokedOperatorBody {
            operator: Box::new(operator),
            operand: Box::new(operand),
            boxed_operand: false,
            arguments: arguments.into_vec(),
            boxed_arguments,
            labels,
        },
        cache: InvOpCache {
            unmodified: Cell::new(Some(Box::new(initial))),
            cached_result: Cell::new(Some(Box::new(modified))),
        },
    }
}

#[inline(always)]
fn normalize_operand(body: &InvokedOperatorBody) -> Value {
    let operand = body.operand.as_ref().clone();
    if body.boxed_operand {
        operand.elide_lvalue()
    } else {
        operand
    }
}

#[inline(always)]
fn normalize_argument(body: &InvokedOperatorBody, arg_idx: usize) -> Value {
    let arg = body.arguments[arg_idx].clone();
    if body.boxed_arguments.get(arg_idx).copied().unwrap_or(false) {
        arg.elide_lvalue()
    } else {
        arg
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
                Some(result) => *result,
                None => {
                    let operator = match this.body.operator.as_ref().clone().elide_lvalue() {
                        Value::Operator(op) => op,
                        other => {
                            return Err(ExecutorError::type_error("operator", other.type_name()));
                        }
                    };

                    let mut full_args = Vec::with_capacity(this.body.arguments.len() + 1);
                    full_args.push(normalize_operand(&this.body));
                    full_args.extend(
                        (0..this.body.arguments.len())
                            .map(|arg_idx| normalize_argument(&this.body, arg_idx)),
                    );
                    let full_args = fill_defaults(full_args, &operator.0);
                    let prepared_args = prepare_eager_call_args(full_args, &operator.0)?;
                    let trace_parent_idx = Some(executor.state.last_stack_idx);

                    let raw = executor
                        .eagerly_invoke_lambda(&operator.0, &prepared_args, trace_parent_idx)
                        .await?;
                    let (initial, modified) = extract_operator_result(raw)?;
                    let initial = executor.materialize_cached_value(initial).await?;
                    let modified = executor.materialize_cached_value(modified).await?;
                    this.cache.unmodified.set(Some(Box::new(initial)));
                    modified
                }
            };
            this.cache.cached_result.set(Some(Box::new(result.clone())));
            Ok(result)
        })
    }
}

/// split a `[initial, modified]` list returned by an operator lambda.
pub fn extract_operator_result(result: Value) -> Result<(Value, Value), ExecutorError> {
    fn clone_cached_value(cell: &Cell<Option<Box<Value>>>) -> Option<Value> {
        let cached = cell.take();
        let cloned = cached.as_ref().map(|value| (**value).clone());
        cell.set(cached);
        cloned
    }

    match result {
        Value::List(list) if list.elements.len() == 2 => {
            use crate::heap::with_heap;
            let initial = with_heap(|h| h.get(list.elements[0].key()).clone());
            let modified = with_heap(|h| h.get(list.elements[1].key()).clone());
            Ok((initial, modified))
        }
        Value::InvokedOperator(inv) => {
            let initial = clone_cached_value(&inv.cache.unmodified).ok_or_else(|| {
                ExecutorError::invalid_invocation(
                    "live operator result is missing its cached initial value",
                )
            })?;
            let modified = clone_cached_value(&inv.cache.cached_result).ok_or_else(|| {
                ExecutorError::invalid_invocation(
                    "live operator result is missing its cached modified value",
                )
            })?;
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
