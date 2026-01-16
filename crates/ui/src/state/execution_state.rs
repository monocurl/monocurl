// Any state that's necessary for actual execution
#[derive(Default)]
pub struct ExecutionState {
    slide: usize,
    time: f64,

    bytecode: Vec<u8>,

    background_color: (u8, u8, u8),
    camera_position: (f32, f32, f32),
    mesh_state: Vec<u8>,
    parameter_state: Vec<u8>,

    frames: Vec<Vec<u8>>,
}
