use std::cell::Cell;

use crate::{
    error::ExecutorError,
    executor::Executor,
    heap::{HeapKey, VRc, heap_replace, with_heap, with_heap_mut},
};

use super::{
    Value,
    container::Map,
    invoked_function::InvokedFunction,
    invoked_operator::InvokedOperator,
    stateful::{Stateful, StatefulNode, reset_stateful_cache, to_follower_stateful},
};

enum AttrMutation {
    Direct(Value),
    Boxed(Value),
    Lvalue,
}

enum AttrMutationResult {
    Updated,
    Lvalue(HeapKey),
}

impl AttrMutation {
    fn boxes_containing_slots(&self) -> bool {
        matches!(self, Self::Boxed(_) | Self::Lvalue)
    }
}

fn elided_heap_ref_value(value_ref: &VRc) -> VRc {
    let key = value_ref.key();
    let val = with_heap(|h| h.get(key).clone());
    let value = if val.may_need_lvalue_leader_elision() {
        val.elide_lvalue_leader_rec()
    } else {
        val
    };
    VRc::new(value)
}

fn clone_cached_value(cell: &Cell<Option<Box<Value>>>) -> Option<Value> {
    let cached = cell.take();
    let cloned = cached.as_ref().map(|value| (**value).clone());
    cell.set(cached);
    cloned
}

fn cached_elided_heap_ref_value(value_ref: &VRc) -> VRc {
    let key = value_ref.key();
    let value = with_heap(|h| h.get(key).clone()).elide_cached_wrappers_rec();
    VRc::new(value)
}

impl Value {
    #[inline(always)]
    pub fn check_truthy(&self) -> Result<bool, ExecutorError> {
        match self {
            Value::Integer(n) => Ok(*n != 0),
            Value::Float(f) => Ok(*f != 0.0),
            Value::Complex { re, im } => Ok(*re != 0.0 || *im != 0.0),
            _ => Err(ExecutorError::InvalidCondition(self.type_name())),
        }
    }

    #[inline(always)]
    fn may_need_lvalue_leader_elision(&self) -> bool {
        self.is_lvalue() || matches!(self, Value::List(_) | Value::Map(_) | Value::Leader(_))
    }

    /// creates owned copy of self which elides lvalues and leaders recursively
    pub fn elide_lvalue_leader_rec(self) -> Value {
        match self {
            Value::Lvalue(vrc) => with_heap(|h| h.get(vrc.key()).clone()).elide_lvalue_leader_rec(),
            Value::WeakLvalue(vweak) => {
                with_heap(|h| h.get(vweak.key()).clone()).elide_lvalue_leader_rec()
            }
            Value::Leader(ref leader) => {
                with_heap(|h| h.get(leader.leader_rc.key()).clone()).elide_lvalue_leader_rec()
            }
            Value::List(mut list) => {
                list.elements = list.elements.iter().map(elided_heap_ref_value).collect();
                Value::List(list)
            }
            Value::Map(map) => {
                let mut out = Map::new();
                for key in &map.insertion_order {
                    let value_ref = map
                        .get(key)
                        .expect("map insertion order points to missing entry");
                    out.insert(key.clone(), elided_heap_ref_value(value_ref));
                }
                Value::Map(out)
            }
            other => other,
        }
    }

    /// read through an lvalue or weak lvalue
    pub fn elide_lvalue(self) -> Value {
        match self {
            Value::Lvalue(vrc) => with_heap(|h| h.get(vrc.key()).clone()),
            Value::WeakLvalue(vweak) => with_heap(|h| h.get(vweak.key()).clone()),
            other => other,
        }
    }

