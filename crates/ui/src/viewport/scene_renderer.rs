use std::sync::Arc;

use gpui::{Bounds, Pixels, RenderImage, Window};
use image::Frame;
use renderer::{RenderSize, RenderView, Renderer, RgbaImage, SceneRenderData, scene_fingerprint};

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
        window: &mut Window,
    ) -> Option<Arc<RenderImage>> {
        let view = SceneImageKey::view_for(output_bounds, projection_bounds, scale_factor);
        if self
            .key
            .as_ref()
            .is_some_and(|key| key.revision == revision && key.view == view)
        {
            return self.image.clone();
        }
        if view.is_empty() {
            if let Some(previous) = self.image.take() {
                let _ = window.drop_image(previous);
            }
            self.key = Some(SceneImageKey {
                revision,
                scene: None,
                view,
            });
            return None;
        }

        let scene_fingerprint = self
            .key
            .as_ref()
            .filter(|key| key.revision == revision)
            .and_then(|key| key.scene)
            .unwrap_or_else(|| scene_fingerprint(scene));
        let key = SceneImageKey {
            revision,
            scene: Some(scene_fingerprint),
            view,
        };
        if self
            .key
            .as_ref()
            .is_some_and(|previous| previous.scene == key.scene && previous.view == key.view)
        {
            self.key = Some(key);
            return self.image.clone();
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
    revision: SceneImageRevision,
    scene: Option<u64>,
    view: RenderView,
}

impl SceneImageKey {
    fn view_for(
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
}

fn gpui_image_from_rgba(image: RgbaImage) -> RenderImage {
    RenderImage::new([Frame::new(image)])
}
