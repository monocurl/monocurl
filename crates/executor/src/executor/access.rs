use std::rc::Rc;

use crate::{
    error::ExecutorError,
    heap::{HeapKey, VRc, VWeak, heap_ref_count, heap_replace, with_heap, with_heap_mut},
    state::LeaderKind,
    value::{Value, container::HashableKey, stateful::StatefulNode},
};

use super::{ExecSingle, Executor};

fn is_empty_mesh_value(value: &Value) -> bool {
    matches!(
        value.clone().elide_lvalue_leader_rec(),
        Value::List(list) if list.is_empty()
    )
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

        let target = with_heap(|h| h.get(key).clone());

        match target {
            Value::Leader(leader) => {
                if matches!(rhs, Value::Stateful(_)) && leader.kind != LeaderKind::Mesh {
                    return ExecSingle::Error(ExecutorError::stateful_requires_mesh_assignment());
                }
                let stack_id = self.state.stack(stack_idx).stack_id;
                if leader.kind == LeaderKind::Mesh {
                    let follower = with_heap(|h| h.get(leader.follower_rc.key()).clone());
                    if is_empty_mesh_value(&follower) {
                        let current = with_heap(|h| h.get(leader.leader_rc.key()).clone());
                        if !is_empty_mesh_value(&current) {
                            heap_replace(leader.follower_rc.key(), current.to_follower_stateful());
                            with_heap_mut(|h| {
                                if let Value::Leader(l) = &mut *h.get_mut(key) {
                                    l.follower_version += 1;
                                }
                            });
                        }
                    }
                }
                heap_replace(leader.leader_rc.key(), rhs);
                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(key) {
                        l.last_modified_stack = Some(stack_id);
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
        let rhs = stack.pop();
        let lhs = stack.pop();
        let assigned = rhs.clone();

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
                Rc::make_mut(&mut list).elements.push(VRc::new(rhs));
                self.state.stack_mut(stack_idx).push(Value::List(list));
                ExecSingle::Continue
            }
            _ => ExecSingle::Error(ExecutorError::CannotSubscript(lhs.type_name())),
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

        let base_val = with_heap(|h| h.get(key).clone());
        match base_val {
            Value::List(mut list) => {
                Rc::make_mut(&mut list).elements.push(VRc::new(rhs));
                heap_replace(key, Value::List(list));
            }
            _ => {
                return ExecSingle::Error(ExecutorError::type_error("list", base_val.type_name()));
            }
        }
        self.state
            .stack_mut(stack_idx)
            .push(Value::WeakLvalue(VWeak::from(key)));
        ExecSingle::Continue
    }

    pub(super) fn exec_subscript(&mut self, stack_idx: usize, mutable: bool) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let index = stack.pop();
        let base = stack.pop();

        let index = index.elide_lvalue();

        if mutable {
            let base_key = match base.as_lvalue_key() {
                Some(k) => k,
                None => {
                    return ExecSingle::Error(ExecutorError::CannotSubscript(base.type_name()));
                }
            };

            // update last_modified_stack if base is a leader
            let base_val = with_heap(|h| h.get(base_key).clone());
            if let Value::Leader(_) = &base_val {
                let stack_id = self.state.stack(stack_idx).stack_id;
                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(base_key) {
                        l.last_modified_stack = Some(stack_id);
                        l.leader_version += 1;
                    }
                });
            }

            match base_val {
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

                    with_heap_mut(|h| {
                        let Value::List(list_mut) = &mut *h.get_mut(base_key) else {
                            unreachable!();
                        };
                        Rc::make_mut(list_mut);
                    });

                    let elem_ref = with_heap(|h| match &*h.get(base_key) {
                        Value::List(list) => list.elements[idx].clone(),
                        _ => unreachable!(),
                    });
                    let elem_ref = if heap_ref_count(elem_ref.key()) > 1 {
                        let val = with_heap(|h| h.get(elem_ref.key()).clone());
                        let new_ref = VRc::new(val);
                        let _old_ref = with_heap_mut(|h| {
                            let Value::List(list_mut) = &mut *h.get_mut(base_key) else {
                                unreachable!();
                            };
                            std::mem::replace(
                                &mut Rc::make_mut(list_mut).elements[idx],
                                new_ref.clone(),
                            )
                        });
                        new_ref
                    } else {
                        elem_ref
                    };

                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::WeakLvalue(elem_ref.downgrade()));
                }
                Value::Map(_) => {
                    let key_hash = match HashableKey::try_from_value(&index) {
                        Ok(k) => k,
                        Err(e) => return ExecSingle::Error(e),
                    };

                    with_heap_mut(|h| {
                        let Value::Map(map_mut) = &mut *h.get_mut(base_key) else {
                            unreachable!();
                        };
                        Rc::make_mut(map_mut);
                    });

                    let existing = with_heap(|h| match &*h.get(base_key) {
                        Value::Map(map) => map.get(&key_hash).cloned(),
                        _ => unreachable!(),
                    });

                    let elem_ref = match existing {
                        Some(val_ref) if heap_ref_count(val_ref.key()) > 1 => {
                            let val = with_heap(|h| h.get(val_ref.key()).clone());
                            let new_ref = VRc::new(val);
                            let _old_ref = with_heap_mut(|h| {
                                let Value::Map(map_mut) = &mut *h.get_mut(base_key) else {
                                    unreachable!();
                                };
                                let map = Rc::make_mut(map_mut);
                                std::mem::replace(
                                    map.entries.get_mut(&key_hash).unwrap(),
                                    new_ref.clone(),
                                )
                            });
                            new_ref
                        }
                        Some(val_ref) => val_ref,
                        None => {
                            let new_ref = VRc::new(Value::Nil);
                            with_heap_mut(|h| {
                                let Value::Map(map_mut) = &mut *h.get_mut(base_key) else {
                                    unreachable!();
                                };
                                Rc::make_mut(map_mut).insert(key_hash, new_ref.clone());
                            });
                            new_ref
                        }
                    };
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::WeakLvalue(elem_ref.downgrade()));
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

            let base = base.elide_lvalue();
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

            let base_val = with_heap(|h| h.get(base_key).clone());
            if let Value::Leader(_) = &base_val {
                let stack_id = self.state.stack(stack_idx).stack_id;
                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(base_key) {
                        l.last_modified_stack = Some(stack_id);
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
                Value::InvokedFunction(ref inv) => {
                    let label_idx = inv.body.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let arg_val = inv.body.arguments[arg_idx].clone();
                        let arg_ref = VRc::new(arg_val);
                        with_heap_mut(|h| {
                            if let Value::InvokedFunction(inv_mut) = &mut *h.get_mut(base_key) {
                                Rc::make_mut(&mut inv_mut.body).arguments[arg_idx] =
                                    Value::Lvalue(arg_ref.clone());
                                inv_mut.cache.0.take();
                            }
                        });
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(arg_ref.downgrade()));
                    } else {
                        return ExecSingle::Error(ExecutorError::missing_labeled_argument(
                            attr_name.clone(),
                        ));
                    }
                }
                Value::InvokedOperator(ref inv) => {
                    let label_idx = inv.body.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let arg_val = inv.body.arguments[arg_idx].clone();
                        let arg_ref = VRc::new(arg_val);
                        with_heap_mut(|h| {
                            if let Value::InvokedOperator(inv_mut) = &mut *h.get_mut(base_key) {
                                Rc::make_mut(&mut inv_mut.body).arguments[arg_idx] =
                                    Value::Lvalue(arg_ref.clone());
                                inv_mut.cache.cached_result.take();
                                inv_mut.cache.unmodified.take();
                            }
                        });
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(arg_ref.downgrade()));
                    } else {
                        let operand_ref =
                            VRc::new(inv.body.operand.as_ref().clone().elide_lvalue());
                        with_heap_mut(|h| {
                            if let Value::InvokedOperator(inv_mut) = &mut *h.get_mut(base_key) {
                                Rc::make_mut(&mut inv_mut.body).operand =
                                    Box::new(Value::Lvalue(operand_ref.clone()));
                                inv_mut.cache.cached_result.take();
                                inv_mut.cache.unmodified.take();
                            }
                        });
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(operand_ref.downgrade()));
                        return self.exec_attribute(stack_idx, section_idx, true, string_index);
                    }
                }
                Value::Stateful(ref stateful) => match &stateful.body.root {
                    StatefulNode::LabeledCall { labels, args, .. } => {
                        let label_idx = labels.iter().find(|(_, name)| name == &attr_name);
                        if let Some(&(arg_idx, _)) = label_idx {
                            self.state
                                .stack_mut(stack_idx)
                                .push(Value::WeakLvalue(args[arg_idx].downgrade()));
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
                            self.state
                                .stack_mut(stack_idx)
                                .push(Value::WeakLvalue(extra_args[arg_idx].downgrade()));
                        } else {
                            self.state
                                .stack_mut(stack_idx)
                                .push(Value::Lvalue(operand.clone()));
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
