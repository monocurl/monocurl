use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{heap_replace, with_heap, with_heap_mut},
    value::{
        Value,
        invoked_operator::invalidate_invoked_operator_cache,
        stateful::{Stateful, StatefulNode, reset_stateful_cache},
    },
};
use stdlib_macros::stdlib_func;

use super::helpers::read_string;

fn bool_value(value: bool) -> Value {
    Value::Integer(i64::from(value))
}

fn follow_heap_lvalues(mut key: executor::heap::HeapKey) -> (executor::heap::HeapKey, Value) {
    let mut value = with_heap(|h| h.get(key).clone());
    while let Some(next_key) = value.as_lvalue_key() {
        key = next_key;
        value = with_heap(|h| h.get(key).clone());
    }
    (key, value)
}

fn read_elided_value(executor: &Executor, stack_idx: usize, index: i32) -> Value {
    executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
        .elide_leader()
}

fn has_attr_on_value(value: Value, attr_name: &str) -> bool {
    match value.elide_lvalue() {
        Value::Leader(leader) => has_attr_on_value(
            with_heap(|h| h.get(leader.leader_rc.key()).clone()),
            attr_name,
        ),
        Value::InvokedFunction(inv) => inv.body.labels.iter().any(|(_, label)| label == attr_name),
        Value::InvokedOperator(inv) => {
            inv.body.labels.iter().any(|(_, label)| label == attr_name)
                || has_attr_on_value(inv.body.operand.as_ref().clone(), attr_name)
        }
        Value::Stateful(stateful) => match &stateful.body.root {
            StatefulNode::LabeledCall { labels, .. } => {
                labels.iter().any(|(_, label)| label == attr_name)
            }
            StatefulNode::LabeledOperatorCall {
                labels, operand, ..
            } => {
                labels.iter().any(|(_, label)| label == attr_name)
                    || has_attr_on_value(with_heap(|h| h.get(operand.key()).clone()), attr_name)
            }
            _ => false,
        },
        _ => false,
    }
}

fn get_attr_from_value(value: Value, attr_name: &str) -> Result<Value, ExecutorError> {
    let base = value.elide_lvalue();
    match base {
        Value::Leader(leader) => get_attr_from_value(
            with_heap(|h| h.get(leader.leader_rc.key()).clone()),
            attr_name,
        ),
        Value::InvokedFunction(inv) => {
            if let Some((arg_idx, _)) = inv.body.labels.iter().find(|(_, label)| label == attr_name)
            {
                Ok(inv.body.arguments[*arg_idx].clone().elide_lvalue())
            } else {
                Err(ExecutorError::missing_labeled_argument(attr_name))
            }
        }
        Value::InvokedOperator(inv) => {
            if let Some((arg_idx, _)) = inv.body.labels.iter().find(|(_, label)| label == attr_name)
            {
                Ok(inv.body.arguments[*arg_idx].clone().elide_lvalue())
            } else {
                get_attr_from_value(inv.body.operand.as_ref().clone(), attr_name)
            }
        }
        Value::Stateful(stateful) => match &stateful.body.root {
            StatefulNode::LabeledCall { labels, args, .. } => {
                if let Some((arg_idx, _)) = labels.iter().find(|(_, label)| label == attr_name) {
                    Ok(with_heap(|h| h.get(args[*arg_idx].key()).clone()).elide_lvalue())
                } else {
                    Err(ExecutorError::missing_labeled_argument(attr_name))
                }
            }
            StatefulNode::LabeledOperatorCall {
                labels,
                operand,
                extra_args,
                ..
            } => {
                if let Some((arg_idx, _)) = labels.iter().find(|(_, label)| label == attr_name) {
                    Ok(with_heap(|h| h.get(extra_args[*arg_idx].key()).clone()).elide_lvalue())
                } else {
                    get_attr_from_value(with_heap(|h| h.get(operand.key()).clone()), attr_name)
                }
            }
            _ => Err(ExecutorError::CannotAttribute("stateful expression")),
        },
        _ => Err(ExecutorError::CannotAttribute(base.type_name())),
    }
}

