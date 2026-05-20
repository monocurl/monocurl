mod camera;
mod params;
mod render;
mod style;

use std::{collections::HashMap, time::Duration};

use gpui::*;
use renderer::Renderer;

use crate::{
    services::{PresentationUpdateTarget, ServiceManager},
    state::execution_state::ExecutionState,
    theme::ThemeSettings,
    viewport::scene_renderer::SceneImageCache,
};

use self::{
    camera::{CameraDragState, PresentationCameraState, PreviewCameraState},
    params::DragState,
    style::{OVERDRAG_TICK, RingStyle, TRANSPARENT},
};

const PAUSE_HINT_DURATION: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AspectRatioPreset {
    Wide,
    Standard,
    Square,
    FeedPortrait,
    Portrait,
    Ultrawide,
}

impl AspectRatioPreset {
    pub const ALL: [Self; 6] = [
        Self::Wide,
        Self::Standard,
        Self::Square,
        Self::FeedPortrait,
        Self::Portrait,
        Self::Ultrawide,
    ];

    pub const fn aspect_ratio(self) -> f32 {
        match self {
            Self::Wide => 16.0 / 9.0,
            Self::Standard => 4.0 / 3.0,
            Self::Square => 1.0,
            Self::FeedPortrait => 4.0 / 5.0,
            Self::Portrait => 9.0 / 16.0,
            Self::Ultrawide => 21.0 / 9.0,
        }
    }
}

pub struct Viewport {
    services: Entity<ServiceManager>,
    execution_state: Entity<ExecutionState>,
    is_presenting: bool,
    aspect_preset: AspectRatioPreset,
    drag_state: Option<DragState>,
    camera_drag: Option<CameraDragState>,
    preview_camera: Option<PreviewCameraState>,
    copied_preview_camera: Option<String>,
    presentation_camera: Option<PresentationCameraState>,
    show_pause_hint: bool,
    pause_hint_nonce: u64,
    scene_camera_version: u64,
    viewport_camera_version: u64,
    scroll_handle: ScrollHandle,
    slider_bounds: HashMap<PresentationUpdateTarget, [f64; 4]>,
    ring_style: Option<RingStyle>,
    ring_previous: RingStyle,
    ring_animation_nonce: usize,
    renderer: Renderer,
    scene_image_cache: SceneImageCache,
    audience_renderer: Renderer,
    audience_scene_image_cache: SceneImageCache,
}

impl Viewport {
    pub fn new(services: Entity<ServiceManager>, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        let execution_state = services.read(cx).execution_state().clone();
        cx.observe(&execution_state, |viewport, _, cx| {
            viewport.sync_camera_from_execution(cx);
            cx.notify();
        })
        .detach();

        let scene_camera_version = execution_state.read(cx).camera_version;
        let viewport = Self {
            services,
            execution_state,
            is_presenting: false,
            aspect_preset: AspectRatioPreset::Wide,
            drag_state: None,
            camera_drag: None,
            preview_camera: None,
            copied_preview_camera: None,
            presentation_camera: None,
            show_pause_hint: false,
            pause_hint_nonce: 0,
            scene_camera_version,
            viewport_camera_version: 0,
            scroll_handle: ScrollHandle::new(),
            slider_bounds: HashMap::new(),
            ring_style: None,
            ring_previous: RingStyle {
                color: TRANSPARENT,
                width: 0.0,
            },
            ring_animation_nonce: 0,
            renderer: Renderer::default(),
            scene_image_cache: SceneImageCache::default(),
            audience_renderer: Renderer::default(),
            audience_scene_image_cache: SceneImageCache::default(),
        };

        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor().timer(OVERDRAG_TICK).await;
                let should_continue = weak
                    .update(cx, |viewport, cx| {
                        viewport.tick_overdrag(cx);
                    })
                    .is_ok();
                if !should_continue {
                    break;
                }
            }
        })
        .detach();

        viewport
    }

    pub fn set_presenting(&mut self, presenting: bool, cx: &mut Context<Self>) {
        self.is_presenting = presenting;
        self.bump_viewport_camera_version();
        self.camera_drag = None;
        self.preview_camera = None;
        self.copied_preview_camera = None;
        self.presentation_camera = None;
        self.show_pause_hint = false;
        self.pause_hint_nonce = self.pause_hint_nonce.wrapping_add(1);
        if presenting {
            let hidden_ring = RingStyle {
                color: TRANSPARENT,
                width: 0.0,
            };
            self.ring_style = Some(hidden_ring);
            self.ring_previous = hidden_ring;
        }
        if !presenting {
            self.drag_state = None;
            self.slider_bounds.clear();
        }
        cx.notify();
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.aspect_preset.aspect_ratio()
    }

    pub fn set_aspect_preset(&mut self, preset: AspectRatioPreset, cx: &mut Context<Self>) {
        self.aspect_preset = preset;
        self.bump_viewport_camera_version();
        let aspect_ratio = self.aspect_ratio();
        self.services.update(cx, |services, _| {
            services.update_aspect_ratio(aspect_ratio);
        });
        cx.notify();
    }

    pub fn show_pause_hint(&mut self, cx: &mut Context<Self>) {
        if !self.is_presenting {
            return;
        }

        self.show_pause_hint = true;
        self.pause_hint_nonce = self.pause_hint_nonce.wrapping_add(1);
        let nonce = self.pause_hint_nonce;
        cx.notify();

        cx.spawn(async move |weak, cx| {
            cx.background_executor().timer(PAUSE_HINT_DURATION).await;
            let _ = weak.update(cx, |viewport, cx| {
                if viewport.pause_hint_nonce == nonce {
                    viewport.show_pause_hint = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn clear_pause_hint(&mut self, cx: &mut Context<Self>) {
        if !self.show_pause_hint {
            return;
        }

        self.show_pause_hint = false;
        self.pause_hint_nonce = self.pause_hint_nonce.wrapping_add(1);
        cx.notify();
    }

    pub fn toggle_params(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub fn sync_viewport_camera(&mut self, cx: &mut Context<Self>) {
        self.camera_drag = None;
        self.bump_viewport_camera_version();
        if self.is_presenting {
            self.reset_presentation_camera(cx);
        } else {
            self.preview_camera = None;
            self.copied_preview_camera = None;
            cx.notify();
        }
    }

    pub(super) fn bump_viewport_camera_version(&mut self) {
        self.viewport_camera_version = self.viewport_camera_version.wrapping_add(1);
    }
}
