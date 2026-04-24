use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result, anyhow, ensure};
use blade_graphics as gpu;
use blade_util::{BufferBelt, BufferBeltDescriptor};
use bytemuck::{Pod, Zeroable};
use executor::camera::CameraBasis;
use geo::{
    mesh::Mesh,
    simd::{Float3, Float4},
};
use image::RgbaImage;

use crate::{RenderSize, RenderStyle, RenderView, SceneRenderData, mesh_fingerprint};

const TARGET_FORMAT: gpu::TextureFormat = gpu::TextureFormat::Bgra8Unorm;
const TEXTURE_FORMAT: gpu::TextureFormat = gpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: gpu::TextureFormat = gpu::TextureFormat::Depth32Float;
const MAX_FRAME_TIME_MS: u32 = 10_000;
const DEPTH_STEP: f32 = 1e-6;
const DESIRED_MSAA_SAMPLE_COUNT: u32 = 4;
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

struct Pipelines {
    background: gpu::RenderPipeline,
    triangles: gpu::RenderPipeline,
    lines: gpu::RenderPipeline,
    dots: gpu::RenderPipeline,
}

struct OffscreenTarget {
    size: RenderSize,
    color: gpu::Texture,
    color_view: gpu::TextureView,
    color_msaa: Option<gpu::Texture>,
    color_msaa_view: Option<gpu::TextureView>,
    depth: gpu::Texture,
    depth_view: gpu::TextureView,
    readback: gpu::Buffer,
    needs_init: bool,
}

struct CachedMesh {
    fingerprint: u64,
    triangles: Option<BufferWithCount>,
    lines: Option<BufferWithCount>,
    dots: Option<BufferWithCount>,
    last_used_frame: u64,
}

struct BufferWithCount {
    buffer: gpu::Buffer,
    count: u32,
}

struct IndexedBuffer {
    buffer: gpu::Buffer,
    count: u32,
}

struct CachedTexture {
    texture: gpu::Texture,
    view: gpu::TextureView,
}

struct TextureCacheEntry {
    texture: Option<CachedTexture>,
    last_used_frame: u64,
}

struct PendingBufferUpload {
    src: gpu::BufferPiece,
    dst: gpu::Buffer,
    size: u64,
}

struct PendingTextureUpload {
    src: gpu::BufferPiece,
    dst: gpu::Texture,
    bytes_per_row: u32,
    size: gpu::Extent,
}

