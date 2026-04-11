use structs::futures::PeriodicYielder;

use crate::{
    error::ExecutorError,
    value::{
        Value,
        anim_block::AnimBlock,
        invoked_function::InvokedFunction,
        invoked_operator::InvokedOperator,
        lambda::Lambda, rc_value,
    },
};

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

        let captures: Vec<Value> = stack.var_stack[start..start + capture_count as usize].to_vec();
        let defaults: Vec<Value> =
            stack.var_stack[start + capture_count as usize..stack_len].to_vec();
        stack.pop_n(total_pop);

        let lambda = Lambda {
            ip: (proto.section, proto.ip),
            captures,
            required_args: proto.required_args as u16,
            defaults,
        };
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
        let captures: Vec<Value> = stack.var_stack[start..stack_len].to_vec();
        stack.pop_n(capture_count as usize);

        let anim_block = AnimBlock::new(captures, (proto.section, proto.ip));
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
            Value::Lambda(l) => l,
            _ => {
                return ExecSingle::Error(ExecutorError::type_error("lambda", lambda_val.type_name()))
            }
        };

        let labels = self.drain_labels(stack_idx, section_idx, labeled);

        let n = num_args as usize;
        let stack = self.state.stack_mut(stack_idx);
        let stack_len = stack.stack_len();
        let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
        stack.pop_n(n);

        let full_args = fill_defaults(args, &lambda);

        if labeled || stateful {
            match self.eagerly_invoke_lambda(&lambda, &full_args).await {
                Ok(result_val) => {
                    let inv = if labeled {
                        InvokedFunction::Labeled {
                            lambda: Box::new(Value::Lambda(lambda)),
                            arguments: full_args,
                            labels,
                            cached_result: Some(Box::new(result_val)),
                        }
                    } else {
                        InvokedFunction::Unlabeled {
                            result: Box::new(result_val),
                        }
                    };
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::InvokedFunction(inv));
                    ExecSingle::Continue
                }
                Err(e) => ExecSingle::Error(e),
            }
        } else {
            // non-labeled, non-stateful: just push a call frame and continue
            self.setup_lambda_call(stack_idx, &lambda, &full_args);
            ExecSingle::Continue
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
                return ExecSingle::Error(ExecutorError::type_error("operator", op_val.type_name()))
            }
        };

        let labels = self.drain_labels(stack_idx, section_idx, labeled);

        let n = num_args as usize;
        let stack = self.state.stack_mut(stack_idx);
        let stack_len = stack.stack_len();
        let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
        stack.pop_n(n);
        let operand = stack.pop();

        // operator's lambda takes (target, ...extra_args)
        let mut full_args = vec![operand.clone()];
        full_args.extend(args.iter().cloned());
        let full_args = fill_defaults(full_args, &operator.0);

        if labeled || stateful {
            let result = self.eagerly_invoke_lambda(&operator.0, &full_args).await;
            let inv = InvokedOperator {
                operator: Box::new(Value::Operator(operator)),
                arguments: args,
                operand: Box::new(operand),
                labels,
                cached_result: result.as_ref().ok().map(|v| Box::new(v.clone())),
            };
            match result {
                Ok(_) => {
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::InvokedOperator(inv));
                    ExecSingle::Continue
                }
                Err(e) => ExecSingle::Error(e),
            }
        } else {
            self.setup_lambda_call(stack_idx, &operator.0, &full_args);
            ExecSingle::Continue
        }
    }

    pub(super) async fn exec_native_invoke(
        &mut self,
        stack_idx: usize,
        func_index: u16,
        arg_count: u16,
    ) -> ExecSingle {
        let idx = func_index as usize;

        let n = arg_count as usize;
        let stack = self.state.stack_mut(stack_idx);
        let stack_len = stack.stack_len();
        let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
        stack.pop_n(n);

        // resolve lvalues in args before passing to native func
        let resolved: Vec<Value> = args.into_iter().map(|v| v.elide_lvalue()).collect();

        let func = self.native_funcs[idx];
        match func(resolved).await {
            Ok(val) => {
                self.state.stack_mut(stack_idx).push(val);
                ExecSingle::Continue
            }
            Err(e) => ExecSingle::Error(e),
        }
    }

    pub(super) fn exec_return(&mut self, stack_idx: usize, stack_delta: i32) -> ExecSingle {
        let ret_val = self.state.stack_mut(stack_idx).pop();
        let ret_val = ret_val.elide_lvalue();

        let to_pop = (-stack_delta) as usize;
        let stack = self.state.stack_mut(stack_idx);
        stack.pop_n(to_pop);
        stack.push(ret_val);

        if let Some(ret_ip) = stack.call_stack.pop() {
            stack.ip = ret_ip;
            ExecSingle::Continue
        } else {
            ExecSingle::EndOfHead
        }
    }

    /// set up a direct (non-labeled) lambda call by pushing a call frame
    fn setup_lambda_call(&mut self, stack_idx: usize, lambda: &Lambda, args: &[Value]) {
        let stack = self.state.stack_mut(stack_idx);
        stack.call_stack.push(stack.ip);

        for cap in &lambda.captures {
            stack.push(cap.clone());
        }
        for arg in args {
            stack.push(Value::Lvalue(rc_value(arg.clone())));
        }

        stack.ip = lambda.ip;
    }

    /// eagerly call a lambda body and return its result.
    /// used for labeled/stateful invocations.
    /// yields between instructions so the async executor can interrupt if needed.
    /// boxed to break the execute_one ↔ call_lambda_body async recursion cycle.
    fn eagerly_invoke_lambda<'a>(
        &'a mut self,
        lambda: &'a Lambda,
        args: &'a [Value],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            let temp_idx = self.state.alloc_stack(lambda.ip, None);
            let stack = self.state.stack_mut(temp_idx);
            for cap in &lambda.captures {
                stack.push(cap.clone());
            }
            for arg in args {
                stack.push(Value::Lvalue(rc_value(arg.clone())));
            }

            let mut yielder = PeriodicYielder::default();

            loop {
                yielder.tick().await;

                match self.execute_one(temp_idx).await {
                    ExecSingle::Continue => {}
                    ExecSingle::EndOfHead => {
                        let result = if self.state.stack(temp_idx).stack_len() > 0 {
                            self.state.stack_mut(temp_idx).pop()
                        } else {
                            Value::Nil
                        };
                        self.state.free_stack(temp_idx);
                        return Ok(result);
                    }
                    ExecSingle::Play => {
                        self.state.free_stack(temp_idx);
                        return Err(ExecutorError::PlayInLabeledInvocation);
                    }
                    ExecSingle::Error(e) => {
                        self.state.free_stack(temp_idx);
                        return Err(e);
                    }
                }
            }
        }) // Box::pin
    }

    /// drain label buffer and resolve label names from string pool
    fn drain_labels(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        labeled: bool,
    ) -> Vec<(usize, String)> {
        if !labeled {
            return Vec::new();
        }
        let label_indices: Vec<u32> = self
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
}

/// fill default arguments if fewer args were provided
fn fill_defaults(mut args: Vec<Value>, lambda: &Lambda) -> Vec<Value> {
    let total = lambda.required_args as usize + lambda.defaults.len();
    if args.len() < total {
        let missing = total - args.len();
        let default_start = lambda.defaults.len().saturating_sub(missing);
        for def in &lambda.defaults[default_start..] {
            args.push(def.clone());
        }
    }
    args
}
