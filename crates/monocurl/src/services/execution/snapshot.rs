use executor::{
    camera::camera_value_from_snapshot,
    error::RuntimeError,
    executor::Executor,
    heap::{VRc, with_heap},
    scene_snapshot::SceneSnapshot,
    state::LeaderKind,
    value::{MeshAttributePathSegment, Value, container::List},
};
use futures::channel::mpsc::UnboundedSender;
use structs::rope::{Rope, TextAggregate};

use crate::{services::ServiceManagerMessage, state::diagnostics::Diagnostic};

use super::{
    ExecutionService, ExecutionSnapshot, ExecutionStatus, MeshAttributeSnapshot, MeshEntrySnapshot,
    ParameterSnapshot, ParameterValue, PlaybackMode, PresentationUpdateTarget,
    diagnostics::format_runtime_error_message,
};

impl ExecutionService {
    pub(super) async fn capture_stable_scene_snapshot(
        executor: &mut Executor,
    ) -> Result<SceneSnapshot, RuntimeError> {
        executor.capture_stable_scene_snapshot().await
    }

    pub(super) fn runtime_value_from_parameter(value: &ParameterValue) -> Option<Value> {
        Some(match value {
            ParameterValue::Int(n) => Value::Integer(*n),
            ParameterValue::VectorInt(values) => Value::List(List::new_with(
                values.iter().map(|&value| VRc::new(Value::Integer(value))),
            )),
            ParameterValue::Float(f) => Value::Float(*f),
            ParameterValue::VectorFloat(values) => Value::List(List::new_with(
                values.iter().map(|&value| VRc::new(Value::Float(value))),
            )),
            ParameterValue::Complex { re, im } => Value::Complex { re: *re, im: *im },
            ParameterValue::Camera(camera) => camera_value_from_snapshot(camera),
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
        let mut meshes = Vec::new();

        for (leader_index, entry) in executor.state.leaders.iter().enumerate() {
            let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
            let locked = matches!(&cell_val, Value::Leader(l) if l.locked_by_anim.is_some());

            match entry.kind {
                LeaderKind::Scene => {}
                LeaderKind::Mesh => {
                    let follower_val = with_heap(|h| h.get(entry.follower_value).clone());
                    meshes.push(MeshEntrySnapshot {
                        leader_index,
                        name: entry.name.clone(),
                        locked,
                        attributes: Self::mesh_attributes_from_runtime(
                            leader_index,
                            follower_val,
                            &[],
                        ),
                    });
                }
            }
        }

        ParameterSnapshot {
            params: Vec::new(),
            meshes,
        }
    }

    fn mesh_attributes_from_runtime(
        leader_index: usize,
        value: Value,
        parent_path: &[MeshAttributePathSegment],
    ) -> Vec<MeshAttributeSnapshot> {
        match value.elide_lvalue() {
            Value::InvokedFunction(inv) => inv
                .body
                .labels
                .iter()
                .filter_map(|(arg_idx, name)| {
                    let value = inv.body.arguments.get(*arg_idx)?.clone();
                    Some(Self::mesh_labeled_attribute_snapshot(
                        leader_index,
                        parent_path,
                        MeshAttributePathSegment::FunctionArgument(*arg_idx),
                        name.clone(),
                        value,
                    ))
                })
                .collect(),
            Value::InvokedOperator(inv) => {
                let mut attributes = Vec::new();
                for (arg_idx, name) in &inv.body.labels {
                    let Some(value) = inv.body.arguments.get(*arg_idx).cloned() else {
                        continue;
                    };
                    attributes.push(Self::mesh_labeled_attribute_snapshot(
                        leader_index,
                        parent_path,
                        MeshAttributePathSegment::OperatorArgument(*arg_idx),
                        name.clone(),
                        value,
                    ));
                }

                let mut operand_path = parent_path.to_vec();
                operand_path.push(MeshAttributePathSegment::OperatorOperand);
                let operand_attributes = Self::mesh_attributes_from_runtime(
                    leader_index,
                    inv.body.operand.as_ref().clone(),
                    &operand_path,
                );
                attributes.extend(operand_attributes);
                attributes
            }
            Value::List(list) => list
                .elements()
                .iter()
                .enumerate()
                .filter_map(|(index, element)| {
                    let mut item_path = parent_path.to_vec();
                    item_path.push(MeshAttributePathSegment::ListIndex(index));
                    let attributes = Self::mesh_attributes_from_runtime(
                        leader_index,
                        with_heap(|h| h.get(element.key()).clone()),
                        &item_path,
                    );
                    (!attributes.is_empty()).then(|| MeshAttributeSnapshot {
                        target: None,
                        name: format!("item {}", index + 1),
                        value: ParameterValue::Other,
                        children: attributes,
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn mesh_labeled_attribute_snapshot(
        leader_index: usize,
        parent_path: &[MeshAttributePathSegment],
        segment: MeshAttributePathSegment,
        name: String,
        value: Value,
    ) -> MeshAttributeSnapshot {
        let mut path = parent_path.to_vec();
        path.push(segment);
        MeshAttributeSnapshot {
            target: Some(PresentationUpdateTarget::MeshAttribute {
                leader_index,
                path: path.clone(),
            }),
            name,
            value: Self::parameter_value_from_runtime(value.clone().elide_cached_wrappers_rec()),
            children: Self::mesh_attributes_from_runtime(leader_index, value, &path),
        }
    }

    pub(super) async fn emit_snapshot(
        sm_tx: &UnboundedSender<ServiceManagerMessage>,
        executor: &Executor,
        root_text_rope: &Rope<TextAggregate>,
        current_timestamp: executor::time::Timestamp,
        target_timestamp: executor::time::Timestamp,
        has_compiler_error: bool,
        is_playing: bool,
        is_loading: bool,
        playback_mode: PlaybackMode,
        version: usize,
        scene_snapshot: Option<SceneSnapshot>,
    ) {
        let parameters = (playback_mode == PlaybackMode::Presentation)
            .then(|| Self::parameter_snapshot(executor));
        let status = if has_compiler_error {
            ExecutionStatus::CompileError
        } else if executor.state.has_errors() {
            ExecutionStatus::RuntimeError
        } else if is_playing {
            ExecutionStatus::Playing
        } else {
            ExecutionStatus::Paused
        };

        let (background, camera, camera_version, meshes) = match scene_snapshot {
            Some(scene) => (
                Some(scene.background),
                Some(scene.camera),
                Some(scene.camera_version),
                Some(scene.meshes),
            ),
            None => (None, None, None, None),
        };

        let transcript = if is_loading || status == ExecutionStatus::CompileError {
            None
        } else {
            Some(executor.state.transcript.sections.clone())
        };

        let snapshot = ExecutionSnapshot {
            background,
            camera,
            camera_version,
            meshes,
            current_timestamp,
            target_timestamp,
            status,
            is_loading,
            slide_count: executor.real_slide_count(),
            slide_names: executor.real_slide_names(),
            slide_durations: executor.real_slide_durations(),
            minimum_slide_durations: executor.real_minimum_slide_durations(),
            parameters,
            transcript: transcript.clone(),
        };

        sm_tx
            .unbounded_send(ServiceManagerMessage::ExecutionStateUpdated { snapshot })
            .ok();

        if let Some(transcript) = transcript {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateTranscript {
                    transcript,
                    version,
                })
                .ok();
        }

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
