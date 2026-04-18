use std::collections::HashMap;

use crate::services::{
    ExecutionSnapshot, ExecutionStatus, MeshDebugSnapshot, ParameterSnapshot, ParameterValue,
};
use executor::time::Timestamp;

#[derive(Clone, Default)]
pub struct Camera {
    position: (f32, f32, f32),

}

// Any state that's necessary for actual execution
pub struct ExecutionState {
    pub background_color: (f64, f64, f64, f64),
    pub camera: Camera,
    pub mesh_debug: Vec<MeshDebugSnapshot>,
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
            background_color: (0.0, 0.0, 0.0, 0.0),
            camera: Camera::default(),
            mesh_debug: Vec::new(),
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
        self.current_timestamp = snapshot.current_timestamp;
        self.status = snapshot.status;
        self.slide_durations = snapshot.slide_durations;
        self.minimum_slide_durations = snapshot.minimum_slide_durations;
        self.slide_count = snapshot.slide_count;
        if let Some(ref params) = snapshot.parameters {
            self.parameter_state = params.parameters.clone();
        }
        self.parameters = snapshot.parameters;
        self.mesh_debug = snapshot.mesh_debug;
    }
}
