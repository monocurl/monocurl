use std::sync::Arc;

use gpui::{Bounds, Pixels, RenderImage, Window};
use image::Frame;
use renderer::{RenderSize, Renderer, RgbaImage, SceneRenderData, scene_fingerprint};

#[derive(Default)]
pub struct SceneImageCache {
    key: Option<SceneImageKey>,
    image: Option<Arc<RenderImage>>,
}

impl SceneImageCache {
    pub fn image_for(
        &mut self,
        renderer: &mut Renderer,
        scene: &SceneRenderData,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        window: &mut Window,
    ) -> Option<Arc<RenderImage>> {
        let key = SceneImageKey::new(scene, bounds, scale_factor);
        if self.key.as_ref() == Some(&key) {
            return self.image.clone();
        }
        if key.pixel_size.is_empty() {
            if let Some(previous) = self.image.take() {
                let _ = window.drop_image(previous);
            }
            self.key = Some(key);
            return None;
        }

        let image = renderer
            .render(scene, key.pixel_size)
            .ok()
            .map(gpui_image_from_rgba)
            .map(Arc::new)?;

        if let Some(previous) = self.image.replace(image.clone()) {
            let _ = window.drop_image(previous);
        }
        self.key = Some(key);
        Some(image)
    }
}

#[derive(Clone, PartialEq)]
struct SceneImageKey {
    scene: u64,
    pixel_size: RenderSize,
}

impl SceneImageKey {
    fn new(scene: &SceneRenderData, bounds: Bounds<Pixels>, scale_factor: f32) -> Self {
        let width = (f32::from(bounds.size.width) * scale_factor)
            .ceil()
            .max(0.0) as u32;
        let height = (f32::from(bounds.size.height) * scale_factor)
            .ceil()
            .max(0.0) as u32;
        Self {
            scene: scene_fingerprint(scene),
            pixel_size: RenderSize::new(width, height),
        }
    }
}

fn gpui_image_from_rgba(image: RgbaImage) -> RenderImage {
    RenderImage::new([Frame::new(image)])
}
