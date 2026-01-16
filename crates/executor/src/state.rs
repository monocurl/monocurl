use compiler::bytecode::{SectionBytecode};

pub struct Timestamp {
    slide: usize,
    time: f64,
}

pub struct ExecutionState {
    timestamp: Timestamp,

    error_state: Vec<u8>,
    meshes: Vec<u8>,
    variables: Vec<u8>,
    parameters: Vec<u8>,
}

pub struct SlideCache {
    execution_snapshot: ExecutionState,
    bytecode: SectionBytecode,
}
