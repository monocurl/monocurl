use crate::services::ExecutionSnapshot;
use executor::time::Timestamp;

// Any state that's necessary for actual execution
#[derive(Default)]
pub struct ExecutionState {
    pub slide: usize,
    pub time: f64,

    pub background_color: (f32, f32, f32),
    pub camera_position: (f32, f32, f32),
    pub mesh_state: Vec<u8>,
    pub parameter_state: Vec<u8>,

    pub frames: Vec<Vec<u8>>,

    // runtime info reported by the executor thread
    pub current_timestamp: Timestamp,
    pub runtime_errors: Vec<String>,
    pub has_compiler_error: bool,
    /// cached duration of each slide; None if the slide hasn't been fully executed yet
    pub slide_durations: Vec<Option<f64>>,
    pub slide_count: usize,
    pub is_playing: bool,
}

impl ExecutionState {
    pub fn apply_snapshot(&mut self, snapshot: ExecutionSnapshot) {
        self.current_timestamp = snapshot.current_timestamp;
        self.runtime_errors = snapshot.runtime_errors;
        self.slide_durations = snapshot.slide_durations;
        self.slide_count = snapshot.slide_count;
    }
}
