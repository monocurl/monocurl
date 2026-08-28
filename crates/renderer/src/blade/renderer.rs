use std::{path::Path, sync::Arc};

use anyhow::{Result, anyhow, ensure};
use blade_graphics as gpu;
use blade_util::BufferBeltDescriptor;
use bytemuck::Pod;
use executor::camera::CameraBasis;
use image::RgbaImage;

use crate::{RenderSize, RenderView, SceneRenderData};

use super::{
    BladeRenderer, DEPTH_FORMAT, DEPTH_STEP, LINE_INDICES_PER_INSTANCE, LINE_VERTICES_PER_INSTANCE,
    MAX_FRAME_TIME_MS, MeshWorkItem, TARGET_FORMAT, TEXTURE_FORMAT, WHITE_TEXTURE,
    geometry::{
        build_dot_indices, build_dot_instances, build_line_vertices, build_triangle_vertices,
        mesh_centroid, mesh_dot_radius_px, mesh_has_translucent_vertex, mesh_line_miter_scale,
        mesh_line_radius_px,
    },
    order,
    pipelines::Pipelines,
    resources::{
        BufferWithCount, CachedMesh, CachedTexture, IndexedBuffer, OffscreenTarget,
        PendingBufferUpload, PendingTextureUpload, TextureCacheEntry, choose_sample_count,
        create_oit_targets, create_sampled_texture, destroy_offscreen_target, destroy_texture,
        extent, load_texture,
    },
    types::{
        BackgroundData, BackgroundParams, CameraParams, DotShaderParams, DotsData,
        LineShaderParams, LinesData, OitCompositeData, TriShaderParams, TrianglesData,
    },
};

