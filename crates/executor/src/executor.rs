use std::sync::Arc;

use bytecode::Bytecode;

use crate::{state::ExecutionState, time::Timestamp};

#[derive(Clone)]
pub struct AllocTracker(Arc<()>);
impl AllocTracker {
    pub fn count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

pub struct Executor {
    alloc_tracker: AllocTracker,

    state: ExecutionState,
    cache: Vec<ExecutionState>,
    bytecode: Bytecode,
}

pub enum SeekPrimitiveResult {
    Error,
    EndOfSection,
    PrimitiveAnim
}

pub enum StepResult {
    Error,
    Continue,
    EndOfAllAnims,
}

impl Executor {
    pub async fn section_init(
        &mut self,
        bytecode: &Bytecode,
        state: &mut ExecutionState,
    ) {

    }

    // run all execution heads until they reach a primitive play (or end of section)
    // may yield intermittently
    async fn seek_primitive_anim(&mut self) -> SeekPrimitiveResult {
        SeekPrimitiveResult::Error
    }

    async fn step_primitive_anim(&mut self, dt: f64) -> StepResult {
        StepResult::Error
    }

    pub async fn seek_to(timestamp: Timestamp) {

    }
}

impl Executor {

}
