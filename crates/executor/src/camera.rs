use geo::simd::Float3;

use crate::{
    error::ExecutorError,
    executor::Executor,
    heap::with_heap,
    value::{
        Value,
        container::{HashableKey, Map},
    },
};

pub const DEFAULT_CAMERA_FOV: f32 = 0.698_131_7;
pub const DEFAULT_CAMERA_ASPECT: f32 = 16.0 / 9.0;
pub const MIN_CAMERA_NEAR: f32 = 0.01;

#[derive(Clone, Debug, PartialEq)]
pub struct CameraSnapshot {
    pub position: Float3,
    pub forward: Float3,
    pub up: Float3,
    pub near: f32,
    pub far: f32,
}

impl Default for CameraSnapshot {
    fn default() -> Self {
        Self {
            position: Float3::new(0.0, 0.0, -10.0),
            forward: Float3::Z,
            up: Float3::Y,
            near: 0.1,
            far: 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraBasis {
    pub position: Float3,
    pub right: Float3,
    pub up: Float3,
    pub forward: Float3,
    pub near: f32,
    pub far: f32,
    pub fov: f32,
}

impl CameraSnapshot {
    pub fn basis(&self) -> CameraBasis {
        let forward = normalized_or(self.forward, Float3::Z);
        let up_hint = normalized_or(self.up, Float3::Y);

        let mut right = up_hint.cross(forward);
        if right.len_sq() <= 1e-6 {
            let fallback_up = if Float3::Y.cross(forward).len_sq() > 1e-6 {
                Float3::Y
            } else {
                Float3::Z
            };
            right = fallback_up.cross(forward);
        }

        let right = right.normalize();
        let up = forward.cross(right).normalize();
        let near = self.near.max(MIN_CAMERA_NEAR);

        CameraBasis {
            position: self.position,
            right,
            up,
            forward,
            near,
            far: self.far.max(near),
            fov: DEFAULT_CAMERA_FOV,
        }
    }
}

fn normalized_or(value: Float3, fallback: Float3) -> Float3 {
    if value.len_sq() <= 1e-6 {
        fallback
    } else {
        value.normalize()
    }
}

fn map_field_value(map: &Map, name: &str) -> Option<Value> {
    map.get(&HashableKey::String(name.to_string()))
        .map(|value| with_heap(|h| h.get(value.key()).clone()))
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
        return Err(ExecutorError::invalid_scene(format!(
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

pub async fn parse_camera_value(
    executor: &mut Executor,
    value: Value,
    target: &'static str,
) -> Result<CameraSnapshot, ExecutorError> {
    let value = value.elide_wrappers(executor).await?;
    let Value::Map(map) = value else {
        return Err(ExecutorError::type_error_for(
            "camera",
            value.type_name(),
            target,
        ));
    };

    let Some(kind) = map_field_value(&map, "kind") else {
        return Err(ExecutorError::missing_field("camera", "kind"));
    };
    let kind = kind.elide_wrappers(executor).await?;
    if !matches!(kind, Value::String(ref kind) if kind == "camera") {
        return Err(ExecutorError::invalid_scene(format!(
            "camera must resolve to a camera object, got kind {}",
            match kind {
                Value::String(ref kind) => kind.as_str(),
                other => other.type_name(),
            }
        )));
    }

    let Some(position) = map_field_value(&map, "position") else {
        return Err(ExecutorError::missing_field("camera", "position"));
    };
    let position = read_float3(executor, position, "camera.position").await?;
    let forward = if let Some(forward) = map_field_value(&map, "forward") {
        read_float3(executor, forward, "camera.forward").await?
    } else if let Some(look_at) = map_field_value(&map, "look_at") {
        read_float3(executor, look_at, "camera.look_at").await? - position
    } else {
        return Err(ExecutorError::missing_field("camera", "forward"));
    };
    let Some(up) = map_field_value(&map, "up") else {
        return Err(ExecutorError::missing_field("camera", "up"));
    };
    let Some(near) = map_field_value(&map, "near") else {
        return Err(ExecutorError::missing_field("camera", "near"));
    };
    let Some(far) = map_field_value(&map, "far") else {
        return Err(ExecutorError::missing_field("camera", "far"));
    };

    Ok(CameraSnapshot {
        position,
        forward,
        up: read_float3(executor, up, "camera.up").await?,
        near: read_f32(executor, near, "camera.near").await?,
        far: read_f32(executor, far, "camera.far").await?,
    })
}

pub async fn parse_camera_arg(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    target: &'static str,
) -> Result<CameraSnapshot, ExecutorError> {
    let value = executor.state.stack(stack_idx).read_at(index).clone();
    parse_camera_value(executor, value, target).await
}
