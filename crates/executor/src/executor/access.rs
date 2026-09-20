use crate::{
    error::ExecutorError,
    heap::{HeapKey, VRc, VWeak, heap_replace, with_heap, with_heap_mut},
    state::LeaderKind,
    value::{
        Value, container::HashableKey, invoked_function::InvokedFunction,
        invoked_operator::InvokedOperator, stateful::lift_append_to_stateful,
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

/// walk lvalue indirection down to the slot that actually holds the value,
/// without copying anything along the way
fn follow_heap_lvalue_key(mut key: HeapKey) -> HeapKey {
    while let Some(next_key) = with_heap(|heap| heap.get(key).as_lvalue_key()) {
        key = next_key;
    }
    key
}

/// append to the list living in `key` in place. the element is allocated before
/// the slot is borrowed, because allocating reborrows the heap
fn append_to_list_slot(key: HeapKey, element: Value) -> Result<(), ExecutorError> {
    let element = VRc::new(element);
    with_heap_mut(|heap| match &mut *heap.get_mut(key) {
        Value::List(list) => {
            list.elements.push(element);
            Ok(())
        }
        other => Err(ExecutorError::type_error("list", other.type_name())),
    })
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

        if matches!(rhs, Value::Stateful(_)) || matches!(lhs, Value::Stateful(_)) {
            return match lift_append_to_stateful(lhs, rhs) {
                Ok(stateful) => {
                    self.state.stack_mut(stack_idx).push(stateful);
                    ExecSingle::Continue
                }
                Err(e) => ExecSingle::Error(e),
            };
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
            None => return ExecSingle::Error(ExecutorError::invalid_lvalue("append-assign")),
        };

        let rhs = rhs.elide_lvalue_leader_rec();
        // appending to a plain list is the common case and must stay O(1): copying
        // the list out of its slot to push one element would make building a list
        // quadratic in its length
        let key = follow_heap_lvalue_key(key);
        let is_plain_list = with_heap(|heap| matches!(&*heap.get(key), Value::List(_)));

        let appended_key = if is_plain_list {
            if matches!(rhs, Value::Stateful(_)) {
                return ExecSingle::Error(ExecutorError::stateful_requires_mesh_assignment());
            }
            if let Err(error) = append_to_list_slot(key, rhs) {
                return ExecSingle::Error(error);
            }
            key
        } else {
            let base_val = with_heap(|heap| heap.get(key).clone());
            let Value::Leader(leader) = base_val else {
                return ExecSingle::Error(ExecutorError::type_error("list", base_val.type_name()));
            };

            let inner_key = follow_heap_lvalue_key(leader.leader_rc.key());
            let inner_is_stateful =
                with_heap(|heap| matches!(&*heap.get(inner_key), Value::Stateful(_)));

            if matches!(rhs, Value::Stateful(_)) || inner_is_stateful {
                let inner_val = with_heap(|heap| heap.get(inner_key).clone());
                let new_stateful = match lift_append_to_stateful(inner_val, rhs) {
                    Ok(v) => v,
                    Err(e) => return ExecSingle::Error(e),
                };
                heap_replace(inner_key, new_stateful);
            } else if let Err(error) = append_to_list_slot(inner_key, rhs) {
                return ExecSingle::Error(error);
            }

            with_heap_mut(|h| {
                if let Value::Leader(l) = &mut *h.get_mut(key) {
                    l.last_modified_stack = Some(stack_idx);
                    l.leader_version += 1;
                }
            });

            inner_key
        };

        self.state
            .stack_mut(stack_idx)
            .push(Value::WeakLvalue(VWeak::from(appended_key)));
        ExecSingle::Continue
    }

    /// resolve wrappers around a subscript base or index without descending into
    /// container elements: only the element that is actually selected needs to be
    /// elided, and deep-eliding the whole container makes indexing O(n)
    async fn read_subscript_value(&mut self, value: Value) -> Result<Value, ExecutorError> {
        let mut value = value.elide_lvalue();
        loop {
            value = match value {
                Value::Leader(ref leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
                Value::InvokedFunction(ref inv) => InvokedFunction::value(inv, self).await?,
                Value::InvokedOperator(ref inv) => InvokedOperator::value(inv, self).await?,
                other @ (Value::List(_) | Value::Map(_)) => return Ok(other),
                other => return Ok(other.elide_cached_wrappers_rec()),
            };
        }
    }

    /// read the container living at `stack_delta` without copying it out of its
    /// slot, then push the element selected by the index on top of stack
    pub(super) async fn exec_subscript_local(
        &mut self,
        stack_idx: usize,
        stack_delta: i32,
    ) -> ExecSingle {
        let index = self.state.stack_mut(stack_idx).pop();
        let index = match self.read_subscript_value(index).await {
            Ok(value) => value.elide_cached_wrappers_rec(),
            Err(error) => return ExecSingle::Error(error),
        };

        let Value::Integer(idx) = index else {
            return ExecSingle::Error(ExecutorError::type_error("int", index.type_name()));
        };
        let idx = idx as usize;

        // the index has been popped, so the container sits one slot higher than
        // the delta the compiler emitted
        let container_delta = stack_delta + 1;
        let element = self
            .state
            .stack(stack_idx)
            .read_at(container_delta)
            .with_elided_cached_wrappers(|resolved| match resolved {
                Value::List(list) => match list.elements().get(idx) {
                    Some(element) => Ok(Some(with_heap(|h| h.get(element.key()).clone()))),
                    None => Err(ExecutorError::IndexOutOfBounds {
                        index: idx,
                        len: list.len(),
                    }),
                },
                // maps, strings, and invocations that have not been evaluated yet
                // fall back to the general path
                _ => Ok(None),
            });

        match element {
            Ok(Some(element)) => {
                self.state
                    .stack_mut(stack_idx)
                    .push(element.elide_cached_wrappers_rec());
                ExecSingle::Continue
            }
            Ok(None) => {
                let container = self.state.stack(stack_idx).read_at(container_delta).clone();
                let stack = self.state.stack_mut(stack_idx);
                stack.push(container);
                stack.push(Value::Integer(idx as i64));
                self.exec_subscript(stack_idx, false).await
            }
            Err(error) => ExecSingle::Error(error),
        }
    }

    /// push the length of the container living at `stack_delta`, reading it in place
    pub(super) fn exec_container_len(&mut self, stack_idx: usize, stack_delta: i32) -> ExecSingle {
        let len = self
            .state
            .stack(stack_idx)
            .read_at(stack_delta)
            .with_elided_cached_wrappers(|resolved| match resolved {
                Value::List(list) => Ok(list.len()),
                Value::Map(_) => Err(ExecutorError::invalid_operation(
                    "cannot iterate over a map directly; use map_items(map) to iterate [key, value] pairs",
                )),
                other => Err(ExecutorError::type_error("list", other.type_name())),
            });

        match len {
            Ok(len) => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Integer(len as i64));
                ExecSingle::Continue
            }
            Err(error) => ExecSingle::Error(error),
        }
    }

    pub(super) async fn exec_subscript(&mut self, stack_idx: usize, mutable: bool) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let index = stack.pop();
        let base = stack.pop();

        let index = index.elide_cached_wrappers_rec();

        if mutable {
            return self.exec_mutable_subscript(stack_idx, base, index);
        }

        self.exec_read_subscript(stack_idx, base, index).await
    }

    fn exec_mutable_subscript(
        &mut self,
        stack_idx: usize,
        base: Value,
        index: Value,
    ) -> ExecSingle {
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
                    return ExecSingle::Error(ExecutorError::type_error("int", index.type_name()));
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
                return self.exec_mutable_subscript(
                    stack_idx,
                    Value::Lvalue(leader.leader_rc.clone()),
                    index,
                );
            }
            _ => {
                return ExecSingle::Error(ExecutorError::CannotSubscript(base_val.type_name()));
            }
        }

        ExecSingle::Continue
    }

    async fn exec_read_subscript(
        &mut self,
        stack_idx: usize,
        base: Value,
        index: Value,
    ) -> ExecSingle {
        let base = match self.read_subscript_value(base).await {
            Ok(value) => value,
            Err(error) => return ExecSingle::Error(error),
        };
        let index = match self.read_subscript_value(index).await {
            Ok(value) => value,
            Err(error) => return ExecSingle::Error(error),
        };

        if matches!(base, Value::Stateful(_)) || matches!(index, Value::Stateful(_)) {
            return ExecSingle::Error(ExecutorError::stateful_subscript());
        }

        match base {
            Value::List(list) => {
                let Value::Integer(idx) = index else {
                    return ExecSingle::Error(ExecutorError::type_error("int", index.type_name()));
                };
                let idx = idx as usize;
                if idx >= list.elements.len() {
                    return ExecSingle::Error(ExecutorError::IndexOutOfBounds {
                        index: idx,
                        len: list.elements.len(),
                    });
                }
                let val = with_heap(|h| h.get(list.elements[idx].key()).clone())
                    .elide_cached_wrappers_rec();
                self.state.stack_mut(stack_idx).push(val);
            }
            Value::Map(map) => {
                let key_hash = match HashableKey::try_from_value(&index) {
                    Ok(k) => k,
                    Err(e) => return ExecSingle::Error(e),
                };
                let val = map
                    .get(&key_hash)
                    .map(|k| with_heap(|h| h.get(k.key()).clone()).elide_cached_wrappers_rec())
                    .unwrap_or(Value::Nil);
                self.state.stack_mut(stack_idx).push(val);
            }
            Value::String(s) => {
                let Value::Integer(idx) = index else {
                    return ExecSingle::Error(ExecutorError::type_error("int", index.type_name()));
                };
                let idx = idx as usize;
                let ch = s.chars().nth(idx).unwrap_or('\0');
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::String(ch.to_string().into()));
            }
            _ => {
                return ExecSingle::Error(ExecutorError::CannotSubscript(base.type_name()));
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
            let Some(base_key) = base.as_lvalue_key() else {
                return ExecSingle::Error(ExecutorError::CannotAttribute(base.type_name()));
            };

            match Value::attr_lvalue_by_name_in_heap(base_key, &attr_name, Some(stack_idx)) {
                Ok(key) => self.state.stack_mut(stack_idx).push(retained_lvalue(key)),
                Err(error) => return ExecSingle::Error(error),
            }
        } else {
            match base.attr_by_name(&attr_name) {
                Ok(value) => self.state.stack_mut(stack_idx).push(value),
                Err(error) => return ExecSingle::Error(error),
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
