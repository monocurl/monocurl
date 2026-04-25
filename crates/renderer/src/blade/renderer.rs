use std::{path::Path, sync::Arc};

use anyhow::{Result, anyhow, ensure};
use blade_graphics as gpu;
use blade_util::BufferBeltDescriptor;
use bytemuck::Pod;
use executor::camera::CameraBasis;
use image::RgbaImage;

use crate::{RenderSize, RenderView, SceneRenderData, mesh_fingerprint};

use super::{
    BladeRenderer, DEPTH_FORMAT, DEPTH_STEP, LINE_INDICES_PER_INSTANCE, LINE_VERTICES_PER_INSTANCE,
    MAX_FRAME_TIME_MS, MeshWorkItem, TARGET_FORMAT, TEXTURE_FORMAT, WHITE_TEXTURE,
    geometry::{
        build_dot_indices, build_dot_instances, build_line_vertices, build_triangle_vertices,
        mesh_dot_radius_px, mesh_line_miter_scale, mesh_line_radius_px,
    },
    pipelines::Pipelines,
    resources::{
        BufferWithCount, CachedMesh, CachedTexture, IndexedBuffer, OffscreenTarget,
        PendingBufferUpload, PendingTextureUpload, TextureCacheEntry, choose_sample_count,
        create_sampled_texture, destroy_offscreen_target, destroy_texture, extent, load_texture,
    },
    types::{
        BackgroundData, BackgroundParams, CameraParams, DotShaderParams, DotsData,
        LineShaderParams, LinesData, TriShaderParams, TrianglesData,
    },
};

impl BladeRenderer {
    pub(crate) fn new(style: crate::RenderStyle) -> Result<Self> {
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
            upload_belt: blade_util::BufferBelt::new(BufferBeltDescriptor {
                memory: gpu::Memory::Upload,
                min_chunk_size: 0x10000,
                alignment: 64,
            }),
            white_texture,
            dot_index_buffers: std::collections::HashMap::new(),
            target: None,
            mesh_cache: std::collections::HashMap::new(),
            texture_cache: std::collections::HashMap::new(),
            pending_buffer_uploads: Vec::new(),
            pending_texture_uploads: Vec::new(),
            style,
            sample_count,
            frame_index: 0,
        };
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

    fn ensure_mesh(&mut self, mesh: &Arc<geo::mesh::Mesh>, frame_index: u64) -> usize {
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
                    encoder.draw(
                        0,
                        LINE_INDICES_PER_INSTANCE,
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
