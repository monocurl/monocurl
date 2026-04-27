use std::path::Path;

use anyhow::{Context as _, Result};
use blade_graphics as gpu;
use image::RgbaImage;

use crate::RenderSize;

use super::DESIRED_MSAA_SAMPLE_COUNT;

pub(super) struct OffscreenTarget {
    pub(super) size: RenderSize,
    pub(super) color: gpu::Texture,
    pub(super) color_view: gpu::TextureView,
    pub(super) color_msaa: Option<gpu::Texture>,
    pub(super) color_msaa_view: Option<gpu::TextureView>,
    pub(super) depth: gpu::Texture,
    pub(super) depth_view: gpu::TextureView,
    pub(super) readback: gpu::Buffer,
    pub(super) needs_init: bool,
}

pub(super) struct CachedMesh {
    pub(super) version: u64,
    pub(super) triangles: Option<BufferWithCount>,
    pub(super) lines: Option<BufferWithCount>,
    pub(super) dots: Option<BufferWithCount>,
    pub(super) last_used_frame: u64,
}

pub(super) struct BufferWithCount {
    pub(super) buffer: gpu::Buffer,
    pub(super) count: u32,
}

pub(super) struct IndexedBuffer {
    pub(super) buffer: gpu::Buffer,
    pub(super) count: u32,
}

pub(super) struct CachedTexture {
    pub(super) texture: gpu::Texture,
    pub(super) view: gpu::TextureView,
}

pub(super) struct TextureCacheEntry {
    pub(super) texture: Option<CachedTexture>,
    pub(super) last_used_frame: u64,
}

pub(super) struct PendingBufferUpload {
    pub(super) src: gpu::BufferPiece,
    pub(super) dst: gpu::Buffer,
    pub(super) size: u64,
}

pub(super) struct PendingTextureUpload {
    pub(super) src: gpu::BufferPiece,
    pub(super) dst: gpu::Texture,
    pub(super) bytes_per_row: u32,
    pub(super) size: gpu::Extent,
}

impl CachedMesh {
    pub(super) fn destroy(self, gpu: &gpu::Context) {
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

pub(super) fn load_texture(path: &Path) -> Result<RgbaImage> {
    image::open(path)
        .with_context(|| format!("opening {}", path.display()))
        .map(|image| image.into_rgba8())
}

pub(super) fn create_sampled_texture(
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

pub(super) fn destroy_texture(gpu: &gpu::Context, texture: CachedTexture) {
    gpu.destroy_texture_view(texture.view);
    gpu.destroy_texture(texture.texture);
}

pub(super) fn destroy_offscreen_target(gpu: &gpu::Context, target: OffscreenTarget) {
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

pub(super) fn choose_sample_count(gpu: &gpu::Context) -> u32 {
    choose_sample_count_from_mask(gpu.capabilities().sample_count_mask)
}

pub(super) fn extent(size: RenderSize) -> gpu::Extent {
    gpu::Extent {
        width: size.width,
        height: size.height,
        depth: 1,
    }
}

fn choose_sample_count_from_mask(supported: u32) -> u32 {
    let mut sample_count = DESIRED_MSAA_SAMPLE_COUNT;
    while sample_count > 1 {
        if supported & sample_count != 0 {
            return sample_count;
        }
        sample_count >>= 1;
    }

    1
}

#[cfg(test)]
mod tests {
    use super::choose_sample_count_from_mask;

    #[test]
    fn chooses_highest_supported_count_not_exceeding_desired() {
        assert_eq!(choose_sample_count_from_mask(1 | 2 | 4 | 8), 8);
        assert_eq!(choose_sample_count_from_mask(1 | 2 | 4), 4);
        assert_eq!(choose_sample_count_from_mask(1 | 2), 2);
        assert_eq!(choose_sample_count_from_mask(1), 1);
    }
}
