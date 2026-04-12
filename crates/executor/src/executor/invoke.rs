use std::rc::Rc;

use smallvec::SmallVec;
use structs::futures::PeriodicYielder;

use crate::{
    error::ExecutorError,
    state::{MAX_CALL_DEPTH},
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

        let captures: SmallVec<[Value; 4]> =
            stack.var_stack[start..start + capture_count as usize].iter().cloned().collect();
        let defaults: SmallVec<[Value; 1]> =
            stack.var_stack[start + capture_count as usize..stack_len].iter()
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
                return ExecSingle::Error(ExecutorError::type_error("lambda", lambda_val.type_name()))
            }
        };

        let min_args = lambda.required_args as usize;
        let max_args = min_args + lambda.defaults.len();

        if num_args < min_args as u32 {
            return ExecSingle::Error(ExecutorError::TooFewArguments { minimum: min_args, got: num_args as usize });
        }

        if num_args > max_args as u32 {
            return ExecSingle::Error(ExecutorError::TooManyArguments { maximum: max_args, got: num_args as usize });
        }

        if stateful {
            todo!()
        }
        else if labeled {
            let labels = self.drain_labels(stack_idx, section_idx);

            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);

            let full_args = fill_defaults(args, &lambda);

            match self.eagerly_invoke_lambda(&lambda, &full_args).await {
                Ok(result_val) => {
                    let inv =  InvokedFunction::Labeled {
                        lambda: Box::new(Value::Lambda(lambda)),
                        arguments: full_args.into(),
                        labels,
                        cached_result: Some(Box::new(result_val)),
                    };
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::InvokedFunction(Rc::new(inv)));

                    ExecSingle::Continue
                }
                Err(e) => ExecSingle::Error(e),
            }
        }
        else {
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
                return ExecSingle::Error(ExecutorError::type_error("operator", op_val.type_name()))
            }
        };
        let lambda = &operator.0;

        let min_args = lambda.required_args as usize;
        let max_args = min_args + lambda.defaults.len();

        if num_args + 1 < min_args as u32 {
            return ExecSingle::Error(ExecutorError::TooFewArguments { minimum: min_args, got: num_args as usize + 1 });
        }

        if num_args + 1 > max_args as u32 {
            return ExecSingle::Error(ExecutorError::TooManyArguments { maximum: max_args, got: num_args as usize + 1 });
        }

        if stateful {
            todo!()
        }
        else if labeled {
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

            let labels = self.drain_labels(stack_idx, section_idx);

            let n = num_args as usize;
            let stack = self.state.stack_mut(stack_idx);
            let stack_len = stack.stack_len();
            let args: Vec<Value> = stack.var_stack[stack_len - n..stack_len].to_vec();
            stack.pop_n(n);

            let result = self.eagerly_invoke_lambda(&operator.0, &full_args).await;
            let inv = Rc::new(InvokedOperator {
                operator: Box::new(Value::Operator(operator)),
                arguments: args.into(),
                operand: Box::new(operand),
                labels,
                cached_result: result.as_ref().ok().map(|v| Box::new(v.clone())),
            });
            match result {
                Ok(_) => {
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::InvokedOperator(inv));
                    ExecSingle::Continue
                }
                Err(e) => ExecSingle::Error(e),
            }
        }
        else {
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
        match func(&mut self.state, stack_idx).await {
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

    fn setup_lambda_call(&mut self, stack_idx: usize, pushed_args: usize, lambda: &Lambda) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);

        if stack.call_stack.len() >= MAX_CALL_DEPTH {
            return ExecSingle::Error(ExecutorError::StackOverflow);
        }

        for def in &lambda.defaults[pushed_args - lambda.required_args as usize..] {
            stack.push(def.clone());
        }
        for cap in &lambda.captures {
            stack.push(cap.clone());
        }

        stack.call_stack.push(stack.ip);
        stack.ip = lambda.ip;
        self.state.call_depth += 1;

        ExecSingle::Continue
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
            if self.state.call_depth >= MAX_CALL_DEPTH {
                return Err(ExecutorError::StackOverflow);
            }
            self.state.call_depth += 1;

            let temp_idx = self.state.alloc_stack(lambda.ip, None).map_err(|_| ExecutorError::TooManyActiveAnimations)?;
            let stack = self.state.stack_mut(temp_idx);
            for arg in args {
                stack.push(arg.clone());
            }

            for cap in &lambda.captures {
                stack.push(cap.clone());
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

    /// drain label buffer and resolve label names from string pool
    fn drain_labels(
        &mut self,
        stack_idx: usize,
        section_idx: usize
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
}

/// fill default arguments if fewer args were provided
fn fill_defaults(mut args: Vec<Value>, lambda: &Lambda) -> Vec<Value> {
    let total = lambda.required_args as usize + lambda.defaults.len();
    if args.len() < total {
        let missing = total - args.len();
        let default_start = lambda.defaults.len().saturating_sub(missing);
        args.extend(lambda.defaults[default_start..].iter().cloned());
    }
    args
}
