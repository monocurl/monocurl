mod geometry;
mod pipelines;
mod renderer;
mod resources;
mod types;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use blade_graphics as gpu;
use blade_util::BufferBelt;
use geo::mesh::Mesh;

use crate::RenderStyle;

use self::{
    pipelines::Pipelines,
    resources::{
        CachedMesh, CachedTexture, IndexedBuffer, OffscreenTarget, PendingBufferUpload,
        PendingTextureUpload, TextureCacheEntry,
    },
};

const TARGET_FORMAT: gpu::TextureFormat = gpu::TextureFormat::Bgra8Unorm;
const TEXTURE_FORMAT: gpu::TextureFormat = gpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: gpu::TextureFormat = gpu::TextureFormat::Depth32Float;
const MAX_FRAME_TIME_MS: u32 = 10_000;
const DEPTH_STEP: f32 = 1e-6;
const DESIRED_MSAA_SAMPLE_COUNT: u32 = 8;
const DEFAULT_LINE_MITER_SCALE: f32 = 4.0;
const LINE_VERTICES_PER_INSTANCE: u32 = 6;
const REFERENCE_WIDTH: f32 = 1480.0;
const WHITE_TEXTURE: [u8; 4] = [255, 255, 255, 255];

pub(crate) struct BladeRenderer {
    gpu: Arc<gpu::Context>,
    command_encoder: gpu::CommandEncoder,
    pipelines: Pipelines,
    texture_sampler: gpu::Sampler,
    upload_belt: BufferBelt,
    white_texture: CachedTexture,
    line_index_buffer: Option<IndexedBuffer>,
    dot_index_buffers: HashMap<u16, IndexedBuffer>,
    target: Option<OffscreenTarget>,
    mesh_cache: HashMap<usize, CachedMesh>,
    texture_cache: HashMap<PathBuf, TextureCacheEntry>,
    pending_buffer_uploads: Vec<PendingBufferUpload>,
    pending_texture_uploads: Vec<PendingTextureUpload>,
    style: RenderStyle,
    sample_count: u32,
    frame_index: u64,
}

#[derive(Clone)]
struct MeshWorkItem {
    key: usize,
    order: usize,
    mesh: Arc<Mesh>,
    texture_path: Option<PathBuf>,
    z_index: i32,
}
