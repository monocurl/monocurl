use std::{collections::VecDeque, sync::Arc};

use gpui::{Bounds, Pixels, RenderImage, Window};
use image::Frame;
use renderer::{RenderSize, RenderView, Renderer, RgbaImage, SceneRenderData};

const PREVIEW_RETIRED_IMAGE_LIMIT: usize = 4;
// Fullscreen presentation images often occupy one Windows GPUI atlas texture per
// frame. Keep them alive longer so recent frames do not reference freed slots.
const PRESENTATION_RETIRED_IMAGE_LIMIT: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneImageRevision {
    scene: u64,
    viewport_camera: u64,
}

impl SceneImageRevision {
    pub fn new(scene: u64, viewport_camera: u64) -> Self {
        Self {
            scene,
            viewport_camera,
        }
    }
}

#[derive(Default)]
pub struct SceneImageCache {
    key: Option<SceneImageKey>,
    image: Option<Arc<RenderImage>>,
    retired_images: VecDeque<Arc<RenderImage>>,
}

impl SceneImageCache {
    pub fn image_for(
        &mut self,
        renderer: &mut Renderer,
        scene: &SceneRenderData,
        revision: SceneImageRevision,
        output_bounds: Bounds<Pixels>,
        projection_bounds: Bounds<Pixels>,
        scale_factor: f32,
        retired_image_limit: usize,
        window: &mut Window,
    ) -> Option<Arc<RenderImage>> {
        let view = render_view_for(output_bounds, projection_bounds, scale_factor);
        let key = SceneImageKey { revision, view };
        if self.key.as_ref() == Some(&key) && self.image.is_some() {
            return self.image.clone();
        }

        if view.is_empty() {
            if let Some(previous) = self.image.take() {
                self.retire_image(previous, retired_image_limit, window);
            }
            self.key = Some(key);
            return None;
        }

        let image = match renderer.render_view(scene, view) {
            Ok(image) => Arc::new(gpui_image_from_rgba(image)),
            Err(error) => {
                log::warn!("scene render failed: {error:#}");
                return self.image.clone();
            }
        };

        if let Some(previous) = self.image.replace(image.clone()) {
            self.retire_image(previous, retired_image_limit, window);
        }
        self.key = Some(key);
        Some(image)
    }

    fn retire_image(
        &mut self,
        image: Arc<RenderImage>,
        retired_image_limit: usize,
        window: &mut Window,
    ) {
        self.retired_images.push_back(image);
        while self.retired_images.len() > retired_image_limit {
            if let Some(image) = self.retired_images.pop_front() {
                let _ = window.drop_image(image);
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct SceneImageKey {
    revision: SceneImageRevision,
    view: RenderView,
}

fn render_view_for(
    output_bounds: Bounds<Pixels>,
    projection_bounds: Bounds<Pixels>,
    scale_factor: f32,
) -> RenderView {
    let output_width = (f32::from(output_bounds.size.width) * scale_factor)
        .ceil()
        .max(0.0) as u32;
    let output_height = (f32::from(output_bounds.size.height) * scale_factor)
        .ceil()
        .max(0.0) as u32;
    let projection_width = (f32::from(projection_bounds.size.width) * scale_factor)
        .ceil()
        .max(0.0) as u32;
    let projection_height = (f32::from(projection_bounds.size.height) * scale_factor)
        .ceil()
        .max(0.0) as u32;
    RenderView::with_raster_scale(
        RenderSize::new(output_width, output_height),
        RenderSize::new(projection_width, projection_height),
        scale_factor,
    )
}

fn gpui_image_from_rgba(image: RgbaImage) -> RenderImage {
    RenderImage::new([Frame::new(image)])
}

pub fn retired_image_limit_for_presentation(is_presenting: bool) -> usize {
    if is_presenting {
        PRESENTATION_RETIRED_IMAGE_LIMIT
    } else {
        PREVIEW_RETIRED_IMAGE_LIMIT
    }
}
