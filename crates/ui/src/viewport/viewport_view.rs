mod camera;
mod params;
mod render;
mod style;

use std::{collections::HashMap, time::Duration};

use gpui::*;
use renderer::Renderer;

use crate::{
    services::ServiceManager, state::execution_state::ExecutionState, theme::ThemeSettings,
    viewport::scene_renderer::SceneImageCache,
};

use self::{
    camera::{CameraDragState, PresentationCameraState, PreviewCameraState},
    params::DragState,
    style::{OVERDRAG_TICK, RingStyle, TRANSPARENT},
};

const PAUSE_HINT_DURATION: Duration = Duration::from_millis(500);

pub struct Viewport {
    services: Entity<ServiceManager>,
    execution_state: Entity<ExecutionState>,
    is_presenting: bool,
    show_params: bool,
    drag_state: Option<DragState>,
    camera_drag: Option<CameraDragState>,
    preview_camera: Option<PreviewCameraState>,
    copied_preview_camera: Option<String>,
    presentation_camera: Option<PresentationCameraState>,
    show_pause_hint: bool,
    pause_hint_nonce: u64,
    scene_camera_version: u64,
    scroll_handle: ScrollHandle,
    slider_bounds: HashMap<String, [f64; 4]>,
    ring_style: Option<RingStyle>,
    ring_previous: RingStyle,
    ring_animation_nonce: usize,
    renderer: Renderer,
    scene_image_cache: SceneImageCache,
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
            show_params: false,
            drag_state: None,
            camera_drag: None,
            preview_camera: None,
            copied_preview_camera: None,
            presentation_camera: None,
            show_pause_hint: false,
            pause_hint_nonce: 0,
            scene_camera_version,
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
            self.show_params = false;
            self.drag_state = None;
            self.slider_bounds.clear();
        }
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
        self.show_params = !self.show_params;
        cx.notify();
    }

    pub fn sync_viewport_camera(&mut self, cx: &mut Context<Self>) {
        self.camera_drag = None;
        if self.is_presenting {
            self.reset_presentation_camera(cx);
        } else {
            self.preview_camera = None;
            self.copied_preview_camera = None;
            cx.notify();
        }
    }
}
