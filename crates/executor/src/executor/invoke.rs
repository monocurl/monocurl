use std::rc::Rc;

use crate::{
    error::ExecutorError,
    heap::{VRc, with_heap},
    state::MAX_CALL_DEPTH,
    value::{
        Value,
        anim_block::AnimBlock,
        invoked_function::make_invoked_function,
        invoked_operator::{extract_operator_result, make_invoked_operator},
        lambda::Lambda,
        stateful::{StatefulNode, collect_roots_from_value, make_stateful, value_into_stateful_node},
    },
};
use smallvec::SmallVec;

use super::{
    ExecSingle, Executor,
    eager::{fill_defaults, prepare_eager_call_args},
};

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
    /// a plain unlabeled call of a lambda with no default arguments is just a
    /// frame setup, so it never has to suspend. anything else (stateful calls,
    /// labeled calls, defaults to fill in) goes through the asynchronous path
    pub(super) fn try_lambda_invoke(
        &mut self,
        stack_idx: usize,
        stateful: bool,
        labeled: bool,
        num_args: u32,
    ) -> Option<ExecSingle> {
        if stateful || labeled {
            return None;
        }

        let lambda = match self
            .state
            .stack(stack_idx)
            .peek()
            .clone()
            .elide_cached_wrappers()
        {
            Value::Lambda(lambda) if lambda.defaults.is_empty() => lambda,
            // type errors and default filling are reported by the general path
            _ => return None,
        };

        if num_args != u32::from(lambda.required_args) {
            return None;
        }

        self.state.stack_mut(stack_idx).pop();
        if let Some(error) = self.ensure_non_stateful_lambda_args(stack_idx, num_args as usize) {
            return Some(ExecSingle::Error(error));
        }
        if let Some(result) = self.try_kernel_call(stack_idx, &lambda, num_args as usize) {
            return Some(result);
        }

        Some(self.setup_lambda_call(stack_idx, num_args as usize, &lambda))
    }

    pub(super) async fn exec_lambda_invoke(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        stateful: bool,
        labeled: bool,
        num_args: u32,
    ) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);

        let lambda_val = stack.pop().elide_cached_wrappers();
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
            ExecSingle::Continue
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
            ExecSingle::Continue
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
        let func = self.native_funcs[func_index as usize].call;
        let result = func(self, stack_idx).await;
        self.finish_native_invoke(stack_idx, arg_count, result)
    }

    /// natives that cannot suspend are called directly, with no future allocated
    pub(super) fn try_native_invoke(
        &mut self,
        stack_idx: usize,
        func_index: u16,
        arg_count: u16,
    ) -> Option<ExecSingle> {
        let call = self.native_funcs[func_index as usize].call_sync?;
        let result = call(self, stack_idx);
        Some(self.finish_native_invoke(stack_idx, arg_count, result))
    }

    fn finish_native_invoke(
        &mut self,
        stack_idx: usize,
        arg_count: u16,
        result: Result<Value, ExecutorError>,
    ) -> ExecSingle {
        match result {
            Ok(val) => {
                let stack = self.state.stack_mut(stack_idx);
                stack.pop_n(arg_count as usize);
                stack.push(val);
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
            // only reference parameters need rewriting, and the argument is
            // already sitting in the slot, so take it rather than copying it
            for arg_idx in 0..pushed_args {
                if !lambda.arg_is_reference(arg_idx) {
                    continue;
                }
                let slot_idx = arg_start + arg_idx;
                let arg = std::mem::replace(&mut stack.var_stack[slot_idx], Value::Nil);
                stack.var_stack[slot_idx] = match wrap_reference_argument(arg, true) {
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

pub(super) fn wrap_reference_argument(
    arg: Value,
    require_reference_literal: bool,
) -> Result<Value, ExecutorError> {
    if require_reference_literal && !reference_argument_shape_is_allowed(&arg) {
        return Err(invalid_reference_argument());
    }
    Ok(Value::Lvalue(VRc::new(arg)))
}

#[inline(always)]
pub(super) fn prepare_lambda_argument(
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

}

