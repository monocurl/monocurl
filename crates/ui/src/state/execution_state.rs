use crate::services::{ExecutionSnapshot, ExecutionStatus};
use executor::time::Timestamp;

// Any state that's necessary for actual execution
pub struct ExecutionState {
    pub background_color: (f32, f32, f32),
    pub camera_position: (f32, f32, f32),
    pub mesh_state: Vec<u8>,
    pub parameter_state: Vec<u8>,

    pub frames: Vec<Vec<u8>>,

    // runtime info reported by the executor thread
    pub current_timestamp: Timestamp,
    pub status: ExecutionStatus,
    /// cached duration of each slide; None if the slide hasn't been fully executed yet
    pub slide_durations: Vec<Option<f64>>,
    pub slide_count: usize,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            background_color: (0.0, 0.0, 0.0),
            camera_position: (0.0, 0.0, 0.0),
            mesh_state: Vec::new(),
            parameter_state: Vec::new(),
            frames: Vec::new(),
            current_timestamp: Timestamp::default(),
            status: ExecutionStatus::Paused,
            slide_durations: Vec::new(),
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
        self.slide_count = snapshot.slide_count;
    }
}
