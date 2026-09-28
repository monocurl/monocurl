//! eager invocation: running a lambda to completion on a temporary execution
//! head, one call at a time or as a batch that reuses one frame (and, for
//! pure numeric bodies, hands the batch to the kernel tier)

use crate::{
    error::ExecutorError,
    heap::heap_replace,
    kernel::{BatchOutcome, KVal, KernelMode, kernel_value_to_value, strictly_equal},
    state::MAX_CALL_DEPTH,
    value::{Value, lambda::Lambda},
};
use smallvec::SmallVec;

use super::{
    ExecSingle, Executor,
    invoke::{prepare_lambda_argument, wrap_reference_argument},
};

impl Executor {
    #[inline]
    pub(crate) fn eagerly_invoke_lambda<'a>(
        &'a mut self,
        lambda: &'a Lambda,
        args: &'a [Value],
        trace_parent_idx: Option<usize>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            if self.state.call_depth >= MAX_CALL_DEPTH {
                self.state.last_stack_idx =
                    trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                return Err(ExecutorError::StackOverflow);
            }
            self.state.call_depth += 1;

            let temp_idx = self
                .state
                .alloc_stack(lambda.ip, None, trace_parent_idx)
                .ok_or_else(|| {
                    self.state.last_stack_idx =
                        trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                    ExecutorError::TooManyActiveAnimations
                })?;
            let stack = self.state.stack_mut(temp_idx);
            for arg in args {
                stack.push(arg.clone());
            }
            for cap in &lambda.captures {
                stack.push(cap.clone());
            }

