use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::Arc,
};

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, with_heap},
    state::LeaderKind,
    value::{
        Value,
        container::{HashableKey, List, Map},
    },
};
use futures::channel::mpsc::UnboundedSender;
use geo::{mesh::Mesh, simd::Float3};
use structs::rope::{Rope, TextAggregate};

use crate::{services::ServiceManagerMessage, state::diagnostics::Diagnostic};

use super::{
    ExecutionService, ExecutionSnapshot, ExecutionStatus, ParameterSnapshot, ParameterValue,
    PlaybackMode, ViewportBackgroundSnapshot, ViewportCameraSnapshot,
    diagnostics::format_runtime_error_message,
};

pub(super) type StableSceneSnapshot = (
    ViewportBackgroundSnapshot,
    ViewportCameraSnapshot,
    Vec<Arc<Mesh>>,
);

impl ExecutionService {
    fn collect_scene_meshes<'a>(
        executor: &'a mut Executor,
        value: Value,
        target_name: &'a str,
        out: &'a mut Vec<Arc<Mesh>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            let value = value.elide_wrappers(executor).await?;
            match value {
                Value::Mesh(mesh) => {
                    out.push(mesh);
                    Ok(())
                }
                Value::List(list) => {
                    for item in list.elements() {
                        let item = with_heap(|h| h.get(item.key()).clone());
                        Self::collect_scene_meshes(executor, item, target_name, out).await?;
                    }
                    Ok(())
                }
                other => Err(ExecutorError::Other(format!(
                    "on-screen mesh '{}' must resolve to a mesh tree, got {}",
                    target_name,
                    other.type_name()
                ))),
            }
        })
    }

    async fn scene_meshes(executor: &mut Executor) -> Result<Vec<Arc<Mesh>>, ExecutorError> {
        let mut meshes = Vec::new();
        let leaders = executor
            .state
            .leaders
            .iter()
            .filter(|entry| entry.kind == LeaderKind::Mesh)
            .map(|entry| (entry.name.clone(), entry.follower_value))
            .collect::<Vec<_>>();

        for (name, follower_value) in leaders {
            let follower = with_heap(|h| h.get(follower_value).clone());
            Self::collect_scene_meshes(executor, follower, &name, &mut meshes).await?;
        }
        Ok(meshes)
    }

    fn map_field_value(map: &Map, name: &str) -> Option<Value> {
        map.get(&HashableKey::String(name.to_string()))
            .map(|value| with_heap(|h| h.get(value.key()).clone()))
    }

    async fn scene_field_value(
        executor: &mut Executor,
        name: &'static str,
    ) -> Result<Option<Value>, ExecutorError> {
        let follower = executor
            .state
            .leaders
            .iter()
            .rev()
            .find(|entry| entry.name == name)
            .map(|entry| with_heap(|h| h.get(entry.follower_value).clone()));

        match follower {
            Some(value) => Ok(Some(value.elide_wrappers(executor).await?)),
            None => Ok(None),
        }
    }

    async fn read_f32(
        executor: &mut Executor,
        value: Value,
        target: &'static str,
    ) -> Result<f32, ExecutorError> {
        match value.elide_wrappers(executor).await? {
            Value::Integer(n) => Ok(n as f32),
            Value::Float(f) => Ok(f as f32),
            other => Err(ExecutorError::type_error_for(
                "number",
                other.type_name(),
                target,
            )),
        }
    }

    async fn read_bool_flag(
        executor: &mut Executor,
        value: Value,
        target: &'static str,
    ) -> Result<bool, ExecutorError> {
        match value.elide_wrappers(executor).await? {
            Value::Integer(n) => Ok(n != 0),
            Value::Float(f) => Ok(f != 0.0),
            other => Err(ExecutorError::type_error_for(
                "number",
                other.type_name(),
                target,
            )),
        }
    }

    async fn read_float3(
        executor: &mut Executor,
        value: Value,
        target: &'static str,
    ) -> Result<Float3, ExecutorError> {
        let value = value.elide_wrappers(executor).await?;
        let Value::List(list) = value else {
            return Err(ExecutorError::type_error_for(
                "list of length 3",
                value.type_name(),
                target,
            ));
        };
        if list.len() != 3 {
            return Err(ExecutorError::Other(format!(
                "{}: expected list of length 3, got list of length {}",
                target,
                list.len()
            )));
        }

        let mut components = [0.0; 3];
        for (slot, component) in components.iter_mut().zip(list.elements()) {
            *slot = Self::read_f32(executor, with_heap(|h| h.get(component.key()).clone()), target)
                .await?;
        }
        Ok(Float3::from_array(components))
    }

    async fn read_float4(
        executor: &mut Executor,
        value: Value,
        target: &'static str,
    ) -> Result<(f32, f32, f32, f32), ExecutorError> {
        let value = value.elide_wrappers(executor).await?;
        let Value::List(list) = value else {
            return Err(ExecutorError::type_error_for(
                "list of length 4",
                value.type_name(),
                target,
            ));
        };
        if list.len() != 4 {
            return Err(ExecutorError::Other(format!(
                "{}: expected list of length 4, got list of length {}",
                target,
                list.len()
            )));
        }

        let mut components = [0.0; 4];
        for (slot, component) in components.iter_mut().zip(list.elements()) {
            *slot = Self::read_f32(executor, with_heap(|h| h.get(component.key()).clone()), target)
                .await?;
        }
        Ok((
            components[0],
            components[1],
            components[2],
            components[3],
        ))
    }

    async fn camera_snapshot_from_value(
        executor: &mut Executor,
        value: Value,
    ) -> Result<ViewportCameraSnapshot, ExecutorError> {
        let value = value.elide_wrappers(executor).await?;
        let Value::Map(map) = value else {
            return Err(ExecutorError::type_error_for(
                "camera",
                value.type_name(),
                "camera",
            ));
        };

        let Some(kind) = Self::map_field_value(&map, "kind") else {
            return Err(ExecutorError::Other("camera: missing 'kind' field".into()));
        };
        let kind = kind.elide_wrappers(executor).await?;
        if !matches!(kind, Value::String(ref kind) if kind == "camera") {
            return Err(ExecutorError::Other(format!(
                "camera must resolve to a camera object, got kind {}",
                match kind {
                    Value::String(ref kind) => kind.as_str(),
                    other => other.type_name(),
                }
            )));
        }

        let Some(position) = Self::map_field_value(&map, "position") else {
            return Err(ExecutorError::Other("camera: missing 'position' field".into()));
        };
        let Some(look_at) = Self::map_field_value(&map, "look_at") else {
            return Err(ExecutorError::Other("camera: missing 'look_at' field".into()));
        };
        let Some(up) = Self::map_field_value(&map, "up") else {
            return Err(ExecutorError::Other("camera: missing 'up' field".into()));
        };
        let Some(fov) = Self::map_field_value(&map, "fov") else {
            return Err(ExecutorError::Other("camera: missing 'fov' field".into()));
        };
        let Some(near) = Self::map_field_value(&map, "near") else {
            return Err(ExecutorError::Other("camera: missing 'near' field".into()));
        };
        let Some(far) = Self::map_field_value(&map, "far") else {
            return Err(ExecutorError::Other("camera: missing 'far' field".into()));
        };
        let Some(ortho) = Self::map_field_value(&map, "ortho") else {
            return Err(ExecutorError::Other("camera: missing 'ortho' field".into()));
        };

        Ok(ViewportCameraSnapshot {
            position: Self::read_float3(executor, position, "camera.position").await?,
            look_at: Self::read_float3(executor, look_at, "camera.look_at").await?,
            up: Self::read_float3(executor, up, "camera.up").await?,
            fov: Self::read_f32(executor, fov, "camera.fov").await?,
            near: Self::read_f32(executor, near, "camera.near").await?,
            far: Self::read_f32(executor, far, "camera.far").await?,
            ortho: Self::read_bool_flag(executor, ortho, "camera.ortho").await?,
        })
    }

    async fn camera_snapshot(
        executor: &mut Executor,
    ) -> Result<ViewportCameraSnapshot, ExecutorError> {
        match Self::scene_field_value(executor, "camera").await? {
            Some(value) => Self::camera_snapshot_from_value(executor, value).await,
            None => Ok(ViewportCameraSnapshot::default()),
        }
    }

    async fn background_snapshot_from_value(
        executor: &mut Executor,
        value: Value,
    ) -> Result<ViewportBackgroundSnapshot, ExecutorError> {
        let value = value.elide_wrappers(executor).await?;
        if matches!(value, Value::List(_)) {
            return Ok(ViewportBackgroundSnapshot {
                color: Self::read_float4(executor, value, "background").await?,
            });
        }

        let Value::Map(map) = value else {
            return Err(ExecutorError::type_error_for(
                "solid background / rgba 4-vector",
                value.type_name(),
                "background",
            ));
        };

        let Some(kind) = Self::map_field_value(&map, "kind") else {
            return Err(ExecutorError::Other("background: missing 'kind' field".into()));
        };
        let kind = kind.elide_wrappers(executor).await?;
        if !matches!(kind, Value::String(ref kind) if kind == "solid_background") {
            return Err(ExecutorError::Other(format!(
                "background must resolve to a solid background, got kind {}",
                match kind {
                    Value::String(ref kind) => kind.as_str(),
                    other => other.type_name(),
                }
            )));
        }

        let Some(color) = Self::map_field_value(&map, "color") else {
            return Err(ExecutorError::Other("background: missing 'color' field".into()));
        };

        Ok(ViewportBackgroundSnapshot {
            color: Self::read_float4(executor, color, "background.color").await?,
        })
    }

    async fn background_snapshot(
        executor: &mut Executor,
    ) -> Result<ViewportBackgroundSnapshot, ExecutorError> {
        match Self::scene_field_value(executor, "background").await? {
            Some(value) => Self::background_snapshot_from_value(executor, value).await,
            None => Ok(ViewportBackgroundSnapshot::default()),
        }
    }

    async fn stable_scene_snapshot(
        executor: &mut Executor,
    ) -> Result<StableSceneSnapshot, ExecutorError> {
        let meshes = Self::scene_meshes(executor).await?;
        let background = Self::background_snapshot(executor).await?;
        let camera = Self::camera_snapshot(executor).await?;
        Ok((background, camera, meshes))
    }

    pub(super) async fn capture_stable_scene_snapshot(
        executor: &mut Executor,
    ) -> Option<StableSceneSnapshot> {
        match Self::stable_scene_snapshot(executor).await {
            Ok(scene) => Some(scene),
            Err(error) => {
                executor.record_runtime_error_at_root(error);
                None
            }
        }
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
            ParameterValue::VectorFloat(values) => {
                Value::List(std::rc::Rc::new(List::new_with(
                    values
                        .iter()
                        .map(|&value| VRc::new(Value::Float(value)))
                        .collect(),
                )))
            }
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
        scene_snapshot: Option<StableSceneSnapshot>,
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
            Some((background, camera, meshes)) => (Some(background), Some(camera), Some(meshes)),
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
