use std::rc::Rc;

use crate::{
    error::ExecutorError,
    heap::{VRc, heap_replace, with_heap},
    state::MAX_CALL_DEPTH,
    value::{
        Value,
        anim_block::AnimBlock,
        container::{List, Map},
        invoked_function::{InvokedFunction, make_invoked_function},
        invoked_operator::{InvokedOperator, extract_operator_result, make_invoked_operator},
        lambda::Lambda,
        stateful::{
            StatefulNode, StatefulReadKind, collect_roots_from_value, make_stateful,
            value_into_stateful_node,
        },
    },
};
use smallvec::SmallVec;

use super::{ExecSingle, Executor};

impl Executor {
    #[inline]
    pub(super) fn exec_make_lambda(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        capture_count: u16,
        prototype_index: u32,
    ) {
        let proto =
            self.bytecode.sections[section_idx].lambda_prototypes[prototype_index as usize].clone();
        let stack = self.state.stack_mut(stack_idx);

        let total_pop = capture_count as usize + proto.default_arg_count as usize;
        let stack_len = stack.stack_len();
        let start = stack_len - total_pop;

        let captures: SmallVec<[Value; 4]> = stack.var_stack[start..start + capture_count as usize]
            .iter()
            .cloned()
            .collect();
        let defaults: SmallVec<[Value; 1]> = stack.var_stack
            [start + capture_count as usize..stack_len]
            .iter()
            .cloned()
            .collect();
        stack.pop_n(total_pop);

        let lambda = Rc::new(Lambda {
            ip: (proto.section, proto.ip),
            captures,
            required_args: proto.required_args as u16,
            defaults,
            reference_args: proto.reference_args,
            arg_names: proto.arg_names,
        });
        self.state.stack_mut(stack_idx).push(Value::Lambda(lambda));
    }

    #[inline]
    pub(super) fn exec_make_anim(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        capture_count: u16,
        prototype_index: u32,
    ) {
        let proto =
            self.bytecode.sections[section_idx].anim_prototypes[prototype_index as usize].clone();
        let stack = self.state.stack_mut(stack_idx);

        let stack_len = stack.stack_len();
        let start = stack_len - capture_count as usize;
        let captures: SmallVec<[Value; 8]> =
            stack.var_stack[start..stack_len].iter().cloned().collect();
        stack.pop_n(capture_count as usize);

        let anim_block = Rc::new(AnimBlock::new(captures, (proto.section, proto.ip)));
        self.state
            .stack_mut(stack_idx)
            .push(Value::AnimBlock(anim_block));
    }

