use compiler::bytecode::Bytecode;

use crate::state::{ExecutionState};

pub struct Executor {

}

pub enum SectionExecutionResult {
    Continue,
    Pause,
    Stop,
}

impl Executor {
    pub async fn section_init(
        &mut self,
        bytecode: &Bytecode,
        state: &mut ExecutionState,
    ) {

    }

    pub async fn section_step(
        &mut self,
        bytecode: &Bytecode,
        state: &mut ExecutionState,
        dt: f64
    ) -> SectionExecutionResult {
        SectionExecutionResult::Stop
    }
}
