use std::{future::Future, sync::Arc};

use geo::mesh::Mesh;

use crate::{
    camera::parse_camera_value,
    error::{ExecutorError, RuntimeError},
    executor::Executor,
    heap::with_heap,
    state::LeaderKind,
    value::{
        Value,
        container::{HashableKey, Map},
    },
};

pub use crate::camera::CameraSnapshot;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackgroundSnapshot {
    pub color: (f32, f32, f32, f32),
}

impl Default for BackgroundSnapshot {
    fn default() -> Self {
        Self {
            color: (0.0, 0.0, 0.0, 1.0),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SceneSnapshot {
    pub background: BackgroundSnapshot,
    pub camera: CameraSnapshot,
    pub meshes: Vec<Arc<Mesh>>,
}

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
                    collect_scene_meshes(executor, item, target_name, out).await?;
                }
                Ok(())
            }
            Value::Stateful(ref s) => {
                let resolved = executor.eval_stateful(s).await?;
                collect_scene_meshes(executor, resolved, target_name, out).await
            }
            other => Err(ExecutorError::invalid_scene(format!(
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
        collect_scene_meshes(executor, follower, &name, &mut meshes).await?;
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
        return Err(ExecutorError::invalid_scene(format!(
            "{}: expected list of length 4, got list of length {}",
            target,
            list.len()
        )));
    }

    let mut components = [0.0; 4];
    for (slot, component) in components.iter_mut().zip(list.elements()) {
        *slot = read_f32(
            executor,
            with_heap(|h| h.get(component.key()).clone()),
            target,
        )
        .await?;
    }
    Ok((components[0], components[1], components[2], components[3]))
}

async fn camera_snapshot_from_value(
    executor: &mut Executor,
    value: Value,
) -> Result<CameraSnapshot, ExecutorError> {
    parse_camera_value(executor, value, "camera").await
}

async fn camera_snapshot(executor: &mut Executor) -> Result<CameraSnapshot, ExecutorError> {
    match scene_field_value(executor, "camera").await? {
        Some(value) => camera_snapshot_from_value(executor, value).await,
        None => Ok(CameraSnapshot::default()),
    }
}

async fn background_snapshot_from_value(
    executor: &mut Executor,
    value: Value,
) -> Result<BackgroundSnapshot, ExecutorError> {
    let value = value.elide_wrappers(executor).await?;
    if matches!(value, Value::List(_)) {
        return Ok(BackgroundSnapshot {
            color: read_float4(executor, value, "background").await?,
        });
    }

    let Value::Map(map) = value else {
        return Err(ExecutorError::type_error_for(
            "solid background / list of length 4",
            value.type_name(),
            "background",
        ));
    };

    let Some(kind) = map_field_value(&map, "kind") else {
        return Err(ExecutorError::missing_field("background", "kind"));
    };
    let kind = kind.elide_wrappers(executor).await?;
    if !matches!(kind, Value::String(ref kind) if kind == "solid_background") {
        return Err(ExecutorError::invalid_scene(format!(
            "background must resolve to a solid background, got kind {}",
            match kind {
                Value::String(ref kind) => kind.as_str(),
                other => other.type_name(),
            }
        )));
    }

    let Some(color) = map_field_value(&map, "color") else {
        return Err(ExecutorError::missing_field("background", "color"));
    };

    Ok(BackgroundSnapshot {
        color: read_float4(executor, color, "background.color").await?,
    })
}

async fn background_snapshot(executor: &mut Executor) -> Result<BackgroundSnapshot, ExecutorError> {
    match scene_field_value(executor, "background").await? {
        Some(value) => background_snapshot_from_value(executor, value).await,
        None => Ok(BackgroundSnapshot::default()),
    }
}

impl Executor {
    pub async fn stable_scene_snapshot(&mut self) -> Result<SceneSnapshot, ExecutorError> {
        Ok(SceneSnapshot {
            meshes: scene_meshes(self).await?,
            background: background_snapshot(self).await?,
            camera: camera_snapshot(self).await?,
        })
    }

    pub async fn capture_stable_scene_snapshot(&mut self) -> Result<SceneSnapshot, RuntimeError> {
        match self.stable_scene_snapshot().await {
            Ok(scene) => Ok(scene),
            Err(error) => Err(self.record_runtime_error_at_root(error)),
        }
    }
}
