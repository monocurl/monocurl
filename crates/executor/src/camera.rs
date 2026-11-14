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
pub const DEFAULT_ORTHOGRAPHIC_HEIGHT: f32 = 8.0;
pub const MIN_CAMERA_NEAR: f32 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    /// vertical field of view in radians
    Perspective { fov: f32 },
    /// world-space height of the visible region, constant with depth
    Orthographic { height: f32 },
}

impl Default for Projection {
    fn default() -> Self {
        Self::Perspective {
            fov: DEFAULT_CAMERA_FOV,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraSnapshot {
    pub position: Float3,
    pub look_at: Float3,
    pub up: Float3,
    pub near: f32,
    pub far: f32,
    pub projection: Projection,
}

pub fn initial_camera_snapshot() -> CameraSnapshot {
    CameraSnapshot {
        position: Float3::new(0.0, 0.0, 4.0),
        look_at: Float3::ZERO,
        up: Float3::Y,
        near: 0.1,
        far: 100.0,
        projection: Projection::default(),
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
            projection: Projection::default(),
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
    pub projection: Projection,
}

impl CameraBasis {
    /// half the vertical extent of the view at the given eye-space depth
    pub fn half_height_at(&self, depth: f32) -> f32 {
        match self.projection {
            Projection::Perspective { fov } => depth * tan_half(fov),
            Projection::Orthographic { height } => height * 0.5,
        }
    }

    pub fn is_orthographic(&self) -> bool {
        matches!(self.projection, Projection::Orthographic { .. })
    }
}

/// tangent of the half angle, clamped away from zero so the projection stays finite
pub fn tan_half(fov: f32) -> f32 {
    (fov * 0.5).tan().max(0.05)
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
            projection: self.projection,
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
            .map(|component| crate::heap::VRc::new(Value::Float(component as f64))),
    ))
}

pub fn camera_value_from_snapshot(snapshot: &CameraSnapshot) -> Value {
    let mut map = Map::new();
    let mut insert = |key: &str, value: Value| {
        map.insert(
            HashableKey::String(key.to_string()),
            crate::heap::VRc::new(value),
        );
    };
    insert("kind", Value::String("camera".into()));
    insert("position", float3_value(snapshot.position));
    insert("look_at", float3_value(snapshot.look_at));
    insert("up", float3_value(snapshot.up));
    insert("near", Value::Float(snapshot.near as f64));
    insert("far", Value::Float(snapshot.far as f64));
    match snapshot.projection {
        Projection::Perspective { fov } => {
            insert("projection", Value::String("perspective".into()));
            insert("fov", Value::Float(fov as f64));
        }
        Projection::Orthographic { height } => {
            insert("projection", Value::String("orthographic".into()));
            insert("height", Value::Float(height as f64));
        }
    }
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

async fn read_projection(executor: &mut Executor, map: &Map) -> Result<Projection, ExecutorError> {
    let kind = match map_field_value(map, "projection") {
        Some(value) => match value.elide_wrappers_rec(executor).await? {
            Value::String(kind) => kind.to_string(),
            other => {
                return Err(ExecutorError::type_error_for(
                    "string",
                    other.type_name(),
                    "camera.projection",
                ));
            }
        },
        None => "perspective".to_string(),
    };

    match kind.as_str() {
        "perspective" => {
            let fov = match map_field_value(map, "fov") {
                Some(value) => read_f32(executor, value, "camera.fov").await?,
                None => DEFAULT_CAMERA_FOV,
            };
            if !(fov > 0.0 && fov < std::f32::consts::PI) {
                return Err(ExecutorError::invalid_scene(format!(
                    "camera.fov must be strictly between 0 and PI radians, got {fov}"
                )));
            }
            Ok(Projection::Perspective { fov })
        }
        "orthographic" => {
            let height = match map_field_value(map, "height") {
                Some(value) => read_f32(executor, value, "camera.height").await?,
                None => DEFAULT_ORTHOGRAPHIC_HEIGHT,
            };
            if !(height > 0.0) {
                return Err(ExecutorError::invalid_scene(format!(
                    "camera.height must be positive, got {height}"
                )));
            }
            Ok(Projection::Orthographic { height })
        }
        other => Err(ExecutorError::invalid_scene(format!(
            "camera.projection must be \"perspective\" or \"orthographic\", got \"{other}\""
        ))),
    }
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
    if !matches!(kind, Value::String(ref kind) if kind.as_ref() == "camera") {
        return Err(ExecutorError::invalid_scene(format!(
            "camera must resolve to a camera object, got kind {}",
            match kind {
                Value::String(ref kind) => kind.as_ref(),
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
    let projection = read_projection(executor, &map).await?;

    Ok(CameraSnapshot {
        position,
        look_at: read_float3(executor, look_at, "camera.look_at").await?,
        up: read_float3(executor, up, "camera.up").await?,
        near: read_f32(executor, near, "camera.near").await?,
        far: read_f32(executor, far, "camera.far").await?,
        projection,
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

    use super::{CameraSnapshot, Projection};

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
            projection: Projection::default(),
        }
        .basis();

        assert!(Float3::X.dot(basis.right) > 0.999);
        assert!(Float3::Y.dot(basis.up) > 0.999);
        assert!(Float3::Z.dot(-basis.forward) > 0.999);
    }

    #[test]
    fn orthographic_half_height_ignores_depth() {
        let basis = CameraSnapshot {
            projection: Projection::Orthographic { height: 6.0 },
            ..CameraSnapshot::default()
        }
        .basis();

        assert_eq!(basis.half_height_at(1.0), 3.0);
        assert_eq!(basis.half_height_at(50.0), 3.0);
        assert!(basis.is_orthographic());
    }

    #[test]
    fn perspective_half_height_grows_with_depth() {
        let basis = CameraSnapshot::default().basis();

        assert!(!basis.is_orthographic());
        assert!(basis.half_height_at(20.0) > basis.half_height_at(10.0));
    }
}