fn set_attr_on_stateful(
    stateful: &mut Stateful,
    attr_name: &str,
    rhs: &Value,
    stack_id: usize,
) -> Result<(), ExecutorError> {
    enum Target {
        Call(usize),
        OperatorArg(usize),
        OperatorOperand,
    }

    let target = match &stateful.body.root {
        StatefulNode::LabeledCall { labels, .. } => labels
            .iter()
            .find_map(|(arg_idx, label)| (label == attr_name).then_some(Target::Call(*arg_idx)))
            .ok_or_else(|| ExecutorError::missing_labeled_argument(attr_name))?,
        StatefulNode::LabeledOperatorCall { labels, .. } => labels
            .iter()
            .find_map(|(arg_idx, label)| {
                (label == attr_name).then_some(Target::OperatorArg(*arg_idx))
            })
            .unwrap_or(Target::OperatorOperand),
        _ => {
            return Err(ExecutorError::CannotAttribute("stateful expression"));
        }
    };

    match target {
        Target::Call(arg_idx) => {
            let key = {
                let body = &mut stateful.body;
                let StatefulNode::LabeledCall { args, .. } = &mut body.root else {
                    unreachable!();
                };
                args[arg_idx].make_mut()
            };
            heap_replace(key, rhs.clone());
        }
        Target::OperatorArg(arg_idx) => {
            let key = {
                let body = &mut stateful.body;
                let StatefulNode::LabeledOperatorCall { extra_args, .. } = &mut body.root else {
                    unreachable!();
                };
                extra_args[arg_idx].make_mut()
            };
            heap_replace(key, rhs.clone());
        }
        Target::OperatorOperand => {
            let key = {
                let body = &mut stateful.body;
                let StatefulNode::LabeledOperatorCall { operand, .. } = &mut body.root else {
                    unreachable!();
                };
                operand.make_mut()
            };
            set_attr_in_heap(key, attr_name, rhs, stack_id)?;
        }
    }

    reset_stateful_cache(stateful);
    Ok(())
}

fn set_attr_on_value(
    value: &mut Value,
    attr_name: &str,
    rhs: &Value,
    stack_id: usize,
) -> Result<(), ExecutorError> {
    match value {
        Value::Lvalue(vrc) => set_attr_in_heap(vrc.key(), attr_name, rhs, stack_id),
        Value::WeakLvalue(vweak) => set_attr_in_heap(vweak.key(), attr_name, rhs, stack_id),
        Value::Leader(leader) => set_attr_in_heap(leader.leader_rc.key(), attr_name, rhs, stack_id),
        Value::InvokedFunction(inv) => {
            let Some(arg_idx) = inv
                .body
                .labels
                .iter()
                .find_map(|(arg_idx, label)| (label == attr_name).then_some(*arg_idx))
            else {
                return Err(ExecutorError::missing_labeled_argument(attr_name));
            };

            let key = {
                let body = &mut inv.body;
                let key = body.arguments[arg_idx].make_mut_lvalue();
                body.boxed_arguments.resize(body.arguments.len(), false);
                body.boxed_arguments[arg_idx] = true;
                key
            };
            heap_replace(key, rhs.clone());
            inv.cache.0.take();
            Ok(())
        }
        Value::InvokedOperator(inv) => {
            if let Some(arg_idx) = inv
                .body
                .labels
                .iter()
                .find_map(|(arg_idx, label)| (label == attr_name).then_some(*arg_idx))
            {
                let key = {
                    let body = &mut inv.body;
                    let key = body.arguments[arg_idx].make_mut_lvalue();
                    body.boxed_arguments.resize(body.arguments.len(), false);
                    body.boxed_arguments[arg_idx] = true;
                    key
                };
                heap_replace(key, rhs.clone());
                invalidate_invoked_operator_cache(inv);
                return Ok(());
            }

            let key = {
                let body = &mut inv.body;
                let key = body.operand.as_mut().make_mut_lvalue();
                body.boxed_operand = true;
                key
            };
            invalidate_invoked_operator_cache(inv);
            set_attr_in_heap(key, attr_name, rhs, stack_id)
        }
        Value::Stateful(stateful) => set_attr_on_stateful(stateful, attr_name, rhs, stack_id),
        _ => Err(ExecutorError::CannotAttribute(value.type_name())),
    }
}

