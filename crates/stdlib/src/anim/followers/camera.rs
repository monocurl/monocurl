use executor::{
    camera::{CameraSnapshot, camera_value_from_snapshot, parse_camera_arg},
    error::ExecutorError,
    executor::Executor,
    value::Value,
};
use geo::simd::Float3;
use stdlib_macros::stdlib_func;

use super::embed_triplet;

const CAMERA_LERP_EPSILON: f32 = 0.00001;

#[stdlib_func]
pub async fn camera_lerp_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = parse_camera_arg(executor, stack_idx, -2, "start").await?;
    let destination = parse_camera_arg(executor, stack_idx, -1, "destination").await?;

    Ok(embed_triplet(
        camera_value_from_snapshot(&camera_lerp_snapshot(&start, &destination, 0.0)),
        camera_value_from_snapshot(&camera_lerp_snapshot(&start, &destination, 1.0)),
        Value::Nil,
    ))
}

#[stdlib_func]
pub async fn camera_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = parse_camera_arg(executor, stack_idx, -3, "start").await?;
    let end = parse_camera_arg(executor, stack_idx, -2, "end").await?;
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;

    Ok(camera_value_from_snapshot(&camera_lerp_snapshot(
        &start, &end, t,
    )))
}

fn camera_lerp_snapshot(start: &CameraSnapshot, end: &CameraSnapshot, t: f32) -> CameraSnapshot {
    let start_basis = start.basis();
    let end_basis = end.basis();

    let position = start.position.lerp(end.position, t);
    let forward = start_basis.forward.lerp(end_basis.forward, t);

    CameraSnapshot {
        position,
        look_at: position + forward,
        up: slerp(start_basis.up, end_basis.up, t),
        near: lerp_f32(start.near, end.near, t),
        far: lerp_f32(start.far, end.far, t),
    }
}

fn slerp(start: Float3, end: Float3, t: f32) -> Float3 {
    let alpha = start.dot(end).clamp(-1.0, 1.0).acos();
    let sin_alpha = alpha.sin();

    if sin_alpha.abs() < CAMERA_LERP_EPSILON {
        return start.lerp(end, t);
    }

    let start_scale = (alpha * (1.0 - t)).sin() / sin_alpha;
    let end_scale = (alpha * t).sin() / sin_alpha;
    start * start_scale + end * end_scale
}

fn lerp_f32(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_float3_close(actual: Float3, expected: Float3) {
        let delta = actual - expected;
        assert!(
            delta.len_sq() <= 1e-8,
            "expected {actual:?} to be close to {expected:?}"
        );
    }

    #[test]
    fn slerp_rotates_between_orthogonal_vectors() {
        assert_float3_close(
            slerp(Float3::X, Float3::Y, 0.5),
            Float3::new(0.5_f32.sqrt(), 0.5_f32.sqrt(), 0.0),
        );
    }

    #[test]
    fn camera_lerp_uses_linear_forward_and_spherical_up() {
        let start = CameraSnapshot {
            position: Float3::ZERO,
            look_at: Float3::Z,
            up: Float3::Y,
            near: 0.1,
            far: 10.0,
        };
        let end = CameraSnapshot {
            position: Float3::new(2.0, 0.0, 0.0),
            look_at: Float3::new(2.0, 1.0, 0.0),
            up: Float3::Z,
            near: 0.5,
            far: 20.0,
        };

        let midpoint = camera_lerp_snapshot(&start, &end, 0.5);

        assert_float3_close(midpoint.position, Float3::new(1.0, 0.0, 0.0));
        assert_float3_close(midpoint.look_at, Float3::new(1.0, 0.5, 0.5));
        assert!((midpoint.near - 0.3).abs() <= 1e-6);
        assert!((midpoint.far - 15.0).abs() <= 1e-6);
    }
}