/// Borrowed view of the renderer state needed to record one mesh item's draws,
/// kept separate from `command_encoder` so a render pass (which mutably borrows
/// the encoder) and the per-item draw helper can coexist.
struct DrawCtx<'a> {
    pipelines: &'a Pipelines,
    mesh_cache: &'a std::collections::HashMap<usize, CachedMesh>,
    texture_cache: &'a std::collections::HashMap<std::path::PathBuf, TextureCacheEntry>,
    dot_index_buffers: &'a std::collections::HashMap<u16, IndexedBuffer>,
    white_texture_view: gpu::TextureView,
    texture_sampler: gpu::Sampler,
    style: crate::RenderStyle,
}

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

        let basis = scene.camera.basis();
        let mut items = Vec::with_capacity(scene.meshes.len());
        for (order, mesh) in scene.meshes.iter().enumerate() {
            if mesh.uniform.alpha <= 0.0 {
                continue;
            }

            let key = self.ensure_mesh(mesh, frame_index);
            let mut texture_has_alpha = false;
            if let Some(path) = mesh.uniform.img.as_deref() {
                self.ensure_texture(path, frame_index);
                texture_has_alpha = self
                    .texture_cache
                    .get(path)
                    .is_some_and(|entry| entry.has_alpha);
            }

            let (centroid, translucent_vertices) = self
                .mesh_cache
                .get(&key)
                .map(|cached| (cached.centroid, cached.translucent_vertices))
                .unwrap_or((geo::simd::Float3::ZERO, false));

            items.push(MeshWorkItem {
                key,
                order,
                mesh: Arc::clone(mesh),
                texture_path: mesh.uniform.img.clone(),
                z_index: mesh.uniform.z_index,
                transparent: order::is_transparent(
                    mesh.uniform.alpha,
                    translucent_vertices,
                    texture_has_alpha,
                ),
                depth: order::camera_depth(centroid, basis.position, basis.forward),
                tri_bias: 0.0,
                line_bias: 0.0,
                dot_bias: 0.0,
            });
        }

        // Canonical declaration rank drives the per-primitive NDC depth offsets,
        // so the depth-test outcome for coplanar meshes is identical no matter
        // how the paint order below reshuffles the draw sequence.
        items.sort_by_key(|item| (item.z_index, item.order));
        let item_count = items.len();
        for (rank, item) in items.iter_mut().enumerate() {
            let bias = order::rank_bias(rank, item_count, DEPTH_STEP);
            item.tri_bias = bias.tri;
            item.line_bias = bias.line;
            item.dot_bias = bias.dot;
        }

        // Final paint order: within each z_index, opaque (declaration order,
        // depth-write on) then transparent (back-to-front, depth-write off).
        items.sort_by(|a, b| order::draw_order_cmp(&a.sort_key(), &b.sort_key()));

        self.flush_pending_uploads();
        self.draw_meshes(&items, view, basis, Some(scene.background.color));
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
            if let Some(target) = &mut self.target
                && target.needs_init
            {
                self.command_encoder.init_texture(target.color);
                if let Some(color_msaa) = target.color_msaa {
                    self.command_encoder.init_texture(color_msaa);
                }
                self.command_encoder.init_texture(target.depth);
                target.needs_init = false;
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
            oit: None,
            needs_init: false,
        });
    }

    fn ensure_mesh(&mut self, mesh: &Arc<geo::mesh::Mesh>, frame_index: u64) -> usize {
        let key = Arc::as_ptr(mesh) as usize;
        let version = mesh.version();
        if let Some(cached) = self.mesh_cache.get_mut(&key) {
            let same_mesh = cached
                .mesh
                .upgrade()
                .is_some_and(|cached_mesh| Arc::ptr_eq(&cached_mesh, mesh));
            if same_mesh && cached.version == version {
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
                mesh: Arc::downgrade(mesh),
                version,
                triangles,
                lines,
                dots,
                centroid: mesh_centroid(mesh),
                translucent_vertices: mesh_has_translucent_vertex(mesh),
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

        let (texture, has_alpha) = match load_texture(path) {
            Ok(image) => {
                let has_alpha = image.pixels().any(|pixel| pixel[3] < 255);
                (self.create_texture_with_upload(path, &image), has_alpha)
            }
            Err(error) => {
                log::warn!(
                    "failed to load renderer texture {}: {error:#}",
                    path.display()
                );
                (None, false)
            }
        };

        self.texture_cache.insert(
            path.to_path_buf(),
            TextureCacheEntry {
                texture,
                has_alpha,
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
            let dot_radius = mesh_dot_radius_px(item.mesh.as_ref(), self.style, view.raster_scale);
            let dot_vertex_count = item.mesh.uniform.dot_vertex_count.max(3);
            if dot_radius > f32::EPSILON {
                let _ = self.ensure_dot_index_buffer(dot_vertex_count);
            }
        }

        let camera = CameraParams::from_basis(basis, view);
        let size = view.output_size;
        let transparent_present = items.iter().any(|item| item.transparent);

        // Lazily allocate the weighted-blended OIT scratch targets the first time
        // a frame actually contains transparent geometry. Opaque-only scenes never
        // enter this branch and render through the unchanged single pass below.
        if transparent_present
            && self
                .target
                .as_ref()
                .is_some_and(|target| target.oit.is_none())
        {
            let oit = create_oit_targets(&self.gpu, size, self.sample_count);
            self.command_encoder.init_texture(oit.accum);
            self.command_encoder.init_texture(oit.reveal);
            if let Some(texture) = oit.accum_msaa {
                self.command_encoder.init_texture(texture);
            }
            if let Some(texture) = oit.reveal_msaa {
                self.command_encoder.init_texture(texture);
            }
            self.target.as_mut().expect("target should exist").oit = Some(oit);
        }

        let ctx = DrawCtx {
            pipelines: &self.pipelines,
            mesh_cache: &self.mesh_cache,
            texture_cache: &self.texture_cache,
            dot_index_buffers: &self.dot_index_buffers,
            white_texture_view: self.white_texture.view,
            texture_sampler: self.texture_sampler,
            style: self.style,
        };

        let target = self.target.as_ref().expect("target should exist");
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

        // ---- Pass 1: background + opaque geometry (writes depth). ----
        // The depth buffer must survive for the OIT pass to test against it.
        {
            let mut pass = self.command_encoder.render(
                "renderer-opaque",
                gpu::RenderTargetSet {
                    colors: &[color_target],
                    depth_stencil: Some(gpu::RenderTarget {
                        view: target.depth_view,
                        init_op: gpu::InitOp::Clear(gpu::TextureColor::White),
                        finish_op: if transparent_present {
                            gpu::FinishOp::Store
                        } else {
                            gpu::FinishOp::Discard
                        },
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

            for item in items.iter().filter(|item| !item.transparent) {
                Self::draw_item_primitives(&ctx, &mut pass, item, camera, view, size, false);
            }
        }

        if !transparent_present {
            return;
        }

        // ---- Pass 2: weighted-blended OIT accumulation for transparent meshes.
        // Depth test against the opaque depth buffer, no depth write. ----
        let target = self.target.as_ref().expect("target should exist");
        let oit = target.oit.as_ref().expect("oit targets should exist");
        let (accum_rt, reveal_rt) = match (oit.accum_msaa_view, oit.reveal_msaa_view) {
            (Some(accum_msaa), Some(reveal_msaa)) => (
                gpu::RenderTarget {
                    view: accum_msaa,
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                    finish_op: gpu::FinishOp::ResolveTo(oit.accum_view),
                },
                gpu::RenderTarget {
                    view: reveal_msaa,
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::White),
                    finish_op: gpu::FinishOp::ResolveTo(oit.reveal_view),
                },
            ),
            _ => (
                gpu::RenderTarget {
                    view: oit.accum_view,
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                    finish_op: gpu::FinishOp::Store,
                },
                gpu::RenderTarget {
                    view: oit.reveal_view,
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::White),
                    finish_op: gpu::FinishOp::Store,
                },
            ),
        };
        {
            let mut pass = self.command_encoder.render(
                "renderer-oit",
                gpu::RenderTargetSet {
                    colors: &[accum_rt, reveal_rt],
                    depth_stencil: Some(gpu::RenderTarget {
                        view: target.depth_view,
                        init_op: gpu::InitOp::Load,
                        finish_op: gpu::FinishOp::Discard,
                    }),
                },
            );
            for item in items.iter().filter(|item| item.transparent) {
                Self::draw_item_primitives(&ctx, &mut pass, item, camera, view, size, true);
            }
        }

        // ---- Pass 3: composite the weighted average over the resolved colour. ----
        let target = self.target.as_ref().expect("target should exist");
        let oit = target.oit.as_ref().expect("oit targets should exist");
        {
            let mut pass = self.command_encoder.render(
                "renderer-oit-composite",
                gpu::RenderTargetSet {
                    colors: &[gpu::RenderTarget {
                        view: target.color_view,
                        init_op: gpu::InitOp::Load,
                        finish_op: gpu::FinishOp::Store,
                    }],
                    depth_stencil: None,
                },
            );
            let mut encoder = pass.with(&self.pipelines.oit_composite);
            encoder.bind(
                0,
                &OitCompositeData {
                    oit_accum_tex: oit.accum_view,
                    oit_reveal_tex: oit.reveal_view,
                },
            );
            encoder.draw(0, 4, 0, 1);
        }
    }

    /// Draws one mesh item's triangles, then lines, then dots into `pass`.
    /// With `oit` the geometry is routed through the weighted-blended OIT
    /// pipelines (MRT accum + revealage); otherwise through the opaque /
    /// simple-blend pipelines.
    fn draw_item_primitives(
        ctx: &DrawCtx<'_>,
        pass: &mut gpu::RenderCommandEncoder<'_>,
        item: &MeshWorkItem,
        camera: CameraParams,
        view: RenderView,
        size: RenderSize,
        oit: bool,
    ) {
        let Some(buffers) = ctx.mesh_cache.get(&item.key) else {
            return;
        };

        if let Some(triangles) = buffers.triangles.as_ref() {
            let texture_view = item
                .texture_path
                .as_ref()
                .and_then(|path| {
                    ctx.texture_cache
                        .get(path)
                        .and_then(|entry| entry.texture.as_ref())
                })
                .map_or(ctx.white_texture_view, |texture| texture.view);

            let pipeline = if oit {
                &ctx.pipelines.triangles_oit
            } else {
                &ctx.pipelines.triangles
            };
            let mut encoder = pass.with(pipeline);
            encoder.bind(
                0,
                &TrianglesData {
                    tri_camera: camera,
                    tri_params: TriShaderParams {
                        values: [
                            item.mesh.uniform.alpha as f32,
                            item.tri_bias,
                            item.mesh.uniform.gloss,
                            if item.mesh.uniform.smooth { 1.0 } else { 0.0 },
                        ],
                    },
                    t_color: texture_view,
                    s_color: ctx.texture_sampler,
                    tri_vertices: triangles.buffer.into(),
                },
            );
            encoder.draw(0, triangles.count, 0, 1);
        }

        if let Some(lines) = buffers.lines.as_ref() {
            let line_radius = mesh_line_radius_px(item.mesh.as_ref(), size, ctx.style);
            if line_radius > f32::EPSILON {
                let pipeline = if oit {
                    &ctx.pipelines.lines_oit
                } else {
                    &ctx.pipelines.lines
                };
                let mut encoder = pass.with(pipeline);
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
                                item.line_bias,
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
            }
        }

        if let Some(dots) = buffers.dots.as_ref() {
            let dot_radius = mesh_dot_radius_px(item.mesh.as_ref(), ctx.style, view.raster_scale);
            let dot_vertex_count = item.mesh.uniform.dot_vertex_count.max(3);
            if dot_radius > f32::EPSILON
                && let Some(index_buffer) = ctx.dot_index_buffers.get(&dot_vertex_count)
            {
                let pipeline = if oit {
                    &ctx.pipelines.dots_oit
                } else {
                    &ctx.pipelines.dots
                };
                let mut encoder = pass.with(pipeline);
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
                            depth_bias: [item.dot_bias, dot_vertex_count as f32, 0.0, 0.0],
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
            if let Some(entry) = self.texture_cache.remove(&path)
                && let Some(texture) = entry.texture
            {
                destroy_texture(&self.gpu, texture);
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
