use std::{cell::Cell, rc::Rc};

use crate::{
    error::ExecutorError,
    state::MAX_CALL_DEPTH,
    value::{
        RcValue, Value,
        anim_block::AnimBlock,
        container::{List, Map},
        invoked_function::InvokedFunction,
        invoked_operator::{InvokedOperator, build_invoked_operator, extract_operator_result},
        lambda::Lambda,
        leader::Leader,
        rc_value,
        stateful::{
            Stateful, StatefulNode, StatefulReadKind, collect_roots_from_value,
            value_into_stateful_node,
        },
    },
};
use smallvec::SmallVec;

use super::{ExecSingle, Executor};

impl Executor {
    pub fn debug_resolve_value<'a>(
        &'a mut self,
        value: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            match value.elide_lvalue() {
                Value::Stateful(stateful) => {
                    println!("STATEFUL KIND {:?}", stateful.read_kind);
                    self.eval_stateful(&stateful).await
                },
                Value::InvokedFunction(invoked) => InvokedFunction::value(&invoked, self).await,
                Value::InvokedOperator(invoked) => InvokedOperator::value(&invoked, self).await,
                Value::Leader(leader) => {
                    let leader_value = self
                        .debug_resolve_value(leader.leader_rc.borrow().clone())
                        .await?;
                    let follower_value = self
                        .debug_resolve_value(leader.follower_rc.borrow().clone())
                        .await?;

                    Ok(Value::Leader(Leader {
                        kind: leader.kind,
                        last_modified_stack: leader.last_modified_stack,
                        locked_by_anim: leader.locked_by_anim,
                        leader_rc: rc_value(leader_value),
                        leader_version: leader.leader_version,
                        follower_rc: rc_value(follower_value),
                        follower_version: leader.follower_version,
                    }))
                }
                Value::List(list) => {
                    let mut resolved = SmallVec::with_capacity(list.elements.len());
                    for value in &list.elements {
                        resolved.push(rc_value(
                            self.debug_resolve_value(value.borrow().clone()).await?,
                        ));
                    }
                    Ok(Value::List(Rc::new(List { elements: resolved })))
                }
                Value::Map(map) => {
                    let mut resolved = Map::new();
                    for (key, value) in map.iter() {
                        resolved.insert(
                            key.clone(),
                            rc_value(self.debug_resolve_value(value.borrow().clone()).await?),
                        );
                    }
                    Ok(Value::Map(Rc::new(resolved)))
                }
                other => Ok(other),
            }
        })
    }

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
            .map(|def| Value::Lvalue(rc_value(def.clone())))
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

        // stack layout: [arg0, arg1, ..., argN-1, lambda]
        let lambda_val = stack.pop().elide_lvalue();
        let lambda = match lambda_val {
            Value::Lambda(rc) => rc,
            _ => {
                return ExecSingle::Error(
                    ExecutorError::type_error("lambda", lambda_val.type_name()),
                );
            }
        };

        let min_args = lambda.required_args as usize;
        let max_args = min_args + lambda.defaults.len();

        if num_args < min_args as u32 {
            return ExecSingle::Error(
                ExecutorError::TooFewArguments {
                    minimum: min_args,
                    got: num_args as usize,
                    operator: false,
                },
            );
        }

        if num_args > max_args as u32 {
            return ExecSingle::Error(
                ExecutorError::TooManyArguments {
                    maximum: max_args,
                    got: num_args as usize,
                    operator: false,
                },
            );
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
            let arg_rcs: Vec<RcValue> = args
                .into_iter()
                .map(|a| {
                    collect_roots_from_value(&a, &mut roots);
                    rc_value(a)
                })
                .collect();
            let read_kind = arg_rcs
                .iter()
                .find_map(|arg| match arg.borrow().force_elide_lvalue() {
                    Value::Stateful(stateful) => Some(stateful.read_kind),
                    other => {
                        println!("Type {:?}", other.type_name());
                        None
                    }
                })
                .expect("No stateful argument despite marked stateful invocation");

            self.state
                .stack_mut(stack_idx)
                .push(Value::Stateful(Stateful::new(
                    roots,
                    StatefulNode::LabeledCall {
                        func: Box::new(func_node),
                        args: arg_rcs,
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
                    let inv = InvokedFunction {
                        lambda: Box::new(Value::Lambda(lambda)),
                        arguments: full_args.into(),
                        labels,
                        cached_result: Cell::new(Some(Box::new(result_val))),
                    };
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::InvokedFunction(Rc::new(inv)));

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

        // stack layout: [operand, arg0, ..., argN-1, operator]
        let op_val = stack.pop().elide_lvalue();
        let operator = match op_val {
            Value::Operator(o) => o,
            _ => {
                return ExecSingle::Error(
                    ExecutorError::type_error("operator", op_val.type_name()),
                );
            }
        };
        let lambda = &operator.0;

        let min_args = lambda.required_args as usize;
        let max_args = min_args + lambda.defaults.len();

        if num_args + 1 < min_args as u32 {
            return ExecSingle::Error(
                ExecutorError::TooFewArguments {
                    minimum: min_args,
                    got: num_args as usize + 1,
                    operator: true,
                },
            );
        }

        if num_args + 1 > max_args as u32 {
            return ExecSingle::Error(
                ExecutorError::TooManyArguments {
                    maximum: max_args,
                    got: num_args as usize + 1,
                    operator: true,
                },
            );
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
            let operand_rc = rc_value(operand);
            let extra_arg_rcs: Vec<RcValue> = extra_args
                .into_iter()
                .map(|a| {
                    collect_roots_from_value(&a, &mut roots);
                    rc_value(a)
                })
                .collect();
            let read_kind = std::iter::once(operand_rc.borrow().clone())
                .chain(
                    extra_arg_rcs.iter()
                        .map(|arg| arg.borrow().force_elide_lvalue())
                )
                .find_map(|value| match value {
                    Value::Stateful(stateful) => Some(stateful.read_kind),
                    _ => None,
                })
                .expect("No stateful argument despite marked stateful invocation");

            self.state
                .stack_mut(stack_idx)
                .push(Value::Stateful(Stateful::new(
                    roots,
                    StatefulNode::LabeledOperatorCall {
                        operator: Box::new(op_node),
                        operand: operand_rc,
                        extra_args: extra_arg_rcs,
                        labels,
                    },
                    read_kind,
                )));

            // skip next instruction (operator invoke, which is not necessary)
            self.state
                .stack_mut(stack_idx)
                .ip.1 += 1;

            return ExecSingle::Continue;
        } else if labeled {
            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);
            let operand = stack.pop();

            let labels = self.drain_labels(stack_idx, section_idx);

            // lambda takes (target, ...extra_args)
            let mut full_args = vec![operand.clone()];
            full_args.extend(args.iter().cloned());
            let full_args = fill_defaults(full_args, &operator.0);

            match self
                .eagerly_invoke_lambda(&operator.0, &full_args, Some(stack_idx))
                .await
            {
                Ok(raw) => match extract_operator_result(raw) {
                    Ok((initial, modified)) => {
                        let inv = Rc::new(build_invoked_operator(
                            Value::Operator(operator),
                            operand,
                            args.into(),
                            labels,
                            initial,
                            modified,
                        ));
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
            self.setup_lambda_call(stack_idx, 1 + num_args as usize, lambda)
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
            return ExecSingle::Error(ExecutorError::Other("Cannot return a stateful value".into()));
        }

        let to_pop = (-stack_delta) as usize;
        let stack = self.state.stack_mut(stack_idx);
        stack.pop_n(to_pop);
        stack.push(ret_val);

        let ret_ip = self.state.stack_mut(stack_idx).call_stack.pop();
        if let Some(ip) = ret_ip {
            self.state.call_depth -= 1;
            self.state.stack_mut(stack_idx).ip = ip;
            ExecSingle::Continue
        } else {
            // running on isolated head
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

            // stack.ip is already the instruction after the call site — use it as return address
            stack.call_stack.push(stack.ip);
            stack.ip = lambda.ip;
        }
        self.state.call_depth += 1;

        ExecSingle::Continue
    }

    /// eagerly call a lambda body and return its result.
    /// used for labeled/stateful invocations.
    /// yields between instructions so the async executor can interrupt if needed.
    /// boxed to break the execute_one ↔ call_lambda_body async recursion cycle.
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
        }) // Box::pin
    }

    /// eagerly invoke a pure lambda with defaults filled in.
    /// exposed for stdlib helpers that need predicate-style callbacks.
    pub fn invoke_lambda<'a>(
        &'a mut self,
        lambda: &'a Lambda,
        args: Vec<Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            let full_args = fill_defaults(args, lambda);
            self.eagerly_invoke_lambda(lambda, &full_args, None).await
        })
    }

    /// drain label buffer and resolve label names from string pool
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

    /// convert the result of a non-labeled operator invoke into a live value.
    /// the operator lambda returns `[initial, modified]`; we push `modified`.
    /// for labeled invocations the stack already has an InvokedOperator — pass through.
    pub(super) fn exec_convert_to_live_operator(&mut self, stack_idx: usize) -> ExecSingle {
        let val = self.state.stack_mut(stack_idx).pop().elide_lvalue();
        match val {
            Value::InvokedOperator(_) => {
                // either labeled path, or the implicit assumption that we are just using that operator result
                self.state.stack_mut(stack_idx).push(val);
            }
            Value::List(ref list) if list.elements.len() == 2 => {
                // non-labeled path: lambda returned [initial, modified]; take modified
                let live = list.elements[1].borrow().clone();
                self.state.stack_mut(stack_idx).push(live);
            }
            Value::List(ref list) => {
                return ExecSingle::Error(
                    ExecutorError::Other(format!(
                        "operator must return a 2-element list, got {}",
                        list.elements.len()
                    )),
                );
            }
            other => {
                return ExecSingle::Error(
                    ExecutorError::type_error("[initial, modified] list", other.type_name()),
                );
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
        stateful: &'a Stateful,
        override_read_kind: StatefulReadKind,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            if let Some(cached) = stateful.cache_valid() {
                return Ok(cached);
            }
            let result = self
                .eval_stateful_node(&stateful.root, override_read_kind)
                .await?;
            stateful.update_cache(result.clone());
            Ok(result)
        })
    }

    /// evaluate a stateful expression using current param follower values.
    /// checks version-based cache first; falls back to full tree evaluation.
    pub(crate) fn eval_stateful<'a>(
        &'a mut self,
        stateful: &'a Stateful,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            self.eval_stateful_read_kind(stateful, stateful.read_kind).await
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
                StatefulNode::LeaderRef(rc) => {
                    let inner = rc.borrow().clone();
                    match inner {
                        Value::Leader(leader) => Ok(match read_kind {
                            StatefulReadKind::Leader => {
                                leader.leader_rc.borrow().clone()
                            },
                            StatefulReadKind::Follower => {
                                leader.follower_rc.borrow().clone()
                            },
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
                    for arg_rc in args {
                        let arg_val = arg_rc.borrow().clone().elide_lvalue();
                        let resolved = match arg_val {
                            Value::Stateful(ref s) => self.eval_stateful_read_kind(s, read_kind).await?,
                            other => other,
                        };
                        evaled.push(resolved);
                    }
                    let full_args = fill_defaults(evaled, &lambda);
                    let result = self
                        .eagerly_invoke_lambda(&lambda, &full_args, None)
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
                        let v = operand.borrow().clone().elide_lvalue();
                        match v {
                            Value::Stateful(ref s) => self.eval_stateful_read_kind(s, read_kind).await?,
                            other => other,
                        }
                    };

                    let mut evaled: Vec<Value> = vec![operand_val];
                    for arg_rc in extra_args {
                        let arg_val = arg_rc.borrow().clone().elide_lvalue();
                        let resolved = match arg_val {
                            Value::Stateful(ref s) => self.eval_stateful(s).await?,
                            other => other,
                        };
                        evaled.push(resolved);
                    }
                    let full_args = fill_defaults(evaled, &operator_inner.0);
                    let raw = self
                        .eagerly_invoke_lambda(&operator_inner.0, &full_args, None)
                        .await?;
                    let (_, modified) = extract_operator_result(raw)?;
                    self.resolve_live_value(modified).await
                }
            }
        })
    }

    /// resolve an InvokedFunction/InvokedOperator to its live concrete value.
    async fn resolve_live_value(&mut self, val: Value) -> Result<Value, ExecutorError> {
        match val {
            Value::InvokedFunction(inv) => InvokedFunction::value(&inv, self).await,
            Value::InvokedOperator(inv) => InvokedOperator::value(&inv, self).await,
            other => Ok(other),
        }
    }
}

/// fill default arguments if fewer args were provided
pub(crate) fn fill_defaults(mut args: Vec<Value>, lambda: &Lambda) -> Vec<Value> {
    let total = lambda.required_args as usize + lambda.defaults.len();
    if args.len() < total {
        let missing = total - args.len();
        let default_start = lambda.defaults.len().saturating_sub(missing);
        args.extend(lambda.defaults[default_start..].iter().cloned());
    }
    args
}
