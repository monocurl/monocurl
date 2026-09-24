use bytecode::CopyValueMode;

use crate::{
    error::ExecutorError,
    heap::{HeapKey, VRc, VWeak, heap_ref_count, heap_replace, with_heap, with_heap_mut},
    state::LeaderKind,
    value::{
        Value, container::HashableKey, invoked_function::InvokedFunction,
        invoked_operator::{InvokedOperator, invalidate_invoked_operator_cache},
        stateful::lift_append_to_stateful,
    },
};

use super::{ExecSingle, Executor};

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

/// select `container[i0][i1]...` without copying any level along the way.
/// `None` for anything but plain integer indices into lists and hashable keys
/// into maps, so the general path can resolve it or raise the error it always has.
///
/// runs under a heap borrow, so the element is only cloned shallowly here and
/// the caller elides it once the borrow is released
fn select_in_place(container: &Value, indices: &[Value]) -> Option<Result<Value, ExecutorError>> {
    let (index, rest) = indices.split_first()?;
    container.with_elided_cached_wrappers(|resolved| {
        let slot = match (resolved, index) {
            (Value::List(list), Value::Integer(index)) => {
                let index = *index as usize;
                match list.elements().get(index) {
                    Some(slot) => slot.key(),
                    None => {
                        return Some(Err(ExecutorError::IndexOutOfBounds {
                            index,
                            len: list.len(),
                        }));
                    }
                }
            }
            (Value::Map(map), key) => match map.get(&HashableKey::try_from_value(key).ok()?) {
                Some(slot) => slot.key(),
                None if rest.is_empty() => return Some(Ok(Value::Nil)),
                None => return None,
            },
            _ => return None,
        };

        with_heap(|heap| {
            let element = heap.get(slot);
            if rest.is_empty() {
                Some(Ok(element.clone()))
            } else {
                select_in_place(&element, rest)
            }
        })
    })
}

/// the slot holding `container[index]`, ready to be written through: a slot
/// shared with another value is detached first, and a missing map key gets a
/// fresh nil slot. the container is edited in place, because copying it out to
/// write one element would make every indexed write O(n)
fn writable_element_slot(container_key: HeapKey, index: &Value) -> Result<HeapKey, ExecutorError> {
    let container_key = writable_container_key(container_key);

    enum Position {
        Index(usize),
        Key(HashableKey),
    }

    let (position, slot) = with_heap(|heap| match &*heap.get(container_key) {
        Value::List(list) => {
            let Value::Integer(i) = index else {
                return Err(ExecutorError::type_error("int", index.type_name()));
            };
            let i = *i as usize;
            let slot = list
                .elements()
                .get(i)
                .ok_or(ExecutorError::IndexOutOfBounds {
                    index: i,
                    len: list.len(),
                })?;
            Ok((Position::Index(i), Some(slot.key())))
        }
        Value::Map(map) => {
            let key = HashableKey::try_from_value(index)?;
            let slot = map.get(&key).map(VRc::key);
            Ok((Position::Key(key), slot))
        }
        other => Err(ExecutorError::CannotSubscript(other.type_name())),
    })?;

    // allocating reborrows the heap, so the replacement is built before the
    // container is borrowed mutably
    let replacement = match slot {
        Some(slot) if heap_ref_count(slot) == 1 => return Ok(slot),
        Some(slot) => VRc::new(with_heap(|heap| heap.get(slot).clone())),
        None => VRc::new(Value::Nil),
    };
    let replacement_key = replacement.key();

    // dropped once the borrow is released, since releasing it can free slots
    let _displaced = with_heap_mut(|heap| match (&mut *heap.get_mut(container_key), position) {
        (Value::List(list), Position::Index(i)) => {
            Some(std::mem::replace(&mut list.elements[i], replacement))
        }
        (Value::Map(map), Position::Key(key)) => map.insert(key, replacement),
        _ => unreachable!("container changed shape while detaching an element"),
    });
    Ok(replacement_key)
}