            match self.run_until_break(temp_idx).await {
                ExecSingle::EndOfHead => {
                    let result = if self.state.stack(temp_idx).stack_len() > 0 {
                        self.state.stack_mut(temp_idx).pop()
                    } else {
                        Value::Nil
                    };
                    self.state.free_stack(temp_idx);
                    self.state.last_stack_idx =
                        trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                    self.state.call_depth -= 1;
                    Ok(result)
                }
                ExecSingle::Play => {
                    self.state.free_stack(temp_idx);
                    self.state.call_depth -= 1;
                    Err(ExecutorError::PlayInLabeledInvocation)
                }
                ExecSingle::Error(e) => {
                    self.state.free_stack(temp_idx);
                    self.state.call_depth -= 1;
                    Err(e)
                }
                ExecSingle::Continue => unreachable!("run_until_break never returns Continue"),
            }
        })
    }

    pub fn eagerly_invoke_lambda_many<'a, A>(
        &'a mut self,
        lambda: &'a Lambda,
        args: &'a [A],
        trace_parent_idx: Option<usize>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Value>, ExecutorError>> + 'a>>
    where
        A: AsRef<[Value]> + 'a,
    {
        self.eagerly_invoke_lambda_many_mapped(
            lambda,
            args,
            trace_parent_idx,
            kernel_value_to_value,
            Ok,
        )
    }

    /// `eagerly_invoke_lambda_many` for callers that immediately reduce each
    /// result to some `T`: `from_kernel` reads it straight off a kernel value
    /// (returning `None` for shapes it does not handle, which then take the
    /// heap route), and `from_value` reads it from an interpreter value. this
    /// spares sampled constructors a heap allocation per result
    pub fn eagerly_invoke_lambda_many_mapped<'a, A, T, K, V>(
        &'a mut self,
        lambda: &'a Lambda,
        args: &'a [A],
        trace_parent_idx: Option<usize>,
        from_kernel: K,
        from_value: V,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<T>, ExecutorError>> + 'a>>
    where
        A: AsRef<[Value]> + 'a,
        T: 'a,
        K: Fn(&KVal) -> Option<T> + 'a,
        V: Fn(Value) -> Result<T, ExecutorError> + 'a,
    {
        Box::pin(async move {
            if args.is_empty() {
                return Ok(Vec::new());
            }
            for call_args in args {
                validate_eager_arg_count(call_args.as_ref().len(), lambda)?;
            }
            let kernel_results = match self.kernel_batch(lambda, args).await {
                BatchOutcome::Results(results) => Some(results),
                BatchOutcome::Interpreter => None,
            };
            if self.kernel_mode() != KernelMode::Verify
                && let Some(results) = kernel_results
            {
                let mut mapped = Vec::with_capacity(results.len());
                for result in &results {
                    let item = match from_kernel(result) {
                        Some(item) => item,
                        None => match kernel_value_to_value(result) {
                            Some(value) => from_value(value)?,
                            None => {
                                self.kernel_result_unconvertible(lambda);
                                return self
                                    .interpret_lambda_many(lambda, args, trace_parent_idx)
                                    .await?
                                    .into_iter()
                                    .map(&from_value)
                                    .collect();
                            }
                        },
                    };
                    mapped.push(item);
                }
                self.state.last_stack_idx =
                    trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                return Ok(mapped);
            }
            let interpreted = self
                .interpret_lambda_many(lambda, args, trace_parent_idx)
                .await;
            if let Some(results) = kernel_results {
                let interpreted = interpreted.as_ref().unwrap_or_else(|error| {
                    panic!(
                        "kernel tier produced results for a batch the interpreter rejects: {error}"
                    )
                });
                for (index, (kernel, interpreter)) in results.iter().zip(interpreted).enumerate() {
                    let kernel = kernel_value_to_value(kernel);
                    assert!(
                        kernel.as_ref().is_some_and(|kernel| strictly_equal(kernel, interpreter)),
                        "kernel tier disagrees with the interpreter on call {index} of lambda at {:?}:\n  kernel:      {}\n  interpreter: {}",
                        lambda.ip,
                        kernel
                            .as_ref()
                            .map(crate::transcript::stringify_for_transcript)
                            .unwrap_or_else(|| "<unconvertible>".to_string()),
                        crate::transcript::stringify_for_transcript(interpreter),
                    );
                }
            }
            interpreted?.into_iter().map(&from_value).collect()
        })
    }

    /// the interpreter's batch path: one reusable frame, reseeded per call
    fn interpret_lambda_many<'a, A>(
        &'a mut self,
        lambda: &'a Lambda,
        args: &'a [A],
        trace_parent_idx: Option<usize>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Value>, ExecutorError>> + 'a>>
    where
        A: AsRef<[Value]> + 'a,
    {
        Box::pin(async move {
            if self.state.call_depth >= MAX_CALL_DEPTH {
                self.state.last_stack_idx =
                    trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                return Err(ExecutorError::StackOverflow);
            }
            self.state.call_depth += 1;

            let temp_idx = self
                .state
                .alloc_stack(lambda.ip, None, trace_parent_idx)
                .ok_or_else(|| {
                    self.state.last_stack_idx =
                        trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                    ExecutorError::TooManyActiveAnimations
                })?;
            let full_arg_len = lambda.required_args as usize + lambda.defaults.len();

            let first_args = args[0].as_ref();
            let prepared_first = prepare_eager_call_args(first_args.iter().cloned(), lambda)?;
            {
                let stack = self.state.stack_mut(temp_idx);
                stack
                    .var_stack
                    .reserve(prepared_first.len() + lambda.captures.len());
                stack.var_stack.extend(prepared_first);
                for cap in &lambda.captures {
                    stack.push(cap.clone());
                }
                stack.set_retained_prefix_len(full_arg_len + lambda.captures.len());
            }

            let mut results = Vec::with_capacity(args.len());
            for call_args in args {
                if let Err(e) = self.reseed_eager_many_stack(temp_idx, lambda, call_args.as_ref()) {
                    self.state.free_stack(temp_idx);
                    self.state.call_depth -= 1;
                    return Err(e);
                }

                match self.run_until_break(temp_idx).await {
                    ExecSingle::EndOfHead => {
                        let raw = if self.state.stack(temp_idx).stack_len()
                            > self.state.stack(temp_idx).retained_prefix_len
                        {
                            self.state.stack_mut(temp_idx).pop()
                        } else {
                            Value::Nil
                        };
                        let result = match self.materialize_cached_value(raw).await {
                            Ok(result) => result,
                            Err(e) => {
                                self.state.free_stack(temp_idx);
                                self.state.call_depth -= 1;
                                return Err(e);
                            }
                        };
                        results.push(result);
                    }
                    ExecSingle::Play => {
                        self.state.free_stack(temp_idx);
                        self.state.call_depth -= 1;
                        return Err(ExecutorError::PlayInLabeledInvocation);
                    }
                    ExecSingle::Error(e) => {
                        self.state.free_stack(temp_idx);
                        self.state.call_depth -= 1;
                        return Err(e);
                    }
                    ExecSingle::Continue => unreachable!("run_until_break never returns Continue"),
                }
            }

            self.state.free_stack(temp_idx);
            self.state.last_stack_idx =
                trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
            self.state.call_depth -= 1;
            Ok(results)
        })
    }

    pub fn invoke_lambda<'a>(
        &'a mut self,
        lambda: &'a Lambda,
        args: Vec<Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            let full_args = prepare_eager_call_args(args, lambda)?;
            self.eagerly_invoke_lambda(lambda, &full_args, None).await
        })
    }

    fn reseed_eager_many_stack(
        &mut self,
        stack_idx: usize,
        lambda: &Lambda,
        args: &[Value],
    ) -> Result<(), ExecutorError> {
        let stack = self.state.stack_mut(stack_idx);
        stack.truncate_to_retained_prefix();
        stack.ip = lambda.ip;
        stack.call_stack.clear();
        stack.label_buffer.clear();
        stack.conditional_flag = false;
        stack.active_child_count = 0;

        for idx in 0..lambda.total_args() {
            let value = if idx < args.len() {
                args[idx].clone()
            } else {
                lambda.defaults[idx - lambda.required_args as usize].clone()
            };

            if lambda.arg_is_reference(idx) {
                match &stack.var_stack[idx] {
                    Value::Lvalue(vrc) => {
                        heap_replace(vrc.key(), value);
                    }
                    Value::WeakLvalue(vweak) => {
                        heap_replace(vweak.key(), value);
                    }
                    _ => {
                        stack.var_stack[idx] = wrap_reference_argument(value, idx < args.len())?;
                    }
                }
            } else {
                stack.var_stack[idx] = value;
            }
        }
        Ok(())
    }
}

