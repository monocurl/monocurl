use std::rc::Rc;

use bytecode::Instruction;

use crate::{
    error::ExecutorError,
    heap::with_heap,
    state::LeaderKind,
    value::{
        Value,
        container::{List, Map},
        lambda::Operator,
        stateful::{StatefulNode, StatefulReadKind, make_stateful},
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

            Instruction::ConvertVar { allow_stateful } => {
                if !allow_stateful
                    && matches!(self.state.stack(stack_idx).peek(), Value::Stateful(_))
                {
                    return ExecSingle::Error(ExecutorError::Other(
                        "illegal assignment of stateful value. Stateful values must only be assigned to meshes".into(),
                    ));
                }
                self.state.promote_to_var(stack_idx);
            }
            Instruction::ConvertMesh { name_index } => {
                let name =
                    self.bytecode.sections[section_idx].string_pool[name_index as usize].clone();
                self.state
                    .promote_to_leader(stack_idx, LeaderKind::Mesh, name);
            }
            Instruction::ConvertParam { name_index } => {
                if matches!(
                    self.state.stack(stack_idx).peek().clone().elide_lvalue(),
                    Value::Stateful(_)
                ) {
                    return ExecSingle::Error(ExecutorError::Other(
                        "stateful values can only be assigned to mesh variables".into(),
                    ));
                }
                let name =
                    self.bytecode.sections[section_idx].string_pool[name_index as usize].clone();
                self.state
                    .promote_to_leader(stack_idx, LeaderKind::Param, name);
            }

            Instruction::PushCopy {
                stack_delta,
                mutable,
                pop_tos,
            } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let lvalue_resolved = if mutable {
                    val.force_elide_lvalue()
                } else {
                    val.elide_lvalue().elide_leader()
                };

                if let Value::Stateful(_) = lvalue_resolved {
                    return ExecSingle::Error(ExecutorError::Other(
                        "attempt to copy a stateful value directly. Use $<ident> to use the live value, and *ident to read the current value".into(),
                    ));
                }

                if pop_tos {
                    self.state.stack_mut(stack_idx).pop();
                }
                self.state.stack_mut(stack_idx).push(lvalue_resolved);
            }
            Instruction::PushLvalue {
                stack_delta,
                force_ephemeral,
            } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let vrc = match val {
                    Value::Lvalue(vrc) => vrc,
                    Value::WeakLvalue(vweak) => vweak.upgrade(),
                    _ => panic!("PushLvalue: not an lvalue at delta {}", stack_delta),
                };
                if force_ephemeral {
                    self.state.ephemeral_pool.push(vrc.clone());
                }
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::WeakLvalue(vrc.downgrade()));
            }
            Instruction::PushDereference { stack_delta } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();
                let inner = resolve_leader_value(&val);
                let resolved = match inner {
                    Value::Stateful(ref s) => match self.eval_stateful(s).await {
                        Ok(v) => v,
                        Err(e) => return ExecSingle::Error(e),
                    },
                    other => other,
                };
                self.state.stack_mut(stack_idx).push(resolved);
            }
            Instruction::PushStateful { stack_delta } => {
                let val = self.state.stack(stack_idx).read_at(stack_delta).clone();

                let leader_cell_key = match val.as_lvalue_key() {
                    Some(k) => k,
                    None => {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "param variable",
                            val.type_name(),
                        ));
                    }
                };

                let cell_val = with_heap(|h| h.get(leader_cell_key).clone());
                match cell_val {
                    Value::Leader(ref leader) => {
                        if leader.kind != LeaderKind::Param {
                            // if wrapping a stateful, allow it
                            let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
                            if let Value::Stateful(stateful) = inner {
                                self.state
                                    .stack_mut(stack_idx)
                                    .push(Value::Stateful(stateful));
                                return ExecSingle::Continue;
                            }

                            return ExecSingle::Error(ExecutorError::Other(
                                "$ can only be used with 'param' variables, not 'mesh' (unless the mesh contains a stateful value)  ".into(),
                            ));
                        }
                    }
                    _ => {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "param leader",
                            cell_val.type_name(),
                        ));
                    }
                }

                let stateful = make_stateful(
                    vec![leader_cell_key],
                    StatefulNode::LeaderRef(leader_cell_key),
                    StatefulReadKind::Leader,
                );
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Stateful(stateful));
            }

            Instruction::BufferLabelOrAttribute { string_index } => {
                self.state
                    .stack_mut(stack_idx)
                    .label_buffer
                    .push(string_index);
            }

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
                    Value::Lambda(rc) => stack.push(Value::Operator(Operator(rc))),
                    _ => {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "lambda",
                            val.type_name(),
                        ));
                    }
                }
            }

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

            Instruction::Jump { section, to } => {
                self.state.stack_mut(stack_idx).ip = (section, to);
            }
            Instruction::ConditionalJump { section, to } => {
                let val = self.state.stack_mut(stack_idx).pop();
                let val = match val.elide_wrappers(self).await {
                    Ok(v) => v,
                    Err(e) => return ExecSingle::Error(e),
                };
                match val.check_truthy() {
                    Ok(true) => {
                        self.state.stack_mut(stack_idx).ip = (section, to);
                    }
                    Ok(false) => {}
                    Err(e) => return ExecSingle::Error(e),
                }
            }
            Instruction::Return { stack_delta } => {
                return self.exec_return(stack_idx, stack_delta);
            }
            Instruction::Pop { count } => {
                self.state.stack_mut(stack_idx).pop_n(count as usize);
            }

            Instruction::NativeInvoke { index, arg_count } => {
                return self.exec_native_invoke(stack_idx, index, arg_count).await;
            }

            Instruction::Play => {
                return self.exec_play(stack_idx);
            }

            Instruction::Negate => {
                let val = self.state.stack_mut(stack_idx).pop();
                match self.exec_negate(val).await {
                    Ok(v) => self.state.stack_mut(stack_idx).push(v),
                    Err(e) => return ExecSingle::Error(e),
                }
            }
            Instruction::Not => {
                let val = self.state.stack_mut(stack_idx).pop();
                match self.exec_not(val).await {
                    Ok(v) => self.state.stack_mut(stack_idx).push(v),
                    Err(e) => return ExecSingle::Error(e),
                }
            }

            Instruction::Subscript { mutable } => {
                return self.exec_subscript(stack_idx, mutable);
            }
            Instruction::Attribute {
                mutable,
                string_index,
            } => {
                return self.exec_attribute(stack_idx, section_idx, mutable, string_index);
            }

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

fn resolve_leader_value(val: &Value) -> Value {
    let key = match val {
        Value::Lvalue(vrc) => vrc.key(),
        Value::WeakLvalue(vweak) => vweak.key(),
        other => return other.clone(),
    };
    let inner = with_heap(|h| h.get(key).clone());
    match inner {
        Value::Leader(leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
        other => other,
    }
}
