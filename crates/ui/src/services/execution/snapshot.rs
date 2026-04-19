use std::collections::{HashMap, HashSet};

use executor::{
    executor::Executor,
    heap::{VRc, with_heap},
    scene_snapshot::SceneSnapshot,
    value::{Value, container::List},
};
use futures::channel::mpsc::UnboundedSender;
use structs::rope::{Rope, TextAggregate};

use crate::{services::ServiceManagerMessage, state::diagnostics::Diagnostic};

use super::{
    ExecutionService, ExecutionSnapshot, ExecutionStatus, ParameterSnapshot, ParameterValue,
    PlaybackMode, diagnostics::format_runtime_error_message,
};

impl ExecutionService {
    pub(super) async fn capture_stable_scene_snapshot(
        executor: &mut Executor,
    ) -> Option<SceneSnapshot> {
        executor.capture_stable_scene_snapshot().await
    }

    pub(super) fn runtime_value_from_parameter(value: &ParameterValue) -> Option<Value> {
        Some(match value {
            ParameterValue::Int(n) => Value::Integer(*n),
            ParameterValue::VectorInt(values) => Value::List(std::rc::Rc::new(List::new_with(
                values
                    .iter()
                    .map(|&value| VRc::new(Value::Integer(value)))
                    .collect(),
            ))),
            ParameterValue::Float(f) => Value::Float(*f),
            ParameterValue::VectorFloat(values) => Value::List(std::rc::Rc::new(List::new_with(
                values
                    .iter()
                    .map(|&value| VRc::new(Value::Float(value)))
                    .collect(),
            ))),
            ParameterValue::Complex { re, im } => Value::Complex { re: *re, im: *im },
            ParameterValue::Other => return None,
        })
    }

    fn parameter_value_from_runtime(value: Value) -> ParameterValue {
        match value {
            Value::Integer(n) => ParameterValue::Int(n),
            Value::Float(f) => ParameterValue::Float(f),
            Value::Complex { re, im } => ParameterValue::Complex { re, im },
            Value::List(list) => {
                let ints = list
                    .elements()
                    .iter()
                    .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
                        Value::Integer(n) => Some(n),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(ints) = ints {
                    return ParameterValue::VectorInt(ints);
                }

                let floats = list
                    .elements()
                    .iter()
                    .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
                        Value::Integer(n) => Some(n as f64),
                        Value::Float(f) => Some(f),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                floats.map_or(ParameterValue::Other, ParameterValue::VectorFloat)
            }
            _ => ParameterValue::Other,
        }
    }

    fn parameter_snapshot(executor: &Executor) -> ParameterSnapshot {
        let mut parameters = HashMap::new();
        let mut locked_params = HashSet::new();
        let mut param_order = Vec::new();
        for param in &executor.state.active_params {
            let follower_val = with_heap(|h| h.get(param.follower_value).clone());
            let value = Self::parameter_value_from_runtime(follower_val);
            let cell_val = with_heap(|h| h.get(param.leader_cell.key()).clone());
            if matches!(&cell_val, Value::Leader(l) if l.locked_by_anim.is_some()) {
                locked_params.insert(param.name.clone());
            }
            parameters.insert(param.name.clone(), value);
            param_order.push(param.name.clone());
        }
        ParameterSnapshot {
            parameters,
            locked_params,
            param_order,
        }
    }

    pub(super) async fn emit_snapshot(
        sm_tx: &UnboundedSender<ServiceManagerMessage>,
        executor: &Executor,
        root_text_rope: &Rope<TextAggregate>,
        has_compiler_error: bool,
        is_playing: bool,
        is_loading: bool,
        playback_mode: PlaybackMode,
        version: usize,
        scene_snapshot: Option<SceneSnapshot>,
    ) {
        let parameters = Self::parameter_snapshot(executor);
        let status = if has_compiler_error {
            ExecutionStatus::CompileError
        } else if executor.state.has_errors() {
            ExecutionStatus::RuntimeError
        } else if is_loading {
            ExecutionStatus::Seeking
        } else if is_playing {
            ExecutionStatus::Playing
        } else {
            ExecutionStatus::Paused
        };

        let (background, camera, meshes) = match scene_snapshot {
            Some(scene) => (
                Some(scene.background),
                Some(scene.camera),
                Some(scene.meshes),
            ),
            None => (None, None, None),
        };

        let snapshot = ExecutionSnapshot {
            background,
            camera,
            meshes,
            current_timestamp: executor.internal_to_user_timestamp(executor.state.timestamp),
            status,
            slide_count: executor.real_slide_count(),
            slide_durations: executor.real_slide_durations(),
            minimum_slide_durations: executor.real_minimum_slide_durations(),
            parameters: (playback_mode == PlaybackMode::Presentation).then_some(parameters),
        };

        sm_tx
            .unbounded_send(ServiceManagerMessage::ExecutionStateUpdated { snapshot })
            .ok();

        let diagnostics = executor
            .state
            .errors
            .iter()
            .map(|runtime_error| Diagnostic {
                dtype: crate::state::diagnostics::DiagnosticType::RuntimeError,
                span: runtime_error.span.clone(),
                title: "Runtime Error".into(),
                message: format_runtime_error_message(executor, root_text_rope, runtime_error),
            })
            .collect();

        if has_compiler_error {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateRuntimeDiagnostics {
                    diagnostics: Vec::new(),
                    version,
                })
                .ok();
        } else {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateRuntimeDiagnostics {
                    diagnostics,
                    version,
                })
                .ok();
        }
    }
}
