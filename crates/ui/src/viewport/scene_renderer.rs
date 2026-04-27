use std::sync::Arc;

use gpui::{Bounds, Pixels, RenderImage, Window};
use image::Frame;
use renderer::{RenderSize, RenderView, Renderer, RgbaImage, SceneRenderData, scene_fingerprint};

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
        output_bounds: Bounds<Pixels>,
        projection_bounds: Bounds<Pixels>,
        scale_factor: f32,
        window: &mut Window,
    ) -> Option<Arc<RenderImage>> {
        let key = SceneImageKey::new(scene, output_bounds, projection_bounds, scale_factor);
        if self.key.as_ref() == Some(&key) {
            return self.image.clone();
        }
        if key.view.is_empty() {
            if let Some(previous) = self.image.take() {
                let _ = window.drop_image(previous);
            }
            self.key = Some(key);
            return None;
        }

        let image = renderer
            .render_view(scene, key.view)
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
    view: RenderView,
}

impl SceneImageKey {
    fn new(
        scene: &SceneRenderData,
        output_bounds: Bounds<Pixels>,
        projection_bounds: Bounds<Pixels>,
        scale_factor: f32,
    ) -> Self {
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
        Self {
            scene: scene_fingerprint(scene),
            view: RenderView::with_raster_scale(
                RenderSize::new(output_width, output_height),
                RenderSize::new(projection_width, projection_height),
                scale_factor,
            ),
        }
    }
}

fn gpui_image_from_rgba(image: RgbaImage) -> RenderImage {
    RenderImage::new([Frame::new(image)])
}
