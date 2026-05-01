use blade_graphics as gpu;
use bytemuck::{Pod, Zeroable};
use executor::camera::CameraBasis;
use geo::simd::{Float3, Float4};

use crate::RenderView;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct BackgroundParams {
    pub(super) color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct CameraParams {
    pub(super) position: [f32; 4],
    pub(super) right: [f32; 4],
    pub(super) up: [f32; 4],
    pub(super) forward: [f32; 4],
    pub(super) clip: [f32; 4],
    pub(super) viewport: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct TriShaderParams {
    pub(super) values: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LineShaderParams {
    pub(super) viewport_and_line_width: [f32; 4],
    pub(super) depth_bias: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct DotShaderParams {
    pub(super) viewport_and_radius: [f32; 4],
    pub(super) depth_bias: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct TriVertexPod {
    pub(super) pos: [f32; 4],
    pub(super) norm: [f32; 4],
    pub(super) col: [f32; 4],
    pub(super) uv: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LineVertexPod {
    pub(super) pos: [f32; 4],
    pub(super) col: [f32; 4],
    pub(super) tangent: [f32; 4],
    pub(super) prev_tangent: [f32; 4],
    pub(super) extrude: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct DotInstancePod {
    pub(super) pos: [f32; 4],
    pub(super) col: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct PositionKey([u32; 3]);

impl PositionKey {
    pub(super) fn new(pos: Float3) -> Self {
        Self([pos.x.to_bits(), pos.y.to_bits(), pos.z.to_bits()])
    }
}

#[derive(blade_macros::ShaderData)]
pub(super) struct BackgroundData {
    pub(super) background: BackgroundParams,
}

#[derive(blade_macros::ShaderData)]
pub(super) struct TrianglesData {
    pub(super) tri_camera: CameraParams,
    pub(super) tri_params: TriShaderParams,
    pub(super) t_color: gpu::TextureView,
    pub(super) s_color: gpu::Sampler,
    pub(super) tri_vertices: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
pub(super) struct LinesData {
    pub(super) line_camera: CameraParams,
    pub(super) line_params: LineShaderParams,
    pub(super) line_vertices: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
pub(super) struct DotsData {
    pub(super) dot_camera: CameraParams,
    pub(super) dot_params: DotShaderParams,
    pub(super) dot_instances: gpu::BufferPiece,
}

impl CameraParams {
    pub(super) fn from_basis(basis: CameraBasis, view: RenderView) -> Self {
        let output_size = view.output_size;
        let projection_size = view.projection_size;
        let aspect = projection_size.width.max(1) as f32 / projection_size.height.max(1) as f32;
        let viewport_scale_x =
            projection_size.width.max(1) as f32 / output_size.width.max(1) as f32;
        let viewport_scale_y =
            projection_size.height.max(1) as f32 / output_size.height.max(1) as f32;
        Self {
            position: float4_from_xyz(basis.position.x, basis.position.y, basis.position.z, 0.0),
            right: float4_from_xyz(basis.right.x, basis.right.y, basis.right.z, 0.0),
            up: float4_from_xyz(basis.up.x, basis.up.y, basis.up.z, 0.0),
            forward: float4_from_xyz(basis.forward.x, basis.forward.y, basis.forward.z, 0.0),
            clip: [
                basis.near,
                basis.far,
                (basis.fov * 0.5).tan().max(0.05),
                aspect.max(0.1),
            ],
            viewport: [viewport_scale_x, viewport_scale_y, 0.0, 0.0],
        }
    }
}

pub(super) fn float4_from_xyz(x: f32, y: f32, z: f32, w: f32) -> [f32; 4] {
    [x, y, z, w]
}

pub(super) fn float4_from_float4(value: Float4) -> [f32; 4] {
    [value.x, value.y, value.z, value.w]
}