/// the slot that a write into the container living at `key` should edit. a live
/// function is replaced by its result, since editing the result detaches it from
/// the call, while a live operator is edited through its operand so that it keeps
/// applying. anything that does not resolve to a list or map is left untouched
/// for the caller to reject
fn writable_container_key(key: HeapKey) -> HeapKey {
    let key = follow_heap_lvalue_key(key);
    let live = with_heap(|heap| match &*heap.get(key) {
        Value::InvokedOperator(invoked) if resolves_to_container(&invoked.body.operand) => {
            Some(Value::InvokedOperator(invoked.clone()))
        }
        live @ Value::InvokedFunction(_) if resolves_to_container(live) => Some(live.clone()),
        _ => None,
    });

    match live {
        Some(Value::InvokedOperator(mut invoked)) => {
            invalidate_invoked_operator_cache(&invoked);
            invoked.body.boxed_operand = true;
            let operand_key = invoked.body.operand.make_mut_lvalue();
            heap_replace(key, Value::InvokedOperator(invoked));
            writable_container_key(operand_key)
        }
        Some(live) => {
            heap_replace(key, live.elide_cached_wrappers());
            key
        }
        None => key,
    }
}

fn resolves_to_container(value: &Value) -> bool {
    value.with_elided_cached_wrappers(|value| matches!(value, Value::List(_) | Value::Map(_)))
}

/// what the `len` native reports for a value it accepts
fn plain_len(value: &Value) -> Option<usize> {
    match value {
        Value::List(list) => Some(list.len()),
        Value::Map(map) => Some(map.len()),
        Value::String(s) => Some(s.chars().count()),
        _ => None,
    }
}

fn retained_lvalue(key: HeapKey) -> Value {
    Value::Lvalue(VRc::retain_key(key))
}