fn set_attr_in_heap(
    key: executor::heap::HeapKey,
    attr_name: &str,
    rhs: &Value,
    stack_id: usize,
) -> Result<(), ExecutorError> {
    let (key, base) = follow_heap_lvalues(key);

    if let Value::Leader(leader) = base {
        with_heap_mut(|h| {
            if let Value::Leader(stored_leader) = &mut *h.get_mut(key) {
                stored_leader.last_modified_stack = Some(stack_id);
                stored_leader.leader_version += 1;
            }
        });
        return set_attr_in_heap(leader.leader_rc.key(), attr_name, rhs, stack_id);
    }

    let mut base = base;
    set_attr_on_value(&mut base, attr_name, rhs, stack_id)?;
    heap_replace(key, base);
    Ok(())
}

macro_rules! type_predicate {
    ($name:ident, |$value:ident| $body:expr) => {
        #[stdlib_func]
        pub async fn $name(
            executor: &mut Executor,
            stack_idx: usize,
        ) -> Result<Value, ExecutorError> {
            let $value = read_elided_value(executor, stack_idx, -1);
            Ok(bool_value($body))
        }
    };
}

#[stdlib_func]
pub async fn runtime_error(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let message = read_string(executor, stack_idx, -1, "message").await?;
    Err(ExecutorError::invalid_operation(message))
}

#[stdlib_func]
pub async fn type_of(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    Ok(Value::String(
        read_elided_value(executor, stack_idx, -1)
            .type_name()
            .to_string(),
    ))
}

#[stdlib_func]
pub async fn has_attr(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let target = executor.state.stack(stack_idx).read_at(-2).clone();
    let attr_name = read_string(executor, stack_idx, -1, "name").await?;
    Ok(bool_value(has_attr_on_value(target, &attr_name)))
}

#[stdlib_func]
pub async fn get_attr(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let target = executor.state.stack(stack_idx).read_at(-2).clone();
    let attr_name = read_string(executor, stack_idx, -1, "name").await?;
    get_attr_from_value(target, &attr_name)
}

#[stdlib_func]
pub async fn set_attr(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let target = executor.state.stack(stack_idx).read_at(-3).clone();
    let attr_name = read_string(executor, stack_idx, -2, "name").await?;
    let rhs = executor.state.stack(stack_idx).read_at(-1).clone();
    let stack_id = stack_idx;

    if let Some(key) = target.as_lvalue_key() {
        set_attr_in_heap(key, &attr_name, &rhs, stack_id)?;
        return Ok(with_heap(|h| h.get(key).clone()).elide_lvalue());
    }

    let mut updated = target;
    set_attr_on_value(&mut updated, &attr_name, &rhs, stack_id)?;
    Ok(updated)
}

type_predicate!(is_nil, |value| matches!(value, Value::Nil));
type_predicate!(is_int, |value| matches!(value, Value::Integer(_)));
type_predicate!(is_float, |value| matches!(value, Value::Float(_)));
type_predicate!(is_complex, |value| matches!(value, Value::Complex { .. }));
type_predicate!(is_number, |value| matches!(
    value,
    Value::Integer(_) | Value::Float(_) | Value::Complex { .. }
));
type_predicate!(is_string, |value| matches!(value, Value::String(_)));
type_predicate!(is_list, |value| matches!(value, Value::List(_)));
type_predicate!(is_map, |value| matches!(value, Value::Map(_)));
type_predicate!(is_mesh, |value| matches!(value, Value::Mesh(_)));
type_predicate!(is_primitive_anim, |value| matches!(
    value,
    Value::PrimitiveAnim(_)
));
type_predicate!(is_anim_block, |value| matches!(value, Value::AnimBlock(_)));
type_predicate!(is_function, |value| matches!(
    value,
    Value::Lambda(_) | Value::InvokedFunction(_)
));
type_predicate!(is_operator, |value| matches!(
    value,
    Value::Operator(_) | Value::InvokedOperator(_)
));
type_predicate!(is_live_function, |value| matches!(
    value,
    Value::InvokedFunction(_)
));
type_predicate!(is_live_operator, |value| matches!(
    value,
    Value::InvokedOperator(_)
));
type_predicate!(is_callable, |value| matches!(
    value,
    Value::Lambda(_) | Value::Operator(_) | Value::InvokedFunction(_) | Value::InvokedOperator(_)
));
type_predicate!(is_stateful, |value| matches!(value, Value::Stateful(_)));
