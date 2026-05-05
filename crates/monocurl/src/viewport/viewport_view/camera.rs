use std::{
    collections::{HashMap, VecDeque},
    f32::consts::{FRAC_PI_2, PI},
};

use executor::{
    camera::{DEFAULT_CAMERA_FOV, MIN_CAMERA_NEAR},
    scene_snapshot::CameraSnapshot,
};
use geo::simd::Float3;
use gpui::*;

use crate::services::{ParameterValue, PresentationUpdateTarget};

use super::Viewport;

const CAMERA_ORBIT_RADIANS_PER_VIEW: f32 = PI;
const CAMERA_MAX_PITCH: f32 = FRAC_PI_2 - 0.05;
const CAMERA_COMPARE_EPS: f32 = 1e-4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CameraDragMode {
    Orbit,
    Pan,
}

#[derive(Clone)]
pub(super) struct CameraDragState {
    pub mode: CameraDragMode,
    pub start_mouse: Point<Pixels>,
    pub start_camera: CameraSnapshot,
    pub scene_size: Size<Pixels>,
}

#[derive(Clone)]
pub(super) struct PreviewCameraState {
    pub current: CameraSnapshot,
    pub reset_camera: CameraSnapshot,
}

#[derive(Clone)]
pub(super) struct PresentationCameraState {
    pub current: CameraSnapshot,
    pub reset_camera: CameraSnapshot,
    pub pending_updates: VecDeque<CameraSnapshot>,
}

impl Viewport {
    pub(super) fn sync_camera_from_execution(&mut self, cx: &mut Context<Self>) {
        let (scene_camera, scene_camera_version) = {
            let execution = self.execution_state.read(cx);
            (execution.camera.clone(), execution.camera_version)
        };

        if scene_camera_version == self.scene_camera_version {
            return;
        }
        self.scene_camera_version = scene_camera_version;

        if self.is_presenting {
            self.presentation_camera = match self.presentation_camera.take() {
                Some(mut state) if cameras_close(&state.current, &scene_camera) => {
                    state.pending_updates.clear();
                    (!cameras_close(&state.reset_camera, &scene_camera)).then_some(
                        PresentationCameraState {
                            current: scene_camera,
                            reset_camera: state.reset_camera,
                            pending_updates: VecDeque::new(),
                        },
                    )
                }
                Some(mut state) => {
                    if let Some(ack_index) = state
                        .pending_updates
                        .iter()
                        .position(|pending| cameras_close(pending, &scene_camera))
                    {
                        state.pending_updates.drain(..=ack_index);
                        Some(state)
                    } else if cameras_close(&state.reset_camera, &scene_camera) {
                        Some(state)
                    } else {
                        self.camera_drag = None;
                        None
                    }
                }
                None => {
                    self.camera_drag = None;
                    None
                }
            };
        } else {
            self.preview_camera = match self.preview_camera.take() {
                Some(state) if cameras_close(&state.current, &scene_camera) => {
                    self.camera_drag = None;
                    self.copied_preview_camera = None;
                    None
                }
                Some(state) if cameras_close(&state.reset_camera, &scene_camera) => Some(state),
                Some(_) => {
                    self.camera_drag = None;
                    self.copied_preview_camera = None;
                    None
                }
                None => {
                    self.camera_drag = None;
                    None
                }
            };
        }
    }

    pub(super) fn display_camera(&self, scene_camera: &CameraSnapshot) -> CameraSnapshot {
        if self.is_presenting {
            self.presentation_camera
                .as_ref()
                .map(|state| state.current.clone())
                .unwrap_or_else(|| scene_camera.clone())
        } else {
            self.preview_camera
                .as_ref()
                .map(|state| state.current.clone())
                .unwrap_or_else(|| scene_camera.clone())
        }
    }

    pub(super) fn should_show_preview_reset(&self) -> bool {
        !self.is_presenting && self.preview_camera.is_some()
    }

    pub(super) fn preview_camera_summary(&self) -> Option<String> {
        (!self.is_presenting)
            .then_some(self.preview_camera.as_ref())
            .flatten()
            .map(|state| format_camera_surface(&state.current))
    }

    pub(super) fn is_preview_camera_copied(&self, summary: &str) -> bool {
        self.copied_preview_camera.as_deref() == Some(summary)
    }

    pub(super) fn mark_preview_camera_copied(&mut self, summary: String, cx: &mut Context<Self>) {
        self.copied_preview_camera = Some(summary);
        cx.notify();
    }

    pub(super) fn should_show_presentation_reset(&self, scene_camera: &CameraSnapshot) -> bool {
        self.is_presenting
            && self
                .presentation_camera
                .as_ref()
                .is_some_and(|state| !cameras_close(&state.reset_camera, scene_camera))
    }