impl Executor {
    fn exec_assign_dfs(&mut self, lhs: Value, rhs: Value, stack_idx: usize) -> ExecSingle {
        if let Value::List(llhs) = &lhs {
            let rhs = rhs.elide_cached_wrappers();
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

        // peek at the target's shape rather than copying it out: assigning to a
        // list-valued variable would otherwise duplicate the whole list first
        let key = follow_heap_lvalue_key(key);
        let leader = with_heap(|heap| match &*heap.get(key) {
            Value::Leader(leader) => Some((leader.leader_rc.key(), leader.kind)),
            _ => None,
        });

        match leader {
            Some((leader_key, kind)) => {
                let rhs = rhs.elide_lvalue_leader_rec();
                if matches!(rhs, Value::Stateful(_)) && kind != LeaderKind::Mesh {
                    return ExecSingle::Error(ExecutorError::stateful_requires_mesh_assignment());
                }
                heap_replace(leader_key, rhs);
                with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(key) {
                        l.last_modified_stack = Some(stack_idx);
                        l.leader_version += 1;
                    }
                });
            }
            None => {
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

        match lhs.elide_cached_wrappers() {
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
        let key = writable_container_key(key);
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

            let inner_key = writable_container_key(leader.leader_rc.key());
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

    /// the shapes indexing actually produces: plain integer or key indices into
    /// plain lists and maps. resolved without suspending, so loops that index
    /// stay in the synchronous run
    pub(super) fn try_subscript_local(
        &mut self,
        stack_idx: usize,
        stack_delta: i32,
        depth: u16,
    ) -> Option<ExecSingle> {
        let depth = usize::from(depth);
        let stack = self.state.stack(stack_idx);
        let selected = select_in_place(stack.read_at(stack_delta), stack.top(depth))?;

        let stack = self.state.stack_mut(stack_idx);
        stack.pop_n(depth);
        Some(match selected {
            Ok(element) => {
                stack.push(element.elide_cached_wrappers_rec());
                ExecSingle::Continue
            }
            Err(error) => ExecSingle::Error(error),
        })
    }

    /// pop `depth` indices and push `container[i0][i1]...`, reading the container
    /// living at `stack_delta` in place rather than copying it
    pub(super) async fn exec_subscript_local(
        &mut self,
        stack_idx: usize,
        stack_delta: i32,
        depth: u16,
        copy_mode: CopyValueMode,
    ) -> ExecSingle {
        let depth = usize::from(depth);
        let mut indices: Vec<Value> = (0..depth)
            .map(|_| self.state.stack_mut(stack_idx).pop())
            .collect();
        indices.reverse();
        for index in &mut indices {
            let raw = std::mem::replace(index, Value::Nil);
            *index = match self.read_subscript_value(raw).await {
                Ok(value) => value.elide_cached_wrappers_rec(),
                Err(error) => return ExecSingle::Error(error),
            };
        }

        // the indices have been popped, so the container sits that much higher
        // than the delta the compiler emitted
        let container_delta = stack_delta + depth as i32;
        let container = self.state.stack(stack_idx).read_at(container_delta);
        match select_in_place(container, &indices) {
            Some(Ok(element)) => {
                self.state
                    .stack_mut(stack_idx)
                    .push(element.elide_cached_wrappers_rec());
                ExecSingle::Continue
            }
            Some(Err(error)) => ExecSingle::Error(error),
            None => {
                self.exec_subscript_copied(stack_idx, container_delta, copy_mode, indices)
                    .await
            }
        }
    }

    /// the general path for everything [`select_in_place`] declines: copy the
    /// container out the way a plain read would, then subscript level by level
    async fn exec_subscript_copied(
        &mut self,
        stack_idx: usize,
        container_delta: i32,
        copy_mode: CopyValueMode,
        indices: Vec<Value>,
    ) -> ExecSingle {
        let container = match self.copy_local(stack_idx, container_delta, copy_mode).await {
            Ok(container) => container,
            Err(error) => return ExecSingle::Error(error),
        };

        self.state.stack_mut(stack_idx).push(container);
        for index in indices {
            self.state.stack_mut(stack_idx).push(index);
            match self.exec_subscript(stack_idx, false).await {
                ExecSingle::Continue => {}
                other => return other,
            }
        }
        ExecSingle::Continue
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

    /// `len(...)` of the container living at `stack_delta`, read in place
    pub(super) fn try_len_local(&mut self, stack_idx: usize, stack_delta: i32) -> Option<ExecSingle> {
        let len = self
            .state
            .stack(stack_idx)
            .read_at(stack_delta)
            .with_elided_cached_wrappers(plain_len)?;
        self.state
            .stack_mut(stack_idx)
            .push(Value::Integer(len as i64));
        Some(ExecSingle::Continue)
    }

    /// the general path for what [`Self::try_len_local`] declines: the length of
    /// the copy the `len` native would have been handed
    pub(super) async fn exec_len_copied(
        &mut self,
        stack_idx: usize,
        stack_delta: i32,
        copy_mode: CopyValueMode,
    ) -> ExecSingle {
        let container = match self.copy_local(stack_idx, stack_delta, copy_mode).await {
            Ok(container) => container.elide_cached_wrappers(),
            Err(error) => return ExecSingle::Error(error),
        };
        match plain_len(&container) {
            Some(len) => {
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Integer(len as i64));
                ExecSingle::Continue
            }
            None => ExecSingle::Error(ExecutorError::type_error(
                "list / map / string",
                container.type_name(),
            )),
        }
    }

    /// copy the value living at `stack_delta` out the way a plain read of it would
    async fn copy_local(
        &mut self,
        stack_idx: usize,
        stack_delta: i32,
        copy_mode: CopyValueMode,
    ) -> Result<Value, ExecutorError> {
        let value = self.state.stack(stack_idx).read_at(stack_delta).clone();
        let value = match copy_mode {
            CopyValueMode::Read => self.read_current_value(value).await?,
            CopyValueMode::Reference => value.force_elide_lvalue(),
            CopyValueMode::Raw => value,
        };
        match value {
            Value::Stateful(_) => Err(ExecutorError::direct_stateful_copy()),
            value => Ok(value),
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
        let Some(base_key) = base.as_lvalue_key() else {
            return ExecSingle::Error(ExecutorError::CannotSubscript(base.type_name()));
        };

        let base_key = follow_heap_lvalue_key(base_key);
        let leader_key = with_heap_mut(|heap| match &mut *heap.get_mut(base_key) {
            Value::Leader(leader) => {
                leader.last_modified_stack = Some(stack_idx);
                leader.leader_version += 1;
                Some(leader.leader_rc.key())
            }
            _ => None,
        });
        if let Some(leader_key) = leader_key {
            return self.exec_mutable_subscript(stack_idx, retained_lvalue(leader_key), index);
        }

        match writable_element_slot(base_key, &index) {
            Ok(key) => {
                self.state.stack_mut(stack_idx).push(retained_lvalue(key));
                ExecSingle::Continue
            }
            Err(error) => ExecSingle::Error(error),
        }
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
