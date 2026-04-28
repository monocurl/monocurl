use crate::{
    error::ExecutorError,
    heap::{HeapKey, VRc, VWeak, heap_replace, with_heap, with_heap_mut},
    state::LeaderKind,
    value::{
        Value,
        container::HashableKey,
        stateful::{StatefulNode, reset_stateful_cache},
    },
};

use super::{ExecSingle, Executor};

fn follow_heap_lvalues(mut key: HeapKey) -> (HeapKey, Value) {
    let mut value = with_heap(|h| h.get(key).clone());
    while let Some(next_key) = value.as_lvalue_key() {
        key = next_key;
        value = with_heap(|h| h.get(key).clone());
    }
    (key, value)
}

fn retained_lvalue(key: HeapKey) -> Value {
    Value::Lvalue(VRc::retain_key(key))
}

impl Executor {
    fn exec_assign_dfs(&mut self, lhs: Value, rhs: Value, stack_idx: usize) -> ExecSingle {
        if let Value::List(llhs) = &lhs {
            return match &rhs {
                Value::List(lrhs) if llhs.len() == lrhs.len() => {
                    for (lk, rk) in llhs.elements.iter().zip(lrhs.elements.iter()) {
                        let lv = with_heap(|h| h.get(lk.key()).clone());
                        let rv = with_heap(|h| h.get(rk.key()).clone());
                        let res = self.exec_assign_dfs(lv, rv, stack_idx);
                        if let ExecSingle::Error(_) = res {
                            return res;
                        }
                    }
                    ExecSingle::Continue
                }
                Value::List(lrhs) => ExecSingle::Error(ExecutorError::DestructuringError {
                    lhs_size: llhs.len(),
                    rhs_size: Some(lrhs.len()),
                    rhs_type: rhs.type_name(),
                }),
                _ => ExecSingle::Error(ExecutorError::DestructuringError {
                    lhs_size: llhs.len(),
                    rhs_size: None,
                    rhs_type: rhs.type_name(),
                }),
            };
        }

        let key = match lhs.as_lvalue_key() {
            Some(k) => k,
            None => return ExecSingle::Error(ExecutorError::CannotAssignTo(lhs.type_name())),
        };

        let (key, target) = follow_heap_lvalues(key);

        match target {
            Value::Leader(leader) => {
                let rhs = rhs.elide_lvalue_leader_rec();
                if matches!(rhs, Value::Stateful(_)) && leader.kind != LeaderKind::Mesh {
                    return ExecSingle::Error(ExecutorError::stateful_requires_mesh_assignment());
                }
                heap_replace(leader.leader_rc.key(), rhs);
                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(key) {
                        l.last_modified_stack = Some(stack_idx);
                        l.leader_version += 1;
                    }
                });
            }
            _ => {
                if matches!(rhs, Value::Stateful(_)) {
                    return ExecSingle::Error(ExecutorError::stateful_requires_mesh_assignment());
                }
                heap_replace(key, rhs.elide_lvalue_leader_rec());
            }
        }

        ExecSingle::Continue
    }

    pub(super) fn exec_assign(&mut self, stack_idx: usize) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let rhs = stack.pop().elide_lvalue_leader_rec();
        let lhs = stack.pop();
        let assigned = lhs.clone();

        let ret = self.exec_assign_dfs(lhs, rhs, stack_idx);
        self.state.stack_mut(stack_idx).push(assigned);
        ret
    }

    pub(super) fn exec_append(&mut self, stack_idx: usize) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let rhs = stack.pop();
        let lhs = stack.pop();

        if matches!(rhs, Value::Stateful(_)) {
            return ExecSingle::Error(ExecutorError::stateful_cannot_append());
        }

        match lhs {
            Value::List(mut list) => {
                list.elements.push(VRc::new(rhs));
                self.state.stack_mut(stack_idx).push(Value::List(list));
                ExecSingle::Continue
            }
            other => ExecSingle::Error(ExecutorError::CannotSubscript(other.type_name())),
        }
    }

    pub(super) fn exec_append_assign(&mut self, stack_idx: usize) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let rhs = stack.pop();
        let lhs = stack.pop();

        let key = match lhs.as_lvalue_key() {
            Some(k) => k,
            None => {
                return ExecSingle::Error(ExecutorError::invalid_lvalue("append-assign"));
            }
        };

        if matches!(rhs, Value::Stateful(_)) {
            return ExecSingle::Error(ExecutorError::stateful_cannot_append());
        }

        let (key, base_val) = follow_heap_lvalues(key);
        let appended_key = match base_val {
            Value::List(mut list) => {
                list.elements.push(VRc::new(rhs.elide_lvalue_leader_rec()));
                heap_replace(key, Value::List(list));
                key
            }
            Value::Leader(leader) => {
                let (inner_key, inner_val) = follow_heap_lvalues(leader.leader_rc.key());
                let Value::List(mut list) = inner_val else {
                    return ExecSingle::Error(ExecutorError::type_error(
                        "list",
                        inner_val.type_name(),
                    ));
                };

                list.elements.push(VRc::new(rhs.elide_lvalue_leader_rec()));
                heap_replace(inner_key, Value::List(list));

                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(key) {
                        l.last_modified_stack = Some(stack_idx);
                        l.leader_version += 1;
                    }
                });

                inner_key
            }
            _ => {
                return ExecSingle::Error(ExecutorError::type_error("list", base_val.type_name()));
            }
        };
        self.state
            .stack_mut(stack_idx)
            .push(Value::WeakLvalue(VWeak::from(appended_key)));
        ExecSingle::Continue
    }

    pub(super) fn exec_subscript(&mut self, stack_idx: usize, mutable: bool) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let index = stack.pop();
        let base = stack.pop();

        let index = index.elide_cached_wrappers_rec();

        if mutable {
            let base_key = match base.as_lvalue_key() {
                Some(k) => k,
                None => {
                    return ExecSingle::Error(ExecutorError::CannotSubscript(base.type_name()));
                }
            };

            let (base_key, base_val) = follow_heap_lvalues(base_key);
            if let Value::Leader(_) = &base_val {
                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(base_key) {
                        l.last_modified_stack = Some(stack_idx);
                        l.leader_version += 1;
                    }
                });
            }

            match base_val {
                Value::List(mut list) => {
                    let Value::Integer(idx) = index else {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "int",
                            index.type_name(),
                        ));
                    };
                    let idx = idx as usize;
                    if idx >= list.elements.len() {
                        return ExecSingle::Error(ExecutorError::IndexOutOfBounds {
                            index: idx,
                            len: list.elements.len(),
                        });
                    }

                    let key = list.elements[idx].make_mut();
                    heap_replace(base_key, Value::List(list));

                    self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                }
                Value::Map(mut map) => {
                    let key_hash = match HashableKey::try_from_value(&index) {
                        Ok(k) => k,
                        Err(e) => return ExecSingle::Error(e),
                    };

                    let key = {
                        match map.get_mut(&key_hash) {
                            Some(value_ref) => value_ref.make_mut(),
                            None => {
                                let new_ref = VRc::new(Value::Nil);
                                let key = new_ref.key();
                                map.insert(key_hash, new_ref);
                                key
                            }
                        }
                    };
                    heap_replace(base_key, Value::Map(map));
                    self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                }
                Value::Leader(leader) => {
                    // push a weak lvalue for the leader's inner slot, then recurse
                    let stack = self.state.stack_mut(stack_idx);
                    stack.push(Value::Lvalue(leader.leader_rc.clone()));
                    stack.push(index);
                    return self.exec_subscript(stack_idx, true);
                }
                _ => {
                    return ExecSingle::Error(ExecutorError::CannotSubscript(base_val.type_name()));
                }
            }
        } else {
            if matches!(base, Value::Stateful(_)) || matches!(index, Value::Stateful(_)) {
                return ExecSingle::Error(ExecutorError::stateful_subscript());
            }

            let base = base.elide_cached_wrappers_rec();
            match &base {
                Value::List(list) => {
                    let Value::Integer(idx) = index else {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "int",
                            index.type_name(),
                        ));
                    };
                    let idx = idx as usize;
                    if idx >= list.elements.len() {
                        return ExecSingle::Error(ExecutorError::IndexOutOfBounds {
                            index: idx,
                            len: list.elements.len(),
                        });
                    }
                    let val = with_heap(|h| h.get(list.elements[idx].key()).clone());
                    self.state.stack_mut(stack_idx).push(val);
                }
                Value::Map(map) => {
                    let key_hash = match HashableKey::try_from_value(&index) {
                        Ok(k) => k,
                        Err(e) => return ExecSingle::Error(e),
                    };
                    let val = map
                        .get(&key_hash)
                        .map(|k| with_heap(|h| h.get(k.key()).clone()))
                        .unwrap_or(Value::Nil);
                    self.state.stack_mut(stack_idx).push(val);
                }
                Value::String(s) => {
                    let Value::Integer(idx) = index else {
                        return ExecSingle::Error(ExecutorError::type_error(
                            "int",
                            index.type_name(),
                        ));
                    };
                    let idx = idx as usize;
                    let ch = s.chars().nth(idx).unwrap_or('\0');
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::String(ch.to_string()));
                }
                Value::Leader(leader) => {
                    let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
                    self.state.stack_mut(stack_idx).push(inner);
                    self.state.stack_mut(stack_idx).push(index);
                    return self.exec_subscript(stack_idx, false);
                }
                _ => {
                    return ExecSingle::Error(ExecutorError::CannotSubscript(base.type_name()));
                }
            }
        }

        ExecSingle::Continue
    }

    pub(super) fn exec_attribute(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        mutable: bool,
        string_index: u32,
    ) -> ExecSingle {
        let attr_name =
            self.bytecode.sections[section_idx].string_pool[string_index as usize].clone();
        let stack = self.state.stack_mut(stack_idx);
        let base = stack.pop();

        if mutable {
            let base_key = match base.as_lvalue_key() {
                Some(k) => k,
                None => {
                    return ExecSingle::Error(ExecutorError::CannotAttribute(base.type_name()));
                }
            };

            let (base_key, base_val) = follow_heap_lvalues(base_key);
            if let Value::Leader(_) = &base_val {
                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(base_key) {
                        l.last_modified_stack = Some(stack_idx);
                        l.leader_version += 1;
                    }
                });
            }

            match base_val {
                Value::Leader(leader) => {
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::Lvalue(leader.leader_rc.clone()));
                    return self.exec_attribute(stack_idx, section_idx, true, string_index);
                }
                Value::InvokedFunction(mut inv) => {
                    let label_idx = inv.body.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let key = {
                            let body = &mut inv.body;
                            let key = body.arguments[arg_idx].make_mut_lvalue();
                            body.boxed_arguments.resize(body.arguments.len(), false);
                            body.boxed_arguments[arg_idx] = true;
                            key
                        };
                        inv.cache.0.take();
                        heap_replace(base_key, Value::InvokedFunction(inv));
                        self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                    } else {
                        return ExecSingle::Error(ExecutorError::missing_labeled_argument(
                            attr_name.clone(),
                        ));
                    }
                }
                Value::InvokedOperator(mut inv) => {
                    let label_idx = inv.body.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let key = {
                            let body = &mut inv.body;
                            let key = body.arguments[arg_idx].make_mut_lvalue();
                            body.boxed_arguments.resize(body.arguments.len(), false);
                            body.boxed_arguments[arg_idx] = true;
                            key
                        };
                        inv.cache.cached_result.take();
                        inv.cache.unmodified.take();
                        heap_replace(base_key, Value::InvokedOperator(inv));
                        self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                    } else {
                        let key = {
                            let body = &mut inv.body;
                            let key = body.operand.as_mut().make_mut_lvalue();
                            body.boxed_operand = true;
                            key
                        };
                        inv.cache.cached_result.take();
                        inv.cache.unmodified.take();
                        heap_replace(base_key, Value::InvokedOperator(inv));
                        self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                        return self.exec_attribute(stack_idx, section_idx, true, string_index);
                    }
                }
                Value::Stateful(mut stateful) => match &stateful.body.root {
                    StatefulNode::LabeledCall {
                        labels, args: _, ..
                    } => {
                        let label_idx = labels.iter().find(|(_, name)| name == &attr_name);
                        if let Some(&(arg_idx, _)) = label_idx {
                            let key = {
                                let body = &mut stateful.body;
                                let StatefulNode::LabeledCall { args, .. } = &mut body.root else {
                                    unreachable!();
                                };
                                args[arg_idx].make_mut()
                            };
                            reset_stateful_cache(&stateful);
                            heap_replace(base_key, Value::Stateful(stateful));
                            self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                        } else {
                            return ExecSingle::Error(ExecutorError::missing_labeled_argument(
                                attr_name.clone(),
                            ));
                        }
                    }
                    StatefulNode::LabeledOperatorCall {
                        labels,
                        operand: _,
                        extra_args: _,
                        ..
                    } => {
                        let label_idx = labels.iter().find(|(_, name)| name == &attr_name);
                        if let Some(&(arg_idx, _)) = label_idx {
                            let key = {
                                let body = &mut stateful.body;
                                let StatefulNode::LabeledOperatorCall { extra_args, .. } =
                                    &mut body.root
                                else {
                                    unreachable!();
                                };
                                extra_args[arg_idx].make_mut()
                            };
                            reset_stateful_cache(&stateful);
                            heap_replace(base_key, Value::Stateful(stateful));
                            self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                        } else {
                            let key = {
                                let body = &mut stateful.body;
                                let StatefulNode::LabeledOperatorCall { operand, .. } =
                                    &mut body.root
                                else {
                                    unreachable!();
                                };
                                operand.make_mut()
                            };
                            reset_stateful_cache(&stateful);
                            heap_replace(base_key, Value::Stateful(stateful));
                            self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                            return self.exec_attribute(stack_idx, section_idx, true, string_index);
                        }
                    }
                    _ => {
                        return ExecSingle::Error(ExecutorError::CannotAttribute(
                            "stateful expression".into(),
                        ));
                    }
                },
                _ => {
                    return ExecSingle::Error(ExecutorError::CannotAttribute(base_val.type_name()));
                }
            }
        } else {
            let base = base.elide_lvalue();
            match &base {
                Value::Leader(leader) => {
                    let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
                    self.state.stack_mut(stack_idx).push(inner);
                    return self.exec_attribute(stack_idx, section_idx, false, string_index);
                }
                Value::InvokedFunction(inv) => {
                    let label_idx = inv.body.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let val = inv.body.arguments[arg_idx].clone().elide_lvalue();
                        self.state.stack_mut(stack_idx).push(val);
                    } else {
                        return ExecSingle::Error(ExecutorError::missing_labeled_argument(
                            attr_name.clone(),
                        ));
                    }
                }
                Value::InvokedOperator(inv) => {
                    let label_idx = inv.body.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let val = inv.body.arguments[arg_idx].clone().elide_lvalue();
                        self.state.stack_mut(stack_idx).push(val);
                    } else {
                        self.state
                            .stack_mut(stack_idx)
                            .push(inv.body.operand.as_ref().clone().elide_lvalue());
                        return self.exec_attribute(stack_idx, section_idx, false, string_index);
                    }
                }
                Value::Stateful(stateful) => match &stateful.body.root {
                    StatefulNode::LabeledCall { labels, args, .. } => {
                        let label_idx = labels.iter().find(|(_, name)| name == &attr_name);
                        if let Some(&(arg_idx, _)) = label_idx {
                            let val =
                                with_heap(|h| h.get(args[arg_idx].key()).clone()).elide_lvalue();
                            self.state.stack_mut(stack_idx).push(val);
                        } else {
                            return ExecSingle::Error(ExecutorError::missing_labeled_argument(
                                attr_name.clone(),
                            ));
                        }
                    }
                    StatefulNode::LabeledOperatorCall {
                        labels,
                        operand,
                        extra_args,
                        ..
                    } => {
                        let label_idx = labels.iter().find(|(_, name)| name == &attr_name);
                        if let Some(&(arg_idx, _)) = label_idx {
                            let val = with_heap(|h| h.get(extra_args[arg_idx].key()).clone())
                                .elide_lvalue();
                            self.state.stack_mut(stack_idx).push(val);
                        } else {
                            let operand_val =
                                with_heap(|h| h.get(operand.key()).clone()).elide_lvalue();
                            self.state.stack_mut(stack_idx).push(operand_val);
                            return self.exec_attribute(
                                stack_idx,
                                section_idx,
                                false,
                                string_index,
                            );
                        }
                    }
                    _ => {
                        return ExecSingle::Error(ExecutorError::CannotAttribute(
                            "stateful expression".into(),
                        ));
                    }
                },
                _ => {
                    return ExecSingle::Error(ExecutorError::CannotAttribute(base.type_name()));
                }
            }
        }

        ExecSingle::Continue
    }
}

impl VWeak {
    pub fn from(key: HeapKey) -> Self {
        VWeak(key)
    }
}
