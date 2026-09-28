//! evaluation of stateful values: the reactive expressions behind `$param`
//! reads, reevaluated against the current leader or follower state

use crate::{
    error::ExecutorError,
    heap::{VRc, with_heap},
    value::{
        Value,
        container::List,
        invoked_function::make_invoked_function,
        invoked_operator::{extract_operator_result, make_invoked_operator},
        stateful::{Stateful, StatefulNode, StatefulReadKind},
    },
};
use smallvec::SmallVec;

use super::{Executor, eager::prepare_eager_call_args};

impl Executor {
    pub(crate) fn eval_stateful_read_kind<'a>(
        &'a mut self,
        stateful: &'a crate::value::stateful::Stateful,
        override_read_kind: StatefulReadKind,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            if let Some(cached) = crate::value::stateful::stateful_cache_valid(stateful) {
                return Ok(cached);
            }
            let result = self
                .eval_stateful_node(&stateful.body.root, override_read_kind)
                .await?;
            let result = self.materialize_cached_value(result).await?;
            crate::value::stateful::stateful_update_cache(stateful, result.clone());
            Ok(result)
        })
    }

    pub fn eval_stateful<'a>(
        &'a mut self,
        stateful: &'a crate::value::stateful::Stateful,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            let read_kind = stateful.cache.read_kind;
            self.eval_stateful_read_kind(stateful, read_kind).await
        })
    }

    fn eval_stateful_node<'a>(
        &'a mut self,
        node: &'a StatefulNode,
        read_kind: StatefulReadKind,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            match node {
                StatefulNode::LeaderRef(key) => {
                    let inner = with_heap(|h| h.get(*key).clone());
                    match inner {
                        Value::Leader(leader) => Ok(match read_kind {
                            StatefulReadKind::Leader => {
                                with_heap(|h| h.get(leader.leader_rc.key()).clone())
                            }
                            StatefulReadKind::Follower => {
                                with_heap(|h| h.get(leader.follower_rc.key()).clone())
                            }
                        }),
                        other => Ok(other),
                    }
                }
                StatefulNode::Constant(val) => Ok(*val.clone()),
                StatefulNode::List(children) => {
                    let mut elements = Vec::with_capacity(children.len());
                    for child in children {
                        let val = self.eval_stateful_node(child, read_kind).await?;
                        elements.push(VRc::new(val));
                    }
                    Ok(Value::List(List::new_with(elements)))
                }
                StatefulNode::LabeledCall {
                    func,
                    args,
                    labels: _,
                } => {
                    let func_val = self.eval_stateful_node(func, read_kind).await?;
                    let lambda = match func_val.elide_lvalue() {
                        Value::Lambda(rc) => rc,
                        other => {
                            return Err(ExecutorError::type_error("lambda", other.type_name()));
                        }
                    };

                    let mut evaled: Vec<Value> = Vec::with_capacity(args.len());
                    for arg_key in args {
                        let arg_val = with_heap(|h| h.get(arg_key.key()).clone()).elide_lvalue();
                        let resolved = match arg_val {
                            Value::Stateful(ref s) => {
                                self.eval_stateful_read_kind(s, read_kind).await?
                            }
                            other => other,
                        };
                        evaled.push(resolved);
                    }
                    let full_args = prepare_eager_call_args(evaled, &lambda)?;
                    let trace_parent_idx = Some(self.state.last_stack_idx);
                    let result = self
                        .eagerly_invoke_lambda(&lambda, &full_args, trace_parent_idx)
                        .await?;
                    self.resolve_live_value(result).await
                }
                StatefulNode::LabeledOperatorCall {
                    operator,
                    operand,
                    extra_args,
                    ..
                } => {
                    let op_val = self.eval_stateful_node(operator, read_kind).await?;
                    let operator_inner = match op_val.elide_lvalue() {
                        Value::Operator(op) => op,
                        other => {
                            return Err(ExecutorError::type_error("operator", other.type_name()));
                        }
                    };

                    let operand_val = {
                        let v = with_heap(|h| h.get(operand.key()).clone()).elide_lvalue();
                        match v {
                            Value::Stateful(ref s) => {
                                self.eval_stateful_read_kind(s, read_kind).await?
                            }
                            other => other,
                        }
                    };

                    let mut evaled: Vec<Value> = vec![operand_val];
                    for arg_key in extra_args {
                        let arg_val = with_heap(|h| h.get(arg_key.key()).clone()).elide_lvalue();
                        let resolved = match arg_val {
                            Value::Stateful(ref s) => {
                                self.eval_stateful_read_kind(s, read_kind).await?
                            }
                            other => other,
                        };
                        evaled.push(resolved);
                    }
                    let full_args = prepare_eager_call_args(evaled, &operator_inner.0)?;
                    let trace_parent_idx = Some(self.state.last_stack_idx);
                    let raw = self
                        .eagerly_invoke_lambda(&operator_inner.0, &full_args, trace_parent_idx)
                        .await?;
                    let (_, modified) = extract_operator_result(raw)?;
                    self.resolve_live_value(modified).await
                }
            }
        })
    }

    /// the live call a stateful value currently stands for: its reactive reads are
    /// resolved, but each call keeps its operand and arguments so that it can be
    /// interpolated like any other live call. shapes other than calls evaluate as usual
    pub(crate) fn stateful_as_live_call<'a>(
        &'a mut self,
        stateful: &'a Stateful,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            self.stateful_node_as_live_call(&stateful.body.root, stateful.cache.read_kind)
                .await
        })
    }

    fn stateful_node_as_live_call<'a>(
        &'a mut self,
        node: &'a StatefulNode,
        read_kind: StatefulReadKind,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            match node {
                StatefulNode::LabeledCall { func, args, labels } => {
                    let func_val = self.eval_stateful_node(func, read_kind).await?;
                    let Value::Lambda(lambda) = func_val.clone().elide_lvalue() else {
                        return Err(ExecutorError::type_error("lambda", func_val.type_name()));
                    };
                    let arguments = self.stateful_slots_as_live_calls(args, read_kind).await?;

                    let full_args = prepare_eager_call_args(arguments.iter().cloned(), &lambda)?;
                    let trace_parent_idx = Some(self.state.last_stack_idx);
                    let result = self
                        .eagerly_invoke_lambda(&lambda, &full_args, trace_parent_idx)
                        .await?;
                    let result = self.materialize_cached_value(result).await?;

                    Ok(Value::InvokedFunction(make_invoked_function(
                        func_val,
                        arguments,
                        labels.clone(),
                        Some(result),
                    )))
                }
                StatefulNode::LabeledOperatorCall {
                    operator,
                    operand,
                    extra_args,
                    labels,
                } => {
                    let operator_val = self.eval_stateful_node(operator, read_kind).await?;
                    let Value::Operator(operator_inner) = operator_val.clone().elide_lvalue() else {
                        return Err(ExecutorError::type_error(
                            "operator",
                            operator_val.type_name(),
                        ));
                    };
                    let operand_val = self.stateful_slot_as_live_call(operand, read_kind).await?;
                    let arguments = self
                        .stateful_slots_as_live_calls(extra_args, read_kind)
                        .await?;

                    let full_args = prepare_eager_call_args(
                        std::iter::once(operand_val.clone()).chain(arguments.iter().cloned()),
                        &operator_inner.0,
                    )?;
                    let trace_parent_idx = Some(self.state.last_stack_idx);
                    let raw = self
                        .eagerly_invoke_lambda(&operator_inner.0, &full_args, trace_parent_idx)
                        .await?;
                    let (initial, modified) = extract_operator_result(raw)?;
                    let initial = self.materialize_cached_value(initial).await?;
                    let modified = self.materialize_cached_value(modified).await?;

                    Ok(Value::InvokedOperator(make_invoked_operator(
                        operator_val,
                        operand_val,
                        arguments,
                        labels.clone(),
                        initial,
                        modified,
                    )))
                }
                other => self.eval_stateful_node(other, read_kind).await,
            }
        })
    }

    async fn stateful_slot_as_live_call(
        &mut self,
        slot: &VRc,
        read_kind: StatefulReadKind,
    ) -> Result<Value, ExecutorError> {
        match with_heap(|h| h.get(slot.key()).clone()).elide_lvalue() {
            Value::Stateful(stateful) => {
                self.stateful_node_as_live_call(&stateful.body.root, read_kind)
                    .await
            }
            other => Ok(other),
        }
    }

    async fn stateful_slots_as_live_calls(
        &mut self,
        slots: &[VRc],
        read_kind: StatefulReadKind,
    ) -> Result<SmallVec<[Value; 8]>, ExecutorError> {
        let mut values = SmallVec::with_capacity(slots.len());
        for slot in slots {
            values.push(self.stateful_slot_as_live_call(slot, read_kind).await?);
        }
        Ok(values)
    }
}
