use std::{collections::HashMap, sync::Arc};

use crate::services::{ExecutionSnapshot, ExecutionStatus, ParameterSnapshot, ParameterValue};
use executor::scene_snapshot::{BackgroundSnapshot, CameraSnapshot};
use executor::time::Timestamp;
use geo::mesh::Mesh;

// Any state that's necessary for actual execution
pub struct ExecutionState {
    pub background: BackgroundSnapshot,
    pub camera: CameraSnapshot,
    pub meshes: Vec<Arc<Mesh>>,
    pub parameter_state: HashMap<String, ParameterValue>,

    // runtime info reported by the executor thread
    pub current_timestamp: Timestamp,
    pub status: ExecutionStatus,
    /// cached duration of each slide; None if the slide hasn't been fully executed yet
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,
    pub parameters: Option<ParameterSnapshot>,
    pub slide_count: usize,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: Vec::new(),
            parameter_state: HashMap::new(),
            current_timestamp: Timestamp::default(),
            status: ExecutionStatus::Paused,
            slide_durations: Vec::new(),
            minimum_slide_durations: Vec::new(),
            parameters: None,
            slide_count: 0,
        }
    }
}

impl ExecutionState {
    pub fn is_playing(&self) -> bool {
        matches!(self.status, ExecutionStatus::Playing)
    }

    pub fn has_error(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::CompileError | ExecutionStatus::RuntimeError
        )
    }

    pub fn apply_snapshot(&mut self, snapshot: ExecutionSnapshot) {
        if let Some(background) = snapshot.background {
            self.background = background;
        }
        if let Some(camera) = snapshot.camera {
            self.camera = camera;
        }
        if let Some(meshes) = snapshot.meshes {
            self.meshes = meshes;
        }
        self.current_timestamp = snapshot.current_timestamp;
        self.status = snapshot.status;
        // causes visual glitches if in seek mode
        if snapshot.status != ExecutionStatus::Seeking {
            self.slide_durations = snapshot.slide_durations;
            self.minimum_slide_durations = snapshot.minimum_slide_durations;
        }
        self.slide_count = snapshot.slide_count;
        if let Some(ref params) = snapshot.parameters {
            self.parameter_state = params.parameters.clone();
        }
        self.parameters = snapshot.parameters;
    }
}