fn validate_eager_arg_count(arg_count: usize, lambda: &Lambda) -> Result<(), ExecutorError> {
    let minimum = lambda.required_args as usize;
    let maximum = minimum + lambda.defaults.len();
    if arg_count < minimum {
        return Err(ExecutorError::TooFewArguments {
            minimum,
            got: arg_count,
            operator: false,
        });
    }
    if arg_count > maximum {
        return Err(ExecutorError::TooManyArguments {
            maximum,
            got: arg_count,
            operator: false,
        });
    }
    Ok(())
}

#[inline]
pub(crate) fn fill_defaults(mut args: Vec<Value>, lambda: &Lambda) -> Vec<Value> {
    let total = lambda.total_args();
    if args.len() < total {
        let missing = total - args.len();
        let default_start = lambda.defaults.len().saturating_sub(missing);
        args.extend(lambda.defaults[default_start..].iter().cloned());
    }
    args
}

#[inline]
pub(crate) fn prepare_eager_call_args(
    args: impl IntoIterator<Item = Value>,
    lambda: &Lambda,
) -> Result<SmallVec<[Value; 4]>, ExecutorError> {
    let mut raw = SmallVec::<[Value; 4]>::new();
    raw.extend(args);
    let minimum = lambda.required_args as usize;
    let maximum = lambda.total_args();
    if raw.len() < minimum {
        return Err(ExecutorError::TooFewArguments {
            minimum,
            got: raw.len(),
            operator: false,
        });
    }
    if raw.len() > maximum {
        return Err(ExecutorError::TooManyArguments {
            maximum,
            got: raw.len(),
            operator: false,
        });
    }
    let provided_count = raw.len();
    if raw.len() < maximum {
        let missing = maximum - raw.len();
        let default_start = lambda.defaults.len().saturating_sub(missing);
        raw.extend(lambda.defaults[default_start..].iter().cloned());
    }

    let mut prepared = SmallVec::<[Value; 4]>::with_capacity(raw.len());
    for (arg_idx, arg) in raw.into_iter().enumerate() {
        prepared.push(prepare_lambda_argument(
            lambda,
            arg_idx,
            arg,
            arg_idx < provided_count,
        )?);
    }
    Ok(prepared)
}
