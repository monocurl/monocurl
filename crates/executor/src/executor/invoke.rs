use std::rc::Rc;

use crate::{
    error::ExecutorError,
    heap::{VRc, with_heap},
    state::MAX_CALL_DEPTH,
    value::{
        Value,
        anim_block::AnimBlock,
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
            .map(|def| Value::Lvalue(VRc::new(def.clone())))
            .collect();
        stack.pop_n(total_pop);

        let lambda = Rc::new(Lambda {
            ip: (proto.section, proto.ip),
            captures,
            required_args: proto.required_args as u16,
            defaults,
        });
        self.state.stack_mut(stack_idx).push(Value::Lambda(lambda));
    }

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

            self.state
                .stack_mut(stack_idx)
                .push(Value::Stateful(make_stateful(
                    roots,
                    StatefulNode::LabeledCall {
                        func: Box::new(func_node),
                        args: arg_refs,
                        labels,
                    },
                    read_kind,
                )));
            return ExecSingle::Continue;
        } else if labeled {
            let labels = self.drain_labels(stack_idx, section_idx);

            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);

            let full_args = fill_defaults(args, &lambda);

            match self
                .eagerly_invoke_lambda(&lambda, &full_args, Some(stack_idx))
                .await
            {
                Ok(result_val) => {
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

            self.state
                .stack_mut(stack_idx)
                .push(Value::Stateful(make_stateful(
                    roots,
                    StatefulNode::LabeledOperatorCall {
                        operator: Box::new(op_node),
                        operand: operand_ref,
                        extra_args: extra_arg_refs,
                        labels,
                    },
                    read_kind,
                )));

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
            let full_args = fill_defaults(full_args, &operator.0);

            match self
                .eagerly_invoke_lambda(&operator.0, &full_args, Some(stack_idx))
                .await
            {
                Ok(raw) => match extract_operator_result(raw) {
                    Ok((initial, modified)) => {
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
            let full_args = fill_defaults(full_args, &operator.0);

            match self
                .eagerly_invoke_lambda(&operator.0, &full_args, Some(stack_idx))
                .await
            {
                Ok(raw) => match extract_operator_result(raw) {
                    Ok((initial, modified)) => {
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

    pub(super) fn exec_return(&mut self, stack_idx: usize, stack_delta: i32) -> ExecSingle {
        let ret_val = self.state.stack_mut(stack_idx).pop();

        if matches!(ret_val, Value::Stateful(_)) {
            return ExecSingle::Error(ExecutorError::Other(
                "Cannot return a stateful value".into(),
            ));
        }

        let to_pop = (-stack_delta) as usize;
        let stack = self.state.stack_mut(stack_idx);
        if to_pop > stack.stack_len() {
            return ExecSingle::Error(ExecutorError::Other(
                "internal error: return stack underflow".into(),
            ));
        }
        stack.pop_n(to_pop);
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
            for def in &lambda.defaults[pushed_args - lambda.required_args as usize..] {
                stack.push(def.clone());
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
                    trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_ID);
                return Err(ExecutorError::StackOverflow);
            }
            self.state.call_depth += 1;

            let temp_idx = self
                .state
                .alloc_stack(lambda.ip, None, trace_parent_idx)
                .map_err(|_| {
                    self.state.last_stack_idx =
                        trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_ID);
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
                        self.state.last_stack_idx =
                            trace_parent_idx.unwrap_or(crate::state::ExecutionState::ROOT_STACK_ID);
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
                return ExecSingle::Error(ExecutorError::Other(format!(
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
            crate::value::stateful::stateful_update_cache(stateful, result.clone());
            Ok(result)
        })
    }

    pub(crate) fn eval_stateful<'a>(
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

    async fn resolve_live_value(&mut self, val: Value) -> Result<Value, ExecutorError> {
        match val {
            Value::InvokedFunction(ref inv) => InvokedFunction::value(inv, self).await,
            Value::InvokedOperator(ref inv) => InvokedOperator::value(inv, self).await,
            other => Ok(other),
        }
    }
}

pub(crate) fn fill_defaults(mut args: Vec<Value>, lambda: &Lambda) -> Vec<Value> {
    let total = lambda.required_args as usize + lambda.defaults.len();
    if args.len() < total {
        let missing = total - args.len();
        let default_start = lambda.defaults.len().saturating_sub(missing);
        args.extend(lambda.defaults[default_start..].iter().cloned());
    }
    args
}

pub(crate) fn prepare_eager_call_args(
    args: impl IntoIterator<Item = Value>,
    lambda: &Lambda,
) -> Result<SmallVec<[Value; 4]>, ExecutorError> {
    let mut prepared = SmallVec::<[Value; 4]>::new();
    prepared.extend(args.into_iter().map(|arg| {
        if arg.is_lvalue() {
            arg
        } else {
            Value::Lvalue(VRc::new(arg))
        }
    }));
    let minimum = lambda.required_args as usize;
    let maximum = minimum + lambda.defaults.len();
    if prepared.len() < minimum {
        return Err(ExecutorError::TooFewArguments {
            minimum,
            got: prepared.len(),
            operator: false,
        });
    }
    if prepared.len() > maximum {
        return Err(ExecutorError::TooManyArguments {
            maximum,
            got: prepared.len(),
            operator: false,
        });
    }
    let total = lambda.required_args as usize + lambda.defaults.len();
    if prepared.len() < total {
        let missing = total - prepared.len();
        let default_start = lambda.defaults.len().saturating_sub(missing);
        prepared.extend(lambda.defaults[default_start..].iter().cloned());
    }
    Ok(prepared)
}