    pub(super) fn reset_presentation_camera(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.presentation_camera.clone() else {
            return;
        };
        if cameras_close(&state.current, &state.reset_camera) {
            return;
        }

        self.bump_viewport_camera_version();
        self.presentation_camera = Some(PresentationCameraState {
            current: state.reset_camera.clone(),
            reset_camera: state.reset_camera.clone(),
            pending_updates: VecDeque::from([state.reset_camera.clone()]),
        });
        self.update_scene_camera(state.reset_camera, cx);
        cx.notify();
    }

    pub(super) fn begin_camera_drag(
        &mut self,
        mode: CameraDragMode,
        position: Point<Pixels>,
        scene_size: Size<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let scene_camera = self.execution_state.read(cx).camera.clone();
        self.camera_drag = Some(CameraDragState {
            mode,
            start_mouse: position,
            start_camera: self.display_camera(&scene_camera),
            scene_size,
        });
        cx.notify();
    }

    pub(super) fn update_camera_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.camera_drag.clone() else {
            return;
        };

        let dx = f32::from(drag.start_mouse.x - position.x);
        let dy = f32::from(position.y - drag.start_mouse.y);
        let scene_camera = self.execution_state.read(cx).camera.clone();
        let previous_display_camera = self.display_camera(&scene_camera);
        let next_camera = match drag.mode {
            CameraDragMode::Orbit => orbit_camera(&drag.start_camera, dx, dy, drag.scene_size),
            CameraDragMode::Pan => pan_camera(&drag.start_camera, dx, dy, drag.scene_size),
        };

        if self.is_presenting {
            let mut state =
                self.presentation_camera
                    .clone()
                    .unwrap_or_else(|| PresentationCameraState {
                        current: scene_camera.clone(),
                        reset_camera: scene_camera.clone(),
                        pending_updates: VecDeque::new(),
                    });
            state.current = next_camera.clone();
            if state
                .pending_updates
                .back()
                .is_none_or(|pending| !cameras_close(pending, &next_camera))
            {
                state.pending_updates.push_back(next_camera.clone());
            }
            self.presentation_camera = Some(state);
            self.update_scene_camera(next_camera, cx);
        } else if cameras_close(&next_camera, &scene_camera) {
            self.preview_camera = None;
            self.copied_preview_camera = None;
        } else {
            let reset_camera = self
                .preview_camera
                .as_ref()
                .map(|state| state.reset_camera.clone())
                .unwrap_or_else(|| scene_camera.clone());
            if cameras_close(&next_camera, &reset_camera) {
                self.preview_camera = None;
                self.copied_preview_camera = None;
            } else {
                self.preview_camera = Some(PreviewCameraState {
                    current: next_camera,
                    reset_camera,
                });
            }
        }
        let next_display_camera = self.display_camera(&scene_camera);
        if !cameras_close(&previous_display_camera, &next_display_camera) {
            self.bump_viewport_camera_version();
        }
        cx.notify();
    }

    pub(super) fn end_camera_drag(&mut self, cx: &mut Context<Self>) {
        if self.camera_drag.take().is_some() {
            cx.notify();
        }
    }

    fn update_scene_camera(&mut self, camera: CameraSnapshot, cx: &mut Context<Self>) {
        let target = self
            .execution_state
            .read(cx)
            .parameters
            .as_ref()
            .and_then(|params| {
                params
                    .params
                    .iter()
                    .find(|entry| entry.name == "camera")
                    .map(|entry| entry.target.clone())
            })
            .unwrap_or(PresentationUpdateTarget::Scene { leader_index: 0 });
        self.services.update(cx, |services, _| {
            services.update_parameters(HashMap::from([(target, ParameterValue::Camera(camera))]))
        });
    }
}

fn normalized_or(value: Float3, fallback: Float3) -> Float3 {
    if value.len_sq() <= 1e-6 {
        fallback
    } else {
        value.normalize()
    }
}

fn rotate_around_axis(vector: Float3, axis: Float3, angle: f32) -> Float3 {
    let axis = normalized_or(axis, Float3::Y);
    let cos = angle.cos();
    let sin = angle.sin();
    vector * cos + axis.cross(vector) * sin + axis * axis.dot(vector) * (1.0 - cos)
}

fn cameras_close(a: &CameraSnapshot, b: &CameraSnapshot) -> bool {
    (a.position - b.position).len_sq() <= CAMERA_COMPARE_EPS * CAMERA_COMPARE_EPS
        && (a.look_at - b.look_at).len_sq() <= CAMERA_COMPARE_EPS * CAMERA_COMPARE_EPS
        && (a.up - b.up).len_sq() <= CAMERA_COMPARE_EPS * CAMERA_COMPARE_EPS
        && (a.near - b.near).abs() <= CAMERA_COMPARE_EPS
        && (a.far - b.far).abs() <= CAMERA_COMPARE_EPS
}

