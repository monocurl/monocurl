use crate::{
    error::ExecutorError,
    heap::{heap_replace, with_heap, with_heap_mut},
    state::LeaderKind,
    value::{MeshAttributePathSegment, Value},
};

use super::Executor;

impl Executor {
    pub fn update_parameter(&mut self, name: &str, value: Value) -> Result<(), ExecutorError> {
        let leader_index = self
            .state
            .leaders
            .iter()
            .position(|entry| entry.kind == LeaderKind::Param && entry.name == name)
            .ok_or_else(|| ExecutorError::unknown_parameter(name))?;
        self.update_parameter_by_leader_index(leader_index, value)
    }

    pub fn update_parameter_by_leader_index(
        &mut self,
        leader_index: usize,
        value: Value,
    ) -> Result<(), ExecutorError> {
        let param = self
            .state
            .leaders
            .get(leader_index)
            .filter(|entry| entry.kind == LeaderKind::Param)
            .ok_or_else(|| ExecutorError::unknown_parameter(format!("#{leader_index}")))?;
        let value = value.elide_lvalue().elide_leader();

        let leader_cell_key = param.leader_cell.key();
        let cell_val = with_heap(|h| h.get(leader_cell_key).clone());
        if matches!(&cell_val, Value::Leader(leader) if leader.locked_by_anim.is_some()) {
            return Ok(());
        }

        let leader_value_key = param.leader_value;
        let follower_value_key = param.follower_value;

        heap_replace(leader_value_key, value.clone());
        heap_replace(follower_value_key, value);
        with_heap_mut(|h| {
            if let Value::Leader(l) = &mut *h.get_mut(leader_cell_key) {
                l.leader_version += 1;
                l.follower_version += 1;
            }
        });
        Ok(())
    }

    pub fn update_mesh_attribute(
        &mut self,
        leader_index: usize,
        path: &[MeshAttributePathSegment],
        value: Value,
    ) -> Result<(), ExecutorError> {
        let entry = self
            .state
            .leaders
            .get(leader_index)
            .filter(|entry| entry.kind == LeaderKind::Mesh)
            .ok_or_else(|| {
                ExecutorError::invalid_access(format!("unknown mesh #{leader_index}"))
            })?;
        if path.is_empty() {
            return Err(ExecutorError::invalid_access(
                "mesh attribute update requires an attribute path",
            ));
        }

        let cell_key = entry.leader_cell.key();
        let cell_val = with_heap(|h| h.get(cell_key).clone());
        let Value::Leader(leader) = cell_val else {
            return Err(ExecutorError::type_error("leader", cell_val.type_name()));
        };
        if leader.locked_by_anim.is_some() {
            return Err(ExecutorError::ConcurrentAnimation);
        }

        let mut leader_value = with_heap(|h| h.get(leader.leader_rc.key()).clone());
        let mut follower_value = with_heap(|h| h.get(leader.follower_rc.key()).clone());
        let value = value.elide_lvalue().elide_leader();

        update_live_attribute_value(&mut leader_value, path, value.clone())?;
        update_live_attribute_value(&mut follower_value, path, value)?;

        heap_replace(leader.leader_rc.key(), leader_value);
        heap_replace(leader.follower_rc.key(), follower_value);
        with_heap_mut(|h| {
            if let Value::Leader(l) = &mut *h.get_mut(cell_key) {
                l.leader_version += 1;
                l.follower_version += 1;
            }
        });

        Ok(())
    }
}