#[derive(Clone)]
struct MeshWorkItem {
    key: usize,
    order: usize,
    mesh: Arc<Mesh>,
    texture_path: Option<PathBuf>,
    z_index: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BackgroundParams {
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraParams {
    position: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    forward: [f32; 4],
    clip: [f32; 4],
    viewport: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TriShaderParams {
    values: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineShaderParams {
    viewport_and_line_width: [f32; 4],
    depth_bias: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DotShaderParams {
    viewport_and_radius: [f32; 4],
    depth_bias: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TriVertexPod {
    pos: [f32; 4],
    norm: [f32; 4],
    col: [f32; 4],
    uv: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineVertexPod {
    pos: [f32; 4],
    col: [f32; 4],
    tangent: [f32; 4],
    prev_tangent: [f32; 4],
    extrude: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DotInstancePod {
    pos: [f32; 4],
    col: [f32; 4],
}

#[derive(blade_macros::ShaderData)]
struct BackgroundData {
    background: BackgroundParams,
}

#[derive(blade_macros::ShaderData)]
struct TrianglesData {
    tri_camera: CameraParams,
    tri_params: TriShaderParams,
    t_color: gpu::TextureView,
    s_color: gpu::Sampler,
    tri_vertices: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct LinesData {
    line_camera: CameraParams,
    line_params: LineShaderParams,
    line_vertices: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct DotsData {
    dot_camera: CameraParams,
    dot_params: DotShaderParams,
    dot_instances: gpu::BufferPiece,
}

impl BladeRenderer {
    pub(crate) fn new(style: RenderStyle) -> Result<Self> {
        let gpu = Arc::new(
            unsafe {
                gpu::Context::init(gpu::ContextDesc {
                    presentation: false,
                    validation: false,
                    ..Default::default()
                })
            }
            .map_err(|error| anyhow!("{error:?}"))?,
        );
        let sample_count = choose_sample_count(&gpu);
        let pipelines = Pipelines::new(&gpu, sample_count);
        let command_encoder = gpu.create_command_encoder(gpu::CommandEncoderDesc {
            name: "renderer-offscreen",
            buffer_count: 2,
        });
        let texture_sampler = gpu.create_sampler(gpu::SamplerDesc {
            name: "renderer-linear",
            mag_filter: gpu::FilterMode::Linear,
            min_filter: gpu::FilterMode::Linear,
            mipmap_filter: gpu::FilterMode::Linear,
            ..Default::default()
        });
        let white_texture = create_sampled_texture(&gpu, "renderer-white", 1, 1, TEXTURE_FORMAT);

        let mut renderer = Self {
            gpu,
            command_encoder,
            pipelines,
            texture_sampler,
            upload_belt: BufferBelt::new(BufferBeltDescriptor {
                memory: gpu::Memory::Upload,
                min_chunk_size: 0x10000,
                alignment: 64,
            }),
            white_texture,
            line_index_buffer: None,
            dot_index_buffers: HashMap::new(),
            target: None,
            mesh_cache: HashMap::new(),
            texture_cache: HashMap::new(),
            pending_buffer_uploads: Vec::new(),
            pending_texture_uploads: Vec::new(),
            style,
            sample_count,
            frame_index: 0,
        };
        renderer.line_index_buffer = renderer
            .create_buffer_with_upload("renderer-line-indices", &build_line_indices())
            .map(|buffer| IndexedBuffer {
                buffer: buffer.buffer,
                count: buffer.count,
            })
            .ok_or_else(|| anyhow!("failed to create line index buffer"))?
            .into();
        renderer.initialize_white_texture()?;
        Ok(renderer)
    }

    pub(crate) fn render(
        &mut self,
        scene: &SceneRenderData,
        view: RenderView,
    ) -> Result<RgbaImage> {
        self.frame_index += 1;
        let frame_index = self.frame_index;

        self.command_encoder.start();
        self.ensure_target(view.output_size);

        let mut items = Vec::with_capacity(scene.meshes.len());
        for (order, mesh) in scene.meshes.iter().enumerate() {
            if mesh.uniform.alpha <= 0.0 {
                continue;
            }

            let key = self.ensure_mesh(mesh, frame_index);
            if let Some(path) = mesh.uniform.img.as_deref() {
                self.ensure_texture(path, frame_index);
            }

            items.push(MeshWorkItem {
                key,
                order,
                mesh: Arc::clone(mesh),
                texture_path: mesh.uniform.img.clone(),
                z_index: mesh.uniform.z_index,
            });
        }
        items.sort_by_key(|item| (item.z_index, item.order));

        self.flush_pending_uploads();
        self.draw_meshes(
            &items,
            view,
            scene.camera.basis(),
            Some(scene.background.color),
        );
        self.copy_target_to_readback(view.output_size);

        let sync_point = self.gpu.submit(&mut self.command_encoder);
        self.upload_belt.flush(&sync_point);
        ensure!(
            self.gpu.wait_for(&sync_point, MAX_FRAME_TIME_MS),
            "timed out waiting for renderer GPU work"
        );

        let image = self.readback_image(view.output_size)?;
        self.prune_caches(frame_index);
        Ok(image)
    }

    fn initialize_white_texture(&mut self) -> Result<()> {
        self.command_encoder.start();
        self.command_encoder
            .init_texture(self.white_texture.texture);
        let upload = self.upload_belt.alloc_bytes(&WHITE_TEXTURE, &self.gpu);
        {
            let mut transfers = self.command_encoder.transfer("renderer-white");
            transfers.copy_buffer_to_texture(
                upload,
                4,
                self.white_texture.texture.into(),
                gpu::Extent {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
            );
        }
        let sync_point = self.gpu.submit(&mut self.command_encoder);
        self.upload_belt.flush(&sync_point);
        ensure!(
            self.gpu.wait_for(&sync_point, MAX_FRAME_TIME_MS),
            "timed out initializing renderer white texture"
        );
        Ok(())
    }

    fn ensure_target(&mut self, size: RenderSize) {
        let needs_resize = self
            .target
            .as_ref()
            .is_none_or(|target| target.size != size);
        if !needs_resize {
            if let Some(target) = &mut self.target {
                if target.needs_init {
                    self.command_encoder.init_texture(target.color);
                    if let Some(color_msaa) = target.color_msaa {
                        self.command_encoder.init_texture(color_msaa);
                    }
                    self.command_encoder.init_texture(target.depth);
                    target.needs_init = false;
                }
            }
            return;
        }

        if let Some(target) = self.target.take() {
            destroy_offscreen_target(&self.gpu, target);
        }

        let color = self.gpu.create_texture(gpu::TextureDesc {
            name: "renderer-color",
            format: TARGET_FORMAT,
            size: extent(size),
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: gpu::TextureDimension::D2,
            usage: gpu::TextureUsage::COPY | gpu::TextureUsage::TARGET,
            external: None,
        });
        let color_view = self.gpu.create_texture_view(
            color,
            gpu::TextureViewDesc {
                name: "renderer-color-view",
                format: TARGET_FORMAT,
                dimension: gpu::ViewDimension::D2,
                subresources: &Default::default(),
            },
        );

        let (color_msaa, color_msaa_view) = if self.sample_count > 1 {
            let texture = self.gpu.create_texture(gpu::TextureDesc {
                name: "renderer-color-msaa",
                format: TARGET_FORMAT,
                size: extent(size),
                array_layer_count: 1,
                mip_level_count: 1,
                sample_count: self.sample_count,
                dimension: gpu::TextureDimension::D2,
                usage: gpu::TextureUsage::TARGET,
                external: None,
            });
            let view = self.gpu.create_texture_view(
                texture,
                gpu::TextureViewDesc {
                    name: "renderer-color-msaa-view",
                    format: TARGET_FORMAT,
                    dimension: gpu::ViewDimension::D2,
                    subresources: &Default::default(),
                },
            );
            (Some(texture), Some(view))
        } else {
            (None, None)
        };

        let depth = self.gpu.create_texture(gpu::TextureDesc {
            name: "renderer-depth",
            format: DEPTH_FORMAT,
            size: extent(size),
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: self.sample_count,
            dimension: gpu::TextureDimension::D2,
            usage: gpu::TextureUsage::TARGET,
            external: None,
        });
        let depth_view = self.gpu.create_texture_view(
            depth,
            gpu::TextureViewDesc {
                name: "renderer-depth-view",
                format: DEPTH_FORMAT,
                dimension: gpu::ViewDimension::D2,
                subresources: &Default::default(),
            },
        );

        let readback = self.gpu.create_buffer(gpu::BufferDesc {
            name: "renderer-readback",
            size: size.width as u64 * size.height as u64 * 4,
            memory: gpu::Memory::Shared,
        });

        self.command_encoder.init_texture(color);
        if let Some(color_msaa) = color_msaa {
            self.command_encoder.init_texture(color_msaa);
        }
        self.command_encoder.init_texture(depth);
        self.target = Some(OffscreenTarget {
            size,
            color,
            color_view,
            color_msaa,
            color_msaa_view,
            depth,
            depth_view,
            readback,
            needs_init: false,
        });
    }

    fn ensure_mesh(&mut self, mesh: &Arc<Mesh>, frame_index: u64) -> usize {
        let key = Arc::as_ptr(mesh) as usize;
        let fingerprint = mesh_fingerprint(mesh);
        if let Some(cached) = self.mesh_cache.get_mut(&key) {
            if cached.fingerprint == fingerprint {
                cached.last_used_frame = frame_index;
                return key;
            }
        }
        if let Some(stale) = self.mesh_cache.remove(&key) {
            stale.destroy(&self.gpu);
        }

        let triangles =
            self.create_buffer_with_upload("renderer-triangles", &build_triangle_vertices(mesh));
        let lines = self.create_buffer_with_upload("renderer-lines", &build_line_vertices(mesh));
        let dots = self.create_buffer_with_upload("renderer-dots", &build_dot_instances(mesh));

        self.mesh_cache.insert(
            key,
            CachedMesh {
                fingerprint,
                triangles,
                lines,
                dots,
                last_used_frame: frame_index,
            },
        );
        key
    }

    fn ensure_texture(&mut self, path: &Path, frame_index: u64) {
        if let Some(entry) = self.texture_cache.get_mut(path) {
            entry.last_used_frame = frame_index;
            return;
        }

        let texture = match load_texture(path) {
            Ok(image) => self.create_texture_with_upload(path, &image),
            Err(error) => {
                log::warn!(
                    "failed to load renderer texture {}: {error:#}",
                    path.display()
                );
                None
            }
        };

        self.texture_cache.insert(
            path.to_path_buf(),
            TextureCacheEntry {
                texture,
                last_used_frame: frame_index,
            },
        );
    }

    fn create_buffer_with_upload<T: Pod>(
        &mut self,
        name: &'static str,
        data: &[T],
    ) -> Option<BufferWithCount> {
        if data.is_empty() {
            return None;
        }

        let size = std::mem::size_of_val(data) as u64;
        let buffer = self.gpu.create_buffer(gpu::BufferDesc {
            name,
            size,
            memory: gpu::Memory::Device,
        });
        let src = self.upload_belt.alloc_pod(data, &self.gpu);
        self.pending_buffer_uploads.push(PendingBufferUpload {
            src,
            dst: buffer,
            size,
        });

        Some(BufferWithCount {
            buffer,
            count: data.len() as u32,
        })
    }

    fn ensure_dot_index_buffer(&mut self, vertex_count: u16) -> Option<()> {
        let vertex_count = vertex_count.max(3);
        if self.dot_index_buffers.contains_key(&vertex_count) {
            return Some(());
        }

        let indices = build_dot_indices(vertex_count);
        let buffer = self.create_buffer_with_upload("renderer-dot-indices", &indices)?;
        self.dot_index_buffers.insert(
            vertex_count,
            IndexedBuffer {
                buffer: buffer.buffer,
                count: buffer.count,
            },
        );
        Some(())
    }

    fn create_texture_with_upload(
        &mut self,
        path: &Path,
        image: &RgbaImage,
    ) -> Option<CachedTexture> {
        if image.width() == 0 || image.height() == 0 {
            return None;
        }

        let texture = create_sampled_texture(
            &self.gpu,
            path.to_string_lossy().as_ref(),
            image.width(),
            image.height(),
            TEXTURE_FORMAT,
        );
        self.command_encoder.init_texture(texture.texture);

        let bytes_per_row = image.width() * 4;
        let src = self.upload_belt.alloc_bytes(image.as_raw(), &self.gpu);
        self.pending_texture_uploads.push(PendingTextureUpload {
            src,
            dst: texture.texture,
            bytes_per_row,
            size: gpu::Extent {
                width: image.width(),
                height: image.height(),
                depth: 1,
            },
        });

        Some(texture)
    }

    fn flush_pending_uploads(&mut self) {
        if self.pending_buffer_uploads.is_empty() && self.pending_texture_uploads.is_empty() {
            return;
        }

        let mut transfers = self.command_encoder.transfer("renderer-uploads");
        for upload in self.pending_buffer_uploads.drain(..) {
            transfers.copy_buffer_to_buffer(upload.src, upload.dst.into(), upload.size);
        }
        for upload in self.pending_texture_uploads.drain(..) {
            transfers.copy_buffer_to_texture(
                upload.src,
                upload.bytes_per_row,
                upload.dst.into(),
                upload.size,
            );
        }
    }

    fn draw_meshes(
        &mut self,
        items: &[MeshWorkItem],
        view: RenderView,
        basis: CameraBasis,
        background: Option<(f32, f32, f32, f32)>,
    ) {
        if items.is_empty() && background.is_none() {
            return;
        }

        for item in items {
            let dot_radius = mesh_dot_radius_px(item.mesh.as_ref(), self.style);
            let dot_vertex_count = item.mesh.uniform.dot_vertex_count.max(3);
            if dot_radius > f32::EPSILON {
                let _ = self.ensure_dot_index_buffer(dot_vertex_count);
            }
        }

        let target = self.target.as_ref().expect("target should exist");
        let camera = CameraParams::from_basis(basis, view);
        let size = view.output_size;
        let mut z_offset = 0.0;
        let color_target = match target.color_msaa_view {
            Some(msaa_view) => gpu::RenderTarget {
                view: msaa_view,
                init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                finish_op: gpu::FinishOp::ResolveTo(target.color_view),
            },
            None => gpu::RenderTarget {
                view: target.color_view,
                init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                finish_op: gpu::FinishOp::Store,
            },
        };
        let mut pass = self.command_encoder.render(
            "renderer-scene",
            gpu::RenderTargetSet {
                colors: &[color_target],
                depth_stencil: Some(gpu::RenderTarget {
                    view: target.depth_view,
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::White),
                    finish_op: gpu::FinishOp::Discard,
                }),
            },
        );

        if let Some(color) = background {
            let mut encoder = pass.with(&self.pipelines.background);
            encoder.bind(
                0,
                &BackgroundData {
                    background: BackgroundParams {
                        color: [color.0, color.1, color.2, color.3],
                    },
                },
            );
            encoder.draw(0, 4, 0, 1);
        }

        for item in items {
            let Some(buffers) = self.mesh_cache.get(&item.key) else {
                continue;
            };

            if let Some(triangles) = buffers.triangles.as_ref() {
                let texture_view = item
                    .texture_path
                    .as_ref()
                    .and_then(|path| {
                        self.texture_cache
                            .get(path)
                            .and_then(|entry| entry.texture.as_ref())
                    })
                    .map_or(self.white_texture.view, |texture| texture.view);

                let mut encoder = pass.with(&self.pipelines.triangles);
                encoder.bind(
                    0,
                    &TrianglesData {
                        tri_camera: camera,
                        tri_params: TriShaderParams {
                            values: [
                                item.mesh.uniform.alpha as f32,
                                z_offset,
                                item.mesh.uniform.gloss,
                                if item.mesh.uniform.smooth { 1.0 } else { 0.0 },
                            ],
                        },
                        t_color: texture_view,
                        s_color: self.texture_sampler,
                        tri_vertices: triangles.buffer.into(),
                    },
                );
                encoder.draw(0, triangles.count, 0, 1);
                z_offset += DEPTH_STEP;
            }

            if let Some(lines) = buffers.lines.as_ref() {
                let line_radius = mesh_line_radius_px(item.mesh.as_ref(), size, self.style);
                if line_radius > f32::EPSILON {
                    let Some(index_buffer) = self.line_index_buffer.as_ref() else {
                        continue;
                    };
                    let mut encoder = pass.with(&self.pipelines.lines);
                    encoder.bind(
                        0,
                        &LinesData {
                            line_camera: camera,
                            line_params: LineShaderParams {
                                viewport_and_line_width: [
                                    size.width as f32,
                                    size.height as f32,
                                    line_radius,
                                    item.mesh.uniform.alpha as f32,
                                ],
                                depth_bias: [
                                    z_offset,
                                    mesh_line_miter_scale(item.mesh.as_ref()),
                                    0.0,
                                    0.0,
                                ],
                            },
                            line_vertices: lines.buffer.into(),
                        },
                    );
                    encoder.draw_indexed(
                        index_buffer.buffer.into(),
                        gpu::IndexType::U16,
                        index_buffer.count,
                        0,
                        0,
                        lines.count / LINE_VERTICES_PER_INSTANCE,
                    );
                    z_offset += DEPTH_STEP;
                }
            }

            if let Some(dots) = buffers.dots.as_ref() {
                let dot_radius = mesh_dot_radius_px(item.mesh.as_ref(), self.style);
                let dot_vertex_count = item.mesh.uniform.dot_vertex_count.max(3);
                if dot_radius > f32::EPSILON {
                    if let Some(index_buffer) = self.dot_index_buffers.get(&dot_vertex_count) {
                        let mut encoder = pass.with(&self.pipelines.dots);
                        encoder.bind(
                            0,
                            &DotsData {
                                dot_camera: camera,
                                dot_params: DotShaderParams {
                                    viewport_and_radius: [
                                        size.width as f32,
                                        size.height as f32,
                                        dot_radius,
                                        item.mesh.uniform.alpha as f32,
                                    ],
                                    depth_bias: [z_offset, dot_vertex_count as f32, 0.0, 0.0],
                                },
                                dot_instances: dots.buffer.into(),
                            },
                        );
                        encoder.draw_indexed(
                            index_buffer.buffer.into(),
                            gpu::IndexType::U16,
                            index_buffer.count,
                            0,
                            0,
                            dots.count,
                        );
                        z_offset += DEPTH_STEP;
                    }
                }
            }
        }
    }

    fn copy_target_to_readback(&mut self, size: RenderSize) {
        let target = self.target.as_ref().expect("target should exist");
        let mut transfers = self.command_encoder.transfer("renderer-readback");
        transfers.copy_texture_to_buffer(
            target.color.into(),
            target.readback.into(),
            size.width * 4,
            extent(size),
        );
    }

    fn readback_image(&self, size: RenderSize) -> Result<RgbaImage> {
        let target = self.target.as_ref().expect("target should exist");
        self.gpu.sync_buffer(target.readback);

        let byte_len = size.width as usize * size.height as usize * 4;
        let bytes =
            unsafe { std::slice::from_raw_parts(target.readback.data(), byte_len) }.to_vec();
        RgbaImage::from_raw(size.width, size.height, bytes)
            .ok_or_else(|| anyhow!("renderer readback dimensions did not match buffer size"))
    }

    fn prune_caches(&mut self, frame_index: u64) {
        let stale_meshes = self
            .mesh_cache
            .iter()
            .filter_map(|(&key, mesh)| (mesh.last_used_frame != frame_index).then_some(key))
            .collect::<Vec<_>>();
        for key in stale_meshes {
            if let Some(mesh) = self.mesh_cache.remove(&key) {
                mesh.destroy(&self.gpu);
            }
        }

        let stale_textures = self
            .texture_cache
            .iter()
            .filter_map(|(path, texture)| {
                (texture.last_used_frame != frame_index).then_some(path.clone())
            })
            .collect::<Vec<_>>();
        for path in stale_textures {
            if let Some(entry) = self.texture_cache.remove(&path) {
                if let Some(texture) = entry.texture {
                    destroy_texture(&self.gpu, texture);
                }
            }
        }
    }

    fn destroy(&mut self) {
        for (_, mesh) in self.mesh_cache.drain() {
            mesh.destroy(&self.gpu);
        }
        for (_, entry) in self.texture_cache.drain() {
            if let Some(texture) = entry.texture {
                destroy_texture(&self.gpu, texture);
            }
        }
        if let Some(target) = self.target.take() {
            destroy_offscreen_target(&self.gpu, target);
        }
        if let Some(index_buffer) = self.line_index_buffer.take() {
            self.gpu.destroy_buffer(index_buffer.buffer);
        }
        for (_, index_buffer) in self.dot_index_buffers.drain() {
            self.gpu.destroy_buffer(index_buffer.buffer);
        }
        destroy_texture(
            &self.gpu,
            CachedTexture {
                texture: self.white_texture.texture,
                view: self.white_texture.view,
            },
        );
        self.upload_belt.destroy(&self.gpu);
        self.gpu.destroy_sampler(self.texture_sampler);
        self.pipelines.destroy(&self.gpu);
        self.gpu.destroy_command_encoder(&mut self.command_encoder);
    }
}

impl Drop for BladeRenderer {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl Pipelines {
    fn new(gpu: &gpu::Context, sample_count: u32) -> Self {
        use gpu::ShaderData as _;

        let shader = gpu.create_shader(gpu::ShaderDesc {
            source: include_str!("blade.wgsl"),
        });
        shader.check_struct_size::<BackgroundParams>();
        shader.check_struct_size::<CameraParams>();
        shader.check_struct_size::<TriShaderParams>();
        shader.check_struct_size::<LineShaderParams>();
        shader.check_struct_size::<DotShaderParams>();
        shader.check_struct_size::<TriVertexPod>();
        shader.check_struct_size::<LineVertexPod>();
        shader.check_struct_size::<DotInstancePod>();

        let alpha_target = [gpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend: Some(gpu::BlendState::ALPHA_BLENDING),
            write_mask: gpu::ColorWrites::default(),
        }];
        let replace_target = [gpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend: None,
            write_mask: gpu::ColorWrites::default(),
        }];
        let depth_stencil = gpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: gpu::CompareFunction::LessEqual,
            stencil: Default::default(),
            bias: Default::default(),
        };

        Self {
            background: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-background",
                data_layouts: &[&BackgroundData::layout()],
                vertex: shader.at("vs_background"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_background")),
                color_targets: &replace_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            triangles: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-triangles",
                data_layouts: &[&TrianglesData::layout()],
                vertex: shader.at("vs_triangle"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    front_face: gpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil.clone()),
                fragment: Some(shader.at("fs_triangle")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            lines: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-lines",
                data_layouts: &[&LinesData::layout()],
                vertex: shader.at("vs_line"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil.clone()),
                fragment: Some(shader.at("fs_line")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
            dots: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "renderer-dots",
                data_layouts: &[&DotsData::layout()],
                vertex: shader.at("vs_dot"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil),
                fragment: Some(shader.at("fs_dot")),
                color_targets: &alpha_target,
                multisample_state: gpu::MultisampleState {
                    sample_count,
                    ..Default::default()
                },
            }),
        }
    }

    fn destroy(&mut self, gpu: &gpu::Context) {
        gpu.destroy_render_pipeline(&mut self.background);
        gpu.destroy_render_pipeline(&mut self.triangles);
        gpu.destroy_render_pipeline(&mut self.lines);
        gpu.destroy_render_pipeline(&mut self.dots);
    }
}

impl CachedMesh {
    fn destroy(self, gpu: &gpu::Context) {
        if let Some(buffer) = self.triangles {
            gpu.destroy_buffer(buffer.buffer);
        }
        if let Some(buffer) = self.lines {
            gpu.destroy_buffer(buffer.buffer);
        }
        if let Some(buffer) = self.dots {
            gpu.destroy_buffer(buffer.buffer);
        }
    }
}

impl CameraParams {
    fn from_basis(basis: CameraBasis, view: RenderView) -> Self {
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

fn build_triangle_vertices(mesh: &Mesh) -> Vec<TriVertexPod> {
    let mut vertices = Vec::with_capacity(mesh.tris.len() * 3);
    for tri in &mesh.tris {
        if tri.a.col.w <= f32::EPSILON && tri.b.col.w <= f32::EPSILON && tri.c.col.w <= f32::EPSILON
        {
            continue;
        }
        let normal = triangle_face_normal(tri.a.pos, tri.b.pos, tri.c.pos);
        vertices.push(tri_vertex(tri.a, normal));
        vertices.push(tri_vertex(tri.b, normal));
        vertices.push(tri_vertex(tri.c, normal));
    }
    vertices
}

fn build_line_vertices(mesh: &Mesh) -> Vec<LineVertexPod> {
    let mut lines = Vec::with_capacity(mesh.lins.len() * LINE_VERTICES_PER_INSTANCE as usize);
    for (line_idx, line) in mesh.lins.iter().enumerate() {
        let (source_idx, source) = match line_inverse_index(mesh, line.inv) {
            Some(inv_idx) if line_idx > inv_idx => continue,
            Some(inv_idx) if !line_visible(line) && line_visible(&mesh.lins[inv_idx]) => {
                (inv_idx, &mesh.lins[inv_idx])
            }
            _ => (line_idx, line),
        };

        if !line_visible(source) {
            continue;
        }

        let prev = resolved_line_neighbor(mesh, source_idx, source.prev, NeighborDirection::Prev)
            .unwrap_or(source);
        let next = resolved_line_neighbor(mesh, source_idx, source.next, NeighborDirection::Next)
            .unwrap_or(source);
        let tangent = source.b.pos - source.a.pos;
        let prev_tangent = source.a.pos - prev.a.pos;
        let next_tangent = next.b.pos - source.b.pos;

        lines.extend([
            line_vertex(source.a.pos, source.a.col, tangent, prev_tangent, 1.0),
            line_vertex(source.a.pos, source.a.col, tangent, tangent, 0.0),
            line_vertex(source.a.pos, source.a.col, tangent, tangent, 1.0),
            line_vertex(source.b.pos, source.b.col, tangent, tangent, 0.0),
            line_vertex(source.b.pos, source.b.col, tangent, tangent, 1.0),
            line_vertex(source.b.pos, source.b.col, tangent, next_tangent, 1.0),
        ]);
    }
    lines
}

#[derive(Clone, Copy)]
enum NeighborDirection {
    Prev,
    Next,
}

fn resolved_line_neighbor(
    mesh: &Mesh,
    line_idx: usize,
    explicit: i32,
    direction: NeighborDirection,
) -> Option<&geo::mesh::Lin> {
    if let Some(neighbor) = line_neighbor(mesh, explicit)
        .filter(|neighbor| line_neighbor_matches(&mesh.lins[line_idx], neighbor, direction))
    {
        return Some(neighbor);
    }

    let current = &mesh.lins[line_idx];
    let inverse_idx = line_inverse_index(mesh, current.inv);
    let candidates = mesh
        .lins
        .iter()
        .enumerate()
        .filter(|(candidate_idx, candidate)| {
            *candidate_idx != line_idx
                && Some(*candidate_idx) != inverse_idx
                && line_visible(candidate)
                && line_neighbor_matches(current, candidate, direction)
        })
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();

    if candidates.len() == 1 {
        Some(candidates[0])
    } else {
        None
    }
}

fn line_neighbor_matches(
    current: &geo::mesh::Lin,
    candidate: &geo::mesh::Lin,
    direction: NeighborDirection,
) -> bool {
    match direction {
        NeighborDirection::Prev => {
            same_position(candidate.b.pos, current.a.pos)
                && !same_position(candidate.a.pos, current.b.pos)
        }
        NeighborDirection::Next => {
            same_position(candidate.a.pos, current.b.pos)
                && !same_position(candidate.b.pos, current.a.pos)
        }
    }
}

fn line_neighbor(mesh: &Mesh, index: i32) -> Option<&geo::mesh::Lin> {
    (index >= 0)
        .then_some(index as usize)
        .and_then(|index| mesh.lins.get(index))
}

fn line_inverse_index(mesh: &Mesh, index: i32) -> Option<usize> {
    (index >= 0)
        .then_some(index as usize)
        .filter(|&index| index < mesh.lins.len())
}

fn line_visible(line: &geo::mesh::Lin) -> bool {
    line.a.col.w > f32::EPSILON || line.b.col.w > f32::EPSILON
}

fn same_position(a: Float3, b: Float3) -> bool {
    (a - b).len_sq() <= 1e-12
}

fn build_dot_instances(mesh: &Mesh) -> Vec<DotInstancePod> {
    let mut dots = Vec::with_capacity(mesh.dots.len());
    for dot in &mesh.dots {
        if dot.col.w <= f32::EPSILON {
            continue;
        }
        dots.push(DotInstancePod {
            pos: float4_from_xyz(dot.pos.x, dot.pos.y, dot.pos.z, 0.0),
            col: float4_from_float4(dot.col),
        });
    }
    dots
}

fn line_vertex(
    pos: Float3,
    col: Float4,
    tangent: Float3,
    prev_tangent: Float3,
    extrude: f32,
) -> LineVertexPod {
    LineVertexPod {
        pos: float4_from_xyz(pos.x, pos.y, pos.z, 0.0),
        col: float4_from_float4(col),
        tangent: float4_from_xyz(tangent.x, tangent.y, tangent.z, 0.0),
        prev_tangent: float4_from_xyz(prev_tangent.x, prev_tangent.y, prev_tangent.z, 0.0),
        extrude: [extrude, 0.0, 0.0, 0.0],
    }
}

fn tri_vertex(vertex: geo::mesh::TriVertex, normal: Float3) -> TriVertexPod {
    TriVertexPod {
        pos: float4_from_xyz(vertex.pos.x, vertex.pos.y, vertex.pos.z, 0.0),
        norm: float4_from_xyz(normal.x, normal.y, normal.z, 0.0),
        col: float4_from_float4(vertex.col),
        uv: [vertex.uv.x, vertex.uv.y, 0.0, 0.0],
    }
}

fn triangle_face_normal(a: Float3, b: Float3, c: Float3) -> Float3 {
    let normal = (b - a).cross(c - a);
    if normal.len_sq() <= 1e-12 {
        Float3::Z
    } else {
        normal.normalize()
    }
}

fn build_dot_indices(vertex_count: u16) -> Vec<u16> {
    let vertex_count = vertex_count.max(3);
    let mut indices = Vec::with_capacity((vertex_count as usize - 2) * 3);
    for i in 1..vertex_count - 1 {
        indices.push(0);
        indices.push(i);
        indices.push(i + 1);
    }
    indices
}

fn build_line_indices() -> [u16; 12] {
    [0, 2, 1, 1, 2, 4, 1, 4, 3, 3, 4, 5]
}

fn mesh_line_radius_px(mesh: &Mesh, size: RenderSize, style: RenderStyle) -> f32 {
    let radius = if mesh.uniform.stroke_radius.is_finite() {
        mesh.uniform.stroke_radius.max(0.0)
    } else {
        style.line_width_px.max(0.0) * 0.5
    };
    radius * size.width.max(1) as f32 / REFERENCE_WIDTH
}

fn mesh_line_miter_scale(mesh: &Mesh) -> f32 {
    if mesh.uniform.stroke_miter_radius_scale.is_finite() {
        mesh.uniform.stroke_miter_radius_scale.max(0.0)
    } else {
        DEFAULT_LINE_MITER_SCALE
    }
}

fn mesh_dot_radius_px(mesh: &Mesh, style: RenderStyle) -> f32 {
    if mesh.uniform.dot_radius.is_finite() {
        mesh.uniform.dot_radius.max(0.0)
    } else {
        style.dot_radius_px.max(0.0)
    }
}

fn load_texture(path: &Path) -> Result<RgbaImage> {
    image::open(path)
        .with_context(|| format!("opening {}", path.display()))
        .map(|image| image.into_rgba8())
}

fn create_sampled_texture(
    gpu: &gpu::Context,
    name: &str,
    width: u32,
    height: u32,
    format: gpu::TextureFormat,
) -> CachedTexture {
    let texture = gpu.create_texture(gpu::TextureDesc {
        name,
        format,
        size: gpu::Extent {
            width,
            height,
            depth: 1,
        },
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count: 1,
        dimension: gpu::TextureDimension::D2,
        usage: gpu::TextureUsage::COPY | gpu::TextureUsage::RESOURCE,
        external: None,
    });
    let view = gpu.create_texture_view(
        texture,
        gpu::TextureViewDesc {
            name,
            format,
            dimension: gpu::ViewDimension::D2,
            subresources: &Default::default(),
        },
    );
    CachedTexture { texture, view }
}

fn destroy_texture(gpu: &gpu::Context, texture: CachedTexture) {
    gpu.destroy_texture_view(texture.view);
    gpu.destroy_texture(texture.texture);
}

fn destroy_offscreen_target(gpu: &gpu::Context, target: OffscreenTarget) {
    gpu.destroy_texture_view(target.color_view);
    gpu.destroy_texture(target.color);
    if let Some(color_msaa_view) = target.color_msaa_view {
        gpu.destroy_texture_view(color_msaa_view);
    }
    if let Some(color_msaa) = target.color_msaa {
        gpu.destroy_texture(color_msaa);
    }
    gpu.destroy_texture_view(target.depth_view);
    gpu.destroy_texture(target.depth);
    gpu.destroy_buffer(target.readback);
}

fn choose_sample_count(gpu: &gpu::Context) -> u32 {
    let supported = gpu.capabilities().sample_count_mask;
    if supported & DESIRED_MSAA_SAMPLE_COUNT != 0 {
        DESIRED_MSAA_SAMPLE_COUNT
    } else {
        1
    }
}

fn extent(size: RenderSize) -> gpu::Extent {
    gpu::Extent {
        width: size.width,
        height: size.height,
        depth: 1,
    }
}

fn float4_from_xyz(x: f32, y: f32, z: f32, w: f32) -> [f32; 4] {
    [x, y, z, w]
}

fn float4_from_float4(value: Float4) -> [f32; 4] {
    [value.x, value.y, value.z, value.w]
}

#[cfg(test)]
mod tests {
    use super::{LINE_VERTICES_PER_INSTANCE, build_line_vertices};
    use geo::{
        mesh::{Lin, LinVertex, Mesh, Uniforms},
        simd::{Float3, Float4},
    };
    use naga::{
        ShaderStage,
        front::wgsl,
        valid::{Capabilities, ValidationFlags, Validator},
    };

    #[test]
    fn blade_shader_parses_and_validates() {
        let source = include_str!("blade.wgsl");
        assert!(source.contains("struct TriVertexPod"));
        assert!(source.contains("struct LineVertexPod"));
        assert!(source.contains("struct DotInstancePod"));

        let module = wgsl::parse_str(source).expect("blade.wgsl should parse successfully");
        Validator::new(
            ValidationFlags::all() & !ValidationFlags::BINDINGS,
            Capabilities::all(),
        )
        .validate(&module)
        .expect("blade.wgsl should validate successfully");

        for entry_point in &module.entry_points {
            match entry_point.stage {
                ShaderStage::Fragment => {
                    assert!(
                        entry_point
                            .function
                            .result
                            .as_ref()
                            .and_then(|result| result.binding.as_ref())
                            .is_some(),
                        "fragment entry point '{}' must have an explicitly bound output",
                        entry_point.name
                    );
                }
                ShaderStage::Vertex => {
                    let Some(result) = entry_point.function.result.as_ref() else {
                        panic!(
                            "vertex entry point '{}' must return a varying struct",
                            entry_point.name
                        );
                    };
                    let naga::TypeInner::Struct { ref members, .. } = module.types[result.ty].inner
                    else {
                        panic!(
                            "vertex entry point '{}' must return a struct so varying bindings are explicit",
                            entry_point.name
                        );
                    };
                    assert!(
                        members.iter().all(|member| member.binding.is_some()),
                        "vertex entry point '{}' has an unbound varying member",
                        entry_point.name
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn line_vertices_render_inverse_pairs_once_even_without_dominant_sibling_flags() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 0.0, 0.0),
                    1,
                    -1,
                    -1,
                    1.0,
                ),
                test_line(
                    Float3::new(1.0, 0.0, 0.0),
                    Float3::new(0.0, 0.0, 0.0),
                    0,
                    -1,
                    -1,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices.len(), LINE_VERTICES_PER_INSTANCE as usize);
    }

    #[test]
    fn line_vertices_fall_back_to_visible_inverse_sibling() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 0.0, 0.0),
                    1,
                    -1,
                    -1,
                    0.0,
                ),
                test_line(
                    Float3::new(1.0, 0.0, 0.0),
                    Float3::new(0.0, 0.0, 0.0),
                    0,
                    -1,
                    -1,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices.len(), LINE_VERTICES_PER_INSTANCE as usize);
        assert_eq!(vertices[0].pos, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[5].pos, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[0].col[3], 1.0);
    }

    #[test]
    fn line_vertices_infer_missing_neighbors_from_shared_endpoints() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(0.0, 1.0, 0.0),
                    -1,
                    -1,
                    -1,
                    1.0,
                ),
                test_line(
                    Float3::new(0.0, 1.0, 0.0),
                    Float3::new(1.0, 1.0, 0.0),
                    -1,
                    -1,
                    -1,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices.len(), (LINE_VERTICES_PER_INSTANCE * 2) as usize);
        assert_eq!(vertices[5].prev_tangent, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[6].prev_tangent, [0.0, 1.0, 0.0, 0.0]);
    }

    fn test_line(a: Float3, b: Float3, inv: i32, prev: i32, next: i32, alpha: f32) -> Lin {
        Lin {
            a: LinVertex {
                pos: a,
                col: Float4::new(1.0, 1.0, 1.0, alpha),
            },
            b: LinVertex {
                pos: b,
                col: Float4::new(1.0, 1.0, 1.0, alpha),
            },
            norm: Float3::Z,
            prev,
            next,
            inv,
            anti: -1,
            is_dom_sib: false,
        }
    }
}
