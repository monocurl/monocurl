use std::rc::Rc;

use crate::{
    error::ExecutorError,
    value::{RcValue, Value, container::HashableKey, rc_value},
};

use super::{ExecSingle, Executor};

impl Executor {
    fn exec_assign_dfs(&mut self, lhs: Value, rhs: Value, stack_idx: usize) -> ExecSingle {
        if let Value::List(llhs) = &lhs {
            return match &rhs {
                Value::List(lrhs) if llhs.len() == lrhs.len() => {
                    for (l, r) in llhs.elements.iter().zip(lrhs.elements.iter()) {
                        let res =
                            self.exec_assign_dfs(l.borrow().clone(), r.borrow().clone(), stack_idx);

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

        let rc = match lhs.as_lvalue_rc() {
            Some(rc) => rc,
            None => return ExecSingle::Error(ExecutorError::CannotAssignTo(lhs.type_name())),
        };

        let target = rc.borrow().clone();

        match target {
            Value::Leader(leader) => {
                let stack_id = self.state.stack(stack_idx).stack_id;

                *leader.leader_rc.borrow_mut() = rhs;

                if let Value::Leader(l) = &mut *rc.borrow_mut() {
                    l.last_modified_stack = Some(stack_id);
                }
            }
            _ => {
                *rc.borrow_mut() = rhs;
            }
        }

        ExecSingle::Continue
    }

    pub(super) fn exec_assign(&mut self, stack_idx: usize) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);

        let rhs = stack.pop();
        let lhs = stack.pop();

        let ret = self.exec_assign_dfs(lhs.clone(), rhs, stack_idx);
        self.state.stack_mut(stack_idx).push(lhs);

        ret
    }

    pub(super) fn exec_append(&mut self, stack_idx: usize) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let rhs = stack.pop();
        let lhs = stack.pop();

        match lhs {
            Value::List(mut list) => {
                Rc::make_mut(&mut list).elements.push(rc_value(rhs));
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

        let rc = match lhs.as_lvalue_rc() {
            Some(rc) => rc,
            None => {
                return ExecSingle::Error(ExecutorError::Other(
                    "append-assign: lhs is not an lvalue".into(),
                ));
            }
        };

        let mut borrowed = rc.borrow_mut();
        match &mut *borrowed {
            Value::List(list) => {
                Rc::make_mut(list).elements.push(rc_value(rhs));
            }
            _ => return ExecSingle::Error(ExecutorError::type_error("list", borrowed.type_name())),
        }
        drop(borrowed);
        self.state
            .stack_mut(stack_idx)
            .push(Value::WeakLvalue(RcValue::downgrade(&rc)));
        ExecSingle::Continue
    }

    pub(super) fn exec_subscript(&mut self, stack_idx: usize, mutable: bool) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let index = stack.pop();
        let base = stack.pop();

        let index = index.elide_lvalue();

        if mutable {
            let base_rc = match base.as_lvalue_rc() {
                Some(rc) => rc,
                None => return ExecSingle::Error(ExecutorError::CannotSubscript(base.type_name())),
            };

            // update last_modified_stack if base is a leader
            {
                let base_val = base_rc.borrow();
                if let Value::Leader(_) = &*base_val {
                    let stack_id = self.state.stack(stack_idx).stack_id;
                    drop(base_val);
                    if let Value::Leader(l) = &mut *base_rc.borrow_mut() {
                        l.last_modified_stack = Some(stack_id);
                    }
                }
            }

            let base_val = base_rc.borrow();
            match &*base_val {
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
                    let elem_rc = list.elements[idx].clone();
                    // COW: if element is shared, replace with a fresh clone
                    if Rc::strong_count(&elem_rc) > 1 {
                        let cloned = rc_value(elem_rc.borrow().clone());
                        drop(base_val);
                        if let Value::List(list) = &mut *base_rc.borrow_mut() {
                            Rc::make_mut(list).elements[idx] = cloned.clone();
                        }
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(RcValue::downgrade(&cloned)));
                    } else {
                        drop(base_val);
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(RcValue::downgrade(&elem_rc)));
                    }
                }
                Value::Map(map) => {
                    let key = match HashableKey::try_from_value(&index) {
                        Ok(k) => k,
                        Err(e) => return ExecSingle::Error(e),
                    };
                    match map.get(&key) {
                        Some(val_rc) => {
                            let val_rc = val_rc.clone();
                            if Rc::strong_count(&val_rc) > 1 {
                                let cloned = rc_value(val_rc.borrow().clone());
                                drop(base_val);
                                if let Value::Map(map) = &mut *base_rc.borrow_mut() {
                                    Rc::make_mut(map).insert(key, cloned.clone());
                                }
                                self.state
                                    .stack_mut(stack_idx)
                                    .push(Value::WeakLvalue(RcValue::downgrade(&cloned)));
                            } else {
                                drop(base_val);
                                self.state
                                    .stack_mut(stack_idx)
                                    .push(Value::WeakLvalue(RcValue::downgrade(&val_rc)));
                            }
                        }
                        None => {
                            let new_rc = rc_value(Value::Nil);
                            drop(base_val);
                            if let Value::Map(map) = &mut *base_rc.borrow_mut() {
                                Rc::make_mut(map).insert(key, new_rc.clone());
                            }
                            self.state
                                .stack_mut(stack_idx)
                                .push(Value::WeakLvalue(RcValue::downgrade(&new_rc)));
                        }
                    }
                }
                Value::Leader(leader) => {
                    let leader_rc = leader.leader_rc.clone();
                    drop(base_val);
                    // push a weak lvalue for the leader's inner value, then recurse
                    let stack = self.state.stack_mut(stack_idx);
                    stack.push(Value::Lvalue(leader_rc));
                    stack.push(index);
                    return self.exec_subscript(stack_idx, true);
                }
                _ => {
                    return ExecSingle::Error(ExecutorError::CannotSubscript(base_val.type_name()));
                }
            }
        } else {
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
                    let val = list.elements[idx].borrow().clone();
                    self.state.stack_mut(stack_idx).push(val);
                }
                Value::Map(map) => {
                    let key = match HashableKey::try_from_value(&index) {
                        Ok(k) => k,
                        Err(e) => return ExecSingle::Error(e),
                    };
                    let val = map
                        .get(&key)
                        .map(|rc| rc.borrow().clone())
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
                    let inner = leader.leader_rc.borrow().clone();
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
            let base_rc = match base.as_lvalue_rc() {
                Some(rc) => rc,
                None => return ExecSingle::Error(ExecutorError::CannotAttribute(base.type_name())),
            };

            // update last_modified_stack for leaders
            {
                let base_val = base_rc.borrow();
                if let Value::Leader(_) = &*base_val {
                    let stack_id = self.state.stack(stack_idx).stack_id;
                    drop(base_val);
                    if let Value::Leader(l) = &mut *base_rc.borrow_mut() {
                        l.last_modified_stack = Some(stack_id);
                    }
                }
            }

            let base_val = base_rc.borrow();
            match &*base_val {
                Value::Leader(leader) => {
                    let leader_rc = leader.leader_rc.clone();
                    drop(base_val);
                    self.state
                        .stack_mut(stack_idx)
                        .push(Value::Lvalue(leader_rc));
                    return self.exec_attribute(stack_idx, section_idx, true, string_index);
                }
                Value::InvokedFunction(inv_rc) => {
                    let label_idx = inv_rc.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let arg_val = inv_rc.arguments[arg_idx].clone();
                        let arg_rc = rc_value(arg_val);
                        drop(base_val);
                        // COW: make exclusive before mutating
                        if let Value::InvokedFunction(ref mut inner_rc) = *base_rc.borrow_mut() {
                            let inv = Rc::make_mut(inner_rc);
                            inv.arguments[arg_idx] = Value::Lvalue(arg_rc.clone());
                            inv.cached_result.take();
                        }
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(RcValue::downgrade(&arg_rc)));
                    } else {
                        return ExecSingle::Error(ExecutorError::Other(format!(
                            "no labeled argument '{}'",
                            attr_name
                        )));
                    }
                }
                Value::InvokedOperator(inv_rc) => {
                    let label_idx = inv_rc.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let arg_val = inv_rc.arguments[arg_idx].clone();
                        let arg_rc = rc_value(arg_val);
                        drop(base_val);

                        if let Value::InvokedOperator(ref mut inner_rc) = *base_rc.borrow_mut() {
                            let inv = Rc::make_mut(inner_rc);
                            inv.arguments[arg_idx] = Value::Lvalue(arg_rc.clone());
                            inv.invalidate_cache();
                        }
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(RcValue::downgrade(&arg_rc)));
                    } else {
                        let operand_rc = rc_value(inv_rc.operand.as_ref().clone().elide_lvalue());
                        drop(base_val);

                        if let Value::InvokedOperator(ref mut inner_rc) = *base_rc.borrow_mut() {
                            let inv = Rc::make_mut(inner_rc);
                            inv.operand = Box::new(Value::Lvalue(operand_rc.clone()));
                            inv.invalidate_cache();
                        }
                        self.state
                            .stack_mut(stack_idx)
                            .push(Value::WeakLvalue(RcValue::downgrade(&operand_rc)));
                        return self.exec_attribute(stack_idx, section_idx, true, string_index);
                    }
                }
                _ => {
                    return ExecSingle::Error(ExecutorError::CannotAttribute(base_val.type_name()));
                }
            }
        } else {
            let base = base.elide_lvalue();
            match &base {
                Value::Leader(leader) => {
                    let inner = leader.leader_rc.borrow().clone();
                    self.state.stack_mut(stack_idx).push(inner);
                    return self.exec_attribute(stack_idx, section_idx, false, string_index);
                }
                Value::InvokedFunction(inv_rc) => {
                    let label_idx = inv_rc.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let val = inv_rc.arguments[arg_idx].clone().elide_lvalue();
                        self.state.stack_mut(stack_idx).push(val);
                    } else {
                        return ExecSingle::Error(ExecutorError::Other(format!(
                            "no labeled argument '{}'",
                            attr_name
                        )));
                    }
                }
                Value::InvokedOperator(inv_rc) => {
                    let label_idx = inv_rc.labels.iter().find(|(_, name)| name == &attr_name);
                    if let Some(&(arg_idx, _)) = label_idx {
                        let val = inv_rc.arguments[arg_idx].clone().elide_lvalue();
                        self.state.stack_mut(stack_idx).push(val);
                    } else {
                        self.state
                            .stack_mut(stack_idx)
                            .push(inv_rc.operand.as_ref().clone().elide_lvalue());
                        return self.exec_attribute(stack_idx, section_idx, false, string_index);
                    }
                }
                _ => {
                    return ExecSingle::Error(ExecutorError::CannotAttribute(base.type_name()));
                }
            }
        }

        ExecSingle::Continue
    }
}