    #[inline]
    pub(super) async fn exec_lambda_invoke(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        stateful: bool,
        labeled: bool,
        num_args: u32,
    ) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);

        let lambda_val = stack.pop().elide_lvalue();
        let lambda = match lambda_val {
            Value::Lambda(ref rc) => rc.clone(),
            _ => {
                return ExecSingle::Error(ExecutorError::type_error(
                    "lambda",
                    lambda_val.type_name(),
                ));
            }
        };

        let min_args = lambda.required_args as usize;
        let max_args = min_args + lambda.defaults.len();

        if num_args < min_args as u32 {
            return ExecSingle::Error(ExecutorError::TooFewArguments {
                minimum: min_args,
                got: num_args as usize,
                operator: false,
            });
        }
        if num_args > max_args as u32 {
            return ExecSingle::Error(ExecutorError::TooManyArguments {
                maximum: max_args,
                got: num_args as usize,
                operator: false,
            });
        }
        if !stateful
            && let Some(error) = self.ensure_non_stateful_lambda_args(stack_idx, num_args as usize)
        {
            return ExecSingle::Error(error);
        }

        if stateful {
            let labels = if labeled {
                self.drain_labels(stack_idx, section_idx)
            } else {
                SmallVec::new()
            };

            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);

            let (func_node, mut roots) = value_into_stateful_node(Value::Lambda(lambda));
            let arg_refs: Vec<VRc> = args
                .into_iter()
                .map(|a| {
                    collect_roots_from_value(&a, &mut roots);
                    VRc::new(a)
                })
                .collect();

            let read_kind = arg_refs
                .iter()
                .find_map(|arg_ref| {
                    let val = with_heap(|h| h.get(arg_ref.key()).clone()).elide_lvalue();
                    if let Value::Stateful(s) = val {
                        Some(s.cache.read_kind)
                    } else {
                        None
                    }
                })
                .expect("No stateful argument despite marked stateful invocation");

            let stateful = make_stateful(
                roots,
                StatefulNode::LabeledCall {
                    func: Box::new(func_node),
                    args: arg_refs,
                    labels,
                },
                read_kind,
            );
            if let Err(error) = self.eval_stateful(&stateful).await {
                return ExecSingle::Error(error);
            }
            self.state
                .stack_mut(stack_idx)
                .push(Value::Stateful(stateful));
            return ExecSingle::Continue;
        } else if labeled || !lambda.defaults.is_empty() {
            let labels = if labeled {
                self.drain_labels(stack_idx, section_idx)
            } else {
                SmallVec::new()
            };

            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);

            let prepared_args = match prepare_eager_call_args(args.iter().cloned(), &lambda) {
                Ok(args) => args,
                Err(error) => return ExecSingle::Error(error),
            };
            let full_args = fill_defaults(args, &lambda);

            match self
                .eagerly_invoke_lambda(&lambda, &prepared_args, Some(stack_idx))
                .await
            {
                Ok(result_val) => {
                    let result_val = match self.materialize_cached_value(result_val).await {
                        Ok(value) => value,
                        Err(error) => return ExecSingle::Error(error),
                    };
                    let inv = make_invoked_function(
                        Value::Lambda(lambda),
                        full_args.into(),
                        labels,
                        Some(result_val),
                    );
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::InvokedFunction(inv));
                    ExecSingle::Continue
                }
                Err(e) => ExecSingle::Error(e),
            }
        } else {
            self.setup_lambda_call(stack_idx, num_args as usize, &lambda)
        }
    }

    #[inline]
    pub(super) async fn exec_operator_invoke(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        stateful: bool,
        labeled: bool,
        num_args: u32,
    ) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);

        let op_val = stack.pop().elide_lvalue();
        let operator = match op_val {
            Value::Operator(ref o) => o.clone(),
            _ => {
                return ExecSingle::Error(ExecutorError::type_error(
                    "operator",
                    op_val.type_name(),
                ));
            }
        };
        let lambda = &operator.0;

        let min_args = lambda.required_args as usize;
        let max_args = min_args + lambda.defaults.len();

        if num_args + 1 < min_args as u32 {
            return ExecSingle::Error(ExecutorError::TooFewArguments {
                minimum: min_args,
                got: num_args as usize + 1,
                operator: true,
            });
        }
        if num_args + 1 > max_args as u32 {
            return ExecSingle::Error(ExecutorError::TooManyArguments {
                maximum: max_args,
                got: num_args as usize + 1,
                operator: true,
            });
        }

        if stateful {
            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let extra_args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);
            let operand = stack.pop().elide_lvalue();

            let labels = if labeled {
                self.drain_labels(stack_idx, section_idx)
            } else {
                SmallVec::new()
            };

            let (op_node, mut roots) = value_into_stateful_node(Value::Operator(operator));
            collect_roots_from_value(&operand, &mut roots);

            let operand_ref = VRc::new(operand);

            let extra_arg_refs: Vec<VRc> = extra_args
                .into_iter()
                .map(|a| {
                    collect_roots_from_value(&a, &mut roots);
                    VRc::new(a)
                })
                .collect();

            let read_kind = std::iter::once(&operand_ref)
                .chain(extra_arg_refs.iter())
                .find_map(|value_ref| {
                    let val = with_heap(|h| h.get(value_ref.key()).clone()).elide_lvalue();
                    if let Value::Stateful(s) = val {
                        Some(s.cache.read_kind)
                    } else {
                        None
                    }
                })
                .expect("No stateful argument despite marked stateful invocation");

            let stateful = make_stateful(
                roots,
                StatefulNode::LabeledOperatorCall {
                    operator: Box::new(op_node),
                    operand: operand_ref,
                    extra_args: extra_arg_refs,
                    labels,
                },
                read_kind,
            );
            if let Err(error) = self.eval_stateful(&stateful).await {
                return ExecSingle::Error(error);
            }
            self.state
                .stack_mut(stack_idx)
                .push(Value::Stateful(stateful));

            self.state.stack_mut(stack_idx).ip.1 += 1;
            return ExecSingle::Continue;
        } else if labeled {
            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);
            let operand = stack.pop();

            let labels = self.drain_labels(stack_idx, section_idx);

            let mut full_args = vec![operand.clone()];
            full_args.extend(args.iter().cloned());
            let prepared_args =
                match prepare_eager_call_args(full_args.iter().cloned(), &operator.0) {
                    Ok(args) => args,
                    Err(error) => return ExecSingle::Error(error),
                };

            match self
                .eagerly_invoke_lambda(&operator.0, &prepared_args, Some(stack_idx))
                .await
            {
                Ok(raw) => match extract_operator_result(raw) {
                    Ok((initial, modified)) => {
                        let initial = match self.materialize_cached_value(initial).await {
                            Ok(value) => value,
                            Err(error) => return ExecSingle::Error(error),
                        };
                        let modified = match self.materialize_cached_value(modified).await {
                            Ok(value) => value,
                            Err(error) => return ExecSingle::Error(error),
                        };
                        let inv = make_invoked_operator(
                            Value::Operator(operator),
                            operand,
                            args.into(),
                            labels,
                            initial,
                            modified,
                        );
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::InvokedOperator(inv));
                        ExecSingle::Continue
                    }
                    Err(e) => ExecSingle::Error(e),
                },
                Err(e) => ExecSingle::Error(e),
            }
        } else {
            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);
            let operand = stack.pop();

            let mut full_args = vec![operand.clone()];
            full_args.extend(args.iter().cloned());
            let prepared_args =
                match prepare_eager_call_args(full_args.iter().cloned(), &operator.0) {
                    Ok(args) => args,
                    Err(error) => return ExecSingle::Error(error),
                };

            match self
                .eagerly_invoke_lambda(&operator.0, &prepared_args, Some(stack_idx))
                .await
            {
                Ok(raw) => match extract_operator_result(raw) {
                    Ok((initial, modified)) => {
                        let initial = match self.materialize_cached_value(initial).await {
                            Ok(value) => value,
                            Err(error) => return ExecSingle::Error(error),
                        };
                        let modified = match self.materialize_cached_value(modified).await {
                            Ok(value) => value,
                            Err(error) => return ExecSingle::Error(error),
                        };
                        let inv = make_invoked_operator(
                            Value::Operator(operator),
                            operand,
                            args.into(),
                            SmallVec::new(),
                            initial,
                            modified,
                        );
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::InvokedOperator(inv));
                        ExecSingle::Continue
                    }
                    Err(e) => ExecSingle::Error(e),
                },
                Err(e) => ExecSingle::Error(e),
            }
        }
    }

    #[inline]
    pub(super) async fn exec_native_invoke(
        &mut self,
        stack_idx: usize,
        func_index: u16,
        arg_count: u16,
    ) -> ExecSingle {
        let func = self.native_funcs[func_index as usize];
        match func(self, stack_idx).await {
            Ok(val) => {
                self.state.stack_mut(stack_idx).pop_n(arg_count as usize);
                self.state.stack_mut(stack_idx).push(val);
                ExecSingle::Continue
            }
            Err(e) => ExecSingle::Error(e),
        }
    }

    #[inline]
    pub(super) fn exec_return(&mut self, stack_idx: usize, stack_delta: i32) -> ExecSingle {
        let ret_val = self.state.stack_mut(stack_idx).pop();

        if matches!(ret_val, Value::Stateful(_)) {
            return ExecSingle::Error(ExecutorError::invalid_invocation(
                "Cannot return a stateful value",
            ));
        }

        let to_pop = (-stack_delta) as usize;
        let stack = self.state.stack_mut(stack_idx);
        if to_pop > stack.stack_len() {
            return ExecSingle::Error(ExecutorError::internal(
                "internal error: return stack underflow",
            ));
        }
        stack.pop_n_retaining_prefix(to_pop);
        stack.push(ret_val);

        let ret_ip = self.state.stack_mut(stack_idx).call_stack.pop();
        if let Some(ip) = ret_ip {
            self.state.call_depth -= 1;
            self.state.stack_mut(stack_idx).ip = ip;
            ExecSingle::Continue
        } else {
            ExecSingle::EndOfHead
        }
    }

    #[inline(always)]
    fn setup_lambda_call(
        &mut self,
        stack_idx: usize,
        pushed_args: usize,
        lambda: &Lambda,
    ) -> ExecSingle {
        if self.state.stack(stack_idx).call_stack.len() >= MAX_CALL_DEPTH {
            return ExecSingle::Error(ExecutorError::StackOverflow);
        }

        {
            let stack = self.state.stack_mut(stack_idx);
            let arg_start = stack.stack_len() - pushed_args;
            for arg_idx in 0..pushed_args {
                let slot_idx = arg_start + arg_idx;
                let arg = stack.var_stack[slot_idx].clone();
                stack.var_stack[slot_idx] =
                    match prepare_lambda_argument(lambda, arg_idx, arg, true) {
                        Ok(arg) => arg,
                        Err(error) => return ExecSingle::Error(error),
                    };
            }

            let missing = lambda.total_args().saturating_sub(pushed_args);
            let default_start = lambda.defaults.len().saturating_sub(missing);
            for (default_idx, def) in lambda.defaults[default_start..].iter().enumerate() {
                let arg_idx = pushed_args + default_idx;
                let prepared = match prepare_lambda_argument(lambda, arg_idx, def.clone(), false) {
                    Ok(arg) => arg,
                    Err(error) => return ExecSingle::Error(error),
                };
                stack.push(prepared);
            }
            for cap in &lambda.captures {
                stack.push(cap.clone());
            }

            stack.call_stack.push(stack.ip);
            stack.ip = lambda.ip;
        }
        self.state.call_depth += 1;

        ExecSingle::Continue
    }

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
                .map_err(|_| {
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

            loop {
                self.tick_yielder().await;

                match self.execute_one(temp_idx).await {
                    ExecSingle::Continue => {}
                    ExecSingle::EndOfHead => {
                        let result = if self.state.stack(temp_idx).stack_len() > 0 {
                            self.state.stack_mut(temp_idx).pop()
                        } else {
                            Value::Nil
                        };
                        self.state.free_stack(temp_idx);
                        self.state.last_stack_idx = trace_parent_idx
                            .unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                        self.state.call_depth -= 1;
                        return Ok(result);
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
                }
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
        Box::pin(async move {
            if args.is_empty() {
                return Ok(Vec::new());
            }
            for call_args in args {
                validate_eager_arg_count(call_args.as_ref().len(), lambda)?;
            }
            if self.state.call_depth >= MAX_CALL_DEPTH {
                self.state.last_stack_idx =
                    trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_IDX);
                return Err(ExecutorError::StackOverflow);
            }
            self.state.call_depth += 1;

            let temp_idx = self
                .state
                .alloc_stack(lambda.ip, None, trace_parent_idx)
                .map_err(|_| {
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

                loop {
                    self.tick_yielder().await;

                    match self.execute_one(temp_idx).await {
                        ExecSingle::Continue => {}
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
                            break;
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
                    }
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

    #[inline]
    fn drain_labels(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
    ) -> SmallVec<[(usize, String); 4]> {
        let label_indices: SmallVec<[u32; 8]> = self
            .state
            .stack_mut(stack_idx)
            .label_buffer
            .drain(..)
            .collect();
        let string_pool = &self.bytecode.sections[section_idx].string_pool;
        label_indices
            .into_iter()
            .enumerate()
            .filter_map(|(i, si)| {
                if si == u32::MAX {
                    None
                } else {
                    Some((i, string_pool[si as usize].clone()))
                }
            })
            .collect()
    }

    pub(super) fn exec_convert_to_live_operator(&mut self, stack_idx: usize) -> ExecSingle {
        let val = self.state.stack_mut(stack_idx).pop().elide_lvalue();
        match val {
            Value::InvokedOperator(_) => {
                self.state.stack_mut(stack_idx).push(val);
            }
            Value::List(ref list) if list.elements.len() == 2 => {
                let live = with_heap(|h| h.get(list.elements[1].key()).clone());
                self.state.stack_mut(stack_idx).push(live);
            }
            Value::List(ref list) => {
                return ExecSingle::Error(ExecutorError::invalid_invocation(format!(
                    "operator must return a 2-element list, got {}",
                    list.elements.len()
                )));
            }
            other => {
                return ExecSingle::Error(ExecutorError::type_error(
                    "[initial, modified] list",
                    other.type_name(),
                ));
            }
        }
        ExecSingle::Continue
    }

    fn ensure_non_stateful_lambda_args(
        &self,
        stack_idx: usize,
        num_args: usize,
    ) -> Option<ExecutorError> {
        let stack = self.state.stack(stack_idx);
        let stack_len = stack.stack_len();
        stack.var_stack[stack_len - num_args..]
            .iter()
            .any(|arg| matches!(arg, Value::Stateful(_)))
            .then_some(ExecutorError::stateful_illegal_assignment())
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

fn reference_argument_shape_is_allowed(arg: &Value) -> bool {
    match arg {
        Value::Lvalue(_) | Value::WeakLvalue(_) => true,
        Value::List(list) => list.elements().iter().all(|element| {
            let element = with_heap(|h| h.get(element.key()).clone());
            reference_argument_shape_is_allowed(&element)
        }),
        _ => false,
    }
}

fn invalid_reference_argument() -> ExecutorError {
    ExecutorError::invalid_invocation(
        "reference arguments must be explicit &param, &mesh, &reference values, or list literals of references",
    )
}

fn wrap_reference_argument(
    arg: Value,
    require_reference_literal: bool,
) -> Result<Value, ExecutorError> {
    if require_reference_literal && !reference_argument_shape_is_allowed(&arg) {
        return Err(invalid_reference_argument());
    }
    Ok(Value::Lvalue(VRc::new(arg)))
}

#[inline(always)]
fn prepare_lambda_argument(
    lambda: &Lambda,
    arg_idx: usize,
    arg: Value,
    require_reference_literal: bool,
) -> Result<Value, ExecutorError> {
    if lambda.arg_is_reference(arg_idx) {
        wrap_reference_argument(arg, require_reference_literal)
    } else {
        Ok(arg)
    }
}

// ---------------------------------------------------------------------------
// stateful evaluation
// ---------------------------------------------------------------------------

impl Executor {
    pub(crate) fn eval_stateful_read_kind<'a>(
        &'a mut self,
        stateful: &'a crate::value::stateful::Stateful,
        override_read_kind: StatefulReadKind,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            if let Some(cached) = crate::value::stateful::stateful_cache_valid(stateful) {
                return Ok(cached);
            }
            let result = self
                .eval_stateful_node(&stateful.body.root, override_read_kind)
                .await?;
            let result = self.materialize_cached_value(result).await?;
            crate::value::stateful::stateful_update_cache(stateful, result.clone());
            Ok(result)
        })
    }

    pub fn eval_stateful<'a>(
        &'a mut self,
        stateful: &'a crate::value::stateful::Stateful,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            let read_kind = stateful.cache.read_kind;
            self.eval_stateful_read_kind(stateful, read_kind).await
        })
    }

    fn eval_stateful_node<'a>(
        &'a mut self,
        node: &'a StatefulNode,
        read_kind: StatefulReadKind,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            match node {
                StatefulNode::LeaderRef(key) => {
                    let inner = with_heap(|h| h.get(*key).clone());
                    match inner {
                        Value::Leader(leader) => Ok(match read_kind {
                            StatefulReadKind::Leader => {
                                with_heap(|h| h.get(leader.leader_rc.key()).clone())
                            }
                            StatefulReadKind::Follower => {
                                with_heap(|h| h.get(leader.follower_rc.key()).clone())
                            }
                        }),
                        other => Ok(other),
                    }
                }
                StatefulNode::Constant(val) => Ok(*val.clone()),
                StatefulNode::LabeledCall {
                    func,
                    args,
                    labels: _,
                } => {
                    let func_val = self.eval_stateful_node(func, read_kind).await?;
                    let lambda = match func_val.elide_lvalue() {
                        Value::Lambda(rc) => rc,
                        other => {
                            return Err(ExecutorError::type_error("lambda", other.type_name()));
                        }
                    };

                    let mut evaled: Vec<Value> = Vec::with_capacity(args.len());
                    for arg_key in args {
                        let arg_val = with_heap(|h| h.get(arg_key.key()).clone()).elide_lvalue();
                        let resolved = match arg_val {
                            Value::Stateful(ref s) => {
                                self.eval_stateful_read_kind(s, read_kind).await?
                            }
                            other => other,
                        };
                        evaled.push(resolved);
                    }
                    let full_args = prepare_eager_call_args(evaled, &lambda)?;
                    let trace_parent_idx = Some(self.state.last_stack_idx);
                    let result = self
                        .eagerly_invoke_lambda(&lambda, &full_args, trace_parent_idx)
                        .await?;
                    self.resolve_live_value(result).await
                }
                StatefulNode::LabeledOperatorCall {
                    operator,
                    operand,
                    extra_args,
                    ..
                } => {
                    let op_val = self.eval_stateful_node(operator, read_kind).await?;
                    let operator_inner = match op_val.elide_lvalue() {
                        Value::Operator(op) => op,
                        other => {
                            return Err(ExecutorError::type_error("operator", other.type_name()));
                        }
                    };

                    let operand_val = {
                        let v = with_heap(|h| h.get(operand.key()).clone()).elide_lvalue();
                        match v {
                            Value::Stateful(ref s) => {
                                self.eval_stateful_read_kind(s, read_kind).await?
                            }
                            other => other,
                        }
                    };

                    let mut evaled: Vec<Value> = vec![operand_val];
                    for arg_key in extra_args {
                        let arg_val = with_heap(|h| h.get(arg_key.key()).clone()).elide_lvalue();
                        let resolved = match arg_val {
                            Value::Stateful(ref s) => {
                                self.eval_stateful_read_kind(s, read_kind).await?
                            }
                            other => other,
                        };
                        evaled.push(resolved);
                    }
                    let full_args = prepare_eager_call_args(evaled, &operator_inner.0)?;
                    let trace_parent_idx = Some(self.state.last_stack_idx);
                    let raw = self
                        .eagerly_invoke_lambda(&operator_inner.0, &full_args, trace_parent_idx)
                        .await?;
                    let (_, modified) = extract_operator_result(raw)?;
                    self.resolve_live_value(modified).await
                }
            }
        })
    }

    pub(crate) fn materialize_cached_value<'a>(
        &'a mut self,
        val: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            match val {
                Value::Lvalue(vrc) => {
                    let inner = with_heap(|h| h.get(vrc.key()).clone());
                    self.materialize_cached_value(inner).await
                }
                Value::WeakLvalue(vweak) => {
                    let inner = with_heap(|h| h.get(vweak.key()).clone());
                    self.materialize_cached_value(inner).await
                }
                Value::Leader(leader) => {
                    let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
                    self.materialize_cached_value(inner).await
                }
                Value::InvokedFunction(inv) => {
                    let inner = InvokedFunction::value(&inv, self).await?;
                    self.materialize_cached_value(inner).await
                }
                Value::InvokedOperator(inv) => {
                    let inner = InvokedOperator::value(&inv, self).await?;
                    self.materialize_cached_value(inner).await
                }
                Value::Stateful(stateful) => {
                    let inner = self.eval_stateful(&stateful).await?;
                    self.materialize_cached_value(inner).await
                }
                Value::List(list) => {
                    let mut elements = Vec::with_capacity(list.len());
                    for value_ref in list.elements() {
                        let value = with_heap(|h| h.get(value_ref.key()).clone());
                        elements.push(VRc::new(self.materialize_cached_value(value).await?));
                    }
                    Ok(Value::List(List::new_with(elements)))
                }
                Value::Map(map) => {
                    let mut out = Map::new();
                    for key in &map.insertion_order {
                        let value_ref = map
                            .get(key)
                            .expect("map insertion order points to missing entry");
                        let value = with_heap(|h| h.get(value_ref.key()).clone());
                        out.insert(
                            key.clone(),
                            VRc::new(self.materialize_cached_value(value).await?),
                        );
                    }
                    Ok(Value::Map(out))
                }
                other => Ok(other),
            }
        })
    }

    async fn resolve_live_value(&mut self, val: Value) -> Result<Value, ExecutorError> {
        self.materialize_cached_value(val).await
    }
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
