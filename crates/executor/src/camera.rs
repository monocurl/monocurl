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

pub const DEFAULT_CAMERA_FOV: f32 = 1.024_778_9;
pub const DEFAULT_CAMERA_ASPECT: f32 = 16.0 / 9.0;
pub const MIN_CAMERA_NEAR: f32 = 0.01;

#[derive(Clone, Debug, PartialEq)]
pub struct CameraSnapshot {
    pub position: Float3,
    pub look_at: Float3,
    pub up: Float3,
    pub near: f32,
    pub far: f32,
}

pub fn initial_camera_snapshot() -> CameraSnapshot {
    CameraSnapshot {
        position: Float3::new(0.0, 0.0, 4.0),
        look_at: Float3::ZERO,
        up: Float3::Y,
        near: 0.1,
        far: 100.0,
    }
}

impl Default for CameraSnapshot {
    fn default() -> Self {
        Self {
            position: Float3::new(0.0, 0.0, 10.0),
            look_at: Float3::ZERO,
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
        let forward = normalized_or(self.look_at - self.position, -Float3::Z);
        let up_hint = normalized_or(self.up, Float3::Y);

        let mut right = forward.cross(up_hint);
        if right.len_sq() <= 1e-6 {
            let fallback_up = if forward.cross(Float3::Y).len_sq() > 1e-6 {
                Float3::Y
            } else {
                Float3::Z
            };
            right = forward.cross(fallback_up);
        }

        let right = right.normalize();
        let up = right.cross(forward).normalize();
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

fn float3_value(value: Float3) -> Value {
    Value::List(crate::value::container::List::new_with(
        value
            .to_array()
            .into_iter()
            .map(|component| crate::heap::VRc::new(Value::Float(component as f64)))
            .collect(),
    ))
}

pub fn camera_value_from_snapshot(snapshot: &CameraSnapshot) -> Value {
    let mut map = Map::new();
    map.insert(
        HashableKey::String("kind".to_string()),
        crate::heap::VRc::new(Value::String("camera".to_string())),
    );
    map.insert(
        HashableKey::String("position".to_string()),
        crate::heap::VRc::new(float3_value(snapshot.position)),
    );
    map.insert(
        HashableKey::String("look_at".to_string()),
        crate::heap::VRc::new(float3_value(snapshot.look_at)),
    );
    map.insert(
        HashableKey::String("up".to_string()),
        crate::heap::VRc::new(float3_value(snapshot.up)),
    );
    map.insert(
        HashableKey::String("near".to_string()),
        crate::heap::VRc::new(Value::Float(snapshot.near as f64)),
    );
    map.insert(
        HashableKey::String("far".to_string()),
        crate::heap::VRc::new(Value::Float(snapshot.far as f64)),
    );
    Value::Map(map)
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
    match value.elide_wrappers_rec(executor).await? {
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
    let value = value.elide_wrappers_rec(executor).await?;
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
    let value = value.elide_wrappers_rec(executor).await?;
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
    let kind = kind.elide_wrappers_rec(executor).await?;
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
    let Some(look_at) = map_field_value(&map, "look_at") else {
        return Err(ExecutorError::missing_field("camera", "look_at"));
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
        look_at: read_float3(executor, look_at, "camera.look_at").await?,
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

#[cfg(test)]
mod tests {
    use geo::simd::Float3;

    use super::CameraSnapshot;

    fn assert_float3_close(actual: Float3, expected: Float3) {
        let delta = actual - expected;
        assert!(
            delta.len_sq() <= 1e-10,
            "expected {actual:?} to be close to {expected:?}"
        );
    }

    #[test]
    fn default_camera_basis_is_right_handed() {
        let basis = CameraSnapshot::default().basis();

        assert_float3_close(basis.position, Float3::new(0.0, 0.0, 10.0));
        assert_float3_close(basis.right, Float3::X);
        assert_float3_close(basis.up, Float3::Y);
        assert_float3_close(basis.forward, -Float3::Z);
        assert_float3_close(basis.right.cross(basis.up), -basis.forward);
    }

    #[test]
    fn camera_basis_keeps_screen_axes_unflipped() {
        let basis = CameraSnapshot {
            position: Float3::new(0.0, 0.0, 4.0),
            look_at: Float3::ZERO,
            up: Float3::Y,
            near: 0.1,
            far: 100.0,
        }
        .basis();

        assert!(Float3::X.dot(basis.right) > 0.999);
        assert!(Float3::Y.dot(basis.up) > 0.999);
        assert!(Float3::Z.dot(-basis.forward) > 0.999);
    }
}
