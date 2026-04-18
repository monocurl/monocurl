use std::rc::Rc;

use bytecode::Instruction;

use crate::{
    error::ExecutorError,
    state::LeaderKind,
    value::{
        RcValue, Value,
        container::{List, Map},
        lambda::Operator,
        stateful::{Stateful, StatefulNode},
    },
};

use super::{ExecSingle, Executor, ops::BinOp};

impl Executor {
    #[inline]
    pub(super) async fn execute_instr(
        &mut self,
        section_idx: usize,
        stack_idx: usize,
        instr: Instruction,
    ) -> ExecSingle {
        match instr {
            // ----- push constants -----
            Instruction::PushInt { index } => {
                let val = self.bytecode.sections[section_idx].int_pool[index as usize];
                self.state.stack_mut(stack_idx).push(Value::Integer(val));
            }
            Instruction::PushFloat { index } => {
                let val = self.bytecode.sections[section_idx].float_pool[index as usize];
                self.state.stack_mut(stack_idx).push(Value::Float(val));
            }
            Instruction::PushImaginary { index } => {
                let im = self.bytecode.sections[section_idx].float_pool[index as usize];
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Complex { re: 0.0, im });
            }
            Instruction::PushChar { char: c } => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::String(c.to_string()));
            }
            Instruction::PushString { index } => {
                let s = self.bytecode.sections[section_idx].string_pool[index as usize].clone();
                self.state.stack_mut(stack_idx).push(Value::String(s));
            }
            Instruction::PushEmptyMap => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Map(Rc::new(Map::new())));
            }
            Instruction::PushEmptyVector => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::List(Rc::new(List::new())));
            }

            Instruction::SyncAllLeaders => {
                self.state.sync_all_leaders();
            }

            // ----- variable promotion -----
            Instruction::ConvertVar {} => {
                self.state.promote_to_var(stack_idx);
            }
            Instruction::ConvertMesh { name_index } => {
                let name =
                    self.bytecode.sections[section_idx].string_pool[name_index as usize].clone();
                self.state
                    .promote_to_leader(stack_idx, LeaderKind::Mesh, name);
            }
            Instruction::ConvertParam { name_index } => {
                let name =
                    self.bytecode.sections[section_idx].string_pool[name_index as usize].clone();
                self.state
                    .promote_to_leader(stack_idx, LeaderKind::Param, name);
            }

            // ----- stack reads -----
            Instruction::PushCopy {
                stack_delta,
                mutable,
                pop_tos,
            } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let resolved = if mutable {
                    // want to keep the nested layers of lvalue
                    val.force_elide_lvalue()
                } else {
                    val.elide_lvalue_rec()
                };
                if pop_tos {
                    self.state.stack_mut(stack_idx).pop();
                }
                self.state.stack_mut(stack_idx).push(resolved);
            }
            Instruction::PushLvalue {
                stack_delta,
                force_ephemeral,
            } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let rc = match val {
                    Value::Lvalue(rc) => rc,
                    Value::WeakLvalue(weak) => weak.upgrade().unwrap(),
                    _ => panic!("PushLvalue: not an lvalue at delta {}", stack_delta),
                };
                if force_ephemeral {
                    // keep a strong ref alive for the duration of the section
                    self.state.ephemeral_pool.push(rc.clone());
                }
                // always push a non-owning weak ref
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::WeakLvalue(RcValue::downgrade(&rc)));
            }
            Instruction::PushDereference { stack_delta } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let resolved = resolve_dereference(&val);
                self.state.stack_mut(stack_idx).push(resolved);
            }
            Instruction::PushStateful { stack_delta } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let leader_cell_rc = match val.as_lvalue_rc() {
                    Some(rc) => rc,
                    None => {
                        return ExecSingle::error(stack_idx, ExecutorError::type_error(
                            "state or param variable",
                            val.type_name(),
                        ));
                    }
                };
                if !matches!(&*leader_cell_rc.borrow(), Value::Leader(_)) {
                    return ExecSingle::error(stack_idx, ExecutorError::type_error(
                        "leader",
                        leader_cell_rc.borrow().type_name(),
                    ));
                }
                let stateful = Stateful {
                    roots: vec![leader_cell_rc.clone()],
                    root: StatefulNode::LeaderRef(leader_cell_rc),
                };
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Stateful(stateful));
            }

            // ----- labels -----
            Instruction::BufferLabelOrAttribute { string_index } => {
                self.state
                    .stack_mut(stack_idx)
                    .label_buffer
                    .push(string_index);
            }

            // ----- closures -----
            Instruction::MakeLambda {
                capture_count,
                prototype_index,
            } => {
                self.exec_make_lambda(stack_idx, section_idx, capture_count, prototype_index);
            }
            Instruction::MakeAnim {
                capture_count,
                prototype_index,
            } => {
                self.exec_make_anim(stack_idx, section_idx, capture_count, prototype_index);
            }
            Instruction::MakeOperator => {
                let stack = self.state.stack_mut(stack_idx);
                let val = stack.pop();
                match val {
                    // lambda is Rc<Lambda>, so Operator(rc) is a cheap pointer copy
                    Value::Lambda(rc) => stack.push(Value::Operator(Operator(rc))),
                    _ => {
                        return ExecSingle::error(stack_idx, ExecutorError::type_error(
                            "lambda",
                            val.type_name(),
                        ));
                    }
                }
            }

            // ----- invocation -----
            Instruction::LambdaInvoke {
                stateful,
                labeled,
                num_args,
            } => {
                return self
                    .exec_lambda_invoke(stack_idx, section_idx, stateful, labeled, num_args)
                    .await;
            }
            Instruction::OperatorInvoke {
                stateful,
                labeled,
                num_args,
            } => {
                return self
                    .exec_operator_invoke(stack_idx, section_idx, stateful, labeled, num_args)
                    .await;
            }
            Instruction::ConvertToLiveOperator => {
                return self.exec_convert_to_live_operator(stack_idx);
            }

            // ----- control flow -----
            Instruction::Jump { section, to } => {
                self.state.stack_mut(stack_idx).ip = (section, to);
            }
            Instruction::ConditionalJump { section, to } => {
                let val = self.state.stack_mut(stack_idx).pop();
                let val = match val.elide_wrappers(self).await {
                    Ok(v) => v,
                    Err(e) => return ExecSingle::error(stack_idx, e),
                };
                match val.check_truthy() {
                    Ok(true) => {
                        self.state.stack_mut(stack_idx).ip = (section, to);
                    }
                    Ok(false) => {}
                    Err(e) => return ExecSingle::error(stack_idx, e),
                }
            }
            Instruction::Return { stack_delta } => {
                return self.exec_return(stack_idx, stack_delta);
            }
            Instruction::Pop { count } => {
                self.state.stack_mut(stack_idx).pop_n(count as usize);
            }

            // ----- native -----
            Instruction::NativeInvoke { index, arg_count } => {
                return self.exec_native_invoke(stack_idx, index, arg_count).await;
            }

            // ----- play -----
            Instruction::Play => {
                return self.exec_play(stack_idx);
            }

            // ----- unary -----
            Instruction::Negate => {
                let val = self.state.stack_mut(stack_idx).pop();
                match self.exec_negate(val).await {
                    Ok(v) => self.state.stack_mut(stack_idx).push(v),
                    Err(e) => return ExecSingle::error(stack_idx, e),
                }
            }
            Instruction::Not => {
                let val = match self
                    .state
                    .stack_mut(stack_idx)
                    .pop()
                    .elide_wrappers(self)
                    .await
                {
                    Ok(val) => val,
                    Err(e) => return ExecSingle::error(stack_idx, e),
                };
                match val.check_truthy() {
                    Ok(truthy) => {
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::Integer(!truthy as i64));
                    }
                    Err(e) => return ExecSingle::error(stack_idx, e),
                }
            }

            // ----- subscript / attribute -----
            Instruction::Subscript { mutable } => {
                return self.exec_subscript(stack_idx, mutable);
            }
            Instruction::Attribute {
                mutable,
                string_index,
            } => {
                return self.exec_attribute(stack_idx, section_idx, mutable, string_index);
            }

            // ----- binary -----
            Instruction::Add => return self.exec_binary_op(stack_idx, BinOp::Add).await,
            Instruction::Sub => return self.exec_binary_op(stack_idx, BinOp::Sub).await,
            Instruction::Mul => return self.exec_binary_op(stack_idx, BinOp::Mul).await,
            Instruction::Div => return self.exec_binary_op(stack_idx, BinOp::Div).await,
            Instruction::Power => return self.exec_binary_op(stack_idx, BinOp::Power).await,
            Instruction::Lt => return self.exec_binary_op(stack_idx, BinOp::Lt).await,
            Instruction::Le => return self.exec_binary_op(stack_idx, BinOp::Le).await,
            Instruction::Gt => return self.exec_binary_op(stack_idx, BinOp::Gt).await,
            Instruction::Ge => return self.exec_binary_op(stack_idx, BinOp::Ge).await,
            Instruction::Eq => return self.exec_binary_op(stack_idx, BinOp::Eq).await,
            Instruction::Ne => return self.exec_binary_op(stack_idx, BinOp::Ne).await,
            Instruction::IntDiv => return self.exec_binary_op(stack_idx, BinOp::IntDiv).await,
            Instruction::In => return self.exec_binary_op(stack_idx, BinOp::In).await,
            Instruction::Assign => return self.exec_assign(stack_idx),
            Instruction::AppendAssign => return self.exec_append_assign(stack_idx),
            Instruction::Append => return self.exec_append(stack_idx),

            Instruction::EndOfExecutionHead => {
                self.finish_execution_head(stack_idx);
                return ExecSingle::EndOfHead;
            }
        }

        ExecSingle::Continue
    }
}

/// dereference: given an lvalue pointing to a leader, return the follower value.
/// for non-leader lvalues, return the inner value.
fn resolve_dereference(val: &Value) -> Value {
    let rc = match val {
        Value::Lvalue(rc) => rc.borrow().clone(),
        Value::WeakLvalue(weak) => match weak.upgrade() {
            Some(rc) => rc.borrow().clone(),
            None => return Value::Nil,
        },
        other => return other.clone(),
    };
    match rc {
        Value::Leader(leader) => leader.follower_rc.borrow().clone(),
        other => other,
    }
}
