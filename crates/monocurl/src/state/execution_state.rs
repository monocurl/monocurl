use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::services::{
    ExecutionSnapshot, ExecutionStatus, ParameterSnapshot, ParameterValue, PresentationUpdateTarget,
};
use executor::scene_snapshot::{BackgroundSnapshot, CameraSnapshot};
use executor::time::Timestamp;
use executor::transcript::SectionTranscript;
use geo::mesh::Mesh;
use gpui::Context;

const LOADING_DISPLAY_DELAY: Duration = Duration::from_millis(100);

// Any state that's necessary for actual execution
pub struct ExecutionState {
    pub scene_version: u64,
    pub background: BackgroundSnapshot,
    pub camera: CameraSnapshot,
    pub camera_version: u64,
    pub meshes: Vec<Arc<Mesh>>,
    pub parameter_state: HashMap<PresentationUpdateTarget, ParameterValue>,

    // runtime info reported by the executor thread
    pub current_timestamp: Timestamp,
    pub target_timestamp: Timestamp,
    pub status: ExecutionStatus,
    /// cached duration of each slide; None if the slide hasn't been fully executed yet
    pub slide_names: Vec<Option<String>>,
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,
    pub parameters: Option<ParameterSnapshot>,
    pub slide_count: usize,
    pub transcript: Vec<Arc<SectionTranscript>>,
    pub is_loading: bool,
    loading_pending: bool,
    loading_nonce: u64,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            scene_version: 0,
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            camera_version: 0,
            meshes: Vec::new(),
            parameter_state: HashMap::new(),
            current_timestamp: Timestamp::default(),
            target_timestamp: Timestamp::default(),
            status: ExecutionStatus::Paused,
            slide_names: Vec::new(),
            slide_durations: Vec::new(),
            minimum_slide_durations: Vec::new(),
            parameters: None,
            slide_count: 0,
            transcript: Vec::new(),
            is_loading: false,
            loading_pending: false,
            loading_nonce: 0,
        }
    }
}

impl ExecutionState {
    pub fn is_playing(&self) -> bool {
        matches!(self.status, ExecutionStatus::Playing)
    }

    pub fn fast_apply_seek(&mut self, mut target: Timestamp, cx: &mut Context<Self>) {
        if self.slide_count == 0 {
            target = Timestamp::default();
        } else {
            target.slide = target.slide.min(self.slide_count);
        }

        self.target_timestamp = target;
        self.apply_loading(true, cx);
    }

    pub fn apply_snapshot(&mut self, snapshot: ExecutionSnapshot, cx: &mut Context<Self>) {
        self.apply_loading(snapshot.is_loading, cx);

        let scene_updated =
            snapshot.background.is_some() || snapshot.camera.is_some() || snapshot.meshes.is_some();
        if let Some(background) = snapshot.background {
            self.background = background;
        }
        if let Some(camera) = snapshot.camera {
            self.camera = camera;
        }
        if let Some(camera_version) = snapshot.camera_version {
            self.camera_version = camera_version;
        }
        if let Some(meshes) = snapshot.meshes {
            self.meshes = meshes;
        }
        if scene_updated {
            self.scene_version = self.scene_version.wrapping_add(1);
        }
        if !snapshot.is_loading {
            self.current_timestamp = snapshot.current_timestamp;
            self.target_timestamp = snapshot.target_timestamp;
        }
        self.status = snapshot.status;
        self.slide_names = snapshot.slide_names;
        // causes visual glitches while showing pre-work loading snapshots
        if !snapshot.is_loading {
            self.slide_durations = snapshot.slide_durations;
            self.minimum_slide_durations = snapshot.minimum_slide_durations;
        }
        self.slide_count = snapshot.slide_count;
        if let Some(ref params) = snapshot.parameters {
            self.parameter_state = params
                .params
                .iter()
                .map(|entry| (entry.target.clone(), entry.value.clone()))
                .collect();
        }
        self.parameters = snapshot.parameters;
        if let Some(transcript) = snapshot.transcript {
            self.transcript = transcript;
        }
    }

    fn apply_loading(&mut self, is_loading: bool, cx: &mut Context<Self>) {
        if !is_loading {
            self.finish_loading();
            return;
        }

        if self.loading_pending {
            return;
        }

        self.loading_nonce = self.loading_nonce.wrapping_add(1);
        if self.is_loading {
            return;
        }

        self.loading_pending = true;
        let nonce = self.loading_nonce;
        cx.spawn(async move |state, cx| {
            cx.background_executor().timer(LOADING_DISPLAY_DELAY).await;
            state
                .update(cx, |state, cx| {
                    if state.loading_pending && state.loading_nonce == nonce {
                        state.loading_pending = false;
                        state.is_loading = true;
                        cx.notify();
                    }
                })
                .ok();
        })
        .detach();
    }

    fn finish_loading(&mut self) {
        if self.loading_pending {
            self.loading_nonce = self.loading_nonce.wrapping_add(1);
            self.loading_pending = false;
            return;
        }

        self.loading_nonce = self.loading_nonce.wrapping_add(1);
        self.is_loading = false;
    }
}