    /// synchronously read through wrappers that already have a cached concrete value.
    pub fn elide_cached_wrappers_rec(self) -> Value {
        match self.elide_lvalue() {
            Value::Leader(leader) => {
                with_heap(|h| h.get(leader.leader_rc.key()).clone()).elide_cached_wrappers_rec()
            }
            Value::InvokedFunction(inv) => clone_cached_value(&inv.cache.0)
                .map(Value::elide_cached_wrappers_rec)
                .unwrap_or(Value::InvokedFunction(inv)),
            Value::InvokedOperator(inv) => clone_cached_value(&inv.cache.cached_result)
                .map(Value::elide_cached_wrappers_rec)
                .unwrap_or(Value::InvokedOperator(inv)),
            Value::List(mut list) => {
                list.elements = list
                    .elements
                    .iter()
                    .map(cached_elided_heap_ref_value)
                    .collect();
                Value::List(list)
            }
            Value::Map(map) => {
                let mut out = Map::new();
                for key in &map.insertion_order {
                    let value_ref = map
                        .get(key)
                        .expect("map insertion order points to missing entry");
                    out.insert(key.clone(), cached_elided_heap_ref_value(value_ref));
                }
                Value::Map(out)
            }
            other => other,
        }
    }

    pub async fn elide_wrappers_rec(self, executor: &mut Executor) -> Result<Value, ExecutorError> {
        let mut base = self.elide_lvalue();
        loop {
            base = match base {
                Value::Leader(ref leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
                Value::InvokedOperator(ref op) => InvokedOperator::value(op, executor).await?,
                Value::InvokedFunction(ref func) => InvokedFunction::value(func, executor).await?,
                Value::Stateful(ref stateful) => executor.eval_stateful(stateful).await?,
                other => return Ok(other),
            };
        }
    }

    pub fn to_follower_stateful(&self) -> Value {
        match self {
            Value::Stateful(stateful) => Value::Stateful(to_follower_stateful(stateful)),
            other => other.clone(),
        }
    }

    pub fn has_attr_by_name(self, attr_name: &str) -> bool {
        self.attr_by_name(attr_name).is_ok()
    }

    pub fn attr_by_name(self, attr_name: &str) -> Result<Value, ExecutorError> {
        match self.elide_lvalue() {
            Value::Leader(leader) => {
                with_heap(|h| h.get(leader.leader_rc.key()).clone()).attr_by_name(attr_name)
            }
            Value::InvokedFunction(inv) => read_function_attr(inv, attr_name),
            Value::InvokedOperator(inv) => read_operator_attr(inv, attr_name),
            Value::Stateful(stateful) => read_stateful_attr(stateful, attr_name),
            other => Err(ExecutorError::CannotAttribute(other.type_name())),
        }
    }

    pub fn attr_lvalue_by_name_in_heap(
        key: HeapKey,
        attr_name: &str,
        modified_stack: Option<usize>,
    ) -> Result<HeapKey, ExecutorError> {
        match mutate_attr_in_heap(key, attr_name, AttrMutation::Lvalue, modified_stack)? {
            AttrMutationResult::Lvalue(key) => Ok(key),
            AttrMutationResult::Updated => Err(ExecutorError::internal(
                "mutable attribute access did not produce an lvalue",
            )),
        }
    }

    /// update the first outermost labeled argument matching `attr_name`.
    pub fn set_attr_by_name(&mut self, attr_name: &str, rhs: Value) -> Result<(), ExecutorError> {
        self.mutate_attr_by_name(attr_name, AttrMutation::Direct(rhs), None)
            .map(|_| ())
    }

    /// update the first outermost labeled argument, storing the replacement in an lvalue slot.
    pub fn set_attr_by_name_boxed(
        &mut self,
        attr_name: &str,
        rhs: Value,
    ) -> Result<(), ExecutorError> {
        self.mutate_attr_by_name(attr_name, AttrMutation::Boxed(rhs), None)
            .map(|_| ())
    }

    pub fn set_attr_by_name_boxed_in_heap(
        key: HeapKey,
        attr_name: &str,
        rhs: Value,
        modified_stack: Option<usize>,
    ) -> Result<(), ExecutorError> {
        mutate_attr_in_heap(key, attr_name, AttrMutation::Boxed(rhs), modified_stack).map(|_| ())
    }

    fn mutate_attr_by_name(
        &mut self,
        attr_name: &str,
        mutation: AttrMutation,
        modified_stack: Option<usize>,
    ) -> Result<AttrMutationResult, ExecutorError> {
        match self {
            Value::Lvalue(vrc) => {
                mutate_attr_in_heap(vrc.key(), attr_name, mutation, modified_stack)
            }
            Value::WeakLvalue(vweak) => {
                mutate_attr_in_heap(vweak.key(), attr_name, mutation, modified_stack)
            }
            Value::Leader(leader) => {
                mutate_attr_in_heap(leader.leader_rc.key(), attr_name, mutation, modified_stack)
            }
            Value::InvokedFunction(inv) => mutate_function_attr(inv, attr_name, mutation),
            Value::InvokedOperator(inv) => {
                mutate_operator_attr(inv, attr_name, mutation, modified_stack)
            }
            Value::Stateful(stateful) => {
                mutate_stateful_attr(stateful, attr_name, mutation, modified_stack)
            }
            _ => Err(ExecutorError::CannotAttribute(self.type_name())),
        }
    }

    #[inline(always)]
    pub fn elide_leader(self) -> Value {
        match self {
            Value::Leader(ref leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
            other => other,
        }
    }

    #[inline(always)]
    pub fn force_elide_lvalue(&self) -> Value {
        match self {
            Value::Lvalue(vrc) => with_heap(|h| h.get(vrc.key()).clone()),
            Value::WeakLvalue(vweak) => with_heap(|h| h.get(vweak.key()).clone()),
            _ => panic!("Expected Lvalue, got {}", self.type_name()),
        }
    }

    /// try to get the underlying HeapKey (upgrading weak refs).
    #[inline(always)]
    pub fn as_lvalue_key(&self) -> Option<HeapKey> {
        match self {
            Value::Lvalue(vrc) => Some(vrc.key()),
            Value::WeakLvalue(vweak) => Some(vweak.key()),
            _ => None,
        }
    }

    pub fn make_mut_lvalue(&mut self) -> HeapKey {
        match self {
            Value::Lvalue(vrc) => vrc.make_mut(),
            Value::WeakLvalue(vweak) => {
                let value = with_heap(|h| h.get(vweak.key()).clone());
                let vrc = VRc::new(value);
                let key = vrc.key();
                *self = Value::Lvalue(vrc);
                key
            }
            _ => {
                let value = std::mem::replace(self, Value::Nil);
                let vrc = VRc::new(value);
                let key = vrc.key();
                *self = Value::Lvalue(vrc);
                key
            }
        }
    }

    #[inline(always)]
    pub fn is_lvalue(&self) -> bool {
        matches!(self, Value::Lvalue(_) | Value::WeakLvalue(_))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Float(_) => "float",
            Value::Integer(_) => "int",
            Value::Complex { .. } => "complex",
            Value::String(_) => "string",
            Value::Mesh(_) => "mesh",
            Value::PrimitiveAnim(_) => "primitive_anim",
            Value::Lambda(_) => "lambda",
            Value::Operator(_) => "operator",
            Value::AnimBlock(_) => "anim_block",
            Value::Map(_) => "map",
            Value::List(_) => "list",
            Value::Stateful(_) => "stateful",
            Value::Leader(_) => "leader",
            Value::InvokedOperator(_) => "live operator",
            Value::InvokedFunction(_) => "live function",
            Value::Lvalue(_) => "lvalue",
            Value::WeakLvalue(_) => "lvalue",
        }
    }
}

fn follow_heap_lvalues(mut key: HeapKey) -> (HeapKey, Value) {
    let mut value = with_heap(|h| h.get(key).clone());
    while let Some(next_key) = value.as_lvalue_key() {
        key = next_key;
        value = with_heap(|h| h.get(key).clone());
    }
    (key, value)
}

fn mark_leader_modified(key: HeapKey, stack_idx: usize) {
    with_heap_mut(|h| {
        if let Value::Leader(leader) = &mut *h.get_mut(key) {
            leader.last_modified_stack = Some(stack_idx);
            leader.leader_version += 1;
        }
    });
}

fn mutate_attr_in_heap(
    key: HeapKey,
    attr_name: &str,
    mutation: AttrMutation,
    modified_stack: Option<usize>,
) -> Result<AttrMutationResult, ExecutorError> {
    let (key, mut value) = follow_heap_lvalues(key);

    if let Value::Leader(leader) = value {
        if let Some(stack_idx) = modified_stack {
            mark_leader_modified(key, stack_idx);
        }
        return mutate_attr_in_heap(leader.leader_rc.key(), attr_name, mutation, modified_stack);
    }

    let result = value.mutate_attr_by_name(attr_name, mutation, modified_stack)?;
    heap_replace(key, value);
    Ok(result)
}

fn read_function_attr(inv: InvokedFunction, attr_name: &str) -> Result<Value, ExecutorError> {
    labeled_argument_index(&inv.body.labels, attr_name, inv.body.arguments.len())
        .map(|arg_idx| inv.body.arguments[arg_idx].clone().elide_lvalue())
        .ok_or_else(|| ExecutorError::missing_labeled_argument(attr_name))
}

fn read_operator_attr(inv: InvokedOperator, attr_name: &str) -> Result<Value, ExecutorError> {
    if let Some(arg_idx) =
        labeled_argument_index(&inv.body.labels, attr_name, inv.body.arguments.len())
    {
        Ok(inv.body.arguments[arg_idx].clone().elide_lvalue())
    } else {
        inv.body.operand.as_ref().clone().attr_by_name(attr_name)
    }
}

fn read_stateful_attr(stateful: Stateful, attr_name: &str) -> Result<Value, ExecutorError> {
    match &stateful.body.root {
        StatefulNode::LabeledCall { labels, args, .. } => {
            labeled_argument_index(labels, attr_name, args.len())
                .map(|arg_idx| with_heap(|h| h.get(args[arg_idx].key()).clone()).elide_lvalue())
                .ok_or_else(|| ExecutorError::missing_labeled_argument(attr_name))
        }
        StatefulNode::LabeledOperatorCall {
            labels,
            operand,
            extra_args,
            ..
        } => {
            if let Some(arg_idx) = labeled_argument_index(labels, attr_name, extra_args.len()) {
                Ok(with_heap(|h| h.get(extra_args[arg_idx].key()).clone()).elide_lvalue())
            } else {
                with_heap(|h| h.get(operand.key()).clone()).attr_by_name(attr_name)
            }
        }
        _ => Err(ExecutorError::CannotAttribute("stateful expression")),
    }
}

fn mutate_function_attr(
    inv: &mut InvokedFunction,
    attr_name: &str,
    mutation: AttrMutation,
) -> Result<AttrMutationResult, ExecutorError> {
    let Some(arg_idx) =
        labeled_argument_index(&inv.body.labels, attr_name, inv.body.arguments.len())
    else {
        return Err(ExecutorError::missing_labeled_argument(attr_name));
    };

    let body = &mut inv.body;
    let result = set_argument(
        &mut body.arguments,
        &mut body.boxed_arguments,
        arg_idx,
        mutation,
    )?;
    inv.cache.0.take();
    Ok(result)
}

fn mutate_operator_attr(
    inv: &mut InvokedOperator,
    attr_name: &str,
    mutation: AttrMutation,
    modified_stack: Option<usize>,
) -> Result<AttrMutationResult, ExecutorError> {
    if let Some(arg_idx) =
        labeled_argument_index(&inv.body.labels, attr_name, inv.body.arguments.len())
    {
        let body = &mut inv.body;
        let result = set_argument(
            &mut body.arguments,
            &mut body.boxed_arguments,
            arg_idx,
            mutation,
        )?;
        inv.cache.cached_result.take();
        inv.cache.unmodified.take();
        return Ok(result);
    }

    let boxes_operand = mutation.boxes_containing_slots();
    let result = inv
        .body
        .operand
        .mutate_attr_by_name(attr_name, mutation, modified_stack)?;
    inv.body.boxed_operand = boxes_operand;
    inv.cache.cached_result.take();
    inv.cache.unmodified.take();
    Ok(result)
}

fn mutate_stateful_attr(
    stateful: &mut Stateful,
    attr_name: &str,
    mutation: AttrMutation,
    modified_stack: Option<usize>,
) -> Result<AttrMutationResult, ExecutorError> {
    enum Target {
        Call(usize),
        OperatorArg(usize),
        OperatorOperand,
    }

    let target = match &stateful.body.root {
        StatefulNode::LabeledCall { labels, args, .. } => {
            labeled_argument_index(labels, attr_name, args.len())
                .map(Target::Call)
                .ok_or_else(|| ExecutorError::missing_labeled_argument(attr_name))?
        }
        StatefulNode::LabeledOperatorCall {
            labels, extra_args, ..
        } => labeled_argument_index(labels, attr_name, extra_args.len())
            .map(Target::OperatorArg)
            .unwrap_or(Target::OperatorOperand),
        _ => return Err(ExecutorError::CannotAttribute("stateful expression")),
    };

    let result = match target {
        Target::Call(arg_idx) => {
            let StatefulNode::LabeledCall { args, .. } = &mut stateful.body.root else {
                unreachable!();
            };
            apply_attr_mutation_to_heap_ref(&mut args[arg_idx], mutation)
        }
        Target::OperatorArg(arg_idx) => {
            let StatefulNode::LabeledOperatorCall { extra_args, .. } = &mut stateful.body.root
            else {
                unreachable!();
            };
            apply_attr_mutation_to_heap_ref(&mut extra_args[arg_idx], mutation)
        }
        Target::OperatorOperand => {
            let StatefulNode::LabeledOperatorCall { operand, .. } = &mut stateful.body.root else {
                unreachable!();
            };
            let key = operand.make_mut();
            mutate_attr_in_heap(key, attr_name, mutation, modified_stack)
        }
    }?;

    reset_stateful_cache(stateful);
    Ok(result)
}

fn labeled_argument_index(
    labels: &smallvec::SmallVec<[(usize, String); 4]>,
    attr_name: &str,
    arg_len: usize,
) -> Option<usize> {
    labels
        .iter()
        .find_map(|(arg_idx, label)| (*arg_idx < arg_len && label == attr_name).then_some(*arg_idx))
}

fn set_argument(
    arguments: &mut [Value],
    boxed_arguments: &mut smallvec::SmallVec<[bool; 8]>,
    arg_idx: usize,
    mutation: AttrMutation,
) -> Result<AttrMutationResult, ExecutorError> {
    boxed_arguments.resize(arguments.len(), false);
    match mutation {
        AttrMutation::Direct(rhs) => {
            arguments[arg_idx] = rhs;
            boxed_arguments[arg_idx] = false;
            Ok(AttrMutationResult::Updated)
        }
        AttrMutation::Boxed(rhs) => {
            let key = arguments[arg_idx].make_mut_lvalue();
            heap_replace(key, rhs);
            boxed_arguments[arg_idx] = true;
            Ok(AttrMutationResult::Updated)
        }
        AttrMutation::Lvalue => {
            let key = arguments[arg_idx].make_mut_lvalue();
            boxed_arguments[arg_idx] = true;
            Ok(AttrMutationResult::Lvalue(key))
        }
    }
}

fn apply_attr_mutation_to_heap_ref(
    value_ref: &mut VRc,
    mutation: AttrMutation,
) -> Result<AttrMutationResult, ExecutorError> {
    let key = value_ref.make_mut();
    match mutation {
        AttrMutation::Direct(rhs) | AttrMutation::Boxed(rhs) => {
            heap_replace(key, rhs);
            Ok(AttrMutationResult::Updated)
        }
        AttrMutation::Lvalue => Ok(AttrMutationResult::Lvalue(key)),
    }
}