fn orbit_camera(
    camera: &CameraSnapshot,
    dx: f32,
    dy: f32,
    scene_size: Size<Pixels>,
) -> CameraSnapshot {
    let width = f32::from(scene_size.width).max(1.0);
    let height = f32::from(scene_size.height).max(1.0);
    let yaw = dx / width * CAMERA_ORBIT_RADIANS_PER_VIEW;
    let pitch_delta = dy / height * CAMERA_ORBIT_RADIANS_PER_VIEW;
    let world_up = normalized_or(camera.up, Float3::Y);
    let offset = camera.position - camera.look_at;
    let radius = offset.len().max(MIN_CAMERA_NEAR);
    let horizontal = offset - world_up * offset.dot(world_up);
    let horizontal_dir = if horizontal.len_sq() <= 1e-6 {
        normalized_or(camera.basis().right.cross(world_up), Float3::Z)
    } else {
        horizontal.normalize()
    };
    let current_pitch = offset.dot(world_up).atan2(horizontal.len().max(1e-6));
    let pitch = (current_pitch + pitch_delta).clamp(-CAMERA_MAX_PITCH, CAMERA_MAX_PITCH);
    let horizontal_dir = rotate_around_axis(horizontal_dir, world_up, yaw);
    let next_offset = horizontal_dir * (radius * pitch.cos()) + world_up * (radius * pitch.sin());

    CameraSnapshot {
        position: camera.look_at + next_offset,
        look_at: camera.look_at,
        up: world_up,
        near: camera.near,
        far: camera.far,
    }
}

fn pan_camera(
    camera: &CameraSnapshot,
    dx: f32,
    dy: f32,
    scene_size: Size<Pixels>,
) -> CameraSnapshot {
    let width = f32::from(scene_size.width).max(1.0);
    let height = f32::from(scene_size.height).max(1.0);
    let basis = camera.basis();
    let depth = (camera.look_at - camera.position)
        .dot(basis.forward)
        .max(MIN_CAMERA_NEAR);
    let aspect = (width / height).max(0.1);
    let tan_half_fov = (DEFAULT_CAMERA_FOV * 0.5).tan().max(0.05);
    let half_height = depth * tan_half_fov;
    let half_width = half_height * aspect;
    let translation = basis.right * (2.0 * half_width * dx / width)
        + basis.up * (2.0 * half_height * dy / height);

    CameraSnapshot {
        position: camera.position + translation,
        look_at: camera.look_at + translation,
        up: camera.up,
        near: camera.near,
        far: camera.far,
    }
}

fn format_camera_surface(camera: &CameraSnapshot) -> String {
    format!(
        "Camera({}, {}, {})",
        format_camera_point(camera.position),
        format_camera_point(camera.look_at),
        format_axis_vector(camera.up)
    )
}

fn format_camera_point(point: Float3) -> String {
    format!(
        "[{}, {}, {}]",
        format_point_scalar(point.x),
        format_point_scalar(point.y),
        format_point_scalar(point.z)
    )
}

fn format_point_scalar(value: f32) -> String {
    if value.abs() < 0.005 {
        "0.00".to_string()
    } else {
        format!("{value:.2}")
    }
}

fn format_axis_vector(vector: Float3) -> String {
    let mut result = String::new();

    for (positive_axis, negative_axis, value) in [
        ("r", "l", vector.x),
        ("u", "d", vector.y),
        ("b", "f", vector.z),
    ] {
        if value.abs() <= 1e-4 {
            continue;
        }

        let axis = if value < 0.0 {
            negative_axis
        } else {
            positive_axis
        };
        let magnitude = format_axis_scalar(value.abs());
        if !result.is_empty() {
            result.push_str(" + ");
        }
        result.push_str(&magnitude);
        result.push_str(axis);
    }

    if result.is_empty() {
        "0r".to_string()
    } else {
        result
    }
}

fn format_axis_scalar(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    format!("{rounded:.2}")
}

#[cfg(test)]
mod tests {
    use executor::scene_snapshot::CameraSnapshot;
    use geo::simd::Float3;

    use super::{format_axis_vector, format_camera_surface};

    #[test]
    fn format_axis_vector_uses_right_handed_depth_literals() {
        assert_eq!(format_axis_vector(Float3::new(0.0, 0.0, 4.0)), "4.00b");
        assert_eq!(format_axis_vector(Float3::new(0.0, 0.0, -4.0)), "4.00f");
    }

    #[test]
    fn format_axis_vector_uses_directional_suffixes_for_negative_axes() {
        assert_eq!(
            format_axis_vector(Float3::new(-1.0, -2.0, 3.0)),
            "1.00l + 2.00d + 3.00b"
        );
    }

    #[test]
    fn format_camera_surface_keeps_position_and_look_at_components() {
        let camera = CameraSnapshot {
            position: Float3::new(0.0, -1.234, 4.0),
            look_at: Float3::new(0.004, 2.0, -0.0),
            up: Float3::Y,
            near: 0.1,
            far: 100.0,
        };

        assert_eq!(
            format_camera_surface(&camera),
            "Camera([0.00, -1.23, 4.00], [0.00, 2.00, 0.00], 1.00u)"
        );
    }
}
