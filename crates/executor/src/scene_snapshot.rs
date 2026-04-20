use std::{future::Future, sync::Arc};

use geo::{mesh::Mesh, simd::Float3};

use crate::{
    error::{ExecutorError, RuntimeError},
    executor::Executor,
    heap::with_heap,
    state::LeaderKind,
    value::{
        Value,
        container::{HashableKey, Map},
    },
};

#[derive(Clone, Debug, PartialEq)]
pub struct CameraSnapshot {
    pub position: Float3,
    pub look_at: Float3,
    pub up: Float3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub ortho: bool,
}

impl Default for CameraSnapshot {
    fn default() -> Self {
        Self {
            position: Float3::new(0.0, 0.0, -10.0),
            look_at: Float3::ZERO,
            up: Float3::Y,
            fov: 0.698_131_7,
            near: 0.1,
            far: 100.0,
            ortho: false,
        }
    }
}

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
        *slot = read_f32(
            executor,
            with_heap(|h| h.get(component.key()).clone()),
            target,
        )
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
    let value = value.elide_wrappers(executor).await?;
    let Value::Map(map) = value else {
        return Err(ExecutorError::type_error_for(
            "camera",
            value.type_name(),
            "camera",
        ));
    };

    let Some(kind) = map_field_value(&map, "kind") else {
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

    let Some(position) = map_field_value(&map, "position") else {
        return Err(ExecutorError::Other(
            "camera: missing 'position' field".into(),
        ));
    };
    let Some(look_at) = map_field_value(&map, "look_at") else {
        return Err(ExecutorError::Other(
            "camera: missing 'look_at' field".into(),
        ));
    };
    let Some(up) = map_field_value(&map, "up") else {
        return Err(ExecutorError::Other("camera: missing 'up' field".into()));
    };
    let Some(fov) = map_field_value(&map, "fov") else {
        return Err(ExecutorError::Other("camera: missing 'fov' field".into()));
    };
    let Some(near) = map_field_value(&map, "near") else {
        return Err(ExecutorError::Other("camera: missing 'near' field".into()));
    };
    let Some(far) = map_field_value(&map, "far") else {
        return Err(ExecutorError::Other("camera: missing 'far' field".into()));
    };
    let Some(ortho) = map_field_value(&map, "ortho") else {
        return Err(ExecutorError::Other("camera: missing 'ortho' field".into()));
    };

    Ok(CameraSnapshot {
        position: read_float3(executor, position, "camera.position").await?,
        look_at: read_float3(executor, look_at, "camera.look_at").await?,
        up: read_float3(executor, up, "camera.up").await?,
        fov: read_f32(executor, fov, "camera.fov").await?,
        near: read_f32(executor, near, "camera.near").await?,
        far: read_f32(executor, far, "camera.far").await?,
        ortho: read_bool_flag(executor, ortho, "camera.ortho").await?,
    })
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
            "solid background / rgba 4-vector",
            value.type_name(),
            "background",
        ));
    };

    let Some(kind) = map_field_value(&map, "kind") else {
        return Err(ExecutorError::Other(
            "background: missing 'kind' field".into(),
        ));
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

    let Some(color) = map_field_value(&map, "color") else {
        return Err(ExecutorError::Other(
            "background: missing 'color' field".into(),
        ));
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