fn update_live_attribute_value(
    value: &mut Value,
    path: &[MeshAttributePathSegment],
    replacement: Value,
) -> Result<(), ExecutorError> {
    if matches!(value, Value::Stateful(_)) {
        return Err(ExecutorError::stateful_value(
            "cannot update attributes on stateful meshes",
        ));
    }

    match value {
        Value::Lvalue(vrc) => {
            let mut inner = with_heap(|h| h.get(vrc.key()).clone());
            update_live_attribute_value(&mut inner, path, replacement)?;
            *value = inner;
            return Ok(());
        }
        Value::WeakLvalue(vweak) => {
            let mut inner = with_heap(|h| h.get(vweak.key()).clone());
            update_live_attribute_value(&mut inner, path, replacement)?;
            *value = inner;
            return Ok(());
        }
        _ => {}
    }

    let Some((segment, rest)) = path.split_first() else {
        return Err(ExecutorError::invalid_access(
            "mesh attribute update requires an attribute path",
        ));
    };

    match (value, segment) {
        (Value::List(list), MeshAttributePathSegment::ListIndex(index)) => {
            let Some(element) = list.elements.get(*index) else {
                return Err(ExecutorError::IndexOutOfBounds {
                    index: *index,
                    len: list.len(),
                });
            };
            let mut element_value = with_heap(|h| h.get(element.key()).clone());
            update_live_attribute_value(&mut element_value, rest, replacement)?;
            list.elements[*index] = crate::heap::VRc::new(element_value);
            Ok(())
        }
        (Value::InvokedFunction(inv), MeshAttributePathSegment::FunctionArgument(index)) => {
            if *index >= inv.body.arguments.len() {
                return Err(ExecutorError::invalid_access(format!(
                    "live function has no argument #{index}",
                )));
            }
            if rest.is_empty() {
                inv.body.arguments[*index] = replacement;
            } else {
                let mut argument = inv.body.arguments[*index].clone().elide_lvalue();
                update_live_attribute_value(&mut argument, rest, replacement)?;
                inv.body.arguments[*index] = argument;
            }
            let arg_len = inv.body.arguments.len();
            inv.body.boxed_arguments.resize(arg_len, false);
            inv.body.boxed_arguments[*index] = false;
            inv.cache.0.take();
            Ok(())
        }
        (Value::InvokedOperator(inv), MeshAttributePathSegment::OperatorArgument(index)) => {
            if *index >= inv.body.arguments.len() {
                return Err(ExecutorError::invalid_access(format!(
                    "live operator has no argument #{index}",
                )));
            }
            if rest.is_empty() {
                inv.body.arguments[*index] = replacement;
            } else {
                let mut argument = inv.body.arguments[*index].clone().elide_lvalue();
                update_live_attribute_value(&mut argument, rest, replacement)?;
                inv.body.arguments[*index] = argument;
            }
            let arg_len = inv.body.arguments.len();
            inv.body.boxed_arguments.resize(arg_len, false);
            inv.body.boxed_arguments[*index] = false;
            inv.cache.cached_result.take();
            inv.cache.unmodified.take();
            Ok(())
        }
        (Value::InvokedOperator(inv), MeshAttributePathSegment::OperatorOperand) => {
            if rest.is_empty() {
                return Err(ExecutorError::invalid_access(
                    "cannot directly update a live operator operand",
                ));
            }
            let mut operand = inv.body.operand.as_ref().clone().elide_lvalue();
            update_live_attribute_value(&mut operand, rest, replacement)?;
            *inv.body.operand = operand;
            inv.body.boxed_operand = false;
            inv.cache.cached_result.take();
            inv.cache.unmodified.take();
            Ok(())
        }
        (Value::Stateful(_), _) => Err(ExecutorError::stateful_value(
            "cannot update attributes on stateful meshes",
        )),
        (other, _) => Err(ExecutorError::CannotAttribute(other.type_name())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytecode::{Bytecode, SectionBytecode, SectionFlags};
    use smallvec::smallvec;

    use super::Executor;
    use crate::{
        error::ExecutorError,
        heap::{heap_replace, with_heap, with_heap_mut},
        state::LeaderKind,
        value::{
            MeshAttributePathSegment, Value, invoked_function::make_invoked_function,
            invoked_operator::make_invoked_operator,
        },
    };

    fn executor_with_sections(flags: &[SectionFlags]) -> Executor {
        Executor::new(
            Bytecode::new(
                flags
                    .iter()
                    .cloned()
                    .map(SectionBytecode::new)
                    .map(Arc::new)
                    .collect(),
            ),
            Vec::new(),
        )
    }

    fn empty_executor() -> Executor {
        executor_with_sections(&[SectionFlags {
            is_stdlib: false,
            is_library: false,
            is_init: false,
            is_root_module: true,
        }])
    }

    #[test]
    fn update_parameter_syncs_leader_and_follower() {
        let mut executor = empty_executor();
        executor
            .state
            .stack_mut(crate::state::ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(5));
        executor.state.promote_to_leader(
            crate::state::ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Param,
            "speed".into(),
        );

        executor
            .update_parameter("speed", Value::Float(2.5))
            .unwrap();

        let param = &executor.state.active_params[0];
        let leader_val = with_heap(|h| h.get(param.leader_value).clone());
        match leader_val {
            Value::Float(value) => assert_eq!(value, 2.5),
            other => panic!("expected float leader value, got {}", other.type_name()),
        }
        let follower_val = with_heap(|h| h.get(param.follower_value).clone());
        match follower_val {
            Value::Float(value) => assert_eq!(value, 2.5),
            other => panic!("expected float follower value, got {}", other.type_name()),
        }
    }

    #[test]
    fn update_parameter_errors_for_missing_name() {
        let mut executor = empty_executor();
        let error = executor.update_parameter("missing", Value::Integer(1));
        assert!(matches!(error, Err(ExecutorError::UnknownParameter(_))));
    }

    #[test]
    fn update_parameter_elides_top_level_lvalue_value() {
        let mut executor = empty_executor();
        executor
            .state
            .stack_mut(crate::state::ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(5));
        executor.state.promote_to_leader(
            crate::state::ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Param,
            "speed".into(),
        );

        executor
            .update_parameter(
                "speed",
                Value::Lvalue(crate::heap::VRc::new(Value::Float(2.5))),
            )
            .unwrap();

        let param = &executor.state.active_params[0];
        match with_heap(|h| h.get(param.leader_value).clone()) {
            Value::Float(value) => assert_eq!(value, 2.5),
            other => panic!("expected float leader value, got {}", other.type_name()),
        }
        match with_heap(|h| h.get(param.follower_value).clone()) {
            Value::Float(value) => assert_eq!(value, 2.5),
            other => panic!("expected float follower value, got {}", other.type_name()),
        }
    }

    #[test]
    fn update_parameter_no_ops_when_locked_by_animation() {
        let mut executor = empty_executor();
        executor
            .state
            .stack_mut(crate::state::ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(5));
        executor.state.promote_to_leader(
            crate::state::ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Param,
            "speed".into(),
        );
        let param = executor.state.active_params[0].clone();
        with_heap_mut(|h| {
            if let Value::Leader(leader) = &mut *h.get_mut(param.leader_cell.key()) {
                leader.locked_by_anim = Some(0);
            }
        });

        executor
            .update_parameter("speed", Value::Float(2.5))
            .unwrap();

        assert!(matches!(
            with_heap(|h| h.get(param.leader_value).clone()),
            Value::Integer(5)
        ));
        assert!(matches!(
            with_heap(|h| h.get(param.follower_value).clone()),
            Value::Integer(5)
        ));
    }

    #[test]
    fn update_mesh_attribute_syncs_leader_and_follower() {
        let mut executor = empty_executor();
        let base = Value::InvokedFunction(make_invoked_function(
            Value::Nil,
            smallvec![Value::Float(1.0)],
            smallvec![(0, "radius".into())],
            Some(Value::Nil),
        ));
        let live = Value::InvokedOperator(make_invoked_operator(
            Value::Nil,
            base,
            smallvec![Value::Integer(2)],
            smallvec![(0, "copies".into())],
            Value::Nil,
            Value::Nil,
        ));
        executor
            .state
            .stack_mut(crate::state::ExecutionState::ROOT_STACK_IDX)
            .push(live.clone());
        executor.state.promote_to_leader(
            crate::state::ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Mesh,
            "shape".into(),
        );
        let entry = executor.state.leaders[0].clone();
        heap_replace(entry.follower_value, live);

        executor
            .update_mesh_attribute(
                0,
                &[
                    MeshAttributePathSegment::OperatorOperand,
                    MeshAttributePathSegment::FunctionArgument(0),
                ],
                Value::Float(3.0),
            )
            .unwrap();

        for key in [entry.leader_value, entry.follower_value] {
            let Value::InvokedOperator(op) = with_heap(|h| h.get(key).clone()) else {
                panic!("expected live operator");
            };
            let Value::InvokedFunction(func) = op.body.operand.as_ref() else {
                panic!("expected live function operand");
            };
            match &func.body.arguments[0] {
                Value::Float(value) => assert_eq!(*value, 3.0),
                other => panic!("expected float argument, got {}", other.type_name()),
            }
        }
    }

    #[test]
    fn update_mesh_attribute_errors_when_locked_by_animation() {
        let mut executor = empty_executor();
        let live = Value::InvokedFunction(make_invoked_function(
            Value::Nil,
            smallvec![Value::Float(1.0)],
            smallvec![(0, "radius".into())],
            Some(Value::Nil),
        ));
        executor
            .state
            .stack_mut(crate::state::ExecutionState::ROOT_STACK_IDX)
            .push(live.clone());
        executor.state.promote_to_leader(
            crate::state::ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Mesh,
            "shape".into(),
        );
        let entry = executor.state.leaders[0].clone();
        heap_replace(entry.follower_value, live);
        with_heap_mut(|h| {
            if let Value::Leader(leader) = &mut *h.get_mut(entry.leader_cell.key()) {
                leader.locked_by_anim = Some(0);
            }
        });

        let error = executor.update_mesh_attribute(
            0,
            &[MeshAttributePathSegment::FunctionArgument(0)],
            Value::Float(3.0),
        );

        assert!(matches!(error, Err(ExecutorError::ConcurrentAnimation)));
    }
}
